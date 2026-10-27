# VTNS for Linux: .deb and .AppImage, built in Ubuntu 22.04 so the result runs
# on glibc 2.35 and newer (Ubuntu 22.04+, Debian 12+, Raspberry Pi OS
# bookworm+). The context is vtpaul-rs; Dockerfile.dockerignore keeps its
# build output out.
#
# build_targets.ps1 runs it; by hand, from vtpaul-rs:
#     docker build --platform linux/amd64 -f gui/linux/Dockerfile \
#         --output type=local,dest=out .
#
# Built natively on each host (Docker Desktop for amd64, the Pi 5 for arm64):
# under qemu the release build with LTO takes hours.

FROM ubuntu:22.04 AS base

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential ca-certificates curl wget file pkg-config \
        libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev \
        xdg-utils \
        gstreamer1.0-plugins-base gstreamer1.0-plugins-good patchelf \
    && rm -rf /var/lib/apt/lists/*

RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
ENV PATH=/root/.cargo/bin:$PATH
RUN --mount=type=cache,target=/root/.cargo/registry \
    cargo install tauri-cli --version "^2" --locked

WORKDIR /src/vtpaul-rs
# rust-toolchain.toml pins the compiler; installing it here keeps it in a
# cached layer instead of downloading it on every build
COPY rust-toolchain.toml .
RUN rustup toolchain install

FROM base AS build
# --build-arg TAURI_FLAGS=--verbose shows linuxdeploy's own output on failure
ARG TAURI_FLAGS=""
COPY . .
# Speak plays through WebKit's <audio>, which is GStreamer; the AppImage
# carries the plugins above (bundleMediaFramework in tauri.conf.json), or
# WebKit finds no autoaudiosink and its web process crashes. The gstreamer
# linuxdeploy plugin needs patchelf.
# linuxdeploy is itself an AppImage, and a container has no FUSE to mount it;
# its strip is too old for current toolchains' sections, hence NO_STRIP.
ENV APPIMAGE_EXTRACT_AND_RUN=1 \
    NO_STRIP=true \
    CARGO_TARGET_DIR=/target
RUN --mount=type=cache,target=/root/.cargo/registry \
    --mount=type=cache,target=/target \
    cd gui \
    && rm -rf /target/release/bundle \
    && cargo tauri build --bundles deb,appimage $TAURI_FLAGS \
    && mkdir /out \
    && cp /target/release/bundle/deb/*.deb /target/release/bundle/appimage/*.AppImage /out/

FROM scratch
COPY --from=build /out/ /
