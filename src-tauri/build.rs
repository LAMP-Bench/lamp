use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    // Embed git commit short SHA + epoch seconds at compile time so the
    // about panel can show which exact build the user is running. Both are
    // exposed as env vars consumed via env!() in lib.rs.
    let sha = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=LAMP_BENCH_GIT_SHA={sha}");

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    println!("cargo:rustc-env=LAMP_BENCH_BUILD_EPOCH={now}");

    // Rerun the build script whenever HEAD or the index changes — that's the
    // cheapest way to keep the embedded SHA current without rebuilding on
    // every source edit.
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/index");

    tauri_build::build()
}
