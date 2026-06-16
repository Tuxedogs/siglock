# Releasing SigLock

SigLock uses Tauri's signed updater and GitHub Releases. Installed copies check:

`https://github.com/Tuxedogs/siglock/releases/latest/download/latest.json`

This endpoint only works for published, non-prerelease GitHub Releases. GitHub's
`/releases/latest` endpoint ignores draft releases and prereleases, so beta
builds that should be visible to the updater must be published as regular
releases while SigLock uses this endpoint.

## One-time GitHub setup

Generate and securely back up a Tauri updater signing key outside the
repository. Never commit or share the private key. Losing it prevents existing
installations from trusting future updates.

In the GitHub repository, open **Settings > Secrets and variables > Actions** and
create these repository secrets:

- `TAURI_SIGNING_PRIVATE_KEY`: the complete contents of the updater private key
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: required only when the key is password-protected

## Publish a release

1. Update the version in `package.json`, `package-lock.json`,
   `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, and
   `src-tauri/tauri.conf.json`. Update README/known-issues version text when it
   names the current release.
2. Commit and push the changes.
3. Create and push a matching tag, for example:

   ```powershell
   git tag v0.1.0-beta.2
   git push origin v0.1.0-beta.2
   ```

4. GitHub Actions builds a release containing the NSIS installer, installer
   `.sig`, and Tauri updater `latest.json`.
5. Confirm the release is published and is not marked as a prerelease.
6. Run the updater manifest validation:

   ```powershell
   $env:EXPECTED_VERSION="0.1.0-beta.2"
   $env:RELEASE_TAG="v0.1.0-beta.2"
   npm run validate:updater
   ```

7. From an installed older build, select **Check Updates** and confirm the app
   detects, installs, and relaunches into the new version.

Users can then select **Check Updates** inside SigLock. Tauri downloads the
signed installer, verifies it, installs it, and relaunches the app.

Release installers and other generated binaries belong in GitHub Releases, not
in the source repository.
