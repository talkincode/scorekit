#!/usr/bin/env sh
# Install MuseScore_General.sf2 0.2.0 from the official MuseScore mirror.
# The SoundFont and its FluidR3-derived samples are distributed under MIT.
#
# SCOREKIT_SOUNDFONT_URL / SCOREKIT_SOUNDFONT_LICENSE_URL /
# SCOREKIT_SOUNDFONT_SHA256 override the source, so `make test-install` can
# exercise this exact download/verify/rename path against local file:// URLs
# instead of pulling the real 200 MB asset.
set -eu

url=${SCOREKIT_SOUNDFONT_URL:-'https://ftp.osuosl.org/pub/musescore/soundfont/MuseScore_General/MuseScore_General.sf2'}
license_url=${SCOREKIT_SOUNDFONT_LICENSE_URL:-'https://ftp.osuosl.org/pub/musescore/soundfont/MuseScore_General/MuseScore_General_License.md'}
expected_sha256=${SCOREKIT_SOUNDFONT_SHA256:-'ee51d2c4b1525e70f19a45909c4fd7a2e26d91d115fa89dbf5a6bc413d8b9bf3'}

if test -n "${SCOREKIT_SOUND_LIBRARY_DIR:-}"; then
  library_dir=$SCOREKIT_SOUND_LIBRARY_DIR
elif test -n "${XDG_DATA_HOME:-}"; then
  library_dir=$XDG_DATA_HOME/scorekit/sounds
else
  library_dir=${HOME:?HOME is required}/.local/share/scorekit/sounds
fi

sf2_dir=$library_dir/sf2
destination=$sf2_dir/MuseScore_General.sf2
license=$sf2_dir/MuseScore_General_License.md
part=$destination.part
license_part=$license.part

sha256() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    sha256sum "$1" | awk '{print $1}'
  fi
}

cleanup() {
  if test -f "$part"; then unlink "$part"; fi
  if test -f "$license_part"; then unlink "$license_part"; fi
}
trap cleanup EXIT HUP INT TERM

mkdir -p "$sf2_dir"
if test -f "$destination" && test "$(sha256 "$destination")" = "$expected_sha256"; then
  if test ! -f "$license"; then
    curl --fail --location --retry 3 --output "$license_part" "$license_url"
    mv "$license_part" "$license"
  fi
  printf '%s\n' "$destination"
  exit 0
fi

curl --fail --location --retry 3 --output "$part" "$url"
actual_sha256=$(sha256 "$part")
if test "$actual_sha256" != "$expected_sha256"; then
  printf 'checksum mismatch for MuseScore_General.sf2: expected %s, got %s\n' \
    "$expected_sha256" "$actual_sha256" >&2
  exit 1
fi
curl --fail --location --retry 3 --output "$license_part" "$license_url"
mv "$part" "$destination"
mv "$license_part" "$license"
trap - EXIT HUP INT TERM
printf '%s\n' "$destination"
