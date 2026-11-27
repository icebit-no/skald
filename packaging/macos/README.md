# Skald.app

How Skald is built into a macOS app bundle, signed with a Developer ID, notarized by Apple and stapled. Why the
bundle is made this way is in [D-34](../../docs/decisions.md); the release pipeline that runs these steps on a tag
belongs to M6 in [plan.md](../../docs/plan.md).

| File | Does |
|---|---|
| `bundle.sh` | Builds `skald` in release mode and makes `target/release/Skald.app`, signed ad hoc or with the Developer ID |
| `notarize.sh` | Sends a Developer ID bundle to Apple, staples the ticket, checks Gatekeeper's verdict and packs the zip |
| `Info.plist` | The bundle's Info.plist; `bundle.sh` fills in the version and the oldest macOS |
| `skald.entitlements` | The one entitlement the app has: the microphone |

The bundle holds one executable, `skald`, the same binary that is the command line (D-25). It has no Dock icon
(`LSUIElement`); Skald lives in the menu bar. The icon is packed with `iconutil` from the PNG files in
[assets/icon](../../assets/icon/README.md) on every build, so no `.icns` is kept in the repository.

## 1. Build

Needs macOS with Xcode or its command line tools, Rust through rustup (the toolchain in `rust-toolchain.toml` is
installed on first use) and CMake for transcribe.cpp (`brew install cmake`).

```sh
packaging/macos/bundle.sh
```

This builds for the Mac's own architecture, writes `target/release/Skald.app` and signs it ad hoc with the
hardened runtime and the microphone entitlement, so it runs as a released bundle does. macOS ties the microphone
and Accessibility grants to the signature, and an ad hoc signature changes with every build, so macOS asks for
both again after each one. Skald ships for Apple Silicon only (D-34), so a Developer ID build refuses any other
architecture.

## 2. Sign with the Developer ID

Needs the *Developer ID Application* identity of team `Q4LJS2JYGD` in the login keychain, and a network connection
for Apple's timestamp.

```sh
packaging/macos/bundle.sh --developer-id
```

The same build, signed with that identity, the hardened runtime and a secure timestamp. The grants then follow the
team rather than the build, so they survive rebuilds and updates. The first time codesign uses the key, macOS may
ask to allow it; «Always Allow» lets later builds sign without asking.

Both builds check what they made: the signature with `codesign --verify --strict`, the identifier
`app.icebit.skald`, the hardened runtime, the entitlements (the microphone and nothing else), that the executable
answers `skald --version` with the workspace's version, and for the Developer ID the team and the timestamp. Last
they print Gatekeeper's verdict from `spctl`, which rejects every bundle until it is notarized: «Unnotarized
Developer ID» is the expected answer here.

## 3. Notarize and staple

Needs a bundle from step 2, a network connection, and an App Store Connect API key with the Developer role
(App Store Connect → Users and Access → Integrations → Team Keys; the `.p8` file can be downloaded once). The key is
read from three environment variables:

| Variable | Holds |
|---|---|
| `APPLE_API_KEY_P8` | The text of the key's `AuthKey_<id>.p8` file |
| `APPLE_API_KEY_ID` | The key's id, ten letters and digits |
| `APPLE_API_ISSUER_ID` | The issuer id shown above the team's keys, a UUID |

Give them on the command line of the one run rather than exporting them. With the key in 1Password:

```sh
APPLE_API_KEY_P8="$(op read 'op://VAULT/ITEM/FIELD')" \
APPLE_API_KEY_ID="$(op read 'op://VAULT/ITEM/FIELD')" \
APPLE_API_ISSUER_ID="$(op read 'op://VAULT/ITEM/FIELD')" \
    packaging/macos/notarize.sh
```

`notarize.sh` takes `target/release/Skald.app`, or the bundle named as its argument. It removes the three
variables from its environment before it runs anything, writes the key to a file only the user can read and
deletes it however the run ends. Then it:

1. checks that the bundle is signed for the team, with the hardened runtime and a timestamp;
2. sends it with `notarytool submit --wait`, which usually takes a few minutes, and prints Apple's answer and log;
3. staples the ticket with `stapler staple`, so the bundle opens on a Mac without a network, and validates it;
4. requires Gatekeeper to answer `source=Notarized Developer ID`;
5. packs `target/release/Skald-VERSION.zip` with `ditto`, unpacks it again to check that the signature and the
   ticket survived, and prints its SHA-256.

## Install, and `skald` on the PATH

Quit Skald, remove an older copy, and copy the bundle with `ditto`, which keeps the signature and the ticket:

```sh
rm -rf /Applications/Skald.app
ditto target/release/Skald.app /Applications/Skald.app
```

Start Skald from /Applications, Spotlight or Launchpad, so macOS asks for the microphone and Accessibility in
Skald's own name; `skald` started from a terminal runs under the terminal's grants.

The command line is the executable inside the bundle. Link it into a folder on the PATH rather than copying it, so
it stays the same build as the app after every update:

```sh
mkdir -p ~/.local/bin
ln -sf /Applications/Skald.app/Contents/MacOS/skald ~/.local/bin/skald
```

`skald last`, `skald vocab add …` and the other commands then talk to the running Skald, or work on the
configuration file when it is not running ([design.md](../../docs/design.md#command-line)). The Homebrew cask
(D-14, M6) will make the same link for its users.

## Credits

`bundle.sh` and `notarize.sh` are adapted from the macOS packaging of Ferric (MIT, by the same author).
