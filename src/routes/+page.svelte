<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { matchObservedValue, type MatchResult } from '$lib/data/signatures';

  // State
  let observed = $state("3885");   // Sample that hits exact in our tiny dataset
  let tolerance = $state(25);
  let matches = $state<MatchResult[]>([]);
  let lastScanTime = $state<string | null>(null);
  let lastScanSource = $state<"manual" | "ocr" | null>(null);

  // Region + OCR state (synced with Rust)
  let hasRegion = $state(false);
  let regionInfo = $state<string | null>(null);
  let activeScanOn = $state(false);
  let overlayVisible = $state(true);
  let ocrError = $state<string | null>(null);

  // Guard to prevent overlapping scans
  let isScanning = $state(false);

  // Capture preview state (from Capture Test button)
  let capturePreviewPath = $state<string | null>(null);
  let capturePreviewUrl = $state<string | null>(null);
  let lastCaptureInfo = $state<string | null>(null);

  // ==================== OCR Debug / Tuning State ====================
  let showDebug = $state(false);

  // Current tunable config (dev only)
  let ocrConfig = $state({
    upscale: 2,
    threshold_enabled: true,
    threshold: 200,
    grayscale: true,
    invert: false,
    sharpen: false,
    psm: 7,
    numeric_only: true,
  });

  // Latest debug info from last real scan
  let debugRawText = $state("");
  let debugNormalized = $state<number | null>(null);
  let debugError = $state<string | null>(null);
  let debugLastScan = $state<string | null>(null);
  let debugCaptureSize = $state<string | null>(null);

  // Paths to latest debug images (fixed names, overwritten each scan)
  let debugRawPath = $state<string | null>(null);
  let debugRawUrl = $state<string | null>(null);
  let debugPreprocessedPath = $state<string | null>(null);
  let debugPreprocessedUrl = $state<string | null>(null);

  // Tesseract status
  let tesseractStatus = $state<any>(null);

  // Use the real JSON-driven matcher (no more Rust formula)
  function runMatch() {
    const cleaned = observed.replace(/[^0-9]/g, "");
    const num = parseInt(cleaned, 10);

    if (!num || num < 100) {
      matches = [];
      return;
    }

    try {
      // This now calls the matcher in src/lib/data/signatures.ts
      // which loads ONLY from src/lib/data/signatures.json
      const result = matchObservedValue(num, tolerance);
      matches = result;
      lastScanTime = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
      lastScanSource = "manual";
    } catch (e) {
      console.error("match failed", e);
      matches = [];
    }
  }

  // Auto-match on input change (debounced feel via Svelte 5 runes)
  $effect(() => {
    const cleaned = observed.replace(/[^0-9]/g, "");
    const num = parseInt(cleaned, 10);
    if (num >= 100) {
      runMatch();
    } else {
      matches = [];
    }
  });

  function clearInput() {
    observed = "";
    matches = [];
  }

  function formatDelta(delta: number): string {
    if (delta === 0) return "0";
    return delta > 0 ? `+${delta}` : `${delta}`;
  }

  // ==================== Region + OCR (new foundation) ====================

  async function refreshRegionStatus() {
    try {
      const reg = await invoke<any | null>("get_crop_region");
      hasRegion = !!reg;
      if (reg) {
        regionInfo = `${reg.width}×${reg.height} @ (${reg.x}, ${reg.y})`;
      } else {
        regionInfo = null;
      }
    } catch (e) {
      hasRegion = false;
      regionInfo = null;
    }
  }

  async function setRegion() {
    console.log("Opening region picker...");
    ocrError = "Opening region picker...";
    try {
      await invoke("open_region_picker");
      console.log("Region picker opened");
      ocrError = "Region picker opened";  // Brief visible confirmation
      // Clear the message quickly so it doesn't linger
      setTimeout(() => {
        if (ocrError === "Region picker opened") ocrError = null;
      }, 1200);
      // Poll for region update after picker closes
      setTimeout(refreshRegionStatus, 800);
      setTimeout(refreshRegionStatus, 1800);
    } catch (e) {
      const msg = "Failed to open region picker: " + String(e);
      console.error(msg);
      ocrError = msg;
    }
  }

  async function clearRegion() {
    try {
      await invoke("clear_crop_region");
      await refreshRegionStatus();
    } catch (e) {
      console.error(e);
    }
  }

  // ==================== Shared Scan Result Processor (avoids duplication) ====================
  async function processOcrResult(result: any) {
    const err = result?.error || null;
    debugError = err;
    debugRawText = result?.raw_text || "";
    debugNormalized = result?.normalized_value ?? null;
    debugLastScan = result?.scanned_at || new Date().toISOString();
    debugCaptureSize = (result?.capture_width && result?.capture_height)
      ? `${result.capture_width}×${result.capture_height}`
      : null;

    // Update debug image paths (fixed latest files)
    debugRawPath = result?.raw_crop_path || null;
    debugPreprocessedPath = result?.preprocessed_path || null;

    if (debugRawPath) {
      const { convertFileSrc } = await import('@tauri-apps/api/core');
      // Cache bust because file is always named last_capture.png
      debugRawUrl = convertFileSrc(`${debugRawPath}?t=${Date.now()}`);
    } else {
      debugRawUrl = null;
    }
    if (debugPreprocessedPath) {
      const { convertFileSrc } = await import('@tauri-apps/api/core');
      debugPreprocessedUrl = convertFileSrc(`${debugPreprocessedPath}?t=${Date.now()}`);
    } else {
      debugPreprocessedUrl = null;
    }

    const normalized = result?.normalized_value;

    // Special handling for Tesseract missing - surface clearly
    if (err && err.toLowerCase().includes("tesseract not found")) {
      ocrError = err;
      // Do not set matches or lastScanSource in this case
      return;
    }

    if (normalized) {
      // Feed to the single JSON matcher
      const matched = matchObservedValue(normalized, tolerance);
      matches = matched;
      lastScanTime = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
      lastScanSource = "ocr";
      ocrError = null;
    } else if (err) {
      ocrError = err;
    } else {
      ocrError = "No valid signature detected.";
    }
  }

  async function performOcrScan() {
    if (!hasRegion) {
      ocrError = "No region selected. Click 'Set Region' first.";
      return;
    }

    if (isScanning) {
      // Skip to prevent overlapping OCR jobs
      return;
    }

    isScanning = true;

    try {
      // Pass current dev config on every scan (Scan Now and Active Scan ticks)
      const result = await invoke("scan_selected_region", { config: ocrConfig });
      await processOcrResult(result);
    } catch (e) {
      ocrError = "OCR scan failed: " + String(e);
      debugError = ocrError;
    } finally {
      isScanning = false;
    }
  }

  // Public entry points that both use the shared processor
  async function scanNow() {
    matches = [];
    await performOcrScan();
  }

  // Called by Active Scan timer ticks
  let lastActiveScanError = $state<string | null>(null);

  async function runActiveScanTick() {
    if (!activeScanOn || !hasRegion || isScanning) return;

    await performOcrScan();

    // Error safety: only update UI error if it actually changed (avoid spamming every 3s)
    if (debugError && debugError !== lastActiveScanError) {
      lastActiveScanError = debugError;
      ocrError = debugError;

      // If Tesseract is missing, automatically disable Active Scan to prevent hammering
      if (debugError.toLowerCase().includes("tesseract not found")) {
        activeScanOn = false;
        ocrError = "Tesseract not found. Active Scan has been disabled. Install Tesseract and re-enable Active Scan.";
      }
    } else if (!debugError) {
      lastActiveScanError = null;
    }
  }

  // ==================== Real Region Capture Test (debug only) ====================
  async function captureTest() {
    lastCaptureInfo = null;
    capturePreviewPath = null;
    capturePreviewUrl = null;

    if (!hasRegion) {
      lastCaptureInfo = "Set a region first.";
      return;
    }

    try {
      const result: any = await invoke("capture_region_preview");
      console.log("[CaptureTest] Raw result from Rust:", result);

      if (!result.success) {
        lastCaptureInfo = result.error || "Capture failed.";
        return;
      }

      lastCaptureInfo = `${result.width}×${result.height} captured at ${new Date().toLocaleTimeString()}`;

      // Prefer base64 data URL returned from Rust (most reliable for dev/debug)
      if (result.preview_data_url) {
        capturePreviewUrl = result.preview_data_url;
        capturePreviewPath = result.image_path || null;
        console.log("[CaptureTest] Using base64 preview_data_url from Rust");
        return;
      }

      // Fallback: use convertFileSrc + cache busting
      if (result.image_path) {
        capturePreviewPath = result.image_path;
        const { convertFileSrc } = await import('@tauri-apps/api/core');
        const cacheBusted = `${result.image_path}?t=${Date.now()}`;
        capturePreviewUrl = convertFileSrc(cacheBusted);
        console.log("[CaptureTest] Final img src via convertFileSrc:", capturePreviewUrl);
      } else {
        lastCaptureInfo = "Capture succeeded but no preview path returned.";
      }
    } catch (e) {
      lastCaptureInfo = "Capture test failed: " + String(e);
      console.error("[CaptureTest] Error:", e);
    }
  }

  async function checkTesseract() {
    try {
      tesseractStatus = await invoke("check_tesseract");
    } catch (e) {
      tesseractStatus = { available: false, error: String(e) };
    }
  }

  // Toggle Active Scan via Rust (keeps Rust state and frontend in sync)
  async function toggleActiveScan() {
    try {
      const newState: boolean = await invoke("toggle_active_scan");
      activeScanOn = newState;

      if (!newState) {
        // Stop immediately
        ocrError = null;
        lastActiveScanError = null;
      }
    } catch (e) {
      ocrError = "Failed to toggle Active Scan: " + String(e);
    }
  }

  // Load region status on mount + listen for Active Scan timer ticks
  onMount(async () => {
    refreshRegionStatus();
    checkTesseract(); // one-time availability check on startup

    // Listen for timer heartbeats from Rust.
    // Active Scan performs a real scan (via the shared path) on each valid tick.
    const { listen } = await import('@tauri-apps/api/event');

    await listen('timer-scan-tick', () => {
      if (activeScanOn && hasRegion && !isScanning) {
        runActiveScanTick();
      }
    });

    // Sync with Rust when Active Scan is toggled (e.g. via hotkey)
    await listen('active-scan-toggled', (event: any) => {
      activeScanOn = !!event.payload;
      if (!activeScanOn) {
        lastActiveScanError = null;
        isScanning = false;
      }
    });
  });
</script>

<div class="min-h-screen bg-bg text-text flex flex-col">
  <!-- Thin status row -->
  <div class="status-row">
    <div class="flex items-center gap-1">
      <span class="status-dot {hasRegion ? 'bg-success' : 'bg-danger'}"></span>
      <span>{hasRegion ? "Region Ready" : "No Region"}</span>
    </div>

    <div class="flex items-center gap-1">
      <span class="status-dot {tesseractStatus?.available ? 'bg-success' : 'bg-danger'}"></span>
      <span>{tesseractStatus?.available ? "Tesseract Ready" : "Tesseract Missing"}</span>
    </div>

    <div class="flex items-center gap-1">
      <span class="status-dot {activeScanOn ? 'bg-success' : 'bg-text-muted'}"></span>
      <span>Active Scan: {activeScanOn ? "ON" : "OFF"}</span>
    </div>

    <div class="flex-1"></div>

    {#if lastScanTime}
      <div class="text-[10px] text-text-muted">
        Last: {lastScanTime} • {lastScanSource?.toUpperCase() || ''} {debugNormalized ? `(${debugNormalized})` : ''}
      </div>
    {:else}
      <div class="text-[10px] text-text-muted">Last: never</div>
    {/if}
  </div>

  <div class="p-3 flex flex-col gap-3 flex-1">
    <!-- Header -->
    <div class="flex items-center justify-between">
      <div>
        <span class="font-semibold text-base tracking-tight">SigLock</span>
        <span class="text-xs text-text-muted ml-1">v0.1</span>
      </div>
      <div class="text-[10px] text-text-muted">Mining Signature Overlay</div>
    </div>

    <!-- Manual Input (primary, always available) — uses JSON-driven matcher -->
    <div class="card p-3">
      <div class="text-xs text-text-muted mb-1.5 font-medium">MANUAL INPUT — from src/lib/data/signatures.json</div>
      
      <div class="flex gap-2 items-center">
        <input
          type="text"
          class="input flex-1 text-xl font-mono tracking-wider"
          bind:value={observed}
          placeholder="e.g. 10800"
          onkeydown={(e) => e.key === 'Enter' && runMatch()}
        />
        <button class="btn btn-primary px-5" onclick={runMatch}>
          Match
        </button>
        <button class="btn btn-secondary px-3" onclick={clearInput}>
          Clear
        </button>
      </div>
      
      <div class="mt-2 flex items-center gap-2 text-xs">
        <label class="text-text-muted">Tolerance</label>
        <input 
          type="number" 
          class="input w-16 text-center py-0.5 text-sm" 
          bind:value={tolerance}
          min="0"
          max="200"
        />
        <span class="text-text-muted">± RS</span>
        <span class="ml-auto text-[10px] text-text-muted">(default 25)</span>
      </div>
    </div>

    <!-- Region + OCR Controls -->
    <div class="card p-3">
      <div class="flex items-center justify-between mb-2">
        <div class="font-medium text-sm">OCR Region</div>
        <div class="text-xs {hasRegion ? 'text-success' : 'text-danger'}">
          {hasRegion ? "Region Ready" : "No Region"}
        </div>
      </div>

      {#if regionInfo}
        <div class="text-[10px] text-text-muted mb-2 font-mono">{regionInfo}</div>
      {/if}

      <div class="flex gap-2 mb-2">
        <button class="btn btn-secondary flex-1 text-xs py-1.5" onclick={setRegion}>
          Set Region
        </button>
        <button class="btn btn-secondary flex-1 text-xs py-1.5" onclick={clearRegion} disabled={!hasRegion}>
          Clear
        </button>
        <button 
          class="btn btn-primary flex-1 text-xs py-1.5" 
          onclick={scanNow}
          disabled={!hasRegion}
        >
          Scan Now
        </button>
      </div>

      <!-- Active Scan Toggle -->
      <div class="flex items-center justify-between mt-2">
        <div class="font-medium text-sm">Active Scan</div>
        <div class="toggle" onclick={toggleActiveScan}>
          <div class="toggle-switch {activeScanOn ? 'on' : ''}"></div>
          <span class="text-xs font-medium {activeScanOn ? 'text-success' : 'text-text-muted'}">
            {activeScanOn ? "ON" : "OFF"}
          </span>
        </div>
      </div>

      <!-- Capture Test for alignment verification (real capture, mock OCR still used elsewhere) -->
      <div class="flex gap-2 mb-2">
        <button 
          class="btn btn-secondary flex-1 text-xs py-1.5" 
          onclick={captureTest}
          disabled={!hasRegion}
        >
          Capture Test
        </button>
      </div>

      {#if lastCaptureInfo}
        <div class="text-[10px] mb-1 {lastCaptureInfo.includes('failed') || lastCaptureInfo.includes('Set a region') ? 'text-danger' : 'text-text-muted'}">
          {lastCaptureInfo}
        </div>
      {/if}

      {#if capturePreviewUrl}
        <div class="mt-1">
          <div class="text-[10px] text-text-muted mb-1">Captured crop preview:</div>
          <img 
            src={capturePreviewUrl} 
            alt="Captured region preview" 
            class="max-w-full border border-border rounded max-h-[120px] bg-black/50 object-contain"
            onerror={() => { 
              console.error("[CaptureTest] Image failed to load. Path was:", capturePreviewPath); 
            }}
          />
          {#if capturePreviewPath}
            <div class="text-[9px] text-text-muted mt-0.5 font-mono break-all">{capturePreviewPath}</div>
          {/if}
        </div>
      {/if}

      {#if ocrError}
        <div class="text-xs text-danger bg-bg-elev p-1 rounded mt-1">{ocrError}</div>
      {/if}

      <div class="text-[10px] text-text-muted mt-1">
        Capture Test does real screen capture of the crop. Scan Now / Active Scan use the current OCR settings below.
      </div>

      <!-- Collapsible OCR Debug / Tuning Panel (dev only) - collapsed by default -->
      <details class="mt-2" bind:open={showDebug}>
        <summary class="cursor-pointer text-xs font-medium text-text-muted select-none">
          OCR Debug &amp; Tuning {showDebug ? '▼' : '▶'}
        </summary>

        <div class="mt-2 space-y-2 text-xs">
          <!-- Tesseract status -->
          <div>
            <button class="btn btn-secondary text-[10px] py-0.5 px-2" onclick={checkTesseract}>Check Tesseract</button>
            {#if tesseractStatus}
              <span class={tesseractStatus.available ? 'text-success' : 'text-danger'}>
                {tesseractStatus.available ? '✓ Found' : '✗ Missing'}
              </span>
              {#if tesseractStatus.version}<span class="text-text-muted"> — {tesseractStatus.version}</span>{/if}
              {#if tesseractStatus.error}<div class="text-danger text-[10px]">{tesseractStatus.error}</div>{/if}
            {/if}
          </div>

          <!-- Preprocessing controls -->
          <div class="grid grid-cols-2 gap-x-3 gap-y-1">
            <label class="flex items-center gap-1">
              Upscale
              <select bind:value={ocrConfig.upscale} class="input text-[10px] py-0 px-1 w-12">
                <option value={1}>1x</option>
                <option value={2}>2x</option>
                <option value={3}>3x</option>
                <option value={4}>4x</option>
              </select>
            </label>

            <label class="flex items-center gap-1">
              PSM
              <select bind:value={ocrConfig.psm} class="input text-[10px] py-0 px-1 w-12">
                <option value={7}>7 (line)</option>
                <option value={8}>8 (word)</option>
                <option value={13}>13 (raw)</option>
              </select>
            </label>

            <label class="flex items-center gap-1 col-span-2">
              <input type="checkbox" bind:checked={ocrConfig.threshold_enabled} />
              Threshold
              {#if ocrConfig.threshold_enabled}
                <input type="number" bind:value={ocrConfig.threshold} class="input text-[10px] py-0 w-12" min="0" max="255" />
              {/if}
            </label>

            <label class="flex items-center gap-1">
              <input type="checkbox" bind:checked={ocrConfig.grayscale} /> Grayscale
            </label>
            <label class="flex items-center gap-1">
              <input type="checkbox" bind:checked={ocrConfig.invert} /> Invert
            </label>
            <label class="flex items-center gap-1">
              <input type="checkbox" bind:checked={ocrConfig.sharpen} /> Sharpen
            </label>
            <label class="flex items-center gap-1">
              <input type="checkbox" bind:checked={ocrConfig.numeric_only} /> Numeric only
            </label>
          </div>

          <!-- Debug outputs -->
          <div class="space-y-1 pt-1 border-t border-border/50">
            <div><strong>Raw OCR:</strong> <span class="font-mono">{debugRawText || '—'}</span></div>
            <div><strong>Normalized:</strong> {debugNormalized ?? '—'}</div>
            <div><strong>Size:</strong> {debugCaptureSize || '—'}</div>
            <div><strong>Last scan:</strong> {debugLastScan ? new Date(debugLastScan).toLocaleTimeString() : '—'}</div>

            {#if debugError}
              <div class="text-danger"><strong>Error:</strong> {debugError}</div>
            {/if}

            <!-- Debug image previews -->
            <div class="grid grid-cols-2 gap-2 pt-1">
              <div>
                <div class="text-[10px] text-text-muted">Raw crop</div>
                {#if debugRawUrl}
                  <img 
                    src={debugRawUrl} 
                    alt="Raw crop" 
                    class="max-h-16 border border-border rounded bg-black/30" 
                    onerror={() => console.error("[Debug] Raw crop image failed to load. Path:", debugRawPath)}
                  />
                  <div class="text-[9px] text-text-muted mt-0.5 font-mono break-all">{debugRawPath}</div>
                {:else}
                  <div class="text-[10px] text-text-muted">—</div>
                {/if}
              </div>
              <div>
                <div class="text-[10px] text-text-muted">Preprocessed</div>
                {#if debugPreprocessedUrl}
                  <img 
                    src={debugPreprocessedUrl} 
                    alt="Preprocessed" 
                    class="max-h-16 border border-border rounded bg-black/30" 
                    onerror={() => console.error("[Debug] Preprocessed image failed to load. Path:", debugPreprocessedPath)}
                  />
                  <div class="text-[9px] text-text-muted mt-0.5 font-mono break-all">{debugPreprocessedPath}</div>
                {:else}
                  <div class="text-[10px] text-text-muted">—</div>
                {/if}
              </div>
            </div>
          </div>
        </div>
      </details>
    </div>

    <!-- Results (now driven by src/lib/data/signatures.json via signatures.ts) -->
    <div class="card p-3 flex-1 min-h-[180px]">
      <div class="flex items-center justify-between mb-2">
        <div class="font-medium text-sm">Matches</div>
        <div class="text-xs text-text-muted">{matches.length} result{matches.length === 1 ? '' : 's'}</div>
      </div>

      {#if matches.length === 0}
        <div class="text-center py-8 text-text-muted text-xs">
          Enter a scan signature above (try 3885, 10800, 3170, 3600).<br>
          Data comes only from <span class="font-mono">src/lib/data/signatures.json</span>
        </div>
      {:else}
        <div class="overflow-x-auto">
          <table class="w-full text-xs">
            <thead>
              <tr class="text-text-muted border-b border-border">
                <th class="text-left py-1 pr-2">Material</th>
                <th class="text-right py-1 px-2">Rocks</th>
                <th class="text-right py-1 px-2">Expected</th>
                <th class="text-right py-1 px-2">Observed</th>
                <th class="text-right py-1 px-2">Delta</th>
                <th class="text-center py-1 px-2">Type</th>
                <th class="text-right py-1 pl-2">Conf</th>
              </tr>
            </thead>
            <tbody>
              {#each matches as m}
                <tr class="border-b border-border/50 last:border-none">
                  <td class="py-1 pr-2 font-semibold">{m.material}</td>
                  <td class="py-1 px-2 text-right font-mono tabular-nums">×{m.rockCount}</td>
                  <td class="py-1 px-2 text-right font-mono tabular-nums text-text-muted">{m.expected}</td>
                  <td class="py-1 px-2 text-right font-mono tabular-nums">{m.observed}</td>
                  <td class="py-1 px-2 text-right font-mono tabular-nums {m.delta === 0 ? 'text-success font-medium' : 'text-warning'}">
                    {formatDelta(m.delta)}
                  </td>
                  <td class="py-1 px-2 text-center">
                    <span class={m.matchType === 'exact' ? 'text-success font-medium' : 'text-warning'}>
                      {m.matchType}
                    </span>
                  </td>
                  <td class="py-1 pl-2 text-right font-mono tabular-nums text-accent">
                    {(m.confidence * 100).toFixed(0)}%
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>

    <!-- Footer note -->
    <div class="text-center text-[10px] text-text-muted pt-1">
      Manual input always works • Data source: src/lib/data/signatures.json (single source of truth)
    </div>
  </div>
</div>
