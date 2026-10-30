# syntax=docker/dockerfile:1
# Debian 12 / glibc 2.36 baseline. Update the tag and digest together.
FROM rust:1.95.0-bookworm@sha256:6258907abe69656e41cd992e0b705cdcfabcbbe3db374f92ed2d47121282d4a1 AS build
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libx11-dev libxi-dev libxcursor-dev libxrandr-dev libxinerama-dev \
    libxkbcommon-dev libwayland-dev libegl1-mesa-dev libgl1-mesa-dev libudev-dev \
    python3 && rm -rf /var/lib/apt/lists/*
WORKDIR /source
COPY Cargo.toml Cargo.lock ./
COPY vendor ./vendor
COPY prototypes ./prototypes
RUN mkdir src && printf 'fn main() {}\n' > src/main.rs
RUN --mount=type=cache,id=disco-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=disco-git,target=/usr/local/cargo/git,sharing=locked \
    cargo fetch --locked
COPY src ./src
COPY assets ./assets
COPY docs ./docs
COPY licenses ./licenses
COPY install.py disco.desktop README.md README.uk.md CHANGELOG.md categories.example.toml layout.default.toml ./
COPY scripts/package.py ./scripts/package.py
ARG TARGETARCH
RUN --mount=type=cache,id=disco-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=disco-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=disco-target-${TARGETARCH},target=/source/target,sharing=locked \
    cargo build --release --locked --bin disco && \
    ./target/release/disco --version && \
    python3 scripts/package.py target/release/disco /out "${TARGETARCH}"

# No compiler, container runtime or build cache is needed after extraction.
FROM scratch AS artifacts
COPY --from=build /out/ /
