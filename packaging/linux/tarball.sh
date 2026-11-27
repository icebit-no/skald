#!/bin/sh
# Packs Skald's Linux release tarball from this checkout: the release `skald`, which is also the command line
# (D-25), its desktop entry and icons, the Hyprland lines, the README, the licence and THIRD-PARTY-NOTICES once
# it exists. The tree inside is laid out as under /usr, so a package installs it as it is (packaging/arch).
#
#   packaging/linux/tarball.sh           builds skald in release mode first
#   packaging/linux/tarball.sh BINARY    packs BINARY, built for this checkout's version
#
# The tarball lands in target/dist/skald-VERSION-linux-ARCH.tar.gz, with its SHA-256 beside it in a .sha256 file.
set -eu

usage="usage: packaging/linux/tarball.sh [BINARY]"

fail() {
    printf '%s\n' "$*" >&2
    exit 1
}

[ $# -le 1 ] || fail "$usage"
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"

version=$(sed -n '/^\[workspace\.package\]/,/^\[/ s/^version = "\(.*\)"$/\1/p' Cargo.toml)
[ -n "$version" ] || fail "no version under [workspace.package] in Cargo.toml"

if [ $# -eq 1 ]; then
    binary=$1
else
    cargo build --release --locked -p skald
    binary=target/release/skald
fi
[ -x "$binary" ] || fail "$binary is not an executable"
said=$("$binary" --version)
[ "$said" = "skald $version" ] || fail "the binary says '$said', not 'skald $version'"

arch=$(uname -m)
name="skald-$version-linux-$arch"
dist="$root/target/dist"
stage="$dist/$name"
rm -rf "$stage" "$stage.tar.gz" "$stage.tar.gz.sha256"
mkdir -p "$stage"

install -Dm755 "$binary" "$stage/usr/bin/skald"
install -Dm644 packaging/linux/app.icebit.skald.desktop "$stage/usr/share/applications/app.icebit.skald.desktop"
for size in 16 32 64 128 256 512; do
    install -Dm644 "assets/icon/app-icon-$size.png" \
        "$stage/usr/share/icons/hicolor/${size}x${size}/apps/app.icebit.skald.png"
done
install -Dm644 packaging/linux/hyprland.conf "$stage/usr/share/skald/hyprland.conf"
install -Dm644 packaging/linux/README.md "$stage/usr/share/doc/skald/README.md"
install -Dm644 LICENSE "$stage/usr/share/licenses/skald/LICENSE"
if [ -f THIRD-PARTY-NOTICES ]; then
    install -Dm644 THIRD-PARTY-NOTICES "$stage/usr/share/licenses/skald/THIRD-PARTY-NOTICES"
fi

# The same tree packs to the same bytes: sorted names, one owner, and SOURCE_DATE_EPOCH or the last commit's
# time on every file.
mtime=${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct)}
tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="@$mtime" \
    -C "$dist" -cf - "$name" | gzip -n -9 > "$stage.tar.gz"
(cd "$dist" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
rm -rf "$stage"
printf '%s\n' "$stage.tar.gz" "$(cut -d' ' -f1 "$stage.tar.gz.sha256")"
