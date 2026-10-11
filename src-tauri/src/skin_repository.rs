//! Only immediate children of the managed skins directory are selectable.
use crate::{skin::Skin, storage::Storage};
use serde::Serialize;
use std::fs;

#[derive(Serialize)]
pub struct SkinEntry {
    pub id: String,
    pub name: String,
    pub idle_image: Option<String>,
    pub selected: bool,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct SkinList {
    pub directory: std::path::PathBuf,
    pub skins: Vec<SkinEntry>,
}

pub fn list(storage: &Storage, selected: &std::path::Path) -> Result<SkinList, String> {
    let directory = storage.skins_dir();
    let mut skins = Vec::new();
    for entry in fs::read_dir(&directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.path().is_dir() {
            continue;
        }
        let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let result = storage
            .skin_by_id(&id)
            .map_err(|e| e.to_string())
            .and_then(|path| Skin::load(&path).map_err(|e| e.to_string()));
        let (name, idle_image, is_selected, error) = match result {
            Ok(skin) => (
                skin.config.name,
                skin.view.images.first().cloned(),
                skin.view.directory == selected,
                None,
            ),
            Err(error) => (id.clone(), None, false, Some(error)),
        };
        skins.push(SkinEntry {
            id,
            name,
            idle_image,
            selected: is_selected,
            error,
        });
    }
    skins.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    Ok(SkinList { directory, skins })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skin::install_example;

    #[test]
    fn lists_valid_and_invalid_skins_and_rejects_external_paths() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage {
            data_dir: dir.path().to_path_buf(),
            settings_path: dir.path().join("settings.json"),
            portable_root: None,
        };
        let example = install_example(&storage.skins_dir()).unwrap();
        fs::create_dir(storage.skins_dir().join("broken")).unwrap();
        let selected = fs::canonicalize(example).unwrap();
        let entries = list(&storage, &selected).unwrap().skins;
        assert_eq!(entries.len(), 2);
        let idle = entries
            .iter()
            .find(|entry| entry.id == "example")
            .unwrap()
            .idle_image
            .as_ref()
            .unwrap();
        assert_eq!(idle, &Skin::load(&selected).unwrap().view.images[0]);
        assert!(idle.starts_with("data:image/png;base64,"));
        assert!(entries
            .iter()
            .find(|entry| entry.id == "broken")
            .unwrap()
            .idle_image
            .is_none());
        assert!(entries
            .iter()
            .any(|entry| entry.id == "example" && entry.selected && entry.error.is_none()));
        assert!(entries
            .iter()
            .any(|entry| entry.id == "broken" && entry.error.is_some()));
        for id in ["../example", "", ".", "example/nested"] {
            assert!(storage.skin_by_id(id).is_err());
        }
        assert!(storage.validate_skin(dir.path()).is_err());
        fs::create_dir(storage.skins_dir().join("example/nested")).unwrap();
        assert!(storage
            .validate_skin(&storage.skins_dir().join("example/nested"))
            .is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path(), storage.skins_dir().join("escape")).unwrap();
            assert!(storage.skin_by_id("escape").is_err());
        }
        let installed_settings = crate::config::AppSettings {
            selected_skin: selected,
            ..Default::default()
        };
        storage.save(&installed_settings).unwrap();
        assert_eq!(
            crate::config::AppSettings::load(&storage.settings_path)
                .0
                .selected_skin,
            std::path::PathBuf::from("skins/example")
        );
    }
}
