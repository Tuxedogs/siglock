import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const read = (path) => readFile(new URL(path, import.meta.url), 'utf8');

const [page, picker, settings, matcher, rust] = await Promise.all([
  read('../src/routes/+page.svelte'),
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
assert.match(matcher, /includeFpsRoc\?: boolean/);
assert.match(matcher, /\['fps', 'vehicle'\]\.includes/);

assert.ok(!page.includes('capturePreviewTimer'));
assert.ok(!page.includes('startCapturePreview'));
assert.ok(!page.includes('setInterval('));
assert.match(page, /function openSettingsPane\(\)[\s\S]*?refreshCapturePreview\(\)/);
assert.match(page, /crop-region-updated[\s\S]*?settingsOpen\) void refreshCapturePreview\(\)/);

assert.ok(!rust.includes('[Capture] Saved raw crop to:'));
assert.ok(!rust.includes('last_capture.png'));
assert.ok(!rust.includes('last_preprocessed.png'));
assert.match(rust, /async fn capture_region_preview[\s\S]*?Cursor::new[\s\S]*?image_path: None/);
assert.match(rust, /set_size\(Size::Physical\(monitor_size\)\)/);
assert.match(rust, /capture_area_ignore_area_check/);
assert.match(rust, /region\.x - display_x/);
assert.ok(!rust.includes('with_extension("json.tmp")'));
assert.match(rust, /OpenOptions::new\(\)[\s\S]*?truncate\(true\)[\s\S]*?sync_all\(\)/);
assert.match(picker, /innerPosition\(\)/);
assert.match(picker, /scaleFactor\(\)/);
assert.match(picker, /Math\.round\(x \* pickerScaleFactor\)/);

console.log('Settings and capture preview acceptance checks passed.');
