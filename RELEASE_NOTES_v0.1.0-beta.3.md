# SigLock v0.1.0 Beta 3 — Open Beta

This release opens SigLock to public beta testing with clearer scan results, system-aware material matching, and a more reliable region-selection workflow.

## Highlights

- Preview the saved capture region directly in Settings and refresh it on demand.
- See a realistic solved scan in the overlay preview without running the scanner.
- Control whether results include Salvage, FPS/ROC mineables, rock Composition, the scanned Signature Value, or only solved captures.
- Filter ambiguous signatures by All, Stanton, Pyro, or Nyx. For signature `3185`, Stanton resolves to Quantanium, Pyro to Stileron, and Nyx to Savrillium.
- View true composition materials from the matched rock data instead of unrelated materials that happen to share a signature.

## Reliability fixes

- Saved region state loads and validates before a missing-region state can appear.
- A paused scanner with a saved region reports “Ready when you are.”
- Saving or cancelling the picker now has an explicit outcome; Escape keeps the existing region and restores the previous scanner status.
- Only the title strip initiates window dragging, and double-click no longer maximizes or restores the window.
- Drag state is cleared on mouse-up, mouse-leave, blur, visibility loss, and teardown.
- Capture preview no longer runs continuously or writes repeated debug captures while scanning.

## Update notes

- Beta 2 users can install this release through **Settings → Check for Updates** after the signed updater manifest is published and validated.
- The installer is not Windows Authenticode signed, so Windows SmartScreen may show an unrecognized publisher warning.
- OCR accuracy still depends on capture region, game UI scale, contrast, and effects behind the scanned value.
