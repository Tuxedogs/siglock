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

assert.equal(compositions.schemaVersion, 2);
assert.match(compositions.source, /Scintel normalized mining contract/);
assert.equal(compositions.profiles.length, signatures.filter((entry) => entry.category === 'Mineable').length);

const aslariteProfile = compositions.profiles.find((profile) => profile.primaryMaterial === 'Aslarite');
assert.equal(aslariteProfile.sourceMaterial, 'Aslarite');
assert.deepEqual(aslariteProfile.systems, ['Pyro', 'Stanton']);
assert.deepEqual(aslariteProfile.traces.map((trace) => ({
  material: trace.material,
  percentRange: trace.percentRange,
})), [
  { material: 'Agricium', percentRange: [2, 5] },
  { material: 'Titanium', percentRange: [2, 5] },
]);
assert.ok(!aslariteProfile.traces.some((trace) => trace.material === 'Aslarite'));
assert.ok(!aslariteProfile.traces.some((trace) => trace.material === 'Laranite'));

const agriciumProfile = compositions.profiles.find((profile) => profile.primaryMaterial === 'Agricium');
assert.deepEqual(agriciumProfile.traces.map((trace) => trace.material), ['Aslarite', 'Titanium']);

const iceProfile = compositions.profiles.find((profile) => profile.primaryMaterial === 'Ice');
assert.equal(iceProfile.sourceMaterial, 'Raw Ice');
assert.deepEqual(iceProfile.traces, []);

const savrilium = signatures.find((entry) => entry.materialName === 'Savrilium');
assert.ok(savrilium, 'Savrilium must use the user-facing spelling in the canonical signature inventory');
assert.equal(savrilium.signatures[0].value, 3200);
const savriliumProfile = compositions.profiles.find((profile) => profile.primaryMaterial === 'Savrilium');
assert.equal(savriliumProfile?.sourceMaterial, 'Savrilium');

console.log('Scintel trace-material composition acceptance checks passed.');
