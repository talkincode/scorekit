# Rendering and Dependencies

scorekit intentionally delegates synthesis and post-processing. `doctor` considers the environment ready when FFmpeg and at least one renderer are executable on `PATH`.

| Tool | Role | Requirement |
| --- | --- | --- |
| FFmpeg | Audio conversion and export | Required for complete audio builds |
| FluidSynth | Primary SF2 renderer; uses MuseScore General by default | At least one renderer is required |
| TiMidity++ | Alternate SF2 renderer | Optional |
| `sfizz_render` | SFZ renderer | Optional |

On macOS, install the standard dependencies with:

```bash
brew install fluid-synth timidity ffmpeg
```

On Debian or Ubuntu:

```bash
sudo apt-get install fluidsynth timidity ffmpeg
```

Homebrew and prebuilt scorekit archives do not bundle `sfizz_render` inside the main `scorekit` package. Install the optional backend with `brew install talkincode/tap/scorekit-sfizz`, or from a source checkout use `make install` / `make sfizz`. Apple Silicon builds `sfizz_render` from source because upstream macOS binaries are x86_64-only.

`make install` downloads the official MuseScore General 0.2.0 SF2 and its MIT license to `~/.local/share/scorekit/sounds/sf2/`, or a custom `SCOREKIT_SOUND_LIBRARY_DIR`. FluidSynth and TiMidity use this file when `--soundfont` is omitted. An explicit SF2 overrides the default; sfizz still requires an explicit external renderer profile. `doctor` validates the default file's SF2 header and reports `ok`, `missing`, or `invalid`.
