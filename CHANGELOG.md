# Changelog

## v0.1.0-beta.2

### Added

- Added Settings-based manual update checking and release notes access.
- Added a Settings panel with shortcut, scan, overlay, and debug sections.
- Added configurable Manual Scan and Toggle Auto Scan shortcuts, including supported mouse-button bindings.
- Added optional display of scanned values on the overlay.
- Added richer scan feed details for invalid, skipped, no-match, and matched scans, including nearest-candidate context for no-match reads.

### Changed

- Refined the beta UI around current scan status, capture region state, auto scan controls, current finds, and scan feed filtering.
- Changed the default Toggle Auto Scan shortcut to `Ctrl+Shift+S`.
- Kept Active Scan disabled on launch as a safety measure.
- Updated release workflow metadata for `v0.1.0-beta.2` and the Tauri updater release path.

### Fixed

- Fixed the GitHub release workflow action reference by using `tauri-apps/tauri-action@v0`.
- Added guarded shutdown handling that stops Active Scan, unregisters shortcuts, closes secondary windows, and exits once. Shutdown cleanup needs verification on tester machines.
- Persisted the selected scan region through native settings. Region persistence needs verification on tester machines.

### Update Notes

- Beta 1 users must manually download and install beta.2 because beta.1 did not include in-app update checking.
- Starting with beta.2, update checking is available from Settings if the GitHub release publishes a valid signed `latest.json`.
- Do not treat updater support as verified end to end until the beta.2 release is published, `latest.json` is validated, and an installed older build successfully updates.
- GitHub Actions must have `TAURI_SIGNING_PRIVATE_KEY` configured to publish updater signatures and `latest.json`.

### Known Issues

- The installer is not Windows Authenticode code-signed, so Windows SmartScreen may show an unrecognized publisher warning.
- The updater requires a published, non-prerelease GitHub release because the app checks `/releases/latest/download/latest.json`.
- OCR accuracy still depends on region selection, game UI scale, contrast, and visual effects behind the scanned number.
- Region persistence and shutdown cleanup are implemented but still need tester verification.
