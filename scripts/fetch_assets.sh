#!/usr/bin/env bash
# Fetch test/dev assets that are not committed to the repo.
# TimGM6mb.sf2 (Tim Brechbill, GPLv2) — small GM SoundFont used for tests only.
set -euo pipefail

dir="$(cd "$(dirname "$0")/.." && pwd)/assets"
mkdir -p "$dir"
sf2="$dir/TimGM6mb.sf2"
expected_sha256='82475b91a76de15cb28a104707d3247ba932e228bada3f47bba63c6b31aaf7a1'
part="$sf2.part"

sha256() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    sha256sum "$1" | awk '{print $1}'
  fi
}

cleanup() {
  if [ -f "$part" ]; then unlink "$part"; fi
}
trap cleanup EXIT HUP INT TERM

if [ ! -f "$sf2" ] || [ "$(sha256 "$sf2")" != "$expected_sha256" ]; then
  echo "downloading TimGM6mb.sf2 ..." >&2
  curl -fsSL -o "$part" \
    "https://github.com/craffel/pretty-midi/raw/main/pretty_midi/TimGM6mb.sf2"
  actual_sha256=$(sha256 "$part")
  if [ "$actual_sha256" != "$expected_sha256" ]; then
    printf 'checksum mismatch for TimGM6mb.sf2: expected %s, got %s\n' \
      "$expected_sha256" "$actual_sha256" >&2
    exit 1
  fi
  mv "$part" "$sf2"
fi

trap - EXIT HUP INT TERM
echo "$sf2"
