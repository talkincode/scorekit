//! Active renderer-profile verification. Schema validation proves the YAML is
//! shaped correctly; this module proves each referenced SFZ actually renders,
//! produces audible PCM, repeats deterministically with the pinned tool, and
//! measurably responds to every automation target it declares.
//!
//! Failure handling is a recorded, isolated recheck — not blind retry: a
//! failed comparison (silent or nondeterministic) captures environment
//! diagnostics (load average, tool identity, both render hashes, timings)
//! and re-runs that one patch once; an isolated pass downgrades the failure
//! to a `load_sensitive_flake` warning with the evidence attached, an
//! isolated failure stays a hard failure carrying both attempts' diagnostics.

use crate::composer::{BendEvent, ControlEvent, NoteEvent, ScoreIr, TrackIr};
use crate::error::{Error, Result};
use crate::profile;
use crate::schema::{AutomationTarget, TimeSig};
use crate::{midi, tools};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

const SILENCE_PEAK: u32 = 1;
const DETERMINISM_TOLERANCE: f64 = 1.0e-6;
const CONTROL_RESPONSE_TOLERANCE: f64 = 1.0e-6;
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
    /// Golden certification hash. A single ordinary probe uses its first WAV
    /// hash directly; multi-probe and control-aware patches fold every passing
    /// probe hash into one deterministic digest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render_sha256: Option<String>,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub flake_diagnostics: Vec<FlakeDiagnostics>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub control_probes: Vec<ControlProbeReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControlProbeReport {
    pub target: String,
    pub mappings: Vec<String>,
    pub status: String,
    pub difference_rms_ratio: f64,
    pub deterministic: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render_sha256: Option<[String; 2]>,
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

struct RenderPairFiles {
    paths: [PathBuf; 2],
    cleanup_required: bool,
}

impl RenderPairFiles {
    fn new(a: PathBuf, b: PathBuf) -> Self {
        Self {
            paths: [a, b],
            cleanup_required: true,
        }
    }

    fn remove(mut self) -> Result<()> {
        for path in &self.paths {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(Error::Io {
                        path: path.display().to_string(),
                        source,
                    });
                }
            }
        }
        self.cleanup_required = false;
        Ok(())
    }
}

impl Drop for RenderPairFiles {
    fn drop(&mut self) {
        if self.cleanup_required {
            for path in &self.paths {
                let _ = std::fs::remove_file(path);
            }
        }
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

fn probe_midi(drum_channel: bool, control: Option<(AutomationTarget, ControlVariant)>) -> Vec<u8> {
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
    let mut controls = Vec::new();
    let mut bends = Vec::new();
    if let Some((target, variant)) = control {
        for index in 0..keys.len() {
            let tick = index as u32 * step;
            let high_first = (index % 2 == 0) ^ matches!(variant, ControlVariant::B);
            for (offset, high) in [(0, high_first), (step / 2, !high_first)] {
                if let Some(controller) = target.controller() {
                    controls.push(ControlEvent {
                        tick: tick + offset,
                        controller,
                        value: if high { 96 } else { 32 },
                    });
                } else {
                    bends.push(BendEvent {
                        tick: tick + offset,
                        value: if high { 12_288 } else { 4_096 },
                    });
                }
            }
        }
    }
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
            controls,
            bends,
        }],
    })
}

#[derive(Debug, Clone, Copy)]
enum ControlVariant {
    A,
    B,
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
        control_probes: Vec::new(),
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
    pcm: Pcm,
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
    let outputs = RenderPairFiles::new(a_path.clone(), b_path.clone());
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
    let outcome = PairResult::Rendered(Box::new(PairOutcome {
        verdict,
        peak_abs,
        rms,
        difference_rms_ratio,
        hashes,
        times_ms,
        diagnostics,
        pcm: a,
    }));
    outputs.remove()?;
    Ok(outcome)
}

struct RenderCertification {
    status: String,
    peak_abs: u32,
    rms: f64,
    deterministic: bool,
    difference_rms_ratio: f64,
    render_sha256: Option<String>,
    warnings: Vec<String>,
    flake_diagnostics: Vec<FlakeDiagnostics>,
    error: Option<String>,
    pcm: Option<Pcm>,
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

fn certify_midi(
    midi: &Path,
    path: &Path,
    scratch: &Path,
    index: usize,
    label: &str,
    sample_rate: u32,
) -> Result<RenderCertification> {
    let first_tag = format!("{label}-first");
    let first = match render_pair(midi, path, scratch, index, &first_tag, sample_rate)? {
        PairResult::Failed(error) => {
            return Ok(RenderCertification {
                status: "render_failed".to_owned(),
                peak_abs: 0,
                rms: 0.0,
                deterministic: false,
                difference_rms_ratio: f64::INFINITY,
                render_sha256: None,
                warnings: Vec::new(),
                flake_diagnostics: Vec::new(),
                error: Some(format!("{label} probe: {error}")),
                pcm: None,
            });
        }
        PairResult::Rendered(outcome) => outcome,
    };

    if first.verdict == Verdict::Pass {
        return Ok(RenderCertification {
            status: "ok".to_owned(),
            peak_abs: first.peak_abs,
            rms: first.rms,
            deterministic: true,
            difference_rms_ratio: first.difference_rms_ratio,
            render_sha256: Some(first.hashes[0].clone()),
            warnings: warnings(&first.diagnostics),
            flake_diagnostics: Vec::new(),
            error: None,
            pcm: Some(first.pcm),
        });
    }

    let first_snapshot = flake_snapshot("first", &first);
    let recheck_tag = format!("{label}-recheck");
    let recheck = match render_pair(midi, path, scratch, index, &recheck_tag, sample_rate)? {
        PairResult::Failed(error) => {
            return Ok(RenderCertification {
                status: "render_failed".to_owned(),
                peak_abs: 0,
                rms: 0.0,
                deterministic: false,
                difference_rms_ratio: f64::INFINITY,
                render_sha256: None,
                warnings: Vec::new(),
                flake_diagnostics: vec![first_snapshot],
                error: Some(format!("{label} probe: {error}")),
                pcm: None,
            });
        }
        PairResult::Rendered(outcome) => outcome,
    };

    if recheck.verdict == Verdict::Pass {
        let mut patch_warnings = warnings(&recheck.diagnostics);
        patch_warnings.push(format!(
            "load_sensitive_flake: {label} probe first attempt was {} (RMS ratio {:.8}); \
             isolated recheck passed — see flake_diagnostics",
            first_snapshot.observed_status, first_snapshot.difference_rms_ratio,
        ));
        return Ok(RenderCertification {
            status: "ok".to_owned(),
            peak_abs: recheck.peak_abs,
            rms: recheck.rms,
            deterministic: true,
            difference_rms_ratio: recheck.difference_rms_ratio,
            render_sha256: Some(recheck.hashes[0].clone()),
            warnings: patch_warnings,
            flake_diagnostics: vec![first_snapshot],
            error: None,
            pcm: Some(recheck.pcm),
        });
    }

    let recheck_snapshot = flake_snapshot("recheck", &recheck);
    let error = match recheck.verdict {
        Verdict::Silent => format!("{label} probe produced no audible PCM"),
        _ => format!(
            "{label} probe renders differ (RMS ratio {:.8}); isolated recheck failed too",
            recheck.difference_rms_ratio
        ),
    };
    Ok(RenderCertification {
        status: recheck.verdict.status().to_owned(),
        peak_abs: recheck.peak_abs,
        rms: recheck.rms,
        deterministic: false,
        difference_rms_ratio: recheck.difference_rms_ratio,
        render_sha256: None,
        warnings: warnings(&recheck.diagnostics),
        flake_diagnostics: vec![first_snapshot, recheck_snapshot],
        error: Some(error),
        pcm: None,
    })
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
    let certification = certify_midi(midi, path, scratch, index, probe_name, sample_rate)?;
    Ok(PatchReport {
        path: path.display().to_string(),
        mappings,
        probes: vec![probe_name.to_owned()],
        status: certification.status,
        peak_abs: certification.peak_abs,
        rms: certification.rms,
        deterministic: certification.deterministic,
        difference_rms_ratio: certification.difference_rms_ratio,
        render_sha256: certification.render_sha256,
        warnings: certification.warnings,
        flake_diagnostics: certification.flake_diagnostics,
        control_probes: Vec::new(),
        error: certification.error,
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
            hasher.update(b"\n");
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
        control_probes: Vec::new(),
        error: (!errors.is_empty()).then(|| errors.join("; ")),
    }
}

struct ControlCertification {
    report: ControlProbeReport,
    warnings: Vec<String>,
    flake_diagnostics: Vec<FlakeDiagnostics>,
}

fn certify_control(
    midis: &(PathBuf, PathBuf),
    path: &Path,
    scratch: &Path,
    index: usize,
    target: AutomationTarget,
    mappings: Vec<String>,
    sample_rate: u32,
) -> Result<ControlCertification> {
    let mut variants = Vec::with_capacity(2);
    for (name, midi) in [("a", &midis.0), ("b", &midis.1)] {
        let label = format!("control-{}-{name}", target.key());
        let mut certification = certify_midi(midi, path, scratch, index, &label, sample_rate)?;
        for warning in &mut certification.warnings {
            *warning = format!("control {} variant {name}: {warning}", target.key());
        }
        for diagnostic in &mut certification.flake_diagnostics {
            diagnostic.attempt = format!("control:{}:{name}:{}", target.key(), diagnostic.attempt);
        }
        if certification.status != "ok" {
            let status = certification.status.clone();
            let detail = certification
                .error
                .unwrap_or_else(|| "render probe failed without detail".to_owned());
            let error = format!(
                "declared control `{}` variant {name} failed certification: {detail}",
                target.key()
            );
            return Ok(ControlCertification {
                report: ControlProbeReport {
                    target: target.key().to_owned(),
                    mappings,
                    status,
                    difference_rms_ratio: f64::INFINITY,
                    deterministic: certification.deterministic,
                    render_sha256: None,
                    error: Some(error),
                },
                warnings: certification.warnings,
                flake_diagnostics: certification.flake_diagnostics,
            });
        }
        variants.push(certification);
    }

    let left = variants.remove(0);
    let right = variants.remove(0);
    let ratio = difference_ratio(
        left.pcm.as_ref().expect("passing certification has PCM"),
        right.pcm.as_ref().expect("passing certification has PCM"),
    );
    let responsive = ratio.is_finite() && ratio > CONTROL_RESPONSE_TOLERANCE;
    let mut control_warnings = left.warnings;
    control_warnings.extend(right.warnings);
    let mut flake_diagnostics = left.flake_diagnostics;
    flake_diagnostics.extend(right.flake_diagnostics);
    Ok(ControlCertification {
        report: ControlProbeReport {
            target: target.key().to_owned(),
            mappings,
            status: if responsive { "ok" } else { "unresponsive" }.to_owned(),
            difference_rms_ratio: ratio,
            deterministic: true,
            render_sha256: Some([
                left.render_sha256
                    .expect("passing certification has a render hash"),
                right
                    .render_sha256
                    .expect("passing certification has a render hash"),
            ]),
            error: (!responsive).then(|| {
                if ratio.is_finite() {
                    format!(
                        "declared control `{}` produced no measurable PCM change",
                        target.key()
                    )
                } else {
                    format!(
                        "declared control `{}` produced incomparable PCM formats",
                        target.key()
                    )
                }
            }),
        },
        warnings: control_warnings,
        flake_diagnostics,
    })
}

fn extend_certification_hash(base: &str, control: &ControlProbeReport) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"scorekit-profile-certification-v1\0");
    hasher.update(base.as_bytes());
    hasher.update([0]);
    hasher.update(control.target.as_bytes());
    hasher.update([0]);
    for mapping in &control.mappings {
        hasher.update(mapping.as_bytes());
        hasher.update([0]);
    }
    for hash in control
        .render_sha256
        .as_ref()
        .expect("passing control probe has render hashes")
    {
        hasher.update(hash.as_bytes());
        hasher.update([0]);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Default)]
struct PatchRequirements {
    probes: BTreeMap<Probe, Vec<String>>,
    controls: BTreeMap<AutomationTarget, Vec<String>>,
}

pub fn check(profile_path: &Path, sample_rate: u32) -> Result<Report> {
    let loaded = profile::load_profile(profile_path)?;
    let profile_dir = profile_path.parent().unwrap_or_else(|| Path::new("."));
    let mappings = loaded.resolved_mappings(profile_dir);
    let mapping_count = mappings.len();

    let mut patches: BTreeMap<PathBuf, PatchRequirements> = BTreeMap::new();
    for mapping in mappings {
        let path = std::fs::canonicalize(&mapping.path).unwrap_or(mapping.path);
        let probe = if mapping.instrument.is_percussion() {
            Probe::Percussion
        } else {
            Probe::Melodic
        };
        let name = format!("{}.{}", mapping.instrument_key, mapping.articulation_key);
        let requirements = patches.entry(path).or_default();
        requirements
            .probes
            .entry(probe)
            .or_default()
            .push(name.clone());
        for target in mapping.controls {
            requirements
                .controls
                .entry(target)
                .or_default()
                .push(name.clone());
        }
    }

    let unique_patches = patches.len();
    let scratch = Scratch::create()?;
    let melodic_midi = scratch.path.join("probe-melodic.mid");
    let drum_midi = scratch.path.join("probe-drums.mid");
    tools::write_atomic(&melodic_midi, &probe_midi(false, None))?;
    tools::write_atomic(&drum_midi, &probe_midi(true, None))?;
    let mut control_midis = BTreeMap::new();
    for target in patches
        .values()
        .flat_map(|requirements| requirements.controls.keys())
    {
        if control_midis.contains_key(target) {
            continue;
        }
        let a = scratch.path.join(format!("control-{}-a.mid", target.key()));
        let b = scratch.path.join(format!("control-{}-b.mid", target.key()));
        tools::write_atomic(&a, &probe_midi(false, Some((*target, ControlVariant::A))))?;
        tools::write_atomic(&b, &probe_midi(false, Some((*target, ControlVariant::B))))?;
        control_midis.insert(*target, (a, b));
    }

    let mut reports = Vec::with_capacity(unique_patches);
    for (index, (path, mut requirements)) in patches.into_iter().enumerate() {
        for names in requirements.probes.values_mut() {
            names.sort();
        }
        for names in requirements.controls.values_mut() {
            names.sort();
            names.dedup();
        }
        let mapping_names = requirements
            .probes
            .values()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let probes = requirements
            .probes
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
        let mut probe_reports = Vec::with_capacity(requirements.probes.len());
        for (probe, names) in requirements.probes {
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
        let mut report = merge_probe_reports(&path, mapping_names, probe_reports);
        if report.status == "ok" {
            let mut certification_hash = report.render_sha256.take();
            let mut control_status = None;
            let mut control_errors = Vec::new();
            for (target, names) in requirements.controls {
                let certification = certify_control(
                    control_midis
                        .get(&target)
                        .expect("required control MIDI was written"),
                    &path,
                    &scratch.path,
                    index,
                    target,
                    names,
                    sample_rate,
                )?;
                report.warnings.extend(certification.warnings);
                report
                    .flake_diagnostics
                    .extend(certification.flake_diagnostics);
                if certification.report.status != "ok" {
                    let status = if certification.report.status == "render_failed" {
                        "render_failed".to_owned()
                    } else {
                        format!("control_{}", certification.report.status)
                    };
                    if control_status.is_none() || status == "render_failed" {
                        control_status = Some(status);
                    }
                    report.deterministic &= certification.report.deterministic;
                    certification_hash = None;
                    control_errors.extend(certification.report.error.iter().cloned());
                } else if let Some(base) = certification_hash.as_deref() {
                    certification_hash =
                        Some(extend_certification_hash(base, &certification.report));
                }
                report.control_probes.push(certification.report);
            }
            if let Some(status) = control_status {
                report.status = status;
            }
            report.render_sha256 = certification_hash;
            report.error = (!control_errors.is_empty()).then(|| control_errors.join("; "));
        }
        reports.push(report);
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
