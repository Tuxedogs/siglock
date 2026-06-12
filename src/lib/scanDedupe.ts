export const DUPLICATE_WINDOW_MS = 3000;

type MatchIdentity = {
  material: string;
  rockCount: number;
};

export type LastAcceptedResult = {
  key: string;
  acceptedAt: number;
};

export function normalizeMaterial(material: string): string {
  return material.trim().toLowerCase().replace(/\s+/g, ' ');
}

export function buildScanResultKey(matches: MatchIdentity[], normalizedSignature: string): string {
  const identities = matches
    .map((match) => `${normalizeMaterial(match.material)}|${match.rockCount}`)
    .sort()
    .join(',');
  return `matched|${identities}|${normalizedSignature.replace(/\D/g, '')}`;
}

export function isDuplicateResult(lastAccepted: LastAcceptedResult | null, key: string, now: number): boolean {
  return !!lastAccepted
    && lastAccepted.key === key
    && now - lastAccepted.acceptedAt < DUPLICATE_WINDOW_MS;
}
