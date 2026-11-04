# Releasing aless

A release is one run of `.github/workflows/release.yml`. You dispatch
the run by hand, on `main`, with the tag to make.
[dist](https://github.com/axodotdev/cargo-dist) generates that workflow
from [`dist-workspace.toml`](dist-workspace.toml), so never edit the
workflow by hand. To change the workflow, change `dist-workspace.toml`
and run `dist generate`. Every pull request runs `dist plan`. That
check fails when the workflow and `dist-workspace.toml` disagree.

This page keeps the reason for each step beside the step.

## What a release publishes

A release publishes a GitHub Release with its files, the Homebrew
formula, and the crate on crates.io.

### The GitHub Release

The run creates a GitHub Release named `vX.Y.Z`, where `X.Y.Z` is the
version in `Cargo.toml`. It creates the tag when it creates the
Release, so a run that fails before the Release exists leaves no tag.
The tag is at the commit you dispatched the run on. The run attaches
every file at once.

Those files include an archive for each target. Each archive holds
`aless` or `aless.exe`, `LICENSE`, `README.md`, `CHANGELOG.md`,
`THIRD_PARTY_NOTICES.md`, the man page `man/aless.1`, and the shells'
completions under `completions/`. Each archive comes with its `.sha256`
file. The archive is named for its target:

| Target | Archive |
|---|---|
| Linux x86_64, glibc 2.35 or newer | `aless-x86_64-unknown-linux-gnu.tar.xz` |
| Linux x86_64, static | `aless-x86_64-unknown-linux-musl.tar.xz` |
| Linux aarch64, glibc 2.35 or newer | `aless-aarch64-unknown-linux-gnu.tar.xz` |
| Linux aarch64, static | `aless-aarch64-unknown-linux-musl.tar.xz` |
| macOS x86_64 | `aless-x86_64-apple-darwin.tar.xz` |
| macOS aarch64 | `aless-aarch64-apple-darwin.tar.xz` |
| Windows x86_64 | `aless-x86_64-pc-windows-msvc.zip` |
| Windows aarch64 | `aless-aarch64-pc-windows-msvc.zip` |

The run also attaches the installers `aless-installer.sh` and
`aless-installer.ps1`, the Homebrew formula `aless.rb`, `source.tar.gz`,
`sha256.sum`, a CycloneDX software bill of materials (SBOM) named
`aless.cdx.xml`, and `dist-manifest.json`.

Each attached file has a build-provenance attestation. The command
`gh attestation verify` checks the attestation.

cargo-auditable embeds each binary's dependency tree in that binary, so
`cargo audit bin "$(command -v aless)"` audits an installed copy.

### The Homebrew formula

[`publish-homebrew.yml`](.github/workflows/publish-homebrew.yml) pushes
the formula to
[rjrodger/homebrew-tap](https://github.com/rjrodger/homebrew-tap).
`brew install rjrodger/tap/aless` installs aless from that tap. The
formula is the one dist writes, with three additions:

- It installs the man page where the formulas Homebrew itself ships put
  theirs.
- It installs the bash, zsh, fish, and PowerShell completions the same
  way.
- It carries a test for `brew test`.

### The crate

[`publish-crates.yml`](.github/workflows/publish-crates.yml) publishes
the crate to crates.io. The workflow uses trusted publishing. When the
version in `Cargo.toml` is on crates.io already, the workflow publishes
nothing.

## Once, before the first release

The steps in this section need the owner's accounts, so an automated
session cannot do them.

### Make the Homebrew tap

1. On GitHub, create the public repository `rjrodger/homebrew-tap`
   with a README, so that it has a first commit. Homebrew reads
   `rjrodger/tap` as that repository, so
   `brew install rjrodger/tap/aless` works.
2. Create a fine-grained personal access token whose only repository is
   `rjrodger/homebrew-tap`, with the permission
   **Contents: Read and write**.
3. Store the token in this repository as the Actions secret
   `HOMEBREW_TAP_TOKEN`.

### Publish the first crates.io version by hand

crates.io accepts a trusted publisher only for a crate that exists, so
you publish the first version with a token:

1. On crates.io, create an API token with the `publish-new` and
   `publish-update` scopes. Limit it to the crate `aless`, and have it
   expire in a day.
2. From a clean checkout of the commit you are releasing, run
   `CARGO_REGISTRY_TOKEN=TOKEN cargo publish --locked`, where `TOKEN` is
   the token from step 1.
3. In the crate's settings on crates.io, add a trusted publisher:
   GitHub, owner `rjrodger`, repository `aless`, workflow `release.yml`,
   environment `release`. Name the workflow `release.yml`, not
   `publish-crates.yml`: in a called workflow, the OpenID Connect (OIDC)
   token names the caller.
4. Require trusted publishing for the crate.
5. Revoke the token.

### Set up the homepage

The homepage is `https://aless.tabnas.dev`. `Cargo.toml` names it as
`homepage`. crates.io and the Homebrew formula link to it, so it should
answer before the first release.

[`pages.yml`](.github/workflows/pages.yml) builds the site from `docs/`.
It runs on every push to `main`. Once you have set up Pages, the workflow
deploys the site too. Until then, the workflow builds and checks the
site, and skips the deploy with a notice. To set up the homepage, take
these steps:

1. Verify `aless.tabnas.dev` for your account, and not `tabnas.dev`:
   verifying `tabnas.dev` would keep the tabnas organisation's
   repositories off `tabnas.dev` and off every name directly under it.
   GitHub recommends verifying a domain before you add it to a
   repository. In your profile, open **Settings** > **Pages** >
   **Add a domain**, and add `aless.tabnas.dev`.
2. In the Cloudflare DNS for `tabnas.dev`, add the `TXT` record GitHub
   gives, at the name `_github-pages-challenge-rjrodger.aless`.
3. Click **Verify**. Keep the `TXT` record afterwards. A verified name can
   be published only from your account's repositories, so no one else
   can claim it. That protection holds even if this repository's setting
   is lost or Pages is turned off while the `CNAME` record of step 6
   still points at GitHub.
4. In this repository, open **Settings** > **Pages**, and set **Source**
   to **GitHub Actions**.
5. On the same page, before the `CNAME` record exists, set
   **Custom domain** to `aless.tabnas.dev`, and save. A record that
   points at GitHub before a repository claims the name lets someone
   else's repository claim it. With an Actions workflow, GitHub reads the domain from this
   setting, not from a `CNAME` file.
6. In the Cloudflare DNS for `tabnas.dev`, add a `CNAME` record with the
   name `aless` and the target `rjrodger.github.io`, set to
   **DNS only** (the grey cloud). GitHub can then see the record and
   issue the certificate. Add no wildcard record: GitHub warns that one
   invites takeovers.
7. To run the `pages` workflow, open **Actions** > **pages** >
   **Run workflow** on `main`. Merging anything runs it too.
8. Once the DNS check on **Settings** > **Pages** passes, select
   **Enforce HTTPS**. The option can take up to a day to appear, while
   GitHub issues the certificate.

### Turn on release immutability

Optional: under **Settings** > **General** > **Releases**, turn on
release immutability. A published release can then never be changed,
and its tag can never be moved or reused. Release immutability is
recommended rather than required.

### Keep the `release` environment

By the maintainer's choice, the `release` environment has no
required reviewer. So crates.io publishes as soon as the GitHub Release
exists, with no approval step. Keep the environment itself, named
`release`, because the crates.io trusted publisher names it. Keep the
deployment-branch rule for `main` on the environment, where one is set,
so that only a run from `main` can publish.

## Each release

1. Set the version, in a pull request:
   1. Set `version` in `Cargo.toml`.
   2. Run `cargo update --workspace`, because CI builds with
      `--locked`. The command moves only the entry for aless itself
      in `Cargo.lock`.
   3. In [CHANGELOG.md](CHANGELOG.md), rename `## [Unreleased]` to
      `## [X.Y.Z] - YYYY-MM-DD`, where `YYYY-MM-DD` is the date of the
      release. That section becomes the Release's notes.
   4. Run `admin/scripts/generate.sh`, because the man page names the
      version and the date the changelog gives that version. CI fails
      until the man page does.
   5. In [PACKAGING.md](PACKAGING.md), set the version in the Arch
      User Repository (AUR) recipe (`pkgver`) and in the Scoop manifest
      (`version` and the two download URLs). The published crate
      carries that page as a packager's starting point.
   6. Once CI passes, merge the pull request.
2. Make a dry run. Open **Actions** > **release** > **Run workflow** on
   `main`, and leave the tag as `dry-run`. Every target builds. The run
   publishes nothing.
3. Make the release the same way, with the tag `vX.Y.Z`. The tag must
   match `Cargo.toml`'s version.
4. Check the release with the GitHub CLI, Homebrew, and Cargo:

   ```bash
   gh release view vX.Y.Z --repo rjrodger/aless
   gh release download vX.Y.Z --repo rjrodger/aless --pattern 'aless-x86_64-unknown-linux-gnu.tar.xz*'
   sha256sum -c aless-x86_64-unknown-linux-gnu.tar.xz.sha256
   gh attestation verify aless-x86_64-unknown-linux-gnu.tar.xz --repo rjrodger/aless
   brew install rjrodger/tap/aless && brew test rjrodger/tap/aless && man -w aless
   cargo install --locked aless   # or: cargo binstall aless
   ```

## When a run fails

A run can fail before the Release exists: in `plan`, in a build, or in
`host`. Such a run publishes nothing. It makes no tag either, because
the run creates the tag when it creates the Release. Fix the cause and
dispatch again.

A run can fail after the Release exists: in the formula or on
crates.io. Such a run leaves the Release and the tag standing. Fix the
cause, such as the secret or the trusted publisher, and re-run the
failed jobs of the same run. A new dispatch would fail on the existing
Release.

Never delete a published release to retry it. Release a new patch
version instead. Under release immutability, the release's tag can
never be reused. A crates.io version can only be yanked.

## Changing how aless is released

1. Edit `dist-workspace.toml`.
2. Install dist at the version `dist-workspace.toml` names, with the
   installer dist itself provides, and run `dist generate`:

   ```bash
   curl --proto '=https' --tlsv1.2 -LsSf https://github.com/axodotdev/cargo-dist/releases/download/v0.33.0/cargo-dist-installer.sh | sh
   dist generate
   ```

3. Commit both files, `dist-workspace.toml` and
   `.github/workflows/release.yml`.

To move to a newer dist:

1. Change `cargo-dist-version` in `dist-workspace.toml`.
2. Run `dist init`.
3. Read the diff.
4. Re-pin the actions in `[dist.github-action-commits]` to the majors
   the new workflow uses, at their latest releases.

[`publish-homebrew.yml`](.github/workflows/publish-homebrew.yml) and
[`admin/scripts/homebrew-formula.py`](admin/scripts/homebrew-formula.py)
are this repository's own, not files that dist writes. The script adds
the man page, the completions, and a test to the formula dist writes.
It adds them after the formula's `install_binary_aliases!` line, so it
refuses a formula without that line. A dry run does not publish, so it
runs neither the workflow nor the script. Instead, after a dist
upgrade, take `aless.rb` from a dry run's `artifacts-build-global`
artifact, and check that
`python3 admin/scripts/homebrew-formula.py aless.rb` still writes the
formula.

A release made with `GITHUB_TOKEN` starts no other workflow. So a
further publishing step, such as winget or a Scoop bucket, belongs in
`publish-jobs`, not in a workflow triggered `on: release`.
