# Releasing SigLock

SigLock uses Tauri's signed updater and GitHub Releases. Installed copies check:

`https://github.com/Tuxedogs/siglock/releases/latest/download/latest.json`

## One-time GitHub setup

Generate and securely back up a Tauri updater signing key outside the
repository. Never commit or share the private key. Losing it prevents existing
installations from trusting future updates.

In the GitHub repository, open **Settings > Secrets and variables > Actions** and
create these repository secrets:

- `TAURI_SIGNING_PRIVATE_KEY`: the complete contents of the updater private key
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: required only when the key is password-protected

## Publish a release

1. Update the version in `package.json`, `src-tauri/Cargo.toml`, and
   `src-tauri/tauri.conf.json`.
2. Commit and push the changes.
3. Create and push a matching tag, for example:

   ```powershell
   git tag v0.1.0-beta.1
   git push origin v0.1.0-beta.1
   ```

4. GitHub Actions builds a draft release containing the installer, signature,
   and `latest.json`.
5. Test the draft installer, then publish the release in GitHub.

Users can then select **Check Updates** inside SigLock. Tauri downloads the
signed installer, verifies it, installs it, and relaunches the app.

Release installers and other generated binaries belong in GitHub Releases, not
in the source repository.
