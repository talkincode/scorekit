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
//! the same metadata, so the descriptive fields are required, not optional:
//! optional metadata makes the contract only as good as the laziest entry.
//!
//! The split of responsibility is deliberate:
//!
//! - **This file declares intent** — path, category, tags, playback
//!   constraints, scene use cases, originating library.
//! - **`texture_check` measures physics** — existence, decodability,
//!   duration, loudness, checksum. Measured facts are never hand-written
//!   here, because a hand-written duration is a fact nothing can verify and
//!   everything can outdate.

use crate::error::{Error, Location, Result};
use crate::schema::TextureMode;
use schemars::JsonSchema;
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// Protocol version of the texture-profile format. The only supported value.
pub const SCHEMA_VERSION: u16 = 1;

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
    pub description: String,
    /// Coarse sound family; the stable axis agents filter on first.
    pub category: Category,
    /// Free-form descriptors (`[a-z][a-z0-9_-]{0,31}`), 1..=16, no duplicates.
    pub tags: Vec<String>,
    /// Declared scheduling constraints.
    pub playback: Playback,
    /// Scene intents this source is meant to serve (`forest`, `dungeon`,
    /// `tension`), same syntax and limits as `tags`.
    pub use_cases: Vec<String>,
    /// Originating library identity.
    pub provenance: Provenance,
}

/// A source binding accepts the original path-only form for build
/// compatibility, while discovery and certification require the structured
/// form. Keeping both variants in the exported schema makes the v0 protocol
/// additive rather than repurposing the published `sources` values.
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
    /// Protocol version. The only supported value is 1.
    #[serde(default = "default_schema_version")]
    #[schemars(range(min = 1, max = 1))]
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

impl TextureSource {
    fn validate(&self, field: &str) -> Result<()> {
        if self.path.trim().is_empty() {
            return fail(
                format!("{field}.path"),
                "audio path must not be empty".to_owned(),
            );
        }
        if self.description.trim().is_empty() {
            return fail(
                format!("{field}.description"),
                "description must not be empty (it is what an agent reads when choosing)"
                    .to_owned(),
            );
        }
        validate_tokens(&format!("{field}.tags"), &self.tags)?;
        validate_tokens(&format!("{field}.use_cases"), &self.use_cases)?;
        if !valid_library_identity(&self.provenance.library) {
            return fail(
                format!("{field}.provenance.library"),
                format!(
                    "`{}` must be a versioned identity matching <library>@<version>",
                    self.provenance.library
                ),
            );
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

    pub fn to_json(&self, name: &str, resolved: &Path) -> serde_json::Value {
        json!({
            "source": name,
            "path": self.path,
            "resolved_path": resolved.display().to_string(),
            "description": self.description,
            "category": self.category.key(),
            "tags": self.tags,
            "playback": {
                "modes": self.playback.modes.iter().map(|m| mode_key(*m)).collect::<Vec<_>>(),
                "default_mode": mode_key(self.playback.default_mode),
            },
            "use_cases": self.use_cases,
            "provenance": { "library": self.provenance.library },
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

    fn validate(&self, field: &str) -> Result<()> {
        match self {
            Self::LegacyPath(path) if path.trim().is_empty() => {
                fail(field.to_owned(), "audio path must not be empty".to_owned())
            }
            Self::LegacyPath(_) => Ok(()),
            Self::Discoverable(source) => source.validate(field),
        }
    }

    pub fn discoverable(&self, field: &str) -> Result<&TextureSource> {
        match self {
            Self::Discoverable(source) => Ok(source),
            Self::LegacyPath(_) => fail(
                field.to_owned(),
                "path-only legacy binding has no discovery metadata; migrate it to a structured \
                 source before using `texture inspect` or `texture check`"
                    .to_owned(),
            ),
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
        if self.schema_version != SCHEMA_VERSION {
            return fail(
                "schema_version".to_owned(),
                format!(
                    "{} is unsupported; expected {SCHEMA_VERSION}",
                    self.schema_version
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
            source.validate(&format!("sources.{name}"))?;
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
        self.sources.get(name).ok_or_else(|| Error::Validation {
            path: format!("texture_profile.sources.{name}"),
            message: format!(
                "texture profile `{}` has no mapping for source `{name}`",
                self.name
            ),
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

    pub fn resolved_discoverable_sources<'a>(
        &'a self,
        profile_dir: &Path,
    ) -> Result<Vec<(&'a String, &'a TextureSource, PathBuf)>> {
        self.resolved_sources(profile_dir)
            .into_iter()
            .map(|(name, binding, resolved)| {
                binding
                    .discoverable(&format!("sources.{name}"))
                    .map(|source| (name, source, resolved))
            })
            .collect()
    }
}

/// Exact, explainable selection criteria. Every populated field must match
/// (AND), and every comparison is equality or set membership — never a
/// similarity score. Ranking candidates by "closeness" would put creative
/// judgement inside the compiler and would hand back a plausible-looking
/// wrong answer exactly when the honest answer is "nothing fits".
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub source: Option<String>,
    pub category: Option<Category>,
    pub tags: Vec<String>,
    pub mode: Option<TextureMode>,
    pub use_case: Option<String>,
}

impl Filter {
    fn matches(&self, name: &str, source: &TextureSource) -> bool {
        if let Some(wanted) = &self.source
            && name != wanted
        {
            return false;
        }
        if let Some(wanted) = self.category
            && source.category != wanted
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
        {
            return false;
        }
        true
    }

    fn to_json(&self) -> serde_json::Value {
        json!({
            "source": self.source,
            "category": self.category.map(Category::key),
            "tags": self.tags,
            "mode": self.mode.map(mode_key),
            "use_case": self.use_case,
        })
    }
}

#[derive(Debug)]
pub struct InspectReport {
    pub profile: String,
    pub total: usize,
    pub status: &'static str,
    filter: Filter,
    matched: Vec<serde_json::Value>,
}

impl InspectReport {
    pub fn matched(&self) -> usize {
        self.matched.len()
    }

    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "profile": self.profile,
            "total": self.total,
            "matched": self.matched(),
            "status": self.status,
            "filters": self.filter.to_json(),
            "categories": Category::keys(),
            "sources": self.matched,
        })
    }

    pub fn summary(&self) -> String {
        if self.matched.is_empty() {
            return format!(
                "no_match: profile `{}`: 0 of {} source(s) match; no suitable source exists — \
                 acquire and declare one rather than substituting an approximation",
                self.profile, self.total
            );
        }
        let mut lines = vec![format!(
            "ok: profile `{}`: {} of {} source(s) match",
            self.profile,
            self.matched(),
            self.total
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
                "  {} [{}] modes={} tags={} use_cases={} — {}",
                text(&source["source"]),
                text(&source["category"]),
                list(&source["playback"]["modes"]),
                list(&source["tags"]),
                list(&source["use_cases"]),
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
    let matched: Vec<serde_json::Value> = profile
        .resolved_discoverable_sources(profile_dir)?
        .into_iter()
        .filter(|(name, source, _)| filter.matches(name, source))
        .map(|(name, source, resolved)| source.to_json(name, &resolved))
        .collect();
    Ok(InspectReport {
        profile: profile.name.clone(),
        total: profile.sources.len(),
        status: if matched.is_empty() {
            "no_match"
        } else {
            "match"
        },
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
    value["properties"]["schema_version"]["const"] = json!(SCHEMA_VERSION);
    serde_json::to_string_pretty(&value).expect("schema serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(path: &str) -> TextureSource {
        TextureSource {
            path: path.to_owned(),
            description: "Wide river bed".to_owned(),
            category: Category::Organic,
            tags: vec!["water".to_owned(), "flowing".to_owned()],
            playback: Playback {
                modes: vec![TextureMode::Loop],
                default_mode: TextureMode::Loop,
            },
            use_cases: vec!["forest".to_owned()],
            provenance: Provenance {
                library: "vcsl@1.2.2".to_owned(),
            },
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
    fn legacy_path_bindings_remain_loadable_but_are_not_discoverable() {
        let profile: TextureProfile =
            serde_yaml_ng::from_str("name: legacy\nsources:\n  river: river.wav\n").unwrap();
        assert_eq!(profile.schema_version, SCHEMA_VERSION);
        profile.validate().unwrap();
        assert_eq!(
            profile.resolve(Path::new("/profiles"), "river").unwrap(),
            Path::new("/profiles/river.wav")
        );
        let error = inspect(&profile, Path::new("/profiles"), &Filter::default()).unwrap_err();
        assert!(matches!(error, Error::Validation { ref path, .. } if path == "sources.river"));
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let mut profile = profile(vec![("river", source("river.wav"))]);
        profile.schema_version = 2;
        let error = profile.validate().unwrap_err();
        assert!(matches!(error, Error::Validation { ref path, .. } if path == "schema_version"));
    }

    #[test]
    fn rejects_incomplete_discovery_metadata() {
        type Mutate = Box<dyn Fn(&mut TextureSource)>;
        let cases: Vec<(&str, Mutate)> = vec![
            (
                "sources.river.description",
                Box::new(|s: &mut TextureSource| s.description = "  ".to_owned()),
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
                Box::new(|s: &mut TextureSource| s.provenance.library = String::new()),
            ),
            (
                "sources.river.provenance.library",
                Box::new(|s: &mut TextureSource| s.provenance.library = "unversioned".to_owned()),
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
        grind.category = Category::Industrial;
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
