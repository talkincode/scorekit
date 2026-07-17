//! Sample-exact WAV surgery, all in-process — no reliance on external tool
//! filter semantics for correctness-critical cuts.
//!
//! Why the loop seal exists: FluidSynth's MIDI player schedules events on a
//! millisecond clock, so a rendered pass is *not* exactly `L` samples long
//! (measured ≈ `L − 28` with jitter for the forest scene). True sample
//! periodicity is therefore unreachable and the naive `[L, 2L)` cut clicks on
//! wrap-around. Instead we keep the `[L, 2L)` window (its head carries pass
//! one's decay tail) and linearly crossfade its final samples toward
//! `raw[L − X .. L)` — the material that originally preceded window start
//! `raw[L]`. The final output sample equals `raw[L − 1]` bit-exactly, so the
//! wrap `out[last] → out[0]` reproduces an adjacent-sample pair of the
//! original render: maximal continuity, verifiable exactly in tests.
//!
//! The seal is linear with position-only coefficients, so applying it to each
//! stem individually preserves "sum of stems == full mix" up to rounding.

use crate::error::{Error, Result};
use crate::tools;
use std::path::Path;

/// A sample-exact window over an input WAV.
#[derive(Debug, Clone, Copy)]
pub struct Window {
    /// Skip this many sample frames from the start.
    pub skip: u64,
    /// Emit exactly this many frames, zero-padding past end of input.
    pub take: u64,
    /// Crossfade the final `crossfade` frames toward the frames immediately
    /// before `skip` (the loop seal). Clamped to `min(skip, take)`.
    pub crossfade: u64,
}

fn wav_err(path: &Path, e: hound::Error) -> Error {
    match e {
        hound::Error::IoError(source) => Error::Io {
            path: path.display().to_string(),
            source,
        },
        other => Error::Validation {
            path: path.display().to_string(),
            message: format!("not a usable WAV file: {other}"),
        },
    }
}

/// Number of sample frames in a WAV file.
pub fn frames(path: &Path) -> Result<u64> {
    let reader = hound::WavReader::open(path).map_err(|e| wav_err(path, e))?;
    Ok(u64::from(reader.duration()))
}

/// Extract `window` from `input` into `output` (16-bit PCM in and out),
/// written atomically. Deterministic: integer/f64 position-only math.
pub fn extract(input: &Path, output: &Path, window: Window) -> Result<()> {
    let mut reader = hound::WavReader::open(input).map_err(|e| wav_err(input, e))?;
    let spec = reader.spec();
    if spec.bits_per_sample != 16 || spec.sample_format != hound::SampleFormat::Int {
        return Err(Error::Validation {
            path: input.display().to_string(),
            message: format!(
                "expected 16-bit integer PCM, got {}-bit {:?}",
                spec.bits_per_sample, spec.sample_format
            ),
        });
    }
    let channels = u64::from(spec.channels.max(1));
    let samples: Vec<i16> = reader
        .samples::<i16>()
        .collect::<std::result::Result<_, _>>()
        .map_err(|e| wav_err(input, e))?;
    let frames = samples.len() as u64 / channels;

    let cf = window.crossfade.min(window.skip).min(window.take);
    let sample_at = |frame: u64, ch: u64| -> f64 {
        if frame < frames {
            f64::from(samples[usize::try_from(frame * channels + ch).expect("index fits")])
        } else {
            0.0
        }
    };

    let mut cursor = std::io::Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(&mut cursor, spec).map_err(|e| wav_err(output, e))?;
    for i in 0..window.take {
        for ch in 0..channels {
            let mut v = sample_at(window.skip + i, ch);
            if cf > 0 && i >= window.take - cf {
                // j runs 1..=cf; weight hits exactly 1.0 on the final frame,
                // making the last output frame bit-equal to the pre-window
                // material — the seamless-loop guarantee.
                let j = i - (window.take - cf) + 1;
                let a = j as f64 / cf as f64;
                let pre = sample_at(window.skip - cf + (j - 1), ch);
                v += (pre - v) * a;
            }
            let s = v.round().clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
            writer.write_sample(s).map_err(|e| wav_err(output, e))?;
        }
    }
    writer.finalize().map_err(|e| wav_err(output, e))?;
    tools::write_atomic(output, &cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_wav(path: &Path, data: &[i16]) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for &s in data {
            w.write_sample(s).unwrap();
        }
        w.finalize().unwrap();
    }

    fn read_wav(path: &Path) -> Vec<i16> {
        hound::WavReader::open(path)
            .unwrap()
            .samples::<i16>()
            .map(|s| s.unwrap())
            .collect()
    }

    #[test]
    fn seal_last_frame_is_bit_exact_pre_window() {
        let dir = std::env::temp_dir().join(format!("sk-audio-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("in.wav");
        let output = dir.join("out.wav");
        // input: 0..20, window skip=10 take=10 cf=4
        let data: Vec<i16> = (0..20).map(|i| i * 100).collect();
        write_wav(&input, &data);
        extract(
            &input,
            &output,
            Window {
                skip: 10,
                take: 10,
                crossfade: 4,
            },
        )
        .unwrap();
        let out = read_wav(&output);
        assert_eq!(out.len(), 10);
        assert_eq!(out[0], 1000); // untouched copy of input[10]
        assert_eq!(out[5], 1500); // before the fade region
        // final frame == input[skip - 1] exactly
        assert_eq!(out[9], 900);
        // fade ramps toward pre-window values: out[6] = in[16] + (in[6]-in[16])*1/4
        assert_eq!(out[6], (1600.0f64 + (600.0 - 1600.0) * 0.25).round() as i16);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn zero_pads_past_end_of_input() {
        let dir = std::env::temp_dir().join(format!("sk-audio-pad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("in.wav");
        let output = dir.join("out.wav");
        write_wav(&input, &[7; 5]);
        extract(
            &input,
            &output,
            Window {
                skip: 0,
                take: 8,
                crossfade: 0,
            },
        )
        .unwrap();
        assert_eq!(read_wav(&output), vec![7, 7, 7, 7, 7, 0, 0, 0]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
