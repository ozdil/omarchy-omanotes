#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$DIR"

echo "Building omanotes-engine from source..."
cargo build --release --locked
install -m 755 target/release/omanotes-engine ./omanotes-engine

echo "omanotes-engine successfully compiled and placed at ${DIR}/omanotes-engine"

if command -v omarchy-restart-shell >/dev/null 2>&1; then
    echo "Reloading Omarchy shell..."
    omarchy-restart-shell >/dev/null 2>&1 || true
fi

echo "OmaNotes is ready."
