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

const aliases = { Aluminium: 'Aluminum', Heph: 'Hephaestanite', Ice: 'Raw Ice' };
const withoutPublishedLocations = names.filter((name) => {
  const locationKey = aliases[name] ?? name;
  return !(locationKey in locations.locations);
});
assert.ok(withoutPublishedLocations.includes('Savrilium'), 'Savrilium currently has no separate location row and must still remain visible');

console.log(`Canonical material coverage checks passed (${names.length} recognition profiles; ${withoutPublishedLocations.length} without location rows).`);
