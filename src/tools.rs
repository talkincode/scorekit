//! External tool orchestration (FluidSynth, FFmpeg) and atomic file output.
//! Hard rule: on failure nothing half-written may remain at the output path.

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

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

/// Write bytes atomically: temp sibling + rename.
pub fn write_atomic(output: &Path, bytes: &[u8]) -> Result<()> {
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

/// Run a tool that writes `output`; the tool receives a temp path which is
/// atomically renamed on success and removed on any failure. `error_markers`
/// catches tools (FluidSynth) that report fatal errors on stderr yet exit 0.
fn run_to_file(
    tool: &str,
    hint: &str,
    error_markers: &[&str],
    build_args: impl FnOnce(&Path) -> Vec<std::ffi::OsString>,
    output: &Path,
) -> Result<()> {
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }
    let tmp = tmp_sibling(output);
    let args = build_args(&tmp);
    let result = Command::new(tool).args(&args).output();
    let cleanup = || {
        let _ = std::fs::remove_file(&tmp);
    };
    let out = match result {
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
        Ok(out) => out,
    };
    let stderr_full = String::from_utf8_lossy(&out.stderr).into_owned();
    let stderr_tail = tail(&stderr_full, 8);
    if !out.status.success() {
        cleanup();
        return Err(Error::ToolFailure {
            tool: tool.to_owned(),
            status: out.status.to_string(),
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
    })
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

/// Render a MIDI file to WAV via FluidSynth.
pub fn render(
    midi: &Path,
    soundfont: &Path,
    output: &Path,
    sample_rate: u32,
    gain: f32,
) -> Result<()> {
    require_file(midi, "midi")?;
    require_sf2(soundfont)?;
    run_to_file(
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
    )
}

/// Convert audio (WAV → OGG/Vorbis) via FFmpeg.
/// Prefers `libvorbis`; falls back to the built-in `vorbis` encoder when the
/// local FFmpeg build lacks it (quality is lower but the pipeline stays usable).
pub fn export(input: &Path, output: &Path, quality: u8) -> Result<()> {
    require_file(input, "input")?;
    let attempts: [(&str, &[&str]); 2] =
        [("libvorbis", &[]), ("vorbis", &["-strict", "experimental"])];
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
                    "-c:a".into(),
                    codec.into(),
                ];
                for a in extra {
                    args.push((*a).into());
                }
                args.push("-q:a".into());
                args.push(quality.to_string().into());
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
