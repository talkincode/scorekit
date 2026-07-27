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
- **Exact event clips** — stable-ID pitched/percussion events, section variants, and step automation for arrangements that need authored syncopation, fills, or talking-bass motion.
- **Sound textures** — layer field recordings, ambience, and SFX (water, birds, engines…) as deterministic loops or beat-scheduled one-shots without baking local paths into the scene.
- **`meta.json`** — exact loop points and sample counts, ready for your engine to consume.

## Install

### Recommended (macOS/Linux): Homebrew

```bash
brew install talkincode/tap/scorekit
# Optional: SFZ backend for `--renderer sfizz`
brew install talkincode/tap/scorekit-sfizz
scorekit --version
scorekit doctor
```

Homebrew is the easiest path: `scorekit` installs the prebuilt CLI, FFmpeg, FluidSynth, the bundled Agent skill, and the default MuseScore General SoundFont. Install `scorekit-sfizz` when you also need SFZ rendering (`--renderer sfizz`). If `scorekit doctor` reports all green, you're ready to build scenes.

### Other install options

Prebuilt archives for Linux, macOS (Intel & Apple Silicon), and Windows are on [GitHub Releases](https://github.com/talkincode/scorekit/releases).

From a source checkout:

```bash
make install
scorekit --version
scorekit doctor
```

For source installs you'll need [FluidSynth](https://www.fluidsynth.org/) and [FFmpeg](https://ffmpeg.org/) available on your machine (`brew install fluid-synth ffmpeg` or `apt install fluidsynth ffmpeg`). `make install` downloads the MIT-licensed MuseScore General SoundFont automatically. Full details: [Installation manual](https://talkincode.github.io/scorekit/installation.html).

## Your first track

This is a complete scene — a calm 8-bar forest loop with four instruments:

```yaml
# forest.yaml
title: Forest Theme
story: Calm nocturnal forest for the exploration loop — safe, unhurried.
tempo: 92
key: D_minor
time_signature: "4/4"
bars: 8
loop: true
tracks:
  - id: harmony
    instrument: strings
    pattern: sustain
    intensity: 0.4
  - id: motion
    instrument: piano
    pattern: arpeggio
    intensity: 0.55
  - id: foundation
    instrument: bass
    pattern: bass
    intensity: 0.45
  - id: pulse
    instrument: drums
    pattern: drums
    intensity: 0.3
```

Every track needs a stable `id` (used by stems, `mute`, `--solo`, and `meta.json`).

```bash
scorekit build forest.yaml -o forest.ogg --stems
```

Output: `forest.ogg` (loops seamlessly), `forest.stems/01-harmony.ogg … 04-pulse.ogg`, and `forest.meta.json`. That's the whole workflow.

Want a full scene set? Put `sections:` in one file and get `forest-intro.ogg`, `forest-explore.ogg`, `forest-combat.ogg`, `forest-victory.ogg` — all built on the same motifs — plus `forest.ogg`, the main playback file with every section concatenated in order. See [forest_suite.yaml](examples/scenes/forest_suite.yaml).

## World instruments without fake substitutes

The portable vocabulary also includes `erhu`, `pipa`, `guzheng`, `dizi`,
`shakuhachi`, `shamisen`, `sitar`, `tabla`, `oud`, `ney`, and `duduk`.
MuseScore General supplies exact GM programs for shakuhachi, sitar, and
shamisen. The reference ScoreData profile additionally certifies real CC0
erhu and CC BY 4.0 tabla sources. Every other identity requires an exact local
renderer-profile mapping and fails visibly when absent — scorekit never turns
a guzheng into a koto or an erhu into a generic fiddle.

Tabla is a dedicated channel-10 instrument with `pattern: tabla`, a
deterministic 16-beat theka distinct from the GM drum-kit groove. For a
30-second solo-erhu example, see
[`old_spring_lament.yaml`](examples/scenes/old_spring_lament.yaml).
Standalone `scorekit midi` has no renderer-profile input, so it rejects
profile-only melodic identities before writing a file rather than publish MIDI
that a GM player would interpret as piano. Render those scenes through
`build --renderer sfizz --orchestration <file>` instead; Tabla remains a
channel-10 MIDI instrument.

## Field recordings, ambience, and SFX

Non-instrument sound can be part of the composition too. A scene names
portable sources and schedules them in quarter-note beats:

```yaml
textures:
  - { source: river, mode: loop, gain: 0.25 }
  - { source: forest_birds, mode: one_shot, at: [2, 14, 27.5], gain: 0.5 }
```

A separate machine-local profile maps those names to recordings — and
describes each one, so an agent can choose without opening the files:

```yaml
# textures.yaml
schema_version: 1
name: forest-field-recordings
root: /Volumes/Samples
sources:
  river:
    path: ambience/river.flac
    description: Wide river bed, mid-distance, no bird calls
    category: organic
    tags: [water, flowing, continuous]
    playback: { modes: [loop], default_mode: loop }
    use_cases: [forest, travel]
    provenance: { library: field-recordings@2024.1 }
  forest_birds:
    path: wildlife/birds.wav
    description: Single dawn chorus swell, ends on silence
    category: organic
    tags: [wildlife, chirping]
    playback: { modes: [one_shot], default_mode: one_shot }
    use_cases: [forest, dawn]
    provenance: { library: field-recordings@2024.1 }
```

Existing path-only bindings such as `river: ambience/river.flac` remain
build-compatible. They are intentionally unavailable to `texture inspect` and
`texture check` until migrated to the structured form above; scorekit never
invents discovery metadata for a legacy entry.

```bash
scorekit texture inspect textures.yaml --category organic --tag water
scorekit texture check textures.yaml
scorekit build forest.yaml --texture-profile textures.yaml -o forest.ogg --stems
```

`texture inspect` filters exactly and conjunctively — no similarity ranking,
so "nothing fits" is an answer you can trust rather than a plausible wrong
pick. `texture check` proves every source exists, decodes, and is audible,
and records a `sha256` of each recording.

FFmpeg normalizes each recording to the build sample rate; scorekit only
performs deterministic placement and gain-applied summation. Runtime sounds
driven by player position, weather, or engine RPM still belong in the game
engine — texture tracks are score-timeline material.

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

For a 140 BPM heavy-Dubstep reference that is reusable, semantic-diffable, and
linted for drum density/backbeats/talking-bass automation, see the
[`heavy_dubstep` scene](examples/scenes/heavy_dubstep.yaml) with its
[`heavy_dubstep` grammar](examples/grammars/heavy_dubstep.yaml). Exact event and
automation IDs keep edits reviewable through `scorekit diff`. Release-quality
growl/FM/talking timbre requires a real certified SFZ mapping that declares its
supported controls. The companion ScoreData corpus provides a first-party CC0
growl plus separate growl/sub/metal/drums palettes and four honest industrial
texture bindings; `scoredata-forge` produced the frozen WAV/SFZ asset offline
and is never a scorekit runtime dependency.
`scorekit profile check` actively verifies that each declared CC1/CC11/CC74 or
pitch-bend target produces deterministic, non-silent, measurably different PCM;
a declaration that the patch ignores is a hard failure.

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

## Real instruments (SFZ sample libraries)

The default SoundFont covers sketching; for release-quality audio, switch the renderer to [sfizz](https://sfz.tools/sfizz/) and a free sample library like [VSCO 2 Community Edition](https://vis.versilstudios.com/vsco-community.html) (CC0). Sfizz builds are routed by an **orchestration profile** — a small YAML that maps logical palettes to certified renderer profiles:

```yaml
# orchestration.yaml
schema_version: 1
name: vsco2-ce
default_palette: ensemble
palettes:
  ensemble:
    profile: examples/profiles/vsco2-ce.yaml
```

```bash
scorekit orchestration check orchestration.yaml
scorekit build elegy.yaml --renderer sfizz \
    --orchestration orchestration.yaml -o elegy.ogg --stems
```

Your scene stays portable — it says `instrument: violin`, and each track's optional `palette` (defaulting to `default_palette` when omitted) picks which renderer profile in the orchestration resolves it to real `.sfz` sample files. A scene can layer palettes explicitly (e.g. one track pinned to a `solo` palette next to section tracks on `ensemble`) to get a soloist over a section without ever naming a local path. See the [profiles manual](https://talkincode.github.io/scorekit/profiles.html).

## Style gallery

Ready-to-build references in [examples/scenes/](examples/scenes/): [chiptune](examples/scenes/chiptune.yaml) (8-bit), [dance](examples/scenes/dance.yaml), [epic](examples/scenes/epic.yaml), [ballad](examples/scenes/ballad.yaml) (3/4), [elegy](examples/scenes/elegy.yaml) (violin), [dunes](examples/scenes/dunes.yaml) (film-score texture, conforms to the [grief grammar](examples/grammars/grief.yaml)).

```bash
scorekit batch \
  examples/scenes/chiptune.yaml examples/scenes/dance.yaml \
  examples/scenes/epic.yaml examples/scenes/ballad.yaml \
  examples/scenes/elegy.yaml examples/scenes/dunes.yaml \
  --out-dir out/
```

## For engineers

The technical manual lives at **[talkincode.github.io/scorekit](https://talkincode.github.io/scorekit/)**: [command reference](https://talkincode.github.io/scorekit/commands.html), [rendering & dependencies](https://talkincode.github.io/scorekit/rendering.html), [architecture & guarantees](https://talkincode.github.io/scorekit/architecture.html), and the [machine interface](https://talkincode.github.io/scorekit/machine-interface.html) — the stable contract (exit codes, `--json` errors, schemas, the `scorekit mcp` stdio server) for building integrations on top of the CLI. A pinned [`Dockerfile`](Dockerfile) ships the exact FluidSynth/FFmpeg/SoundFont toolchain for cloud and CI use — published multi-arch on every release as [`talkincode/scorekit`](https://hub.docker.com/r/talkincode/scorekit) (Docker Hub) and `ghcr.io/talkincode/scorekit` (GHCR).

Project profile, non-goals (iron rules), roadmap, and the acceptance matrix binding every tier-1 feature to E2E tests: [docs/roadmap.md](docs/roadmap.md). Contributor hard rules: [AGENTS.md](AGENTS.md).

### Multi-repo workspace

scorekit is the hub of a small constellation — [scorebench](https://github.com/talkincode/scorebench) (workbench), [scorebench-samples](https://github.com/talkincode/scorebench-samples) (sample corpus), [scoredata-forge](https://github.com/talkincode/scoredata-forge) (CC0 sample foundry), and the local `ScoreData` sound-source inventory. The committed map of that constellation is [`scorekit-workspace.json`](scorekit-workspace.json); machine-specific paths go in `scorekit-workspace.local.json` (gitignored) next to it. `python3 scripts/workspace.py doctor` checks the layout on your machine (default convention: sibling directories) and prints clone/provisioning hints; `python3 scripts/workspace.py gen` deterministically regenerates the shared `scorekit.code-workspace` and an env fragment exporting `SCOREKIT_SOUND_LIBRARY_DIR` at the workspace root. This is developer tooling only — the scorekit CLI never reads these files.

## License

[MIT](LICENSE)
