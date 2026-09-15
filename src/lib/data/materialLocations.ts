import rawMaterialLocations from './material-locations.json';
import { canonicalMaterialKey } from './rockCompositions';

export type SigLockLocation =
  | 'daymar' | 'yela' | 'cellin' | 'aberdeen' | 'lyria' | 'wala'
  | 'calliope' | 'clio' | 'euterpe' | 'ariel' | 'magda' | 'ita';

export type MinableReference = {
  material: string;
  locations: SigLockLocation[];
};

const aliases: Record<string, string> = {
  aluminium: 'aluminum',
  heph: 'hephaestanite',
  ice: 'rawice',
};

const rows: MinableReference[] = Object.entries(rawMaterialLocations.locations).map(([material, locations]) => ({
  material,
  locations: locations as SigLockLocation[],
}));

const locationsByMaterial = new Map(
  rows.map((row) => [canonicalLocationMaterialKey(row.material), row.locations]),
);

function canonicalLocationMaterialKey(material: string) {
  const key = canonicalMaterialKey(material);
  return aliases[key] ?? key;
}

export function locationsForMaterial(material: string): SigLockLocation[] {
  return locationsByMaterial.get(canonicalLocationMaterialKey(material)) ?? [];
}

export function materialValidAtLocation(material: string, location?: string | null): boolean {
  if (!location) return true;
  const locations = locationsForMaterial(material);
  return locations.includes(location.toLowerCase() as SigLockLocation);
}

export function getMinableReference(): MinableReference[] {
  return rows;
}

export function locationLabel(location: string) {
  const labels: Record<string, string> = { 'cru-l1': 'CRU-L1' };
  return labels[location] ?? location.charAt(0).toUpperCase() + location.slice(1);
}
