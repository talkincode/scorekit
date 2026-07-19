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
    /// SF2 SoundFont path. Required for `Renderer::Fluidsynth`/`Timidity`,
    /// must be `None` for `Renderer::Sfizz` (see `profile` instead).
    pub soundfont: Option<&'a Path>,
    /// Renderer profile path (maps instruments to `.sfz` files). Required
    /// for `Renderer::Sfizz`, must be `None` for the SF2 backends.
    pub profile: Option<&'a Path>,
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

/// A validated renderer backend: exactly the input the selected renderer
/// consumes, so downstream code matches on this instead of re-checking
/// `soundfont`/`profile` optionality.
enum Backend<'a> {
    /// SF2-consuming renderers (FluidSynth, TiMidity++).
    Sf2 { soundfont: &'a Path },
    /// `sfizz_render`, driven by a renderer profile mapping instruments to `.sfz`.
    Sfizz { profile: &'a Path },
}

/// Exactly one of `soundfont`/`profile` must be set, matching what the
/// selected renderer actually consumes — this is checked once up front so
/// every scene in a batch fails the same clear way instead of partway
/// through a long run.
fn require_backend<'a>(
    renderer: tools::Renderer,
    soundfont: Option<&'a Path>,
    profile: Option<&'a Path>,
) -> Result<Backend<'a>> {
    match renderer {
        tools::Renderer::Sfizz => {
            let Some(profile) = profile else {
                return Err(validation(
                    "--profile",
                    "renderer `sfizz` requires --profile (maps instruments to .sfz files); see `scorekit schema --profile`"
                        .to_owned(),
                ));
            };
            if soundfont.is_some() {
                return Err(validation(
                    "--soundfont",
                    "renderer `sfizz` does not use --soundfont; pass --profile instead".to_owned(),
                ));
            }
            Ok(Backend::Sfizz { profile })
        }
        tools::Renderer::Fluidsynth | tools::Renderer::Timidity => {
            let Some(soundfont) = soundfont else {
                return Err(validation(
                    "--soundfont",
                    "this renderer requires --soundfont".to_owned(),
                ));
            };
            if profile.is_some() {
                return Err(validation(
                    "--profile",
                    "this renderer does not use --profile; pass --soundfont instead".to_owned(),
                ));
            }
            Ok(Backend::Sf2 { soundfont })
        }
    }
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

/// Batch: render many scenes in one run; per-scene failures don't abort the
/// run, they land in a machine-readable report instead.
pub struct BatchArgs<'a> {
    pub scenes: &'a [PathBuf],
    pub soundfont: Option<&'a Path>,
    pub profile: Option<&'a Path>,
    pub out_dir: &'a Path,
    pub format: &'a str,
    pub renderer: tools::Renderer,
    pub sample_rate: u32,
    pub gain: f32,
    pub quality: u8,
    pub stems: bool,
    pub tail: f64,
    pub crossfade_ms: u32,
    pub report: Option<&'a Path>,
}

/// Build every scene into `<out-dir>/<scene-stem>.<format>` and write a
/// report JSON. Returns the first failure (after the report is written) so
/// the exit code reflects it; agents read the report for the full picture.
pub fn batch(args: &BatchArgs) -> Result<String> {
    require_backend(args.renderer, args.soundfont, args.profile)?;
    let mut stems_seen = std::collections::BTreeMap::new();
    for (i, scene) in args.scenes.iter().enumerate() {
        let stem = scene
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Some(prev) = stems_seen.insert(stem.clone(), i) {
            return Err(validation(
                &format!("scenes[{i}]"),
                format!(
                    "duplicate scene stem `{stem}` (also scenes[{prev}]); outputs would collide"
                ),
            ));
        }
    }
    let report_path = args
        .report
        .map(Path::to_path_buf)
        .unwrap_or_else(|| args.out_dir.join("report.json"));

    let mut items = Vec::new();
    let mut first_err = None;
    let mut succeeded = 0usize;
    for scene in args.scenes {
        let stem = scene
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let output = args.out_dir.join(format!("{stem}.{}", args.format));
        let result = build(&BuildArgs {
            scene,
            soundfont: args.soundfont,
            profile: args.profile,
            output: &output,
            renderer: args.renderer,
            sample_rate: args.sample_rate,
            gain: args.gain,
            quality: args.quality,
            stems: args.stems,
            tail: args.tail,
            crossfade_ms: args.crossfade_ms,
            keep_intermediates: false,
        });
        match result {
            Ok(msg) => {
                succeeded += 1;
                items.push(json!({
                    "scene": scene.display().to_string(),
                    "ok": true,
                    "output": output.display().to_string(),
                    "meta": output.with_extension("meta.json").display().to_string(),
                    "message": msg,
                }));
            }
            Err(e) => {
                items.push(json!({
                    "scene": scene.display().to_string(),
                    "ok": false,
                    "error": {
                        "code": e.code(),
                        "message": e.to_string(),
                        "exit_code": e.exit_code(),
                    },
                }));
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
    }
    let report = json!({
        "total": args.scenes.len(),
        "succeeded": succeeded,
        "failed": args.scenes.len() - succeeded,
        "items": items,
    });
    tools::write_atomic(
        &report_path,
        &serde_json::to_vec_pretty(&report).expect("report"),
    )?;
    match first_err {
        None => Ok(format!(
            "built {} scene(s), report {}",
            succeeded,
            report_path.display()
        )),
        Some(e) => Err(e),
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
        tools::export(&cut, output, quality)?;
        // Remove eagerly: stem cuts live in the staging dir, which is renamed
        // into place before the deferred Cleanup runs — the recorded path
        // would go stale and the cut would ship inside the stems folder.
        if !cleanup.keep {
            let _ = std::fs::remove_file(&cut);
        }
        Ok(())
    }
}

pub fn build(args: &BuildArgs) -> Result<String> {
    require_backend(args.renderer, args.soundfont, args.profile)?;
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
        "story": scene.story,
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
    let mut stem_rel: Vec<String> = Vec::new();

    match require_backend(args.renderer, args.soundfont, args.profile)? {
        Backend::Sfizz {
            profile: profile_path,
        } => {
            // sfizz_render only ever plays one instrument: render every track
            // solo (the same per-track pass `--stems` needs anyway), then mix
            // them in-process into the full-mix raw. "Sum of stems == full mix"
            // therefore holds by construction, same as the SF2 backends.
            let profile = crate::profile::load_profile(profile_path)?;
            let profile_dir = profile_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));

            let staging = output.with_extension(format!("sfizz.tmp-{}", std::process::id()));
            std::fs::create_dir_all(&staging).map_err(|e| Error::Io {
                path: staging.display().to_string(),
                source: e,
            })?;
            cleanup.dirs.push(staging.clone());

            let mut track_raws: Vec<PathBuf> = Vec::with_capacity(scene.tracks.len());
            for (i, track) in scene.tracks.iter().enumerate() {
                let sfz = profile.resolve(&profile_dir, track.instrument, track.articulation)?;
                let mid_i = staging.join(format!("{:02}.mid", i + 1));
                let raw_i = staging.join(format!("{:02}.raw.wav", i + 1));
                tools::write_atomic(&mid_i, &midi_bytes(scene, passes, Some(i))?)?;
                // sfizz_render has no gain flag (unlike fluidsynth -g / timidity
                // -A), so gain is applied here in-process instead — on each
                // track individually, so stems and the mixed-down full track
                // carry the same gain and still sum correctly.
                tools::render_sfz(&mid_i, &sfz, &raw_i, args.sample_rate)?;
                let raw_i_gain = staging.join(format!("{:02}.gain.wav", i + 1));
                audio::mix(std::slice::from_ref(&raw_i), &raw_i_gain, args.gain)?;
                track_raws.push(raw_i_gain);
            }
            // Gain is already baked into each track above; mix at unity here.
            audio::mix(&track_raws, &raw, 1.0)?;
            produce(&raw, output, ext, args.quality, window, &mut cleanup)?;

            if args.stems {
                let stems_dir = output.with_extension("stems");
                let stems_staging =
                    output.with_extension(format!("stems.tmp-{}", std::process::id()));
                std::fs::create_dir_all(&stems_staging).map_err(|e| Error::Io {
                    path: stems_staging.display().to_string(),
                    source: e,
                })?;
                cleanup.dirs.push(stems_staging.clone());
                for (i, track) in scene.tracks.iter().enumerate() {
                    let name = format!("{:02}-{}.{ext}", i + 1, instrument_name(track));
                    produce(
                        &track_raws[i],
                        &stems_staging.join(&name),
                        ext,
                        args.quality,
                        window,
                        &mut cleanup,
                    )?;
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
                std::fs::rename(&stems_staging, &stems_dir).map_err(|e| Error::Io {
                    path: stems_dir.display().to_string(),
                    source: e,
                })?;
                cleanup.dirs.retain(|d| d != &stems_staging);
            }
        }
        Backend::Sf2 { soundfont } => {
            // Full mix: render, cut sample-exactly in-process, then encode if needed.
            tools::write_atomic(&mid, &midi_bytes(scene, passes, None)?)?;
            tools::render(
                args.renderer,
                &mid,
                soundfont,
                &raw,
                args.sample_rate,
                args.gain,
            )?;
            produce(&raw, output, ext, args.quality, window, &mut cleanup)?;

            // Stems: staged in a temp dir, swapped in only when every track rendered.
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
                        soundfont,
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
        }
    }

    // Machine-readable metadata for game engines and agent pipelines.
    let audio_name = output
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(json!({
        "title": scene.title,
        "story": scene.story,
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
