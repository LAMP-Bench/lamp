//! Which Linux this is, and how to install packages on it.
//!
//! Deliberately keyed on the *family*, not the release. A recipe that says
//! "ubuntu-22.04" is wrong the moment someone runs Mint, Pop!_OS or the next
//! Ubuntu; what actually determines the package names is the family, and
//! `/etc/os-release` reports that directly via `ID_LIKE`.
//!
//! `ID` is checked first because a derivative can have a family-mate's `ID`
//! in its own `ID_LIKE` chain (CachyOS says `ID_LIKE=arch`, Mint says
//! `ID_LIKE="ubuntu debian"`), and the most specific answer that still maps
//! to one package manager is the one we want.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Debian,
    Fedora,
    Arch,
    Suse,
    Alpine,
    Void,
    Gentoo,
    /// Recognised Linux, but not a family we know how to install packages on.
    /// The build can still be attempted, the user just has to bring their
    /// own toolchain.
    Unknown,
}

impl Family {
    /// Key used to look up package names in `binaries.json`.
    pub fn key(&self) -> &'static str {
        match self {
            Family::Debian => "debian",
            Family::Fedora => "fedora",
            Family::Arch => "arch",
            Family::Suse => "suse",
            Family::Alpine => "alpine",
            Family::Void => "void",
            Family::Gentoo => "gentoo",
            Family::Unknown => "unknown",
        }
    }

    /// The command that installs `packages`, as argv. `None` when we don't
    /// know this family's package manager, the caller then tells the user to
    /// install the build tools themselves rather than guessing.
    ///
    /// Every one of these is non-interactive and treats an already-installed
    /// package as success, so it's safe to hand it the whole dependency list
    /// instead of only what's missing.
    pub fn install_argv(&self, packages: &[String]) -> Option<Vec<String>> {
        if packages.is_empty() {
            return None;
        }
        let mut argv: Vec<String> = match self {
            Family::Debian => ["apt-get", "install", "-y", "--no-install-recommends"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            Family::Fedora => ["dnf", "install", "-y"].iter().map(|s| s.to_string()).collect(),
            // --needed keeps pacman from reinstalling what's already there,
            // which on Arch would otherwise churn through half the toolchain.
            Family::Arch => ["pacman", "-S", "--needed", "--noconfirm"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            Family::Suse => ["zypper", "--non-interactive", "install"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            Family::Alpine => ["apk", "add", "--no-cache"].iter().map(|s| s.to_string()).collect(),
            Family::Void => ["xbps-install", "-Sy"].iter().map(|s| s.to_string()).collect(),
            // Gentoo builds from source anyway; --noreplace avoids rebuilding
            // packages the user already has.
            Family::Gentoo => ["emerge", "--noreplace"].iter().map(|s| s.to_string()).collect(),
            Family::Unknown => return None,
        };
        argv.extend(packages.iter().cloned());
        Some(argv)
    }
}

#[derive(Debug, Clone)]
pub struct Distro {
    /// Raw `ID=`, "cachyos", "linuxmint", "fedora".
    pub id: String,
    /// `PRETTY_NAME=`, for showing the user what we detected.
    pub name: String,
    pub family: Family,
}

impl Distro {
    pub fn family_key(&self) -> &'static str {
        self.family.key()
    }
}

/// Read `/etc/os-release`. Falls back to `/usr/lib/os-release`, which is
/// where the file lives on a stateless or immutable system.
pub fn detect() -> Distro {
    for path in ["/etc/os-release", "/usr/lib/os-release"] {
        if let Ok(content) = std::fs::read_to_string(path) {
            return parse_os_release(&content);
        }
    }
    Distro {
        id: String::new(),
        name: "unknown Linux".to_string(),
        family: Family::Unknown,
    }
}

fn unquote(v: &str) -> String {
    v.trim().trim_matches('"').trim_matches('\'').to_string()
}

pub fn parse_os_release(content: &str) -> Distro {
    let mut id = String::new();
    let mut id_like = String::new();
    let mut pretty = String::new();
    for line in content.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "ID" => id = unquote(value),
            "ID_LIKE" => id_like = unquote(value),
            "PRETTY_NAME" => pretty = unquote(value),
            _ => {}
        }
    }

    // `ID` wins when we recognise it; otherwise walk `ID_LIKE`, which is a
    // space-separated list ordered from closest relative outward.
    let family = family_for(&id).or_else(|| {
        id_like
            .split_whitespace()
            .find_map(family_for)
    });

    Distro {
        name: if pretty.is_empty() {
            if id.is_empty() { "unknown Linux".to_string() } else { id.clone() }
        } else {
            pretty
        },
        id,
        family: family.unwrap_or(Family::Unknown),
    }
}

fn family_for(id: &str) -> Option<Family> {
    match id {
        "debian" | "ubuntu" | "linuxmint" | "pop" | "elementary" | "raspbian" | "devuan"
        | "kali" | "zorin" | "neon" => Some(Family::Debian),
        "fedora" | "rhel" | "centos" | "rocky" | "almalinux" | "ol" | "nobara" | "amzn" => {
            Some(Family::Fedora)
        }
        "arch" | "archarm" | "manjaro" | "endeavouros" | "cachyos" | "garuda" | "artix" => {
            Some(Family::Arch)
        }
        "opensuse" | "opensuse-leap" | "opensuse-tumbleweed" | "sles" | "sled" | "suse" => {
            Some(Family::Suse)
        }
        "alpine" => Some(Family::Alpine),
        "void" => Some(Family::Void),
        "gentoo" => Some(Family::Gentoo),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fam(content: &str) -> Family {
        parse_os_release(content).family
    }

    #[test]
    fn recognises_the_big_four_by_id() {
        assert_eq!(fam("ID=debian\n"), Family::Debian);
        assert_eq!(fam("ID=fedora\n"), Family::Fedora);
        assert_eq!(fam("ID=arch\n"), Family::Arch);
        assert_eq!(fam("ID=\"opensuse-tumbleweed\"\n"), Family::Suse);
    }

    #[test]
    fn derivatives_resolve_through_id_like() {
        // The whole point: none of these are a release we'd ever hardcode.
        assert_eq!(
            fam("ID=cachyos\nID_LIKE=arch\nPRETTY_NAME=\"CachyOS\"\n"),
            Family::Arch
        );
        assert_eq!(
            fam("ID=linuxmint\nID_LIKE=\"ubuntu debian\"\n"),
            Family::Debian
        );
        assert_eq!(fam("ID=rocky\nID_LIKE=\"rhel centos fedora\"\n"), Family::Fedora);
        // An ID we've never seen, but whose ID_LIKE we do know.
        assert_eq!(fam("ID=someneworange\nID_LIKE=debian\n"), Family::Debian);
    }

    #[test]
    fn unknown_stays_unknown_rather_than_guessing() {
        assert_eq!(fam("ID=plan9\n"), Family::Unknown);
        assert_eq!(fam(""), Family::Unknown);
        assert!(Family::Unknown.install_argv(&["gcc".to_string()]).is_none());
    }

    #[test]
    fn pretty_name_is_preferred_for_display() {
        let d = parse_os_release("ID=fedora\nPRETTY_NAME=\"Fedora Linux 41 (Workstation)\"\n");
        assert_eq!(d.name, "Fedora Linux 41 (Workstation)");
        assert_eq!(d.id, "fedora");
        // Falls back to the bare ID when there's no pretty name.
        assert_eq!(parse_os_release("ID=void\n").name, "void");
    }

    #[test]
    fn install_commands_are_non_interactive() {
        let pkgs = vec!["gcc".to_string(), "make".to_string()];
        assert_eq!(
            Family::Debian.install_argv(&pkgs).unwrap(),
            vec!["apt-get", "install", "-y", "--no-install-recommends", "gcc", "make"]
        );
        assert_eq!(
            Family::Arch.install_argv(&pkgs).unwrap(),
            vec!["pacman", "-S", "--needed", "--noconfirm", "gcc", "make"]
        );
        assert_eq!(
            Family::Fedora.install_argv(&pkgs).unwrap(),
            vec!["dnf", "install", "-y", "gcc", "make"]
        );
        // Nothing to install means no command at all, not an empty one.
        assert!(Family::Debian.install_argv(&[]).is_none());
    }
}
