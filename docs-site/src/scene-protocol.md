# Scene Protocol

A scene file is not a configuration file. It is the **wire format between a composing agent and the compiler** — the input half of scorekit's machine contract, exactly as the [Machine Interface](machine-interface.md) is the invocation half. An agent that emits a valid scene document is speaking a protocol: every field has one deterministic, observable compile semantic, the format is versioned under the same semver as the binary, and violations are rejected loudly with machine-readable positions, never silently ignored.

This chapter is the normative specification of that protocol. The live, machine-readable source of truth is always `scorekit schema` (JSON Schema); when this prose and the binary disagree, the binary wins.

## Why a protocol, not just a format

Five properties separate a protocol from a mere file format, and the scene DSL commits to all of them:

1. **Strictness.** Unknown fields are rejected (`deny_unknown_fields` at every level). A typo, or a scene written for a newer scorekit, fails with exit 2 and a line/column — it is never silently dropped. Silent tolerance is how formats rot; loud rejection is how protocols stay honest.
2. **Deterministic semantics.** The same scene + the same scorekit version compiles to **byte-identical MIDI**, always. There is no "interpretation", no randomness outside the seeded `performance.humanize`, and no field whose effect depends on the machine it runs on.
3. **No dead fields.** Every field must have a deterministic compile semantic. Mood tags, game-state hints, and creative intent (`avoid: too_bright`) are *by design* not part of the protocol — they belong in the agent's prompt space. A field that "doesn't do anything" is a protocol violation waiting to be invented twice.
4. **Machine discoverability.** `scorekit schema` exports the full JSON Schema, including field docs, ranges, and defaults. An agent can learn to write valid scenes from the schema plus one error message, without reading source code.
5. **Text-native.** UTF-8 YAML, line-oriented, diff-friendly. Version control, review, merge, and rollback are git's job; the protocol's job is to make diffs *mean* something (see `scorekit diff` for semantic comparison that ignores formatting).

## Envelope

- One UTF-8 YAML document per file; one document = one scene (or one suite, when `sections` is present).
- Field names are `snake_case`. Unknown fields anywhere are an error.
- Validation is two-layered, and both layers are part of the contract:
  - **Structural** — shape and types, as exported by `scorekit schema`. Parse errors report `location: {line, column}`.
  - **Semantic** — ranges, cross-field rules (e.g. `motif` required iff `pattern: melody`), reference checks. Violations report a `field` path like `tracks[2].glide`.

## Versioning and stability

The scene protocol is versioned by the scorekit binary's semantic version. There is deliberately **no version field inside the scene file**: strict unknown-field rejection already makes version skew detectable at the only moment it matters (compile time), and a self-declared version number that the compiler must trust would be weaker than one it derives from its own schema.

Within a major version:

- **Additive only.** New fields are optional with defaults that preserve prior output — a scene that does not use a new field compiles to *byte-identical MIDI* before and after the addition. (Precedent: when `pan`/`reverb`/`glide` landed in 0.2.0, the golden SMF byte-comparison test did not change.)
- **No repurposing.** An existing field's name, type, range, default, or compile semantic does not change. The normative transform order (below) is part of the semantic and is equally frozen.
- **Old scene, new binary:** always valid, same MIDI bytes.
- **New scene, old binary:** rejected with exit 2 and the offending field path — a readable upgrade signal, not a corrupted asset.

Anything that breaks these rules is a major-version change. Determinism across *tool* versions (FluidSynth/FFmpeg) is a separate boundary owned by the [Machine Interface](machine-interface.md#stability-contract): pin your toolchain for identical audio; MIDI needs only the scorekit version pinned.

### Rules for extending the protocol

A proposed field must clear all of these gates (this is the governance encoded in the project's iron rules):

- It has exactly one deterministic compile semantic, statable in one sentence ("compiles to X").
- It is achievable with free/open sound sources — the schema must not contain fields only commercial libraries can honor.
- Its absence compiles byte-identically to the world before the field existed.
- It ships in the same change as at least one happy-path E2E test, one failure-path test if validation is involved, and an acceptance-matrix row.

## Scene document

| Field | Type / range | Default | Compile semantic |
| --- | --- | --- | --- |
| `title` | string | — | informational only; never affects output |
| `story` | string | — | informational only; never affects output — freeform narrative brief (theme, mood, dramatic intent) carried into `meta.json` so review agents can audit the music against its story |
| `tempo` | int, 20..=300 | required | BPM; sets the SMF tempo meta event |
| `key` | `<Note>_<major\|minor>` (`C_major`, `F#_minor`, `Eb_major`, …) | `C_major` | root + scale for all degree/numeral resolution |
| `time_signature` | `N/D`, N 1..=12, D ∈ {2,4,8,16} | `4/4` | bar length in ticks; drives pattern shapes |
| `bars` | int, 1..=256 | required | scene length; total ticks = bars × N × (PPQ·4/D) |
| `loop` | bool | `false` | `true` = seamless-loop asset (sample-exact length, sealed seam); `false` = one-shot with `--tail` decay |
| `harmony` | `[numeral, …]`, diatonic `i`..`vii` (case per quality) | major `I-V-vi-IV`, minor `i-VI-III-VII` | one triad per bar, cycled to fill; sustain/arpeggio/bass derive from it |
| `motifs` | `{name: [note, …]}` | `{}` | melodies referenced by `pattern: melody` tracks; map is order-insensitive (sorted for determinism) |
| `performance` | object | absent | deterministic humanization (below); absent = exact mechanical rendering |
| `textures` | `[texture, …]`, ≤16 | `[]` | deterministic ambience/SFX layers; source names bind through `--texture-profile` at build time and never affect MIDI |
| `tracks` | `[track, …]`, 1..=16 | required | ≤15 melodic + ≤1 drums |
| `sections` | `[section, …]` | `[]` | turns the scene into a suite; one output asset per section |

## Track

| Field | Type / range | Default | Compile semantic |
| --- | --- | --- | --- |
| `instrument` | GM name (see `scorekit schema` for the enum) or `drums` | required | program change at tick 0; `drums` ↔ channel 10 |
| `pattern` | `sustain` `arpeggio` `bass` `drums` `melody` | required | note-generation algorithm (below) |
| `motif` | motif name | — | required iff `pattern: melody`; must exist in `motifs` |
| `intensity` | 0.0..=1.0 | 0.6 | scales note velocities |
| `articulation` | `sustain` `staccato` `spiccato` `pizzicato` `tremolo` `mute` | `sustain` | render-time SFZ sample selector only; **never changes the compiled MIDI** |
| `pan` | 0.0..=1.0 | absent | CC10 = `round(v·127)` once at tick 0; absent = no CC10 emitted |
| `reverb` | 0.0..=1.0 | absent | CC91 = `round(v·127)` once at tick 0; absent = no CC91 emitted |
| `glide` | 0.0..=1.0, melody-only | absent | tail portamento via pitch bend, clamped ±2 semitones (GM default bend range); loops glide last note → first note, seam-continuous |

## Texture track

Texture tracks are score-timeline material: field recordings, ambience, and
sound effects used as part of the composition. They do not model runtime
world audio such as distance attenuation, weather state, or engine RPM.

| Field | Type / range | Default | Compile semantic |
| --- | --- | --- | --- |
| `source` | `[a-z][a-z0-9_-]{0,63}` | required | portable key resolved by `--texture-profile`; scene files never contain audio paths |
| `mode` | `loop` \| `one_shot` | required | continuous source repetition or one full source playback at each trigger |
| `start_beat` | quarter-note beat ≥0 | 0 | loop-only start position; must be 0 when the scene or any section loops |
| `at` | `[beat, …]`, 1..=64 entries | `[]` | one-shot-only trigger positions; the same schedule repeats in every scene-loop pass |
| `gain` | 0.0..=1.0 | 1.0 | linear gain applied to the arranged texture stem before summation |

Beat positions are quantized to the nearest PPQ-480 tick, then converted to
sample frames using the same quantized MIDI tempo as musical tracks. Loop
textures run continuously across the two render passes; the normal loop seal
therefore joins adjacent source frames even when the source duration does not
divide the scene duration. The recording itself should be prepared as a
loop-ready asset because scorekit does not conceal clicks inside the source.
For a looping scene, a one-shot source must not be longer than one complete
scene pass; this guarantees that one prior pass contains all tail material
needed at the loop boundary.

### Motif note

| Field | Type / range | Semantic |
| --- | --- | --- |
| `degree` | int, -21..=21 | scale step in the scene's key; **0 = rest**; 8 = tonic an octave up; negatives descend |
| `beats` | 0.125..=16 | duration in quarter-note beats |

### Section

| Field | Type / range | Default | Semantic |
| --- | --- | --- | --- |
| `name` | `[A-Za-z0-9_-]+`, unique | required | output asset suffix (`out-<name>.ogg`) |
| `bars` | 1..=256 | required | section length |
| `tempo` | 20..=300 | scene tempo | per-section override |
| `loop` | bool | `false` | per-section loop treatment |
| `mute` | `[track index, …]` | `[]` | 0-based; muting every track is rejected |
| `intensity` | 0.0..=2.0 | 1.0 | multiplier on each track's intensity |

Sections inherit the scene's key, tracks (including spatial fields), textures, motifs, harmony, and performance.
A shared texture schedule must be valid for every section: each trigger is
checked against the shortest derived section timeline, preventing a trigger
from silently wrapping into a later pass of a shorter cue.

### Performance

All optional, all deterministic:

| Field | Type / range | Effect |
| --- | --- | --- |
| `humanize` | `{timing_ms: 0..=50, velocity: 0..=30, seed: u64}` | seeded jitter — same seed, same bytes |
| `swing` | 0.0..=0.5 | delays off-beat eighths |
| `legato` | bool | extends non-drum durations ~12% |
| `dynamics` | `{start, peak}` of `pp p mp mf f ff` | loop-safe arch start→peak→start |

## Normative compile semantics

These are observable guarantees an integration may rely on; changing any of them is a breaking protocol change.

**Time and encoding.** Output is SMF format 1, PPQ 480. A bar is `N × (480·4/D)` ticks. Events are encoded in the canonical order `(tick, kind rank: note-off < pitch-bend < note-on, key)` — this ordering *is* the byte-determinism guarantee.

**Channels and programs.** Melodic tracks take channels in declaration order (0, 1, 2, …), skipping channel 10 (index 9), which is reserved for the single `drums` track. Each track opens with its GM program change; `pan`/`reverb` CCs follow immediately at tick 0. Channel state persists across loop passes, so CCs are emitted once.

**Patterns.**

- `sustain` — the bar's full triad held for the whole bar.
- `arpeggio` — eighth notes cycling chord tones in the fixed order root, third, fifth, third.
- `bass` — chord root two octaves down; two half-bar notes when the meter's numerator is even and ≥4, else one whole-bar note.
- `drums` — kick on beat 1 (plus the midpoint beat when N ≥ 4), snare on off-beats, closed hi-hat on every beat and half-beat.
- `melody` — the named motif, looped or truncated to exactly fill the length; degrees resolve in the scene key.

**Transform order.** `swing → dynamics → legato → humanize`, then `glide` bend computation (it must observe final onsets), then loop-pass duplication. Loop math is applied last so looped scenes stay sample-exact under every transform.

**Harmony.** One numeral per bar, cycled. Numerals resolve to diatonic triads of the scene key; sustain/arpeggio/bass all read the same per-bar chord, which is what keeps multi-track scenes harmonically coherent by construction.

**Texture assembly.** FFmpeg first normalizes every referenced recording to
stereo signed 16-bit PCM at the requested build sample rate. scorekit then
performs only deterministic cutting, repetition, zero-padding, placement,
linear gain, and summation. Texture stems use the same exact output window and
loop seal as instrument stems, so every stem stays sample-aligned and sums
back to the full mix within the existing rounding tolerance.

## Error contract

Protocol violations follow the machine-interface error shape: exit 2, and with `--json` a single structured object on stderr carrying `location` (parse) or `field` (semantic). The `field` path grammar is stable: `tracks[i].pan`, `sections[i].mute[j]`, `harmony[i]`, `performance.swing`. An agent is expected to repair a scene from `field` + `message` alone; that expectation is part of the protocol's design, not a nicety.

## What the protocol will not carry

- **No mood/state/intent fields** (`emotion`, `danger`, `avoid`) — no deterministic compile semantic exists for them; they live in the agent's brief.
- **No renderer or recording paths** (`.sf2`/`.sfz`/audio locations) — sound sources are bound at invocation time (`--soundfont`, `--profile`, `--texture-profile`), so a scene never bakes in one machine's disk layout.
- **No embedded version negotiation, includes, or macros** — one file, one scene, strict fields. Composition happens in the agent, not in a template engine.
