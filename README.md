# scorekit

**ScoreKit is an Agent-oriented Music Compiler, not an AI music generator.**

An Agent-driven game score compiler: text DSL (YAML) → MIDI → swappable render backend (FluidSynth/TiMidity++ + SF2, or sfizz + SFZ sample libraries) → FFmpeg post-processing → game-ready audio assets (seamless loops, split stems, scene transitions). It reliably compiles high-level musical semantics into executable assets; the creative intelligence always belongs to the upstream Agent.

> Status: M0–M8 complete (full pipeline, three render backends, profile certification, environment diagnostics, binary releases, local installation, and English documentation). See [docs/roadmap.md](docs/roadmap.md) for the project profile, non-goals (iron rules), and roadmap.

```text
scene.yaml ─► validate ─► midi ─► render ─► export ─► scene.ogg + stems/
```

## Installation

GitHub Releases publish archives for Linux x86_64/arm64, macOS Intel/Apple Silicon, and Windows x86_64, plus `SHA256SUMS`. A source checkout can install both the binary and the Agent skill locally:

```bash
make install                              # tools + skill + MuseScore General + sound directories
make install PREFIX=/usr/local            # custom binary prefix
make install-skill SKILLS_DIR=~/.codex/skills
scorekit doctor                           # platform + dependency diagnosis
```

The default sound root is `~/.local/share/scorekit/sounds/`, with `sf2/`, `sfz/`, and `profiles/` subdirectories. Override it with `SCOREKIT_SOUND_LIBRARY_DIR=/path/to/sounds`. `make install` downloads the official MIT-licensed `MuseScore_General.sf2` 0.2.0 into `sf2/`; FluidSynth and TiMidity builds use it automatically when `--soundfont` is omitted. Explicit `--soundfont` and sfizz `--profile` arguments always win.

The English mdBook manual lives in [`docs-site/`](docs-site/) and is published by the GitHub Pages workflow.

## Quick start

Dependencies: Rust, FluidSynth (`brew install fluid-synth` / `apt install fluidsynth`), FFmpeg; optional TiMidity++ (second render backend, `--renderer timidity`); optional sfizz (`--renderer sfizz`, sample-level SFZ rendering — not in Homebrew, built by `make install`).

```bash
./scripts/fetch_assets.sh              # download a test GM SoundFont into assets/
./scripts/fetch_default_soundfont.sh   # install MuseScore General into the sound library
cargo build --release
./target/release/scorekit doctor       # verify OS/architecture and external tools

# Validate a scene → one command straight to game assets (seamless loop + stems + metadata)
./target/release/scorekit validate examples/scenes/forest.yaml
./target/release/scorekit build examples/scenes/forest.yaml \
    -o forest.ogg --stems
# Output: forest.ogg (exact loop length, seamless loopback)
#         forest.stems/01-strings.ogg … 04-drums.ogg (sample-aligned with the full mix, dynamically layerable)
#         forest.meta.json (loop_samples/total_samples/stems listing, for the engine and Agent to consume)

# Suite: one scene file → multiple assets (intro/explore/combat/victory) sharing thematic motifs
./target/release/scorekit build examples/scenes/forest_suite.yaml \
    -o forest.ogg
# Output: forest-intro.ogg forest-explore.ogg(loop) forest-combat.ogg(loop, 132bpm)
#         forest-victory.ogg + forest.meta.json (suite manifest)
./target/release/scorekit midi examples/scenes/forest_suite.yaml \
    --section combat -o combat.mid   # compile just one section

# Step by step (--renderer timidity switches to the second backend; asset structure/length unchanged)
./target/release/scorekit midi examples/scenes/forest.yaml -o forest.mid
./target/release/scorekit render forest.mid -o forest.wav
./target/release/scorekit export forest.wav -o forest.ogg

# Sample-level rendering (--renderer sfizz switches to real SFZ instrument libraries,
# e.g. VSCO 2 Community Edition, instead of a single GM SoundFont). A *renderer profile*
# maps the DSL's portable instrument/articulation names to local .sfz files, so scene
# YAML never contains a machine-specific sample-library path.
./target/release/scorekit build examples/scenes/elegy.yaml --renderer sfizz \
    --profile examples/profiles/vsco2-ce.yaml -o elegy.ogg --stems
./target/release/scorekit profile check examples/profiles/vsco2-ce.yaml
./target/release/scorekit schema --profile   # JSON Schema for renderer profiles

# Agent integration
./target/release/scorekit schema       # JSON Schema for the DSL
./target/release/scorekit --json validate scene.yaml   # machine-readable errors (stderr JSON)
./target/release/scorekit diff old.yaml new.yaml       # semantic diff (--json emits a JSON array)
./target/release/scorekit batch a.yaml b.yaml \
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
  --out-dir out/
```

How seamless looping works: a loop scene is rendered twice and the second pass `[L, 2L)` is taken (its start already carries the reverb tail from the first pass); `L` is derived exactly from the quantized MIDI tempo. Because FluidSynth schedules in milliseconds and the actual period drifts, a short crossfade seals the tail so the final frame matches the loopback target bit-for-bit (`--crossfade-ms` is adjustable, default 50ms).

Design stance:

- Determinism above all — the same input must produce a reproducible result; this is the foundation for Agent regression testing.
- Everything is text — the DSL is versioned natively by git diff/merge/rollback; no in-house version control.
- Thin orchestration — no in-house creative DSP (synthesis/reverb/compression), no GUI/DAW, no embedded compositional intelligence.

Renderer profiles: a scene declares only portable semantics (`instrument: violin`, `articulation: pizzicato`); it never names a `.sfz` file or a local install path. A renderer profile (`--profile some.yaml`, only used with `--renderer sfizz`) is the one place that maps those DSL names onto real sample files on *your* machine — see [examples/profiles/vsco2-ce.yaml](examples/profiles/vsco2-ce.yaml) for a full worked example against the free, CC0 [VSCO 2 Community Edition](https://vis.versilstudios.com/vsco-community.html) library, or [examples/profiles/vsco2-vcsl.yaml](examples/profiles/vsco2-vcsl.yaml) for a hybrid that also pulls piano/harp/timpani from [VCSL](https://github.com/sgossner/VCSL) (a CC0 *supplement* to VSCO 2 CE, not a substitute — it has almost no strings/brass/choir of its own, but noticeably better piano/harp/timpani). One profile can mix libraries freely, per instrument. Keeping that mapping out of the scene file is what keeps scenes diff-friendly and shareable across machines. Before using a new or edited profile, run `scorekit profile check <profile.yaml>`: it resolves and deduplicates every mapped patch, renders broad melodic/drum probes at varied velocities twice, rejects missing or silent patches, captures sfizz warnings, verifies repeatability, and removes all temporary probe files. Add global `--json` for a machine-readable certification report.

## Agent Skill (third-party install)

[skills/scorekit/](skills/scorekit/) is an installable Agent skill (`SKILL.md` format): it teaches any skill-capable coding Agent the full scorekit composition workflow — writing the scene DSL, the validate/lint loop, building game assets — plus practical composing experience and a full DSL reference ([reference.md](skills/scorekit/reference.md)).

For a complete prompt-to-audio walkthrough, see the copy-pasteable Chinese Agent prompt and narrative mapping in [the narrative film-score example](skills/scorekit/examples/narrative-film-score.md), backed by a validated [scene YAML](skills/scorekit/examples/exile-in-the-dunes.yaml).

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
