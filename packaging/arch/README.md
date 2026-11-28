# skald-bin for Arch Linux

The AUR package `skald-bin` installs Skald from the Linux release tarball, which
[packaging/linux/tarball.sh](../linux/tarball.sh) packs (D-52). Nothing is published yet: the release pipeline
(M6 in [plan.md](../../docs/plan.md)) builds the tarball on a tag, attaches it to the GitHub release and pushes
the package to the AUR.

| File | Does |
|---|---|
| `PKGBUILD.in` | The package, with the version, the tarball's URL and its SHA-256 left for `aur.sh` |
| `aur.sh` | Writes `PKGBUILD`, and `.SRCINFO` where `makepkg` is there, for a tarball |

## Try it before anything is published

On Arch, with a tarball packed on x86_64:

```sh
packaging/linux/tarball.sh
packaging/arch/aur.sh --local target/dist/skald-0.1.0-linux-x86_64.tar.gz
cd target/dist/aur && makepkg -si
```

`--local` makes the tarball itself the package's source. Without it the source is the tarball on the release's
GitHub page, which is what goes to the AUR.

## What the package holds and needs

The tarball's tree is installed under `/usr` as it is: `skald`, its desktop entry and icons (the name
`app.icebit.skald`), the Hyprland lines in `/usr/share/skald/hyprland.conf`, the Linux README, and the licence
with THIRD-PARTY-NOTICES under `/usr/share/licenses/skald-bin/`.

The dependencies are what the binary links (glibc, libgcc, libstdc++, alsa-lib, libxcb, libxkbcommon,
libxkbcommon-x11, vulkan-icd-loader) or opens as it runs (wayland), the programs it runs (wtype for the paste,
PipeWire's pw-play for the sounds), and hicolor-icon-theme for the icons. A Vulkan driver, pipewire-alsa, the freedesktop sound theme and Hyprland's portal are
optional: without them Skald computes on the processor, reaches the microphone through ALSA directly, plays no
sounds and takes the keys from the bindings alone.
