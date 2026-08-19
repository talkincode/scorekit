# Introduction

scorekit is an Agent-oriented music compiler for game and film-style scoring workflows. It compiles a reviewable YAML scene into deterministic MIDI, then delegates audio rendering and export to established external tools. The same scene can also compile to a MakeCode synth-song for Arcade and micro:bit V2.

The compiler produces seamless loops, sample-aligned instrument/texture stems, suite sections, metadata, OGG or WAV assets, and optional MakeCode `music.createSong` TypeScript. Texture tracks bring deterministic field recordings, ambience, and SFX into the score timeline. Creative decisions stay in the upstream Agent and the text scene; scorekit does not contain a generative model.

```text
scene.yaml -> Score IR -> MIDI -> renderer -> WAV -> FFmpeg -> game assets
                     \-> MakeCode song TypeScript
```

The supported render backends are FluidSynth and TiMidity++ for SF2 SoundFonts, and sfizz for SFZ sample libraries.

## Design priorities

- Deterministic MIDI for the same scene and toolchain.
- Portable, diff-friendly text inputs.
- Atomic file output: failed builds do not leave partial assets.
- External DSP and rendering tools instead of in-house synthesis.
- Machine-readable schemas, diagnostics, and reports for Agent workflows.
