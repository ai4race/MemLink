#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT_DIR="${1:-dist}"
NAME="memlink-rust2024"
ARCHIVE="$OUT_DIR/$NAME.tar.gz"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

# Ensure the source package is reproducible and excludes generated outputs.
cargo fmt --all -- --check
cargo test --workspace >/tmp/memlink-package-test.log

tar \
  --exclude='./target' \
  --exclude='./data' \
  --exclude='./reports' \
  --exclude='./dist' \
  --exclude='./.git' \
  -czf "$ARCHIVE" \
  Cargo.toml Cargo.lock README.md LICENSE .gitignore .github crates docs suites tasks scripts

sha256sum "$ARCHIVE" > "$ARCHIVE.sha256" 2>/dev/null || shasum -a 256 "$ARCHIVE" > "$ARCHIVE.sha256"

echo "archive=$ARCHIVE"
echo "checksum=$ARCHIVE.sha256"
