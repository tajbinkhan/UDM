# Pullbay on a server or NAS, without a window (docs/server.md).
#
#   docker build -t pullbay .
#   docker run -d --name pullbay -v pullbay-config:/config -v /srv/downloads:/downloads pullbay
#   docker exec pullbay pullbay-cli get https://example.com/file.zip --no-wait
#
# The daemon runs in server mode: it stays up, logs to standard output
# (docker logs), and listens on 127.0.0.1 inside the container only, so the
# image publishes no port. pullbay-cli and pullbay-mcp in the container reach
# it through `docker exec`.
#
# Build with --build-arg FFMPEG=1 to add ffmpeg, which joins the video and
# audio of HLS and DASH streams.

# The build: the whole workspace's dependencies are compiled, the window's
# too (they share one package), so the GTK headers are needed here but not in
# the image.
FROM rust:1-slim-bookworm AS build
RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config libssl-dev libgtk-3-dev \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p pullbay-app \
      --bin pullbay-daemon --bin pullbay-cli --bin pullbay-mcp \
 && mkdir -p /out \
 && cp target/release/pullbay-daemon target/release/pullbay-cli target/release/pullbay-mcp /out/

# The image: the three programs, OpenSSL for HTTPS and FTPS, the CA
# certificates, the time zones (TZ sets the one queue schedules use), and
# tini, which hands `docker stop`'s signal to the daemon so it saves its
# list before it exits.
FROM debian:bookworm-slim
ARG FFMPEG=0
ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libssl3 tzdata tini \
 && if [ "$FFMPEG" = 1 ]; then apt-get install -y --no-install-recommends ffmpeg; fi \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --uid 1000 --user-group --create-home --home-dir /home/pullbay pullbay \
 && mkdir -p /config /downloads \
 && chown pullbay:pullbay /config /downloads
COPY --from=build /out/ /usr/local/bin/

# Every program in the container finds the same folders, so `docker exec`
# reaches the running daemon with its token.
ENV PULLBAY_CONFIG_DIR=/config \
    PULLBAY_DATA_DIR=/config/data \
    PULLBAY_DOWNLOADS_DIR=/downloads
VOLUME ["/config", "/downloads"]
USER pullbay
WORKDIR /downloads

LABEL org.opencontainers.image.title="Pullbay" \
      org.opencontainers.image.description="Pullbay's download daemon in server mode" \
      org.opencontainers.image.source="https://github.com/tajbinkhan/pullbay" \
      org.opencontainers.image.licenses="MIT"

ENTRYPOINT ["/usr/bin/tini", "--", "pullbay-daemon", "--server"]
