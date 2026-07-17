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
fn build_ogg_stems_leave_no_intermediates() {
    // Regression: encoded stems go through a `.cut.wav` intermediate inside
    // the staging dir; it must not ship inside the renamed stems folder.
    let dir = tempfile::tempdir().unwrap();
    let ogg = dir.path().join("forest.ogg");
    bin()
        .arg("build")
        .arg(forest())
        .arg("--soundfont")
        .arg(sf2())
        .arg("-o")
        .arg(&ogg)
        .arg("--stems")
        .assert()
        .success();
    assert_dir_contains_exactly(
        dir.path(),
        &["forest.ogg", "forest.meta.json", "forest.stems"],
    );
    assert_dir_contains_exactly(
        &dir.path().join("forest.stems"),
        &[
            "01-strings.ogg",
            "02-piano.ogg",
            "03-bass.ogg",
            "04-drums.ogg",
        ],
    );
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

// ---- suites: sections + motifs (M2) ----

/// Two-section suite with a shared motif; small bars for fast rendering.
fn suite_yaml() -> &'static str {
    "tempo: 120\nbars: 2\nkey: C_major\nmotifs:\n  theme:\n    - { degree: 1, beats: 1 }\n    - { degree: 5, beats: 1 }\n    - { degree: 3, beats: 2 }\ntracks:\n  - instrument: flute\n    pattern: melody\n    motif: theme\n  - instrument: strings\n    pattern: sustain\nsections:\n  - name: explore\n    bars: 2\n    loop: true\n  - name: sting\n    bars: 1\n    tempo: 140\n    mute: [0]\n"
}

#[test]
fn build_suite_emits_per_section_assets_with_exact_lengths() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("suite.yaml");
    fs::write(&scene, suite_yaml()).unwrap();
    let out = dir.path().join("suite.wav");
    bin()
        .arg("build")
        .arg(&scene)
        .arg("--soundfont")
        .arg(sf2())
        .arg("-o")
        .arg(&out)
        .assert()
        .success();
    assert_dir_contains_exactly(
        dir.path(),
        &[
            "suite.yaml",
            "suite-explore.wav",
            "suite-sting.wav",
            "suite.meta.json",
        ],
    );
    // explore: 2 bars 4/4 @120, loop → exactly L frames
    let l_explore = exact_samples(2 * 4 * 480, 120, 44100);
    let (spec, explore) = read_frames(&dir.path().join("suite-explore.wav"));
    assert_eq!(explore.len() as u64, l_explore * u64::from(spec.channels));
    // sting: 1 bar @140 (tempo override), non-loop → L + 4s tail
    let l_sting = exact_samples(4 * 480, 140, 44100) + 4 * 44100;
    let (spec, sting) = read_frames(&dir.path().join("suite-sting.wav"));
    assert_eq!(sting.len() as u64, l_sting * u64::from(spec.channels));
    // manifest describes the whole suite
    let meta: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("suite.meta.json")).unwrap()).unwrap();
    assert_eq!(meta["suite"], true);
    let sections = meta["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0]["name"], "explore");
    assert_eq!(sections[0]["loop"], true);
    assert_eq!(sections[0]["loop_samples"], l_explore);
    assert_eq!(sections[1]["name"], "sting");
    assert_eq!(sections[1]["tempo"], 140);
    // muted track dropped from the sting section
    assert_eq!(sections[1]["tracks"].as_array().unwrap().len(), 1);
}

#[test]
fn midi_section_selector_compiles_that_section_deterministically() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("suite.yaml");
    fs::write(&scene, suite_yaml()).unwrap();
    let a = dir.path().join("a.mid");
    let b = dir.path().join("b.mid");
    for out in [&a, &b] {
        bin()
            .arg("midi")
            .arg(&scene)
            .arg("-o")
            .arg(out)
            .args(["--section", "sting"])
            .assert()
            .success();
    }
    assert_eq!(fs::read(&a).unwrap(), fs::read(&b).unwrap());
    // full scene compiles differently from a single section
    let full = dir.path().join("full.mid");
    bin()
        .arg("midi")
        .arg(&scene)
        .arg("-o")
        .arg(&full)
        .assert()
        .success();
    assert_ne!(fs::read(&a).unwrap(), fs::read(&full).unwrap());
}

#[test]
fn midi_unknown_section_is_input_error() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("suite.yaml");
    fs::write(&scene, suite_yaml()).unwrap();
    let out = bin()
        .arg("midi")
        .arg(&scene)
        .arg("-o")
        .arg(dir.path().join("x.mid"))
        .args(["--section", "boss"])
        .assert()
        .code(2);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    assert!(stderr.contains("boss"), "stderr: {stderr}");
    assert_dir_contains_exactly(dir.path(), &["suite.yaml"]);
}

#[test]
fn validate_rejects_unknown_motif_reference() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("bad.yaml");
    fs::write(
        &scene,
        "tempo: 100\nbars: 2\ntracks:\n  - instrument: flute\n    pattern: melody\n    motif: nonexistent\n",
    )
    .unwrap();
    let out = bin()
        .args(["--json", "validate"])
        .arg(&scene)
        .assert()
        .code(2);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    let v: serde_json::Value = serde_json::from_str(stderr.trim()).expect("stderr is JSON");
    assert_eq!(v["field"], "tracks[0].motif");
}

#[test]
fn validate_rejects_duplicate_section_names_and_mute_all() {
    let dir = tempfile::tempdir().unwrap();
    let dup = dir.path().join("dup.yaml");
    fs::write(
        &dup,
        "tempo: 100\nbars: 2\ntracks:\n  - instrument: piano\n    pattern: sustain\nsections:\n  - { name: a, bars: 1 }\n  - { name: a, bars: 2 }\n",
    )
    .unwrap();
    let out = bin().arg("validate").arg(&dup).assert().code(2);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    assert!(stderr.contains("sections[1].name"), "stderr: {stderr}");

    let mute = dir.path().join("mute.yaml");
    fs::write(
        &mute,
        "tempo: 100\nbars: 2\ntracks:\n  - instrument: piano\n    pattern: sustain\nsections:\n  - { name: a, bars: 1, mute: [0] }\n",
    )
    .unwrap();
    let out = bin().arg("validate").arg(&mute).assert().code(2);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    assert!(stderr.contains("sections[0].mute"), "stderr: {stderr}");
}

#[test]
fn example_suite_validates() {
    bin()
        .arg("validate")
        .arg(repo("examples/scenes/forest_suite.yaml"))
        .assert()
        .success();
}

// ---- renderer backends (M3) ----

/// Same DSL, second backend: identical sample-exact length, different timbre.
#[test]
fn build_timidity_backend_same_length_different_timbre() {
    let dir = tempfile::tempdir().unwrap();
    let tim = dir.path().join("tim.wav");
    let flu = dir.path().join("flu.wav");
    for (out, renderer) in [(&tim, "timidity"), (&flu, "fluidsynth")] {
        bin()
            .arg("build")
            .arg(forest())
            .arg("--soundfont")
            .arg(sf2())
            .arg("-o")
            .arg(out)
            .args(["--renderer", renderer])
            .assert()
            .success();
    }
    let (spec_t, t) = read_frames(&tim);
    let (spec_f, f) = read_frames(&flu);
    let expected = forest_loop_samples() * u64::from(spec_t.channels);
    assert_eq!(t.len() as u64, expected, "timidity length");
    assert_eq!(f.len() as u64, expected, "fluidsynth length");
    assert_eq!(spec_t.sample_rate, spec_f.sample_rate);
    assert_ne!(t, f, "backends should produce different renders");
    // Both produce actual audio, not silence.
    assert!(t.iter().any(|&s| s.abs() > 100), "timidity is silent");
    assert!(f.iter().any(|&s| s.abs() > 100), "fluidsynth is silent");
}

/// Corrupt SF2 that passes the magic pre-check: TiMidity exits 0 and writes a
/// header-only WAV; the zero-frame backstop must turn that into a failure.
#[test]
fn render_timidity_corrupt_soundfont_fails_without_partial_output() {
    let dir = tempfile::tempdir().unwrap();
    let mid = make_midi(dir.path());
    let fake = dir.path().join("fake.sf2");
    let mut bytes = b"RIFF\x10\x00\x00\x00sfbk".to_vec();
    bytes.extend_from_slice(&[0u8; 16]);
    fs::write(&fake, bytes).unwrap();
    bin()
        .arg("render")
        .arg(&mid)
        .arg("--soundfont")
        .arg(&fake)
        .arg("-o")
        .arg(dir.path().join("scene.wav"))
        .args(["--renderer", "timidity"])
        .assert()
        .code(4);
    assert_dir_contains_exactly(dir.path(), &["scene.mid", "fake.sf2"]);
}

#[test]
fn render_timidity_missing_soundfont_is_input_error() {
    let dir = tempfile::tempdir().unwrap();
    let mid = make_midi(dir.path());
    bin()
        .arg("render")
        .arg(&mid)
        .arg("--soundfont")
        .arg(dir.path().join("nope.sf2"))
        .arg("-o")
        .arg(dir.path().join("scene.wav"))
        .args(["--renderer", "timidity"])
        .assert()
        .code(2);
    assert_dir_contains_exactly(dir.path(), &["scene.mid"]);
}

// ---- diff: semantic scene comparison (M4) ----

#[test]
fn diff_reports_semantic_changes_and_ignores_formatting() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.yaml");
    let b = dir.path().join("b.yaml");
    fs::write(
        &a,
        "tempo: 100\nbars: 2\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    // Same music, different formatting/key order → empty diff.
    fs::write(
        &b,
        "bars: 2\ntempo: 100\ntracks:\n  - {instrument: piano, pattern: sustain}\n",
    )
    .unwrap();
    let out = bin().arg("diff").arg(&a).arg(&b).assert().success();
    assert_eq!(String::from_utf8_lossy(&out.get_output().stdout).trim(), "");

    let c = dir.path().join("c.yaml");
    fs::write(
        &c,
        "tempo: 120\nbars: 2\ntracks:\n  - instrument: piano\n    pattern: sustain\n    intensity: 0.9\n",
    )
    .unwrap();
    let out = bin().arg("diff").arg(&a).arg(&c).assert().success();
    let text = String::from_utf8_lossy(&out.get_output().stdout).into_owned();
    assert!(text.contains("~ tempo 100 -> 120"), "stdout: {text}");
    assert!(
        text.contains("~ tracks[0].intensity 0.6 -> 0.9"),
        "stdout: {text}"
    );

    // --json emits the same records as a machine-readable array.
    let out = bin()
        .args(["--json", "diff"])
        .arg(&a)
        .arg(&c)
        .assert()
        .success();
    let v: serde_json::Value =
        serde_json::from_slice(&out.get_output().stdout).expect("stdout is JSON");
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert!(arr.iter().any(|c| c["path"] == "tempo" && c["op"] == "~"));
}

#[test]
fn diff_invalid_scene_is_input_error() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.yaml");
    let bad = dir.path().join("bad.yaml");
    fs::write(
        &a,
        "tempo: 100\nbars: 2\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    fs::write(&bad, "tempo: 9999\nbars: 2\ntracks: []\n").unwrap();
    bin().arg("diff").arg(&a).arg(&bad).assert().code(2);
}

// ---- batch: many scenes, machine-readable report (M4) ----

#[test]
fn batch_builds_all_scenes_and_writes_report() {
    let dir = tempfile::tempdir().unwrap();
    let s1 = dir.path().join("one.yaml");
    let s2 = dir.path().join("two.yaml");
    fs::write(
        &s1,
        "tempo: 120\nbars: 1\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    fs::write(
        &s2,
        "tempo: 140\nbars: 1\nloop: true\ntracks:\n  - instrument: strings\n    pattern: sustain\n",
    )
    .unwrap();
    let out_dir = dir.path().join("out");
    bin()
        .arg("batch")
        .arg(&s1)
        .arg(&s2)
        .arg("--soundfont")
        .arg(sf2())
        .arg("--out-dir")
        .arg(&out_dir)
        .args(["--format", "wav"])
        .assert()
        .success();
    assert_dir_contains_exactly(
        &out_dir,
        &[
            "one.wav",
            "one.meta.json",
            "two.wav",
            "two.meta.json",
            "report.json",
        ],
    );
    // two.yaml loops: exactly L frames at 140 BPM.
    let (spec, frames) = read_frames(&out_dir.join("two.wav"));
    let expected = exact_samples(4 * 480, 140, 44100) * u64::from(spec.channels);
    assert_eq!(frames.len() as u64, expected);
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(out_dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["total"], 2);
    assert_eq!(report["succeeded"], 2);
    assert_eq!(report["failed"], 0);
    assert_eq!(report["items"].as_array().unwrap().len(), 2);
}

#[test]
fn batch_partial_failure_reports_and_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("good.yaml");
    let bad = dir.path().join("bad.yaml");
    fs::write(
        &good,
        "tempo: 120\nbars: 1\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    fs::write(&bad, "tempo: 9999\nbars: 2\ntracks: []\n").unwrap();
    let out_dir = dir.path().join("out");
    bin()
        .arg("batch")
        .arg(&good)
        .arg(&bad)
        .arg("--soundfont")
        .arg(sf2())
        .arg("--out-dir")
        .arg(&out_dir)
        .args(["--format", "wav"])
        .assert()
        .code(2); // exit reflects the first failure
    // The good scene still built; the failure is recorded in the report.
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(out_dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["succeeded"], 1);
    assert_eq!(report["failed"], 1);
    let items = report["items"].as_array().unwrap();
    assert_eq!(items[0]["ok"], true);
    assert_eq!(items[1]["ok"], false);
    assert_eq!(items[1]["error"]["exit_code"], 2);
    assert!(out_dir.join("good.wav").is_file());
    assert!(!out_dir.join("bad.wav").exists());
}

#[test]
fn batch_duplicate_scene_stems_is_input_error() {
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("sub");
    fs::create_dir(&sub).unwrap();
    let a = dir.path().join("same.yaml");
    let b = sub.join("same.yaml");
    for p in [&a, &b] {
        fs::write(
            p,
            "tempo: 120\nbars: 1\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
        )
        .unwrap();
    }
    let out_dir = dir.path().join("out");
    bin()
        .arg("batch")
        .arg(&a)
        .arg(&b)
        .arg("--soundfont")
        .arg(sf2())
        .arg("--out-dir")
        .arg(&out_dir)
        .assert()
        .code(2);
    assert!(!out_dir.exists(), "nothing should be built");
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
