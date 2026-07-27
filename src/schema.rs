use crate::error::{Error, Location, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

const MAX_EXPANDED_CLIP_EVENTS_PER_TRACK: u64 = 65_536;

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
    /// Named exact event sequences referenced by tracks with pattern `clip`.
    /// Sorted maps make clip and event declaration order semantically inert.
    #[serde(default)]
    #[schemars(length(max = 128))]
    pub clips: BTreeMap<String, Clip>,
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
    /// Instrument tracks. 1..=16 entries, at most 15 melodic plus one
    /// percussion track (`drums` or `tabla`).
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

/// An exact authored event sequence. Beat positions are quarter-note beats,
/// independently of the scene's time-signature denominator.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Clip {
    /// Pitched notes or General MIDI percussion voices.
    pub kind: ClipKind,
    /// Clip duration in quarter-note beats.
    #[schemars(range(min = 0.0020833333333333333, max = 1024.0))]
    pub length_beats: f64,
    /// Play once from the track start or repeat to fill the compiled timeline.
    pub mode: ClipMode,
    /// Stable event identity to exact note/percussion data.
    #[schemars(length(min = 1, max = 2048))]
    pub events: BTreeMap<String, ClipEvent>,
    /// Deterministic step automation keyed by stable lane identity.
    #[serde(default)]
    #[schemars(length(max = 4))]
    pub automation: BTreeMap<String, AutomationLane>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClipKind {
    Pitched,
    Percussion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClipMode {
    Once,
    Loop,
}

/// One exact clip event. `pitch`/`duration` are required for pitched clips;
/// `voice` is required for percussion clips and duration defaults to 1/8 beat.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClipEvent {
    /// Onset in quarter-note beats from the clip start.
    #[schemars(range(min = 0.0))]
    pub at: f64,
    /// Duration in quarter-note beats.
    #[serde(default)]
    #[schemars(range(min = 0.0020833333333333333, max = 1024.0))]
    pub duration: Option<f64>,
    /// Absolute scientific pitch (`C-1` = MIDI 0, `C4` = MIDI 60).
    #[serde(default)]
    pub pitch: Option<String>,
    /// Frozen General MIDI percussion identity.
    #[serde(default)]
    pub voice: Option<PercussionVoice>,
    /// Authored MIDI velocity.
    #[schemars(range(min = 1, max = 127))]
    pub velocity: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AutomationLane {
    /// Portable MIDI controller or pitch-bend target.
    pub target: AutomationTarget,
    /// Stable point identity to exact step value.
    #[schemars(length(min = 1, max = 512))]
    pub points: BTreeMap<String, AutomationPoint>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AutomationTarget {
    Cc1,
    Cc11,
    Cc74,
    PitchBend,
}

impl AutomationTarget {
    pub fn key(self) -> &'static str {
        match self {
            AutomationTarget::Cc1 => "cc1",
            AutomationTarget::Cc11 => "cc11",
            AutomationTarget::Cc74 => "cc74",
            AutomationTarget::PitchBend => "pitch_bend",
        }
    }

    pub fn controller(self) -> Option<u8> {
        match self {
            AutomationTarget::Cc1 => Some(1),
            AutomationTarget::Cc11 => Some(11),
            AutomationTarget::Cc74 => Some(74),
            AutomationTarget::PitchBend => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AutomationPoint {
    /// Position in quarter-note beats from the clip start.
    #[schemars(range(min = 0.0))]
    pub at: f64,
    /// CC targets use 0..=127; pitch bend uses -8192..=8191 around center 0.
    #[schemars(range(min = -8192, max = 8191))]
    pub value: i16,
}

/// Portable percussion names with permanently fixed General MIDI note keys.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PercussionVoice {
    Kick,
    Snare,
    Clap,
    ClosedHat,
    PedalHat,
    OpenHat,
    LowTom,
    MidTom,
    HighTom,
    Crash,
    Ride,
}

impl PercussionVoice {
    pub fn midi_key(self) -> u8 {
        match self {
            PercussionVoice::Kick => 36,
            PercussionVoice::Snare => 38,
            PercussionVoice::Clap => 39,
            PercussionVoice::ClosedHat => 42,
            PercussionVoice::PedalHat => 44,
            PercussionVoice::LowTom => 45,
            PercussionVoice::OpenHat => 46,
            PercussionVoice::MidTom => 47,
            PercussionVoice::HighTom => 50,
            PercussionVoice::Crash => 49,
            PercussionVoice::Ride => 51,
        }
    }
}

fn default_percussion_duration() -> f64 {
    0.125
}

/// Quantize quarter-note beats to the protocol's fixed PPQ-480 grid.
pub fn quarter_beats_to_ticks(beats: f64) -> Option<u32> {
    if !beats.is_finite() || beats < 0.0 {
        return None;
    }
    let ticks = (beats * 480.0).round();
    if ticks > f64::from(u32::MAX) {
        None
    } else {
        Some(ticks as u32)
    }
}

fn expanded_clip_event_count(clip: &Clip, timeline_ticks: u32) -> u64 {
    let authored = clip.events.len() as u64
        + clip
            .automation
            .values()
            .map(|lane| lane.points.len() as u64)
            .sum::<u64>();
    let passes = match clip.mode {
        ClipMode::Once => 1,
        ClipMode::Loop => {
            let clip_ticks =
                quarter_beats_to_ticks(clip.length_beats).expect("clip length is validated");
            u64::from(timeline_ticks / clip_ticks)
        }
    };
    authored.saturating_mul(passes)
}

/// Parse scientific pitch notation with `C-1 = 0`, `C4 = 60`, `A4 = 69`.
pub fn parse_absolute_pitch(raw: &str) -> std::result::Result<u8, String> {
    let mut chars = raw.chars();
    let letter = chars
        .next()
        .ok_or_else(|| "expected pitch like `F1` or `C#2`".to_owned())?
        .to_ascii_uppercase();
    let natural = match letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return Err(format!("unknown pitch letter `{letter}`")),
    };
    let rest: String = chars.collect();
    let (accidental, octave_text) = match rest.as_bytes().first() {
        Some(b'#') => (1, &rest[1..]),
        Some(b'b') => (-1, &rest[1..]),
        _ => (0, rest.as_str()),
    };
    if octave_text.is_empty() {
        return Err(format!("pitch `{raw}` is missing an octave"));
    }
    let octave: i32 = octave_text
        .parse()
        .map_err(|_| format!("pitch `{raw}` has an invalid octave"))?;
    let midi = (i64::from(octave) + 1) * 12 + i64::from(natural + accidental);
    if !(0..=127).contains(&midi) {
        return Err(format!("pitch `{raw}` resolves outside MIDI 0..=127"));
    }
    Ok(midi as u8)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Track {
    /// Stable scene-local identity used by routing, sections, stems and metadata.
    #[schemars(regex(pattern = "^[a-z][a-z0-9_-]{0,63}$"))]
    pub id: String,
    /// Logical orchestration palette. Absent uses the orchestration's default.
    /// Routing metadata only: it never changes compiled MIDI.
    #[serde(default)]
    #[schemars(regex(pattern = "^[a-z][a-z0-9_-]{0,63}$"))]
    pub palette: Option<String>,
    /// Portable instrument identity. Some require an exact renderer-profile
    /// source because General MIDI has no matching program.
    pub instrument: Instrument,
    /// What this track plays. `drums` and `tabla` each pair only with their
    /// namesake percussion instrument.
    pub pattern: Pattern,
    /// Motif name to play; required with (and only with) pattern `melody`.
    #[serde(default)]
    pub motif: Option<String>,
    /// Named exact event sequence; required with (and only with) pattern `clip`.
    #[serde(default)]
    pub clip: Option<String>,
    /// Dynamic level 0.0..=1.0, scales note velocities. Default: 0.6.
    #[serde(default = "default_intensity")]
    #[schemars(range(min = 0.0, max = 1.0))]
    pub intensity: f32,
    /// Playing technique used to pick samples from the leaf renderer profile
    /// selected by `--orchestration` (`--renderer sfizz`). Does not change the
    /// compiled MIDI; SF2 backends ignore it. Default: sustain.
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

/// Playing technique; selects which SFZ file the track's orchestration palette
/// maps to. Purely a sample-selection hint — compiled MIDI is identical across
/// articulations.
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
    /// Stable IDs of tracks silenced in this section.
    #[serde(default)]
    pub mute: Vec<String>,
    /// Section-local clip replacements keyed by stable track ID.
    #[serde(default)]
    pub clips: BTreeMap<String, String>,
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
    /// Plays the exact event sequence named by the track's `clip` field.
    Clip,
    /// Deterministic 16-beat tabla theka, cycled across the scene.
    Tabla,
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
    Erhu,
    Pipa,
    Guzheng,
    Dizi,
    Shakuhachi,
    Shamisen,
    Sitar,
    Tabla,
    Oud,
    Ney,
    Duduk,
}

impl Instrument {
    /// Exact General MIDI program number. `None` means the instrument needs a
    /// renderer-profile mapping (or uses the percussion channel).
    pub fn gm_program(self) -> Option<u8> {
        use Instrument::*;
        match self {
            Piano => Some(0),
            BrightPiano => Some(1),
            Epiano => Some(4),
            Harpsichord => Some(6),
            Celesta => Some(8),
            Glockenspiel => Some(9),
            MusicBox => Some(10),
            Vibraphone => Some(11),
            Marimba => Some(12),
            Xylophone => Some(13),
            TubularBells => Some(14),
            Organ => Some(19),
            Accordion => Some(21),
            Guitar => Some(24),
            SteelGuitar => Some(25),
            ElectricGuitar => Some(27),
            MutedGuitar => Some(28),
            Bass => Some(33),
            PickedBass => Some(34),
            FretlessBass => Some(35),
            SlapBass => Some(36),
            SynthBass => Some(38),
            Violin => Some(40),
            Viola => Some(41),
            Cello => Some(42),
            Contrabass => Some(43),
            TremoloStrings => Some(44),
            Pizzicato => Some(45),
            Harp => Some(46),
            Timpani => Some(47),
            Strings => Some(48),
            SlowStrings => Some(49),
            SynthStrings => Some(50),
            Choir => Some(52),
            Voice => Some(53),
            Trumpet => Some(56),
            Trombone => Some(57),
            Tuba => Some(58),
            Horn => Some(60),
            Brass => Some(61),
            Sax => Some(65),
            Oboe => Some(68),
            EnglishHorn => Some(69),
            Bassoon => Some(70),
            Clarinet => Some(71),
            Piccolo => Some(72),
            Flute => Some(73),
            Recorder => Some(74),
            PanFlute => Some(75),
            Whistle => Some(78),
            Ocarina => Some(79),
            SquareLead => Some(80),
            SawLead => Some(81),
            Pad => Some(88),
            WarmPad => Some(89),
            ChoirPad => Some(91),
            BowedPad => Some(92),
            HaloPad => Some(94),
            SweepPad => Some(95),
            Shakuhachi => Some(77),
            Sitar => Some(104),
            Shamisen => Some(106),
            Drums | Erhu | Pipa | Guzheng | Dizi | Tabla | Oud | Ney | Duduk => None,
        }
    }

    /// Unpitched instruments that reserve MIDI channel 10.
    pub fn is_percussion(self) -> bool {
        matches!(self, Instrument::Drums | Instrument::Tabla)
    }

    /// Whether a General MIDI sound source can represent this identity exactly.
    pub fn has_exact_gm_sound(self) -> bool {
        self == Instrument::Drums || self.gm_program().is_some()
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
            if !crate::texture::valid_logical_name(&texture.source) {
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
        if self.clips.len() > 128 {
            return fail(
                "clips",
                format!("{} clips exceed the limit of 128", self.clips.len()),
            );
        }
        for (name, clip) in &self.clips {
            if !crate::texture::valid_logical_name(name) {
                return fail(
                    &format!("clips.{name}"),
                    format!("`{name}` must match [a-z][a-z0-9_-]{{0,63}} (stable clip identity)"),
                );
            }
            let Some(length_ticks) = quarter_beats_to_ticks(clip.length_beats) else {
                return fail(
                    &format!("clips.{name}.length_beats"),
                    format!("{} must be finite and non-negative", clip.length_beats),
                );
            };
            if length_ticks == 0 || clip.length_beats > 1024.0 {
                return fail(
                    &format!("clips.{name}.length_beats"),
                    format!(
                        "{} must quantize to at least 1 tick and be <= 1024 beats",
                        clip.length_beats
                    ),
                );
            }
            if clip.events.is_empty() {
                return fail(
                    &format!("clips.{name}.events"),
                    "clip must contain at least one event".to_owned(),
                );
            }
            if clip.events.len() > 2048 {
                return fail(
                    &format!("clips.{name}.events"),
                    format!("{} events exceed the limit of 2048", clip.events.len()),
                );
            }
            let mut pitched_spans: Vec<(u8, u32, u32, &str)> = Vec::new();
            let mut percussion_onsets = std::collections::BTreeSet::new();
            for (event_id, event) in &clip.events {
                let event_path = format!("clips.{name}.events.{event_id}");
                if !crate::texture::valid_logical_name(event_id) {
                    return fail(
                        &event_path,
                        format!(
                            "`{event_id}` must match [a-z][a-z0-9_-]{{0,63}} \
                             (stable event identity)"
                        ),
                    );
                }
                let Some(at_ticks) = quarter_beats_to_ticks(event.at) else {
                    return fail(
                        &format!("{event_path}.at"),
                        format!("{} must be finite and non-negative", event.at),
                    );
                };
                if at_ticks >= length_ticks {
                    return fail(
                        &format!("{event_path}.at"),
                        format!(
                            "{} quantizes outside clip length {}",
                            event.at, clip.length_beats
                        ),
                    );
                }
                if !(1..=127).contains(&event.velocity) {
                    return fail(
                        &format!("{event_path}.velocity"),
                        format!("{} out of range 1..=127", event.velocity),
                    );
                }
                let duration = match (clip.kind, event.duration) {
                    (ClipKind::Pitched, None) => {
                        return fail(
                            &format!("{event_path}.duration"),
                            "pitched events require `duration`".to_owned(),
                        );
                    }
                    (ClipKind::Percussion, None) => default_percussion_duration(),
                    (_, Some(duration)) => duration,
                };
                let Some(duration_ticks) = quarter_beats_to_ticks(duration) else {
                    return fail(
                        &format!("{event_path}.duration"),
                        format!("{duration} must be finite and non-negative"),
                    );
                };
                if duration_ticks == 0 {
                    return fail(
                        &format!("{event_path}.duration"),
                        format!("{duration} must quantize to at least 1 tick"),
                    );
                }
                if at_ticks.saturating_add(duration_ticks) > length_ticks {
                    return fail(
                        &format!("{event_path}.duration"),
                        "event crosses the clip boundary".to_owned(),
                    );
                }
                match clip.kind {
                    ClipKind::Pitched => {
                        if event.voice.is_some() {
                            return fail(
                                &format!("{event_path}.voice"),
                                "`voice` is only valid in percussion clips".to_owned(),
                            );
                        }
                        let pitch = event.pitch.as_deref().ok_or_else(|| Error::Validation {
                            path: format!("{event_path}.pitch"),
                            message: "pitched events require `pitch`".to_owned(),
                        })?;
                        let key =
                            parse_absolute_pitch(pitch).map_err(|message| Error::Validation {
                                path: format!("{event_path}.pitch"),
                                message,
                            })?;
                        pitched_spans.push((key, at_ticks, at_ticks + duration_ticks, event_id));
                    }
                    ClipKind::Percussion => {
                        if event.pitch.is_some() {
                            return fail(
                                &format!("{event_path}.pitch"),
                                "`pitch` is only valid in pitched clips".to_owned(),
                            );
                        }
                        let voice = event.voice.ok_or_else(|| Error::Validation {
                            path: format!("{event_path}.voice"),
                            message: "percussion events require `voice`".to_owned(),
                        })?;
                        if !percussion_onsets.insert((voice, at_ticks)) {
                            return fail(
                                &format!("{event_path}.at"),
                                format!("duplicate `{voice:?}` onset at quantized tick {at_ticks}"),
                            );
                        }
                    }
                }
            }
            pitched_spans.sort_unstable_by_key(|(key, start, end, _)| (*key, *start, *end));
            for pair in pitched_spans.windows(2) {
                let (left_key, _, left_end, _) = pair[0];
                let (right_key, right_start, _, right_id) = pair[1];
                if left_key == right_key && right_start < left_end {
                    return fail(
                        &format!("clips.{name}.events.{right_id}.at"),
                        format!("pitch {right_key} overlaps its preceding event"),
                    );
                }
            }
            if clip.automation.len() > 4 {
                return fail(
                    &format!("clips.{name}.automation"),
                    format!(
                        "{} automation lanes exceed the limit of 4",
                        clip.automation.len()
                    ),
                );
            }
            if clip.kind == ClipKind::Percussion && !clip.automation.is_empty() {
                return fail(
                    &format!("clips.{name}.automation"),
                    "automation is only valid in pitched clips".to_owned(),
                );
            }
            let mut targets = std::collections::BTreeSet::new();
            for (lane_id, lane) in &clip.automation {
                let lane_path = format!("clips.{name}.automation.{lane_id}");
                if !crate::texture::valid_logical_name(lane_id) {
                    return fail(
                        &lane_path,
                        format!(
                            "`{lane_id}` must match [a-z][a-z0-9_-]{{0,63}} \
                             (stable automation-lane identity)"
                        ),
                    );
                }
                if !targets.insert(lane.target) {
                    return fail(
                        &format!("{lane_path}.target"),
                        format!("duplicate automation target `{:?}`", lane.target),
                    );
                }
                if lane.points.is_empty() {
                    return fail(
                        &format!("{lane_path}.points"),
                        "automation lane must contain at least one point".to_owned(),
                    );
                }
                if lane.points.len() > 512 {
                    return fail(
                        &format!("{lane_path}.points"),
                        format!("{} points exceed the limit of 512", lane.points.len()),
                    );
                }
                let mut points = Vec::with_capacity(lane.points.len());
                let mut point_ticks = std::collections::BTreeSet::new();
                for (point_id, point) in &lane.points {
                    let point_path = format!("{lane_path}.points.{point_id}");
                    if !crate::texture::valid_logical_name(point_id) {
                        return fail(
                            &point_path,
                            format!(
                                "`{point_id}` must match [a-z][a-z0-9_-]{{0,63}} \
                                 (stable automation-point identity)"
                            ),
                        );
                    }
                    let Some(tick) = quarter_beats_to_ticks(point.at) else {
                        return fail(
                            &format!("{point_path}.at"),
                            format!("{} must be finite and non-negative", point.at),
                        );
                    };
                    if tick >= length_ticks {
                        return fail(
                            &format!("{point_path}.at"),
                            format!(
                                "{} quantizes outside clip length {}",
                                point.at, clip.length_beats
                            ),
                        );
                    }
                    if !point_ticks.insert(tick) {
                        return fail(
                            &format!("{point_path}.at"),
                            format!("another point quantizes to tick {tick}"),
                        );
                    }
                    let valid_value = match lane.target {
                        AutomationTarget::Cc1 | AutomationTarget::Cc11 | AutomationTarget::Cc74 => {
                            (0..=127).contains(&point.value)
                        }
                        AutomationTarget::PitchBend => (-8192..=8191).contains(&point.value),
                    };
                    if !valid_value {
                        let range = if lane.target == AutomationTarget::PitchBend {
                            "-8192..=8191"
                        } else {
                            "0..=127"
                        };
                        return fail(
                            &format!("{point_path}.value"),
                            format!("{} out of range {range}", point.value),
                        );
                    }
                    points.push((tick, point.value, point_id));
                }
                points.sort_unstable_by_key(|(tick, _, _)| *tick);
                if points[0].0 != 0 {
                    return fail(
                        &format!("{lane_path}.points.{}.at", points[0].2),
                        "automation lanes require an initial point at beat 0".to_owned(),
                    );
                }
                if clip.mode == ClipMode::Loop
                    && points.last().expect("points is non-empty").1 != points[0].1
                {
                    return fail(
                        &format!(
                            "{lane_path}.points.{}.value",
                            points.last().expect("points is non-empty").2
                        ),
                        "loop automation must end at its initial value".to_owned(),
                    );
                }
            }
        }
        if self.tracks.is_empty() {
            return fail("tracks", "at least one track is required".to_owned());
        }
        let melodic = self
            .tracks
            .iter()
            .filter(|t| !t.instrument.is_percussion())
            .count();
        let percussion = self.tracks.len() - melodic;
        if melodic > 15 {
            return fail(
                "tracks",
                format!("{melodic} melodic tracks exceed the 15-channel limit"),
            );
        }
        if percussion > 1 {
            return fail(
                "tracks",
                "at most one percussion track (`drums` or `tabla`) is supported".to_owned(),
            );
        }
        let mut track_ids = std::collections::BTreeSet::new();
        let scene_ticks =
            u32::from(self.bars) * u32::from(time_sig.num) * 480 * 4 / u32::from(time_sig.den);
        for (i, t) in self.tracks.iter().enumerate() {
            if !crate::texture::valid_logical_name(&t.id) {
                return fail(
                    &format!("tracks[{i}].id"),
                    format!(
                        "`{}` must match [a-z][a-z0-9_-]{{0,63}} (stable track identity)",
                        t.id
                    ),
                );
            }
            if !track_ids.insert(t.id.as_str()) {
                return fail(
                    &format!("tracks[{i}].id"),
                    format!("duplicate track id `{}`", t.id),
                );
            }
            if let Some(palette) = &t.palette
                && !crate::texture::valid_logical_name(palette)
            {
                return fail(
                    &format!("tracks[{i}].palette"),
                    format!(
                        "`{palette}` must match [a-z][a-z0-9_-]{{0,63}} (logical palette name)"
                    ),
                );
            }
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
            match t.instrument {
                Instrument::Drums if !matches!(t.pattern, Pattern::Drums | Pattern::Clip) => {
                    return fail(
                        &format!("tracks[{i}].pattern"),
                        "instrument `drums` requires pattern `drums` or `clip`".to_owned(),
                    );
                }
                Instrument::Tabla if t.pattern != Pattern::Tabla => {
                    return fail(
                        &format!("tracks[{i}].pattern"),
                        "instrument `tabla` requires pattern `tabla`".to_owned(),
                    );
                }
                _ => {}
            }
            if t.pattern == Pattern::Drums && t.instrument != Instrument::Drums {
                return fail(
                    &format!("tracks[{i}].pattern"),
                    "pattern `drums` requires instrument `drums`".to_owned(),
                );
            }
            if t.pattern == Pattern::Tabla && t.instrument != Instrument::Tabla {
                return fail(
                    &format!("tracks[{i}].pattern"),
                    "pattern `tabla` requires instrument `tabla`".to_owned(),
                );
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
            match (t.pattern == Pattern::Clip, &t.clip) {
                (true, None) => {
                    return fail(
                        &format!("tracks[{i}].clip"),
                        "pattern `clip` requires a `clip` name".to_owned(),
                    );
                }
                (true, Some(name)) => {
                    let clip = self.clips.get(name).ok_or_else(|| Error::Validation {
                        path: format!("tracks[{i}].clip"),
                        message: format!(
                            "unknown clip `{name}` (defined: {:?})",
                            self.clips.keys().collect::<Vec<_>>()
                        ),
                    })?;
                    let expected = if t.instrument == Instrument::Drums {
                        ClipKind::Percussion
                    } else {
                        ClipKind::Pitched
                    };
                    if clip.kind != expected {
                        return fail(
                            &format!("tracks[{i}].clip"),
                            format!(
                                "instrument `{}` requires a {expected:?} clip, but `{name}` is {:?}",
                                instrument_key(t.instrument),
                                clip.kind
                            ),
                        );
                    }
                    if clip.mode == ClipMode::Loop {
                        let clip_ticks =
                            quarter_beats_to_ticks(clip.length_beats).expect("clip is validated");
                        if scene_ticks % clip_ticks != 0 {
                            return fail(
                                &format!("tracks[{i}].clip"),
                                format!(
                                    "loop clip `{name}` ({clip_ticks} ticks) does not divide \
                                     scene length ({scene_ticks} ticks)"
                                ),
                            );
                        }
                    }
                    let expanded = expanded_clip_event_count(clip, scene_ticks);
                    if expanded > MAX_EXPANDED_CLIP_EVENTS_PER_TRACK {
                        return fail(
                            &format!("tracks[{i}].clip"),
                            format!(
                                "clip `{name}` expands to {expanded} note/control events, \
                                 exceeding the per-track expanded event limit of \
                                 {MAX_EXPANDED_CLIP_EVENTS_PER_TRACK}"
                            ),
                        );
                    }
                }
                (false, Some(_)) => {
                    return fail(
                        &format!("tracks[{i}].clip"),
                        "`clip` is only valid with pattern `clip`".to_owned(),
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
            let muted: std::collections::BTreeSet<&str> =
                s.mute.iter().map(String::as_str).collect();
            for (j, m) in s.mute.iter().enumerate() {
                if !track_ids.contains(m.as_str()) {
                    return fail(
                        &format!("sections[{i}].mute[{j}]"),
                        format!(
                            "unknown track id `{m}` (defined: {:?})",
                            self.tracks.iter().map(|t| &t.id).collect::<Vec<_>>()
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
            for (track_id, clip_id) in &s.clips {
                let Some(track) = self.tracks.iter().find(|track| track.id == *track_id) else {
                    return fail(
                        &format!("sections[{i}].clips.{track_id}"),
                        format!("unknown track id `{track_id}`"),
                    );
                };
                if track.pattern != Pattern::Clip {
                    return fail(
                        &format!("sections[{i}].clips.{track_id}"),
                        "section clip overrides require a track with pattern `clip`".to_owned(),
                    );
                }
                let clip = self.clips.get(clip_id).ok_or_else(|| Error::Validation {
                    path: format!("sections[{i}].clips.{track_id}"),
                    message: format!("unknown clip `{clip_id}`"),
                })?;
                let expected = if track.instrument == Instrument::Drums {
                    ClipKind::Percussion
                } else {
                    ClipKind::Pitched
                };
                if clip.kind != expected {
                    return fail(
                        &format!("sections[{i}].clips.{track_id}"),
                        format!("track `{track_id}` requires a {expected:?} clip"),
                    );
                }
            }
            let section_ticks =
                u32::from(s.bars) * u32::from(time_sig.num) * 480 * 4 / u32::from(time_sig.den);
            for track in self.tracks.iter().filter(|track| {
                track.pattern == Pattern::Clip && !muted.contains(track.id.as_str())
            }) {
                let clip_id = s
                    .clips
                    .get(&track.id)
                    .or(track.clip.as_ref())
                    .expect("clip tracks are validated");
                let clip = &self.clips[clip_id];
                if clip.mode == ClipMode::Loop {
                    let clip_ticks =
                        quarter_beats_to_ticks(clip.length_beats).expect("clip is validated");
                    if section_ticks % clip_ticks != 0 {
                        return fail(
                            &format!("sections[{i}].clips.{}", track.id),
                            format!(
                                "loop clip `{clip_id}` ({clip_ticks} ticks) does not divide \
                                 section length ({section_ticks} ticks)"
                            ),
                        );
                    }
                }
                let expanded = expanded_clip_event_count(clip, section_ticks);
                if expanded > MAX_EXPANDED_CLIP_EVENTS_PER_TRACK {
                    return fail(
                        &format!("sections[{i}].clips.{}", track.id),
                        format!(
                            "clip `{clip_id}` expands to {expanded} note/control events, \
                             exceeding the per-track expanded event limit of \
                             {MAX_EXPANDED_CLIP_EVENTS_PER_TRACK}"
                        ),
                    );
                }
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
            .filter(|t| !section.mute.contains(&t.id))
            .map(|t| {
                let mut t = t.clone();
                if let Some(clip) = section.clips.get(&t.id) {
                    t.clip = Some(clip.clone());
                }
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
