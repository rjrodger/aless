# Releasing aless

A release is one run of `.github/workflows/release.yml`, dispatched by hand
on `main` with the tag to make. [dist](https://github.com/axodotdev/cargo-dist)
generates that workflow from [`dist-workspace.toml`](dist-workspace.toml):
change the config and run `dist generate`, never the workflow by hand.
Every pull request runs `dist plan`, which fails when the two disagree.

## What a release publishes

- **A GitHub Release, `v<version>`.** The run creates it, and with it the
  tag, at the commit it was dispatched on, with every file attached at
  once:
  - an archive per target, holding `aless` (or `aless.exe`), `LICENSE`,
    `README.md`, `CHANGELOG.md`, `THIRD_PARTY_NOTICES.md`, the man page
    (`man/aless.1`) and the shells' completions (`completions/`), each
    archive with its `.sha256`:

    | Target | Archive |
    |---|---|
    | Linux x86_64, glibc ≥ 2.35 | `aless-x86_64-unknown-linux-gnu.tar.xz` |
    | Linux x86_64, static | `aless-x86_64-unknown-linux-musl.tar.xz` |
    | Linux aarch64, glibc ≥ 2.35 | `aless-aarch64-unknown-linux-gnu.tar.xz` |
    | Linux aarch64, static | `aless-aarch64-unknown-linux-musl.tar.xz` |
    | macOS x86_64 | `aless-x86_64-apple-darwin.tar.xz` |
    | macOS aarch64 | `aless-aarch64-apple-darwin.tar.xz` |
    | Windows x86_64 | `aless-x86_64-pc-windows-msvc.zip` |
    | Windows aarch64 | `aless-aarch64-pc-windows-msvc.zip` |

  - the installers `aless-installer.sh` and `aless-installer.ps1`;
  - the Homebrew formula `aless.rb`, `source.tar.gz`, `sha256.sum`,
    a CycloneDX SBOM (`aless.cdx.xml`) and `dist-manifest.json`;
  - a build-provenance attestation for each file, which
    `gh attestation verify` checks.

  Each binary embeds its dependency tree (cargo-auditable), so
  `cargo audit bin "$(command -v aless)"` audits an installed copy.
- **The Homebrew formula,** pushed to
  [rjrodger/homebrew-tap](https://github.com/rjrodger/homebrew-tap) by
  [`publish-homebrew.yml`](.github/workflows/publish-homebrew.yml):
  `brew install rjrodger/tap/aless`. It is the formula dist writes, with
  the man page and the bash, zsh, fish and PowerShell completions
  installed where Homebrew's own formulas put theirs, and a test for
  `brew test`.
- **The crate on crates.io,** by trusted publishing
  ([`publish-crates.yml`](.github/workflows/publish-crates.yml)), or
  nothing when that version is there already.

## Once, before the first release

These need the owner's accounts, so an automated session cannot do them.

1. **The Homebrew tap.** On GitHub, create the public repository
   `rjrodger/homebrew-tap`, with a README so that it has a first commit.
   Homebrew reads `rjrodger/tap` as that repository, which is what makes
   `brew install rjrodger/tap/aless` work. Create a fine-grained personal
   access token whose only repository is `rjrodger/homebrew-tap`, with
   *Contents: Read and write*. Store it in this repository as the Actions
   secret `HOMEBREW_TAP_TOKEN`.
2. **The first crates.io version, by hand.** crates.io accepts a trusted
   publisher only for a crate that exists, so the first version goes up
   with a token:
   1. On crates.io, create an API token with the `publish-new` and
      `publish-update` scopes, limited to the crate `aless`, expiring in
      a day.
   2. From a clean checkout of the commit you will release:
      `CARGO_REGISTRY_TOKEN=… cargo publish --locked`.
   3. In the crate's settings on crates.io, add a trusted publisher:
      GitHub, owner `rjrodger`, repository `aless`, workflow `release.yml`,
      environment `release`. Then require trusted publishing for the
      crate, and revoke the token.

   The workflow named is `release.yml`, not `publish-crates.yml`: in a
   called workflow, the OIDC token names the caller.
3. **The homepage, `https://aless.tabnas.dev`.** `Cargo.toml`'s
   `homepage` names it, and crates.io and the Homebrew formula link to it,
   so it should answer before the first release.
   [`pages.yml`](.github/workflows/pages.yml) builds the site from `site/`
   on every push to `main`, and deploys it once Pages is set up; until
   then it builds and checks the site and skips the deploy with a notice.
   1. Verify `aless.tabnas.dev` for your account first, as GitHub
      recommends before a domain is added to a repository: your profile's
      *Settings → Pages → Add a domain*, then the TXT record it gives
      (in Cloudflare's DNS for `tabnas.dev`, the name
      `_github-pages-challenge-rjrodger.aless`), then *Verify*. Keep the
      record. A verified name can be published only from your account's
      repositories, so no one else can claim it, even if this
      repository's setting is lost or Pages is turned off while the
      `CNAME` record below still points at GitHub. Verify
      `aless.tabnas.dev`, not `tabnas.dev`: verifying `tabnas.dev` would
      keep the tabnas organisation's repositories off `tabnas.dev` and
      every name directly under it.
   2. *Settings → Pages*: set *Source* to *GitHub Actions*.
   3. On the same page, set *Custom domain* to `aless.tabnas.dev` and
      save. Do this before the `CNAME` record exists: a record that
      points at GitHub before a repository claims the name lets someone
      else's repository claim it. With an Actions workflow GitHub reads
      the domain from this setting, not from a `CNAME` file.
   4. In Cloudflare's DNS for `tabnas.dev`, add a `CNAME` record, name
      `aless`, target `rjrodger.github.io`, *DNS only* (grey cloud), so that
      GitHub can see the record and issue the certificate. No wildcard
      record: GitHub warns that one invites takeovers.
   5. *Actions → pages → Run workflow* on `main`, or merge anything.
   6. Once the DNS check on *Settings → Pages* passes, tick *Enforce
      HTTPS*. The option can take up to a day to appear, while the
      certificate is issued.
4. **Recommended:** turn on release immutability (*Settings → General →
   Releases*). A published release can then never be changed, and its tag
   never moved or reused.

   The `release` environment has no required reviewer, by the maintainer's
   choice: crates.io publishes as soon as the GitHub Release exists, with
   no approval step. Keep the environment itself, named `release`, since
   the crates.io trusted publisher names it, and a deployment-branch rule
   for `main` on it, where one is set, so that only a run from `main` can
   publish.

## Each release

1. **The version.** In a pull request:
   - set `version` in `Cargo.toml`;
   - run `cargo update --workspace`, which moves only aless's own entry
     in `Cargo.lock`, since CI builds with `--locked`;
   - rename `## Unreleased` in [CHANGELOG.md](CHANGELOG.md) to
     `## [X.Y.Z] - YYYY-MM-DD`, which is what becomes the Release's notes;
   - run `scripts/generate.sh`: the man page names the version, and the
     date the changelog gives it, and CI fails until it does.

   Merge it once CI is green.
2. **A dry run.** *Actions → release → Run workflow*, on `main`, leaving the
   tag as `dry-run`. Every target builds, and nothing is published.
3. **The release.** The same, with the tag `vX.Y.Z`, which must equal
   `Cargo.toml`'s version.
4. **Check it.**

   ```bash
   gh release view vX.Y.Z --repo rjrodger/aless
   gh release download vX.Y.Z --repo rjrodger/aless --pattern 'aless-x86_64-unknown-linux-gnu.tar.xz*'
   sha256sum -c aless-x86_64-unknown-linux-gnu.tar.xz.sha256
   gh attestation verify aless-x86_64-unknown-linux-gnu.tar.xz --repo rjrodger/aless
   brew install rjrodger/tap/aless && brew test rjrodger/tap/aless && man -w aless
   cargo install --locked aless   # or: cargo binstall aless
   ```

## When a run fails

- **Before the Release exists** (`plan`, a build, or `host` failed),
  nothing is published and no tag exists. Fix the cause and dispatch again.
- **After it** (the formula or crates.io failed), the Release and tag stand.
  Fix the cause, such as the secret or the trusted publisher, and re-run
  the failed jobs of the same run. A new dispatch would fail on the
  existing Release.
- **Never delete a published release to retry it.** Release a new patch
  version instead. Under release immutability its tag can never be reused,
  and crates.io versions can only be yanked.

## Changing how aless is released

1. Edit `dist-workspace.toml`.
2. Run dist's own installer at the version it names:

   ```bash
   curl --proto '=https' --tlsv1.2 -LsSf https://github.com/axodotdev/cargo-dist/releases/download/v0.33.0/cargo-dist-installer.sh | sh
   dist generate
   ```

3. Commit both files.

To move to a newer dist, change `cargo-dist-version`, run `dist init`, and
read the diff. Then re-pin the actions in `[dist.github-action-commits]` to
the majors the new workflow uses, at their latest releases.

[`publish-homebrew.yml`](.github/workflows/publish-homebrew.yml) and
[`scripts/homebrew-formula.py`](scripts/homebrew-formula.py) are this
repository's own, not dist's: the script adds the man page, the
completions and a test to the formula dist writes, after its
`install_binary_aliases!` line, and refuses a formula without one. A dry
run does not publish, so it does not run them: after a dist upgrade, take
`aless.rb` from a dry run's `artifacts-build-global` artifact and check
that `python3 scripts/homebrew-formula.py aless.rb` still writes the
formula.

A release made with `GITHUB_TOKEN` starts no other workflow. So any
further publishing step, such as winget or a Scoop bucket, belongs in
`publish-jobs`, and not in a workflow triggered `on: release`.
