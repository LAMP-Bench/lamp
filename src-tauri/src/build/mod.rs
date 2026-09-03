//! Compiling service binaries on the user's machine.
//!
//! Upstream publishes no prebuilt Unix binaries for Apache, PHP, nginx or
//! Redis, only source tarballs. Rather than pre-building a matrix of every
//! version against every architecture and hosting it forever, the app can
//! compile what it needs where it runs. That is also the only way version
//! switching works properly on Unix: on Windows there are five PHP builds to
//! download because windows.php.net publishes five, and matching that from a
//! release matrix would mean rebuilding on every patch release. Compiling on
//! demand handles any version for free.
//!
//! This is the fallback tier. When `binaries.json` has a prebuilt entry for
//! the current platform that always wins. It is faster and cannot fail
//! halfway through a 20-minute compile.
//!
//! **Why nothing is bundled or RPATH-patched here.** The CI recipe copies
//! shared libraries next to the binaries and rewrites RPATH, because the
//! result has to run on a machine that never had the build dependencies. A
//! local build has no such problem: it links against libraries that are
//! installed on this very machine and stay installed, so the dynamic loader
//! finds them the ordinary way. That also means `patchelf` is not a
//! requirement, and neither is building APR or PCRE2 from source, the
//! distro's `-dev`/`-devel` packages are exactly what we asked the user to
//! install.

pub mod distro;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// Everything needed to build one component from source, read from the
/// `source` block of its `binaries.json` entry. Versions, URLs and package
/// names are data; the build steps themselves are Rust, because each one is
/// genuinely bespoke (httpd wants apxs afterwards, PHP needs its tree
/// flattened, Xdebug has to be phpize'd against a PHP we just built) and
/// expressing that in JSON would mean inventing a build DSL.
#[derive(Debug, Deserialize, Clone)]
pub struct SourceRecipe {
    /// The release actually built, which is not always the entry's `version`:
    /// mod_fcgid's last source release is 2.3.9 while the Windows entry pins
    /// ApacheLounge's 2.3.10, and Redis pins a Windows fork's 5.0.14.1 while
    /// upstream source is 7.2.x. The UI shows this so nobody is surprised.
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub strip_root_dir: Option<String>,
    /// Tools and pkg-config modules that must resolve before we start. Used
    /// only to decide whether to offer the install step, the build itself is
    /// the real test.
    #[serde(default)]
    pub probe: Probe,
    /// Package names per distro family. Keys match `distro::Family::key()`.
    #[serde(default)]
    pub build_deps: std::collections::HashMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Probe {
    /// Executables that must be on PATH.
    #[serde(default)]
    pub bins: Vec<String>,
    /// pkg-config module names that must resolve.
    #[serde(default)]
    pub pkgconfig: Vec<String>,
}

/// What the UI shows before starting a build.
#[derive(Debug, Serialize)]
pub struct DepReport {
    /// Human-readable distro, e.g. "CachyOS".
    pub distro: String,
    /// Raw os-release ID, worth showing when someone reports a build failure.
    pub distro_id: String,
    pub family: String,
    /// The release that would be built. Not always the entry's version, see
    /// `SourceRecipe::version`, so the UI can say "Redis 7.2.5" honestly.
    pub source_version: Option<String>,
    /// Probes that failed, missing compilers, headers, tools.
    pub missing: Vec<String>,
    /// Packages we would install to satisfy them.
    pub packages: Vec<String>,
    /// The exact command, for display. `None` when we don't know this
    /// family's package manager, in which case the user installs by hand.
    pub install_command: Option<String>,
    /// False when there is no source recipe for this component at all.
    pub buildable: bool,
}

pub fn recipe_for(name: &str) -> Option<SourceRecipe> {
    crate::downloads::load_manifest()
        .ok()?
        .entries
        .get(name)?
        .source
        .clone()
}

fn have_bin(name: &str) -> bool {
    // `which` rather than a PATH walk so shell functions and odd layouts
    // resolve the same way the build itself will see them.
    crate::services::hidden_command("sh")
        .arg("-c")
        .arg(format!("command -v {name}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn have_pkgconfig(module: &str) -> bool {
    crate::services::hidden_command("pkg-config")
        .arg("--exists")
        .arg(module)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check the toolchain without touching anything.
pub fn dep_report(name: &str) -> DepReport {
    let d = distro::detect();
    // Source builds are a Unix answer to a Unix problem: upstream publishes
    // Windows binaries for everything, and none of this, ./configure, a
    // system package manager, pkexec, exists there. Report not-buildable
    // rather than offering a button that cannot work.
    let recipe = if cfg!(windows) { None } else { recipe_for(name) };
    let Some(recipe) = recipe else {
        return DepReport {
            family: d.family_key().to_string(),
            distro: d.name,
            distro_id: d.id,
            source_version: None,
            missing: Vec::new(),
            packages: Vec::new(),
            install_command: None,
            buildable: false,
        };
    };

    let mut missing = Vec::new();
    for b in &recipe.probe.bins {
        if !have_bin(b) {
            missing.push(b.clone());
        }
    }
    // Without pkg-config itself we can't probe the libraries, so say so once
    // rather than reporting every module as missing.
    if !recipe.probe.pkgconfig.is_empty() && !have_bin("pkg-config") {
        if !missing.iter().any(|m| m == "pkg-config") {
            missing.push("pkg-config".to_string());
        }
    } else {
        for m in &recipe.probe.pkgconfig {
            if !have_pkgconfig(m) {
                missing.push(m.clone());
            }
        }
    }

    let packages = recipe
        .build_deps
        .get(d.family_key())
        .cloned()
        .unwrap_or_default();
    let install_command = d
        .family
        .install_argv(&packages)
        .map(|argv| argv.join(" "));

    DepReport {
        family: d.family_key().to_string(),
        distro: d.name,
        distro_id: d.id,
        source_version: Some(recipe.version.clone()),
        missing,
        packages,
        install_command,
        buildable: true,
    }
}

/// Install the build dependencies, elevated. Only ever called after the user
/// has seen `dep_report` and agreed. This is the one place Lamp Bench touches
/// the system package manager, and it installs build tooling only, never a
/// service we would then supervise.
pub fn install_deps(name: &str) -> Result<(), String> {
    let d = distro::detect();
    let recipe = recipe_for(name).ok_or_else(|| format!("{name} has no source recipe"))?;
    let packages = recipe
        .build_deps
        .get(d.family_key())
        .cloned()
        .unwrap_or_default();
    let argv = d.family.install_argv(&packages).ok_or_else(|| {
        format!(
            "Don't know how to install packages on {}. Install these by hand: {}",
            d.name,
            packages.join(" ")
        )
    })?;

    let runner = elevation()?;
    let mut cmd = crate::services::hidden_command(&runner);
    if runner == "sudo" {
        cmd.arg("-n");
    }
    cmd.args(&argv);
    let out = cmd
        .output()
        .map_err(|e| format!("spawn {runner}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "{} failed: {}",
            argv.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

fn elevation() -> Result<String, String> {
    for runner in ["pkexec", "sudo"] {
        if have_bin(runner) {
            return Ok(runner.to_string());
        }
    }
    Err("need pkexec or sudo to install build dependencies".into())
}

/// Sink for build output. A compile takes minutes, so the UI streams the log
/// rather than showing a spinner and hoping.
pub type LogSink<'a> = &'a mut dyn FnMut(&str);

pub struct Ctx<'a> {
    pub resources_dir: PathBuf,
    /// Scratch space for source trees and the build log.
    pub work_dir: PathBuf,
    pub jobs: usize,
    pub log: LogSink<'a>,
}

impl Ctx<'_> {
    fn say(&mut self, line: &str) {
        (self.log)(line);
    }
}

/// Fetch, verify and unpack a source tarball, returning the extracted tree.
/// Tarballs are cached, so a failed build can be retried without a second
/// download.
fn fetch_source(recipe: &SourceRecipe, ctx: &mut Ctx) -> Result<PathBuf, String> {
    let cache = ctx.work_dir.join(".cache");
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let filename = recipe
        .url
        .rsplit('/')
        .next()
        .ok_or("source url has no filename")?
        .to_string();
    let archive = cache.join(&filename);

    if !archive.exists() {
        ctx.say(&format!("downloading {filename}"));
        let resp = ureq::get(&recipe.url)
            .call()
            .map_err(|e| format!("download {filename}: {e}"))?;
        let mut body = Vec::new();
        std::io::Read::read_to_end(&mut resp.into_reader(), &mut body)
            .map_err(|e| format!("read {filename}: {e}"))?;

        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&body);
        let actual = format!("{:X}", hasher.finalize());
        if actual != recipe.sha256.to_uppercase() {
            return Err(format!(
                "SHA256 mismatch for {filename}: expected {} got {actual}",
                recipe.sha256
            ));
        }
        std::fs::write(&archive, &body).map_err(|e| e.to_string())?;
    } else {
        ctx.say(&format!("using cached {filename}"));
    }

    let tree = ctx.work_dir.join("src");
    if tree.exists() {
        std::fs::remove_dir_all(&tree).map_err(|e| e.to_string())?;
    }
    ctx.say("unpacking");
    let format = crate::downloads::detect_format(&filename)?;
    match format {
        crate::downloads::ArchiveFormat::Zip => {
            crate::downloads::extract_zip(&archive, &tree, recipe.strip_root_dir.as_deref())?
        }
        other => crate::downloads::extract_tar(
            &archive,
            &tree,
            other,
            recipe.strip_root_dir.as_deref(),
        )?,
    }
    Ok(tree)
}

/// Run a build step, streaming stdout and stderr into the log as they arrive.
fn run(ctx: &mut Ctx, cwd: &Path, program: &str, args: &[&str]) -> Result<(), String> {
    use std::io::{BufRead, BufReader};

    ctx.say(&format!("$ {program} {}", args.join(" ")));
    let mut child = crate::services::hidden_command(program)
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn {program}: {e}"))?;

    // stderr is where configure and make put the interesting parts, so both
    // streams are surfaced. Read stdout on this thread and stderr on another
    // so neither pipe can fill up and deadlock the compile.
    let stderr = child.stderr.take();
    let err_handle = stderr.map(|s| {
        std::thread::spawn(move || {
            let mut collected = Vec::new();
            for line in BufReader::new(s).lines().map_while(Result::ok) {
                collected.push(line);
            }
            collected
        })
    });

    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            ctx.say(&line);
        }
    }

    let status = child.wait().map_err(|e| e.to_string())?;
    let err_lines = err_handle.and_then(|h| h.join().ok()).unwrap_or_default();
    for line in &err_lines {
        ctx.say(line);
    }

    if !status.success() {
        // The last few stderr lines are almost always the actual cause; the
        // full log is in the file the caller is writing.
        let tail: Vec<&str> = err_lines.iter().rev().take(15).map(|s| s.as_str()).collect();
        return Err(format!(
            "{program} failed (exit {}): {}",
            status.code().unwrap_or(-1),
            tail.into_iter().rev().collect::<Vec<_>>().join("\n")
        ));
    }
    Ok(())
}

/// Build a component from source and install it into `resources/` in the same
/// layout a prebuilt download would have produced, so nothing downstream can
/// tell the difference.
pub fn build(name: &str, ctx: &mut Ctx) -> Result<(), String> {
    if cfg!(windows) {
        return Err("source builds aren't supported on Windows, every \
                    component has a prebuilt binary there"
            .into());
    }
    let recipe = recipe_for(name)
        .ok_or_else(|| format!("{name} cannot be built from source (no recipe)"))?;
    std::fs::create_dir_all(&ctx.work_dir).map_err(|e| e.to_string())?;

    let d = distro::detect();
    ctx.say(&format!(
        "building {name} on {} with {} job(s)",
        d.name, ctx.jobs
    ));

    let tree = fetch_source(&recipe, ctx)?;

    if name == "apache" {
        build_apache(&tree, ctx)
    } else if let Some(v) = name.strip_prefix("php-") {
        build_php(&tree, v, ctx)
    } else if let Some(v) = name.strip_prefix("xdebug-") {
        build_xdebug(&tree, v, ctx)
    } else if name == "nginx" {
        build_nginx(&tree, ctx)
    } else if name == "redis" {
        build_redis(&tree, ctx)
    } else {
        Err(format!("no build steps defined for {name}"))
    }
}

fn jobs_flag(ctx: &Ctx) -> String {
    format!("-j{}", ctx.jobs)
}

fn build_apache(tree: &Path, ctx: &mut Ctx) -> Result<(), String> {
    let prefix = ctx.resources_dir.join("apache");
    let prefix_s = prefix.to_string_lossy().to_string();

    // mods-shared/mpms-shared are load bearing: the generated httpd.conf
    // loads mpm_event and mod_unixd as DSOs on Unix (see services/apache.rs),
    // so they must not be compiled in. APR and PCRE2 come from the distro
    // packages we asked for, which is why there's no srclib dance here.
    run(
        ctx,
        tree,
        "./configure",
        &[
            &format!("--prefix={prefix_s}"),
            "--enable-mods-shared=most",
            "--enable-mpms-shared=all",
            "--enable-ssl",
            "--enable-rewrite",
            "--enable-so",
        ],
    )?;
    let j = jobs_flag(ctx);
    run(ctx, tree, "make", &[&j])?;
    run(ctx, tree, "make", &["install"])?;

    // mod_fcgid is a separate upstream tarball, built against the httpd we
    // just installed. Without it no PHP is served at all, so it is part of
    // building "apache" rather than its own component.
    ctx.say("building mod_fcgid");
    let fcgid_recipe = recipe_for("mod_fcgid")
        .ok_or("mod_fcgid has no source recipe, but Apache needs it")?;
    let mut sub = Ctx {
        resources_dir: ctx.resources_dir.clone(),
        work_dir: ctx.work_dir.join("mod_fcgid"),
        jobs: ctx.jobs,
        log: ctx.log,
    };
    std::fs::create_dir_all(&sub.work_dir).map_err(|e| e.to_string())?;
    let fcgid_tree = fetch_source(&fcgid_recipe, &mut sub)?;
    let apxs = prefix.join("bin").join("apxs");
    run(
        &mut sub,
        &fcgid_tree,
        "sh",
        &[
            "-c",
            &format!("APXS={} ./configure.apxs", apxs.to_string_lossy()),
        ],
    )?;
    let j = jobs_flag(&sub);
    run(&mut sub, &fcgid_tree, "make", &[&j])?;
    // services::apache::FCGID_MODULE looks for it alongside every other
    // module on Unix; only Windows uses the separate modules-extra/ dir.
    std::fs::copy(
        fcgid_tree.join("modules/fcgid/.libs/mod_fcgid.so"),
        prefix.join("modules").join("mod_fcgid.so"),
    )
    .map_err(|e| format!("install mod_fcgid.so: {e}"))?;

    ctx.say("apache built");
    Ok(())
}

fn build_php(tree: &Path, version: &str, ctx: &mut Ctx) -> Result<(), String> {
    let dest = ctx.resources_dir.join(format!("php-{version}"));
    // The full install tree is kept next to the flattened layout: `phpize`
    // and `php-config` live there, and building Xdebug (or any PECL
    // extension later) needs them plus the PHP headers.
    let sdk = dest.join("_sdk");
    let sdk_s = sdk.to_string_lossy().to_string();

    // Everything the app's generated php.ini writes an `extension=` line for
    // has to be =shared, matching the Windows build where they ship as DLLs
    // in ext/. Built statically they still work, but php.ini then names .so
    // files that don't exist and PHP logs "Unable to load dynamic library"
    // on every request, and the Versions panel has nothing to toggle.
    run(
        ctx,
        tree,
        "./configure",
        &[
            &format!("--prefix={sdk_s}"),
            "--enable-cgi",
            "--without-pear",
            "--enable-mysqlnd",
            "--with-mysqli=shared,mysqlnd",
            "--with-pdo-mysql=shared,mysqlnd",
            "--with-curl=shared",
            "--enable-mbstring=shared",
            "--with-openssl=shared",
            "--enable-gd=shared",
            "--with-jpeg",
            "--with-freetype",
            "--enable-intl=shared",
            "--with-zip=shared",
            "--enable-exif=shared",
            "--enable-fileinfo=shared",
            "--enable-opcache",
            "--enable-soap=shared",
            "--enable-sockets=shared",
            "--with-zlib",
            "--with-bz2",
        ],
    )?;
    let j = jobs_flag(ctx);
    run(ctx, tree, "make", &[&j])?;
    run(ctx, tree, "make", &["install"])?;

    // Flatten to the layout every caller expects: php and php-cgi at the root
    // of the version dir, extensions in ext/, the ini template alongside.
    // That is the Windows zip's shape, and keeping it identical is what lets
    // the only cross-platform difference stay EXE_SUFFIX.
    let ext = dest.join("ext");
    std::fs::create_dir_all(&ext).map_err(|e| e.to_string())?;
    for bin in ["php", "php-cgi"] {
        let from = sdk.join("bin").join(bin);
        std::fs::copy(&from, dest.join(bin))
            .map_err(|e| format!("install {}: {e}", from.display()))?;
    }
    // The extension dir is named after the PHP API version, which we don't
    // want to hardcode, glob for it instead.
    let ext_src = sdk.join("lib").join("php").join("extensions");
    if let Ok(entries) = std::fs::read_dir(&ext_src) {
        for api_dir in entries.flatten() {
            if let Ok(sos) = std::fs::read_dir(api_dir.path()) {
                for so in sos.flatten() {
                    if so.path().extension().is_some_and(|e| e == "so") {
                        let _ = std::fs::copy(so.path(), ext.join(so.file_name()));
                    }
                }
            }
        }
    }
    std::fs::copy(tree.join("php.ini-development"), dest.join("php.ini-development"))
        .map_err(|e| format!("install php.ini-development: {e}"))?;

    ctx.say(&format!("php {version} built"));
    Ok(())
}

fn build_xdebug(tree: &Path, version: &str, ctx: &mut Ctx) -> Result<(), String> {
    // Xdebug is a Zend extension compiled against one exact PHP build, so the
    // matching PHP has to exist first, its _sdk tree carries phpize and the
    // headers.
    let php_dest = ctx.resources_dir.join(format!("php-{version}"));
    let sdk = php_dest.join("_sdk");
    let phpize = sdk.join("bin").join("phpize");
    let php_config = sdk.join("bin").join("php-config");
    if !phpize.exists() {
        return Err(format!(
            "PHP {version} has to be built from source before Xdebug, {} is missing",
            phpize.display()
        ));
    }

    run(ctx, tree, &phpize.to_string_lossy(), &[])?;
    run(
        ctx,
        tree,
        "./configure",
        &[&format!("--with-php-config={}", php_config.to_string_lossy())],
    )?;
    let j = jobs_flag(ctx);
    run(ctx, tree, "make", &[&j])?;
    std::fs::copy(
        tree.join("modules").join("xdebug.so"),
        php_dest.join("ext").join("xdebug.so"),
    )
    .map_err(|e| format!("install xdebug.so: {e}"))?;

    ctx.say(&format!("xdebug for php {version} built"));
    Ok(())
}

fn build_nginx(tree: &Path, ctx: &mut Ctx) -> Result<(), String> {
    let prefix = ctx.resources_dir.join("nginx");
    let prefix_s = prefix.to_string_lossy().to_string();
    run(
        ctx,
        tree,
        "./configure",
        &[
            &format!("--prefix={prefix_s}"),
            "--with-http_ssl_module",
            "--with-pcre",
        ],
    )?;
    let j = jobs_flag(ctx);
    run(ctx, tree, "make", &[&j])?;
    run(ctx, tree, "make", &["install"])?;
    // The app looks for the binary at the root of the dir, matching the
    // Windows zip, not at sbin/nginx.
    let sbin = prefix.join("sbin").join("nginx");
    if sbin.exists() {
        std::fs::rename(&sbin, prefix.join("nginx"))
            .map_err(|e| format!("move nginx into place: {e}"))?;
        let _ = std::fs::remove_dir(prefix.join("sbin"));
    }
    ctx.say("nginx built");
    Ok(())
}

fn build_redis(tree: &Path, ctx: &mut Ctx) -> Result<(), String> {
    let j = jobs_flag(ctx);
    run(ctx, tree, "make", &[&j])?;
    let dest = ctx.resources_dir.join("redis");
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    // redis-server at the root, matching the Windows zip layout that
    // services/redis.rs expects.
    for bin in ["redis-server", "redis-cli"] {
        std::fs::copy(tree.join("src").join(bin), dest.join(bin))
            .map_err(|e| format!("install {bin}: {e}"))?;
    }
    ctx.say("redis built");
    Ok(())
}
