# SigLock v0.1.0 Beta 7 — Open Beta

Beta 7 delivers the supplied SigLock by Tux mark as the actual packaged application icon.

## Improvements

- Regenerated the Windows taskbar and installer icon set from the supplied PNG logo.
- Updated the web favicon from the same square PNG source.
- Reduced the sidebar utility/build footer text by a further 3px.

## Known issues

- Settings categories can clip at constrained window sizes.
- Overlay background opacity currently fades before the far edge and does not fill the entire overlay surface.

## Validation

- Type checks, feature acceptance checks, production build, and Rust tests are run before release publication.
- GitHub Actions produces the Windows installer and signed updater artifacts after the `v0.1.0-beta.7` tag is published.
