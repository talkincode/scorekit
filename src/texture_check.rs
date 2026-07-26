//! Active texture-source verification.
//!
//! Schema validation proves a profile is *shaped* correctly; this module
//! proves each declared source actually exists, decodes through the same
//! FFmpeg normalization the build uses, and carries audible PCM. It is the
//! texture-side counterpart of `profile_check`, and the same rule applies:
//! a mapping counts as coverage only once it has been certified.
//!
//! It is also where **measured** facts are produced. The profile YAML
//! deliberately declares no duration, channel count or checksum: those are
//! properties of the file, and a hand-written copy of them is a fact nothing
//! verifies and every re-export invalidates. The stored `--json` report is
//! the durable record, exactly like the renderer profile's certification
//! report.

use crate::error::{Error, Result};
use crate::texture::{self, TextureProfile, mode_key};
use crate::tools;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

/// Peak sample magnitude at or below which a source counts as silent.
const SILENCE_PEAK: u32 = 1;

#[derive(Debug, Clone, Serialize)]
pub struct SourceReport {
    pub source: String,
    pub path: String,
    pub status: String,
    pub category: String,
    pub modes: Vec<String>,
    pub default_mode: String,
    pub library: String,
    /// SHA-256 of the **source file** bytes, not of the normalized PCM: it
    /// identifies the recording itself and stays stable across FFmpeg
    /// versions, so a stored report doubles as a corpus-drift baseline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frames: Option<u64>,
    pub peak_abs: u32,
    pub rms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub profile: String,
    pub sources: usize,
    pub passed: usize,
    pub failed: usize,
    /// Rate the sources were normalized to for measurement.
    pub sample_rate: u32,
    pub entries: Vec<SourceReport>,
}

impl Report {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("texture report serializes")
    }

    pub fn summary(&self) -> String {
        format!(
            "ok: texture profile `{}`: {} source(s), {} certified",
            self.profile, self.sources, self.passed
        )
    }

    fn failure_lines(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|entry| entry.status != "ok")
            .map(|entry| {
                format!(
                    "{} @ {}: {}",
                    entry.status,
                    entry.source,
                    entry.error.as_deref().unwrap_or(&entry.path)
                )
            })
            .collect()
    }
}

/// Command-scoped scratch directory; removed on success and on every failure
/// path, including panics, via `Drop`.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn create() -> Result<Self> {
        let root = std::env::var_os("SCOREKIT_TMPDIR")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&root).map_err(|source| Error::Io {
            path: root.display().to_string(),
            source,
        })?;
        let path = root.join(format!("scorekit-texture-check-{}", std::process::id()));
        std::fs::create_dir(&path).map_err(|source| Error::Io {
            path: path.display().to_string(),
            source,
        })?;
        Ok(Self { path })
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

struct Measured {
    frames: u64,
    peak_abs: u32,
    rms: f64,
}

fn measure(path: &Path) -> Result<Measured> {
    let mut reader = hound::WavReader::open(path).map_err(|e| Error::Validation {
        path: path.display().to_string(),
        message: format!("normalized texture is unreadable: {e}"),
    })?;
    let spec = reader.spec();
    let mut peak = 0u32;
    let mut sum = 0.0f64;
    let mut count = 0u64;
    for sample in reader.samples::<i16>() {
        let sample = sample.map_err(|e| Error::Validation {
            path: path.display().to_string(),
            message: format!("normalized texture has invalid PCM: {e}"),
        })?;
        let value = i32::from(sample);
        peak = peak.max(value.unsigned_abs());
        sum += f64::from(value) * f64::from(value);
        count += 1;
    }
    let rms = if count == 0 {
        0.0
    } else {
        (sum / count as f64).sqrt()
    };
    let channels = u64::from(spec.channels).max(1);
    Ok(Measured {
        frames: count / channels,
        peak_abs: peak,
        rms,
    })
}

fn sha256_file(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(|source| Error::Io {
            path: path.display().to_string(),
            source,
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn entry(name: &str, path: &Path, source: &texture::TextureSource, status: &str) -> SourceReport {
    SourceReport {
        source: name.to_owned(),
        path: path.display().to_string(),
        status: status.to_owned(),
        category: source.category.key().to_owned(),
        modes: source
            .playback
            .modes
            .iter()
            .map(|mode| mode_key(*mode).to_owned())
            .collect(),
        default_mode: mode_key(source.playback.default_mode).to_owned(),
        library: source.provenance.library.clone(),
        sha256: None,
        duration_seconds: None,
        frames: None,
        peak_abs: 0,
        rms: 0.0,
        error: None,
    }
}

pub fn check(profile_path: &Path, sample_rate: u32) -> Result<Report> {
    let profile: TextureProfile = texture::load_profile(profile_path)?;
    let profile_dir = texture::profile_dir(profile_path);
    let scratch = Scratch::create()?;

    let mut entries = Vec::with_capacity(profile.sources.len());
    for (index, (name, source, resolved)) in profile
        .resolved_discoverable_sources(&profile_dir)?
        .into_iter()
        .enumerate()
    {
        let mut report = entry(name, &resolved, source, "ok");
        if !resolved.is_file() {
            report.status = "missing".to_owned();
            report.error = Some(format!(
                "texture source file does not exist: {}",
                resolved.display()
            ));
            entries.push(report);
            continue;
        }
        report.sha256 = Some(sha256_file(&resolved)?);

        let normalized = scratch.path.join(format!("{index:04}-{name}.wav"));
        match tools::normalize_texture(&resolved, &normalized, sample_rate) {
            // A missing FFmpeg is an environment fault, not a corpus defect:
            // reporting it per-source would falsely blame every recording.
            Err(e @ Error::MissingDependency { .. }) => return Err(e),
            Err(e) => {
                report.status = "undecodable".to_owned();
                report.error = Some(e.to_string());
                entries.push(report);
                continue;
            }
            Ok(()) => {}
        }

        let measured = measure(&normalized)?;
        // Release each normalized copy immediately: a corpus-sized profile
        // would otherwise stage gigabytes of scratch PCM at once.
        let _ = std::fs::remove_file(&normalized);

        report.frames = Some(measured.frames);
        report.duration_seconds = Some(measured.frames as f64 / f64::from(sample_rate));
        report.peak_abs = measured.peak_abs;
        report.rms = measured.rms;
        if measured.peak_abs <= SILENCE_PEAK {
            report.status = "silent".to_owned();
            report.error = Some("source decodes but carries no audible PCM".to_owned());
        }
        entries.push(report);
    }

    let passed = entries.iter().filter(|e| e.status == "ok").count();
    let failed = entries.len() - passed;
    let report = Report {
        profile: profile.name.clone(),
        sources: entries.len(),
        passed,
        failed,
        sample_rate,
        entries,
    };
    if failed == 0 {
        return Ok(report);
    }
    let status_code = if report
        .entries
        .iter()
        .any(|entry| entry.status == "undecodable")
    {
        4
    } else {
        2
    };
    Err(Error::TextureCheck {
        profile: report.profile.clone(),
        count: failed,
        status_code,
        porcelain: report.failure_lines(),
        report: report.to_json(),
    })
}
