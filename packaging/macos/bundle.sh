#!/bin/sh
# Builds Skald.app on a Mac from this checkout: the release `skald` as the bundle's executable, which is also the
# command line (D-25), the Info.plist with the workspace's version, and the icon packed from assets/icon/. Every
# build is signed with the hardened runtime and the entitlements in skald.entitlements, so the bundle runs here as
# it runs released. Adapted from Ferric's packaging/macos/bundle.sh (MIT, by the same author).
#
#   packaging/macos/bundle.sh                 signed ad hoc
#   packaging/macos/bundle.sh --developer-id  signed with the Developer ID Application identity of the team
#                                             below, from the login keychain, with a secure timestamp: ready for
#                                             packaging/macos/notarize.sh
#
# The bundle lands in target/release/Skald.app, for this Mac's architecture; Skald ships for Apple Silicon only
# (D-34), so --developer-id refuses any other. Microphone and Accessibility grants follow the signature: an ad hoc
# bundle is asked for them again after every build, a Developer ID bundle keeps them across builds and updates.
set -eu

# The grants follow the signing team, so every Developer ID build is signed by this one.
team=Q4LJS2JYGD
usage="usage: packaging/macos/bundle.sh [--developer-id]"

fail() {
    printf '%s\n' "$*" >&2
    exit 1
}

developer_id=false
case "${1:-}" in
    "") ;;
    --developer-id) developer_id=true ;;
    *) fail "$usage" ;;
esac
[ $# -le 1 ] || fail "$usage"
[ "$(uname -s)" = Darwin ] || fail "Skald.app is built on a Mac"

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
cd "$root"

if $developer_id; then
    found=$(security find-identity -v -p codesigning 2>/dev/null |
        grep -E "^ *[0-9]+\) [0-9A-F]{40} \"Developer ID Application: .* \($team\)\"\$" || true)
    [ -n "$found" ] || fail "no valid Developer ID Application identity of team $team in the keychains on the search list.
An identity that is there and not valid wants Apple's Developer ID Certification Authority: https://www.apple.com/certificateauthority/"
    # Two are valid while a renewed certificate overlaps the old one; either keeps the grants, as both are the team's.
    sign=$(printf '%s\n' "$found" | head -1 | sed -E 's/^ *[0-9]+\) ([0-9A-F]{40}) .*/\1/')
    timestamp=--timestamp
else
    sign=-
    timestamp=--timestamp=none
fi

version=$(sed -n '/^\[workspace\.package\]/,/^\[/ s/^version = "\(.*\)"$/\1/p' Cargo.toml)
[ -n "$version" ] || fail "no version under [workspace.package] in Cargo.toml"

cargo build --release --locked -p skald
binary=target/release/skald
said=$("$binary" --version)
architectures=$(lipo -archs "$binary")
if $developer_id && [ "$architectures" != arm64 ]; then
    fail "the binary is built for $architectures; Skald ships for Apple Silicon only (D-34)"
fi
[ "$said" = "skald $version" ] || fail "the binary says '$said', not 'skald $version'"
# The oldest macOS the binary was linked for, which the bundle then promises.
minimum=$(otool -l "$binary" | awk '$1 == "minos" { print $2; exit }')
[ -n "$minimum" ] || fail "$binary names no minimum macOS version"

app=target/release/Skald.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/skald"
sed -e "s/@VERSION@/$version/g" -e "s/@MINIMUM_SYSTEM@/$minimum/g" "$here/Info.plist" > "$app/Contents/Info.plist"
plutil -lint -s "$app/Contents/Info.plist"

# iconutil packs an iconset of 16 to 512 points at 1x and 2x. The 16 and 32 px files are drawn for their size
# (assets/icon/README.md), so each pixel size takes its own file wherever it appears.
work=$(mktemp -d "${TMPDIR:-/tmp}/skald-bundle.XXXXXX")
trap 'rm -rf "$work"' EXIT
iconset="$work/AppIcon.iconset"
mkdir "$iconset"
icons="$root/assets/icon"
for points in 16 32 128 256 512; do
    cp "$icons/app-icon-$points.png" "$iconset/icon_${points}x${points}.png"
    cp "$icons/app-icon-$((points * 2)).png" "$iconset/icon_${points}x${points}@2x.png"
done
iconutil --convert icns --output "$app/Contents/Resources/AppIcon.icns" "$iconset"

codesign --force --sign "$sign" --options runtime $timestamp \
    --entitlements "$here/skald.entitlements" "$app"

codesign --verify --strict --deep --verbose=2 "$app"
details=$(codesign -dvv "$app" 2>&1)
field() {
    printf '%s\n' "$details" | sed -n "s/^$1=//p" | head -1
}
[ "$(field Identifier)" = app.icebit.skald ] || fail "the bundle is signed as $(field Identifier), not app.icebit.skald"
printf '%s\n' "$details" | grep -Eq '^CodeDirectory .*flags=.*runtime' ||
    fail "the bundle is not signed with the hardened runtime"
entitlements=$(codesign -d --entitlements - --xml "$app" 2>/dev/null | plutil -convert json -o - - 2>/dev/null || true)
[ "$entitlements" = '{"com.apple.security.device.audio-input":true}' ] ||
    fail "the bundle carries the entitlements $entitlements, not the microphone alone"
if $developer_id; then
    [ "$(field TeamIdentifier)" = "$team" ] || fail "the bundle is signed for team $(field TeamIdentifier), not $team"
    [ -n "$(field Timestamp)" ] || fail "the bundle carries no secure timestamp, which notarization requires"
    signer=$(field Authority)
else
    signer="ad hoc"
fi
said=$("$app/Contents/MacOS/skald" --version)
[ "$said" = "skald $version" ] || fail "the bundle's executable says '$said', not 'skald $version'"

# Gatekeeper accepts only a notarized Developer ID bundle; until notarize.sh has run it rejects this one.
spctl --assess --type execute -vv "$app" 2>&1 || true
echo "built $app, skald $version for $architectures and macOS $minimum and later, signed by $signer"
