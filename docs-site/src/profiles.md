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
