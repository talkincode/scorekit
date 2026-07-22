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
    /// Optional narrative description of the scene — theme, mood, dramatic
    /// intent. Informational only: never affects compiled output. Carried
    /// into `meta.json` so downstream agents can review the music against
    /// its intended story. (Freeform prose, unlike the rejected semantic
    /// story/character fields — see docs/roadmap.md.)
    #[serde(default)]
    pub story: Option<String>,
    /// Tempo in BPM. Range: 20..=300.
    #[schemars(range(min = 20, max = 300))]
    pub tempo: u16,
    /// Key, e.g. `C_major`, `D_minor`, `F#_minor`, `Eb_major`. Default: C_major.
    #[serde(default = "default_key")]
    pub key: String,
    /// Time signature as `N/D`, e.g. `4/4`, `3/4`, `6/8`. Default: 4/4.
    #[serde(default = "default_time_signature")]
    pub time_signature: String,
    /// Length in bars. Range: 1..=256.
    #[schemars(range(min = 1, max = 256))]
    pub bars: u16,
    /// Whether this scene is intended to loop seamlessly (asset metadata).
    #[serde(default)]
    pub r#loop: bool,
    /// Named melodic motifs, referenced by tracks with pattern `melody`.
    /// Sorted map keeps compilation deterministic.
    #[serde(default)]
    pub motifs: BTreeMap<String, Vec<MotifNote>>,
    /// Harmonic progression as diatonic roman numerals (`i`..`vii`, case
    /// conventional), one chord per bar, cycled. All harmony-following
    /// patterns (sustain/arpeggio/bass) derive from it. Default when absent:
    /// I-V-vi-IV in major, i-VI-III-VII in minor.
    #[serde(default)]
    #[schemars(length(max = 32))]
    pub harmony: Vec<String>,
    /// Performance rendering: deterministic humanization, dynamics, swing,
    /// legato. Absent means the exact mechanical rendering (byte-stable).
    #[serde(default)]
    pub performance: Option<Performance>,
    /// Deterministically scheduled field recordings, ambience and sound
    /// effects. Logical source names are bound to audio files by a separate
    /// texture profile at build time; paths never enter the scene protocol.
    #[serde(default)]
    #[schemars(length(max = 16))]
    pub textures: Vec<TextureTrack>,
    /// Instrument tracks. 1..=16 entries, at most 15 melodic plus one drums.
    #[schemars(length(min = 1, max = 16))]
    pub tracks: Vec<Track>,
    /// Suite sections. When present, `build` emits one asset per section
    /// (e.g. intro / explore / combat / victory), all sharing this scene's
    /// tracks, motifs and key.
    #[serde(default)]
    pub sections: Vec<Section>,
}

/// A non-instrument audio layer. `loop` repeats one source from `start_beat`;
/// `one_shot` places the source once at every entry in `at`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextureTrack {
    /// Portable source name resolved through `--texture-profile`.
    pub source: String,
    /// Continuous ambience or beat-scheduled one-shot events.
    pub mode: TextureMode,
    /// Start in quarter-note beats, valid only for `mode: loop`. Default: 0.
    #[serde(default)]
    #[schemars(range(min = 0.0))]
    pub start_beat: Option<f64>,
    /// Trigger positions in quarter-note beats, required for `mode: one_shot`.
    #[serde(default)]
    #[schemars(length(max = 64), inner(range(min = 0.0)))]
    pub at: Vec<f64>,
    /// Linear amplitude multiplier applied before summation. Default: 1.
    #[serde(default = "default_texture_gain")]
    #[schemars(range(min = 0.0, max = 1.0))]
    pub gain: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextureMode {
    Loop,
    OneShot,
}

fn default_texture_gain() -> f32 {
    1.0
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
    #[schemars(range(min = 0.0, max = 1.0))]
    pub intensity: f32,
    /// Playing technique used to pick samples when rendering through an SFZ
    /// renderer profile (`--renderer sfizz`). Does not change the compiled
    /// MIDI; SF2 backends ignore it. Default: sustain.
    #[serde(default)]
    pub articulation: Articulation,
    /// Stereo position 0.0 (hard left)..=1.0 (hard right), 0.5 = center.
    /// Compiled to MIDI CC10 at the start of the track. Absent: no CC10 is
    /// emitted and the synth default (center) applies.
    #[serde(default)]
    #[schemars(range(min = 0.0, max = 1.0))]
    pub pan: Option<f32>,
    /// Reverb send 0.0..=1.0, compiled to MIDI CC91 at the start of the
    /// track — spatial depth (near/far). Absent: no CC91 is emitted.
    /// SFZ instruments respond only if the `.sfz` maps these controllers.
    #[serde(default)]
    #[schemars(range(min = 0.0, max = 1.0))]
    pub reverb: Option<f32>,
    /// Tail portamento, only with pattern `melody`: during the final `glide`
    /// fraction of each note, pitch bends deterministically toward the next
    /// note (clamped to the GM ±2-semitone bend range), resetting exactly at
    /// the next onset. In loop scenes the last note glides toward the first
    /// note's pitch, so the gesture carries across the loop seam. Range:
    /// 0.0..=1.0. Absent or 0: no pitch-bend events are emitted.
    #[serde(default)]
    #[schemars(range(min = 0.0, max = 1.0))]
    pub glide: Option<f32>,
}

/// Playing technique; selects which SFZ file a renderer profile maps the
/// track to. Purely a sample-selection hint — compiled MIDI is identical
/// across articulations.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum Articulation {
    /// Held notes (default).
    #[default]
    Sustain,
    /// Short detached notes.
    Staccato,
    /// Bounced bow (strings).
    Spiccato,
    /// Plucked strings.
    Pizzicato,
    /// Rapid bow repetition (strings).
    Tremolo,
    /// Muted (brass/strings).
    Mute,
}

/// One step of a motif, in scale degrees of the scene key.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MotifNote {
    /// Scale degree: 1 = tonic, 8 = tonic an octave up, negative = below,
    /// 0 = rest. Range: -21..=21.
    #[schemars(range(min = -21, max = 21))]
    pub degree: i8,
    /// Duration in beats. Range: 0.125..=16.
    #[schemars(range(min = 0.125, max = 16.0))]
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
    #[schemars(range(min = 1, max = 256))]
    pub bars: u16,
    /// Whether this section loops seamlessly.
    #[serde(default)]
    pub r#loop: bool,
    /// Optional tempo override in BPM. Range: 20..=300.
    #[serde(default)]
    #[schemars(range(min = 20, max = 300))]
    pub tempo: Option<u16>,
    /// 0-based indices of tracks silenced in this section.
    #[serde(default)]
    pub mute: Vec<usize>,
    /// Multiplier applied to every track's intensity. Range: 0.0..=2.0. Default: 1.
    #[serde(default = "default_section_intensity")]
    #[schemars(range(min = 0.0, max = 2.0))]
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

/// Deterministic performance rendering. Every field has exact compilation
/// semantics; identical input (including seed) yields identical MIDI bytes.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Performance {
    /// Seeded random velocity/timing variation per note.
    #[serde(default)]
    pub humanize: Option<Humanize>,
    /// Swing feel: offbeat eighths delayed by this fraction of half a beat.
    /// Range: 0.0..=0.5. Default: 0 (straight).
    #[serde(default)]
    #[schemars(range(min = 0.0, max = 0.5))]
    pub swing: f32,
    /// Extend melodic note durations so consecutive notes overlap slightly.
    #[serde(default)]
    pub legato: bool,
    /// Dynamic arch over the piece: `start` level rising to `peak` at the
    /// midpoint and returning to `start` — loop-safe by construction.
    #[serde(default)]
    pub dynamics: Option<Dynamics>,
}

/// Per-note random variation from a seeded deterministic generator.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Humanize {
    /// Max onset shift in milliseconds, uniform in ±timing_ms. Range: 0..=50.
    #[serde(default)]
    #[schemars(range(max = 50))]
    pub timing_ms: u8,
    /// Max velocity shift, uniform in ±velocity. Range: 0..=30.
    #[serde(default)]
    #[schemars(range(max = 30))]
    pub velocity: u8,
    /// Random seed; same seed reproduces the same performance bit-exactly.
    #[serde(default)]
    pub seed: u64,
}

/// Dynamic arch endpoints as conventional marks.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Dynamics {
    /// Level at the beginning and end of the piece.
    pub start: Dyn,
    /// Level reached at the midpoint.
    pub peak: Dyn,
}

/// Dynamic marks, mapped to velocity multipliers at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Dyn {
    Pp,
    P,
    Mp,
    Mf,
    F,
    Ff,
}

impl Dyn {
    /// Velocity multiplier for this mark (mf ≈ written intensity).
    pub fn factor(self) -> f32 {
        match self {
            Dyn::Pp => 0.55,
            Dyn::P => 0.7,
            Dyn::Mp => 0.85,
            Dyn::Mf => 1.0,
            Dyn::F => 1.15,
            Dyn::Ff => 1.3,
        }
    }
}

/// Parse a diatonic roman numeral into a 0-based scale-degree index.
/// Case is conventional only: triads are built from the scene's scale either
/// way, so `VI` and `vi` select the same diatonic chord.
pub fn parse_numeral(s: &str) -> std::result::Result<usize, String> {
    match s.to_ascii_lowercase().as_str() {
        "i" => Ok(0),
        "ii" => Ok(1),
        "iii" => Ok(2),
        "iv" => Ok(3),
        "v" => Ok(4),
        "vi" => Ok(5),
        "vii" => Ok(6),
        other => Err(format!(
            "unknown numeral `{other}`, expected one of i..vii/I..VII"
        )),
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
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

/// Deserialization accepts canonical snake_case names plus the registered
/// aliases and case/separator variants (`French Horn` → `horn`); see
/// `instrument::resolve_name`. Serialization always emits the canonical
/// name, so scene round-trips and compiled MIDI stay byte-stable however
/// the instrument was spelled.
impl<'de> Deserialize<'de> for Instrument {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        match crate::instrument::resolve_name(&raw) {
            Some(r) => Ok(r.instrument),
            None => {
                let suggestions = crate::instrument::suggest(&raw);
                let hint = if suggestions.is_empty() {
                    "see `scorekit schema` for the instrument list".to_owned()
                } else {
                    format!("did you mean {}?", suggestions.join(", "))
                };
                Err(serde::de::Error::custom(format!(
                    "unknown instrument `{raw}`; {hint}"
                )))
            }
        }
    }
}

/// snake_case key for an `Instrument`, e.g. `slow_strings` — used by
/// renderer profiles (`--renderer sfizz`) to look up sample mappings without
/// duplicating the enum's serde naming.
pub fn instrument_key(i: Instrument) -> String {
    serde_json::to_value(i)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Parse a snake_case instrument key back into an `Instrument`; used to
/// validate renderer-profile keys against the real enum instead of accepting
/// arbitrary strings.
pub fn parse_instrument_key(s: &str) -> Option<Instrument> {
    serde_json::from_value(serde_json::Value::String(s.to_owned())).ok()
}

/// snake_case key for an `Articulation`, e.g. `spiccato`.
pub fn articulation_key(a: Articulation) -> String {
    serde_json::to_value(a)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Parse a snake_case articulation key back into an `Articulation`.
pub fn parse_articulation_key(s: &str) -> Option<Articulation> {
    serde_json::from_value(serde_json::Value::String(s.to_owned())).ok()
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
        let time_sig =
            parse_time_signature(&self.time_signature).map_err(|m| Error::Validation {
                path: "time_signature".to_owned(),
                message: m,
            })?;
        if self.textures.len() > 16 {
            return fail(
                "textures",
                format!(
                    "{} texture tracks exceed the limit of 16",
                    self.textures.len()
                ),
            );
        }
        // Textures are shared by every section, so their schedule must fit the
        // shortest compiled timeline. Validating against the longest section
        // lets an event from pass one wrap into pass two of a shorter loop.
        // If a section length is itself invalid, defer range checks here so the
        // dedicated section validation below reports the primary error.
        let beats_for =
            |bars: u16| f64::from(bars) * f64::from(time_sig.num) * 4.0 / f64::from(time_sig.den);
        let texture_timeline = if self.sections.is_empty() {
            Some(("scene".to_owned(), beats_for(self.bars)))
        } else if self.sections.iter().all(|s| (1..=256).contains(&s.bars)) {
            self.sections
                .iter()
                .map(|s| (format!("section `{}`", s.name), beats_for(s.bars)))
                .min_by(|a, b| a.1.total_cmp(&b.1))
        } else {
            None
        };
        let has_loop_section = self.r#loop || self.sections.iter().any(|s| s.r#loop);
        for (i, texture) in self.textures.iter().enumerate() {
            if !crate::texture::valid_source_name(&texture.source) {
                return fail(
                    &format!("textures[{i}].source"),
                    format!(
                        "`{}` must match [a-z][a-z0-9_-]{{0,63}} (portable source name)",
                        texture.source
                    ),
                );
            }
            if !texture.gain.is_finite() || !(0.0..=1.0).contains(&texture.gain) {
                return fail(
                    &format!("textures[{i}].gain"),
                    format!("{} out of range 0.0..=1.0", texture.gain),
                );
            }
            match texture.mode {
                TextureMode::Loop => {
                    if !texture.at.is_empty() {
                        return fail(
                            &format!("textures[{i}].at"),
                            "`at` is only valid with mode `one_shot`".to_owned(),
                        );
                    }
                    let start = texture.start_beat.unwrap_or(0.0);
                    if !start.is_finite() {
                        return fail(
                            &format!("textures[{i}].start_beat"),
                            format!("{start} must be finite"),
                        );
                    }
                    if let Some((timeline, max_beats)) = &texture_timeline
                        && !(0.0..*max_beats).contains(&start)
                    {
                        return fail(
                            &format!("textures[{i}].start_beat"),
                            format!("{start} out of range 0.0..{max_beats} for {timeline}"),
                        );
                    }
                    if has_loop_section && start != 0.0 {
                        return fail(
                            &format!("textures[{i}].start_beat"),
                            "loop textures must start at beat 0 when the scene or any section loops"
                                .to_owned(),
                        );
                    }
                }
                TextureMode::OneShot => {
                    if texture.start_beat.is_some() {
                        return fail(
                            &format!("textures[{i}].start_beat"),
                            "`start_beat` is only valid with mode `loop`".to_owned(),
                        );
                    }
                    if texture.at.is_empty() {
                        return fail(
                            &format!("textures[{i}].at"),
                            "mode `one_shot` requires at least one trigger beat".to_owned(),
                        );
                    }
                    if texture.at.len() > 64 {
                        return fail(
                            &format!("textures[{i}].at"),
                            format!("{} trigger beats exceed the limit of 64", texture.at.len()),
                        );
                    }
                    for (j, at) in texture.at.iter().enumerate() {
                        if !at.is_finite() {
                            return fail(
                                &format!("textures[{i}].at[{j}]"),
                                format!("{at} must be finite"),
                            );
                        }
                        if let Some((timeline, max_beats)) = &texture_timeline
                            && !(0.0..*max_beats).contains(at)
                        {
                            return fail(
                                &format!("textures[{i}].at[{j}]"),
                                format!("{at} out of range 0.0..{max_beats} for {timeline}"),
                            );
                        }
                    }
                }
            }
        }
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
            if let Some(pan) = t.pan
                && !(0.0..=1.0).contains(&pan)
            {
                return fail(
                    &format!("tracks[{i}].pan"),
                    format!("{pan} out of range 0.0..=1.0"),
                );
            }
            if let Some(reverb) = t.reverb
                && !(0.0..=1.0).contains(&reverb)
            {
                return fail(
                    &format!("tracks[{i}].reverb"),
                    format!("{reverb} out of range 0.0..=1.0"),
                );
            }
            if let Some(glide) = t.glide {
                if !(0.0..=1.0).contains(&glide) {
                    return fail(
                        &format!("tracks[{i}].glide"),
                        format!("{glide} out of range 0.0..=1.0"),
                    );
                }
                if t.pattern != Pattern::Melody {
                    return fail(
                        &format!("tracks[{i}].glide"),
                        "`glide` is only valid with pattern `melody`".to_owned(),
                    );
                }
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
        if self.harmony.len() > 32 {
            return fail(
                "harmony",
                format!("{} chords exceed the limit of 32", self.harmony.len()),
            );
        }
        for (j, numeral) in self.harmony.iter().enumerate() {
            if let Err(m) = parse_numeral(numeral) {
                return fail(&format!("harmony[{j}]"), m);
            }
        }
        if let Some(p) = &self.performance {
            if !(0.0..=0.5).contains(&p.swing) {
                return fail(
                    "performance.swing",
                    format!("{} out of range 0.0..=0.5", p.swing),
                );
            }
            if let Some(h) = &p.humanize {
                if h.timing_ms > 50 {
                    return fail(
                        "performance.humanize.timing_ms",
                        format!("{} out of range 0..=50", h.timing_ms),
                    );
                }
                if h.velocity > 30 {
                    return fail(
                        "performance.humanize.velocity",
                        format!("{} out of range 0..=30", h.velocity),
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
