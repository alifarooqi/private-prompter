#!/usr/bin/env bash
#
# Notarize a built .dmg with Apple's notary service. Used by the release
# pipeline; can also be run locally during development.
#
# Required environment:
#   APPLE_ID            — Apple ID email used for the Developer account.
#   APPLE_PASSWORD       — App-Specific Password for that ID.
#   APPLE_TEAM_ID        — 10-character Team ID.
#
# Optional:
#   DMG_PATH             — Path to the .dmg to notarize. Defaults to the
#                          first .dmg found under src-tauri/target.
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ -z "${APPLE_ID:-}" ] || [ -z "${APPLE_PASSWORD:-}" ] || [ -z "${APPLE_TEAM_ID:-}" ]; then
  echo "ERROR: APPLE_ID, APPLE_PASSWORD, and APPLE_TEAM_ID must be set" >&2
  exit 1
fi

if [ -z "${DMG_PATH:-}" ]; then
  DMG_PATH=$(find "$REPO_ROOT/src-tauri/target" -name '*.dmg' -path '*/release/bundle/*' 2>/dev/null | head -1 || true)
fi

if [ -z "$DMG_PATH" ] || [ ! -f "$DMG_PATH" ]; then
  echo "ERROR: no .dmg found. Set DMG_PATH or build first." >&2
  exit 1
fi

echo "==> Submitting $DMG_PATH for notarization"
xcrun notarytool submit "$DMG_PATH" \
  --apple-id "$APPLE_ID" \
  --password "$APPLE_PASSWORD" \
  --team-id "$APPLE_TEAM_ID" \
  --wait

echo "==> Stapling notarization ticket"
xcrun stapler staple "$DMG_PATH"
xcrun stapler validate "$DMG_PATH"

echo "==> Done. $DMG_PATH is notarized."