import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const read = (path) => readFile(new URL(path, import.meta.url), 'utf8');
const [signatures, page, locations] = await Promise.all([
  read('../src/lib/data/signatures.json').then(JSON.parse),
  read('../src/routes/+page.svelte'),
  read('../src/lib/data/material-locations.json').then(JSON.parse),
]);

const names = signatures.map((entry) => entry.materialName);
assert.equal(new Set(names.map((name) => name.toLowerCase())).size, names.length, 'Canonical signature names must be unique');
assert.ok(signatures.every((entry) => entry.signatures.some((signature) => signature.value != null)), 'Every exposed material must be solvable');
assert.match(page, /getSignatures\(\)\.materials/, 'Minables / Materials must enumerate the recognition inventory');
assert.match(page, /toggleWatch\(row\.material\)/, 'Every canonical row must support watch state');
assert.ok(names.includes('Savrilium'));

const mineableNames = signatures.filter((entry) => entry.category === 'Mineable').map((entry) => entry.materialName);
const withoutPublishedLocations = mineableNames.filter((name) => !(name in locations.locations));
assert.deepEqual(withoutPublishedLocations, [], 'Every recognized mineable must retain its canonical availability row');
assert.ok(locations.locations.Savrilium.includes('GlaciemRing'), 'Savrilium must retain its published Nyx availability');
assert.ok(locations.locations.Bexalite.includes('CRU-L1'), 'Bexalite must retain its published Stanton availability');

console.log(`Canonical material coverage checks passed (${names.length} recognition profiles; ${withoutPublishedLocations.length} without location rows).`);
