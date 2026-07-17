use crate::error::{Error, Location, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// A scene is the unit of compilation: one loopable piece of game music, or —
/// when `sections` is present — a suite of related cues sharing tracks,
/// motifs, key and tempo.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    /// Optional human-readable title (informational only).
    #[serde(default)]
    pub title: Option<String>,
    /// Tempo in BPM. Range: 20..=300.
    pub tempo: u16,
    /// Key, e.g. `C_major`, `D_minor`, `F#_minor`, `Eb_major`. Default: C_major.
    #[serde(default = "default_key")]
    pub key: String,
    /// Time signature as `N/D`, e.g. `4/4`, `3/4`, `6/8`. Default: 4/4.
    #[serde(default = "default_time_signature")]
    pub time_signature: String,
    /// Length in bars. Range: 1..=256.
    pub bars: u16,
    /// Whether this scene is intended to loop seamlessly (asset metadata).
    #[serde(default)]
    pub r#loop: bool,
    /// Named melodic motifs, referenced by tracks with pattern `melody`.
    /// Sorted map keeps compilation deterministic.
    #[serde(default)]
    pub motifs: BTreeMap<String, Vec<MotifNote>>,
    /// Instrument tracks. 1..=16 entries, at most 15 melodic plus one drums.
    pub tracks: Vec<Track>,
    /// Suite sections. When present, `build` emits one asset per section
    /// (e.g. intro / explore / combat / victory), all sharing this scene's
    /// tracks, motifs and key.
    #[serde(default)]
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Track {
    /// General MIDI instrument name, or `drums` for the percussion channel.
    pub instrument: Instrument,
    /// What this track plays. `drums` pattern pairs only with the `drums` instrument.
    pub pattern: Pattern,
    /// Motif name to play; required with (and only with) pattern `melody`.
    #[serde(default)]
    pub motif: Option<String>,
    /// Dynamic level 0.0..=1.0, scales note velocities. Default: 0.6.
    #[serde(default = "default_intensity")]
    pub intensity: f32,
}

/// One step of a motif, in scale degrees of the scene key.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MotifNote {
    /// Scale degree: 1 = tonic, 8 = tonic an octave up, negative = below,
    /// 0 = rest. Range: -21..=21.
    pub degree: i8,
    /// Duration in beats. Range: 0.125..=16.
    pub beats: f64,
}

/// A named cue in a suite. Sections share the scene's tracks, motifs, key and
/// (unless overridden) tempo, so every cue develops the same material —
/// transitions are just short non-loop sections.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Section {
    /// Section name, used in output file names. `[A-Za-z0-9_-]+`.
    pub name: String,
    /// Length in bars. Range: 1..=256.
    pub bars: u16,
    /// Whether this section loops seamlessly.
    #[serde(default)]
    pub r#loop: bool,
    /// Optional tempo override in BPM. Range: 20..=300.
    #[serde(default)]
    pub tempo: Option<u16>,
    /// 0-based indices of tracks silenced in this section.
    #[serde(default)]
    pub mute: Vec<usize>,
    /// Multiplier applied to every track's intensity. Range: 0.0..=2.0. Default: 1.
    #[serde(default = "default_section_intensity")]
    pub intensity: f32,
}

fn default_key() -> String {
    "C_major".to_owned()
}

fn default_time_signature() -> String {
    "4/4".to_owned()
}

fn default_intensity() -> f32 {
    0.6
}

fn default_section_intensity() -> f32 {
    1.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Pattern {
    /// Whole-bar chords.
    Sustain,
    /// Broken chords in eighth notes.
    Arpeggio,
    /// Root notes anchoring the harmony, one to two octaves down.
    Bass,
    /// Kick / snare / hi-hat groove on the percussion channel.
    Drums,
    /// Plays the motif named by the track's `motif` field, looped/truncated
    /// to fill the section.
    Melody,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Instrument {
    Piano,
    BrightPiano,
    Epiano,
    Harpsichord,
    Celesta,
    Glockenspiel,
    MusicBox,
    Vibraphone,
    Marimba,
    Xylophone,
    TubularBells,
    Organ,
    Accordion,
    Guitar,
    SteelGuitar,
    ElectricGuitar,
    MutedGuitar,
    Bass,
    PickedBass,
    FretlessBass,
    SlapBass,
    SynthBass,
    Violin,
    Viola,
    Cello,
    Contrabass,
    TremoloStrings,
    Pizzicato,
    Harp,
    Timpani,
    Strings,
    SlowStrings,
    SynthStrings,
    Choir,
    Voice,
    Trumpet,
    Trombone,
    Tuba,
    Horn,
    Brass,
    Sax,
    Oboe,
    EnglishHorn,
    Bassoon,
    Clarinet,
    Piccolo,
    Flute,
    Recorder,
    PanFlute,
    Whistle,
    Ocarina,
    SquareLead,
    SawLead,
    Pad,
    WarmPad,
    ChoirPad,
    BowedPad,
    HaloPad,
    SweepPad,
    Drums,
}

impl Instrument {
    /// General MIDI program number; `None` for the percussion channel.
    pub fn gm_program(self) -> Option<u8> {
        use Instrument::*;
        Some(match self {
            Piano => 0,
            BrightPiano => 1,
            Epiano => 4,
            Harpsichord => 6,
            Celesta => 8,
            Glockenspiel => 9,
            MusicBox => 10,
            Vibraphone => 11,
            Marimba => 12,
            Xylophone => 13,
            TubularBells => 14,
            Organ => 19,
            Accordion => 21,
            Guitar => 24,
            SteelGuitar => 25,
            ElectricGuitar => 27,
            MutedGuitar => 28,
            Bass => 33,
            PickedBass => 34,
            FretlessBass => 35,
            SlapBass => 36,
            SynthBass => 38,
            Violin => 40,
            Viola => 41,
            Cello => 42,
            Contrabass => 43,
            TremoloStrings => 44,
            Pizzicato => 45,
            Harp => 46,
            Timpani => 47,
            Strings => 48,
            SlowStrings => 49,
            SynthStrings => 50,
            Choir => 52,
            Voice => 53,
            Trumpet => 56,
            Trombone => 57,
            Tuba => 58,
            Horn => 60,
            Brass => 61,
            Sax => 65,
            Oboe => 68,
            EnglishHorn => 69,
            Bassoon => 70,
            Clarinet => 71,
            Piccolo => 72,
            Flute => 73,
            Recorder => 74,
            PanFlute => 75,
            Whistle => 78,
            Ocarina => 79,
            SquareLead => 80,
            SawLead => 81,
            Pad => 88,
            WarmPad => 89,
            ChoirPad => 91,
            BowedPad => 92,
            HaloPad => 94,
            SweepPad => 95,
            Drums => return None,
        })
    }
}

/// Parsed key: pitch class of the root (0 = C) and mode.
#[derive(Debug, Clone, Copy)]
pub struct Key {
    pub root_pc: u8,
    pub minor: bool,
}

/// Parsed time signature.
#[derive(Debug, Clone, Copy)]
pub struct TimeSig {
    pub num: u8,
    pub den: u8,
}

pub fn parse_key(s: &str) -> std::result::Result<Key, String> {
    let (note, mode) = s
        .split_once('_')
        .ok_or_else(|| format!("expected `<Note>_<major|minor>`, got `{s}`"))?;
    let root_pc = match note {
        "C" => 0,
        "C#" | "Db" => 1,
        "D" => 2,
        "D#" | "Eb" => 3,
        "E" => 4,
        "F" => 5,
        "F#" | "Gb" => 6,
        "G" => 7,
        "G#" | "Ab" => 8,
        "A" => 9,
        "A#" | "Bb" => 10,
        "B" => 11,
        other => return Err(format!("unknown note `{other}`")),
    };
    let minor = match mode {
        "major" => false,
        "minor" => true,
        other => return Err(format!("unknown mode `{other}`, expected major|minor")),
    };
    Ok(Key { root_pc, minor })
}

pub fn parse_time_signature(s: &str) -> std::result::Result<TimeSig, String> {
    let (num, den) = s
        .split_once('/')
        .ok_or_else(|| format!("expected `N/D`, got `{s}`"))?;
    let num: u8 = num
        .parse()
        .map_err(|_| format!("invalid numerator `{num}`"))?;
    let den: u8 = den
        .parse()
        .map_err(|_| format!("invalid denominator `{den}`"))?;
    if !(1..=12).contains(&num) {
        return Err(format!("numerator {num} out of range 1..=12"));
    }
    if ![2, 4, 8, 16].contains(&den) {
        return Err(format!("denominator {den} must be one of 2, 4, 8, 16"));
    }
    Ok(TimeSig { num, den })
}

impl Scene {
    /// Semantic validation beyond what serde enforces. Errors carry a field path.
    pub fn validate(&self) -> Result<()> {
        let fail = |path: &str, message: String| {
            Err(Error::Validation {
                path: path.to_owned(),
                message,
            })
        };
        if !(20..=300).contains(&self.tempo) {
            return fail("tempo", format!("{} out of range 20..=300", self.tempo));
        }
        if !(1..=256).contains(&self.bars) {
            return fail("bars", format!("{} out of range 1..=256", self.bars));
        }
        parse_key(&self.key).map_err(|m| Error::Validation {
            path: "key".to_owned(),
            message: m,
        })?;
        parse_time_signature(&self.time_signature).map_err(|m| Error::Validation {
            path: "time_signature".to_owned(),
            message: m,
        })?;
        if self.tracks.is_empty() {
            return fail("tracks", "at least one track is required".to_owned());
        }
        let melodic = self
            .tracks
            .iter()
            .filter(|t| t.instrument != Instrument::Drums)
            .count();
        let drums = self.tracks.len() - melodic;
        if melodic > 15 {
            return fail(
                "tracks",
                format!("{melodic} melodic tracks exceed the 15-channel limit"),
            );
        }
        if drums > 1 {
            return fail("tracks", "at most one drums track is supported".to_owned());
        }
        for (i, t) in self.tracks.iter().enumerate() {
            if !(0.0..=1.0).contains(&t.intensity) {
                return fail(
                    &format!("tracks[{i}].intensity"),
                    format!("{} out of range 0.0..=1.0", t.intensity),
                );
            }
            match (
                t.instrument == Instrument::Drums,
                t.pattern == Pattern::Drums,
            ) {
                (true, false) => {
                    return fail(
                        &format!("tracks[{i}].pattern"),
                        "instrument `drums` requires pattern `drums`".to_owned(),
                    );
                }
                (false, true) => {
                    return fail(
                        &format!("tracks[{i}].pattern"),
                        "pattern `drums` requires instrument `drums`".to_owned(),
                    );
                }
                _ => {}
            }
            match (t.pattern == Pattern::Melody, &t.motif) {
                (true, None) => {
                    return fail(
                        &format!("tracks[{i}].motif"),
                        "pattern `melody` requires a `motif` name".to_owned(),
                    );
                }
                (true, Some(name)) if !self.motifs.contains_key(name) => {
                    return fail(
                        &format!("tracks[{i}].motif"),
                        format!(
                            "unknown motif `{name}` (defined: {:?})",
                            self.motifs.keys().collect::<Vec<_>>()
                        ),
                    );
                }
                (false, Some(_)) => {
                    return fail(
                        &format!("tracks[{i}].motif"),
                        "`motif` is only valid with pattern `melody`".to_owned(),
                    );
                }
                _ => {}
            }
        }
        for (name, notes) in &self.motifs {
            if notes.is_empty() {
                return fail(&format!("motifs.{name}"), "motif has no notes".to_owned());
            }
            for (j, n) in notes.iter().enumerate() {
                if !(-21..=21).contains(&n.degree) {
                    return fail(
                        &format!("motifs.{name}[{j}].degree"),
                        format!("{} out of range -21..=21", n.degree),
                    );
                }
                if !(0.125..=16.0).contains(&n.beats) {
                    return fail(
                        &format!("motifs.{name}[{j}].beats"),
                        format!("{} out of range 0.125..=16", n.beats),
                    );
                }
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        for (i, s) in self.sections.iter().enumerate() {
            if s.name.is_empty()
                || !s
                    .name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                return fail(
                    &format!("sections[{i}].name"),
                    format!(
                        "`{}` must match [A-Za-z0-9_-]+ (used in file names)",
                        s.name
                    ),
                );
            }
            if !seen.insert(s.name.as_str()) {
                return fail(
                    &format!("sections[{i}].name"),
                    format!("duplicate section name `{}`", s.name),
                );
            }
            if !(1..=256).contains(&s.bars) {
                return fail(
                    &format!("sections[{i}].bars"),
                    format!("{} out of range 1..=256", s.bars),
                );
            }
            if let Some(t) = s.tempo
                && !(20..=300).contains(&t)
            {
                return fail(
                    &format!("sections[{i}].tempo"),
                    format!("{t} out of range 20..=300"),
                );
            }
            if !(0.0..=2.0).contains(&s.intensity) {
                return fail(
                    &format!("sections[{i}].intensity"),
                    format!("{} out of range 0.0..=2.0", s.intensity),
                );
            }
            let muted: std::collections::BTreeSet<usize> = s.mute.iter().copied().collect();
            for (j, &m) in s.mute.iter().enumerate() {
                if m >= self.tracks.len() {
                    return fail(
                        &format!("sections[{i}].mute[{j}]"),
                        format!(
                            "track index {m} out of range (scene has {})",
                            self.tracks.len()
                        ),
                    );
                }
            }
            if muted.len() >= self.tracks.len() {
                return fail(
                    &format!("sections[{i}].mute"),
                    "section mutes every track".to_owned(),
                );
            }
        }
        Ok(())
    }

    /// Derive the standalone scene a section compiles to: shared key, motifs
    /// and tracks; section-local bars, loop flag, tempo and dynamics.
    pub fn for_section(&self, section: &Section) -> Scene {
        let mut derived = self.clone();
        derived.title = Some(match &self.title {
            Some(t) => format!("{t} — {}", section.name),
            None => section.name.clone(),
        });
        derived.tempo = section.tempo.unwrap_or(self.tempo);
        derived.bars = section.bars;
        derived.r#loop = section.r#loop;
        derived.sections = Vec::new();
        derived.tracks = self
            .tracks
            .iter()
            .enumerate()
            .filter(|(i, _)| !section.mute.contains(i))
            .map(|(_, t)| {
                let mut t = t.clone();
                t.intensity = (t.intensity * section.intensity).clamp(0.0, 1.0);
                t
            })
            .collect();
        derived
    }
}

/// Read, parse and validate a scene file.
pub fn load_scene(path: &Path) -> Result<Scene> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })?;
    let scene: Scene = serde_yaml_ng::from_str(&text).map_err(|e| Error::Parse {
        message: e.to_string(),
        location: e.location().map(|l| Location {
            line: l.line(),
            column: l.column(),
        }),
    })?;
    scene.validate()?;
    Ok(scene)
}

/// JSON Schema of the scene DSL, for agent consumption.
pub fn schema_json() -> String {
    let schema = schemars::schema_for!(Scene);
    serde_json::to_string_pretty(&schema).expect("schema serializes")
}
