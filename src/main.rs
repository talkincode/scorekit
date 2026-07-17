mod composer;
mod error;
mod midi;
mod schema;
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
    /// Validate a scene file
    Validate { scene: PathBuf },
    /// Print the JSON Schema of the scene DSL
    Schema,
    /// Compile a scene to a Standard MIDI File
    Midi {
        scene: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Render a MIDI file to WAV via FluidSynth + SoundFont
    Render {
        midi: PathBuf,
        #[arg(long)]
        soundfont: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long, default_value_t = 44100)]
        sample_rate: u32,
        #[arg(long, default_value_t = 0.8)]
        gain: f32,
    },
    /// Convert rendered audio to OGG/Vorbis via FFmpeg
    Export {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        /// Vorbis quality 0..=10
        #[arg(long, default_value_t = 5)]
        quality: u8,
    },
    /// Full chain: scene -> MIDI -> WAV -> OGG
    Build {
        scene: PathBuf,
        #[arg(long)]
        soundfont: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long, default_value_t = 44100)]
        sample_rate: u32,
        #[arg(long, default_value_t = 0.8)]
        gain: f32,
        #[arg(long, default_value_t = 5)]
        quality: u8,
        /// Keep the intermediate .mid and .wav next to the output
        #[arg(long)]
        keep_intermediates: bool,
    },
}

fn compile_midi(scene_path: &Path, output: &Path) -> Result<()> {
    let scene = schema::load_scene(scene_path)?;
    let ir = composer::compose(&scene);
    let bytes = midi::to_smf_bytes(&ir);
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| error::Error::Io {
            path: parent.display().to_string(),
            source: e,
        })?;
    }
    tools::write_atomic(output, &bytes)
}

fn run(command: &Command) -> Result<String> {
    match command {
        Command::Validate { scene } => {
            let s = schema::load_scene(scene)?;
            Ok(format!(
                "ok: {} bars, {} track(s), tempo {}",
                s.bars,
                s.tracks.len(),
                s.tempo
            ))
        }
        Command::Schema => Ok(schema::schema_json()),
        Command::Midi { scene, output } => {
            compile_midi(scene, output)?;
            Ok(format!("wrote {}", output.display()))
        }
        Command::Render {
            midi,
            soundfont,
            output,
            sample_rate,
            gain,
        } => {
            tools::render(midi, soundfont, output, *sample_rate, *gain)?;
            Ok(format!("wrote {}", output.display()))
        }
        Command::Export {
            input,
            output,
            quality,
        } => {
            tools::export(input, output, *quality)?;
            Ok(format!("wrote {}", output.display()))
        }
        Command::Build {
            scene,
            soundfont,
            output,
            sample_rate,
            gain,
            quality,
            keep_intermediates,
        } => {
            let mid = output.with_extension("mid");
            let wav = output.with_extension("wav");
            compile_midi(scene, &mid)?;
            let result = tools::render(&mid, soundfont, &wav, *sample_rate, *gain)
                .and_then(|()| tools::export(&wav, output, *quality));
            if !keep_intermediates {
                let _ = std::fs::remove_file(&mid);
                let _ = std::fs::remove_file(&wav);
            }
            result?;
            Ok(format!("wrote {}", output.display()))
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli.command) {
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
