import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const read = (path) => readFile(new URL(path, import.meta.url), 'utf8');

const [page, overlay, picker, settings, matcher, rust, css] = await Promise.all([
  read('../src/routes/+page.svelte'),
  read('../src/routes/overlay/+page.svelte'),
  read('../src/routes/region-picker/+page.svelte'),
  read('../src/lib/settings.ts'),
  read('../src/lib/data/signatures.ts'),
  read('../src-tauri/src/lib.rs'),
  read('../src/app.css'),
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
assert.ok(!page.includes('capturePreviewUrl'), 'Dashboard must not retain a persistent capture-preview state');
assert.ok(!page.includes('capture_region_preview'), 'Dashboard must not request a persistent capture preview');
assert.match(page, /<aside class="side-rail">/);
assert.match(page, /currentPage === 'settings'[\s\S]*?class="workspace-page settings-page"/);
assert.ok(!page.includes('settings-backdrop'));
assert.ok(!page.includes('class="app-nav"'));
assert.ok(!page.includes('class="status-strip"'));
assert.match(page, /<h1>Minables<\/h1>/);
assert.ok(!page.includes('Minables / Materials'));
assert.match(page, /getSignatures\(\)\.materials[\s\S]*?locationsForMaterial/);
assert.ok(!page.includes("currentPage === 'materials'"), 'Material list must have one canonical UI destination');
assert.ok(!/\['watch'/.test(page), 'A separate Watch destination must not return');
assert.match(settings, /overlayOpacity:\s*0,/);
assert.match(settings, /overlayOpacity, DEFAULT_SETTINGS\.overlayOpacity, 0, 1/);

assert.ok(!rust.includes('[Capture] Saved raw crop to:'));
assert.ok(!rust.includes('last_capture.png'));
assert.ok(!rust.includes('last_preprocessed.png'));
assert.ok(!rust.includes('capture_region_preview'), 'Unused capture preview command must be removed');
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
assert.match(page, /minables-watchlist[\s\S]*?toggleWatch\(row\.material\)/);
assert.match(page, /create_region/);
assert.match(page, /delete_region/);
assert.match(page, /function beginNewRegion\(\)[\s\S]*?newRegionDraft = true/);
assert.ok(!page.includes('regionEditMode'), 'Region controls must not be gated behind an edit mode');
assert.ok(!page.includes('toggleRegionEditMode'), 'Obsolete region edit-mode handler must be removed');
assert.match(page, /class="region-row-actions">[\s\S]*?selectOrResetRegion/);
assert.match(page, /class="coordinate-readout"[\s\S]*?beginCoordinateEdit\(profile\)/);
assert.match(page, /class="coordinate-inline-editor"[\s\S]*?onblur=\{\(\) => void commitCoordinateEdit\(\)\}/);
assert.match(page, /function handleCoordinateKeydown[\s\S]*?event\.key === 'Enter'[\s\S]*?commitCoordinateEdit[\s\S]*?event\.key === 'Escape'[\s\S]*?cancelCoordinateEdit/);
assert.ok(!page.includes('Save coordinates'), 'Coordinate save action must not be permanently displayed');
assert.ok(!page.includes('region-editor'), 'Region-manager editor panel must not remain below the list');
assert.ok(!page.includes('coordinate-grid'), 'Coordinate editor must remain inline with its row');
assert.match(page, /function completeRegionAction\(\)[\s\S]*?deleteRegion[\s\S]*?clearRegion/);
assert.ok(!page.includes("'regions'"), 'Regions must not remain as a standalone dashboard destination');
assert.match(page, /loadDetectionHistory[\s\S]*?saveDetectionHistory/);
assert.match(page, /relativeTime\(entry\.timestamp\)/);
assert.ok(!page.includes('Redraw bounds'));
assert.ok(!page.includes('activeRegionProfile().name} region'));
assert.match(page, /currentPage === 'overlay'[\s\S]*?Overlay configuration/);
assert.ok(!/openSettingsSection === 'overlay'/.test(page), 'Overlay configuration must not remain in Settings');
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
assert.match(activeScanCommand, /overlay_override != Some\(false\)/, 'Manual HUD hide must override Auto Scan');
assert.match(rust, /overlay_visibility_override: Option<bool>/);
assert.match(rust, /overlay_visibility_override = Some\(target_visible\)/);
assert.match(css, /\.app-workspace \{ display:flex; flex-direction:column; min-width:0; height:100vh; overflow:hidden;/);
assert.match(css, /\.dashboard-grid \{ display:grid; flex:1; min-height:0;/);
assert.match(css, /\.dashboard-grid \{ display:grid; flex:1; min-height:0; grid-template-columns:repeat\(12,minmax\(0,1fr\)\); grid-template-rows:188px 142px minmax\(0,1fr\);/);
assert.match(css, /\.region-module \{ grid-column:1\/7; grid-row:1; \}\.scanner-module \{ grid-column:7\/13; grid-row:1; \}\.watch-module \{ grid-column:8\/13; grid-row:2; \}\.finds-module \{ grid-column:1\/8; grid-row:2\/4; \}\.overlay-dashboard \{ grid-column:8\/13; grid-row:3; \}/);
assert.ok(!css.includes('.dashboard-column'), 'Dashboard cards must share one coordinated grid rather than independent columns');
assert.match(css, /\.detection-history\.populated \{ max-height:220px; overflow:auto; \}/);
assert.ok(!css.includes('.capture-preview'), 'Persistent capture preview styling must be removed');
assert.match(rust, /Capture coordinates exceed the physical virtual-desktop range/);
assert.match(css, /\.data-table \{ flex:1; min-height:0; overflow:auto;/);

console.log('Settings and capture preview acceptance checks passed.');
