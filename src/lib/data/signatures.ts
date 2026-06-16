/**
 * SigLock Signature Data Loader + Matcher
 *
 * SINGLE SOURCE OF TRUTH: src/lib/data/signatures.json
 *
 * UI components must NEVER hardcode mining values.
 * All matching logic lives here.
 */

import rawSignatures from './signatures.json';

export interface SignatureEntry {
  rockCount: number;
  value: number | null;
}

export interface MaterialSignature {
  materialName: string;
  category?: string;
  signatures: SignatureEntry[];
  notes?: string;
}

export interface SignatureData {
  version?: string;
  description?: string;
  materials: MaterialSignature[];
}

export interface MatchResult {
  material: string;
  rockCount: number;
  expected: number;
  observed: number;
  delta: number;
  matchType: 'exact' | 'near';
  confidence: number; // 0-1
}

// Normalize to support both root array (current) and legacy { materials: [...] } shape.
// This keeps the loader flexible without changing the JSON file.
const raw: any = rawSignatures;

const materials: MaterialSignature[] = Array.isArray(raw)
  ? raw
  : Array.isArray(raw?.materials)
    ? raw.materials
    : [];

if (!materials.length) {
  console.error('[SigLock] Invalid signatures.json shape — expected root array or { materials: [...] }', raw);
}

/**
 * Returns the signature data (synchronous, single source).
 * For compatibility, wraps the normalized materials array.
 */
export function getSignatures(): SignatureData {
  return {
    materials,
  };
}

/**
 * Core matcher (synchronous).
 * - Exact matches (delta === 0) rank first.
 * - Near matches within ±25 absolute tolerance.
 * - Null / missing values in JSON are ignored.
 */
export function findMatches(
  observed: number,
  tolerance: number = 25
): MatchResult[] {
  if (!observed || observed < 100) return [];

  const results: MatchResult[] = [];

  for (const mat of materials) {
    for (const entry of mat.signatures) {
      if (entry.value === null || entry.value === undefined) continue;

      const expected = entry.value;
      const delta = observed - expected;
      const absDelta = Math.abs(delta);

      if (absDelta === 0 || absDelta <= tolerance) {
        const matchType: 'exact' | 'near' = absDelta === 0 ? 'exact' : 'near';

        const confidence = absDelta === 0
          ? 1.0
          : Math.max(0.1, 1 - (absDelta / (tolerance * 2)));

        results.push({
          material: mat.materialName,
          rockCount: entry.rockCount,
          expected,
          observed,
          delta,
          matchType,
          confidence: Math.round(confidence * 100) / 100,
        });
      }
    }
  }

  // Exact first, then smallest absolute delta
  results.sort((a, b) => {
    if (a.matchType === 'exact' && b.matchType !== 'exact') return -1;
    if (b.matchType === 'exact' && a.matchType !== 'exact') return 1;
    return Math.abs(a.delta) - Math.abs(b.delta);
  });

  return results;
}

/**
 * Convenience wrapper for manual input (sync).
 */
export function matchObservedValue(
  observed: number,
  tolerance = 25
): MatchResult[] {
  return findMatches(observed, tolerance);
}

export function findNearestSignature(observed: number): MatchResult | null {
  if (!observed || observed < 100) return null;

  let nearest: MatchResult | null = null;
  for (const mat of materials) {
    for (const entry of mat.signatures) {
      if (entry.value === null || entry.value === undefined) continue;

      const expected = entry.value;
      const delta = observed - expected;
      const candidate: MatchResult = {
        material: mat.materialName,
        rockCount: entry.rockCount,
        expected,
        observed,
        delta,
        matchType: delta === 0 ? 'exact' : 'near',
        confidence: 0,
      };

      if (!nearest || Math.abs(candidate.delta) < Math.abs(nearest.delta)) {
        nearest = candidate;
      }
    }
  }

  return nearest;
}
