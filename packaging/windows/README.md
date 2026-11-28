# Skald for Windows

How Skald is packed for Windows, and how the manifests for Scoop and winget are made. Why it is packed this way
is in [D-51](../../docs/decisions.md); how the release workflow publishes the zip and the manifests is in
[RELEASING.md](../RELEASING.md).

| File | Does |
|---|---|
| `package.ps1` | Builds `skald` in release mode, packs `target\package\skald-<version>-windows-x86_64.zip`, checks it and writes its SHA-256 |
| `manifests.ps1` | Fills the Scoop and winget manifests from a version and the zip's SHA-256 |
| `README.txt` | The README in the zip, for the user, in bokmål |
| `scoop/skald.json` | The Scoop manifest for `icebit-no/scoop-bucket` (D-14), with placeholders |
| `winget/Icebit.Skald*.yaml` | The three winget manifests for `Icebit.Skald`, with placeholders |

The zip holds `skald.exe`, the one binary that is also the command line (D-25), and beside it the DLLs
transcribe.cpp builds: `transcribe.dll`, `ggml.dll`, `ggml-base.dll`, one `ggml-cpu-*.dll` per instruction-set
tier and `ggml-vulkan.dll`, which Skald loads at run time (D-43). Then `README.txt`, `LICENSE.txt` and, once it
exists, `THIRD-PARTY-NOTICES.txt`. The files sit at the top of the zip. The exe carries Skald's icon, which the
binary's build script packs from the PNG files in [assets/icon](../../assets/icon/README.md) on every build, so no
`.ico` is kept in the repository.

## 1. Build the zip

Needs Windows 10 or 11 with:

- Visual Studio 2022 or its Build Tools, with «Desktop development with C++», which brings MSVC and the Windows
  SDK (GPUI compiles its shaders with the SDK's `fxc.exe` in release builds);
- CMake;
- Rust through rustup (the toolchain in `rust-toolchain.toml` is installed on first use);
- [LunarG's Vulkan SDK](https://vulkan.lunarg.com/sdk/home#windows) 1.4.350.0, the version CI uses, for the
  shader compiler and the Vulkan headers.

```powershell
pwsh packaging/windows/package.ps1
```

It builds `skald` in release mode, checks that every DLL is there, packs the zip, unpacks it into a folder of its
own and starts `skald.exe --version` there, so a DLL the zip lacks shows up here and not on a user's machine, and
writes `skald-<version>-windows-x86_64.zip.sha256` beside the zip. `-NoBuild` packs what `target\release`
already holds.

The exe and the DLLs need Microsoft's Visual C++ runtime (2015–2022, x64), which most Windows machines have.
It is not put in the zip; winget installs it as a dependency, Scoop suggests it, and the README says where to
get it.

## 2. Fill the manifests

```powershell
pwsh packaging/windows/manifests.ps1 -Version 0.1.0 -Sha256 <the zip's SHA-256>
```

It writes `target\package\manifests\scoop\skald.json` and the three winget manifests under
`target\package\manifests\winget\manifests\i\Icebit\Skald\<version>\`, laid out as
[winget-pkgs](https://github.com/microsoft/winget-pkgs) keeps them. The zip's address is the release asset on
GitHub unless `-Url` names another; the release date is today unless `-ReleaseDate` names another. The script
refuses a version, hash, date or address that is not one, and checks that no placeholder is left.

- **Scoop** installs the zip, puts a shim for `skald` on PATH and a Skald shortcut in the Start menu. Its
  `autoupdate` reads the next version's hash from the `.sha256` file beside the zip, so the release uploads both.
- **winget** installs the zip as a portable package and puts its folder on PATH (`ArchiveBinariesDependOnPath`),
  since a command alias in winget's Links folder would not find the DLLs beside the exe. The manifests use
  schema 1.28.0 and the locale nb-NO, and were checked against winget's JSON schemas. Before the first release,
  `winget validate` and a local `winget install --manifest` on a Windows machine are worth running.

The `windows-package` workflow runs both scripts for every change to main and keeps the zip, its hash and the
manifests as the run's artifact for two weeks, so the newest build can be tried on a Windows machine without
building it.

## 3. Sign (not yet)

Windows builds are to be signed through [SignPath Foundation](https://signpath.org) (D-14), which signs
open-source projects for free with a certificate issued to SignPath Foundation; Windows and SmartScreen then show
SignPath Foundation as the publisher. It needs an application by Thomas at https://signpath.org/apply.html, and
SignPath's conditions as they stood on 2026-11-28:

- An OSI-approved licence and a public repository: Skald is MIT, at github.com/icebit-no/skald.
- No proprietary component in what is signed. The zip holds only what is built from the repository's sources;
  the Visual C++ runtime stays out of it.
- A fully automated build from the repository on a build service SignPath can verify, here GitHub Actions, so
  the signed files come from the checked-in source. The release workflow (M6) builds the zip as
  `windows-package` does and hands it to SignPath's GitHub action before it is uploaded.
- Multi-factor authentication for everyone on the team, on GitHub and on SignPath.
- A code signing policy on the project's site or in the repository: who may commit, who reviews and who
  approves a signing, and the attribution «Free code signing provided by SignPath.io, certificate by SignPath
  Foundation». A privacy statement belongs there too: Skald sends nothing but a model download and a check for a
  new version.

Once the application is accepted, SignPath gives an organisation id, a project and a signing policy, and an API
token for CI. The token goes into 1Password and from there into the repository's secrets; the workflow signs
`skald.exe` and every DLL inside the zip before it is published, and a signed zip gets a new SHA-256, so the
manifests are filled after signing.
