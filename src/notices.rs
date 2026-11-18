//! The licence notices of what the launcher embeds (fonts, icons) and derives from, for the
//! routes that write files themselves: Windows Setup and a self-update of its install.
//! `packaging/notices.txt` lists the same files for every other route; the test below keeps the
//! two lists, and the scripts that read the second, in step.

/// File name in the `LICENSES` folder, and the notice.
pub const FILES: &[(&str, &str)] = &[
    (
        "MIT-Horizon.txt",
        include_str!("../assets/LICENSES/MIT-Horizon.txt"),
    ),
    (
        "OFL-Figtree.txt",
        include_str!("../assets/fonts/OFL-Figtree.txt"),
    ),
    (
        "OFL-Geist.txt",
        include_str!("../crates/theme/fonts/OFL-Geist.txt"),
    ),
    (
        "OFL-BricolageGrotesque.txt",
        include_str!("../crates/theme/fonts/OFL-BricolageGrotesque.txt"),
    ),
    (
        "OFL-PixelifySans.txt",
        include_str!("../crates/theme/fonts/OFL-PixelifySans.txt"),
    ),
    (
        "MIT-Pixelarticons.txt",
        include_str!("../crates/theme/icons/pixel/LICENSE-pixelarticons.txt"),
    ),
    (
        "MIT-Hugeicons.txt",
        include_str!("../crates/theme/icons/glass/LICENSE-hugeicons.txt"),
    ),
];

/// Writes every notice into `dir/LICENSES`.
pub fn write_into(dir: &std::path::Path) -> std::io::Result<()> {
    let folder = dir.join("LICENSES");
    std::fs::create_dir_all(&folder)?;
    for (name, text) in FILES {
        std::fs::write(folder.join(name), text)?;
    }
    Ok(())
}

/// Writes the notices that are missing from `dir/LICENSES`, and none that are there.
pub fn write_missing(dir: &std::path::Path) -> std::io::Result<()> {
    let folder = dir.join("LICENSES");
    for (name, text) in FILES {
        if !folder.join(name).is_file() {
            std::fs::create_dir_all(&folder)?;
            std::fs::write(folder.join(name), text)?;
        }
    }
    Ok(())
}

/// The folder a binary-only self-update (Linux tarball, portable ZIP, a Windows install) leaves
/// the notices in: the binary's own. None for what carries its notices inside itself or is
/// not the launcher's to write in: an AppImage, a `.app` bundle, a system folder.
pub fn beside(exe: &std::path::Path, appimage: bool, bundle: bool) -> Option<&std::path::Path> {
    let dir = exe.parent()?;
    let system = ["/usr/bin", "/usr/local/bin", "/bin"]
        .iter()
        .any(|s| dir == std::path::Path::new(s));
    (!appimage && !bundle && !system).then_some(dir)
}

/// Removes the notices [`write_into`] wrote, and their folder when nothing else is in it.
#[cfg(any(windows, test))]
pub fn remove_from(dir: &std::path::Path) -> Vec<String> {
    let folder = dir.join("LICENSES");
    let mut left = Vec::new();
    for (name, _) in FILES {
        match std::fs::remove_file(folder.join(name)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => left.push(format!("{}: {e}", folder.join(name).display())),
        }
    }
    let _ = std::fs::remove_dir(&folder);
    left
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn a_binary_only_update_puts_the_notices_beside_the_binary_and_only_there() {
        let exe = Path::new("/home/p/sunrise/sunrise-launcher");
        assert_eq!(
            beside(exe, false, false),
            Some(Path::new("/home/p/sunrise"))
        );
        assert_eq!(
            beside(exe, true, false),
            None,
            "an AppImage holds them itself"
        );
        assert_eq!(beside(exe, false, true), None, "a bundle holds them itself");
        assert_eq!(
            beside(Path::new("/usr/bin/sunrise-launcher"), false, false),
            None
        );
        let dir = std::env::temp_dir().join(format!("notices-beside-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        write_missing(&dir).unwrap();
        for (name, text) in FILES {
            assert_eq!(
                &std::fs::read_to_string(dir.join("LICENSES").join(name)).unwrap(),
                text
            );
        }
        // A file the player or a newer package changed is left alone at first start.
        let first = dir.join("LICENSES").join(FILES[0].0);
        std::fs::write(&first, "mine").unwrap();
        std::fs::remove_file(dir.join("LICENSES").join(FILES[1].0)).unwrap();
        write_missing(&dir).unwrap();
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "mine");
        assert!(dir.join("LICENSES").join(FILES[1].0).is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
    }

    fn listed() -> Vec<(String, String)> {
        std::fs::read_to_string(root().join("packaging/notices.txt"))
            .unwrap()
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| {
                let (name, source) = l.split_once(' ').unwrap();
                (name.to_string(), source.to_string())
            })
            .collect()
    }

    #[test]
    fn the_packaging_list_and_the_embedded_notices_are_the_same_files() {
        let listed = listed();
        assert_eq!(listed.len(), FILES.len());
        for ((name, text), (listed_name, source)) in FILES.iter().zip(&listed) {
            assert_eq!(name, listed_name);
            let on_disk = std::fs::read_to_string(root().join(source)).unwrap();
            assert_eq!(*text, on_disk, "{name} differs from {source}");
        }
    }

    #[test]
    fn every_embedded_asset_has_its_notice_in_the_list() {
        let names: Vec<_> = FILES.iter().map(|(n, _)| *n).collect();
        // Fonts and icons bundled into the binary: Figtree here, Geist, Bricolage and Pixelify
        // Sans and both icon sets in the theme crate.
        for needed in [
            "OFL-Figtree.txt",
            "OFL-Geist.txt",
            "OFL-BricolageGrotesque.txt",
            "OFL-PixelifySans.txt",
            "MIT-Pixelarticons.txt",
            "MIT-Hugeicons.txt",
            "MIT-Horizon.txt",
        ] {
            assert!(names.contains(&needed), "{needed}");
        }
        let fonts = std::fs::read_to_string(root().join("crates/theme/src/lib.rs")).unwrap();
        for family in ["Geist", "Bricolage", "Pixelify"] {
            if fonts.contains(family) {
                assert!(names.iter().any(|n| n.contains(family)), "{family}");
            }
        }
    }

    #[test]
    fn every_route_copies_the_list() {
        let script = std::fs::read_to_string(root().join("packaging/copy-notices.sh")).unwrap();
        assert!(script.contains("packaging/notices.txt"));
        for route in [
            "packaging/linux/appimage.sh",
            "packaging/macos/package.sh",
            ".github/workflows/release.yml",
        ] {
            let text = std::fs::read_to_string(root().join(route)).unwrap();
            assert!(text.contains("copy-notices.sh"), "{route}");
            for (name, _) in FILES {
                assert!(
                    !text.contains(name),
                    "{route} names {name}: the list is the only place"
                );
            }
        }
    }

    #[test]
    fn setup_writes_and_uninstall_removes_every_notice() {
        let dir = std::env::temp_dir().join(format!("sunrise-notices-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write_into(&dir).unwrap();
        for (name, text) in FILES {
            assert_eq!(
                &std::fs::read_to_string(dir.join("LICENSES").join(name)).unwrap(),
                text
            );
        }
        assert!(remove_from(&dir).is_empty());
        assert!(!dir.join("LICENSES").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
