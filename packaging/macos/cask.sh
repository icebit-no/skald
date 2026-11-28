#!/bin/sh
# Writes the Homebrew cask for a release's Skald-VERSION.zip, as notarize.sh packs it: skald.rb with the zip's
# version and SHA-256 and the oldest macOS its bundle runs on, for icebit-no/homebrew-tap (D-14).
#
#   packaging/macos/cask.sh ZIP [DIR]     DIR defaults to target/dist/homebrew
#
# The version is read from the bundle's Info.plist inside the zip and must be the one in the zip's name. Runs on
# macOS and Linux alike.
set -eu

usage="usage: packaging/macos/cask.sh ZIP [DIR]"

fail() {
    printf '%s\n' "$*" >&2
    exit 1
}

[ $# -ge 1 ] && [ $# -le 2 ] || fail "$usage"
zip=$1
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
dir=${2:-$root/target/dist/homebrew}

[ -f "$zip" ] || fail "no zip at $zip"
name=$(basename "$zip")
version=$(printf '%s\n' "$name" | sed -n 's/^Skald-\(.*\)\.zip$/\1/p')
[ -n "$version" ] || fail "$name is not named Skald-VERSION.zip"

# The plist as one line, so each key and the string after it can be read with sed.
plist=$(unzip -p "$zip" Skald.app/Contents/Info.plist | tr -d '\n\t')
[ -n "$plist" ] || fail "$name holds no Skald.app/Contents/Info.plist"
value() {
    printf '%s\n' "$plist" | sed -n "s:.*<key>$1</key> *<string>\([^<]*\)</string>.*:\1:p"
}
said=$(value CFBundleShortVersionString)
[ "$said" = "$version" ] || fail "the bundle in $name says version '$said', not $version"
minimum=$(value LSMinimumSystemVersion)
# Homebrew names each macOS by its release; the cask takes the oldest the bundle promises, and newer ones with it.
case "${minimum%%.*}" in
    11) macos=big_sur ;;
    12) macos=monterey ;;
    13) macos=ventura ;;
    14) macos=sonoma ;;
    15) macos=sequoia ;;
    26) macos=tahoe ;;
    *) fail "the bundle asks for macOS '$minimum', which this script has no Homebrew name for" ;;
esac

if command -v sha256sum >/dev/null 2>&1; then
    sha256=$(sha256sum "$zip" | cut -d' ' -f1)
else
    sha256=$(shasum -a 256 "$zip" | cut -d' ' -f1)
fi

mkdir -p "$dir"
cask="$dir/skald.rb"
sed -e "s/@VERSION@/$version/g" -e "s/@SHA256@/$sha256/g" -e "s/@MACOS@/$macos/g" "$here/skald.rb" > "$cask"
if grep -n '@[A-Z0-9_]*@' "$cask" >&2; then
    fail "$cask still holds a placeholder"
fi
if command -v ruby >/dev/null 2>&1; then
    ruby -c "$cask" > /dev/null
fi
printf '%s\n' "$cask"
