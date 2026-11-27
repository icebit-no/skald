#!/bin/sh
# Notarizes a Skald.app that bundle.sh signed with --developer-id, staples the ticket to it, reads Gatekeeper's
# verdict back, and packs the bundle as Skald-VERSION.zip beside it. Adapted from Ferric's
# packaging/macos/sign-release.sh (MIT, by the same author).
#
#   packaging/macos/notarize.sh [APP]     APP is target/release/Skald.app when not given
#
# An App Store Connect API key sends the bundle to Apple. It is read from the environment:
#
#   APPLE_API_KEY_P8      the text of the key's AuthKey_<id>.p8 file
#   APPLE_API_KEY_ID      the key's id
#   APPLE_API_ISSUER_ID   the issuer id of the team the key belongs to
#
# They leave the environment as the script starts, so no command it runs inherits them. The key is written to a
# file only this user can read, in a folder of the run's own that is deleted however the run ends. Give them on
# the command line of this one run rather than exporting them; README.md shows how.
set -eu

# The team bundle.sh signs for; Apple notarizes the bundle under it.
team=Q4LJS2JYGD
secrets="APPLE_API_KEY_P8 APPLE_API_KEY_ID APPLE_API_ISSUER_ID"
usage="usage: packaging/macos/notarize.sh [APP]"

fail() {
    printf '%s\n' "$*" >&2
    exit 1
}

[ $# -le 1 ] || fail "$usage"
[ "$(uname -s)" = Darwin ] || fail "Skald.app is notarized on a Mac"
here=$(cd "$(dirname "$0")" && pwd)
app=${1:-$here/../../target/release/Skald.app}
[ -f "$app/Contents/Info.plist" ] || fail "$app is not an app bundle; packaging/macos/bundle.sh --developer-id builds one"
app=$(cd "$app" && pwd)

missing=""
for name in $secrets; do
    eval "value=\${$name:-}"
    [ -n "$value" ] || missing="$missing $name"
done
[ -z "$missing" ] || fail "not set:$missing. packaging/macos/README.md says what each of them is."
api_key=$APPLE_API_KEY_P8
api_key_id=$APPLE_API_KEY_ID
api_issuer=$APPLE_API_ISSUER_ID
# shellcheck disable=SC2086 # the names, each its own argument
unset $secrets value

work=$(mktemp -d "${TMPDIR:-/tmp}/skald-notarize.XXXXXX")
trap 'rm -rf "$work"' EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

printf '\n== Check the signature\n'
codesign --verify --strict --deep --verbose=2 "$app"
details=$(codesign -dvv "$app" 2>&1)
field() {
    printf '%s\n' "$details" | sed -n "s/^$1=//p" | head -1
}
[ "$(field Identifier)" = app.icebit.skald ] || fail "the bundle is signed as $(field Identifier), not app.icebit.skald"
[ "$(field TeamIdentifier)" = "$team" ] ||
    fail "the bundle is signed for team $(field TeamIdentifier), not $team; packaging/macos/bundle.sh --developer-id signs it"
printf '%s\n' "$details" | grep -Eq '^CodeDirectory .*flags=.*runtime' ||
    fail "the bundle is not signed with the hardened runtime, which notarization requires"
[ -n "$(field Timestamp)" ] || fail "the bundle carries no secure timestamp, which notarization requires"
version=$(plutil -extract CFBundleShortVersionString raw -o - "$app/Contents/Info.plist")
echo "Skald $version, signed by $(field Authority)"

printf '\n== Notarize\n'
key="$work/notary.p8"
(umask 077 && printf '%s\n' "$api_key" > "$key")
unset api_key
submission="$work/Skald.zip"
ditto -c -k --keepParent "$app" "$submission"
answer="$work/notarization.json"
submitted=0
xcrun notarytool submit "$submission" --key "$key" --key-id "$api_key_id" --issuer "$api_issuer" \
    --wait --timeout 1h --output-format json > "$answer" || submitted=$?
cat "$answer"
echo
id=$(plutil -extract id raw -o - "$answer" 2>/dev/null || true)
status=$(plutil -extract status raw -o - "$answer" 2>/dev/null || true)
# The log says why Apple refused, and lists warnings on a submission it accepted.
if [ -n "$id" ]; then
    xcrun notarytool log "$id" --key "$key" --key-id "$api_key_id" --issuer "$api_issuer" ||
        echo "Apple has no log for submission $id yet"
fi
rm -f "$key"
unset api_key_id api_issuer
[ "$status" = Accepted ] ||
    fail "Apple answered ${status:-nothing readable} for submission ${id:-that got no id}, and notarytool exited $submitted. Nothing is stapled."

printf '\n== Staple the ticket\n'
# Apple takes a moment to publish a ticket it has just issued.
attempt=1
until xcrun stapler staple "$app"; do
    [ "$attempt" -lt 5 ] || fail "no ticket could be stapled to the bundle, so a Mac without a network would refuse it"
    echo "attempt $attempt found no ticket; waiting for Apple to publish it"
    attempt=$((attempt + 1))
    sleep 30
done
xcrun stapler validate "$app"

printf "\n== Gatekeeper's verdict\n"
verdict=$(spctl --assess --type execute -vv "$app" 2>&1 || true)
printf '%s\n' "$verdict"
printf '%s\n' "$verdict" | grep -q '^source=Notarized Developer ID$' ||
    fail "Gatekeeper does not take the bundle for a notarized Developer ID build"

printf '\n== Pack\n'
name=$(basename "$app")
zip="$(dirname "$app")/Skald-$version.zip"
rm -f "$zip"
ditto -c -k --keepParent "$app" "$zip"
# Read back as a user unpacks it, so a zip that loses part of the signature or the ticket fails here.
unpacked="$work/unpacked"
mkdir "$unpacked"
ditto -x -k "$zip" "$unpacked"
codesign --verify --strict --deep "$unpacked/$name"
xcrun stapler validate "$unpacked/$name"
echo "$zip"
echo "sha256 $(shasum -a 256 "$zip" | cut -d' ' -f1)"
