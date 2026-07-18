//! Turns a validated `Scene` into a deterministic note-event IR.
//! Determinism rules: integer/table math only, track order preserved,
//! no hash maps, no randomness, no time or environment reads.

use crate::schema::{
    Instrument, Key, Pattern, Performance, Scene, TimeSig, parse_key, parse_numeral,
    parse_time_signature,
};

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

/// One pitch-bend event; `value` is the 14-bit MIDI bend (8192 = center).
#[derive(Debug, Clone, Copy)]
pub struct BendEvent {
    pub tick: u32,
    pub value: u16,
}

#[derive(Debug)]
pub struct TrackIr {
    pub channel: u8,
    /// GM program; `None` on the drum channel.
    pub program: Option<u8>,
    /// CC10 value at track start; `None` emits no controller.
    pub pan: Option<u8>,
    /// CC91 value at track start; `None` emits no controller.
    pub reverb: Option<u8>,
    pub notes: Vec<NoteEvent>,
    /// Tail-portamento pitch bends (`glide`), tick-sorted by construction.
    pub bends: Vec<BendEvent>,
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

/// The progression used when a scene declares no `harmony`.
pub fn default_progression(minor: bool) -> [usize; 4] {
    if minor { MINOR_PROG } else { MAJOR_PROG }
}

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

/// MIDI note for a signed 1-based melody degree (0 is a rest and never maps),
/// one octave above the harmony register.
fn melody_note(key: Key, degree: i8) -> u8 {
    let base: i32 = if key.root_pc > 6 { 60 } else { 72 };
    let idx = if degree > 0 {
        i32::from(degree) - 1
    } else {
        i32::from(degree)
    };
    let octave = idx.div_euclid(7);
    let step = idx.rem_euclid(7) as usize;
    let semis = i32::from(scale(key)[step]) + 12 * octave;
    (base + i32::from(key.root_pc) + semis).clamp(0, 127) as u8
}

/// Root-position triad for the bar's chord, one progression step per bar.
fn chord_for_bar(key: Key, bar: u32, prog: &[usize]) -> [u8; 3] {
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

/// Fill `total_ticks` by cycling the motif, truncating the last note at the
/// end. Rests (degree 0) advance time silently.
fn melody_notes(
    steps: &[crate::schema::MotifNote],
    key: Key,
    beat_ticks: u32,
    total_ticks: u32,
    vel: u8,
) -> Vec<NoteEvent> {
    let mut notes = Vec::new();
    let total = u64::from(total_ticks);
    let mut tick: u64 = 0;
    while tick < total {
        let pass_start = tick;
        for step in steps {
            let dur = u64::from((step.beats * f64::from(beat_ticks)).round() as u32);
            if dur == 0 {
                continue;
            }
            let end = (tick + dur).min(total);
            if step.degree != 0 && end > tick {
                notes.push(NoteEvent {
                    tick: tick as u32,
                    dur: (end - tick) as u32,
                    key: melody_note(key, step.degree),
                    vel,
                });
            }
            tick += dur;
            if tick >= total {
                break;
            }
        }
        // Guard against motifs whose steps all round to zero ticks.
        if tick == pass_start {
            break;
        }
    }
    notes
}

pub fn compose(scene: &Scene) -> ScoreIr {
    let key = parse_key(&scene.key).expect("scene is validated");
    let ts = parse_time_signature(&scene.time_signature).expect("scene is validated");
    let beat_ticks = PPQ * 4 / u32::from(ts.den);
    let bar_ticks = beat_ticks * u32::from(ts.num);
    let bars = u32::from(scene.bars);
    let total_ticks = bar_ticks * bars;
    let prog: Vec<usize> = if scene.harmony.is_empty() {
        let d = default_progression(key.minor);
        d.to_vec()
    } else {
        scene
            .harmony
            .iter()
            .map(|n| parse_numeral(n).expect("scene is validated"))
            .collect()
    };

    let mut tracks = Vec::with_capacity(scene.tracks.len());
    let mut next_channel: u8 = 0;
    for track in &scene.tracks {
        let vel = base_velocity(track.intensity);
        let mut notes = if track.pattern == Pattern::Melody {
            // Melody flows across barlines; the per-bar loop below is skipped.
            let name = track.motif.as_deref().expect("scene is validated");
            melody_notes(&scene.motifs[name], key, beat_ticks, total_ticks, vel)
        } else {
            Vec::new()
        };
        let harmony_bars = if track.pattern == Pattern::Melody {
            0
        } else {
            bars
        };
        for bar in 0..harmony_bars {
            let start = bar * bar_ticks;
            let chord = chord_for_bar(key, bar, &prog);
            match track.pattern {
                Pattern::Melody => unreachable!("handled above"),
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
            pan: track.pan.map(|p| (p * 127.0).round() as u8),
            reverb: track.reverb.map(|r| (r * 127.0).round() as u8),
            notes,
            bends: Vec::new(),
        });
    }

    if let Some(p) = &scene.performance {
        apply_performance(&mut tracks, p, scene.tempo, beat_ticks, total_ticks);
    }

    // Glide reads final note positions, so it runs after every performance
    // transform and before the loop `repeat` (bends copy with their notes).
    for (track, spec) in tracks.iter_mut().zip(&scene.tracks) {
        if let Some(glide) = spec.glide
            && glide > 0.0
        {
            track.bends = glide_bends(&track.notes, glide, scene.r#loop, total_ticks);
        }
    }

    ScoreIr {
        tempo: scene.tempo,
        ts,
        total_ticks,
        tracks,
    }
}

/// Tiny deterministic LCG (Knuth MMIX constants). No external RNG: identical
/// seed must yield identical bytes on every platform, forever.
struct Lcg(u64);

impl Lcg {
    /// Uniform value in `-max..=max`.
    fn jitter(&mut self, max: u32) -> i64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        if max == 0 {
            return 0;
        }
        let span = u64::from(max) * 2 + 1;
        ((self.0 >> 33) % span) as i64 - i64::from(max)
    }
}

/// Deterministic performance transforms, applied before any loop `repeat` so
/// every pass carries the identical performance and loop math is untouched.
/// Order: swing (grid) → dynamics (phrase) → legato (articulation) →
/// humanize (noise on top).
fn apply_performance(
    tracks: &mut [TrackIr],
    p: &Performance,
    tempo: u16,
    beat_ticks: u32,
    total_ticks: u32,
) {
    if p.swing > 0.0 {
        let offbeat = beat_ticks / 2;
        let shift = (p.swing * offbeat as f32).round() as u32;
        for track in tracks.iter_mut() {
            for n in &mut track.notes {
                if n.tick % beat_ticks == offbeat {
                    n.tick += shift;
                }
            }
        }
    }
    if let Some(d) = &p.dynamics {
        // Sine arch: start level at both ends, peak at the midpoint — the
        // loop boundary sees the same level on both sides by construction.
        let (start, peak) = (d.start.factor(), d.peak.factor());
        for track in tracks.iter_mut() {
            for n in &mut track.notes {
                let pos = f64::from(n.tick) / f64::from(total_ticks.max(1));
                let arch = (std::f64::consts::PI * pos).sin();
                let factor = f64::from(start) + f64::from(peak - start) * arch;
                n.vel = (f64::from(n.vel) * factor).round().clamp(1.0, 127.0) as u8;
            }
        }
    }
    if p.legato {
        for track in tracks.iter_mut().filter(|t| t.channel != DRUM_CHANNEL) {
            for n in &mut track.notes {
                n.dur += n.dur / 8;
            }
        }
    }
    if let Some(h) = &p.humanize {
        let mut rng = Lcg(h.seed.wrapping_mul(2862933555777941757).wrapping_add(1));
        let max_ticks = u32::from(h.timing_ms) * u32::from(tempo) * PPQ / 60_000;
        for track in tracks.iter_mut() {
            for n in &mut track.notes {
                let dt = rng.jitter(max_ticks);
                n.tick = (i64::from(n.tick) + dt).max(0) as u32;
                let dv = rng.jitter(u32::from(h.velocity));
                n.vel = (i64::from(n.vel) + dv).clamp(1, 127) as u8;
            }
        }
    }
}

/// Center value of the 14-bit pitch-bend range.
const BEND_CENTER: i32 = 8192;
/// GM default bend range in semitones; glide targets clamp to it.
const BEND_RANGE_SEMIS: i32 = 2;
/// Bend-curve sampling grid in ticks (PPQ/8 = a 32nd note).
const BEND_GRID: u32 = PPQ / 8;

/// Deterministic tail portamento for a monophonic melody track.
///
/// For each consecutive note pair (vector order = motif order), the final
/// `glide` fraction of the note — capped at the next onset so legato overlap
/// glides *into* the new note — ramps the channel pitch bend linearly from
/// center toward the next pitch (clamped to ±2 semitones), then resets to
/// center exactly at the next onset. In loop scenes the last note glides
/// toward the first note's pitch with the reset pinned to `total_ticks`, so
/// the gesture stays continuous across the loop seam. Pure position math;
/// same input, same bytes.
fn glide_bends(notes: &[NoteEvent], glide: f32, looping: bool, total_ticks: u32) -> Vec<BendEvent> {
    let mut bends = Vec::new();
    if notes.is_empty() {
        return bends;
    }
    let mut push_pair = |cur: &NoteEvent, next_key: u8, next_onset: u32| {
        let delta =
            (i32::from(next_key) - i32::from(cur.key)).clamp(-BEND_RANGE_SEMIS, BEND_RANGE_SEMIS);
        if delta == 0 {
            return;
        }
        let end = (cur.tick + cur.dur).min(next_onset);
        let window = (f64::from(cur.dur) * f64::from(glide)).round() as u32;
        let window = window.min(end.saturating_sub(cur.tick));
        if window == 0 {
            return;
        }
        let start = end - window;
        if next_onset <= start {
            // Humanized onsets can reorder; a degenerate window emits nothing.
            return;
        }
        let mut t = start;
        while t < end {
            let frac = f64::from(t - start) / f64::from(window);
            let offset = (frac * f64::from(delta) * 4096.0).round() as i32;
            bends.push(BendEvent {
                tick: t,
                value: (BEND_CENTER + offset).clamp(0, 16383) as u16,
            });
            t += BEND_GRID;
        }
        bends.push(BendEvent {
            tick: next_onset,
            value: BEND_CENTER as u16,
        });
    };
    for pair in notes.windows(2) {
        push_pair(&pair[0], pair[1].key, pair[1].tick);
    }
    if looping && notes.len() > 1 {
        let last = notes[notes.len() - 1];
        push_pair(&last, notes[0].key, total_ticks);
    }
    bends
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
        let one_bends = track.bends.clone();
        for pass in 1..u32::from(times) {
            let offset = base * pass;
            track.notes.extend(one.iter().map(|n| NoteEvent {
                tick: n.tick + offset,
                ..*n
            }));
            track.bends.extend(one_bends.iter().map(|b| BendEvent {
                tick: b.tick + offset,
                ..*b
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
