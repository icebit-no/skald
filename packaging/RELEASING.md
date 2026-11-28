# Releasing Skald

How a release is cut, and what has to be in place before the first one. The pipeline is
[.github/workflows/release.yml](../.github/workflows/release.yml); why it is built this way is D-58 and D-59 in
[decisions.md](../docs/decisions.md). Everything in this file is done by Thomas: creating the repositories, the
keys, the secrets and the tags, and approving each release.

## What a release does

An annotated tag `vX.Y.Z` pushed to GitHub starts the workflow:

1. **approval** waits until Thomas approves the release ([below](#approving-a-release)). Nothing runs before he
   does.
2. **version** requires the tag to be `v` and the workspace's version in `Cargo.toml`, and takes the release notes
   from the tag's annotation. A release with the wrong tag stops here, before any build.
3. **macos**, **windows** and **linux** build at once on GitHub's runners, with no secret: `packaging/macos/bundle.sh`
   (signed ad hoc), `packaging/windows/package.ps1` and `manifests.ps1`, and `packaging/linux/tarball.sh` with
   skald-bin built, installed and run on Arch by `packaging/arch/container.sh`.
4. **macos-sign** requires the five Mac secrets below, with a certificate that is team `Q4LJS2JYGD`'s Developer
   ID Application certificate and in date. Then it signs the bundle with the Developer ID in a keychain of its own
   (`packaging/macos/sign.sh`), and notarizes, staples and packs it (`notarize.sh`). It never runs cargo.
   **macos-cask** runs the signed `skald` from the zip and writes the cask (`cask.sh`), which Homebrew lints.
5. **release** writes `SHA256SUMS`, creates the GitHub release as a draft with the files below and the notes,
   publishes it, and downloads every file back to compare it with what the run made.
6. **publish** commits the cask to `icebit-no/homebrew-tap` as `Casks/skald.rb`, the Scoop manifest to
   `icebit-no/scoop-bucket` as `bucket/skald.json`, and pushes `PKGBUILD` and `.SRCINFO` to the AUR's
   `skald-bin`. Each target whose key is not set is skipped with a notice, and the others go on.
7. The winget manifests are the run's artifact `winget`, for a pull request to winget-pkgs by hand (below).

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

## Who can start a release, and who can read the secrets

The secrets can sign any app with team `Q4LJS2JYGD`'s Developer ID and push to the tap, the bucket and the AUR. Skald's repository is public
and takes pull requests, so three settings on GitHub keep the secrets away from everything except a release that
Thomas has approved.

**The tag ruleset «Release tags».** Only an admin of the repository can create, move or delete a tag that starts
with `v`, and Thomas is the only admin. A `v` tag is what starts a release, so nobody else can start one or move
a released tag to other code. The ruleset is under Settings → Rules → Rulesets.

**The environment `release`.** A GitHub environment is a named set of rules, and of secrets if it has any, that a
job in a workflow can name. A job that names one waits until the rules are met. `release` has Thomas as its
required reviewer and admits only runs started by a `v` tag. It holds no secret. The job **approval** names it,
so every release waits for Thomas to approve it once, before anything is built.

**The environment `release-keys`.** It holds all eight secrets below. It admits only runs started by a `v` tag,
so a run on a branch or a pull request, even one that changes the workflow, cannot read them; and since they are
not repository secrets, a workflow that does not name `release-keys` gets nothing. It has no reviewer, so
**macos-sign** and **publish**, the jobs that read the secrets, do not ask Thomas a second time.

Both environments are under Settings → Environments. A dry run uses neither of them.

## Approving a release

After `git push origin vX.Y.Z`:

1. The run starts at once and stops at **approval**, which shows as *Waiting*. GitHub notifies Thomas that a
   deployment is waiting for his review.
2. Open the run: the repository's **Actions** tab, the workflow **release** on the left, then the newest run, which
   shows the tag under its title. A banner above the jobs says the run is waiting for review.
3. Click **Review deployments**, tick `release`, write a comment if you like, and click **Approve and deploy**.
4. The rest of the run goes on without asking again.

**Reject** instead stops the run before anything is built or published. A run that nobody approves fails after
30 days. «Re-run failed jobs» does not ask again, since **approval** has already passed; «Re-run all jobs» asks
again.

The same approval from a terminal, with the run's id from `gh run list -R icebit-no/skald -w release`:

```sh
gh api -X POST repos/icebit-no/skald/actions/runs/RUN_ID/pending_deployments \
  -F "environment_ids[]=$(gh api repos/icebit-no/skald/environments/release -q .id)" \
  -f state=approved -f comment='Release'
```

## The dry run

A dry run builds and packs everything a release would, rehearses the Mac signing with a self-signed identity made
on the runner and thrown away, and keeps what a release would publish as the run's artifacts: `release` (the
files above), `macos` (the zip and `homebrew/skald.rb`), `windows` (the zip and `manifests/scoop/skald.json`),
`winget`, `linux` (the tarball and `aur/`) and `notes`. It uses no environment, asks for no approval, reads no
secret, signs nothing with the Developer ID and publishes nothing. It runs on every pull request that changes the
packaging or the workflow, and by hand on any branch:

```sh
gh workflow run release.yml -R icebit-no/skald --ref main
```

## The secrets

All are secrets of the environment `release-keys` on `icebit-no/skald`, never of the repository: a repository
secret can be read by a workflow run on any branch. GitHub never gives a secret back, so each is kept in Thomas's
1Password (account `my.1password.com`, vault `icebit`) and set from there.

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

The names of the secrets that are set, without their values:

```sh
gh secret list --env release-keys -R icebit-no/skald
```

A secret set on the repository by mistake is deleted with `gh secret delete NAME -R icebit-no/skald` and set
again with `--env release-keys`.

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
OP_ACCOUNT=my.1password.com op document get 'CERT' --vault icebit | base64 | gh secret set DEVELOPER_ID_P12_BASE64 --env release-keys -R icebit-no/skald
OP_ACCOUNT=my.1password.com op read -n 'op://icebit/CERT/CERT_PASSWORD_FIELD' | gh secret set DEVELOPER_ID_P12_PASSWORD --env release-keys -R icebit-no/skald
# The .p8 file, kept as a Document, and the key's id and issuer
OP_ACCOUNT=my.1password.com op document get 'KEY' --vault icebit | gh secret set APPLE_API_KEY_P8 --env release-keys -R icebit-no/skald
OP_ACCOUNT=my.1password.com op read -n 'op://icebit/KEY/KEY_ID_FIELD' | gh secret set APPLE_API_KEY_ID --env release-keys -R icebit-no/skald
OP_ACCOUNT=my.1password.com op read -n 'op://icebit/KEY/ISSUER_FIELD' | gh secret set APPLE_API_ISSUER_ID --env release-keys -R icebit-no/skald
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
OP_ACCOUNT=my.1password.com op read 'op://icebit/Skald - homebrew-tap key/private key?ssh-format=openssh' | gh secret set HOMEBREW_TAP_DEPLOY_KEY --env release-keys -R icebit-no/skald
OP_ACCOUNT=my.1password.com op read 'op://icebit/Skald - scoop-bucket key/private key?ssh-format=openssh' | gh secret set SCOOP_BUCKET_DEPLOY_KEY --env release-keys -R icebit-no/skald
OP_ACCOUNT=my.1password.com op read 'op://icebit/Skald - aur key/private key?ssh-format=openssh' | gh secret set AUR_SSH_KEY --env release-keys -R icebit-no/skald
```

## Before the first release

1. Create the tap and the bucket, shared by every icebit app (D-14), each with a `main` branch; a README is enough,
   and the workflow adds `Casks/` and `bucket/`:

   ```sh
   gh repo create icebit-no/homebrew-tap --public --add-readme --description 'Homebrew casks for icebit apps'
   gh repo create icebit-no/scoop-bucket --public --add-readme --description 'Scoop manifests for icebit apps'
   ```

   Homebrew loads casks only from a tap its user trusts, so users install with `brew tap icebit-no/tap`,
   `brew trust icebit-no/tap` and `brew install --cask icebit-no/tap/skald`; with Scoop,
   `scoop bucket add icebit https://github.com/icebit-no/scoop-bucket` and `scoop install icebit/skald`.
2. A rule in Cloudflare Email Routing that forwards `skald@icebit.no` to Thomas. skald-bin's PKGBUILD names it as
   the package's maintainer, so Arch users who write about the package reach him there.
3. An AUR account to maintain `skald-bin`, with the AUR key's public half; `skald@icebit.no` can be its email
   address. The name was free on 2026-11-28; the workflow's first push creates the package, with that account as
   its maintainer.
4. The secrets above, in `release-keys`. Without the five Apple ones no release runs; without a key its target is
   skipped.
5. A dry run of main, as above.
6. A pre-release, as below, to try the approval, the secrets, the signing and the notarization before a version
   goes to users.

## A first pre-release

A pre-release runs everything a release does up to the GitHub release, which it marks as a pre-release. It goes
to no tap, bucket or AUR, so no user's install changes. `v0.1.0-rc.1` is the first:

1. On a branch from main, set the version under `[workspace.package]` in `Cargo.toml` to `0.1.0-rc.1`, run
   `cargo build` so `Cargo.lock` follows, and open a pull request.
2. Before merging, try the version on the branch with a dry run, since a change to `Cargo.toml` alone does not
   start one:

   ```sh
   gh workflow run release.yml -R icebit-no/skald --ref BRANCH
   ```

3. Merge it, then tag main's commit and push the tag:

   ```sh
   git switch main && git pull --ff-only
   git tag -a v0.1.0-rc.1 -m 'Skald 0.1.0-rc.1' -m 'A pre-release to try the release pipeline.'
   git push origin v0.1.0-rc.1
   ```

4. Approve it, as above, and watch it with `gh run watch -R icebit-no/skald`.
5. Check the pre-release on https://github.com/icebit-no/skald/releases: the five files are there, and for the Mac
   zip, downloaded and unpacked, `spctl -a -vv Skald.app` says `accepted` and `source=Notarized Developer ID`.

A pre-release may stay on the releases page. If you delete it, delete the release only and keep the tag, since a
tag is never used twice. Then set the version to `0.1.0` and release it as below.

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

4. Approve the release ([above](#approving-a-release)).
5. Watch the run: `gh run watch -R icebit-no/skald` and pick the release run.
6. Send the winget manifests (below).

A tag is never moved or used twice; the ruleset refuses both to everyone but an admin. A job that failed is run again with «Re-run
failed jobs»: the release step takes up a draft it left, and compares the files of a release already published
with the run's. A failure that needs a change to the repository is fixed on main and released as the next patch
version.

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
