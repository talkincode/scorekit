//! Turns a validated `Scene` into a deterministic note-event IR.
//! Determinism rules: integer/table math only, track order preserved,
//! no hash maps, no randomness, no time or environment reads.

use crate::schema::{Instrument, Key, Pattern, Scene, TimeSig, parse_key, parse_time_signature};

pub const PPQ: u32 = 480;
const DRUM_CHANNEL: u8 = 9;
const KICK: u8 = 36;
const SNARE: u8 = 38;
const HIHAT: u8 = 42;

#[derive(Debug, Clone, Copy)]
pub struct NoteEvent {
    pub tick: u32,
    pub dur: u32,
    pub key: u8,
    pub vel: u8,
}

#[derive(Debug)]
pub struct TrackIr {
    pub channel: u8,
    /// GM program; `None` on the drum channel.
    pub program: Option<u8>,
    pub notes: Vec<NoteEvent>,
}

#[derive(Debug)]
pub struct ScoreIr {
    pub tempo: u16,
    pub ts: TimeSig,
    pub total_ticks: u32,
    pub tracks: Vec<TrackIr>,
}

const MAJOR_SCALE: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
const MINOR_SCALE: [u8; 7] = [0, 2, 3, 5, 7, 8, 10];
/// Chord progressions as scale-degree indices, one chord per bar, cycled.
const MAJOR_PROG: [usize; 4] = [0, 4, 5, 3]; // I V vi IV
const MINOR_PROG: [usize; 4] = [0, 5, 2, 6]; // i VI III VII

fn scale(key: Key) -> [u8; 7] {
    if key.minor { MINOR_SCALE } else { MAJOR_SCALE }
}

/// MIDI note for an absolute scale degree, centered near middle C.
fn degree_note(key: Key, degree: usize) -> u8 {
    // Keep the tonic within F#3..F#4 so voicings stay in a musical register.
    let base: i32 = if key.root_pc > 6 { 48 } else { 60 };
    let semis = i32::from(scale(key)[degree % 7]) + 12 * (degree / 7) as i32;
    (base + i32::from(key.root_pc) + semis).clamp(0, 127) as u8
}

/// Root-position triad for the bar's chord.
fn chord_for_bar(key: Key, bar: u32) -> [u8; 3] {
    let prog = if key.minor { MINOR_PROG } else { MAJOR_PROG };
    let deg = prog[(bar as usize) % prog.len()];
    [
        degree_note(key, deg),
        degree_note(key, deg + 2),
        degree_note(key, deg + 4),
    ]
}

fn base_velocity(intensity: f32) -> u8 {
    let v = (30.0 + intensity * 70.0).round();
    v.clamp(1.0, 127.0) as u8
}

fn scaled(vel: u8, factor: f32) -> u8 {
    ((f32::from(vel) * factor).round()).clamp(1.0, 127.0) as u8
}

pub fn compose(scene: &Scene) -> ScoreIr {
    let key = parse_key(&scene.key).expect("scene is validated");
    let ts = parse_time_signature(&scene.time_signature).expect("scene is validated");
    let beat_ticks = PPQ * 4 / u32::from(ts.den);
    let bar_ticks = beat_ticks * u32::from(ts.num);
    let bars = u32::from(scene.bars);
    let total_ticks = bar_ticks * bars;

    let mut tracks = Vec::with_capacity(scene.tracks.len());
    let mut next_channel: u8 = 0;
    for track in &scene.tracks {
        let vel = base_velocity(track.intensity);
        let mut notes = Vec::new();
        for bar in 0..bars {
            let start = bar * bar_ticks;
            let chord = chord_for_bar(key, bar);
            match track.pattern {
                Pattern::Sustain => {
                    for k in chord {
                        notes.push(NoteEvent {
                            tick: start,
                            dur: bar_ticks,
                            key: k,
                            vel,
                        });
                    }
                }
                Pattern::Arpeggio => {
                    let step = PPQ / 2;
                    let order = [0usize, 1, 2, 1];
                    let mut i = 0u32;
                    while i * step < bar_ticks {
                        let k = chord[order[(i as usize) % order.len()]];
                        notes.push(NoteEvent {
                            tick: start + i * step,
                            dur: step,
                            key: k,
                            vel,
                        });
                        i += 1;
                    }
                }
                Pattern::Bass => {
                    let root = chord[0].saturating_sub(24).max(24);
                    if ts.num >= 4 && ts.num.is_multiple_of(2) {
                        let half = bar_ticks / 2;
                        notes.push(NoteEvent {
                            tick: start,
                            dur: half,
                            key: root,
                            vel,
                        });
                        notes.push(NoteEvent {
                            tick: start + half,
                            dur: half,
                            key: root,
                            vel,
                        });
                    } else {
                        notes.push(NoteEvent {
                            tick: start,
                            dur: bar_ticks,
                            key: root,
                            vel,
                        });
                    }
                }
                Pattern::Drums => {
                    for beat in 0..u32::from(ts.num) {
                        let t = start + beat * beat_ticks;
                        if beat == 0 || (ts.num >= 4 && beat == u32::from(ts.num) / 2) {
                            notes.push(NoteEvent {
                                tick: t,
                                dur: 60,
                                key: KICK,
                                vel,
                            });
                        }
                        if beat % 2 == 1 {
                            notes.push(NoteEvent {
                                tick: t,
                                dur: 60,
                                key: SNARE,
                                vel: scaled(vel, 0.9),
                            });
                        }
                        notes.push(NoteEvent {
                            tick: t,
                            dur: 30,
                            key: HIHAT,
                            vel: scaled(vel, 0.6),
                        });
                        notes.push(NoteEvent {
                            tick: t + beat_ticks / 2,
                            dur: 30,
                            key: HIHAT,
                            vel: scaled(vel, 0.5),
                        });
                    }
                }
            }
        }
        let channel = if track.instrument == Instrument::Drums {
            DRUM_CHANNEL
        } else {
            // Skip the reserved drum channel for melodic tracks.
            if next_channel == DRUM_CHANNEL {
                next_channel += 1;
            }
            let c = next_channel;
            next_channel += 1;
            c
        };
        tracks.push(TrackIr {
            channel,
            program: track.instrument.gm_program(),
            notes,
        });
    }

    ScoreIr {
        tempo: scene.tempo,
        ts,
        total_ticks,
        tracks,
    }
}

/// Repeat the composed material `times` back to back (tick-shifted copies).
/// Used for seamless-loop rendering: render two passes, keep the second.
pub fn repeat(ir: &mut ScoreIr, times: u8) {
    if times <= 1 {
        return;
    }
    let base = ir.total_ticks;
    for track in &mut ir.tracks {
        let one = track.notes.clone();
        for pass in 1..u32::from(times) {
            let offset = base * pass;
            track.notes.extend(one.iter().map(|n| NoteEvent {
                tick: n.tick + offset,
                ..*n
            }));
        }
    }
    ir.total_ticks = base * u32::from(times);
}

/// Keep only the track at `index`, preserving its original channel/program
/// so a solo render is bit-compatible with its part in the full mix.
pub fn solo(ir: &mut ScoreIr, index: usize) {
    let track = ir.tracks.remove(index);
    ir.tracks = vec![track];
}
