# Changelog

## v0.1.0-beta.10

### Fixed

- Restricted the branded startup screen to the main application window; the in-game HUD overlay no longer receives a logo or splash layer.

## v0.1.0-beta.9

### Fixed

- Updated the sidebar and packaged application icon to use the supplied transparent SigLock logo.
- Replaced the blank startup flash with a branded SigLock loading screen.

## v0.1.0-beta.8

### Fixed

- Set both lines of the sidebar utility/build footer to a consistent 9px size.
- Fixed expanded Settings categories clipping into adjacent content; Settings now scrolls cleanly when a section exceeds the available height.
- Removed the duplicate OCR capture preview from Settings.

## v0.1.0-beta.7

### Changed

- Replaced the packaged Windows taskbar icon, installer icon set, and web favicon with the supplied SigLock by Tux PNG.
- Reduced the sidebar utility/build footer text by a further 3px.

## v0.1.0-beta.6

### Changed

- Updated the sidebar lockup with the supplied SigLock by Tux PNG logo.
- Refined the OCR capture label and sidebar build footer to a more compact size.

### Fixed

- Restored the visible HUD preview in the Overlay workspace while retaining its live style bindings.

### Known Issues

- Settings categories can clip at constrained window sizes.
- Overlay background opacity currently fades before the far edge and does not fill the entire overlay surface.

## v0.1.0-beta.5

### Added

- Added independent OCR region profiles for Golem, Prospector, MOLE, and custom ships, with a persistent active profile.
- Added per-profile overlay-position persistence, plus safe restoration and reset controls.
- Added a location-aware Minables index and a persistent watch list for materials relevant to the current system and location.
- Added configurable overlay text, background, accent, opacity, size, compact mode, high contrast, result lifetime, composition, and signature-value controls.

### Changed

- Overhauled the desktop workspace with a clearer instrumentation hierarchy, readable metadata, and a responsive sidebar treatment.
- Refreshed the dashboard OCR, scanner, watch-list, material, and overlay-preview panels around the active ship profile.
- Made dashboard and Settings HUD previews match the live overlay’s configured colors, opacity, accent, and text size.

### Fixed

- Fixed monitor-relative capture coordinates and validation for mixed-DPI, secondary, and negative-coordinate monitor layouts.
- Fixed region and overlay-position persistence races during native-settings hydration and verify saved region state after writes.
- Improved Game.log discovery, monitoring recovery, scan de-duplication, OCR normalization, and diagnostic reporting for unreliable scans.
- Fixed overlay opacity so the configured value is fully applied instead of being attenuated a second time.

## v0.1.0-beta.4

### Improvements

- Refined Settings into distinct Shortcuts, Scan, Overlay, and Advanced Debug sections.
- Added concise section descriptions and clearer expand/collapse indicators.
- Kept version, update, and release-note controls visually separate from Settings content.

## v0.1.0-beta.3

### Added

- Added a saved-region capture preview in Settings with explicit refresh support.
- Added realistic solved-result mock data to the overlay Settings preview.
- Added persistent controls for Salvage, FPS/ROC mineables, rock Composition, Signature Value, and solved-only overlay captures.
- Added the always-visible All, Stanton, Pyro, and Nyx system filter for system-aware signature disambiguation.
- Added generated rock-composition data so secondary materials describe materials inside the matched rock rather than unrelated shared-signature candidates.

### Changed

- Shared signatures remain match candidates but are filtered by the selected system before choosing the primary result.
- Capture previews now update only when Settings opens, the user refreshes, or a saved region changes.
- Paused scanning with a valid saved region now reports “Ready when you are.”
- Region state is hydrated and validated before the UI can report that the region is missing.

### Fixed

- Fixed a native-settings race that could overwrite the saved capture region with stale overlay-position data, and now verify region persistence immediately after save.
- Improved OCR normalization and retry diagnostics so failed reads log the active region, crop size, raw OCR text, normalized signature text, candidate matches, and final resolution path.
- Replaced "Secondary Materials" with true Rock Composition driven by the solved primary material, including the corrected Agricium composition profile in overlay displays.
- Fixed repeated preview capture calls and continuous `last_capture.png` debug writes during scanning.
- Fixed titlebar dragging and double-click behavior caused by overlapping native drag mechanisms.
- Fixed stale region status caused by region hydration running after slower startup initialization.
- Replaced picker polling with explicit saved and cancelled outcomes; Escape preserves the existing region and prior scanner status.
- Ensured drag state clears on mouse-up, mouse-leave, blur, visibility loss, and component teardown.

### Update Notes

- Beta 2 installations can update through Settings after the signed updater manifest is published and validated.
- The GitHub release must remain a published, non-prerelease release because SigLock uses `/releases/latest/download/latest.json`.
- The installer is not Windows Authenticode signed, so Windows SmartScreen may display an unrecognized publisher warning.

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
