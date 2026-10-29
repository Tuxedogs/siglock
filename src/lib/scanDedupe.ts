// OCR can report the same stationary readout every second. Treat a sustained read
// as one detection event until it has been stable for a short, human-scale window.
export const DUPLICATE_WINDOW_MS = 15_000;

type MatchIdentity = {
  material: string;
  rockCount: number;
};

export type LastAcceptedResult = {
  key: string;
  acceptedAt: number;
};

export function normalizeMaterial(material: string): string {
  const normalized = material.trim().toLowerCase().replace(/\s+/g, ' ');
  return normalized === 'savrillium' ? 'savrilium' : normalized;
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
