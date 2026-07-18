//! Renderer profiles for SFZ-based rendering (`--renderer sfizz`).
//!
//! A profile is external data that maps the DSL's portable
//! `instrument` + `articulation` vocabulary to concrete `.sfz` sample files
//! on this machine. Scene YAML never names a sample library path directly —
//! that would tie a scene to one person's disk layout and defeat the DSL's
//! whole reason to be diff-friendly, portable text. Swap sound sources by
//! swapping `--profile`, not by editing scenes.

use crate::error::{Error, Location, Result};
use crate::schema::{
    Articulation, Instrument, articulation_key, instrument_key, parse_instrument_key,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Maps every instrument this profile covers to one `.sfz` file per
/// articulation it supports. Paths are relative to `root` (or the profile
/// file's own directory when `root` is absent).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Profile name, echoed in error messages.
    pub name: String,
    /// What sound source this profile wires up, for humans.
    #[serde(default)]
    pub description: Option<String>,
    /// Sample root directory. Relative paths resolve against the profile
    /// file's own directory; absolute paths are used as-is. Default: the
    /// profile file's directory.
    #[serde(default)]
    pub root: Option<String>,
    /// `instrument key -> articulation key -> .sfz path (relative to root)`.
    /// Every instrument must define at least a `sustain` mapping, used as
    /// the fallback when a track asks for an articulation this profile
    /// doesn't have a dedicated sample for.
    pub instruments: BTreeMap<String, BTreeMap<String, String>>,
}

/// One explicit instrument/articulation mapping resolved to a local path.
/// Profile checking uses this to inspect every declared patch, including
/// mappings not exercised by the shipped example scenes.
#[derive(Debug, Clone)]
pub struct ResolvedMapping {
    pub instrument: Instrument,
    pub instrument_key: String,
    pub articulation_key: String,
    pub path: PathBuf,
}

impl Profile {
    /// Semantic validation beyond what serde enforces: every instrument key
    /// must be a real `Instrument` variant, every articulation key a real
    /// `Articulation` variant, and every instrument needs a `sustain` entry
    /// to serve as the fallback.
    pub fn validate(&self) -> Result<()> {
        let fail = |path: String, message: String| Err(Error::Validation { path, message });
        if self.instruments.is_empty() {
            return fail(
                "instruments".to_owned(),
                "profile maps no instruments".to_owned(),
            );
        }
        for (ikey, arts) in &self.instruments {
            if parse_instrument_key(ikey).is_none() {
                return fail(
                    format!("instruments.{ikey}"),
                    format!("`{ikey}` is not a known instrument name"),
                );
            }
            if arts.is_empty() {
                return fail(
                    format!("instruments.{ikey}"),
                    "no articulations mapped".to_owned(),
                );
            }
            if !arts.contains_key("sustain") {
                return fail(
                    format!("instruments.{ikey}"),
                    "missing required `sustain` mapping (used as the fallback for \
                     articulations this instrument has no dedicated sample for)"
                        .to_owned(),
                );
            }
            for akey in arts.keys() {
                if crate::schema::parse_articulation_key(akey).is_none() {
                    return fail(
                        format!("instruments.{ikey}.{akey}"),
                        format!("`{akey}` is not a known articulation name"),
                    );
                }
            }
        }
        Ok(())
    }

    /// Resolve the sample root: `root` (relative to the profile file's
    /// directory) or that directory itself.
    fn resolved_root(&self, profile_dir: &Path) -> PathBuf {
        match &self.root {
            Some(r) => {
                let p = Path::new(r);
                if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    profile_dir.join(p)
                }
            }
            None => profile_dir.to_path_buf(),
        }
    }

    /// Resolve one track's instrument+articulation to an absolute `.sfz`
    /// path. Falls back to the instrument's `sustain` mapping when the
    /// requested articulation has no dedicated sample.
    pub fn resolve(
        &self,
        profile_dir: &Path,
        instrument: Instrument,
        articulation: Articulation,
    ) -> Result<PathBuf> {
        let ikey = instrument_key(instrument);
        let arts = self
            .instruments
            .get(&ikey)
            .ok_or_else(|| Error::Validation {
                path: format!("profile.instruments.{ikey}"),
                message: format!(
                    "renderer profile `{}` has no mapping for instrument `{ikey}`",
                    self.name
                ),
            })?;
        let akey = articulation_key(articulation);
        // `validate()` guarantees `sustain` exists, so this unwrap is safe
        // for any profile that passed `load_profile`.
        let rel = arts
            .get(&akey)
            .or_else(|| arts.get("sustain"))
            .expect("validate() guarantees a sustain fallback");
        Ok(self.resolved_root(profile_dir).join(rel))
    }

    /// Resolve every mapping explicitly declared in this profile. Validation
    /// has already guaranteed the keys are known, so these parses cannot fail.
    pub fn resolved_mappings(&self, profile_dir: &Path) -> Vec<ResolvedMapping> {
        let root = self.resolved_root(profile_dir);
        let mut out = Vec::new();
        for (instrument_key, articulations) in &self.instruments {
            let instrument = parse_instrument_key(instrument_key)
                .expect("validate() guarantees known instrument keys");
            for (articulation_key, rel) in articulations {
                out.push(ResolvedMapping {
                    instrument,
                    instrument_key: instrument_key.clone(),
                    articulation_key: articulation_key.clone(),
                    path: root.join(rel),
                });
            }
        }
        out
    }
}

/// Read, parse and validate a renderer profile file.
pub fn load_profile(path: &Path) -> Result<Profile> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })?;
    let profile: Profile = serde_yaml_ng::from_str(&text).map_err(|e| Error::Parse {
        message: format!("invalid renderer profile: {e}"),
        location: e.location().map(|l| Location {
            line: l.line(),
            column: l.column(),
        }),
    })?;
    profile.validate()?;
    Ok(profile)
}

/// JSON Schema of the renderer profile DSL, for agent consumption.
pub fn schema_json() -> String {
    let schema = schemars::schema_for!(Profile);
    serde_json::to_string_pretty(&schema).expect("schema serializes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Articulation;

    fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, text).unwrap();
        p
    }

    #[test]
    fn resolves_sustain_and_falls_back_for_missing_articulation() {
        let dir = std::env::temp_dir().join(format!("sk-profile-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let text = "name: test\ninstruments:\n  violin:\n    sustain: violin_sus.sfz\n    pizzicato: violin_pizz.sfz\n";
        let path = write(&dir, "profile.yaml", text);
        let profile = load_profile(&path).unwrap();
        let sus = profile
            .resolve(&dir, Instrument::Violin, Articulation::Sustain)
            .unwrap();
        assert_eq!(sus, dir.join("violin_sus.sfz"));
        let pizz = profile
            .resolve(&dir, Instrument::Violin, Articulation::Pizzicato)
            .unwrap();
        assert_eq!(pizz, dir.join("violin_pizz.sfz"));
        // staccato has no dedicated mapping: falls back to sustain.
        let stac = profile
            .resolve(&dir, Instrument::Violin, Articulation::Staccato)
            .unwrap();
        assert_eq!(stac, dir.join("violin_sus.sfz"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unmapped_instrument_is_a_clear_validation_error() {
        let dir = std::env::temp_dir().join(format!("sk-profile-um-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let text = "name: test\ninstruments:\n  violin:\n    sustain: v.sfz\n";
        let path = write(&dir, "profile.yaml", text);
        let profile = load_profile(&path).unwrap();
        let err = profile
            .resolve(&dir, Instrument::Piano, Articulation::Sustain)
            .unwrap_err();
        assert!(err.to_string().contains("piano"), "{err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_sustain_mapping_fails_to_load() {
        let dir = std::env::temp_dir().join(format!("sk-profile-nosus-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let text = "name: test\ninstruments:\n  violin:\n    pizzicato: v.sfz\n";
        let path = write(&dir, "profile.yaml", text);
        let err = load_profile(&path).unwrap_err();
        assert!(err.to_string().contains("sustain"), "{err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unknown_instrument_key_fails_to_load() {
        let dir = std::env::temp_dir().join(format!("sk-profile-badkey-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let text = "name: test\ninstruments:\n  not_a_real_instrument:\n    sustain: v.sfz\n";
        let path = write(&dir, "profile.yaml", text);
        let err = load_profile(&path).unwrap_err();
        assert!(err.to_string().contains("not_a_real_instrument"), "{err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn absolute_root_overrides_profile_directory() {
        let dir = std::env::temp_dir().join(format!("sk-profile-root-{}", std::process::id()));
        let samples =
            std::env::temp_dir().join(format!("sk-profile-samples-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(&samples).unwrap();
        let text = format!(
            "name: test\nroot: \"{}\"\ninstruments:\n  violin:\n    sustain: v.sfz\n",
            samples.display()
        );
        let path = write(&dir, "profile.yaml", &text);
        let profile = load_profile(&path).unwrap();
        let resolved = profile
            .resolve(&dir, Instrument::Violin, Articulation::Sustain)
            .unwrap();
        assert_eq!(resolved, samples.join("v.sfz"));
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&samples).unwrap();
    }

    /// Every instrument named by `instrument:` in the shipped example
    /// scenes (`examples/scenes/*.yaml`), scanned as plain text so this
    /// never needs the full scene parser.
    fn instruments_used_by_shipped_scenes(
        manifest_dir: &Path,
    ) -> std::collections::BTreeSet<String> {
        let scenes_dir = manifest_dir.join("examples/scenes");
        let mut used = std::collections::BTreeSet::new();
        for entry in std::fs::read_dir(&scenes_dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "yaml") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for line in text.lines() {
                let line = line.trim().trim_start_matches('-').trim();
                if let Some(rest) = line.strip_prefix("instrument:") {
                    let value = rest.split('#').next().unwrap_or("").trim();
                    used.insert(value.to_owned());
                }
            }
        }
        assert!(
            used.len() >= 15,
            "expected the shipped scenes to exercise many instruments, found {}: {used:?}",
            used.len()
        );
        used
    }

    /// Guards a shipped renderer profile against schema drift and against
    /// silently falling out of sync with the instruments the shipped
    /// example scenes actually use — the whole point of a renderer profile
    /// is that it stays valid without the real (multi-GB, not checked in)
    /// sample library present, so this never touches disk beyond the
    /// profile YAML itself.
    fn assert_shipped_profile_covers_shipped_scenes(profile_file: &str) {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let profile_path = manifest_dir.join("examples/profiles").join(profile_file);
        let profile = load_profile(&profile_path).unwrap();
        let used = instruments_used_by_shipped_scenes(manifest_dir);

        for key in &used {
            let instrument = parse_instrument_key(key).unwrap_or_else(|| {
                panic!("`{key}` (used in a shipped scene) is not a known instrument")
            });
            profile
                .resolve(
                    profile_path.parent().unwrap(),
                    instrument,
                    Articulation::Sustain,
                )
                .unwrap_or_else(|e| {
                    panic!(
                        "{profile_file} has no mapping for `{key}`, used by a shipped scene: {e}"
                    )
                });
        }
    }

    #[test]
    fn shipped_vsco2_profile_validates_and_covers_shipped_scenes() {
        assert_shipped_profile_covers_shipped_scenes("vsco2-ce.yaml");
    }

    #[test]
    fn shipped_vsco2_vcsl_profile_validates_and_covers_shipped_scenes() {
        assert_shipped_profile_covers_shipped_scenes("vsco2-vcsl.yaml");
    }
}
