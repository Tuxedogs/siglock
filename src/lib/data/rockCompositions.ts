import rawCompositions from './rock-compositions.json';
import type { MatchResult } from './signatures';

export type CompositionStatus = 'resolved' | 'unavailable';

type RawTrace = {
  materialId: string;
  material: string;
  percentRange: number[];
};

type RawTraceProfile = {
  primaryMaterial: string;
  primaryMaterialId: string;
  systems: string[];
  traces: RawTrace[];
};

export type CompositionEntry = {
  materialId: string;
  displayName: string;
  percentMin: number;
  percentMax: number;
};

export type MaterialCompositionProfile = {
  primaryMaterialId: string;
  primaryDisplayName: string;
  entries: CompositionEntry[];
};

export type ResolvedComposition = {
  compositionStatus: CompositionStatus;
  compositionProfile: MaterialCompositionProfile | null;
};

export type ScanResult = ResolvedComposition & {
  primaryMatch: MatchResult | null;
  otherCandidates: MatchResult[];
};

const aliases: Record<string, string> = {
  aluminum: 'aluminium',
  hephaestanite: 'heph',
  pressurizedice: 'ice',
  quantainium: 'quantanium',
  rawice: 'ice',
  savrillium: 'savrilium',
};

export function canonicalMaterialKey(value: string): string {
  const key = value.trim().toLowerCase().replace(/[^a-z0-9]+/g, '');
  return aliases[key] ?? key;
}

const profiles = new Map<string, MaterialCompositionProfile>();
const systemsByMaterial = new Map<string, string[]>();
for (const profile of rawCompositions.profiles as unknown as RawTraceProfile[]) {
  const materialKey = canonicalMaterialKey(profile.primaryMaterial);
  profiles.set(materialKey, {
    primaryMaterialId: profile.primaryMaterialId,
    primaryDisplayName: profile.primaryMaterial,
    entries: profile.traces.map((trace) => ({
      materialId: trace.materialId,
      displayName: trace.material,
      percentMin: trace.percentRange[0],
      percentMax: trace.percentRange[1],
    })),
  });
  systemsByMaterial.set(materialKey, profile.systems);
}

export function systemsForMaterial(material: string): string[] {
  return systemsByMaterial.get(canonicalMaterialKey(material)) ?? [];
}

export function resolveRockComposition(primaryMaterial: string): ResolvedComposition {
  const profile = profiles.get(canonicalMaterialKey(primaryMaterial)) ?? null;
  return profile
    ? { compositionStatus: 'resolved', compositionProfile: profile }
    : { compositionStatus: 'unavailable', compositionProfile: null };
}

export function resolveScanResult(matches: MatchResult[]): ScanResult {
  const primaryMatch = matches[0] ?? null;
  if (!primaryMatch) {
    return {
      primaryMatch: null,
      otherCandidates: [],
      compositionStatus: 'unavailable',
      compositionProfile: null,
    };
  }

  return {
    primaryMatch,
    otherCandidates: matches.slice(1),
    ...resolveRockComposition(primaryMatch.material),
  };
}
