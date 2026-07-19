# Installation

## Prebuilt binaries

GitHub Releases publish these archives:

- `scorekit-x86_64-unknown-linux-gnu.tar.gz`
- `scorekit-aarch64-unknown-linux-gnu.tar.gz`
- `scorekit-x86_64-apple-darwin.tar.gz`
- `scorekit-aarch64-apple-darwin.tar.gz`
- `scorekit-x86_64-pc-windows-msvc.zip`

Each release also includes `SHA256SUMS`. Extract the matching archive and place `scorekit` or `scorekit.exe` on `PATH`.

## Homebrew

On macOS or Linux:

```bash
brew install talkincode/tap/scorekit
scorekit doctor
```

The Homebrew formula is updated automatically on tagged releases. It installs the matching prebuilt archive, FFmpeg, FluidSynth, the Agent skill under Homebrew's package share directory, and the default MuseScore General SoundFont. The installed wrapper sets `SCOREKIT_SOUND_LIBRARY_DIR` to that package-managed sound library. Set `SCOREKIT_SOUND_LIBRARY_DIR` yourself or pass `--soundfont` when you want a project-managed sound library instead.

## Install from a source checkout

Rust is required for a source build.

```bash
git clone https://github.com/talkincode/scorekit.git
cd scorekit
make install
```

By default this installs `scorekit` and `sfizz_render` to `~/.local/bin/`, installs the Agent skill to `~/.agents/skills/scorekit`, creates a user-managed sound root at `~/.local/share/scorekit/sounds/`, and downloads the official MIT-licensed MuseScore General 0.2.0 SoundFont.

```bash
make install PREFIX=/usr/local
make install-skill SKILLS_DIR="$HOME/.codex/skills"
make install-sound-dir SCOREKIT_SOUND_LIBRARY_DIR="/Volumes/Samples/scorekit"
make install-default-soundfont
```

The sound root contains `sf2/`, `sfz/`, and `profiles/` directories. SF2 builds default to `sf2/MuseScore_General.sf2`; an explicit `--soundfont` overrides it. SFZ builds continue to require an explicit renderer profile.

Run `scorekit doctor` after installation. It reports the runtime OS and architecture, probes external dependencies, and prints platform-specific setup guidance.
