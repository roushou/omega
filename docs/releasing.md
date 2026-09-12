# Releasing Omega

Omega's workspace crates share one version. Publish the crates before publishing
the GitHub release: the CLI scaffolds configurations against that version of the
SDK and document crate.

Update the workspace version, internal dependency requirements, lockfile, and
bundled renderer manifest. Use `cliff.toml` to prepend the new release's notes,
then review and commit the result. For example:

```sh
git cliff --offline --github-repo roushou/omega --unreleased --tag v0.2.3 --prepend CHANGELOG.md
cargo publish --workspace --dry-run
```

After the checks pass, publish the crates and push the release commit and tag:

```sh
cargo publish --workspace
git tag -a v0.2.3 -m "Release v0.2.3"
git push origin main
git push origin v0.2.3
```

The `release` workflow validates the tag against the committed workspace, crate,
and renderer versions. It verifies that every publishable crate is available and
not yanked on crates.io, extracts this version's section from `CHANGELOG.md`, and
runs CI against the tagged commit. It builds and smoke-tests the Linux x86-64
binary on Ubuntu 22.04, giving the download a glibc 2.35 baseline. The archive
contains the binary, license, and installation instructions; `SHA256SUMS` is
uploaded alongside it. The renderer is embedded in the binary.

Only the final publishing job receives `contents: write`. It uploads into a draft
and publishes after all checks and uploads succeed. No personal access token is
needed. A failure may leave a draft; rerunning the workflow can replace its assets
and finish publication. An already-published release is refused rather than
overwritten. This workflow handles stable `vMAJOR.MINOR.PATCH` tags.

To backfill an existing tag, first land the workflow on the default branch, then
run it manually from GitHub Actions or the CLI:

```sh
gh workflow run release.yml --ref main -f tag=v0.2.2
```

Manual runs use the selected tag's code and changelog, with the default branch's
release automation. Do not move the tag. If registry propagation or a transient
service failure blocks a release, rerun the failed workflow after it resolves.

Check the release automation locally with:

```sh
python3 -m unittest discover -s .github -p 'test_release.py'
actionlint
python3 .github/release.py v0.2.2 /tmp/omega-release
```

The last command checks crates.io and writes notes and metadata locally; it does
not build, upload, or publish anything.
