# Releasing Skald

How a release is cut, and what has to be in place before the first one. The pipeline is
[.github/workflows/release.yml](../.github/workflows/release.yml); why it is built this way is D-58 in
[decisions.md](../docs/decisions.md). Everything in this file is done by Thomas: creating the repositories, the
keys, the secrets and the tags.

## What a release does

An annotated tag `vX.Y.Z` pushed to GitHub starts the workflow:

1. **version** requires the tag to be `v` and the workspace's version in `Cargo.toml`, takes the release notes from
   the tag's annotation, and requires the five Mac secrets below, with a certificate that is team `Q4LJS2JYGD`'s
   Developer ID Application certificate and in date. A release that cannot finish stops here, before any build.
2. **macos**, **windows** and **linux** build at once on GitHub's runners, with no secret: `packaging/macos/bundle.sh`
   (signed ad hoc), `packaging/windows/package.ps1` and `manifests.ps1`, and `packaging/linux/tarball.sh` with
   skald-bin built, installed and run on Arch by `packaging/arch/container.sh`.
3. **macos-sign** signs the bundle with the Developer ID in a keychain of its own (`packaging/macos/sign.sh`),
   notarizes, staples and packs it (`notarize.sh`), runs the signed `skald`, and writes the cask
   (`cask.sh`), which Homebrew lints. It never runs cargo.
4. **release** writes `SHA256SUMS`, creates the GitHub release as a draft with the files below and the notes,
   publishes it, and downloads every file back to compare it with what the run made.
5. **publish** commits the cask to `icebit-no/homebrew-tap` as `Casks/skald.rb`, the Scoop manifest to
   `icebit-no/scoop-bucket` as `bucket/skald.json`, and pushes `PKGBUILD` and `.SRCINFO` to the AUR's
   `skald-bin`. Each target whose key is not set is skipped with a notice, and the others go on.
6. The winget manifests are the run's artifact `winget`, for a pull request to winget-pkgs by hand (below).

| Release file | Is |
|---|---|
| `Skald-X.Y.Z.zip` | Skald.app for Apple Silicon, signed, notarized and stapled |
| `skald-X.Y.Z-windows-x86_64.zip` | The Windows zip (D-51), not signed until SignPath Foundation signs it |
| `skald-X.Y.Z-windows-x86_64.zip.sha256` | Its SHA-256, which Scoop's `autoupdate` reads |
| `skald-X.Y.Z-linux-x86_64.tar.gz` | The Linux tarball (D-52), which skald-bin installs |
| `SHA256SUMS` | The SHA-256 of the three archives |

A tag with a pre-release part, as `v0.2.0-rc.1`, becomes a GitHub pre-release and goes to no tap, bucket or AUR.
A version older than one already released, a fix on an older line, is not marked as the latest release and goes
to none of them either, so they keep the newer one.

## The dry run

A dry run builds and packs everything a release would, rehearses the Mac signing with a self-signed identity made
on the runner and thrown away, and keeps what a release would publish as the run's artifacts: `release` (the
files above), `macos` (the zip and `homebrew/skald.rb`), `windows` (the zip and `manifests/scoop/skald.json`),
`winget`, `linux` (the tarball and `aur/`) and `notes`. It reads no secret, signs nothing with the Developer ID and
publishes nothing. It runs on every pull request that changes the packaging or the workflow, and by hand on any
branch:

```sh
gh workflow run release.yml -R icebit-no/skald --ref main
```

## The secrets

All are secrets of `icebit-no/skald`. GitHub never gives one back, so each is kept in Thomas's 1Password (account
`my.1password.com`, vault `icebit`) and set from there.

| Secret | Holds | Comes from | Needed for |
|---|---|---|---|
| `DEVELOPER_ID_P12_BASE64` | The *Developer ID Application* certificate of team `Q4LJS2JYGD` with its private key, exported from Keychain Access as a `.p12` file, in base64 | Ferric's, tag `ferric` | Every release |
| `DEVELOPER_ID_P12_PASSWORD` | The password the `.p12` file was exported with | Ferric's, tag `ferric` | Every release |
| `APPLE_API_KEY_P8` | An App Store Connect API key with the Developer role: the text of its `AuthKey_<id>.p8` file | Ferric's, tag `ferric` | Every release |
| `APPLE_API_KEY_ID` | That key's id, ten letters and digits | Ferric's, tag `ferric` | Every release |
| `APPLE_API_ISSUER_ID` | The issuer id shown above the team's keys, a UUID | Ferric's, tag `ferric` | Every release |
| `HOMEBREW_TAP_DEPLOY_KEY` | The private half of an SSH key whose public half is a deploy key with write access on `icebit-no/homebrew-tap` | New, tag `skald` | The cask |
| `SCOOP_BUCKET_DEPLOY_KEY` | The private half of an SSH key whose public half is a deploy key with write access on `icebit-no/scoop-bucket` | New, tag `skald` | The Scoop manifest |
| `AUR_SSH_KEY` | The private half of an SSH key whose public half is on the AUR account that maintains `skald-bin` | New, tag `skald` | skald-bin |

The five Apple secrets are the ones Ferric's release reads, under the same names: the certificate and the key
belong to the team, and every Skald release has to be signed by that team (D-34). The workflow warns from 30 days
before the certificate expires and stops once it has; a renewed one is set the same way.

### Setting the Apple secrets

Each command pipes the value from 1Password straight into `gh secret set`, so no secret touches a file or the
screen. Ferric's items name the secrets they fill in their `config key` field; their titles are listed, without
any secret, with:

```sh
OP_ACCOUNT=my.1password.com op item list --vault icebit --tags ferric
```

With `CERT`, `CERT_PASSWORD_FIELD`, `KEY`, `KEY_ID_FIELD` and `ISSUER_FIELD` replaced by those items' titles and
fields:

```sh
# The .p12 file, kept as a Document, in base64
OP_ACCOUNT=my.1password.com op document get 'CERT' --vault icebit | base64 | gh secret set -R icebit-no/skald DEVELOPER_ID_P12_BASE64
OP_ACCOUNT=my.1password.com op read -n 'op://icebit/CERT/CERT_PASSWORD_FIELD' | gh secret set -R icebit-no/skald DEVELOPER_ID_P12_PASSWORD
# The .p8 file, kept as a Document, and the key's id and issuer
OP_ACCOUNT=my.1password.com op document get 'KEY' --vault icebit | gh secret set -R icebit-no/skald APPLE_API_KEY_P8
OP_ACCOUNT=my.1password.com op read -n 'op://icebit/KEY/KEY_ID_FIELD' | gh secret set -R icebit-no/skald APPLE_API_KEY_ID
OP_ACCOUNT=my.1password.com op read -n 'op://icebit/KEY/ISSUER_FIELD' | gh secret set -R icebit-no/skald APPLE_API_ISSUER_ID
```

Where an item keeps the base64 text in a field rather than the `.p12` file, `op read -n 'op://icebit/CERT/FIELD'`
takes the place of `op document get … | base64`. `-n` leaves out the newline `op read` would print after a
value that is used as one word.

### Making the three SSH keys

Each key is made by 1Password, so its private half is never written to a disk or shown:

```sh
for name in homebrew-tap scoop-bucket aur; do
  OP_ACCOUNT=my.1password.com op item create --category ssh --ssh-generate-key=ed25519 \
    --vault icebit --tags skald --title "Skald - $name key" > /dev/null
done
```

The public halves, which are not secret:

```sh
gh repo deploy-key add <(OP_ACCOUNT=my.1password.com op read 'op://icebit/Skald - homebrew-tap key/public key') \
  -R icebit-no/homebrew-tap --allow-write --title 'skald release'
gh repo deploy-key add <(OP_ACCOUNT=my.1password.com op read 'op://icebit/Skald - scoop-bucket key/public key') \
  -R icebit-no/scoop-bucket --allow-write --title 'skald release'
OP_ACCOUNT=my.1password.com op read 'op://icebit/Skald - aur key/public key'
```

A deploy key that `gh` adds is tied to `gh`'s authorization and goes if that is revoked; pasted under the
repository's Settings → Deploy keys (with «Allow write access») it is not. The AUR key's public half goes under
My Account → SSH Public Key on aur.archlinux.org, on a line of its own beside any key already there.

The private halves, in OpenSSH's format:

```sh
OP_ACCOUNT=my.1password.com op read 'op://icebit/Skald - homebrew-tap key/private key?ssh-format=openssh' | gh secret set -R icebit-no/skald HOMEBREW_TAP_DEPLOY_KEY
OP_ACCOUNT=my.1password.com op read 'op://icebit/Skald - scoop-bucket key/private key?ssh-format=openssh' | gh secret set -R icebit-no/skald SCOOP_BUCKET_DEPLOY_KEY
OP_ACCOUNT=my.1password.com op read 'op://icebit/Skald - aur key/private key?ssh-format=openssh' | gh secret set -R icebit-no/skald AUR_SSH_KEY
```

## Before the first release

1. Create the tap and the bucket, shared by every icebit app (D-14), each with a `main` branch; a README is enough,
   and the workflow adds `Casks/` and `bucket/`:

   ```sh
   gh repo create icebit-no/homebrew-tap --public --add-readme --description 'Homebrew casks for icebit apps'
   gh repo create icebit-no/scoop-bucket --public --add-readme --description 'Scoop manifests for icebit apps'
   ```

   Users then install with `brew install --cask icebit-no/tap/skald`, and with
   `scoop bucket add icebit https://github.com/icebit-no/scoop-bucket` and `scoop install icebit/skald`.
2. An AUR account to maintain `skald-bin`, with the AUR key's public half. The name was free on 2026-11-28; the
   workflow's first push creates the package, with that account as its maintainer.
3. The secrets above. Without the five Apple ones no release runs; without a key its target is skipped.
4. A dry run of main, as above.

## Cutting a release

1. Set the version under `[workspace.package]` in `Cargo.toml`, run `cargo build` so `Cargo.lock` follows, and
   merge that to main through a pull request.
2. Write the release notes in Markdown. The tag's annotation is the release's notes on GitHub, and «Sagaen» on the
   site reads them from there.
3. Tag main's commit and push the tag:

   ```sh
   git switch main && git pull --ff-only
   git tag -a v0.1.0 -F notes.md
   git push origin v0.1.0
   ```

4. Watch the run: `gh run watch -R icebit-no/skald` and pick the release run.
5. Send the winget manifests (below).

A tag is never moved or used twice. A job that failed is run again with «Re-run failed jobs»: the release step
takes up a draft it left, and compares the files of a release already published with the run's. A failure that
needs a change to the repository is fixed on main and released as the next patch version.

## winget

winget-pkgs takes a pull request from a person's account, so it is sent by hand once the release is out:

```sh
gh run download RUN_ID -R icebit-no/skald -n winget -D winget
```

The artifact holds `manifests/i/Icebit/Skald/X.Y.Z/`, laid out as in winget-pkgs. On a Windows machine,
`winget validate --manifest winget/manifests/i/Icebit/Skald/X.Y.Z` and
`winget install --manifest winget/manifests/i/Icebit/Skald/X.Y.Z` check it. Then copy the folder to the same path
in a fork of https://github.com/microsoft/winget-pkgs and open a pull request titled
`New version: Icebit.Skald version X.Y.Z` (`New package: …` the first time).
