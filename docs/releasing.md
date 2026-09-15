# Releasing

For maintainers. Installing is in the README.

## How a release happens

1. **Commits merged into `main` decide the version.** They are Conventional
   Commits already: before 1.0, `feat` and breaking changes bump the minor
   version, `fix` the patch.
2. **release-please keeps a release pull request open** (`release-please.yml`),
   with the next version and its `CHANGELOG.md` section, and updates it at every
   merge.
3. **Merging that pull request releases.** release-please bumps the version,
   tags the merge commit `vX.Y.Z`, opens a **draft** release carrying the
   changelog, and calls `release.yml`.
4. **`release.yml` builds and publishes**:
   - Windows: NSIS and MSI;
   - Linux: `.deb` and `.rpm`, each checked to contain the udev rule;
   - then `SHA256SUMS`, and the draft is published.

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
  and publishing requires exactly the four installers and `SHA256SUMS`.
- **Numbers-only versions.** The MSI refuses a pre-release version, so a tag like
  `v1.2.0-beta` is refused before anything is built.
- **No cache.** What ships is built from nothing another run left behind.
- **Least privilege.** Actions are pinned to a commit, and each job gets only the
  permissions it needs.

## Things to know

- **The release pull request runs no CI.** release-please opens it with
  `GITHUB_TOKEN`, and GitHub starts no workflow for it. It only changes versions
  and the changelog; the release build compiles and packages everything, and CI
  runs on `main` after the merge.
- **Repository setting:** *Settings → Actions → General → Allow GitHub Actions to
  create and approve pull requests* must be on, or release-please cannot open its
  pull request.
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

## Not yet

- **Authenticode signing:** Windows warns about an unknown publisher. It is also
  what a Microsoft Store listing of the MSI or EXE requires.
- **An updater** (`tauri-plugin-updater`): until then, updating is installing the
  next release.
