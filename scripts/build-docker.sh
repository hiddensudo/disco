#!/bin/sh
# Compile in Docker and export a native Linux bundle, without host Rust/dev packages.
set -eu
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
out=${1:-"$repo/dist"}
if [ "$#" -gt 1 ]; then
    printf '%s\n' 'Usage: scripts/build-docker.sh [output-directory]' >&2
    exit 2
fi
case "$out" in *,*) printf '%s\n' 'The output path must not contain a comma (Docker exporter syntax).' >&2; exit 2;; esac
mkdir -p -- "$out"
out=$(CDPATH= cd -- "$out" && pwd)
if [ -n "${DISCO_BUILDX:-}" ]; then
    exec "$DISCO_BUILDX" build --progress=plain --target artifacts --output "type=local,dest=$out" "$repo"
fi
if ! docker buildx version >/dev/null 2>&1; then
    printf '%s\n' 'Docker Buildx is required. Install the Buildx plugin for your Docker Engine:' \
        'https://docs.docker.com/build/buildx/install/' >&2
    exit 1
fi
exec docker buildx build --progress=plain --target artifacts --output "type=local,dest=$out" "$repo"
