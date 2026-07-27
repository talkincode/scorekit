---
name: scorekit
description: >
  Compose and render game music with scorekit, an agent-oriented music
  compiler: write a YAML scene DSL, compile it deterministically to MIDI,
  render seamless loops, stems, and OGG/WAV assets, and lint compositions
  against aesthetic grammar profiles. Use when the user asks for game BGM,
  background music, a music loop, adaptive-music stems, film-style scoring,
  a scene.yaml, or scorekit itself (game score, background music, looping
  music, stems, generated music, composing a piece). Also use it to audit,
  review, or critique an existing arrangement/scene (arrangement audit,
  编曲审计) — standalone or as the post-build self-check.
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
`sf2/MuseScore_General.sf2` by default; sfizz requires an explicit
`--orchestration` profile (see below).
Override the install location with `SCOREKIT_SOUND_LIBRARY_DIR`.
The default SF2 is `sf2/MuseScore_General.sf2`; omit `--soundfont` to use it,
or pass an explicit file to override it.

## Core workflow

1. **Ask the schema, never guess:** `scorekit schema` (scene DSL),
   `scorekit schema --grammar` (grammar profiles),
   `scorekit schema --texture-profile` (ambience/SFX source mappings),
   `scorekit schema --resolver` (instrument-resolver config), `scorekit
   schema --profile` (leaf renderer profiles), and `scorekit schema
   --orchestration` (orchestration profiles) print JSON Schema.
2. **Declare a palette first** ([palettes.md](palettes.md)): name your
   inertia answer, weigh 2–3 candidate palettes against the brief, pick
   one for stated reasons, and note which variation axes differ from your
   recent pieces. Orchestral strings must win this step, never inherit it.
3. **Write the scene** (see cheat sheet below and [reference.md](reference.md)).
4. **Validate:** `scorekit --json validate scene.yaml` — errors are
   machine-readable on stderr with `field` paths and line numbers. Fix and
   repeat until exit 0.
5. **Lint (if the project has grammar profiles):**
   `scorekit lint scene.yaml --grammar grammars/<style>.yaml` — violations
   report measured vs wanted values; edit the scene until it conforms.
6. **Build:**
   ```bash
   scorekit build scene.yaml -o out/scene.ogg
   ```
   Add `--stems` for per-track files in `out/scene.ogg.stems/` (adaptive
   game audio), `--renderer timidity` for the alternate backend. Non-loop
   scenes get a reverb tail (`--tail`, default 4s). A scene declaring
   `textures` also needs `--texture-profile <file>`.
7. **Self-audit:** run the arrangement audit ([audit.md](audit.md)) —
   deterministic gates (G1–G5) plus the craft rubric — and fix or justify
   every finding before reporting completion.
8. **Iterate by ear:** play the file for the user; when revising, keep the
   scene under version control — `scorekit diff old.yaml new.yaml` shows
   semantic changes only.

When the request begins as a story, character, or film scene rather than a
scene YAML, read [examples/narrative-film-score.md](examples/narrative-film-score.md).
Use its prompt-to-brief mapping: keep narrative language in the creative brief,
then translate only deterministic musical decisions into the DSL. The worked
artifact is [examples/exile-in-the-dunes.yaml](examples/exile-in-the-dunes.yaml).

Completion gate: `doctor` is ready, the scene passes `validate`, any requested
grammar passes `lint`, `build` succeeds, and the **arrangement audit**
([audit.md](audit.md)) ran self-check — deterministic gates pass, craft
findings carry a fix or a justification. The response names the scene,
audio, metadata, and stem paths plus the motif/orchestration choices made,
and ends with the audit verdict line.

Batch many scenes: `scorekit batch a.yaml b.yaml --out-dir assets/` →
per-scene results in `assets/report.json`, one failure
doesn't stop the rest.

With `--renderer sfizz`, pass `--orchestration <file>` (maps logical track
`palette`s to certified renderer profiles; see `scorekit schema
--orchestration` and `scorekit orchestration check <file>`). Instruments the
active palette's renderer profile doesn't map resolve through a scored
same-family fallback confined to that one palette (never silently to
strings, never across palettes; exit 2 code `resolution` when nothing
qualifies). Preview with
`scorekit inspect-instruments scene.yaml --orchestration orchestration.yaml`;
tune with `--fallback-mode strict|conservative|flexible` or
`--resolver <config>`. Substitutions print `WARN instrument fallback:` lines
and land in `meta.json` as `instrument_resolution`, alongside each track's
`id`, declared/effective palette, and resolved renderer profile/SFZ path.
An SFZ patch used by clip automation must use the structured leaf mapping
`{path: ..., controls: [cc1, cc11, cc74, pitch_bend]}` and declare every target
the active clips use; missing capability fails before build staging. Run
`scorekit profile check` before wiring the leaf profile: every declaration must
produce two non-silent, deterministic control-probe renders with measurably
different decoded PCM, or certification fails.

Exit codes: `0` ok · `1` io · `2` invalid input / lint violations ·
`3` missing dependency · `4` external tool failed. Global `--json` flag
turns every error into one structured JSON object on stderr. For MCP
clients, `scorekit mcp` serves the same commands as stdio MCP tools — a
pure adapter, same contract as the CLI.

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

clips:                         # exact, stable-ID events for authored rhythm
  talking_bass:
    kind: pitched              # pitched | percussion
    length_beats: 4
    mode: loop                 # once | loop
    events:
      bark_01: { at: 0, duration: 0.5, pitch: F1, velocity: 127 }
    automation:                # step lanes: cc1 | cc11 | cc74 | pitch_bend
      mouth:
        target: cc1
        points:
          shut: { at: 0, value: 0 }
          open: { at: 0.25, value: 127 }
          seal: { at: 3.75, value: 0 }

tracks:
  - { id: lead, instrument: violin, pattern: melody, motif: lament, intensity: 0.65 }
  - { id: motion, instrument: harp,   pattern: arpeggio, intensity: 0.3 }
  - { id: harmony, instrument: slow_strings, pattern: sustain, intensity: 0.35 }  # "pad"
  - { id: foundation, instrument: cello,  pattern: bass, intensity: 0.35 }
  - { id: pulse, instrument: drums,  pattern: drums, intensity: 0.4 }  # drums↔drums only
  # - { id: talker, instrument: synth_bass, pattern: clip, clip: talking_bass }
  # ^ example wiring only — do NOT default to this string-quartet palette.
  #   Pick a palette deliberately from palettes.md (chiptune, jazz-noir,
  #   music-box, synth-ambient, east-asian, …) before writing tracks.
  # optional: palette: solo | ensemble | ... — routes to an orchestration
  # palette (--orchestration). Omitted = the orchestration's default_palette.
  # Routing metadata only; never changes compiled MIDI.

textures:                     # optional field recordings / ambience / SFX
  - { source: river, mode: loop, gain: 0.25 }
  - { source: birds, mode: one_shot, at: [2, 10], gain: 0.5 }
```

Every track needs a stable, unique `id` (`[a-z][a-z0-9_-]{0,63}`) — used by
`sections[].mute`, `midi --solo`, stem file names (`NN-<id>.ext`), and
`meta.json`.

Patterns: `melody` (plays its `motif`) · `sustain` (whole-bar chords) ·
`arpeggio` (broken chords) · `bass` (roots) · `drums` (groove, `drums`
instrument only) · `tabla` (fixed 16-beat theka, `tabla` instrument only) ·
`clip` (exact pitched/percussion events and optional step automation).
Clip/event/lane/point maps use stable IDs, so key reordering is MIDI- and
diff-inert. Beat positions quantize to PPQ 480; loop clips must divide every
active scene/section and automation must return to its initial value. Clip
timing ignores swing/legato/humanize; intensity/dynamics still scale velocity.
The vocabulary contains the 60-instrument core plus 11 exact-source world
identities (`erhu`, `pipa`, `guzheng`, `dizi`, `shakuhachi`, `shamisen`,
`sitar`, `tabla`, `oud`, `ney`, `duduk`). Only shakuhachi, sitar, and shamisen
have exact GM programs; the others require real renderer-profile mappings and
must never be replaced by a related-looking instrument. Core examples include
`piano`, `epiano`, `music_box`, `slow_strings`, `choir`, `voice`,
`pan_flute`, `square_lead`, `warm_pad`, and `choir_pad`; the full table is in
[reference.md](reference.md).
Standalone `scorekit midi` rejects profile-only melodic identities because it
has no renderer-profile input; use an sfizz `build` for those scenes. Tabla is
the channel-10 exception.
`choir`/`voice`/`choir_pad` are sampled vowels (ahh/ooh), not lyrics.

Texture `source` names are portable keys, never paths. Bind them externally
in a profile that also *describes* each source, so you can pick one without
opening the files:

```yaml
schema_version: 1
name: forest
root: /path/to/recordings
sources:
  river:
    path: river.flac
    description: Wide river bed, mid-distance
    category: organic          # closed enum — see schema --texture-profile
    tags: [water, flowing]     # open vocabulary
    playback: { modes: [loop], default_mode: loop }
    use_cases: [forest]
    provenance: { library: field-recordings@2024.1 }
```

**Never guess a source name.** Enumerate and filter first:

```bash
scorekit texture inspect textures.yaml --category organic --tag water
scorekit texture check textures.yaml     # certify: exists, decodes, audible
```

Filters are exact and conjunctive (repeated `--tag` intersects); there is no
similarity ranking, so `no_match` means re-orchestrate or add a real source
— not "pick the closest one". `loop` repeats continuously; `one_shot`
triggers at quarter-note beats, and a mode the profile does not declare in
`playback.modes` is rejected at build time. Keep runtime/world-driven audio
(distance, weather, RPM) in the game engine.

Suites (multi-section pieces sharing motifs — intro/explore/combat/victory)
use `sections:`; see [reference.md](reference.md).

## Composition craft (learned from real scoring sessions)

- **Break palette inertia first.** The examples in this skill lean
  strings because they came from film briefs — that is not a default.
  [palettes.md](palettes.md) is the antidote: name your habitual answer,
  make palettes compete, vary ≥3 axes between consecutive pieces
  (palette, key/mode, tempo class, meter, lead timbre, pulse, harmony
  color, density).
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

For rhythm-first styles, grammar rules can also measure
`percussion_events_per_bar_min`, exact `percussion_onsets`, and
`automation_activity` (point density/value span, optionally by track), with
additional `section_rules` keyed by suite section. Shipped heavy-Dubstep pair:
`examples/scenes/heavy_dubstep.yaml` +
`examples/grammars/heavy_dubstep.yaml`.

When the companion ScoreData corpus is available, use its production identity
instead of a generic synth profile. It routes growl, sub, metal, and drums
through separate leaf profiles and binds the scene's honest industrial texture
roles, including the visibly re-orchestrated `mechanical_riser`:

```bash
SCOREDATA_ROOT=/path/to/ScoreData
scorekit orchestration check "$SCOREDATA_ROOT/profiles/orchestrations/heavy-dubstep.yaml"
scorekit texture check "$SCOREDATA_ROOT/profiles/textures/heavy-dubstep.yaml"
scorekit build examples/scenes/heavy_dubstep.yaml --renderer sfizz \
  --fallback-mode strict \
  --orchestration "$SCOREDATA_ROOT/profiles/orchestrations/heavy-dubstep.yaml" \
  --texture-profile "$SCOREDATA_ROOT/profiles/textures/heavy-dubstep.yaml" \
  --tail 0 --stems -o heavy-dubstep.wav
```

## Arrangement audit (self-check + standalone)

[audit.md](audit.md) is the review protocol: deterministic gates
(`validate`, `lint`, `inspect-instruments`, `texture inspect/check`,
`meta.json` evidence — blocking) plus a measured craft rubric (motif
economy, voice discipline, breathing, emotional curve, role coverage,
register spacing, loop seal, determinism hygiene, story alignment, source
honesty, style independence vs your recent pieces — advisory). Two modes:

- **Self-check:** mandatory after every completed compose/build — part of
  the completion gate above. A failed gate blocks completion; craft
  findings ship only with a fix or a one-line justification.
- **Standalone:** when the user asks to audit/review/critique an existing
  scene (with or without composing anything), run the same protocol and
  deliver the report — `measured` vs `want`, verdict
  `BLOCKED | SHIP WITH NOTES | CLEAN` — without editing the scene.

Recurring craft findings graduate into grammar rules (see above), turning
advisory review into a deterministic gate.

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
Arrangement-audit gates, craft rubric, and report format: [audit.md](audit.md).
Palette catalog, inertia rule, and variation axes: [palettes.md](palettes.md).
