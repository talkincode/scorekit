# scorekit reference

Verified against scorekit v0.1. When in doubt, trust the binary:
`scorekit schema` / `scorekit schema --grammar` are the live source of truth.

## Commands

Global flag: `--json` — every error becomes one JSON object on stderr:
`{"code", "message", "location": {line, column} | null, "field": "path" | null, "exit_code"}`.
Lint failures add `"violations": [{rule, subject, measured, want}]`.

Exit codes: `0` ok · `1` io · `2` invalid input / lint violations · `3` missing dependency · `4` external tool failed.

| Command | Purpose | Flags (default) |
| --- | --- | --- |
| `validate <scene>` | check DSL, print summary | — |
| `schema` | JSON Schema of scene DSL | `--grammar` → grammar-profile schema instead |
| `lint <scene> --grammar <file>` | check scene against aesthetic grammar | — |
| `midi <scene> -o <out.mid>` | compile to SMF (format 1, PPQ 480) | `--passes` 1..=8 (1), `--solo <track#>`, `--section <name>` |
| `render <mid> --soundfont <sf2> -o <out.wav>` | synthesize WAV | `--renderer fluidsynth\|timidity` (fluidsynth), `--sample-rate` (44100), `--gain` (0.8) |
| `export <in> -o <out>` | FFmpeg convert (.ogg Vorbis / .wav PCM) | `--quality` 0..=10 (5), `--seek-samples` (0), `--take-samples` |
| `build <scene> --soundfont <sf2> -o <out.ogg\|wav>` | full chain + meta.json | render/export flags plus `--stems`, `--tail` secs (4.0, non-loop), `--crossfade-ms` (50, loop seal), `--keep-intermediates` |
| `diff <old> <new>` | semantic scene diff (ignores formatting) | — |
| `batch <scenes...> --soundfont <sf2> --out-dir <dir>` | build many; report.json; failures don't stop the rest | `--format ogg\|wav` (ogg) + render/export flags |

All file writes are atomic (temp + rename): a failed command leaves no
partial output.

## Scene DSL

Unknown fields are rejected (typos fail loudly, with line/column).

| Field | Type / range | Default | Notes |
| --- | --- | --- | --- |
| `title` | string | — | optional |
| `tempo` | 20..=300 | required | BPM |
| `key` | `<Note>_<major\|minor>` | `C_major` | `C_major`, `D_minor`, `F#_minor`, `Eb_major`, … |
| `time_signature` | `N/D`, N 1..=12 | `4/4` | |
| `bars` | 1..=256 | required | length (per scene, or default per section) |
| `loop` | bool | `false` | `true` = seamless loop build; `false` = one-shot + `--tail` |
| `harmony` | `[numeral, …]` | minor `i-VI-III-VII`, major `I-V-vi-IV` | one chord per bar, cycles; diatonic `i..vii` (case-insensitive, triads from scale) |
| `performance` | object | absent | see below; absent = raw compile (bit-stable) |
| `motifs` | `{name: [note, …]}` | `{}` | melodies for `pattern: melody` tracks |
| `tracks` | `[track, …]` | required | 1..=16 (≤15 melodic + ≤1 drums) |
| `sections` | `[section, …]` | `[]` | turns the scene into a suite |

### Track

| Field | Type / range | Default | Notes |
| --- | --- | --- | --- |
| `instrument` | enum (below) | required | `drums` instrument ↔ `drums` pattern, exclusively both ways |
| `pattern` | `sustain` `arpeggio` `bass` `drums` `melody` | required | melody plays the named motif, looped/truncated to fill |
| `motif` | motif name | — | required iff `pattern: melody` |
| `intensity` | 0.0..=1.0 | 0.6 | velocity scale |

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
plus a manifest.

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
  "title": "...", "loop": true, "tempo": 92, "key": "D_minor",
  "time_signature": "4/4", "bars": 16, "sample_rate": 44100,
  "loop_samples": 1841216, "total_samples": 1841216,
  "crossfade_samples": 2205, "seconds": 41.75,
  "audio": "scene.ogg",
  "stems": ["scene.stems/00-piano.ogg", "..."],
  "tracks": [{ "instrument": "piano", "pattern": "sustain", "intensity": 0.6 }]
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
