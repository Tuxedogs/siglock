# SigLock v0.1.0 Beta 5 — Open Beta

Beta 5 is a substantial workspace and reliability release for the mining scanner.

## Highlights

- Rebuilt the mining workspace around a clearer, more readable deep-space instrumentation UI.
- Added independent OCR region profiles for Golem, Prospector, MOLE, and custom ships.
- Persisted each profile’s OCR region and overlay position, with safe reset and monitor-aware restoration.
- Added a location-aware Minables index and a persistent watch list.
- Expanded overlay customization for text, background, accent, opacity, size, compact mode, high contrast, result lifetime, composition, and signature values.

## Reliability fixes

- Fixed capture-coordinate handling across secondary, mixed-DPI, and negative-coordinate monitors.
- Prevented native-settings startup races from overwriting saved regions or overlay positions.
- Improved Game.log discovery and monitoring recovery, scan de-duplication, OCR normalization, and diagnostic detail for scan failures.
- Fixed live overlay opacity handling; the dashboard and Settings previews now accurately reflect the configured overlay style.

## Validation

- Type checks, feature acceptance checks, production build, and Rust tests pass locally.
- The Windows installer and signed updater artifacts are produced by GitHub Actions after the `v0.1.0-beta.5` tag is published.

## Update notes

- This is a published non-prerelease GitHub Release so installed SigLock copies can use the updater endpoint.
- The installer is not Windows Authenticode signed, so Windows SmartScreen may show an unrecognized publisher warning.
