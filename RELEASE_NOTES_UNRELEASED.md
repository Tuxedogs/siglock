# SigLock - Unreleased Patch Notes

Draft source update: October 29, 2026
Based on: v0.1.0-beta.11

These are patch notes for the next release, **not a published beta update**.

## What's new

- **Detection history survives restarts.** SigLock retains the most recent 200 OCR events, including scan status, time, detected material, rock count, and repeated-read totals.
- **Region controls are on the dashboard.** Create and manage capture profiles inside OCR Capture without switching to a separate Regions page.
- **Edit coordinates where you see them.** Click the coordinate readout to adjust physical screen bounds. Enter saves, Escape cancels, and destructive region operations require confirmation.

## Improvements

- **Less duplicate noise.** Repeated stationary OCR results are consolidated for 15 seconds (previously 3 seconds).
- **Clearer dashboard.** Current Materials shows recent detections with relative timestamps, and panel sizing and scrolling have been refined for constrained windows.
- **Safer multi-monitor capture.** Validates edited capture bounds against the physical virtual-desktop coordinate range, including negative-origin secondary displays.

## Cleanup

- Removed the obsolete persistent capture-preview screen and its unused native capture/PNG encoding command.
- Updated the relevant feature acceptance checks.

## Release status

This update is committed to the development branch only. It does not change the application version, publish a signed installer, or notify installed beta clients. A future tagged release must follow RELEASING.md.
