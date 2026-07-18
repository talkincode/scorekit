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

/// Mix same-spec 16-bit PCM WAVs into one, applying `gain` and clamping.
/// Deterministic float summation (position-only, no filtering), zero-pads
/// shorter inputs to the longest track's length. Used by the `sfizz`
/// renderer backend, which can only render one instrument (one SFZ file)
/// per pass: the "full mix" is built by rendering every track solo and
/// mixing here, so it's bit-for-bit `sum of stems` by construction — the
/// same invariant the single-pass FluidSynth/TiMidity mix already gives us.
pub fn mix(inputs: &[std::path::PathBuf], output: &Path, gain: f32) -> Result<()> {
    if inputs.is_empty() {
        return Err(Error::Validation {
            path: "mix".to_owned(),
            message: "no inputs to mix".to_owned(),
        });
    }
    let mut spec: Option<hound::WavSpec> = None;
    let mut tracks: Vec<Vec<i16>> = Vec::with_capacity(inputs.len());
    for p in inputs {
        let mut reader = hound::WavReader::open(p).map_err(|e| wav_err(p, e))?;
        let s = reader.spec();
        if s.bits_per_sample != 16 || s.sample_format != hound::SampleFormat::Int {
            return Err(Error::Validation {
                path: p.display().to_string(),
                message: format!(
                    "expected 16-bit integer PCM, got {}-bit {:?}",
                    s.bits_per_sample, s.sample_format
                ),
            });
        }
        match spec {
            None => spec = Some(s),
            Some(sp) if sp.channels == s.channels && sp.sample_rate == s.sample_rate => {}
            Some(sp) => {
                return Err(Error::Validation {
                    path: p.display().to_string(),
                    message: format!(
                        "spec mismatch: expected {}ch@{}Hz, got {}ch@{}Hz",
                        sp.channels, sp.sample_rate, s.channels, s.sample_rate
                    ),
                });
            }
        }
        let samples: Vec<i16> = reader
            .samples::<i16>()
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| wav_err(p, e))?;
        tracks.push(samples);
    }
    let spec = spec.expect("checked inputs is non-empty above");
    let max_len = tracks.iter().map(Vec::len).max().unwrap_or(0);
    let g = f64::from(gain);

    let mut cursor = std::io::Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(&mut cursor, spec).map_err(|e| wav_err(output, e))?;
    for i in 0..max_len {
        let mut sum = 0.0f64;
        for t in &tracks {
            if let Some(&s) = t.get(i) {
                sum += f64::from(s);
            }
        }
        let v = (sum * g)
            .round()
            .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
        writer.write_sample(v).map_err(|e| wav_err(output, e))?;
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
    fn mix_sums_tracks_zero_pads_and_clamps() {
        let dir = std::env::temp_dir().join(format!("sk-audio-mix-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.wav");
        let b = dir.join("b.wav");
        let out = dir.join("mix.wav");
        write_wav(&a, &[100, 200, 300]);
        write_wav(&b, &[50, 60]); // shorter: zero-padded past its end
        mix(&[a.clone(), b.clone()], &out, 1.0).unwrap();
        let mixed = read_wav(&out);
        assert_eq!(mixed, vec![150, 260, 300]);

        // Overflow clamps to i16::MAX instead of wrapping.
        let c = dir.join("c.wav");
        let d = dir.join("d.wav");
        let out2 = dir.join("mix2.wav");
        write_wav(&c, &[30000]);
        write_wav(&d, &[30000]);
        mix(&[c, d], &out2, 1.0).unwrap();
        assert_eq!(read_wav(&out2), vec![i16::MAX]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn mix_rejects_mismatched_specs() {
        let dir = std::env::temp_dir().join(format!("sk-audio-mix-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.wav");
        let stereo = dir.join("stereo.wav");
        write_wav(&a, &[1, 2, 3]);
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&stereo, spec).unwrap();
        w.write_sample(1i16).unwrap();
        w.write_sample(1i16).unwrap();
        w.finalize().unwrap();
        let out = dir.join("mix.wav");
        let err = mix(&[a, stereo], &out, 1.0).unwrap_err();
        assert!(err.to_string().contains("spec mismatch"), "{err}");
        assert!(!out.exists());
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
