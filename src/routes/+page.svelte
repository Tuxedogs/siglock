<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { emitTo, listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { isRegistered, register, unregister } from '@tauri-apps/plugin-global-shortcut';
  import { relaunch } from '@tauri-apps/plugin-process';
  import { check } from '@tauri-apps/plugin-updater';
  import { dev } from '$app/environment';
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
  let historyFilter = $state<'all' | 'matches' | 'issues'>('all');
  let capturePreviewUrl = $state<string | null>(null);
  let debugResult = $state<any>(null);
  let overlayError = $state<string | null>(null);
  let updateStatus = $state('Check Updates');
  let updating = $state(false);
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
      const result: any = await invoke('scan_selected_region', { config: ocrConfig, trigger });
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

  function keybindWarning(message: string) {
    if (/already registered|already active/i.test(message)) return 'Shortcut already active. See Advanced Debug.';
    if (/unavailable|could not register|hotkey/i.test(message)) return 'Shortcut unavailable. See Advanced Debug.';
    return message.length > 72 ? 'Shortcut warning. See Advanced Debug.' : message;
  }

  function visibleHistory() {
    if (historyFilter === 'matches') return history.filter((entry) => entry.status === 'matched');
    if (historyFilter === 'issues') return history.filter((entry) => entry.status !== 'matched');
    return history;
  }

  function historyTitle(entry: HistoryEntry) {
    if (isSystemError(entry)) return 'Scan error';
    if (entry.status === 'failed') return 'No valid number';
    if (entry.status === 'no match') return 'No match';
    if (entry.status === 'skipped') return entry.material === 'No region' ? 'Region not set' : 'Scan skipped';
    return entry.material;
  }

  function historyStatus(entry: HistoryEntry) {
    if (entry.status === 'failed') return 'Issue';
    if (entry.status === 'no match') return 'No match';
    if (entry.status === 'matched') return 'Match';
    return entry.status;
  }

  function triggerLabel(trigger: Trigger) {
    return trigger === 'Active' ? 'Auto' : trigger;
  }

  function isSystemError(entry: HistoryEntry) {
    return entry.status === 'failed' && /^OCR scan failed:/i.test(entry.material);
  }

  function scannerSummary() {
    if (!tesseractStatus?.available) return { value: 'Error', detail: 'Scanner unavailable', tone: 'danger' };
    if (!hasRegion) return { value: 'Needs region', detail: 'Choose a capture region', tone: 'warning' };
    if (isScanning) return { value: 'Active', detail: 'Reading capture region', tone: 'active' };
    return { value: 'Ready', detail: activeScanOn ? 'Auto scan is running' : 'Live capture ready', tone: 'good' };
  }

  function lastScanSummaryCard() {
    const entry = history[0];
    if (!entry) return { value: 'Not scanned', detail: 'Ready when you are', tone: 'neutral' };
    const value = entry.status === 'matched' ? 'Matched'
      : entry.status === 'no match' ? 'No match'
      : entry.status === 'failed' ? 'Failed'
      : 'Skipped';
    return {
      value,
      detail: `${new Date(entry.timestamp).toLocaleTimeString()} · ${entry.durationMs}ms`,
      tone: entry.status === 'matched' ? 'good' : entry.status === 'failed' ? 'danger' : 'neutral',
    };
  }

  function regionSize() {
    return regionInfo?.split(' @ ')[0] ?? null;
  }

  function setIntervalSeconds(seconds: number) {
    settings.activeScanIntervalMs = seconds * 1000;
    void updateInterval();
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

  async function checkForUpdates() {
    if (updating) return;
    updating = true;
    updateStatus = 'Checking...';
    try {
      const update = await check();
      if (!update) {
        updateStatus = 'Up to date';
        return;
      }

      updateStatus = `Installing ${update.version}...`;
      await update.downloadAndInstall();
      await relaunch();
    } catch (error) {
      updateStatus = 'Update failed';
      scannerStatus = `Update failed: ${String(error)}`;
    } finally {
      updating = false;
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
      } else {
        if (await isRegistered(settings.scanNowKeybind)) await unregister(settings.scanNowKeybind);
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
      <img class="brand-mark" src="/siglock-icon.png" alt="" data-tauri-drag-region />
      <strong data-tauri-drag-region>SigLock</strong>
      <span data-tauri-drag-region>Mining Signature Overlay</span>
      <i data-tauri-drag-region></i>
    </div>
    <div class="status-pills">
      <button onclick={checkForUpdates} disabled={updating}>{updateStatus}</button>
      <button class:good={overlayVisible} onclick={toggleOverlay}>Overlay {overlayVisible ? 'On' : 'Off'}</button>
      <span class:good={activeScanOn}>Auto {activeScanOn ? 'On' : 'Off'}</span>
    </div>
  </header>

  <section class="session-summary" aria-label="Session summary">
    <div class="summary-card {scannerSummary().tone}"><span class="summary-icon">◎</span><div><small>Scanner</small><strong>{scannerSummary().value}</strong><span>{scannerSummary().detail}</span></div></div>
    <div class="summary-card {lastScanSummaryCard().tone}"><span class="summary-icon">⌁</span><div><small>Last Scan</small><strong>{lastScanSummaryCard().value}</strong><span>{lastScanSummaryCard().detail}</span></div></div>
    <div class="summary-card {hasRegion ? 'good' : 'warning'}"><span class="summary-icon">◉</span><div><small>Capture Region</small><strong>{hasRegion ? 'Ready' : 'Not set'}</strong><span>{regionSize() ?? 'Choose a capture region'}</span></div></div>
  </section>

  <div class="app-grid">
    <div class="column">
      <div class="column-label">Controls</div>
      <section class="card">
        <div class="section-title"><h2><span class="title-icon blue">⌖</span>Manual Scan</h2></div>
        <div class="button-row">
          <button class="primary" onclick={() => void performScan('Manual')} disabled={isScanning && queuedTrigger !== null}>Scan Now</button>
          <input class="signature-input" aria-label="Test signature" placeholder="Test signature" bind:value={observed} onkeydown={(event) => event.key === 'Enter' && runManualMatch()} />
          <button onclick={runManualMatch}>Match Value</button>
        </div>
        <p class="hint">Run a scan or test a known signature.</p>
      </section>

      <section class="card">
        <div class="section-title"><h2><span class="title-icon green">⟳</span>Auto Scan</h2><button class:primary={!activeScanOn} class:active={activeScanOn} onclick={toggleActiveScan}>{activeScanOn ? 'Stop' : 'Start'}</button></div>
        <p class="hint card-intro">Runs one scan at a time.</p>
        <div class="interval-control"><span>Interval</span><div class="segment-group">{#each [1, 2, 3, 4] as seconds}<button class:active={settings.activeScanIntervalMs === seconds * 1000} onclick={() => setIntervalSeconds(seconds)}>{seconds}s</button>{/each}</div></div>
      </section>

      <section class="card">
        <div class="section-title keybind-title"><h2><span class="title-icon purple">⌨</span>Scan Shortcut</h2><kbd>{capturingKeybind ? 'Press a key or mouse button...' : keybindLabel(settings.scanNowKeybind)}</kbd></div>
        <div class="button-row">
          <button class="primary" onclick={() => { capturingKeybind = true; keybindError = null; }}>Set Shortcut</button>
          <button onclick={resetKeybind}>Reset</button>
        </div>
        <p class="hint">{keybindError ? keybindWarning(keybindError) : 'Press the shortcut to trigger a manual scan.'}</p>
      </section>

      <section class="card">
        <div class="section-title"><h2><span class="title-icon amber">⌾</span>Capture Region</h2>{#if hasRegion}<span class="region-summary">{regionSize()}</span>{/if}</div>
        <div class="button-row">
          <button class="primary" onclick={setRegion}>Set Region</button>
          <button onclick={clearRegion} disabled={!hasRegion}>Clear</button>
          {#if dev}<button onclick={captureTest} disabled={!hasRegion}>Test</button>{/if}
        </div>
        <div class="region-state">
          {#if dev && capturePreviewUrl}<img class="capture-preview" src={capturePreviewUrl} alt="Capture preview" />{/if}
          <div><strong>{hasRegion ? 'Region ready' : 'No region selected'}</strong><span>{hasRegion ? `${regionSize()} pixels` : 'Choose the signature area to scan.'}</span></div>
        </div>
      </section>

      {#if dev}<details class="card">
        <summary><span><span class="title-icon muted">&lt;/&gt;</span>Advanced Debug</span><i>Raw logs, OCR output, and internal details.</i></summary>
        <div class="debug-grid">
          <label>Tolerance <input type="number" min="0" max="200" bind:value={tolerance} /></label>
          <pre>{JSON.stringify({ tesseractStatus, debugResult, ocrError, overlayError, keybindError, scannerStatus, regionInfo, lastScanSummary, lastScanTime }, null, 2)}</pre>
        </div>
      </details>{/if}
    </div>

    <div class="column">
      <div class="column-label">Results + Overlay</div>
      <section class="card">
        <div class="section-title overlay-title"><h2><span class="title-icon purple">◉</span>Overlay Style</h2><span class="button-row"><button class:active={overlaySetupMode} onclick={toggleOverlaySetupMode}>{overlaySetupMode ? 'Lock Overlay' : 'Unlock'}</button><button onclick={resetOverlayPosition}>Reset Position</button></span></div>
        <div class="appearance-grid">
          <label>Text color <input type="color" bind:value={settings.overlayTextColor} onchange={persistSettings} /></label>
          <label>Background color <input type="color" bind:value={settings.overlayBackgroundColor} onchange={persistSettings} /></label>
          <label>Accent color <input type="color" bind:value={settings.overlayAccentColor} onchange={persistSettings} /></label>
          <label>Opacity <strong>{Math.round(settings.overlayOpacity * 100)}%</strong><input type="range" min="0.35" max="1" step="0.05" bind:value={settings.overlayOpacity} onchange={persistSettings} /></label>
          <label>Text size <strong>{settings.overlayFontSize}px</strong><input type="range" min="11" max="20" step="1" bind:value={settings.overlayFontSize} onchange={persistSettings} /></label>
          <label>Result lifetime <strong>{settings.overlayResultLifetimeSeconds}s</strong><input type="range" min="5" max="120" step="5" bind:value={settings.overlayResultLifetimeSeconds} onchange={persistSettings} /></label>
          <label class="toggle"><input type="checkbox" bind:checked={settings.overlayHighContrast} onchange={persistSettings} /><span></span> High contrast</label>
          <label class="toggle"><input type="checkbox" bind:checked={settings.overlayCompactMode} onchange={persistSettings} /><span></span> Compact mode</label>
        </div>
      </section>

      <section class="card results">
        <div class="section-title"><h2><span class="title-icon green">◇</span>Current Finds</h2><span class="count-pill">{overlayMatches.length}</span></div>
        {#if overlayMatches.length}
          <div class="find-grid">{#each overlayMatches.slice(0, 3) as match}
            <div class="find-card"><strong>{match.material}</strong><span>{match.rockCount} rocks</span>{#if match.repeatCount > 1}<b>×{match.repeatCount}</b>{/if}</div>
          {/each}</div>
        {:else}<p class="empty">No finds yet. Matching signatures will appear here.</p>{/if}
      </section>

      <section class="card history-card">
        <div class="section-title history-title">
          <h2><span class="title-icon blue">⌁</span>Scan Feed</h2>
          <div class="history-actions">
            <div class="filter-group" aria-label="Filter scan results">
              <button class:active={historyFilter === 'all'} onclick={() => historyFilter = 'all'}>All</button>
              <button class:active={historyFilter === 'matches'} onclick={() => historyFilter = 'matches'}>Matches</button>
              <button class:active={historyFilter === 'issues'} onclick={() => historyFilter = 'issues'}>Issues</button>
            </div>
            <button onclick={() => history = []} disabled={!history.length}>Clear</button>
          </div>
        </div>
        <div class="history-list">
          {#if visibleHistory().length}
            {#each visibleHistory() as entry (entry.id)}
              <div class:matched-row={entry.status === 'matched'} class="history-row">
                <span class:system-error={isSystemError(entry)} class="status {entry.status}">{historyStatus(entry)}</span>
                <div class="history-primary"><strong>{historyTitle(entry)}</strong><span>{triggerLabel(entry.trigger)} · {new Date(entry.timestamp).toLocaleTimeString()} · {entry.durationMs}ms{entry.confidence !== null ? ` · ${Math.round(entry.confidence * 100)}%` : ''}</span></div>
                <div class="history-value">
                  {#if entry.status === 'matched'}<strong>{entry.rawValue}</strong>{/if}
                </div>
                {#if entry.repeatCount > 1}<b class="repeat">×{entry.repeatCount}</b>{/if}
              </div>
            {/each}
          {:else}<p class="empty">{history.length ? 'No results in this view.' : 'Scans will appear here, newest first.'}</p>{/if}
        </div>
      </section>
    </div>
  </div>
</main>
