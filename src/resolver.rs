//! Instrument resolution: map each track's requested instrument onto what a
//! sound source actually provides — explainably, deterministically, and
//! never by silently absorbing missing instruments into strings.
//!
//! Resolution order (per request): exact profile mapping → alias-normalized
//! mapping → scored same-family candidates → caller-allowed related/explicit
//! families → `missing`. Every non-exact outcome carries a score, reasons
//! and warnings; builds surface them as WARN lines, `meta.json` embeds the
//! full report, and `scorekit inspect-instruments` prints it standalone.
//!
//! Determinism: scoring is fixed-order arithmetic over static registry data;
//! candidates are ordered by (score desc, canonical key asc) via `total_cmp`,
//! so the same scene + profile + policy always resolves identically.

use crate::error::{Error, Location, Result};
use crate::instrument::{self, Family, Role, spec};
use crate::schema::{
    Articulation, Instrument, Pattern, Scene, Track, articulation_key, instrument_key,
    parse_articulation_key, parse_instrument_key,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Scoring weights (sum = 1.0). Family dominates by design: an in-family
/// candidate always outranks a cross-family one on this axis, and strings
/// get no global bonus anywhere.
const W_FAMILY: f32 = 0.30;
const W_RANGE: f32 = 0.20;
const W_ARTICULATION: f32 = 0.15;
const W_ENVELOPE: f32 = 0.15;
const W_ROLE: f32 = 0.15;
const W_TIMBRE: f32 = 0.05;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, clap::ValueEnum, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum FallbackMode {
    /// No substitution at all: an unmapped instrument fails the command.
    Strict,
    /// Same-family substitution only (default): no cross-family, no synth
    /// stand-ins, minimum score 0.70.
    #[default]
    Conservative,
    /// Also reach related families (see `instrument::family_affinity`) and
    /// synth stand-ins. Substituting *into* strings still requires an
    /// explicit `allowed_families` entry.
    Flexible,
}

/// Caller-controlled substitution policy.
#[derive(Debug, Clone, PartialEq)]
pub struct FallbackPolicy {
    pub mode: FallbackMode,
    /// Candidates scoring below this are rejected (0.0..=1.0).
    pub minimum_score: f32,
    /// Reach related families without listing them one by one.
    pub allow_cross_family: bool,
    /// Allow synthetic timbres to stand in for acoustic requests.
    /// Synth-for-synth substitution is always allowed.
    pub allow_synth: bool,
    /// Families explicitly permitted as cross-family fallback targets.
    /// This is also the only way to permit substituting *into* strings.
    pub allowed_families: BTreeSet<Family>,
    /// Families never used as fallback targets, overriding everything else.
    pub excluded_families: BTreeSet<Family>,
}

impl FallbackPolicy {
    pub fn for_mode(mode: FallbackMode) -> Self {
        let (allow_cross_family, allow_synth) = match mode {
            FallbackMode::Strict | FallbackMode::Conservative => (false, false),
            FallbackMode::Flexible => (true, true),
        };
        FallbackPolicy {
            mode,
            minimum_score: 0.70,
            allow_cross_family,
            allow_synth,
            allowed_families: BTreeSet::new(),
            excluded_families: BTreeSet::new(),
        }
    }
}

impl Default for FallbackPolicy {
    fn default() -> Self {
        Self::for_mode(FallbackMode::Conservative)
    }
}

/// External resolver configuration (`--resolver resolver.yaml`); every field
/// optional, layered over the selected mode's defaults. An explicit
/// `--fallback-mode` flag beats `default_mode`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolverConfig {
    /// Fallback mode when the command line does not pass `--fallback-mode`.
    #[serde(default)]
    pub default_mode: Option<FallbackMode>,
    /// Minimum candidate score, 0.0..=1.0.
    #[serde(default)]
    pub minimum_score: Option<f32>,
    #[serde(default)]
    pub allow_cross_family: Option<bool>,
    #[serde(default)]
    pub allow_synth: Option<bool>,
    /// Families explicitly permitted as cross-family fallback targets (the
    /// only way to permit substituting into strings).
    #[serde(default)]
    pub allowed_families: Option<Vec<Family>>,
    /// Families never used as fallback targets.
    #[serde(default)]
    pub excluded_families: Option<Vec<Family>>,
    /// Deprecated migration switch: treats strings as an allowed fallback
    /// family, restoring the old "missing instruments become strings"
    /// behavior. Must stay `false` outside temporary migrations; every use
    /// emits a deprecation warning.
    #[serde(default)]
    pub legacy_string_fallback: Option<bool>,
}

/// Read and validate a resolver configuration file.
pub fn load_config(path: &Path) -> Result<ResolverConfig> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })?;
    let config: ResolverConfig = serde_yaml_ng::from_str(&text).map_err(|e| Error::Parse {
        message: format!("invalid resolver config: {e}"),
        location: e.location().map(|l| Location {
            line: l.line(),
            column: l.column(),
        }),
    })?;
    if let Some(m) = config.minimum_score
        && !(m.is_finite() && (0.0..=1.0).contains(&m))
    {
        return Err(Error::Validation {
            path: "minimum_score".to_owned(),
            message: format!("{m} out of range 0.0..=1.0"),
        });
    }
    Ok(config)
}

/// JSON Schema of the resolver configuration, for agent consumption.
pub fn schema_json() -> String {
    let schema = schemars::schema_for!(ResolverConfig);
    serde_json::to_string_pretty(&schema).expect("schema serializes")
}

/// Assemble the effective policy: mode defaults ← config file ← CLI flag.
/// Returns the policy plus any deprecation warnings to print.
pub fn build_policy(
    mode_flag: Option<FallbackMode>,
    config: Option<&ResolverConfig>,
) -> (FallbackPolicy, Vec<String>) {
    let mode = mode_flag
        .or(config.and_then(|c| c.default_mode))
        .unwrap_or_default();
    let mut policy = FallbackPolicy::for_mode(mode);
    let mut warnings = Vec::new();
    if let Some(c) = config {
        if let Some(m) = c.minimum_score {
            policy.minimum_score = m;
        }
        if let Some(x) = c.allow_cross_family {
            policy.allow_cross_family = x;
        }
        if let Some(x) = c.allow_synth {
            policy.allow_synth = x;
        }
        if let Some(fs) = &c.allowed_families {
            policy.allowed_families = fs.iter().copied().collect();
        }
        if let Some(fs) = &c.excluded_families {
            policy.excluded_families = fs.iter().copied().collect();
        }
        if c.legacy_string_fallback == Some(true) {
            policy.allowed_families.insert(Family::Strings);
            warnings.push(
                "WARN legacy_string_fallback is deprecated and only for temporary migration: \
                 strings are treated as an allowed fallback family; prefer explicit \
                 `allowed_families: [strings]` or fix the missing instruments"
                    .to_owned(),
            );
        }
    }
    (policy, warnings)
}

/// What a sound source provides: instrument → articulations with dedicated
/// samples. `None` elsewhere means "GM world" (SF2 backends): every
/// instrument in the vocabulary is available.
pub type Available = BTreeMap<Instrument, BTreeSet<Articulation>>;

/// Availability as declared by a renderer profile. Explicit profile mappings
/// are the profile author's own (visible, diffable) choices — the resolver
/// treats them as ground truth and only manages what the profile does *not*
/// map.
pub fn available_from_profile(profile: &crate::profile::Profile) -> Available {
    let mut out = Available::new();
    for (ikey, arts) in &profile.instruments {
        let Some(instrument) = parse_instrument_key(ikey) else {
            continue; // load_profile validation rejects unknown keys
        };
        let set = arts
            .keys()
            .filter_map(|a| parse_articulation_key(a))
            .collect();
        out.insert(instrument, set);
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionStatus {
    /// Requested instrument is available as spelled.
    Exact,
    /// Available after alias normalization.
    Alias,
    /// Substituted by a scored candidate under the active policy.
    Fallback,
    /// Fallback was allowed but no candidate met the policy.
    Missing,
    /// Fallback disabled (strict mode) and the instrument is unavailable.
    Rejected,
}

/// One scored substitution candidate.
#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub instrument: String,
    pub score: f32,
    pub reasons: Vec<String>,
    /// Why the policy made this candidate ineligible; `None` = eligible.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected: Option<String>,
}

/// The full, explainable outcome for one track's instrument request.
#[derive(Debug, Clone, Serialize)]
pub struct Resolution {
    pub track: usize,
    /// The instrument as requested (raw spelling when known).
    pub requested: String,
    /// Canonical instrument key after alias normalization.
    pub canonical: String,
    /// Canonical key of the instrument that will actually sound.
    pub resolved: Option<String>,
    pub status: ResolutionStatus,
    /// 1.0 for exact/alias; the candidate score for fallback; 0.0 otherwise.
    pub score: f32,
    pub reasons: Vec<String>,
    pub warnings: Vec<String>,
    /// Best-scoring candidate that was still not used (missing/rejected).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_candidate: Option<Candidate>,
    /// Full scored candidate list (verbose mode only).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<Candidate>,
    #[serde(skip)]
    target: Option<Instrument>,
}

impl Resolution {
    /// The instrument to render with, when resolution succeeded.
    pub fn target(&self) -> Option<Instrument> {
        self.target
    }

    pub fn failed(&self) -> bool {
        matches!(
            self.status,
            ResolutionStatus::Missing | ResolutionStatus::Rejected
        )
    }
}

/// Role a track's pattern implies, for candidate scoring.
fn role_of(track: &Track) -> Role {
    match track.pattern {
        Pattern::Melody => Role::Melody,
        Pattern::Bass => Role::Bass,
        Pattern::Drums => Role::Rhythm,
        Pattern::Sustain | Pattern::Arpeggio => Role::HarmonicSupport,
    }
}

fn round3(x: f32) -> f32 {
    (x * 1000.0).round() / 1000.0
}

/// Score one candidate against a request and decide its policy eligibility.
fn score_candidate(
    requested: Instrument,
    articulation: Articulation,
    role: Role,
    candidate: Instrument,
    candidate_arts: &BTreeSet<Articulation>,
    policy: &FallbackPolicy,
) -> Candidate {
    let req = spec(requested);
    let cand = spec(candidate);
    let mut reasons = Vec::new();
    let mut rejected: Option<String> = None;
    fn reject(r: &str, slot: &mut Option<String>) {
        if slot.is_none() {
            *slot = Some(r.to_owned());
        }
    }

    // Hard eligibility gates. All are still *scored* so reports can show
    // what would have been chosen and why it was not.
    if candidate == Instrument::Drums || requested == Instrument::Drums {
        reject("percussion_kit_is_never_substituted", &mut rejected);
    }
    if policy.excluded_families.contains(&cand.family) {
        reject("family_excluded", &mut rejected);
    }
    if cand.synthetic
        && !req.synthetic
        && !(policy.allow_synth || policy.allowed_families.contains(&Family::Synth))
    {
        reject("synth_forbidden", &mut rejected);
    }
    let affinity = instrument::family_affinity(req.family, cand.family);
    let family_score = if cand.family == req.family {
        if cand.subfamily == req.subfamily {
            reasons.push("same_subfamily".to_owned());
        } else {
            reasons.push("same_family".to_owned());
        }
        if cand.subfamily == req.subfamily {
            1.0
        } else {
            0.85
        }
    } else {
        // Substituting *into* strings always needs an explicit allowance —
        // strings must never be the default absorber for missing brass,
        // woodwinds or plucked instruments.
        if cand.family == Family::Strings && !policy.allowed_families.contains(&Family::Strings) {
            reject(
                "strings_fallback_requires_explicit_allowance",
                &mut rejected,
            );
        }
        if policy.allowed_families.contains(&cand.family) {
            reasons.push("cross_family_allowed".to_owned());
            affinity.max(0.30)
        } else if policy.allow_cross_family && affinity > 0.0 {
            reasons.push("related_family".to_owned());
            affinity
        } else {
            reject(
                if policy.allow_cross_family {
                    "unrelated_family"
                } else {
                    "cross_family_forbidden"
                },
                &mut rejected,
            );
            affinity
        }
    };

    let range = req.range_coverage_by(&cand);
    if range == 0.0 {
        reject("incompatible_range", &mut rejected);
    }
    reasons.push(
        if range >= 0.9 {
            "compatible_range"
        } else if range > 0.0 {
            "partial_range"
        } else {
            "incompatible_range"
        }
        .to_owned(),
    );

    // Dedicated sample beats idiomatic-but-sustain-served (the profile's
    // silent sustain fallback) beats an articulation the instrument does not
    // idiomatically play at all.
    let articulation_score = if candidate_arts.contains(&articulation) {
        reasons.push("articulation_supported".to_owned());
        1.0
    } else if cand.articulations.contains(&articulation) {
        reasons.push("articulation_via_sustain_fallback".to_owned());
        0.5
    } else {
        reasons.push("articulation_unsupported".to_owned());
        0.0
    };

    let envelope = 0.4 * req.envelope.attack.similarity(cand.envelope.attack)
        + 0.2 * req.envelope.release.similarity(cand.envelope.release)
        + 0.4 * f32::from(u8::from(req.envelope.sustain == cand.envelope.sustain));
    reasons.push(
        if envelope >= 0.7 {
            "envelope_similar"
        } else {
            "envelope_differs"
        }
        .to_owned(),
    );

    let role_score = if cand.roles.contains(&role) {
        reasons.push("role_compatible".to_owned());
        1.0
    } else {
        reasons.push("role_mismatch".to_owned());
        0.0
    };

    let timbre = 1.0
        - ((req.timbre.brightness - cand.timbre.brightness).abs()
            + (req.timbre.warmth - cand.timbre.warmth).abs())
            / 2.0;

    let score = W_FAMILY * family_score
        + W_RANGE * range
        + W_ARTICULATION * articulation_score
        + W_ENVELOPE * envelope
        + W_ROLE * role_score
        + W_TIMBRE * timbre;
    Candidate {
        instrument: instrument_key(candidate),
        score: round3(score),
        reasons,
        rejected,
    }
}

/// Resolve one track against the available instrument set.
/// `raw` is the spelling as written in the scene file, when known.
fn resolve_track(
    index: usize,
    track: &Track,
    raw: Option<&str>,
    available: Option<&Available>,
    policy: &FallbackPolicy,
    verbose: bool,
) -> Resolution {
    let canonical = instrument_key(track.instrument);
    let requested = raw.map(str::to_owned).unwrap_or_else(|| canonical.clone());
    let aliased = requested != canonical;
    let base_status = if aliased {
        ResolutionStatus::Alias
    } else {
        ResolutionStatus::Exact
    };
    let mut alias_reasons = Vec::new();
    if aliased {
        alias_reasons.push("alias_normalized".to_owned());
    }

    // GM world: the SF2 soundfont serves every GM program, so everything in
    // the vocabulary is available by construction.
    let Some(available) = available else {
        return Resolution {
            track: index,
            requested,
            canonical: canonical.clone(),
            resolved: Some(canonical),
            status: base_status,
            score: 1.0,
            reasons: {
                let mut r = vec!["available".to_owned()];
                r.extend(alias_reasons);
                r
            },
            warnings: Vec::new(),
            best_candidate: None,
            candidates: Vec::new(),
            target: Some(track.instrument),
        };
    };

    if let Some(arts) = available.get(&track.instrument) {
        let mut warnings = Vec::new();
        if track.articulation != Articulation::Sustain && !arts.contains(&track.articulation) {
            warnings.push(format!(
                "articulation `{}` has no dedicated sample; the `sustain` mapping will sound",
                articulation_key(track.articulation)
            ));
        }
        return Resolution {
            track: index,
            requested,
            canonical: canonical.clone(),
            resolved: Some(canonical),
            status: base_status,
            score: 1.0,
            reasons: {
                let mut r = vec!["mapped_by_profile".to_owned()];
                r.extend(alias_reasons);
                r
            },
            warnings,
            best_candidate: None,
            candidates: Vec::new(),
            target: Some(track.instrument),
        };
    }

    // Unmapped: generate deterministic scored candidates.
    let role = role_of(track);
    let mut candidates: Vec<Candidate> = available
        .iter()
        .map(|(&cand, arts)| {
            score_candidate(
                track.instrument,
                track.articulation,
                role,
                cand,
                arts,
                policy,
            )
        })
        .collect();
    // BTreeMap iteration is already key-ordered; sort by score with the key
    // as tiebreak so equal scores can never flip between runs.
    candidates.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.instrument.cmp(&b.instrument))
    });

    let mut warnings = vec!["requested_instrument_not_available".to_owned()];
    let best_overall = candidates.first().cloned();

    if policy.mode == FallbackMode::Strict {
        warnings.push("fallback_disabled_by_strict_mode".to_owned());
        return Resolution {
            track: index,
            requested,
            canonical,
            resolved: None,
            status: ResolutionStatus::Rejected,
            score: 0.0,
            reasons: alias_reasons,
            warnings,
            best_candidate: best_overall,
            candidates: if verbose { candidates } else { Vec::new() },
            target: None,
        };
    }

    let best_eligible = candidates
        .iter()
        .find(|c| c.rejected.is_none() && c.score >= policy.minimum_score)
        .cloned();
    match best_eligible {
        Some(chosen) => {
            let target = parse_instrument_key(&chosen.instrument);
            let mut reasons = alias_reasons;
            reasons.extend(chosen.reasons.iter().cloned());
            Resolution {
                track: index,
                requested,
                canonical,
                resolved: Some(chosen.instrument.clone()),
                status: ResolutionStatus::Fallback,
                score: chosen.score,
                reasons,
                warnings,
                best_candidate: None,
                candidates: if verbose { candidates } else { Vec::new() },
                target,
            }
        }
        None => {
            warnings.push("no_candidate_satisfies_fallback_policy".to_owned());
            Resolution {
                track: index,
                requested,
                canonical,
                resolved: None,
                status: ResolutionStatus::Missing,
                score: 0.0,
                reasons: alias_reasons,
                warnings,
                best_candidate: best_overall,
                candidates: if verbose { candidates } else { Vec::new() },
                target: None,
            }
        }
    }
}

/// Resolution of every track in a scene, in track order.
#[derive(Debug, Clone, Serialize)]
pub struct SceneResolution {
    pub mode: FallbackMode,
    pub minimum_score: f32,
    pub tracks: Vec<Resolution>,
}

impl SceneResolution {
    pub fn counts(&self) -> BTreeMap<&'static str, usize> {
        let mut counts = BTreeMap::from([
            ("exact", 0),
            ("alias", 0),
            ("fallback", 0),
            ("missing", 0),
            ("rejected", 0),
        ]);
        for r in &self.tracks {
            let key = match r.status {
                ResolutionStatus::Exact => "exact",
                ResolutionStatus::Alias => "alias",
                ResolutionStatus::Fallback => "fallback",
                ResolutionStatus::Missing => "missing",
                ResolutionStatus::Rejected => "rejected",
            };
            *counts.get_mut(key).expect("all statuses present") += 1;
        }
        counts
    }

    /// WARN lines for every substitution and every unresolved instrument —
    /// one line each, so default builds stay quiet unless something needs
    /// attention (full candidate lists live behind `inspect-instruments
    /// --verbose`).
    pub fn warn_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for r in &self.tracks {
            match r.status {
                ResolutionStatus::Fallback => lines.push(format!(
                    "WARN instrument fallback: track={} requested={} resolved={} score={:.2} reason={}",
                    r.track,
                    r.requested,
                    r.resolved.as_deref().unwrap_or("-"),
                    r.score,
                    r.reasons.join(",")
                )),
                ResolutionStatus::Missing | ResolutionStatus::Rejected => {
                    let best = r.best_candidate.as_ref();
                    lines.push(format!(
                        "WARN instrument missing: track={} requested={} fallback_rejected=true best_candidate={} candidate_score={} minimum_score={:.2}",
                        r.track,
                        r.requested,
                        best.map(|c| c.instrument.as_str()).unwrap_or("none"),
                        best.map(|c| format!("{:.2}", c.score))
                            .unwrap_or_else(|| "-".to_owned()),
                        self.minimum_score
                    ));
                }
                _ => {}
            }
        }
        lines
    }

    /// Human-readable report for `inspect-instruments`: one line per track,
    /// candidate lines when verbose resolution captured them, and a summary.
    pub fn human_report(&self) -> String {
        let mut out = Vec::new();
        for r in &self.tracks {
            let status = match r.status {
                ResolutionStatus::Exact => "exact",
                ResolutionStatus::Alias => "alias",
                ResolutionStatus::Fallback => "fallback",
                ResolutionStatus::Missing => "missing",
                ResolutionStatus::Rejected => "rejected",
            };
            let mut line = format!(
                "tracks[{}]: {} -> {} ({status}, score {:.2})",
                r.track,
                r.requested,
                r.resolved.as_deref().unwrap_or("∅"),
                r.score,
            );
            if !r.reasons.is_empty() {
                line.push_str(&format!(" reasons: {}", r.reasons.join(",")));
            }
            out.push(line);
            for w in &r.warnings {
                out.push(format!("  warning: {w}"));
            }
            for c in &r.candidates {
                out.push(format!(
                    "  candidate: {} score {:.2}{}{}",
                    c.instrument,
                    c.score,
                    if c.reasons.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", c.reasons.join(","))
                    },
                    c.rejected
                        .as_deref()
                        .map(|why| format!(" rejected: {why}"))
                        .unwrap_or_default(),
                ));
            }
        }
        let counts = self.counts();
        out.push(format!(
            "summary: {} exact, {} alias, {} fallback, {} missing, {} rejected",
            counts["exact"],
            counts["alias"],
            counts["fallback"],
            counts["missing"],
            counts["rejected"],
        ));
        out.join("\n")
    }

    /// The `instrument_resolution` object embedded in `meta.json` and the
    /// suite manifest, and the core of `inspect-instruments` output.
    pub fn to_json(&self) -> serde_json::Value {
        let counts = self.counts();
        let missing: Vec<&str> = self
            .tracks
            .iter()
            .filter(|r| r.failed())
            .map(|r| r.canonical.as_str())
            .collect();
        let fallbacks: Vec<serde_json::Value> = self
            .tracks
            .iter()
            .filter(|r| matches!(r.status, ResolutionStatus::Fallback))
            .map(|r| {
                serde_json::json!({
                    "requested": r.requested,
                    "resolved": r.resolved,
                    "score": r.score,
                })
            })
            .collect();
        serde_json::json!({
            "mode": self.mode,
            "summary": counts,
            "missing_instruments": missing,
            "fallbacks": fallbacks,
            "tracks": self.tracks,
        })
    }

    /// Aggregate unresolved tracks into one actionable error carrying the
    /// full machine-readable report.
    pub fn to_error(&self, scene: &str) -> Option<Error> {
        let failed: Vec<&Resolution> = self.tracks.iter().filter(|r| r.failed()).collect();
        if failed.is_empty() {
            return None;
        }
        let mut porcelain: Vec<String> = failed
            .iter()
            .map(|r| {
                let best = r
                    .best_candidate
                    .as_ref()
                    .map(|c| {
                        format!(
                            " (best candidate `{}` score {:.2}{})",
                            c.instrument,
                            c.score,
                            c.rejected
                                .as_deref()
                                .map(|why| format!(", rejected: {why}"))
                                .unwrap_or_default()
                        )
                    })
                    .unwrap_or_default();
                format!(
                    "tracks[{}]: `{}` is not available{best}",
                    r.track, r.requested
                )
            })
            .collect();
        porcelain.push(match self.mode {
            FallbackMode::Strict => "hint: strict mode performs no substitution; map the \
                                     instrument in the renderer profile or drop --fallback-mode \
                                     strict"
                .to_owned(),
            _ => "hint: map the instrument in the renderer profile, pick another instrument, or \
                  widen the fallback policy (see `scorekit inspect-instruments`)"
                .to_owned(),
        });
        Some(Error::Resolution {
            scene: scene.to_owned(),
            count: failed.len(),
            porcelain,
            report: self.to_json(),
        })
    }
}

/// Resolve every track of a scene. `raws[i]` carries the instrument
/// spelling as written in the file when the caller has it (inspect command);
/// builds pass `None` and report canonical names.
pub fn resolve_scene(
    scene: &Scene,
    raws: Option<&[Option<String>]>,
    available: Option<&Available>,
    policy: &FallbackPolicy,
    verbose: bool,
) -> SceneResolution {
    let tracks = scene
        .tracks
        .iter()
        .enumerate()
        .map(|(i, track)| {
            let raw = raws
                .and_then(|r| r.get(i))
                .and_then(|r| r.as_deref())
                .filter(|r| instrument::resolve_name(r).is_some());
            resolve_track(i, track, raw, available, policy, verbose)
        })
        .collect();
    SceneResolution {
        mode: policy.mode,
        minimum_score: policy.minimum_score,
        tracks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(instrument: Instrument, pattern: Pattern, articulation: Articulation) -> Track {
        serde_yaml_ng::from_str(&format!(
            "instrument: {}\npattern: {}\narticulation: {}",
            instrument_key(instrument),
            serde_json::to_value(pattern).unwrap().as_str().unwrap(),
            articulation_key(articulation),
        ))
        .unwrap()
    }

    fn avail(instruments: &[Instrument]) -> Available {
        instruments
            .iter()
            .map(|&i| {
                (
                    i,
                    BTreeSet::from([Articulation::Sustain, Articulation::Staccato]),
                )
            })
            .collect()
    }

    fn resolve(
        t: &Track,
        raw: Option<&str>,
        available: Option<&Available>,
        policy: &FallbackPolicy,
    ) -> Resolution {
        resolve_track(0, t, raw, available, policy, false)
    }

    #[test]
    fn mapped_instrument_resolves_exact() {
        let a = avail(&[Instrument::Violin]);
        let t = track(Instrument::Violin, Pattern::Sustain, Articulation::Sustain);
        let r = resolve(&t, None, Some(&a), &FallbackPolicy::default());
        assert!(matches!(r.status, ResolutionStatus::Exact));
        assert_eq!(r.resolved.as_deref(), Some("violin"));
        assert_eq!(r.score, 1.0);
        assert_eq!(r.target(), Some(Instrument::Violin));
    }

    #[test]
    fn alias_spelling_reports_alias_status() {
        let a = avail(&[Instrument::Horn]);
        let t = track(Instrument::Horn, Pattern::Sustain, Articulation::Sustain);
        let r = resolve(
            &t,
            Some("french_horn"),
            Some(&a),
            &FallbackPolicy::default(),
        );
        assert!(matches!(r.status, ResolutionStatus::Alias));
        assert_eq!(r.requested, "french_horn");
        assert_eq!(r.canonical, "horn");
        assert_eq!(r.resolved.as_deref(), Some("horn"));
        assert!(r.reasons.iter().any(|x| x == "alias_normalized"));
    }

    #[test]
    fn gm_world_serves_everything() {
        let t = track(Instrument::Ocarina, Pattern::Sustain, Articulation::Sustain);
        let r = resolve(&t, None, None, &FallbackPolicy::default());
        assert!(matches!(r.status, ResolutionStatus::Exact));
        assert_eq!(r.score, 1.0);
    }

    #[test]
    fn same_family_fallback_is_scored_and_explained() {
        let a = avail(&[Instrument::Violin, Instrument::Cello]);
        let t = track(Instrument::Viola, Pattern::Sustain, Articulation::Sustain);
        let r = resolve(&t, None, Some(&a), &FallbackPolicy::default());
        assert!(matches!(r.status, ResolutionStatus::Fallback));
        let resolved = r.resolved.as_deref().unwrap();
        assert!(resolved == "violin" || resolved == "cello", "{resolved}");
        assert!(r.score >= 0.70, "{}", r.score);
        assert!(r.reasons.iter().any(|x| x == "same_subfamily"));
        assert!(
            r.warnings
                .iter()
                .any(|x| x == "requested_instrument_not_available")
        );
    }

    #[test]
    fn brass_request_never_falls_back_to_strings_by_default() {
        let a = avail(&[Instrument::Strings, Instrument::Violin, Instrument::Cello]);
        let t = track(Instrument::Horn, Pattern::Sustain, Articulation::Sustain);
        // Conservative: cross-family is off entirely.
        let r = resolve(&t, None, Some(&a), &FallbackPolicy::default());
        assert!(
            matches!(r.status, ResolutionStatus::Missing),
            "{:?}",
            r.status
        );
        assert!(r.resolved.is_none());
        // Flexible: cross-family on — strings still require explicit allowance.
        let flexible = FallbackPolicy::for_mode(FallbackMode::Flexible);
        let r = resolve(&t, None, Some(&a), &flexible);
        assert!(
            matches!(r.status, ResolutionStatus::Missing),
            "{:?}",
            r.status
        );
        let best = r.best_candidate.expect("best candidate reported");
        assert_eq!(
            best.rejected.as_deref(),
            Some("strings_fallback_requires_explicit_allowance")
        );
        // Explicit caller opt-in is the only path to strings.
        let mut allowed = flexible.clone();
        allowed.allowed_families.insert(Family::Strings);
        let r = resolve(&t, None, Some(&a), &allowed);
        assert!(matches!(r.status, ResolutionStatus::Fallback));
        assert!(r.reasons.iter().any(|x| x == "cross_family_allowed"));
    }

    #[test]
    fn flexible_mode_reaches_related_synth_but_conservative_does_not() {
        let a = avail(&[Instrument::WarmPad]);
        let t = track(Instrument::Horn, Pattern::Sustain, Articulation::Sustain);
        let r = resolve(&t, None, Some(&a), &FallbackPolicy::default());
        assert!(matches!(r.status, ResolutionStatus::Missing));
        let r = resolve(
            &t,
            None,
            Some(&a),
            &FallbackPolicy::for_mode(FallbackMode::Flexible),
        );
        assert!(
            matches!(r.status, ResolutionStatus::Fallback),
            "{:?}",
            r.warnings
        );
        assert_eq!(r.resolved.as_deref(), Some("warm_pad"));
        assert!(r.reasons.iter().any(|x| x == "related_family"));
    }

    #[test]
    fn synth_gate_blocks_synthetic_candidates_for_acoustic_requests() {
        let a = avail(&[Instrument::SynthStrings]);
        let t = track(Instrument::Viola, Pattern::Sustain, Articulation::Sustain);
        let mut policy = FallbackPolicy {
            allow_synth: false,
            ..FallbackPolicy::default()
        };
        let r = resolve(&t, None, Some(&a), &policy);
        assert!(matches!(r.status, ResolutionStatus::Missing));
        assert_eq!(
            r.best_candidate.unwrap().rejected.as_deref(),
            Some("synth_forbidden")
        );
        // Same family + allow_synth: passes.
        policy.allow_synth = true;
        let r = resolve(&t, None, Some(&a), &policy);
        assert!(matches!(r.status, ResolutionStatus::Fallback));
        // Synth-for-synth needs no gate.
        let a = avail(&[Instrument::WarmPad]);
        let t = track(Instrument::Pad, Pattern::Sustain, Articulation::Sustain);
        let r = resolve(&t, None, Some(&a), &FallbackPolicy::default());
        assert!(matches!(r.status, ResolutionStatus::Fallback));
    }

    #[test]
    fn excluded_families_are_hard_filtered() {
        let a = avail(&[Instrument::Cello]);
        let t = track(Instrument::Viola, Pattern::Sustain, Articulation::Sustain);
        let mut policy = FallbackPolicy::default();
        policy.excluded_families.insert(Family::Strings);
        let r = resolve(&t, None, Some(&a), &policy);
        assert!(matches!(r.status, ResolutionStatus::Missing));
        assert_eq!(
            r.best_candidate.unwrap().rejected.as_deref(),
            Some("family_excluded")
        );
    }

    #[test]
    fn strict_mode_rejects_without_substitution() {
        let a = avail(&[Instrument::Violin]);
        let t = track(Instrument::Viola, Pattern::Sustain, Articulation::Sustain);
        let r = resolve(
            &t,
            None,
            Some(&a),
            &FallbackPolicy::for_mode(FallbackMode::Strict),
        );
        assert!(matches!(r.status, ResolutionStatus::Rejected));
        assert!(r.resolved.is_none());
        assert!(
            r.warnings
                .iter()
                .any(|x| x == "fallback_disabled_by_strict_mode")
        );
        assert!(r.best_candidate.is_some());
    }

    #[test]
    fn minimum_score_rejects_weak_candidates() {
        let a = avail(&[Instrument::Violin]);
        let t = track(Instrument::Viola, Pattern::Sustain, Articulation::Sustain);
        let policy = FallbackPolicy {
            minimum_score: 0.99,
            ..FallbackPolicy::default()
        };
        let r = resolve(&t, None, Some(&a), &policy);
        assert!(matches!(r.status, ResolutionStatus::Missing));
        assert!(
            r.warnings
                .iter()
                .any(|x| x == "no_candidate_satisfies_fallback_policy")
        );
    }

    #[test]
    fn drums_are_never_substituted_in_either_direction() {
        // Kit missing: melodic instruments must not stand in.
        let a = avail(&[Instrument::Timpani, Instrument::Marimba]);
        let t = track(Instrument::Drums, Pattern::Drums, Articulation::Sustain);
        let r = resolve(&t, None, Some(&a), &FallbackPolicy::default());
        assert!(matches!(r.status, ResolutionStatus::Missing));
        // Melodic request: the kit must never be a candidate.
        let a = avail(&[Instrument::Drums]);
        let t = track(Instrument::Marimba, Pattern::Sustain, Articulation::Sustain);
        let r = resolve(&t, None, Some(&a), &FallbackPolicy::default());
        assert!(matches!(r.status, ResolutionStatus::Missing));
    }

    #[test]
    fn resolution_is_deterministic_across_runs() {
        let a = avail(&[
            Instrument::Violin,
            Instrument::Cello,
            Instrument::Contrabass,
            Instrument::Harp,
        ]);
        let t = track(Instrument::Viola, Pattern::Sustain, Articulation::Sustain);
        let first = serde_json::to_string(&resolve_track(
            0,
            &t,
            None,
            Some(&a),
            &FallbackPolicy::default(),
            true,
        ))
        .unwrap();
        for _ in 0..10 {
            let again = serde_json::to_string(&resolve_track(
                0,
                &t,
                None,
                Some(&a),
                &FallbackPolicy::default(),
                true,
            ))
            .unwrap();
            assert_eq!(first, again);
        }
    }

    #[test]
    fn articulation_availability_shapes_scores_and_warnings() {
        // Exact instrument, articulation without a dedicated sample: warn.
        let mut a = avail(&[Instrument::Violin]);
        let t = track(
            Instrument::Violin,
            Pattern::Sustain,
            Articulation::Pizzicato,
        );
        let r = resolve(&t, None, Some(&a), &FallbackPolicy::default());
        assert!(matches!(r.status, ResolutionStatus::Exact));
        assert!(!r.warnings.is_empty(), "{:?}", r.warnings);
        // Candidate with the requested articulation outranks one without.
        a.insert(
            Instrument::Cello,
            BTreeSet::from([Articulation::Sustain, Articulation::Pizzicato]),
        );
        let t = track(Instrument::Viola, Pattern::Sustain, Articulation::Pizzicato);
        let r = resolve(&t, None, Some(&a), &FallbackPolicy::default());
        assert_eq!(r.resolved.as_deref(), Some("cello"));
        assert!(r.reasons.iter().any(|x| x == "articulation_supported"));
    }

    #[test]
    fn legacy_string_fallback_switch_warns_and_allowlists_strings() {
        let config = ResolverConfig {
            legacy_string_fallback: Some(true),
            ..ResolverConfig::default()
        };
        let (policy, warnings) = build_policy(None, Some(&config));
        assert!(policy.allowed_families.contains(&Family::Strings));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("deprecated"), "{}", warnings[0]);
        // Default: off, no warning, no strings allowance.
        let (policy, warnings) = build_policy(None, Some(&ResolverConfig::default()));
        assert!(policy.allowed_families.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn policy_layering_flag_beats_config_mode() {
        let config = ResolverConfig {
            default_mode: Some(FallbackMode::Flexible),
            minimum_score: Some(0.5),
            ..ResolverConfig::default()
        };
        let (policy, _) = build_policy(Some(FallbackMode::Strict), Some(&config));
        assert_eq!(policy.mode, FallbackMode::Strict);
        assert_eq!(policy.minimum_score, 0.5);
        let (policy, _) = build_policy(None, Some(&config));
        assert_eq!(policy.mode, FallbackMode::Flexible);
        let (policy, _) = build_policy(None, None);
        assert_eq!(policy.mode, FallbackMode::Conservative);
        assert_eq!(policy.minimum_score, 0.70);
        assert!(!policy.allow_cross_family);
        assert!(!policy.allow_synth);
    }
}
