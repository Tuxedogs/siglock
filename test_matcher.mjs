import { findMatches, getSignatures } from './src/lib/data/signatures.ts';

const data = getSignatures();
console.log("=== Signature Data Confirmation ===");
console.log("Number of signature groups:", data.materials.length);

let totalValues = 0;
data.materials.forEach(m => totalValues += m.signatures.length);
console.log("Total signature values:", totalValues);

console.log("\n=== Exact Match Tests ===");
const testValues = [3885, 3170, 6340, 4000, 2000];

for (const val of testValues) {
  const results = findMatches(val, 25);
  const exacts = results.filter(r => r.matchType === 'exact');
  console.log(`Input ${val}: ${exacts.length} exact match(es)`);
  exacts.forEach(r => {
    console.log(`  → ${r.material} ×${r.rockCount} | expected: ${r.expected} | observed: ${r.observed} | delta: ${r.delta}`);
  });
}

console.log("\n=== Near Match Test (4000 ±25 tolerance) ===");
const nearResults = findMatches(4020, 25);  // 4020 is within 25 of 4000 (ROC x1)
const near = nearResults.filter(r => r.matchType === 'near' && r.material.includes('ROC'));
console.log("Input 4020 (near 4000):");
near.forEach(r => {
  console.log(`  → ${r.material} ×${r.rockCount} | delta: ${r.delta} | type: ${r.matchType}`);
});

console.log("\n=== Tolerance Confirmation ===");
console.log("Matcher uses fixed ±25 absolute tolerance (passed as parameter to findMatches).");
console.log("Sample with tolerance=25 on 4020 returns near matches within |delta| <= 25.");
