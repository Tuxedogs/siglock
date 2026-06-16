import { Store } from '@tauri-apps/plugin-store';

export interface SigLockSettings {
  scanNowKeybind: string;
  toggleAutoScanKeybind: string;
  activeScanIntervalMs: number;
  overlayTextColor: string;
  overlayBackgroundColor: string;
  overlayAccentColor: string;
  overlayOpacity: number;
  overlayFontSize: number;
  overlayHighContrast: boolean;
  overlayCompactMode: boolean;
  showScannedValueOnOverlay: boolean;
  overlayResultLifetimeSeconds: number;
  rollingHistoryLimit: number;
}

export const DEFAULT_SETTINGS: SigLockSettings = {
  scanNowKeybind: 'Ctrl+Alt+F9',
  toggleAutoScanKeybind: 'Ctrl+Shift+S',
  activeScanIntervalMs: 3000,
  overlayTextColor: '#e5e7eb',
  overlayBackgroundColor: '#0f1115',
  overlayAccentColor: '#3b82f6',
  overlayOpacity: 0.96,
  overlayFontSize: 13,
  overlayHighContrast: true,
  overlayCompactMode: true,
  showScannedValueOnOverlay: false,
  overlayResultLifetimeSeconds: 20,
  rollingHistoryLimit: 30,
};

let store: Store | null = null;

function color(value: unknown, fallback: string): string {
  return typeof value === 'string' && /^#[0-9a-f]{6}$/i.test(value) ? value : fallback;
}

function numberInRange(value: unknown, fallback: number, min: number, max: number): number {
  return typeof value === 'number' && Number.isFinite(value)
    ? Math.min(max, Math.max(min, value))
    : fallback;
}

export function sanitizeSettings(value: unknown): SigLockSettings {
  const raw = value && typeof value === 'object' ? value as Partial<SigLockSettings> : {};
  return {
    scanNowKeybind: typeof raw.scanNowKeybind === 'string' && raw.scanNowKeybind.trim().length > 0
      ? raw.scanNowKeybind
      : DEFAULT_SETTINGS.scanNowKeybind,
    toggleAutoScanKeybind: typeof raw.toggleAutoScanKeybind === 'string' && raw.toggleAutoScanKeybind.trim().length > 0
      ? raw.toggleAutoScanKeybind
      : DEFAULT_SETTINGS.toggleAutoScanKeybind,
    activeScanIntervalMs: numberInRange(raw.activeScanIntervalMs, DEFAULT_SETTINGS.activeScanIntervalMs, 1000, 4000),
    overlayTextColor: color(raw.overlayTextColor, DEFAULT_SETTINGS.overlayTextColor),
    overlayBackgroundColor: color(raw.overlayBackgroundColor, DEFAULT_SETTINGS.overlayBackgroundColor),
    overlayAccentColor: color(raw.overlayAccentColor, DEFAULT_SETTINGS.overlayAccentColor),
    overlayOpacity: numberInRange(raw.overlayOpacity, DEFAULT_SETTINGS.overlayOpacity, 0.35, 1),
    overlayFontSize: numberInRange(raw.overlayFontSize, DEFAULT_SETTINGS.overlayFontSize, 11, 20),
    overlayHighContrast: typeof raw.overlayHighContrast === 'boolean' ? raw.overlayHighContrast : DEFAULT_SETTINGS.overlayHighContrast,
    overlayCompactMode: typeof raw.overlayCompactMode === 'boolean' ? raw.overlayCompactMode : DEFAULT_SETTINGS.overlayCompactMode,
    showScannedValueOnOverlay: typeof raw.showScannedValueOnOverlay === 'boolean' ? raw.showScannedValueOnOverlay : DEFAULT_SETTINGS.showScannedValueOnOverlay,
    overlayResultLifetimeSeconds: numberInRange(raw.overlayResultLifetimeSeconds, DEFAULT_SETTINGS.overlayResultLifetimeSeconds, 5, 120),
    rollingHistoryLimit: numberInRange(raw.rollingHistoryLimit, DEFAULT_SETTINGS.rollingHistoryLimit, 25, 50),
  };
}

export async function loadSettings(): Promise<SigLockSettings> {
  store = await Store.load('settings.json', { defaults: { settings: DEFAULT_SETTINGS }, autoSave: 250 });
  const settings = sanitizeSettings(await store.get('settings'));
  await store.set('settings', settings);
  await store.save();
  return settings;
}

export async function loadSettingsReadOnly(): Promise<SigLockSettings> {
  const readOnlyStore = await Store.load('settings.json', { defaults: { settings: DEFAULT_SETTINGS }, autoSave: false });
  return sanitizeSettings(await readOnlyStore.get('settings'));
}

export async function saveSettings(settings: SigLockSettings): Promise<void> {
  if (!store) store = await Store.load('settings.json', { defaults: { settings: DEFAULT_SETTINGS }, autoSave: 250 });
  await store.set('settings', sanitizeSettings(settings));
  await store.save();
}
