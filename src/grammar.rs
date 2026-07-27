//! Aesthetic grammar profiles: deterministic style assertions checked
//! against the composed IR. A profile is external data — the compiler
//! verifies it, it never writes a note. This is a linter for taste:
//! the constitution lives in YAML, survives model generations, and the
//! agent gets a regression test for its own aesthetics.

use crate::composer::{self, PPQ};
use crate::error::{Error, Result};
use crate::schema::{
    AutomationTarget, Instrument, Pattern, PercussionVoice, Scene, parse_key, parse_numeral,
    parse_time_signature, quarter_beats_to_ticks,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

/// Notes separated by less than this many beats of silence belong to
/// the same phrase when measuring `phrase_min_beats`.
const PHRASE_GAP_BEATS: u32 = 2;

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Grammar {
    /// Profile name, echoed in every finding.
    pub name: String,
    /// What this aesthetic is about, for humans.
    #[serde(default)]
    pub description: Option<String>,
    /// The assertions. Every rule is optional; absent means unchecked.
    pub rules: Rules,
    /// Additional assertions applied only to named suite sections.
    #[serde(default)]
    pub section_rules: BTreeMap<String, Rules>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    /// Minimum tempo in BPM (inclusive).
    #[serde(default)]
    pub tempo_min: Option<u16>,
    /// Maximum tempo in BPM (inclusive).
    #[serde(default)]
    pub tempo_max: Option<u16>,
    /// Maximum number of sustain-pattern (pad) tracks.
    #[serde(default)]
    pub pads_max: Option<usize>,
    /// Maximum melody-pattern tracks sounding at the same instant.
    #[serde(default)]
    pub melodic_voices_max: Option<usize>,
    /// Minimum fraction (0..=1) of the timeline each melody track must
    /// leave silent — every voice has to breathe.
    #[serde(default)]
    pub melody_rest_ratio_min: Option<f64>,
    /// Minimum length of every melodic phrase, in beats.
    #[serde(default)]
    pub phrase_min_beats: Option<f64>,
    /// Whether the final melody note must (`complete`) or must not
    /// (`incomplete`) land on the tonic.
    #[serde(default)]
    pub resolution: Option<ResolutionRule>,
    /// Roman-numeral whitelist; the scene's progression (or the
    /// built-in default) must stay inside it.
    #[serde(default)]
    pub harmony_allowed: Option<Vec<String>>,
    /// Require a `performance` block (humanized playback).
    #[serde(default)]
    pub require_performance: Option<bool>,
    /// Minimum compiled percussion note-on count per bar.
    #[serde(default)]
    pub percussion_events_per_bar_min: Option<f64>,
    /// Required GM percussion voices at exact within-bar quarter-beat positions.
    #[serde(default)]
    pub percussion_onsets: Vec<PercussionOnsetRule>,
    /// Minimum authored automation activity, optionally scoped to a track ID.
    #[serde(default)]
    pub automation_activity: Vec<AutomationActivityRule>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PercussionOnsetRule {
    pub voice: PercussionVoice,
    #[schemars(length(min = 1, max = 32), inner(range(min = 0.0, max = 64.0)))]
    pub positions: Vec<f64>,
    #[schemars(range(min = 0.0, max = 1.0))]
    pub coverage_min: f64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AutomationActivityRule {
    /// Optional stable track ID; absent aggregates matching lanes on all tracks.
    #[serde(default)]
    #[schemars(regex(pattern = "^[a-z][a-z0-9_-]{0,63}$"))]
    pub track: Option<String>,
    pub target: AutomationTarget,
    #[serde(default)]
    #[schemars(range(min = 0.0, max = 512.0))]
    pub points_per_bar_min: Option<f64>,
    #[serde(default)]
    #[schemars(range(min = 0, max = 16383))]
    pub value_span_min: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionRule {
    Complete,
    Incomplete,
}

/// One failed assertion: what was measured, against what limit, where.
#[derive(Debug)]
pub struct Violation {
    pub rule: &'static str,
    pub subject: String,
    pub measured: String,
    pub want: String,
}

impl Violation {
    pub fn porcelain(&self) -> String {
        format!(
            "{} @ {}: measured {}, want {}",
            self.rule, self.subject, self.measured, self.want
        )
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "rule": self.rule,
            "subject": self.subject,
            "measured": self.measured,
            "want": self.want,
        })
    }
}

impl Rules {
    pub fn active_count(&self) -> usize {
        usize::from(self.tempo_min.is_some())
            + usize::from(self.tempo_max.is_some())
            + usize::from(self.pads_max.is_some())
            + usize::from(self.melodic_voices_max.is_some())
            + usize::from(self.melody_rest_ratio_min.is_some())
            + usize::from(self.phrase_min_beats.is_some())
            + usize::from(self.resolution.is_some())
            + usize::from(self.harmony_allowed.is_some())
            + usize::from(self.require_performance.is_some())
            + usize::from(self.percussion_events_per_bar_min.is_some())
            + usize::from(!self.percussion_onsets.is_empty())
            + usize::from(!self.automation_activity.is_empty())
    }
}

impl Grammar {
    fn validate(&self) -> Result<()> {
        let fail = |path: &str, message: String| {
            Err(Error::Validation {
                path: path.to_owned(),
                message,
            })
        };
        if self.name.is_empty() {
            return fail("name", "must not be empty".to_owned());
        }
        if self.rules.active_count() == 0 && self.section_rules.is_empty() {
            return fail("rules", "at least one rule must be set".to_owned());
        }
        validate_rules(&self.rules, "rules")?;
        for (section, rules) in &self.section_rules {
            if section.is_empty()
                || !section
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            {
                return fail(
                    &format!("section_rules.{section}"),
                    "section name must match [A-Za-z0-9_-]+".to_owned(),
                );
            }
            if rules.active_count() == 0 {
                return fail(
                    &format!("section_rules.{section}"),
                    "at least one rule must be set".to_owned(),
                );
            }
            validate_rules(rules, &format!("section_rules.{section}"))?;
        }
        Ok(())
    }
}

fn validate_rules(rules: &Rules, prefix: &str) -> Result<()> {
    let fail = |suffix: &str, message: String| {
        Err(Error::Validation {
            path: if suffix.is_empty() {
                prefix.to_owned()
            } else {
                format!("{prefix}.{suffix}")
            },
            message,
        })
    };
    if let (Some(lo), Some(hi)) = (rules.tempo_min, rules.tempo_max)
        && lo > hi
    {
        return fail("tempo_min", format!("{lo} exceeds tempo_max {hi}"));
    }
    if let Some(r) = rules.melody_rest_ratio_min
        && !(0.0..=1.0).contains(&r)
    {
        return fail(
            "melody_rest_ratio_min",
            format!("{r} out of range 0.0..=1.0"),
        );
    }
    if let Some(p) = rules.phrase_min_beats
        && !(0.0..=64.0).contains(&p)
    {
        return fail("phrase_min_beats", format!("{p} out of range 0.0..=64.0"));
    }
    if let Some(numerals) = &rules.harmony_allowed {
        if numerals.is_empty() {
            return fail("harmony_allowed", "must not be empty".to_owned());
        }
        for (i, numeral) in numerals.iter().enumerate() {
            if let Err(error) = parse_numeral(numeral) {
                return fail(&format!("harmony_allowed[{i}]"), error);
            }
        }
    }
    if let Some(min) = rules.percussion_events_per_bar_min
        && (!min.is_finite() || !(0.0..=4096.0).contains(&min))
    {
        return fail(
            "percussion_events_per_bar_min",
            format!("{min} out of range 0.0..=4096.0"),
        );
    }
    for (index, onset) in rules.percussion_onsets.iter().enumerate() {
        let path = format!("percussion_onsets[{index}]");
        if onset.positions.is_empty() {
            return fail(&format!("{path}.positions"), "must not be empty".to_owned());
        }
        if onset.positions.len() > 32 {
            return fail(
                &format!("{path}.positions"),
                format!(
                    "{} positions exceed the published limit 32",
                    onset.positions.len()
                ),
            );
        }
        if !onset.coverage_min.is_finite() || !(0.0..=1.0).contains(&onset.coverage_min) {
            return fail(
                &format!("{path}.coverage_min"),
                format!("{} out of range 0.0..=1.0", onset.coverage_min),
            );
        }
        let mut ticks = BTreeSet::new();
        for (position_index, position) in onset.positions.iter().enumerate() {
            let Some(tick) = quarter_beats_to_ticks(*position) else {
                return fail(
                    &format!("{path}.positions[{position_index}]"),
                    format!("{position} must be finite and non-negative"),
                );
            };
            if *position > 64.0 {
                return fail(
                    &format!("{path}.positions[{position_index}]"),
                    format!("{position} exceeds 64 quarter-note beats"),
                );
            }
            if !ticks.insert(tick) {
                return fail(
                    &format!("{path}.positions[{position_index}]"),
                    format!("another position quantizes to tick {tick}"),
                );
            }
        }
    }
    for (index, activity) in rules.automation_activity.iter().enumerate() {
        let path = format!("automation_activity[{index}]");
        if activity.points_per_bar_min.is_none() && activity.value_span_min.is_none() {
            return fail(
                &path,
                "set `points_per_bar_min`, `value_span_min`, or both".to_owned(),
            );
        }
        if let Some(track) = &activity.track
            && !crate::texture::valid_logical_name(track)
        {
            return fail(
                &format!("{path}.track"),
                format!("`{track}` must match [a-z][a-z0-9_-]{{0,63}}"),
            );
        }
        if let Some(min) = activity.points_per_bar_min
            && (!min.is_finite() || !(0.0..=512.0).contains(&min))
        {
            return fail(
                &format!("{path}.points_per_bar_min"),
                format!("{min} out of range 0.0..=512.0"),
            );
        }
        if let Some(span) = activity.value_span_min {
            let max = if activity.target == AutomationTarget::PitchBend {
                16383
            } else {
                127
            };
            if span > max {
                return fail(
                    &format!("{path}.value_span_min"),
                    format!("{span} out of range 0..={max} for {:?}", activity.target),
                );
            }
        }
    }
    Ok(())
}

/// Read, parse and validate a grammar profile.
pub fn load_grammar(path: &Path) -> Result<Grammar> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })?;
    let grammar: Grammar = serde_yaml_ng::from_str(&text).map_err(|e| Error::Parse {
        message: format!("invalid grammar: {e}"),
        location: e.location().map(|l| crate::error::Location {
            line: l.line(),
            column: l.column(),
        }),
    })?;
    grammar.validate()?;
    Ok(grammar)
}

/// JSON Schema of the grammar profile DSL, for agent consumption.
pub fn schema_json() -> String {
    let schema = schemars::schema_for!(Grammar);
    serde_json::to_string_pretty(&schema).expect("schema serializes")
}

/// Check a validated scene against a grammar. Suite scenes are checked
/// section by section (each section is what a player actually hears).
pub fn check(scene: &Scene, grammar: &Grammar) -> Vec<Violation> {
    let mut violations = Vec::new();
    if scene.sections.is_empty() {
        check_one(scene, "scene", &grammar.rules, &mut violations);
        for section in grammar.section_rules.keys() {
            violations.push(Violation {
                rule: "section_required",
                subject: format!("section `{section}`"),
                measured: "absent".to_owned(),
                want: "present".to_owned(),
            });
        }
    } else {
        let mut seen = BTreeSet::new();
        for section in &scene.sections {
            let derived = scene.for_section(section);
            let subject = format!("section `{}`", section.name);
            check_one(&derived, &subject, &grammar.rules, &mut violations);
            if let Some(rules) = grammar.section_rules.get(&section.name) {
                check_one(&derived, &subject, rules, &mut violations);
                seen.insert(section.name.as_str());
            }
        }
        for section in grammar.section_rules.keys() {
            if !seen.contains(section.as_str()) {
                violations.push(Violation {
                    rule: "section_required",
                    subject: format!("section `{section}`"),
                    measured: "absent".to_owned(),
                    want: "present".to_owned(),
                });
            }
        }
    }
    violations
}

fn check_one(scene: &Scene, subject: &str, rules: &Rules, out: &mut Vec<Violation>) {
    let mut push = |rule: &'static str, measured: String, want: String| {
        out.push(Violation {
            rule,
            subject: subject.to_owned(),
            measured,
            want,
        });
    };

    if let Some(lo) = rules.tempo_min
        && scene.tempo < lo
    {
        push("tempo_min", scene.tempo.to_string(), format!(">= {lo}"));
    }
    if let Some(hi) = rules.tempo_max
        && scene.tempo > hi
    {
        push("tempo_max", scene.tempo.to_string(), format!("<= {hi}"));
    }

    if let Some(max) = rules.pads_max {
        let pads = scene
            .tracks
            .iter()
            .filter(|t| t.pattern == Pattern::Sustain)
            .count();
        if pads > max {
            push("pads_max", pads.to_string(), format!("<= {max}"));
        }
    }

    if rules.require_performance == Some(true) && scene.performance.is_none() {
        push(
            "require_performance",
            "absent".to_owned(),
            "a `performance` block".to_owned(),
        );
    }

    if let Some(allowed) = &rules.harmony_allowed {
        let allowed_idx: Vec<usize> = allowed
            .iter()
            .map(|n| parse_numeral(n).expect("grammar is validated"))
            .collect();
        let key = parse_key(&scene.key).expect("scene is validated");
        let (prog_idx, prog_names): (Vec<usize>, Vec<String>) = if scene.harmony.is_empty() {
            let names: &[&str] = if key.minor {
                &["i", "VI", "III", "VII"]
            } else {
                &["I", "V", "vi", "IV"]
            };
            (
                composer::default_progression(key.minor).to_vec(),
                names.iter().map(|s| (*s).to_owned()).collect(),
            )
        } else {
            (
                scene
                    .harmony
                    .iter()
                    .map(|n| parse_numeral(n).expect("scene is validated"))
                    .collect(),
                scene.harmony.clone(),
            )
        };
        for (idx, name) in prog_idx.iter().zip(&prog_names) {
            if !allowed_idx.contains(idx) {
                push(
                    "harmony_allowed",
                    format!("`{name}`"),
                    format!("one of {allowed:?}"),
                );
            }
        }
    }

    // The remaining rules measure the composed result — after patterns
    // are expanded and the performance transforms have been applied.
    let needs_ir = rules.melodic_voices_max.is_some()
        || rules.melody_rest_ratio_min.is_some()
        || rules.phrase_min_beats.is_some()
        || rules.resolution.is_some()
        || rules.percussion_events_per_bar_min.is_some()
        || !rules.percussion_onsets.is_empty()
        || !rules.automation_activity.is_empty();
    if !needs_ir {
        return;
    }
    let ir = composer::compose(scene);
    let ts = parse_time_signature(&scene.time_signature).expect("scene is validated");
    let beat_ticks = PPQ * 4 / u32::from(ts.den);
    let bar_ticks = beat_ticks * u32::from(ts.num);

    let percussion_notes: Vec<&crate::composer::NoteEvent> = scene
        .tracks
        .iter()
        .zip(&ir.tracks)
        .filter(|(track, _)| track.instrument.is_percussion())
        .flat_map(|(_, track_ir)| track_ir.notes.iter())
        .collect();
    let drum_onsets: HashSet<(u32, u8)> = scene
        .tracks
        .iter()
        .zip(&ir.tracks)
        .filter(|(track, _)| track.instrument == Instrument::Drums)
        .flat_map(|(_, track_ir)| track_ir.notes.iter().map(|note| (note.tick, note.key)))
        .collect();
    if let Some(min) = rules.percussion_events_per_bar_min {
        let density = percussion_notes.len() as f64 / f64::from(scene.bars);
        if density < min {
            push(
                "percussion_events_per_bar_min",
                format!("{density:.2} per bar"),
                format!(">= {min}"),
            );
        }
    }
    for onset in &rules.percussion_onsets {
        let key = onset.voice.midi_key();
        let positions: Vec<u32> = onset
            .positions
            .iter()
            .map(|position| quarter_beats_to_ticks(*position).expect("grammar is validated"))
            .collect();
        let required = usize::from(scene.bars) * positions.len();
        let mut matched = 0usize;
        for bar in 0..u32::from(scene.bars) {
            for position in &positions {
                if *position < bar_ticks {
                    let tick = bar * bar_ticks + position;
                    if drum_onsets.contains(&(tick, key)) {
                        matched += 1;
                    }
                }
            }
        }
        let coverage = if required == 0 {
            0.0
        } else {
            matched as f64 / required as f64
        };
        if coverage < onset.coverage_min {
            push(
                "percussion_onsets",
                format!(
                    "{coverage:.2} coverage for {:?} at {:?}",
                    onset.voice, onset.positions
                ),
                format!(">= {}", onset.coverage_min),
            );
        }
    }
    for activity in &rules.automation_activity {
        let mut values = Vec::new();
        let mut points = 0usize;
        for (track, track_ir) in scene.tracks.iter().zip(&ir.tracks) {
            if track.pattern != Pattern::Clip
                || activity
                    .track
                    .as_ref()
                    .is_some_and(|wanted| wanted != &track.id)
            {
                continue;
            }
            match activity.target {
                AutomationTarget::PitchBend => {
                    points += track_ir.bends.len();
                    values.extend(
                        track_ir
                            .bends
                            .iter()
                            .map(|event| i32::from(event.value) - 8192),
                    );
                }
                target => {
                    let controller = target
                        .controller()
                        .expect("non-bend automation has a controller");
                    let matching = track_ir
                        .controls
                        .iter()
                        .filter(|event| event.controller == controller);
                    for event in matching {
                        points += 1;
                        values.push(i32::from(event.value));
                    }
                }
            }
        }
        if let Some(min) = activity.points_per_bar_min {
            let density = points as f64 / f64::from(scene.bars);
            if density < min {
                push(
                    "automation_points_per_bar_min",
                    format!(
                        "{density:.2} per bar ({:?}{})",
                        activity.target,
                        activity
                            .track
                            .as_ref()
                            .map(|track| format!(", track {track}"))
                            .unwrap_or_default()
                    ),
                    format!(">= {min}"),
                );
            }
        }
        if let Some(min) = activity.value_span_min {
            let span = values
                .iter()
                .min()
                .zip(values.iter().max())
                .map(|(lo, hi)| hi - lo)
                .unwrap_or(0);
            if span < i32::from(min) {
                push(
                    "automation_value_span_min",
                    format!(
                        "{span} ({:?}{})",
                        activity.target,
                        activity
                            .track
                            .as_ref()
                            .map(|track| format!(", track {track}"))
                            .unwrap_or_default()
                    ),
                    format!(">= {min}"),
                );
            }
        }
    }

    // Per melody track: merged sounding intervals, in track order.
    let melody: Vec<(usize, Vec<(u32, u32)>)> = scene
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| t.pattern == Pattern::Melody)
        .map(|(i, _)| {
            let mut spans: Vec<(u32, u32)> = ir.tracks[i]
                .notes
                .iter()
                .map(|n| (n.tick, n.tick.saturating_add(n.dur).min(ir.total_ticks)))
                .collect();
            spans.sort_unstable();
            (i, merge(spans))
        })
        .collect();

    if let Some(max) = rules.melodic_voices_max {
        let peak = peak_concurrency(&melody);
        if peak > max {
            push(
                "melodic_voices_max",
                format!("{peak} at once"),
                format!("<= {max}"),
            );
        }
    }

    if let Some(min) = rules.melody_rest_ratio_min {
        for (track, spans) in &melody {
            let covered: u64 = spans.iter().map(|(a, b)| u64::from(b - a)).sum();
            let ratio = 1.0 - covered as f64 / f64::from(ir.total_ticks);
            if ratio < min {
                push(
                    "melody_rest_ratio_min",
                    format!("{ratio:.2} (track {track})"),
                    format!(">= {min}"),
                );
            }
        }
    }

    if let Some(min) = rules.phrase_min_beats {
        let gap = beat_ticks * PHRASE_GAP_BEATS;
        for (track, spans) in &melody {
            for (start, end) in phrases(spans, gap) {
                let beats = f64::from(end - start) / f64::from(beat_ticks);
                if beats < min {
                    let bar = start / (beat_ticks * u32::from(ts.num)) + 1;
                    push(
                        "phrase_min_beats",
                        format!("{beats:.1} beats (track {track}, bar {bar})"),
                        format!(">= {min}"),
                    );
                }
            }
        }
    }

    if let Some(rule) = rules.resolution {
        let key = parse_key(&scene.key).expect("scene is validated");
        let last = melody
            .iter()
            .flat_map(|(i, _)| ir.tracks[*i].notes.iter())
            .max_by_key(|n| (n.tick.saturating_add(n.dur), n.tick, n.key));
        match last {
            None => push(
                "resolution",
                "no melodic material".to_owned(),
                "a final melody note".to_owned(),
            ),
            Some(n) => {
                let on_tonic = n.key % 12 == key.root_pc % 12;
                match rule {
                    ResolutionRule::Complete if !on_tonic => push(
                        "resolution",
                        "final note off the tonic".to_owned(),
                        "complete (ends on the tonic)".to_owned(),
                    ),
                    ResolutionRule::Incomplete if on_tonic => push(
                        "resolution",
                        "final note on the tonic".to_owned(),
                        "incomplete (ends off the tonic)".to_owned(),
                    ),
                    _ => {}
                }
            }
        }
    }
}

/// Merge sorted, possibly overlapping intervals.
fn merge(spans: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::with_capacity(spans.len());
    for (a, b) in spans {
        if a >= b {
            continue;
        }
        match out.last_mut() {
            Some((_, end)) if a <= *end => *end = (*end).max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

/// Group merged intervals into phrases: gaps shorter than `gap` join.
fn phrases(spans: &[(u32, u32)], gap: u32) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for &(a, b) in spans {
        match out.last_mut() {
            Some((_, end)) if a.saturating_sub(*end) < gap => *end = b,
            _ => out.push((a, b)),
        }
    }
    out
}

/// Maximum number of tracks sounding at the same instant.
fn peak_concurrency(melody: &[(usize, Vec<(u32, u32)>)]) -> usize {
    let mut events: Vec<(u32, i32)> = Vec::new();
    for (_, spans) in melody {
        for &(a, b) in spans {
            events.push((a, 1));
            events.push((b, -1));
        }
    }
    // At equal ticks, ends (-1) sort before starts (+1): touching
    // intervals do not count as overlap.
    events.sort_unstable();
    let (mut cur, mut peak) = (0i32, 0i32);
    for (_, d) in events {
        cur += d;
        peak = peak.max(cur);
    }
    peak.max(0) as usize
}
