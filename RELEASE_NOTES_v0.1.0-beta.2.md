# SigLock v0.1.0 Beta 2

SigLock v0.1.0-beta.2 is a closed beta release focused on the app UI, Settings flow, shortcuts, shutdown behavior, region persistence, and preparing the signed updater release path.

## Added

- Settings-based manual update checking and release notes access.
- Settings sections for shortcuts, scan controls, overlay options, and debug details.
- Configurable Manual Scan and Toggle Auto Scan shortcuts, including supported mouse-button bindings.
- Optional overlay display for scanned values.
- More detailed scan feed rows for matches, no-match reads, invalid OCR reads, skipped scans, and nearest-candidate context.

## Changed

- Refined the main beta UI around scanner state, region state, auto scan, current finds, and scan feed filtering.
- Changed the default Toggle Auto Scan shortcut to `Ctrl+Shift+S`.
- Active Scan still starts disabled after launch.

## Fixed

- Fixed the release workflow to use `tauri-apps/tauri-action@v0`.
- Added guarded shutdown handling to stop Active Scan, unregister shortcuts, close secondary windows, and exit once. This needs tester verification.
- Persisted the selected scan region in native settings. This needs tester verification.

## Update Notes

- Beta 1 users must manually download and install beta.2 because beta.1 did not include in-app update checking.
- Starting with beta.2, update checking is available from Settings if the updater manifest publishes correctly.
- Updater support should not be considered verified end to end until the GitHub release is published, `latest.json` is validated, and an installed older build successfully updates.
- The beta.2 GitHub release must be published as a normal release, not marked prerelease, because the app checks `/releases/latest/download/latest.json`.
- The installer is not Windows Authenticode signed. Windows SmartScreen may show an unrecognized publisher warning.

## Known Issues

- Region persistence needs verification after restart.
- Shutdown cleanup needs verification to confirm no SigLock or WebView2 processes remain.
- OCR accuracy still depends on capture region, game UI scale, contrast, and visual effects behind the number.
- Global shortcuts can conflict with shortcuts registered by other apps.
