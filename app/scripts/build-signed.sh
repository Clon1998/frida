#!/usr/bin/env bash
# Builds a signed macOS app (PLAN.md M13).
#
#   scripts/build-signed.sh dev      Apple Development identity + "Frida Dev" profile: runs on the
#                                    Macs registered in that profile; no notarization.
#   scripts/build-signed.sh release  Developer ID identity + "Frida Release" profile, notarized:
#                                    runs on any Mac.
#
# Profiles are read from $FRIDA_SIGNING_DIR (default ~/.frida-signing) and copied to
# src-tauri/signing/ (git-ignored) for the build. Release builds need APPLE_API_ISSUER,
# APPLE_API_KEY and APPLE_API_KEY_PATH (the .p8 file) for notarization.
set -euo pipefail

mode="${1:-dev}"
cd "$(dirname "$0")/.."
signing_dir="${FRIDA_SIGNING_DIR:-$HOME/.frida-signing}"

case "$mode" in
  dev)
    profile="$signing_dir/Frida_Dev.provisionprofile"
    identity_kind="Apple Development"
    ;;
  release)
    profile="$signing_dir/Frida_Release.provisionprofile"
    identity_kind="Developer ID Application"
    for var in APPLE_API_ISSUER APPLE_API_KEY APPLE_API_KEY_PATH; do
      if [ -z "${!var:-}" ]; then
        echo "error: $var must be set for notarization" >&2
        exit 1
      fi
    done
    ;;
  *)
    echo "usage: $0 dev|release" >&2
    exit 1
    ;;
esac

if [ ! -f "$profile" ]; then
  echo "error: provisioning profile not found: $profile" >&2
  exit 1
fi

# The signing identity: APPLE_SIGNING_IDENTITY if set, else the first matching one in the
# keychain (e.g. "Developer ID Application: Patrick Schmidt (Q4XB68D4T7)").
if [ -z "${APPLE_SIGNING_IDENTITY:-}" ]; then
  APPLE_SIGNING_IDENTITY="$(security find-identity -v -p codesigning \
    | sed -n "s/.*\"\($identity_kind: .*\)\"/\1/p" | head -n 1)"
fi
if [ -z "$APPLE_SIGNING_IDENTITY" ]; then
  echo "error: no \"$identity_kind\" certificate in the keychain" >&2
  exit 1
fi
export APPLE_SIGNING_IDENTITY
echo "Signing with: $APPLE_SIGNING_IDENTITY"

mkdir -p src-tauri/signing
cp "$profile" src-tauri/signing/embedded.provisionprofile

pnpm tauri build --bundles app,dmg --config src-tauri/tauri.signed.conf.json "${@:2}"
