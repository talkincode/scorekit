//! Score IR → MakeCode song assets (`music.createSong(hex`…`)`).
//!
//! The byte layout is pxt's song encoding **version 0**, defined by
//! `encodeSong`/`decodeSong` in `microsoft/pxt` `pxtlib/music.ts` (MIT) and
//! played back by the MakeCode `music.sequencer` runtime (micro:bit V2,
//! Arcade). scorekit only *serializes* synthesizer parameters into that
//! layout — every sound is synthesized by the MakeCode runtime, so this
//! module is pure byte assembly, not DSP.
//!
//! Pitch model: the runtime's frequency table starts at B0 (≈31 Hz, MIDI 23)
//! and ends at B8 (≈7902 Hz, MIDI 119), so a MakeCode note number is
//! `midi_key - 23`. A note byte stores the low six bits of
//! `note - (octave - 2) * 12`, so each track's pitch window spans 64
//! semitones positioned by its instrument `octave` field, which this encoder
//! solves per track from the compiled note range.
//!
//! Unrepresentable scene features fail with a structured validation error
//! (pitch bends / CC automation / textures) or are dropped with an explicit
//! warning recorded in the build manifest (`pan` / `reverb`).

use crate::composer::{NoteEvent, ScoreIr};
use crate::error::{Error, Result};
use crate::schema::{Instrument, Scene, instrument_key};

/// MIDI key of MakeCode note 0 (B0, first entry of the runtime frequency table).
const MIDI_BASE: u8 = 23;
/// Highest MIDI key in the runtime frequency table (B8, table index 96).
const MIDI_MAX: u8 = 119;
/// Size of one encoded melodic instrument block.
const INSTRUMENT_BYTES: usize = 28;

// ---------------------------------------------------------------------------
// Raw song model — a byte-faithful mirror of the version-0 encoding.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Envelope {
    pub attack: u16,
    pub decay: u16,
    pub sustain: u16,
    pub release: u16,
    pub amplitude: u16,
}

const SILENT_ENVELOPE: Envelope = Envelope {
    attack: 0,
    decay: 0,
    sustain: 0,
    release: 0,
    amplitude: 0,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lfo {
    pub frequency: u8,
    pub amplitude: u16,
}

const NO_LFO: Lfo = Lfo {
    frequency: 0,
    amplitude: 0,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MelodicInstrument {
    pub waveform: u8,
    pub amp: Envelope,
    pub pitch: Envelope,
    pub amp_lfo: Lfo,
    pub pitch_lfo: Lfo,
    pub octave: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrumStep {
    pub waveform: u8,
    pub frequency: u16,
    pub volume: u16,
    pub duration: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrumInstrument {
    pub start_frequency: u16,
    pub start_volume: u16,
    pub steps: Vec<DrumStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEvent {
    pub start_tick: u16,
    pub end_tick: u16,
    /// Encoded note bytes (melodic: windowed pitch + enharmonic flags;
    /// drums: index into the track's drum list).
    pub notes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawTrackKind {
    Melodic(MelodicInstrument),
    Drums(Vec<DrumInstrument>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawTrack {
    pub id: u8,
    pub kind: RawTrackKind,
    pub events: Vec<RawEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSong {
    pub bpm: u16,
    pub beats_per_measure: u8,
    pub ticks_per_beat: u8,
    pub measures: u8,
    pub tracks: Vec<RawTrack>,
    /// Optional per-note-event velocity blocks: (track id, one byte per event).
    pub velocities: Vec<(u8, Vec<u8>)>,
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn encode_instrument(out: &mut Vec<u8>, i: &MelodicInstrument) {
    out.push(i.waveform);
    for env in [&i.amp, &i.pitch] {
        push_u16(out, env.attack);
        push_u16(out, env.decay);
        push_u16(out, env.sustain);
        push_u16(out, env.release);
        push_u16(out, env.amplitude);
    }
    out.push(i.amp_lfo.frequency);
    push_u16(out, i.amp_lfo.amplitude);
    out.push(i.pitch_lfo.frequency);
    push_u16(out, i.pitch_lfo.amplitude);
    out.push(i.octave);
}

fn encode_drum(out: &mut Vec<u8>, d: &DrumInstrument) {
    out.push(d.steps.len() as u8);
    push_u16(out, d.start_frequency);
    push_u16(out, d.start_volume);
    for step in &d.steps {
        out.push(step.waveform);
        push_u16(out, step.frequency);
        push_u16(out, step.volume);
        push_u16(out, step.duration);
    }
}

/// Serialize to the version-0 byte encoding. Pure function of the model;
/// identical input yields identical bytes.
pub fn encode_raw(song: &RawSong) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(0); // encoding version
    push_u16(&mut out, song.bpm);
    out.push(song.beats_per_measure);
    out.push(song.ticks_per_beat);
    out.push(song.measures);
    out.push(song.tracks.len() as u8);
    for track in &song.tracks {
        out.push(track.id);
        match &track.kind {
            RawTrackKind::Melodic(instrument) => {
                out.push(0);
                push_u16(&mut out, INSTRUMENT_BYTES as u16);
                encode_instrument(&mut out, instrument);
            }
            RawTrackKind::Drums(drums) => {
                out.push(1);
                let len: usize = drums.iter().map(|d| 5 + 7 * d.steps.len()).sum();
                push_u16(&mut out, len as u16);
                for drum in drums {
                    encode_drum(&mut out, drum);
                }
            }
        }
        let note_len: usize = track.events.iter().map(|e| 5 + e.notes.len()).sum();
        push_u16(&mut out, note_len as u16);
        for event in &track.events {
            push_u16(&mut out, event.start_tick);
            push_u16(&mut out, event.end_tick);
            out.push(event.notes.len() as u8);
            out.extend_from_slice(&event.notes);
        }
    }
    for (id, velocities) in &song.velocities {
        out.push(*id);
        out.extend_from_slice(velocities);
    }
    out
}

pub fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

// ---------------------------------------------------------------------------
// Decoder — used by tests to prove interoperability with pxt's encoder and
// by nothing at runtime. Kept next to the encoder so the two cannot drift.
// ---------------------------------------------------------------------------

#[cfg(test)]
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

#[cfg(test)]
impl<'a> Reader<'a> {
    fn u8(&mut self) -> std::result::Result<u8, String> {
        let v = *self
            .buf
            .get(self.pos)
            .ok_or_else(|| format!("truncated song buffer at byte {}", self.pos))?;
        self.pos += 1;
        Ok(v)
    }

    fn u16(&mut self) -> std::result::Result<u16, String> {
        let lo = self.u8()?;
        let hi = self.u8()?;
        Ok(u16::from_le_bytes([lo, hi]))
    }
}

/// Parse version-0 song bytes back into the raw model. Returns a plain
/// `String` error: this is a test/verification helper, not a CLI surface.
#[cfg(test)]
pub fn decode_raw(bytes: &[u8]) -> std::result::Result<RawSong, String> {
    let mut r = Reader { buf: bytes, pos: 0 };
    let version = r.u8()?;
    if version != 0 {
        return Err(format!("unsupported song encoding version {version}"));
    }
    let bpm = r.u16()?;
    let beats_per_measure = r.u8()?;
    let ticks_per_beat = r.u8()?;
    let measures = r.u8()?;
    let track_count = r.u8()?;
    let mut tracks = Vec::with_capacity(usize::from(track_count));
    for _ in 0..track_count {
        let id = r.u8()?;
        let flags = r.u8()?;
        let block_len = usize::from(r.u16()?);
        let kind = if flags & 1 == 1 {
            let end = r.pos + block_len;
            let mut drums = Vec::new();
            while r.pos < end {
                let steps = usize::from(r.u8()?);
                let start_frequency = r.u16()?;
                let start_volume = r.u16()?;
                let mut list = Vec::with_capacity(steps);
                for _ in 0..steps {
                    list.push(DrumStep {
                        waveform: r.u8()?,
                        frequency: r.u16()?,
                        volume: r.u16()?,
                        duration: r.u16()?,
                    });
                }
                drums.push(DrumInstrument {
                    start_frequency,
                    start_volume,
                    steps: list,
                });
            }
            if r.pos != end {
                return Err("drum block length does not match its contents".to_owned());
            }
            RawTrackKind::Drums(drums)
        } else {
            if block_len != INSTRUMENT_BYTES {
                return Err(format!(
                    "melodic instrument block is {block_len} bytes, expected {INSTRUMENT_BYTES}"
                ));
            }
            let waveform = r.u8()?;
            let mut envs = [SILENT_ENVELOPE; 2];
            for env in &mut envs {
                *env = Envelope {
                    attack: r.u16()?,
                    decay: r.u16()?,
                    sustain: r.u16()?,
                    release: r.u16()?,
                    amplitude: r.u16()?,
                };
            }
            let amp_lfo = Lfo {
                frequency: r.u8()?,
                amplitude: r.u16()?,
            };
            let pitch_lfo = Lfo {
                frequency: r.u8()?,
                amplitude: r.u16()?,
            };
            let octave = r.u8()?;
            RawTrackKind::Melodic(MelodicInstrument {
                waveform,
                amp: envs[0],
                pitch: envs[1],
                amp_lfo,
                pitch_lfo,
                octave,
            })
        };
        let note_len = usize::from(r.u16()?);
        let end = r.pos + note_len;
        let mut events = Vec::new();
        while r.pos < end {
            let start_tick = r.u16()?;
            let end_tick = r.u16()?;
            let polyphony = usize::from(r.u8()?);
            let mut notes = Vec::with_capacity(polyphony);
            for _ in 0..polyphony {
                notes.push(r.u8()?);
            }
            events.push(RawEvent {
                start_tick,
                end_tick,
                notes,
            });
        }
        if r.pos != end {
            return Err("note block length does not match its contents".to_owned());
        }
        tracks.push(RawTrack { id, kind, events });
    }
    let mut velocities = Vec::new();
    while r.pos < bytes.len() {
        let id = r.u8()?;
        let track = tracks
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| format!("velocity block references unknown track id {id}"))?;
        let mut vels = Vec::with_capacity(track.events.len());
        for _ in 0..track.events.len() {
            vels.push(r.u8()?);
        }
        velocities.push((id, vels));
    }
    Ok(RawSong {
        bpm,
        beats_per_measure,
        ticks_per_beat,
        measures,
        tracks,
        velocities,
    })
}

// ---------------------------------------------------------------------------
// Chip identities — the MakeCode song editor's built-in synth presets
// (`getEmptySong` in `microsoft/pxt` `pxtlib/music.ts`, MIT). These are the
// target platform's own curated voices; scorekit maps its portable
// instrument vocabulary onto them and reports the choice per track.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Chip {
    Dog,
    Duck,
    Cat,
    Fish,
    Car,
    Computer,
    Burger,
    Cherry,
    Lemon,
}

impl Chip {
    fn name(self) -> &'static str {
        match self {
            Chip::Dog => "dog",
            Chip::Duck => "duck",
            Chip::Cat => "cat",
            Chip::Fish => "fish",
            Chip::Car => "car",
            Chip::Computer => "computer",
            Chip::Burger => "burger",
            Chip::Cherry => "cherry",
            Chip::Lemon => "lemon",
        }
    }

    /// Editor preset parameters; `octave` is a placeholder the encoder
    /// re-solves per track from the compiled note range.
    fn params(self) -> MelodicInstrument {
        let env = |attack, decay, sustain, release| Envelope {
            attack,
            decay,
            sustain,
            release,
            amplitude: 1024,
        };
        let penv = |attack, decay, sustain, release, amplitude| Envelope {
            attack,
            decay,
            sustain,
            release,
            amplitude,
        };
        let lfo = |frequency, amplitude| Lfo {
            frequency,
            amplitude,
        };
        let base = MelodicInstrument {
            waveform: 0,
            amp: SILENT_ENVELOPE,
            pitch: SILENT_ENVELOPE,
            amp_lfo: NO_LFO,
            pitch_lfo: NO_LFO,
            octave: 4,
        };
        match self {
            Chip::Dog => MelodicInstrument {
                waveform: 1,
                amp: env(10, 100, 500, 100),
                pitch_lfo: lfo(5, 0),
                ..base
            },
            Chip::Duck => MelodicInstrument {
                waveform: 15,
                amp: env(5, 530, 705, 450),
                pitch: penv(5, 40, 0, 100, 40),
                amp_lfo: lfo(3, 20),
                pitch_lfo: lfo(6, 2),
                ..base
            },
            Chip::Cat => MelodicInstrument {
                waveform: 12,
                amp: env(150, 100, 365, 400),
                pitch: penv(120, 300, 0, 100, 50),
                pitch_lfo: lfo(10, 6),
                ..base
            },
            Chip::Fish => MelodicInstrument {
                waveform: 1,
                amp: env(220, 105, 1024, 350),
                amp_lfo: lfo(5, 100),
                pitch_lfo: lfo(1, 4),
                ..base
            },
            Chip::Car => MelodicInstrument {
                waveform: 16,
                amp: env(5, 100, 1024, 30),
                pitch_lfo: lfo(10, 4),
                ..base
            },
            Chip::Computer => MelodicInstrument {
                waveform: 15,
                amp: env(10, 100, 500, 10),
                ..base
            },
            Chip::Burger => MelodicInstrument {
                waveform: 1,
                amp: env(10, 100, 500, 100),
                ..base
            },
            Chip::Cherry => MelodicInstrument {
                waveform: 2,
                amp: env(10, 100, 500, 100),
                ..base
            },
            Chip::Lemon => MelodicInstrument {
                waveform: 14,
                amp: env(5, 70, 870, 50),
                pitch: penv(10, 45, 0, 100, 20),
                amp_lfo: lfo(1, 50),
                pitch_lfo: lfo(2, 1),
                ..base
            },
        }
    }
}

/// Curated identity mapping: portable instrument vocabulary → the closest of
/// the nine MakeCode chip voices. Percussion identities return `None` and
/// route through the drum-kit mapping instead. Every choice is reported in
/// the build manifest, so a substitution is always visible, never silent.
fn chip_for(instrument: Instrument) -> Option<Chip> {
    use Instrument::*;
    Some(match instrument {
        // Plain triangle, balanced envelope: keyboards and pure-tone winds.
        Piano | BrightPiano | Epiano | Celesta | Recorder | Whistle | Ocarina => Chip::Dog,
        // Bright square with pitch attack: brass and square leads.
        Trumpet | Trombone | Horn | Brass | SynthBrass | Sax | SquareLead => Chip::Duck,
        // Soft square, slow attack, vibrato: bowed strings and expressive winds.
        Flute | Piccolo | PanFlute | Oboe | EnglishHorn | Clarinet | Bassoon | Violin | Viola
        | Erhu | Dizi | Ney | Duduk | Shakuhachi => Chip::Cat,
        // Full-sustain triangle with slow tremolo: sections, pads and voices.
        Strings | SlowStrings | SynthStrings | TremoloStrings | Cello | Contrabass | Choir
        | Voice | Pad | WarmPad | ChoirPad | BowedPad | HaloPad | SweepPad => Chip::Fish,
        // Instant attack, full sustain: organ-family sustain.
        Organ | Accordion => Chip::Car,
        // Short low square: bass family.
        Bass | PickedBass | FretlessBass | SlapBass | SynthBass => Chip::Computer,
        // Plain low triangle: dark, round low voices.
        Tuba | Timpani => Chip::Burger,
        // Sawtooth: bright, buzzy sustain.
        SawLead | Harpsichord | Clavinet => Chip::Cherry,
        // Plucked square with a pitch blip: plucked strings and mallets.
        Guitar | SteelGuitar | ElectricGuitar | MutedGuitar | Harp | Pizzicato | MusicBox
        | Glockenspiel | Vibraphone | Marimba | Xylophone | TubularBells | Pipa | Guzheng
        | Shamisen | Sitar | Oud => Chip::Lemon,
        Drums | Tabla => return None,
    })
}

/// The MakeCode song editor's built-in drum synth definitions
/// (`getEmptySong` in `microsoft/pxt` `pxtlib/music.ts`, MIT).
struct DrumPreset {
    name: &'static str,
    start_frequency: u16,
    start_volume: u16,
    steps: &'static [DrumStep],
}

macro_rules! steps {
    ($(($w:expr, $f:expr, $v:expr, $d:expr)),* $(,)?) => {
        &[$(DrumStep { waveform: $w, frequency: $f, volume: $v, duration: $d }),*]
    };
}

const NEUTRAL_KICK: DrumPreset = DrumPreset {
    name: "neutral kick",
    start_frequency: 100,
    start_volume: 1024,
    steps: steps![(3, 120, 1024, 10), (3, 1, 0, 100)],
};
const THUMP_1: DrumPreset = DrumPreset {
    name: "thump 1",
    start_frequency: 200,
    start_volume: 1024,
    steps: steps![(4, 200, 15, 100), (4, 150, 0, 200)],
};
const THUMP_2: DrumPreset = DrumPreset {
    name: "thump 2",
    start_frequency: 450,
    start_volume: 1024,
    steps: steps![(4, 350, 15, 100), (4, 300, 0, 100)],
};
const SNARE_1: DrumPreset = DrumPreset {
    name: "snare 1",
    start_frequency: 175,
    start_volume: 1024,
    steps: steps![
        (1, 200, 1024, 10),
        (1, 150, 1024, 20),
        (5, 1, 100, 20),
        (5, 1, 0, 300),
    ],
};
const SNARE_2: DrumPreset = DrumPreset {
    name: "snare 2",
    start_frequency: 220,
    start_volume: 1024,
    steps: steps![
        (1, 250, 1024, 10),
        (1, 200, 1024, 20),
        (5, 2000, 100, 20),
        (5, 2000, 0, 200),
    ],
};
const HAT_1: DrumPreset = DrumPreset {
    name: "hat 1",
    start_frequency: 400,
    start_volume: 500,
    steps: steps![(5, 450, 500, 10), (5, 400, 20, 20)],
};
const HAT_2: DrumPreset = DrumPreset {
    name: "hat 2",
    start_frequency: 400,
    start_volume: 0,
    steps: steps![(5, 450, 500, 5), (5, 900, 5, 50), (5, 900, 0, 250)],
};
const HAT_4: DrumPreset = DrumPreset {
    name: "hat 4",
    start_frequency: 400,
    start_volume: 0,
    steps: steps![
        (5, 450, 500, 5),
        (5, 900, 200, 100),
        (5, 900, 5, 200),
        (5, 900, 0, 500),
    ],
};
const DOUBLE_HAT: DrumPreset = DrumPreset {
    name: "double hat",
    start_frequency: 3500,
    start_volume: 1024,
    steps: steps![
        (4, 4000, 0, 10),
        (4, 3500, 800, 1),
        (4, 4000, 0, 40),
        (4, 3500, 400, 1),
        (4, 4000, 0, 40),
    ],
};
const METALLIC: DrumPreset = DrumPreset {
    name: "metallic",
    start_frequency: 2000,
    start_volume: 1024,
    steps: steps![(4, 1800, 15, 100), (4, 1800, 0, 200)],
};
const LOW_TOM: DrumPreset = DrumPreset {
    name: "low tom",
    start_frequency: 200,
    start_volume: 200,
    steps: steps![(14, 125, 200, 25), (14, 100, 15, 50), (14, 120, 0, 250)],
};
const MID_TOM: DrumPreset = DrumPreset {
    name: "mid tom",
    start_frequency: 300,
    start_volume: 200,
    steps: steps![(14, 225, 200, 25), (14, 200, 15, 50), (14, 220, 0, 250)],
};
const HI_TOM: DrumPreset = DrumPreset {
    name: "hi tom",
    start_frequency: 500,
    start_volume: 200,
    steps: steps![(14, 425, 200, 25), (14, 400, 15, 50), (14, 420, 0, 250)],
};
const LO_TOM_2: DrumPreset = DrumPreset {
    name: "lo tom 2",
    start_frequency: 200,
    start_volume: 1024,
    steps: steps![(1, 75, 0, 200)],
};
const MID_TOM_2: DrumPreset = DrumPreset {
    name: "mid tom 2",
    start_frequency: 300,
    start_volume: 1024,
    steps: steps![(1, 200, 0, 200)],
};
const HI_TOM_2: DrumPreset = DrumPreset {
    name: "hi tom 2",
    start_frequency: 400,
    start_volume: 1024,
    steps: steps![(1, 300, 0, 200)],
};
const CYMBAL: DrumPreset = DrumPreset {
    name: "cymbal",
    start_frequency: 2500,
    start_volume: 1024,
    steps: steps![(4, 2500, 100, 150), (4, 2550, 0, 500)],
};
const CRASH_1: DrumPreset = DrumPreset {
    name: "crash 1",
    start_frequency: 3000,
    start_volume: 1024,
    steps: steps![(4, 3000, 100, 300), (4, 3060, 0, 500)],
};
const BUZZER: DrumPreset = DrumPreset {
    name: "buzzer",
    start_frequency: 2000,
    start_volume: 1024,
    steps: steps![(16, 2000, 100, 150), (16, 2000, 0, 200)],
};

/// Curated GM-percussion-key → MakeCode drum voice mapping. Covers every key
/// the composer and the percussion clip vocabulary can emit; an unknown key
/// is a structured error, never a silent substitution.
fn drum_for(gm_key: u8) -> Option<&'static DrumPreset> {
    Some(match gm_key {
        36 => &NEUTRAL_KICK, // kick / tabla dha
        37 => &THUMP_1,      // tabla dhin (GM side stick)
        38 => &SNARE_1,      // snare / tabla tin
        39 => &SNARE_2,      // clap / tabla ta
        42 => &HAT_1,        // closed hat
        44 => &HAT_2,        // pedal hat
        45 => &LOW_TOM,      // low tom
        46 => &HAT_4,        // open hat
        47 => &MID_TOM,      // mid tom
        49 => &CRASH_1,      // crash
        50 => &HI_TOM,       // high tom
        51 => &CYMBAL,       // ride
        54 => &DOUBLE_HAT,   // tambourine
        56 => &METALLIC,     // cowbell
        60 => &HI_TOM_2,     // high bongo
        61 => &MID_TOM_2,    // low bongo
        62 => &THUMP_2,      // mute high conga
        63 => &HI_TOM_2,     // open high conga
        64 => &LO_TOM_2,     // low conga
        65 => &HI_TOM,       // high timbale
        66 => &LOW_TOM,      // low timbale
        67 => &METALLIC,     // high agogo
        68 => &BUZZER,       // low agogo
        69 => &HAT_2,        // cabasa
        70 => &HAT_1,        // maracas
        _ => return None,
    })
}

impl From<&DrumPreset> for DrumInstrument {
    fn from(preset: &DrumPreset) -> Self {
        DrumInstrument {
            start_frequency: preset.start_frequency,
            start_volume: preset.start_volume,
            steps: preset.steps.to_vec(),
        }
    }
}

// ---------------------------------------------------------------------------
// Compilation: Score IR → RawSong + per-track report.
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct TrackReport {
    pub scene_track_id: String,
    pub instrument: String,
    pub makecode_track: u8,
    /// "melodic" or "drums".
    pub kind: &'static str,
    pub chip_preset: Option<&'static str>,
    pub octave: Option<u8>,
    /// GM key → MakeCode drum voice name, in drum-list order.
    pub voices: Vec<(u8, &'static str)>,
    pub note_events: usize,
}

#[derive(Debug)]
pub struct EncodedSong {
    pub bytes: Vec<u8>,
    pub bpm: u16,
    pub beats_per_measure: u8,
    pub ticks_per_beat: u8,
    pub measures: u8,
    pub tracks: Vec<TrackReport>,
    pub warnings: Vec<String>,
}

impl EncodedSong {
    pub fn hex(&self) -> String {
        to_hex(&self.bytes)
    }
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// MakeCode Arcade / micro:bit speakers clip hard when several full-scale
/// chip voices stack. The song editor's own 4-track songs sit around
/// 220–384 / 1024 per voice; we budget ~1.2 full-scale across the mix and
/// then apply the scene track's intensity.
fn mix_amplitude(preset: u16, intensity: f32, active_tracks: usize) -> u16 {
    let n = active_tracks.max(1) as f32;
    let budget = 1200.0 / n;
    let scaled = f32::from(preset) / 1024.0 * budget * intensity.clamp(0.05, 1.0);
    scaled.round().clamp(48.0, 1024.0) as u16
}

fn scale_instrument(instrument: &mut MelodicInstrument, intensity: f32, active_tracks: usize) {
    instrument.amp.amplitude = mix_amplitude(instrument.amp.amplitude, intensity, active_tracks);
}

fn scale_drum(drum: &mut DrumInstrument, intensity: f32, active_tracks: usize) {
    let gain = f32::from(mix_amplitude(1024, intensity, active_tracks)) / 1024.0;
    let scale = |v: u16| ((f32::from(v) * gain).round() as u16).clamp(0, 1024);
    drum.start_volume = scale(drum.start_volume);
    for step in &mut drum.steps {
        step.volume = scale(step.volume);
    }
}

fn validation(path: impl Into<String>, message: impl Into<String>) -> Error {
    Error::Validation {
        path: path.into(),
        message: message.into(),
    }
}

/// Notes sorted and deduplicated on (tick, key): simultaneous duplicates keep
/// the longest duration and loudest velocity so the merge is deterministic.
fn normalized_notes(notes: &[NoteEvent]) -> Vec<NoteEvent> {
    let mut sorted: Vec<NoteEvent> = notes.to_vec();
    sorted.sort_by_key(|n| {
        (
            n.tick,
            n.key,
            std::cmp::Reverse(n.dur),
            std::cmp::Reverse(n.vel),
        )
    });
    sorted.dedup_by(|next, kept| {
        if next.tick == kept.tick && next.key == kept.key {
            kept.dur = kept.dur.max(next.dur);
            kept.vel = kept.vel.max(next.vel);
            true
        } else {
            false
        }
    });
    sorted
}

/// Compile one composed scene into MakeCode song bytes.
pub fn encode_song(scene: &Scene, ir: &ScoreIr) -> Result<EncodedSong> {
    // 1. Reject features the format has no representation for.
    for (index, (track, spec)) in ir.tracks.iter().zip(&scene.tracks).enumerate() {
        if !track.bends.is_empty() {
            let source = if spec.glide.is_some_and(|g| g > 0.0) {
                "`glide`"
            } else {
                "clip `pitch_bend` automation"
            };
            return Err(validation(
                format!("tracks[{index}]"),
                format!(
                    "track `{}` uses {source}; the MakeCode song format has no pitch bends. \
                     Remove it or target `scorekit build` instead",
                    spec.id
                ),
            ));
        }
        if !track.controls.is_empty() {
            return Err(validation(
                format!("tracks[{index}]"),
                format!(
                    "track `{}` uses clip CC automation lanes; the MakeCode song format has no \
                     controllers. Remove the automation or target `scorekit build` instead",
                    spec.id
                ),
            ));
        }
    }

    // 2. Header fields, with explicit format-limit checks.
    let beat_ticks = crate::composer::PPQ * 4 / u32::from(ir.ts.den);
    let bar_ticks = beat_ticks * u32::from(ir.ts.num);
    let bpm_x4 = u32::from(ir.tempo) * u32::from(ir.ts.den);
    if !bpm_x4.is_multiple_of(4) {
        return Err(validation(
            "tempo",
            format!(
                "tempo {} in {}/{} time maps to a fractional MakeCode beats-per-minute; \
                 use a tempo where tempo*{}/4 is an integer",
                ir.tempo, ir.ts.num, ir.ts.den, ir.ts.den
            ),
        ));
    }
    let bpm = bpm_x4 / 4;
    let bpm: u16 = bpm.try_into().map_err(|_| {
        validation(
            "tempo",
            format!("MakeCode beats-per-minute {bpm} exceeds 65535"),
        )
    })?;
    let measures = ir.total_ticks / bar_ticks;
    let measures: u8 = measures.try_into().map_err(|_| {
        validation(
            "bars",
            format!("{measures} measures exceed the MakeCode song format cap of 255"),
        )
    })?;

    // 3. Solve the coarsest tick grid that represents every onset exactly
    //    (and every melodic duration; drum voices are one-shots, so their
    //    IR durations do not constrain the grid).
    let mut step = beat_ticks;
    for (track, spec) in ir.tracks.iter().zip(&scene.tracks) {
        let percussion = spec.instrument.is_percussion();
        for n in &track.notes {
            step = gcd(step, n.tick);
            if !percussion {
                step = gcd(step, n.dur);
            }
        }
    }
    if step == 0 {
        step = beat_ticks;
    }
    let ticks_per_beat = beat_ticks / step;
    let ticks_per_beat: u8 = ticks_per_beat.try_into().map_err(|_| {
        validation(
            "tracks",
            format!(
                "note timing requires {ticks_per_beat} ticks per beat; the MakeCode song format \
                 caps at 255. Quantize the material (e.g. remove `performance.humanize`) and retry"
            ),
        )
    })?;
    let total_mc_ticks = ir.total_ticks / step;
    if total_mc_ticks > u32::from(u16::MAX) {
        return Err(validation(
            "bars",
            format!("song spans {total_mc_ticks} MakeCode ticks, exceeding the u16 tick range"),
        ));
    }

    // 4. Per-track events.
    let active_tracks = ir
        .tracks
        .iter()
        .filter(|track| !track.notes.is_empty())
        .count();
    let mut warnings = Vec::new();
    let mut tracks = Vec::new();
    let mut reports = Vec::new();
    let mut velocities = Vec::new();
    for (index, (track, spec)) in ir.tracks.iter().zip(&scene.tracks).enumerate() {
        for (field, set) in [
            ("pan", spec.pan.is_some()),
            ("reverb", spec.reverb.is_some()),
        ] {
            if set {
                warnings.push(format!(
                    "tracks[{index}] (`{}`): `{field}` has no MakeCode representation and was \
                     dropped from this target",
                    spec.id
                ));
            }
        }
        if track.notes.is_empty() {
            warnings.push(format!(
                "tracks[{index}] (`{}`): compiled to no notes and was omitted",
                spec.id
            ));
            continue;
        }
        let notes = normalized_notes(&track.notes);
        if notes.len() != track.notes.len() {
            warnings.push(format!(
                "tracks[{index}] (`{}`): {} duplicate same-tick note(s) merged",
                spec.id,
                track.notes.len() - notes.len()
            ));
        }
        let id = tracks.len() as u8;
        let percussion = spec.instrument.is_percussion();
        let (kind, report) = if percussion {
            build_drum_track(&notes, spec, index, active_tracks)?
        } else {
            build_melodic_track(&notes, spec, index, active_tracks)?
        };
        let (events, event_velocities) = build_events(&notes, percussion, step, &kind);
        velocities.push((id, event_velocities));
        reports.push(TrackReport {
            makecode_track: id,
            note_events: events.len(),
            ..report
        });
        tracks.push(RawTrack { id, kind, events });
    }
    if tracks.is_empty() {
        warnings.push("scene compiled to an empty song (no notes on any track)".to_owned());
    }

    let raw = RawSong {
        bpm,
        beats_per_measure: ir.ts.num,
        ticks_per_beat,
        measures,
        tracks,
        velocities,
    };
    Ok(EncodedSong {
        bytes: encode_raw(&raw),
        bpm,
        beats_per_measure: ir.ts.num,
        ticks_per_beat,
        measures,
        tracks: reports,
        warnings,
    })
}

fn build_melodic_track(
    notes: &[NoteEvent],
    spec: &crate::schema::Track,
    index: usize,
    active_tracks: usize,
) -> Result<(RawTrackKind, TrackReport)> {
    let chip = chip_for(spec.instrument).expect("caller checked percussion");
    let mut min = u8::MAX;
    let mut max = u8::MIN;
    for n in notes {
        if n.key < MIDI_BASE || n.key > MIDI_MAX {
            return Err(validation(
                format!("tracks[{index}]"),
                format!(
                    "track `{}` plays MIDI key {} at tick {}, outside the MakeCode pitch range \
                     B0..=B8 (MIDI {MIDI_BASE}..={MIDI_MAX})",
                    spec.id, n.key, n.tick
                ),
            ));
        }
        min = min.min(n.key);
        max = max.max(n.key);
    }
    // Position the 64-semitone note window: octave field is defined as
    // window_base = (octave - 2) * 12 in MakeCode note numbers (MIDI - 23).
    let base = (min - MIDI_BASE) / 12 * 12;
    if max - MIDI_BASE - base > 63 {
        return Err(validation(
            format!("tracks[{index}]"),
            format!(
                "track `{}` spans MIDI keys {min}..={max}; the MakeCode note window covers at \
                 most 64 semitones per track",
                spec.id
            ),
        ));
    }
    let octave = base / 12 + 2;
    let mut instrument = chip.params();
    instrument.octave = octave;
    scale_instrument(&mut instrument, spec.intensity, active_tracks);
    Ok((
        RawTrackKind::Melodic(instrument),
        TrackReport {
            scene_track_id: spec.id.clone(),
            instrument: instrument_key(spec.instrument),
            makecode_track: 0,
            kind: "melodic",
            chip_preset: Some(chip.name()),
            octave: Some(octave),
            voices: Vec::new(),
            note_events: 0,
        },
    ))
}

fn build_drum_track(
    notes: &[NoteEvent],
    spec: &crate::schema::Track,
    index: usize,
    active_tracks: usize,
) -> Result<(RawTrackKind, TrackReport)> {
    let mut keys: Vec<u8> = notes.iter().map(|n| n.key).collect();
    keys.sort_unstable();
    keys.dedup();
    let mut drums = Vec::with_capacity(keys.len());
    let mut voices = Vec::with_capacity(keys.len());
    for key in &keys {
        let preset = drum_for(*key).ok_or_else(|| {
            validation(
                format!("tracks[{index}]"),
                format!(
                    "track `{}` plays GM percussion key {key}, which has no curated MakeCode \
                     drum voice",
                    spec.id
                ),
            )
        })?;
        let mut drum = DrumInstrument::from(preset);
        scale_drum(&mut drum, spec.intensity, active_tracks);
        drums.push(drum);
        voices.push((*key, preset.name));
    }
    Ok((
        RawTrackKind::Drums(drums),
        TrackReport {
            scene_track_id: spec.id.clone(),
            instrument: instrument_key(spec.instrument),
            makecode_track: 0,
            kind: "drums",
            chip_preset: None,
            octave: None,
            voices,
            note_events: 0,
        },
    ))
}

/// Group normalized notes into polyphonic note events on the solved grid.
/// Chord tones share one event; its end is the longest member's end (drum
/// events span one grid step — MakeCode drums are one-shots).
fn build_events(
    notes: &[NoteEvent],
    percussion: bool,
    step: u32,
    kind: &RawTrackKind,
) -> (Vec<RawEvent>, Vec<u8>) {
    let octave = match kind {
        RawTrackKind::Melodic(instrument) => instrument.octave,
        RawTrackKind::Drums(_) => 0,
    };
    let drum_keys: Vec<u8> = match kind {
        RawTrackKind::Drums(_) => {
            let mut keys: Vec<u8> = notes.iter().map(|n| n.key).collect();
            keys.sort_unstable();
            keys.dedup();
            keys
        }
        RawTrackKind::Melodic(_) => Vec::new(),
    };
    let mut events = Vec::new();
    let mut velocities = Vec::new();
    let mut i = 0;
    while i < notes.len() {
        let tick = notes[i].tick;
        let mut j = i;
        let mut end = 0u32;
        let mut vel = 0u8;
        let mut bytes = Vec::new();
        while j < notes.len() && notes[j].tick == tick {
            let n = &notes[j];
            end = end.max(n.tick + n.dur);
            vel = vel.max(n.vel);
            let byte = if percussion {
                drum_keys
                    .iter()
                    .position(|k| *k == n.key)
                    .expect("key collected above") as u8
            } else {
                (n.key - MIDI_BASE) - (octave - 2) * 12
            };
            bytes.push(byte);
            j += 1;
        }
        let start_tick = (tick / step) as u16;
        let end_tick = if percussion {
            start_tick + 1
        } else {
            (end / step) as u16
        };
        events.push(RawEvent {
            start_tick,
            end_tick,
            notes: bytes,
        });
        velocities.push(vel);
        i = j;
    }
    (events, velocities)
}

// ---------------------------------------------------------------------------
// Scene-level compilation: one song per suite section (or one for the scene).
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct SongUnit {
    /// Section name for suite scenes; `None` for a single-scene song.
    pub section: Option<String>,
    pub looping: bool,
    pub song: EncodedSong,
}

/// Compile a validated scene into MakeCode songs — one per section for a
/// suite, otherwise a single song.
pub fn compile_scene(scene: &Scene) -> Result<Vec<SongUnit>> {
    if !scene.textures.is_empty() {
        return Err(validation(
            "textures",
            "texture layers are rendered audio and cannot be represented in the MakeCode \
             synth-song format; remove `textures` or target `scorekit build` instead",
        ));
    }
    let mut units = Vec::new();
    if scene.sections.is_empty() {
        units.push(SongUnit {
            section: None,
            looping: scene.r#loop,
            song: encode_song(scene, &crate::composer::compose(scene))?,
        });
    } else {
        for section in &scene.sections {
            let section_scene = scene.for_section(section);
            units.push(SongUnit {
                section: Some(section.name.clone()),
                looping: section_scene.r#loop,
                song: encode_song(&section_scene, &crate::composer::compose(&section_scene))?,
            });
        }
    }
    Ok(units)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_hex(hex: &str) -> Vec<u8> {
        let clean: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
        (0..clean.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).expect("valid hex"))
            .collect()
    }

    /// Real songs produced by the MakeCode song editor must decode and
    /// re-encode byte-identically: proof of interoperability with pxt's
    /// encoder, pinned as golden vectors against upstream format drift.
    #[test]
    fn roundtrips_makecode_editor_songs_byte_exactly() {
        for (name, hex) in [
            ("patrol", PATROL_BGM),
            ("combat", COMBAT_BGM),
            ("boss", BOSS_BGM),
            ("victory", VICTORY_BGM),
        ] {
            let bytes = from_hex(hex);
            let song = decode_raw(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(encode_raw(&song), bytes, "{name} did not roundtrip");
        }
    }

    #[test]
    fn decodes_editor_header_fields() {
        let song = decode_raw(&from_hex(PATROL_BGM)).unwrap();
        assert_eq!(song.bpm, 120);
        assert_eq!(song.beats_per_measure, 4);
        assert_eq!(song.ticks_per_beat, 8);
        assert_eq!(song.measures, 4);
        assert_eq!(song.tracks.len(), 4);
        let victory = decode_raw(&from_hex(VICTORY_BGM)).unwrap();
        assert_eq!(victory.bpm, 148);
        assert_eq!(victory.measures, 2);
        assert_eq!(victory.tracks.len(), 3);
    }

    #[test]
    fn drum_map_covers_every_composer_and_clip_percussion_key() {
        // Composer patterns: kick 36, snare 38, hat 42, tabla theka 36..=39.
        for key in [36u8, 37, 38, 39, 42] {
            assert!(drum_for(key).is_some(), "missing composer key {key}");
        }
        // Every percussion-clip voice in the schema vocabulary.
        use crate::schema::PercussionVoice::*;
        for voice in [
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
            Tambourine,
            Cowbell,
            HighBongo,
            LowBongo,
            MuteHighConga,
            OpenHighConga,
            LowConga,
            HighTimbale,
            LowTimbale,
            HighAgogo,
            LowAgogo,
            Cabasa,
            Maracas,
        ] {
            assert!(
                drum_for(voice.midi_key()).is_some(),
                "missing clip voice key {}",
                voice.midi_key()
            );
        }
    }

    #[test]
    fn mix_amplitude_keeps_four_track_chip_songs_in_editor_range() {
        let amp = mix_amplitude(1024, 0.75, 4);
        assert!(
            (220..=384).contains(&amp),
            "4-track intensity 0.75 should match MakeCode editor volumes, got {amp}"
        );
    }

    #[test]
    fn octave_window_positions_the_six_bit_note() {
        // A note at MIDI 60 (C4): MakeCode note 37, window base 36, octave 5.
        let notes = [NoteEvent {
            tick: 0,
            dur: 480,
            key: 60,
            vel: 100,
        }];
        let kind = RawTrackKind::Melodic(MelodicInstrument {
            octave: 5,
            ..Chip::Dog.params()
        });
        let (events, velocities) = build_events(&notes, false, 240, &kind);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].start_tick, 0);
        assert_eq!(events[0].end_tick, 2);
        assert_eq!(events[0].notes, vec![(60 - 23) - 36]);
        assert_eq!(velocities, vec![100]);
    }

    const PATROL_BGM: &str = "0078000408040400001c00010a006400f401640080010000000000000000000000000005000004420000000600012e1000160001311c002000011d20002600013528002e00013830003600013540004600012e50005600012c60006600012968006e00012c70007600012e01001c000f05001202c102c20104010500280000006400280003140006020004c00000000300011204000700011608000b0001190c000f00011d10001300011214001700011618001b0001191c001f00011d20002300011024002700011428002b0001172c002f00011b30003300011034003700011438003b0001173c003f00011b40004300010f44004700011348004b0001174c004f00011a50005300010f54005700011358005b0001175c005f00011a60006300011064006700011468006b0001176c006f00011b70007300011074007700011478007b0001177c007f00011b02001c000c960064006d019001540178002c010000640032000000000a060005300000000e0001f510001e0001f520002e0001fa30003e0001fa40004e0001f350005e0001f360006e0001f870007e0001f403001c0001dc00690000045e01dc00000000000000000000000564000104000360000000010001fc0800090001fc1000110001fc1800190001fc2000210001fc2800290001fc3000310001fc3800390001fc4000410001fc4800490001fc5000510001fc5800590001fc6000610001fc6800690001fc7000710001fc7800790001fc";

    const COMBAT_BGM: &str = "00a0000408040400001c00010a006400f40164008001000000000000000000000000000500000496000000030001250400070001250c000f00012910001300012c14001700012918001b0001251c00200001192000230001272400270001272c002f00012c30003300013034003700012c38003b0001274000430001254400470001254c004f00012950005300012c54005700012958005b0001256000630001276400670001276c006f00012c70007300013074007700012c78007b00012701001c000f05001202c102c20104010500280000006400280003140006020004c00000000300011104000700011508000b0001180c000f00011c10001300011314001700011718001b00011a1c001f00011e20002300011124002700011528002b0001182c002f00011c30003300011334003700011738003b00011a3c003f00011e40004300011144004700011548004b0001184c004f00011c50005300011354005700011758005b00011a5c005f00011e60006300011164006700011568006b0001186c006f00011c70007300011374007700011778007b00011a7c007f00011e02001c000c960064006d019001540178002c010000640032000000000a06000560000000070001f508000f0001f51000170001f818001f0001f82000270001fa28002f0001fa3000370001f838003f0001f44000470001f548004f0001f55000570001f858005f0001f86000670001fa68006f0001fa7000770001f878007f0001f403001c0001dc00690000045e01dc00000000000000000000000564000104000390000000010001fc0800090001fc0c000d0001fc1000110001fc1800190001fc1c001d0001fc2000210001fc2800290001fc2c002d0001fc3000310001fc3800390001fc3c003d0001fc4000410001fc4800490001fc4c004d0001fc5000510001fc5800590001fc5c005d0001fc6000610001fc6800690001fc6c006d0001fc7000710001fc7800790001fc7c007d0001fc";

    const BOSS_BGM: &str = "0068000408040400001c00010a006400f4016400800100000000000000000000000000050000043c0000000700012410001700012418001f00013020002700012e30003700012940004700012448004f00012750005700012960006700012370007700012401001c000f05001202c102c20104010500280000006400280003140006020004c00000000300010c04000700010f08000b0001130c000f00011810001300010b14001700010f18001b0001121c001f00011720002300010c24002700010f28002b0001132c002f00011830003300010b34003700010f38003b0001123c003f00011740004300010c44004700010f48004b0001134c004f00011850005300010b54005700010f58005b0001125c005f00011760006300010c64006700010f68006b0001136c006f00011870007300010b74007700010f78007b0001127c007f00011702001c000c960064006d019001540178002c010000640032000000000a060005300000000f0001f010001f0001f020002f0001f330003f0001f340004f0001ee50005f0001ee60006f0001ef70007f0001ef03001c0001dc00690000045e01dc00000000000000000000000564000104000360000000020001fc0c000e0001fc1000120001fc1c001e0001fc2000220001fc2c002e0001fc3000320001fc3c003e0001fc4000420001fc4c004e0001fc5000520001fc5c005e0001fc6000620001fc6c006e0001fc7000720001fc7c007e0001fc";

    const VICTORY_BGM: &str = "0094000408020300001c00010a006400f401640080010000000000000000000000000005000004300000000700012408000f00012810001700012c18001f00013020002700012c28002f00012830003700012738003f00012401001c000f05001202c102c20104010500280000006400280003140006020004600000000300011004000700011408000b0001170c000f00011c10001300011014001700011418001b0001171c001f00011c20002300011024002700011428002b0001172c002f00011c30003300011034003700011438003b0001173c003f00011c02001c000c960064006d019001540178002c010000640032000000000a060005180000000e0001f410001e0001f820002e0001fa30003e0001f4";
}
