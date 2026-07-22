//! E2E tests backing the acceptance matrix in docs/roadmap.md.
//! Audio tests need `fluidsynth`, `ffmpeg` and `assets/TimGM6mb.sf2`
//! (run `scripts/fetch_assets.sh` once to download the SoundFont).

use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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

fn write_default_soundfont_library(dir: &Path) -> PathBuf {
    let root = dir.join("sound-library");
    let sf2_dir = root.join("sf2");
    fs::create_dir_all(&sf2_dir).unwrap();
    fs::copy(sf2(), sf2_dir.join("MuseScore_General.sf2")).unwrap();
    root
}

fn forest() -> PathBuf {
    repo("examples/scenes/forest.yaml")
}

/// Tests use a local `assets/bin/sfizz_render` binary so SFZ E2E coverage does
/// not depend on any system-wide installation.
fn sfizz_render_bin() -> PathBuf {
    let p = repo("assets/bin/sfizz_render");
    assert!(
        p.is_file(),
        "missing sfizz_render binary {} — run scripts/build_sfizz.sh first",
        p.display()
    );
    p
}

/// Prepend `assets/bin` to PATH so the CLI's `sfizz_render` lookup succeeds,
/// without requiring it to be installed system-wide.
fn sfizz_path_env() -> std::ffi::OsString {
    let bin_dir = sfizz_render_bin().parent().unwrap().to_path_buf();
    let existing = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![bin_dir];
    paths.extend(std::env::split_paths(&existing));
    std::env::join_paths(paths).unwrap()
}

/// A tiny, self-contained SFZ instrument (one region, one synthetic sine
/// sample) generated on the fly — no committed binary fixture, no external
/// sample library needed for the sfizz test suite to run anywhere.
fn write_sine_sfz(dir: &Path) -> PathBuf {
    let wav_path = dir.join("sine.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&wav_path, spec).unwrap();
    for i in 0..4410u32 {
        let t = f64::from(i) / 44100.0;
        let v = (3000.0 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i16;
        writer.write_sample(v).unwrap();
    }
    writer.finalize().unwrap();

    let sfz_path = dir.join("mini.sfz");
    fs::write(&sfz_path, "<region>\nsample=sine.wav\nlokey=0\nhikey=127\n").unwrap();
    sfz_path
}

/// Renderer profile mapping `violin`/`cello` (used by the tiny sfizz test
/// scenes below) to the synthetic sine instrument.
fn write_test_profile(dir: &Path) -> PathBuf {
    write_sine_sfz(dir);
    let profile_path = dir.join("profile.yaml");
    fs::write(
        &profile_path,
        "name: test-profile\ninstruments:\n  violin:\n    sustain: mini.sfz\n  cello:\n    sustain: mini.sfz\n",
    )
    .unwrap();
    profile_path
}

fn tiny_sfizz_scene(dir: &Path) -> PathBuf {
    let scene = dir.join("duo.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 2\nloop: false\ntracks:\n  - instrument: violin\n    pattern: sustain\n  - instrument: cello\n    pattern: sustain\n",
    )
    .unwrap();
    scene
}

/// A short single-track MIDI (2 bars @ 120 BPM, ~4s) — deliberately not
/// `make_midi(forest())`'s full 4-track scene, which would push hundreds of
/// simultaneous notes through one tiny single-cycle sine region and take
/// minutes to render.
fn make_tiny_midi(dir: &Path) -> PathBuf {
    let scene = tiny_sfizz_scene(dir);
    let mid = dir.join("scene.mid");
    bin()
        .arg("midi")
        .arg(&scene)
        .arg("-o")
        .arg(&mid)
        .arg("--solo")
        .arg("0")
        .assert()
        .success();
    mid
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

#[cfg(unix)]
fn write_fake_tool(dir: &Path, name: &str, version: &str) {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\nprintf '%s\\n' '{version}'\n")).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

// ---- environment diagnostics ----

#[cfg(unix)]
#[test]
fn doctor_reports_platform_and_ready_toolchain_as_json() {
    let dir = tempfile::tempdir().unwrap();
    let sound_library = write_default_soundfont_library(dir.path());
    write_fake_tool(dir.path(), "ffmpeg", "ffmpeg test 1.0");
    write_fake_tool(dir.path(), "fluidsynth", "FluidSynth test 1.0");
    write_fake_tool(dir.path(), "timidity", "TiMidity++ test 1.0");
    write_fake_tool(dir.path(), "sfizz_render", "sfizz test 1.0");

    let out = bin()
        .args(["--json", "doctor"])
        .env("PATH", dir.path())
        .env("SCOREKIT_SOUND_LIBRARY_DIR", &sound_library)
        .assert()
        .success();
    let report: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(report["scorekit_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(report["ready"], true);
    assert_eq!(report["platform"]["os"], std::env::consts::OS);
    assert_eq!(report["platform"]["arch"], std::env::consts::ARCH);
    assert!(
        report["platform"]["release_asset"]
            .as_str()
            .unwrap()
            .contains(std::env::consts::ARCH)
    );
    assert_eq!(report["tools"].as_array().unwrap().len(), 4);
    assert!(
        report["tools"]
            .as_array()
            .unwrap()
            .iter()
            .all(|tool| tool["status"] == "ok")
    );
    assert_eq!(report["sound_library"]["default_soundfont"]["status"], "ok");
    assert_dir_contains_exactly(
        dir.path(),
        &[
            "ffmpeg",
            "fluidsynth",
            "sfizz_render",
            "sound-library",
            "timidity",
        ],
    );
}

#[cfg(unix)]
#[test]
fn doctor_missing_renderer_returns_dependency_report_and_arch_help() {
    let dir = tempfile::tempdir().unwrap();
    write_fake_tool(dir.path(), "ffmpeg", "ffmpeg test 1.0");

    let out = bin()
        .args(["--json", "doctor"])
        .env("PATH", dir.path())
        .assert()
        .code(3);
    let payload: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    assert_eq!(payload["code"], "doctor");
    assert_eq!(payload["exit_code"], 3);
    assert_eq!(payload["report"]["ready"], false);
    assert_eq!(
        payload["report"]["scorekit_version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(payload["report"]["requirements"]["ffmpeg"], true);
    assert_eq!(payload["report"]["requirements"]["renderer"], false);
    assert!(
        payload["report"]["hints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hint| hint.as_str().unwrap().contains(std::env::consts::ARCH))
    );
    assert_dir_contains_exactly(dir.path(), &["ffmpeg"]);
}

// ---- validate / schema ----

#[test]
fn validate_happy_path() {
    bin().args(["validate"]).arg(forest()).assert().success();
}

#[test]
fn all_shipped_examples_validate() {
    // Guards examples/scenes/ against schema drift: every scene we ship
    // must always pass `validate`.
    let dir = repo("examples/scenes");
    let mut count = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "yaml") {
            bin().arg("validate").arg(&path).assert().success();
            count += 1;
        }
    }
    assert!(count >= 7, "expected shipped examples, found {count}");
}

#[test]
fn skill_narrative_worked_example_validates() {
    for example in ["exile-in-the-dunes.yaml", "exile-in-the-dunes-v2.yaml"] {
        bin()
            .arg("validate")
            .arg(repo("skills/scorekit/examples").join(example))
            .assert()
            .success();
    }
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
    assert!(
        v["properties"]["story"].is_object(),
        "schema has story: {v}"
    );
    assert!(
        v["properties"]["textures"].is_object(),
        "schema has textures: {v}"
    );
    assert_eq!(v["properties"]["tempo"]["minimum"], 20);
    assert_eq!(v["properties"]["tempo"]["maximum"], 300);
    assert_eq!(v["properties"]["bars"]["minimum"], 1);
    assert_eq!(v["properties"]["bars"]["maximum"], 256);
    assert_eq!(
        v["$defs"]["TextureTrack"]["properties"]["gain"]["minimum"],
        0.0
    );
    assert_eq!(
        v["$defs"]["TextureTrack"]["properties"]["gain"]["maximum"],
        1.0
    );
    assert_eq!(
        v["$defs"]["TextureTrack"]["properties"]["start_beat"]["minimum"],
        0.0
    );
    assert_eq!(
        v["$defs"]["TextureTrack"]["properties"]["at"]["items"]["minimum"],
        0.0
    );
    assert_eq!(
        v["$defs"]["MotifNote"]["properties"]["beats"]["minimum"],
        0.125
    );
    assert_eq!(
        v["$defs"]["MotifNote"]["properties"]["beats"]["maximum"],
        16.0
    );
    assert_eq!(
        v["$defs"]["Track"]["properties"]["intensity"]["minimum"],
        0.0
    );
    assert_eq!(
        v["$defs"]["Track"]["properties"]["intensity"]["maximum"],
        1.0
    );

    let out = bin()
        .args(["schema", "--texture-profile"])
        .assert()
        .success();
    let profile: serde_json::Value =
        serde_json::from_slice(&out.get_output().stdout).expect("valid profile schema");
    assert!(profile["properties"]["sources"].is_object());
}

#[test]
fn story_is_informational_and_never_affects_midi_bytes() {
    // `story` is an annotation for downstream agent review; the protocol
    // guarantees it never changes compiled output. Same scene with and
    // without a story must validate and produce byte-identical MIDI.
    let dir = tempfile::tempdir().unwrap();
    let base = "tempo: 100\nbars: 2\ntracks:\n  - instrument: piano\n    pattern: sustain\n";
    let plain = dir.path().join("plain.yaml");
    let storied = dir.path().join("storied.yaml");
    fs::write(&plain, base).unwrap();
    fs::write(
        &storied,
        format!("story: A quiet dawn over the ruined citadel.\n{base}"),
    )
    .unwrap();
    bin().arg("validate").arg(&storied).assert().success();
    let a = dir.path().join("a.mid");
    let b = dir.path().join("b.mid");
    bin()
        .arg("midi")
        .arg(&plain)
        .arg("-o")
        .arg(&a)
        .assert()
        .success();
    bin()
        .arg("midi")
        .arg(&storied)
        .arg("-o")
        .arg(&b)
        .assert()
        .success();
    assert_eq!(
        fs::read(&a).unwrap(),
        fs::read(&b).unwrap(),
        "story must not change compiled MIDI bytes"
    );
}

#[test]
fn textures_do_not_change_midi_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let base = "tempo: 120\nbars: 2\ntracks:\n  - instrument: piano\n    pattern: sustain\n";
    let plain = dir.path().join("plain.yaml");
    let textured = dir.path().join("textured.yaml");
    fs::write(&plain, base).unwrap();
    fs::write(
        &textured,
        format!(
            "textures:\n  - {{ source: river, mode: loop, gain: 0.25 }}\n  - {{ source: birds, mode: one_shot, at: [1, 5] }}\n{base}"
        ),
    )
    .unwrap();
    let a = dir.path().join("a.mid");
    let b = dir.path().join("b.mid");
    bin()
        .arg("midi")
        .arg(&plain)
        .arg("-o")
        .arg(&a)
        .assert()
        .success();
    bin()
        .arg("midi")
        .arg(&textured)
        .arg("-o")
        .arg(&b)
        .assert()
        .success();
    assert_eq!(fs::read(a).unwrap(), fs::read(b).unwrap());
}

#[test]
fn validate_rejects_ambiguous_texture_placement() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("bad.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 2\nloop: true\ntextures:\n  - source: river\n    mode: loop\n    start_beat: 1\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    let out = bin()
        .args(["--json", "validate"])
        .arg(&scene)
        .assert()
        .code(2);
    let error: serde_json::Value =
        serde_json::from_slice(&out.get_output().stderr).expect("structured validation error");
    assert_eq!(error["field"], "textures[0].start_beat");
}

#[test]
fn validate_rejects_texture_trigger_outside_shortest_section() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("suite.yaml");
    fs::write(
        &scene,
        "tempo: 60\nbars: 2\ntextures:\n  - source: bell\n    mode: one_shot\n    at: [5]\ntracks:\n  - instrument: piano\n    pattern: sustain\nsections:\n  - name: short\n    bars: 1\n    loop: true\n  - name: long\n    bars: 2\n    loop: false\n",
    )
    .unwrap();
    let out = bin()
        .args(["--json", "validate"])
        .arg(&scene)
        .assert()
        .code(2);
    let error: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    assert_eq!(error["field"], "textures[0].at[0]");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("section `short`"),
        "error identifies the section whose timeline would wrap: {error}"
    );
    assert_dir_contains_exactly(dir.path(), &["suite.yaml"]);
}

#[test]
fn validate_rejects_non_string_story_with_location() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("bad.yaml");
    fs::write(
        &scene,
        "story: { mood: 0.9 }\ntempo: 100\nbars: 4\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    let out = bin().arg("validate").arg(&scene).assert().code(2);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    assert!(stderr.contains("story"), "stderr: {stderr}");
    assert!(stderr.contains("line"), "expected line info, got: {stderr}");
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
fn render_uses_musescore_general_from_default_sound_library() {
    let dir = tempfile::tempdir().unwrap();
    let library = write_default_soundfont_library(dir.path());
    let mid = make_tiny_midi(dir.path());
    let wav = dir.path().join("default.wav");
    bin()
        .arg("render")
        .arg(&mid)
        .arg("-o")
        .arg(&wav)
        .env("SCOREKIT_SOUND_LIBRARY_DIR", &library)
        .assert()
        .success();
    assert!(fs::metadata(wav).unwrap().len() > 1_000);
}

/// `build` with `--soundfont` omitted resolves MuseScore General from the
/// configured sound library, same as `render`.
#[test]
fn build_uses_musescore_general_from_default_sound_library() {
    let dir = tempfile::tempdir().unwrap();
    let library = write_default_soundfont_library(dir.path());
    let scene = dir.path().join("solo.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 1\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    let wav = dir.path().join("solo.wav");
    bin()
        .arg("build")
        .arg(&scene)
        .arg("-o")
        .arg(&wav)
        .env("SCOREKIT_SOUND_LIBRARY_DIR", &library)
        .assert()
        .success();
    assert!(fs::metadata(&wav).unwrap().len() > 1_000);
    assert!(dir.path().join("solo.meta.json").is_file());
}

/// `batch` with `--soundfont` omitted resolves the same default; the check
/// runs once up front, so an empty library fails before any file is written.
#[test]
fn batch_uses_musescore_general_from_default_sound_library() {
    let dir = tempfile::tempdir().unwrap();
    let library = write_default_soundfont_library(dir.path());
    let scene = dir.path().join("solo.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 1\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    let out_dir = dir.path().join("out");
    bin()
        .arg("batch")
        .arg(&scene)
        .arg("--out-dir")
        .arg(&out_dir)
        .args(["--format", "wav"])
        .env("SCOREKIT_SOUND_LIBRARY_DIR", &library)
        .assert()
        .success();
    assert!(fs::metadata(out_dir.join("solo.wav")).unwrap().len() > 1_000);
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(out_dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["succeeded"], 1);
}

#[test]
fn batch_missing_default_soundfont_fails_before_writing_anything() {
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("empty-library");
    fs::create_dir(&library).unwrap();
    let scene = dir.path().join("solo.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 1\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    let out_dir = dir.path().join("out");
    let out = bin()
        .args(["--json", "batch"])
        .arg(&scene)
        .arg("--out-dir")
        .arg(&out_dir)
        .args(["--format", "wav"])
        .env("SCOREKIT_SOUND_LIBRARY_DIR", &library)
        .assert()
        .code(2);
    let error: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    assert_eq!(error["code"], "validation");
    assert_eq!(error["field"], "--soundfont");
    assert!(!out_dir.exists(), "no out-dir may be created on failure");
}

#[test]
fn render_missing_default_soundfont_is_structured_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("empty-library");
    fs::create_dir(&library).unwrap();
    let mid = make_tiny_midi(dir.path());
    let wav = dir.path().join("default.wav");
    let out = bin()
        .args(["--json", "render"])
        .arg(&mid)
        .arg("-o")
        .arg(&wav)
        .env("SCOREKIT_SOUND_LIBRARY_DIR", &library)
        .assert()
        .code(2);
    let error: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    assert_eq!(error["code"], "validation");
    assert_eq!(error["field"], "--soundfont");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("MuseScore_General.sf2")
    );
    assert!(!wav.exists());
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

fn write_texture_wave(path: &Path, frequency: f64, seconds: f64) {
    // Deliberately mono/22.05 kHz: the E2E proves FFmpeg normalization is
    // part of the texture boundary rather than an undocumented input rule.
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 22_050,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    let frames = (seconds * f64::from(spec.sample_rate)).round() as u32;
    for i in 0..frames {
        let t = f64::from(i) / f64::from(spec.sample_rate);
        let sample = (2500.0 * (2.0 * std::f64::consts::PI * frequency * t).sin()) as i16;
        writer.write_sample(sample).unwrap();
    }
    writer.finalize().unwrap();
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
    // The scene's story annotation is echoed for downstream agent review.
    assert!(
        meta["story"].as_str().unwrap().contains("forest"),
        "meta.json carries story: {meta}"
    );
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
fn build_rejects_nonfinite_tail_as_structured_input_error() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("sting.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 1\nloop: false\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    let output = dir.path().join("sting.wav");
    let out = bin()
        .args(["--json", "build"])
        .arg(&scene)
        .arg("--soundfont")
        .arg(sf2())
        .arg("--tail")
        .arg("inf")
        .arg("-o")
        .arg(&output)
        .assert()
        .code(2);
    let error: serde_json::Value =
        serde_json::from_slice(&out.get_output().stderr).expect("stderr is one JSON error");
    assert_eq!(error["code"], "validation");
    assert_eq!(error["field"], "--tail");
    assert_dir_contains_exactly(dir.path(), &["sting.yaml"]);
}

#[test]
fn numeric_cli_options_reject_out_of_range_values_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("scene.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 1\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    let midi = dir.path().join("missing.mid");
    let audio = dir.path().join("missing.wav");

    let cases = [
        (
            vec![
                "render".into(),
                midi.as_os_str().to_owned(),
                "--sample-rate".into(),
                "0".into(),
                "-o".into(),
                dir.path().join("rate.wav").into_os_string(),
            ],
            "--sample-rate",
        ),
        (
            vec![
                "render".into(),
                midi.as_os_str().to_owned(),
                "--gain".into(),
                "NaN".into(),
                "-o".into(),
                dir.path().join("gain.wav").into_os_string(),
            ],
            "--gain",
        ),
        (
            vec![
                "export".into(),
                audio.as_os_str().to_owned(),
                "--quality".into(),
                "11".into(),
                "-o".into(),
                dir.path().join("quality.ogg").into_os_string(),
            ],
            "--quality",
        ),
        (
            vec![
                "build".into(),
                scene.as_os_str().to_owned(),
                "--crossfade-ms".into(),
                "60001".into(),
                "-o".into(),
                dir.path().join("crossfade.wav").into_os_string(),
            ],
            "--crossfade-ms",
        ),
    ];

    for (args, field) in cases {
        let out = bin().arg("--json").args(args).assert().code(2);
        let error: serde_json::Value =
            serde_json::from_slice(&out.get_output().stderr).expect("stderr is one JSON error");
        assert_eq!(error["code"], "validation");
        assert_eq!(error["field"], field);
    }
    assert_dir_contains_exactly(dir.path(), &["scene.yaml"]);
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
fn build_textures_normalizes_places_mixes_and_emits_stems() {
    let dir = tempfile::tempdir().unwrap();
    let river = dir.path().join("river.wav");
    let birds = dir.path().join("birds.wav");
    write_texture_wave(&river, 137.0, 0.2);
    write_texture_wave(&birds, 733.0, 0.08);
    let profile = dir.path().join("textures.yaml");
    fs::write(
        &profile,
        "name: field-recordings\nsources:\n  river: river.wav\n  birds: birds.wav\n",
    )
    .unwrap();
    let scene = dir.path().join("scene.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 2\nloop: true\ntextures:\n  - source: river\n    mode: loop\n    gain: 0.25\n  - source: birds\n    mode: one_shot\n    at: [1, 5]\n    gain: 0.5\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    let output = dir.path().join("scene.wav");
    bin()
        .arg("build")
        .arg(&scene)
        .arg("--soundfont")
        .arg(sf2())
        .arg("--texture-profile")
        .arg(&profile)
        .arg("--stems")
        .arg("-o")
        .arg(&output)
        .assert()
        .success();

    assert_dir_contains_exactly(
        dir.path(),
        &[
            "birds.wav",
            "river.wav",
            "scene.meta.json",
            "scene.stems",
            "scene.wav",
            "scene.yaml",
            "textures.yaml",
        ],
    );
    let stems_dir = dir.path().join("scene.stems");
    let stem_names = [
        "01-piano.wav",
        "02-texture-river.wav",
        "03-texture-birds.wav",
    ];
    assert_dir_contains_exactly(&stems_dir, &stem_names);
    let expected_frames = exact_samples(2 * 4 * 480, 120, 44_100);
    let (spec, mix) = read_frames(&output);
    assert_eq!(spec.channels, 2, "texture normalization targets stereo");
    assert_eq!(spec.sample_rate, 44_100);
    assert_eq!(mix.len() as u64, expected_frames * 2);
    let stems: Vec<Vec<i16>> = stem_names
        .iter()
        .map(|name| {
            let (stem_spec, samples) = read_frames(&stems_dir.join(name));
            assert_eq!(stem_spec, spec);
            assert_eq!(samples.len(), mix.len());
            samples
        })
        .collect();
    assert!(
        stems[1].iter().any(|&sample| sample != 0),
        "loop texture stem is audible"
    );
    assert!(
        stems[2].iter().any(|&sample| sample != 0),
        "one-shot texture stem is audible"
    );
    let (mut diff2, mut reference2) = (0.0f64, 0.0f64);
    for i in 0..mix.len() {
        let stem_sum: f64 = stems.iter().map(|stem| f64::from(stem[i])).sum();
        let full = f64::from(mix[i]);
        diff2 += (stem_sum - full).powi(2);
        reference2 += full.powi(2);
    }
    let ratio = (diff2 / reference2.max(1.0)).sqrt();
    assert!(ratio < 0.02, "texture stems do not sum to mix: {ratio:.4}");

    let metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("scene.meta.json")).unwrap()).unwrap();
    assert_eq!(metadata["textures"].as_array().unwrap().len(), 2);
    assert_eq!(metadata["textures"][0]["source"], "river");
    assert_eq!(metadata["stems"].as_array().unwrap().len(), 3);
}

#[test]
fn build_missing_texture_source_leaves_no_partial_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("scene.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 1\ntextures:\n  - { source: river, mode: loop }\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    let profile = dir.path().join("textures.yaml");
    fs::write(
        &profile,
        "name: missing-source\nsources:\n  river: missing.wav\n",
    )
    .unwrap();
    bin()
        .arg("build")
        .arg(&scene)
        .arg("--soundfont")
        .arg(sf2())
        .arg("--texture-profile")
        .arg(&profile)
        .arg("--stems")
        .arg("-o")
        .arg(dir.path().join("scene.wav"))
        .assert()
        .code(2);
    assert_dir_contains_exactly(dir.path(), &["scene.yaml", "textures.yaml"]);
}

#[test]
fn suite_failure_rolls_back_all_previously_built_sections() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("long-bell.wav");
    write_texture_wave(&source, 440.0, 5.0);
    let profile = dir.path().join("textures.yaml");
    fs::write(
        &profile,
        "name: rollback-test\nsources:\n  long_bell: long-bell.wav\n",
    )
    .unwrap();
    let scene = dir.path().join("suite.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 2\ntextures:\n  - source: long_bell\n    mode: one_shot\n    at: [0]\ntracks:\n  - instrument: piano\n    pattern: sustain\nsections:\n  - name: long\n    bars: 2\n    loop: false\n  - name: short\n    bars: 1\n    loop: true\n",
    )
    .unwrap();
    let out = bin()
        .args(["--json", "build"])
        .arg(&scene)
        .arg("--soundfont")
        .arg(sf2())
        .arg("--texture-profile")
        .arg(&profile)
        .args(["--sample-rate", "8000", "--tail", "0"])
        .arg("-o")
        .arg(dir.path().join("suite.wav"))
        .assert()
        .code(2);
    let error: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    assert_eq!(error["field"], "textures[0].source");
    assert_dir_contains_exactly(
        dir.path(),
        &["long-bell.wav", "suite.yaml", "textures.yaml"],
    );

    // A failed rebuild must also leave an already published suite byte-for-byte
    // untouched, rather than exposing a mixture of old and new sections.
    let prior = [
        ("suite.wav", b"old-main".as_slice()),
        ("suite-long.wav", b"old-long".as_slice()),
        ("suite-short.wav", b"old-short".as_slice()),
        ("suite.meta.json", b"old-manifest".as_slice()),
    ];
    for (name, contents) in prior {
        fs::write(dir.path().join(name), contents).unwrap();
    }
    bin()
        .args(["--json", "build"])
        .arg(&scene)
        .arg("--soundfont")
        .arg(sf2())
        .arg("--texture-profile")
        .arg(&profile)
        .args(["--sample-rate", "8000", "--tail", "0"])
        .arg("-o")
        .arg(dir.path().join("suite.wav"))
        .assert()
        .code(2);
    for (name, contents) in prior {
        assert_eq!(fs::read(dir.path().join(name)).unwrap(), contents);
    }
    assert_dir_contains_exactly(
        dir.path(),
        &[
            "long-bell.wav",
            "suite.yaml",
            "textures.yaml",
            "suite.wav",
            "suite-long.wav",
            "suite-short.wav",
            "suite.meta.json",
        ],
    );
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
            "suite.wav",
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
    // main playback file: all sections concatenated in order, sample-exactly
    let (spec, main) = read_frames(&out);
    assert_eq!(
        main.len() as u64,
        (l_explore + l_sting) * u64::from(spec.channels)
    );
    assert_eq!(
        &main[..explore.len()],
        &explore[..],
        "main starts with explore"
    );
    assert_eq!(&main[explore.len()..], &sting[..], "main ends with sting");
    // manifest describes the whole suite, main file included
    let meta: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("suite.meta.json")).unwrap()).unwrap();
    assert_eq!(meta["suite"], true);
    assert_eq!(meta["audio"], "suite.wav");
    assert_eq!(meta["loop"], false);
    assert_eq!(meta["total_samples"], l_explore + l_sting);
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
fn successful_suite_build_replaces_existing_artifacts_as_one_set() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("suite.yaml");
    fs::write(&scene, suite_yaml()).unwrap();
    for name in [
        "suite.wav",
        "suite-explore.wav",
        "suite-sting.wav",
        "suite.meta.json",
    ] {
        fs::write(dir.path().join(name), b"old incomplete suite").unwrap();
    }

    let output = dir.path().join("suite.wav");
    bin()
        .arg("build")
        .arg(&scene)
        .arg("--soundfont")
        .arg(sf2())
        .arg("-o")
        .arg(&output)
        .assert()
        .success();

    let (_, main) = read_frames(&output);
    let (_, explore) = read_frames(&dir.path().join("suite-explore.wav"));
    let (_, sting) = read_frames(&dir.path().join("suite-sting.wav"));
    assert_eq!(main.len(), explore.len() + sting.len());
    let meta: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("suite.meta.json")).unwrap()).unwrap();
    assert_eq!(meta["suite"], true);
    assert_dir_contains_exactly(
        dir.path(),
        &[
            "suite.yaml",
            "suite.wav",
            "suite-explore.wav",
            "suite-sting.wav",
            "suite.meta.json",
        ],
    );
}

#[test]
fn build_suite_to_ogg_emits_main_file_without_leftover_cuts() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("suite.yaml");
    fs::write(&scene, suite_yaml()).unwrap();
    let out = dir.path().join("suite.ogg");
    bin()
        .arg("build")
        .arg(&scene)
        .arg("--soundfont")
        .arg(sf2())
        .arg("-o")
        .arg(&out)
        .assert()
        .success();
    // The intermediate section/main `.cut.wav` files must not survive.
    assert_dir_contains_exactly(
        dir.path(),
        &[
            "suite.yaml",
            "suite.ogg",
            "suite-explore.ogg",
            "suite-sting.ogg",
            "suite.meta.json",
        ],
    );
    assert!(fs::metadata(&out).unwrap().len() > 10_000);
    let meta: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("suite.meta.json")).unwrap()).unwrap();
    assert_eq!(meta["audio"], "suite.ogg");
    let l_explore = exact_samples(2 * 4 * 480, 120, 44100);
    let l_sting = exact_samples(4 * 480, 140, 44100) + 4 * 44100;
    assert_eq!(meta["total_samples"], l_explore + l_sting);
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

// ---- sfizz renderer + renderer profiles (M5) ----

/// Happy path: sfizz renders each track solo and mixes them in-process;
/// stems must sum back to the full mix, same invariant as the SF2 backends.
#[test]
fn build_sfizz_happy_path_produces_stems_and_sums_to_mix() {
    let dir = tempfile::tempdir().unwrap();
    let scene = tiny_sfizz_scene(dir.path());
    let profile = write_test_profile(dir.path());
    let wav = dir.path().join("duo.wav");
    bin()
        .arg("build")
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("--profile")
        .arg(&profile)
        .arg("-o")
        .arg(&wav)
        .arg("--stems")
        .env("PATH", sfizz_path_env())
        .assert()
        .success();
    let stems_dir = dir.path().join("duo.stems");
    assert_dir_contains_exactly(&stems_dir, &["01-violin.wav", "02-cello.wav"]);
    let (spec, mix) = read_frames(&wav);
    assert_eq!(spec.sample_rate, 44100);
    assert!(mix.iter().any(|&s| s.abs() > 50), "mix is silent");
    let ch = spec.channels as usize;
    let stems: Vec<Vec<i16>> = ["01-violin.wav", "02-cello.wav"]
        .iter()
        .map(|n| {
            let (s, data) = read_frames(&stems_dir.join(n));
            assert_eq!(s.channels, spec.channels);
            assert_eq!(data.len(), mix.len(), "stem {n} must match mix length");
            data
        })
        .collect();
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
        "sfizz stems do not sum to mix: RMS ratio {ratio:.4}"
    );
    let _ = ch;
}

#[test]
fn build_sfizz_missing_profile_is_input_error() {
    let dir = tempfile::tempdir().unwrap();
    let scene = tiny_sfizz_scene(dir.path());
    bin()
        .arg("build")
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("-o")
        .arg(dir.path().join("duo.wav"))
        .env("PATH", sfizz_path_env())
        .assert()
        .code(2);
    assert_dir_contains_exactly(dir.path(), &["duo.yaml"]);
}

#[test]
fn build_sfizz_rejects_soundfont_flag() {
    let dir = tempfile::tempdir().unwrap();
    let scene = tiny_sfizz_scene(dir.path());
    let profile = write_test_profile(dir.path());
    bin()
        .arg("build")
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("--profile")
        .arg(&profile)
        .arg("--soundfont")
        .arg(sf2())
        .arg("-o")
        .arg(dir.path().join("duo.wav"))
        .env("PATH", sfizz_path_env())
        .assert()
        .code(2);
}

#[test]
fn build_sfizz_unmapped_instrument_leaves_no_partial_output() {
    let dir = tempfile::tempdir().unwrap();
    let scene = tiny_sfizz_scene(dir.path());
    // Profile only maps `violin`; the scene's `cello` track has no mapping.
    write_sine_sfz(dir.path());
    let profile = dir.path().join("profile.yaml");
    fs::write(
        &profile,
        "name: test-profile\ninstruments:\n  violin:\n    sustain: mini.sfz\n",
    )
    .unwrap();
    bin()
        .arg("build")
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("--profile")
        .arg(&profile)
        .arg("-o")
        .arg(dir.path().join("duo.wav"))
        .env("PATH", sfizz_path_env())
        .assert()
        .code(2);
    assert_dir_contains_exactly(
        dir.path(),
        &["duo.yaml", "mini.sfz", "sine.wav", "profile.yaml"],
    );
}

#[test]
fn build_sfizz_missing_binary_is_dependency_error() {
    let dir = tempfile::tempdir().unwrap();
    let scene = tiny_sfizz_scene(dir.path());
    let profile = write_test_profile(dir.path());
    bin()
        .arg("build")
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("--profile")
        .arg(&profile)
        .arg("-o")
        .arg(dir.path().join("duo.wav"))
        .env("PATH", "")
        .assert()
        .code(3);
    assert_dir_contains_exactly(
        dir.path(),
        &["duo.yaml", "mini.sfz", "sine.wav", "profile.yaml"],
    );
}

/// Malformed `.sfz` content: `sfizz_render` exits non-zero; must not leave a
/// partial WAV or intermediate staging directory behind.
#[test]
fn build_sfizz_corrupt_sfz_fails_without_partial_output() {
    let dir = tempfile::tempdir().unwrap();
    let scene = tiny_sfizz_scene(dir.path());
    fs::write(dir.path().join("mini.sfz"), "<region sample=").unwrap();
    let profile = dir.path().join("profile.yaml");
    fs::write(
        &profile,
        "name: test-profile\ninstruments:\n  violin:\n    sustain: mini.sfz\n  cello:\n    sustain: mini.sfz\n",
    )
    .unwrap();
    bin()
        .arg("build")
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("--profile")
        .arg(&profile)
        .arg("-o")
        .arg(dir.path().join("duo.wav"))
        .env("PATH", sfizz_path_env())
        .assert()
        .code(4);
    assert_dir_contains_exactly(dir.path(), &["duo.yaml", "mini.sfz", "profile.yaml"]);
}

/// Low-level single-instrument path: `render --renderer sfizz --sfz ...`,
/// distinct from the profile-driven multi-instrument `build` path.
#[test]
fn render_sfizz_happy_path_produces_exact_rate_wav() {
    let dir = tempfile::tempdir().unwrap();
    let mid = make_tiny_midi(dir.path());
    let sfz = write_sine_sfz(dir.path());
    let wav = dir.path().join("scene.wav");
    bin()
        .arg("render")
        .arg(&mid)
        .args(["--renderer", "sfizz"])
        .arg("--sfz")
        .arg(&sfz)
        .arg("-o")
        .arg(&wav)
        .env("PATH", sfizz_path_env())
        .assert()
        .success();
    let (spec, out) = read_frames(&wav);
    assert_eq!(spec.sample_rate, 44100);
    assert!(out.iter().any(|&s| s.abs() > 50), "sfizz render is silent");
}

#[test]
fn render_sfizz_requires_sfz_not_soundfont() {
    let dir = tempfile::tempdir().unwrap();
    let mid = make_midi(dir.path());
    bin()
        .arg("render")
        .arg(&mid)
        .args(["--renderer", "sfizz"])
        .arg("--soundfont")
        .arg(sf2())
        .arg("-o")
        .arg(dir.path().join("scene.wav"))
        .env("PATH", sfizz_path_env())
        .assert()
        .code(2);
}

// ---- renderer profile health check ----

#[test]
fn profile_check_renders_unique_patches_and_reports_json() {
    let dir = tempfile::tempdir().unwrap();
    let profile = write_test_profile(dir.path());
    let out = bin()
        .args(["--json", "profile", "check"])
        .arg(&profile)
        .env("PATH", sfizz_path_env())
        .env("TMPDIR", dir.path())
        .assert()
        .success();
    let v: serde_json::Value =
        serde_json::from_slice(&out.get_output().stdout).expect("stdout is one JSON report");
    assert_eq!(v["profile"], "test-profile");
    assert_eq!(v["mappings"], 2);
    assert_eq!(v["unique_patches"], 1);
    assert_eq!(v["passed"], 1);
    assert_eq!(v["failed"], 0);
    let patches = v["patches"].as_array().unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0]["status"], "ok");
    assert_eq!(patches[0]["deterministic"], true);
    assert!(patches[0]["peak_abs"].as_u64().unwrap() > 50);
    assert_dir_contains_exactly(dir.path(), &["mini.sfz", "profile.yaml", "sine.wav"]);
}

#[test]
fn profile_check_missing_patch_is_structured_and_leaves_no_temp_files() {
    let dir = tempfile::tempdir().unwrap();
    let profile = dir.path().join("profile.yaml");
    fs::write(
        &profile,
        "name: missing-patch\ninstruments:\n  violin:\n    sustain: absent.sfz\n",
    )
    .unwrap();
    let out = bin()
        .args(["--json", "profile", "check"])
        .arg(&profile)
        .env("PATH", sfizz_path_env())
        .env("TMPDIR", dir.path())
        .assert()
        .code(2);
    let v: serde_json::Value =
        serde_json::from_slice(&out.get_output().stderr).expect("stderr is one JSON object");
    assert_eq!(v["code"], "profile_check");
    assert_eq!(v["report"]["failed"], 1);
    assert_eq!(v["report"]["patches"][0]["status"], "missing");
    assert_dir_contains_exactly(dir.path(), &["profile.yaml"]);
}

#[test]
fn profile_check_rejects_silent_patch_and_leaves_no_temp_files() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("silence.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&wav, spec).unwrap();
    for _ in 0..4410 {
        writer.write_sample(0i16).unwrap();
    }
    writer.finalize().unwrap();
    fs::write(
        dir.path().join("silent.sfz"),
        "<region>\nsample=silence.wav\nlokey=0\nhikey=127\n",
    )
    .unwrap();
    let profile = dir.path().join("profile.yaml");
    fs::write(
        &profile,
        "name: silent-patch\ninstruments:\n  violin:\n    sustain: silent.sfz\n",
    )
    .unwrap();
    let out = bin()
        .args(["--json", "profile", "check"])
        .arg(&profile)
        .env("PATH", sfizz_path_env())
        .env("TMPDIR", dir.path())
        .assert()
        .code(2);
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    assert_eq!(v["report"]["patches"][0]["status"], "silent");
    assert_dir_contains_exactly(dir.path(), &["profile.yaml", "silence.wav", "silent.sfz"]);
}

#[test]
fn profile_check_missing_sfizz_is_dependency_error_without_residue() {
    let dir = tempfile::tempdir().unwrap();
    let profile = write_test_profile(dir.path());
    bin()
        .args(["profile", "check"])
        .arg(&profile)
        .env("PATH", "")
        .env("TMPDIR", dir.path())
        .assert()
        .code(3);
    assert_dir_contains_exactly(dir.path(), &["mini.sfz", "profile.yaml", "sine.wav"]);
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

// ---- performance & harmony (M5) ----

fn perf_yaml(seed: u64) -> String {
    format!(
        "tempo: 92\nkey: D_minor\nbars: 2\nloop: true\nharmony: [i, iv, VI, v]\nperformance:\n  humanize: {{ timing_ms: 12, velocity: 8, seed: {seed} }}\n  swing: 0.12\n  legato: true\n  dynamics: {{ start: p, peak: f }}\ntracks:\n  - {{ instrument: piano, pattern: arpeggio, intensity: 0.6 }}\n  - {{ instrument: bass, pattern: bass, intensity: 0.5 }}\n  - {{ instrument: drums, pattern: drums, intensity: 0.5 }}\n"
    )
}

#[test]
fn performance_same_seed_is_byte_identical_different_seed_differs() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("perf.yaml");
    fs::write(&scene, perf_yaml(42)).unwrap();
    let (a, b, c) = (
        dir.path().join("a.mid"),
        dir.path().join("b.mid"),
        dir.path().join("c.mid"),
    );
    for out in [&a, &b] {
        bin()
            .arg("midi")
            .arg(&scene)
            .arg("-o")
            .arg(out)
            .assert()
            .success();
    }
    assert_eq!(
        fs::read(&a).unwrap(),
        fs::read(&b).unwrap(),
        "same seed must reproduce the performance bit-exactly"
    );
    fs::write(&scene, perf_yaml(43)).unwrap();
    bin()
        .arg("midi")
        .arg(&scene)
        .arg("-o")
        .arg(&c)
        .assert()
        .success();
    assert_ne!(
        fs::read(&a).unwrap(),
        fs::read(&c).unwrap(),
        "a different seed must change the performance"
    );
}

#[test]
fn performance_build_keeps_loop_sample_exact() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("perf.yaml");
    fs::write(&scene, perf_yaml(42)).unwrap();
    let wav = dir.path().join("perf.wav");
    bin()
        .arg("build")
        .arg(&scene)
        .arg("--soundfont")
        .arg(sf2())
        .arg("-o")
        .arg(&wav)
        .assert()
        .success();
    let want = exact_samples(2 * 4 * 480, 92, 44100);
    let (spec, frames) = read_frames(&wav);
    assert_eq!(
        frames.len() as u64 / u64::from(spec.channels),
        want,
        "humanize/swing must not disturb the sample-exact loop length"
    );
}

#[test]
fn harmony_changes_notes_at_same_length() {
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("plain.yaml");
    let harm = dir.path().join("harm.yaml");
    let base =
        "tempo: 92\nkey: D_minor\nbars: 4\ntracks:\n  - instrument: piano\n    pattern: arpeggio\n";
    fs::write(&plain, base).unwrap();
    fs::write(&harm, format!("harmony: [i, iv, VI, v]\n{base}")).unwrap();
    let (m0, m1) = (dir.path().join("p.mid"), dir.path().join("h.mid"));
    for (scene, out) in [(&plain, &m0), (&harm, &m1)] {
        bin()
            .arg("midi")
            .arg(scene)
            .arg("-o")
            .arg(out)
            .assert()
            .success();
    }
    assert_ne!(
        fs::read(&m0).unwrap(),
        fs::read(&m1).unwrap(),
        "a custom progression must change the notes"
    );
}

#[test]
fn validate_rejects_bad_swing_and_bad_numeral() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("bad.yaml");
    fs::write(
        &scene,
        "tempo: 92\nbars: 2\nperformance:\n  swing: 0.9\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    let out = bin()
        .args(["--json", "validate"])
        .arg(&scene)
        .assert()
        .code(2);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    assert!(stderr.contains("performance.swing"), "stderr: {stderr}");
    fs::write(
        &scene,
        "tempo: 92\nbars: 2\nharmony: [viii]\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    let out = bin()
        .args(["--json", "validate"])
        .arg(&scene)
        .assert()
        .code(2);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    assert!(stderr.contains("harmony[0]"), "stderr: {stderr}");
}

// ---- spatial performance fields (M10) ----

fn spatial_yaml(with_spatial: bool) -> String {
    let spatial = if with_spatial {
        "    glide: 0.4\n    pan: 0.25\n    reverb: 0.8\n"
    } else {
        ""
    };
    format!(
        "tempo: 92\nkey: D_minor\nbars: 2\nloop: true\nmotifs:\n  line:\n    - {{ degree: 1, beats: 1 }}\n    - {{ degree: 2, beats: 1 }}\n    - {{ degree: 3, beats: 1 }}\n    - {{ degree: 2, beats: 1 }}\ntracks:\n  - instrument: violin\n    pattern: melody\n    motif: line\n{spatial}  - instrument: cello\n    pattern: sustain\n    intensity: 0.5\n"
    )
}

/// Collect (controller, value) and pitch-bend values across all MIDI tracks.
fn midi_controls(bytes: &[u8]) -> (Vec<(u8, u8)>, Vec<u16>) {
    let smf = midly::Smf::parse(bytes).expect("produced MIDI parses");
    let mut ccs = Vec::new();
    let mut bends = Vec::new();
    for track in &smf.tracks {
        for event in track {
            if let midly::TrackEventKind::Midi { message, .. } = event.kind {
                match message {
                    midly::MidiMessage::Controller { controller, value } => {
                        ccs.push((controller.as_int(), value.as_int()));
                    }
                    midly::MidiMessage::PitchBend { bend } => {
                        bends.push(bend.0.as_int());
                    }
                    _ => {}
                }
            }
        }
    }
    (ccs, bends)
}

#[test]
fn spatial_fields_emit_cc_and_pitch_bend_deterministically() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("spatial.yaml");
    fs::write(&scene, spatial_yaml(true)).unwrap();
    let (a, b) = (dir.path().join("a.mid"), dir.path().join("b.mid"));
    for out in [&a, &b] {
        bin()
            .arg("midi")
            .arg(&scene)
            .arg("-o")
            .arg(out)
            .assert()
            .success();
    }
    let bytes = fs::read(&a).unwrap();
    assert_eq!(
        bytes,
        fs::read(&b).unwrap(),
        "spatial fields must compile deterministically"
    );

    let (ccs, bends) = midi_controls(&bytes);
    assert!(
        ccs.contains(&(10, 32)),
        "pan 0.25 must emit CC10 = 32, got {ccs:?}"
    );
    assert!(
        ccs.contains(&(91, 102)),
        "reverb 0.8 must emit CC91 = 102, got {ccs:?}"
    );
    assert!(
        bends.iter().any(|&v| v != 8192),
        "glide must emit off-center pitch bends"
    );
    assert!(
        bends.contains(&8192),
        "every glide must reset the bend to center at the next onset"
    );

    let plain = dir.path().join("plain.yaml");
    fs::write(&plain, spatial_yaml(false)).unwrap();
    let p = dir.path().join("p.mid");
    bin()
        .arg("midi")
        .arg(&plain)
        .arg("-o")
        .arg(&p)
        .assert()
        .success();
    let plain_bytes = fs::read(&p).unwrap();
    assert_ne!(bytes, plain_bytes, "spatial fields must change the MIDI");
    let (plain_ccs, plain_bends) = midi_controls(&plain_bytes);
    assert!(
        plain_ccs.is_empty() && plain_bends.is_empty(),
        "a scene without spatial fields must emit no controllers or bends"
    );
}

#[test]
fn spatial_build_keeps_loop_sample_exact() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("spatial.yaml");
    fs::write(&scene, spatial_yaml(true)).unwrap();
    let wav = dir.path().join("spatial.wav");
    bin()
        .arg("build")
        .arg(&scene)
        .arg("--soundfont")
        .arg(sf2())
        .arg("-o")
        .arg(&wav)
        .assert()
        .success();
    let want = exact_samples(2 * 4 * 480, 92, 44100);
    let (spec, frames) = read_frames(&wav);
    assert_eq!(
        frames.len() as u64 / u64::from(spec.channels),
        want,
        "pan/reverb/glide must not disturb the sample-exact loop length"
    );
}

#[test]
fn validate_rejects_bad_pan_and_glide_on_non_melody() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("bad.yaml");
    fs::write(
        &scene,
        "tempo: 92\nbars: 2\ntracks:\n  - instrument: piano\n    pattern: sustain\n    pan: 1.5\n",
    )
    .unwrap();
    let out = bin()
        .args(["--json", "validate"])
        .arg(&scene)
        .assert()
        .code(2);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    assert!(stderr.contains("tracks[0].pan"), "stderr: {stderr}");

    fs::write(
        &scene,
        "tempo: 92\nbars: 2\ntracks:\n  - instrument: piano\n    pattern: sustain\n    glide: 0.3\n",
    )
    .unwrap();
    let out = bin()
        .args(["--json", "validate"])
        .arg(&scene)
        .assert()
        .code(2);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    assert!(
        stderr.contains("tracks[0].glide") && stderr.contains("melody"),
        "stderr: {stderr}"
    );
}

// ---- lint: aesthetic grammar (M6) ----

/// The shipped reference pair must always agree: dunes.yaml is the
/// living proof that the `grief` constitution is satisfiable.
#[test]
fn lint_shipped_scene_conforms_to_shipped_grammar() {
    bin()
        .arg("lint")
        .arg(repo("examples/scenes/dunes.yaml"))
        .arg("--grammar")
        .arg(repo("examples/grammars/grief.yaml"))
        .assert()
        .success()
        .stdout(predicates::str::contains("ok: conforms to `grief`"));
}

/// Violations carry the measured value so the agent can fix the scene:
/// rule name, subject, actual vs wanted — in text and in `--json`.
#[test]
fn lint_reports_violations_with_measured_values() {
    let out = bin()
        .arg("lint")
        .arg(forest())
        .arg("--grammar")
        .arg(repo("examples/grammars/grief.yaml"))
        .assert()
        .code(2);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    assert!(
        stderr.contains("tempo_max @ scene: measured 92, want <= 60"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("require_performance"), "stderr: {stderr}");
    assert!(
        stderr.contains("grammar violation(s) against `grief`"),
        "stderr: {stderr}"
    );

    let out = bin()
        .args(["--json", "lint"])
        .arg(forest())
        .arg("--grammar")
        .arg(repo("examples/grammars/grief.yaml"))
        .assert()
        .code(2);
    let v: serde_json::Value =
        serde_json::from_slice(&out.get_output().stderr).expect("stderr is one JSON object");
    assert_eq!(v["code"], "lint");
    let violations = v["violations"].as_array().unwrap();
    assert!(!violations.is_empty());
    assert!(
        violations
            .iter()
            .any(|x| x["rule"] == "tempo_max" && x["measured"] == "92")
    );
}

/// Deep rules measure the compiled IR, not the YAML surface: a melody
/// with zero rests must be caught by `melody_rest_ratio_min`.
#[test]
fn lint_measures_rest_ratio_from_compiled_ir() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("busy.yaml");
    fs::write(
        &scene,
        "tempo: 50\nbars: 2\nmotifs:\n  wall:\n    - { degree: 1, beats: 4 }\n    - { degree: 2, beats: 4 }\ntracks:\n  - instrument: violin\n    pattern: melody\n    motif: wall\n",
    )
    .unwrap();
    let grammar = dir.path().join("g.yaml");
    fs::write(
        &grammar,
        "name: sparse\nrules:\n  melody_rest_ratio_min: 0.35\n",
    )
    .unwrap();
    let out = bin()
        .arg("lint")
        .arg(&scene)
        .arg("--grammar")
        .arg(&grammar)
        .assert()
        .code(2);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    assert!(
        stderr.contains("melody_rest_ratio_min") && stderr.contains("want >= 0.35"),
        "stderr: {stderr}"
    );
}

/// A grammar that asserts nothing is a config bug, not a lint pass.
#[test]
fn lint_rejects_grammar_without_rules() {
    let dir = tempfile::tempdir().unwrap();
    let grammar = dir.path().join("empty.yaml");
    fs::write(&grammar, "name: hollow\nrules: {}\n").unwrap();
    let out = bin()
        .arg("lint")
        .arg(forest())
        .arg("--grammar")
        .arg(&grammar)
        .assert()
        .code(2);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    assert!(stderr.contains("at least one rule"), "stderr: {stderr}");
}

/// `schema --grammar` documents the constitution format for agents.
#[test]
fn schema_grammar_flag_emits_grammar_schema() {
    let out = bin().args(["schema", "--grammar"]).assert().success();
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(v["title"], "Grammar");
    assert!(v["properties"]["rules"].is_object());
}

#[test]
fn schema_profile_flag_emits_renderer_profile_schema() {
    let out = bin().args(["schema", "--profile"]).assert().success();
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(v["title"], "Profile");
    assert!(v["properties"]["instruments"].is_object());
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

// ---- MCP stdio server (`scorekit mcp`) ----------------------------------

/// Run `scorekit mcp`, feed newline-delimited JSON-RPC requests on stdin,
/// and return the parsed response objects in order (stdin EOF ends the loop).
fn mcp_roundtrip(requests: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let stdin = requests
        .iter()
        .map(|r| format!("{r}\n"))
        .collect::<String>();
    let out = bin().arg("mcp").write_stdin(stdin).assert().success();
    String::from_utf8_lossy(&out.get_output().stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("each response line is JSON"))
        .collect()
}

#[test]
fn mcp_initialize_lists_tools_and_validates_scene() {
    let replies = mcp_roundtrip(&[
        serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2024-11-05", "capabilities": {},
                       "clientInfo": {"name": "test", "version": "0"}}}),
        serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "validate",
                       "arguments": {"scene": forest().to_str().unwrap()}}}),
        serde_json::json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": {"name": "schema", "arguments": {"kind": "grammar"}}}),
        serde_json::json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call",
            "params": {"name": "schema", "arguments": {"kind": "texture_profile"}}}),
    ]);
    // The notification gets no response: 5 replies for 6 messages.
    assert_eq!(replies.len(), 5, "replies: {replies:?}");

    let init = &replies[0]["result"];
    assert_eq!(init["serverInfo"]["name"], "scorekit");
    assert!(init["capabilities"]["tools"].is_object());

    let tools: Vec<&str> = replies[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for expected in ["doctor", "validate", "schema", "lint", "build", "diff"] {
        assert!(
            tools.contains(&expected),
            "missing tool {expected}: {tools:?}"
        );
    }

    let call = &replies[2]["result"];
    assert_eq!(call["isError"], false);
    let text = call["content"][0]["text"].as_str().unwrap();
    assert!(text.starts_with("ok:"), "validate text: {text}");

    let schema_text = replies[3]["result"]["content"][0]["text"].as_str().unwrap();
    let schema: serde_json::Value = serde_json::from_str(schema_text).unwrap();
    assert!(schema["$schema"].is_string(), "grammar schema: {schema}");

    let texture_schema_text = replies[4]["result"]["content"][0]["text"].as_str().unwrap();
    let texture_schema: serde_json::Value = serde_json::from_str(texture_schema_text).unwrap();
    assert!(
        texture_schema["properties"]["sources"].is_object(),
        "texture profile schema: {texture_schema}"
    );
}

#[test]
fn mcp_tool_failure_passes_structured_error_through() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("bad.yaml");
    fs::write(
        &scene,
        "tempo: 999\nbars: 4\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    let replies = mcp_roundtrip(&[
        serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": "validate",
                       "arguments": {"scene": scene.to_str().unwrap()}}}),
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "no_such_tool", "arguments": {}}}),
        serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "bogus/method"}),
    ]);
    assert_eq!(replies.len(), 3, "replies: {replies:?}");

    // A failing tool is an MCP-level success with isError=true, and the text
    // is the CLI's structured `--json` error object, passed through verbatim.
    let call = &replies[0]["result"];
    assert_eq!(call["isError"], true);
    let payload: serde_json::Value =
        serde_json::from_str(call["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["code"], "validation");
    assert_eq!(payload["exit_code"], 2);
    assert_eq!(payload["field"], "tempo");

    // Unknown tool and unknown method are JSON-RPC protocol errors.
    assert_eq!(replies[1]["error"]["code"], -32602);
    assert_eq!(replies[2]["error"]["code"], -32601);
}
