<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { emitTo } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  let startX = 0;
  let startY = 0;
  let currentX = 0;
  let currentY = 0;
  let isSelecting = false;
  let selectionBox: HTMLDivElement;

  let status = 'Drag to select the area where the scan number appears. Press Esc to cancel.';

  function updateBox() {
    if (!selectionBox) return;

    const x = Math.min(startX, currentX);
    const y = Math.min(startY, currentY);
    const w = Math.abs(currentX - startX);
    const h = Math.abs(currentY - startY);

    selectionBox.style.left = `${x}px`;
    selectionBox.style.top = `${y}px`;
    selectionBox.style.width = `${w}px`;
    selectionBox.style.height = `${h}px`;
    selectionBox.style.display = w > 5 && h > 5 ? 'block' : 'none';
  }

  function onMouseDown(e: MouseEvent) {
    isSelecting = true;
    startX = e.clientX;
    startY = e.clientY;
    currentX = startX;
    currentY = startY;
    updateBox();
  }

  function onMouseMove(e: MouseEvent) {
    if (!isSelecting) return;
    currentX = e.clientX;
    currentY = e.clientY;
    updateBox();
  }

  async function onMouseUp() {
    if (!isSelecting) return;
    isSelecting = false;

    const x = Math.min(startX, currentX);
    const y = Math.min(startY, currentY);
    const width = Math.abs(currentX - startX);
    const height = Math.abs(currentY - startY);

    // Validate minimum size
    if (width < 30 || height < 15) {
      status = 'Selection too small. Try again.';
      if (selectionBox) selectionBox.style.display = 'none';
      return;
    }

    // For v1: Assume picker window covers primary monitor starting at ~ (0,0)
    // In real multi-monitor this would need monitor info from Rust.
    const region = {
      x: Math.round(x),
      y: Math.round(y),
      width: Math.round(width),
      height: Math.round(height),
    };

    try {
      await invoke('set_crop_region', { region });
      await emitTo('main', 'crop-region-updated', region);
      status = 'Region saved!';
      // Close immediately after successful save
      await getCurrentWindow().close();
    } catch (err) {
      status = 'Failed to save region: ' + err;
      // Still close the picker so it doesn't stay as a giant overlay
      setTimeout(async () => {
        try { await getCurrentWindow().close(); } catch (_) {}
      }, 200);
    }
  }

  async function cancel() {
    console.info('[SigLock] region picker cancelled, preserving existing region');
    try {
      await emitTo('main', 'region-picker-cancelled');
    } finally {
      await getCurrentWindow().close();
    }
  }

  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      cancel();
    }
  }

  onMount(async () => {
    document.addEventListener('keydown', handleKey);
    document.addEventListener('mousedown', onMouseDown);
    document.addEventListener('mousemove', onMouseMove);
    document.addEventListener('mouseup', onMouseUp);

    document.body.style.cursor = 'crosshair';
  });

  onDestroy(() => {
    document.removeEventListener('keydown', handleKey);
    document.removeEventListener('mousedown', onMouseDown);
    document.removeEventListener('mousemove', onMouseMove);
    document.removeEventListener('mouseup', onMouseUp);
  });
</script>

<div class="picker-root fixed inset-0 z-50" style="cursor: crosshair;">
  <!-- Instructions -->
  <div class="fixed top-4 left-1/2 -translate-x-1/2 bg-black/80 text-white px-6 py-2 rounded text-sm z-[100] pointer-events-none">
    {status}
  </div>

  <!-- Rubber band selection box -->
  <div
    bind:this={selectionBox}
    class="fixed border-2 border-blue-400 bg-blue-400/10 pointer-events-none z-[90]"
    style="display: none;"
  ></div>

  <div class="fixed bottom-4 right-4 text-white/70 text-xs pointer-events-none">
    Drag over the scan number • Esc to cancel
  </div>
</div>

<style>
  /* Force transparency - override any global SigLock dark theme that leaks into this window */
  :global(html),
  :global(body),
  :global(#app),
  :global(.sveltekit-body),
  :global(div[data-sveltekit-preload-data]) {
    background: transparent !important;
    background-color: transparent !important;
    background-image: none !important;
    margin: 0 !important;
    padding: 0 !important;
    overflow: hidden !important;
  }

  /* Semi-transparent overlay tint only - this is the visual background of the picker */
  .picker-root {
    background-color: rgba(0, 0, 0, 0.40) !important;
  }

  :global(body) {
    cursor: crosshair;
  }
</style>
