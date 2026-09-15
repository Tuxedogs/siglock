<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { loadSettingsReadOnly, type SigLockSettings } from '$lib/settings';
  import {
    resolveRockComposition,
    type CompositionEntry,
    type CompositionStatus,
    type MaterialCompositionProfile,
  } from '$lib/data/rockCompositions';

  type OverlayMatch = {
    key: string;
    material: string;
    compositionProfile?: MaterialCompositionProfile | null;
    otherCandidates?: string[];
    compositionStatus?: CompositionStatus;
    rockCount: number;
    valueLabel?: string;
    detailLabel?: string;
    repeatCount: number;
    updatedAt: string;
    watched?: boolean;
    detected?: boolean;
  };

  let settings = $state<SigLockSettings | null>(null);
  let matches = $state<OverlayMatch[]>([]);
  let setupMode = $state(false);
  let activeScanOn = $state(false);
  let now = $state(Date.now());
  let unlisteners: UnlistenFn[] = [];
  let expiryTimer: ReturnType<typeof setInterval> | null = null;

  async function anchorOverlay(event: MouseEvent) {
    event.stopPropagation();
    setupMode = await invoke<boolean>('set_overlay_setup_mode', { enabled: false });
  }

  const rgba = (hex: string, opacity: number) => {
    const value = Number.parseInt(hex.slice(1), 16);
    return `rgba(${value >> 16}, ${(value >> 8) & 255}, ${value & 255}, ${opacity})`;
  };

  $effect(() => {
    if (!settings) return;
    const root = document.documentElement;
    root.style.setProperty('--result-text', settings.overlayTextColor);
    root.style.setProperty('--result-bg', rgba(settings.overlayBackgroundColor, settings.overlayOpacity));
    root.style.setProperty('--result-accent', settings.overlayAccentColor);
    root.style.setProperty('--result-font-size', `${settings.overlayFontSize}px`);
    root.dataset.highContrast = String(settings.overlayHighContrast);
    root.dataset.compact = String(settings.overlayCompactMode);
  });

  let visibleMatches = $derived(matches.filter((match) => {
    if (!settings) return true;
    if (settings.onlyShowSolvedResults && match.rockCount <= 0) return false;
    return now - new Date(match.updatedAt).getTime() < settings.overlayResultLifetimeSeconds * 1000;
  }).slice(0, 3));

  function mockPreviewMatch(): OverlayMatch {
    const composition = resolveRockComposition('Agricium');
    return {
      key: 'overlay-preview',
      material: 'Agricium',
      compositionProfile: composition.compositionProfile,
      otherCandidates: [],
      compositionStatus: composition.compositionStatus,
      rockCount: 1,
      valueLabel: '3840',
      detailLabel: 'Solved signature',
      repeatCount: 1,
      updatedAt: new Date().toISOString(),
      watched: true,
      detected: true,
    };
  }

  function compositionPercent(entry: CompositionEntry) {
    const format = (value: number) => Number.isInteger(value) ? String(value) : value.toFixed(1);
    return `${format(entry.percentMin)}-${format(entry.percentMax)}%`;
  }

  let displayedMatches = $derived(setupMode ? [mockPreviewMatch()] : visibleMatches);

  onMount(async () => {
    settings = await loadSettingsReadOnly();
    unlisteners.push(await listen<SigLockSettings>('overlay-settings-updated', (event) => settings = event.payload));
    unlisteners.push(await listen<{ matches: OverlayMatch[] }>('overlay-result-updated', (event) => matches = event.payload.matches));
    unlisteners.push(await listen<boolean>('overlay-setup-mode-changed', (event) => setupMode = event.payload));
    unlisteners.push(await listen<boolean>('active-scan-toggled', (event) => activeScanOn = event.payload));
    setupMode = await invoke<boolean>('get_overlay_setup_mode');
    const appState = await invoke<{ active_scan_enabled?: boolean }>('get_app_state');
    activeScanOn = !!appState.active_scan_enabled;
    expiryTimer = setInterval(() => now = Date.now(), 1000);
  });

  onDestroy(() => {
    unlisteners.forEach((unlisten) => unlisten());
    if (expiryTimer) clearInterval(expiryTimer);
  });
</script>

{#if setupMode || displayedMatches.length}
<div class="overlay-shell">
  {#if setupMode}
    <div class="setup-handle" data-tauri-drag-region>
      <span data-tauri-drag-region>Overlay position</span>
      <button onclick={anchorOverlay}>Anchor</button>
    </div>
  {/if}
  <header class="hud-header">
    <strong>SIGLOCK</strong>
    <span><i class:online={activeScanOn}></i>AUTO</span>
  </header>
  {#if displayedMatches.length}
    <div class="matches">
      {#each displayedMatches as match (match.key)}
        <div class="match-item">
          <p><span>{match.material}</span>{#if match.watched}<i class:pulsing={match.detected} class="watch-dot" aria-label="Watched material"></i>{/if}</p>
          {#if settings?.showScannedValueOnOverlay && (match.valueLabel || match.detailLabel)}
            <small>{match.valueLabel || match.detailLabel}</small>
          {/if}
          {#if settings?.showComposition && match.rockCount > 0 && match.compositionProfile?.entries?.length}
            <div class="composition-block">
              <em>Trace materials</em>
              {#each match.compositionProfile.entries as entry}
                <span>{entry.displayName} {compositionPercent(entry)}</span>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {:else if setupMode}
    <p>Results appear here</p>
  {/if}
</div>
{/if}

<style>
  :global(html), :global(body) {
    width: max-content;
    height: max-content;
    margin: 0;
    overflow: hidden;
    background: transparent !important;
  }
  :global(body) {
    color: var(--result-text, #e5e7eb);
    font: var(--result-font-size, 13px)/1.3 "Bahnschrift", "Arial Narrow", system-ui, sans-serif;
  }
  :global(:root[data-high-contrast="true"] body) {
    text-shadow: 0 1px 2px #000, 0 0 5px #000;
  }
  .overlay-shell {
    width: max-content;
    min-width: 172px;
    max-width: 260px;
    padding: 7px 10px 8px;
    background: linear-gradient(90deg, color-mix(in srgb, var(--result-bg, #061019) 18%, transparent), transparent 84%);
    border-top: 1px solid color-mix(in srgb, var(--result-accent, #48b9d6) 58%, transparent);
    border-left: 1px solid color-mix(in srgb, var(--result-accent, #48b9d6) 45%, transparent);
    clip-path: polygon(0 0, calc(100% - 9px) 0, 100% 9px, 100% 100%, 7px 100%, 0 calc(100% - 7px));
    filter: drop-shadow(0 1px 2px rgba(0, 0, 0, .65));
  }
  .hud-header { display: flex; align-items: center; justify-content: space-between; gap: 20px; margin-bottom: 5px; padding-bottom: 4px; border-bottom: 1px solid color-mix(in srgb, var(--result-accent, #48b9d6) 28%, transparent); }
  .hud-header strong { color: color-mix(in srgb, var(--result-accent, #48b9d6) 88%, white); font-size: .68em; letter-spacing: .18em; }
  .hud-header span { display: flex; align-items: center; gap: 5px; color: color-mix(in srgb, var(--result-text, #e5e7eb) 72%, transparent); font: 600 .61em/1 ui-monospace, monospace; letter-spacing: .11em; }
  .hud-header i { width: 5px; height: 5px; border-radius: 50%; background: #ef665f; box-shadow: 0 0 5px rgba(239, 102, 95, .48); }
  .hud-header i.online { background: #61d993; box-shadow: 0 0 5px rgba(97, 217, 147, .5); }
  .setup-handle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-width: 180px;
    padding: 1px 0 4px;
    color: var(--result-accent, #3b82f6);
    border-bottom: 1px solid color-mix(in srgb, var(--result-accent, #3b82f6) 45%, transparent);
    cursor: move;
    font-size: .75em;
    font-weight: 700;
    letter-spacing: .08em;
    text-transform: uppercase;
  }
  .setup-handle button {
    padding: 1px 5px;
    color: var(--result-text, #e5e7eb);
    background: transparent;
    border: 1px solid color-mix(in srgb, var(--result-accent, #3b82f6) 60%, transparent);
    border-radius: 2px;
    cursor: pointer;
    font: inherit;
    text-transform: uppercase;
  }
  .match-item {
    min-width: 0;
    padding: 1px 0;
  }
  .match-item p {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-width: 0;
    margin: 0;
    white-space: nowrap;
  }
  .match-item p > span { overflow: hidden; text-overflow: ellipsis; letter-spacing: .015em; }
  .watch-dot { flex: 0 0 5px; width: 5px; height: 5px; border: 1px solid color-mix(in srgb, var(--result-accent, #48b9d6) 78%, white); border-radius: 50%; background: transparent; }
  .watch-dot.pulsing { background: color-mix(in srgb, var(--result-accent, #48b9d6) 82%, white); animation: watch-pulse 1.8s ease-in-out infinite; }
  .match-item small {
    display: block;
    margin-top: -1px;
    color: color-mix(in srgb, var(--result-text, #e5e7eb) 72%, transparent);
    font: 700 .78em ui-monospace, monospace;
  }
  .composition-block {
    display: grid;
    gap: 1px;
    margin-top: 3px;
    padding-left: 15px;
  }
  .composition-block em {
    color: color-mix(in srgb, var(--result-text, #e5e7eb) 66%, transparent);
    font-size: .72em;
    font-style: normal;
    letter-spacing: .06em;
    text-transform: uppercase;
  }
  .composition-block span {
    color: color-mix(in srgb, var(--result-text, #e5e7eb) 82%, transparent);
    font: 600 .76em/1.25 ui-monospace, monospace;
    white-space: nowrap;
  }
  .overlay-shell > p { margin: 5px 0 0; color: color-mix(in srgb, var(--result-text, #e5e7eb) 65%, transparent); font-size: .8em; }
  :global(:root[data-compact="true"]) .overlay-shell { padding: 3px 5px; }
  :global(:root[data-compact="true"]) .match-item { padding: 0; }
  @keyframes watch-pulse { 0%, 100% { opacity: .48; transform: scale(.82); } 50% { opacity: 1; transform: scale(1.18); } }
  @media (prefers-reduced-motion: reduce) { .watch-dot.pulsing { animation: none; opacity: 1; } }
</style>
