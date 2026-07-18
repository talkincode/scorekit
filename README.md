# scorekit

**Describe your music in a few lines of text. Get game-ready audio back.**

scorekit is a music compiler for game and film-style scoring. You (or your AI Agent) write a small YAML file describing a piece — tempo, key, mood of each track — and one command turns it into a seamless loop, sample-aligned stems, and scene variations you can drop straight into a game engine. No DAW, no timeline, no plugins.

```text
scene.yaml ─► validate ─► midi ─► render ─► export ─► scene.ogg + stems/
```

Because the score is plain text, it lives in git next to your code: diff it, review it, roll it back. And because compilation is deterministic, the same scene always produces the same music — your soundtrack is reproducible, forever.

> **ScoreKit is an Agent-oriented music compiler, not an AI music generator.** The creative decisions stay with you and your Agent; scorekit reliably compiles them into assets.

## What you get from one command

- **Seamless loops** — sample-exact length, no click at the loopback point.
- **Stems** — one file per track (strings, piano, bass, drums…), all equal length and sample-aligned, so the engine can layer them dynamically (calm exploration → add drums when combat starts).
- **Scene suites** — intro / explore / combat / victory sections that share the same musical motifs, compiled from a single file.
- **`meta.json`** — exact loop points and sample counts, ready for your engine to consume.

## Install

Prebuilt archives for Linux, macOS (Intel & Apple Silicon), and Windows are on [GitHub Releases](https://github.com/talkincode/scorekit/releases). From a source checkout:

```bash
make install        # binary + Agent skill + a free orchestral SoundFont + sound directories
scorekit doctor     # checks that FluidSynth / FFmpeg are ready, tells you what to install if not
```

You'll need [FluidSynth](https://www.fluidsynth.org/) (`brew install fluid-synth` / `apt install fluidsynth`) and [FFmpeg](https://ffmpeg.org/). `make install` downloads the MIT-licensed MuseScore General SoundFont, so rendering works out of the box. Full details: [Installation manual](https://talkincode.github.io/scorekit/installation.html).

## Your first track

This is a complete scene — a calm 8-bar forest loop with four instruments:

```yaml
# forest.yaml
title: Forest Theme
tempo: 92
key: D_minor
time_signature: "4/4"
bars: 8
loop: true
tracks:
  - instrument: strings
    pattern: sustain
    intensity: 0.4
  - instrument: piano
    pattern: arpeggio
    intensity: 0.55
  - instrument: bass
    pattern: bass
    intensity: 0.45
  - instrument: drums
    pattern: drums
    intensity: 0.3
```

```bash
scorekit build forest.yaml -o forest.ogg --stems
```

Output: `forest.ogg` (loops seamlessly), `forest.stems/01-strings.ogg … 04-drums.ogg`, and `forest.meta.json`. That's the whole workflow.

Want a full scene set? Put `sections:` in one file and get `forest-intro.ogg`, `forest-explore.ogg`, `forest-combat.ogg`, `forest-victory.ogg` — all built on the same motifs. See [forest_suite.yaml](examples/scenes/forest_suite.yaml).

## Let your Agent compose

The intended workflow: you describe the *feeling* ("wistful exile theme, sand dunes at dusk, solo violin over sparse piano"), your coding Agent writes the scene YAML, scorekit compiles it, and the Agent iterates on validation and lint feedback until it's right.

[skills/scorekit/](skills/scorekit/) is an installable Agent skill that teaches any skill-capable Agent the full workflow — DSL reference, composing craft, and the validate → lint → build loop:

```bash
npx skills add talkincode/scorekit           # skills CLI ecosystem
# or copy manually:
cp -r skills/scorekit ~/.claude/skills/      # Claude Code
cp -r skills/scorekit ~/.agents/skills/      # generic agents directory
```

For a complete prompt-to-audio walkthrough, see the [narrative film-score example](skills/scorekit/examples/narrative-film-score.md) with its finished [scene YAML](skills/scorekit/examples/exile-in-the-dunes.yaml).

Prefer MCP? `scorekit mcp` serves the same commands as MCP tools over stdio — register `{ "command": "scorekit", "args": ["mcp"] }` in any MCP client.

## Making it sound human

Two blocks remove the "MIDI demo" feel. `harmony` declares a chord progression (all accompaniment follows it), `performance` adds seeded, reproducible humanity — same seed, same bytes:

```yaml
harmony: [i, iv, VI, v]          # roman numerals, one chord per bar, cycles
performance:
  humanize: { timing_ms: 18, velocity: 10, seed: 7 }
  legato: true                   # melody notes connect smoothly
  swing: 0.12
  dynamics: { start: pp, peak: mf }   # loop-safe dynamics arc
```

## Keeping your project's style consistent

A *grammar profile* turns your project's aesthetic into checkable rules — "grief doesn't rush, grief leaves space" — and `scorekit lint` measures the compiled music against them:

```yaml
# grief.yaml — what loneliness sounds like in this project
rules:
  tempo_max: 60
  melodic_voices_max: 2
  melody_rest_ratio_min: 0.35    # breathing room per voice
  resolution: incomplete         # never lands on the tonic — the question stays open
  harmony_allowed: [i, iv, v, VI, VII]
  require_performance: true
```

```text
$ scorekit lint scene.yaml --grammar grief.yaml
tempo_max @ scene: measured 92, want <= 60
```

Violations report measured values, so an Agent can fix the scene directly. Great as a CI gate for your soundtrack.

## Real instruments (SFZ sample libraries)

The default SoundFont covers sketching; for release-quality audio, switch the renderer to [sfizz](https://sfz.tools/sfizz/) and a free sample library like [VSCO 2 Community Edition](https://vis.versilstudios.com/vsco-community.html) (CC0):

```bash
scorekit build elegy.yaml --renderer sfizz \
    --profile examples/profiles/vsco2-ce.yaml -o elegy.ogg --stems
```

Your scene stays portable — it says `instrument: violin`, and a *renderer profile* (a separate file, per machine) maps that to real `.sfz` sample files. See the [profiles manual](https://talkincode.github.io/scorekit/profiles.html).

## Style gallery

Ready-to-build references in [examples/scenes/](examples/scenes/): [chiptune](examples/scenes/chiptune.yaml) (8-bit), [dance](examples/scenes/dance.yaml), [epic](examples/scenes/epic.yaml), [ballad](examples/scenes/ballad.yaml) (3/4), [elegy](examples/scenes/elegy.yaml) (violin), [dunes](examples/scenes/dunes.yaml) (film-score texture, conforms to the [grief grammar](examples/grammars/grief.yaml)).

```bash
scorekit batch examples/scenes/*.yaml --out-dir out/
```

## For engineers

The technical manual lives at **[talkincode.github.io/scorekit](https://talkincode.github.io/scorekit/)**: [command reference](https://talkincode.github.io/scorekit/commands.html), [rendering & dependencies](https://talkincode.github.io/scorekit/rendering.html), [architecture & guarantees](https://talkincode.github.io/scorekit/architecture.html), and the [machine interface](https://talkincode.github.io/scorekit/machine-interface.html) — the stable contract (exit codes, `--json` errors, schemas, the `scorekit mcp` stdio server) for building integrations on top of the CLI. A pinned [`Dockerfile`](Dockerfile) ships the exact FluidSynth/FFmpeg/SoundFont toolchain for cloud and CI use.

Project profile, non-goals (iron rules), roadmap, and the acceptance matrix binding every tier-1 feature to E2E tests: [docs/roadmap.md](docs/roadmap.md). Contributor hard rules: [AGENTS.md](AGENTS.md).

## License

[MIT](LICENSE)
