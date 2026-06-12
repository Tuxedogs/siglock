<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { emitTo, listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { isRegistered, register, unregister } from '@tauri-apps/plugin-global-shortcut';
  import { matchObservedValue, type MatchResult } from '$lib/data/signatures';
  import { buildScanResultKey, isDuplicateResult, normalizeMaterial } from '$lib/scanDedupe';
  import { DEFAULT_SETTINGS, loadSettings, saveSettings, type SigLockSettings } from '$lib/settings';

  type Trigger = 'Manual' | 'Active';
  type ScanStatus = 'matched' | 'no match' | 'failed' | 'skipped';
  type HistoryEntry = {
    id: number;
    timestamp: string;
    trigger: Trigger;
    material: string;
    rawValue: string;
    confidence: number | null;
    status: ScanStatus;
    durationMs: number;
    repeatCount: number;
  };
  type OverlayMatch = {
    key: string;
    material: string;
    rockCount: number;
    repeatCount: number;
    updatedAt: string;
  };
  type LastAcceptedScan = {
    key: string;
    acceptedAt: number;
    historyId: number;
    overlayKeys: string[];
  };

  const ocrConfig = {
    upscale: 2,
    threshold_enabled: true,
    threshold: 200,
    grayscale: true,
    invert: false,
    sharpen: false,
    psm: 7,
    numeric_only: true,
  };

  let settings = $state<SigLockSettings>({ ...DEFAULT_SETTINGS });
  let settingsReady = $state(false);
  let observed = $state('3885');
  let tolerance = $state(25);
  let matches = $state<MatchResult[]>([]);
  let history = $state<HistoryEntry[]>([]);
  let hasRegion = $state(false);
  let regionInfo = $state<string | null>(null);
  let activeScanOn = $state(false);
  let overlayVisible = $state(true);
  let overlaySetupMode = $state(false);
  let overlayMatches = $state<OverlayMatch[]>([]);
  let lastAcceptedScan: LastAcceptedScan | null = null;
  let isScanning = $state(false);
  let queuedTrigger = $state<Trigger | null>(null);
  let scannerStatus = $state('Ready');
  let ocrError = $state<string | null>(null);
  let tesseractStatus = $state<any>(null);
  let lastScanTime = $state<string | null>(null);
  let lastScanSummary = $state('Never scanned');
  let capturingKeybind = $state(false);
  let keybindError = $state<string | null>(null);
  let capturePreviewUrl = $state<string | null>(null);
  let debugResult = $state<any>(null);
  let overlayError = $state<string | null>(null);
  let unlisteners: UnlistenFn[] = [];
  let saveTimer: ReturnType<typeof setTimeout> | null = null;

  function persistSettings() {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      try {
        await saveSettings({ ...settings });
        await emitTo('overlay', 'overlay-settings-updated', { ...settings });
        overlayError = null;
      } catch (error) {
        overlayError = `Overlay settings update failed: ${String(error)}`;
      }
    }, 150);
  }

  async function publishOverlayMatches() {
    try {
      await emitTo('overlay', 'overlay-result-updated', {
        matches: overlayMatches.slice(0, 3),
      });
      overlayError = null;
    } catch (error) {
      overlayError = `Overlay result update failed: ${String(error)}`;
    }
  }

  function addOverlayMatches(nextMatches: MatchResult[], rawValue: string) {
    const updatedAt = new Date().toISOString();
    for (const match of [...nextMatches].reverse()) {
      const key = `${normalizeMaterial(match.material)}|${match.rockCount}|${rawValue.replace(/\D/g, '')}`;
      const existing = overlayMatches.find((item) => item.key === key);
      const next: OverlayMatch = {
        key,
        material: match.material,
        rockCount: match.rockCount,
        repeatCount: (existing?.repeatCount ?? 0) + 1,
        updatedAt,
      };
      overlayMatches = [next, ...overlayMatches.filter((item) => item !== existing)].slice(0, 3);
    }
    void publishOverlayMatches();
  }

  function incrementDuplicate(scan: LastAcceptedScan) {
    const updatedAt = new Date().toISOString();
    history = history.map((entry) => entry.id === scan.historyId
      ? { ...entry, timestamp: updatedAt, repeatCount: entry.repeatCount + 1 }
      : entry);
    overlayMatches = overlayMatches.map((match) => scan.overlayKeys.includes(match.key)
      ? { ...match, updatedAt, repeatCount: match.repeatCount + 1 }
      : match);
    void publishOverlayMatches();
  }

  function addHistory(entry: Omit<HistoryEntry, 'id' | 'repeatCount'>) {
    const signature = `${entry.status}|${entry.material}|${entry.rawValue}`;
    const first = history[0];
    const firstSignature = first ? `${first.status}|${first.material}|${first.rawValue}` : '';
    if (first && signature === firstSignature) {
      history = [{ ...first, timestamp: entry.timestamp, trigger: entry.trigger, durationMs: entry.durationMs, repeatCount: first.repeatCount + 1 }, ...history.slice(1)];
    } else {
      history = [{ ...entry, id: Date.now() + Math.random(), repeatCount: 1 }, ...history].slice(0, settings.rollingHistoryLimit);
    }
  }

  function runManualMatch() {
    const value = Number.parseInt(observed.replace(/\D/g, ''), 10);
    matches = value >= 100 ? matchObservedValue(value, tolerance) : [];
    if (matches.length) addOverlayMatches(matches, String(value));
  }

  async function refreshRegionStatus() {
    try {
      const region = await invoke<any | null>('get_crop_region');
      hasRegion = !!region;
      regionInfo = region ? `${region.width}x${region.height} @ (${region.x}, ${region.y})` : null;
    } catch {
      hasRegion = false;
      regionInfo = null;
    }
  }

  async function performScan(trigger: Trigger) {
    if (!hasRegion) {
      scannerStatus = 'Scan skipped: set a region first.';
      addHistory({ timestamp: new Date().toISOString(), trigger, material: 'No region', rawValue: '-', confidence: null, status: 'skipped', durationMs: 0 });
      return;
    }
    if (isScanning) {
      queuedTrigger = queuedTrigger ?? trigger;
      scannerStatus = 'Scan queued: already scanning.';
      addHistory({ timestamp: new Date().toISOString(), trigger, material: 'Already scanning', rawValue: '-', confidence: null, status: 'skipped', durationMs: 0 });
      return;
    }

    isScanning = true;
    scannerStatus = `${trigger} scan started`;
    const started = performance.now();
    try {
      const result: any = await invoke('scan_selected_region', { config: ocrConfig });
      debugResult = result;
      const rawValue = result?.raw_text || (result?.normalized_value?.toString() ?? '-');
      const durationMs = Math.round(performance.now() - started);
      const normalized = result?.normalized_value;
      const normalizedSignature = normalized?.toString() ?? rawValue.replace(/\D/g, '');
      const nextMatches = normalized ? matchObservedValue(normalized, tolerance) : [];
      matches = nextMatches;
      ocrError = result?.error || null;
      lastScanTime = new Date().toLocaleTimeString();

      const status: ScanStatus = result?.error ? 'failed' : nextMatches.length ? 'matched' : 'no match';
      const material = nextMatches.length ? nextMatches.map((match) => match.material).join(', ') : (result?.error || 'No match');
      const confidence = nextMatches[0]?.confidence ?? result?.confidence ?? null;
      lastScanSummary = status === 'matched' ? `${material} (${rawValue})` : `${status}: ${rawValue}`;
      scannerStatus = `${trigger} scan ${status}`;
      const timestamp = result?.scanned_at || new Date().toISOString();
      if (status === 'matched') {
        const now = Date.now();
        const key = buildScanResultKey(nextMatches, normalizedSignature);
        if (lastAcceptedScan && isDuplicateResult(lastAcceptedScan, key, now)) {
          incrementDuplicate(lastAcceptedScan);
          scannerStatus = trigger === 'Manual' ? 'Duplicate suppressed' : `${trigger} scan duplicate suppressed`;
        } else {
          addOverlayMatches(nextMatches, normalizedSignature);
          addHistory({ timestamp, trigger, material, rawValue, confidence, status, durationMs });
          lastAcceptedScan = {
            key,
            acceptedAt: now,
            historyId: history[0].id,
            overlayKeys: nextMatches.map((match) => `${normalizeMaterial(match.material)}|${match.rockCount}|${normalizedSignature}`),
          };
        }
      } else {
        addHistory({ timestamp, trigger, material, rawValue, confidence, status, durationMs });
      }
    } catch (error) {
      const message = `OCR scan failed: ${String(error)}`;
      ocrError = message;
      scannerStatus = message;
      addHistory({ timestamp: new Date().toISOString(), trigger, material: message, rawValue: '-', confidence: null, status: 'failed', durationMs: Math.round(performance.now() - started) });
    } finally {
      isScanning = false;
      const followUp = queuedTrigger;
      queuedTrigger = null;
      if (followUp) void performScan(followUp);
    }
  }

  async function toggleActiveScan() {
    try {
      activeScanOn = await invoke<boolean>('toggle_active_scan');
      scannerStatus = `Active Scan ${activeScanOn ? 'enabled' : 'disabled'}`;
    } catch (error) {
      scannerStatus = `Active Scan failed: ${String(error)}`;
    }
  }

  async function toggleOverlay() {
    try {
      overlayVisible = await invoke<boolean>('toggle_overlay_visibility');
      overlayError = null;
    } catch (error) {
      overlayError = `Overlay visibility failed: ${String(error)}`;
    }
  }

  async function toggleOverlaySetupMode() {
    try {
      overlaySetupMode = await invoke<boolean>('set_overlay_setup_mode', { enabled: !overlaySetupMode });
      overlayError = null;
    } catch (error) {
      overlayError = `Overlay setup mode failed: ${String(error)}`;
    }
  }

  async function resetOverlayPosition() {
    try {
      await invoke('reset_overlay_position');
      overlayError = null;
      scannerStatus = 'Overlay position reset to x=80, y=120';
    } catch (error) {
      overlayError = `Overlay position reset failed: ${String(error)}`;
    }
  }

  function startMainWindowDrag(event: MouseEvent) {
    if (event.button === 0) void getCurrentWindow().startDragging();
  }

  async function updateInterval() {
    settings.activeScanIntervalMs = Math.min(4000, Math.max(1000, Math.round(settings.activeScanIntervalMs / 1000) * 1000));
    settings.activeScanIntervalMs = await invoke<number>('set_scan_interval', { intervalMs: settings.activeScanIntervalMs });
    scannerStatus = `Active interval set to ${settings.activeScanIntervalMs / 1000}s`;
    persistSettings();
  }

  function isMouseBinding(binding: string) {
    return ['Middle Mouse', 'Mouse4', 'Mouse5'].includes(binding);
  }

  async function applyScanKeybind(binding: string, previous: string) {
    if (binding === previous) return;
    if (isMouseBinding(binding)) {
      await invoke('set_scan_now_mouse_binding', { binding });
      try {
        if (!isMouseBinding(previous) && await isRegistered(previous)) await unregister(previous);
      } catch (error) {
        await invoke('set_scan_now_mouse_binding', { binding: null });
        throw error;
      }
      return;
    }

    await register(binding, (event) => {
      if (event.state === 'Pressed' && !capturingKeybind) void performScan('Manual');
    });
    try {
      await invoke('set_scan_now_mouse_binding', { binding: null });
      if (!isMouseBinding(previous) && await isRegistered(previous)) await unregister(previous);
    } catch (error) {
      await unregister(binding).catch(() => {});
      if (!isMouseBinding(previous) && !await isRegistered(previous)) {
        await register(previous, (event) => {
          if (event.state === 'Pressed' && !capturingKeybind) void performScan('Manual');
        }).catch(() => {});
      }
      throw error;
    }
  }

  function keyName(event: KeyboardEvent): string | null {
    if (['Control', 'Shift', 'Alt', 'Meta', 'Escape'].includes(event.key)) return null;
    if (/^F([1-9]|1[0-9]|2[0-4])$/.test(event.key)) return event.key;
    if (/^Numpad[0-9]$/.test(event.code)) return `Numpad${event.code.slice(-1)}`;
    if (/^Key[A-Z]$/.test(event.code)) return event.code.slice(3);
    if (/^Digit[0-9]$/.test(event.code)) return event.code.slice(5);
    const allowed: Record<string, string> = {
      Space: 'Space', Enter: 'Enter', Tab: 'Tab', Backspace: 'Backspace', Insert: 'Insert', Delete: 'Delete',
      Home: 'Home', End: 'End', PageUp: 'PageUp', PageDown: 'PageDown', ArrowUp: 'ArrowUp',
      ArrowDown: 'ArrowDown', ArrowLeft: 'ArrowLeft', ArrowRight: 'ArrowRight', NumpadAdd: 'NumpadAdd',
      NumpadSubtract: 'NumpadSubtract', NumpadMultiply: 'NumpadMultiply', NumpadDivide: 'NumpadDivide',
      NumpadDecimal: 'NumpadDecimal', NumpadEnter: 'NumpadEnter', BracketLeft: 'BracketLeft',
      BracketRight: 'BracketRight', Semicolon: 'Semicolon', Comma: 'Comma', Period: 'Period',
      Slash: 'Slash', Backslash: 'Backslash', Quote: 'Quote', Minus: 'Minus', Equal: 'Equal', Backquote: 'Backquote',
    };
    return allowed[event.code] ?? null;
  }

  async function captureKeybind(event: KeyboardEvent) {
    if (!capturingKeybind) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.key === 'Escape') {
      capturingKeybind = false;
      return;
    }
    const key = keyName(event);
    const modifiers = [event.ctrlKey && 'Ctrl', event.altKey && 'Alt', event.shiftKey && 'Shift', event.metaKey && 'Super'].filter(Boolean);
    if (!key) {
      keybindError = 'That key is not supported. Press Escape to cancel.';
      return;
    }
    const next = [...modifiers, key].join('+');
    try {
      await applyScanKeybind(next, settings.scanNowKeybind);
      settings.scanNowKeybind = next;
      persistSettings();
      keybindError = null;
      scannerStatus = `Scan Now keybind set to ${next}`;
      capturingKeybind = false;
    } catch (error) {
      keybindError = `Could not register ${next}: ${String(error)}`;
    }
  }

  async function captureMouseKeybind(event: MouseEvent) {
    if (!capturingKeybind) return;
    event.preventDefault();
    event.stopPropagation();
    const binding = event.button === 1 ? 'Middle Mouse' : event.button === 3 ? 'Mouse4' : event.button === 4 ? 'Mouse5' : null;
    if (!binding) {
      keybindError = event.button <= 2 ? 'Left click and right click cannot be bound.' : 'That mouse button is not supported.';
      return;
    }
    try {
      await applyScanKeybind(binding, settings.scanNowKeybind);
      settings.scanNowKeybind = binding;
      persistSettings();
      keybindError = null;
      scannerStatus = `Scan Now keybind set to ${binding}`;
      capturingKeybind = false;
    } catch (error) {
      keybindError = `Could not register ${binding}: ${String(error)}`;
    }
  }

  async function resetKeybind() {
    try {
      await applyScanKeybind(DEFAULT_SETTINGS.scanNowKeybind, settings.scanNowKeybind);
      settings.scanNowKeybind = DEFAULT_SETTINGS.scanNowKeybind;
      persistSettings();
      keybindError = null;
    } catch (error) {
      keybindError = String(error);
    }
  }

  function keybindLabel(binding: string) {
    return binding.replace(/^Numpad([0-9])$/, 'Numpad $1');
  }

  async function setRegion() {
    scannerStatus = 'Opening region picker...';
    try {
      await invoke('open_region_picker');
      setTimeout(refreshRegionStatus, 900);
      setTimeout(refreshRegionStatus, 1800);
    } catch (error) {
      scannerStatus = `Region picker failed: ${String(error)}`;
    }
  }

  async function clearRegion() {
    await invoke('clear_crop_region');
    await refreshRegionStatus();
  }

  async function captureTest() {
    try {
      const result: any = await invoke('capture_region_preview');
      capturePreviewUrl = result?.preview_data_url ?? null;
      scannerStatus = result?.success ? `Captured ${result.width}x${result.height}` : (result?.error || 'Capture failed');
    } catch (error) {
      scannerStatus = `Capture failed: ${String(error)}`;
    }
  }

  onMount(async () => {
    document.addEventListener('keydown', captureKeybind, true);
    document.addEventListener('mousedown', captureMouseKeybind, true);
    try {
      settings = await loadSettings();
    } catch (error) {
      keybindError = `Settings load warning; using safe defaults: ${String(error)}`;
      settings = { ...DEFAULT_SETTINGS };
    }
    settingsReady = true;
    try {
      await invoke('set_scan_interval', { intervalMs: settings.activeScanIntervalMs });
      if (isMouseBinding(settings.scanNowKeybind)) {
        await invoke('set_scan_now_mouse_binding', { binding: settings.scanNowKeybind });
      } else if (!await isRegistered(settings.scanNowKeybind)) {
        await register(settings.scanNowKeybind, (event) => {
          if (event.state === 'Pressed' && !capturingKeybind) void performScan('Manual');
        });
      }
    } catch (error) {
      if (settings.scanNowKeybind !== DEFAULT_SETTINGS.scanNowKeybind) {
        try {
          await applyScanKeybind(DEFAULT_SETTINGS.scanNowKeybind, settings.scanNowKeybind);
          settings.scanNowKeybind = DEFAULT_SETTINGS.scanNowKeybind;
          await saveSettings({ ...settings });
          keybindError = `Saved keybind was unavailable; using ${DEFAULT_SETTINGS.scanNowKeybind}.`;
        } catch (fallbackError) {
          keybindError = `Keybind registration warning: ${String(fallbackError)}`;
        }
      } else {
        keybindError = `Keybind registration warning: ${String(error)}`;
      }
    }

    await refreshRegionStatus();
    try {
      tesseractStatus = await invoke('check_tesseract');
      const appState: any = await invoke('get_app_state');
      activeScanOn = !!appState?.active_scan_enabled;
      overlayVisible = !!appState?.overlay_visible;
      overlaySetupMode = !!appState?.overlay_setup_mode;
    } catch (error) {
      scannerStatus = `Backend check failed: ${String(error)}`;
    }

    unlisteners.push(await listen('timer-scan-tick', () => void performScan('Active')));
    unlisteners.push(await listen('scan-now-input', () => { if (!capturingKeybind) void performScan('Manual'); }));
    unlisteners.push(await listen<boolean>('active-scan-toggled', (event) => activeScanOn = !!event.payload));
    unlisteners.push(await listen('hotkey-toggle-active', () => void toggleActiveScan()));
    unlisteners.push(await listen<boolean>('overlay-visibility-changed', (event) => overlayVisible = !!event.payload));
    unlisteners.push(await listen<boolean>('overlay-setup-mode-changed', (event) => overlaySetupMode = !!event.payload));
  });

  onDestroy(() => {
    document.removeEventListener('keydown', captureKeybind, true);
    document.removeEventListener('mousedown', captureMouseKeybind, true);
    unlisteners.forEach((unlisten) => unlisten());
    if (saveTimer) clearTimeout(saveTimer);
  });
</script>

<svelte:head><title>SigLock</title></svelte:head>

<main>
  <header class="topbar">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="drag-title" data-tauri-drag-region onmousedown={startMainWindowDrag}>
      <strong data-tauri-drag-region>SigLock</strong>
      <span data-tauri-drag-region>Control Center</span>
      <i data-tauri-drag-region></i>
    </div>
    <div class="status-pills">
      <span class:good={tesseractStatus?.available}>{tesseractStatus?.available ? 'Scanner ready' : 'Scanner unavailable'}</span>
      <button class:good={overlayVisible} onclick={toggleOverlay}>Overlay {overlayVisible ? 'visible' : 'hidden'}</button>
      <span class:good={activeScanOn}>Active {activeScanOn ? 'on' : 'off'}</span>
    </div>
  </header>

  <section class="scanner-strip">
    <div><small>SCANNER STATUS</small><strong>{isScanning ? 'Scanning...' : scannerStatus}</strong></div>
    <div><small>LAST RESULT</small><strong>{lastScanSummary}</strong>{#if lastScanTime}<span>{lastScanTime}</span>{/if}</div>
    <div><small>REGION</small><strong>{hasRegion ? 'Ready' : 'Not set'}</strong><span>{regionInfo ?? 'Choose a capture region'}</span></div>
  </section>

  <div class="app-grid">
    <div class="column">
      <section class="card">
        <h2>Manual Controls</h2>
        <div class="button-row">
          <button class="primary" onclick={() => void performScan('Manual')} disabled={isScanning && queuedTrigger !== null}>Scan Now</button>
          <input aria-label="Manual signature value" bind:value={observed} onkeydown={(event) => event.key === 'Enter' && runManualMatch()} />
          <button onclick={runManualMatch}>Match Value</button>
        </div>
        <p class="hint">Scan Now works whether Active Scan is on or off. Manual value matching does not run OCR.</p>
      </section>

      <section class="card">
        <div class="section-title"><h2>Active Scan</h2><button class:active={activeScanOn} onclick={toggleActiveScan}>{activeScanOn ? 'Turn Off' : 'Turn On'}</button></div>
        <label>Interval <strong>{settings.activeScanIntervalMs / 1000}s</strong>
          <input type="range" min="1000" max="4000" step="1000" bind:value={settings.activeScanIntervalMs} onchange={updateInterval} />
        </label>
        <p class="hint">One controlled loop, updated immediately. Scans never overlap.</p>
      </section>

      <section class="card">
        <h2>Keybinds</h2>
        <div class="keybind-row"><span>Scan Now</span><kbd>{capturingKeybind ? 'Press a key or mouse button...' : keybindLabel(settings.scanNowKeybind)}</kbd></div>
        <div class="button-row">
          <button class="primary" onclick={() => { capturingKeybind = true; keybindError = null; }}>Set Keybind</button>
          <button onclick={resetKeybind}>Reset Default</button>
        </div>
        {#if keybindError}<p class="error">{keybindError}</p>{/if}
      </section>

      <section class="card">
        <h2>Region / Capture Settings</h2>
        <div class="button-row">
          <button class="primary" onclick={setRegion}>Set Region</button>
          <button onclick={clearRegion} disabled={!hasRegion}>Clear</button>
          <button onclick={captureTest} disabled={!hasRegion}>Capture Test</button>
        </div>
        {#if capturePreviewUrl}<img class="capture-preview" src={capturePreviewUrl} alt="Capture preview" />{/if}
      </section>

      <details class="card">
        <summary>Advanced Debug</summary>
        <div class="debug-grid">
          <label>Tolerance <input type="number" min="0" max="200" bind:value={tolerance} /></label>
          <pre>{JSON.stringify({ tesseractStatus, debugResult, ocrError, overlayError }, null, 2)}</pre>
        </div>
      </details>
    </div>

    <div class="column">
      <section class="card">
        <div class="section-title"><h2>Overlay Appearance</h2><span class="button-row"><button class:active={overlaySetupMode} onclick={toggleOverlaySetupMode}>{overlaySetupMode ? 'Finish positioning' : 'Unlock overlay'}</button><button onclick={resetOverlayPosition}>Reset overlay position</button></span></div>
        <div class="appearance-grid">
          <label>Text <input type="color" bind:value={settings.overlayTextColor} onchange={persistSettings} /></label>
          <label>Background <input type="color" bind:value={settings.overlayBackgroundColor} onchange={persistSettings} /></label>
          <label>Accent <input type="color" bind:value={settings.overlayAccentColor} onchange={persistSettings} /></label>
          <label>Opacity <input type="range" min="0.35" max="1" step="0.05" bind:value={settings.overlayOpacity} onchange={persistSettings} /></label>
          <label>Font size <input type="range" min="11" max="20" step="1" bind:value={settings.overlayFontSize} onchange={persistSettings} /></label>
          <label>Result lifetime <input type="range" min="5" max="120" step="5" bind:value={settings.overlayResultLifetimeSeconds} onchange={persistSettings} /></label>
          <label class="check"><input type="checkbox" bind:checked={settings.overlayHighContrast} onchange={persistSettings} /> High contrast</label>
          <label class="check"><input type="checkbox" bind:checked={settings.overlayCompactMode} onchange={persistSettings} /> Compact mode</label>
        </div>
      </section>

      <section class="card results">
        <div class="section-title"><h2>Current Matches</h2><span>{matches.length}</span></div>
        {#if matches.length}
          {#each matches.slice(0, 5) as match}
            <div class="match-row"><strong>{match.material}</strong><span>x{match.rockCount}</span><span>{match.observed}</span><span>{Math.round(match.confidence * 100)}%</span></div>
          {/each}
        {:else}<p class="empty">No current matches.</p>{/if}
      </section>

      <section class="card history-card">
        <div class="section-title"><h2>Rolling Scan Results</h2><button onclick={() => history = []} disabled={!history.length}>Clear History</button></div>
        <div class="history-list">
          {#if history.length}
            {#each history as entry (entry.id)}
              <div class="history-row">
                <span class="status {entry.status}">{entry.status}</span>
                <div><strong>{entry.material}</strong><span>{entry.trigger} · {new Date(entry.timestamp).toLocaleTimeString()} · {entry.durationMs}ms</span></div>
                <div class="history-value"><strong>{entry.rawValue}</strong>{#if entry.confidence !== null}<span>{Math.round(entry.confidence * 100)}%</span>{/if}</div>
                {#if entry.repeatCount > 1}<b class="repeat">x{entry.repeatCount}</b>{/if}
              </div>
            {/each}
          {:else}<p class="empty">Scans will appear here, newest first.</p>{/if}
        </div>
      </section>
    </div>
  </div>
</main>
