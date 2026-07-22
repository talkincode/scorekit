# scorekit — pinned, reproducible toolchain image.
#
# Determinism travels with tool versions: this image pins the base distro,
# FluidSynth, FFmpeg, and the default SoundFont (verified by SHA-256 via
# scripts/fetch_default_soundfont.sh) so the same scene compiles to the same
# assets on any host. scorekit itself stays a single-invocation CLI; if you
# need an HTTP API or an MCP endpoint, put your own thin gateway in front of
# this image (`scorekit mcp` serves MCP over stdio out of the box).
#
#   docker build -t scorekit .
#   docker run --rm -v "$PWD:/work" -w /work scorekit build scene.yaml -o scene.ogg --stems
#   docker run --rm -i scorekit mcp        # stdio MCP server

FROM rust:1.88.0-bookworm@sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0 AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim@sha256:7b140f374b289a7c2befc338f42ebe6441b7ea838a042bbd5acbfca6ec875818
# Freeze the complete APT dependency graph through an immutable Debian
# snapshot, then also name the top-level package versions as executable
# documentation. Rebuilding the same source either gets these exact tools or
# fails closed; it never silently advances the audio toolchain.
ARG DEBIAN_SNAPSHOT=20260713T000000Z
RUN printf '%s\n' \
      "deb [check-valid-until=no] http://snapshot.debian.org/archive/debian/${DEBIAN_SNAPSHOT} bookworm main" \
      "deb [check-valid-until=no] http://snapshot.debian.org/archive/debian-security/${DEBIAN_SNAPSHOT} bookworm-security main" \
      > /etc/apt/sources.list \
    && rm -f /etc/apt/sources.list.d/debian.sources \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
      fluidsynth=2.3.1-2 \
      ffmpeg=7:5.1.9-0+deb12u1 \
      curl=7.88.1-10+deb12u15 \
      ca-certificates=20230311+deb12u1 \
    && rm -rf /var/lib/apt/lists/* \
    && fluidsynth --version && ffmpeg -version

# Default GM SoundFont, checksum-pinned (MIT-licensed MuseScore General).
ENV SCOREKIT_SOUND_LIBRARY_DIR=/opt/scorekit/sounds
COPY scripts/fetch_default_soundfont.sh /tmp/fetch_default_soundfont.sh
RUN sh /tmp/fetch_default_soundfont.sh \
    && mkdir -p "$SCOREKIT_SOUND_LIBRARY_DIR/sfz" "$SCOREKIT_SOUND_LIBRARY_DIR/profiles" \
    && rm /tmp/fetch_default_soundfont.sh

COPY --from=build /src/target/release/scorekit /usr/local/bin/scorekit
RUN scorekit doctor

WORKDIR /work
ENTRYPOINT ["scorekit"]
CMD ["doctor"]
