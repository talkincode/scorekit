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
    /// Portable texture-source profile. Required only when the scene declares
    /// `textures`; independent of the selected musical renderer.
    pub texture_profile: Option<&'a Path>,
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

fn path_io(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.display().to_string(),
        source,
    }
}

fn create_work_dir(parent: &Path, label: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(parent).map_err(|source| path_io(parent, source))?;
    for attempt in 0..1_000u16 {
        let candidate = parent.join(format!(".{label}-{}-{attempt}", std::process::id()));
        match std::fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(path_io(&candidate, source)),
        }
    }
    Err(path_io(
        parent,
        std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("could not allocate a unique {label} directory"),
        ),
    ))
}

fn remove_path(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
            std::fs::remove_dir_all(path)
        }
        Ok(_) => std::fs::remove_file(path),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(source),
    }
}

/// Publish a complete suite staging directory as one recoverable operation.
/// Existing destinations are first moved into a same-filesystem backup. Any
/// rename failure removes newly published entries and restores every backup,
/// so a failed command never exposes a half-old/half-new suite.
fn publish_staged_suite(staging: &Path, destination_dir: &Path, label: &str) -> Result<()> {
    let mut sources = std::fs::read_dir(staging)
        .map_err(|source| path_io(staging, source))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|source| path_io(staging, source))?;
    sources.sort();
    let backup = create_work_dir(destination_dir, &format!("{label}.suite.backup"))?;
    let mut backups: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut published: Vec<PathBuf> = Vec::new();

    let publish_result = (|| -> Result<()> {
        for source in sources {
            let name = source.file_name().ok_or_else(|| {
                validation(
                    "--output",
                    format!("staged artifact has no file name: {}", source.display()),
                )
            })?;
            let destination = destination_dir.join(name);
            if std::fs::symlink_metadata(&destination).is_ok() {
                let saved = backup.join(name);
                std::fs::rename(&destination, &saved)
                    .map_err(|source| path_io(&destination, source))?;
                backups.push((saved, destination.clone()));
            }
            std::fs::rename(&source, &destination)
                .map_err(|source| path_io(&destination, source))?;
            published.push(destination);
        }
        Ok(())
    })();

    if let Err(error) = publish_result {
        for destination in published.iter().rev() {
            let _ = remove_path(destination);
        }
        for (saved, destination) in backups.iter().rev() {
            let _ = std::fs::rename(saved, destination);
        }
        let _ = std::fs::remove_dir_all(&backup);
        return Err(error);
    }

    // Old complete artifacts are no longer needed. Public outputs are already
    // fully committed; cleanup residue is private and best-effort on Drop.
    let _ = std::fs::remove_dir_all(&backup);
    Ok(())
}

/// Batch: render many scenes in one run; per-scene failures don't abort the
/// run, they land in a machine-readable report instead.
pub struct BatchArgs<'a> {
    pub scenes: &'a [PathBuf],
    pub soundfont: Option<&'a Path>,
    pub profile: Option<&'a Path>,
    pub texture_profile: Option<&'a Path>,
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
            texture_profile: args.texture_profile,
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

fn beat_frame(beat: f64, tempo: u16, sample_rate: u32) -> u64 {
    let ticks = (beat * f64::from(crate::composer::PPQ)).round() as u32;
    midi::exact_samples(ticks, tempo, sample_rate)
}

/// Resolve, normalize and arrange every texture source before music render.
/// All intermediates live in one cleanup-scoped staging directory, so a bad
/// mapping or recording cannot leave a partial public artifact.
fn render_texture_tracks(
    args: &BuildArgs<'_>,
    scene: &Scene,
    output: &Path,
    passes: u8,
    pass_frames: u64,
    total_raw_frames: u64,
    cleanup: &mut Cleanup,
) -> Result<Vec<PathBuf>> {
    if scene.textures.is_empty() {
        return Ok(Vec::new());
    }
    let profile_path = args.texture_profile.ok_or_else(|| {
        validation(
            "--texture-profile",
            "scene declares `textures`; pass --texture-profile to map portable source names to audio files"
                .to_owned(),
        )
    })?;
    let profile = crate::texture::load_profile(profile_path)?;
    let profile_dir = profile_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let staging = output.with_extension(format!("textures.tmp-{}", std::process::id()));
    std::fs::create_dir_all(&staging).map_err(|source| Error::Io {
        path: staging.display().to_string(),
        source,
    })?;
    cleanup.dirs.push(staging.clone());

    let mut normalized = std::collections::BTreeMap::<String, PathBuf>::new();
    let mut arranged = Vec::with_capacity(scene.textures.len());
    for (i, texture) in scene.textures.iter().enumerate() {
        let normalized_path = if let Some(path) = normalized.get(&texture.source) {
            path.clone()
        } else {
            let source = profile.resolve(&profile_dir, &texture.source)?;
            if !source.is_file() {
                return Err(validation(
                    &format!("texture_profile.sources.{}", texture.source),
                    format!("texture source file does not exist: {}", source.display()),
                ));
            }
            let path = staging.join(format!("source-{}.wav", texture.source));
            tools::normalize_texture(&source, &path, args.sample_rate)?;
            normalized.insert(texture.source.clone(), path.clone());
            path
        };
        if texture.mode == crate::schema::TextureMode::OneShot
            && passes > 1
            && audio::frames(&normalized_path)? > pass_frames
        {
            return Err(validation(
                &format!("textures[{i}].source"),
                format!(
                    "one-shot source `{}` is longer than one scene loop; use a shorter recording",
                    texture.source
                ),
            ));
        }
        let raw = staging.join(format!("{:02}-{}.raw.wav", i + 1, texture.source));
        let start = beat_frame(
            texture.start_beat.unwrap_or(0.0),
            scene.tempo,
            args.sample_rate,
        );
        let triggers: Vec<u64> = texture
            .at
            .iter()
            .map(|&beat| beat_frame(beat, scene.tempo, args.sample_rate))
            .collect();
        audio::arrange_texture(
            &normalized_path,
            &raw,
            audio::TextureArrangement {
                mode: texture.mode,
                start_frame: start,
                trigger_frames: &triggers,
                pass_frames,
                passes,
                total_frames: total_raw_frames,
                gain: texture.gain,
            },
        )?;
        arranged.push(raw);
    }
    Ok(arranged)
}

/// Cut `window` out of `raw` and deliver it at `output`: a direct
/// sample-exact WAV, or an intermediate cut encoded to OGG. With
/// `keep_cut`, the intermediate `.cut.wav` survives for the caller (suite
/// main-file assembly) instead of being cleaned up here; for WAV output the
/// output itself is the cut, so the flag is a no-op.
fn produce(
    raw: &Path,
    output: &Path,
    ext: &str,
    quality: u8,
    window: audio::Window,
    cleanup: &mut Cleanup,
    keep_cut: bool,
) -> Result<()> {
    if ext == "wav" {
        audio::extract(raw, output, window)
    } else {
        let cut = output.with_extension("cut.wav");
        if !keep_cut {
            cleanup.files.push(cut.clone());
        }
        audio::extract(raw, &cut, window)?;
        tools::export(&cut, output, quality)?;
        // Remove eagerly: stem cuts live in the staging dir, which is renamed
        // into place before the deferred Cleanup runs — the recorded path
        // would go stale and the cut would ship inside the stems folder.
        if !cleanup.keep && !keep_cut {
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
        let entry = build_one(args, &scene, args.output, &ext, false)?;
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

    // Suite: one asset per section, all sharing tracks, motifs and key —
    // plus the main playback file: every section concatenated in declaration
    // order at `--output` itself, so one `build` always yields an asset under
    // the requested path regardless of scene mode.
    let stem = args
        .output
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let destination_dir = args
        .output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let output_name = args.output.file_name().ok_or_else(|| {
        validation(
            "--output",
            format!("output has no file name: {}", args.output.display()),
        )
    })?;
    let staging = create_work_dir(destination_dir, &format!("{stem}.suite.tmp"))?;
    let staged_output = staging.join(output_name);
    let mut cleanup = Cleanup {
        files: Vec::new(),
        dirs: vec![staging.clone()],
        keep: false,
    };
    let mut entries = Vec::new();
    let mut names = Vec::new();
    let mut cuts: Vec<PathBuf> = Vec::new();
    for section in &scene.sections {
        let derived = scene.for_section(section);
        let output = staged_output.with_file_name(format!("{stem}-{}.{ext}", section.name));
        // WAV sections are already sample-exact cuts; OGG sections keep
        // their pre-encode `.cut.wav` inside the private suite staging
        // directory until the main concatenation has finished.
        let cut = if ext == "wav" {
            output.clone()
        } else {
            output.with_extension("cut.wav")
        };
        let mut entry = build_one(args, &derived, &output, &ext, ext != "wav")?;
        entry["name"] = json!(section.name);
        entries.push(entry);
        names.push(section.name.clone());
        cuts.push(cut);
    }

    // Main playback file: sample-exact concatenation of the section cuts.
    if ext == "wav" {
        audio::concat(&cuts, &staged_output)?;
    } else {
        let main_cut = staged_output.with_extension("cut.wav");
        audio::concat(&cuts, &main_cut)?;
        tools::export(&main_cut, &staged_output, args.quality)?;
        for cut in &cuts {
            std::fs::remove_file(cut).map_err(|source| path_io(cut, source))?;
        }
        std::fs::remove_file(&main_cut).map_err(|source| path_io(&main_cut, source))?;
    }

    let total_samples: u64 = entries
        .iter()
        .map(|entry| entry["total_samples"].as_u64().unwrap_or(0))
        .sum();
    let manifest = json!({
        "title": scene.title,
        "story": scene.story,
        "suite": true,
        "tempo": scene.tempo,
        "key": scene.key,
        "time_signature": scene.time_signature,
        "sample_rate": args.sample_rate,
        "audio": args.output.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        "loop": false,
        "total_samples": total_samples,
        "seconds": total_samples as f64 / f64::from(args.sample_rate),
        "sections": entries,
    });
    let meta_bytes = serde_json::to_vec_pretty(&manifest).expect("manifest serializes");
    let staged_meta = staged_output.with_extension("meta.json");
    tools::write_atomic(&staged_meta, &meta_bytes)?;
    publish_staged_suite(&staging, destination_dir, &stem)?;
    std::fs::remove_dir(&staging).map_err(|source| path_io(&staging, source))?;
    cleanup.dirs.clear();
    Ok(format!(
        "wrote {} ({} samples) from {} section(s) [{}], {}",
        args.output.display(),
        total_samples,
        names.len(),
        names.join(", "),
        meta_path.display(),
    ))
}

/// Compile one scene into one audio asset (+ optional stems) at `output` and
/// return its metadata entry. Does not write the meta.json file. With
/// `keep_cut`, the full-mix pre-encode cut WAV survives next to `output`
/// for suite main-file assembly (see `produce`).
fn build_one(
    args: &BuildArgs,
    scene: &Scene,
    output: &Path,
    ext: &str,
    keep_cut: bool,
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
    let total_raw_frames = window.skip + window.take;

    let mid = output.with_extension("mid");
    let raw = output.with_extension("raw.wav");
    let mut cleanup = Cleanup {
        files: vec![mid.clone(), raw.clone()],
        dirs: Vec::new(),
        keep: args.keep_intermediates,
    };
    let mut stem_rel: Vec<String> = Vec::new();
    let texture_raws = render_texture_tracks(
        args,
        scene,
        output,
        passes,
        loop_samples,
        total_raw_frames,
        &mut cleanup,
    )?;
    let stems_dir = output.with_extension("stems");
    let stems_staging = if args.stems {
        let staging = output.with_extension(format!("stems.tmp-{}", std::process::id()));
        std::fs::create_dir_all(&staging).map_err(|source| Error::Io {
            path: staging.display().to_string(),
            source,
        })?;
        cleanup.dirs.push(staging.clone());
        Some(staging)
    } else {
        None
    };

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
            if args.stems {
                let stems_staging = stems_staging.as_ref().expect("created when --stems");
                for (i, track) in scene.tracks.iter().enumerate() {
                    let name = format!("{:02}-{}.{ext}", i + 1, instrument_name(track));
                    produce(
                        &track_raws[i],
                        &stems_staging.join(&name),
                        ext,
                        args.quality,
                        window,
                        &mut cleanup,
                        false,
                    )?;
                    let dir_name = stems_dir
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    stem_rel.push(format!("{dir_name}/{name}"));
                }
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
            // Stems: staged in a temp dir, swapped in only when every track rendered.
            if args.stems {
                let staging = stems_staging.as_ref().expect("created when --stems");
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
                        false,
                    )?;
                    let _ = std::fs::remove_file(&mid_i);
                    let _ = std::fs::remove_file(&raw_i);
                    let dir_name = stems_dir
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    stem_rel.push(format!("{dir_name}/{name}"));
                }
            }
        }
    }

    let full_raw = if texture_raws.is_empty() {
        raw.clone()
    } else {
        let combined = output.with_extension("combined.raw.wav");
        cleanup.files.push(combined.clone());
        let mut inputs = Vec::with_capacity(texture_raws.len() + 1);
        inputs.push(raw.clone());
        inputs.extend(texture_raws.iter().cloned());
        audio::mix(&inputs, &combined, 1.0)?;
        combined
    };
    produce(
        &full_raw,
        output,
        ext,
        args.quality,
        window,
        &mut cleanup,
        keep_cut,
    )?;

    if let Some(staging) = &stems_staging {
        for (i, (texture, texture_raw)) in scene.textures.iter().zip(&texture_raws).enumerate() {
            let name = format!(
                "{:02}-texture-{}.{ext}",
                scene.tracks.len() + i + 1,
                texture.source
            );
            produce(
                texture_raw,
                &staging.join(&name),
                ext,
                args.quality,
                window,
                &mut cleanup,
                false,
            )?;
            let dir_name = stems_dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            stem_rel.push(format!("{dir_name}/{name}"));
        }
        if stems_dir.exists() {
            std::fs::remove_dir_all(&stems_dir).map_err(|source| Error::Io {
                path: stems_dir.display().to_string(),
                source,
            })?;
        }
        std::fs::rename(staging, &stems_dir).map_err(|source| Error::Io {
            path: stems_dir.display().to_string(),
            source,
        })?;
        cleanup.dirs.retain(|dir| dir != staging);
    }

    // Machine-readable metadata for game engines and agent pipelines.
    let audio_name = output
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut metadata = json!({
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
    });
    if !scene.textures.is_empty() {
        metadata["textures"] = json!(
            scene
                .textures
                .iter()
                .map(|texture| json!({
                    "source": texture.source,
                    "mode": texture.mode,
                    "start_beat": texture.start_beat,
                    "at": texture.at,
                    "gain": texture.gain,
                }))
                .collect::<Vec<_>>()
        );
    }
    Ok(metadata)
}
