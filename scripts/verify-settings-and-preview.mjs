import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const read = (path) => readFile(new URL(path, import.meta.url), 'utf8');

const [page, settings, matcher, rust] = await Promise.all([
  read('../src/routes/+page.svelte'),
  read('../src/lib/settings.ts'),
  read('../src/lib/data/signatures.ts'),
  read('../src-tauri/src/lib.rs'),
]);

for (const label of [
  'Salvage',
  'FPS/ROC',
  'Composition',
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

console.log('Settings and capture preview acceptance checks passed.');
