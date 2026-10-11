//! Installed and executable-relative portable storage, independent of Tauri.
use crate::config::AppSettings;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Storage {
    pub data_dir: PathBuf,
    pub settings_path: PathBuf,
    pub portable_root: Option<PathBuf>,
}

impl Storage {
    pub fn discover(
        executable: &Path,
        windows: bool,
        data_dir: PathBuf,
        settings_path: PathBuf,
    ) -> io::Result<Self> {
        let root = executable
            .parent()
            .ok_or_else(|| io::Error::other("Executable has no parent directory"))?;
        if windows && root.join("portable.json").is_file() {
            let marker: serde_json::Value =
                serde_json::from_slice(&fs::read(root.join("portable.json"))?)?;
            if marker.get("version").and_then(|value| value.as_u64()) != Some(1) {
                return Err(io::Error::other("Unsupported portable.json version"));
            }
            let root = fs::canonicalize(root)?;
            // Test actual write access, including the ability to create atomic-save files.
            tempfile::NamedTempFile::new_in(&root).map_err(|error| {
                io::Error::other(format!(
                    "Portable folder is not writable. Move Tapkin to a writable folder: {error}"
                ))
            })?;
            fs::create_dir_all(root.join("webview-data"))?;
            Ok(Self {
                data_dir: root.clone(),
                settings_path: root.join("settings.json"),
                portable_root: Some(root),
            })
        } else {
            Ok(Self {
                data_dir,
                settings_path,
                portable_root: None,
            })
        }
    }

    pub fn resolve_skin(&self, path: &Path) -> PathBuf {
        if !path.as_os_str().is_empty() && path.is_relative() {
            self.data_dir.join(path)
        } else {
            path.to_path_buf()
        }
    }

    pub fn skins_dir(&self) -> PathBuf {
        self.data_dir.join("skins")
    }

    pub fn validate_skin(&self, path: &Path) -> io::Result<PathBuf> {
        let root = fs::canonicalize(self.skins_dir())?;
        let path = fs::canonicalize(path)?;
        if path.parent() != Some(root.as_path()) || !path.is_dir() {
            return Err(io::Error::other("Choose a skin directly inside the skins folder. Move external skins into that folder first."));
        }
        Ok(path)
    }

    pub fn skin_by_id(&self, id: &str) -> io::Result<PathBuf> {
        let mut components = Path::new(id).components();
        if !matches!(components.next(), Some(std::path::Component::Normal(_)))
            || components.next().is_some()
        {
            return Err(io::Error::other("Invalid skin folder identifier"));
        }
        self.validate_skin(&self.skins_dir().join(id))
    }

    pub fn save(&self, settings: &AppSettings) -> io::Result<()> {
        let mut settings = settings.clone();
        let path = self.validate_skin(&self.resolve_skin(&settings.selected_skin))?;
        let name = path
            .file_name()
            .ok_or_else(|| io::Error::other("Skin has no folder name"))?;
        settings.selected_skin = PathBuf::from("skins").join(name);
        if self.portable_root.is_some() {
            settings.launch_at_login = false;
        }
        settings.save(&self.settings_path)
    }

    pub fn webview_data(&self) -> Option<PathBuf> {
        self.portable_root
            .as_ref()
            .map(|root| root.join("webview-data"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storage(root: &Path) -> Storage {
        Storage::discover(
            &root.join("Tapkin.exe"),
            true,
            root.join("installed-data"),
            root.join("installed-config/settings.json"),
        )
        .unwrap()
    }

    #[test]
    fn installed_paths_remain_unchanged_without_marker() {
        let dir = tempfile::tempdir().unwrap();
        let paths = storage(dir.path());
        assert!(paths.portable_root.is_none());
        assert_eq!(paths.data_dir, dir.path().join("installed-data"));
        assert_eq!(
            paths.settings_path,
            dir.path().join("installed-config/settings.json")
        );
    }

    #[test]
    fn portable_settings_and_skin_survive_folder_move() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("original");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("portable.json"), r#"{"version":1}"#).unwrap();
        let paths = storage(&root);
        let example = crate::skin::install_example(&paths.data_dir.join("skins")).unwrap();
        let settings = AppSettings {
            selected_skin: fs::canonicalize(example).unwrap(),
            launch_at_login: true,
            ..Default::default()
        };
        paths.save(&settings).unwrap();
        let saved = AppSettings::load(&paths.settings_path).0;
        assert_eq!(saved.selected_skin, PathBuf::from("skins/example"));
        assert!(!saved.launch_at_login);
        let moved = dir.path().join("moved");
        fs::rename(&root, &moved).unwrap();
        let paths = storage(&moved);
        assert!(crate::skin::Skin::load(&paths.resolve_skin(&saved.selected_skin)).is_ok());
        assert!(paths.webview_data().unwrap().is_dir());
        assert_eq!(paths.resolve_skin(&PathBuf::from("")), PathBuf::from(""));
        let external = dir.path().join("external");
        let settings = AppSettings {
            selected_skin: external.clone(),
            ..Default::default()
        };
        assert!(paths.save(&settings).is_err());
    }

    #[test]
    fn packaged_defaults_match_application_defaults() {
        let settings: AppSettings =
            serde_json::from_str(include_str!("../../portable/settings.json")).unwrap();
        assert_eq!(
            settings,
            AppSettings {
                selected_skin: PathBuf::from("skins/example"),
                ..Default::default()
            }
        );
    }

    #[test]
    fn missing_settings_and_skin_files_recover_without_overwriting_edits() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("portable.json"), r#"{"version":1}"#).unwrap();
        let paths = storage(dir.path());
        let (settings, warning) = AppSettings::load(&paths.settings_path);
        assert_eq!(settings, AppSettings::default());
        assert!(warning.is_none());
        let example = crate::skin::install_example(&paths.data_dir.join("skins")).unwrap();
        fs::write(example.join("pet.toml"), "user edit").unwrap();
        fs::remove_file(example.join("idle.png")).unwrap();
        crate::skin::install_example(&paths.data_dir.join("skins")).unwrap();
        assert_eq!(
            fs::read_to_string(example.join("pet.toml")).unwrap(),
            "user edit"
        );
        assert!(example.join("idle.png").is_file());
        fs::write(&paths.settings_path, "invalid").unwrap();
        assert!(AppSettings::load(&paths.settings_path).1.is_some());
    }

    #[test]
    fn marker_is_windows_only_and_invalid_versions_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("portable.json"), r#"{"version":2}"#).unwrap();
        let discover = |windows| {
            Storage::discover(
                &dir.path().join("Tapkin.exe"),
                windows,
                dir.path().join("data"),
                dir.path().join("config"),
            )
        };
        assert!(discover(true).is_err());
        assert!(discover(false).unwrap().portable_root.is_none());
    }
}
