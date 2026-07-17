//! E2E tests backing the acceptance matrix in docs/roadmap.md.
//! Audio tests need `fluidsynth`, `ffmpeg` and `assets/TimGM6mb.sf2`
//! (run `scripts/fetch_assets.sh` once to download the SoundFont).

use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};

fn bin() -> Command {
    Command::cargo_bin("scorekit").expect("binary builds")
}

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

fn sf2() -> PathBuf {
    let p = repo("assets/TimGM6mb.sf2");
    assert!(
        p.is_file(),
        "missing test SoundFont {} — run scripts/fetch_assets.sh first",
        p.display()
    );
    p
}

fn forest() -> PathBuf {
    repo("examples/scenes/forest.yaml")
}

/// Only the files we placed may remain: failures must not leak temp/partial output.
fn assert_dir_contains_exactly(dir: &Path, expected: &[&str]) {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut expected: Vec<String> = expected.iter().map(|s| (*s).to_owned()).collect();
    expected.sort();
    assert_eq!(names, expected, "unexpected files in {}", dir.display());
}

// ---- validate / schema ----

#[test]
fn validate_happy_path() {
    bin().args(["validate"]).arg(forest()).assert().success();
}

#[test]
fn validate_rejects_unknown_field_with_location() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("bad.yaml");
    fs::write(&scene, "tempo: 100\nbars: 4\nbogus_field: 1\ntracks:\n  - instrument: piano\n    pattern: sustain\n").unwrap();
    let out = bin().arg("validate").arg(&scene).assert().code(2);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    assert!(stderr.contains("bogus_field"), "stderr: {stderr}");
    assert!(stderr.contains("line"), "expected line info, got: {stderr}");
}

#[test]
fn validate_rejects_semantic_error_with_field_path_json() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("bad.yaml");
    fs::write(
        &scene,
        "tempo: 999\nbars: 4\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    let out = bin()
        .args(["--json", "validate"])
        .arg(&scene)
        .assert()
        .code(2);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    let v: serde_json::Value = serde_json::from_str(stderr.trim()).expect("stderr is JSON");
    assert_eq!(v["code"], "validation");
    assert_eq!(v["field"], "tempo");
    assert_eq!(v["exit_code"], 2);
}

#[test]
fn validate_rejects_drums_pattern_on_melodic_instrument() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("bad.yaml");
    fs::write(
        &scene,
        "tempo: 100\nbars: 4\ntracks:\n  - instrument: piano\n    pattern: drums\n",
    )
    .unwrap();
    let out = bin().arg("validate").arg(&scene).assert().code(2);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    assert!(stderr.contains("tracks[0].pattern"), "stderr: {stderr}");
}

#[test]
fn schema_emits_json_schema() {
    let out = bin().arg("schema").assert().success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).into_owned();
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert!(
        v["properties"]["tempo"].is_object(),
        "schema has tempo: {v}"
    );
    assert!(
        v["properties"]["tracks"].is_object(),
        "schema has tracks: {v}"
    );
}

// ---- midi ----

#[test]
fn midi_matches_golden_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("forest.mid");
    bin()
        .arg("midi")
        .arg(forest())
        .arg("-o")
        .arg(&out)
        .assert()
        .success();
    let produced = fs::read(&out).unwrap();
    let golden = fs::read(repo("tests/golden/forest.mid")).unwrap();
    assert_eq!(
        produced, golden,
        "MIDI bytes must be identical to the golden file"
    );
}

#[test]
fn midi_is_deterministic_across_runs() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.mid");
    let b = dir.path().join("b.mid");
    bin()
        .arg("midi")
        .arg(forest())
        .arg("-o")
        .arg(&a)
        .assert()
        .success();
    bin()
        .arg("midi")
        .arg(forest())
        .arg("-o")
        .arg(&b)
        .assert()
        .success();
    assert_eq!(fs::read(&a).unwrap(), fs::read(&b).unwrap());
}

#[test]
fn midi_invalid_scene_leaves_no_partial_file() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("bad.yaml");
    fs::write(
        &scene,
        "tempo: 999\nbars: 4\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    let out = dir.path().join("out.mid");
    bin()
        .arg("midi")
        .arg(&scene)
        .arg("-o")
        .arg(&out)
        .assert()
        .code(2);
    assert_dir_contains_exactly(dir.path(), &["bad.yaml"]);
}

// ---- render ----

fn make_midi(dir: &Path) -> PathBuf {
    let mid = dir.join("scene.mid");
    bin()
        .arg("midi")
        .arg(forest())
        .arg("-o")
        .arg(&mid)
        .assert()
        .success();
    mid
}

#[test]
fn render_happy_path_produces_exact_rate_wav() {
    let dir = tempfile::tempdir().unwrap();
    let mid = make_midi(dir.path());
    let wav = dir.path().join("scene.wav");
    bin()
        .arg("render")
        .arg(&mid)
        .arg("--soundfont")
        .arg(sf2())
        .arg("-o")
        .arg(&wav)
        .assert()
        .success();
    let reader = hound::WavReader::open(&wav).unwrap();
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, 44100);
    let secs = reader.duration() as f64 / f64::from(spec.sample_rate);
    // 8 bars of 4/4 at 92 BPM = 20.87s of music; FluidSynth appends a decay tail.
    let musical = 8.0 * 4.0 * 60.0 / 92.0;
    assert!(
        secs >= musical,
        "render shorter than the music: {secs:.2}s < {musical:.2}s"
    );
    assert!(secs <= musical + 15.0, "unreasonably long tail: {secs:.2}s");
}

#[test]
fn render_text_file_as_soundfont_is_input_error() {
    let dir = tempfile::tempdir().unwrap();
    let mid = make_midi(dir.path());
    let fake = dir.path().join("fake.sf2");
    fs::write(&fake, "this is not a soundfont").unwrap();
    let wav = dir.path().join("scene.wav");
    bin()
        .arg("render")
        .arg(&mid)
        .arg("--soundfont")
        .arg(&fake)
        .arg("-o")
        .arg(&wav)
        .assert()
        .code(2);
    assert_dir_contains_exactly(dir.path(), &["scene.mid", "fake.sf2"]);
}

#[test]
fn render_corrupt_soundfont_fails_without_partial_output() {
    let dir = tempfile::tempdir().unwrap();
    let mid = make_midi(dir.path());
    // Valid RIFF/sfbk magic so it passes the structural pre-check,
    // but the body is garbage: FluidSynth reports errors yet exits 0.
    let fake = dir.path().join("fake.sf2");
    let mut bytes = b"RIFF\x10\x00\x00\x00sfbk".to_vec();
    bytes.extend_from_slice(&[0u8; 16]);
    fs::write(&fake, bytes).unwrap();
    let wav = dir.path().join("scene.wav");
    bin()
        .arg("render")
        .arg(&mid)
        .arg("--soundfont")
        .arg(&fake)
        .arg("-o")
        .arg(&wav)
        .assert()
        .code(4);
    assert_dir_contains_exactly(dir.path(), &["scene.mid", "fake.sf2"]);
}

#[test]
fn render_missing_soundfont_file_is_input_error() {
    let dir = tempfile::tempdir().unwrap();
    let mid = make_midi(dir.path());
    bin()
        .arg("render")
        .arg(&mid)
        .arg("--soundfont")
        .arg(dir.path().join("nope.sf2"))
        .arg("-o")
        .arg(dir.path().join("out.wav"))
        .assert()
        .code(2);
}

#[test]
fn render_missing_fluidsynth_is_dependency_error() {
    let dir = tempfile::tempdir().unwrap();
    let mid = make_midi(dir.path());
    bin()
        .arg("render")
        .arg(&mid)
        .arg("--soundfont")
        .arg(sf2())
        .arg("-o")
        .arg(dir.path().join("out.wav"))
        .env("PATH", "")
        .assert()
        .code(3);
    assert_dir_contains_exactly(dir.path(), &["scene.mid"]);
}

// ---- export ----

#[test]
fn export_happy_path_produces_ogg() {
    let dir = tempfile::tempdir().unwrap();
    let mid = make_midi(dir.path());
    let wav = dir.path().join("scene.wav");
    bin()
        .arg("render")
        .arg(&mid)
        .arg("--soundfont")
        .arg(sf2())
        .arg("-o")
        .arg(&wav)
        .assert()
        .success();
    let ogg = dir.path().join("scene.ogg");
    bin()
        .arg("export")
        .arg(&wav)
        .arg("-o")
        .arg(&ogg)
        .assert()
        .success();
    let size = fs::metadata(&ogg).unwrap().len();
    assert!(size > 10_000, "ogg suspiciously small: {size} bytes");
}

#[test]
fn export_missing_input_is_input_error() {
    let dir = tempfile::tempdir().unwrap();
    bin()
        .arg("export")
        .arg(dir.path().join("nope.wav"))
        .arg("-o")
        .arg(dir.path().join("out.ogg"))
        .assert()
        .code(2);
    assert_dir_contains_exactly(dir.path(), &[]);
}

// ---- build (full chain) ----

#[test]
fn build_full_chain_scene_to_ogg() {
    let dir = tempfile::tempdir().unwrap();
    let ogg = dir.path().join("forest.ogg");
    bin()
        .arg("build")
        .arg(forest())
        .arg("--soundfont")
        .arg(sf2())
        .arg("-o")
        .arg(&ogg)
        .assert()
        .success();
    assert!(fs::metadata(&ogg).unwrap().len() > 10_000);
    // Intermediates are cleaned up unless --keep-intermediates is passed.
    assert_dir_contains_exactly(dir.path(), &["forest.ogg"]);
}
