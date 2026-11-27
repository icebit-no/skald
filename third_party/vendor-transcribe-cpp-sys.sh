#!/bin/sh
# Writes third_party/transcribe-cpp-sys: the transcribe-cpp-sys crate as published on crates.io, checked
# against its SHA-256, with every patch in third_party/patches/transcribe-cpp-sys applied in name order.
# Running it again on an unchanged checkout changes nothing; to move to a new release, change the version
# and the hash below, run it, and fix or drop each patch that no longer applies.
#
# Needs curl, tar, git and shasum (macOS) or sha256sum.

set -eu

VERSION=0.5.1
SHA256=323903fad2bcebaf025314ccb5dc3e178ba8e61622fc935fe17096d5ab93244b

here=$(cd "$(dirname "$0")" && pwd)
crate="$here/transcribe-cpp-sys"
patches="$here/patches/transcribe-cpp-sys"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

curl --fail --silent --show-error --location \
    "https://static.crates.io/crates/transcribe-cpp-sys/transcribe-cpp-sys-$VERSION.crate" \
    --output "$work/crate.tar.gz"

if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$work/crate.tar.gz" | cut -d' ' -f1)
else
    actual=$(shasum -a 256 "$work/crate.tar.gz" | cut -d' ' -f1)
fi
if [ "$actual" != "$SHA256" ]; then
    echo "transcribe-cpp-sys $VERSION: SHA-256 $actual, expected $SHA256" >&2
    exit 1
fi

tar -xzf "$work/crate.tar.gz" -C "$work"
src="$work/transcribe-cpp-sys-$VERSION"
# The temporary folder is outside any git repository, so git apply works as a plain patch tool there.
for patch in "$patches"/*.patch; do
    [ -e "$patch" ] || continue
    (cd "$src" && git apply -p1 "$patch")
done

# SKALD.md is Skald's own note on what the copy carries; everything else comes from the crate.
find "$crate" -mindepth 1 -maxdepth 1 ! -name SKALD.md -exec rm -rf {} +
mkdir -p "$crate"
(cd "$src" && tar -cf - .) | (cd "$crate" && tar -xf -)
