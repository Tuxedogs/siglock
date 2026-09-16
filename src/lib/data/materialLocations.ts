import rawMaterialLocations from './material-locations.json';
import { canonicalMaterialKey } from './rockCompositions';

export type SigLockLocation = string;

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
  return locations.some((candidate) => canonicalLocationKey(candidate) === canonicalLocationKey(location));
}

export function getMinableReference(): MinableReference[] {
  return rows;
}

export function locationLabel(location: string) {
  return location;
}

function canonicalLocationKey(location: string) {
  return location.trim().toLowerCase().replace(/[^a-z0-9]+/g, '');
}
