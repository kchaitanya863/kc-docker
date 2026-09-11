#!/bin/sh
# Convenience wrapper for check_parity.py
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
exec python3 "${SCRIPT_DIR}/scripts/check_parity.py" "$@"
