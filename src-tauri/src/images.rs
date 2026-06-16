//! Image optimizer for the Tools panel. Recursively walks a folder, applies
//! lossy re-encoding to JPGs at a user-chosen quality, and runs oxipng on
//! PNGs (lossless). Skips files that don't shrink — we never grow images.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Serialize)]
pub struct CompressReport {
    pub files_total: usize,
    pub files_changed: usize,
    pub bytes_before: u64,
    pub bytes_after: u64,
    pub errors: Vec<String>,
}

pub fn compress_folder(
    folder: &Path,
    jpeg_quality: u8,
    include_png: bool,
    include_jpg: bool,
) -> Result<CompressReport, String> {
    if !folder.is_dir() {
        return Err(format!("not a folder: {}", folder.display()));
    }
    let mut report = CompressReport::default();
    walk_and_process(
        folder,
        jpeg_quality,
        include_png,
        include_jpg,
        &mut report,
    );
    Ok(report)
}

fn walk_and_process(
    dir: &Path,
    jpeg_quality: u8,
    include_png: bool,
    include_jpg: bool,
    report: &mut CompressReport,
) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            report.errors.push(format!("read_dir {}: {e}", dir.display()));
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_and_process(&path, jpeg_quality, include_png, include_jpg, report);
            continue;
        }
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase());
        let kind = match ext.as_deref() {
            Some("jpg" | "jpeg") if include_jpg => Kind::Jpeg,
            Some("png") if include_png => Kind::Png,
            _ => continue,
        };
        report.files_total += 1;
        let before = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        report.bytes_before += before;
        match process_one(&path, kind, jpeg_quality) {
            Ok(after) => {
                report.bytes_after += after;
                if after < before {
                    report.files_changed += 1;
                }
            }
            Err(e) => {
                report.bytes_after += before;
                report
                    .errors
                    .push(format!("{}: {e}", path.display()));
            }
        }
    }
}

enum Kind {
    Jpeg,
    Png,
}

fn process_one(path: &Path, kind: Kind, jpeg_quality: u8) -> Result<u64, String> {
    let original_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    match kind {
        Kind::Jpeg => {
            let img = image::ImageReader::open(path)
                .map_err(|e| e.to_string())?
                .with_guessed_format()
                .map_err(|e| e.to_string())?
                .decode()
                .map_err(|e| e.to_string())?;
            let rgb = img.to_rgb8();
            let mut out: Vec<u8> = Vec::new();
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                &mut out,
                jpeg_quality,
            );
            rgb.write_with_encoder(encoder).map_err(|e| e.to_string())?;
            commit_if_smaller(path, &out, original_size)
        }
        Kind::Png => {
            // oxipng default options are conservative + safe; level 3 is a
            // good time/space tradeoff and runs in milliseconds for typical
            // web assets.
            let mut opts = oxipng::Options::from_preset(3);
            opts.strip = oxipng::StripChunks::Safe;
            let out = oxipng::optimize_from_memory(&fs::read(path).map_err(|e| e.to_string())?, &opts)
                .map_err(|e| e.to_string())?;
            commit_if_smaller(path, &out, original_size)
        }
    }
}

/// Write the new bytes in place only if they're smaller. Returns the final
/// size on disk (original if we kept the file untouched). The "skip if
/// bigger" guard means re-running the optimiser is idempotent — never grows
/// files even on lossless formats that fail to shrink further.
fn commit_if_smaller(path: &Path, new_bytes: &[u8], original_size: u64) -> Result<u64, String> {
    let new_size = new_bytes.len() as u64;
    if new_size >= original_size {
        return Ok(original_size);
    }
    // Write to a sibling tempfile then rename for atomicity — partial writes
    // on a power loss would otherwise corrupt user assets.
    let tmp: PathBuf = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|s| s.to_str())
            .unwrap_or("bin")
    ));
    fs::write(&tmp, new_bytes).map_err(|e| format!("write tmp: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| format!("rename: {e}"))?;
    Ok(new_size)
}
