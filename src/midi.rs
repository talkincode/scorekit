//! IR → Standard MIDI File bytes. Output must be byte-stable for identical input.

use crate::composer::{ScoreIr, TrackIr};
use midly::num::{u4, u7, u15, u24, u28};
use midly::{Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind};

/// Absolute-time event before delta encoding.
struct AbsEvent {
    tick: u32,
    /// Sort rank at equal tick: note-offs (0) before note-ons (1) to avoid retriggering.
    rank: u8,
    key: u8,
    vel: u8,
}

fn track_events(track: &TrackIr, total_ticks: u32) -> Vec<TrackEvent<'static>> {
    let mut abs: Vec<AbsEvent> = Vec::with_capacity(track.notes.len() * 2);
    for n in &track.notes {
        abs.push(AbsEvent {
            tick: n.tick,
            rank: 1,
            key: n.key,
            vel: n.vel,
        });
        abs.push(AbsEvent {
            tick: n.tick + n.dur,
            rank: 0,
            key: n.key,
            vel: 0,
        });
    }
    abs.sort_by_key(|e| (e.tick, e.rank, e.key));

    let channel = u4::new(track.channel);
    let mut events = Vec::with_capacity(abs.len() + 2);
    let mut cursor = 0u32;
    if let Some(program) = track.program {
        events.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Midi {
                channel,
                message: MidiMessage::ProgramChange {
                    program: u7::new(program),
                },
            },
        });
    }
    for e in abs {
        let delta = e.tick - cursor;
        cursor = e.tick;
        let message = if e.rank == 1 {
            MidiMessage::NoteOn {
                key: u7::new(e.key),
                vel: u7::new(e.vel),
            }
        } else {
            MidiMessage::NoteOff {
                key: u7::new(e.key),
                vel: u7::new(0),
            }
        };
        events.push(TrackEvent {
            delta: u28::new(delta),
            kind: TrackEventKind::Midi { channel, message },
        });
    }
    // Pin the track end to the exact bar boundary so file length is exact.
    events.push(TrackEvent {
        delta: u28::new(total_ticks.saturating_sub(cursor)),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    });
    events
}

pub fn to_smf_bytes(ir: &ScoreIr) -> Vec<u8> {
    let header = Header::new(
        Format::Parallel,
        Timing::Metrical(u15::new(crate::composer::PPQ as u16)),
    );
    let mut smf = Smf::new(header);

    let denom_pow2 = ir.ts.den.trailing_zeros() as u8;
    let micros_per_beat = 60_000_000u32 / u32::from(ir.tempo);
    let conductor = vec![
        TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::TimeSignature(ir.ts.num, denom_pow2, 24, 8)),
        },
        TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(micros_per_beat))),
        },
        TrackEvent {
            delta: u28::new(ir.total_ticks),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        },
    ];
    smf.tracks.push(conductor);
    for track in &ir.tracks {
        smf.tracks.push(track_events(track, ir.total_ticks));
    }

    let mut bytes = Vec::new();
    smf.write_std(&mut bytes)
        .expect("writing to Vec cannot fail");
    bytes
}
