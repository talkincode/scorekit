//! Read-only environment diagnostics for scorekit's external tool boundary.
//! The compiler itself remains self-contained; audio rendering and export are
//! deliberately delegated to tools discovered on PATH.

use serde::Serialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct PlatformReport {
    pub os: String,
    pub arch: String,
    pub target: String,
    pub release_asset: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequirementsReport {
    pub ffmpeg: bool,
    pub renderer: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolReport {
    pub name: String,
    pub role: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DefaultSoundfontReport {
    pub path: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SoundLibraryReport {
    pub path: String,
    pub source: String,
    pub default_soundfont: DefaultSoundfontReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub scorekit_version: String,
    pub ready: bool,
    pub platform: PlatformReport,
    pub requirements: RequirementsReport,
    pub tools: Vec<ToolReport>,
    pub sound_library: SoundLibraryReport,
    pub hints: Vec<String>,
}

impl Report {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("doctor report serializes")
    }

    pub fn human(&self) -> String {
        let mut lines = vec![
            format!("scorekit doctor (scorekit {})", self.scorekit_version),
            format!(
                "platform: {}/{} ({})",
                self.platform.os, self.platform.arch, self.platform.target
            ),
        ];
        for tool in &self.tools {
            let detail = tool
                .version
                .as_deref()
                .or(tool.error.as_deref())
                .unwrap_or("not found on PATH");
            let path = tool
                .path
                .as_deref()
                .map(|path| format!(" @ {path}"))
                .unwrap_or_default();
            lines.push(format!(
                "[{}] {} ({}): {}{}",
                tool.status, tool.name, tool.role, detail, path
            ));
        }
        lines.push(format!(
            "status: {} (ffmpeg={}, renderer={})",
            if self.ready { "ready" } else { "not ready" },
            self.requirements.ffmpeg,
            self.requirements.renderer
        ));
        lines.push(format!(
            "[{}] default SoundFont: {}",
            self.sound_library.default_soundfont.status, self.sound_library.default_soundfont.path
        ));
        lines.push("help:".to_owned());
        lines.extend(self.hints.iter().map(|hint| format!("  {hint}")));
        lines.join("\n")
    }
}

struct ToolSpec {
    name: &'static str,
    role: &'static str,
    version_args: &'static [&'static str],
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn candidate_names(name: &str) -> Vec<OsString> {
    #[cfg(windows)]
    {
        let mut names = vec![OsString::from(name)];
        let extensions =
            std::env::var_os("PATHEXT").unwrap_or_else(|| OsString::from(".COM;.EXE;.BAT;.CMD"));
        for extension in extensions.to_string_lossy().split(';') {
            if !extension.is_empty() {
                names.push(OsString::from(format!("{name}{extension}")));
                names.push(OsString::from(format!(
                    "{name}{}",
                    extension.to_ascii_lowercase()
                )));
            }
        }
        names
    }
    #[cfg(not(windows))]
    {
        vec![OsString::from(name)]
    }
}

fn find_executable(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let candidates = candidate_names(name);
    for directory in std::env::split_paths(&path) {
        for candidate in &candidates {
            let executable = directory.join(candidate);
            if is_executable(&executable) {
                return Some(executable);
            }
        }
    }
    None
}

fn first_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

fn probe(spec: &ToolSpec) -> ToolReport {
    let Some(path) = find_executable(spec.name) else {
        return ToolReport {
            name: spec.name.to_owned(),
            role: spec.role.to_owned(),
            status: "missing".to_owned(),
            path: None,
            version: None,
            error: None,
        };
    };
    let result = Command::new(&path).args(spec.version_args).output();
    match result {
        Ok(output) if output.status.success() => ToolReport {
            name: spec.name.to_owned(),
            role: spec.role.to_owned(),
            status: "ok".to_owned(),
            path: Some(path.display().to_string()),
            version: first_line(&output.stdout).or_else(|| first_line(&output.stderr)),
            error: None,
        },
        Ok(output) => ToolReport {
            name: spec.name.to_owned(),
            role: spec.role.to_owned(),
            status: "broken".to_owned(),
            path: Some(path.display().to_string()),
            version: None,
            error: Some(format!("version probe exited with {}", output.status)),
        },
        Err(error) => ToolReport {
            name: spec.name.to_owned(),
            role: spec.role.to_owned(),
            status: "broken".to_owned(),
            path: Some(path.display().to_string()),
            version: None,
            error: Some(error.to_string()),
        },
    }
}

fn target(os: &str, arch: &str) -> String {
    match (os, arch) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        _ => return format!("{arch}-{os}"),
    }
    .to_owned()
}

fn platform() -> PlatformReport {
    let os = std::env::consts::OS.to_owned();
    let arch = std::env::consts::ARCH.to_owned();
    let target = target(&os, &arch);
    let extension = if os == "windows" { "zip" } else { "tar.gz" };
    PlatformReport {
        os,
        arch,
        release_asset: format!("scorekit-{target}.{extension}"),
        target,
    }
}

fn hints(platform: &PlatformReport) -> Vec<String> {
    let mut hints = vec![format!(
        "Platform {}/{}: use release asset {}.",
        platform.os, platform.arch, platform.release_asset
    )];
    match platform.os.as_str() {
        "macos" => {
            hints.push(
                "Install the standard toolchain with `brew install fluid-synth timidity ffmpeg`."
                    .to_owned(),
            );
            if platform.arch == "aarch64" {
                hints.push("Apple Silicon: sfizz has no native Homebrew formula or official arm64 renderer; from a scorekit source checkout run `make sfizz`.".to_owned());
            } else {
                hints.push("Intel macOS: install the upstream sfizz x86_64 renderer or build it from a scorekit source checkout with `make sfizz`.".to_owned());
            }
        }
        "linux" => {
            hints.push(
                "Debian/Ubuntu: `sudo apt-get install fluidsynth timidity ffmpeg`.".to_owned(),
            );
            hints.push(format!(
                "Linux {}: build sfizz_render from source with `make sfizz` when SFZ rendering is required.",
                platform.arch
            ));
        }
        "windows" => {
            hints.push("Windows x86_64: install FFmpeg and FluidSynth or TiMidity++, then add their executable directories to PATH.".to_owned());
            hints.push("For SFZ rendering, install an upstream Windows sfizz_render build and add it to PATH.".to_owned());
        }
        _ => hints.push(format!(
            "{} on {} is not a prebuilt release target; build scorekit and its external tools from source.",
            platform.arch, platform.os
        )),
    }
    hints.push(
        "Run `make install-default-soundfont` when MuseScore_General.sf2 is missing; SFZ rendering still requires a user-supplied library/profile."
            .to_owned(),
    );
    hints
}

pub fn check() -> Report {
    let specs = [
        ToolSpec {
            name: "ffmpeg",
            role: "audio export",
            version_args: &["-version"],
        },
        ToolSpec {
            name: "fluidsynth",
            role: "SF2 renderer",
            version_args: &["--version"],
        },
        ToolSpec {
            name: "timidity",
            role: "alternate SF2 renderer",
            version_args: &["--version"],
        },
        ToolSpec {
            name: "sfizz_render",
            role: "SFZ renderer",
            version_args: &["--help"],
        },
    ];
    let tools: Vec<_> = specs.iter().map(probe).collect();
    let ffmpeg = tools[0].status == "ok";
    let renderer = tools[1..].iter().any(|tool| tool.status == "ok");
    let platform = platform();
    let library = crate::soundfont::library_dir().unwrap_or_default();
    let default_soundfont = library.join("sf2").join(crate::soundfont::DEFAULT_NAME);
    let default_status = if crate::soundfont::has_sf2_magic(&default_soundfont) {
        "ok"
    } else if default_soundfont.is_file() {
        "invalid"
    } else {
        "missing"
    };
    Report {
        scorekit_version: env!("CARGO_PKG_VERSION").to_owned(),
        ready: ffmpeg && renderer,
        requirements: RequirementsReport { ffmpeg, renderer },
        hints: hints(&platform),
        platform,
        tools,
        sound_library: SoundLibraryReport {
            path: library.display().to_string(),
            source: if std::env::var_os(crate::soundfont::LIBRARY_ENV).is_some() {
                crate::soundfont::LIBRARY_ENV.to_owned()
            } else {
                "platform default".to_owned()
            },
            default_soundfont: DefaultSoundfontReport {
                path: default_soundfont.display().to_string(),
                status: default_status.to_owned(),
            },
        },
    }
}
