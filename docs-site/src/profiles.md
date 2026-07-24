# Orchestration and Renderer Profiles

`--renderer sfizz` builds are routed by an **orchestration profile** — the
only sfizz build/inspect input — which maps each scene track's logical
`palette` to an independently certified **leaf renderer profile**. The leaf
profile is unchanged from earlier releases: it maps scorekit instruments and
articulations to local SFZ patches, keeping machine-specific sample paths out
of portable scene files. For where the patches themselves come from — public
acquisition channels, the directory/manifest contract, and the certification
workflow — see [Building a Sound Library](sound-library.md).

## Orchestration profiles

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

```bash
scorekit schema --orchestration
scorekit orchestration check orchestration.yaml
scorekit --json orchestration check orchestration.yaml
```

`orchestration check` loads every referenced leaf profile relative to the
orchestration file, resolves every leaf profile's `.sfz` paths, and fails
loudly — with no partial output — if a palette's leaf profile or any mapped
SFZ file is missing.

Each scene track carries a stable `id` and an optional `palette`
(`[a-z][a-z0-9_-]{0,63}`, same syntax as `id`). A track that omits `palette`
uses the orchestration's `default_palette`. `palette`/`id` are routing
metadata only — they never change compiled MIDI bytes, and `sections[].mute`
and `midi --solo` already select by `id`. Resolution and instrument fallback
are strictly isolated per palette: a track routed to `solo` only ever
resolves against `solo`'s leaf profile, never falling back to `ensemble`'s
mappings even if `ensemble` happens to cover the same instrument. Declaring
several tracks with different `palette` values is how a scene expresses an
explicit soloist-over-section layering (e.g. one `violin` track pinned to
`solo`, another `violin` track left on the default `ensemble` palette) — both
render and mix independently, each through its own leaf profile.

```bash
scorekit build scene.yaml --renderer sfizz \
    --orchestration orchestration.yaml -o scene.ogg --stems
scorekit inspect-instruments scene.yaml --orchestration orchestration.yaml
```

There is no per-build `--profile` flag for `build`/`batch`/`inspect-instruments`
any more — `--orchestration` is the only routing input for sfizz. The
low-level `render` command is unaffected: `render --sfz <file>` still renders
one `.sfz` instrument directly, outside any orchestration.

## Leaf renderer profiles

```yaml
# renderers/scoredata-symphonic.yaml
name: scoredata-symphonic
root: /Volumes/Samples
instruments:
  violin:
    sustain: VSCO/Strings/Violin-Sustain.sfz
    pizzicato: VSCO/Strings/Violin-Pizzicato.sfz
  drums:
    sustain: Virtuosity/Programs/01-basic-kit.sfz
```

Every instrument requires a `sustain` mapping, which is also the fallback when a dedicated articulation is absent. A leaf profile's own path in `palettes.<name>.profile` resolves relative to the orchestration file that references it; its `.sfz` paths then resolve relative to its own `root` (or its own file's directory), exactly as before — orchestration only routes a palette name to one of these files, it never rewrites sample paths.

Certify a leaf profile before wiring it into any orchestration:

```bash
scorekit profile check profile.yaml
scorekit --json profile check profile.yaml > profile-report.json
```

The check deduplicates shared patch paths, renders melodic or drum probes twice, rejects missing and silent patches, captures sfizz warnings, and checks repeatability. Each passing patch reports a `render_sha256` golden hash, so a saved report acts as a baseline: re-running the check after a tool or library change and diffing the hashes reveals exactly which patches drifted. If a comparison fails on the first attempt, the check records diagnostics (load average, tool identity, both render hashes, timings) and re-runs that patch once in isolation — a pass is reported as `ok` with a `load_sensitive_flake` warning and the evidence kept under `flake_diagnostics`; a repeat failure is final. Temporary probe files are removed on success and failure; set `SCOREKIT_TMPDIR` to place them on another disk (created if absent, otherwise the system temp dir is used).

Probe renders are bounded: `sfizz_render` runs with `--use-eot` (stop at the MIDI end instead of waiting for output silence, which a looping sustain patch never reaches), and a watchdog kills any tool that exceeds its wall-clock timeout or output-size cap, reporting the patch as `render_failed`. `SCOREKIT_TOOL_TIMEOUT_SECS` and `SCOREKIT_TOOL_MAX_OUTPUT_MB` override the limits.

`scorekit schema --profile` prints a leaf profile's JSON Schema.

## Instrument resolution and fallback

Leaf profile mappings are the author's ground truth: a mapped instrument
always resolves exactly. Instruments a scene requests but the active
palette's leaf profile does not map go through the instrument resolver
instead of failing outright — scoped to that one palette only:

- Same-family substitutes are scored on range, articulation, envelope, role,
  and timbre; the best candidate above the minimum score (default 0.70) is
  used, and the build prints a `WARN instrument fallback:` line naming the
  substitute, its score, and its reasons.
- Strings are never a default absorber: no missing brass, woodwind, or
  plucked instrument falls back to a string patch unless the resolver config
  explicitly lists `strings` in `allowed_families`.
- Drums are never substituted in either direction, and synth stand-ins
  require `--fallback-mode flexible` or `allow_synth: true`.
- When nothing qualifies the build fails with exit 2 (code `resolution`)
  before any file is staged, and the error carries the full resolution
  report including the best rejected candidate.
- **Fallback never crosses palettes.** A track routed to `solo` is only ever
  scored against `solo`'s leaf profile's own vocabulary, even when a
  different palette in the same orchestration maps the same instrument.

Preview the outcome without building —
`scorekit inspect-instruments scene.yaml --orchestration orchestration.yaml`
— or pin behavior with a resolver config
(`scorekit schema --resolver` prints its schema):

```yaml
# resolver.yaml
default_mode: conservative   # strict | conservative | flexible
minimum_score: 0.70
allow_cross_family: false
allow_synth: false
excluded_families: []        # families never used as substitutes
allowed_families: []         # explicit cross-family opt-ins (incl. strings)
```

Substitution only changes which SFZ patch is rendered; MIDI bytes, stem
names, and `meta.json` track names keep the requested instrument. The
`inspect-instruments` and build-time `instrument_resolution` reports carry,
per track: `track_id`, declared/effective `palette`, the resolved leaf
profile's `name`/`profile_path`, requested/effective `articulation`, and the
resolved `sfz` path.

## Texture source profiles

Texture profiles solve the same portability problem for field recordings,
ambience, and SFX. A scene uses a stable logical name; the external profile
binds it to a machine-local audio file:

```yaml
# scene.yaml
textures:
  - { source: river, mode: loop, gain: 0.25 }
  - { source: birds, mode: one_shot, at: [2, 10], gain: 0.5 }
```

```yaml
# textures.yaml
name: forest
root: /Volumes/Samples
sources:
  river: ambience/river.flac
  birds: wildlife/birds.wav
```

```bash
scorekit schema --texture-profile
scorekit build scene.yaml --texture-profile textures.yaml -o scene.ogg --stems
```

Source keys match `[a-z][a-z0-9_-]{0,63}`. Paths may be relative to `root`
(or the profile directory when `root` is absent) or absolute. Every source a
scene uses must be mapped to a real file; failure leaves no normalized,
arranged, output, stem, or metadata artifact behind.
