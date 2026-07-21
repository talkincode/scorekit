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
