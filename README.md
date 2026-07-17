# scorekit

**ScoreKit is an Agent-oriented Music Compiler, not an AI music generator.**

An Agent-driven game score compiler: text DSL (YAML) → MIDI → swappable render backend (FluidSynth/TiMidity++ + SF2) → FFmpeg post-processing → game-ready audio assets (seamless loops, split stems, scene transitions). It reliably compiles high-level musical semantics into executable assets; the creative intelligence always belongs to the upstream Agent.

> Status: M0–M6 all complete (full pipeline + seamless loops + stems + suites + dual backends + Agent workflow + performance layer/harmony declaration + music grammar linting). See [docs/roadmap.md](docs/roadmap.md) for the project profile, non-goals (iron rules), and roadmap.

```text
scene.yaml ─► validate ─► midi ─► render ─► export ─► scene.ogg + stems/
```

## Quick start

Dependencies: Rust, FluidSynth (`brew install fluid-synth` / `apt install fluidsynth`), FFmpeg; optional TiMidity++ (second render backend, `--renderer timidity`).

```bash
./scripts/fetch_assets.sh              # download a test GM SoundFont into assets/
cargo build --release

# Validate a scene → one command straight to game assets (seamless loop + stems + metadata)
./target/release/scorekit validate examples/scenes/forest.yaml
./target/release/scorekit build examples/scenes/forest.yaml \
    --soundfont assets/TimGM6mb.sf2 -o forest.ogg --stems
# Output: forest.ogg (exact loop length, seamless loopback)
#         forest.stems/01-strings.ogg … 04-drums.ogg (sample-aligned with the full mix, dynamically layerable)
#         forest.meta.json (loop_samples/total_samples/stems listing, for the engine and Agent to consume)

# Suite: one scene file → multiple assets (intro/explore/combat/victory) sharing thematic motifs
./target/release/scorekit build examples/scenes/forest_suite.yaml \
    --soundfont assets/TimGM6mb.sf2 -o forest.ogg
# Output: forest-intro.ogg forest-explore.ogg(loop) forest-combat.ogg(loop, 132bpm)
#         forest-victory.ogg + forest.meta.json (suite manifest)
./target/release/scorekit midi examples/scenes/forest_suite.yaml \
    --section combat -o combat.mid   # compile just one section

# Step by step (--renderer timidity switches to the second backend; asset structure/length unchanged)
./target/release/scorekit midi examples/scenes/forest.yaml -o forest.mid
./target/release/scorekit render forest.mid --soundfont assets/TimGM6mb.sf2 -o forest.wav
./target/release/scorekit export forest.wav -o forest.ogg

# Agent integration
./target/release/scorekit schema       # JSON Schema for the DSL
./target/release/scorekit --json validate scene.yaml   # machine-readable errors (stderr JSON)
./target/release/scorekit diff old.yaml new.yaml       # semantic diff (--json emits a JSON array)
./target/release/scorekit batch a.yaml b.yaml --soundfont game.sf2 \
    --out-dir assets/   # batch render; per-scene results written to assets/report.json, failures don't stop the rest

# Aesthetic regression testing: does a scene conform to the project's "music grammar"?
./target/release/scorekit lint examples/scenes/dunes.yaml \
    --grammar examples/grammars/grief.yaml
./target/release/scorekit schema --grammar   # JSON Schema for grammar profiles
```

Exit codes: `0` success · `1` IO · `2` invalid input · `3` missing dependency · `4` external tool failed.

Removing the "AI feel" — `harmony` declares a chord progression (all accompaniment tracks change together), `performance` does deterministic performance rendering (same seed → same bytes):

```yaml
harmony: [i, iv, VI, v]          # roman numerals, one chord per bar, cycles
performance:
  humanize: { timing_ms: 18, velocity: 10, seed: 7 }  # seeded velocity/timing jitter
  legato: true                   # melody notes connect smoothly
  swing: 0.12                    # swing feel (0..0.5)
  dynamics: { start: pp, peak: mf }  # dynamics arc, same level at start/end, loop-safe
```

Capturing "aesthetics" — a grammar profile turns the project's style system into checkable assertions; `lint` measures against the compiled score (not the YAML surface), reports violations with measured values, so the Agent can fix them directly:

```yaml
# examples/grammars/grief.yaml — what loneliness looks like in this project
name: grief
rules:
  tempo_max: 60                  # grief doesn't rush
  pads_max: 1
  melodic_voices_max: 2          # peak number of simultaneous speaking voices
  melody_rest_ratio_min: 0.35    # each voice's own breathing room (measured per track)
  phrase_min_beats: 5            # no fragmented phrases allowed
  resolution: incomplete         # doesn't land on the tonic at the end — the question stays open
  harmony_allowed: [i, iv, v, VI, VII]
  require_performance: true      # must sound human-performed
```

```text
$ scorekit lint examples/scenes/forest.yaml --grammar examples/grammars/grief.yaml
tempo_max @ scene: measured 92, want <= 60
require_performance @ scene: measured absent, want a `performance` block
error[lint]: 4 grammar violation(s) against `grief`   # exit 2
```

Scene DSL examples in [examples/scenes/](examples/scenes/): [forest.yaml](examples/scenes/forest.yaml) (single scene), [forest_suite.yaml](examples/scenes/forest_suite.yaml) (a suite with motifs/sections), and six style references — [chiptune.yaml](examples/scenes/chiptune.yaml) (8-bit game), [dance.yaml](examples/scenes/dance.yaml) (upbeat dance track), [epic.yaml](examples/scenes/epic.yaml) (light epic), [ballad.yaml](examples/scenes/ballad.yaml) (3/4 ballad), [elegy.yaml](examples/scenes/elegy.yaml) (violin elegy), [dunes.yaml](examples/scenes/dunes.yaml) (film score: single-motif dialogue-style texture, conforms to the [grief grammar](examples/grammars/grief.yaml)). Render them all at once:

```bash
./target/release/scorekit batch examples/scenes/*.yaml \
  --soundfont assets/TimGM6mb.sf2 --out-dir out/
```

How seamless looping works: a loop scene is rendered twice and the second pass `[L, 2L)` is taken (its start already carries the reverb tail from the first pass); `L` is derived exactly from the quantized MIDI tempo. Because FluidSynth schedules in milliseconds and the actual period drifts, a short crossfade seals the tail so the final frame matches the loopback target bit-for-bit (`--crossfade-ms` is adjustable, default 50ms).

Design stance:

- Determinism above all — the same input must produce a reproducible result; this is the foundation for Agent regression testing.
- Everything is text — the DSL is versioned natively by git diff/merge/rollback; no in-house version control.
- Thin orchestration — no in-house DSP, no GUI/DAW, no embedded compositional intelligence.

## Agent Skill (third-party install)

[skills/scorekit/](skills/scorekit/) is an installable Agent skill (`SKILL.md` format): it teaches any skill-capable coding Agent the full scorekit composition workflow — writing the scene DSL, the validate/lint loop, building game assets — plus practical composing experience and a full DSL reference ([reference.md](skills/scorekit/reference.md)).

```bash
npx skills add talkincode/scorekit           # skills CLI ecosystem
# or copy it manually into your Agent's skills directory, e.g.:
cp -r skills/scorekit ~/.claude/skills/      # Claude Code
cp -r skills/scorekit ~/.agents/skills/      # generic agents directory
```

## Quality & acceptance

Every tier-1 feature is bound by the [docs/roadmap.md acceptance matrix](docs/roadmap.md#acceptance-matrix-business-capability-coverage-matrix): every tier-1 feature must have a happy-path E2E test, every high-risk feature must cover its failure paths, and every file-writing operation must verify that a failure leaves no half-finished artifact. Adding a new tier-1 feature requires updating the matrix in the same change. See [AGENTS.md](AGENTS.md) for the full hard rules.

## License

[MIT](LICENSE)
