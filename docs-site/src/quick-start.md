# Quick Start

Check the local environment first:

```bash
scorekit doctor
scorekit --json doctor
```

Validate and build the bundled forest scene with an SF2 SoundFont:

```bash
scorekit validate examples/scenes/forest.yaml
scorekit build examples/scenes/forest.yaml \
  --stems \
  -o out/forest.ogg
```

The build writes the full mix, one sample-aligned file per track, and a metadata file containing exact sample counts and loop information.

For an Agent-authored scene, query the live schema before writing YAML:

```bash
scorekit schema > scene.schema.json
scorekit --json validate scene.yaml
```

Use `scorekit diff old.yaml new.yaml` to review musical changes independently of YAML formatting.
