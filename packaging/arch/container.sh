#!/bin/sh
# Runs as root in an Arch Linux container (archlinux:base-devel): builds skald-bin from a release tarball with
# --local, installs it with its dependencies and runs `skald --version` and `skald --help` from it, then writes
# the PKGBUILD and .SRCINFO that go to the AUR, whose source is the tarball on the release's GitHub page.
#
#   packaging/arch/container.sh TARBALL OUT
#
# The checkout and the tarball may be mounted read-only; OUT receives PKGBUILD and .SRCINFO. The release workflow
# runs it with the checkout at /src:
#
#   docker run --rm -v "$PWD:/src:ro" -v "$PWD/target/dist:/dist" archlinux:base-devel \
#       /src/packaging/arch/container.sh /dist/skald-VERSION-linux-x86_64.tar.gz /dist/aur
set -eu

usage="usage: packaging/arch/container.sh TARBALL OUT"

fail() {
    printf '%s\n' "$*" >&2
    exit 1
}

[ $# -eq 2 ] || fail "$usage"
tarball=$1
out=$2
here=$(cd "$(dirname "$0")" && pwd)
[ -f "$tarball" ] || fail "no tarball at $tarball"
[ "$(id -u)" = 0 ] || fail "runs as root in an Arch container"
command -v pacman >/dev/null 2>&1 || fail "runs in an Arch container"

# An image is a snapshot; Arch supports no partial upgrade, so the whole system is brought up to date first.
pacman -Syu --noconfirm --needed > /dev/null
# makepkg refuses to run as root.
id builder >/dev/null 2>&1 || useradd --create-home builder
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
chown builder "$work"

printf '\n== Build skald-bin from the tarball\n'
cp "$here/aur.sh" "$here/PKGBUILD.in" "$work/"
cp "$tarball" "$work/"
chown builder "$work"/*
su builder -c "sh '$work/aur.sh' --local '$work/$(basename "$tarball")' '$work/try'"
# The dependencies come from the repositories through pacman here, since builder may not run it.
depends=$(su builder -c "cd '$work/try' && . ./PKGBUILD && printf '%s\n' \"\${depends[@]}\"")
# shellcheck disable=SC2086 # one package a word
pacman -S --noconfirm --needed $depends > /dev/null
su builder -c "cd '$work/try' && makepkg --noconfirm"

printf '\n== Install it and run skald\n'
pacman -U --noconfirm "$work"/try/skald-bin-*.pkg.tar.zst
version=$(sed -n 's/^pkgver=//p' "$work/try/PKGBUILD")
said=$(skald --version)
[ "$said" = "skald $version" ] || fail "the installed skald says '$said', not 'skald $version'"
skald --help > /dev/null
pacman -Ql skald-bin
pacman -Rns --noconfirm skald-bin > /dev/null

printf '\n== The package for the AUR\n'
mkdir -p "$out"
su builder -c "sh '$work/aur.sh' '$work/$(basename "$tarball")' '$work/aur'"
cp "$work/aur/PKGBUILD" "$work/aur/.SRCINFO" "$out/"
cat "$out/PKGBUILD"
