# Introduction

scorekit is an Agent-oriented music compiler for game and film-style scoring workflows. It compiles a reviewable YAML scene into deterministic MIDI, then delegates audio rendering and export to established external tools.

The compiler produces seamless loops, sample-aligned stems, suite sections, metadata, and OGG or WAV assets. Creative decisions stay in the upstream Agent and the text scene; scorekit does not contain a generative model.

```text
scene.yaml -> Score IR -> MIDI -> renderer -> WAV -> FFmpeg -> game assets
```

The supported render backends are FluidSynth and TiMidity++ for SF2 SoundFonts, and sfizz for SFZ sample libraries.

## Design priorities

- Deterministic MIDI for the same scene and toolchain.
- Portable, diff-friendly text inputs.
- Atomic file output: failed builds do not leave partial assets.
- External DSP and rendering tools instead of in-house synthesis.
- Machine-readable schemas, diagnostics, and reports for Agent workflows.
