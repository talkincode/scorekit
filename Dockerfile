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

FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim
# bookworm ships FluidSynth 2.3.x and FFmpeg 5.1.x; the distro release pins
# the tool major/minor versions this image guarantees.
RUN apt-get update \
    && apt-get install -y --no-install-recommends fluidsynth ffmpeg curl ca-certificates \
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
