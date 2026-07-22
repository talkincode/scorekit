# scorekit reference

Verified against scorekit v0.3. When in doubt, trust the binary:
`scorekit schema` / `scorekit schema --grammar` /
`scorekit schema --texture-profile` are the live source of truth.
The normative spec (protocol stance, stability rules, compile semantics)
is `docs-site/src/scene-protocol.md`.

## Commands

Global flag: `--json` — every error becomes one JSON object on stderr:
`{"code", "message", "location": {line, column} | null, "field": "path" | null, "exit_code"}`.
Lint failures add `"violations": [{rule, subject, measured, want}]`.

Exit codes: `0` ok · `1` io · `2` invalid input / lint violations · `3` missing dependency · `4` external tool failed.

| Command | Purpose | Flags (default) |
| --- | --- | --- |
| `doctor` | check OS/architecture, FFmpeg, and all render backends | global `--json` emits the full environment report; exit 3 if FFmpeg or every renderer is unavailable |
| `validate <scene>` | check DSL, print summary | — |
| `schema` | JSON Schema of scene DSL | `--grammar` → grammar profile; `--profile` → renderer profile; `--texture-profile` → texture-source profile |
| `profile check <profile>` | certify all explicit SFZ mappings with real probe renders | `--sample-rate` 8000..=384000 (44100); global `--json` emits the full report |
| `lint <scene> --grammar <file>` | check scene against aesthetic grammar | — |
| `midi <scene> -o <out.mid>` | compile to SMF (format 1, PPQ 480) | `--passes` 1..=8 (1), `--solo <track#>`, `--section <name>` |
| `render <mid> -o <out.wav>` | synthesize WAV | `--soundfont <sf2>` (defaults to `$SCOREKIT_SOUND_LIBRARY_DIR/sf2/MuseScore_General.sf2`) **or** `--sfz <file>` (sfizz, single instrument); `--renderer fluidsynth\|timidity\|sfizz` (fluidsynth), `--sample-rate` 8000..=384000 (44100), `--gain` 0.0..=8.0 (0.8, ignored by sfizz) |
| `export <in> -o <out>` | FFmpeg convert (.ogg Vorbis / .wav PCM) | `--quality` 0..=10 (5), `--seek-samples` (0), `--take-samples` |
| `build <scene> -o <out.ogg\|wav>` | full chain + meta.json | default MuseScore General, explicit `--soundfont <sf2>`, **or** `--profile <file>` (sfizz); `--texture-profile <file>` when `textures` are declared; `--renderer fluidsynth\|timidity\|sfizz`; plus `--stems`, `--tail` 0.0..=3600.0 secs (4.0, non-loop), `--crossfade-ms` 0..=60000 (50, loop seal), `--keep-intermediates` |
| `diff <old> <new>` | semantic scene diff (ignores formatting) | — |
| `batch <scenes...> --out-dir <dir>` | build many; report.json; failures don't stop the rest | default MuseScore General, explicit `--soundfont <sf2>`, **or** `--profile <file>` (sfizz); `--format ogg\|wav` (ogg) + render/export flags |

Individual file writes are atomic (temp + rename). Suite builds additionally
stage every section, main asset, stem directory, and manifest as one set; a
failed command leaves the previously published set unchanged.

## Scene DSL

Unknown fields are rejected (typos fail loudly, with line/column).

| Field | Type / range | Default | Notes |
| --- | --- | --- | --- |
| `title` | string | — | optional |
| `story` | string | — | optional; freeform narrative brief (theme, mood, intent) — informational only, echoed into `meta.json` for later agent review |
| `tempo` | 20..=300 | required | BPM |
| `key` | `<Note>_<major\|minor>` | `C_major` | `C_major`, `D_minor`, `F#_minor`, `Eb_major`, … |
| `time_signature` | `N/D`, N 1..=12 | `4/4` | |
| `bars` | 1..=256 | required | length (per scene, or default per section) |
| `loop` | bool | `false` | `true` = seamless loop build; `false` = one-shot + `--tail` |
| `harmony` | `[numeral, …]` | minor `i-VI-III-VII`, major `I-V-vi-IV` | one chord per bar, cycles; diatonic `i..vii` (case-insensitive, triads from scale) |
| `performance` | object | absent | see below; absent = raw compile (bit-stable) |
| `motifs` | `{name: [note, …]}` | `{}` | melodies for `pattern: melody` tracks |
| `textures` | `[texture, …]` | `[]` | field recordings/ambience/SFX; portable source names bind through `--texture-profile` |
| `tracks` | `[track, …]` | required | 1..=16 (≤15 melodic + ≤1 drums) |
| `sections` | `[section, …]` | `[]` | turns the scene into a suite |

### Track

| Field | Type / range | Default | Notes |
| --- | --- | --- | --- |
| `instrument` | enum (below) | required | `drums` instrument ↔ `drums` pattern, exclusively both ways |
| `pattern` | `sustain` `arpeggio` `bass` `drums` `melody` | required | melody plays the named motif, looped/truncated to fill |
| `motif` | motif name | — | required iff `pattern: melody` |
| `intensity` | 0.0..=1.0 | 0.6 | velocity scale |
| `articulation` | `sustain` `staccato` `spiccato` `pizzicato` `tremolo` `mute` | `sustain` | render-time only, no MIDI change; ignored by fluidsynth/timidity, selects the `.sfz` file under `--renderer sfizz --profile ...` (falls back to the instrument's `sustain` mapping if unmapped) |
| `pan` | 0.0..=1.0 | — | stereo position → CC10 (`0` left, `0.5` center, `1` right); omitted = renderer default |
| `reverb` | 0.0..=1.0 | — | reverb send → CC91; omitted = renderer default |
| `glide` | 0.0..=1.0 | — | melody-only tail portamento: the last `glide` fraction of each note pitch-bends toward the next pitch (clamped ±2 semitones); loops bend last→first, seam-safe |

`pan`/`reverb`/`glide` compile to deterministic MIDI (CC10/CC91/pitch-bend).
fluidsynth/timidity honor all three; sfizz honors pitch bend, but CC10/91
only take effect if the `.sfz` maps those CCs.

### Texture track

| Field | Type / range | Default | Notes |
| --- | --- | --- | --- |
| `source` | `[a-z][a-z0-9_-]{0,63}` | required | portable profile key, never a local path |
| `mode` | `loop` \| `one_shot` | required | continuous repetition or full-source triggers |
| `start_beat` | ≥0 | 0 | loop-only; must be 0 if the scene/any section loops |
| `at` | 1..=64 quarter-note beats | — | required for one-shot; schedule repeats per loop pass |
| `gain` | 0.0..=1.0 | 1.0 | linear gain before summation |

Texture profile:

```yaml
name: forest
root: /path/to/recordings
sources:
  river: ambience/river.flac
  birds: wildlife/birds.wav
```

FFmpeg normalizes sources to stereo 16-bit PCM at the build sample rate;
scorekit then performs deterministic placement only. With `--stems`, texture
stems follow instrument stems (`03-texture-river.wav`, etc.) and are the same
exact length. World-driven audio such as positional water, weather, or engine
RPM belongs to the game runtime, not texture tracks.

### Motif note

| Field | Type / range | Notes |
| --- | --- | --- |
| `degree` | -21..=21 | scale step in the scene's key; **0 = rest**; 8 = tonic an octave up; negatives descend below the tonic |
| `beats` | 0.125..=16 | quarter-note beats; split rests longer than 16 into several entries |

### Section (suites)

| Field | Type / range | Default | Notes |
| --- | --- | --- | --- |
| `name` | string | required | unique; asset suffix (`out-<name>.ogg`) |
| `bars` | 1..=256 | required | |
| `tempo` | 20..=300 | scene tempo | per-section override |
| `mute` | `[track index, …]` | `[]` | 0-based; muting all tracks is rejected |
| `intensity` | 0.0..=2.0 | 1.0 | multiplier on every track's intensity |

Sections share the scene's key, tracks, motifs, harmony and performance.
`midi --section <name>` compiles one; `build` emits one asset per section
plus a manifest. A shared texture trigger must fit the shortest section
timeline, so it cannot wrap silently in a shorter cue.

### Performance (all optional, all deterministic)

| Field | Type / range | Effect |
| --- | --- | --- |
| `humanize` | `{timing_ms: 0..=50, velocity: 0..=30, seed: u64}` | seeded jitter; same seed = byte-identical MIDI |
| `swing` | 0.0..=0.5 | delays off-beat eighths |
| `legato` | bool | extends non-drum note durations ~12% |
| `dynamics` | `{start, peak}` of `pp p mp mf f ff` | arch start→peak→start (loop-safe); `mf` ≈ written intensity |

Order applied: swing → dynamics → legato → humanize, before loop
duplication — loop math stays sample-exact.

## Instruments (GM program in parentheses)

- **Keys:** `piano` (0), `bright_piano` (1), `epiano` (4), `harpsichord` (6), `celesta` (8), `organ` (19), `accordion` (21)
- **Mallets/bells:** `glockenspiel` (9), `music_box` (10), `vibraphone` (11), `marimba` (12), `xylophone` (13), `tubular_bells` (14)
- **Guitars:** `guitar` (24), `steel_guitar` (25), `electric_guitar` (27), `muted_guitar` (28)
- **Basses:** `bass` (33), `picked_bass` (34), `fretless_bass` (35), `slap_bass` (36), `synth_bass` (38)
- **Strings:** `violin` (40), `viola` (41), `cello` (42), `contrabass` (43), `tremolo_strings` (44), `pizzicato` (45), `harp` (46), `strings` (48), `slow_strings` (49), `synth_strings` (50)
- **Voices (vowels, no lyrics):** `choir` (52), `voice` (53), `choir_pad` (91)
- **Brass:** `trumpet` (56), `trombone` (57), `tuba` (58), `horn` (60), `brass` (61)
- **Winds:** `sax` (65), `oboe` (68), `english_horn` (69), `bassoon` (70), `clarinet` (71), `piccolo` (72), `flute` (73), `recorder` (74), `pan_flute` (75), `whistle` (78), `ocarina` (79)
- **Synth:** `square_lead` (80), `saw_lead` (81), `pad` (88), `warm_pad` (89), `bowed_pad` (92), `halo_pad` (94), `sweep_pad` (95)
- **Percussion:** `timpani` (47) — pitched; `drums` — GM percussion channel, `pattern: drums` only

## Renderer profiles (`--renderer sfizz`)

sfizz renders real `.sfz` sample libraries (e.g. free CC0 [VSCO 2 Community
Edition](https://vis.versilstudios.com/vsco-community.html)) instead of a
single GM SoundFont — one instrument per invocation; scorekit renders every
track solo and mixes the results in-process, so stems and the full mix are
sample-aligned by construction. Build the binary once with
`scripts/build_sfizz.sh` (not packaged by Homebrew).

A scene never names a `.sfz` file or a local path — only `instrument` +
`articulation` (portable, shareable). A **renderer profile** (external YAML,
`--profile <file>`, only valid with `--renderer sfizz`) is the one place that
maps those DSL names onto real sample files on a given machine:

```yaml
name: vsco2-ce
root: /path/to/VSCO-2-CE-1.1.0   # optional; default = the profile file's own directory
instruments:
  violin:
    sustain: SViolinVib.sfz      # required — fallback for any unmapped articulation
    pizzicato: SViolinPizz.sfz
    tremolo: SViolinTrem.sfz
  drums:
    sustain: GM-StylePerc.sfz
```

Every `Instrument` used by a scene must have an entry with at least a
`sustain` mapping; missing instruments or malformed paths fail loudly at
build time (input error, no partial output). `.sfz` paths are relative to
`root`, and one profile can span multiple sample libraries per-instrument
by prefixing each `.sfz` path with the library's own subfolder name under a
shared `root` (no per-instrument `root:` override exists or is needed). See
`scorekit schema --profile` for the full JSON Schema,
[examples/profiles/vsco2-ce.yaml](../../examples/profiles/vsco2-ce.yaml) for
a complete worked mapping (including orchestral substitutions for
synth/vocal instruments VSCO2 doesn't provide, e.g. `square_lead` → flute),
and [examples/profiles/vsco2-vcsl.yaml](../../examples/profiles/vsco2-vcsl.yaml)
for a hybrid that also pulls piano/harp/epiano/timpani from VCSL (a CC0
supplement library, not a substitute for VSCO2's strings/brass/choir).

Run `scorekit profile check <profile.yaml>` before using a new or changed
profile. The check deduplicates shared patch paths, renders broad melodic or
GM-drum probes at varied velocities twice, rejects missing and silent patches,
captures sfizz warnings, and verifies repeatability. It writes no persistent
audio; command-scoped probe files are removed on success and failure. Use
`scorekit --json profile check <profile.yaml>` to retain a machine-readable
certification report.

## Grammar profiles (`lint`)

External YAML, unknown fields rejected; `name` plus **at least one rule**
required. Surface rules read the scene; deep rules measure the **compiled
score** (after pattern expansion and performance transforms). Suites are
checked per section.

| Rule | Type | Measures |
| --- | --- | --- |
| `tempo_min` / `tempo_max` | BPM | scene tempo (min ≤ max enforced) |
| `pads_max` | count | tracks with `pattern: sustain` |
| `melodic_voices_max` | count | **peak** simultaneous sounding notes across melody tracks (touching notes don't overlap) |
| `melody_rest_ratio_min` | 0.0..=1.0 | **per melody track**: 1 − sounding/total time — each voice's own breathing room |
| `phrase_min_beats` | 0.0..=64 | shortest phrase on any melody track; notes < 2 beats apart merge into one phrase; violations name track + bar |
| `resolution` | `complete` \| `incomplete` | whether the last melody note's pitch class lands on the tonic |
| `harmony_allowed` | `[numeral, …]` | whitelist; scenes without `harmony` are checked against the built-in default progression |
| `require_performance` | bool | scene must have a `performance` block |

Violation format: `{rule} @ {subject}: measured {value}, want {constraint}`
(subject is `scene` or ``section `name` ``); exit 2; `--json` → `violations`
array. Shipped example: `examples/grammars/grief.yaml` (satisfied by
`examples/scenes/dunes.yaml`).

## meta.json

Single scene (`build`):

```json
{
  "title": "...", "story": "...", "loop": true, "tempo": 92, "key": "D_minor",
  "time_signature": "4/4", "bars": 16, "sample_rate": 44100,
  "loop_samples": 1841216, "total_samples": 1841216,
  "crossfade_samples": 2205, "seconds": 41.75,
  "audio": "scene.ogg",
  "stems": ["scene.stems/00-piano.ogg", "..."],
  "tracks": [{ "instrument": "piano", "pattern": "sustain", "intensity": 0.6 }],
  "textures": [{ "source": "river", "mode": "loop", "start_beat": null, "at": [], "gain": 0.25 }]
}
```

Suite: `{"suite": true, "tempo", "key", "time_signature", "sample_rate",
"sections": [ …single-scene entries each with "name"… ]}`.
Loop the file by playing `[0, loop_samples)`; `total_samples` includes the
tail for non-loop scenes. `batch` writes `report.json`:
`{"total", "succeeded", "failed", "items": [{scene, ok, output, meta,
message} | {scene, ok: false, error: {code, message, exit_code}}]}`.

## Seamless-loop internals (why trust the output)

Loop scenes are rendered twice back-to-back and the second pass `[L, 2L)` is
cut out — its head already carries the previous pass's reverb tail. `L` is
derived sample-exactly from the quantized MIDI tempo. A short crossfade
(`--crossfade-ms`, default 50) seals the join bit-exactly against synth
timing drift. Non-loop scenes keep a `--tail` (default 4 s) decay.
