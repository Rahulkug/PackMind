#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT/target/release/prefixgraph"
EXAMPLE="$ROOT/examples/small-python-service"

if [[ ! -x "$BIN" ]]; then
  echo "Building PrefixGraph release binary..."
  cargo build --release --manifest-path "$ROOT/Cargo.toml"
fi

echo
echo "== PrefixGraph playground =="
echo "Example repo: $EXAMPLE"
echo

run() {
  echo
  echo "$ $*"
  "$@"
}

run "$BIN" init "$EXAMPLE"
run "$BIN" --repo "$EXAMPLE" index --force
run "$BIN" --repo "$EXAMPLE" status
run "$BIN" --repo "$EXAMPLE" search "PaymentValidator FxRateService"
run "$BIN" --repo "$EXAMPLE" tests payments.py
run "$BIN" --repo "$EXAMPLE" impact PaymentValidator --depth 2
run "$BIN" --repo "$EXAMPLE" ask-context \
  "What should I read before changing PaymentValidator?" \
  --budget 900

echo
echo "Next:"
echo "  $BIN --repo $EXAMPLE pack \"Explain the payment flow\" --budget 900 --render plain"
echo "  $BIN --repo $EXAMPLE mcp"

