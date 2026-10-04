use crate::window::{Position, Size};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppSettings {
    pub selected_skin: PathBuf,
    pub window_position: Option<Position>,
    pub window_size: Size,
    pub always_on_top: bool,
    pub click_through: bool,
    pub lock_position: bool,
    pub launch_at_login: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            selected_skin: PathBuf::new(),
            window_position: None,
            window_size: Size {
                width: 240.0,
                height: 240.0,
            },
            always_on_top: true,
            click_through: false,
            lock_position: false,
            launch_at_login: false,
        }
    }
}

impl AppSettings {
    pub fn load(path: &Path) -> (Self, Option<String>) {
        match fs::read_to_string(path) {
            Ok(text) => match serde_json::from_str::<Self>(&text) {
                Ok(settings) if settings.valid() => (settings, None),
                _ => (
                    Self::default(),
                    Some("Saved settings were invalid. Tapkin restored defaults.".into()),
                ),
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => (Self::default(), None),
            Err(e) => (
                Self::default(),
                Some(format!("Could not read settings: {e}")),
            ),
        }
    }

    fn valid(&self) -> bool {
        [self.window_size.width, self.window_size.height]
            .iter()
            .all(|n| n.is_finite() && *n > 0.0 && *n <= 800.0)
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        // Replace the old file only after a complete, flushed write (also works on Windows).
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("Settings path needs a parent"))?;
        fs::create_dir_all(parent)?;
        let bytes = serde_json::to_vec_pretty(self)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        use io::Write;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|e| e.error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_round_trip_and_atomic_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut settings = AppSettings {
            selected_skin: PathBuf::from("cat"),
            ..AppSettings::default()
        };
        settings.save(&path).unwrap();
        settings.click_through = true;
        settings.save(&path).unwrap();
        let (loaded, warning) = AppSettings::load(&path);
        assert_eq!(loaded, settings);
        assert!(warning.is_none());
    }
    #[test]
    fn corrupted_settings_fall_back_and_missing_fields_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "broken").unwrap();
        assert_eq!(AppSettings::load(&path).0, AppSettings::default());
        assert!(AppSettings::load(&path).1.is_some());
        fs::write(&path, r#"{"click_through":true}"#).unwrap();
        assert!(AppSettings::load(&path).0.click_through);
        fs::write(&path, r#"{"window_size":{"width":-1,"height":100}}"#).unwrap();
        assert!(AppSettings::load(&path).1.is_some());
    }
}
