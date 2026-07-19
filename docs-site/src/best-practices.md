# Best Practices

This chapter collects proven scene patterns and workflow discipline from the bundled examples (`examples/scenes/`). Each recipe is a professional starting point: copy the shape, keep the discipline, change the music.

## Workflow discipline

Follow the same loop for every scene, whether authored by a human or an Agent:

```bash
scorekit schema > scene.schema.json   # 1. learn the live contract first
scorekit validate scene.yaml          # 2. validate before every build
scorekit diff old.yaml new.yaml       # 3. review musical changes, not YAML noise
scorekit build scene.yaml --stems -o out/scene.ogg   # 4. build atomically
```

- **Schema first.** Never write a scene from memory of an older version; `scorekit schema` is the live source of truth and strict unknown-field rejection will catch drift loudly (exit 2, field path).
- **Validate early and often.** Validation is cheap and errors carry a machine-readable `field`/`location`; fix at the YAML layer, not by inspecting audio.
- **Review with `diff`, not `git diff`.** `scorekit diff` compares compiled semantics, so a reformat shows as "no musical change" while a single `intensity` tweak is surfaced precisely.
- **Pin the toolchain for audio.** The same scene + scorekit version always yields byte-identical MIDI; identical *audio* additionally requires pinning FluidSynth/sfizz/FFmpeg versions and the sound source. Record all of them next to the scene in version control.
- **Trust atomic output.** Failed builds never leave partial assets, so a build script may safely overwrite in place and treat exit code 0 as "asset is complete".

## Recipe: seamless ambient loop

The bread-and-butter game asset — background music that loops without an audible seam (see `examples/scenes/forest.yaml`):

```yaml
title: Forest Theme
story: >-
  Calm nocturnal forest ambience for the exploration loop — soft,
  unhurried, no threat.
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

Practices that make a loop work:

- **`loop: true` is the whole seam contract.** The compiler seals note tails across the boundary and the build emits a sample-exact asset; do not try to fade the seam in post.
- **Match `bars` to the harmony cycle.** With a 4-numeral progression, use 4, 8, or 16 bars so the loop point lands on the start of the progression, not mid-cycle.
- **Keep ambient intensities low and close together** (0.3–0.6). A loop heard for minutes must leave headroom and avoid one element dominating.
- **Write the `story` field.** It never affects output, but it travels into `meta.json` so a reviewing Agent (or a colleague, months later) can audit the music against its intent.

## Recipe: one file, one theme, four game states

Instead of composing four unrelated cues, write **one scene with sections** that share key, motif, and instrumentation (see `examples/scenes/forest_suite.yaml`):

```yaml
motifs:
  theme:                      # the shared melodic identity
    - { degree: 1, beats: 1.5 }
    - { degree: 3, beats: 0.5 }
    - { degree: 5, beats: 1 }
    - { degree: 4, beats: 1 }
    - { degree: 3, beats: 1.5 }
    - { degree: 2, beats: 0.5 }
    - { degree: 1, beats: 2 }

sections:
  - name: intro               # theme alone, no rhythm section
    bars: 4
    mute: [3, 4]
    intensity: 0.7
  - name: explore             # full band, seamless loop
    bars: 8
    loop: true
  - name: combat              # same theme, faster and harder
    bars: 8
    loop: true
    tempo: 132
    intensity: 1.4
  - name: victory             # short sting with natural decay
    bars: 2
    mute: [4]
```

- **Vary state with `tempo`, `mute`, and the `intensity` multiplier — not new material.** The player must always recognize the same place; the shared motif is what carries that identity across intro, exploration, combat, and victory.
- **Transitions are just short non-loop sections.** A 2-bar sting with a couple of tracks muted is a victory fanfare; no separate scene file needed.
- **One file per location keeps review honest.** A `scorekit diff` on the suite shows exactly which dramatic state changed.

## Recipe: film-style expressive cue

For narrative scoring, the `performance` block turns a mechanical render into a played one (see `examples/scenes/elegy.yaml`):

```yaml
tempo: 58
key: D_minor
harmony: [i, iv, VI, v]       # plagal grief — override the default progression

performance:
  humanize: { timing_ms: 18, velocity: 10, seed: 7 }
  legato: true                # bow stays on the string
  dynamics: { start: pp, peak: mf }   # one long arch, returning to silence

tracks:
  - instrument: violin        # the lone voice
    pattern: melody
    motif: lament
    intensity: 0.65
  - instrument: harp          # sparse consolation
    pattern: arpeggio
    intensity: 0.3
```

- **Always seed `humanize`.** The jitter is deterministic per seed; commit the seed and every rebuild is byte-identical. Change the seed only when you *want* a different performance.
- **`dynamics` is loop-safe by construction** (start → peak → start), so an expressive arch still works inside a looping cue.
- **Slow music needs rests.** End a lament motif with `{ degree: 0, beats: 1 }` — degree 0 is a rest, and the breath before the phrase returns is part of the phrase.
- **Choose harmony deliberately.** The default progressions are safe, not expressive; four numerals (`[i, iv, VI, v]`) are often the single most characterful line in the file.

## Motif craftsmanship

Motifs are where the musical identity lives; the patterns around them are accompaniment.

- **Give a motif an arch or a hook, not a scale.** The elegy's lament reaches up (5 → 8) then descends stepwise to rest; the chiptune hook (`examples/scenes/chiptune.yaml`) bounces between chord tones in octaves. Both are singable after one hearing.
- **Size the motif to the harmony.** A motif whose total beats equal one or two bars re-aligns with the chord cycle naturally; odd lengths create drifting phase — use that only on purpose.
- **Reuse one motif across tracks and sections** rather than inventing several. Recognition beats variety in functional scoring.
- **Reserve `glide` for one lead voice.** Portamento on everything is mud; on a single melody track it is expressive, and in loops it glides seam-continuously from last note back to first.

## Mixing inside the scene

The scene is also the mix, and it is fully deterministic:

- **Build a depth hierarchy with `intensity`:** lead ≈ 0.65–0.7, harmonic bed ≈ 0.4–0.5, drums lowest. Leave the 0.8+ range for moments that must cut through.
- **Separate voices with `pan`** — e.g. harp 0.35, strings 0.65 — instead of hoping the renderer sorts it out. `pan`/`reverb` compile to single CCs at tick 0: predictable, diff-able, portable across renderers.
- **`articulation` never changes the MIDI.** It only selects SFZ samples at render time, so switching `sustain` → `tremolo` is a render decision you can audition freely without invalidating the compiled score.
- **Use `--stems` when the game engine mixes.** Sample-aligned per-track files let the engine duck, mute, or crossfade layers at runtime — often better than baking several intensity variants.

## Style governance with grammar profiles

When several scenes (or several Agents) must share an aesthetic, encode the aesthetic as measurable rules (see `examples/grammars/grief.yaml`):

```yaml
name: grief
rules:
  tempo_max: 60
  melodic_voices_max: 2
  melody_rest_ratio_min: 0.35
  resolution: incomplete
  harmony_allowed: [i, iv, v, VI, VII]
  require_performance: true
```

```bash
scorekit lint scene.yaml --grammar examples/grammars/grief.yaml
```

- **A grammar is a constitution, not a composition.** It says what the style is *allowed* to sound like; the scene says what it does sound like. Keep them in separate files, both under version control.
- **Lint in CI.** Every rule is deterministic and machine-checkable, so a pull request that breaks the project's musical language fails the same way one that breaks the schema does.

## Batch and CI

For a project with many scenes:

```bash
scorekit --json doctor                          # gate: fail fast on missing tools
scorekit batch scenes/*.yaml --out-dir out/     # one JSON report for all assets
```

- **Gate CI on `doctor`** so a missing renderer fails with exit 3 and a clear diagnosis instead of a mid-batch surprise.
- **Parse the batch JSON report** rather than scraping logs; per-scene status plus stable exit codes (`0/1/2/3/4`) is the whole integration contract.
- **Cache by content, not by mtime.** Because compilation is deterministic, the hash of (scene file + scorekit version) is a valid cache key for MIDI; add tool and sound-source versions to the key for audio.
