//! Default SoundFont discovery. Explicit CLI arguments always win; otherwise
//! SF2 renderers use MuseScore General from the configured sound library.

use crate::error::{Error, Result};
use crate::tools::Renderer;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const LIBRARY_ENV: &str = "SCOREKIT_SOUND_LIBRARY_DIR";
pub const DEFAULT_NAME: &str = "MuseScore_General.sf2";

pub fn library_dir() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(LIBRARY_ENV).filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(path));
    }
    #[cfg(windows)]
    if let Some(path) = std::env::var_os("LOCALAPPDATA").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(path).join("scorekit").join("sounds"));
    }
    if let Some(path) = std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(path).join("scorekit").join("sounds"));
    }
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|home| home.join(".local/share/scorekit/sounds"))
}

pub fn default_path() -> Option<PathBuf> {
    library_dir().map(|root| root.join("sf2").join(DEFAULT_NAME))
}

pub fn has_sf2_magic(path: &Path) -> bool {
    let mut head = [0u8; 12];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut head))
        .is_ok()
        && &head[0..4] == b"RIFF"
        && &head[8..12] == b"sfbk"
}

pub fn resolve(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }
    let path = default_path().ok_or_else(|| Error::Validation {
        path: "--soundfont".to_owned(),
        message: format!("no default SoundFont location; set {LIBRARY_ENV} or pass --soundfont"),
    })?;
    if path.is_file() {
        Ok(path)
    } else {
        Err(Error::Validation {
            path: "--soundfont".to_owned(),
            message: format!(
                "default SoundFont not found: {}; run `make install-default-soundfont` or pass --soundfont",
                path.display()
            ),
        })
    }
}

pub fn for_renderer(renderer: Renderer, explicit: Option<&Path>) -> Result<Option<PathBuf>> {
    if renderer == Renderer::Sfizz {
        Ok(explicit.map(Path::to_path_buf))
    } else {
        resolve(explicit).map(Some)
    }
}
