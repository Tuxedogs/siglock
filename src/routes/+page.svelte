<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { getVersion } from '@tauri-apps/api/app';
  import { invoke } from '@tauri-apps/api/core';
  import { emitTo, listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { isRegistered, register, unregister } from '@tauri-apps/plugin-global-shortcut';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { relaunch } from '@tauri-apps/plugin-process';
  import { check, type Update } from '@tauri-apps/plugin-updater';
  import { findNearestSignature, getSignatures, matchObservedValue, type MatchResult } from '$lib/data/signatures';
  import { locationLabel, locationsForMaterial, materialValidAtLocation } from '$lib/data/materialLocations';
  import {
    resolveScanResult,
    type CompositionEntry,
    type CompositionStatus,
    type MaterialCompositionProfile,
    type ScanResult,
  } from '$lib/data/rockCompositions';
  import { buildScanResultKey, isDuplicateResult, normalizeMaterial } from '$lib/scanDedupe';
  import { DEFAULT_SETTINGS, loadSettings, saveSettings, type SigLockSettings, type SystemFilter } from '$lib/settings';

  type Trigger = 'Manual' | 'Active';
  type ScanStatus = 'matched' | 'no match' | 'invalid' | 'failed' | 'skipped';
  type ShortcutAction = 'manual' | 'auto';
  type AppPage = 'dashboard' | 'regions' | 'minables' | 'overlay' | 'settings';
  type ShipId = 'golem' | 'prospector' | 'mole';
  type ScanRegion = { x: number; y: number; width: number; height: number };
  type RegionProfile = {
    id: string;
    name: string;
    builtIn: boolean;
    scanRegion: ScanRegion | null;
  };
  type RegionProfilesSnapshot = {
    activeRegionId: string;
    profiles: RegionProfile[];
  };
  type GameLogStatus = {
    path: string | null;
    channel: string | null;
    health: string;
    currentLocation: string | null;
    locationSource: string | null;
    locationConfidence: string | null;
    detectedShip: ShipId | null;
    shipSource: string | null;
    playerHandle: string | null;
    lastLineAt: string | null;
  };
  type HistoryEntry = {
    id: number;
    timestamp: string;
    trigger: Trigger;
    material: string;
    rawValue: string;
    parsedNumber: number | null;
    normalizedAttemptValue: string | null;
    matchedValue: number | null;
    nearestCandidateMaterial: string | null;
    nearestCandidateValue: number | null;
    delta: number | null;
    rockCount: number | null;
    confidence: number | null;
    otherCandidateMaterials: string[];
    compositionStatus: CompositionStatus;
    status: ScanStatus;
    durationMs: number;
    repeatCount: number;
  };
  type OverlayMatch = {
    key: string;
    material: string;
    compositionProfile: MaterialCompositionProfile | null;
    otherCandidates: string[];
    compositionStatus: CompositionStatus;
    rockCount: number;
    valueLabel?: string;
    detailLabel?: string;
    repeatCount: number;
    updatedAt: string;
    watched: boolean;
    detected: boolean;
  };
  type LastAcceptedScan = {
    key: string;
    acceptedAt: number;
    historyId: number;
    overlayKeys: string[];
  };
  type ReleaseNoteVersion = {
    version: string;
    date?: string;
    items: string[];
    url?: string;
  };

  const GITHUB_RELEASES_API = 'https://api.github.com/repos/Tuxedogs/siglock/releases';
  const GITHUB_RELEASES_URL = 'https://github.com/Tuxedogs/siglock/releases';

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
  let observed = $state('3885');
  let tolerance = $state(25);
  let matches = $state<MatchResult[]>([]);
  let history = $state<HistoryEntry[]>([]);
  let captureRegion = $state<ScanRegion | null>(null);
  let activeRegionId = $state('prospector');
  let regionProfiles = $state<RegionProfile[]>([
    { id: 'golem', name: 'Golem', builtIn: true, scanRegion: null },
    { id: 'prospector', name: 'Prospector', builtIn: true, scanRegion: null },
    { id: 'mole', name: 'MOLE', builtIn: true, scanRegion: null },
  ]);
  let newRegionName = $state('');
  let regionLoadComplete = $state(false);
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
  let capturingShortcutAction = $state<ShortcutAction | null>(null);
  let keybindError = $state<string | null>(null);
  let historyFilter = $state<'all' | 'matches' | 'issues'>('all');
  let capturePreviewUrl = $state<string | null>(null);
  let capturePreviewError = $state<string | null>(null);
  let debugResult = $state<any>(null);
  let overlayError = $state<string | null>(null);
  let appVersion = $state<string | null>(null);
  let updateStatus = $state<string | null>(null);
  let updating = $state(false);
  let latestKnownVersion = $state<string | null>(null);
  let latestKnownNotes = $state<string | null>(null);
  let releaseNotesOpen = $state(false);
  let releaseNotesLoading = $state(false);
  let releaseNotesError = $state(false);
  let releaseNotes = $state<ReleaseNoteVersion[]>([]);
  let openSettingsSection = $state<'shortcuts' | 'scan' | 'advanced'>('shortcuts');
  let unlisteners: UnlistenFn[] = [];
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let capturePreviewLoading = $state(false);
  let lastCapturePreviewAt = 0;
  let scannerStatusBeforeRegionPicker = 'Ready';
  let mainWindowDragActive = false;
  let gameLogStatus = $state<GameLogStatus>({
    path: null, channel: null, health: 'starting', currentLocation: null,
    locationSource: null, locationConfidence: null, detectedShip: null,
    shipSource: null, playerHandle: null, lastLineAt: null,
  });
  let gameLogPathInput = $state('');
  let currentPage = $state<AppPage>('dashboard');
  let minableSearch = $state('');
  let newRegionDraft = $state(false);

  function isValidRegion(region: unknown): region is ScanRegion {
    if (!region || typeof region !== 'object') return false;
    const candidate = region as Partial<ScanRegion>;
    return Number.isInteger(candidate.x)
      && Number.isInteger(candidate.y)
      && Number.isInteger(candidate.width)
      && Number.isInteger(candidate.height)
      && (candidate.width ?? 0) >= 20
      && (candidate.height ?? 0) >= 10;
  }

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

  function matchOptions() {
    return {
      system: settings.selectedSystemFilter,
      includeSalvage: settings.returnSalvageResults,
      includeFpsRoc: settings.includeFpsRocResults,
      location: gameLogStatus.currentLocation,
    };
  }

  function materialLabel(material: string): string {
    return material;
  }

  function activeRegionProfile() {
    return regionProfiles.find((profile) => profile.id === activeRegionId) ?? regionProfiles[0] ?? {
      id: 'prospector', name: 'Prospector', builtIn: true, scanRegion: null,
    };
  }

  function applyRegionSnapshot(snapshot: RegionProfilesSnapshot) {
    activeRegionId = snapshot.activeRegionId;
    regionProfiles = snapshot.profiles;
    const region = activeRegionProfile().scanRegion;
    captureRegion = isValidRegion(region) ? region : null;
  }

  function isWatched(material: string) {
    const key = normalizeMaterial(material);
    return settings.watchedMaterials.some((item) => normalizeMaterial(item) === key);
  }

  function toggleWatch(material: string) {
    settings.watchedMaterials = isWatched(material)
      ? settings.watchedMaterials.filter((item) => normalizeMaterial(item) !== normalizeMaterial(material))
      : [...settings.watchedMaterials, material];
    overlayMatches = overlayMatches.map((match) => ({ ...match, watched: isWatched(match.material) }));
    void publishOverlayMatches();
    persistSettings();
  }

  function visibleMinables() {
    const query = minableSearch.trim().toLowerCase();
    return getSignatures().materials
      .map((material) => ({ ...material, material: material.materialName, locations: locationsForMaterial(material.materialName) }))
      .filter((row) => !query || row.material.toLowerCase().includes(query) || row.locations.some((location) => location.includes(query)))
      .sort((a, b) => {
        const location = gameLogStatus.currentLocation;
        const validDelta = Number(materialValidAtLocation(b.material, location)) - Number(materialValidAtLocation(a.material, location));
        return validDelta || a.material.localeCompare(b.material);
      });
  }

  function locationValidity(row: ReturnType<typeof visibleMinables>[number]) {
    if (!gameLogStatus.currentLocation || !row.locations.length) return '—';
    return materialValidAtLocation(row.material, gameLogStatus.currentLocation) ? 'VALID' : 'OUT';
  }

  function formatCompositionNumber(value: number) {
    return Number.isInteger(value) ? String(value) : value.toFixed(1);
  }

  function compositionPercent(entry: CompositionEntry) {
    return `${formatCompositionNumber(entry.percentMin)}-${formatCompositionNumber(entry.percentMax)}%`;
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

  function addOverlayMatches(scanResult: ScanResult, rawValue: string) {
    const primary = scanResult.primaryMatch;
    if (!primary) return;
    const updatedAt = new Date().toISOString();
    const normalizedValue = rawValue.replace(/\D/g, '') || rawValue;
    const key = `${normalizeMaterial(primary.material)}|${primary.rockCount}|${normalizedValue}`;
    const existing = overlayMatches.find((item) => item.key === key);
    const next: OverlayMatch = {
      key,
      material: primary.material,
      compositionProfile: scanResult.compositionProfile,
      otherCandidates: scanResult.otherCandidates.map((match) => match.material),
      compositionStatus: scanResult.compositionStatus,
      rockCount: primary.rockCount,
      valueLabel: normalizedValue,
      detailLabel: String(primary.expected),
      repeatCount: (existing?.repeatCount ?? 0) + 1,
      updatedAt,
      watched: isWatched(primary.material),
      detected: true,
    };
    overlayMatches = [next, ...overlayMatches.filter((item) => item !== existing && item.rockCount > 0)].slice(0, 3);
    void publishOverlayMatches();
  }

  function showOverlayRead(material: string, valueLabel: string, detailLabel?: string) {
    if (!settings.showScannedValueOnOverlay || settings.onlyShowSolvedResults) return;
    const updatedAt = new Date().toISOString();
    overlayMatches = [{
      key: `${normalizeMaterial(material)}|0|${valueLabel}|${updatedAt}`,
      material,
      compositionProfile: null,
      otherCandidates: [],
      compositionStatus: 'unavailable' as const,
      rockCount: 0,
      valueLabel,
      detailLabel,
      repeatCount: 1,
      updatedAt,
      watched: false,
      detected: false,
    }, ...overlayMatches.filter((item) => item.rockCount > 0)].slice(0, 3);
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
    const signature = `${entry.status}|${entry.material}|${entry.rawValue}|${entry.normalizedAttemptValue}`;
    const first = history[0];
    const firstSignature = first ? `${first.status}|${first.material}|${first.rawValue}|${first.normalizedAttemptValue}` : '';
    if (first && signature === firstSignature) {
      history = [{ ...first, timestamp: entry.timestamp, trigger: entry.trigger, durationMs: entry.durationMs, repeatCount: first.repeatCount + 1 }, ...history.slice(1)];
    } else {
      history = [{ ...entry, id: Date.now() + Math.random(), repeatCount: 1 }, ...history].slice(0, settings.rollingHistoryLimit);
    }
  }

  function runManualMatch() {
    const value = Number.parseInt(observed.replace(/\D/g, ''), 10);
    matches = value >= 100 ? matchObservedValue(value, tolerance, matchOptions()) : [];
    if (matches.length) addOverlayMatches(resolveScanResult(matches), String(value));
  }

  async function loadSavedRegion(startup = false) {
    try {
      applyRegionSnapshot(await invoke<RegionProfilesSnapshot>('get_region_profiles'));
    } catch {
      captureRegion = null;
    } finally {
      regionLoadComplete = true;
      if (startup) {
        console.info(captureRegion
          ? '[SigLock] startup region load: found valid saved region'
          : '[SigLock] startup region load: no valid saved region');
      }
    }
  }

  async function saveGameLogPath() {
    try {
      gameLogStatus = await invoke<GameLogStatus>('configure_game_log_path', { path: gameLogPathInput });
      gameLogPathInput = gameLogStatus.path ?? gameLogPathInput;
      scannerStatus = `Monitoring ${gameLogStatus.channel ?? 'Game.log'}`;
    } catch (error) {
      scannerStatus = `Game.log path failed: ${String(error)}`;
    }
  }

  async function switchRegion(regionId: string) {
    if (regionId === activeRegionId) return;
    try {
      applyRegionSnapshot(await invoke<RegionProfilesSnapshot>('set_active_region', { regionId }));
      newRegionDraft = false;
      capturePreviewUrl = null;
      capturePreviewError = null;
      matches = [];
      scannerStatus = captureRegion
        ? `${activeRegionProfile().name} capture loaded`
        : `${activeRegionProfile().name} needs capture bounds`;
    } catch (error) {
      scannerStatus = `Region switch failed: ${String(error)}`;
    }
  }

  async function createCustomRegion() {
    try {
      applyRegionSnapshot(await invoke<RegionProfilesSnapshot>('create_region', { name: newRegionName }));
      newRegionName = '';
      newRegionDraft = false;
      capturePreviewUrl = null;
      scannerStatus = `${activeRegionProfile().name} created; set its OCR bounds.`;
    } catch (error) {
      scannerStatus = `Region creation failed: ${String(error)}`;
    }
  }

  async function deleteActiveRegion() {
    const profile = activeRegionProfile();
    if (!profile || profile.builtIn) return;
    try {
      applyRegionSnapshot(await invoke<RegionProfilesSnapshot>('delete_region', { regionId: profile.id }));
      capturePreviewUrl = null;
      scannerStatus = `${profile.name} deleted.`;
    } catch (error) {
      scannerStatus = `Region deletion failed: ${String(error)}`;
    }
  }

  function skippedEntry(trigger: Trigger, material: string): Omit<HistoryEntry, 'id' | 'repeatCount'> {
    return {
      timestamp: new Date().toISOString(),
      trigger,
      material,
      rawValue: '-',
      parsedNumber: null,
      normalizedAttemptValue: null,
      matchedValue: null,
      nearestCandidateMaterial: null,
      nearestCandidateValue: null,
      delta: null,
      rockCount: null,
      confidence: null,
      otherCandidateMaterials: [],
      compositionStatus: 'unavailable',
      status: 'skipped',
      durationMs: 0,
    };
  }

  async function performScan(trigger: Trigger) {
    if (!captureRegion) {
      scannerStatus = 'Scan skipped: set a region first.';
      addHistory(skippedEntry(trigger, 'No region'));
      return;
    }
    if (isScanning) {
      queuedTrigger = queuedTrigger ?? trigger;
      scannerStatus = 'Scan queued: already scanning.';
      addHistory(skippedEntry(trigger, 'Already scanning'));
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
      const normalized = typeof result?.normalized_value === 'number' ? result.normalized_value : null;
      const normalizedSignature = result?.normalized_text || normalized?.toString() || rawValue.replace(/\D/g, '');
      const candidateMatchesBeforeFilters = normalized
        ? matchObservedValue(normalized, tolerance, { system: 'All', includeSalvage: true, includeFpsRoc: true })
        : [];
      const nextMatches = normalized ? matchObservedValue(normalized, tolerance, matchOptions()) : [];
      const unfilteredMatches = normalized && !settings.returnSalvageResults
        ? matchObservedValue(normalized, tolerance, { ...matchOptions(), includeSalvage: true })
        : nextMatches;
      const suppressedSalvage = !settings.returnSalvageResults
        && unfilteredMatches.length > 0
        && unfilteredMatches.every((match) => match.category?.toLowerCase() === 'salvage');
      if (suppressedSalvage) {
        matches = [];
        ocrError = null;
        lastScanTime = new Date().toLocaleTimeString();
        lastScanSummary = 'Salvage result hidden';
        scannerStatus = 'Salvage result filtered';
        return;
      }
      const nearest = normalized ? findNearestSignature(normalized, matchOptions()) : null;
      const resolvedScan = resolveScanResult(nextMatches);
      matches = nextMatches;
      ocrError = result?.error || null;
      lastScanTime = new Date().toLocaleTimeString();

      const status: ScanStatus = result?.error ? 'invalid' : nextMatches.length ? 'matched' : normalized ? 'no match' : 'invalid';
      const material = resolvedScan.primaryMatch
        ? materialLabel(resolvedScan.primaryMatch.material)
        : (result?.error || 'No match');
      const confidence = nextMatches[0]?.confidence ?? result?.confidence ?? null;
      lastScanSummary = status === 'matched' ? `${material} (${rawValue})` : `${status}: ${rawValue}`;
      scannerStatus = `${trigger} scan ${status}`;
      console.info('[SigLock] scan diagnostics', {
        trigger,
        activeRegionUsed: captureRegion,
        cropDimensions: result?.capture_width && result?.capture_height ? `${result.capture_width}x${result.capture_height}` : null,
        ocrRawText: result?.raw_text ?? '',
        normalizedSignatureText: normalizedSignature || null,
        parsedNumericSignature: normalized,
        candidateMaterialsBeforeFilters: candidateMatchesBeforeFilters.map((match) => `${match.material}:${match.expected}`),
        candidateMaterialsAfterFilters: nextMatches.map((match) => `${match.material}:${match.expected}`),
        finalSelectedMaterial: resolvedScan.primaryMatch?.material ?? null,
        unknownReason: status === 'matched'
          ? null
          : result?.error || (normalized ? 'No valid material remained after filters.' : 'OCR did not yield a parseable signature.'),
      });
      const timestamp = result?.scanned_at || new Date().toISOString();
      const baseHistory: Omit<HistoryEntry, 'id' | 'repeatCount'> = {
        timestamp,
        trigger,
        material,
        rawValue,
        parsedNumber: normalized,
        normalizedAttemptValue: normalizedSignature || null,
        matchedValue: nextMatches[0]?.expected ?? null,
        nearestCandidateMaterial: nearest?.material ?? null,
        nearestCandidateValue: nearest?.expected ?? null,
        delta: nearest?.delta ?? null,
        rockCount: nextMatches[0]?.rockCount ?? null,
        confidence,
        otherCandidateMaterials: resolvedScan.otherCandidates.map((match) => match.material),
        compositionStatus: resolvedScan.compositionStatus,
        status,
        durationMs,
      };

      if (status === 'matched') {
        const now = Date.now();
        const key = buildScanResultKey(nextMatches, normalizedSignature);
        if (lastAcceptedScan && isDuplicateResult(lastAcceptedScan, key, now)) {
          incrementDuplicate(lastAcceptedScan);
          scannerStatus = trigger === 'Manual' ? 'Duplicate suppressed' : `${trigger} scan duplicate suppressed`;
        } else {
          addOverlayMatches(resolvedScan, normalizedSignature);
          addHistory(baseHistory);
          lastAcceptedScan = {
            key,
            acceptedAt: now,
            historyId: history[0].id,
            overlayKeys: [`${normalizeMaterial(nextMatches[0].material)}|${nextMatches[0].rockCount}|${normalizedSignature}`],
          };
        }
      } else {
        addHistory(baseHistory);
        if (status === 'no match') {
          showOverlayRead('No match', `Read: ${normalizedSignature}`, nearest ? `nearest: ${nearest.material} ${nearest.expected}` : undefined);
        } else if (status === 'invalid') {
          showOverlayRead('Invalid read', `Raw: ${rawValue}`);
        }
      }
    } catch (error) {
      const message = `OCR scan failed: ${String(error)}`;
      ocrError = message;
      scannerStatus = message;
      addHistory({
        timestamp: new Date().toISOString(),
        trigger,
        material: message,
        rawValue: '-',
        parsedNumber: null,
        normalizedAttemptValue: null,
        matchedValue: null,
        nearestCandidateMaterial: null,
        nearestCandidateValue: null,
        delta: null,
        rockCount: null,
        confidence: null,
        otherCandidateMaterials: [],
        compositionStatus: 'unavailable',
        status: 'failed',
        durationMs: Math.round(performance.now() - started),
      });
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

  async function startMainWindowDrag(event: MouseEvent) {
    if (event.button !== 0 || event.detail > 1 || mainWindowDragActive) return;
    event.preventDefault();
    mainWindowDragActive = true;
    console.info('[SigLock] main window drag start');
    try {
      await getCurrentWindow().startDragging();
    } finally {
      endMainWindowDrag();
    }
  }

  function endMainWindowDrag() {
    if (!mainWindowDragActive) return;
    mainWindowDragActive = false;
    console.info('[SigLock] main window drag end');
  }

  function endMainWindowDragWhenHidden() {
    if (document.visibilityState === 'hidden') endMainWindowDrag();
  }

  function preventTitlebarDoubleClick(event: MouseEvent) {
    event.preventDefault();
    event.stopPropagation();
  }

  async function minimizeWindow() {
    await invoke('minimize_main_window');
  }

  async function closeApp() {
    await invoke('request_app_shutdown');
  }

  function displayVersion(version: string | null) {
    if (!version) return 'v...';
    return version.startsWith('v') ? version : `v${version}`;
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

  function shortcutBinding(action: ShortcutAction) {
    return action === 'manual' ? settings.scanNowKeybind : settings.toggleAutoScanKeybind;
  }

  function shortcutDefault(action: ShortcutAction) {
    return action === 'manual' ? DEFAULT_SETTINGS.scanNowKeybind : DEFAULT_SETTINGS.toggleAutoScanKeybind;
  }

  function shortcutMouseCommand(action: ShortcutAction) {
    return action === 'manual' ? 'set_scan_now_mouse_binding' : 'set_auto_toggle_mouse_binding';
  }

  function runShortcutAction(action: ShortcutAction) {
    if (action === 'manual') void performScan('Manual');
    else void toggleActiveScan();
  }

  function setShortcutBinding(action: ShortcutAction, binding: string) {
    if (action === 'manual') settings.scanNowKeybind = binding;
    else settings.toggleAutoScanKeybind = binding;
  }

  function otherShortcutBinding(action: ShortcutAction) {
    return action === 'manual' ? settings.toggleAutoScanKeybind : settings.scanNowKeybind;
  }

  async function applyShortcut(action: ShortcutAction, binding: string, previous: string) {
    if (binding === previous) return;
    if (binding === otherShortcutBinding(action)) {
      throw new Error('That shortcut is already assigned to another action.');
    }

    if (isMouseBinding(binding)) {
      await invoke(shortcutMouseCommand(action), { binding });
      try {
        if (!isMouseBinding(previous) && await isRegistered(previous)) await unregister(previous);
      } catch (error) {
        await invoke(shortcutMouseCommand(action), { binding: null });
        throw error;
      }
      return;
    }

    await register(binding, (event) => {
      if (event.state === 'Pressed' && !capturingKeybind) runShortcutAction(action);
    });
    try {
      await invoke(shortcutMouseCommand(action), { binding: null });
      if (!isMouseBinding(previous) && await isRegistered(previous)) await unregister(previous);
    } catch (error) {
      await unregister(binding).catch(() => {});
      if (!isMouseBinding(previous) && !await isRegistered(previous)) {
        await register(previous, (event) => {
          if (event.state === 'Pressed' && !capturingKeybind) runShortcutAction(action);
        }).catch(() => {});
      }
      throw error;
    }
  }

  async function applyScanKeybind(binding: string, previous: string) {
    await applyShortcut('manual', binding, previous);
  }

  async function registerSavedShortcut(action: ShortcutAction) {
    const binding = shortcutBinding(action);
    if (isMouseBinding(binding)) {
      await invoke(shortcutMouseCommand(action), { binding });
      return;
    }
    if (await isRegistered(binding)) await unregister(binding);
    await register(binding, (event) => {
      if (event.state === 'Pressed' && !capturingKeybind) runShortcutAction(action);
    });
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
    if (!capturingKeybind || !capturingShortcutAction) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.key === 'Escape') {
      capturingKeybind = false;
      capturingShortcutAction = null;
      return;
    }
    const key = keyName(event);
    const modifiers = [event.ctrlKey && 'Ctrl', event.altKey && 'Alt', event.shiftKey && 'Shift', event.metaKey && 'Super'].filter(Boolean);
    if (!key) {
      keybindError = 'That key is not supported. Press Escape to cancel.';
      return;
    }
    const next = [...modifiers, key].join('+');
    const action = capturingShortcutAction;
    try {
      await applyShortcut(action, next, shortcutBinding(action));
      setShortcutBinding(action, next);
      persistSettings();
      keybindError = null;
      scannerStatus = `${action === 'manual' ? 'Manual Scan' : 'Auto Scan'} shortcut set to ${next}`;
      capturingKeybind = false;
      capturingShortcutAction = null;
    } catch (error) {
      keybindError = `Could not register ${next}: ${String(error)}`;
    }
  }

  async function captureMouseKeybind(event: MouseEvent) {
    if (!capturingKeybind || !capturingShortcutAction) return;
    event.preventDefault();
    event.stopPropagation();
    const binding = event.button === 1 ? 'Middle Mouse' : event.button === 3 ? 'Mouse4' : event.button === 4 ? 'Mouse5' : null;
    if (!binding) {
      keybindError = event.button <= 2 ? 'Left click and right click cannot be bound.' : 'That mouse button is not supported.';
      return;
    }
    const action = capturingShortcutAction;
    try {
      await applyShortcut(action, binding, shortcutBinding(action));
      setShortcutBinding(action, binding);
      persistSettings();
      keybindError = null;
      scannerStatus = `${action === 'manual' ? 'Manual Scan' : 'Auto Scan'} shortcut set to ${binding}`;
      capturingKeybind = false;
      capturingShortcutAction = null;
    } catch (error) {
      keybindError = `Could not register ${binding}: ${String(error)}`;
    }
  }

  async function resetKeybind(action: ShortcutAction = 'manual') {
    try {
      const next = shortcutDefault(action);
      await applyShortcut(action, next, shortcutBinding(action));
      setShortcutBinding(action, next);
      persistSettings();
      keybindError = null;
    } catch (error) {
      keybindError = String(error);
    }
  }

  function keybindLabel(binding: string) {
    return binding.replace(/^Numpad([0-9])$/, 'Numpad $1');
  }

  function beginShortcutCapture(action: ShortcutAction) {
    capturingKeybind = true;
    capturingShortcutAction = action;
    keybindError = null;
    openSettingsSection = 'shortcuts';
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

  function operationalHistory() {
    return history.filter((entry) => entry.status === 'matched' || entry.status === 'failed').slice(0, 4);
  }

  function routineReadCount() {
    return history
      .filter((entry) => entry.status === 'invalid' || entry.status === 'no match' || entry.status === 'skipped')
      .reduce((total, entry) => total + entry.repeatCount, 0);
  }

  function signatureSummary(material: ReturnType<typeof getSignatures>['materials'][number]) {
    return material.signatures
      .filter((entry) => entry.value != null)
      .map((entry) => `${entry.rockCount}× ${entry.value}`)
      .join(' · ');
  }

  function historyTitle(entry: HistoryEntry) {
    if (isSystemError(entry)) return 'Scan error';
    if (entry.status === 'failed') return 'Scan failed';
    if (entry.status === 'invalid') return 'Invalid Number';
    if (entry.status === 'no match') return 'No match';
    if (entry.status === 'skipped') return entry.material === 'No region' ? 'Region not set' : 'Scan skipped';
    return entry.material;
  }

  function historyStatus(entry: HistoryEntry) {
    if (entry.status === 'failed') return 'Issue';
    if (entry.status === 'invalid') return 'Invalid Number';
    if (entry.status === 'no match') return 'No Match';
    if (entry.status === 'matched') return 'Match';
    return 'Skipped';
  }

  function triggerLabel(trigger: Trigger) {
    return trigger === 'Active' ? 'Auto' : trigger;
  }

  function isSystemError(entry: HistoryEntry) {
    return entry.status === 'failed' && /^OCR scan failed:/i.test(entry.material);
  }

  function scannerSummary() {
    if (!regionLoadComplete) return { value: 'Scanner Paused', detail: 'Loading capture region', tone: 'neutral' };
    if (!tesseractStatus) return { value: 'Scanner Paused', detail: 'Starting scanner', tone: 'neutral' };
    if (!tesseractStatus?.available) return { value: 'Scanner Error', detail: 'OCR unavailable', tone: 'danger' };
    if (activeScanOn) return { value: 'Auto Running', detail: `${settings.activeScanIntervalMs / 1000}s interval`, tone: 'active' };
    if (!captureRegion) return { value: 'Scanner Paused', detail: 'Set a capture region', tone: 'warning' };
    if (isScanning) return { value: 'Scanner Ready', detail: 'Reading capture region', tone: 'active' };
    return { value: 'Scanner Paused', detail: 'Ready when you are', tone: 'good' };
  }

  function autoSummary() {
    if (activeScanOn) return { value: 'Auto Running', detail: `${settings.activeScanIntervalMs / 1000}s interval`, tone: 'active' };
    return { value: 'Auto Off', detail: 'Manual scans available', tone: 'neutral' };
  }

  function lastScanSummaryCard() {
    const entry = history[0];
    if (!entry) return { value: 'Not scanned', detail: 'Ready when you are', tone: 'neutral' };
    const value = entry.status === 'matched' ? 'Matched'
      : entry.status === 'no match' ? 'No match'
      : entry.status === 'invalid' ? 'Invalid Read'
      : entry.status === 'failed' ? 'Failed'
      : 'Skipped';
    return {
      value,
      detail: `${new Date(entry.timestamp).toLocaleTimeString()} | ${entry.durationMs}ms`,
      tone: entry.status === 'matched' ? 'good' : entry.status === 'failed' || entry.status === 'invalid' ? 'danger' : 'neutral',
    };
  }

  function regionSize() {
    return captureRegion ? `${captureRegion.width}x${captureRegion.height}` : null;
  }

  function displayedCaptureRegion() {
    return newRegionDraft ? null : captureRegion;
  }

  function regionSummary() {
    const size = regionSize();
    if (!regionLoadComplete) return { value: 'Loading', detail: 'Checking saved region', tone: 'neutral', action: 'Set Region' };
    if (captureRegion && size) {
      const [width, height] = size.split('x').map((value) => Number.parseInt(value, 10));
      if (!width || !height) return { value: 'Invalid', detail: size, tone: 'danger', action: 'Fix Region' };
      return { value: 'Ready', detail: size, tone: 'good', action: 'Change Region' };
    }
    return { value: 'Missing', detail: 'No capture region', tone: 'warning', action: 'Set Region' };
  }

  function setIntervalSeconds(seconds: number) {
    settings.activeScanIntervalMs = seconds * 1000;
    void updateInterval();
  }

  function setSystemFilter(system: SystemFilter) {
    if (settings.selectedSystemFilter === system) return;
    settings.selectedSystemFilter = system;
    matches = [];
    overlayMatches = [];
    lastAcceptedScan = null;
    void publishOverlayMatches();
    persistSettings();
  }

  function applyResultSettings() {
    if (settings.onlyShowSolvedResults) {
      overlayMatches = overlayMatches.filter((match) => match.rockCount > 0);
    }
    if (!settings.returnSalvageResults) {
      overlayMatches = overlayMatches.filter((match) => !/^salvage$/i.test(match.material));
      history = history.filter((entry) => !(entry.status === 'matched' && /^salvage$/i.test(entry.material)));
    }
    if (!settings.includeFpsRocResults) {
      overlayMatches = overlayMatches.filter((match) => !/^(fps|roc mineable)$/i.test(match.material));
      history = history.filter((entry) => !(entry.status === 'matched' && /^(fps|roc mineable)(?:\s*\||$)/i.test(entry.material)));
    }
    void publishOverlayMatches();
    persistSettings();
  }

  function currentFinds() {
    return overlayMatches.filter((match) => match.rockCount > 0);
  }

  function historyDetail(entry: HistoryEntry) {
    const bits = [
      `${triggerLabel(entry.trigger)} scan`,
      new Date(entry.timestamp).toLocaleTimeString(),
      `${entry.durationMs}ms`,
    ];
    if (entry.rawValue && entry.rawValue !== '-') bits.push(`OCR: ${entry.rawValue}`);
    if (entry.normalizedAttemptValue) bits.push(`${entry.status === 'matched' ? 'matched' : 'attempted'}: ${entry.normalizedAttemptValue}`);
    if (entry.rockCount) bits.push(`${entry.rockCount} rocks`);
    if (entry.nearestCandidateMaterial && entry.status === 'no match') {
      bits.push(`nearest: ${entry.nearestCandidateMaterial} ${entry.nearestCandidateValue}, delta ${Math.abs(entry.delta ?? 0)}`);
    }
    if (entry.confidence !== null) bits.push(`${Math.round(entry.confidence * 100)}%`);
    if (entry.otherCandidateMaterials.length) bits.push(`other candidates: ${entry.otherCandidateMaterials.join(', ')}`);
    return bits.join(' | ');
  }

  async function setRegion() {
    scannerStatusBeforeRegionPicker = scannerStatus;
    scannerStatus = 'Opening region picker...';
    try {
      await invoke('open_region_picker');
    } catch (error) {
      scannerStatus = `Region picker failed: ${String(error)}`;
    }
  }

  async function clearRegion() {
    await invoke('clear_crop_region');
    captureRegion = null;
    regionProfiles = regionProfiles.map((profile) => profile.id === activeRegionId ? { ...profile, scanRegion: null } : profile);
    capturePreviewUrl = null;
    capturePreviewError = null;
    regionLoadComplete = true;
  }

  async function refreshCapturePreview(force = false) {
    if (!captureRegion) {
      capturePreviewUrl = null;
      capturePreviewError = null;
      return;
    }
    if (capturePreviewLoading || (!force && Date.now() - lastCapturePreviewAt < 500)) return;
    capturePreviewLoading = true;
    try {
      const result: any = await invoke('capture_region_preview');
      capturePreviewUrl = result?.preview_data_url ?? null;
      capturePreviewError = result?.success ? null : (result?.error || 'Capture unavailable');
      lastCapturePreviewAt = Date.now();
    } catch (error) {
      capturePreviewError = `Capture unavailable: ${String(error)}`;
    } finally {
      capturePreviewLoading = false;
    }
  }

  function openSettingsPane() {
    currentPage = 'settings';
    void refreshCapturePreview();
  }

  async function confirmRegionReset() {
    if (!captureRegion) return;
    if (!window.confirm(`Clear the OCR capture region for ${activeRegionProfile().name}? This removes its saved bounds and live preview. You can define it again at any time.`)) return;
    try {
      await clearRegion();
      scannerStatus = `${activeRegionProfile().name} OCR capture region cleared.`;
    } catch (error) {
      scannerStatus = `Region reset failed: ${String(error)}`;
    }
  }

  function navigatePage(page: AppPage) {
    currentPage = page;
    if (page === 'regions' || page === 'settings') void refreshCapturePreview();
  }

  function beginNewRegion() {
    currentPage = 'regions';
    newRegionDraft = true;
    newRegionName = '';
    capturePreviewUrl = null;
    capturePreviewError = null;
  }

  async function showCapturePreview() {
    currentPage = 'regions';
    await refreshCapturePreview(true);
  }

  function openSettings(section: typeof openSettingsSection) {
    openSettingsSection = section;
  }

  function mockOverlayMaterials(): { primary: string; compositionProfile: MaterialCompositionProfile | null } {
    const mockMatches = matchObservedValue(3840, 25, { system: 'All', includeSalvage: true });
    const mockResult = resolveScanResult(mockMatches);
    return {
      primary: mockResult.primaryMatch?.material ?? 'Aslarite',
      compositionProfile: mockResult.compositionProfile,
    };
  }

  async function checkForUpdates() {
    if (updating) return;
    updating = true;
    updateStatus = 'Checking...';
    try {
      const update = await check();
      if (!update) {
        updateStatus = "You're up to date.";
        return;
      }

      latestKnownVersion = update.version;
      latestKnownNotes = update.body ?? null;
      releaseNotes = mergeUpdaterNotes(update, releaseNotes);
      const shouldInstall = window.confirm(`SigLock ${displayVersion(update.version)} is available. Install it now?`);
      if (!shouldInstall) {
        updateStatus = `Update available: ${displayVersion(update.version)}`;
        return;
      }

      updateStatus = `Installing ${displayVersion(update.version)}...`;
      await update.downloadAndInstall();
      await relaunch();
    } catch (error) {
      updateStatus = "Couldn't check updates. Try again later.";
      console.warn('Update check failed', error);
    } finally {
      updating = false;
    }
  }

  function releaseItemsFromBody(body?: string | null) {
    if (!body) return [];
    return body
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter((line) => line && !/^<!--/.test(line))
      .map((line) => line
        .replace(/^#{1,6}\s+/, '')
        .replace(/^[-*+]\s+/, '')
        .replace(/^\d+\.\s+/, '')
        .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '$1')
        .trim())
      .filter(Boolean)
      .slice(0, 12);
  }

  function mergeUpdaterNotes(update: Update, existing: ReleaseNoteVersion[]) {
    const items = releaseItemsFromBody(update.body);
    if (!items.length) return existing;
    const next = {
      version: update.version,
      date: update.date,
      items,
      url: GITHUB_RELEASES_URL,
    };
    return [next, ...existing.filter((release) => release.version !== update.version)];
  }

  function normalizeReleaseVersion(tagName: string, name?: string) {
    return tagName || name || 'Release';
  }

  async function fetchGitHubReleaseNotes() {
    const response = await fetch(`${GITHUB_RELEASES_API}?per_page=6`, {
      headers: { Accept: 'application/vnd.github+json' },
    });
    if (!response.ok) throw new Error(`GitHub releases returned ${response.status}`);
    const releases = await response.json();
    if (!Array.isArray(releases)) return [];
    return releases
      .map((release: any) => ({
        version: normalizeReleaseVersion(String(release?.tag_name ?? ''), release?.name ? String(release.name) : undefined),
        date: release?.published_at ? String(release.published_at) : undefined,
        items: releaseItemsFromBody(release?.body ? String(release.body) : ''),
        url: release?.html_url ? String(release.html_url) : GITHUB_RELEASES_URL,
      }))
      .filter((release) => release.version && release.items.length);
  }

  async function fetchBundledReleaseNotes() {
    const response = await fetch('/CHANGELOG.md');
    if (!response.ok) return [];
    const body = await response.text();
    const items = releaseItemsFromBody(body);
    return items.length ? [{ version: 'Bundled changelog', items }] : [];
  }

  async function openReleaseNotes() {
    releaseNotesOpen = true;
    releaseNotesError = false;
    if (releaseNotes.length) return;
    releaseNotesLoading = true;
    try {
      const githubNotes = await fetchGitHubReleaseNotes();
      releaseNotes = latestKnownNotes && latestKnownVersion
        ? mergeUpdaterNotes({ version: latestKnownVersion, body: latestKnownNotes, date: undefined } as Update, githubNotes)
        : githubNotes;
      if (!releaseNotes.length) releaseNotes = await fetchBundledReleaseNotes();
      releaseNotesError = !releaseNotes.length;
    } catch (error) {
      console.warn('Release notes fetch failed', error);
      try {
        releaseNotes = await fetchBundledReleaseNotes();
      } catch (fallbackError) {
        console.warn('Bundled release notes fetch failed', fallbackError);
      }
      releaseNotesError = !releaseNotes.length;
    } finally {
      releaseNotesLoading = false;
    }
  }

  function openGitHubReleases() {
    void openUrl(GITHUB_RELEASES_URL);
  }

  onMount(async () => {
    document.addEventListener('keydown', captureKeybind, true);
    document.addEventListener('mousedown', captureMouseKeybind, true);
    document.addEventListener('mouseup', endMainWindowDrag, true);
    document.addEventListener('mouseleave', endMainWindowDrag, true);
    document.addEventListener('visibilitychange', endMainWindowDragWhenHidden);
    window.addEventListener('blur', endMainWindowDrag);
    unlisteners.push(await listen<ScanRegion>('crop-region-updated', (event) => {
      if (isValidRegion(event.payload)) {
        captureRegion = event.payload;
        regionProfiles = regionProfiles.map((profile) => profile.id === activeRegionId
          ? { ...profile, scanRegion: event.payload }
          : profile);
        regionLoadComplete = true;
        scannerStatus = 'Capture region saved';
        if (currentPage === 'regions' || currentPage === 'settings') void refreshCapturePreview();
      }
    }));
    unlisteners.push(await listen<GameLogStatus>('game-log-status-updated', (event) => {
      gameLogStatus = event.payload;
      if (!gameLogPathInput && event.payload.path) gameLogPathInput = event.payload.path;
    }));
    unlisteners.push(await listen('region-picker-cancelled', () => {
      scannerStatus = scannerStatusBeforeRegionPicker;
    }));
    await loadSavedRegion(true);
    try {
      appVersion = await getVersion();
      settings = await loadSettings();
      if (settings.scanNowKeybind === settings.toggleAutoScanKeybind) {
        settings.toggleAutoScanKeybind = DEFAULT_SETTINGS.toggleAutoScanKeybind;
        await saveSettings({ ...settings });
        keybindError = 'Duplicate shortcut detected; Auto Scan shortcut was reset.';
      }
    } catch (error) {
      keybindError = `Settings load warning; using safe defaults: ${String(error)}`;
      settings = { ...DEFAULT_SETTINGS };
    }
    try {
      await invoke('set_scan_interval', { intervalMs: settings.activeScanIntervalMs });
      await registerSavedShortcut('manual');
      await registerSavedShortcut('auto');
    } catch (error) {
      if (settings.scanNowKeybind !== DEFAULT_SETTINGS.scanNowKeybind || settings.toggleAutoScanKeybind !== DEFAULT_SETTINGS.toggleAutoScanKeybind) {
        try {
          await applyShortcut('manual', DEFAULT_SETTINGS.scanNowKeybind, settings.scanNowKeybind);
          await applyShortcut('auto', DEFAULT_SETTINGS.toggleAutoScanKeybind, settings.toggleAutoScanKeybind);
          settings.scanNowKeybind = DEFAULT_SETTINGS.scanNowKeybind;
          settings.toggleAutoScanKeybind = DEFAULT_SETTINGS.toggleAutoScanKeybind;
          await saveSettings({ ...settings });
          keybindError = 'Saved shortcut was unavailable; using defaults.';
        } catch (fallbackError) {
          keybindError = `Keybind registration warning: ${String(fallbackError)}`;
        }
      } else {
        keybindError = `Keybind registration warning: ${String(error)}`;
      }
    }

    try {
      tesseractStatus = await invoke('check_tesseract');
      const appState: any = await invoke('get_app_state');
      activeScanOn = !!appState?.active_scan_enabled;
      overlayVisible = !!appState?.overlay_visible;
      overlaySetupMode = !!appState?.overlay_setup_mode;
      gameLogStatus = await invoke<GameLogStatus>('get_game_log_status');
      gameLogPathInput = gameLogStatus.path ?? '';
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
    document.removeEventListener('mouseup', endMainWindowDrag, true);
    document.removeEventListener('mouseleave', endMainWindowDrag, true);
    document.removeEventListener('visibilitychange', endMainWindowDragWhenHidden);
    window.removeEventListener('blur', endMainWindowDrag);
    unlisteners.forEach((unlisten) => unlisten());
    if (saveTimer) clearTimeout(saveTimer);
  });
</script>

<svelte:head><title>SigLock</title></svelte:head>

<main class="app-shell">
  <aside class="side-rail">
    <div class="brand-lockup">
      <img src="/siglock-icon.png" alt="" />
      <div><strong>SigLock</strong><span>scan smarter<br />mine better</span></div>
    </div>
    <nav aria-label="Primary navigation">
      {#each [
        ['dashboard', '⌂', 'Dashboard'], ['overlay', '▣', 'Overlay'], ['regions', '⌖', 'Regions'],
        ['minables', '◈', 'Minables'], ['settings', '⚙', 'Settings'],
      ] as item}
        <button class:active={currentPage === item[0]} onclick={() => navigatePage(item[0] as AppPage)}>
          <span aria-hidden="true">{item[1]}</span>{item[2]}
        </button>
      {/each}
    </nav>
    <div class="rail-footer"><span>Star Citizen utility</span><b>v{displayVersion(appVersion)}</b></div>
  </aside>

  <div class="app-workspace">
    <header class="topbar">
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="drag-title" onmousedown={startMainWindowDrag} ondblclick={preventTitlebarDoubleClick}>
        <span>Star Citizen mining scanner utility</span><i></i>
      </div>
      <div class="connection-state"><i class:live={gameLogStatus.health === 'monitoring'}></i>{gameLogStatus.health === 'monitoring' ? 'Connected' : 'Telemetry offline'}</div>
      <button class="window-control" aria-label="Minimize" title="Minimize" onclick={minimizeWindow}>—</button>
      <button class="window-control close" aria-label="Close" title="Close" onclick={closeApp}>×</button>
    </header>

    <section class="profile-bar" aria-label="Active OCR region">
      <div class="profile-label"><span aria-hidden="true">✥</span><div><small>Active OCR</small><strong>Region</strong></div></div>
      <div class="ship-selector">
        {#each regionProfiles as profile}
          <button class:active={activeRegionId === profile.id} class:configured={!!profile.scanRegion} onclick={() => switchRegion(profile.id)}>{profile.name}</button>
        {/each}
      </div>
      <p>Built-in presets and custom OCR regions persist independently.</p>
      <nav class="system-selector" aria-label="Signature system filter">
        {#each ['All', 'Stanton', 'Pyro', 'Nyx'] as system}
          <button class:active={settings.selectedSystemFilter === system} onclick={() => setSystemFilter(system as SystemFilter)}>{system}</button>
        {/each}
      </nav>
    </section>

  {#if currentPage === 'dashboard'}
  <section class="dashboard-grid">
    <article class="module region-module">
      <header><div><small>OCR capture</small><h2>Region management</h2></div><button onclick={beginNewRegion}>New Region</button></header>
      <p class="module-copy">Screen area scanned for {activeRegionProfile().name} signatures.</p>
      <div class:active={!!captureRegion} class="region-entry">
        <i></i><div><strong>{activeRegionProfile().name} signature readout</strong><span>{captureRegion ? `x ${captureRegion.x} · y ${captureRegion.y} · ${captureRegion.width} × ${captureRegion.height}` : 'No capture area configured'}</span></div><b>{captureRegion ? 'Active' : 'Missing'}</b>
      </div>
      <div class="profile-region-list">
        {#each regionProfiles as profile}
          {#if profile.id !== activeRegionId}<div><i class:configured={!!profile.scanRegion}></i><span>{profile.name}</span><b>{profile.scanRegion ? 'Configured' : 'Not set'}</b></div>{/if}
        {/each}
      </div>
      <footer><button onclick={setRegion} disabled={!captureRegion}>Edit</button><button onclick={showCapturePreview} disabled={!captureRegion}>Capture preview</button><button class="danger-action" onclick={confirmRegionReset} disabled={!captureRegion}>Reset OCR region</button></footer>
    </article>

    <article class="module scanner-module">
      <header><div><h2>Scanner state</h2></div><span class="state-dot {scannerSummary().tone}">{scannerSummary().value}</span></header>
      <div class="auto-control"><div class="scan-orbit" class:running={activeScanOn}><span></span></div><div><small>Auto-Scan</small><strong>{activeScanOn ? 'ON' : 'OFF'}</strong></div><button role="switch" aria-label="Toggle Auto-Scan" aria-checked={activeScanOn} class:active={activeScanOn} onclick={toggleActiveScan}><i></i></button></div>
      <div class="scan-metrics"><div><strong>{settings.activeScanIntervalMs / 1000}s</strong><span>Interval</span></div><div><strong>{currentFinds().length}</strong><span>Current finds</span></div><div><strong>{lastScanSummaryCard().value}</strong><span>Last result</span></div></div>
      <footer><button class="primary" onclick={() => performScan('Manual')} disabled={isScanning || !captureRegion}>{isScanning ? 'Scanning…' : 'Scan now'}</button><span class:visible={overlayVisible}><i></i>HUD {overlayVisible ? 'visible' : 'hidden'}</span></footer>
    </article>

    <article class="module watch-module">
      <header><div><h2>Minables</h2></div><button class="text-action" onclick={() => navigatePage('minables')}>View all →</button></header>
      <div class="watch-list">
        {#each visibleMinables().filter((row) => isWatched(row.material)).slice(0, 5) as row}<button onclick={() => toggleWatch(row.material)}><i class:detected={currentFinds().some((find) => normalizeMaterial(find.material) === normalizeMaterial(row.material))}></i><span>{row.material}</span><b>{currentFinds().some((find) => normalizeMaterial(find.material) === normalizeMaterial(row.material)) ? 'Detected' : 'Watching'}</b></button>{/each}
        {#if !settings.watchedMaterials.length}<div class="compact-empty">No watched materials. Add them from Minables.</div>{/if}
      </div>
    </article>

    <article class="module finds-module">
      <header><div><h2>Current materials</h2></div><span>{currentFinds().length} detected</span></header>
      <div class="material-results">
        {#if currentFinds().length}
          {#each currentFinds().slice(0, 5) as match}<div><i></i><strong>{materialLabel(match.material)}</strong><span>{match.rockCount} rock{match.rockCount === 1 ? '' : 's'}</span><b>{match.valueLabel ?? match.detailLabel ?? 'Matched'}</b></div>{/each}
        {:else}<div class="results-empty"><span aria-hidden="true">◇</span><strong>Waiting for a recognized signature</strong><p>Matched materials appear here; routine invalid OCR reads stay out of the way.</p></div>{/if}
      </div>
      <footer class="activity-summary"><span>{operationalHistory().length} significant events</span>{#if routineReadCount()}<span>{routineReadCount()} routine misses consolidated</span>{/if}<button onclick={() => { openSettings('advanced'); navigatePage('settings'); }}>Diagnostics →</button></footer>
    </article>

    <article class="module overlay-dashboard">
      <header><div><h2>Overlay preview</h2></div><button onclick={() => navigatePage('overlay')}>Configure overlay</button></header>
      <div class="overlay-stage" style={`--preview-text:${settings.overlayTextColor};--preview-bg:${settings.overlayBackgroundColor};--preview-accent:${settings.overlayAccentColor};--preview-opacity:${settings.overlayOpacity};--preview-size:${settings.overlayFontSize}px`}>
        <div class="hud-preview live-preview"><header><strong>SIGLOCK</strong><span><i class:online={activeScanOn}></i>AUTO</span></header>{#if currentFinds().length}{#each currentFinds().slice(0, 3) as match}<p><span>{match.material}</span>{#if isWatched(match.material)}<i class="watch-demo"></i>{/if}</p>{/each}{:else}<p class="hud-empty">Results appear here</p>{/if}</div>
      </div>
      <footer><span class:visible={overlayVisible}><i></i>{overlayVisible ? 'Visible in game' : 'Hidden'}</span><button onclick={toggleOverlay}>{overlayVisible ? 'Hide HUD' : 'Show HUD'}</button></footer>
    </article>
  </section>
  {:else if currentPage === 'regions'}
    <section class="workspace-page regions-workspace">
      <header class="workspace-heading"><div><small>Capture geometry</small><h1>{newRegionDraft ? 'New region' : activeRegionProfile().name}</h1></div><span class="eyebrow">{newRegionDraft ? 'Not saved' : regionSummary().value}</span></header>
      <div class="region-create-row"><input aria-label="Region name" maxlength="40" placeholder="Name this new region" bind:value={newRegionName} /><button onclick={createCustomRegion} disabled={!newRegionName.trim()}>{newRegionDraft ? 'Save new region' : 'Add region'}</button><button class="danger-action" onclick={deleteActiveRegion} disabled={newRegionDraft || activeRegionProfile().builtIn}>Delete selected</button></div>
      <div class="region-layout">
        <section class="instrument-panel region-instrument">
          <div class="instrument-title"><span>Saved region</span><b>{displayedCaptureRegion() ? `${displayedCaptureRegion()!.width} × ${displayedCaptureRegion()!.height}` : 'Not configured'}</b></div>
          <div class="region-readout">
            {#if displayedCaptureRegion()}
              <dl><div><dt>X</dt><dd>{displayedCaptureRegion()!.x}</dd></div><div><dt>Y</dt><dd>{displayedCaptureRegion()!.y}</dd></div><div><dt>Width</dt><dd>{displayedCaptureRegion()!.width}</dd></div><div><dt>Height</dt><dd>{displayedCaptureRegion()!.height}</dd></div></dl>
            {:else}<p>{newRegionDraft ? 'Save a name to begin a clean OCR capture region.' : 'Select the scan-number area for this OCR region.'}</p>{/if}
          </div>
          <div class="button-row"><button class="primary" onclick={setRegion} disabled={newRegionDraft}>{captureRegion ? 'Edit region' : 'Set region'}</button><button class="danger-action" onclick={confirmRegionReset} disabled={!captureRegion || newRegionDraft}>Reset OCR region</button><button onclick={() => refreshCapturePreview(true)} disabled={!captureRegion || newRegionDraft}>Refresh preview</button></div>
        </section>
        <section class="instrument-panel preview-instrument">
          <div class="instrument-title"><span>Live crop</span><b>{capturePreviewError ?? 'OCR input'}</b></div>
          {#if capturePreviewUrl}<img class="capture-preview large" src={capturePreviewUrl} alt="Live preview of the active ship capture region" />{:else}<div class="preview-empty">{displayedCaptureRegion() ? 'Refresh to inspect the current crop.' : newRegionDraft ? 'No capture region has been created yet.' : 'No region stored for this profile.'}</div>{/if}
        </section>
      </div>
    </section>
  {:else if currentPage === 'minables'}
    <section class="workspace-page">
      <header class="workspace-heading"><div><small>Canonical signature reference</small><h1>Minables</h1></div><span class="eyebrow">{gameLogStatus.currentLocation ? locationLabel(gameLogStatus.currentLocation) : 'Location unknown'}</span></header>
      <div class="table-toolbar"><input aria-label="Search minables" placeholder="Filter material or location" bind:value={minableSearch} /><span>{visibleMinables().length} materials · {settings.watchedMaterials.length} watched</span></div>
      {#if settings.watchedMaterials.length}<div class="minables-watchlist" aria-label="Watched materials"><strong>Watchlist</strong>{#each visibleMinables().filter((row) => isWatched(row.material)) as row}<button class="watch-action watched" onclick={() => toggleWatch(row.material)}>{row.material} <span>Remove</span></button>{/each}</div>{/if}
      <div class="data-table minables-table" role="table" aria-label="Minable location reference">
        <div class="table-head" role="row"><span>Material</span><span>Class</span><span>Signature</span><span>Known locations</span><span>Here</span><span>Watch</span></div>
        {#each visibleMinables() as row}
          <div class:valid-here={!!gameLogStatus.currentLocation && materialValidAtLocation(row.material, gameLogStatus.currentLocation)} class="table-row" role="row">
            <strong>{row.material}</strong><span>{row.category ?? 'Mineable'}</span><span class="mono">{signatureSummary(row)}</span><span>{row.locations.length ? row.locations.map(locationLabel).join(' · ') : 'No location restriction published'}</span>
            <span class="validity">{locationValidity(row)}</span>
            <button class:watched={isWatched(row.material)} class="watch-action" aria-label={`${isWatched(row.material) ? 'Unwatch' : 'Watch'} ${row.material}`} onclick={() => toggleWatch(row.material)}><i></i>{isWatched(row.material) ? 'Watched' : 'Watch'}</button>
          </div>
        {/each}
      </div>
    </section>
  {:else if currentPage === 'overlay'}
    <section class="workspace-page overlay-workspace">
      <header class="workspace-heading"><div><small>HUD augmentation</small><h1>Overlay</h1></div><span class="eyebrow">{overlayVisible ? 'Visible' : 'Hidden'}</span></header>
      <div class="overlay-config-grid">
        <section class="instrument-panel"><div class="instrument-title"><span>Positioning</span><b>{activeRegionProfile().name} profile</b></div><div class="button-row"><button class:active={overlaySetupMode} onclick={toggleOverlaySetupMode}>{overlaySetupMode ? 'Lock position' : 'Unlock position'}</button><button onclick={resetOverlayPosition}>Reset</button><button onclick={toggleOverlay}>{overlayVisible ? 'Hide' : 'Show'}</button></div></section>
        <section class="instrument-panel hud-preview-panel"><div class="instrument-title"><span>HUD preview</span><b>Minimal mode</b></div><div class="hud-preview" style={`--preview-text:${settings.overlayTextColor};--preview-bg:${settings.overlayBackgroundColor};--preview-accent:${settings.overlayAccentColor};--preview-opacity:${settings.overlayOpacity};--preview-size:${settings.overlayFontSize}px`}><header><strong>SIGLOCK</strong><span><i class:online={activeScanOn}></i>AUTO</span></header><p>Hadinite</p><p>Aphorite <i class="watch-demo"></i></p><p>Dolivine</p></div></section>
      </div>
      <section class="instrument-panel overlay-settings-panel"><div class="instrument-title"><span>Overlay configuration</span><b>Saved automatically</b></div><div class="appearance-grid">
        <label>Text color <input type="color" bind:value={settings.overlayTextColor} onchange={persistSettings} /></label><label>Background <input type="color" bind:value={settings.overlayBackgroundColor} onchange={persistSettings} /></label><label>Accent <input type="color" bind:value={settings.overlayAccentColor} onchange={persistSettings} /></label><label>Opacity <strong>{Math.round(settings.overlayOpacity * 100)}%</strong><input type="range" min="0" max="1" step="0.05" bind:value={settings.overlayOpacity} onchange={persistSettings} /></label><label>Text size <strong>{settings.overlayFontSize}px</strong><input type="range" min="11" max="20" step="1" bind:value={settings.overlayFontSize} onchange={persistSettings} /></label><label>Result lifetime <strong>{settings.overlayResultLifetimeSeconds}s</strong><input type="range" min="5" max="120" step="5" bind:value={settings.overlayResultLifetimeSeconds} onchange={persistSettings} /></label><label class="toggle"><input type="checkbox" bind:checked={settings.overlayHighContrast} onchange={persistSettings} /><span></span> High contrast</label><label class="toggle"><input type="checkbox" bind:checked={settings.overlayCompactMode} onchange={persistSettings} /><span></span> Compact mode</label><label class="toggle"><input type="checkbox" bind:checked={settings.returnSalvageResults} onchange={applyResultSettings} /><span></span> Salvage</label><label class="toggle"><input type="checkbox" bind:checked={settings.includeFpsRocResults} onchange={applyResultSettings} /><span></span> FPS/ROC</label><label class="toggle"><input type="checkbox" bind:checked={settings.showComposition} onchange={applyResultSettings} /><span></span> Show Composition</label><label class="toggle"><input type="checkbox" bind:checked={settings.showScannedValueOnOverlay} onchange={persistSettings} /><span></span> Signature Value</label><label class="toggle"><input type="checkbox" bind:checked={settings.onlyShowSolvedResults} onchange={applyResultSettings} /><span></span> Only solved captures in overlay</label>
      </div></section>
    </section>
  {/if}

  {#if currentPage === 'settings'}
    <section class="workspace-page settings-page" aria-label="Settings">
      <div class="settings-header">
        <div><small>Application preferences</small><h1>Settings</h1><p>Scanner controls, shortcuts, and diagnostics.</p></div>
      </div>

      <section class="settings-section">
        <button class="accordion-header" class:open={openSettingsSection === 'shortcuts'} aria-expanded={openSettingsSection === 'shortcuts'} aria-controls="settings-shortcuts" onclick={() => openSettings('shortcuts')}>
          <span class="accordion-heading"><strong>Shortcuts</strong><small>Configure keyboard and mouse shortcuts.</small></span>
          <span class="accordion-indicator" aria-hidden="true"></span>
        </button>
        {#if openSettingsSection === 'shortcuts'}
          <div class="accordion-body" id="settings-shortcuts">
            <div class="shortcut-row">
              <strong>Manual Scan</strong>
              <kbd>{capturingShortcutAction === 'manual' ? 'Press a key or mouse button...' : keybindLabel(settings.scanNowKeybind)}</kbd>
              <button onclick={() => beginShortcutCapture('manual')}>Set Shortcut</button>
              <button onclick={() => resetKeybind('manual')}>Reset</button>
            </div>
            <div class="shortcut-row">
              <strong>Toggle Auto Scan</strong>
              <kbd>{capturingShortcutAction === 'auto' ? 'Press a key or mouse button...' : keybindLabel(settings.toggleAutoScanKeybind)}</kbd>
              <button onclick={() => beginShortcutCapture('auto')}>Set Shortcut</button>
              <button onclick={() => resetKeybind('auto')}>Reset</button>
            </div>
            <p class="hint">{keybindError ? keybindWarning(keybindError) : 'Manual Scan triggers one read. Toggle Auto Scan starts or stops the scan loop.'}</p>
          </div>
        {/if}
      </section>

      <section class="settings-section">
        <button class="accordion-header" class:open={openSettingsSection === 'scan'} aria-expanded={openSettingsSection === 'scan'} aria-controls="settings-scan" onclick={() => openSettings('scan')}>
          <span class="accordion-heading"><strong>Scan</strong><small>Configure scanner behavior, timing and detection.</small></span>
          <span class="accordion-indicator" aria-hidden="true"></span>
        </button>
        {#if openSettingsSection === 'scan'}
          <div class="accordion-body" id="settings-scan">
            <div class="button-row">
              <input class="signature-input" aria-label="Test signature" placeholder="Test signature" bind:value={observed} onkeydown={(event) => event.key === 'Enter' && runManualMatch()} />
              <button onclick={runManualMatch}>Match Value</button>
            </div>
            <div class="field log-path-field">
              <label for="game-log-path">Game.log path</label>
              <div class="button-row">
                <input id="game-log-path" bind:value={gameLogPathInput} placeholder="Auto-discovery is active" />
                <button onclick={saveGameLogPath}>Use path</button>
              </div>
              <small>{gameLogStatus.path ? `${gameLogStatus.channel ?? 'Channel'} · ${gameLogStatus.health}` : 'Select a StarCitizen folder, channel folder, or Game.log only if discovery fails.'}</small>
            </div>
            <div class="interval-control"><span>Interval</span><div class="segment-group">{#each [1, 2, 3, 4] as seconds}<button class:active={settings.activeScanIntervalMs === seconds * 1000} onclick={() => setIntervalSeconds(seconds)}>{seconds}s</button>{/each}</div></div>
            <div class="button-row">
              <button class="danger-action" onclick={confirmRegionReset} disabled={!captureRegion}>Reset OCR Region</button>
              <button onclick={showCapturePreview} disabled={!captureRegion || capturePreviewLoading}>{capturePreviewLoading ? 'Refreshing...' : 'Open Preview'}</button>
            </div>
            <div class="capture-preview-card">
              <div><strong>Capture Preview</strong>{#if captureRegion}<span>{captureRegion.width}x{captureRegion.height} saved region</span>{/if}</div>
              {#if !captureRegion}
                <p class="region-missing">Region missing — set a capture region to preview it.</p>
              {:else if capturePreviewUrl}
                <img class="capture-preview" src={capturePreviewUrl} alt="Live preview of the saved capture region" />
              {:else}
                <p>{capturePreviewError ?? 'Loading saved region preview…'}</p>
              {/if}
            </div>
          </div>
        {/if}
      </section>

      <section class="settings-section">
          <button class="accordion-header" class:open={openSettingsSection === 'advanced'} aria-expanded={openSettingsSection === 'advanced'} aria-controls="settings-advanced" onclick={() => openSettings('advanced')}>
            <span class="accordion-heading"><strong>Advanced Debug</strong><small>Diagnostics, OCR and troubleshooting options.</small></span>
            <span class="accordion-indicator" aria-hidden="true"></span>
          </button>
          {#if openSettingsSection === 'advanced'}
          <div class="accordion-body" id="settings-advanced">
          <label class="field">Tolerance <input type="number" min="0" max="200" bind:value={tolerance} /></label>
          <pre>{JSON.stringify({ tesseractStatus, debugResult, ocrError, overlayError, keybindError, scannerStatus, captureRegion, lastScanSummary, lastScanTime }, null, 2)}</pre>
          <section class="diagnostic-feed">
            <header><div><small>Raw troubleshooting data</small><h2>Scan diagnostics</h2></div><div class="filter-group" aria-label="Filter scan results"><button class:active={historyFilter === 'all'} onclick={() => historyFilter = 'all'}>All</button><button class:active={historyFilter === 'matches'} onclick={() => historyFilter = 'matches'}>Matches</button><button class:active={historyFilter === 'issues'} onclick={() => historyFilter = 'issues'}>Issues</button></div></header>
            {#each visibleHistory().slice(0, 12) as entry (entry.id)}<div class="history-row"><span class="status {entry.status}">{historyStatus(entry)}</span><div><strong>{historyTitle(entry)}</strong><small>{historyDetail(entry)}</small></div>{#if entry.repeatCount > 1}<b>x{entry.repeatCount}</b>{/if}</div>{/each}
            {#if !history.length}<p class="compact-empty">No diagnostic events captured.</p>{/if}
          </section>
          </div>
          {/if}
      </section>

      <footer class="settings-footer">
        <span>Current version: {displayVersion(appVersion)}</span>
        <button onclick={checkForUpdates} disabled={updating}>{updating ? 'Checking...' : 'Check for Updates'}</button>
        {#if updateStatus}<p class="update-message">{updateStatus}</p>{/if}
        <button class="link-button" onclick={openReleaseNotes}>Release Notes</button>
      </footer>
    </section>
  {/if}

  </div>

  {#if releaseNotesOpen}
    <div class="modal-backdrop" role="presentation" onclick={() => releaseNotesOpen = false}></div>
    <div class="release-modal" role="dialog" aria-modal="true" aria-labelledby="release-notes-title">
      <div class="settings-header">
        <div>
          <h2 id="release-notes-title">Release Notes</h2>
          <p>Current version: {displayVersion(appVersion)}{#if latestKnownVersion} | Latest: {displayVersion(latestKnownVersion)}{/if}</p>
        </div>
        <button class="window-control" aria-label="Close release notes" onclick={() => releaseNotesOpen = false}>X</button>
      </div>

      {#if releaseNotesLoading}
        <div class="release-empty">Loading release notes...</div>
      {:else if releaseNotesError}
        <div class="release-empty">
          <strong>Release notes are unavailable right now.</strong>
          <button onclick={openGitHubReleases}>GitHub Releases</button>
        </div>
      {:else}
        <div class="release-list">
          {#each releaseNotes as release}
            <article>
              <header>
                <h3>{displayVersion(release.version)}</h3>
                {#if release.date}<time datetime={release.date}>{new Date(release.date).toLocaleDateString()}</time>{/if}
              </header>
              <ul>
                {#each release.items as item}
                  <li>{item}</li>
                {/each}
              </ul>
            </article>
          {/each}
        </div>
        <div class="release-actions">
          <button onclick={openGitHubReleases}>GitHub Releases</button>
        </div>
      {/if}
    </div>
  {/if}
</main>
