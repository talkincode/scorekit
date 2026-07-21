//! Portable texture-source profiles for field recordings, ambience and SFX.
//!
//! Scene files name a logical source (`river`, `birds`, `engine_idle`); this
//! profile is the machine-local binding to an audio file. Keeping the path
//! here preserves the same portability boundary renderer profiles provide
//! for SFZ instruments.

use crate::error::{Error, Location, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextureProfile {
    /// Human-readable profile name.
    pub name: String,
    /// What recordings or library this profile binds, for humans.
    #[serde(default)]
    pub description: Option<String>,
    /// Source root. Relative paths resolve from the profile file directory;
    /// absent means the profile file directory itself.
    #[serde(default)]
    pub root: Option<String>,
    /// Portable source name -> audio file path (WAV, FLAC, OGG, etc.).
    pub sources: BTreeMap<String, String>,
}

pub fn valid_source_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.bytes().enumerate().all(|(i, b)| {
            b.is_ascii_lowercase() || (i > 0 && (b.is_ascii_digit() || b == b'_' || b == b'-'))
        })
}

impl TextureProfile {
    pub fn validate(&self) -> Result<()> {
        if self.sources.is_empty() {
            return Err(Error::Validation {
                path: "sources".to_owned(),
                message: "texture profile maps no sources".to_owned(),
            });
        }
        for (name, path) in &self.sources {
            if !valid_source_name(name) {
                return Err(Error::Validation {
                    path: format!("sources.{name}"),
                    message: format!(
                        "`{name}` must match [a-z][a-z0-9_-]{{0,63}} (portable source name)"
                    ),
                });
            }
            if path.trim().is_empty() {
                return Err(Error::Validation {
                    path: format!("sources.{name}"),
                    message: "audio path must not be empty".to_owned(),
                });
            }
        }
        Ok(())
    }

    fn resolved_root(&self, profile_dir: &Path) -> PathBuf {
        match &self.root {
            Some(root) if Path::new(root).is_absolute() => PathBuf::from(root),
            Some(root) => profile_dir.join(root),
            None => profile_dir.to_path_buf(),
        }
    }

    pub fn resolve(&self, profile_dir: &Path, source: &str) -> Result<PathBuf> {
        let rel = self.sources.get(source).ok_or_else(|| Error::Validation {
            path: format!("texture_profile.sources.{source}"),
            message: format!(
                "texture profile `{}` has no mapping for source `{source}`",
                self.name
            ),
        })?;
        Ok(self.resolved_root(profile_dir).join(rel))
    }
}

pub fn load_profile(path: &Path) -> Result<TextureProfile> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })?;
    let profile: TextureProfile = serde_yaml_ng::from_str(&text).map_err(|e| Error::Parse {
        message: format!("invalid texture profile: {e}"),
        location: e.location().map(|l| Location {
            line: l.line(),
            column: l.column(),
        }),
    })?;
    profile.validate()?;
    Ok(profile)
}

pub fn schema_json() -> String {
    let schema = schemars::schema_for!(TextureProfile);
    serde_json::to_string_pretty(&schema).expect("schema serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_portable_source_relative_to_profile() {
        let profile = TextureProfile {
            name: "field".to_owned(),
            description: None,
            root: Some("audio".to_owned()),
            sources: BTreeMap::from([("river".to_owned(), "river.flac".to_owned())]),
        };
        profile.validate().unwrap();
        assert_eq!(
            profile.resolve(Path::new("/profiles"), "river").unwrap(),
            Path::new("/profiles/audio/river.flac")
        );
        assert!(profile.resolve(Path::new("/profiles"), "birds").is_err());
    }

    #[test]
    fn rejects_nonportable_or_empty_mappings() {
        for name in ["River", "../river", "river.wav", ""] {
            let profile = TextureProfile {
                name: "bad".to_owned(),
                description: None,
                root: None,
                sources: BTreeMap::from([(name.to_owned(), "x.wav".to_owned())]),
            };
            assert!(profile.validate().is_err(), "accepted {name:?}");
        }
    }
}
