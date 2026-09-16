import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const signatures = JSON.parse(await readFile(new URL('../src/lib/data/signatures.json', import.meta.url), 'utf8'));
const reference = JSON.parse(await readFile(new URL('../src/lib/data/material-locations.json', import.meta.url), 'utf8'));

const aliases = { Aluminium: 'Aluminum', Heph: 'Hephaestanite', Ice: 'Raw Ice' };
const locationKey = (value) => value.toLowerCase().replace(/[^a-z0-9]+/g, '');
const locationsFor = (material) => reference.locations[aliases[material] ?? material] ?? [];
const atLocation = (material, location) => locationsFor(material).some((candidate) => locationKey(candidate) === locationKey(location));
const candidates = (observed, location) => signatures
  .filter((material) => material.category !== 'Mineable' || !location || atLocation(material.materialName, location))
  .flatMap((material) => material.signatures
    .filter((entry) => entry.value != null && Math.abs(observed - entry.value) <= 25)
    .map((entry) => ({ material: material.materialName, delta: Math.abs(observed - entry.value) })))
  .sort((a, b) => a.delta - b.delta);

assert.equal(candidates(3185, null)[0]?.material, 'Stileron', 'unknown location preserves unrestricted exact matching');
assert.equal(candidates(3185, 'daymar')[0]?.material, 'Quantanium', 'Daymar filtering occurs before numeric ranking');
assert.ok(!candidates(3185, 'daymar').some((candidate) => candidate.material === 'Stileron'), 'out-of-location Stileron cannot win');
assert.ok(atLocation('Agricium', 'daymar'), 'known valid material is retained for Daymar');
assert.ok(atLocation('Bexalite', 'CRU-L1'), 'published Lagrange availability survives serialization');

console.log('Location-aware material candidate checks passed.');
