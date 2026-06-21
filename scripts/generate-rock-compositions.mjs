import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';

const projectRoot = resolve(import.meta.dirname, '..');
const sourcePath = resolve(process.argv[2] ?? 'D:/scintel/api/mining/mineables.json');
const signaturesPath = resolve(projectRoot, 'src/lib/data/signatures.json');
const outputPath = resolve(projectRoot, 'src/lib/data/rock-compositions.json');

const aliases = {
  Heph: ['Heph', 'Hephaestanite'],
  Ice: ['Ice', 'PressurizedIce'],
  Quantanium: ['Quantanium', 'Quantainium'],
  Savrillium: ['Savrillium', 'Savrilium'],
};

const canonicalNames = new Map([
  ['aluminum', 'Aluminium'],
  ['quantainium', 'Quantanium'],
  ['savrilium', 'Savrillium'],
]);

const compact = (value) => String(value ?? '').toLowerCase().replace(/[^a-z0-9]+/g, '');
const canonicalName = (value) => canonicalNames.get(compact(value)) ?? String(value);
const qualityRange = (scale) => [Math.floor(501 * scale), Math.floor(1000 * scale)];

const signatures = JSON.parse(await readFile(signaturesPath, 'utf8'));
const mineables = JSON.parse(await readFile(sourcePath, 'utf8'));
const variants = [];

for (const signature of signatures.filter((entry) => entry.category === 'Mineable')) {
  const primaryMaterial = signature.materialName;
  const primaryAliases = aliases[primaryMaterial] ?? [primaryMaterial];
  const matchingMineables = mineables.filter((mineable) => {
    if (/test/i.test(mineable.entityRecordName ?? '')) return false;
    const recordKey = compact(mineable.entityRecordName);
    return primaryAliases.some((alias) => recordKey.endsWith(compact(alias)));
  });

  const variantsByRows = new Map();
  for (const mineable of matchingMineables) {
    if (!Array.isArray(mineable.materials) || mineable.materials.length === 0) continue;
    const compositionRows = mineable.materials.map((row) => ({
      material: canonicalName(row.materialName),
      materialId: row.materialId,
      densityRange: [row.minPercentage, row.maxPercentage],
      qualityRange: qualityRange(row.qualityScale),
      qualityScale: row.qualityScale,
    }));
    const rowsKey = JSON.stringify(compositionRows);
    const current = variantsByRows.get(rowsKey) ?? {
      primaryMaterial,
      systems: new Set(),
      mineableKinds: new Set(),
      sourceCompositionGuids: new Set(),
      compositionRows,
    };
    for (const spawn of mineable.spawns ?? []) {
      if (spawn.system) current.systems.add(spawn.system);
    }
    if (mineable.mineableKind) current.mineableKinds.add(mineable.mineableKind);
    if (mineable.compositionGuid) current.sourceCompositionGuids.add(mineable.compositionGuid);
    variantsByRows.set(rowsKey, current);
  }

  for (const variant of variantsByRows.values()) {
    const hash = createHash('sha256')
      .update(`${primaryMaterial}:${JSON.stringify(variant.compositionRows)}`)
      .digest('hex')
      .slice(0, 12);
    variants.push({
      id: `${compact(primaryMaterial)}-${hash}`,
      primaryMaterial,
      systems: [...variant.systems].sort(),
      mineableKinds: [...variant.mineableKinds].sort(),
      sourceCompositionGuids: [...variant.sourceCompositionGuids].sort(),
      compositionRows: variant.compositionRows,
    });
  }
}

variants.sort((a, b) => a.primaryMaterial.localeCompare(b.primaryMaterial) || a.id.localeCompare(b.id));

const output = {
  schemaVersion: 1,
  source: 'Scintel DCB extraction: api/mining/mineables.json',
  variants,
};

await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');
console.log(`Wrote ${variants.length} normalized composition variants to ${outputPath}`);
