import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';

const projectRoot = resolve(import.meta.dirname, '..');
const sourcePath = resolve(process.argv[2] ?? 'D:/scintel/api/mining/material_sources.json');
const signaturesPath = resolve(projectRoot, 'src/lib/data/signatures.json');
const outputPath = resolve(projectRoot, 'src/lib/data/rock-compositions.json');

const sourceAliases = new Map([
  ['aluminium', 'aluminum'],
  ['heph', 'hephaestanite'],
  ['ice', 'rawice'],
  ['savrillium', 'savrilium'],
]);

const compact = (value) => String(value ?? '').toLowerCase().replace(/[^a-z0-9]+/g, '');
const sourceKey = (value) => sourceAliases.get(compact(value)) ?? compact(value);

const signatures = JSON.parse(await readFile(signaturesPath, 'utf8'));
const materialSources = JSON.parse(await readFile(sourcePath, 'utf8'));
const sourcesByMaterial = new Map(materialSources.map((entry) => [sourceKey(entry.materialName), entry]));
const profiles = [];

for (const signature of signatures.filter((entry) => entry.category === 'Mineable')) {
  const sourceMaterial = sourcesByMaterial.get(sourceKey(signature.materialName));
  if (!sourceMaterial) {
    throw new Error(`Scintel has no mining source row for ${signature.materialName}`);
  }

  const traceProfiles = new Map();
  for (const source of sourceMaterial.sources ?? []) {
    const traces = (source.traceMaterialDetails ?? []).map((trace) => ({
      materialId: trace.materialId,
      material: trace.materialName,
      percentRange: [trace.minPercentage, trace.maxPercentage],
    }));
    traceProfiles.set(JSON.stringify(traces), traces);
  }

  if (traceProfiles.size > 1) {
    throw new Error(`${sourceMaterial.materialName} has ${traceProfiles.size} conflicting trace profiles`);
  }

  profiles.push({
    primaryMaterial: signature.materialName,
    sourceMaterial: sourceMaterial.materialName,
    primaryMaterialId: sourceMaterial.primaryMaterialId ?? sourceMaterial.materialId,
    systems: [...new Set((sourceMaterial.sources ?? []).map((source) => source.system).filter(Boolean))].sort(),
    traces: traceProfiles.values().next().value ?? [],
  });
}

profiles.sort((a, b) => a.primaryMaterial.localeCompare(b.primaryMaterial));

const output = {
  schemaVersion: 2,
  source: 'Scintel normalized mining contract: api/mining/material_sources.json',
  profiles,
};

await writeFile(outputPath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');
console.log(`Wrote ${profiles.length} Scintel trace-material profiles to ${outputPath}`);
