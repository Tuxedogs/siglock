import rawCompositions from './rock-compositions.json';
import type { MatchResult } from './signatures';
import type { SystemFilter } from '$lib/settings';

export type CompositionStatus = 'resolved' | 'ambiguous-variant' | 'unavailable';

export type RockCompositionRow = {
  material: string;
  materialId: string;
  densityRange: [number, number];
  qualityRange: [number, number];
  qualityScale: number;
};

export type RockCompositionVariant = {
  id: string;
  primaryMaterial: string;
  systems: string[];
  mineableKinds: string[];
  sourceCompositionGuids: string[];
  compositionRows: RockCompositionRow[];
};

export type ResolvedComposition = {
  compositionStatus: CompositionStatus;
  compositionRows: RockCompositionRow[];
  secondaryMaterials: string[];
};

export type ScanResult = ResolvedComposition & {
  primaryMatch: MatchResult | null;
  otherCandidates: MatchResult[];
};

const aliases: Record<string, string> = {
  aluminum: 'aluminium',
  heph: 'hephaestanite',
  quantainium: 'quantanium',
  savrilium: 'savrillium',
};

export function canonicalMaterialKey(value: string): string {
  const key = value.trim().toLowerCase().replace(/[^a-z0-9]+/g, '');
  return aliases[key] ?? key;
}

const variants = (rawCompositions.variants as unknown as RockCompositionVariant[]);

export function systemsForMaterial(material: string): string[] {
  const primaryKey = canonicalMaterialKey(material);
  return [...new Set(variants
    .filter((variant) => canonicalMaterialKey(variant.primaryMaterial) === primaryKey)
    .flatMap((variant) => variant.systems))]
    .sort();
}

export function resolveRockComposition(
  primaryMaterial: string,
  system: SystemFilter = 'All'
): ResolvedComposition {
  const primaryKey = canonicalMaterialKey(primaryMaterial);
  let candidates = variants.filter((variant) => canonicalMaterialKey(variant.primaryMaterial) === primaryKey);

  if (system !== 'All') {
    candidates = candidates.filter((variant) => variant.systems.includes(system));
  }

  if (candidates.length === 0) {
    return { compositionStatus: 'unavailable', compositionRows: [], secondaryMaterials: [] };
  }

  if (candidates.length !== 1) {
    return { compositionStatus: 'ambiguous-variant', compositionRows: [], secondaryMaterials: [] };
  }

  const compositionRows = candidates[0].compositionRows;
  const secondaryMaterials = [...new Set(compositionRows
    .filter((row) => canonicalMaterialKey(row.material) !== primaryKey)
    .map((row) => row.material))];

  return { compositionStatus: 'resolved', compositionRows, secondaryMaterials };
}

export function resolveScanResult(
  matches: MatchResult[],
  system: SystemFilter = 'All'
): ScanResult {
  const primaryMatch = matches[0] ?? null;
  if (!primaryMatch) {
    return {
      primaryMatch: null,
      otherCandidates: [],
      compositionStatus: 'unavailable',
      compositionRows: [],
      secondaryMaterials: [],
    };
  }

  return {
    primaryMatch,
    otherCandidates: matches.slice(1),
    ...resolveRockComposition(primaryMatch.material, system),
  };
}
