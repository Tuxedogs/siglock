import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const read = (path) => readFile(new URL(path, import.meta.url), 'utf8');

const [page, overlay, picker, settings, matcher, rust] = await Promise.all([
  read('../src/routes/+page.svelte'),
  read('../src/routes/overlay/+page.svelte'),
  read('../src/routes/region-picker/+page.svelte'),
  read('../src/lib/settings.ts'),
  read('../src/lib/data/signatures.ts'),
  read('../src-tauri/src/lib.rs'),
]);

for (const label of [
  'Salvage',
  'FPS/ROC',
  'Show Composition',
  'Signature Value',
  'Only solved captures in overlay',
]) {
  assert.ok(page.includes(`</span> ${label}</label>`), `Missing settings label: ${label}`);
}

for (const oldLabel of [
  'Return Salvage Results',
  'Show Secondary Materials',
  'Show scanned value on overlay',
  'Only Show Solved Results',
]) {
  assert.ok(!page.includes(oldLabel), `Legacy settings label remains: ${oldLabel}`);
}

assert.match(settings, /includeFpsRocResults:\s*true/);
assert.match(settings, /typeof raw\.includeFpsRocResults === 'boolean'/);
assert.match(settings, /watchedMaterials:\s*\[\]/);
assert.match(settings, /Array\.isArray\(raw\.watchedMaterials\)/);
assert.match(settings, /new Set\(raw\.watchedMaterials/);
assert.match(matcher, /includeFpsRoc\?: boolean/);
assert.match(matcher, /\['fps', 'vehicle'\]\.includes/);

assert.ok(!page.includes('capturePreviewTimer'));
assert.ok(!page.includes('startCapturePreview'));
assert.ok(!page.includes('setInterval('));
assert.match(page, /function openSettingsPane\(\)[\s\S]*?refreshCapturePreview\(\)/);
assert.match(page, /crop-region-updated[\s\S]*?currentPage === 'regions' \|\| currentPage === 'settings'/);
assert.match(page, /<aside class="side-rail">/);
assert.match(page, /currentPage === 'settings'[\s\S]*?class="workspace-page settings-page"/);
assert.ok(!page.includes('settings-backdrop'));
assert.ok(!page.includes('class="app-nav"'));
assert.ok(!page.includes('class="status-strip"'));
assert.match(page, /<h1>Minables \/ Materials<\/h1>/);
assert.match(page, /getSignatures\(\)\.materials[\s\S]*?locationsForMaterial/);
assert.ok(!page.includes("currentPage === 'materials'"), 'Material list must have one canonical UI destination');
assert.ok(!/\['watch'/.test(page), 'A separate Watch destination must not return');
assert.match(settings, /overlayOpacity:\s*0,/);
assert.match(settings, /overlayOpacity, DEFAULT_SETTINGS\.overlayOpacity, 0, 1/);

assert.ok(!rust.includes('[Capture] Saved raw crop to:'));
assert.ok(!rust.includes('last_capture.png'));
assert.ok(!rust.includes('last_preprocessed.png'));
assert.match(rust, /async fn capture_region_preview[\s\S]*?Cursor::new[\s\S]*?image_path: None/);
assert.match(rust, /set_size\(Size::Physical\(monitor_size\)\)/);
assert.match(rust, /let monitor = app\.primary_monitor\(\)/);
assert.ok(!/open_region_picker[\s\S]*?current_monitor/.test(rust), 'Region picker must not follow the main window monitor');
assert.match(rust, /capture_area_ignore_area_check/);
assert.match(rust, /region\.x - display_x/);
assert.ok(!rust.includes('with_extension("json.tmp")'));
assert.match(rust, /OpenOptions::new\(\)[\s\S]*?truncate\(true\)[\s\S]*?sync_all\(\)/);
assert.match(picker, /innerPosition\(\)/);
assert.match(picker, /scaleFactor\(\)/);
assert.match(picker, /Math\.round\(x \* pickerScaleFactor\)/);
assert.match(page, /function toggleWatch\(material: string\)/);
assert.match(page, /create_region/);
assert.match(page, /delete_region/);
assert.match(rust, /Built-in region presets cannot be deleted/);
assert.match(page, /watched: isWatched\(primary\.material\)/);
assert.match(overlay, /<strong>SIGLOCK<\/strong>/);
assert.match(overlay, /class:pulsing=\{match\.detected\} class="watch-dot"/);
assert.match(overlay, /@keyframes watch-pulse/);
assert.match(overlay, /setupMode \|\| activeScanOn \|\| displayedMatches\.length/);
assert.ok(!overlay.includes('class:pulsing={match.detected} class="match-item"'));
assert.ok(!overlay.includes('Current Location'));
const activeScanCommand = rust.match(/async fn toggle_active_scan\([\s\S]*?\n\}/)?.[0] ?? '';
assert.ok(activeScanCommand, 'Missing active scan command');
assert.ok(!/\.hide\(\)/.test(activeScanCommand), 'Auto Scan must not hide the HUD overlay');
assert.match(activeScanCommand, /window\.show\(\)/, 'Enabling Auto Scan must show the HUD overlay');

console.log('Settings and capture preview acceptance checks passed.');
