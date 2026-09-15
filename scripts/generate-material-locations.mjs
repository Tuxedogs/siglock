import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';

const projectRoot = resolve(import.meta.dirname, '..');
const sourcePath = resolve(process.argv[2] ?? 'D:/scintel/api/mining/material_sources.json');
const signaturesPath = resolve(projectRoot, 'src/lib/data/signatures.json');
const outputPath = resolve(projectRoot, 'src/lib/data/material-locations.json');

const aliases = new Map([
  ['aluminium', 'aluminum'],
  ['heph', 'hephaestanite'],
  ['ice', 'rawice'],
  ['savrillium', 'savrilium'],
]);

const stantonNames = {
  stanton1a: 'Ariel', stanton1b: 'Aberdeen', stanton1c: 'Magda', stanton1d: 'Ita',
  stanton2a: 'Cellin', stanton2b: 'Daymar', stanton2c: 'Yela',
  stanton3a: 'Wala', stanton3b: 'Lyria',
  stanton4a: 'Calliope', stanton4b: 'Clio', stanton4c: 'Euterpe',
};

const keyFor = (value) => aliases.get(String(value ?? '').toLowerCase().replace(/[^a-z0-9]+/g, ''))
  ?? String(value ?? '').toLowerCase().replace(/[^a-z0-9]+/g, '');

const labelFor = (source) => {
  const raw = source.resolvedLocationName ?? source.location;
  return stantonNames[keyFor(raw)] ?? raw;
};

const [signatures, materialSources] = await Promise.all([
  readFile(signaturesPath, 'utf8').then(JSON.parse),
  readFile(sourcePath, 'utf8').then(JSON.parse),
]);
const sourceByMaterial = new Map(materialSources.map((entry) => [keyFor(entry.materialName), entry]));
const locations = {};

for (const signature of signatures.filter((entry) => entry.category === 'Mineable')) {
  const source = sourceByMaterial.get(keyFor(signature.materialName));
  if (!source) throw new Error(`Scintel has no mining source row for ${signature.materialName}`);
  const values = [...new Set((source.sources ?? []).map(labelFor).filter(Boolean))]
    .sort((a, b) => a.localeCompare(b));
  if (!values.length) throw new Error(`Scintel published no availability locations for ${signature.materialName}`);
  locations[signature.materialName] = values;
}

await writeFile(outputPath, `${JSON.stringify({
  schemaVersion: 2,
  source: 'Scintel accepted material_sources.json resolved availability locations',
  locations,
}, null, 2)}\n`, 'utf8');
console.log(`Wrote ${Object.keys(locations).length} material availability rows to ${outputPath}`);
