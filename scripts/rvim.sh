#!/usr/bin/env bash
# Convenience launcher for rvim (bash).
# Prefers the release binary; falls back to `cargo run`.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin="$here/target/release/rvim"
[[ -x "$bin" ]] || bin="$here/target/release/rvim.exe"

if [[ -x "$bin" ]]; then
    exec "$bin" "$@"
else
    exec cargo run --release --manifest-path "$here/Cargo.toml" -- "$@"
fi
