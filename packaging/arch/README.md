# skald-bin for Arch Linux

The AUR package `skald-bin` installs Skald from the Linux release tarball, which
[packaging/linux/tarball.sh](../linux/tarball.sh) packs (D-52). The release workflow builds the tarball on a tag,
builds, installs and runs the package from it on Arch, attaches the tarball to the GitHub release and pushes the
package to the AUR ([RELEASING.md](../RELEASING.md)).

| File | Does |
|---|---|
| `PKGBUILD.in` | The package, with the version, the tarball's URL and its SHA-256 left for `aur.sh` |
| `aur.sh` | Writes `PKGBUILD`, and `.SRCINFO` where `makepkg` is there, for a tarball |
| `container.sh` | In an Arch container: builds, installs and runs the package from a tarball, then writes the `PKGBUILD` and `.SRCINFO` for the AUR |

## Try it before anything is published

On Arch, with a tarball packed on x86_64:

```sh
packaging/linux/tarball.sh
packaging/arch/aur.sh --local target/dist/skald-0.1.0-linux-x86_64.tar.gz
cd target/dist/aur && makepkg -si
```

`--local` makes the tarball itself the package's source. Without it the source is the tarball on the release's
GitHub page, which is what goes to the AUR.

On any machine with Docker, `container.sh` does the same in an Arch container, as the release workflow runs it:

```sh
docker run --rm -v "$PWD:/src:ro" -v "$PWD/target/dist:/dist" archlinux:base-devel \
    /src/packaging/arch/container.sh /dist/skald-0.1.0-linux-x86_64.tar.gz /dist/aur
```

It brings the container's system up to date, builds the package from the tarball with `--local`, installs it with
its dependencies, requires `skald --version` to answer with the package's version and `skald --help` to run, and
removes it again. Then it writes the `PKGBUILD` and `.SRCINFO` that go to the AUR into `target/dist/aur`.

## What the package holds and needs

The tarball's tree is installed under `/usr` as it is: `skald`, its desktop entry and icons (the name
`app.icebit.skald`), the Hyprland lines in `/usr/share/skald/hyprland.conf`, the Linux README, and the licence
with THIRD-PARTY-NOTICES under `/usr/share/licenses/skald-bin/`.

The dependencies are what the binary links (glibc, libgcc, libstdc++, alsa-lib, libxcb, libxkbcommon,
libxkbcommon-x11, vulkan-icd-loader) or opens as it runs (wayland), the programs it runs (wtype for the paste,
PipeWire's pw-play for the sounds), and hicolor-icon-theme for the icons. A Vulkan driver, pipewire-alsa, the freedesktop sound theme and Hyprland's portal are
optional: without them Skald computes on the processor, reaches the microphone through ALSA directly, plays no
sounds and takes the keys from the bindings alone.
