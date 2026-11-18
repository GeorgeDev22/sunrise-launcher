//! A Sunrise build's first start. The launcher before it (Sunrise's own, then the arismoko
//! fork's: `identity::EARLIER`) left the player's settings, accounts and instances; they are
//! copied into Sunrise's own folders, never moved, so the player keeps them and the earlier
//! launcher keeps working. The earlier launcher's folders are only read.
use crate::data_location::{self, Roots};
use std::fs;
use std::path::Path;

/// What a launcher folder holds that is the player's: settings, accounts (with the secrets file
/// of a computer without a keychain), the instances' names and icons, the servers made in the
/// launcher. Game builds, downloads, caches and logs are made again.
const LAUNCHER_STATE: [&str; 7] = [
    "settings.json",
    "accounts.json",
    crate::account::SECRETS_FILE,
    "account-secrets.key",
    "instances.json",
    "servers.json",
    "servers",
];

/// Runs [`carry_over`] on the player's folders before anything reads them, unless a home
/// override or a moved Sunrise location decides where the data is. A failure is reported and
/// leaves the launcher to start empty: the earlier launcher's data is untouched either way.
pub fn at_start() {
    let overridden = ["SUNRISE_LAUNCHER_HOME", "SUNRISE_HOME"]
        .iter()
        .any(|name| std::env::var_os(name).is_some_and(|v| !v.is_empty()));
    if crate::identity::EARLIER.is_empty()
        || overridden
        || !matches!(data_location::configured_roots(), Ok(None))
    {
        return;
    }
    let (Some(data), Some(config)) = (dirs::data_dir(), dirs::config_dir()) else {
        return;
    };
    if let Err(error) = carry_over(&data, &config) {
        eprintln!("Could not copy the earlier launcher's data: {error}");
    }
}

/// Copies the data of the first earlier launcher that has any into this launcher's folders under
/// `data`, when neither this launcher's data folder nor its game folder exists yet. Earlier
/// launchers' folders are looked for under `data`, or where their `location.json` under `config`
/// moved them. Returns whether anything was copied.
///
/// Everything is copied into a folder beside the targets first and renamed into place at the
/// end, so a start that stops halfway copies again, from the beginning, next time.
pub fn carry_over(data: &Path, config: &Path) -> Result<bool, String> {
    let launcher = crate::identity::launcher_folder_in(data);
    let game = crate::identity::game_folder_in(data);
    let stage = launcher.with_file_name(".carry-over");
    if launcher.exists() || game.exists() {
        // A copy that reached both targets but could not remove its stage (a file appearing in
        // it mid-delete) left it beside them; nothing reads it, so it goes on a later start.
        if launcher.exists() && game.exists() && stage.exists() {
            discard(&stage);
        }
        return Ok(false);
    }
    let Some(source) = crate::identity::EARLIER
        .iter()
        .find_map(|&(launcher, game, location)| {
            earlier_roots(data, config, launcher, game, location)
        })
    else {
        return Ok(false);
    };
    if stage.exists() {
        fs::remove_dir_all(&stage).map_err(|e| format!("{}: {e}", stage.display()))?;
    }
    let staged = Roots {
        launcher: stage.join("launcher"),
        game: stage.join("game"),
        build: stage.join("launcher"),
    };
    fs::create_dir_all(&staged.launcher).map_err(|e| format!("{}: {e}", stage.display()))?;
    // Nothing to skip: the copy never runs inside this launcher's own location folder.
    let bootstrap = stage.join("location.json");
    let mut quiet = |_: &str| {};
    for name in LAUNCHER_STATE {
        let from = source.launcher.join(name);
        if from.exists() {
            data_location::copy_tree(&from, &staged.launcher.join(name), &bootstrap, &mut quiet)?;
        }
    }
    data_location::copy_tree(&source.game, &staged.game, &bootstrap, &mut quiet)?;
    let target = Roots {
        launcher: launcher.clone(),
        game: game.clone(),
        build: launcher.clone(),
    };
    crate::relocation::prepare_paths(&staged, &source, &target)?;
    fs::rename(&staged.launcher, &launcher).map_err(|e| format!("{}: {e}", launcher.display()))?;
    fs::rename(&staged.game, &game).map_err(|e| format!("{}: {e}", game.display()))?;
    discard(&stage);
    Ok(true)
}

/// Removes the stage of a finished copy. A failure is reported and leaves it for the next start.
fn discard(stage: &Path) {
    if let Err(error) = fs::remove_dir_all(stage) {
        eprintln!(
            "{}",
            crate::i18n::trf(
                "Could not remove {path}, left from copying the earlier launcher's data; the next start tries again: {error}",
                &[
                    ("path", &stage.display().to_string()),
                    ("error", &error.to_string()),
                ],
            )
        );
    }
}

/// An earlier launcher's folders, when its launcher or game folder exists: where its
/// `location.json` moved them, else its default folders under `data`.
fn earlier_roots(
    data: &Path,
    config: &Path,
    launcher: &str,
    game: &str,
    location: &str,
) -> Option<Roots> {
    let moved = data_location::resolve_at(&config.join(location).join("location.json"))
        .ok()
        .flatten();
    let roots = moved.unwrap_or_else(|| Roots {
        launcher: data.join(launcher),
        game: data.join(game),
        build: data.join(launcher),
    });
    (roots.launcher.is_dir() || roots.game.is_dir()).then_some(roots)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    /// Every file under `root`, with its bytes.
    fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut out = BTreeMap::new();
        let mut todo = vec![root.to_path_buf()];
        while let Some(dir) = todo.pop() {
            for entry in fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    todo.push(path);
                } else {
                    out.insert(
                        path.strip_prefix(root).unwrap().to_path_buf(),
                        fs::read(&path).unwrap(),
                    );
                }
            }
        }
        out
    }

    #[test]
    fn the_earlier_launchers_data_is_copied_once_and_left_untouched() {
        let Some(&(old_launcher, old_game, _)) = crate::identity::EARLIER.first() else {
            return;
        };
        let base = std::env::temp_dir().join(format!("hz-carry-over-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (data, config) = (base.join("data"), base.join("config"));
        let (from_launcher, from_game) = (data.join(old_launcher), data.join(old_game));
        let background = from_game.join("screenshots/a.png");
        fs::create_dir_all(from_launcher.join("cache")).unwrap();
        fs::create_dir_all(from_game.join("saves/World")).unwrap();
        fs::create_dir_all(from_game.join("instances/modded")).unwrap();
        let settings = serde_json::json!({ "background": background, "language": "fr" });
        fs::write(from_launcher.join("settings.json"), settings.to_string()).unwrap();
        fs::write(from_launcher.join("accounts.json"), r#"{"accounts":[]}"#).unwrap();
        fs::write(from_launcher.join("cache/big.bin"), [7; 64]).unwrap();
        fs::write(from_game.join("saves/World/level.dat"), [1, 2, 3]).unwrap();
        fs::write(from_game.join("instances/modded/options.txt"), "fov:90").unwrap();
        let before = (files(&from_launcher), files(&from_game));

        assert_eq!(carry_over(&data, &config), Ok(true));
        assert_eq!(
            (files(&from_launcher), files(&from_game)),
            before,
            "the earlier launcher's folders are unchanged"
        );
        let launcher = crate::identity::launcher_folder_in(&data);
        let game = crate::identity::game_folder_in(&data);
        assert_eq!(
            fs::read(game.join("saves/World/level.dat")).unwrap(),
            [1, 2, 3]
        );
        assert_eq!(
            fs::read_to_string(game.join("instances/modded/options.txt")).unwrap(),
            "fov:90"
        );
        assert!(launcher.join("accounts.json").is_file());
        assert!(!launcher.join("cache").exists(), "caches are made again");
        // A path into the earlier game folder now names the copy.
        let copied: serde_json::Value =
            serde_json::from_slice(&fs::read(launcher.join("settings.json")).unwrap()).unwrap();
        assert_eq!(
            copied["background"].as_str().map(PathBuf::from),
            Some(game.join("screenshots/a.png"))
        );
        assert_eq!(copied["language"], "fr");

        // Only the first start copies: this launcher's own data is never replaced.
        fs::write(from_game.join("saves/World/level.dat"), [9]).unwrap();
        assert_eq!(carry_over(&data, &config), Ok(false));
        assert_eq!(
            fs::read(game.join("saves/World/level.dat")).unwrap(),
            [1, 2, 3]
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn a_stage_left_beside_a_finished_copy_is_removed_on_a_later_start() {
        let base = std::env::temp_dir().join(format!("hz-carry-stage-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (data, config) = (base.join("data"), base.join("config"));
        let launcher = crate::identity::launcher_folder_in(&data);
        let game = crate::identity::game_folder_in(&data);
        let stage = launcher.with_file_name(".carry-over");
        fs::create_dir_all(stage.join("game/saves")).unwrap();
        fs::write(stage.join("game/saves/level.dat"), [1]).unwrap();
        fs::create_dir_all(&launcher).unwrap();
        fs::write(launcher.join("accounts.json"), "{}").unwrap();

        // Only the launcher folder: the copy may not have finished, so its stage stays.
        assert_eq!(carry_over(&data, &config), Ok(false));
        assert!(stage.join("game/saves/level.dat").is_file());

        fs::create_dir_all(game.join("saves")).unwrap();
        fs::write(game.join("saves/level.dat"), [2]).unwrap();
        assert_eq!(carry_over(&data, &config), Ok(false));
        assert!(!stage.exists());
        assert_eq!(fs::read(game.join("saves/level.dat")).unwrap(), [2]);
        assert_eq!(fs::read(launcher.join("accounts.json")).unwrap(), b"{}");
        let _ = fs::remove_dir_all(&base);
    }
}
