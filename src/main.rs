mod audio;
mod composer;
mod diff;
mod doctor;
mod error;
mod grammar;
mod mcp;
mod midi;
mod pipeline;
mod profile;
mod profile_check;
mod schema;
mod soundfont;
mod texture;
mod tools;

use clap::{Parser, Subcommand};
use error::Result;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "scorekit",
    version,
    about = "Agent-driven game music compiler: YAML scene -> MIDI -> rendered audio assets",
    after_help = "Exit codes: 0 ok, 1 io, 2 invalid input, 3 missing dependency, 4 external tool failure"
)]
struct Cli {
    /// Emit machine-readable JSON errors on stderr
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check platform support and external audio dependencies
    Doctor,
    /// Validate a scene file
    Validate { scene: PathBuf },
    /// Print the JSON Schema of the scene DSL (or one of its external profiles)
    Schema {
        /// Print the grammar-profile schema instead of the scene schema
        #[arg(long)]
        grammar: bool,
        /// Print the renderer-profile schema (--renderer sfizz) instead of the scene schema
        #[arg(long)]
        profile: bool,
        /// Print the texture-source profile schema instead of the scene schema
        #[arg(long)]
        texture_profile: bool,
    },
    /// Check a scene against an aesthetic grammar profile
    Lint {
        scene: PathBuf,
        /// Grammar profile (YAML); see `scorekit schema --grammar`
        #[arg(long)]
        grammar: PathBuf,
    },
    /// Inspect every SFZ mapping in a renderer profile
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },
    /// Compile a scene to a Standard MIDI File
    Midi {
        scene: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        /// Render the material N times back to back (seamless-loop workflows)
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=8))]
        passes: u8,
        /// Keep only the given track (0-based), preserving its mix channel
        #[arg(long)]
        solo: Option<usize>,
        /// Compile one named section of a suite scene
        #[arg(long)]
        section: Option<String>,
    },
    /// Render a MIDI file to WAV via a synthesizer backend + SoundFont (or `--sfz` for sfizz)
    Render {
        midi: PathBuf,
        /// SF2 SoundFont; defaults to MuseScore_General.sf2 in the scorekit sound library
        #[arg(long)]
        soundfont: Option<PathBuf>,
        /// Single `.sfz` instrument; only valid with --renderer sfizz. For
        /// multi-instrument scenes use `scorekit build --renderer sfizz --profile ...`.
        #[arg(long)]
        sfz: Option<PathBuf>,
        #[arg(short, long)]
        output: PathBuf,
        /// Synthesizer backend
        #[arg(long, value_enum, default_value_t = tools::Renderer::Fluidsynth)]
        renderer: tools::Renderer,
        #[arg(long, default_value_t = 44100)]
        sample_rate: u32,
        #[arg(long, default_value_t = 0.8)]
        gain: f32,
    },
    /// Convert rendered audio via FFmpeg (.ogg → Vorbis, .wav → PCM)
    Export {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        /// Vorbis quality 0..=10
        #[arg(long, default_value_t = 5)]
        quality: u8,
        /// Skip this many samples from the start
        #[arg(long, default_value_t = 0)]
        seek_samples: u64,
        /// Emit exactly this many samples (trims and zero-pads as needed)
        #[arg(long)]
        take_samples: Option<u64>,
    },
    /// Full chain: scene -> MIDI -> WAV -> sample-exact loop/stems + meta.json
    Build {
        scene: PathBuf,
        /// SF2 SoundFont; defaults to MuseScore_General.sf2 unless --renderer sfizz
        #[arg(long)]
        soundfont: Option<PathBuf>,
        /// Renderer profile mapping instruments to `.sfz` files; only valid
        /// with --renderer sfizz. See `scorekit schema --profile`.
        #[arg(long)]
        profile: Option<PathBuf>,
        /// Texture profile mapping portable ambience/SFX source names to audio files
        #[arg(long)]
        texture_profile: Option<PathBuf>,
        /// Output audio file (.ogg or .wav)
        #[arg(short, long)]
        output: PathBuf,
        /// Synthesizer backend
        #[arg(long, value_enum, default_value_t = tools::Renderer::Fluidsynth)]
        renderer: tools::Renderer,
        #[arg(long, default_value_t = 44100)]
        sample_rate: u32,
        #[arg(long, default_value_t = 0.8)]
        gain: f32,
        #[arg(long, default_value_t = 5)]
        quality: u8,
        /// Also render one sample-aligned stem per instrument/texture track
        #[arg(long)]
        stems: bool,
        /// Decay tail in seconds kept after non-loop scenes
        #[arg(long, default_value_t = 4.0)]
        tail: f64,
        /// Loop-seal crossfade in milliseconds (loop scenes only)
        #[arg(long, default_value_t = 50)]
        crossfade_ms: u32,
        /// Keep the intermediate .mid and .raw.wav next to the output
        #[arg(long)]
        keep_intermediates: bool,
    },
    /// Semantic diff of two scene files (musical meaning, not text)
    Diff { old: PathBuf, new: PathBuf },
    /// Serve MCP (Model Context Protocol) over stdio; each tool wraps one CLI command
    Mcp,
    /// Build many scenes into one directory; failures land in a JSON report
    Batch {
        /// Scene files (each becomes `<out-dir>/<scene-stem>.<format>`)
        #[arg(required = true)]
        scenes: Vec<PathBuf>,
        /// SF2 SoundFont; defaults to MuseScore_General.sf2 unless --renderer sfizz
        #[arg(long)]
        soundfont: Option<PathBuf>,
        /// Renderer profile mapping instruments to `.sfz` files; only valid
        /// with --renderer sfizz. See `scorekit schema --profile`.
        #[arg(long)]
        profile: Option<PathBuf>,
        /// Texture profile mapping portable ambience/SFX source names to audio files
        #[arg(long)]
        texture_profile: Option<PathBuf>,
        #[arg(long)]
        out_dir: PathBuf,
        /// Output format for every scene
        #[arg(long, default_value = "ogg", value_parser = ["ogg", "wav"])]
        format: String,
        /// Synthesizer backend
        #[arg(long, value_enum, default_value_t = tools::Renderer::Fluidsynth)]
        renderer: tools::Renderer,
        #[arg(long, default_value_t = 44100)]
        sample_rate: u32,
        #[arg(long, default_value_t = 0.8)]
        gain: f32,
        #[arg(long, default_value_t = 5)]
        quality: u8,
        /// Also render instrument/texture stems for every scene
        #[arg(long)]
        stems: bool,
        #[arg(long, default_value_t = 4.0)]
        tail: f64,
        #[arg(long, default_value_t = 50)]
        crossfade_ms: u32,
        /// Report path (default: `<out-dir>/report.json`)
        #[arg(long)]
        report: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ProfileCommand {
    /// Render probe MIDI through every unique patch and verify determinism
    Check {
        profile: PathBuf,
        #[arg(long, default_value_t = 44100)]
        sample_rate: u32,
    },
}

fn compile_midi(
    scene_path: &Path,
    output: &Path,
    passes: u8,
    solo: Option<usize>,
    section: Option<&str>,
) -> Result<()> {
    let mut scene = schema::load_scene(scene_path)?;
    if let Some(name) = section {
        let found = scene.sections.iter().find(|s| s.name == name).cloned();
        match found {
            Some(s) => scene = scene.for_section(&s),
            None => {
                return Err(error::Error::Validation {
                    path: "--section".to_owned(),
                    message: format!(
                        "unknown section `{name}` (defined: {:?})",
                        scene.sections.iter().map(|s| &s.name).collect::<Vec<_>>()
                    ),
                });
            }
        }
    }
    let bytes = pipeline::midi_bytes(&scene, passes, solo)?;
    tools::write_atomic(output, &bytes)
}

fn run(command: &Command, json: bool) -> Result<String> {
    match command {
        Command::Doctor => {
            let report = doctor::check();
            if !report.ready {
                return Err(error::Error::Doctor {
                    porcelain: report.human().lines().map(str::to_owned).collect(),
                    report: report.to_json(),
                });
            }
            if json {
                Ok(report.to_json().to_string())
            } else {
                Ok(report.human())
            }
        }
        Command::Validate { scene } => {
            let s = schema::load_scene(scene)?;
            let sections = if s.sections.is_empty() {
                String::new()
            } else {
                format!(", {} section(s)", s.sections.len())
            };
            Ok(format!(
                "ok: {} bars, {} track(s), tempo {}{sections}",
                s.bars,
                s.tracks.len(),
                s.tempo
            ))
        }
        Command::Schema {
            grammar,
            profile,
            texture_profile,
        } => Ok(if *grammar {
            grammar::schema_json()
        } else if *profile {
            crate::profile::schema_json()
        } else if *texture_profile {
            crate::texture::schema_json()
        } else {
            schema::schema_json()
        }),
        Command::Lint {
            scene,
            grammar: profile,
        } => {
            let s = schema::load_scene(scene)?;
            let g = grammar::load_grammar(profile)?;
            let violations = grammar::check(&s, &g);
            if violations.is_empty() {
                Ok(format!(
                    "ok: conforms to `{}` ({} rule(s) checked)",
                    g.name,
                    g.rules.active_count()
                ))
            } else {
                Err(error::Error::Lint {
                    grammar: g.name.clone(),
                    count: violations.len(),
                    porcelain: violations
                        .iter()
                        .map(grammar::Violation::porcelain)
                        .collect(),
                    json: violations.iter().map(grammar::Violation::to_json).collect(),
                })
            }
        }
        Command::Profile {
            command:
                ProfileCommand::Check {
                    profile,
                    sample_rate,
                },
        } => {
            let report = profile_check::check(profile, *sample_rate)?;
            if json {
                Ok(report.to_json().to_string())
            } else {
                Ok(report.summary())
            }
        }
        Command::Midi {
            scene,
            output,
            passes,
            solo,
            section,
        } => {
            compile_midi(scene, output, *passes, *solo, section.as_deref())?;
            Ok(format!("wrote {}", output.display()))
        }
        Command::Render {
            midi,
            soundfont,
            sfz,
            output,
            renderer,
            sample_rate,
            gain,
        } => {
            match renderer {
                tools::Renderer::Sfizz => {
                    let sfz = sfz.as_deref().ok_or_else(|| error::Error::Validation {
                        path: "--sfz".to_owned(),
                        message: "renderer `sfizz` requires --sfz (a single .sfz instrument); for multi-instrument scenes use `scorekit build --profile`"
                            .to_owned(),
                    })?;
                    if soundfont.is_some() {
                        return Err(error::Error::Validation {
                            path: "--soundfont".to_owned(),
                            message:
                                "renderer `sfizz` does not use --soundfont; pass --sfz instead"
                                    .to_owned(),
                        });
                    }
                    tools::render_sfz(midi, sfz, output, *sample_rate)?;
                }
                tools::Renderer::Fluidsynth | tools::Renderer::Timidity => {
                    if sfz.is_some() {
                        return Err(error::Error::Validation {
                            path: "--sfz".to_owned(),
                            message: "this renderer does not use --sfz; pass --soundfont instead"
                                .to_owned(),
                        });
                    }
                    let soundfont = soundfont::resolve(soundfont.as_deref())?;
                    tools::render(*renderer, midi, &soundfont, output, *sample_rate, *gain)?;
                }
            }
            Ok(format!("wrote {}", output.display()))
        }
        Command::Export {
            input,
            output,
            quality,
            seek_samples,
            take_samples,
        } => {
            if *seek_samples > 0 || take_samples.is_some() {
                let take = match take_samples {
                    Some(t) => *t,
                    None => audio::frames(input)?.saturating_sub(*seek_samples),
                };
                let window = audio::Window {
                    skip: *seek_samples,
                    take,
                    crossfade: 0,
                };
                let wav_out = output
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("wav"));
                if wav_out {
                    audio::extract(input, output, window)?;
                } else {
                    let cut = output.with_extension("cut.wav");
                    audio::extract(input, &cut, window)?;
                    let result = tools::export(&cut, output, *quality);
                    let _ = std::fs::remove_file(&cut);
                    result?;
                }
            } else {
                tools::export(input, output, *quality)?;
            }
            Ok(format!("wrote {}", output.display()))
        }
        Command::Build {
            scene,
            soundfont,
            profile,
            texture_profile,
            output,
            renderer,
            sample_rate,
            gain,
            quality,
            stems,
            tail,
            crossfade_ms,
            keep_intermediates,
        } => {
            let soundfont = soundfont::for_renderer(*renderer, soundfont.as_deref())?;
            pipeline::build(&pipeline::BuildArgs {
                scene,
                soundfont: soundfont.as_deref(),
                profile: profile.as_deref(),
                texture_profile: texture_profile.as_deref(),
                output,
                renderer: *renderer,
                sample_rate: *sample_rate,
                gain: *gain,
                quality: *quality,
                stems: *stems,
                tail: *tail,
                crossfade_ms: *crossfade_ms,
                keep_intermediates: *keep_intermediates,
            })
        }
        Command::Diff { old, new } => {
            let a = schema::load_scene(old)?;
            let b = schema::load_scene(new)?;
            let changes = diff::scenes(&a, &b);
            if json {
                let arr: Vec<_> = changes.iter().map(diff::Change::to_json).collect();
                Ok(serde_json::Value::Array(arr).to_string())
            } else {
                Ok(changes
                    .iter()
                    .map(diff::Change::porcelain)
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
        }
        Command::Mcp => {
            mcp::serve()?;
            Ok(String::new())
        }
        Command::Batch {
            scenes,
            soundfont,
            profile,
            texture_profile,
            out_dir,
            format,
            renderer,
            sample_rate,
            gain,
            quality,
            stems,
            tail,
            crossfade_ms,
            report,
        } => {
            let soundfont = soundfont::for_renderer(*renderer, soundfont.as_deref())?;
            pipeline::batch(&pipeline::BatchArgs {
                scenes,
                soundfont: soundfont.as_deref(),
                profile: profile.as_deref(),
                texture_profile: texture_profile.as_deref(),
                out_dir,
                format,
                renderer: *renderer,
                sample_rate: *sample_rate,
                gain: *gain,
                quality: *quality,
                stems: *stems,
                tail: *tail,
                crossfade_ms: *crossfade_ms,
                report: report.as_deref(),
            })
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli.command, cli.json) {
        Ok(msg) => {
            if !msg.is_empty() {
                println!("{msg}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            e.report(cli.json);
            ExitCode::from(e.exit_code())
        }
    }
}
