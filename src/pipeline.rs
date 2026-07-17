//! The `build` orchestration: scene → MIDI → render → sample-exact assets.
//!
//! Loop scenes are rendered twice back to back and the second pass
//! `[L, 2L)` is kept: its head already carries the previous pass's decay
//! tail. `L` is derived from the quantized MIDI tempo
//! (`midi::exact_samples`), never from raw BPM. Because FluidSynth schedules
//! MIDI on a millisecond clock the render is not exactly `L`-periodic, so
//! `audio::extract` seals the window with a short linear crossfade into the
//! material preceding `raw[L]`, making the wrap-around bit-exactly continuous
//! (see `audio.rs` for the full rationale).
//! Non-loop scenes keep their natural tail, padded to a fixed length so the
//! full mix and every stem stay sample-aligned.

use crate::error::{Error, Result};
use crate::schema::Scene;
use crate::{audio, composer, midi, schema, tools};
use serde_json::json;
use std::path::{Path, PathBuf};

pub struct BuildArgs<'a> {
    pub scene: &'a Path,
    pub soundfont: &'a Path,
    pub output: &'a Path,
    pub renderer: tools::Renderer,
    pub sample_rate: u32,
    pub gain: f32,
    pub quality: u8,
    pub stems: bool,
    /// Decay tail (seconds) appended after the music for non-loop scenes.
    pub tail: f64,
    /// Loop-seal crossfade length in milliseconds.
    pub crossfade_ms: u32,
    pub keep_intermediates: bool,
}

fn validation(path: &str, message: String) -> Error {
    Error::Validation {
        path: path.to_owned(),
        message,
    }
}

/// Compile a scene to SMF bytes with loop passes and optional track solo.
pub fn midi_bytes(scene: &Scene, passes: u8, solo: Option<usize>) -> Result<Vec<u8>> {
    if let Some(i) = solo
        && i >= scene.tracks.len()
    {
        return Err(validation(
            "--solo",
            format!(
                "track index {i} out of range (scene has {})",
                scene.tracks.len()
            ),
        ));
    }
    let mut ir = composer::compose(scene);
    if let Some(i) = solo {
        composer::solo(&mut ir, i);
    }
    composer::repeat(&mut ir, passes);
    Ok(midi::to_smf_bytes(&ir))
}

fn instrument_name(track: &crate::schema::Track) -> String {
    serde_json::to_value(track.instrument)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "track".to_owned())
}

struct Cleanup {
    files: Vec<PathBuf>,
    dirs: Vec<PathBuf>,
    keep: bool,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if self.keep {
            return;
        }
        for f in &self.files {
            let _ = std::fs::remove_file(f);
        }
        for d in &self.dirs {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}

/// Cut `window` out of `raw` and deliver it at `output`: a direct
/// sample-exact WAV, or an intermediate cut encoded to OGG.
fn produce(
    raw: &Path,
    output: &Path,
    ext: &str,
    quality: u8,
    window: audio::Window,
    cleanup: &mut Cleanup,
) -> Result<()> {
    if ext == "wav" {
        audio::extract(raw, output, window)
    } else {
        let cut = output.with_extension("cut.wav");
        cleanup.files.push(cut.clone());
        audio::extract(raw, &cut, window)?;
        tools::export(&cut, output, quality)
    }
}

pub fn build(args: &BuildArgs) -> Result<String> {
    let ext = args
        .output
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if ext != "ogg" && ext != "wav" {
        return Err(validation(
            "--output",
            format!("unsupported output extension `.{ext}`, expected .ogg or .wav"),
        ));
    }
    let scene = schema::load_scene(args.scene)?;
    let meta_path = args.output.with_extension("meta.json");

    if scene.sections.is_empty() {
        let entry = build_one(args, &scene, args.output, &ext)?;
        let meta_bytes = serde_json::to_vec_pretty(&entry).expect("meta serializes");
        tools::write_atomic(&meta_path, &meta_bytes)?;
        return Ok(format!(
            "wrote {} ({} samples{}), {}",
            args.output.display(),
            entry["total_samples"],
            if scene.r#loop { ", seamless loop" } else { "" },
            meta_path.display(),
        ));
    }

    // Suite: one asset per section, all sharing tracks, motifs and key.
    let stem = args
        .output
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut entries = Vec::new();
    let mut names = Vec::new();
    for section in &scene.sections {
        let derived = scene.for_section(section);
        let output = args
            .output
            .with_file_name(format!("{stem}-{}.{ext}", section.name));
        let mut entry = build_one(args, &derived, &output, &ext)?;
        entry["name"] = json!(section.name);
        entries.push(entry);
        names.push(section.name.clone());
    }
    let manifest = json!({
        "title": scene.title,
        "suite": true,
        "tempo": scene.tempo,
        "key": scene.key,
        "time_signature": scene.time_signature,
        "sample_rate": args.sample_rate,
        "sections": entries,
    });
    let meta_bytes = serde_json::to_vec_pretty(&manifest).expect("manifest serializes");
    tools::write_atomic(&meta_path, &meta_bytes)?;
    Ok(format!(
        "wrote {} section(s) [{}] and {}",
        names.len(),
        names.join(", "),
        meta_path.display(),
    ))
}

/// Compile one scene into one audio asset (+ optional stems) at `output` and
/// return its metadata entry. Does not write the meta.json file.
fn build_one(
    args: &BuildArgs,
    scene: &Scene,
    output: &Path,
    ext: &str,
) -> Result<serde_json::Value> {
    let one_pass = composer::compose(scene);
    let loop_samples = midi::exact_samples(one_pass.total_ticks, scene.tempo, args.sample_rate);
    let tail_samples = (args.tail * f64::from(args.sample_rate)).round() as u64;
    let crossfade = u64::from(args.crossfade_ms) * u64::from(args.sample_rate) / 1000;
    let (passes, window) = if scene.r#loop {
        (
            2u8,
            audio::Window {
                skip: loop_samples,
                take: loop_samples,
                crossfade,
            },
        )
    } else {
        (
            1u8,
            audio::Window {
                skip: 0,
                take: loop_samples + tail_samples,
                crossfade: 0,
            },
        )
    };
    let total_samples = window.take;

    let mid = output.with_extension("mid");
    let raw = output.with_extension("raw.wav");
    let mut cleanup = Cleanup {
        files: vec![mid.clone(), raw.clone()],
        dirs: Vec::new(),
        keep: args.keep_intermediates,
    };

    // Full mix: render, cut sample-exactly in-process, then encode if needed.
    tools::write_atomic(&mid, &midi_bytes(scene, passes, None)?)?;
    tools::render(
        args.renderer,
        &mid,
        args.soundfont,
        &raw,
        args.sample_rate,
        args.gain,
    )?;
    produce(&raw, output, ext, args.quality, window, &mut cleanup)?;

    // Stems: staged in a temp dir, swapped in only when every track rendered.
    let mut stem_rel: Vec<String> = Vec::new();
    if args.stems {
        let stems_dir = output.with_extension("stems");
        let staging = output.with_extension(format!("stems.tmp-{}", std::process::id()));
        std::fs::create_dir_all(&staging).map_err(|e| Error::Io {
            path: staging.display().to_string(),
            source: e,
        })?;
        cleanup.dirs.push(staging.clone());
        for (i, track) in scene.tracks.iter().enumerate() {
            let name = format!("{:02}-{}.{ext}", i + 1, instrument_name(track));
            let mid_i = staging.join(format!("{:02}.mid", i + 1));
            let raw_i = staging.join(format!("{:02}.raw.wav", i + 1));
            tools::write_atomic(&mid_i, &midi_bytes(scene, passes, Some(i))?)?;
            tools::render(
                args.renderer,
                &mid_i,
                args.soundfont,
                &raw_i,
                args.sample_rate,
                args.gain,
            )?;
            produce(
                &raw_i,
                &staging.join(&name),
                ext,
                args.quality,
                window,
                &mut cleanup,
            )?;
            let _ = std::fs::remove_file(&mid_i);
            let _ = std::fs::remove_file(&raw_i);
            let dir_name = stems_dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            stem_rel.push(format!("{dir_name}/{name}"));
        }
        if stems_dir.exists() {
            std::fs::remove_dir_all(&stems_dir).map_err(|e| Error::Io {
                path: stems_dir.display().to_string(),
                source: e,
            })?;
        }
        std::fs::rename(&staging, &stems_dir).map_err(|e| Error::Io {
            path: stems_dir.display().to_string(),
            source: e,
        })?;
        cleanup.dirs.clear();
    }

    // Machine-readable metadata for game engines and agent pipelines.
    let audio_name = output
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(json!({
        "title": scene.title,
        "loop": scene.r#loop,
        "tempo": scene.tempo,
        "key": scene.key,
        "time_signature": scene.time_signature,
        "bars": scene.bars,
        "sample_rate": args.sample_rate,
        "loop_samples": loop_samples,
        "total_samples": total_samples,
        "crossfade_samples": if scene.r#loop { crossfade.min(loop_samples) } else { 0 },
        "seconds": total_samples as f64 / f64::from(args.sample_rate),
        "audio": audio_name,
        "stems": stem_rel,
        "tracks": scene.tracks.iter().map(|t| json!({
            "instrument": instrument_name(t),
            "pattern": serde_json::to_value(t.pattern).unwrap_or(json!(null)),
            "intensity": t.intensity,
        })).collect::<Vec<_>>(),
    }))
}
