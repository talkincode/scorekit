# scorekit reference

Verified against scorekit v0.6. When in doubt, trust the binary:
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
| `schema` | JSON Schema of scene DSL | `--grammar` → grammar profile; `--profile` → leaf renderer profile; `--orchestration` → orchestration profile; `--texture-profile` → texture-source profile; `--resolver` → instrument-resolver config |
| `profile check <profile>` | certify all explicit SFZ mappings and declared automation responses with real probe renders | `--sample-rate` 8000..=384000 (44100); a shared melodic/percussion patch must pass both probes; each declared CC1/CC11/CC74/pitch-bend target must produce deterministic, non-silent, measurably different PCM; global `--json` emits the full report |
| `orchestration check <file>` | validate palette bindings and every referenced leaf renderer profile + SFZ file | global `--json` emits `{name, default_palette, palettes: [{name, profile, profile_path, mappings, patches}]}` |
| `texture inspect <profile>` | enumerate/filter texture sources before writing `textures[].source` | `--category`, `--tag` (repeatable, AND), `--mode loop\|one_shot`, `--use-case`, `--source`; exact conjunctive matching only, never similarity ranking; exits 0 with `"status": "no_match"` when nothing satisfies the query, exit 2 on an unknown category; global `--json` emits the full report |
| `texture check <profile>` | certify every declared source exists, decodes, and is audible | `--sample-rate` (44100); reports `sha256`/`duration_seconds`/`frames`/`peak_abs`/`rms` per source; exit 4 if undecodable, 2 for `missing`/`silent`; global `--json` emits the full report |
| `lint <scene> --grammar <file>` | check scene against aesthetic grammar | — |
| `midi <scene> -o <out.mid>` | compile to SMF (format 1, PPQ 480) | `--passes` 1..=8 (1), `--solo <track id>`, `--section <name>`; profile-only melodic identities are rejected before writing because no renderer profile is available |
| `render <mid> -o <out.wav>` | synthesize WAV | `--soundfont <sf2>` (defaults to `$SCOREKIT_SOUND_LIBRARY_DIR/sf2/MuseScore_General.sf2`) **or** `--sfz <file>` (sfizz, single instrument); `--renderer fluidsynth\|timidity\|sfizz` (fluidsynth), `--sample-rate` 8000..=384000 (44100), `--gain` 0.0..=8.0 (0.8, ignored by sfizz) |
| `export <in> -o <out>` | FFmpeg convert (.ogg Vorbis / .wav PCM) | `--quality` 0..=10 (5), `--seek-samples` (0), `--take-samples` |
| `build <scene> -o <out.ogg\|wav>` | full chain + meta.json | default MuseScore General, explicit `--soundfont <sf2>`, **or** `--orchestration <file>` (sfizz only; routes each track's `palette` to a certified leaf renderer profile); `--texture-profile <file>` when `textures` are declared; `--renderer fluidsynth\|timidity\|sfizz`; `--fallback-mode strict\|conservative\|flexible` + `--resolver <config>` (instrument substitution); plus `--stems`, `--tail` 0.0..=3600.0 secs (4.0, non-loop), `--crossfade-ms` 0..=60000 (50, loop seal), `--keep-intermediates` |
| `inspect-instruments <scene>` | resolve instruments, report per-track palette/profile/SFZ routing | `--orchestration <file>` (omit = exact GM subset; non-GM world identities remain missing), `--resolver <config>`, `--fallback-mode`, `--verbose` (all scored candidates); exit 2 when unresolved; global `--json` emits the report |
| `diff <old> <new>` | semantic scene diff (ignores formatting) | — |
| `mcp` | serve MCP over stdio (newline-delimited JSON-RPC 2.0); exposes `doctor`, `validate`, `schema`, `lint`, `build`, `inspect_instruments`, `orchestration_check`, `inspect_textures`, `texture_check`, `diff` as tools, each re-invoking the CLI with `--json` and passing structured output through verbatim | — |
| `batch <scenes...> --out-dir <dir>` | build many; report.json; failures don't stop the rest | default MuseScore General, explicit `--soundfont <sf2>`, **or** `--orchestration <file>` (sfizz); `--format ogg\|wav` (ogg) + render/export/resolver flags |

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
| `clips` | `{stable_id: clip, …}` | `{}` | exact pitched/percussion events plus step automation; map order is semantically inert |
| `textures` | `[texture, …]` | `[]` | field recordings/ambience/SFX; portable source names bind through `--texture-profile` |
| `tracks` | `[track, …]` | required | 1..=16 (≤15 melodic + ≤1 percussion: `drums` or `tabla`) |
| `sections` | `[section, …]` | `[]` | turns the scene into a suite |

### Track

| Field | Type / range | Default | Notes |
| --- | --- | --- | --- |
| `id` | `[a-z][a-z0-9_-]{0,63}`, unique per scene | required | stable scene-local identity; used by `sections[].mute`, `midi --solo`, stem file names (`NN-<id>.ext`), and `meta.json`/`inspect-instruments` reports |
| `palette` | `[a-z][a-z0-9_-]{0,63}` | orchestration's `default_palette` | logical orchestration palette (`--orchestration`, `--renderer sfizz` only); routing metadata only — never changes MIDI or affects SF2/TiMidity backends |
| `instrument` | enum (below) | required | `drums` and `tabla` each pair exclusively with their namesake pattern |
| `pattern` | `sustain` `arpeggio` `bass` `drums` `melody` `tabla` `clip` | required | melody plays a motif; clip plays exact authored events; tabla cycles its deterministic 16-beat theka |
| `motif` | motif name | — | required iff `pattern: melody` |
| `clip` | clip name | — | required iff `pattern: clip`; kind must match the track |
| `intensity` | 0.0..=1.0 | 0.6 | velocity scale |
| `articulation` | `sustain` `staccato` `spiccato` `pizzicato` `tremolo` `mute` | `sustain` | render-time only, no MIDI change; ignored by fluidsynth/timidity; under `--renderer sfizz --orchestration ...` selects the `.sfz` file the track's effective palette's leaf profile resolves to (falls back to the instrument's `sustain` mapping if unmapped) |
| `pan` | 0.0..=1.0 | — | stereo position → CC10 (`0` left, `0.5` center, `1` right); omitted = renderer default |
| `reverb` | 0.0..=1.0 | — | reverb send → CC91; omitted = renderer default |
| `glide` | 0.0..=1.0 | — | melody-only tail portamento: the last `glide` fraction of each note pitch-bends toward the next pitch (clamped ±2 semitones); loops bend last→first, seam-safe |

`pan`/`reverb`/`glide` compile to deterministic MIDI (CC10/CC91/pitch-bend).
fluidsynth/timidity honor all three; sfizz honors pitch bend, but CC10/91
only take effect if the `.sfz` maps those CCs.

### Event clip

Clip IDs, event IDs, lane IDs, and point IDs all match
`[a-z][a-z0-9_-]{0,63}`. They are semantic identity, not list position:
reordering map entries leaves MIDI and `scorekit diff` unchanged.

```yaml
clips:
  talking_bass:
    kind: pitched                 # pitched | percussion
    length_beats: 4               # quarter-note beats, PPQ-480 quantization
    mode: loop                    # once | loop
    events:
      bark_01: { at: 0, duration: 0.5, pitch: F1, velocity: 127 }
      bark_02: { at: 0.75, duration: 0.25, pitch: C2, velocity: 120 }
    automation:                   # pitched clips only; at most 4 lanes
      mouth:
        target: cc1               # cc1 | cc11 | cc74 | pitch_bend
        points:
          shut: { at: 0, value: 0 }
          open: { at: 0.25, value: 127 }
          seal: { at: 3.75, value: 0 }
tracks:
  - { id: bass, instrument: synth_bass, pattern: clip, clip: talking_bass }
```

All positions use quarter-note beats regardless of meter and quantize with
`round(beats * 480)`. Pitched events require scientific `pitch`
(`C-1=0`, `C4=60`, `A4=69`) and `duration`; percussion events require a frozen
GM `voice` (`kick`, `snare`, `clap`, closed/pedal/open hats, low/mid/high tom,
`crash`, `ride`) and default to 0.125 beat. Velocity is 1..=127.

A loop clip must divide every active scene/section timeline. Events cannot
cross the boundary; equal-pitch pitched events cannot overlap; duplicate
voice/tick percussion onsets are rejected. Automation values are 0..=127 for
CCs and -8192..=8191 for bend, start at beat 0, and return to the initial value
at the end of a loop lane. Steps emit exactly; there is no linear mode.
Same-tick order is note-off -> automation CC -> pitch bend -> note-on.
Expanded authored notes plus automation points are capped at 65,536 per active
track for every scene/section timeline; `validate` rejects larger loop
expansions before composition.

Clip timing is exact and ignores swing/legato/humanize; intensity and dynamics
still scale velocities. For sfizz, every automated target must be declared by
the effective leaf profile mapping's `controls` set or orchestration resolution
fails before staging.

### Texture track

| Field | Type / range | Default | Notes |
| --- | --- | --- | --- |
| `source` | `[a-z][a-z0-9_-]{0,63}` | required | portable profile key, never a local path |
| `mode` | `loop` \| `one_shot` | required | continuous repetition or full-source triggers |
| `start_beat` | ≥0 | 0 | loop-only; must be 0 if the scene/any section loops |
| `at` | 1..=64 quarter-note beats | — | required for one-shot; schedule repeats per loop pass |
| `gain` | 0.0..=1.0 | 1.0 | linear gain before summation |

Texture profile — every source carries required discovery metadata, so an
agent can enumerate and filter instead of guessing names:

```yaml
schema_version: 1
name: forest
root: /path/to/recordings
sources:
  river:
    path: ambience/river.flac
    description: Wide river bed, mid-distance, no bird calls
    category: organic
    tags: [water, flowing, continuous]
    playback: { modes: [loop], default_mode: loop }
    use_cases: [forest, travel]
    provenance: { library: field-recordings@2024.1 }
  birds:
    path: wildlife/birds.wav
    description: Single dawn chorus swell, ends on silence
    category: organic
    tags: [wildlife, chirping]
    playback: { modes: [one_shot], default_mode: one_shot }
    use_cases: [forest, dawn]
    provenance: { library: field-recordings@2024.1 }
```

`category` is a closed enum (`ambience`, `foley`, `impact`, `transition`,
`tonal`, `industrial`, `organic`, `sound_design`) documented per value in
`schema --texture-profile`; `tags`/`use_cases` are open tokens matching
`[a-z][a-z0-9_-]{0,31}` (1–16 entries). Physics is never declared here —
`texture check` measures duration, loudness, and `sha256`.
Legacy `name: path` bindings remain usable by `build`, but `texture inspect`
and `texture check` reject them until they are migrated; no discovery metadata
is inferred.

FFmpeg normalizes sources to stereo 16-bit PCM at the build sample rate;
scorekit then performs deterministic placement only. With `--stems`, texture
stems follow instrument stems (`03-texture-river.wav`, etc.) and are the same
exact length. A `textures[i].mode` outside the source's declared
`playback.modes` fails the build before staging. World-driven audio such as
positional water, weather, or engine RPM belongs to the game runtime, not
texture tracks.

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
| `mute` | `[track id, …]` | `[]` | stable track `id`s silenced this section; muting all tracks is rejected |
| `clips` | `{track_id: clip_id, …}` | `{}` | section-local replacement for a `pattern: clip` track |
| `intensity` | 0.0..=2.0 | 1.0 | multiplier on every track's intensity |

Sections share the scene's key, tracks, clips, motifs, harmony, performance,
and texture schedule.
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
- **World, exact GM:** `shakuhachi` (77), `sitar` (104), `shamisen` (106)
- **World, renderer-profile source required:** `erhu`, `pipa`, `guzheng`, `dizi`, `oud`, `ney`, `duduk`
- **Percussion:** `timpani` (47) — pitched; `drums` — GM percussion channel, `pattern: drums` or a percussion `clip`; `tabla` — exact profile source on channel 10, `pattern: tabla` only

The 11 world identities are exact-source-only. Missing mappings never fall
back to another world instrument or into/out of the world family; use a real
source or re-orchestrate visibly.

Common alias spellings are accepted and normalized before compilation
(`french_horn`→`horn`, `fiddle`→`violin`, `contrabass`/`double_bass`,
`grand_piano`→`piano`, …); an unknown name errors with a suggestion.
Aliases are surface syntax only — MIDI bytes are identical either way.

## Orchestration profiles (`--renderer sfizz --orchestration`)

sfizz renders real `.sfz` sample libraries (e.g. free CC0 [VSCO 2 Community
Edition](https://vis.versilstudios.com/vsco-community.html)) instead of a
single GM SoundFont — one instrument per invocation; scorekit renders every
track solo and mixes the results in-process, so stems and the full mix are
sample-aligned by construction. Build the binary once with
`scripts/build_sfizz.sh` (not packaged by Homebrew).

A scene never names a `.sfz` file or a local path — only `instrument` +
`articulation` (portable, shareable) plus each track's stable `id` and an
optional logical `palette`. `build`/`batch --renderer sfizz` and
`inspect-instruments` take a single **orchestration profile**
(`--orchestration <file>`) that is the only sfizz build/inspect input; the
old per-build `--profile` flag is gone:

```yaml
# orchestration.yaml
schema_version: 1
name: hybrid-cinematic
default_palette: ensemble
palettes:
  solo:
    profile: ../renderers/scoredata-chamber.yaml
  ensemble:
    profile: ../renderers/scoredata-symphonic.yaml
```

`scorekit schema --orchestration` prints its schema;
`scorekit orchestration check orchestration.yaml` validates the palette
bindings and loads every referenced leaf profile (and every SFZ path each
leaf profile resolves), failing loudly and without any partial output if a
palette or a leaf profile is missing. Each track's `palette` field selects
which palette routes it (case-sensitive exact match); a track that omits
`palette` uses `default_palette`. Resolution and fallback are strictly
confined to that one palette's leaf profile — there is **no cross-palette
fallback**, so a `solo` track never silently borrows a patch mapped only
under `ensemble`. Declaring several tracks with different `palette` values
(e.g. a `violin` track on `solo` next to a `violin` track on `ensemble`) is
how a scene expresses an explicit soloist-over-section layering; each
resolves through its own palette's leaf profile independently.

### Leaf renderer profiles

A **leaf renderer profile** (external YAML, `schema --profile`) is the
piece that actually maps portable `instrument`/`articulation` DSL names onto
real sample files on a given machine — orchestration only routes a logical
palette name to one of these:

```yaml
# renderers/scoredata-symphonic.yaml
name: scoredata-symphonic
root: /path/to/VSCO-2-CE-1.1.0   # optional; default = the profile file's own directory
instruments:
  violin:
    sustain: SViolinVib.sfz      # required — fallback for any unmapped articulation
    pizzicato: SViolinPizz.sfz
    tremolo: SViolinTrem.sfz
  synth_bass:
    sustain:
      path: Dubstep/Talking-Bass.sfz
      controls: [cc1, cc74, pitch_bend]
  drums:
    sustain: GM-StylePerc.sfz
```

Leaf profile paths in `palettes.<name>.profile` resolve relative to the
orchestration file; each leaf profile's own `.sfz` paths then resolve
relative to `root` (or that profile file's own directory) exactly as
before — never relative to the scene or the orchestration file. A legacy
string mapping declares no automation controls. The structured `{path,
controls}` form can declare `cc1`, `cc11`, `cc74`, and `pitch_bend`; an active
clip using an undeclared target fails at
`clips.<clip>.automation.<lane>.target` before build staging.

Every `Instrument` used by a scene should have an entry with at least a
`sustain` mapping; profile mappings are ground truth and always resolve
exactly. Instruments the active palette's leaf profile doesn't map go
through the **instrument resolver**, isolated per palette: a scored
same-family substitute (range/articulation/envelope/role/timbre, default
minimum 0.70) is used with a `WARN instrument fallback:` line, but strings
are never a default absorber (substituting into strings needs an explicit
`allowed_families: [strings]` in a `--resolver` config), drums are never
substituted, and synth stand-ins need `--fallback-mode flexible`. When
nothing qualifies the build fails before staging (exit 2, code
`resolution`, no partial output) and names the best rejected candidate.
`--fallback-mode strict` disables substitution entirely;
`scorekit inspect-instruments scene.yaml --orchestration orchestration.yaml`
previews per-track statuses (`exact`/`alias`/`fallback`/`missing`/
`rejected`) plus each track's `id`, declared/effective palette, leaf profile
name/path, requested/effective articulation, and resolved SFZ path, without
building. Substitution never changes MIDI bytes or stem/meta names — only
the rendered `.sfz`. Malformed paths still fail loudly at build time.
`.sfz` paths are relative to a leaf profile's `root`, and one leaf profile
can span multiple sample libraries per-instrument by prefixing each `.sfz`
path with the library's own subfolder name under a shared `root` (no
per-instrument `root:` override exists or is needed). See
`scorekit schema --profile` for the leaf profile's full JSON Schema,
[examples/profiles/vsco2-ce.yaml](../../examples/profiles/vsco2-ce.yaml) for
a complete worked mapping (including orchestral substitutions for
synth/vocal instruments VSCO2 doesn't provide, e.g. `square_lead` → flute),
and [examples/profiles/vsco2-vcsl.yaml](../../examples/profiles/vsco2-vcsl.yaml)
for a hybrid that also pulls piano/harp/epiano/timpani from VCSL (a CC0
supplement library, not a substitute for VSCO2's strings/brass/choir).

Every leaf profile is still independently certified with
`scorekit profile check <profile.yaml>` before it is wired into any
orchestration — this is unchanged. The check deduplicates shared patch
paths, renders broad melodic or GM-drum probes at varied velocities twice,
and requires both when one physical patch backs melodic and percussion
mappings. It rejects missing and silent patches, captures sfizz warnings, and
verifies repeatability. Structured mappings also trigger two inverse gesture
probes for every declared CC1/CC11/CC74/pitch-bend target; each gesture is
double-rendered, and decoded PCM must differ by more than the determinism
tolerance. Shared physical patches union their declared targets while the JSON
`control_probes` evidence retains each declaring mapping. A path-only mapping
adds no control probes. This response check does not replace a musical
listening review. It writes no persistent audio: each completed WAV pair is
removed before the next probe, and the command-scoped scratch directory is
removed on success and failure. Use
`scorekit --json profile check <profile.yaml>` to retain a machine-readable
certification report.

## Grammar profiles (`lint`)

External YAML, unknown fields rejected; `name` plus **at least one rule**
required. Surface rules read the scene; deep rules measure the **compiled
score** (after pattern expansion and performance transforms). Suites are
checked per section. `section_rules: {section_name: {…}}` adds assertions only
for named sections and also requires those sections to exist.

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
| `percussion_events_per_bar_min` | count/bar | compiled percussion note-on density |
| `percussion_onsets` | voice + 1..=32 positions + coverage | fraction of bars whose `drums` track contains the GM voice at each exact within-bar quarter-beat; Tabla key numbers never satisfy a GM drum rule |
| `automation_activity` | track? + target + minima | compiled point density per bar and/or value span for CC1/CC11/CC74/pitch bend |

Violation format: `{rule} @ {subject}: measured {value}, want {constraint}`
(subject is `scene` or ``section `name` ``); exit 2; `--json` → `violations`
array. Shipped pairs: `examples/grammars/grief.yaml` with
`examples/scenes/dunes.yaml`, and `examples/grammars/heavy_dubstep.yaml` with
`examples/scenes/heavy_dubstep.yaml`.

## meta.json

Single scene (`build`):

```json
{
  "title": "...", "story": "...", "loop": true, "tempo": 92, "key": "D_minor",
  "time_signature": "4/4", "bars": 16, "sample_rate": 44100,
  "loop_samples": 1841216, "total_samples": 1841216,
  "crossfade_samples": 2205, "seconds": 41.75,
  "audio": "scene.ogg",
  "stems": ["scene.stems/01-piano.ogg", "..."],
  "tracks": [{ "id": "harmony", "palette": null, "instrument": "piano", "articulation": "sustain", "pattern": "sustain", "intensity": 0.6 }],
  "textures": [{ "source": "river", "mode": "loop", "start_beat": null, "at": [], "gain": 0.25 }],
  "orchestration": { "name": "hybrid-cinematic", "default_palette": "ensemble", "palettes": ["..."] }
}
```

`"orchestration"` and per-track `"palette"` are present only for
`--renderer sfizz --orchestration <file>` builds; SF2/TiMidity builds omit
them (`palette` is `null`, `orchestration` is absent).

Suite: `{"suite": true, "tempo", "key", "time_signature", "sample_rate",
"sections": [ …single-scene entries each with "name"… ]}`.
Every build also embeds `"instrument_resolution"` (per-track
status/score/reasons plus a summary; all-exact under GM SoundFonts; under
`--orchestration` each track entry also carries `track_id`,
`palette`/`profile`/`profile_path`, `requested_articulation`/`articulation`,
and `sfz`) in the meta entry and suite manifest.
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
