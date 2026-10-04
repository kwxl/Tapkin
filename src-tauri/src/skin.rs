use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Cursor,
    path::{Component, Path, PathBuf},
};
use thiserror::Error;

const MAX_CONFIG_BYTES: u64 = 64 * 1024;
const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_DIMENSION: u32 = 4096;
const MAX_FRAMES: usize = 16;
const MAX_SKIN_BYTES: usize = 32 * 1024 * 1024;
const MAX_CACHED_PIXELS: u64 = 16 * 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct SkinConfig {
    pub name: String,
    pub idle: PathBuf,
    pub typing: Vec<PathBuf>,
    #[serde(default = "default_timeout")]
    pub typing_timeout_ms: u64,
    #[serde(default = "default_hold")]
    pub frame_hold_ms: u64,
}

fn default_timeout() -> u64 {
    180
}
fn default_hold() -> u64 {
    60
}

#[derive(Debug, Error)]
pub enum SkinError {
    #[error("Cannot read skin: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid pet.toml: {0}")]
    Config(#[from] toml::de::Error),
    #[error("{0}")]
    Invalid(String),
}

#[derive(Clone, Serialize)]
pub struct SkinView {
    pub name: String,
    pub directory: PathBuf,
    pub width: u32,
    pub height: u32,
    pub images: Vec<String>,
}

pub struct Skin {
    pub config: SkinConfig,
    pub view: SkinView,
    frames: Vec<Vec<u8>>,
}

fn bounded_read(path: &Path, limit: u64) -> Result<Vec<u8>, SkinError> {
    use std::io::Read;
    let file = fs::File::open(path)
        .map_err(|e| SkinError::Invalid(format!("Cannot read {}: {e}", path.display())))?;
    if !file.metadata()?.is_file() {
        return Err(SkinError::Invalid(format!(
            "{} must be a file",
            path.display()
        )));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(SkinError::Invalid(format!(
            "{} exceeds the size limit",
            path.display()
        )));
    }
    Ok(bytes)
}

fn asset_path(root: &Path, relative: &Path) -> Result<PathBuf, SkinError> {
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || relative
            .extension()
            .and_then(|s| s.to_str())
            .is_none_or(|s| !s.eq_ignore_ascii_case("png"))
    {
        return Err(SkinError::Invalid(format!(
            "Image must be a relative PNG path without '..': {}",
            relative.display()
        )));
    }
    let full = root
        .join(relative)
        .canonicalize()
        .map_err(|e| SkinError::Invalid(format!("Missing image {}: {e}", relative.display())))?;
    if !full.starts_with(root) {
        return Err(SkinError::Invalid(format!(
            "Image escapes the skin folder: {}",
            relative.display()
        )));
    }
    Ok(full)
}

fn png_size(bytes: &[u8], path: &Path) -> Result<(u32, u32), SkinError> {
    let invalid = |e: String| SkinError::Invalid(format!("Invalid PNG {}: {e}", path.display()));
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_limits(png::Limits {
        bytes: 80 * 1024 * 1024,
    });
    let mut reader = decoder.read_info().map_err(|e| invalid(e.to_string()))?;
    let (width, height) = (reader.info().width, reader.info().height);
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(invalid(format!(
            "dimensions must be 1–{MAX_DIMENSION} pixels"
        )));
    }
    if reader.info().animation_control.is_some() {
        return Err(invalid("animated PNGs are not supported".into()));
    }
    if reader.output_buffer_size() > 80 * 1024 * 1024 {
        return Err(invalid("decoded frame exceeds 80 MiB".into()));
    }
    let mut buffer = vec![0; reader.output_buffer_size()];
    reader
        .next_frame(&mut buffer)
        .map_err(|e| invalid(e.to_string()))?;
    Ok((width, height))
}

impl Skin {
    pub fn load(directory: &Path) -> Result<Self, SkinError> {
        let root = directory.canonicalize()?;
        let config_path = root.join("pet.toml").canonicalize()?;
        if !config_path.starts_with(&root) {
            return Err(SkinError::Invalid(
                "pet.toml escapes the skin folder".into(),
            ));
        }
        let bytes = bounded_read(&config_path, MAX_CONFIG_BYTES)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| SkinError::Invalid("pet.toml must be UTF-8".into()))?;
        let config: SkinConfig = toml::from_str(text)?;
        if config.name.trim().is_empty() || config.name.chars().count() > 80 {
            return Err(SkinError::Invalid(
                "Skin name must contain 1–80 characters".into(),
            ));
        }
        if !(2..=MAX_FRAMES).contains(&config.typing.len()) {
            return Err(SkinError::Invalid(format!(
                "typing must contain 2–{MAX_FRAMES} PNG frames"
            )));
        }
        if !(50..=10_000).contains(&config.typing_timeout_ms)
            || config.frame_hold_ms > config.typing_timeout_ms
        {
            return Err(SkinError::Invalid(
                "typing_timeout_ms must be 50–10000; frame_hold_ms must not exceed it".into(),
            ));
        }
        let mut images = Vec::with_capacity(config.typing.len() + 1);
        let mut frames = Vec::with_capacity(config.typing.len() + 1);
        let mut canvas = None;
        let mut total_bytes = 0;
        for relative in std::iter::once(&config.idle).chain(&config.typing) {
            let path = asset_path(&root, relative)?;
            let bytes = bounded_read(&path, MAX_IMAGE_BYTES)?;
            total_bytes += bytes.len();
            if total_bytes > MAX_SKIN_BYTES {
                return Err(SkinError::Invalid(
                    "Referenced PNG files exceed 32 MiB in total".into(),
                ));
            }
            let size = png_size(&bytes, relative)?;
            if u64::from(size.0) * u64::from(size.1) * (config.typing.len() + 1) as u64
                > MAX_CACHED_PIXELS
            {
                return Err(SkinError::Invalid(
                    "Skin frames exceed the 16-megapixel combined canvas budget".into(),
                ));
            }
            if canvas.is_some_and(|expected| expected != size) {
                return Err(SkinError::Invalid(format!(
                    "{} must match the idle image canvas size",
                    relative.display()
                )));
            }
            canvas = Some(size);
            images.push(format!("data:image/png;base64,{}", STANDARD.encode(&bytes)));
            frames.push(bytes);
        }
        let (width, height) = canvas.expect("the required idle frame has a canvas");
        Ok(Self {
            view: SkinView {
                name: config.name.clone(),
                directory: root,
                width,
                height,
                images,
            },
            config,
            frames,
        })
    }

    /// Frames are returned from the validated load, even if files are edited before reload.
    pub fn frame_png(&self, index: usize) -> Option<&[u8]> {
        self.frames.get(index).map(Vec::as_slice)
    }
}

pub fn install_example(skins_dir: &Path) -> Result<PathBuf, std::io::Error> {
    let root = skins_dir.join("example");
    fs::create_dir_all(&root)?;
    for (name, bytes) in [
        (
            "pet.toml",
            include_bytes!("../../skins/example/pet.toml").as_slice(),
        ),
        (
            "idle.png",
            include_bytes!("../../skins/example/idle.png").as_slice(),
        ),
        (
            "typing_1.png",
            include_bytes!("../../skins/example/typing_1.png").as_slice(),
        ),
        (
            "typing_2.png",
            include_bytes!("../../skins/example/typing_2.png").as_slice(),
        ),
    ] {
        let path = root.join(name);
        if !path.exists() {
            fs::write(path, bytes)?;
        }
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        install_example(dir.path()).unwrap();
        dir
    }
    fn config(dir: &Path, text: &str) {
        fs::write(dir.join("pet.toml"), text).unwrap();
    }

    #[test]
    fn valid_minimal_and_ignored_reactions() {
        let dir = example();
        let root = dir.path().join("example");
        config(&root, "name='Cat'\nidle='idle.png'\ntyping=['typing_1.png','typing_2.png']\nhappy='missing.png'");
        let skin = Skin::load(&root).unwrap();
        assert_eq!(skin.view.images.len(), 3);
        assert_eq!(skin.config.typing_timeout_ms, 180);
    }

    #[test]
    fn missing_idle_is_an_error() {
        let dir = example();
        let root = dir.path().join("example");
        fs::remove_file(root.join("idle.png")).unwrap();
        assert!(Skin::load(&root)
            .err()
            .unwrap()
            .to_string()
            .contains("Missing image idle.png"));
    }

    #[test]
    fn empty_typing_and_malformed_toml_are_errors() {
        let dir = example();
        let root = dir.path().join("example");
        config(&root, "name='Cat'\nidle='idle.png'\ntyping=[]");
        assert!(Skin::load(&root).is_err());
        config(&root, "this is not TOML");
        assert!(Skin::load(&root).is_err());
    }

    #[test]
    fn rejects_traversal_absolute_paths_and_corrupt_png() {
        let dir = example();
        let root = dir.path().join("example");
        for path in ["../idle.png", "/tmp/idle.png"] {
            config(
                &root,
                &format!("name='Cat'\nidle='{path}'\ntyping=['typing_1.png','typing_2.png']"),
            );
            assert!(Skin::load(&root).is_err());
        }
        config(
            &root,
            "name='Cat'\nidle='idle.png'\ntyping=['typing_1.png','typing_2.png']",
        );
        fs::write(root.join("idle.png"), b"not a PNG").unwrap();
        assert!(Skin::load(&root).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        let dir = example();
        let outside = tempfile::NamedTempFile::new().unwrap();
        let root = dir.path().join("example");
        fs::remove_file(root.join("idle.png")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("idle.png")).unwrap();
        assert!(Skin::load(&root)
            .err()
            .unwrap()
            .to_string()
            .contains("escapes"));
    }

    #[test]
    fn rejects_mismatched_canvases_and_invalid_timings() {
        let dir = example();
        let root = dir.path().join("example");
        for timing in [
            "typing_timeout_ms=0",
            "frame_hold_ms=181",
            "typing_timeout_ms=10001",
        ] {
            config(
                &root,
                &format!(
                    "name='Cat'\nidle='idle.png'\ntyping=['typing_1.png','typing_2.png']\n{timing}"
                ),
            );
            assert!(Skin::load(&root).is_err());
        }
        config(
            &root,
            "name='Cat'\nidle='idle.png'\ntyping=['typing_1.png','typing_2.png']",
        );
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[0, 0, 0, 0])
                .unwrap();
        }
        fs::write(root.join("typing_1.png"), bytes).unwrap();
        assert!(Skin::load(&root)
            .err()
            .unwrap()
            .to_string()
            .contains("canvas size"));
    }

    #[test]
    fn rejects_outside_config_and_missing_typing_frame() {
        let dir = example();
        let root = dir.path().join("example");
        fs::remove_file(root.join("typing_2.png")).unwrap();
        assert!(Skin::load(&root)
            .err()
            .unwrap()
            .to_string()
            .contains("typing_2.png"));
        #[cfg(unix)]
        {
            let outside = tempfile::NamedTempFile::new().unwrap();
            fs::remove_file(root.join("pet.toml")).unwrap();
            std::os::unix::fs::symlink(outside.path(), root.join("pet.toml")).unwrap();
            assert!(Skin::load(&root)
                .err()
                .unwrap()
                .to_string()
                .contains("escapes"));
        }
    }

    #[test]
    fn installing_example_preserves_user_edits() {
        let dir = example();
        let root = dir.path().join("example");
        fs::write(root.join("pet.toml"), "user edit").unwrap();
        install_example(dir.path()).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("pet.toml")).unwrap(),
            "user edit"
        );
    }
}
