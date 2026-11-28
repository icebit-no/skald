#!/bin/sh
# Signs a Skald.app that bundle.sh built with a Developer ID handed over in the environment, in a keychain of
# the run's own that is deleted however the run ends. It is how the release workflow signs, on a runner whose
# login keychain holds no identity; on Thomas's Mac, `bundle.sh --developer-id` signs from the login keychain
# instead. Adapted from Ferric's packaging/macos/sign-release.sh (MIT, by the same author).
#
#   packaging/macos/sign.sh APP              with the Developer ID Application identity of the team below
#   packaging/macos/sign.sh --rehearse APP   with whatever code-signing identity the file holds, a self-signed
#                                            one included, so the keychain and codesign can be tried without
#                                            the Developer ID; Gatekeeper and Apple refuse what it signs
#
# The identity is read from the environment:
#
#   DEVELOPER_ID_P12_BASE64    the certificate with its private key, exported as a .p12 file, in base64
#   DEVELOPER_ID_P12_PASSWORD  the password the file was exported with
#
# They leave the environment as the script starts, so no command it runs inherits them. Nothing from the bundle
# runs here: a binary that a build script or a poisoned cache tampered with would run beside the identity.
# packaging/macos/notarize.sh sends the signed bundle to Apple.
set -eu

# The grants follow the signing team, so every release is signed by this one, as bundle.sh signs.
team=Q4LJS2JYGD
secrets="DEVELOPER_ID_P12_BASE64 DEVELOPER_ID_P12_PASSWORD"
usage="usage: packaging/macos/sign.sh [--rehearse] APP"

fail() {
    printf '%s\n' "$*" >&2
    exit 1
}

rehearse=false
if [ "${1:-}" = --rehearse ]; then
    rehearse=true
    shift
fi
[ $# -eq 1 ] || fail "$usage"
[ "$(uname -s)" = Darwin ] || fail "Skald.app is signed on a Mac"
here=$(cd "$(dirname "$0")" && pwd)
app=$1
[ -f "$app/Contents/Info.plist" ] || fail "$app is not an app bundle; packaging/macos/bundle.sh builds one"
app=$(cd "$app" && pwd)

missing=""
for name in $secrets; do
    eval "value=\${$name:-}"
    [ -n "$value" ] || missing="$missing $name"
done
[ -z "$missing" ] || fail "not set:$missing. packaging/RELEASING.md says what each of them is."
p12=$DEVELOPER_ID_P12_BASE64
p12_password=$DEVELOPER_ID_P12_PASSWORD
# shellcheck disable=SC2086 # the names, each its own argument
unset $secrets value

# The user's keychain search list, one keychain a line. `security list-keychains -s` replaces it whole.
search_list() {
    security list-keychains -d user | sed -e 's/^[[:space:]]*"//' -e 's/"[[:space:]]*$//'
}
# Sets the list to the keychains on standard input.
set_search_list() {
    old_ifs=$IFS
    IFS='
'
    set -f
    # shellcheck disable=SC2046 # one keychain a line, each its own argument
    set -- $(cat)
    set +f
    IFS=$old_ifs
    [ $# -gt 0 ] || fail "that would leave the keychain search list empty, so it is left as it is"
    security list-keychains -d user -s "$@"
}

# The physical path, since the search list is compared line by line with the path written to it.
work=$(cd "$(mktemp -d "${TMPDIR:-/tmp}/skald-sign.XXXXXX")" && pwd -P)
keychain="$work/signing.keychain-db"
before=$(search_list)
cleanup() {
    status=$?
    if [ -f "$keychain" ]; then
        security delete-keychain "$keychain" 2>/dev/null || true
    fi
    if search_list | grep -qxF "$keychain"; then
        printf '%s\n' "$before" | set_search_list || true
    fi
    if search_list | grep -qxF "$keychain"; then
        echo "$keychain is still on the keychain search list" >&2
        status=1
    fi
    rm -rf "$work"
    exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

printf '\n== A keychain of this run'"'"'s own\n'
# security asks for a keychain's password when no argument gives it and reads the answer from standard input,
# so the password never stands on a command line; the question is shown only when the command fails.
asked() {
    said=$("$@" 2>&1) || {
        printf '%s\n' "$said" >&2
        return 1
    }
}
password=$(openssl rand -base64 24)
printf '%s\n%s\n' "$password" "$password" | asked security create-keychain "$keychain"
security set-keychain-settings -lut 3600 "$keychain"
printf '%s\n' "$password" | asked security unlock-keychain "$keychain"
(umask 077 && printf '%s' "$p12" | base64 --decode > "$work/identity.p12") ||
    fail "DEVELOPER_ID_P12_BASE64 is not base64"
unset p12
security import "$work/identity.p12" -k "$keychain" -P "$p12_password" -T /usr/bin/codesign -f pkcs12 > /dev/null ||
    fail "DEVELOPER_ID_P12_BASE64 does not open with DEVELOPER_ID_P12_PASSWORD as a PKCS#12 file"
rm -f "$work/identity.p12"
unset p12_password
# Lets codesign use the key without asking.
printf '%s\n' "$password" | asked security set-key-partition-list -S apple-tool:,apple:,codesign: -s "$keychain"
unset password

if $rehearse; then
    found=$(security find-identity -p codesigning "$keychain")
    pattern='^ *[0-9]+\) [0-9A-F]{40} "'
else
    # Valid, so the chain to Apple's root is there for codesign to embed.
    found=$(security find-identity -v -p codesigning "$keychain")
    pattern="^ *[0-9]+\) [0-9A-F]{40} \"Developer ID Application: .* \($team\)\"\$"
fi
chosen=$(printf '%s\n' "$found" | grep -E "$pattern" | sort -u || true)
if [ -z "$chosen" ]; then
    $rehearse && fail "the file holds no code-signing identity"
    fail "the file holds no valid Developer ID Application identity of team $team. It holds:
$(security find-identity -p codesigning "$keychain")
An identity that is there and not valid wants Apple's Developer ID Certification Authority: https://www.apple.com/certificateauthority/"
fi
[ "$(printf '%s\n' "$chosen" | wc -l | tr -d ' ')" = 1 ] || fail "the file holds more than one identity to sign with:
$chosen"
hash=$(printf '%s' "$chosen" | sed -E 's/^ *[0-9]+\) ([0-9A-F]{40}) .*/\1/')
signer=$(printf '%s' "$chosen" | sed -E 's/^ *[0-9]+\) [0-9A-F]{40} "([^"]*)".*/\1/')

# codesign finds an identity only in a keychain on the search list, even when told which keychain to use. At
# the end of the list, so whoever else looks a key up finds their own keychains first.
printf '%s\n%s\n' "$before" "$keychain" | set_search_list

printf '\n== Sign as %s\n' "$signer"
codesign --force --keychain "$keychain" --sign "$hash" --options runtime --timestamp \
    --entitlements "$here/skald.entitlements" "$app"
security delete-keychain "$keychain"
printf '%s\n' "$before" | set_search_list
search_list | grep -qxF "$keychain" && fail "the keychain is still on the search list after it was deleted"

printf '\n== Read the signature back\n'
codesign --verify --strict --deep --verbose=2 "$app"
details=$(codesign -dvv "$app" 2>&1)
field() {
    printf '%s\n' "$details" | sed -n "s/^$1=//p" | head -1
}
[ "$(field Identifier)" = app.icebit.skald ] || fail "the bundle is signed as $(field Identifier), not app.icebit.skald"
[ "$(field Authority)" = "$signer" ] || fail "the bundle is signed by $(field Authority), not $signer"
printf '%s\n' "$details" | grep -Eq '^CodeDirectory .*flags=.*runtime' ||
    fail "the bundle is not signed with the hardened runtime, which notarization requires"
[ -n "$(field Timestamp)" ] || fail "the bundle carries no secure timestamp, which notarization requires"
entitlements=$(codesign -d --entitlements - --xml "$app" 2>/dev/null | plutil -convert json -o - - 2>/dev/null || true)
[ "$entitlements" = '{"com.apple.security.device.audio-input":true}' ] ||
    fail "the bundle carries the entitlements $entitlements, not the microphone alone"
if ! $rehearse; then
    [ "$(field TeamIdentifier)" = "$team" ] || fail "the bundle is signed for team $(field TeamIdentifier), not $team"
fi
version=$(plutil -extract CFBundleShortVersionString raw -o - "$app/Contents/Info.plist")
echo "signed Skald $version as $signer, hardened, timestamped, with the microphone entitlement alone"
