#!/usr/bin/env bash
# Writes the updater manifest `latest.json` (PLAN.md M14) from the signed release assets.
#
#   scripts/update-manifest.sh <tag> <assets dir> [notes file] > latest.json
#
# <assets dir> holds the release's `.sig` files (the installers themselves aren't needed: the
# manifest only links to them). Every installer type gets its own entry
# (`{os}-{arch}-{installer}`, e.g. `linux-x86_64-deb`), so an app installed from a `.deb` is
# updated with a `.deb`; `{os}-{arch}` is the fallback (NSIS on Windows, AppImage on Linux).
set -euo pipefail

tag="$1"
dir="$2"
notes_file="${3:-}"
version="${tag#v}"
base="https://github.com/${GITHUB_REPOSITORY:-Clon1998/frida}/releases/download/$tag"

notes=""
if [ -n "$notes_file" ] && [ -f "$notes_file" ]; then
  notes="$(cat "$notes_file")"
fi

platforms='{}'
add() { # add <key> <installer file name>
  local sig="$dir/$2.sig"
  if [ ! -f "$sig" ]; then
    echo "warning: no signature for $2, skipping $1" >&2
    return
  fi
  platforms=$(jq --arg key "$1" --arg url "$base/$2" --rawfile sig "$sig" \
    '. + {($key): {url: $url, signature: ($sig | rtrimstr("\n"))}}' <<<"$platforms")
}

# The first matching file, by pattern (names contain the version and the architecture).
find_one() { find "$dir" -maxdepth 1 -name "$1.sig" -exec basename {} .sig \; | sort | head -1; }

for arch in aarch64 x86_64; do
  app=$(find_one "Frida_*_${arch}.app.tar.gz")
  if [ -n "$app" ]; then
    add "darwin-$arch" "$app"
    add "darwin-$arch-app" "$app"
  fi
done

nsis=$(find_one "Frida_*_x64-setup.exe")
msi=$(find_one "Frida_*_x64_*.msi")
[ -n "$nsis" ] && add "windows-x86_64" "$nsis" && add "windows-x86_64-nsis" "$nsis"
[ -n "$msi" ] && add "windows-x86_64-msi" "$msi"

appimage=$(find_one "Frida_*_amd64.AppImage")
deb=$(find_one "Frida_*_amd64.deb")
rpm=$(find_one "Frida-*.x86_64.rpm")
[ -n "$appimage" ] && add "linux-x86_64" "$appimage" && add "linux-x86_64-appimage" "$appimage"
[ -n "$deb" ] && add "linux-x86_64-deb" "$deb"
[ -n "$rpm" ] && add "linux-x86_64-rpm" "$rpm"

if [ "$platforms" = '{}' ]; then
  echo "error: no signed installers found in $dir" >&2
  exit 1
fi

jq -n --arg version "$version" --arg notes "$notes" \
  --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --argjson platforms "$platforms" \
  '{version: $version, notes: $notes, pub_date: $date, platforms: $platforms}'
