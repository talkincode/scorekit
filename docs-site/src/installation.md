# Installation

## Recommended: Homebrew (macOS/Linux)

1. Install scorekit:

```bash
brew install talkincode/tap/scorekit
```

2. Optional: install SFZ rendering backend (`--renderer sfizz`):

```bash
brew install talkincode/tap/scorekit-sfizz
```

3. Verify the install:

```bash
scorekit --version
scorekit doctor
```

If `doctor` reports ready renderers and dependencies, you're ready to build scenes.

Homebrew installs the matching prebuilt `scorekit` archive, FFmpeg, FluidSynth, the Agent skill, and the default MuseScore General SoundFont. `scorekit-sfizz` adds the optional `sfizz_render` backend for SFZ profiles. The installed wrapper sets `SCOREKIT_SOUND_LIBRARY_DIR` to Homebrew-managed sounds.

The Homebrew formulae are updated automatically on tagged releases.

## Install from prebuilt binaries (no package manager)

GitHub Releases publish:

- `scorekit-x86_64-unknown-linux-gnu.tar.gz`
- `scorekit-aarch64-unknown-linux-gnu.tar.gz`
- `scorekit-x86_64-apple-darwin.tar.gz`
- `scorekit-aarch64-apple-darwin.tar.gz`
- `scorekit-x86_64-pc-windows-msvc.zip`

Each release also includes `SHA256SUMS`. Extract the matching archive, place `scorekit` (or `scorekit.exe`) on `PATH`, then run:

```bash
scorekit --version
scorekit doctor
```

## Install from a source checkout

A source build requires Rust plus runtime tools (`fluidsynth` and `ffmpeg`).

```bash
git clone https://github.com/talkincode/scorekit.git
cd scorekit
make install
scorekit --version
scorekit doctor
```

By default this installs `scorekit` and `sfizz_render` to `~/.local/bin/`, installs the Agent skill to `~/.agents/skills/scorekit`, creates a user-managed sound root at `~/.local/share/scorekit/sounds/`, and downloads the official MIT-licensed MuseScore General 0.2.0 SoundFont.

```bash
make install PREFIX=/usr/local
make install-skill SKILLS_DIR="$HOME/.codex/skills"
make install-sound-dir SCOREKIT_SOUND_LIBRARY_DIR="/Volumes/Samples/scorekit"
make install-default-soundfont
```

The sound root contains `sf2/`, `sfz/`, and `profiles/`. SF2 builds default to `sf2/MuseScore_General.sf2`; an explicit `--soundfont` overrides it. SFZ builds still require an explicit renderer profile.

## Using a project-managed sound library

If you do not want the Homebrew-managed default, set your own sound root:

```bash
export SCOREKIT_SOUND_LIBRARY_DIR="$HOME/.local/share/scorekit/sounds"
scorekit doctor
```

Or pass `--soundfont` explicitly for a build.
