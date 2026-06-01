# Siglock

Siglock is a lightweight desktop overlay for Star Citizen mining. It watches a user-selected scan region, reads visible scan signature values with OCR, and matches those values against a local signature table to identify likely mineable materials and rock counts.

## What it does

Siglock helps miners quickly interpret Star Citizen scan signatures without manually checking a table mid-flight.

Core features:

* Always-on-top desktop overlay
* User-selected scan region
* OCR-based scan detection
* Manual signature input fallback
* Active Scan / Active Watch mode
* Local signature matching
* Exact and near-match results
* OCR debug/capture preview tools
* Compact overlay-friendly UI

## Current status

Siglock is currently in early development.

Working:

* Region selection
* Region capture preview
* Tesseract OCR detection
* Manual input matching
* JSON-based signature database
* Scan result matching

In progress:

* Active Scan reliability
* Rolling scan history
* Better window dragging and overlay UX
* Release packaging / installer support

Planned:

* Scintel Build Queue target sharing
* Material target alerts
* Sound notification when needed materials are detected
* Bundled Tesseract support for easier installs

## Tech stack

* Tauri
* Svelte / SvelteKit
* TypeScript
* Rust
* Tesseract OCR

## Development setup

Requirements:

* Node.js
* Rust
* Tauri prerequisites
* Tesseract OCR

Install dependencies:

```powershell
npm install
```

Run the app in development mode:

```powershell
npm run tauri dev
```

Check Tesseract:

```powershell
tesseract --version
```

If Tesseract is installed but not found, make sure its install folder is available in PATH. On Windows, the common path is:

```text
C:\Program Files\Tesseract-OCR\tesseract.exe
```

## Signature data

Signature data lives in:

```text
src/lib/data/signatures.json
```

The data uses a root array of signature groups:

```json
[
  {
    "materialName": "Torite",
    "category": "Mineable",
    "signatures": [
      { "rockCount": 1, "value": 3900 },
      { "rockCount": 2, "value": 7800 },
      { "rockCount": 3, "value": 11700 },
      { "rockCount": 4, "value": 15600 },
      { "rockCount": 5, "value": 19500 }
    ],
    "notes": ""
  }
]
```

The number of values defines the valid rock/signature counts for that entry. Siglock does not generate extra counts or infer missing values.

## Notes

Siglock is designed to be local, lightweight, and focused. It does not use accounts, cloud sync, telemetry, or external databases.

Star Citizen is a trademark of Cloud Imperium Games. This project is unofficial and not affiliated with or endorsed by Cloud Imperium Games.
