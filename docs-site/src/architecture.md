# Architecture and Guarantees

scorekit is a thin compiler and orchestration layer.

```text
YAML scene
  -> semantic validation
  -> Score IR
  -> deterministic MIDI
  -> external renderer
  -> PCM audio
  -> external FFmpeg export
```

## Guarantees

- The same DSL and compiler inputs produce byte-identical MIDI.
- Loop and stem lengths are derived from quantized musical time.
- File-writing commands stage output and publish it atomically.
- JSON Schema and structured errors expose the same contract used by the CLI.
- Renderer profiles are external data, so scenes remain portable.

## Deliberate boundaries

scorekit does not implement synthesis, reverb, compression, a DAW, version control, or embedded compositional intelligence. It also does not require fields that only a commercial sound source can satisfy.

The upstream Agent owns narrative and creative decisions. scorekit owns deterministic compilation, validation, orchestration, and artifact integrity.
