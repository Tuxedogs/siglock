<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { loadSettingsReadOnly, type SigLockSettings } from '$lib/settings';
  import { resolveRockComposition, type CompositionStatus } from '$lib/data/rockCompositions';

  type OverlayMatch = {
    key: string;
    material: string;
    secondaryMaterials?: string[];
    otherCandidates?: string[];
    compositionStatus?: CompositionStatus;
    rockCount: number;
    valueLabel?: string;
    detailLabel?: string;
    repeatCount: number;
    updatedAt: string;
  };

  let settings = $state<SigLockSettings | null>(null);
  let matches = $state<OverlayMatch[]>([]);
  let setupMode = $state(false);
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
    const composition = resolveRockComposition('Aslarite', 'All');
    return {
      key: 'overlay-preview',
      material: 'Aslarite',
      secondaryMaterials: composition.secondaryMaterials,
      otherCandidates: [],
      compositionStatus: composition.compositionStatus,
      rockCount: 1,
      valueLabel: '3840',
      detailLabel: 'Solved signature',
      repeatCount: 1,
      updatedAt: new Date().toISOString(),
    };
  }

  function materialLabel(match: OverlayMatch): string {
    return settings?.showSecondaryMaterials && match.secondaryMaterials?.length
      ? `${match.material} | ${match.secondaryMaterials.join(' | ')}`
      : match.material;
  }

  let displayedMatches = $derived(setupMode ? [mockPreviewMatch()] : visibleMatches);

  onMount(async () => {
    settings = await loadSettingsReadOnly();
    unlisteners.push(await listen<SigLockSettings>('overlay-settings-updated', (event) => settings = event.payload));
    unlisteners.push(await listen<{ matches: OverlayMatch[] }>('overlay-result-updated', (event) => matches = event.payload.matches));
    unlisteners.push(await listen<boolean>('overlay-setup-mode-changed', (event) => setupMode = event.payload));
    setupMode = await invoke<boolean>('get_overlay_setup_mode');
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
  {#if displayedMatches.length}
    <div class="matches">
      {#each displayedMatches as match (match.key)}
        <div class="match-item">
          <p>{#if match.rockCount > 0}<strong>x{match.rockCount}</strong>{/if}<span>{materialLabel(match)}</span>{#if match.repeatCount > 1}<b>x{match.repeatCount}</b>{/if}</p>
          {#if settings?.showScannedValueOnOverlay && (match.valueLabel || match.detailLabel)}
            <small>{match.valueLabel || match.detailLabel}</small>
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
    font: var(--result-font-size, 13px)/1.3 Inter, system-ui, sans-serif;
  }
  :global(:root[data-high-contrast="true"] body) {
    text-shadow: 0 1px 2px #000, 0 0 5px #000;
  }
  .overlay-shell {
    width: max-content;
    max-width: 320px;
    padding: 5px 7px;
    background: var(--result-bg, rgba(15, 17, 21, .96));
    border-left: 2px solid color-mix(in srgb, var(--result-accent, #3b82f6) 70%, transparent);
    border-radius: 3px;
  }
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
    align-items: baseline;
    gap: 5px;
    min-width: 0;
    margin: 0;
    white-space: nowrap;
  }
  .match-item small {
    display: block;
    margin-top: -1px;
    color: color-mix(in srgb, var(--result-text, #e5e7eb) 72%, transparent);
    font: 700 .78em ui-monospace, monospace;
  }
  .matches strong { color: var(--result-accent, #3b82f6); font-family: ui-monospace, monospace; }
  .matches b { margin-left: 2px; color: color-mix(in srgb, var(--result-text, #e5e7eb) 70%, transparent); font: 700 .78em ui-monospace, monospace; }
  .overlay-shell > p { margin: 5px 0 0; color: color-mix(in srgb, var(--result-text, #e5e7eb) 65%, transparent); font-size: .8em; }
  :global(:root[data-compact="true"]) .overlay-shell { padding: 3px 5px; }
  :global(:root[data-compact="true"]) .match-item { padding: 0; }
</style>
