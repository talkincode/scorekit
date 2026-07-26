//! Active renderer-profile verification. Schema validation proves the YAML is
//! shaped correctly; this module proves each referenced SFZ actually renders,
//! produces audible PCM, and repeats deterministically with the pinned tool.
//!
//! Failure handling is a recorded, isolated recheck — not blind retry: a
//! failed comparison (silent or nondeterministic) captures environment
//! diagnostics (load average, tool identity, both render hashes, timings)
//! and re-runs that one patch once; an isolated pass downgrades the failure
//! to a `load_sensitive_flake` warning with the evidence attached, an
//! isolated failure stays a hard failure carrying both attempts' diagnostics.

use crate::composer::{NoteEvent, ScoreIr, TrackIr};
use crate::error::{Error, Result};
use crate::profile;
use crate::schema::TimeSig;
use crate::{midi, tools};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

const SILENCE_PEAK: u32 = 1;
const DETERMINISM_TOLERANCE: f64 = 1.0e-6;
const PROBE_TEMPO: u16 = 240;
/// 16 probe notes × 240 ticks each + 960 ticks of release-tail pad past the
/// last note-off; the EndOfTrack lands here and `--use-eot` renders exactly
/// this long (2.5s at tempo 240), keeping probe renders bounded.
const PROBE_TOTAL_TICKS: u32 = 16 * 240 + 960;

/// Environment + evidence snapshot for one failed render-pair attempt.
#[derive(Debug, Clone, Serialize)]
pub struct FlakeDiagnostics {
    pub attempt: String,
    pub observed_status: String,
    pub difference_rms_ratio: f64,
    pub peak_abs: u32,
    pub render_sha256: [String; 2],
    pub render_ms: [u64; 2],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_average: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sfizz_render: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchReport {
    pub path: String,
    pub mappings: Vec<String>,
    /// MIDI channel probes required by the mappings sharing this physical
    /// patch. A mixed melodic/percussion patch must pass both.
    pub probes: Vec<String>,
    pub status: String,
    pub peak_abs: u32,
    pub rms: f64,
    pub deterministic: bool,
    pub difference_rms_ratio: f64,
    /// SHA-256 of the first render's WAV bytes. Stable across runs with the
    /// same tool version, so a stored certified report doubles as a
    /// golden-render baseline: corpus or tool drift shows up as a hash diff.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render_sha256: Option<String>,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub flake_diagnostics: Vec<FlakeDiagnostics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Probe {
    Melodic,
    Percussion,
}

impl Probe {
    fn key(self) -> &'static str {
        match self {
            Probe::Melodic => "melodic",
            Probe::Percussion => "percussion",
        }
    }
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
        // Probe renders can be large; `SCOREKIT_TMPDIR` relocates them (e.g.
        // to an external disk) without touching the system-wide TMPDIR.
        let root = std::env::var_os("SCOREKIT_TMPDIR")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&root).map_err(|source| Error::Io {
            path: root.display().to_string(),
            source,
        })?;
        let path = root.join(format!("scorekit-profile-check-{}", std::process::id()));
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
    debug_assert_eq!(total_ticks, PROBE_TOTAL_TICKS);
    midi::to_smf_bytes(&ScoreIr {
        tempo: PROBE_TEMPO,
        ts: TimeSig { num: 4, den: 4 },
        total_ticks,
        tracks: vec![TrackIr {
            channel: if drum_channel { 9 } else { 0 },
            program: None,
            pan: None,
            reverb: None,
            notes,
            bends: Vec::new(),
        }],
    })
}

fn render_failure(
    path: &Path,
    mappings: Vec<String>,
    probes: Vec<String>,
    status: &str,
    error: impl Into<String>,
) -> PatchReport {
    PatchReport {
        path: path.display().to_string(),
        mappings,
        probes,
        status: status.to_owned(),
        peak_abs: 0,
        rms: 0.0,
        deterministic: false,
        difference_rms_ratio: f64::INFINITY,
        render_sha256: None,
        warnings: Vec::new(),
        flake_diagnostics: Vec::new(),
        error: Some(error.into()),
    }
}

/// Verdict of one double-render comparison.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Verdict {
    Pass,
    Silent,
    Nondeterministic,
}

impl Verdict {
    fn status(self) -> &'static str {
        match self {
            Verdict::Pass => "ok",
            Verdict::Silent => "silent",
            Verdict::Nondeterministic => "nondeterministic",
        }
    }
}

struct PairOutcome {
    verdict: Verdict,
    peak_abs: u32,
    rms: f64,
    difference_rms_ratio: f64,
    hashes: [String; 2],
    times_ms: [u64; 2],
    diagnostics: Vec<tools::ToolDiagnostics>,
}

enum PairResult {
    Rendered(Box<PairOutcome>),
    Failed(String),
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn load_average() -> Option<String> {
    let out = std::process::Command::new("uptime").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.split("load average")
        .nth(1)
        .map(|tail| format!("load average{}", tail.trim()))
}

fn sfizz_identity() -> Option<String> {
    let path = crate::doctor::find_executable("sfizz_render")?;
    Some(path.display().to_string())
}

/// Render the probe twice into `<index>-<tag>-{a,b}.wav` and compare.
/// `Err` propagates only fatal conditions (missing dependency, unreadable
/// output); tool-level render failures come back as `PairResult::Failed`.
fn render_pair(
    midi: &Path,
    sfz: &Path,
    scratch: &Path,
    index: usize,
    tag: &str,
    sample_rate: u32,
) -> Result<PairResult> {
    let a_path = scratch.join(format!("{index:04}-{tag}-a.wav"));
    let b_path = scratch.join(format!("{index:04}-{tag}-b.wav"));
    let probe_secs = midi::exact_samples(PROBE_TOTAL_TICKS, PROBE_TEMPO, sample_rate) as f64
        / f64::from(sample_rate);
    let limits = tools::ToolLimits::for_expected_audio(probe_secs, sample_rate);
    let mut diagnostics = Vec::with_capacity(2);
    let mut times_ms = [0u64; 2];
    for (slot, out_path) in [(0usize, &a_path), (1, &b_path)] {
        let started = Instant::now();
        match tools::render_sfz_with_diagnostics(midi, sfz, out_path, sample_rate, limits) {
            Err(e @ Error::MissingDependency { .. }) => return Err(e),
            Err(e) => return Ok(PairResult::Failed(e.to_string())),
            Ok(diag) => diagnostics.push(diag),
        }
        times_ms[slot] = started.elapsed().as_millis() as u64;
    }
    let a = read_pcm(&a_path)?;
    let b = read_pcm(&b_path)?;
    let (peak_abs, rms) = stats(&a.samples);
    let difference_rms_ratio = difference_ratio(&a, &b);
    let verdict = if peak_abs <= SILENCE_PEAK {
        Verdict::Silent
    } else if difference_rms_ratio > DETERMINISM_TOLERANCE {
        Verdict::Nondeterministic
    } else {
        Verdict::Pass
    };
    let hashes = [sha256_file(&a_path)?, sha256_file(&b_path)?];
    Ok(PairResult::Rendered(Box::new(PairOutcome {
        verdict,
        peak_abs,
        rms,
        difference_rms_ratio,
        hashes,
        times_ms,
        diagnostics,
    })))
}

fn flake_snapshot(attempt: &str, outcome: &PairOutcome) -> FlakeDiagnostics {
    FlakeDiagnostics {
        attempt: attempt.to_owned(),
        observed_status: outcome.verdict.status().to_owned(),
        difference_rms_ratio: outcome.difference_rms_ratio,
        peak_abs: outcome.peak_abs,
        render_sha256: outcome.hashes.clone(),
        render_ms: outcome.times_ms,
        load_average: load_average(),
        sfizz_render: sfizz_identity(),
    }
}

fn certify_probe(
    midi: &Path,
    path: &Path,
    scratch: &Path,
    index: usize,
    probe: Probe,
    mappings: Vec<String>,
    sample_rate: u32,
) -> Result<PatchReport> {
    let probe_name = probe.key();
    let first_tag = format!("{probe_name}-first");
    let first = match render_pair(midi, path, scratch, index, &first_tag, sample_rate)? {
        PairResult::Failed(error) => {
            return Ok(render_failure(
                path,
                mappings,
                vec![probe_name.to_owned()],
                "render_failed",
                format!("{probe_name} probe: {error}"),
            ));
        }
        PairResult::Rendered(outcome) => outcome,
    };

    if first.verdict == Verdict::Pass {
        return Ok(PatchReport {
            path: path.display().to_string(),
            mappings,
            probes: vec![probe_name.to_owned()],
            status: "ok".to_owned(),
            peak_abs: first.peak_abs,
            rms: first.rms,
            deterministic: true,
            difference_rms_ratio: first.difference_rms_ratio,
            render_sha256: Some(first.hashes[0].clone()),
            warnings: warnings(&first.diagnostics),
            flake_diagnostics: Vec::new(),
            error: None,
        });
    }

    let first_snapshot = flake_snapshot("first", &first);
    let recheck_tag = format!("{probe_name}-recheck");
    let recheck = match render_pair(midi, path, scratch, index, &recheck_tag, sample_rate)? {
        PairResult::Failed(error) => {
            let mut report = render_failure(
                path,
                mappings,
                vec![probe_name.to_owned()],
                "render_failed",
                format!("{probe_name} probe: {error}"),
            );
            report.flake_diagnostics = vec![first_snapshot];
            return Ok(report);
        }
        PairResult::Rendered(outcome) => outcome,
    };

    if recheck.verdict == Verdict::Pass {
        let mut patch_warnings = warnings(&recheck.diagnostics);
        patch_warnings.push(format!(
            "load_sensitive_flake: {probe_name} probe first attempt was {} (RMS ratio {:.8}); \
             isolated recheck passed — see flake_diagnostics",
            first_snapshot.observed_status, first_snapshot.difference_rms_ratio,
        ));
        return Ok(PatchReport {
            path: path.display().to_string(),
            mappings,
            probes: vec![probe_name.to_owned()],
            status: "ok".to_owned(),
            peak_abs: recheck.peak_abs,
            rms: recheck.rms,
            deterministic: true,
            difference_rms_ratio: recheck.difference_rms_ratio,
            render_sha256: Some(recheck.hashes[0].clone()),
            warnings: patch_warnings,
            flake_diagnostics: vec![first_snapshot],
            error: None,
        });
    }

    let recheck_snapshot = flake_snapshot("recheck", &recheck);
    let error = match recheck.verdict {
        Verdict::Silent => format!("{probe_name} probe produced no audible PCM"),
        _ => format!(
            "{probe_name} probe renders differ (RMS ratio {:.8}); isolated recheck failed too",
            recheck.difference_rms_ratio
        ),
    };
    Ok(PatchReport {
        path: path.display().to_string(),
        mappings,
        probes: vec![probe_name.to_owned()],
        status: recheck.verdict.status().to_owned(),
        peak_abs: recheck.peak_abs,
        rms: recheck.rms,
        deterministic: false,
        difference_rms_ratio: recheck.difference_rms_ratio,
        render_sha256: None,
        warnings: warnings(&recheck.diagnostics),
        flake_diagnostics: vec![first_snapshot, recheck_snapshot],
        error: Some(error),
    })
}

fn merge_probe_reports(
    path: &Path,
    mappings: Vec<String>,
    mut reports: Vec<PatchReport>,
) -> PatchReport {
    if reports.len() == 1 {
        let mut report = reports.pop().expect("one report");
        report.mappings = mappings;
        return report;
    }

    let all_passed = reports.iter().all(|report| report.status == "ok");
    let status = reports
        .iter()
        .find(|report| report.status != "ok")
        .map(|report| report.status.clone())
        .unwrap_or_else(|| "ok".to_owned());
    let probes = reports
        .iter()
        .flat_map(|report| report.probes.iter().cloned())
        .collect::<Vec<_>>();
    let warnings = reports
        .iter()
        .flat_map(|report| {
            let probe = report.probes[0].clone();
            report
                .warnings
                .iter()
                .map(move |warning| format!("{probe}: {warning}"))
        })
        .collect();
    let flake_diagnostics = reports
        .iter()
        .flat_map(|report| {
            let probe = report.probes[0].clone();
            report
                .flake_diagnostics
                .iter()
                .cloned()
                .map(move |mut item| {
                    item.attempt = format!("{probe}:{}", item.attempt);
                    item
                })
        })
        .collect();
    let errors = reports
        .iter()
        .filter_map(|report| report.error.as_ref())
        .cloned()
        .collect::<Vec<_>>();
    let render_sha256 = all_passed.then(|| {
        let mut hasher = Sha256::new();
        for report in &reports {
            hasher.update(report.probes[0].as_bytes());
            hasher.update([0]);
            hasher.update(
                report
                    .render_sha256
                    .as_deref()
                    .expect("passing probe has a hash")
                    .as_bytes(),
            );
            hasher.update([b'\n']);
        }
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    });

    PatchReport {
        path: path.display().to_string(),
        mappings,
        probes,
        status,
        peak_abs: reports
            .iter()
            .map(|report| report.peak_abs)
            .min()
            .unwrap_or(0),
        rms: reports
            .iter()
            .map(|report| report.rms)
            .reduce(f64::min)
            .unwrap_or(0.0),
        deterministic: reports.iter().all(|report| report.deterministic),
        difference_rms_ratio: reports
            .iter()
            .map(|report| report.difference_rms_ratio)
            .reduce(f64::max)
            .unwrap_or(f64::INFINITY),
        render_sha256,
        warnings,
        flake_diagnostics,
        error: (!errors.is_empty()).then(|| errors.join("; ")),
    }
}

pub fn check(profile_path: &Path, sample_rate: u32) -> Result<Report> {
    let loaded = profile::load_profile(profile_path)?;
    let profile_dir = profile_path.parent().unwrap_or_else(|| Path::new("."));
    let mappings = loaded.resolved_mappings(profile_dir);
    let mapping_count = mappings.len();

    let mut patches: BTreeMap<PathBuf, BTreeMap<Probe, Vec<String>>> = BTreeMap::new();
    for mapping in mappings {
        let path = std::fs::canonicalize(&mapping.path).unwrap_or(mapping.path);
        let probe = if mapping.instrument.is_percussion() {
            Probe::Percussion
        } else {
            Probe::Melodic
        };
        patches
            .entry(path)
            .or_default()
            .entry(probe)
            .or_default()
            .push(format!(
                "{}.{}",
                mapping.instrument_key, mapping.articulation_key
            ));
    }

    let unique_patches = patches.len();
    let scratch = Scratch::create()?;
    let melodic_midi = scratch.path.join("probe-melodic.mid");
    let drum_midi = scratch.path.join("probe-drums.mid");
    tools::write_atomic(&melodic_midi, &probe_midi(false))?;
    tools::write_atomic(&drum_midi, &probe_midi(true))?;

    let mut reports = Vec::with_capacity(unique_patches);
    for (index, (path, mut mapping_groups)) in patches.into_iter().enumerate() {
        for names in mapping_groups.values_mut() {
            names.sort();
        }
        let mapping_names = mapping_groups
            .values()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let probes = mapping_groups
            .keys()
            .map(|probe| probe.key().to_owned())
            .collect::<Vec<_>>();
        if !path.is_file() {
            reports.push(render_failure(
                &path,
                mapping_names,
                probes,
                "missing",
                format!("SFZ file not found: {}", path.display()),
            ));
            continue;
        }
        let mut probe_reports = Vec::with_capacity(mapping_groups.len());
        for (probe, names) in mapping_groups {
            let midi = match probe {
                Probe::Melodic => &melodic_midi,
                Probe::Percussion => &drum_midi,
            };
            probe_reports.push(certify_probe(
                midi,
                &path,
                &scratch.path,
                index,
                probe,
                names,
                sample_rate,
            )?);
        }
        reports.push(merge_probe_reports(&path, mapping_names, probe_reports));
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
