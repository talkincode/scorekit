# SFZ Renderer Profiles

A renderer profile keeps machine-specific sample paths out of portable scene files. It maps scorekit instruments and articulations to local SFZ patches.

```yaml
name: orchestral
root: /Volumes/Samples
instruments:
  violin:
    sustain: VSCO/Strings/Violin-Sustain.sfz
    pizzicato: VSCO/Strings/Violin-Pizzicato.sfz
  drums:
    sustain: Virtuosity/Programs/01-basic-kit.sfz
```

Every instrument requires a `sustain` mapping, which is also the fallback when a dedicated articulation is absent.

Certify a profile before building music:

```bash
scorekit profile check profile.yaml
scorekit --json profile check profile.yaml > profile-report.json
```

The check deduplicates shared patch paths, renders melodic or drum probes twice, rejects missing and silent patches, captures sfizz warnings, and checks repeatability. Temporary probe files are removed on success and failure.

Use the profile with the sfizz backend:

```bash
scorekit build scene.yaml --renderer sfizz --profile profile.yaml -o scene.ogg
```

## Instrument resolution and fallback

Profile mappings are the author's ground truth: a mapped instrument always
resolves exactly. Instruments a scene requests but the profile does not map
go through the instrument resolver instead of failing outright:

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

Preview the outcome without building — `scorekit inspect-instruments
scene.yaml --profile profile.yaml` — or pin behavior with a resolver config
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
names, and `meta.json` track names keep the requested instrument, and the
report is embedded as `instrument_resolution` in `meta.json`.

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
