//! External tool orchestration (FluidSynth, FFmpeg) and atomic file output.
//! Hard rule: on failure nothing half-written may remain at the output path.

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Watchdog limits for one external tool run. Renderers have misbehaved in
/// the wild (`sfizz_render` without `--use-eot` keeps rendering until output
/// power decays below 1e-12 — a non-decaying patch writes an unbounded WAV),
/// so every run is bounded by wall-clock time and output size.
///
/// Env overrides (both optional): `SCOREKIT_TOOL_TIMEOUT_SECS` and
/// `SCOREKIT_TOOL_MAX_OUTPUT_MB` replace the computed values when set.
#[derive(Debug, Clone, Copy)]
pub struct ToolLimits {
    pub timeout: Duration,
    pub max_output_bytes: u64,
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok()?.trim().parse().ok()
}

impl ToolLimits {
    /// Limits for a render whose audio duration is known in advance:
    /// generous multiples so slow disks and large sample libraries never
    /// trip the guard, while a runaway tool is still stopped early.
    pub fn for_expected_audio(expected_secs: f64, sample_rate: u32) -> Self {
        let secs = expected_secs.max(0.0);
        // Stereo 16-bit PCM upper bound for the expected duration.
        let expected_bytes = (secs * f64::from(sample_rate)).ceil() as u64 * 4;
        let max_output_bytes = env_u64("SCOREKIT_TOOL_MAX_OUTPUT_MB")
            .map(|mb| mb.saturating_mul(1024 * 1024))
            .unwrap_or_else(|| expected_bytes.saturating_mul(10) + 64 * 1024 * 1024);
        let timeout_secs =
            env_u64("SCOREKIT_TOOL_TIMEOUT_SECS").unwrap_or(300 + (secs * 30.0).ceil() as u64);
        Self {
            timeout: Duration::from_secs(timeout_secs),
            max_output_bytes,
        }
    }

    /// Fallback limits for tools whose output size is not known in advance
    /// (SF2 renderers, FFmpeg): long timeout, large size cap.
    pub fn generic() -> Self {
        let timeout_secs = env_u64("SCOREKIT_TOOL_TIMEOUT_SECS").unwrap_or(1800);
        let max_output_bytes = env_u64("SCOREKIT_TOOL_MAX_OUTPUT_MB")
            .map(|mb| mb.saturating_mul(1024 * 1024))
            .unwrap_or(8 * 1024 * 1024 * 1024);
        Self {
            timeout: Duration::from_secs(timeout_secs),
            max_output_bytes,
        }
    }
}

/// Which external synthesizer turns MIDI + SF2 into PCM. The rest of the
/// pipeline (loop-seal surgery, stems, export) is renderer-agnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Renderer {
    Fluidsynth,
    Timidity,
    /// SFZ sampler (`sfizz_render`), driven by a renderer profile instead of
    /// a single SF2 file — see `profile::Profile`. Renders one instrument
    /// per pass; the pipeline mixes per-track renders into the full mix.
    Sfizz,
}

/// Sibling temp path that keeps the target extension (needed for type sniffing).
fn tmp_sibling(output: &Path) -> PathBuf {
    let stem = output.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    let ext = output.extension().and_then(|s| s.to_str()).unwrap_or("tmp");
    output.with_file_name(format!("{stem}.tmp-{}.{ext}", std::process::id()))
}

fn io_err(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.display().to_string(),
        source,
    }
}

fn ensure_parent(output: &Path) -> Result<()> {
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }
    Ok(())
}

/// Write bytes atomically: temp sibling + rename.
pub fn write_atomic(output: &Path, bytes: &[u8]) -> Result<()> {
    ensure_parent(output)?;
    let tmp = tmp_sibling(output);
    std::fs::write(&tmp, bytes).map_err(|e| io_err(&tmp, e))?;
    std::fs::rename(&tmp, output).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        io_err(output, e)
    })
}

/// Require an input file to exist; `arg` names the CLI argument for the error.
pub fn require_file(path: &Path, arg: &str) -> Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        Err(Error::Validation {
            path: arg.to_owned(),
            message: format!("file not found: {}", path.display()),
        })
    }
}

fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n")
}

/// Captured output from a successful external tool invocation. Normal build
/// paths discard it; diagnostic commands inspect it for warnings.
#[derive(Debug, Clone)]
pub struct ToolDiagnostics {
    pub stdout: String,
    pub stderr: String,
}

/// Run a tool that writes `output`; the tool receives a temp path which is
/// atomically renamed on success and removed on any failure. `error_markers`
/// catches tools (FluidSynth) that report fatal errors on stderr yet exit 0.
/// A watchdog kills the tool when it exceeds `limits` (wall-clock time or
/// bytes written to the temp file) so a runaway render cannot fill the disk.
fn run_to_file_capture(
    tool: &str,
    hint: &str,
    error_markers: &[&str],
    build_args: impl FnOnce(&Path) -> Vec<std::ffi::OsString>,
    output: &Path,
    limits: ToolLimits,
) -> Result<ToolDiagnostics> {
    ensure_parent(output)?;
    let tmp = tmp_sibling(output);
    let args = build_args(&tmp);
    let cleanup = || {
        let _ = std::fs::remove_file(&tmp);
    };
    let spawned = Command::new(tool)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            cleanup();
            return Err(Error::MissingDependency {
                tool: tool.to_owned(),
                hint: hint.to_owned(),
            });
        }
        Err(e) => {
            cleanup();
            return Err(io_err(Path::new(tool), e));
        }
        Ok(child) => child,
    };
    // Drain pipes on threads so a chatty tool cannot deadlock on a full pipe
    // while the main thread polls the watchdog. Results come back over
    // channels: when the tool is killed, surviving grandchildren may hold the
    // pipe open indefinitely, so the kill path only waits briefly for the
    // drained bytes instead of joining the reader threads.
    let read_all = |r: Option<Box<dyn std::io::Read + Send>>| {
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut r) = r {
                let _ = std::io::Read::read_to_end(&mut r, &mut buf);
            }
            let _ = tx.send(buf);
        });
        rx
    };
    let stdout_rx = read_all(
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    );
    let stderr_rx = read_all(
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    );
    let started = Instant::now();
    let (status, killed) = loop {
        match child.try_wait() {
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                cleanup();
                return Err(io_err(Path::new(tool), e));
            }
            Ok(Some(status)) => break (status, None),
            Ok(None) => {}
        }
        let breach = if started.elapsed() > limits.timeout {
            Some(format!(
                "killed: no result within {}s (override with SCOREKIT_TOOL_TIMEOUT_SECS)",
                limits.timeout.as_secs()
            ))
        } else {
            let written = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
            (written > limits.max_output_bytes).then(|| {
                format!(
                    "killed: output exceeded {} MiB cap (override with SCOREKIT_TOOL_MAX_OUTPUT_MB)",
                    limits.max_output_bytes / (1024 * 1024)
                )
            })
        };
        if let Some(reason) = breach {
            let _ = child.kill();
            let status = child.wait().map_err(|e| io_err(Path::new(tool), e))?;
            break (status, Some(reason));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let drain = |rx: std::sync::mpsc::Receiver<Vec<u8>>, killed: bool| {
        if killed {
            rx.recv_timeout(Duration::from_millis(500))
                .unwrap_or_default()
        } else {
            rx.recv().unwrap_or_default()
        }
    };
    let stdout = drain(stdout_rx, killed.is_some());
    let stderr = drain(stderr_rx, killed.is_some());
    let stderr_full = String::from_utf8_lossy(&stderr).into_owned();
    let stderr_tail = tail(&stderr_full, 8);
    if let Some(reason) = killed {
        cleanup();
        return Err(Error::ToolFailure {
            tool: tool.to_owned(),
            status: reason,
            stderr: stderr_tail,
        });
    }
    if !status.success() {
        cleanup();
        return Err(Error::ToolFailure {
            tool: tool.to_owned(),
            status: status.to_string(),
            stderr: stderr_tail,
        });
    }
    if let Some(marker) = error_markers.iter().find(|m| stderr_full.contains(**m)) {
        cleanup();
        return Err(Error::ToolFailure {
            tool: tool.to_owned(),
            status: format!("exit 0 but stderr matched `{marker}`"),
            stderr: stderr_tail,
        });
    }
    // Guard against tools that exit 0 without producing usable output.
    let produced = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
    if produced == 0 {
        cleanup();
        return Err(Error::ToolFailure {
            tool: tool.to_owned(),
            status: "exit 0 but empty output".to_owned(),
            stderr: stderr_tail,
        });
    }
    std::fs::rename(&tmp, output).map_err(|e| {
        cleanup();
        io_err(output, e)
    })?;
    Ok(ToolDiagnostics {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: stderr_full,
    })
}

fn run_to_file(
    tool: &str,
    hint: &str,
    error_markers: &[&str],
    build_args: impl FnOnce(&Path) -> Vec<std::ffi::OsString>,
    output: &Path,
) -> Result<()> {
    run_to_file_capture(
        tool,
        hint,
        error_markers,
        build_args,
        output,
        ToolLimits::generic(),
    )
    .map(|_| ())
}

/// Cheap structural check: an SF2 is a RIFF container with the `sfbk` form type.
fn require_sf2(path: &Path) -> Result<()> {
    require_file(path, "--soundfont")?;
    let mut head = [0u8; 12];
    let ok = std::fs::File::open(path)
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut head))
        .is_ok()
        && &head[0..4] == b"RIFF"
        && &head[8..12] == b"sfbk";
    if ok {
        Ok(())
    } else {
        Err(Error::Validation {
            path: "--soundfont".to_owned(),
            message: format!("not a SoundFont (SF2) file: {}", path.display()),
        })
    }
}

/// Render a MIDI file to WAV via the selected renderer backend.
///
/// Both backends share the SF2 preflight and the same "exit 0 but broken"
/// defences; the loop-seal math downstream is renderer-agnostic.
pub fn render(
    renderer: Renderer,
    midi: &Path,
    soundfont: &Path,
    output: &Path,
    sample_rate: u32,
    gain: f32,
) -> Result<()> {
    require_file(midi, "midi")?;
    if renderer == Renderer::Sfizz {
        return Err(Error::Validation {
            path: "--renderer".to_owned(),
            message: "renderer `sfizz` is driven by --profile via `render_sfz`, not `render`"
                .to_owned(),
        });
    }
    require_sf2(soundfont)?;
    let result = match renderer {
        Renderer::Fluidsynth => run_to_file(
            "fluidsynth",
            "install FluidSynth (e.g. `brew install fluid-synth` or `apt install fluidsynth`)",
            // FluidSynth exits 0 even when the SoundFont fails to load and the
            // render is silent; treat its error lines as failures.
            &["fluidsynth: error:", "Failed to load the SoundFont"],
            |tmp| {
                vec![
                    "-ni".into(),
                    "-q".into(),
                    "-g".into(),
                    gain.to_string().into(),
                    "-r".into(),
                    sample_rate.to_string().into(),
                    "-F".into(),
                    tmp.as_os_str().to_owned(),
                    soundfont.as_os_str().to_owned(),
                    midi.as_os_str().to_owned(),
                ]
            },
            output,
        ),
        Renderer::Timidity => {
            // TiMidity++ exits 0 and silently falls back to its built-in
            // patches when the SoundFont cannot be read — worse than silence:
            // wrong timbres. `-idq` keeps its `***` error lines on stderr
            // (`-idqq` would suppress them) while staying quiet on success.
            let cfg = format!("soundfont \"{}\"", soundfont.display());
            // TiMidity's -A amplification is 0..=800 (%), default 70.
            let amp = (gain * 100.0).round().clamp(0.0, 800.0) as u32;
            run_to_file(
                "timidity",
                "install TiMidity++ (e.g. `brew install timidity` or `apt install timidity`)",
                &[
                    "*** not a RIFF file",
                    "*** illegal",
                    "Can't open soundfont file",
                    "No instrument mapped",
                ],
                |tmp| {
                    vec![
                        "-x".into(),
                        cfg.into(),
                        "-idq".into(),
                        format!("-A{amp}").into(),
                        "-s".into(),
                        sample_rate.to_string().into(),
                        "-Ow".into(),
                        "-o".into(),
                        tmp.as_os_str().to_owned(),
                        midi.as_os_str().to_owned(),
                    ]
                },
                output,
            )
        }
        Renderer::Sfizz => unreachable!("returned early above"),
    };
    result?;
    // Renderer-agnostic backstop: TiMidity writes a header-only WAV (zero
    // frames) for some corrupt SoundFonts while exiting 0 with a clean stderr.
    let frames = hound::WavReader::open(output)
        .map(|r| r.duration())
        .unwrap_or(0);
    if frames == 0 {
        let _ = std::fs::remove_file(output);
        return Err(Error::ToolFailure {
            tool: renderer_tool(renderer).to_owned(),
            status: "exit 0 but zero audio frames".to_owned(),
            stderr: String::new(),
        });
    }
    Ok(())
}

fn renderer_tool(renderer: Renderer) -> &'static str {
    match renderer {
        Renderer::Fluidsynth => "fluidsynth",
        Renderer::Timidity => "timidity",
        Renderer::Sfizz => "sfizz_render",
    }
}

/// Render a MIDI file through a single SFZ instrument via `sfizz_render`.
/// Unlike `render` (one SF2 covers every General MIDI program), `sfizz_render`
/// plays every note through one loaded instrument — there's no bank/program
/// concept. A scene with several instruments is therefore rendered one track
/// at a time and mixed in-process (`audio::mix`); see `pipeline::build_one`.
///
/// `--use-eot` stops the render at the MIDI EndOfTrack message. Without it,
/// sfizz_render keeps rendering until output power decays below 1e-12, so a
/// patch that never decays (looping organ/string sustains) writes an
/// unbounded WAV. Callers pad the MIDI end past the last note-off when they
/// need a release tail. `limits` bounds the run as defence in depth.
pub fn render_sfz_with_diagnostics(
    midi: &Path,
    sfz: &Path,
    output: &Path,
    sample_rate: u32,
    limits: ToolLimits,
) -> Result<ToolDiagnostics> {
    require_file(midi, "midi")?;
    require_file(sfz, "--profile")?;
    let diagnostics = run_to_file_capture(
        "sfizz_render",
        "install `sfizz_render` (Homebrew: `brew install talkincode/tap/scorekit-sfizz`; source build: https://github.com/sfztools/sfizz with `-DSFIZZ_RENDER=ON -DSFIZZ_JACK=OFF -DSFIZZ_TESTS=OFF`)",
        &[],
        |tmp| {
            vec![
                "--sfz".into(),
                sfz.as_os_str().to_owned(),
                "--midi".into(),
                midi.as_os_str().to_owned(),
                "--wav".into(),
                tmp.as_os_str().to_owned(),
                "-s".into(),
                sample_rate.to_string().into(),
                "--use-eot".into(),
            ]
        },
        output,
        limits,
    )?;
    // Same "exit 0 but zero audio frames" backstop `render` applies below,
    // duplicated here because this path never reaches that shared check.
    let frames = hound::WavReader::open(output)
        .map(|r| r.duration())
        .unwrap_or(0);
    if frames == 0 {
        let _ = std::fs::remove_file(output);
        return Err(Error::ToolFailure {
            tool: "sfizz_render".to_owned(),
            status: "exit 0 but zero audio frames".to_owned(),
            stderr: String::new(),
        });
    }
    Ok(diagnostics)
}

pub fn render_sfz(
    midi: &Path,
    sfz: &Path,
    output: &Path,
    sample_rate: u32,
    limits: ToolLimits,
) -> Result<()> {
    render_sfz_with_diagnostics(midi, sfz, output, sample_rate, limits).map(|_| ())
}

/// Convert audio via FFmpeg. The codec follows the output extension:
/// `.wav` → PCM s16, anything else → OGG/Vorbis, preferring `libvorbis` and
/// falling back to the built-in `vorbis` encoder when the local FFmpeg build
/// lacks it. Sample-exact cutting lives in `audio::extract`, not here.
pub fn export(input: &Path, output: &Path, quality: u8) -> Result<()> {
    require_file(input, "input")?;
    let wav_out = output
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"));
    let attempts: &[(&str, &[&str])] = if wav_out {
        &[("pcm_s16le", &[])]
    } else {
        &[
            ("libvorbis", &["-q:a"]),
            ("vorbis", &["-strict", "experimental", "-q:a"]),
        ]
    };
    let mut last: Option<Error> = None;
    for (codec, extra) in attempts {
        let result = run_to_file(
            "ffmpeg",
            "install FFmpeg (e.g. `brew install ffmpeg` or `apt install ffmpeg`)",
            &[],
            |tmp| {
                let mut args: Vec<std::ffi::OsString> = vec![
                    "-hide_banner".into(),
                    "-loglevel".into(),
                    "error".into(),
                    "-y".into(),
                    "-i".into(),
                    input.as_os_str().to_owned(),
                ];
                args.push("-c:a".into());
                args.push((*codec).into());
                for a in *extra {
                    args.push((*a).into());
                    if *a == "-q:a" {
                        args.push(quality.to_string().into());
                    }
                }
                args.push(tmp.as_os_str().to_owned());
                args
            },
            output,
        );
        match result {
            Err(Error::ToolFailure { ref stderr, .. }) if stderr.contains("Unknown encoder") => {
                last = result.err();
            }
            other => return other,
        }
    }
    Err(last.expect("at least one attempt ran"))
}

/// Normalize an arbitrary audio recording to the pipeline's deterministic
/// interchange format. Resampling/channel conversion stays delegated to
/// FFmpeg; scorekit only performs sample-exact placement after this step.
pub fn normalize_texture(input: &Path, output: &Path, sample_rate: u32) -> Result<()> {
    require_file(input, "--texture-profile")?;
    run_to_file(
        "ffmpeg",
        "install FFmpeg (e.g. `brew install ffmpeg` or `apt install ffmpeg`)",
        &[],
        |tmp| {
            vec![
                "-hide_banner".into(),
                "-loglevel".into(),
                "error".into(),
                "-y".into(),
                "-i".into(),
                input.as_os_str().to_owned(),
                "-vn".into(),
                "-ac".into(),
                "2".into(),
                "-ar".into(),
                sample_rate.to_string().into(),
                "-c:a".into(),
                "pcm_s16le".into(),
                tmp.as_os_str().to_owned(),
            ]
        },
        output,
    )?;
    let reader = hound::WavReader::open(output).map_err(|e| Error::Validation {
        path: output.display().to_string(),
        message: format!("FFmpeg produced an unusable texture WAV: {e}"),
    })?;
    let spec = reader.spec();
    if reader.duration() == 0
        || spec.channels != 2
        || spec.sample_rate != sample_rate
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        let _ = std::fs::remove_file(output);
        return Err(Error::ToolFailure {
            tool: "ffmpeg".to_owned(),
            status: "texture normalization produced an invalid WAV".to_owned(),
            stderr: String::new(),
        });
    }
    Ok(())
}
