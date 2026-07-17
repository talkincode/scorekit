#!/usr/bin/env bash
# Fetch test/dev assets that are not committed to the repo.
# TimGM6mb.sf2 (Tim Brechbill, GPLv2) — small GM SoundFont used for tests only.
set -euo pipefail

dir="$(cd "$(dirname "$0")/.." && pwd)/assets"
mkdir -p "$dir"
sf2="$dir/TimGM6mb.sf2"

if [ ! -f "$sf2" ]; then
  echo "downloading TimGM6mb.sf2 ..." >&2
  curl -fsSL -o "$sf2.part" \
    "https://github.com/craffel/pretty-midi/raw/main/pretty_midi/TimGM6mb.sf2"
  mv "$sf2.part" "$sf2"
fi

echo "$sf2"
