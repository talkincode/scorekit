//! Corpus-based style-independence measurement — audit dimension K, made
//! deterministic.
//!
//! Every piece is easiest to copy from the last one: the same instrument set,
//! the same minor key, the same tempo class, the same 4/4 body. `lint
//! --corpus` compares the scene in hand against the scenes that were actually
//! shipped and reports *measured* overlap per variation axis instead of a
//! vibe. The eight axes are exactly the ones documented in
//! `skills/scorekit/palettes.md`; every value is derived from authored scene
//! fields only, so the same pair of scenes always yields the same numbers and
//! nothing here can change compiled MIDI.

use crate::error::{Error, Result};
use crate::instrument::{self, Family};
use crate::schema::{Instrument, Pattern, Scene, instrument_key, parse_key, parse_time_signature};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The variation axes, in reporting order.
pub const AXES: [&str; AXIS_COUNT] = [
    "palette_family",
    "key_mode",
    "tempo_class",
    "meter",
    "lead_timbre",
    "pulse",
    "harmony_color",
    "density",
];

/// Number of variation axes — the ceiling for `corpus_axes_changed_min`.
pub const AXIS_COUNT: usize = 8;

const TONIC_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

const MINOR_DEFAULT: [&str; 4] = ["i", "VI", "III", "VII"];
const MAJOR_DEFAULT: [&str; 4] = ["I", "V", "vi", "IV"];

/// What one piece sounds like, reduced to the axes that make two pieces
/// recognizable as siblings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    /// Canonical instrument identities that actually sound.
    pub instruments: BTreeSet<String>,
    /// Coarse instrument families, for reporting.
    pub families: BTreeSet<&'static str>,
    /// One measured value per entry of [`AXES`], index-aligned.
    pub axes: [String; AXIS_COUNT],
}

impl Piece {
    /// Measure an authored scene. Suite scenes are described by their base
    /// tracks (sections mute and override, they never add instruments).
    pub fn measure(scene: &Scene) -> Piece {
        let instruments = scene
            .tracks
            .iter()
            .map(|track| instrument_key(track.instrument))
            .collect();
        let families = scene
            .tracks
            .iter()
            .map(|track| family_name(instrument::spec(track.instrument).family))
            .collect();
        Piece {
            instruments,
            families,
            axes: [
                dominant_family(scene).to_owned(),
                key_mode(scene),
                tempo_class(scene.tempo).to_owned(),
                meter(scene),
                lead_timbre(scene).to_owned(),
                pulse(scene).to_owned(),
                harmony_color(scene).to_owned(),
                density(scene).to_owned(),
            ],
        }
    }
}
fn family_name(family: Family) -> &'static str {
    match family {
        Family::Strings => "strings",
        Family::Woodwinds => "woodwinds",
        Family::Brass => "brass",
        Family::Percussion => "percussion",
        Family::Keyboards => "keyboards",
        Family::Plucked => "plucked",
        Family::Guitars => "guitars",
        Family::Bass => "bass",
        Family::Synth => "synth",
        Family::Ethnic => "ethnic",
        Family::Vocals => "vocals",
        Family::Textures => "textures",
    }
}

/// Coarse sound-production class: how the note is excited, which is what
/// keeps two pieces with overlapping instruments from sounding identical.
fn timbre_group(instrument: Instrument) -> &'static str {
    use Instrument::*;
    match instrument {
        Piano | BrightPiano | Epiano | Harpsichord | Celesta | Glockenspiel | MusicBox
        | Vibraphone | Marimba | Xylophone | TubularBells | Clavinet | Timpani | Drums | Tabla => {
            "struck"
        }
        Guitar | SteelGuitar | ElectricGuitar | MutedGuitar | Bass | PickedBass | FretlessBass
        | SlapBass | Harp | Pizzicato | Sitar | Shamisen | Pipa | Guzheng | Oud => "plucked",
        Organ | Accordion | Sax | Oboe | EnglishHorn | Bassoon | Clarinet | Piccolo | Flute
        | Recorder | PanFlute | Whistle | Ocarina | Shakuhachi | Dizi | Ney | Duduk | Trumpet
        | Trombone | Tuba | Horn | Brass => "blown",
        Violin | Viola | Cello | Contrabass | TremoloStrings | Strings | SlowStrings
        | SynthStrings | Erhu => "bowed",
        Choir | Voice | ChoirPad => "vocal",
        SquareLead | SawLead | Pad | WarmPad | BowedPad | HaloPad | SweepPad | SynthBrass
        | SynthBass => "synthesized",
    }
}

/// The family most tracks belong to; a tie means the piece is built from
/// several families at once.
fn dominant_family(scene: &Scene) -> &'static str {
    let mut counts: Vec<(&'static str, usize)> = Vec::new();
    for track in &scene.tracks {
        let family = family_name(instrument::spec(track.instrument).family);
        match counts.iter_mut().find(|(name, _)| *name == family) {
            Some((_, count)) => *count += 1,
            None => counts.push((family, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    match counts.as_slice() {
        [] => "mixed",
        [(_, top), rest @ ..] if rest.first().is_some_and(|(_, count)| count == top) => "mixed",
        [(family, _), ..] => family,
    }
}

fn key_mode(scene: &Scene) -> String {
    match parse_key(&scene.key) {
        Ok(key) => format!(
            "{}:{}",
            TONIC_NAMES[usize::from(key.root_pc)],
            if key.minor { "minor" } else { "major" }
        ),
        // Validated scenes always parse; keep a truthful fallback.
        Err(_) => scene.key.clone(),
    }
}

fn tempo_class(tempo: u16) -> &'static str {
    match tempo {
        0..=69 => "slow",
        70..=110 => "mid",
        111..=140 => "driving",
        _ => "fast",
    }
}

fn meter(scene: &Scene) -> String {
    match parse_time_signature(&scene.time_signature) {
        Ok(sig) => format!("{}/{}", sig.num, sig.den),
        Err(_) => scene.time_signature.clone(),
    }
}

/// Timbre of the track that carries the piece: the loudest melody track when
/// the scene has one, otherwise the loudest non-percussion track.
fn lead_timbre(scene: &Scene) -> &'static str {
    let lead = scene
        .tracks
        .iter()
        .filter(|track| track.pattern == Pattern::Melody)
        .max_by(|a, b| {
            a.intensity
                .total_cmp(&b.intensity)
                .then_with(|| b.id.cmp(&a.id))
        })
        .or_else(|| {
            scene
                .tracks
                .iter()
                .filter(|track| !is_percussion(track.instrument))
                .max_by(|a, b| {
                    a.intensity
                        .total_cmp(&b.intensity)
                        .then_with(|| b.id.cmp(&a.id))
                })
        })
        .or_else(|| scene.tracks.first());
    lead.map_or("struck", |track| timbre_group(track.instrument))
}

fn is_percussion(instrument: Instrument) -> bool {
    matches!(instrument, Instrument::Drums | Instrument::Tabla)
}

fn pulse(scene: &Scene) -> &'static str {
    let loudest = scene
        .tracks
        .iter()
        .filter(|track| is_percussion(track.instrument))
        .map(|track| track.intensity)
        .fold(None::<f32>, |acc, value| {
            Some(acc.map_or(value, |best| best.max(value)))
        });
    match loudest {
        None => "none",
        Some(intensity) if intensity < 0.35 => "soft",
        Some(_) => "groove",
    }
}

/// Harmonic color, from the authored progression (or the built-in default
/// when `harmony` is absent).
fn harmony_color(scene: &Scene) -> &'static str {
    let minor = parse_key(&scene.key).map(|key| key.minor).unwrap_or(true);
    let default: &[&str] = if minor {
        &MINOR_DEFAULT
    } else {
        &MAJOR_DEFAULT
    };
    let progression: Vec<&str> = if scene.harmony.is_empty() {
        default.to_vec()
    } else {
        scene.harmony.iter().map(String::as_str).collect()
    };
    let unique: BTreeSet<String> = progression
        .iter()
        .map(|numeral| numeral.to_ascii_lowercase())
        .collect();
    if unique.len() <= 1 {
        return "static";
    }
    if progression == default {
        return "default";
    }
    if minor
        && progression
            .iter()
            .any(|numeral| *numeral == "VII" || *numeral == "v" || *numeral == "vII")
    {
        return "modal";
    }
    "other"
}

fn density(scene: &Scene) -> &'static str {
    match scene.tracks.len() {
        0..=3 => "chamber",
        4..=5 => "standard",
        _ => "layered",
    }
}

/// One shipped scene the current piece is compared against.
#[derive(Debug, Clone)]
pub struct CorpusScene {
    /// Path as reported in findings, so a violation names the sibling.
    pub label: String,
    pub piece: Piece,
}

/// A loaded corpus plus what was passed over, so the agent can tell "nothing
/// to compare against" from "everything compared".
#[derive(Debug, Clone, Default)]
pub struct Corpus {
    pub scenes: Vec<CorpusScene>,
    /// YAML files that were not scenes (grammar profiles, renderer profiles).
    pub skipped: usize,
}

impl Corpus {
    pub fn is_empty(&self) -> bool {
        self.scenes.is_empty()
    }
}

/// Recursively collect `*.yaml` / `*.yml` scenes under `dirs`, excluding the
/// scene under test. Files that do not parse as scenes (grammar profiles,
/// renderer profiles, batch reports) are counted as skipped, never fatal:
/// pointing `--corpus` at a build directory must not trip over its sidecars.
pub fn load_corpus(dirs: &[PathBuf], exclude: Option<&Path>) -> Result<Corpus> {
    let exclude = exclude.and_then(|path| std::fs::canonicalize(path).ok());
    let mut files = Vec::new();
    for dir in dirs {
        collect_scene_files(dir, &mut files)?;
    }
    files.sort();
    files.dedup();

    let mut corpus = Corpus::default();
    for file in files {
        if exclude
            .as_ref()
            .is_some_and(|skip| std::fs::canonicalize(&file).ok().as_ref() == Some(skip))
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&file) else {
            corpus.skipped += 1;
            continue;
        };
        match serde_yaml_ng::from_str::<Scene>(&text) {
            Ok(scene) if scene.validate().is_ok() => corpus.scenes.push(CorpusScene {
                label: file.display().to_string(),
                piece: Piece::measure(&scene),
            }),
            _ => corpus.skipped += 1,
        }
    }
    Ok(corpus)
}

fn collect_scene_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(dir).map_err(|source| Error::Io {
        path: dir.display().to_string(),
        source,
    })?;
    let mut children: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| Error::Io {
            path: dir.display().to_string(),
            source,
        })?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        children.push(entry.path());
    }
    children.sort();
    for path in children {
        if path.is_dir() {
            collect_scene_files(&path, out)?;
            continue;
        }
        let is_yaml = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml"));
        if is_yaml {
            out.push(path);
        }
    }
    Ok(())
}

/// How one corpus scene compares against the piece under test.
#[derive(Debug, Clone)]
pub struct Comparison {
    pub label: String,
    /// `|A ∩ B| / max(|A|, |B|)` over canonical instrument names.
    pub instrument_overlap: f64,
    /// Instrument names both pieces use.
    pub shared: Vec<String>,
    pub axes_changed: usize,
    pub changed: Vec<&'static str>,
    pub unchanged: Vec<&'static str>,
}

pub fn compare(piece: &Piece, corpus: &CorpusScene) -> Comparison {
    let shared: Vec<String> = piece
        .instruments
        .intersection(&corpus.piece.instruments)
        .cloned()
        .collect();
    let widest = piece
        .instruments
        .len()
        .max(corpus.piece.instruments.len())
        .max(1);
    let mut changed = Vec::new();
    let mut unchanged = Vec::new();
    for (index, axis) in AXES.iter().enumerate() {
        if piece.axes[index] == corpus.piece.axes[index] {
            unchanged.push(*axis);
        } else {
            changed.push(*axis);
        }
    }
    Comparison {
        label: corpus.label.clone(),
        instrument_overlap: shared.len() as f64 / widest as f64,
        shared,
        axes_changed: changed.len(),
        changed,
        unchanged,
    }
}

/// Every comparison against the corpus, sorted most-similar first (then by
/// label) so both the report and the gate stay deterministic.
pub fn compare_all(piece: &Piece, corpus: &Corpus) -> Vec<Comparison> {
    let mut comparisons: Vec<Comparison> = corpus
        .scenes
        .iter()
        .map(|entry| compare(piece, entry))
        .collect();
    comparisons.sort_by(|a, b| {
        b.instrument_overlap
            .total_cmp(&a.instrument_overlap)
            .then_with(|| a.axes_changed.cmp(&b.axes_changed))
            .then_with(|| a.label.cmp(&b.label))
    });
    comparisons
}

/// Human-readable evidence for the nearest neighbour, echoed on a passing
/// lint so the agent can see what "different" was actually measured against.
pub fn summary(comparisons: &[Comparison]) -> Option<String> {
    let nearest = comparisons.first()?;
    let shared = if nearest.shared.is_empty() {
        "no shared instruments".to_owned()
    } else {
        format!("shared {}", nearest.shared.join(", "))
    };
    Some(format!(
        "nearest {} — instrument overlap {:.2} ({shared}), {}/{} axes changed ({})",
        nearest.label,
        nearest.instrument_overlap,
        nearest.axes_changed,
        AXIS_COUNT,
        if nearest.changed.is_empty() {
            "none".to_owned()
        } else {
            nearest.changed.join(", ")
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(yaml: &str) -> Scene {
        let scene: Scene = serde_yaml_ng::from_str(yaml).expect("test scene parses");
        scene.validate().expect("test scene validates");
        scene
    }

    const STRINGS: &str = r#"
tempo: 56
key: D_minor
bars: 8
motifs:
  cell:
    - { degree: 0, beats: 4 }
tracks:
  - { id: lead, instrument: violin, pattern: melody, motif: cell }
  - { id: bed, instrument: slow_strings, pattern: sustain }
  - { id: floor, instrument: cello, pattern: bass }
"#;

    #[test]
    fn axes_are_measured_from_authored_fields() {
        let piece = Piece::measure(&scene(STRINGS));
        assert_eq!(piece.axes[0], "strings", "dominant family");
        assert_eq!(piece.axes[1], "D:minor");
        assert_eq!(piece.axes[2], "slow");
        assert_eq!(piece.axes[3], "4/4");
        assert_eq!(piece.axes[4], "bowed");
        assert_eq!(piece.axes[5], "none");
        assert_eq!(piece.axes[6], "default");
        assert_eq!(piece.axes[7], "chamber");
    }

    #[test]
    fn identical_palette_overlaps_fully_and_changes_no_axis() {
        let mut louder = STRINGS.replace("tempo: 56", "tempo: 58");
        louder.push('\n');
        let piece = Piece::measure(&scene(STRINGS));
        let entry = CorpusScene {
            label: "other.yaml".to_owned(),
            piece: Piece::measure(&scene(&louder)),
        };
        let comparison = compare(&piece, &entry);
        assert_eq!(comparison.instrument_overlap, 1.0);
        // Same palette, key/mode, tempo class, meter, lead timbre, pulse,
        // harmony color and density: the homogenization signature.
        assert_eq!(comparison.axes_changed, 0);
        assert_eq!(comparison.unchanged.len(), AXIS_COUNT);
    }

    #[test]
    fn a_different_palette_changes_several_axes() {
        let chiptune = Piece::measure(&scene(
            r#"
tempo: 148
key: A_major
bars: 8
harmony: [I, V, vi, IV]
motifs:
  cell:
    - { degree: 0, beats: 4 }
tracks:
  - { id: lead, instrument: square_lead, pattern: melody, motif: cell }
  - { id: low, instrument: synth_bass, pattern: bass }
  - { id: kit, instrument: drums, pattern: drums, intensity: 0.6 }
"#,
        ));
        let entry = CorpusScene {
            label: "chiptune.yaml".to_owned(),
            piece: chiptune,
        };
        let comparison = compare(&Piece::measure(&scene(STRINGS)), &entry);
        assert_eq!(comparison.instrument_overlap, 0.0);
        // Palette family, key, tempo class, lead timbre and pulse differ;
        // meter, harmony color and density happen to agree.
        assert!(
            comparison.axes_changed >= 5,
            "axes changed: {:?}",
            comparison.changed
        );
        assert!(comparison.changed.contains(&"palette_family"));
    }

    #[test]
    fn dominant_family_ties_report_mixed() {
        let piece = Piece::measure(&scene(
            r#"
tempo: 90
bars: 4
motifs:
  cell:
    - { degree: 0, beats: 4 }
tracks:
  - { id: lead, instrument: violin, pattern: melody, motif: cell }
  - { id: kit, instrument: drums, pattern: drums }
"#,
        ));
        assert_eq!(piece.axes[0], "mixed");
        assert_eq!(piece.axes[5], "groove");
    }

    #[test]
    fn lead_timbre_prefers_the_loudest_melody_track() {
        let piece = Piece::measure(&scene(
            r#"
tempo: 90
bars: 4
motifs:
  cell:
    - { degree: 0, beats: 4 }
tracks:
  - { id: lead, instrument: flute, pattern: melody, motif: cell, intensity: 0.8 }
  - { id: floor, instrument: contrabass, pattern: bass, intensity: 0.95 }
"#,
        ));
        assert_eq!(piece.axes[4], "blown");
    }

    #[test]
    fn harmony_color_separates_static_modal_and_default() {
        let color = |harmony: &str| {
            Piece::measure(&scene(&format!(
                r#"
tempo: 56
key: D_minor
bars: 4
harmony: {harmony}
tracks:
  - {{ id: bed, instrument: slow_strings, pattern: sustain }}
"#
            )))
            .axes[6]
                .clone()
        };
        assert_eq!(color("[i]"), "static");
        assert_eq!(color("[i, i, i]"), "static");
        assert_eq!(color("[i, VI, III, VII]"), "default");
        assert_eq!(color("[i, VII, iv, v]"), "modal");
        assert_eq!(color("[i, iv, V, i]"), "other");
    }

    #[test]
    fn corpus_scan_skips_non_scene_yaml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("piece.yaml"), STRINGS).unwrap();
        std::fs::write(
            dir.path().join("grammar.yaml"),
            "name: grief\nrules:\n  tempo_max: 60\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("notes.txt"), "not yaml").unwrap();
        let nested = dir.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(nested.join("second.yaml"), STRINGS).unwrap();

        let corpus = load_corpus(&[dir.path().to_path_buf()], None).unwrap();
        assert_eq!(corpus.scenes.len(), 2);
        assert_eq!(corpus.skipped, 1);
        // Deterministic order.
        let labels: Vec<&str> = corpus.scenes.iter().map(|s| s.label.as_str()).collect();
        let mut sorted = labels.clone();
        sorted.sort();
        assert_eq!(labels, sorted);
    }

    #[test]
    fn corpus_scan_excludes_the_scene_under_test() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("piece.yaml");
        std::fs::write(&target, STRINGS).unwrap();
        let corpus = load_corpus(&[dir.path().to_path_buf()], Some(&target)).unwrap();
        assert!(corpus.is_empty());
    }

    #[test]
    fn missing_corpus_directory_is_an_io_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope");
        let error = load_corpus(&[missing], None).expect_err("missing dir must fail loudly");
        assert_eq!(error.code(), "io");
    }
}
