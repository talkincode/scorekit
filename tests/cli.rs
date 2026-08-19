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
fn write_tone_sfz_files(dir: &Path, sfz_stem: &str, wav_stem: &str, frequency: f64) -> PathBuf {
    let wav_path = dir.join(format!("{wav_stem}.wav"));
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&wav_path, spec).unwrap();
    for i in 0..4410u32 {
        let t = f64::from(i) / 44100.0;
        let v = (3000.0 * (2.0 * std::f64::consts::PI * frequency * t).sin()) as i16;
        writer.write_sample(v).unwrap();
    }
    writer.finalize().unwrap();

    let sfz_path = dir.join(format!("{sfz_stem}.sfz"));
    fs::write(
        &sfz_path,
        format!("<region>\nsample={wav_stem}.wav\nlokey=0\nhikey=127\n"),
    )
    .unwrap();
    sfz_path
}

fn write_tone_sfz(dir: &Path, stem: &str, frequency: f64) -> PathBuf {
    write_tone_sfz_files(dir, stem, stem, frequency)
}

fn write_sine_sfz(dir: &Path) -> PathBuf {
    write_tone_sfz_files(dir, "mini", "sine", 440.0)
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

fn write_orchestration_for_profile(dir: &Path, profile: &Path) -> PathBuf {
    let profile = profile
        .file_name()
        .expect("test profile has a file name")
        .to_string_lossy();
    let orchestration = dir.join("orchestration.yaml");
    fs::write(
        &orchestration,
        format!(
            "schema_version: 1\nname: test-orchestration\ndefault_palette: default\npalettes:\n  default: {{ profile: {profile} }}\n"
        ),
    )
    .unwrap();
    orchestration
}

fn write_test_orchestration(dir: &Path) -> PathBuf {
    let profile = write_test_profile(dir);
    let orchestration = write_orchestration_for_profile(dir, &profile);
    fs::write(
        &orchestration,
        "schema_version: 1\nname: hybrid-test\ndefault_palette: default\npalettes:\n  default: { profile: profile.yaml }\n  solo: { profile: profile.yaml }\n",
    )
    .unwrap();
    orchestration
}

fn tiny_sfizz_scene(dir: &Path) -> PathBuf {
    let scene = dir.join("duo.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 2\nloop: false\ntracks:\n  - id: violin\n    instrument: violin\n    pattern: sustain\n  - id: cello\n    instrument: cello\n    pattern: sustain\n",
    )
    .unwrap();
    scene
}

fn world_sfizz_scene(dir: &Path) -> PathBuf {
    let scene = dir.join("world.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 1\nloop: false\ntracks:\n  - id: erhu\n    instrument: erhu\n    pattern: sustain\n  - id: tabla\n    instrument: tabla\n    pattern: tabla\n",
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
        .arg("violin")
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
fn clip_scene_validates_and_schema_exposes_stable_clip_maps() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("clip.yaml");
    fs::write(
        &scene,
        "tempo: 140\nkey: F_minor\nbars: 2\nclips:\n  bass_drop:\n    kind: pitched\n    length_beats: 4\n    mode: loop\n    events:\n      hit_a: { at: 0, duration: 0.5, pitch: F1, velocity: 127 }\n      hit_b: { at: 1.5, duration: 0.25, pitch: C2, velocity: 118 }\n  drop_drums:\n    kind: percussion\n    length_beats: 4\n    mode: loop\n    events:\n      kick_1: { at: 0, voice: kick, velocity: 127 }\n      snare_1: { at: 2, voice: snare, velocity: 127 }\ntracks:\n  - { id: bass, instrument: synth_bass, pattern: clip, clip: bass_drop, intensity: 1 }\n  - { id: drums, instrument: drums, pattern: clip, clip: drop_drums, intensity: 1 }\n",
    )
    .unwrap();

    bin().arg("validate").arg(&scene).assert().success();

    let out = bin().arg("schema").assert().success();
    let schema: serde_json::Value =
        serde_json::from_slice(&out.get_output().stdout).expect("scene schema is JSON");
    assert!(schema["properties"]["clips"].is_object());
    assert!(schema["$defs"]["Clip"]["properties"]["events"].is_object());
    assert!(
        schema["$defs"]["Track"]["properties"]["clip"].is_object(),
        "track schema exposes clip references: {schema}"
    );
}

#[test]
fn clip_events_compile_to_exact_deterministic_midi_independent_of_map_order() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.yaml");
    let b = dir.path().join("b.yaml");
    let prefix = "tempo: 140\nkey: F_minor\nbars: 2\n";
    let tracks = "tracks:\n  - { id: bass, instrument: synth_bass, pattern: clip, clip: bass_drop, intensity: 1 }\n  - { id: drums, instrument: drums, pattern: clip, clip: drop_drums, intensity: 1 }\n";
    fs::write(
        &a,
        format!(
            "{prefix}clips:\n  bass_drop:\n    kind: pitched\n    length_beats: 4\n    mode: loop\n    events:\n      hit_a: {{ at: 0, duration: 0.5, pitch: F1, velocity: 127 }}\n      hit_b: {{ at: 1.5, duration: 0.25, pitch: C2, velocity: 118 }}\n  drop_drums:\n    kind: percussion\n    length_beats: 4\n    mode: loop\n    events:\n      kick_1: {{ at: 0, voice: kick, velocity: 127 }}\n      snare_1: {{ at: 2, voice: snare, velocity: 127 }}\n{tracks}"
        ),
    )
    .unwrap();
    fs::write(
        &b,
        format!(
            "{prefix}clips:\n  drop_drums:\n    kind: percussion\n    length_beats: 4\n    mode: loop\n    events:\n      snare_1: {{ at: 2, voice: snare, velocity: 127 }}\n      kick_1: {{ at: 0, voice: kick, velocity: 127 }}\n  bass_drop:\n    kind: pitched\n    length_beats: 4\n    mode: loop\n    events:\n      hit_b: {{ at: 1.5, duration: 0.25, pitch: C2, velocity: 118 }}\n      hit_a: {{ at: 0, duration: 0.5, pitch: F1, velocity: 127 }}\n{tracks}"
        ),
    )
    .unwrap();
    let midi_a = dir.path().join("a.mid");
    let midi_b = dir.path().join("b.mid");
    for (scene, output) in [(&a, &midi_a), (&b, &midi_b)] {
        bin()
            .arg("midi")
            .arg(scene)
            .arg("-o")
            .arg(output)
            .assert()
            .success();
    }
    let bytes = fs::read(&midi_a).unwrap();
    assert_eq!(bytes, fs::read(&midi_b).unwrap());

    let smf = midly::Smf::parse(&bytes).expect("clip MIDI parses");
    let mut note_ons = Vec::new();
    for track in &smf.tracks {
        let mut tick = 0u32;
        for event in track {
            tick += event.delta.as_int();
            if let midly::TrackEventKind::Midi {
                channel,
                message: midly::MidiMessage::NoteOn { key, vel },
            } = event.kind
                && vel.as_int() > 0
            {
                note_ons.push((tick, channel.as_int(), key.as_int(), vel.as_int()));
            }
        }
    }
    assert_eq!(
        note_ons,
        vec![
            (0, 0, 29, 127),
            (720, 0, 36, 118),
            (1920, 0, 29, 127),
            (2640, 0, 36, 118),
            (0, 9, 36, 127),
            (960, 9, 38, 127),
            (1920, 9, 36, 127),
            (2880, 9, 38, 127),
        ]
    );
}

#[test]
fn explicit_clip_timing_and_velocity_ignore_generative_performance_transforms() {
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("plain.yaml");
    let performed = dir.path().join("performed.yaml");
    let body = "tempo: 140\nkey: F_minor\nbars: 1\nclips:\n  exact:\n    kind: pitched\n    length_beats: 4\n    mode: loop\n    events:\n      offbeat: { at: 0.5, duration: 0.25, pitch: F1, velocity: 101 }\ntracks:\n  - { id: bass, instrument: synth_bass, pattern: clip, clip: exact, intensity: 1 }\n";
    fs::write(&plain, body).unwrap();
    fs::write(
        &performed,
        format!(
            "performance:\n  swing: 0.5\n  legato: true\n  humanize: {{ timing_ms: 50, velocity: 30, seed: 77 }}\n{body}"
        ),
    )
    .unwrap();
    let a = dir.path().join("a.mid");
    let b = dir.path().join("b.mid");
    for (scene, output) in [(&plain, &a), (&performed, &b)] {
        bin()
            .arg("midi")
            .arg(scene)
            .arg("-o")
            .arg(output)
            .assert()
            .success();
    }
    assert_eq!(
        fs::read(a).unwrap(),
        fs::read(b).unwrap(),
        "authored clip events must not be shifted, stretched, or randomized"
    );
}

#[test]
fn section_clip_override_compiles_as_the_equivalent_standalone_scene() {
    let dir = tempfile::tempdir().unwrap();
    let clips = "clips:\n  build_bass:\n    kind: pitched\n    length_beats: 4\n    mode: loop\n    events:\n      hit: { at: 0, duration: 1, pitch: F1, velocity: 100 }\n  drop_bass:\n    kind: pitched\n    length_beats: 4\n    mode: loop\n    events:\n      hit: { at: 0.75, duration: 0.5, pitch: C2, velocity: 127 }\n";
    let suite = dir.path().join("suite.yaml");
    fs::write(
        &suite,
        format!(
            "tempo: 140\nkey: F_minor\nbars: 1\n{clips}tracks:\n  - {{ id: bass, instrument: synth_bass, pattern: clip, clip: build_bass, intensity: 1 }}\nsections:\n  - {{ name: build, bars: 1 }}\n  - name: drop\n    bars: 1\n    clips: {{ bass: drop_bass }}\n"
        ),
    )
    .unwrap();
    let standalone = dir.path().join("standalone.yaml");
    fs::write(
        &standalone,
        format!(
            "tempo: 140\nkey: F_minor\nbars: 1\n{clips}tracks:\n  - {{ id: bass, instrument: synth_bass, pattern: clip, clip: drop_bass, intensity: 1 }}\n"
        ),
    )
    .unwrap();
    let selected = dir.path().join("selected.mid");
    let expected = dir.path().join("expected.mid");
    bin()
        .arg("midi")
        .arg(&suite)
        .arg("-o")
        .arg(&selected)
        .args(["--section", "drop"])
        .assert()
        .success();
    bin()
        .arg("midi")
        .arg(&standalone)
        .arg("-o")
        .arg(&expected)
        .assert()
        .success();
    assert_eq!(fs::read(selected).unwrap(), fs::read(expected).unwrap());
}

#[test]
fn invalid_clip_semantics_fail_with_precise_paths_and_no_midi_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let cases = [
        (
            "overlap",
            "clips.bass.events.second.at",
            "clips:\n  bass:\n    kind: pitched\n    length_beats: 4\n    mode: loop\n    events:\n      first: { at: 0, duration: 1, pitch: F1, velocity: 100 }\n      second: { at: 0.5, duration: 1, pitch: F1, velocity: 100 }\ntracks:\n  - { id: bass, instrument: synth_bass, pattern: clip, clip: bass }\n",
        ),
        (
            "kind",
            "tracks[0].clip",
            "clips:\n  bass:\n    kind: pitched\n    length_beats: 4\n    mode: loop\n    events:\n      first: { at: 0, duration: 1, pitch: F1, velocity: 100 }\ntracks:\n  - { id: drums, instrument: drums, pattern: clip, clip: bass }\n",
        ),
        (
            "division",
            "tracks[0].clip",
            "clips:\n  bass:\n    kind: pitched\n    length_beats: 3\n    mode: loop\n    events:\n      first: { at: 0, duration: 1, pitch: F1, velocity: 100 }\ntracks:\n  - { id: bass, instrument: synth_bass, pattern: clip, clip: bass }\n",
        ),
        (
            "extreme-pitch",
            "clips.bass.events.first.pitch",
            "clips:\n  bass:\n    kind: pitched\n    length_beats: 4\n    mode: loop\n    events:\n      first: { at: 0, duration: 1, pitch: C32767, velocity: 100 }\ntracks:\n  - { id: bass, instrument: synth_bass, pattern: clip, clip: bass }\n",
        ),
    ];
    for (name, field, body) in cases {
        let scene = dir.path().join(format!("{name}.yaml"));
        let output = dir.path().join(format!("{name}.mid"));
        fs::write(&scene, format!("tempo: 140\nbars: 1\n{body}")).unwrap();
        let out = bin()
            .args(["--json", "midi"])
            .arg(&scene)
            .arg("-o")
            .arg(&output)
            .assert()
            .code(2);
        let error: serde_json::Value =
            serde_json::from_slice(&out.get_output().stderr).expect("validation error is JSON");
        assert_eq!(error["field"], field, "error: {error}");
        assert!(!output.exists(), "{output:?} must not be published");
    }
}

#[test]
fn validate_rejects_clip_expansion_beyond_the_per_track_event_budget() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("explosive.yaml");
    fs::write(
        &scene,
        "tempo: 140\ntime_signature: 12/2\nbars: 256\nclips:\n  explosive:\n    kind: pitched\n    length_beats: 0.0020833333333333333\n    mode: loop\n    events:\n      low: { at: 0, duration: 0.0020833333333333333, pitch: C-1, velocity: 100 }\n      high: { at: 0, duration: 0.0020833333333333333, pitch: C#-1, velocity: 100 }\ntracks:\n  - { id: lead, instrument: square_lead, pattern: clip, clip: explosive }\n",
    )
    .unwrap();

    let out = bin()
        .args(["--json", "validate"])
        .arg(&scene)
        .assert()
        .code(2);
    let error: serde_json::Value =
        serde_json::from_slice(&out.get_output().stderr).expect("validation error is JSON");
    assert_eq!(error["field"], "tracks[0].clip");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("expanded event limit of 65536"),
        "error: {error}"
    );
}

#[test]
fn validate_applies_clip_expansion_budget_to_suite_sections() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("suite-explosive.yaml");
    fs::write(
        &scene,
        "tempo: 140\ntime_signature: 12/2\nbars: 1\nclips:\n  explosive:\n    kind: pitched\n    length_beats: 0.0020833333333333333\n    mode: loop\n    events:\n      low: { at: 0, duration: 0.0020833333333333333, pitch: C-1, velocity: 100 }\n      high: { at: 0, duration: 0.0020833333333333333, pitch: C#-1, velocity: 100 }\ntracks:\n  - { id: lead, instrument: square_lead, pattern: clip, clip: explosive }\nsections:\n  - { name: long, bars: 256 }\n",
    )
    .unwrap();

    let out = bin()
        .args(["--json", "validate"])
        .arg(&scene)
        .assert()
        .code(2);
    let error: serde_json::Value =
        serde_json::from_slice(&out.get_output().stderr).expect("validation error is JSON");
    assert_eq!(error["field"], "sections[0].clips.lead");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("expanded event limit of 65536"),
        "error: {error}"
    );
}

#[test]
fn validate_counts_generated_linear_samples_against_the_event_budget() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("linear-explosive.yaml");
    fs::write(
        &scene,
        "tempo: 120\ntime_signature: 12/2\nbars: 256\nclips:\n  sweep:\n    kind: pitched\n    length_beats: 24\n    mode: loop\n    events:\n      hit: { at: 0, duration: 1, pitch: C2, velocity: 100 }\n    automation:\n      brightness:\n        target: cc74\n        interpolation: linear\n        points:\n          start: { at: 0, value: 0 }\n          peak: { at: 12, value: 127 }\n          seal: { at: 23.875, value: 0 }\n      expression:\n        target: cc11\n        interpolation: linear\n        points:\n          start: { at: 0, value: 0 }\n          peak: { at: 12, value: 127 }\n          seal: { at: 23.875, value: 0 }\ntracks:\n  - { id: stab, instrument: synth_brass, pattern: clip, clip: sweep }\n",
    )
    .unwrap();

    let out = bin()
        .args(["--json", "validate"])
        .arg(&scene)
        .assert()
        .code(2);
    let error: serde_json::Value =
        serde_json::from_slice(&out.get_output().stderr).expect("validation error is JSON");
    assert_eq!(error["field"], "tracks[0].clip");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("expands to 98560"),
        "error: {error}"
    );
}

#[test]
fn multiple_percussion_tracks_share_channel_ten_deterministically() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("multi-percussion.yaml");
    fs::write(
        &scene,
        "tempo: 118\nbars: 2\ntracks:\n  - { id: kit, instrument: drums, pattern: drums }\n  - { id: auxiliary, instrument: drums, pattern: drums }\n",
    )
    .unwrap();

    bin().arg("validate").arg(&scene).assert().success();

    let first = dir.path().join("first.mid");
    let second = dir.path().join("second.mid");
    for output in [&first, &second] {
        bin()
            .arg("midi")
            .arg(&scene)
            .arg("-o")
            .arg(output)
            .assert()
            .success();
    }
    let bytes = fs::read(&first).unwrap();
    assert_eq!(bytes, fs::read(&second).unwrap());

    let smf = midly::Smf::parse(&bytes).expect("multi-percussion MIDI parses");
    assert_eq!(smf.tracks.len(), 3, "conductor plus two percussion tracks");
    for track in &smf.tracks[1..] {
        let mut note_ons = 0;
        for event in track {
            match event.kind {
                midly::TrackEventKind::Midi {
                    channel,
                    message: midly::MidiMessage::NoteOn { vel, .. },
                } if vel.as_int() > 0 => {
                    assert_eq!(channel.as_int(), 9);
                    note_ons += 1;
                }
                midly::TrackEventKind::Midi {
                    message: midly::MidiMessage::ProgramChange { .. },
                    ..
                } => panic!("percussion tracks must not emit program changes"),
                _ => {}
            }
        }
        assert!(note_ons > 0);
    }
}

#[test]
fn validate_rejects_conflicting_channel_controls_across_percussion_tracks() {
    let dir = tempfile::tempdir().unwrap();
    let cases = [
        (
            "pan",
            "  - { id: kit, instrument: drums, pattern: drums, pan: 0.25 }\n  - { id: auxiliary, instrument: drums, pattern: drums, pan: 0.75 }\n",
        ),
        (
            "reverb",
            "  - { id: kit, instrument: drums, pattern: drums, reverb: 0.1 }\n  - { id: auxiliary, instrument: drums, pattern: drums, reverb: 0.5 }\n",
        ),
    ];
    for (field, tracks) in cases {
        let scene = dir.path().join(format!("conflicting-{field}.yaml"));
        fs::write(&scene, format!("tempo: 118\nbars: 2\ntracks:\n{tracks}")).unwrap();
        let out = bin()
            .args(["--json", "validate"])
            .arg(&scene)
            .assert()
            .code(2);
        let error: serde_json::Value =
            serde_json::from_slice(&out.get_output().stderr).expect("validation error is JSON");
        assert_eq!(error["field"], format!("tracks[1].{field}"));
        assert!(
            error["message"]
                .as_str()
                .unwrap()
                .contains("share MIDI channel 10"),
            "error: {error}"
        );
    }
}

#[test]
fn disco_auxiliary_percussion_voices_are_public_and_emit_standard_gm_keys() {
    let schema_out = bin().arg("schema").assert().success();
    let schema: serde_json::Value =
        serde_json::from_slice(&schema_out.get_output().stdout).expect("schema is JSON");
    let voices = schema["$defs"]["PercussionVoice"]["enum"]
        .as_array()
        .expect("PercussionVoice enum");
    let expected = [
        ("tambourine", 54u8),
        ("cowbell", 56),
        ("high_bongo", 60),
        ("low_bongo", 61),
        ("mute_high_conga", 62),
        ("open_high_conga", 63),
        ("low_conga", 64),
        ("high_timbale", 65),
        ("low_timbale", 66),
        ("high_agogo", 67),
        ("low_agogo", 68),
        ("cabasa", 69),
        ("maracas", 70),
    ];
    for (name, _) in expected {
        assert!(
            voices.iter().any(|voice| voice == name),
            "schema is missing `{name}`"
        );
    }

    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("disco-percussion.yaml");
    fs::write(
        &scene,
        "tempo: 118\nbars: 1\nclips:\n  auxiliary:\n    kind: percussion\n    length_beats: 4\n    mode: once\n    events:\n      e01: { at: 0.00, duration: 0.125, voice: tambourine, velocity: 100 }\n      e02: { at: 0.25, duration: 0.125, voice: cowbell, velocity: 100 }\n      e03: { at: 0.50, duration: 0.125, voice: high_bongo, velocity: 100 }\n      e04: { at: 0.75, duration: 0.125, voice: low_bongo, velocity: 100 }\n      e05: { at: 1.00, duration: 0.125, voice: mute_high_conga, velocity: 100 }\n      e06: { at: 1.25, duration: 0.125, voice: open_high_conga, velocity: 100 }\n      e07: { at: 1.50, duration: 0.125, voice: low_conga, velocity: 100 }\n      e08: { at: 1.75, duration: 0.125, voice: high_timbale, velocity: 100 }\n      e09: { at: 2.00, duration: 0.125, voice: low_timbale, velocity: 100 }\n      e10: { at: 2.25, duration: 0.125, voice: high_agogo, velocity: 100 }\n      e11: { at: 2.50, duration: 0.125, voice: low_agogo, velocity: 100 }\n      e12: { at: 2.75, duration: 0.125, voice: cabasa, velocity: 100 }\n      e13: { at: 3.00, duration: 0.125, voice: maracas, velocity: 100 }\ntracks:\n  - { id: auxiliary, instrument: drums, pattern: clip, clip: auxiliary }\n",
    )
    .unwrap();
    bin().arg("validate").arg(&scene).assert().success();
    let midi = dir.path().join("disco-percussion.mid");
    bin()
        .arg("midi")
        .arg(&scene)
        .arg("-o")
        .arg(&midi)
        .assert()
        .success();

    let bytes = fs::read(midi).unwrap();
    let smf = midly::Smf::parse(&bytes).expect("auxiliary percussion MIDI parses");
    let keys: Vec<u8> = smf.tracks[1]
        .iter()
        .filter_map(|event| match event.kind {
            midly::TrackEventKind::Midi {
                channel,
                message: midly::MidiMessage::NoteOn { key, vel },
            } if vel.as_int() > 0 => {
                assert_eq!(channel.as_int(), 9);
                Some(key.as_int())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        keys,
        expected.iter().map(|(_, key)| *key).collect::<Vec<_>>()
    );
}

#[test]
fn disco_instruments_are_public_and_emit_exact_gm_programs() {
    let schema_out = bin().arg("schema").assert().success();
    let schema: serde_json::Value =
        serde_json::from_slice(&schema_out.get_output().stdout).expect("schema is JSON");
    let instruments = schema["$defs"]["Instrument"]["enum"]
        .as_array()
        .expect("Instrument enum");
    for name in ["clavinet", "synth_brass"] {
        assert!(
            instruments.iter().any(|instrument| instrument == name),
            "schema is missing `{name}`"
        );
    }

    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("disco-instruments.yaml");
    fs::write(
        &scene,
        "tempo: 118\nbars: 1\ntracks:\n  - { id: clav, instrument: clavinet, pattern: sustain }\n  - { id: brass, instrument: synth_brass, pattern: sustain }\n",
    )
    .unwrap();
    bin().arg("validate").arg(&scene).assert().success();
    let inspect = bin()
        .args(["--json", "inspect-instruments"])
        .arg(&scene)
        .args(["--fallback-mode", "strict"])
        .assert()
        .success();
    let report: serde_json::Value =
        serde_json::from_slice(&inspect.get_output().stdout).expect("inspect report is JSON");
    assert_eq!(report["summary"]["exact"], 2);
    assert_eq!(report["summary"]["missing"], 0);

    let midi = dir.path().join("disco-instruments.mid");
    bin()
        .arg("midi")
        .arg(&scene)
        .arg("-o")
        .arg(&midi)
        .assert()
        .success();

    let bytes = fs::read(midi).unwrap();
    let smf = midly::Smf::parse(&bytes).expect("Disco instrument MIDI parses");
    let programs: Vec<u8> = smf.tracks[1..]
        .iter()
        .map(|track| {
            track
                .iter()
                .find_map(|event| match event.kind {
                    midly::TrackEventKind::Midi {
                        message: midly::MidiMessage::ProgramChange { program },
                        ..
                    } => Some(program.as_int()),
                    _ => None,
                })
                .expect("melodic track has a program change")
        })
        .collect();
    assert_eq!(programs, [7, 62]);
}

#[test]
fn clip_step_automation_emits_canonical_deterministic_midi_events() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("automated.yaml");
    fs::write(
        &scene,
        "tempo: 140\nkey: F_minor\nbars: 1\nclips:\n  talking:\n    kind: pitched\n    length_beats: 4\n    mode: loop\n    events:\n      hit: { at: 0, duration: 1, pitch: F1, velocity: 127 }\n    automation:\n      expression:\n        target: cc11\n        points:\n          start: { at: 0, value: 64 }\n          peak: { at: 1, value: 127 }\n          seal: { at: 3.5, value: 64 }\n      mouth:\n        target: cc1\n        points:\n          start: { at: 0, value: 8 }\n          open: { at: 0.5, value: 100 }\n          seal: { at: 3.5, value: 8 }\n      brightness:\n        target: cc74\n        points:\n          start: { at: 0, value: 24 }\n          bright: { at: 0.5, value: 118 }\n          seal: { at: 3.5, value: 24 }\n      bend:\n        target: pitch_bend\n        points:\n          start: { at: 0, value: 0 }\n          up: { at: 0.5, value: 4096 }\n          seal: { at: 3.5, value: 0 }\ntracks:\n  - id: bass\n    instrument: synth_bass\n    pattern: clip\n    clip: talking\n    intensity: 1\n    pan: 0.5\n    reverb: 0.25\n",
    )
    .unwrap();
    let first = dir.path().join("first.mid");
    let second = dir.path().join("second.mid");
    for output in [&first, &second] {
        bin()
            .arg("midi")
            .arg(&scene)
            .arg("-o")
            .arg(output)
            .assert()
            .success();
    }
    let bytes = fs::read(&first).unwrap();
    assert_eq!(bytes, fs::read(second).unwrap());

    let smf = midly::Smf::parse(&bytes).expect("automation MIDI parses");
    let track = &smf.tracks[1];
    let mut tick = 0u32;
    let mut observed = Vec::new();
    for event in track {
        tick += event.delta.as_int();
        let label = match event.kind {
            midly::TrackEventKind::Midi {
                message: midly::MidiMessage::ProgramChange { .. },
                ..
            } => "program".to_owned(),
            midly::TrackEventKind::Midi {
                message: midly::MidiMessage::Controller { controller, value },
                ..
            } => format!("cc{}={}", controller.as_int(), value.as_int()),
            midly::TrackEventKind::Midi {
                message: midly::MidiMessage::PitchBend { bend },
                ..
            } => format!("bend={}", bend.0.as_int()),
            midly::TrackEventKind::Midi {
                message: midly::MidiMessage::NoteOn { key, vel },
                ..
            } if vel.as_int() > 0 => format!("on{}={}", key.as_int(), vel.as_int()),
            midly::TrackEventKind::Midi {
                message: midly::MidiMessage::NoteOff { key, .. },
                ..
            } => format!("off{}", key.as_int()),
            _ => continue,
        };
        observed.push((tick, label));
    }
    assert_eq!(
        &observed[..8],
        &[
            (0, "program".to_owned()),
            (0, "cc10=64".to_owned()),
            (0, "cc91=32".to_owned()),
            (0, "cc1=8".to_owned()),
            (0, "cc11=64".to_owned()),
            (0, "cc74=24".to_owned()),
            (0, "bend=8192".to_owned()),
            (0, "on29=127".to_owned()),
        ]
    );
    assert!(observed.contains(&(240, "cc1=100".to_owned())));
    assert!(observed.contains(&(240, "cc74=118".to_owned())));
    assert!(observed.contains(&(240, "bend=12288".to_owned())));
    assert!(observed.contains(&(480, "off29".to_owned())));
    assert!(observed.contains(&(480, "cc11=127".to_owned())));
}

#[test]
fn clip_linear_automation_uses_a_fixed_grid_and_preserves_the_step_default() {
    let dir = tempfile::tempdir().unwrap();
    let linear = dir.path().join("linear.yaml");
    let linear_text = "tempo: 120\nbars: 1\nclips:\n  filter:\n    kind: pitched\n    length_beats: 4\n    mode: once\n    events:\n      hit: { at: 0, duration: 1, pitch: C2, velocity: 100 }\n    automation:\n      brightness:\n        target: cc74\n        interpolation: linear\n        points:\n          start: { at: 0, value: 0 }\n          peak: { at: 0.25, value: 127 }\n          settle: { at: 0.5, value: 0 }\n      bend:\n        target: pitch_bend\n        interpolation: linear\n        points:\n          start: { at: 0, value: -8192 }\n          peak: { at: 0.25, value: 8191 }\n          settle: { at: 0.5, value: 0 }\ntracks:\n  - { id: stab, instrument: synth_brass, pattern: clip, clip: filter }\n";
    fs::write(&linear, linear_text).unwrap();
    let first = dir.path().join("linear-first.mid");
    let second = dir.path().join("linear-second.mid");
    for output in [&first, &second] {
        bin()
            .arg("midi")
            .arg(&linear)
            .arg("-o")
            .arg(output)
            .assert()
            .success();
    }
    let bytes = fs::read(&first).unwrap();
    assert_eq!(bytes, fs::read(&second).unwrap());

    let smf = midly::Smf::parse(&bytes).expect("linear automation MIDI parses");
    let mut tick = 0u32;
    let mut controls = Vec::new();
    let mut bends = Vec::new();
    for event in &smf.tracks[1] {
        tick += event.delta.as_int();
        match event.kind {
            midly::TrackEventKind::Midi {
                message: midly::MidiMessage::Controller { controller, value },
                ..
            } if controller.as_int() == 74 => controls.push((tick, value.as_int())),
            midly::TrackEventKind::Midi {
                message: midly::MidiMessage::PitchBend { bend },
                ..
            } => bends.push((tick, bend.0.as_int())),
            _ => {}
        }
    }
    assert_eq!(
        controls,
        [(0, 0), (60, 64), (120, 127), (180, 64), (240, 0)]
    );
    assert_eq!(
        bends,
        [(0, 0), (60, 8191), (120, 16383), (180, 12288), (240, 8192),]
    );

    let omitted = dir.path().join("step-omitted.yaml");
    let explicit = dir.path().join("step-explicit.yaml");
    fs::write(
        &omitted,
        linear_text.replace("        interpolation: linear\n", ""),
    )
    .unwrap();
    fs::write(
        &explicit,
        linear_text.replace("interpolation: linear", "interpolation: step"),
    )
    .unwrap();
    let omitted_midi = dir.path().join("step-omitted.mid");
    let explicit_midi = dir.path().join("step-explicit.mid");
    for (scene, output) in [(&omitted, &omitted_midi), (&explicit, &explicit_midi)] {
        bin()
            .arg("midi")
            .arg(scene)
            .arg("-o")
            .arg(output)
            .assert()
            .success();
    }
    assert_eq!(
        fs::read(omitted_midi).unwrap(),
        fs::read(explicit_midi).unwrap(),
        "omitted interpolation must retain the legacy step semantics"
    );
}

#[test]
fn invalid_clip_automation_fails_before_writing_midi() {
    let dir = tempfile::tempdir().unwrap();
    let cases = [
        (
            "duplicate-target",
            "clips.bass.automation.second.target",
            "      first:\n        target: cc1\n        points:\n          start: { at: 0, value: 0 }\n      second:\n        target: cc1\n        points:\n          start: { at: 0, value: 0 }\n",
        ),
        (
            "missing-start",
            "clips.bass.automation.mouth.points.late.at",
            "      mouth:\n        target: cc1\n        points:\n          late: { at: 0.5, value: 10 }\n",
        ),
        (
            "bad-value",
            "clips.bass.automation.mouth.points.start.value",
            "      mouth:\n        target: cc1\n        points:\n          start: { at: 0, value: 128 }\n",
        ),
        (
            "unsealed-loop",
            "clips.bass.automation.mouth.points.end.value",
            "      mouth:\n        target: cc1\n        points:\n          start: { at: 0, value: 0 }\n          end: { at: 3.5, value: 127 }\n",
        ),
    ];
    for (name, field, automation) in cases {
        let scene = dir.path().join(format!("{name}.yaml"));
        let output = dir.path().join(format!("{name}.mid"));
        fs::write(
            &scene,
            format!(
                "tempo: 140\nbars: 1\nclips:\n  bass:\n    kind: pitched\n    length_beats: 4\n    mode: loop\n    events:\n      hit: {{ at: 0, duration: 1, pitch: F1, velocity: 100 }}\n    automation:\n{automation}tracks:\n  - {{ id: bass, instrument: synth_bass, pattern: clip, clip: bass }}\n"
            ),
        )
        .unwrap();
        let out = bin()
            .args(["--json", "midi"])
            .arg(&scene)
            .arg("-o")
            .arg(&output)
            .assert()
            .code(2);
        let error: serde_json::Value =
            serde_json::from_slice(&out.get_output().stderr).expect("automation error is JSON");
        assert_eq!(error["field"], field, "error: {error}");
        assert!(!output.exists());
    }
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
    fs::write(&scene, "tempo: 100\nbars: 4\nbogus_field: 1\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n").unwrap();
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
        "tempo: 999\nbars: 4\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
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
        "tempo: 100\nbars: 4\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: drums\n",
    )
    .unwrap();
    let out = bin().arg("validate").arg(&scene).assert().code(2);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    assert!(stderr.contains("tracks[0].pattern"), "stderr: {stderr}");
}

#[test]
fn validate_requires_unique_stable_track_and_palette_keys() {
    let dir = tempfile::tempdir().unwrap();

    let missing = dir.path().join("missing-id.yaml");
    fs::write(
        &missing,
        "tempo: 100\nbars: 2\ntracks:\n  - instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    let out = bin().arg("validate").arg(&missing).assert().code(2);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr);
    assert!(stderr.contains("missing field `id`"), "stderr: {stderr}");

    for (name, tracks, field) in [
        (
            "duplicate-id",
            "  - { id: lead, instrument: piano, pattern: sustain }\n  - { id: lead, instrument: cello, pattern: sustain }\n",
            "tracks[1].id",
        ),
        (
            "invalid-id",
            "  - { id: Lead/Violin, instrument: violin, pattern: sustain }\n",
            "tracks[0].id",
        ),
        (
            "invalid-palette",
            "  - { id: lead, palette: ../solo, instrument: violin, pattern: sustain }\n",
            "tracks[0].palette",
        ),
    ] {
        let scene = dir.path().join(format!("{name}.yaml"));
        fs::write(&scene, format!("tempo: 100\nbars: 2\ntracks:\n{tracks}")).unwrap();
        let out = bin()
            .args(["--json", "validate"])
            .arg(&scene)
            .assert()
            .code(2);
        let error: serde_json::Value =
            serde_json::from_slice(&out.get_output().stderr).expect("JSON validation error");
        assert_eq!(error["field"], field, "error: {error}");
    }
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
    let required = v["$defs"]["Track"]["required"].as_array().unwrap();
    assert!(required.iter().any(|value| value == "id"));
    assert_eq!(
        v["$defs"]["Track"]["properties"]["id"]["pattern"],
        "^[a-z][a-z0-9_-]{0,63}$"
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
fn orchestration_schema_and_check_load_relative_leaf_profiles() {
    let dir = tempfile::tempdir().unwrap();
    let orchestration = write_test_orchestration(dir.path());

    let out = bin().args(["schema", "--orchestration"]).assert().success();
    let schema: serde_json::Value =
        serde_json::from_slice(&out.get_output().stdout).expect("orchestration schema is JSON");
    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        serde_json::json!(1)
    );
    assert!(schema["properties"]["default_palette"].is_object());
    assert!(schema["properties"]["palettes"].is_object());

    let out = bin()
        .args(["orchestration", "check"])
        .arg(&orchestration)
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout);
    assert!(stdout.contains("hybrid-test"), "stdout: {stdout}");
    assert!(stdout.contains("2 palette(s)"), "stdout: {stdout}");
    assert!(stdout.contains("default `default`"), "stdout: {stdout}");

    let out = bin()
        .args(["--json", "orchestration", "check"])
        .arg(&orchestration)
        .assert()
        .success();
    let report: serde_json::Value =
        serde_json::from_slice(&out.get_output().stdout).expect("check report is JSON");
    assert_eq!(report["name"], "hybrid-test");
    assert_eq!(report["default_palette"], "default");
    assert_eq!(report["palettes"].as_array().unwrap().len(), 2);
    assert_eq!(report["palettes"][0]["profile_path"], "profile.yaml");
}

#[test]
fn orchestration_check_contextualizes_missing_leaf_profile() {
    let dir = tempfile::tempdir().unwrap();
    let orchestration = dir.path().join("orchestration.yaml");
    fs::write(
        &orchestration,
        "schema_version: 1\nname: broken\ndefault_palette: default\npalettes:\n  default: { profile: missing.yaml }\n",
    )
    .unwrap();

    let out = bin()
        .args(["--json", "orchestration", "check"])
        .arg(&orchestration)
        .assert()
        .code(2);
    let error: serde_json::Value =
        serde_json::from_slice(&out.get_output().stderr).expect("JSON validation error");
    assert_eq!(error["code"], "validation");
    assert_eq!(error["field"], "palettes.default.profile");
    assert!(error["message"].as_str().unwrap().contains("missing.yaml"));
    assert_dir_contains_exactly(dir.path(), &["orchestration.yaml"]);
}

#[test]
fn sfizz_orchestration_requires_declared_clip_automation_controls() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("bass.sfz"), "<region> sample=bass.wav\n").unwrap();
    let scene = dir.path().join("automated.yaml");
    fs::write(
        &scene,
        "tempo: 140\nbars: 1\nclips:\n  talking:\n    kind: pitched\n    length_beats: 4\n    mode: loop\n    events:\n      hit: { at: 0, duration: 1, pitch: F1, velocity: 127 }\n    automation:\n      mouth:\n        target: cc1\n        points:\n          start: { at: 0, value: 0 }\n          open: { at: 0.5, value: 127 }\n          seal: { at: 3.5, value: 0 }\ntracks:\n  - { id: bass, instrument: synth_bass, pattern: clip, clip: talking }\n",
    )
    .unwrap();
    let profile = dir.path().join("profile.yaml");
    fs::write(
        &profile,
        "name: no-controls\ninstruments:\n  synth_bass:\n    sustain: bass.sfz\n",
    )
    .unwrap();
    let orchestration = write_orchestration_for_profile(dir.path(), &profile);

    let out = bin()
        .args(["--json", "inspect-instruments"])
        .arg(&scene)
        .arg("--orchestration")
        .arg(&orchestration)
        .assert()
        .code(2);
    let error: serde_json::Value =
        serde_json::from_slice(&out.get_output().stderr).expect("validation error is JSON");
    assert_eq!(error["field"], "clips.talking.automation.mouth.target");
    assert!(error["message"].as_str().unwrap().contains("cc1"));

    bin()
        .arg("build")
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("--orchestration")
        .arg(&orchestration)
        .arg("-o")
        .arg(dir.path().join("out.wav"))
        .assert()
        .code(2);
    assert_dir_contains_exactly(
        dir.path(),
        &[
            "automated.yaml",
            "bass.sfz",
            "orchestration.yaml",
            "profile.yaml",
        ],
    );

    fs::write(
        &profile,
        "name: controls\ninstruments:\n  synth_bass:\n    sustain:\n      path: bass.sfz\n      controls: [cc1]\n",
    )
    .unwrap();
    bin()
        .arg("inspect-instruments")
        .arg(&scene)
        .arg("--orchestration")
        .arg(&orchestration)
        .assert()
        .success();
}

#[test]
fn sfizz_suite_ignores_controls_from_an_inactive_base_clip() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("bass.sfz"), "<region> sample=bass.wav\n").unwrap();
    let scene = dir.path().join("suite.yaml");
    fs::write(
        &scene,
        "tempo: 140\nbars: 1\nclips:\n  talking:\n    kind: pitched\n    length_beats: 4\n    mode: loop\n    events:\n      hit: { at: 0, duration: 1, pitch: F1, velocity: 127 }\n    automation:\n      mouth:\n        target: cc1\n        points:\n          start: { at: 0, value: 0 }\n          open: { at: 0.5, value: 127 }\n          seal: { at: 3.5, value: 0 }\n  clean:\n    kind: pitched\n    length_beats: 4\n    mode: loop\n    events:\n      hit: { at: 0, duration: 1, pitch: F1, velocity: 127 }\ntracks:\n  - { id: bass, instrument: synth_bass, pattern: clip, clip: talking }\nsections:\n  - name: replaced\n    bars: 1\n    clips: { bass: clean }\n",
    )
    .unwrap();
    let profile = dir.path().join("profile.yaml");
    fs::write(
        &profile,
        "name: no-controls\ninstruments:\n  synth_bass:\n    sustain: bass.sfz\n",
    )
    .unwrap();
    let orchestration = write_orchestration_for_profile(dir.path(), &profile);

    bin()
        .arg("inspect-instruments")
        .arg(&scene)
        .arg("--orchestration")
        .arg(&orchestration)
        .assert()
        .success();
}

#[test]
fn story_is_informational_and_never_affects_midi_bytes() {
    // `story` is an annotation for downstream agent review; the protocol
    // guarantees it never changes compiled output. Same scene with and
    // without a story must validate and produce byte-identical MIDI.
    let dir = tempfile::tempdir().unwrap();
    let base = "tempo: 100\nbars: 2\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n";
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
fn track_identity_and_palette_do_not_change_midi_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let intimate = dir.path().join("intimate.yaml");
    let ensemble = dir.path().join("ensemble.yaml");
    fs::write(
        &intimate,
        "tempo: 100\nbars: 2\ntracks:\n  - id: harmony\n    palette: intimate\n    instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    fs::write(
        &ensemble,
        "tempo: 100\nbars: 2\ntracks:\n  - id: renamed_harmony\n    palette: ensemble\n    instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();

    let a = dir.path().join("a.mid");
    let b = dir.path().join("b.mid");
    bin()
        .arg("midi")
        .arg(&intimate)
        .arg("-o")
        .arg(&a)
        .assert()
        .success();
    bin()
        .arg("midi")
        .arg(&ensemble)
        .arg("-o")
        .arg(&b)
        .assert()
        .success();

    assert_eq!(
        fs::read(a).unwrap(),
        fs::read(b).unwrap(),
        "track identity and palette are routing metadata, not MIDI instructions"
    );
}

#[test]
fn textures_do_not_change_midi_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let base = "tempo: 120\nbars: 2\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n";
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
        "tempo: 120\nbars: 2\nloop: true\ntextures:\n  - source: river\n    mode: loop\n    start_beat: 1\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
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
        "tempo: 60\nbars: 2\ntextures:\n  - source: bell\n    mode: one_shot\n    at: [5]\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\nsections:\n  - name: short\n    bars: 1\n    loop: true\n  - name: long\n    bars: 2\n    loop: false\n",
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
        "story: { mood: 0.9 }\ntempo: 100\nbars: 4\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
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
        "tempo: 999\nbars: 4\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
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

// ---- makecode ----

#[test]
fn makecode_happy_path_writes_song_and_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("forest.ts");
    bin()
        .arg("makecode")
        .arg(forest())
        .arg("-o")
        .arg(&out)
        .assert()
        .success();
    assert_dir_contains_exactly(dir.path(), &["forest.ts", "forest.meta.json"]);

    let ts = fs::read_to_string(&out).unwrap();
    assert!(
        ts.contains("let forest = music.createSong(hex`"),
        "generated TypeScript declares the song: {ts}"
    );

    let meta: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("forest.meta.json")).unwrap()).unwrap();
    assert_eq!(meta["target"], "makecode-song-v0");
    let song = &meta["songs"][0];
    assert_eq!(song["name"], "forest");
    assert_eq!(song["bpm"], 92);
    assert_eq!(song["beats_per_measure"], 4);
    assert_eq!(song["measures"], 8);
    assert_eq!(song["loop"], true);
    let tracks = song["tracks"].as_array().unwrap();
    assert_eq!(tracks.len(), 4, "all four scene tracks are reported");
    assert_eq!(tracks[0]["scene_track"], "harmony");
    assert_eq!(tracks[0]["kind"], "melodic");
    assert_eq!(tracks[0]["chip_preset"], "fish");
    let drums = &tracks[3];
    assert_eq!(drums["kind"], "drums");
    assert_eq!(drums["drum_voices"][0]["key"], 36);
    assert_eq!(drums["drum_voices"][0]["voice"], "neutral kick");
}

#[test]
fn makecode_matches_golden_source() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("forest.ts");
    bin()
        .arg("makecode")
        .arg(forest())
        .arg("-o")
        .arg(&out)
        .assert()
        .success();
    let produced = fs::read(&out).unwrap();
    let golden = fs::read(repo("tests/golden/forest.makecode.ts")).unwrap();
    assert_eq!(
        produced, golden,
        "MakeCode source must be identical to the golden file"
    );
}

#[test]
fn makecode_is_deterministic_across_runs() {
    let dir = tempfile::tempdir().unwrap();
    for sub in ["a", "b"] {
        fs::create_dir(dir.path().join(sub)).unwrap();
        bin()
            .arg("makecode")
            .arg(forest())
            .arg("-o")
            .arg(dir.path().join(sub).join("forest.ts"))
            .assert()
            .success();
    }
    for name in ["forest.ts", "forest.meta.json"] {
        assert_eq!(
            fs::read(dir.path().join("a").join(name)).unwrap(),
            fs::read(dir.path().join("b").join(name)).unwrap(),
            "{name} differs between identical runs"
        );
    }
}

#[test]
fn makecode_suite_emits_one_song_per_section() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("suite.ts");
    bin()
        .arg("makecode")
        .arg(repo("examples/scenes/forest_suite.yaml"))
        .arg("-o")
        .arg(&out)
        .assert()
        .success();
    let ts = fs::read_to_string(&out).unwrap();
    for name in [
        "suite_intro",
        "suite_explore",
        "suite_combat",
        "suite_victory",
    ] {
        assert!(
            ts.contains(&format!("let {name} = music.createSong(hex`")),
            "missing section song `{name}`: {ts}"
        );
    }
    let meta: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("suite.meta.json")).unwrap()).unwrap();
    let songs = meta["songs"].as_array().unwrap();
    assert_eq!(songs.len(), 4);
    assert_eq!(songs[1]["section"], "explore");
    assert_eq!(songs[1]["loop"], true);
    assert_eq!(songs[2]["section"], "combat");
    assert_eq!(songs[2]["bpm"], 132, "section tempo override is honored");
    assert_eq!(songs[3]["loop"], false);
}

#[test]
fn makecode_glide_scene_fails_with_no_partial_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("glide.yaml");
    fs::write(
        &scene,
        "tempo: 100\nkey: C_major\nbars: 2\nmotifs:\n  m:\n    - { degree: 1, beats: 2 }\n    - { degree: 5, beats: 2 }\ntracks:\n  - id: lead\n    instrument: flute\n    pattern: melody\n    motif: m\n    glide: 0.5\n",
    )
    .unwrap();
    let out = bin()
        .args(["--json", "makecode"])
        .arg(&scene)
        .arg("-o")
        .arg(dir.path().join("glide.ts"))
        .assert()
        .code(2);
    let error: serde_json::Value =
        serde_json::from_slice(&out.get_output().stderr).expect("validation error is JSON");
    assert_eq!(error["field"], "tracks[0]");
    assert!(
        error["message"].as_str().unwrap().contains("pitch bends"),
        "error explains the missing capability: {error}"
    );
    assert_dir_contains_exactly(dir.path(), &["glide.yaml"]);
}

#[test]
fn makecode_texture_scene_fails_with_no_partial_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("tex.yaml");
    fs::write(
        &scene,
        "tempo: 100\nkey: C_major\nbars: 2\ntextures:\n  - source: wind\n    mode: loop\ntracks:\n  - id: lead\n    instrument: flute\n    pattern: sustain\n",
    )
    .unwrap();
    let out = bin()
        .args(["--json", "makecode"])
        .arg(&scene)
        .arg("-o")
        .arg(dir.path().join("tex.ts"))
        .assert()
        .code(2);
    let error: serde_json::Value =
        serde_json::from_slice(&out.get_output().stderr).expect("validation error is JSON");
    assert_eq!(error["field"], "textures");
    assert_dir_contains_exactly(dir.path(), &["tex.yaml"]);
}

#[test]
fn makecode_humanized_scene_fails_with_grid_error() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("human.yaml");
    fs::write(
        &scene,
        "tempo: 100\nkey: C_major\nbars: 2\nperformance:\n  humanize:\n    timing_ms: 7\n    seed: 42\ntracks:\n  - id: lead\n    instrument: flute\n    pattern: sustain\n",
    )
    .unwrap();
    let out = bin()
        .arg("makecode")
        .arg(&scene)
        .arg("-o")
        .arg(dir.path().join("human.ts"))
        .assert()
        .code(2);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    assert!(
        stderr.contains("ticks per beat") && stderr.contains("humanize"),
        "error names the grid limit and the fix: {stderr}"
    );
    assert_dir_contains_exactly(dir.path(), &["human.yaml"]);
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
        "tempo: 120\nbars: 1\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
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
        "tempo: 120\nbars: 1\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
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
        "tempo: 120\nbars: 1\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
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

/// One structured texture source. Every discovery field is mandatory in the
/// format, so an agent can rely on all of them being present for every
/// source rather than on whichever the author felt like filling in.
fn texture_source_yaml(
    name: &str,
    path: &str,
    category: &str,
    tags: &[&str],
    modes: &[&str],
    use_cases: &[&str],
) -> String {
    format!(
        "  {name}:\n    path: {path}\n    description: Test recording {name}\n    \
         category: {category}\n    tags: [{}]\n    playback:\n      modes: [{}]\n      \
         default_mode: {}\n    use_cases: [{}]\n    provenance:\n      \
         library: test-fixtures@1.0.0\n",
        tags.join(", "),
        modes.join(", "),
        modes[0],
        use_cases.join(", "),
    )
}

fn texture_profile_yaml(name: &str, sources: &str) -> String {
    format!("schema_version: 1\nname: {name}\nsources:\n{sources}")
}

/// The two-source profile most texture build tests bind against.
fn river_birds_profile() -> String {
    texture_profile_yaml(
        "field-recordings",
        &format!(
            "{}{}",
            texture_source_yaml(
                "river",
                "river.wav",
                "organic",
                &["water", "flowing"],
                &["loop"],
                &["forest"]
            ),
            texture_source_yaml(
                "birds",
                "birds.wav",
                "organic",
                &["wildlife", "chirping"],
                &["one_shot"],
                &["forest"]
            ),
        ),
    )
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
        "tempo: 120\nbars: 2\nloop: false\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
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
        "tempo: 120\nbars: 1\nloop: false\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
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
        "tempo: 120\nbars: 1\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
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
            "01-harmony.wav",
            "02-motion.wav",
            "03-foundation.wav",
            "04-pulse.wav",
        ],
    );
    let l = forest_loop_samples() as usize;
    let (spec, mix) = read_frames(&wav);
    let ch = spec.channels as usize;
    let stems: Vec<Vec<i16>> = [
        "01-harmony.wav",
        "02-motion.wav",
        "03-foundation.wav",
        "04-pulse.wav",
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
    assert_eq!(meta["stems"][0], "forest.stems/01-harmony.wav");
}

#[test]
fn build_multiple_percussion_stems_are_independent_aligned_and_sum_to_mix() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("multi-percussion.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 1\ntracks:\n  - { id: kit, instrument: drums, pattern: drums }\n  - { id: auxiliary, instrument: drums, pattern: drums, intensity: 0.5 }\n",
    )
    .unwrap();
    let wav = dir.path().join("multi-percussion.wav");
    write_tone_sfz(dir.path(), "drums", 120.0);
    let profile = dir.path().join("percussion-profile.yaml");
    fs::write(
        &profile,
        "name: percussion-test\ninstruments:\n  drums:\n    sustain: drums.sfz\n",
    )
    .unwrap();
    let orchestration = write_orchestration_for_profile(dir.path(), &profile);
    bin()
        .arg("build")
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("--orchestration")
        .arg(&orchestration)
        .arg("-o")
        .arg(&wav)
        .arg("--stems")
        .env("PATH", sfizz_path_env())
        .assert()
        .success();

    let stems_dir = dir.path().join("multi-percussion.stems");
    assert_dir_contains_exactly(&stems_dir, &["01-kit.wav", "02-auxiliary.wav"]);
    let (spec, mix) = read_frames(&wav);
    let stems: Vec<Vec<i16>> = ["01-kit.wav", "02-auxiliary.wav"]
        .iter()
        .map(|name| {
            let (stem_spec, data) = read_frames(&stems_dir.join(name));
            assert_eq!(stem_spec.channels, spec.channels);
            assert_eq!(data.len(), mix.len(), "stem {name} must align to the mix");
            assert!(
                data.iter().any(|&sample| sample != 0),
                "stem {name} is silent"
            );
            data
        })
        .collect();
    assert_ne!(stems[0], stems[1], "percussion stems must stay independent");

    let (mut diff2, mut ref2) = (0f64, 0f64);
    for i in 0..mix.len() {
        let sum: f64 = stems.iter().map(|stem| f64::from(stem[i])).sum();
        let reference = f64::from(mix[i]);
        diff2 += (sum - reference) * (sum - reference);
        ref2 += reference * reference;
    }
    let ratio = (diff2 / ref2.max(1.0)).sqrt();
    assert!(
        ratio < 0.02,
        "percussion stems do not sum to mix: RMS ratio {ratio:.4}"
    );
}

#[test]
fn build_textures_normalizes_places_mixes_and_emits_stems() {
    let dir = tempfile::tempdir().unwrap();
    let river = dir.path().join("river.wav");
    let birds = dir.path().join("birds.wav");
    write_texture_wave(&river, 137.0, 0.2);
    write_texture_wave(&birds, 733.0, 0.08);
    let profile = dir.path().join("textures.yaml");
    fs::write(&profile, river_birds_profile()).unwrap();
    let scene = dir.path().join("scene.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 2\nloop: true\ntextures:\n  - source: river\n    mode: loop\n    gain: 0.25\n  - source: birds\n    mode: one_shot\n    at: [1, 5]\n    gain: 0.5\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
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
        "tempo: 120\nbars: 1\ntextures:\n  - { source: river, mode: loop }\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    let profile = dir.path().join("textures.yaml");
    fs::write(
        &profile,
        texture_profile_yaml(
            "missing-source",
            &texture_source_yaml(
                "river",
                "missing.wav",
                "organic",
                &["water"],
                &["loop"],
                &["forest"],
            ),
        ),
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
        texture_profile_yaml(
            "rollback-test",
            &texture_source_yaml(
                "long_bell",
                "long-bell.wav",
                "tonal",
                &["bell", "metallic"],
                &["one_shot"],
                &["ritual"],
            ),
        ),
    )
    .unwrap();
    let scene = dir.path().join("suite.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 2\ntextures:\n  - source: long_bell\n    mode: one_shot\n    at: [0]\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\nsections:\n  - name: long\n    bars: 2\n    loop: false\n  - name: short\n    bars: 1\n    loop: true\n",
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
            "01-harmony.ogg",
            "02-motion.ogg",
            "03-foundation.ogg",
            "04-pulse.ogg",
        ],
    );
}

#[test]
fn build_corrupt_soundfont_leaves_no_partial_output_or_stems() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("scene.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 1\nloop: true\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
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
    "tempo: 120\nbars: 2\nkey: C_major\nmotifs:\n  theme:\n    - { degree: 1, beats: 1 }\n    - { degree: 5, beats: 1 }\n    - { degree: 3, beats: 2 }\ntracks:\n  - id: flute\n    instrument: flute\n    pattern: melody\n    motif: theme\n  - id: strings\n    instrument: strings\n    pattern: sustain\nsections:\n  - name: explore\n    bars: 2\n    loop: true\n  - name: sting\n    bars: 1\n    tempo: 140\n    mute: [flute]\n"
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
fn midi_section_mute_and_solo_address_tracks_by_id() {
    let dir = tempfile::tempdir().unwrap();
    let suite = dir.path().join("suite.yaml");
    let expected = dir.path().join("expected.yaml");
    fs::write(
        &suite,
        "tempo: 120\nbars: 2\ntracks:\n  - { id: lead, instrument: piano, pattern: sustain }\n  - { id: pulse, instrument: bass, pattern: bass }\nsections:\n  - { name: pulse_only, bars: 2, mute: [lead] }\n",
    )
    .unwrap();
    fs::write(
        &expected,
        "tempo: 120\nbars: 2\ntracks:\n  - { id: pulse, instrument: bass, pattern: bass }\n",
    )
    .unwrap();

    let selected = dir.path().join("selected.mid");
    let standalone = dir.path().join("standalone.mid");
    bin()
        .arg("midi")
        .arg(&suite)
        .arg("-o")
        .arg(&selected)
        .args(["--section", "pulse_only", "--solo", "pulse"])
        .assert()
        .success();
    bin()
        .arg("midi")
        .arg(&expected)
        .arg("-o")
        .arg(&standalone)
        .args(["--solo", "pulse"])
        .assert()
        .success();
    assert_eq!(fs::read(&selected).unwrap(), fs::read(&standalone).unwrap());

    let missing = dir.path().join("missing.mid");
    let out = bin()
        .arg("midi")
        .arg(&suite)
        .arg("-o")
        .arg(&missing)
        .args(["--solo", "missing"])
        .assert()
        .code(2);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr);
    assert!(stderr.contains("--solo"), "stderr: {stderr}");
    assert!(!missing.exists());
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
        "tempo: 100\nbars: 2\ntracks:\n  - id: flute\n    instrument: flute\n    pattern: melody\n    motif: nonexistent\n",
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
        "tempo: 100\nbars: 2\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\nsections:\n  - { name: a, bars: 1 }\n  - { name: a, bars: 2 }\n",
    )
    .unwrap();
    let out = bin().arg("validate").arg(&dup).assert().code(2);
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    assert!(stderr.contains("sections[1].name"), "stderr: {stderr}");

    let mute = dir.path().join("mute.yaml");
    fs::write(
        &mute,
        "tempo: 100\nbars: 2\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\nsections:\n  - { name: a, bars: 1, mute: [piano] }\n",
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
    let orchestration = write_orchestration_for_profile(dir.path(), &profile);
    let wav = dir.path().join("duo.wav");
    bin()
        .arg("build")
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("--orchestration")
        .arg(&orchestration)
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
fn build_sfizz_world_instruments_renders_erhu_and_tabla_stems() {
    let dir = tempfile::tempdir().unwrap();
    let scene = world_sfizz_scene(dir.path());
    write_sine_sfz(dir.path());
    let profile = dir.path().join("world-profile.yaml");
    fs::write(
        &profile,
        "name: world-test\ninstruments:\n  erhu:\n    sustain: mini.sfz\n  tabla:\n    sustain: mini.sfz\n",
    )
    .unwrap();
    let orchestration = write_orchestration_for_profile(dir.path(), &profile);
    let wav = dir.path().join("world.wav");

    bin()
        .arg("build")
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("--orchestration")
        .arg(&orchestration)
        .arg("-o")
        .arg(&wav)
        .arg("--stems")
        .args(["--tail", "0"])
        .env("PATH", sfizz_path_env())
        .assert()
        .success();

    let stems = dir.path().join("world.stems");
    assert_dir_contains_exactly(&stems, &["01-erhu.wav", "02-tabla.wav"]);
    for name in ["01-erhu.wav", "02-tabla.wav"] {
        let (_, samples) = read_frames(&stems.join(name));
        assert!(
            samples.iter().any(|&sample| sample.abs() > 50),
            "{name} is silent"
        );
    }
    let meta: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("world.meta.json")).unwrap()).unwrap();
    assert_eq!(meta["instrument_resolution"]["summary"]["exact"], 2);
    assert_eq!(
        meta["instrument_resolution"]["tracks"][1]["canonical"],
        "tabla"
    );
}

#[test]
fn build_sfizz_routes_same_instrument_to_distinct_palette_patches() {
    let dir = tempfile::tempdir().unwrap();
    write_tone_sfz(dir.path(), "solo", 440.0);
    write_tone_sfz(dir.path(), "ensemble", 660.0);
    fs::write(
        dir.path().join("solo-profile.yaml"),
        "name: solo-profile\ninstruments:\n  violin:\n    sustain: solo.sfz\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("ensemble-profile.yaml"),
        "name: ensemble-profile\ninstruments:\n  violin:\n    sustain: ensemble.sfz\n",
    )
    .unwrap();
    let orchestration = dir.path().join("orchestration.yaml");
    fs::write(
        &orchestration,
        "schema_version: 1\nname: hybrid\ndefault_palette: ensemble\npalettes:\n  ensemble: { profile: ensemble-profile.yaml }\n  solo: { profile: solo-profile.yaml }\n",
    )
    .unwrap();
    let scene = dir.path().join("hybrid.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 1\ntracks:\n  - { id: lead, palette: solo, instrument: violin, pattern: sustain }\n  - { id: section, instrument: violin, pattern: sustain }\n",
    )
    .unwrap();
    let wav = dir.path().join("hybrid.wav");

    bin()
        .arg("build")
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("--orchestration")
        .arg(&orchestration)
        .arg("-o")
        .arg(&wav)
        .arg("--stems")
        .env("PATH", sfizz_path_env())
        .assert()
        .success();

    let stems = dir.path().join("hybrid.stems");
    assert_dir_contains_exactly(&stems, &["01-lead.wav", "02-section.wav"]);
    let (_, lead) = read_frames(&stems.join("01-lead.wav"));
    let (_, section) = read_frames(&stems.join("02-section.wav"));
    assert_ne!(
        lead, section,
        "the two palettes must select different patches"
    );
    let (_, mix) = read_frames(&wav);
    assert_eq!(lead.len(), mix.len());
    assert_eq!(section.len(), mix.len());
    let (mut diff2, mut ref2) = (0.0, 0.0);
    for ((lead, section), mix) in lead.iter().zip(&section).zip(&mix) {
        let summed = f64::from(*lead) + f64::from(*section);
        let mix = f64::from(*mix);
        diff2 += (summed - mix) * (summed - mix);
        ref2 += mix * mix;
    }
    let ratio = (diff2 / ref2.max(1.0)).sqrt();
    assert!(
        ratio < 0.02,
        "multi-profile stems do not sum to mix: RMS ratio {ratio:.4}"
    );

    let meta: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("hybrid.meta.json")).unwrap()).unwrap();
    assert_eq!(meta["orchestration"]["name"], "hybrid");
    assert_eq!(meta["tracks"][0]["id"], "lead");
    assert_eq!(meta["tracks"][0]["palette"], "solo");
    assert_eq!(meta["tracks"][1]["id"], "section");
    assert!(meta["tracks"][1]["palette"].is_null());
    assert_eq!(
        meta["instrument_resolution"]["tracks"][0]["profile"],
        "solo-profile"
    );
    assert_eq!(
        meta["instrument_resolution"]["tracks"][1]["profile"],
        "ensemble-profile"
    );
}

#[test]
fn build_sfizz_unknown_palette_fails_before_writing_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let profile = write_test_profile(dir.path());
    let orchestration = write_orchestration_for_profile(dir.path(), &profile);
    let scene = dir.path().join("bad.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 1\ntracks:\n  - { id: lead, palette: unavailable, instrument: violin, pattern: sustain }\n",
    )
    .unwrap();

    let out = bin()
        .args(["--json", "build"])
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("--orchestration")
        .arg(&orchestration)
        .arg("-o")
        .arg(dir.path().join("bad.wav"))
        .assert()
        .code(2);
    let error: serde_json::Value =
        serde_json::from_slice(&out.get_output().stderr).expect("JSON validation error");
    assert_eq!(error["code"], "validation");
    assert_eq!(error["field"], "tracks[0].palette");
    assert_dir_contains_exactly(
        dir.path(),
        &[
            "bad.yaml",
            "mini.sfz",
            "sine.wav",
            "profile.yaml",
            "orchestration.yaml",
        ],
    );
}

#[test]
fn build_sfizz_missing_orchestration_is_input_error() {
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
    let orchestration = write_orchestration_for_profile(dir.path(), &profile);
    bin()
        .arg("build")
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("--orchestration")
        .arg(&orchestration)
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
    // Profile only maps `violin` (strings); the scene's `trumpet` track has
    // no mapping and no same-family candidate — and brass must never fall
    // back to strings silently.
    let scene = dir.path().join("duo.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 2\nloop: false\ntracks:\n  - id: violin\n    instrument: violin\n    pattern: sustain\n  - id: trumpet\n    instrument: trumpet\n    pattern: sustain\n",
    )
    .unwrap();
    write_sine_sfz(dir.path());
    let profile = dir.path().join("profile.yaml");
    fs::write(
        &profile,
        "name: test-profile\ninstruments:\n  violin:\n    sustain: mini.sfz\n",
    )
    .unwrap();
    let orchestration = write_orchestration_for_profile(dir.path(), &profile);
    let out = bin()
        .arg("--json")
        .arg("build")
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("--orchestration")
        .arg(&orchestration)
        .arg("-o")
        .arg(dir.path().join("duo.wav"))
        .env("PATH", sfizz_path_env())
        .assert()
        .code(2);
    let error: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    assert_eq!(error["code"], "resolution");
    assert_eq!(error["report"]["summary"]["missing"], 1);
    assert_eq!(error["report"]["missing_instruments"][0], "trumpet");
    assert_dir_contains_exactly(
        dir.path(),
        &[
            "duo.yaml",
            "mini.sfz",
            "sine.wav",
            "profile.yaml",
            "orchestration.yaml",
        ],
    );
}

#[test]
fn build_sfizz_missing_binary_is_dependency_error() {
    let dir = tempfile::tempdir().unwrap();
    let scene = tiny_sfizz_scene(dir.path());
    let profile = write_test_profile(dir.path());
    let orchestration = write_orchestration_for_profile(dir.path(), &profile);
    bin()
        .arg("build")
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("--orchestration")
        .arg(&orchestration)
        .arg("-o")
        .arg(dir.path().join("duo.wav"))
        .env("PATH", "")
        .assert()
        .code(3);
    assert_dir_contains_exactly(
        dir.path(),
        &[
            "duo.yaml",
            "mini.sfz",
            "sine.wav",
            "profile.yaml",
            "orchestration.yaml",
        ],
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
    let orchestration = write_orchestration_for_profile(dir.path(), &profile);
    bin()
        .arg("build")
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("--orchestration")
        .arg(&orchestration)
        .arg("-o")
        .arg(dir.path().join("duo.wav"))
        .env("PATH", sfizz_path_env())
        .assert()
        .code(4);
    assert_dir_contains_exactly(
        dir.path(),
        &["duo.yaml", "mini.sfz", "profile.yaml", "orchestration.yaml"],
    );
}

// ---- Instrument resolution & fallback (M12) ------------------------------

/// Same-family substitution: `cello` is unmapped, `violin` (same family,
/// compatible range/articulation) stands in — the build succeeds, warns
/// visibly, and meta.json embeds the full explainable resolution report.
#[test]
fn build_sfizz_same_family_fallback_substitutes_and_reports() {
    let dir = tempfile::tempdir().unwrap();
    let scene = tiny_sfizz_scene(dir.path());
    write_sine_sfz(dir.path());
    let profile = dir.path().join("profile.yaml");
    fs::write(
        &profile,
        "name: test-profile\ninstruments:\n  violin:\n    sustain: mini.sfz\n",
    )
    .unwrap();
    let orchestration = write_orchestration_for_profile(dir.path(), &profile);
    let wav = dir.path().join("duo.wav");
    let out = bin()
        .arg("build")
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("--orchestration")
        .arg(&orchestration)
        .arg("-o")
        .arg(&wav)
        .env("PATH", sfizz_path_env())
        .assert()
        .success();
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    assert!(
        stderr.contains("WARN instrument fallback:") && stderr.contains("requested=cello"),
        "missing fallback warning: {stderr}"
    );
    assert!(wav.is_file());
    let meta: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("duo.meta.json")).unwrap()).unwrap();
    let resolution = &meta["instrument_resolution"];
    assert_eq!(resolution["summary"]["exact"], 1);
    assert_eq!(resolution["summary"]["fallback"], 1);
    assert_eq!(resolution["fallbacks"][0]["requested"], "cello");
    assert_eq!(resolution["fallbacks"][0]["resolved"], "violin");
    assert!(resolution["fallbacks"][0]["score"].as_f64().unwrap() >= 0.70);
    let cello = &resolution["tracks"][1];
    assert_eq!(cello["status"], "fallback");
    assert!(
        cello["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r == "same_subfamily" || r == "same_family"),
        "reasons: {}",
        cello["reasons"]
    );
    // Stem naming stays under the *requested* instrument name.
    assert_eq!(meta["tracks"][1]["instrument"], "cello");
}

/// Strict mode: no substitution at all — the same scene that succeeds under
/// the conservative default fails fast, with no partial artifacts.
#[test]
fn build_sfizz_strict_mode_rejects_fallback_without_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let scene = tiny_sfizz_scene(dir.path());
    write_sine_sfz(dir.path());
    let profile = dir.path().join("profile.yaml");
    fs::write(
        &profile,
        "name: test-profile\ninstruments:\n  violin:\n    sustain: mini.sfz\n",
    )
    .unwrap();
    let orchestration = write_orchestration_for_profile(dir.path(), &profile);
    let out = bin()
        .arg("--json")
        .arg("build")
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("--orchestration")
        .arg(&orchestration)
        .args(["--fallback-mode", "strict"])
        .arg("-o")
        .arg(dir.path().join("duo.wav"))
        .env("PATH", sfizz_path_env())
        .assert()
        .code(2);
    let error: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    assert_eq!(error["code"], "resolution");
    assert_eq!(error["report"]["summary"]["rejected"], 1);
    // The report still names the candidate strict mode refused to use.
    assert_eq!(
        error["report"]["tracks"][1]["best_candidate"]["instrument"],
        "violin"
    );
    assert_dir_contains_exactly(
        dir.path(),
        &[
            "duo.yaml",
            "mini.sfz",
            "sine.wav",
            "profile.yaml",
            "orchestration.yaml",
        ],
    );
}

/// A resolver config can widen the policy: flexible mode lets a brass
/// request reach a related-family synth pad — but never strings.
#[test]
fn build_sfizz_flexible_config_reaches_synth_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("solo.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 2\nloop: false\ntracks:\n  - id: horn\n    instrument: horn\n    pattern: sustain\n",
    )
    .unwrap();
    write_sine_sfz(dir.path());
    let profile = dir.path().join("profile.yaml");
    fs::write(
        &profile,
        "name: test-profile\ninstruments:\n  warm_pad:\n    sustain: mini.sfz\n",
    )
    .unwrap();
    let orchestration = write_orchestration_for_profile(dir.path(), &profile);
    let resolver = dir.path().join("resolver.yaml");
    fs::write(&resolver, "default_mode: flexible\n").unwrap();
    // Conservative default: no synth stand-in, the build fails.
    bin()
        .arg("build")
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("--orchestration")
        .arg(&orchestration)
        .arg("-o")
        .arg(dir.path().join("solo.wav"))
        .env("PATH", sfizz_path_env())
        .assert()
        .code(2);
    // Flexible via config file: warm_pad may stand in for the horn.
    bin()
        .arg("build")
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("--orchestration")
        .arg(&orchestration)
        .arg("--resolver")
        .arg(&resolver)
        .arg("-o")
        .arg(dir.path().join("solo.wav"))
        .env("PATH", sfizz_path_env())
        .assert()
        .success();
    let meta: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("solo.meta.json")).unwrap()).unwrap();
    assert_eq!(
        meta["instrument_resolution"]["fallbacks"][0]["resolved"],
        "warm_pad"
    );
}

/// `inspect-instruments` reports all four statuses in one scene, exits 2
/// when instruments are missing, and its report is byte-identical across
/// runs (deterministic resolution).
#[test]
fn inspect_instruments_reports_statuses_and_is_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("mixed.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 2\ntracks:\n  - id: violin\n    instrument: violin\n    pattern: sustain\n  - id: fiddle\n    instrument: fiddle\n    pattern: sustain\n  - id: viola\n    instrument: viola\n    pattern: sustain\n  - id: trumpet\n    instrument: trumpet\n    pattern: sustain\n",
    )
    .unwrap();
    write_sine_sfz(dir.path());
    let profile = dir.path().join("profile.yaml");
    fs::write(
        &profile,
        "name: test-profile\ninstruments:\n  violin:\n    sustain: mini.sfz\n  cello:\n    sustain: mini.sfz\n",
    )
    .unwrap();
    let orchestration = write_orchestration_for_profile(dir.path(), &profile);
    let run = || {
        let out = bin()
            .arg("--json")
            .arg("inspect-instruments")
            .arg(&scene)
            .arg("--orchestration")
            .arg(&orchestration)
            .assert()
            .code(2);
        out.get_output().stderr.clone()
    };
    let first = run();
    let error: serde_json::Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(error["code"], "resolution");
    assert_eq!(error["exit_code"], 2);
    let report = &error["report"];
    assert_eq!(report["summary"]["exact"], 1);
    assert_eq!(report["summary"]["alias"], 1);
    assert_eq!(report["summary"]["fallback"], 1);
    assert_eq!(report["summary"]["missing"], 1);
    assert_eq!(report["missing_instruments"][0], "trumpet");
    // The alias row keeps the requested spelling as written in the file.
    assert_eq!(report["tracks"][1]["requested"], "fiddle");
    assert_eq!(report["tracks"][1]["canonical"], "violin");
    assert_eq!(report["tracks"][1]["status"], "alias");
    // Determinism: same inputs, byte-identical report.
    assert_eq!(first, run(), "resolution report differs between runs");
}

/// All-resolved scenes exit 0; `--json` prints the report on stdout and the
/// human report carries per-track lines plus a summary.
#[test]
fn inspect_instruments_all_resolved_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    let scene = tiny_sfizz_scene(dir.path());
    let profile = write_test_profile(dir.path());
    let orchestration = write_orchestration_for_profile(dir.path(), &profile);
    let out = bin()
        .arg("--json")
        .arg("inspect-instruments")
        .arg(&scene)
        .arg("--orchestration")
        .arg(&orchestration)
        .assert()
        .success();
    let report: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(report["summary"]["exact"], 2);
    assert_eq!(report["summary"]["missing"], 0);
    let human = bin()
        .arg("inspect-instruments")
        .arg(&scene)
        .arg("--orchestration")
        .arg(&orchestration)
        .assert()
        .success();
    let text = String::from_utf8_lossy(&human.get_output().stdout).into_owned();
    assert!(
        text.contains("tracks[violin]: violin -> violin (exact")
            && text.contains("summary: 2 exact"),
        "human report: {text}"
    );
}

#[test]
fn inspect_instruments_reports_per_track_palette_patch_routes() {
    let dir = tempfile::tempdir().unwrap();
    let mini = write_sine_sfz(dir.path());
    fs::copy(&mini, dir.path().join("ensemble.sfz")).unwrap();
    fs::write(
        dir.path().join("solo.yaml"),
        "name: solo-profile\ninstruments:\n  violin:\n    sustain: mini.sfz\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("ensemble.yaml"),
        "name: ensemble-profile\ninstruments:\n  violin:\n    sustain: ensemble.sfz\n",
    )
    .unwrap();
    let orchestration = dir.path().join("orchestration.yaml");
    fs::write(
        &orchestration,
        "schema_version: 1\nname: hybrid\ndefault_palette: ensemble\npalettes:\n  ensemble: { profile: ensemble.yaml }\n  solo: { profile: solo.yaml }\n",
    )
    .unwrap();
    let scene = dir.path().join("layered.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 2\ntracks:\n  - { id: lead, palette: solo, instrument: violin, pattern: sustain }\n  - { id: section, palette: ensemble, instrument: violin, pattern: sustain }\n",
    )
    .unwrap();

    let out = bin()
        .args(["--json", "inspect-instruments"])
        .arg(&scene)
        .arg("--orchestration")
        .arg(&orchestration)
        .assert()
        .success();
    let report: serde_json::Value =
        serde_json::from_slice(&out.get_output().stdout).expect("JSON inspect report");
    assert_eq!(report["summary"]["exact"], 2);
    assert_eq!(report["tracks"][0]["track_id"], "lead");
    assert_eq!(report["tracks"][0]["palette"], "solo");
    assert_eq!(report["tracks"][0]["profile"], "solo-profile");
    assert_eq!(report["tracks"][0]["profile_path"], "solo.yaml");
    assert!(
        report["tracks"][0]["sfz"]
            .as_str()
            .unwrap()
            .ends_with("mini.sfz")
    );
    assert_eq!(report["tracks"][1]["track_id"], "section");
    assert_eq!(report["tracks"][1]["palette"], "ensemble");
    assert_eq!(report["tracks"][1]["profile"], "ensemble-profile");
    assert!(
        report["tracks"][1]["sfz"]
            .as_str()
            .unwrap()
            .ends_with("ensemble.sfz")
    );

    let out = bin()
        .arg("inspect-instruments")
        .arg(&scene)
        .arg("--orchestration")
        .arg(&orchestration)
        .assert()
        .success();
    let human = String::from_utf8_lossy(&out.get_output().stdout);
    assert!(human.contains("tracks[lead]"), "report: {human}");
    assert!(human.contains("palette=solo"), "report: {human}");
    assert!(human.contains("profile=solo-profile"), "report: {human}");
    assert!(human.contains("mini.sfz"), "report: {human}");
}

#[test]
fn inspect_instruments_never_falls_back_across_palettes() {
    let dir = tempfile::tempdir().unwrap();
    write_sine_sfz(dir.path());
    fs::write(
        dir.path().join("solo.yaml"),
        "name: solo-profile\ninstruments:\n  violin:\n    sustain: mini.sfz\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("ensemble.yaml"),
        "name: ensemble-profile\ninstruments:\n  cello:\n    sustain: mini.sfz\n",
    )
    .unwrap();
    let orchestration = dir.path().join("orchestration.yaml");
    fs::write(
        &orchestration,
        "schema_version: 1\nname: isolated\ndefault_palette: ensemble\npalettes:\n  ensemble: { profile: ensemble.yaml }\n  solo: { profile: solo.yaml }\n",
    )
    .unwrap();
    let scene = dir.path().join("isolated.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 2\ntracks:\n  - { id: low_solo, palette: solo, instrument: cello, pattern: sustain }\n",
    )
    .unwrap();

    let out = bin()
        .args(["--json", "inspect-instruments"])
        .arg(&scene)
        .arg("--orchestration")
        .arg(&orchestration)
        .args(["--fallback-mode", "strict"])
        .assert()
        .code(2);
    let error: serde_json::Value =
        serde_json::from_slice(&out.get_output().stderr).expect("JSON resolution error");
    let track = &error["report"]["tracks"][0];
    assert_eq!(track["track_id"], "low_solo");
    assert_eq!(track["status"], "rejected");
    assert_eq!(track["best_candidate"]["instrument"], "violin");
    assert_eq!(track["palette"], "solo");
    assert_eq!(track["profile"], "solo-profile");
    assert!(track["resolved"].is_null());
}

#[test]
fn generic_clip_does_not_gain_a_melody_role_fallback_bonus() {
    let dir = tempfile::tempdir().unwrap();
    write_sine_sfz(dir.path());
    let profile = dir.path().join("profile.yaml");
    fs::write(
        &profile,
        "name: lead-only\ninstruments:\n  square_lead:\n    sustain: mini.sfz\n",
    )
    .unwrap();
    let orchestration = write_orchestration_for_profile(dir.path(), &profile);
    let scene = dir.path().join("pad-clip.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 1\nclips:\n  exact:\n    kind: pitched\n    length_beats: 4\n    mode: loop\n    events:\n      held: { at: 0, duration: 4, pitch: C4, velocity: 100 }\ntracks:\n  - { id: pad, instrument: pad, pattern: clip, clip: exact }\n",
    )
    .unwrap();

    let out = bin()
        .args(["--json", "inspect-instruments"])
        .arg(&scene)
        .arg("--orchestration")
        .arg(&orchestration)
        .assert()
        .code(2);
    let error: serde_json::Value =
        serde_json::from_slice(&out.get_output().stderr).expect("resolution error is JSON");
    let report = &error["report"];
    assert_eq!(report["summary"]["fallback"], 0);
    assert_eq!(report["summary"]["missing"], 1);
    assert_eq!(
        report["tracks"][0]["best_candidate"]["instrument"],
        "square_lead"
    );
    assert!(
        report["tracks"][0]["best_candidate"]["score"]
            .as_f64()
            .unwrap()
            < 0.70,
        "report: {report}"
    );
}

/// Alias spellings are pure surface syntax: `french_horn` and `horn` scenes
/// compile to byte-identical MIDI (determinism guarantee).
#[test]
fn alias_and_canonical_scene_produce_identical_midi() {
    let dir = tempfile::tempdir().unwrap();
    let write_scene = |name: &str, instrument: &str| {
        let p = dir.path().join(name);
        fs::write(
            &p,
            format!(
                "tempo: 110\nbars: 2\ntracks:\n  - id: track\n    instrument: {instrument}\n    pattern: sustain\n"
            ),
        )
        .unwrap();
        p
    };
    let canonical = write_scene("canonical.yaml", "horn");
    let alias = write_scene("alias.yaml", "french_horn");
    let compile = |scene: &Path, out_name: &str| {
        let out = dir.path().join(out_name);
        bin()
            .arg("midi")
            .arg(scene)
            .arg("-o")
            .arg(&out)
            .assert()
            .success();
        fs::read(&out).unwrap()
    };
    assert_eq!(
        compile(&canonical, "canonical.mid"),
        compile(&alias, "alias.mid"),
        "alias spelling changed MIDI bytes"
    );
}

/// SF2 backends carry the original 60-instrument core vocabulary exactly.
#[test]
fn build_sf2_resolution_is_all_exact() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("forest.wav");
    bin()
        .arg("build")
        .arg(forest())
        .arg("--soundfont")
        .arg(sf2())
        .arg("-o")
        .arg(&wav)
        .assert()
        .success();
    let meta: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("forest.meta.json")).unwrap()).unwrap();
    let summary = &meta["instrument_resolution"]["summary"];
    assert_eq!(summary["fallback"], 0);
    assert_eq!(summary["missing"], 0);
    assert_eq!(summary["rejected"], 0);
}

#[test]
fn world_instrument_vocabulary_and_tabla_pattern_validate() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("world.yaml");
    fs::write(
        &scene,
        "tempo: 100\nbars: 1\ntracks:\n  - { id: erhu, instrument: erhu, pattern: sustain }\n  - { id: pipa, instrument: pipa, pattern: sustain }\n  - { id: guzheng, instrument: guzheng, pattern: sustain }\n  - { id: dizi, instrument: dizi, pattern: sustain }\n  - { id: shakuhachi, instrument: shakuhachi, pattern: sustain }\n  - { id: shamisen, instrument: shamisen, pattern: sustain }\n  - { id: sitar, instrument: sitar, pattern: sustain }\n  - { id: tabla, instrument: tabla, pattern: tabla }\n  - { id: oud, instrument: oud, pattern: sustain }\n  - { id: ney, instrument: ney, pattern: sustain }\n  - { id: duduk, instrument: duduk, pattern: sustain }\n",
    )
    .unwrap();
    bin().arg("validate").arg(&scene).assert().success();

    let bad = dir.path().join("bad-tabla.yaml");
    fs::write(
        &bad,
        "tempo: 100\nbars: 1\ntracks:\n  - { id: tabla, instrument: tabla, pattern: drums }\n",
    )
    .unwrap();
    let out = bin()
        .args(["--json", "validate"])
        .arg(&bad)
        .assert()
        .code(2);
    let error: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    assert_eq!(error["field"], "tracks[0].pattern");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("requires pattern `tabla`")
    );
}

#[test]
fn tabla_midi_is_a_deterministic_16_beat_channel_10_theka() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("tabla.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 4\ntracks:\n  - { id: tabla, instrument: tabla, pattern: tabla }\n",
    )
    .unwrap();
    let a = dir.path().join("a.mid");
    let b = dir.path().join("b.mid");
    for output in [&a, &b] {
        bin()
            .arg("midi")
            .arg(&scene)
            .arg("-o")
            .arg(output)
            .assert()
            .success();
    }
    let bytes = fs::read(&a).unwrap();
    assert_eq!(bytes, fs::read(&b).unwrap());

    let smf = midly::Smf::parse(&bytes).expect("tabla MIDI parses");
    let mut note_ons = Vec::new();
    let mut programs = 0;
    for track in &smf.tracks {
        let mut tick = 0u32;
        for event in track {
            tick += event.delta.as_int();
            if let midly::TrackEventKind::Midi { channel, message } = event.kind {
                match message {
                    midly::MidiMessage::NoteOn { key, vel } if vel.as_int() > 0 => {
                        note_ons.push((tick, channel.as_int(), key.as_int()));
                    }
                    midly::MidiMessage::ProgramChange { .. } => programs += 1,
                    _ => {}
                }
            }
        }
    }

    let expected_keys = [
        36, 37, 37, 36, 36, 37, 37, 36, 36, 38, 38, 39, 39, 37, 37, 36,
    ];
    assert_eq!(note_ons.len(), expected_keys.len());
    for (index, ((tick, channel, key), expected_key)) in
        note_ons.iter().zip(expected_keys).enumerate()
    {
        assert_eq!(*tick, index as u32 * 480);
        assert_eq!(*channel, 9, "MIDI channel 10 is zero-based channel 9");
        assert_eq!(*key, expected_key);
    }
    assert_eq!(
        programs, 0,
        "profile-only tabla must not emit a false GM program"
    );
}

#[test]
fn midi_rejects_profile_only_melodic_instrument_without_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("erhu.yaml");
    let midi = dir.path().join("erhu.mid");
    fs::write(
        &scene,
        "tempo: 120\nbars: 1\ntracks:\n  - { id: erhu, instrument: erhu, pattern: sustain }\n",
    )
    .unwrap();

    let out = bin()
        .args(["--json", "midi"])
        .arg(&scene)
        .arg("-o")
        .arg(&midi)
        .assert()
        .failure()
        .code(2);
    let error: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    assert_eq!(error["code"], "validation");
    assert_eq!(error["field"], "tracks[0].instrument");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("program 0 (piano)")
    );
    assert!(!midi.exists());
}

#[test]
fn build_sf2_exact_world_programs_render_without_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("gm-world.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 1\ntracks:\n  - { id: shakuhachi, instrument: shakuhachi, pattern: sustain }\n  - { id: sitar, instrument: sitar, pattern: arpeggio }\n  - { id: shamisen, instrument: shamisen, pattern: arpeggio }\n",
    )
    .unwrap();
    let wav = dir.path().join("gm-world.wav");
    bin()
        .arg("build")
        .arg(&scene)
        .arg("--soundfont")
        .arg(sf2())
        .arg("-o")
        .arg(&wav)
        .args(["--tail", "0"])
        .assert()
        .success();

    let (_, samples) = read_frames(&wav);
    assert!(samples.iter().any(|&sample| sample.abs() > 50));
    let meta: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("gm-world.meta.json")).unwrap()).unwrap();
    assert_eq!(meta["instrument_resolution"]["summary"]["exact"], 3);
    assert_eq!(meta["instrument_resolution"]["summary"]["fallback"], 0);
}

#[test]
fn build_sf2_profile_only_world_instrument_fails_without_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("erhu.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 1\ntracks:\n  - { id: erhu, instrument: erhu, pattern: sustain }\n",
    )
    .unwrap();
    let out = bin()
        .args(["--json", "build"])
        .arg(&scene)
        .arg("--soundfont")
        .arg(sf2())
        .arg("-o")
        .arg(dir.path().join("erhu.wav"))
        .assert()
        .code(2);
    let error: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    assert_eq!(error["code"], "resolution");
    assert_eq!(error["report"]["missing_instruments"][0], "erhu");
    assert_eq!(
        error["report"]["tracks"][0]["best_candidate"]["rejected"],
        "world_instrument_requires_exact_source"
    );
    assert_dir_contains_exactly(dir.path(), &["erhu.yaml"]);
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

/// Write a WAV of `frames` mono samples all set to `amplitude`.
fn write_const_wav(path: &Path, amplitude: i16, frames: usize) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for _ in 0..frames {
        writer.write_sample(amplitude).unwrap();
    }
    writer.finalize().unwrap();
}

/// Install a fake `sfizz_render` that serves canned WAVs by invocation count:
/// call N picks `w<N>.wav`, falling back to the last one provided. Lets tests
/// script exactly which render attempts differ.
#[cfg(unix)]
fn install_counted_sfizz(fake_bin: &Path, outputs: &[i16]) {
    fs::create_dir_all(fake_bin).unwrap();
    for (i, amp) in outputs.iter().enumerate() {
        write_const_wav(&fake_bin.join(format!("w{}.wav", i + 1)), *amp, 4410);
    }
    let script = format!(
        "#!/bin/sh\ndir=\"$(cd \"$(dirname \"$0\")\" && pwd)\"\nout=\"\"; prev=\"\"\nfor a in \"$@\"; do\n  [ \"$prev\" = \"--wav\" ] && out=\"$a\"\n  prev=\"$a\"\ndone\nn=$(cat \"$dir/count\" 2>/dev/null || echo 0)\nn=$((n+1)); printf %s \"$n\" > \"$dir/count\"\n[ $n -gt {max} ] && n={max}\ncp \"$dir/w$n.wav\" \"$out\"\n",
        max = outputs.len()
    );
    let tool = fake_bin.join("sfizz_render");
    fs::write(&tool, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[cfg(unix)]
fn install_cc_responsive_sfizz(fake_bin: &Path, controller_hex: &str) {
    fs::create_dir_all(fake_bin).unwrap();
    write_const_wav(&fake_bin.join("low.wav"), 1000, 4410);
    write_const_wav(&fake_bin.join("high.wav"), 2000, 4410);
    let script = format!(
        r#"#!/bin/sh
dir="$(cd "$(dirname "$0")" && pwd)"
midi=""; out=""; prev=""
for a in "$@"; do
  [ "$prev" = "--midi" ] && midi="$a"
  [ "$prev" = "--wav" ] && out="$a"
  prev="$a"
done
hex="$(od -An -v -t x1 "$midi" | tr -d ' \n')"
last="$(printf '%s\n' "$hex" | sed -E 's/.*b0{controller_hex}([0-9a-f][0-9a-f]).*/\1/')"
if [ "$last" = "60" ]; then
  cp "$dir/high.wav" "$out"
else
  cp "$dir/low.wav" "$out"
fi
"#
    );
    let tool = fake_bin.join("sfizz_render");
    fs::write(&tool, script).unwrap();
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
fn install_scratch_bounded_sfizz(fake_bin: &Path) {
    fs::create_dir_all(fake_bin).unwrap();
    write_const_wav(&fake_bin.join("low.wav"), 1000, 4410);
    write_const_wav(&fake_bin.join("high.wav"), 2000, 4410);
    let script = r#"#!/bin/sh
dir="$(cd "$(dirname "$0")" && pwd)"
midi=""; out=""; prev=""
for a in "$@"; do
  [ "$prev" = "--midi" ] && midi="$a"
  [ "$prev" = "--wav" ] && out="$a"
  prev="$a"
done
retained="$(find "$(dirname "$out")" -maxdepth 1 -type f -name '*.wav' | wc -l | tr -d ' ')"
if [ "$retained" -ge 2 ]; then
  printf 'retained %s completed probe WAVs\n' "$retained" >&2
  exit 42
fi
hex="$(od -An -v -t x1 "$midi" | tr -d ' \n')"
last="$(printf '%s\n' "$hex" | sed -E 's/.*b001([0-9a-f][0-9a-f]).*/\1/')"
if [ "$last" = "60" ]; then
  cp "$dir/high.wav" "$out"
else
  cp "$dir/low.wav" "$out"
fi
"#;
    let tool = fake_bin.join("sfizz_render");
    fs::write(&tool, script).unwrap();
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
fn install_any_control_responsive_sfizz(fake_bin: &Path) {
    fs::create_dir_all(fake_bin).unwrap();
    write_const_wav(&fake_bin.join("low.wav"), 1000, 4410);
    write_const_wav(&fake_bin.join("high.wav"), 2000, 4410);
    let script = r#"#!/bin/sh
dir="$(cd "$(dirname "$0")" && pwd)"
midi=""; out=""; prev=""
for a in "$@"; do
  [ "$prev" = "--midi" ] && midi="$a"
  [ "$prev" = "--wav" ] && out="$a"
  prev="$a"
done
hex="$(od -An -v -t x1 "$midi" | tr -d ' \n')"
last="$(printf '%s\n' "$hex" | sed -E 's/.*(b001|b00b|b04a|e000)([0-9a-f][0-9a-f]).*/\2/')"
if [ "$last" = "60" ]; then
  cp "$dir/high.wav" "$out"
else
  cp "$dir/low.wav" "$out"
fi
"#;
    let tool = fake_bin.join("sfizz_render");
    fs::write(&tool, script).unwrap();
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn profile_check_probes_shared_melodic_and_percussion_patch_separately() {
    let dir = tempfile::tempdir().unwrap();
    let fake_bin = dir.path().join("fakebin");
    // Melodic pair passes. The percussion pair and its isolated recheck are
    // silent. A path-only deduplication bug would run just the first pair and
    // falsely certify the shared patch.
    install_counted_sfizz(&fake_bin, &[1000, 1000, 0, 0, 0, 0]);
    let work = dir.path().join("work");
    fs::create_dir_all(&work).unwrap();
    fs::write(work.join("shared.sfz"), "<region> sample=unused.wav\n").unwrap();
    let profile = work.join("profile.yaml");
    fs::write(
        &profile,
        "name: mixed\ninstruments:\n  violin:\n    sustain: shared.sfz\n  tabla:\n    sustain: shared.sfz\n",
    )
    .unwrap();

    let out = bin()
        .args(["--json", "profile", "check"])
        .arg(&profile)
        .env("PATH", format!("{}:/usr/bin:/bin", fake_bin.display()))
        .env("TMPDIR", dir.path())
        .assert()
        .failure()
        .code(2);
    let report: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    let report = &report["report"];
    assert_eq!(report["unique_patches"], 1);
    assert_eq!(report["failed"], 1);
    assert_eq!(report["patches"].as_array().unwrap().len(), 1);
    assert_eq!(
        report["patches"][0]["probes"],
        serde_json::json!(["melodic", "percussion"])
    );
    assert_eq!(
        report["patches"][0]["mappings"],
        serde_json::json!(["violin.sustain", "tabla.sustain"])
    );
    assert_eq!(report["patches"][0]["status"], "silent");
    assert_eq!(fs::read_to_string(fake_bin.join("count")).unwrap(), "6");
}

#[test]
fn profile_check_reports_render_sha256_golden_hash() {
    let dir = tempfile::tempdir().unwrap();
    let profile = write_test_profile(dir.path());
    let out = bin()
        .args(["--json", "profile", "check"])
        .arg(&profile)
        .env("PATH", sfizz_path_env())
        .env("TMPDIR", dir.path())
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    let hash = v["patches"][0]["render_sha256"].as_str().unwrap();
    assert_eq!(hash.len(), 64, "render_sha256 must be hex SHA-256");
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(v["patches"][0].get("flake_diagnostics").is_none());
}

#[cfg(unix)]
#[test]
fn profile_check_legacy_mapping_keeps_the_original_probe_contract() {
    let dir = tempfile::tempdir().unwrap();
    let fake_bin = dir.path().join("fakebin");
    install_counted_sfizz(&fake_bin, &[1000]);
    let work = dir.path().join("work");
    fs::create_dir_all(&work).unwrap();
    fs::write(work.join("bass.sfz"), "<region> sample=unused.wav\n").unwrap();
    let profile = work.join("profile.yaml");
    fs::write(
        &profile,
        "name: legacy\ninstruments:\n  synth_bass:\n    sustain: bass.sfz\n",
    )
    .unwrap();
    let scratch = dir.path().join("scratch");
    fs::create_dir_all(&scratch).unwrap();

    let out = bin()
        .args(["--json", "profile", "check"])
        .arg(&profile)
        .env("PATH", format!("{}:/usr/bin:/bin", fake_bin.display()))
        .env("SCOREKIT_TMPDIR", &scratch)
        .assert()
        .success();
    let report: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(report["patches"][0]["status"], "ok");
    assert!(report["patches"][0].get("control_probes").is_none());
    assert_eq!(fs::read_to_string(fake_bin.join("count")).unwrap(), "2");
    assert_dir_contains_exactly(&scratch, &[]);
}

#[cfg(unix)]
#[test]
fn profile_check_rejects_declared_control_that_patch_ignores_without_residue() {
    let dir = tempfile::tempdir().unwrap();
    let fake_bin = dir.path().join("fakebin");
    install_counted_sfizz(&fake_bin, &[1000]);
    let work = dir.path().join("work");
    fs::create_dir_all(&work).unwrap();
    fs::write(work.join("bass.sfz"), "<region> sample=unused.wav\n").unwrap();
    let profile = work.join("profile.yaml");
    fs::write(
        &profile,
        "name: ignores-controls\ninstruments:\n  synth_bass:\n    sustain:\n      path: bass.sfz\n      controls: [cc1]\n",
    )
    .unwrap();
    let scratch = dir.path().join("scratch");
    fs::create_dir_all(&scratch).unwrap();

    let out = bin()
        .args(["--json", "profile", "check"])
        .arg(&profile)
        .env("PATH", format!("{}:/usr/bin:/bin", fake_bin.display()))
        .env("SCOREKIT_TMPDIR", &scratch)
        .assert()
        .code(2);
    let error: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    let patch = &error["report"]["patches"][0];
    assert_eq!(patch["status"], "control_unresponsive");
    let control = &patch["control_probes"][0];
    assert_eq!(control["target"], "cc1");
    assert_eq!(
        control["mappings"],
        serde_json::json!(["synth_bass.sustain"])
    );
    assert_eq!(control["status"], "unresponsive");
    assert_eq!(control["difference_rms_ratio"], 0.0);
    assert_eq!(control["deterministic"], true);
    assert_eq!(control["render_sha256"].as_array().unwrap().len(), 2);
    assert_eq!(
        control["error"],
        "declared control `cc1` produced no measurable PCM change"
    );
    assert_dir_contains_exactly(&scratch, &[]);
}

#[cfg(unix)]
#[test]
fn profile_check_certifies_deterministic_control_response() {
    let dir = tempfile::tempdir().unwrap();
    let fake_bin = dir.path().join("fakebin");
    install_cc_responsive_sfizz(&fake_bin, "01");
    let work = dir.path().join("work");
    fs::create_dir_all(&work).unwrap();
    fs::write(work.join("bass.sfz"), "<region> sample=unused.wav\n").unwrap();
    let profile = work.join("profile.yaml");
    fs::write(
        &profile,
        "name: responsive-controls\ninstruments:\n  synth_bass:\n    sustain:\n      path: bass.sfz\n      controls: [cc1]\n",
    )
    .unwrap();
    let scratch = dir.path().join("scratch");
    fs::create_dir_all(&scratch).unwrap();

    let out = bin()
        .args(["--json", "profile", "check"])
        .arg(&profile)
        .env("PATH", format!("{}:/usr/bin:/bin", fake_bin.display()))
        .env("SCOREKIT_TMPDIR", &scratch)
        .assert()
        .success();
    let report: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    let patch = &report["patches"][0];
    let control = &patch["control_probes"][0];
    assert_eq!(patch["status"], "ok");
    assert_eq!(control["target"], "cc1");
    assert_eq!(control["status"], "ok");
    assert!(control["difference_rms_ratio"].as_f64().unwrap() > 0.1);
    assert_eq!(control["deterministic"], true);
    assert_ne!(patch["render_sha256"], control["render_sha256"][0]);
    assert_ne!(patch["render_sha256"], control["render_sha256"][1]);
    assert_dir_contains_exactly(&scratch, &[]);
}

#[cfg(unix)]
#[test]
fn profile_check_removes_each_render_pair_before_the_next_probe() {
    let dir = tempfile::tempdir().unwrap();
    let fake_bin = dir.path().join("fakebin");
    install_scratch_bounded_sfizz(&fake_bin);
    let work = dir.path().join("work");
    fs::create_dir_all(&work).unwrap();
    fs::write(work.join("bass.sfz"), "<region> sample=unused.wav\n").unwrap();
    let profile = work.join("profile.yaml");
    fs::write(
        &profile,
        "name: bounded-scratch\ninstruments:\n  synth_bass:\n    sustain:\n      path: bass.sfz\n      controls: [cc1]\n",
    )
    .unwrap();
    let scratch = dir.path().join("scratch");
    fs::create_dir_all(&scratch).unwrap();

    bin()
        .args(["profile", "check"])
        .arg(&profile)
        .env("PATH", format!("{}:/usr/bin:/bin", fake_bin.display()))
        .env("SCOREKIT_TMPDIR", &scratch)
        .assert()
        .success();
    assert_dir_contains_exactly(&scratch, &[]);
}

#[cfg(unix)]
#[test]
fn profile_check_probes_every_portable_automation_target() {
    let dir = tempfile::tempdir().unwrap();
    let fake_bin = dir.path().join("fakebin");
    install_any_control_responsive_sfizz(&fake_bin);
    let work = dir.path().join("work");
    fs::create_dir_all(&work).unwrap();
    fs::write(work.join("bass.sfz"), "<region> sample=unused.wav\n").unwrap();
    let profile = work.join("profile.yaml");
    fs::write(
        &profile,
        "name: all-controls\ninstruments:\n  synth_bass:\n    sustain:\n      path: bass.sfz\n      controls: [cc1, cc11, cc74, pitch_bend]\n",
    )
    .unwrap();
    let scratch = dir.path().join("scratch");
    fs::create_dir_all(&scratch).unwrap();

    let out = bin()
        .args(["--json", "profile", "check"])
        .arg(&profile)
        .env("PATH", format!("{}:/usr/bin:/bin", fake_bin.display()))
        .env("SCOREKIT_TMPDIR", &scratch)
        .assert()
        .success();
    let report: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    let controls = report["patches"][0]["control_probes"].as_array().unwrap();
    assert_eq!(
        controls
            .iter()
            .map(|control| control["target"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["cc1", "cc11", "cc74", "pitch_bend"]
    );
    assert!(controls.iter().all(|control| control["status"] == "ok"));
    assert!(controls.iter().all(|control| {
        control["difference_rms_ratio"]
            .as_f64()
            .is_some_and(|ratio| ratio > 0.1)
    }));
    assert_dir_contains_exactly(&scratch, &[]);
}

#[cfg(unix)]
#[test]
fn profile_check_unions_control_requirements_for_a_shared_patch() {
    let dir = tempfile::tempdir().unwrap();
    let fake_bin = dir.path().join("fakebin");
    install_cc_responsive_sfizz(&fake_bin, "4a");
    let work = dir.path().join("work");
    fs::create_dir_all(&work).unwrap();
    fs::write(work.join("shared.sfz"), "<region> sample=unused.wav\n").unwrap();
    let profile = work.join("profile.yaml");
    fs::write(
        &profile,
        "name: shared-controls\ninstruments:\n  synth_bass:\n    sustain:\n      path: shared.sfz\n      controls: [cc1]\n    staccato:\n      path: shared.sfz\n      controls: [cc74]\n",
    )
    .unwrap();
    let scratch = dir.path().join("scratch");
    fs::create_dir_all(&scratch).unwrap();

    let out = bin()
        .args(["--json", "profile", "check"])
        .arg(&profile)
        .env("PATH", format!("{}:/usr/bin:/bin", fake_bin.display()))
        .env("SCOREKIT_TMPDIR", &scratch)
        .assert()
        .code(2);
    let error: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    let patch = &error["report"]["patches"][0];
    assert_eq!(patch["status"], "control_unresponsive");
    let controls = patch["control_probes"].as_array().unwrap();
    assert_eq!(controls.len(), 2);
    assert_eq!(controls[0]["target"], "cc1");
    assert_eq!(
        controls[0]["mappings"],
        serde_json::json!(["synth_bass.sustain"])
    );
    assert_eq!(controls[0]["status"], "unresponsive");
    assert_eq!(controls[0]["difference_rms_ratio"], 0.0);
    assert_eq!(controls[0]["deterministic"], true);
    assert_eq!(controls[0]["render_sha256"].as_array().unwrap().len(), 2);
    assert_eq!(
        controls[0]["error"],
        "declared control `cc1` produced no measurable PCM change"
    );
    assert_eq!(controls[1]["target"], "cc74");
    assert_eq!(
        controls[1]["mappings"],
        serde_json::json!(["synth_bass.staccato"])
    );
    assert_eq!(controls[1]["status"], "ok");
    assert_eq!(controls[1]["difference_rms_ratio"], 0.5);
    assert_eq!(controls[1]["deterministic"], true);
    assert_eq!(controls[1]["render_sha256"].as_array().unwrap().len(), 2);
    assert!(controls[1].get("error").is_none());
    assert_dir_contains_exactly(&scratch, &[]);
}

#[cfg(unix)]
#[test]
fn profile_check_rejects_nondeterministic_control_probe_with_diagnostics() {
    let dir = tempfile::tempdir().unwrap();
    let fake_bin = dir.path().join("fakebin");
    // Base probe passes. Control variant A differs on both its first attempt
    // and isolated recheck, so variant B is never needed.
    install_counted_sfizz(&fake_bin, &[1000, 1000, 1000, 2000, 1000, 2000]);
    let work = dir.path().join("work");
    fs::create_dir_all(&work).unwrap();
    fs::write(work.join("bass.sfz"), "<region> sample=unused.wav\n").unwrap();
    let profile = work.join("profile.yaml");
    fs::write(
        &profile,
        "name: unstable-control\ninstruments:\n  synth_bass:\n    sustain:\n      path: bass.sfz\n      controls: [cc1]\n",
    )
    .unwrap();
    let scratch = dir.path().join("scratch");
    fs::create_dir_all(&scratch).unwrap();

    let out = bin()
        .args(["--json", "profile", "check"])
        .arg(&profile)
        .env("PATH", format!("{}:/usr/bin:/bin", fake_bin.display()))
        .env("SCOREKIT_TMPDIR", &scratch)
        .assert()
        .code(2);
    let error: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    let patch = &error["report"]["patches"][0];
    let control = &patch["control_probes"][0];
    assert_eq!(patch["status"], "control_nondeterministic");
    assert_eq!(patch["deterministic"], false);
    assert_eq!(control["status"], "nondeterministic");
    assert_eq!(control["deterministic"], false);
    assert_eq!(
        control["error"],
        "declared control `cc1` variant a failed certification: control-cc1-a probe renders differ (RMS ratio 1.00000000); isolated recheck failed too"
    );
    assert_eq!(
        patch["flake_diagnostics"][0]["attempt"],
        "control:cc1:a:first"
    );
    assert_eq!(
        patch["flake_diagnostics"][1]["attempt"],
        "control:cc1:a:recheck"
    );
    assert_eq!(fs::read_to_string(fake_bin.join("count")).unwrap(), "6");
    assert_dir_contains_exactly(&scratch, &[]);
}

#[cfg(unix)]
#[test]
fn profile_check_flaky_first_pair_recovers_via_isolated_recheck() {
    let dir = tempfile::tempdir().unwrap();
    let fake_bin = dir.path().join("fakebin");
    // First pair differs (1000 vs 2000) -> failed comparison; recheck pair is
    // stable (1500, 1500) -> load-sensitive flake, overall pass.
    install_counted_sfizz(&fake_bin, &[1000, 2000, 1500, 1500]);
    let work = dir.path().join("work");
    fs::create_dir_all(&work).unwrap();
    let profile = work.join("profile.yaml");
    fs::write(
        &profile,
        "name: flaky\ninstruments:\n  violin:\n    sustain: any.sfz\n",
    )
    .unwrap();
    fs::write(work.join("any.sfz"), "<region> sample=w1.wav\n").unwrap();
    let out = bin()
        .args(["--json", "profile", "check"])
        .arg(&profile)
        .env("PATH", format!("{}:/usr/bin:/bin", fake_bin.display()))
        .env("TMPDIR", dir.path())
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(v["failed"], 0);
    let patch = &v["patches"][0];
    assert_eq!(patch["status"], "ok");
    assert!(patch["render_sha256"].as_str().is_some());
    let warnings = patch["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("load_sensitive_flake")),
        "expected load_sensitive_flake warning, got {warnings:?}"
    );
    let flakes = patch["flake_diagnostics"].as_array().unwrap();
    assert_eq!(flakes.len(), 1);
    assert_eq!(flakes[0]["attempt"], "first");
    assert_eq!(flakes[0]["observed_status"], "nondeterministic");
    let hashes = flakes[0]["render_sha256"].as_array().unwrap();
    assert_ne!(hashes[0], hashes[1], "differing renders must hash apart");
}

#[cfg(unix)]
#[test]
fn profile_check_persistent_nondeterminism_fails_with_both_diagnostics() {
    let dir = tempfile::tempdir().unwrap();
    let fake_bin = dir.path().join("fakebin");
    // Both pairs differ -> hard failure carrying first + recheck evidence.
    install_counted_sfizz(&fake_bin, &[1000, 2000, 1500, 2500]);
    let work = dir.path().join("work");
    fs::create_dir_all(&work).unwrap();
    let profile = work.join("profile.yaml");
    fs::write(
        &profile,
        "name: broken\ninstruments:\n  violin:\n    sustain: any.sfz\n",
    )
    .unwrap();
    fs::write(work.join("any.sfz"), "<region> sample=w1.wav\n").unwrap();
    let out = bin()
        .args(["--json", "profile", "check"])
        .arg(&profile)
        .env("PATH", format!("{}:/usr/bin:/bin", fake_bin.display()))
        .env("TMPDIR", dir.path())
        .assert()
        .code(2);
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    assert_eq!(v["report"]["failed"], 1);
    let patch = &v["report"]["patches"][0];
    assert_eq!(patch["status"], "nondeterministic");
    assert!(patch.get("render_sha256").is_none());
    let flakes = patch["flake_diagnostics"].as_array().unwrap();
    assert_eq!(flakes.len(), 2);
    assert_eq!(flakes[0]["attempt"], "first");
    assert_eq!(flakes[1]["attempt"], "recheck");
    assert_eq!(flakes[1]["observed_status"], "nondeterministic");
}

/// A patch that never decays used to make `sfizz_render` write an unbounded
/// WAV (44 GB observed) because nothing stopped the render. The watchdog must
/// kill the tool at the output-size cap, report a structured render failure,
/// and leave no partial files behind.
#[cfg(unix)]
#[test]
fn profile_check_kills_runaway_render_at_size_cap_without_residue() {
    let dir = tempfile::tempdir().unwrap();
    let fake_bin = dir.path().join("fakebin");
    fs::create_dir_all(&fake_bin).unwrap();
    // Ignores --use-eot and appends 1 MiB chunks forever, like a render whose
    // output power never decays.
    let script = "#!/bin/sh\nout=\"\"; prev=\"\"\nfor a in \"$@\"; do\n  [ \"$prev\" = \"--wav\" ] && out=\"$a\"\n  prev=\"$a\"\ndone\nwhile :; do\n  dd if=/dev/zero bs=1048576 count=1 >> \"$out\" 2>/dev/null\n  sleep 0.05\ndone\n";
    let tool = fake_bin.join("sfizz_render");
    fs::write(&tool, script).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let work = dir.path().join("work");
    fs::create_dir_all(&work).unwrap();
    let profile = work.join("profile.yaml");
    fs::write(
        &profile,
        "name: runaway\ninstruments:\n  violin:\n    sustain: any.sfz\n",
    )
    .unwrap();
    fs::write(work.join("any.sfz"), "<region> sample=*sine\n").unwrap();
    let tmp = dir.path().join("tmp");
    fs::create_dir_all(&tmp).unwrap();
    let out = bin()
        .args(["--json", "profile", "check"])
        .arg(&profile)
        .env("PATH", format!("{}:/usr/bin:/bin", fake_bin.display()))
        .env("TMPDIR", &tmp)
        .env("SCOREKIT_TOOL_MAX_OUTPUT_MB", "1")
        .assert()
        .code(4);
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    assert_eq!(v["code"], "profile_check");
    let patch = &v["report"]["patches"][0];
    assert_eq!(patch["status"], "render_failed");
    let error = patch["error"].as_str().unwrap();
    assert!(
        error.contains("exceeded 1 MiB cap"),
        "error should name the size cap: {error}"
    );
    // No scratch dir, no partial/giant WAV may survive the kill.
    assert_dir_contains_exactly(&tmp, &[]);
}

/// A render that produces no output at all (hung tool) must be killed at the
/// wall-clock timeout instead of blocking `profile check` forever.
#[cfg(unix)]
#[test]
fn profile_check_kills_stuck_render_at_timeout_without_residue() {
    let dir = tempfile::tempdir().unwrap();
    let fake_bin = dir.path().join("fakebin");
    fs::create_dir_all(&fake_bin).unwrap();
    let tool = fake_bin.join("sfizz_render");
    fs::write(&tool, "#!/bin/sh\nsleep 30\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let work = dir.path().join("work");
    fs::create_dir_all(&work).unwrap();
    let profile = work.join("profile.yaml");
    fs::write(
        &profile,
        "name: stuck\ninstruments:\n  violin:\n    sustain: any.sfz\n",
    )
    .unwrap();
    fs::write(work.join("any.sfz"), "<region> sample=*sine\n").unwrap();
    let tmp = dir.path().join("tmp");
    fs::create_dir_all(&tmp).unwrap();
    let out = bin()
        .args(["--json", "profile", "check"])
        .arg(&profile)
        .env("PATH", format!("{}:/usr/bin:/bin", fake_bin.display()))
        .env("TMPDIR", &tmp)
        .env("SCOREKIT_TOOL_TIMEOUT_SECS", "1")
        .assert()
        .code(4);
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    let patch = &v["report"]["patches"][0];
    assert_eq!(patch["status"], "render_failed");
    let error = patch["error"].as_str().unwrap();
    assert!(
        error.contains("no result within 1s"),
        "error should name the timeout: {error}"
    );
    assert_dir_contains_exactly(&tmp, &[]);
}

/// The render invocation must pass `--use-eot` (stop at EndOfTrack — the fix
/// that makes runaway renders impossible by construction), and the probe
/// scratch dir must honor `SCOREKIT_TMPDIR` so temp renders can be pointed at
/// another disk.
#[cfg(unix)]
#[test]
fn profile_check_passes_use_eot_and_honors_scorekit_tmpdir() {
    let dir = tempfile::tempdir().unwrap();
    let fake_bin = dir.path().join("fakebin");
    fs::create_dir_all(&fake_bin).unwrap();
    write_const_wav(&fake_bin.join("w1.wav"), 1000, 4410);
    // Records every argument, then emits a constant WAV (deterministic pass).
    let script = "#!/bin/sh\ndir=\"$(cd \"$(dirname \"$0\")\" && pwd)\"\nout=\"\"; prev=\"\"\nfor a in \"$@\"; do\n  printf '%s\\n' \"$a\" >> \"$dir/args.log\"\n  [ \"$prev\" = \"--wav\" ] && out=\"$a\"\n  prev=\"$a\"\ndone\ncp \"$dir/w1.wav\" \"$out\"\n";
    let tool = fake_bin.join("sfizz_render");
    fs::write(&tool, script).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let work = dir.path().join("work");
    fs::create_dir_all(&work).unwrap();
    let profile = work.join("profile.yaml");
    fs::write(
        &profile,
        "name: eot\ninstruments:\n  violin:\n    sustain: any.sfz\n",
    )
    .unwrap();
    fs::write(work.join("any.sfz"), "<region> sample=*sine\n").unwrap();
    // SCOREKIT_TMPDIR does not exist yet: the check must create it and use it
    // even though TMPDIR points elsewhere.
    let sk_tmp = dir.path().join("sk-tmp");
    let sys_tmp = dir.path().join("sys-tmp");
    fs::create_dir_all(&sys_tmp).unwrap();
    bin()
        .args(["--json", "profile", "check"])
        .arg(&profile)
        .env("PATH", format!("{}:/usr/bin:/bin", fake_bin.display()))
        .env("TMPDIR", &sys_tmp)
        .env("SCOREKIT_TMPDIR", &sk_tmp)
        .assert()
        .success();
    let log = fs::read_to_string(fake_bin.join("args.log")).unwrap();
    let args: Vec<&str> = log.lines().collect();
    assert!(
        args.contains(&"--use-eot"),
        "sfizz_render must be invoked with --use-eot: {args:?}"
    );
    let wav = args
        .iter()
        .position(|a| *a == "--wav")
        .map(|i| args[i + 1])
        .expect("--wav argument recorded");
    assert!(
        Path::new(wav).starts_with(&sk_tmp),
        "probe render {wav} must live under SCOREKIT_TMPDIR {}",
        sk_tmp.display()
    );
    // Scratch cleanup applies to the relocated dir too.
    assert_dir_contains_exactly(&sk_tmp, &[]);
    assert_dir_contains_exactly(&sys_tmp, &[]);
}

// ---- diff: semantic scene comparison (M4) ----

#[test]
fn diff_reports_semantic_changes_and_ignores_formatting() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.yaml");
    let b = dir.path().join("b.yaml");
    fs::write(
        &a,
        "tempo: 100\nbars: 2\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    // Same music, different formatting/key order → empty diff.
    fs::write(
        &b,
        "bars: 2\ntempo: 100\ntracks:\n  - {id: piano, instrument: piano, pattern: sustain}\n",
    )
    .unwrap();
    let out = bin().arg("diff").arg(&a).arg(&b).assert().success();
    assert_eq!(String::from_utf8_lossy(&out.get_output().stdout).trim(), "");

    let c = dir.path().join("c.yaml");
    fs::write(
        &c,
        "tempo: 120\nbars: 2\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n    intensity: 0.9\n",
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
fn diff_matches_clip_events_by_stable_identity() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old.yaml");
    let reordered = dir.path().join("reordered.yaml");
    let changed = dir.path().join("changed.yaml");
    let scene = |events: &str| {
        format!(
            "tempo: 140\nbars: 1\nclips:\n  bass:\n    kind: pitched\n    length_beats: 4\n    mode: loop\n    events:\n{events}tracks:\n  - {{ id: bass, instrument: synth_bass, pattern: clip, clip: bass, intensity: 1 }}\n"
        )
    };
    fs::write(
        &old,
        scene(
            "      hit_a: { at: 0, duration: 0.5, pitch: F1, velocity: 127 }\n      hit_b: { at: 1, duration: 0.5, pitch: C2, velocity: 110 }\n",
        ),
    )
    .unwrap();
    fs::write(
        &reordered,
        scene(
            "      hit_b: { at: 1, duration: 0.5, pitch: C2, velocity: 110 }\n      hit_a: { at: 0, duration: 0.5, pitch: F1, velocity: 127 }\n",
        ),
    )
    .unwrap();
    fs::write(
        &changed,
        scene(
            "      hit_b: { at: 1, duration: 0.5, pitch: C2, velocity: 118 }\n      hit_a: { at: 0, duration: 0.5, pitch: F1, velocity: 127 }\n",
        ),
    )
    .unwrap();

    let out = bin()
        .arg("diff")
        .arg(&old)
        .arg(&reordered)
        .assert()
        .success();
    assert_eq!(String::from_utf8_lossy(&out.get_output().stdout).trim(), "");

    let out = bin()
        .args(["--json", "diff"])
        .arg(&old)
        .arg(&changed)
        .assert()
        .success();
    let changes: serde_json::Value =
        serde_json::from_slice(&out.get_output().stdout).expect("diff is JSON");
    assert_eq!(
        changes,
        serde_json::json!([{
            "op": "~",
            "path": "clips.bass.events.hit_b.velocity",
            "old": "110",
            "new": "118"
        }])
    );
}

#[test]
fn diff_invalid_scene_is_input_error() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.yaml");
    let bad = dir.path().join("bad.yaml");
    fs::write(
        &a,
        "tempo: 100\nbars: 2\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
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
        "tempo: 120\nbars: 1\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();
    fs::write(
        &s2,
        "tempo: 140\nbars: 1\nloop: true\ntracks:\n  - id: strings\n    instrument: strings\n    pattern: sustain\n",
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
fn batch_sfizz_forwards_orchestration_to_each_build() {
    let dir = tempfile::tempdir().unwrap();
    let scene = tiny_sfizz_scene(dir.path());
    let profile = write_test_profile(dir.path());
    let orchestration = write_orchestration_for_profile(dir.path(), &profile);
    let out_dir = dir.path().join("out");

    bin()
        .arg("batch")
        .arg(&scene)
        .args(["--renderer", "sfizz"])
        .arg("--orchestration")
        .arg(&orchestration)
        .arg("--out-dir")
        .arg(&out_dir)
        .args(["--format", "wav"])
        .env("PATH", sfizz_path_env())
        .assert()
        .success();

    let meta: serde_json::Value =
        serde_json::from_slice(&fs::read(out_dir.join("duo.meta.json")).unwrap()).unwrap();
    assert_eq!(meta["orchestration"]["name"], "test-orchestration");
    assert_eq!(
        meta["instrument_resolution"]["tracks"][0]["profile"],
        "test-profile"
    );
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(out_dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["succeeded"], 1);
    assert_eq!(report["failed"], 0);
}

#[test]
fn batch_partial_failure_reports_and_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("good.yaml");
    let bad = dir.path().join("bad.yaml");
    fs::write(
        &good,
        "tempo: 120\nbars: 1\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
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
            "tempo: 120\nbars: 1\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
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
        "tempo: 92\nkey: D_minor\nbars: 2\nloop: true\nharmony: [i, iv, VI, v]\nperformance:\n  humanize: {{ timing_ms: 12, velocity: 8, seed: {seed} }}\n  swing: 0.12\n  legato: true\n  dynamics: {{ start: p, peak: f }}\ntracks:\n  - {{ id: piano, instrument: piano, pattern: arpeggio, intensity: 0.6 }}\n  - {{ id: bass, instrument: bass, pattern: bass, intensity: 0.5 }}\n  - {{ id: drums, instrument: drums, pattern: drums, intensity: 0.5 }}\n"
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
    let base = "tempo: 92\nkey: D_minor\nbars: 4\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: arpeggio\n";
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
        "tempo: 92\nbars: 2\nperformance:\n  swing: 0.9\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
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
        "tempo: 92\nbars: 2\nharmony: [viii]\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
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
        "tempo: 92\nkey: D_minor\nbars: 2\nloop: true\nmotifs:\n  line:\n    - {{ degree: 1, beats: 1 }}\n    - {{ degree: 2, beats: 1 }}\n    - {{ degree: 3, beats: 1 }}\n    - {{ degree: 2, beats: 1 }}\ntracks:\n  - id: violin\n    instrument: violin\n    pattern: melody\n    motif: line\n{spatial}  - id: cello\n    instrument: cello\n    pattern: sustain\n    intensity: 0.5\n"
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
        "tempo: 92\nbars: 2\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n    pan: 1.5\n",
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
        "tempo: 92\nbars: 2\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n    glide: 0.3\n",
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

#[test]
fn heavy_dubstep_reference_scene_is_valid_linted_and_deterministic() {
    let scene = repo("examples/scenes/heavy_dubstep.yaml");
    let grammar = repo("examples/grammars/heavy_dubstep.yaml");
    bin().arg("validate").arg(&scene).assert().success();
    bin()
        .arg("lint")
        .arg(&scene)
        .arg("--grammar")
        .arg(&grammar)
        .assert()
        .success()
        .stdout(predicates::str::contains("ok: conforms to `heavy_dubstep`"));

    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.mid");
    let b = dir.path().join("b.mid");
    for output in [&a, &b] {
        bin()
            .arg("midi")
            .arg(&scene)
            .arg("-o")
            .arg(output)
            .assert()
            .success();
    }
    assert_eq!(
        fs::read(a).unwrap(),
        fs::read(b).unwrap(),
        "the shipped heavy-Dubstep protocol example must remain byte-deterministic"
    );
}

#[test]
fn disco_family_reference_scenes_are_valid_linted_and_palette_explicit() {
    for (style, palette) in [
        ("nu_disco", "nu-disco"),
        ("disco_70s", "seventies"),
        ("disco_funk", "funk"),
        ("disco_italo", "italo"),
        ("disco_house", "house"),
    ] {
        let scene = repo(&format!("examples/scenes/{style}.yaml"));
        let grammar = repo(&format!("examples/grammars/{style}.yaml"));
        bin().arg("validate").arg(&scene).assert().success();
        bin()
            .arg("lint")
            .arg(&scene)
            .arg("--grammar")
            .arg(&grammar)
            .assert()
            .success()
            .stdout(predicates::str::contains(format!(
                "ok: conforms to `{style}`"
            )));

        let authored: serde_yaml_ng::Value =
            serde_yaml_ng::from_slice(&fs::read(&scene).unwrap()).unwrap();
        let tracks = authored["tracks"].as_sequence().unwrap();
        for (index, track) in tracks.iter().enumerate() {
            assert_eq!(
                track["palette"].as_str(),
                Some(palette),
                "{style} tracks[{index}] must explicitly select `{palette}`"
            );
        }
    }
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
        "tempo: 50\nbars: 2\nmotifs:\n  wall:\n    - { degree: 1, beats: 4 }\n    - { degree: 2, beats: 4 }\ntracks:\n  - id: violin\n    instrument: violin\n    pattern: melody\n    motif: wall\n",
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

#[test]
fn lint_measures_section_clip_percussion_and_automation_rules() {
    let dir = tempfile::tempdir().unwrap();
    let scene_yaml = |drum_events: &str, open_value: i16| {
        format!(
            "tempo: 140\nkey: F_minor\nbars: 1\nclips:\n  build_bass:\n    kind: pitched\n    length_beats: 4\n    mode: once\n    events:\n      hit: {{ at: 0, duration: 1, pitch: F1, velocity: 90 }}\n  drop_bass:\n    kind: pitched\n    length_beats: 4\n    mode: loop\n    events:\n      hit: {{ at: 0, duration: 0.5, pitch: F1, velocity: 127 }}\n    automation:\n      mouth:\n        target: cc1\n        points:\n          start: {{ at: 0, value: 8 }}\n          open: {{ at: 0.5, value: {open_value} }}\n          seal: {{ at: 3.5, value: 8 }}\n  drop_drums:\n    kind: percussion\n    length_beats: 4\n    mode: loop\n    events:\n{drum_events}tracks:\n  - {{ id: bass, instrument: synth_bass, pattern: clip, clip: build_bass, intensity: 1 }}\n  - {{ id: drums, instrument: drums, pattern: clip, clip: drop_drums, intensity: 1 }}\nsections:\n  - {{ name: build, bars: 1, mute: [drums] }}\n  - name: drop\n    bars: 2\n    clips: {{ bass: drop_bass }}\n"
        )
    };
    let valid_drums = "      kick_1: { at: 0, voice: kick, velocity: 127 }\n      kick_2: { at: 1.5, voice: kick, velocity: 115 }\n      snare: { at: 2, voice: snare, velocity: 127 }\n      hat_1: { at: 0, voice: closed_hat, velocity: 90 }\n      hat_2: { at: 0.5, voice: closed_hat, velocity: 82 }\n      hat_3: { at: 1, voice: closed_hat, velocity: 88 }\n      hat_4: { at: 1.5, voice: closed_hat, velocity: 80 }\n      hat_5: { at: 2, voice: closed_hat, velocity: 94 }\n      hat_6: { at: 2.5, voice: closed_hat, velocity: 84 }\n      hat_7: { at: 3, voice: closed_hat, velocity: 90 }\n";
    let valid = dir.path().join("valid.yaml");
    fs::write(&valid, scene_yaml(valid_drums, 100)).unwrap();
    let grammar = dir.path().join("heavy.yaml");
    fs::write(
        &grammar,
        "name: heavy\nrules:\n  tempo_min: 140\n  tempo_max: 140\nsection_rules:\n  drop:\n    percussion_events_per_bar_min: 10\n    percussion_onsets:\n      - { voice: snare, positions: [2], coverage_min: 1 }\n    automation_activity:\n      - { track: bass, target: cc1, points_per_bar_min: 3, value_span_min: 64 }\n",
    )
    .unwrap();
    bin()
        .arg("lint")
        .arg(&valid)
        .arg("--grammar")
        .arg(&grammar)
        .assert()
        .success()
        .stdout(predicates::str::contains("ok: conforms to `heavy`"));

    let invalid = dir.path().join("invalid.yaml");
    let sparse_drums = "      kick: { at: 0, voice: kick, velocity: 127 }\n      snare: { at: 1.5, voice: snare, velocity: 127 }\n";
    let invalid_yaml =
        scene_yaml(sparse_drums, 30).replace("          open: { at: 0.5, value: 30 }\n", "");
    fs::write(&invalid, invalid_yaml).unwrap();
    let out = bin()
        .args(["--json", "lint"])
        .arg(&invalid)
        .arg("--grammar")
        .arg(&grammar)
        .assert()
        .code(2);
    let error: serde_json::Value =
        serde_json::from_slice(&out.get_output().stderr).expect("lint error is JSON");
    let rules: Vec<&str> = error["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|violation| violation["rule"].as_str())
        .collect();
    assert!(rules.contains(&"percussion_events_per_bar_min"));
    assert!(rules.contains(&"percussion_onsets"));
    assert!(rules.contains(&"automation_points_per_bar_min"));
    assert!(rules.contains(&"automation_value_span_min"));
}

#[test]
fn lint_gm_drum_onsets_do_not_match_overlapping_tabla_keys() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("tabla.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 3\ntracks:\n  - { id: tabla, instrument: tabla, pattern: tabla }\n",
    )
    .unwrap();
    let grammar = dir.path().join("gm-drums.yaml");
    fs::write(
        &grammar,
        "name: gm-drums\nrules:\n  percussion_onsets:\n    - { voice: kick, positions: [0], coverage_min: 1 }\n",
    )
    .unwrap();

    let out = bin()
        .args(["--json", "lint"])
        .arg(&scene)
        .arg("--grammar")
        .arg(&grammar)
        .assert()
        .code(2);
    let error: serde_json::Value =
        serde_json::from_slice(&out.get_output().stderr).expect("lint error is JSON");
    assert!(
        error["violations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|violation| violation["rule"] == "percussion_onsets"),
        "error: {error}"
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

#[test]
fn lint_rejects_more_than_32_required_percussion_onsets() {
    let dir = tempfile::tempdir().unwrap();
    let grammar = dir.path().join("too-many-onsets.yaml");
    let positions = (0..33)
        .map(|index| format!("{}", f64::from(index) / 8.0))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        &grammar,
        format!(
            "name: too-many-onsets\nrules:\n  percussion_onsets:\n    - {{ voice: kick, positions: [{positions}], coverage_min: 0 }}\n"
        ),
    )
    .unwrap();

    let out = bin()
        .args(["--json", "lint"])
        .arg(forest())
        .arg("--grammar")
        .arg(&grammar)
        .assert()
        .code(2);
    let error: serde_json::Value =
        serde_json::from_slice(&out.get_output().stderr).expect("validation error is JSON");
    assert_eq!(error["code"], "validation");
    assert_eq!(error["field"], "rules.percussion_onsets[0].positions");
    assert!(error["message"].as_str().unwrap().contains("limit 32"));
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
    let schema = serde_json::to_string(&v).unwrap();
    assert!(
        schema.contains("\"controls\"")
            && schema.contains("\"cc1\"")
            && schema.contains("\"pitch_bend\""),
        "renderer-profile schema must publish automation capability mappings"
    );
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
    for expected in [
        "doctor",
        "validate",
        "schema",
        "lint",
        "build",
        "diff",
        "inspect_instruments",
        "orchestration_check",
        "inspect_textures",
        "texture_check",
    ] {
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

/// The MCP surface must expose texture discovery, otherwise an agent driving
/// scorekit over MCP is back to guessing source names from a bare path map.
#[test]
fn mcp_exposes_texture_inspect_and_check() {
    let dir = tempfile::tempdir().unwrap();
    write_texture_wave(&dir.path().join("river.wav"), 180.0, 0.4);
    write_texture_wave(&dir.path().join("birds.wav"), 900.0, 0.2);
    let profile = dir.path().join("textures.yaml");
    fs::write(&profile, river_birds_profile()).unwrap();

    let replies = mcp_roundtrip(&[
        serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "inspect_textures",
                       "arguments": {"profile": profile.to_str().unwrap(),
                                     "category": "organic",
                                     "tags": ["water"]}}}),
    ]);
    assert_eq!(replies.len(), 2);

    let tools = replies[0]["result"]["tools"].as_array().unwrap();
    let find = |name: &str| tools.iter().find(|tool| tool["name"] == name).unwrap();
    let inspect = find("inspect_textures");
    for arg in ["profile", "category", "tags", "mode", "use_case"] {
        assert!(
            inspect["inputSchema"]["properties"][arg].is_object(),
            "inspect_textures must accept {arg}: {inspect}"
        );
    }
    assert!(find("texture_check")["inputSchema"]["properties"]["profile"].is_object());

    assert_eq!(replies[1]["result"]["isError"], false);
    let text = replies[1]["result"]["content"][0]["text"].as_str().unwrap();
    let report: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(report["status"], "match");
    let names: Vec<&str> = report["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["source"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["river"], "conjunctive filter over MCP: {report}");
}

#[test]
fn mcp_rejects_malformed_texture_filters() {
    let replies = mcp_roundtrip(&[
        serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {"name": "inspect_textures",
                       "arguments": {"profile": "textures.yaml",
                                     "category": 7}}}),
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "inspect_textures",
                       "arguments": {"profile": "textures.yaml",
                                     "tags": "water"}}}),
    ]);
    assert_eq!(replies.len(), 2);
    for reply in &replies {
        assert_eq!(reply["error"]["code"], -32602);
    }
    assert!(
        replies[0]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("category")
    );
    assert!(
        replies[1]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("array of strings")
    );
}

// ---- textures: source discovery and certification (M11) ----

/// The published schema is the contract an agent reads before authoring a
/// profile. It must show that a source is a structured object with required
/// discovery metadata, and enumerate the closed category vocabulary — not
/// leave the agent to infer either from examples.
#[test]
fn texture_profile_schema_publishes_required_discovery_metadata() {
    let out = bin()
        .args(["--json", "schema", "--texture-profile"])
        .assert()
        .success();
    let schema: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();

    assert_eq!(schema["properties"]["schema_version"]["const"], 1);
    assert_eq!(
        schema["properties"]["sources"]["additionalProperties"]["$ref"],
        "#/$defs/TextureSourceBinding"
    );
    let binding = schema["$defs"]["TextureSourceBinding"]["anyOf"]
        .as_array()
        .unwrap();
    assert!(
        binding.iter().any(|variant| variant["type"] == "string")
            && binding
                .iter()
                .any(|variant| variant["$ref"] == "#/$defs/TextureSource"),
        "the additive schema must preserve path-only profiles while publishing the structured form: {schema}"
    );
    assert!(
        !schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == "schema_version"),
        "adding schema_version must not invalidate legacy profiles: {schema}"
    );

    let source = &schema["$defs"]["TextureSource"];
    let required: Vec<&str> = source["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for field in [
        "path",
        "description",
        "category",
        "tags",
        "playback",
        "use_cases",
        "provenance",
    ] {
        assert!(
            required.contains(&field),
            "{field} must be required: {source}"
        );
    }
    assert_eq!(source["additionalProperties"], false);

    // Each category is published with its meaning inline, so an agent can pick
    // the right one without reverse-engineering it from existing profiles.
    let variants = schema["$defs"]["Category"]["oneOf"].as_array().unwrap();
    let categories: Vec<&str> = variants
        .iter()
        .map(|v| v["const"].as_str().unwrap())
        .collect();
    assert!(
        categories.contains(&"ambience") && categories.contains(&"foley"),
        "category vocabulary must be published: {categories:?}"
    );
    for variant in variants {
        assert!(
            !variant["description"]
                .as_str()
                .unwrap_or_default()
                .is_empty(),
            "each category must document what it means: {variant}"
        );
    }

    // Physics is measured by `texture check`, never declared here: a
    // hand-written duration is a fact nothing can verify and every re-export
    // silently invalidates.
    for measured in ["duration_seconds", "sample_rate", "channels", "peak"] {
        assert!(
            source["properties"][measured].is_null(),
            "{measured} is measured, not declared: {source}"
        );
    }
}

/// Enumeration is the primitive the flat map could not offer. A large
/// inventory must come back complete and byte-identical across runs: an agent
/// that gets a truncated or reordered list cannot reason about coverage, and
/// non-determinism would break the project's core guarantee.
#[test]
fn texture_inspect_enumerates_large_inventory_deterministically() {
    let dir = tempfile::tempdir().unwrap();
    let categories = ["ambience", "foley", "impact", "tonal"];
    let mut sources = String::new();
    for i in 0..1200 {
        sources.push_str(&texture_source_yaml(
            &format!("src_{i:04}"),
            &format!("wav/{i:04}.wav"),
            categories[i % categories.len()],
            &["bulk"],
            &["loop"],
            &["stress"],
        ));
    }
    let profile = dir.path().join("textures.yaml");
    fs::write(&profile, texture_profile_yaml("bulk", &sources)).unwrap();

    let run = || {
        let out = bin()
            .args(["--json", "texture", "inspect"])
            .arg(&profile)
            .assert()
            .success();
        String::from_utf8(out.get_output().stdout.clone()).unwrap()
    };
    let first = run();
    assert_eq!(first, run(), "inspect output must be byte-identical");

    let report: serde_json::Value = serde_json::from_str(&first).unwrap();
    assert_eq!(report["total"], 1200);
    assert_eq!(report["matched"], 1200);
    let names: Vec<&str> = report["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["source"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 1200, "no truncation");
    assert_eq!(names[0], "src_0000");
    assert_eq!(names[1199], "src_1199");

    // Full metadata travels with every entry, so one call is enough to choose
    // a source rather than probing each candidate separately.
    let entry = &report["sources"][0];
    assert_eq!(entry["category"], "ambience");
    assert_eq!(entry["playback"]["modes"][0], "loop");
    assert!(entry["description"].is_string());
    assert!(
        entry["resolved_path"]
            .as_str()
            .unwrap()
            .ends_with("0000.wav")
    );
}

/// Filters are exact and conjunctive by design: scorekit answers "which
/// sources satisfy all of these constraints", never "which is most similar".
/// A truthful `no_match` is the useful answer; a plausible wrong pick is not.
#[test]
fn texture_inspect_filters_exactly_and_admits_no_match() {
    let dir = tempfile::tempdir().unwrap();
    let sources = format!(
        "{}{}{}",
        texture_source_yaml(
            "rain_soft",
            "rain.wav",
            "ambience",
            &["rain", "soft"],
            &["loop"],
            &["night"]
        ),
        texture_source_yaml(
            "rain_hit",
            "hit.wav",
            "impact",
            &["rain", "sharp"],
            &["one_shot"],
            &["night"]
        ),
        texture_source_yaml(
            "door",
            "door.wav",
            "foley",
            &["wood"],
            &["one_shot"],
            &["interior"]
        ),
    );
    let profile = dir.path().join("textures.yaml");
    fs::write(&profile, texture_profile_yaml("mixed", &sources)).unwrap();

    let names = |args: &[&str]| -> Vec<String> {
        let out = bin()
            .args(["--json", "texture", "inspect"])
            .arg(&profile)
            .args(args)
            .assert()
            .success();
        let report: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
        report["sources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["source"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(names(&["--tag", "rain"]), ["rain_hit", "rain_soft"]);
    assert_eq!(names(&["--category", "impact"]), ["rain_hit"]);
    assert_eq!(names(&["--mode", "loop"]), ["rain_soft"]);
    assert_eq!(names(&["--use-case", "interior"]), ["door"]);
    // Repeated --tag intersects; it does not widen the result set.
    assert_eq!(
        names(&["--tag", "rain", "--tag", "soft"]),
        ["rain_soft"],
        "multiple tags must be conjunctive"
    );

    // An unsatisfiable query is a legitimate answer, not a failure: like
    // `diff`, reporting the absence of a match is not an error.
    let out = bin()
        .args([
            "--json", "texture", "inspect", "--tag", "wood", "--mode", "loop",
        ])
        .arg(&profile)
        .assert()
        .success();
    let report: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(report["status"], "no_match");
    assert_eq!(report["matched"], 0);
    assert_eq!(report["total"], 3);

    // A category outside the closed vocabulary is a typo, not an empty result
    // set — returning nothing would silently hide the mistake.
    let out = bin()
        .args(["--json", "texture", "inspect", "--category", "ambient"])
        .arg(&profile)
        .assert()
        .failure()
        .code(2);
    let err: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    assert_eq!(err["code"], "validation");
    assert_eq!(err["field"], "--category");
    assert!(
        err["message"].as_str().unwrap().contains("ambience"),
        "message must list the valid vocabulary: {err}"
    );
}

/// Existing path-only profiles remain valid for the build behavior published
/// before source discovery existed. Discovery refuses to invent metadata for
/// them, so compatibility does not turn into a plausible but dishonest catalog.
#[test]
fn legacy_texture_profile_builds_but_discovery_requires_metadata() {
    let dir = tempfile::tempdir().unwrap();
    write_texture_wave(&dir.path().join("river.wav"), 180.0, 0.5);
    let profile = dir.path().join("textures.yaml");
    fs::write(&profile, "name: legacy\nsources:\n  river: river.wav\n").unwrap();
    let scene = dir.path().join("scene.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 1\ntextures:\n  - { source: river, mode: loop, gain: 0.25 }\ntracks:\n  - { id: piano, instrument: piano, pattern: sustain }\n",
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
        .arg("-o")
        .arg(&output)
        .assert()
        .success();
    assert!(output.is_file());

    let out = bin()
        .args(["--json", "texture", "inspect"])
        .arg(&profile)
        .assert()
        .failure()
        .code(2);
    let error: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    assert_eq!(error["code"], "validation");
    assert_eq!(error["field"], "sources.river");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("legacy binding")
    );
}

/// Structured profiles must carry complete discovery metadata. The additive
/// schema version field defaults to v1 when omitted, but unsupported versions
/// and incomplete structured entries still fail with precise field paths.
#[test]
fn texture_profile_rejects_incomplete_discovery_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let profile = dir.path().join("textures.yaml");
    let complete = texture_source_yaml(
        "river",
        "river.wav",
        "organic",
        &["water"],
        &["loop"],
        &["forest"],
    );

    let reject = |yaml: String, expect: &str| {
        fs::write(&profile, &yaml).unwrap();
        let out = bin()
            .args(["--json", "texture", "inspect"])
            .arg(&profile)
            .assert()
            .failure()
            .code(2);
        let err: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
        let code = err["code"].as_str().unwrap();
        assert!(
            code == "validation" || code == "parse",
            "expected a structured input rejection, got {code}: {err}"
        );
        let detail = format!(
            "{} {}",
            err["field"].as_str().unwrap_or_default(),
            err["message"].as_str().unwrap_or_default()
        );
        assert!(
            detail.contains(expect),
            "rejection {detail:?} should name {expect:?} for:\n{yaml}"
        );
    };

    fs::write(&profile, format!("name: additive\nsources:\n{complete}")).unwrap();
    bin()
        .args(["texture", "inspect"])
        .arg(&profile)
        .assert()
        .success();

    reject(
        format!("schema_version: 2\nname: future\nsources:\n{complete}"),
        "schema_version",
    );
    reject(
        texture_profile_yaml(
            "p",
            &complete.replace("category: organic", "category: ambient"),
        ),
        "category",
    );
    reject(
        texture_profile_yaml(
            "p",
            &complete.replace("    description: Test recording river\n", ""),
        ),
        "description",
    );
    reject(
        texture_profile_yaml("p", &complete.replace("tags: [water]", "tags: []")),
        "tags",
    );
    reject(
        texture_profile_yaml(
            "p",
            &complete.replace("      default_mode: loop", "      default_mode: one_shot"),
        ),
        "default_mode",
    );
    reject(
        texture_profile_yaml(
            "p",
            &complete.replace("use_cases: [forest]", "use_cases: [Forest]"),
        ),
        "use_cases",
    );
    reject(
        texture_profile_yaml(
            "p",
            &complete.replace("      library: test-fixtures@1.0.0\n", ""),
        ),
        "provenance",
    );
    reject(
        texture_profile_yaml("p", &complete.replace("path: river.wav", "path: \"\"")),
        "path",
    );
    reject(
        texture_profile_yaml(
            "p",
            &complete.replace("library: test-fixtures@1.0.0", "library: unversioned"),
        ),
        "provenance.library",
    );
}

/// A profile declares intent; `texture check` measures physics. The command
/// exists so an agent can prove a source exists, decodes and is audible before
/// a scene depends on it, instead of discovering the problem mid-build.
#[test]
fn texture_check_measures_physics_of_declared_sources() {
    let dir = tempfile::tempdir().unwrap();
    write_texture_wave(&dir.path().join("river.wav"), 180.0, 0.5);
    write_texture_wave(&dir.path().join("birds.wav"), 900.0, 0.25);
    let profile = dir.path().join("textures.yaml");
    fs::write(&profile, river_birds_profile()).unwrap();

    let out = bin()
        .args(["--json", "texture", "check"])
        .arg(&profile)
        .assert()
        .success();
    let report: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(report["profile"], "field-recordings");
    assert_eq!(report["sources"], 2);
    assert_eq!(report["passed"], 2);
    assert_eq!(report["failed"], 0);

    let entries = report["entries"].as_array().unwrap();
    let river = entries.iter().find(|e| e["source"] == "river").unwrap();
    assert_eq!(river["status"], "ok");
    assert_eq!(river["sha256"].as_str().unwrap().len(), 64);
    let duration = river["duration_seconds"].as_f64().unwrap();
    assert!(
        (duration - 0.5).abs() < 0.02,
        "measured duration {duration} should match the 0.5s source"
    );
    assert!(river["peak_abs"].as_u64().unwrap() > 0);
    assert!(river["rms"].as_f64().unwrap() > 0.0);
    // Declared intent travels with the measurement, so one report answers
    // both "is it usable" and "what is it for".
    assert_eq!(river["category"], "organic");
    assert_eq!(river["modes"][0], "loop");

    // A measurement is a fact about the file, so it must be reproducible.
    let again = bin()
        .args(["--json", "texture", "check"])
        .arg(&profile)
        .assert()
        .success();
    let repeat: serde_json::Value = serde_json::from_slice(&again.get_output().stdout).unwrap();
    assert_eq!(report, repeat, "check must be deterministic");
}

/// Certification must fail loudly on missing and silent sources — a silent
/// file is the failure mode that survives every structural check and only
/// surfaces as a missing layer in the finished mix — and must leave no
/// scratch residue behind when it does.
#[test]
fn texture_check_rejects_missing_and_silent_sources_without_residue() {
    let dir = tempfile::tempdir().unwrap();
    write_texture_wave(&dir.path().join("good.wav"), 220.0, 0.3);
    write_const_wav(&dir.path().join("silent.wav"), 0, 6615);
    let sources = format!(
        "{}{}{}",
        texture_source_yaml("good", "good.wav", "tonal", &["tone"], &["loop"], &["test"]),
        texture_source_yaml(
            "gone",
            "gone.wav",
            "foley",
            &["absent"],
            &["one_shot"],
            &["test"]
        ),
        texture_source_yaml(
            "silent",
            "silent.wav",
            "ambience",
            &["quiet"],
            &["loop"],
            &["test"]
        ),
    );
    let profile = dir.path().join("textures.yaml");
    fs::write(&profile, texture_profile_yaml("broken", &sources)).unwrap();

    let scratch = dir.path().join("sk-tmp");
    let out = bin()
        .args(["--json", "texture", "check"])
        .arg(&profile)
        .env("SCOREKIT_TMPDIR", &scratch)
        .assert()
        .failure()
        .code(2);
    let err: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    assert_eq!(err["code"], "texture_check");
    let report = &err["report"];
    assert_eq!(report["passed"], 1);
    assert_eq!(report["failed"], 2);
    let status = |name: &str| -> String {
        report["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["source"] == name)
            .unwrap_or_else(|| panic!("{name} missing from {report}"))["status"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(status("good"), "ok");
    assert_eq!(status("gone"), "missing");
    assert_eq!(status("silent"), "silent");

    // The scratch root is honored and swept, so a corpus-sized check cannot
    // accumulate normalized copies of every source on failure.
    assert_dir_contains_exactly(&scratch, &[]);
}

/// `playback.modes` is a claim about how a recording behaves. Honoring it in
/// the build turns that declaration into an enforced constraint: looping a
/// one-shot bell produces an audible seam no structural check would catch.
#[test]
fn build_rejects_texture_mode_the_profile_does_not_declare() {
    let dir = tempfile::tempdir().unwrap();
    write_texture_wave(&dir.path().join("birds.wav"), 900.0, 0.3);
    write_texture_wave(&dir.path().join("river.wav"), 180.0, 0.5);
    let profile = dir.path().join("textures.yaml");
    fs::write(&profile, river_birds_profile()).unwrap();

    let scene = dir.path().join("scene.yaml");
    fs::write(
        &scene,
        "tempo: 120\nbars: 2\ntextures:\n  - source: birds\n    mode: loop\n\
         tracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
    )
    .unwrap();

    let out = bin()
        .args(["--json", "build"])
        .arg(&scene)
        .arg("--soundfont")
        .arg(sf2())
        .arg("--texture-profile")
        .arg(&profile)
        .arg("-o")
        .arg(dir.path().join("out").join("scene.wav"))
        .assert()
        .failure()
        .code(2);
    let err: serde_json::Value = serde_json::from_slice(&out.get_output().stderr).unwrap();
    assert_eq!(err["code"], "validation");
    assert_eq!(err["field"], "textures[0].mode");
    let message = err["message"].as_str().unwrap();
    assert!(
        message.contains("birds") && message.contains("one_shot"),
        "message must name the source and its declared modes: {message}"
    );
    // Rejection happens before staging, so no directory is even created.
    assert!(!dir.path().join("out").exists(), "no partial artifact");
}

#[test]
fn mcp_exposes_orchestration_across_schema_check_build_and_inspect() {
    let dir = tempfile::tempdir().unwrap();
    let orchestration = write_test_orchestration(dir.path());
    let replies = mcp_roundtrip(&[
        serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "schema", "arguments": {"kind": "orchestration"}}}),
        serde_json::json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {"name": "orchestration_check",
                       "arguments": {"orchestration": orchestration.to_str().unwrap()}}}),
    ]);
    assert_eq!(replies.len(), 3);

    let tools = replies[0]["result"]["tools"].as_array().unwrap();
    let find = |name: &str| tools.iter().find(|tool| tool["name"] == name).unwrap();
    find("orchestration_check");
    let build = find("build");
    assert!(build["inputSchema"]["properties"]["orchestration"].is_object());
    assert!(build["inputSchema"]["properties"]["profile"].is_null());
    let inspect = find("inspect_instruments");
    assert!(inspect["inputSchema"]["properties"]["orchestration"].is_object());
    assert!(inspect["inputSchema"]["properties"]["profile"].is_null());

    let schema_text = replies[1]["result"]["content"][0]["text"].as_str().unwrap();
    let schema: serde_json::Value = serde_json::from_str(schema_text).unwrap();
    assert!(schema["properties"]["palettes"].is_object());

    assert_eq!(replies[2]["result"]["isError"], false);
    let check_text = replies[2]["result"]["content"][0]["text"].as_str().unwrap();
    let check: serde_json::Value = serde_json::from_str(check_text).unwrap();
    assert_eq!(check["name"], "hybrid-test");
    assert_eq!(check["palettes"].as_array().unwrap().len(), 2);
}

#[test]
fn mcp_tool_failure_passes_structured_error_through() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("bad.yaml");
    fs::write(
        &scene,
        "tempo: 999\nbars: 4\ntracks:\n  - id: piano\n    instrument: piano\n    pattern: sustain\n",
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
