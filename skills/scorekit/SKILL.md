---
name: scorekit
description: >
  Compose and render game music with scorekit, an agent-oriented music
  compiler: write a YAML scene DSL, compile it deterministically to MIDI,
  render seamless loops, stems, and OGG/WAV assets, and lint compositions
  against aesthetic grammar profiles. Use when the user asks for game BGM,
  background music, a music loop, adaptive-music stems, film-style scoring,
  a scene.yaml, or scorekit itself (game score, background music, looping
  music, stems, generated music, composing a piece).
  Not for singing with lyrics, audio analysis, or editing existing recordings.
---

# scorekit — compile music from a scene DSL

scorekit is a **music compiler, not a music generator**: you (the agent) do
all the composing in a YAML scene file; scorekit deterministically turns it
into MIDI and rendered audio. Same input → byte-identical MIDI, sample-exact
audio. There is no AI inside the tool — the creativity is yours.

Pipeline: `scene.yaml → validate → (lint) → build → .ogg/.wav + meta.json (+ stems/)`

## Setup check

```bash
scorekit doctor || ~/scorekit/target/release/scorekit doctor
```

If missing, install (Rust toolchain required):

```bash
git clone https://github.com/talkincode/scorekit && cd scorekit
make install                     # tools + skill + MuseScore General + sound dirs
# external tools (macOS: brew / Debian: apt)
brew install fluid-synth ffmpeg  # timidity optional (second backend)
```

Any GM-compatible `.sf2` works via `--soundfont`. Exit code 3 = missing
external tool; `scorekit --json doctor` returns the complete dependency and
platform report, including architecture-specific installation help.
The default user-managed sound root is `~/.local/share/scorekit/sounds/`
(`sf2/`, `sfz/`, `profiles/`). FluidSynth and TiMidity use
`sf2/MuseScore_General.sf2` by default; sfizz requires an explicit `--profile`.
Override the install location with `SCOREKIT_SOUND_LIBRARY_DIR`.
The default SF2 is `sf2/MuseScore_General.sf2`; omit `--soundfont` to use it,
or pass an explicit file to override it.

## Core workflow

1. **Ask the schema, never guess:** `scorekit schema` (scene DSL),
   `scorekit schema --grammar` (grammar profiles),
   `scorekit schema --texture-profile` (ambience/SFX source mappings), and
   `scorekit schema --resolver` (instrument-resolver config) print JSON Schema.
2. **Write the scene** (see cheat sheet below and [reference.md](reference.md)).
3. **Validate:** `scorekit --json validate scene.yaml` — errors are
   machine-readable on stderr with `field` paths and line numbers. Fix and
   repeat until exit 0.
4. **Lint (if the project has grammar profiles):**
   `scorekit lint scene.yaml --grammar grammars/<style>.yaml` — violations
   report measured vs wanted values; edit the scene until it conforms.
5. **Build:**
   ```bash
   scorekit build scene.yaml -o out/scene.ogg
   ```
   Add `--stems` for per-track files in `out/scene.ogg.stems/` (adaptive
   game audio), `--renderer timidity` for the alternate backend. Non-loop
   scenes get a reverb tail (`--tail`, default 4s). A scene declaring
   `textures` also needs `--texture-profile <file>`.
6. **Iterate by ear:** play the file for the user; when revising, keep the
   scene under version control — `scorekit diff old.yaml new.yaml` shows
   semantic changes only.

When the request begins as a story, character, or film scene rather than a
scene YAML, read [examples/narrative-film-score.md](examples/narrative-film-score.md).
Use its prompt-to-brief mapping: keep narrative language in the creative brief,
then translate only deterministic musical decisions into the DSL. The worked
artifact is [examples/exile-in-the-dunes.yaml](examples/exile-in-the-dunes.yaml).

Completion gate: `doctor` is ready, the scene passes `validate`, any requested
grammar passes `lint`, `build` succeeds, and the response names the scene,
audio, metadata, and stem paths plus the motif/orchestration choices made.

Batch many scenes: `scorekit batch a.yaml b.yaml --out-dir assets/` →
per-scene results in `assets/report.json`, one failure
doesn't stop the rest.

With `--renderer sfizz`, instruments the profile doesn't map resolve through
a scored same-family fallback (never silently to strings; exit 2 code
`resolution` when nothing qualifies). Preview with
`scorekit inspect-instruments scene.yaml --profile profile.yaml`; tune with
`--fallback-mode strict|conservative|flexible` or `--resolver <config>`.
Substitutions print `WARN instrument fallback:` lines and land in
`meta.json` as `instrument_resolution`.

Exit codes: `0` ok · `1` io · `2` invalid input / lint violations ·
`3` missing dependency · `4` external tool failed. Global `--json` flag
turns every error into one structured JSON object on stderr.

## Scene cheat sheet

```yaml
title: Forest at Dusk          # optional
story: Safe exploration loop   # optional narrative brief; informational only,
                               # echoed into meta.json for later agent review
tempo: 92                      # 20..=300 BPM
key: D_minor                   # <Note>_<major|minor>, e.g. C_major, F#_minor, Eb_major
time_signature: 4/4            # numerator 1..=12
bars: 16                       # 1..=256
loop: true                     # true = seamless loop, false = one-shot + tail

harmony: [i, iv, VI, v]        # roman numerals, one chord per bar, cycles.
                               # default: minor i-VI-III-VII / major I-V-vi-IV

performance:                   # optional, all deterministic (seeded)
  humanize: { timing_ms: 18, velocity: 10, seed: 7 }   # ms 0..=50, vel 0..=30
  legato: true
  swing: 0.12                  # 0.0..=0.5
  dynamics: { start: pp, peak: mf }   # pp..ff; loop scenes: start==end level

motifs:                        # named melodies, referenced by melody tracks
  lament:
    - { degree: 5, beats: 2 }  # degree: scale step, -21..=21, 0 = REST,
    - { degree: 8, beats: 1 }  #   8 = tonic one octave up, negatives go down
    - { degree: 1, beats: 3 }  # beats: 0.125..=16 (split longer rests!)

tracks:
  - { instrument: violin, pattern: melody, motif: lament, intensity: 0.65 }
  - { instrument: harp,   pattern: arpeggio, intensity: 0.3 }
  - { instrument: slow_strings, pattern: sustain, intensity: 0.35 }  # "pad"
  - { instrument: cello,  pattern: bass, intensity: 0.35 }
  - { instrument: drums,  pattern: drums, intensity: 0.4 }  # drums↔drums only

textures:                     # optional field recordings / ambience / SFX
  - { source: river, mode: loop, gain: 0.25 }
  - { source: birds, mode: one_shot, at: [2, 10], gain: 0.5 }
```

Patterns: `melody` (plays its `motif`) · `sustain` (whole-bar chords) ·
`arpeggio` (broken chords) · `bass` (roots) · `drums` (groove, `drums`
instrument only). ~57 GM instruments in snake_case (`piano`, `epiano`,
`music_box`, `slow_strings`, `choir`, `voice`, `pan_flute`, `square_lead`,
`warm_pad`, `choir_pad`, …) — full table in [reference.md](reference.md).
`choir`/`voice`/`choir_pad` are sampled vowels (ahh/ooh), not lyrics.

Texture `source` names are portable keys, never paths. Bind them externally:

```yaml
name: forest
root: /path/to/recordings
sources: { river: river.flac, birds: birds.wav }
```

`loop` repeats continuously; `one_shot` triggers at quarter-note beats. Keep
runtime/world-driven audio (distance, weather, RPM) in the game engine.

Suites (multi-section pieces sharing motifs — intro/explore/combat/victory)
use `sections:`; see [reference.md](reference.md).

## Composition craft (learned from real scoring sessions)

- **Entrances/exits are melody-only.** `sustain`/`arpeggio`/`bass`/`drums`
  fill the whole scene. To bring an instrument in and out mid-piece, give it
  `pattern: melody` and write rests (`degree: 0`) around its material. This
  is how you build dialogue textures and dynamic arcs.
- **One motif, many statements.** Memorable scores restate a single 4–6 note
  motif with variations, they don't pile up new tunes. Vary octave (`+8`),
  rhythm, and which instrument answers.
- **Silence is material.** Leave rests between phrases; a climax can be a
  whole-bar silence. Cap simultaneous melodic voices at ~2.
- **Emotional curve with corners** (e.g. 15%→30%→70%→cut→10%), not a plateau.
  Shape it with intensity, register, and how many tracks are speaking.
- **Pentatonic trick:** in minor keys, restricting melody degrees to
  1/3/4/5/7 (avoid 2 and 6) gives an East-Asian pentatonic color.
- **Loops must seal:** `loop: true` scenes render seamlessly (the tool
  handles it), but keep `dynamics` start level == end level and let the last
  chord want to return to the first.
- **Always seed `humanize`** — same seed = byte-identical output, so takes
  are reproducible and reviewable.
- Long rests: `beats` maxes at 16 per note — split longer silences into
  several `{ degree: 0, beats: … }` entries.

## Aesthetic grammars (style regression tests)

A grammar profile is a YAML constitution of measurable constraints — deep
rules are measured on the **compiled score**, not the YAML surface:

```yaml
name: grief
rules:
  tempo_max: 60
  pads_max: 1                  # counts pattern: sustain tracks
  melodic_voices_max: 2        # peak simultaneous melody voices
  melody_rest_ratio_min: 0.35  # per melody track, its own breathing room
  phrase_min_beats: 5
  resolution: incomplete       # final note must NOT land on the tonic
  harmony_allowed: [i, iv, v, VI, VII]
  require_performance: true
```

`scorekit lint scene.yaml --grammar grief.yaml` → exit 0 or violations like
`tempo_max @ scene: measured 92, want <= 60` (exit 2, `--json` for an array).
Shipped reference pair: `examples/grammars/grief.yaml` +
`examples/scenes/dunes.yaml`. When a user articulates a style ("in this
project, sadness sounds like…"), capture it as a grammar file and lint every
new scene against it — the aesthetic then survives model changes.

## Game asset conventions

- `build` writes `meta.json` next to the audio: exact sample counts, loop
  points, stem listing — feed it to the game engine.
- `--stems` gives per-track and per-texture sample-aligned files for adaptive mixing
  (drop drums when calm, add brass in combat).
- One suite file per game area (shared motifs = one identity), sections for
  states: `scorekit midi scene.yaml -o x.mid --section combat` compiles one
  section; `build` on a suite emits per-section assets.

Full DSL field tables, instrument list, command flags, grammar rule
semantics, and meta.json layout: [reference.md](reference.md).
