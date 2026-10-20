#!/bin/sh
set -eu
: "${CARGO_TARGET_DIR:=/build-cache/target}"
case "$CARGO_TARGET_DIR" in
  /*) ;;
  *) printf '%s\n' 'CARGO_TARGET_DIR must be absolute for this witness' >&2; exit 2 ;;
esac
export CARGO_TARGET_DIR
EDICT_SOURCE="$(pwd)"
EDICT_BINARY="$CARGO_TARGET_DIR/debug/edict"
export EDICT_SOURCE EDICT_BINARY
cargo build --locked -p edict-cli
exec python3 scripts/consumer-witnesses/jedit-state-read.py
