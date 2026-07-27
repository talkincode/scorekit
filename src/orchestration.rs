//! Orchestration profiles route logical scene palettes to leaf renderer profiles.

use crate::error::{Error, Location, Result};
use crate::profile::{self, Profile};
use crate::schema::{Scene, Track, articulation_key};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OrchestrationProfile {
    /// Protocol version. The only supported value is 1.
    #[schemars(range(min = 1, max = 1))]
    pub schema_version: u16,
    /// Stable human-readable orchestration name.
    pub name: String,
    /// Palette used by tracks that omit `palette`.
    pub default_palette: String,
    /// Logical palette name -> independently curated renderer profile.
    pub palettes: BTreeMap<String, PaletteBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PaletteBinding {
    /// Renderer profile path, relative to this orchestration file.
    pub profile: String,
}

#[derive(Debug, Clone)]
pub struct LoadedPalette {
    pub declared_profile_path: String,
    pub profile_path: PathBuf,
    pub profile: Profile,
    pub available: crate::resolver::Available,
}

impl LoadedPalette {
    pub fn profile_dir(&self) -> &Path {
        self.profile_path.parent().unwrap_or_else(|| Path::new("."))
    }
}

#[derive(Debug, Clone)]
pub struct LoadedOrchestration {
    pub name: String,
    pub default_palette: String,
    pub palettes: BTreeMap<String, LoadedPalette>,
}

impl LoadedOrchestration {
    fn palette_for_track(&self, index: usize, track: &Track) -> Result<(&str, &LoadedPalette)> {
        let name = track.palette.as_deref().unwrap_or(&self.default_palette);
        self.palettes
            .get_key_value(name)
            .map(|(name, palette)| (name.as_str(), palette))
            .ok_or_else(|| Error::Validation {
                path: format!("tracks[{index}].palette"),
                message: format!(
                    "orchestration `{}` has no palette `{name}` (defined: {:?})",
                    self.name,
                    self.palettes.keys().collect::<Vec<_>>()
                ),
            })
    }

    /// Resolve instruments and concrete patches with one isolated availability
    /// set per track, so fallback never leaks across palettes.
    pub fn resolve_scene(
        &self,
        scene: &Scene,
        raws: Option<&[Option<String>]>,
        policy: &crate::resolver::FallbackPolicy,
        verbose: bool,
    ) -> Result<crate::resolver::SceneResolution> {
        let palettes: Vec<(&str, &LoadedPalette)> = scene
            .tracks
            .iter()
            .enumerate()
            .map(|(index, track)| self.palette_for_track(index, track))
            .collect::<Result<_>>()?;
        let available: Vec<&crate::resolver::Available> = palettes
            .iter()
            .map(|(_, palette)| &palette.available)
            .collect();
        let mut resolution =
            crate::resolver::resolve_scene_per_track(scene, raws, &available, policy, verbose);
        for ((track, resolved), (palette_name, palette)) in scene
            .tracks
            .iter()
            .zip(&mut resolution.tracks)
            .zip(palettes)
        {
            resolved.set_renderer_context(
                palette_name,
                &palette.profile.name,
                &palette.declared_profile_path,
                &articulation_key(track.articulation),
            );
            let Some(target) = resolved.target() else {
                continue;
            };
            let patch =
                palette
                    .profile
                    .resolve_patch(palette.profile_dir(), target, track.articulation)?;
            let mut clip_names = BTreeSet::new();
            if scene.sections.is_empty() {
                if let Some(clip) = &track.clip {
                    clip_names.insert(clip.as_str());
                }
            } else {
                for section in &scene.sections {
                    if section.mute.contains(&track.id) {
                        continue;
                    }
                    if let Some(clip) = section.clips.get(&track.id).or(track.clip.as_ref()) {
                        clip_names.insert(clip.as_str());
                    }
                }
            }
            for clip_name in clip_names {
                let clip = scene
                    .clips
                    .get(clip_name)
                    .expect("scene validation guarantees referenced clips exist");
                for (lane_name, lane) in &clip.automation {
                    if !patch.controls.contains(&lane.target) {
                        return Err(Error::Validation {
                            path: format!("clips.{clip_name}.automation.{lane_name}.target"),
                            message: format!(
                                "renderer profile `{}` mapping for {}.{} does not declare `{}` support",
                                palette.profile.name,
                                crate::schema::instrument_key(target),
                                patch.articulation_key,
                                lane.target.key()
                            ),
                        });
                    }
                }
            }
            resolved.set_renderer_patch(&patch.articulation_key, &patch.path);
        }
        Ok(resolution)
    }

    pub fn summary(&self) -> String {
        format!(
            "ok: orchestration `{}` has {} palette(s), default `{}`",
            self.name,
            self.palettes.len(),
            self.default_palette
        )
    }

    pub fn to_json(&self) -> serde_json::Value {
        let palettes: Vec<_> = self
            .palettes
            .iter()
            .map(|(name, loaded)| {
                let mappings = loaded.profile.resolved_mappings(loaded.profile_dir());
                let patches: BTreeSet<_> = mappings.iter().map(|m| &m.path).collect();
                json!({
                    "name": name,
                    "profile": loaded.profile.name,
                    "profile_path": loaded.declared_profile_path,
                    "mappings": mappings.len(),
                    "patches": patches.len(),
                })
            })
            .collect();
        json!({
            "schema_version": SCHEMA_VERSION,
            "name": self.name,
            "default_palette": self.default_palette,
            "palettes": palettes,
        })
    }
}

impl OrchestrationProfile {
    fn validate(&self) -> Result<()> {
        let fail = |path: &str, message: String| {
            Err(Error::Validation {
                path: path.to_owned(),
                message,
            })
        };
        if self.schema_version != SCHEMA_VERSION {
            return fail(
                "schema_version",
                format!(
                    "{} is unsupported; expected {SCHEMA_VERSION}",
                    self.schema_version
                ),
            );
        }
        if !crate::texture::valid_logical_name(&self.name) {
            return fail(
                "name",
                format!("`{}` must match [a-z][a-z0-9_-]{{0,63}}", self.name),
            );
        }
        if !crate::texture::valid_logical_name(&self.default_palette) {
            return fail(
                "default_palette",
                format!(
                    "`{}` must match [a-z][a-z0-9_-]{{0,63}}",
                    self.default_palette
                ),
            );
        }
        if self.palettes.is_empty() {
            return fail("palettes", "at least one palette is required".to_owned());
        }
        if !self.palettes.contains_key(&self.default_palette) {
            return fail(
                "default_palette",
                format!(
                    "`{}` is not defined in palettes (defined: {:?})",
                    self.default_palette,
                    self.palettes.keys().collect::<Vec<_>>()
                ),
            );
        }
        for (name, binding) in &self.palettes {
            if !crate::texture::valid_logical_name(name) {
                return fail(
                    &format!("palettes.{name}"),
                    format!("`{name}` must match [a-z][a-z0-9_-]{{0,63}}"),
                );
            }
            if binding.profile.trim().is_empty() {
                return fail(
                    &format!("palettes.{name}.profile"),
                    "renderer profile path must not be empty".to_owned(),
                );
            }
        }
        Ok(())
    }
}

/// Load an orchestration and every leaf renderer profile it references.
pub fn load(path: &Path) -> Result<LoadedOrchestration> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })?;
    let orchestration: OrchestrationProfile =
        serde_yaml_ng::from_str(&text).map_err(|e| Error::Parse {
            message: format!("invalid orchestration profile: {e}"),
            location: e.location().map(|l| Location {
                line: l.line(),
                column: l.column(),
            }),
        })?;
    orchestration.validate()?;

    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut palettes = BTreeMap::new();
    for (name, binding) in orchestration.palettes {
        let declared = Path::new(&binding.profile);
        let profile_path = if declared.is_absolute() {
            declared.to_path_buf()
        } else {
            base.join(declared)
        };
        let loaded = profile::load_profile(&profile_path).map_err(|source| Error::Validation {
            path: format!("palettes.{name}.profile"),
            message: format!(
                "cannot load renderer profile `{}`: {source}",
                profile_path.display()
            ),
        })?;
        let profile_dir = profile_path.parent().unwrap_or_else(|| Path::new("."));
        for mapping in loaded.resolved_mappings(profile_dir) {
            if !mapping.path.is_file() {
                return Err(Error::Validation {
                    path: format!("palettes.{name}.profile"),
                    message: format!(
                        "renderer profile `{}` maps {}.{} to missing SFZ `{}`",
                        loaded.name,
                        mapping.instrument_key,
                        mapping.articulation_key,
                        mapping.path.display()
                    ),
                });
            }
        }
        let available = crate::resolver::available_from_profile(&loaded);
        palettes.insert(
            name,
            LoadedPalette {
                declared_profile_path: binding.profile,
                profile_path,
                profile: loaded,
                available,
            },
        );
    }

    Ok(LoadedOrchestration {
        name: orchestration.name,
        default_palette: orchestration.default_palette,
        palettes,
    })
}

pub fn schema_json() -> String {
    let schema = schemars::schema_for!(OrchestrationProfile);
    let mut value = serde_json::to_value(schema).expect("schema serializes");
    value["properties"]["schema_version"]["const"] = json!(SCHEMA_VERSION);
    serde_json::to_string_pretty(&value).expect("schema serializes")
}
