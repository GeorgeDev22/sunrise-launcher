//! Saved skins: the skins a player added to the Skins tab, kept in the launcher's data folder
//! (`saved-skins/`: each skin's PNG under its id, and `index.json` with their names and arm
//! models) so they can be worn again with one click. They leave the computer only when worn,
//! and then only to the Minecraft services.
use std::path::{Path, PathBuf};

/// One saved skin.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Saved {
    /// The first 16 hex digits of the PNG's SHA-1: the same picture is saved once.
    pub id: String,
    pub name: String,
    pub slim: bool,
}

/// The folder saved skins are kept in.
pub fn folder() -> PathBuf {
    crate::backend::data_dir().join("saved-skins")
}

/// The saved skins in `dir`, oldest first; those whose picture is gone are left out.
pub fn list(dir: &Path) -> Vec<Saved> {
    std::fs::read_to_string(dir.join("index.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<Saved>>(&text).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|s| valid_id(&s.id) && png_path(dir, &s.id).is_file())
        .collect()
}

/// A saved skin's picture.
pub fn png(dir: &Path, id: &str) -> Option<Vec<u8>> {
    valid_id(id)
        .then(|| std::fs::read(png_path(dir, id)).ok())
        .flatten()
}

fn png_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.png"))
}

fn valid_id(id: &str) -> bool {
    id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

fn save(dir: &Path, list: &[Saved]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("index.json.{}.tmp", std::process::id()));
    std::fs::write(&tmp, text + "\n")
        .and_then(|()| std::fs::rename(&tmp, dir.join("index.json")))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            e.to_string()
        })
}

/// Saves the skin `png` as `name`, its arms as the picture suggests
/// ([`crate::player_model::looks_slim`]); the one already saved when the same picture is.
pub fn add(dir: &Path, png: &[u8], name: &str) -> Result<Saved, String> {
    use sha1::Digest;
    if png.len() > crate::account::skins::MAX_SKIN {
        return Err("This picture is too big for a skin.".into());
    }
    let skin = crate::player_model::skin(png)
        .ok_or("This isn't a Minecraft skin: it must be a 64×64 or 64×32 PNG.")?;
    let id: String = sha1::Sha1::digest(png)
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect();
    let mut all = list(dir);
    if let Some(known) = all.iter().find(|s| s.id == id) {
        return Ok(known.clone());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::write(png_path(dir, &id), png).map_err(|e| e.to_string())?;
    let name: String = name.trim().chars().take(40).collect();
    let saved = Saved {
        id,
        name: if name.is_empty() { "Skin".into() } else { name },
        slim: crate::player_model::looks_slim(&skin),
    };
    all.push(saved.clone());
    save(dir, &all)?;
    Ok(saved)
}

/// Gives the saved skin `id` slim or classic arms.
pub fn set_slim(dir: &Path, id: &str, slim: bool) -> Result<(), String> {
    let mut all = list(dir);
    match all.iter_mut().find(|s| s.id == id) {
        Some(s) if s.slim != slim => s.slim = slim,
        _ => return Ok(()),
    }
    save(dir, &all)
}

/// Forgets the saved skin `id` and deletes its picture.
pub fn remove(dir: &Path, id: &str) -> Result<(), String> {
    let mut all = list(dir);
    all.retain(|s| s.id != id);
    save(dir, &all)?;
    if valid_id(id) {
        let _ = std::fs::remove_file(png_path(dir, id));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(w: u32, h: u32, slim: bool) -> Vec<u8> {
        let image = image::RgbaImage::from_fn(w, h, |x, y| {
            let clear = slim && x == 54 && (20..32).contains(&y);
            image::Rgba(if clear {
                [0; 4]
            } else {
                [x as u8, y as u8, 3, 255]
            })
        });
        let mut out = std::io::Cursor::new(Vec::new());
        image.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn skins_are_saved_once_listed_changed_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path().join("saved-skins");
        assert!(list(&dir).is_empty());
        let first = add(&dir, &picture(64, 64, false), "  Knight  ").unwrap();
        assert_eq!((first.name.as_str(), first.slim), ("Knight", false));
        let slim = add(&dir, &picture(64, 64, true), "").unwrap();
        assert!(slim.slim, "the clear arm column says slim");
        assert_eq!(slim.name, "Skin");
        assert_eq!(add(&dir, &picture(64, 64, false), "Again").unwrap(), first);
        assert_eq!(list(&dir), [first.clone(), slim.clone()]);
        assert_eq!(png(&dir, &first.id), Some(picture(64, 64, false)));
        set_slim(&dir, &first.id, true).unwrap();
        assert!(list(&dir)[0].slim);
        remove(&dir, &first.id).unwrap();
        assert_eq!(list(&dir), [slim]);
        assert!(png(&dir, &first.id).is_none());
    }

    #[test]
    fn only_skins_are_saved_and_odd_ids_never_reach_the_disk() {
        let dir = tempfile::tempdir().unwrap();
        assert!(add(dir.path(), &picture(46, 22, false), "Cape").is_err());
        assert!(add(dir.path(), b"text", "Text").is_err());
        assert!(png(dir.path(), "../../etc/passwd").is_none());
        assert!(remove(dir.path(), "../x").is_ok());
        assert!(list(dir.path()).is_empty());
    }
}
