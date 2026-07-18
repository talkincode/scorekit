#!/usr/bin/env bash
# Build `sfizz_render` (offline SFZ -> WAV renderer) from source and install
# it to assets/bin/sfizz_render, for `--renderer sfizz` and its tests.
#
# Why build instead of `brew install sfizz`: sfizz isn't in Homebrew, and the
# official 1.2.3 macOS release binary is x86_64-only. On Apple Silicon
# without a working Rosetta, building for arm64 is the only option. Two
# small source patches are needed on top of the vanilla 1.2.3 tag for a
# clean arm64 clang build (both harmless upstream — arm64 just isn't the
# configuration the original code paths were written for):
#   1. cmake/SfizzConfig.cmake: its 32-bit-ARM regex also matched "arm64" and
#      appended -mfpu=neon/-mfloat-abi=hard, flags clang on macOS arm64
#      rejects outright. Narrowed to 32-bit ARM only.
#   2. external/atomic_queue: a `template` disambiguator before a
#      non-dependent name is a hard error on newer clang
#      (-Wmissing-template-arg-list-after-template-kw). Removed.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
bin_dir="$root/assets/bin"
out="$bin_dir/sfizz_render"

if [ -x "$out" ]; then
  echo "$out"
  exit 0
fi

work="$(mktemp -d)"
cleanup() {
  if [[ -f "$out.part" ]]; then
    rm -f "$out.part"
  fi
  if [[ -d "$work" ]]; then
    find "$work" -depth -delete
  fi
}
trap cleanup EXIT

echo "cloning sfizz 1.2.3 ..." >&2
git clone --branch 1.2.3 --depth 1 --recursive \
  https://github.com/sfztools/sfizz.git "$work/sfizz"
cd "$work/sfizz"

echo "patching for arm64 clang ..." >&2
# 1. Don't apply 32-bit ARM NEON flags to arm64.
patched="$work/SfizzConfig.cmake"
sed \
  -e 's/PROJECT_SYSTEM_PROCESSOR MATCHES "(arm.*)"/PROJECT_SYSTEM_PROCESSOR MATCHES "(arm(v[0-9].*)?)$"/' \
  cmake/SfizzConfig.cmake > "$patched"
mv "$patched" cmake/SfizzConfig.cmake
# 2. Drop the `template` disambiguator newer clang rejects here.
patched="$work/atomic_queue.h"
sed \
  -e 's/Base::template do_pop_any/Base::do_pop_any/' \
  -e 's/Base::template do_push_any/Base::do_push_any/' \
  external/atomic_queue/include/atomic_queue/atomic_queue.h > "$patched"
mv "$patched" external/atomic_queue/include/atomic_queue/atomic_queue.h

echo "building sfizz_render (this takes a few minutes) ..." >&2
cmake -B build -DCMAKE_BUILD_TYPE=Release \
  -DSFIZZ_JACK=OFF -DSFIZZ_RENDER=ON -DSFIZZ_SHARED=OFF \
  -DSFIZZ_TESTS=OFF -DSFIZZ_DEMOS=OFF -DSFIZZ_BENCHMARKS=OFF
jobs="$(getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)"
cmake --build build -j"$jobs" --target sfizz_render

mkdir -p "$bin_dir"
cp build/library/bin/sfizz_render "$out.part"
mv "$out.part" "$out"
chmod +x "$out"

echo "$out"
