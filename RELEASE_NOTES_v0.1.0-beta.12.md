# SigLock v0.1.0 Beta 12

## Added

- Persistent OCR detection history retaining up to 200 recent events across restarts, including scan outcomes, materials, timestamps, and repeated-read counts.
- Inline editing for saved capture-region coordinates, with Enter to save and Escape to cancel.
- Confirmation before deleting or resetting capture regions.

## Improved

- Consolidated region management into the dashboard's OCR Capture area, removing the separate Regions page.
- Reorganized Current Materials, scanner, watch list, and overlay panels, with improved compact-window scrolling and relative detection times.
- Increased stationary OCR duplicate suppression from three to fifteen seconds.
- Added physical virtual-desktop coordinate validation for capture areas, including negative-origin secondary monitors.

## Removed

- Obsolete persistent capture-preview UI and unused native preview command.

## Updating

- This release is distributed through the signed SigLock updater and as a Windows installer on GitHub Releases.
- The installer is not Windows Authenticode signed; Windows SmartScreen may warn about an unrecognized publisher.
