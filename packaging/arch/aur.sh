#!/bin/sh
# Writes the AUR package skald-bin for a release tarball from packaging/linux/tarball.sh: PKGBUILD from
# PKGBUILD.in with the tarball's version and SHA-256, and .SRCINFO where makepkg is there to write it.
#
#   packaging/arch/aur.sh TARBALL [DIR]          the source is the tarball on the release's GitHub page
#   packaging/arch/aur.sh --local TARBALL [DIR]  the source is the tarball itself, copied into DIR, to try
#                                                the package before anything is published
#
# DIR defaults to target/dist/aur. Publishing to the AUR belongs to the release pipeline (M6).
set -eu

usage="usage: packaging/arch/aur.sh [--local] TARBALL [DIR]"

fail() {
    printf '%s\n' "$*" >&2
    exit 1
}

local_source=false
if [ "${1:-}" = "--local" ]; then
    local_source=true
    shift
fi
[ $# -ge 1 ] && [ $# -le 2 ] || fail "$usage"
tarball=$1
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
dir=${2:-$root/target/dist/aur}

[ -f "$tarball" ] || fail "no tarball at $tarball"
name=$(basename "$tarball")
version=$(printf '%s\n' "$name" | sed -n 's/^skald-\(.*\)-linux-x86_64\.tar\.gz$/\1/p')
[ -n "$version" ] || fail "$name is not named skald-VERSION-linux-x86_64.tar.gz"
sha256=$(sha256sum "$tarball" | cut -d' ' -f1)

mkdir -p "$dir"
if $local_source; then
    cp "$tarball" "$dir/$name"
    source=$name
else
    source="https://github.com/icebit-no/skald/releases/download/v$version/$name"
fi
sed -e "s|@VERSION@|$version|g" -e "s|@SOURCE@|$source|g" -e "s|@SHA256@|$sha256|g" \
    "$here/PKGBUILD.in" > "$dir/PKGBUILD"
if command -v makepkg >/dev/null 2>&1; then
    (cd "$dir" && makepkg --printsrcinfo > .SRCINFO)
fi
printf '%s\n' "$dir/PKGBUILD"
