//! Active renderer-profile verification. Schema validation proves the YAML is
//! shaped correctly; this module proves each referenced SFZ actually renders,
//! produces audible PCM, and repeats deterministically with the pinned tool.

use crate::composer::{NoteEvent, ScoreIr, TrackIr};
use crate::error::{Error, Result};
use crate::profile;
use crate::schema::{Instrument, TimeSig};
use crate::{midi, tools};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SILENCE_PEAK: u32 = 1;
const DETERMINISM_TOLERANCE: f64 = 1.0e-6;

#[derive(Debug, Clone, Serialize)]
pub struct PatchReport {
    pub path: String,
    pub mappings: Vec<String>,
    pub status: String,
    pub peak_abs: u32,
    pub rms: f64,
    pub deterministic: bool,
    pub difference_rms_ratio: f64,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub profile: String,
    pub mappings: usize,
    pub unique_patches: usize,
    pub passed: usize,
    pub failed: usize,
    pub sample_rate: u32,
    pub patches: Vec<PatchReport>,
}

impl Report {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("profile report serializes")
    }

    pub fn summary(&self) -> String {
        format!(
            "ok: profile `{}`: {} mapping(s), {} unique patch(es), {} passed",
            self.profile, self.mappings, self.unique_patches, self.passed
        )
    }

    fn failure_lines(&self) -> Vec<String> {
        self.patches
            .iter()
            .filter(|p| p.status != "ok")
            .map(|p| {
                format!(
                    "{} @ {}: {}",
                    p.status,
                    p.mappings.join(","),
                    p.error.as_deref().unwrap_or(&p.path)
                )
            })
            .collect()
    }
}

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn create() -> Result<Self> {
        let path =
            std::env::temp_dir().join(format!("scorekit-profile-check-{}", std::process::id()));
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

#[derive(Debug)]
struct Pcm {
    channels: u16,
    sample_rate: u32,
    samples: Vec<i16>,
}

fn read_pcm(path: &Path) -> Result<Pcm> {
    let mut reader = hound::WavReader::open(path).map_err(|e| Error::Validation {
        path: path.display().to_string(),
        message: format!("sfizz produced an unreadable WAV: {e}"),
    })?;
    let spec = reader.spec();
    let samples = reader
        .samples::<i16>()
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Validation {
            path: path.display().to_string(),
            message: format!("sfizz produced invalid PCM: {e}"),
        })?;
    Ok(Pcm {
        channels: spec.channels,
        sample_rate: spec.sample_rate,
        samples,
    })
}

fn stats(samples: &[i16]) -> (u32, f64) {
    let mut peak = 0u32;
    let mut sum = 0.0f64;
    for &sample in samples {
        let value = i32::from(sample);
        peak = peak.max(value.unsigned_abs());
        sum += f64::from(value) * f64::from(value);
    }
    let rms = if samples.is_empty() {
        0.0
    } else {
        (sum / samples.len() as f64).sqrt()
    };
    (peak, rms)
}

fn difference_ratio(a: &Pcm, b: &Pcm) -> f64 {
    if a.channels != b.channels
        || a.sample_rate != b.sample_rate
        || a.samples.len() != b.samples.len()
    {
        return f64::INFINITY;
    }
    let (mut diff2, mut ref2) = (0.0f64, 0.0f64);
    for (&left, &right) in a.samples.iter().zip(&b.samples) {
        let l = f64::from(left);
        let d = l - f64::from(right);
        diff2 += d * d;
        ref2 += l * l;
    }
    (diff2 / ref2.max(1.0)).sqrt()
}

fn warnings(diagnostics: &[tools::ToolDiagnostics]) -> Vec<String> {
    let mut out = Vec::new();
    for diagnostics in diagnostics {
        for line in diagnostics.stdout.lines().chain(diagnostics.stderr.lines()) {
            let lower = line.to_ascii_lowercase();
            if lower.contains("warn")
                || lower.contains("error")
                || lower.contains("unsupported")
                || lower.contains("failed")
            {
                let line = line.trim().to_owned();
                if !line.is_empty() && !out.contains(&line) {
                    out.push(line);
                }
            }
        }
    }
    out.truncate(20);
    out
}

fn probe_midi(drum_channel: bool) -> Vec<u8> {
    let keys = [
        24u8, 36, 38, 42, 48, 55, 60, 67, 72, 84, 96, 108, 60, 60, 60, 60,
    ];
    let velocities = [32u8, 64, 96, 127];
    let step = 240u32;
    let notes = keys
        .iter()
        .enumerate()
        .map(|(index, &key)| NoteEvent {
            tick: index as u32 * step,
            dur: step,
            key,
            vel: velocities[index % velocities.len()],
        })
        .collect();
    let total_ticks = keys.len() as u32 * step + 960;
    midi::to_smf_bytes(&ScoreIr {
        tempo: 240,
        ts: TimeSig { num: 4, den: 4 },
        total_ticks,
        tracks: vec![TrackIr {
            channel: if drum_channel { 9 } else { 0 },
            program: None,
            notes,
        }],
    })
}

fn render_failure(
    path: &Path,
    mappings: Vec<String>,
    status: &str,
    error: impl Into<String>,
) -> PatchReport {
    PatchReport {
        path: path.display().to_string(),
        mappings,
        status: status.to_owned(),
        peak_abs: 0,
        rms: 0.0,
        deterministic: false,
        difference_rms_ratio: f64::INFINITY,
        warnings: Vec::new(),
        error: Some(error.into()),
    }
}

pub fn check(profile_path: &Path, sample_rate: u32) -> Result<Report> {
    let loaded = profile::load_profile(profile_path)?;
    let profile_dir = profile_path.parent().unwrap_or_else(|| Path::new("."));
    let mappings = loaded.resolved_mappings(profile_dir);
    let mapping_count = mappings.len();

    let mut patches: BTreeMap<PathBuf, (Vec<String>, bool)> = BTreeMap::new();
    for mapping in mappings {
        let path = std::fs::canonicalize(&mapping.path).unwrap_or(mapping.path);
        let entry = patches.entry(path).or_default();
        entry.0.push(format!(
            "{}.{}",
            mapping.instrument_key, mapping.articulation_key
        ));
        entry.1 |= mapping.instrument == Instrument::Drums;
    }

    let unique_patches = patches.len();
    let scratch = Scratch::create()?;
    let melodic_midi = scratch.path.join("probe-melodic.mid");
    let drum_midi = scratch.path.join("probe-drums.mid");
    tools::write_atomic(&melodic_midi, &probe_midi(false))?;
    tools::write_atomic(&drum_midi, &probe_midi(true))?;

    let mut reports = Vec::with_capacity(unique_patches);
    for (index, (path, (mapping_names, drums))) in patches.into_iter().enumerate() {
        if !path.is_file() {
            reports.push(render_failure(
                &path,
                mapping_names,
                "missing",
                format!("SFZ file not found: {}", path.display()),
            ));
            continue;
        }
        let midi = if drums { &drum_midi } else { &melodic_midi };
        let a_path = scratch.path.join(format!("{index:04}-a.wav"));
        let b_path = scratch.path.join(format!("{index:04}-b.wav"));
        let first = match tools::render_sfz_with_diagnostics(midi, &path, &a_path, sample_rate) {
            Err(e @ Error::MissingDependency { .. }) => return Err(e),
            Err(e) => {
                reports.push(render_failure(
                    &path,
                    mapping_names,
                    "render_failed",
                    e.to_string(),
                ));
                continue;
            }
            Ok(diagnostics) => diagnostics,
        };
        let second = match tools::render_sfz_with_diagnostics(midi, &path, &b_path, sample_rate) {
            Err(e @ Error::MissingDependency { .. }) => return Err(e),
            Err(e) => {
                reports.push(render_failure(
                    &path,
                    mapping_names,
                    "render_failed",
                    e.to_string(),
                ));
                continue;
            }
            Ok(diagnostics) => diagnostics,
        };
        let a = read_pcm(&a_path)?;
        let b = read_pcm(&b_path)?;
        let (peak_abs, rms) = stats(&a.samples);
        let difference_rms_ratio = difference_ratio(&a, &b);
        let deterministic = difference_rms_ratio <= DETERMINISM_TOLERANCE;
        let (status, error) = if peak_abs <= SILENCE_PEAK {
            ("silent", Some("probe produced no audible PCM".to_owned()))
        } else if !deterministic {
            (
                "nondeterministic",
                Some(format!(
                    "two renders differ (RMS ratio {difference_rms_ratio:.8})"
                )),
            )
        } else {
            ("ok", None)
        };
        reports.push(PatchReport {
            path: path.display().to_string(),
            mappings: mapping_names,
            status: status.to_owned(),
            peak_abs,
            rms,
            deterministic,
            difference_rms_ratio,
            warnings: warnings(&[first, second]),
            error,
        });
    }

    let passed = reports.iter().filter(|patch| patch.status == "ok").count();
    let failed = reports.len() - passed;
    let report = Report {
        profile: loaded.name,
        mappings: mapping_count,
        unique_patches,
        passed,
        failed,
        sample_rate,
        patches: reports,
    };
    if failed == 0 {
        Ok(report)
    } else {
        let status_code = if report
            .patches
            .iter()
            .any(|patch| patch.status == "render_failed")
        {
            4
        } else {
            2
        };
        Err(Error::ProfileCheck {
            profile: report.profile.clone(),
            count: failed,
            status_code,
            porcelain: report.failure_lines(),
            report: report.to_json(),
        })
    }
}
