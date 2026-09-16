# SigLock v0.1.0 Beta 6 — Open Beta

Beta 6 is a focused usability update for the desktop workspace.

## Improvements

- Added the supplied SigLock by Tux PNG logo to the sidebar lockup.
- Refined the OCR capture label and sidebar build footer for a more compact presentation.
- Restored the visible HUD preview in the Overlay workspace. It continues to reflect the configured overlay colors, opacity, accent, and text size.

## Known issues

- Settings categories can clip at constrained window sizes.
- Overlay background opacity currently fades before the far edge and does not fill the entire overlay surface.

## Validation

- Type checks, feature acceptance checks, production build, and Rust tests are run before release publication.
- GitHub Actions produces the Windows installer and signed updater artifacts after the `v0.1.0-beta.6` tag is published.
