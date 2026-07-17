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

/// Test-side reimplementation of the loop-length math (`midi::exact_samples`):
/// ticks × (60_000_000 / bpm) × rate / (480 × 1_000_000), rounded.
fn exact_samples(ticks: u64, bpm: u16, rate: u32) -> u64 {
    let micros_per_beat = 60_000_000u64 / u64::from(bpm);
    let num = u128::from(ticks) * u128::from(micros_per_beat) * u128::from(rate);
    let den = 480u128 * 1_000_000u128;
    ((num + den / 2) / den) as u64
}

/// forest.yaml: 8 bars of 4/4 at 92 BPM, PPQ 480.
fn forest_loop_samples() -> u64 {
    exact_samples(8 * 4 * 480, 92, 44100)
}

fn read_frames(path: &Path) -> (hound::WavSpec, Vec<i16>) {
    let mut r = hound::WavReader::open(path).unwrap();
    let spec = r.spec();
    let samples = r.samples::<i16>().map(|s| s.unwrap()).collect();
    (spec, samples)
}

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
    assert_dir_contains_exactly(dir.path(), &["forest.ogg", "forest.meta.json"]);
    let meta: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("forest.meta.json")).unwrap()).unwrap();
    assert_eq!(meta["loop"], true);
    assert_eq!(meta["loop_samples"], forest_loop_samples());
    assert_eq!(meta["audio"], "forest.ogg");
}

#[test]
fn build_loop_wav_is_sample_exact_and_sealed() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("forest.wav");
    bin()
        .arg("build")
        .arg(forest())
        .arg("--soundfont")
        .arg(sf2())
        .arg("-o")
        .arg(&wav)
        .arg("--keep-intermediates")
        .assert()
        .success();
    let l = forest_loop_samples() as usize;
    let (spec, out) = read_frames(&wav);
    let ch = spec.channels as usize;
    assert_eq!(out.len(), l * ch, "loop asset must be exactly L frames");
    // The seal guarantee, bit-exact: the window is raw[L, 2L) and its final
    // frame equals raw[L-1], so wrap-around reproduces an adjacent-sample
    // pair of the original continuous render.
    let (_, raw) = read_frames(&dir.path().join("forest.raw.wav"));
    assert_eq!(&out[..ch], &raw[l * ch..(l + 1) * ch], "out[0] == raw[L]");
    assert_eq!(
        &out[(l - 1) * ch..],
        &raw[(l - 1) * ch..l * ch],
        "out[last] == raw[L-1]"
    );
}

#[test]
fn build_nonloop_wav_has_exact_padded_length() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("sting.yaml");
    // 2 bars of 4/4 at 120 BPM: exactly 4s of music (176400 frames),
    // plus the default 4s decay tail = 352800 frames total.
    fs::write(
        &scene,
        "tempo: 120\nbars: 2\nloop: false\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    let wav = dir.path().join("sting.wav");
    bin()
        .arg("build")
        .arg(&scene)
        .arg("--soundfont")
        .arg(sf2())
        .arg("-o")
        .arg(&wav)
        .assert()
        .success();
    let expected = exact_samples(2 * 4 * 480, 120, 44100) + 4 * 44100;
    assert_eq!(expected, 352_800);
    let (spec, out) = read_frames(&wav);
    assert_eq!(out.len() as u64, expected * u64::from(spec.channels));
}

#[test]
fn build_stems_are_aligned_and_sum_to_mix() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("forest.wav");
    bin()
        .arg("build")
        .arg(forest())
        .arg("--soundfont")
        .arg(sf2())
        .arg("-o")
        .arg(&wav)
        .arg("--stems")
        .assert()
        .success();
    assert_dir_contains_exactly(
        dir.path(),
        &["forest.wav", "forest.meta.json", "forest.stems"],
    );
    let stems_dir = dir.path().join("forest.stems");
    assert_dir_contains_exactly(
        &stems_dir,
        &[
            "01-strings.wav",
            "02-piano.wav",
            "03-bass.wav",
            "04-drums.wav",
        ],
    );
    let l = forest_loop_samples() as usize;
    let (spec, mix) = read_frames(&wav);
    let ch = spec.channels as usize;
    let stems: Vec<Vec<i16>> = [
        "01-strings.wav",
        "02-piano.wav",
        "03-bass.wav",
        "04-drums.wav",
    ]
    .iter()
    .map(|n| {
        let (s, data) = read_frames(&stems_dir.join(n));
        assert_eq!(data.len(), l * ch, "stem {n} must be exactly L frames");
        assert_eq!(s.channels, spec.channels);
        data
    })
    .collect();
    // Stems are cut with the same linear seal, so their sample-wise sum must
    // reconstruct the full mix (small tolerance: independent rounding plus
    // synth mixing noise).
    let n = mix.len();
    let (mut diff2, mut ref2) = (0f64, 0f64);
    for i in 0..n {
        let s: f64 = stems.iter().map(|st| f64::from(st[i])).sum();
        let m = f64::from(mix[i]);
        diff2 += (s - m) * (s - m);
        ref2 += m * m;
    }
    let ratio = (diff2 / ref2.max(1.0)).sqrt();
    assert!(
        ratio < 0.02,
        "stems do not sum to mix: RMS ratio {ratio:.4}"
    );
    let meta: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("forest.meta.json")).unwrap()).unwrap();
    assert_eq!(meta["stems"].as_array().unwrap().len(), 4);
    assert_eq!(meta["stems"][0], "forest.stems/01-strings.wav");
}

#[test]
fn build_corrupt_soundfont_leaves_no_partial_output_or_stems() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("scene.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 1\nloop: true\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    let fake = dir.path().join("fake.sf2");
    let mut bytes = b"RIFF\x10\x00\x00\x00sfbk".to_vec();
    bytes.extend_from_slice(&[0u8; 16]);
    fs::write(&fake, bytes).unwrap();
    bin()
        .arg("build")
        .arg(&scene)
        .arg("--soundfont")
        .arg(&fake)
        .arg("-o")
        .arg(dir.path().join("out.wav"))
        .arg("--stems")
        .assert()
        .code(4);
    // No partial audio, no stems dir, no meta.json, no temp litter.
    assert_dir_contains_exactly(dir.path(), &["scene.yaml", "fake.sf2"]);
}

// ---- export: sample-exact window ----

#[test]
fn export_seek_take_cuts_bit_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(&input, spec).unwrap();
    for i in 0..1000i16 {
        w.write_sample(i).unwrap();
    }
    w.finalize().unwrap();
    let out = dir.path().join("out.wav");
    bin()
        .arg("export")
        .arg(&input)
        .arg("-o")
        .arg(&out)
        .args(["--seek-samples", "100", "--take-samples", "300"])
        .assert()
        .success();
    let (_, data) = read_frames(&out);
    assert_eq!(data.len(), 300);
    assert_eq!(data[0], 100);
    assert_eq!(data[299], 399);
}
