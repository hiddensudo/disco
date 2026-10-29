#!/bin/sh
set -eu
prototype_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if [ -x "$prototype_dir/disco-terminal-prototype" ]; then
    exec "$prototype_dir/disco-terminal-prototype" "$@"
fi
exec cargo run --manifest-path "$prototype_dir/Cargo.toml" --locked -- "$@"
