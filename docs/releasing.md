# Releasing

For maintainers. Installing is in the README.

## How a release happens

1. **Commits merged into `main` decide the version.** They are Conventional
   Commits already: before 1.0, `feat` and breaking changes bump the minor
   version, `fix` the patch.
2. **release-please keeps a release pull request open** (`release-please.yml`),
   titled `Go release vX.Y.Z 🚀`, with the next version and its `CHANGELOG.md`
   section, and updates it at every merge.
3. **Merging that pull request releases.** release-please bumps the version,
   tags the merge commit `vX.Y.Z`, opens a **draft** release carrying the
   changelog, and calls `release.yml`.
4. **`release.yml` builds and publishes**:
   - Windows: the Store's MSIX package, kept as an artifact of the run;
   - Linux: `.deb` and `.rpm`, each checked to contain the udev rule;
   - then `SHA256SUMS`, and the draft is published.
5. **Once it is public, it goes to the Store**, which certifies and signs the
   package. Each step does nothing until what it needs is set up.
6. **Once certified, a maintainer attaches the signed package** and sends it to
   winget, with one command (below). Windows ships nothing else: no installer
   of its own, which would need an Authenticode certificate (#305).

One version for the whole application, in `package.json`,
`apps/desktop/package.json`, `packages/effects-api/package.json`,
`tauri.conf.json`, the workspace `Cargo.toml`, and the three workspace crates in
`Cargo.lock`. release-please updates all of them; `release.yml` refuses to build
when one disagrees with the tag.

## What `release.yml` guarantees

- **One commit.** The commit is resolved once, and every job builds that SHA.
- **Tag and draft point at that commit**, checked before the first build and
  again right before publishing.
- **A published release is never rebuilt.** Uploads replace one file at a time,
  so a rebuild would show a mix of old and new installers.
- **Only this run's assets.** A reused draft loses every asset from earlier runs,
  and publishing requires exactly the two Linux packages and `SHA256SUMS`. The
  signed package joins them after publishing, with its line in `SHA256SUMS`.
- **Numbers-only versions.** An MSIX version is four numbers, so a tag like
  `v1.2.0-beta` is refused before anything is built.
- **No cache.** What ships is built from nothing another run left behind.
- **Least privilege.** Actions are pinned to a commit, and each job gets only the
  permissions it needs.

## Things to know

- **The release pull request runs no CI.** release-please opens it with
  `GITHUB_TOKEN`. It only changes versions and the changelog; the release build
  compiles and packages everything, and CI runs on `main` after the merge. Since
  June 2026 GitHub creates CI runs for it anyway, held for approval. Nobody
  approves them, and some end as failures without a job, which read as a broken
  release: `release-please.yml` deletes them after each update.
- **Repository setting:** *Settings → Actions → General → Allow GitHub Actions to
  create and approve pull requests* must be on, or release-please cannot open its
  pull request.
- **Pull request titles are plain language.** GitHub puts the title in every
  merge commit, as its body or its subject, and no setting leaves it out.
  Written as a Conventional Commit, it would appear in the changelog next to the
  commits it merges; 0.1.0 needed that duplicate removed by hand.
- **The first release** is 0.1.0: the commit adding this pipeline carries a
  `Release-As: 0.1.0` footer, and `bootstrap-sha` starts the changelog there.

## When a release fails

The draft stays a draft; nothing is public.

- **A transient failure:** re-run the failed jobs from the Actions tab.
- **A fix is needed:** merge it, then rebuild the draft from its tag:

  ```bash
  gh workflow run release.yml -f tag=vX.Y.Z
  ```

  The tag must exist and point at the commit to build. The fix therefore ships
  in the next version, unless the tag is moved on purpose.
- **Already published:** release a new version. To rebuild it anyway, turn it
  back into a draft first (`gh release edit vX.Y.Z --draft`), then run the
  command above. A run that published and then reported a failure has finished:
  check its assets rather than re-running.

## After publishing: the Microsoft Store

The `windows` job builds the MSIX package from the executable it just built
(`packaging/windows/msix.mjs`, `docs/design/msix.md`) and keeps it as the run's
`msix` artifact, not as a release file. Once the release is public, the `store`
job submits it (#162):

1. the package is uploaded to a **draft** submission;
2. the texts of `packaging/store/listing-*.md` replace those of the draft (short
   description, description, features, search terms), with a *What's new*
   gathered from `packaging/store/news/`: a line per file added since the
   previous release, after the version and before a link to its release notes.
   The submission goes to certification;
3. with no file added, the draft stays in Partner Center, with a warning on the
   run: write its *What's new* there and submit it by hand.

So **a change users will see brings its line**, in both languages, in its own
pull request (`packaging/store/news/README.md`). The release pull request
carries a comment with the *What's new* as it will be sent, updated at each
merge; `node packaging/store/listing.mjs X.Y.Z` prints everything that would be.

What the job needs, set once under *Settings → Secrets and variables → Actions*:

| Name | Kind | Where it comes from |
|---|---|---|
| `MSIX_IDENTITY_NAME` | variable | Partner Center, *Product identity*: `Package/Identity/Name` |
| `MSIX_PUBLISHER` | variable | the same page: `Package/Identity/Publisher`, the `CN=` string |
| `MSIX_PUBLISHER_DISPLAY_NAME` | variable | the same page: `Package/Properties/PublisherDisplayName` |
| `STORE_PRODUCT_ID` | variable | the Store ID, `9…`, on the same page |
| `PARTNER_CENTER_TENANT_ID` | secret | the Microsoft Entra tenant associated with Partner Center |
| `PARTNER_CENTER_CLIENT_ID` | secret | an Entra application added in Partner Center, *User management → Microsoft Entra applications*, with the **Manager** role |
| `PARTNER_CENTER_CLIENT_SECRET` | secret | a client secret of that application |
| `PARTNER_CENTER_SELLER_ID` | secret | Partner Center, *Account settings → Identifiers* |

Without the `MSIX_*` variables no package is built; without `STORE_PRODUCT_ID`
nothing is submitted. A client secret expires: renew it before it does, or the
job fails at the next release. A failed `store` job is re-run on its own from the
run's page; the artifact stays with the run.

## After certification: the signed package, and winget

Windows ships the package the Microsoft Store signs (#305): downloaded from the
Store once certified, attached to the release, and installed by winget. Run on
a Windows signed in with the Microsoft 365 account of the Partner Center tenant:

```powershell
pwsh packaging/windows/store-package.ps1 -Version X.Y.Z
```

It downloads the package with `winget download -s msstore`, checks its version
and its Microsoft Marketplace signature, attaches it as `Candeo_X.Y.Z_x64.msix`,
adds its line to `SHA256SUMS`, and starts `winget.yml`. Run before the
certification ends, it stops: the Store still serves the previous version.

**Why by hand:** the Store hands its signed package to an Entra ID user account
only, which `winget download` reads from the Windows session; it takes no
application credentials, so no runner can do it. It also needs *disconnected
(offline) licensing* allowed in Partner Center (*Pricing and availability* →
*Organizational licensing*), set once.

**What it does not cover:** the package's first launch takes the Store's licence
of a free app, so a Windows without the Store (LTSC, Windows Sandbox) installs it
and does not start it.

The manifests are rendered by `packaging/winget/winget.mjs`:

```powershell
node packaging/winget/winget.mjs X.Y.Z
winget validate --manifest target\winget\manifests\o\O2CSI\Candeo\X.Y.Z
```

The package's checksum comes from the `SHA256SUMS` of the release; the package
is downloaded only for the hash of its signature, and checked against that line.

`winget.yml` does this when the script starts it (#180), then opens the pull request
on [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) with
`wingetcreate submit`. It needs a `WINGET_TOKEN` secret: a classic personal
access token with the `public_repo` scope, from an account that has a fork of
winget-pkgs. It stops, saying why, when there is no token, when winget already
has the version or a pull request proposes it, and **while the package itself is
not in winget yet**: the first submission is a new-package pull request, made
by hand and reviewed by a moderator. Once it is accepted, send the versions
released since:

```bash
gh workflow run winget.yml -f tag=vX.Y.Z
```
