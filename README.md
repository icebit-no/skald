# Skald

Push-to-talk dictation in Norwegian for macOS, Windows and Linux. Hold a key, speak, release, and the text is
inserted where the cursor is, in bokmål or nynorsk. Skald transcribes on your own machine with the speech models
of the National Library of Norway (Nasjonalbiblioteket), and nothing you say leaves it. It is written in Rust on
GPUI and runs the models with transcribe.cpp.

## Status

Built, not yet released. There is nothing to download, so Skald runs from source for now. Before a release,
Thomas's tests of daily use are open on macOS ([#22](https://github.com/icebit-no/skald/issues/22)) and on
Omarchy ([#40](https://github.com/icebit-no/skald/issues/40)), Windows waits for a test on a machine with an
NVIDIA GPU, and the macOS app for Apple's notarization. [docs/plan.md](docs/plan.md) has the details.

## Build and run

Every system needs Rust through [rustup](https://rustup.rs), which installs the toolchain `rust-toolchain.toml`
pins on first use, and a C++ compiler and CMake, which build transcribe.cpp. The first start opens «Kom i gang
med Skald», which downloads NB-Whisper Large (3.1 GB) from the National Library's repository on Hugging Face,
checks its SHA-256 and lets you try a dictation.

### macOS

For Apple Silicon. Needs Xcode or its command line tools, and CMake (`brew install cmake`).

```sh
git clone https://github.com/icebit-no/skald
cd skald
packaging/macos/bundle.sh
open target/release/Skald.app
```

`bundle.sh` builds the release binary into `target/release/Skald.app`, signed ad hoc
([packaging/macos](packaging/macos/README.md)). Skald asks for the microphone, and for Accessibility, which the
key and the insertion need; with an ad hoc signature macOS asks again after every build. Hold right ⌘ to talk.
`cargo run --release` runs the same program from a terminal, which then holds the permissions.

### Windows

Needs Visual Studio's C++ build tools, CMake and LunarG's Vulkan SDK (the version
[windows.yml](.github/workflows/windows.yml) installs), which ggml's Vulkan backend is built against. Check the
repository out without CRLF line endings, so the files Skald embeds keep the repository's bytes:

```powershell
git clone --config core.autocrlf=false https://github.com/icebit-no/skald
cd skald
cargo build --release
target\release\skald.exe
```

The build puts transcribe.cpp's and ggml's DLLs beside `skald.exe`, which needs them. A machine without a
Vulkan driver runs the model on the CPU. Hold right Ctrl to talk.

### Linux

For Wayland compositors, Hyprland first. On Arch and Omarchy:

```sh
sudo pacman -S --needed base-devel cmake alsa-lib wayland libxkbcommon-x11 libx11 fontconfig wtype \
  vulkan-headers vulkan-icd-loader spirv-headers shaderc
git clone https://github.com/icebit-no/skald
cd skald
cargo build --release
target/release/skald
```

Other distributions need the same libraries; the Debian and Ubuntu names are in
[linux.yml](.github/workflows/linux.yml). The model computes on the GPU through its Vulkan driver, or on the
CPU without one. Wayland gives the keyboard to no program, so Skald hears the key
through the desktop's key bindings: add the lines in [packaging/linux](packaging/linux/README.md), which also
says how to start Skald at login. Hold right Ctrl to talk.

## Command line

The `skald` binary is also Skald's command line, for you and for an AI agent: `skald last` prints the latest
text, `skald fix add "mercha" "merget"` adds a correction, `skald vocab add merget` a word, and `skald config`,
`skald mic` and `skald model` change what the settings window does. It talks to the running app over a
connection only you can open, or writes the configuration file when Skald is not running, and every command
takes `--json`. `skald skill install` installs a Claude skill that teaches an agent the commands, and `skald mcp`
offers them as MCP tools. All of them are in [docs/design.md](docs/design.md#command-line).

## Credits

Skald's speech models are made by the National Library of Norway: NB-Whisper Large, the default, by its NoSTram
project, and NB-ASR Bokfink by its NB-ASR project. They are published under the Apache License 2.0; Skald
downloads a model from the National Library's own repository on Hugging Face and does not redistribute it.
Skald transcribes with transcribe.cpp by handy-computer and ggml, finds speech with Silero VAD by the Silero
Team, and sets the overlay's text in Alegreya Sans. [THIRD-PARTY-NOTICES](THIRD-PARTY-NOTICES) credits all of
it, and every Rust crate in the program, with its licence.

## Documents

| File | Holds |
|---|---|
| [docs/design.md](docs/design.md) | How Skald behaves and looks |
| [docs/plan.md](docs/plan.md) | Milestones and what each contains |
| [docs/decisions.md](docs/decisions.md) | What was decided, when and why |
| [docs/research.md](docs/research.md) | Models, licences, existing tools, the stack |
| [docs/working-method.md](docs/working-method.md) | How the work is run |

## Licence

MIT, see [LICENSE](LICENSE). `python3 scripts/notices/notices.py` writes
[THIRD-PARTY-NOTICES](THIRD-PARTY-NOTICES) from `Cargo.lock`; run it after changing a dependency, as CI checks
that the file is up to date.
