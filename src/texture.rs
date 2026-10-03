//! Portable texture-source profiles for field recordings, ambience and SFX.
//!
//! Scene files name a logical source (`river`, `birds`, `engine_idle`); this
//! profile is the machine-local binding to an audio file. Keeping the path
//! here preserves the same portability boundary renderer profiles provide
//! for SFZ instruments.
//!
//! A profile is also the **discovery contract**: an authoring agent must be
//! able to enumerate the available sources, tell materially different
//! candidates apart, and conclude that nothing fits — before it writes
//! `textures[].source` into a scene. That only works if every source carries
//! stable metadata. V1 requires a complete descriptive record; v2 requires
//! family, tags, playback, and scenes, with additional audio descriptors
//! available where curators know them.
//!
//! The split of responsibility is deliberate:
//!
//! - **This file declares curated intent** — path, family, tags, playback
//!   constraints, scenes, and optional descriptive audio properties.
//! - **`texture_check` measures file facts** — existence, decodability,
//!   actual duration, loudness, and checksum. Curated audio descriptors help
//!   discovery but are not substitutes for measurements of the recording.

use crate::error::{Error, Location, Result};
use crate::schema::TextureMode;
use schemars::JsonSchema;
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// Protocol version emitted for profiles that omit an explicit version.
pub const SCHEMA_VERSION: u16 = 1;
const LATEST_SCHEMA_VERSION: u16 = 2;

fn default_schema_version() -> u16 {
    SCHEMA_VERSION
}

/// Upper bound on `tags` / `use_cases` entries. A source described by two
/// dozen tags matches every query and therefore distinguishes nothing.
const MAX_TOKENS: usize = 16;

/// Coarse sound family. Deliberately a closed vocabulary compiled into the
/// binary: the flat mapping this format replaces failed precisely because it
/// had no stable axis to filter on, and a free-form category would drift into
/// `ambience` / `ambient` / `ambiences` and reproduce that failure.
/// Expressive room lives in `tags`, which stay open.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Continuous environmental beds (room tone, wind, rain, crowd).
    Ambience,
    /// Human-scale performed sounds (footsteps, cloth, handling).
    Foley,
    /// Short percussive events (hits, breaks, slams).
    Impact,
    /// Directional sweeps, risers and falls that join two states.
    Transition,
    /// Pitched or resonant material (bowed metal, glass, chimes).
    Tonal,
    /// Machinery, motors, mechanisms.
    Industrial,
    /// Water, fire, earth, vegetation, creatures.
    Organic,
    /// Synthesized or heavily processed abstract material.
    SoundDesign,
}

impl Category {
    pub const ALL: [Category; 8] = [
        Category::Ambience,
        Category::Foley,
        Category::Impact,
        Category::Transition,
        Category::Tonal,
        Category::Industrial,
        Category::Organic,
        Category::SoundDesign,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Category::Ambience => "ambience",
            Category::Foley => "foley",
            Category::Impact => "impact",
            Category::Transition => "transition",
            Category::Tonal => "tonal",
            Category::Industrial => "industrial",
            Category::Organic => "organic",
            Category::SoundDesign => "sound_design",
        }
    }

    pub fn parse(key: &str) -> Option<Category> {
        Category::ALL.into_iter().find(|c| c.key() == key)
    }

    pub fn keys() -> Vec<&'static str> {
        Category::ALL.into_iter().map(Category::key).collect()
    }
}

pub fn mode_key(mode: TextureMode) -> &'static str {
    match mode {
        TextureMode::Loop => "loop",
        TextureMode::OneShot => "one_shot",
    }
}

pub fn parse_mode(key: &str) -> Option<TextureMode> {
    match key {
        "loop" => Some(TextureMode::Loop),
        "one_shot" => Some(TextureMode::OneShot),
        _ => None,
    }
}

/// How a source is allowed to be scheduled by a scene. This is declared
/// intent, not a measured property: a 6-second grinding bed recorded to loop
/// seamlessly and a 6-second one-shot crash are physically identical and
/// only the curator knows which is which.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Playback {
    /// Scheduling modes this recording supports. A scene using any other
    /// mode for this source fails validation before anything is rendered.
    pub modes: Vec<TextureMode>,
    /// The mode to reach for when a scene has no specific reason otherwise.
    /// Must appear in `modes`.
    pub default_mode: TextureMode,
    /// Whether the recording is known to loop seamlessly.
    #[serde(default)]
    pub loopable: Option<bool>,
}

/// Where the recording came from. Points at a corpus library identity
/// (`<library id>@<version>`) rather than restating its license inline —
/// the library manifest owns the license, and a second copy here would be a
/// fact that can silently disagree with the first.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// Manifested library identity, e.g. `vsco2-ce@1.1.0`.
    pub library: String,
}

/// One discoverable texture source.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextureSource {
    /// Audio file path (WAV, FLAC, OGG, …), relative to the profile's `root`
    /// or absolute.
    pub path: String,
    /// One-line human description of what is actually audible.
    #[serde(default)]
    pub description: Option<String>,
    /// Coarse sound family; the stable axis agents filter on first.
    #[serde(default)]
    pub category: Option<Category>,
    /// V2 sound family; v1 uses the closed `category` vocabulary.
    #[serde(default)]
    pub family: Option<String>,
    /// Free-form descriptors (`[a-z][a-z0-9_-]{0,31}`), 1..=16, no duplicates.
    pub tags: Vec<String>,
    /// Declared scheduling constraints.
    pub playback: Playback,
    /// Scene intents this source is meant to serve (`forest`, `dungeon`,
    /// `tension`), same syntax and limits as `tags`.
    #[serde(default)]
    pub use_cases: Vec<String>,
    /// V2 scene intents this source is suitable for.
    #[serde(default)]
    pub scenes: Vec<String>,
    /// Originating library identity.
    #[serde(default)]
    pub provenance: Option<Provenance>,
    /// Curated audio descriptors. `texture check` independently verifies
    /// that the referenced recording exists and decodes.
    #[serde(default)]
    pub audio: Option<AudioMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AudioMetadata {
    /// Declared duration in seconds; finite and greater than zero.
    #[serde(default)]
    pub duration_seconds: Option<f64>,
    /// Relative intensity in the inclusive range 0..=1.
    #[serde(default)]
    pub intensity: Option<f64>,
    /// Relative brightness in the inclusive range 0..=1.
    #[serde(default)]
    pub brightness: Option<f64>,
    /// Curated tonal character such as `atonal`, `dark`, or `bright`.
    #[serde(default)]
    pub tonality: Option<String>,
}

/// A source binding accepts original path-only profiles for build and
/// metadata-free discovery, alongside structured source declarations.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum TextureSourceBinding {
    LegacyPath(String),
    Discoverable(TextureSource),
}

impl<'de> Deserialize<'de> for TextureSourceBinding {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BindingVisitor;

        impl<'de> Visitor<'de> for BindingVisitor {
            type Value = TextureSourceBinding;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an audio path string or a structured texture source")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(TextureSourceBinding::LegacyPath(value.to_owned()))
            }

            fn visit_string<E>(self, value: String) -> std::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(TextureSourceBinding::LegacyPath(value))
            }

            fn visit_map<M>(self, map: M) -> std::result::Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                TextureSource::deserialize(de::value::MapAccessDeserializer::new(map))
                    .map(TextureSourceBinding::Discoverable)
            }
        }

        deserializer.deserialize_any(BindingVisitor)
    }
}

impl From<TextureSource> for TextureSourceBinding {
    fn from(source: TextureSource) -> Self {
        Self::Discoverable(source)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextureProfile {
    /// Protocol version. Omitted versions retain the v1 compatibility format.
    #[serde(default = "default_schema_version")]
    #[schemars(range(min = 1, max = 2))]
    pub schema_version: u16,
    /// Human-readable profile name.
    pub name: String,
    /// What recordings or library this profile binds, for humans.
    #[serde(default)]
    pub description: Option<String>,
    /// Source root. Relative paths resolve from the profile file directory;
    /// absent means the profile file directory itself.
    #[serde(default)]
    pub root: Option<String>,
    /// Portable source name -> path-only legacy binding or discoverable source
    /// declaration. New profiles should always use the structured form.
    pub sources: BTreeMap<String, TextureSourceBinding>,
}

pub fn valid_logical_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.bytes().enumerate().all(|(i, b)| {
            b.is_ascii_lowercase() || (i > 0 && (b.is_ascii_digit() || b == b'_' || b == b'-'))
        })
}

fn valid_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 32
        && token.bytes().enumerate().all(|(i, b)| {
            b.is_ascii_lowercase() || (i > 0 && (b.is_ascii_digit() || b == b'_' || b == b'-'))
        })
}

pub fn valid_filter_token(token: &str) -> bool {
    valid_token(token)
}

fn fail<T>(path: String, message: String) -> Result<T> {
    Err(Error::Validation { path, message })
}

fn validate_tokens(field: &str, values: &[String]) -> Result<()> {
    if values.is_empty() {
        return fail(
            field.to_owned(),
            "must list at least one entry (an undescribed source cannot be discovered)".to_owned(),
        );
    }
    if values.len() > MAX_TOKENS {
        return fail(
            field.to_owned(),
            format!(
                "{} entries exceeds the maximum of {MAX_TOKENS}; a source that matches every \
                 query distinguishes nothing",
                values.len()
            ),
        );
    }
    for (i, value) in values.iter().enumerate() {
        if !valid_token(value) {
            return fail(
                format!("{field}[{i}]"),
                format!("`{value}` must match [a-z][a-z0-9_-]{{0,31}}"),
            );
        }
        if values[..i].contains(value) {
            return fail(format!("{field}[{i}]"), format!("`{value}` is a duplicate"));
        }
    }
    Ok(())
}

fn valid_library_identity(identity: &str) -> bool {
    let Some((library, version)) = identity.split_once('@') else {
        return false;
    };
    !library.is_empty()
        && !version.is_empty()
        && !version.contains('@')
        && library
            .bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_alphanumeric() || (i > 0 && b"._-/".contains(&b)))
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
}

fn edit_distance(left: &str, right: &str) -> usize {
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    for (i, left_byte) in left.bytes().enumerate() {
        let mut current = Vec::with_capacity(right.len() + 1);
        current.push(i + 1);
        for (j, right_byte) in right.bytes().enumerate() {
            current.push(
                (previous[j + 1] + 1)
                    .min(current[j] + 1)
                    .min(previous[j] + usize::from(left_byte != right_byte)),
            );
        }
        previous = current;
    }
    previous[right.len()]
}

impl TextureSource {
    fn validate(&self, field: &str, schema_version: u16) -> Result<()> {
        if self.path.trim().is_empty() {
            return fail(
                format!("{field}.path"),
                "audio path must not be empty".to_owned(),
            );
        }
        if schema_version == 1
            && self
                .description
                .as_deref()
                .is_none_or(|description| description.trim().is_empty())
        {
            return fail(
                format!("{field}.description"),
                "description must not be empty (it is what an agent reads when choosing)"
                    .to_owned(),
            );
        }
        if self
            .description
            .as_deref()
            .is_some_and(|description| description.trim().is_empty())
        {
            return fail(
                format!("{field}.description"),
                "description must not be empty".to_owned(),
            );
        }
        validate_tokens(&format!("{field}.tags"), &self.tags)?;
        if schema_version == 1 {
            if self.category.is_none() {
                return fail(
                    format!("{field}.category"),
                    "field is required in schema_version 1".to_owned(),
                );
            }
            validate_tokens(&format!("{field}.use_cases"), &self.use_cases)?;
            if self.provenance.is_none() {
                return fail(
                    format!("{field}.provenance.library"),
                    "versioned library provenance is required in schema_version 1".to_owned(),
                );
            }
        } else {
            if self.family.is_none() {
                return fail(
                    format!("{field}.family"),
                    "field is required in schema_version 2".to_owned(),
                );
            }
            if self.category.is_some() {
                return fail(
                    format!("{field}.category"),
                    "use `family` in schema_version 2".to_owned(),
                );
            }
            if !self.use_cases.is_empty() {
                return fail(
                    format!("{field}.use_cases"),
                    "use `scenes` in schema_version 2".to_owned(),
                );
            }
            if let Some(family) = &self.family
                && !valid_token(family)
            {
                return fail(
                    format!("{field}.family"),
                    format!("`{family}` must match [a-z][a-z0-9_-]{{0,31}}"),
                );
            }
            let scenes = if self.scenes.is_empty() {
                &self.use_cases
            } else {
                &self.scenes
            };
            validate_tokens(&format!("{field}.scenes"), scenes)?;
        }
        if let Some(provenance) = &self.provenance
            && !valid_library_identity(&provenance.library)
        {
            return fail(
                format!("{field}.provenance.library"),
                format!(
                    "`{}` must be a versioned identity matching <library>@<version>",
                    provenance.library
                ),
            );
        }
        if let Some(audio) = &self.audio {
            if audio
                .duration_seconds
                .is_some_and(|duration| !duration.is_finite() || duration <= 0.0)
            {
                return fail(
                    format!("{field}.audio.duration_seconds"),
                    "must be a finite number greater than 0".to_owned(),
                );
            }
            for (key, value) in [
                ("intensity", audio.intensity),
                ("brightness", audio.brightness),
            ] {
                if value.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
                    return fail(
                        format!("{field}.audio.{key}"),
                        "must be a finite number in the inclusive range 0..=1".to_owned(),
                    );
                }
            }
            if let Some(tonality) = &audio.tonality
                && !valid_token(tonality)
            {
                return fail(
                    format!("{field}.audio.tonality"),
                    format!("`{tonality}` must match [a-z][a-z0-9_-]{{0,31}}"),
                );
            }
        }
        let modes = &self.playback.modes;
        if modes.is_empty() {
            return fail(
                format!("{field}.playback.modes"),
                "must list at least one scheduling mode".to_owned(),
            );
        }
        for (i, mode) in modes.iter().enumerate() {
            if modes[..i].contains(mode) {
                return fail(
                    format!("{field}.playback.modes[{i}]"),
                    format!("`{}` is a duplicate", mode_key(*mode)),
                );
            }
        }
        if !modes.contains(&self.playback.default_mode) {
            return fail(
                format!("{field}.playback.default_mode"),
                format!(
                    "`{}` is not listed in modes ({})",
                    mode_key(self.playback.default_mode),
                    modes
                        .iter()
                        .map(|m| mode_key(*m))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
        }
        Ok(())
    }

    pub fn to_json(&self, name: &str, resolved: &Path, schema_version: u16) -> serde_json::Value {
        if schema_version == 1 {
            return json!({
                "source": name,
                "path": self.path,
                "resolved_path": resolved.display().to_string(),
                "exists": resolved.is_file(),
                "description": self.description,
                "category": self.category.map(Category::key),
                "tags": self.tags,
                "playback": {
                    "modes": self.playback.modes.iter().map(|m| mode_key(*m)).collect::<Vec<_>>(),
                    "default_mode": mode_key(self.playback.default_mode),
                },
                "use_cases": self.use_cases,
                "provenance": self.provenance.as_ref().map(|p| json!({ "library": p.library })),
            });
        }
        let family = self
            .family
            .as_deref()
            .or_else(|| self.category.map(Category::key));
        let scenes = if self.scenes.is_empty() {
            &self.use_cases
        } else {
            &self.scenes
        };
        json!({
            "source": name,
            "path": self.path,
            "resolved_path": resolved.display().to_string(),
            "exists": resolved.is_file(),
            "description": self.description,
            "family": family,
            "tags": self.tags,
            "playback": {
                "modes": self.playback.modes.iter().map(|m| mode_key(*m)).collect::<Vec<_>>(),
                "default_mode": mode_key(self.playback.default_mode),
                "loopable": self.playback.loopable,
            },
            "scenes": scenes,
            "audio": self.audio,
            "provenance": self.provenance.as_ref().map(|p| json!({ "library": p.library })),
        })
    }
}

impl TextureSourceBinding {
    pub fn path(&self) -> &str {
        match self {
            Self::LegacyPath(path) => path,
            Self::Discoverable(source) => &source.path,
        }
    }

    fn validate(&self, field: &str, schema_version: u16) -> Result<()> {
        match self {
            Self::LegacyPath(path) if path.trim().is_empty() => {
                fail(field.to_owned(), "audio path must not be empty".to_owned())
            }
            Self::LegacyPath(_) => Ok(()),
            Self::Discoverable(source) => source.validate(field, schema_version),
        }
    }

    /// Legacy profiles predate scheduling declarations and therefore retain
    /// their original behavior of allowing either scene mode.
    pub fn declared_modes(&self) -> Option<&[TextureMode]> {
        match self {
            Self::LegacyPath(_) => None,
            Self::Discoverable(source) => Some(&source.playback.modes),
        }
    }

    #[cfg(test)]
    fn discoverable_mut(&mut self) -> Option<&mut TextureSource> {
        match self {
            Self::LegacyPath(_) => None,
            Self::Discoverable(source) => Some(source),
        }
    }
}

impl TextureProfile {
    pub fn validate(&self) -> Result<()> {
        if !(SCHEMA_VERSION..=LATEST_SCHEMA_VERSION).contains(&self.schema_version) {
            return fail(
                "schema_version".to_owned(),
                format!(
                    "{} is unsupported; expected {SCHEMA_VERSION} or {LATEST_SCHEMA_VERSION}",
                    self.schema_version,
                ),
            );
        }
        if self.name.trim().is_empty() {
            return fail(
                "name".to_owned(),
                "profile name must not be empty".to_owned(),
            );
        }
        if self.sources.is_empty() {
            return fail(
                "sources".to_owned(),
                "texture profile maps no sources".to_owned(),
            );
        }
        for (name, source) in &self.sources {
            if !valid_logical_name(name) {
                return fail(
                    format!("sources.{name}"),
                    format!("`{name}` must match [a-z][a-z0-9_-]{{0,63}} (portable source name)"),
                );
            }
            source.validate(&format!("sources.{name}"), self.schema_version)?;
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

    /// Look up one declared source, or fail naming the exact profile field.
    pub fn source(&self, name: &str) -> Result<&TextureSourceBinding> {
        self.sources.get(name).ok_or_else(|| {
            let mut suggestions: Vec<(&String, usize)> = self
                .sources
                .keys()
                .map(|candidate| (candidate, edit_distance(name, candidate)))
                .filter(|(candidate, distance)| {
                    *distance <= 2
                        || candidate.starts_with(name)
                        || name.starts_with(candidate.as_str())
                })
                .collect();
            suggestions.sort_by(|(left, left_distance), (right, right_distance)| {
                left_distance
                    .cmp(right_distance)
                    .then_with(|| left.cmp(right))
            });
            let hint = if suggestions.is_empty() {
                String::new()
            } else {
                format!(
                    "; similar source key(s): {}",
                    suggestions
                        .iter()
                        .take(3)
                        .map(|(candidate, _)| candidate.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            Error::Validation {
                path: format!("texture_profile.sources.{name}"),
                message: format!(
                    "texture profile `{}` has no mapping for source `{name}`{hint}; use `scorekit texture inspect <profile>` to enumerate available keys",
                    self.name
                ),
            }
        })
    }

    pub fn resolve(&self, profile_dir: &Path, name: &str) -> Result<PathBuf> {
        let source = self.source(name)?;
        Ok(self.resolved_root(profile_dir).join(source.path()))
    }

    /// Every declared source with its resolved local path, in stable key
    /// order (`BTreeMap`), so reports are byte-identical across runs.
    pub fn resolved_sources<'a>(
        &'a self,
        profile_dir: &Path,
    ) -> Vec<(&'a String, &'a TextureSourceBinding, PathBuf)> {
        let root = self.resolved_root(profile_dir);
        self.sources
            .iter()
            .map(|(name, source)| (name, source, root.join(source.path())))
            .collect()
    }
}

/// Exact, explainable selection criteria. Every populated field must match
/// (AND), and every comparison is exact equality, set membership, or an
/// inclusive numeric bound — never a similarity score. Ranking candidates by
/// "closeness" would put creative judgement inside the compiler and would
/// hand back a plausible-looking wrong answer exactly when the honest answer
/// is "nothing fits".
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub source: Option<String>,
    pub category: Option<Category>,
    pub family: Option<String>,
    pub tags: Vec<String>,
    pub mode: Option<TextureMode>,
    pub use_case: Option<String>,
    pub scene: Option<String>,
    pub min_duration: Option<f64>,
    pub max_duration: Option<f64>,
    pub min_intensity: Option<f64>,
    pub max_intensity: Option<f64>,
    pub min_brightness: Option<f64>,
    pub max_brightness: Option<f64>,
    pub tonality: Option<String>,
    pub loopable: Option<bool>,
    pub limit: Option<usize>,
    pub offset: usize,
}

impl Filter {
    fn has_metadata_constraints(&self) -> bool {
        self.category.is_some()
            || self.family.is_some()
            || !self.tags.is_empty()
            || self.mode.is_some()
            || self.use_case.is_some()
            || self.scene.is_some()
            || self.min_duration.is_some()
            || self.max_duration.is_some()
            || self.min_intensity.is_some()
            || self.max_intensity.is_some()
            || self.min_brightness.is_some()
            || self.max_brightness.is_some()
            || self.tonality.is_some()
            || self.loopable.is_some()
    }

    fn matches(&self, name: &str, source: &TextureSource) -> bool {
        if let Some(wanted) = &self.source
            && name != wanted
        {
            return false;
        }
        if let Some(wanted) = self.category {
            if source.category != Some(wanted) && source.family.as_deref() != Some(wanted.key()) {
                return false;
            }
        }
        if let Some(wanted) = &self.family
            && source
                .family
                .as_deref()
                .or_else(|| source.category.map(Category::key))
                != Some(wanted.as_str())
        {
            return false;
        }
        if !self.tags.iter().all(|tag| source.tags.contains(tag)) {
            return false;
        }
        if let Some(mode) = self.mode
            && !source.playback.modes.contains(&mode)
        {
            return false;
        }
        if let Some(use_case) = &self.use_case
            && !source.use_cases.contains(use_case)
            && !source.scenes.contains(use_case)
        {
            return false;
        }
        if let Some(scene) = &self.scene
            && !source.scenes.contains(scene)
            && !source.use_cases.contains(scene)
        {
            return false;
        }
        if let Some(audio) = &source.audio {
            for (bound, actual, is_min) in [
                (self.min_duration, audio.duration_seconds, true),
                (self.max_duration, audio.duration_seconds, false),
                (self.min_intensity, audio.intensity, true),
                (self.max_intensity, audio.intensity, false),
                (self.min_brightness, audio.brightness, true),
                (self.max_brightness, audio.brightness, false),
            ] {
                if let Some(bound) = bound
                    && actual.is_none_or(|actual| {
                        if is_min {
                            actual < bound
                        } else {
                            actual > bound
                        }
                    })
                {
                    return false;
                }
            }
            if let Some(tonality) = &self.tonality
                && audio.tonality.as_ref() != Some(tonality)
            {
                return false;
            }
        } else if self.min_duration.is_some()
            || self.max_duration.is_some()
            || self.min_intensity.is_some()
            || self.max_intensity.is_some()
            || self.min_brightness.is_some()
            || self.max_brightness.is_some()
            || self.tonality.is_some()
        {
            return false;
        }
        if self
            .loopable
            .is_some_and(|wanted| source.playback.loopable != Some(wanted))
        {
            return false;
        }
        true
    }

    fn to_json(&self) -> serde_json::Value {
        json!({
            "source": self.source,
            "category": self.category.map(Category::key),
            "family": self.family,
            "tags": self.tags,
            "mode": self.mode.map(mode_key),
            "use_case": self.use_case,
            "scene": self.scene,
            "min_duration": self.min_duration,
            "max_duration": self.max_duration,
            "min_intensity": self.min_intensity,
            "max_intensity": self.max_intensity,
            "min_brightness": self.min_brightness,
            "max_brightness": self.max_brightness,
            "tonality": self.tonality,
            "loopable": self.loopable,
            "limit": self.limit,
            "offset": self.offset,
        })
    }
}

#[derive(Debug)]
pub struct InspectReport {
    pub profile: String,
    pub total: usize,
    pub matching: usize,
    pub status: &'static str,
    filter: Filter,
    matched: Vec<serde_json::Value>,
}

impl InspectReport {
    pub fn matched(&self) -> usize {
        self.matching
    }

    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "profile": self.profile,
            "total": self.total,
            "matched": self.matched(),
            "returned": self.matched.len(),
            "offset": self.filter.offset,
            "limit": self.filter.limit,
            "status": self.status,
            "filters": self.filter.to_json(),
            "categories": Category::keys(),
            "sources": self.matched,
        })
    }

    pub fn summary(&self) -> String {
        if self.matching == 0 {
            return format!(
                "no_match: profile `{}`: 0 of {} source(s) match; no suitable source exists — \
                 acquire and declare one rather than substituting an approximation",
                self.profile, self.total
            );
        }
        let mut lines = vec![format!(
            "ok: profile `{}`: {} of {} source(s) match (showing {} from offset {})",
            self.profile,
            self.matched(),
            self.total,
            self.matched.len(),
            self.filter.offset
        )];
        for source in &self.matched {
            let text = |value: &serde_json::Value| value.as_str().unwrap_or_default().to_owned();
            let list = |value: &serde_json::Value| {
                value
                    .as_array()
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .unwrap_or_default()
            };
            lines.push(format!(
                "  {} [{}] modes={} tags={} scenes={} — {}",
                text(&source["source"]),
                source["family"]
                    .as_str()
                    .or_else(|| source["category"].as_str())
                    .unwrap_or_default(),
                list(&source["playback"]["modes"]),
                list(&source["tags"]),
                source["scenes"]
                    .as_array()
                    .or_else(|| source["use_cases"].as_array())
                    .map(|values| values
                        .iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(","))
                    .unwrap_or_default(),
                text(&source["description"]),
            ));
        }
        lines.join("\n")
    }
}

/// Answer one selection query against a profile. A query that matches
/// nothing is a legitimate answer (`status: "no_match"`, exit 0), not an
/// error — the point of the command is to let an agent establish that no
/// exact candidate exists.
pub fn inspect(
    profile: &TextureProfile,
    profile_dir: &Path,
    filter: &Filter,
) -> Result<InspectReport> {
    let mut matched = Vec::new();
    for (name, binding, resolved) in profile.resolved_sources(profile_dir) {
        let entry = match binding {
            TextureSourceBinding::Discoverable(source) => {
                if !filter.matches(name, source) {
                    continue;
                }
                source.to_json(name, &resolved, profile.schema_version)
            }
            TextureSourceBinding::LegacyPath(_) => {
                if filter.has_metadata_constraints()
                    || filter
                        .source
                        .as_deref()
                        .is_some_and(|wanted| wanted != name)
                {
                    continue;
                }
                json!({
                    "source": name,
                    "path": binding.path(),
                    "resolved_path": resolved.display().to_string(),
                    "exists": resolved.is_file(),
                    "metadata_available": false,
                })
            }
        };
        matched.push(entry);
    }
    let matching = matched.len();
    let start = filter.offset.min(matching);
    let end = filter
        .limit
        .map_or(matching, |limit| start.saturating_add(limit).min(matching));
    let matched = matched.drain(start..end).collect();
    Ok(InspectReport {
        profile: profile.name.clone(),
        total: profile.sources.len(),
        matching,
        status: if matching == 0 { "no_match" } else { "match" },
        filter: filter.clone(),
        matched,
    })
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

pub fn profile_dir(path: &Path) -> PathBuf {
    path.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn schema_json() -> String {
    let schema = schemars::schema_for!(TextureProfile);
    let mut value = serde_json::to_value(schema).expect("schema serializes");
    value["properties"]["schema_version"]["enum"] = json!([SCHEMA_VERSION, LATEST_SCHEMA_VERSION]);
    value["$defs"]["TextureSource"]["required"] = json!([
        "path",
        "description",
        "category",
        "tags",
        "playback",
        "use_cases",
        "provenance",
    ]);
    let source_properties = value["$defs"]["TextureSource"]["properties"].clone();
    let mut v2_properties = serde_json::Map::new();
    for field in [
        "path",
        "family",
        "tags",
        "playback",
        "scenes",
        "audio",
        "description",
        "provenance",
    ] {
        if let Some(schema) = source_properties.get(field) {
            v2_properties.insert(field.to_owned(), schema.clone());
        }
    }
    value["$defs"]["TextureSourceV2"] = json!({
        "type": "object",
        "properties": v2_properties,
        "required": ["path", "family", "tags", "playback", "scenes"],
        "additionalProperties": false,
    });
    let family_schema = &mut value["$defs"]["TextureSourceV2"]["properties"]["family"];
    family_schema["type"] = "string".into();
    if let Some(properties) = family_schema.as_object_mut() {
        properties.remove("anyOf");
    }
    value["$defs"]["TextureSourceV2"]["properties"]["family"]["pattern"] =
        "^[a-z][a-z0-9_-]{0,31}$".into();
    value["$defs"]["TextureSourceV2"]["properties"]["scenes"]["items"]["pattern"] =
        "^[a-z][a-z0-9_-]{0,31}$".into();
    value["$defs"]["TextureSourceV2"]["properties"]["scenes"]["minItems"] = 1.into();
    value["$defs"]["TextureSourceV2"]["properties"]["scenes"]["maxItems"] = MAX_TOKENS.into();
    value["$defs"]["TextureSourceV2"]["properties"]["scenes"]["uniqueItems"] = true.into();
    value["$defs"]["AudioMetadata"]["properties"]["duration_seconds"]["exclusiveMinimum"] =
        true.into();
    value["$defs"]["AudioMetadata"]["properties"]["duration_seconds"]["minimum"] = 0.into();
    for field in ["intensity", "brightness"] {
        value["$defs"]["AudioMetadata"]["properties"][field]["minimum"] = 0.into();
        value["$defs"]["AudioMetadata"]["properties"][field]["maximum"] = 1.into();
    }
    value["$defs"]["AudioMetadata"]["properties"]["tonality"]["pattern"] =
        "^[a-z][a-z0-9_-]{0,31}$".into();
    if let Some(variants) = value["$defs"]["TextureSourceBinding"]["anyOf"].as_array_mut() {
        variants.push(json!({"$ref": "#/$defs/TextureSourceV2"}));
    }
    serde_json::to_string_pretty(&value).expect("schema serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(path: &str) -> TextureSource {
        TextureSource {
            path: path.to_owned(),
            description: Some("Wide river bed".to_owned()),
            category: Some(Category::Organic),
            family: None,
            tags: vec!["water".to_owned(), "flowing".to_owned()],
            playback: Playback {
                modes: vec![TextureMode::Loop],
                default_mode: TextureMode::Loop,
                loopable: None,
            },
            use_cases: vec!["forest".to_owned()],
            scenes: Vec::new(),
            provenance: Some(Provenance {
                library: "vcsl@1.2.2".to_owned(),
            }),
            audio: None,
        }
    }

    fn profile(sources: Vec<(&str, TextureSource)>) -> TextureProfile {
        TextureProfile {
            schema_version: SCHEMA_VERSION,
            name: "field".to_owned(),
            description: None,
            root: Some("audio".to_owned()),
            sources: sources
                .into_iter()
                .map(|(name, source)| (name.to_owned(), TextureSourceBinding::Discoverable(source)))
                .collect(),
        }
    }

    #[test]
    fn resolves_portable_source_relative_to_profile() {
        let profile = profile(vec![("river", source("river.flac"))]);
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
            let profile = profile(vec![(name, source("x.wav"))]);
            assert!(profile.validate().is_err(), "accepted {name:?}");
        }
        let mut empty_path = profile(vec![("river", source(""))]);
        assert!(empty_path.validate().is_err());
        empty_path
            .sources
            .get_mut("river")
            .unwrap()
            .discoverable_mut()
            .unwrap()
            .path = "x.wav".to_owned();
        assert!(empty_path.validate().is_ok());
    }

    #[test]
    fn legacy_path_bindings_remain_loadable_and_discoverable_without_invented_metadata() {
        let profile: TextureProfile =
            serde_yaml_ng::from_str("name: legacy\nsources:\n  river: river.wav\n").unwrap();
        assert_eq!(profile.schema_version, SCHEMA_VERSION);
        profile.validate().unwrap();
        assert_eq!(
            profile.resolve(Path::new("/profiles"), "river").unwrap(),
            Path::new("/profiles/river.wav")
        );
        let report = inspect(&profile, Path::new("/profiles"), &Filter::default()).unwrap();
        assert_eq!(report.matched(), 1);
        assert_eq!(report.to_json()["sources"][0]["metadata_available"], false);
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let mut profile = profile(vec![("river", source("river.wav"))]);
        profile.schema_version = 3;
        let error = profile.validate().unwrap_err();
        assert!(matches!(error, Error::Validation { ref path, .. } if path == "schema_version"));
    }

    #[test]
    fn rejects_incomplete_discovery_metadata() {
        type Mutate = Box<dyn Fn(&mut TextureSource)>;
        let cases: Vec<(&str, Mutate)> = vec![
            (
                "sources.river.description",
                Box::new(|s: &mut TextureSource| s.description = Some("  ".to_owned())),
            ),
            (
                "sources.river.tags",
                Box::new(|s: &mut TextureSource| s.tags.clear()),
            ),
            (
                "sources.river.tags[1]",
                Box::new(|s: &mut TextureSource| s.tags = vec!["water".into(), "water".into()]),
            ),
            (
                "sources.river.tags[0]",
                Box::new(|s: &mut TextureSource| s.tags = vec!["Water".into()]),
            ),
            (
                "sources.river.tags",
                Box::new(|s: &mut TextureSource| {
                    s.tags = (0..MAX_TOKENS + 1).map(|i| format!("t{i}")).collect()
                }),
            ),
            (
                "sources.river.use_cases",
                Box::new(|s: &mut TextureSource| s.use_cases.clear()),
            ),
            (
                "sources.river.playback.modes",
                Box::new(|s: &mut TextureSource| s.playback.modes.clear()),
            ),
            (
                "sources.river.playback.default_mode",
                Box::new(|s: &mut TextureSource| s.playback.default_mode = TextureMode::OneShot),
            ),
            (
                "sources.river.provenance.library",
                Box::new(|s: &mut TextureSource| {
                    s.provenance.as_mut().unwrap().library = String::new()
                }),
            ),
            (
                "sources.river.provenance.library",
                Box::new(|s: &mut TextureSource| {
                    s.provenance.as_mut().unwrap().library = "unversioned".to_owned()
                }),
            ),
        ];
        for (field, mutate) in cases {
            let mut profile = profile(vec![("river", source("river.wav"))]);
            mutate(
                profile
                    .sources
                    .get_mut("river")
                    .unwrap()
                    .discoverable_mut()
                    .unwrap(),
            );
            match profile.validate() {
                Err(Error::Validation { path, .. }) => assert_eq!(path, field),
                other => panic!("expected {field} to be rejected, got {other:?}"),
            }
        }
    }

    #[test]
    fn filters_are_exact_and_conjunctive() {
        let mut grind = source("grind.wav");
        grind.category = Some(Category::Industrial);
        grind.tags = vec!["metal".to_owned(), "grinding".to_owned()];
        grind.use_cases = vec!["factory".to_owned()];
        let profile = profile(vec![("river", source("river.wav")), ("grind", grind)]);
        profile.validate().unwrap();
        let dir = Path::new("/profiles");

        let all = inspect(&profile, dir, &Filter::default()).unwrap();
        assert_eq!((all.total, all.matched(), all.status), (2, 2, "match"));

        let industrial = inspect(
            &profile,
            dir,
            &Filter {
                category: Some(Category::Industrial),
                ..Filter::default()
            },
        )
        .unwrap();
        assert_eq!(industrial.matched(), 1);
        assert_eq!(industrial.to_json()["sources"][0]["source"], "grind");

        // Both tags must be present: conjunctive, never best-effort.
        let both = inspect(
            &profile,
            dir,
            &Filter {
                tags: vec!["metal".to_owned(), "grinding".to_owned()],
                ..Filter::default()
            },
        )
        .unwrap();
        assert_eq!(both.matched(), 1);
        let mixed = inspect(
            &profile,
            dir,
            &Filter {
                tags: vec!["metal".to_owned(), "water".to_owned()],
                ..Filter::default()
            },
        )
        .unwrap();
        assert_eq!((mixed.matched(), mixed.status), (0, "no_match"));

        // An unsatisfiable query returns nothing rather than an approximation.
        let unmatched = inspect(
            &profile,
            dir,
            &Filter {
                mode: Some(TextureMode::OneShot),
                ..Filter::default()
            },
        )
        .unwrap();
        assert_eq!((unmatched.matched(), unmatched.status), (0, "no_match"));
    }
}
