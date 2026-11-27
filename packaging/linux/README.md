# Skald on Linux

How to build and run Skald on Linux, on Omarchy and other Hyprland desktops first (D-7), and how the desktop
holds the key you talk with (D-39). Linux is M5 in [plan.md](../../docs/plan.md). Skald runs there in the
terminal: the tray, the overlay and the windows have not come to Linux, and text is not inserted yet, so each
take is printed and kept in «Siste tekster» (`skald last`).

## Build

Needs Rust through rustup (the toolchain in `rust-toolchain.toml` is installed on first use), a C++ compiler,
CMake for transcribe.cpp, and the libraries the crates link. On Arch and Omarchy:

```sh
sudo pacman -S --needed base-devel cmake alsa-lib wayland libxkbcommon-x11 libx11 fontconfig
cargo build --release
install -m 755 target/release/skald ~/.local/bin/skald
```

transcribe.cpp is built for the CPU.

## Run

```sh
skald
```

The first start says where the speech model belongs and how to fetch it, with its size and the `curl` line;
the file is checked against its SHA-256 before it is used. Then Skald says `Klar. Hold høyre Ctrl for å
snakke.` and listens for the key binding below. Ctrl+C in the terminal ends it.

To start it with Hyprland, add `exec-once = skald` to its configuration (on Omarchy,
`~/.config/hypr/autostart.conf`).

## The key

A Wayland desktop gives the keyboard to no program, so Hyprland holds the key for Skald. Add the two lines to
Hyprland's configuration (on Omarchy, `~/.config/hypr/bindings.conf`):

```ini
# Hold right Ctrl to talk to Skald. The n lets right Ctrl through, so right Ctrl+C still copies.
bindn = , Control_R, exec, skald start
bindrn = CTRL, Control_R, exec, skald stop
```

The release line names CTRL because Hyprland matches the release of a modifier with that modifier still in
its mask. If the press does not start a take, add the press line once more with `CTRL` before the comma; a
press or a release that arrives twice counts once.

A take arms once the key has been down for 200 ms and ends when it comes up, as on macOS. Skald knows only
the key the binding reports, so another key pressed with right Ctrl does not cancel the take: a shortcut held
longer than 200 ms records, and a take without speech never becomes text. A release Hyprland never reports
leaves the take running until the 120-second cap.

`skald start` and `skald stop` answer only while Skald runs, and say so otherwise.

### Through the GlobalShortcuts portal

Skald also registers the shortcut `hold` with the GlobalShortcuts portal when it starts. On Hyprland, with
xdg-desktop-portal-hyprland, `hyprctl globalshortcuts` lists it as `app.icebit.skald:hold` (or `:hold`, where
the portal keeps no names for programs started outside a sandbox), and one line binds it in place of the two
above:

```ini
bindn = , Control_R, global, app.icebit.skald:hold
```

Hyprland then sends the press and the release through the portal, and no `skald` process starts at each
press. KDE Plasma and GNOME have the portal too and ask for the key in their own settings; whether they report
the release has not been tried.
