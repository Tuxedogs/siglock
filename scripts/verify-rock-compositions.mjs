import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const signatures = JSON.parse(await readFile(new URL('../src/lib/data/signatures.json', import.meta.url), 'utf8'));
const compositions = JSON.parse(await readFile(new URL('../src/lib/data/rock-compositions.json', import.meta.url), 'utf8'));

const aslarite = signatures.find((entry) => entry.materialName === 'Aslarite');
assert.deepEqual(aslarite.signatures.find((entry) => entry.value === 3840), { rockCount: 1, value: 3840 });

const signatureCandidates = signatures
  .flatMap((material) => material.signatures.map((signature) => ({ material: material.materialName, ...signature })))
  .filter((candidate) => Math.abs(candidate.value - 3840) <= 25)
  .sort((a, b) => Math.abs(a.value - 3840) - Math.abs(b.value - 3840));
assert.equal(signatureCandidates[0].material, 'Aslarite');
assert.deepEqual(signatureCandidates.slice(1).map((candidate) => candidate.material), ['Laranite', 'Titanium']);

const aslariteVariants = compositions.variants.filter((variant) => variant.primaryMaterial === 'Aslarite');
assert.equal(aslariteVariants.length, 1);
const compositionRows = aslariteVariants[0].compositionRows;
assert.equal(compositionRows.length, 4);
assert.equal(compositionRows.filter((row) => row.material === 'Aslarite').length, 2);

const secondaryMaterials = [...new Set(compositionRows
  .filter((row) => row.material !== 'Aslarite')
  .map((row) => row.material))];
assert.deepEqual(secondaryMaterials, ['Agricium', 'Titanium']);
assert.ok(!secondaryMaterials.includes('Laranite'));

assert.deepEqual(compositionRows.map((row) => ({
  material: row.material,
  densityRange: row.densityRange,
  qualityRange: row.qualityRange,
})), [
  { material: 'Aslarite', densityRange: [2.82, 6.82], qualityRange: [501, 1000] },
  { material: 'Aslarite', densityRange: [39.18, 83.18], qualityRange: [245, 490] },
  { material: 'Agricium', densityRange: [2, 5], qualityRange: [395, 789] },
  { material: 'Titanium', densityRange: [2, 5], qualityRange: [395, 789] },
]);

assert.equal(compositions.variants.filter((variant) => variant.primaryMaterial === 'Taranite').length, 1);
assert.equal(compositions.variants.filter((variant) => variant.primaryMaterial === 'No Such Material').length, 0);

console.log('Rock composition acceptance checks passed.');
