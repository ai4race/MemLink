#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cargo fmt --all -- --check
cargo test --workspace
rm -rf reports/demo data
cargo run -p memlink-cli -- demo --rounds "${MEMLINK_DEMO_ROUNDS:-10}" --output-dir reports/demo
cargo run -p memlink-cli -- audit \
  --input-dir reports/demo \
  --min-tasks "${MEMLINK_DEMO_ROUNDS:-10}" \
  --min-state-files 1 \
  --min-memory-hits 1 \
  --min-text-saving "${MEMLINK_MIN_TEXT_SAVING:-0.0}" \
  --min-byte-saving "${MEMLINK_MIN_BYTE_SAVING:-0.0}"

test -s reports/demo/report.md
test -s reports/demo/text-events.jsonl
test -s reports/demo/structured-events.jsonl
test -s reports/demo/observe.json
test -s reports/demo/memory-search.json
STATE_COUNT="$(find reports/demo/state -type f | wc -l | tr -d ' ')"
if [[ "$STATE_COUNT" -lt 1 ]]; then
  echo "expected at least one state file, got $STATE_COUNT" >&2
  exit 1
fi

echo "verify ok: state_files=$STATE_COUNT"
