SigLock v0.1.0-beta.2 is prepared for release.

Beta.1 users will need to manually download and install beta.2 because beta.1 did not include in-app update checking. Starting with beta.2, update checking is available from Settings once the GitHub updater manifest is published correctly.

Main changes:
- Refined main app UI and Settings flow.
- Added Settings-based update checking and release notes access.
- Added configurable Manual Scan and Toggle Auto Scan shortcuts.
- Added more detailed scan feed/no-match information.
- Added region persistence and cleaner shutdown handling, both needing real-machine verification.

Feedback I need:
- Does the selected region persist after restart?
- After closing SigLock, are there no remaining SigLock or WebView2 processes?
- Do Manual Scan and Toggle Auto Scan shortcuts behave correctly?
- Does Auto Scan start/stop and scan at the expected interval?
- For OCR/no-match cases, are the scan feed details useful and accurate?
- From Settings, what happens when you use Check for Updates?
