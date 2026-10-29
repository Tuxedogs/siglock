import { Store } from '@tauri-apps/plugin-store';

export type DetectionEvent = {
  id: string;
  timestamp: string;
  trigger: 'Manual' | 'Active';
  material: string;
  rawValue: string;
  status: 'matched' | 'no match' | 'invalid' | 'failed' | 'skipped';
  rockCount: number | null;
  repeatCount: number;
  durationMs: number;
};

const STORE_FILE = 'detection-history.json';
const STORE_KEY = 'events';
export const DETECTION_HISTORY_LIMIT = 200;

let store: Store | null = null;

function isEvent(value: unknown): value is DetectionEvent {
  if (!value || typeof value !== 'object') return false;
  const event = value as Partial<DetectionEvent>;
  return typeof event.id === 'string'
    && typeof event.timestamp === 'string'
    && (event.trigger === 'Manual' || event.trigger === 'Active')
    && typeof event.material === 'string'
    && typeof event.rawValue === 'string'
    && ['matched', 'no match', 'invalid', 'failed', 'skipped'].includes(event.status ?? '')
    && typeof event.repeatCount === 'number'
    && typeof event.durationMs === 'number';
}

export function trimDetectionHistory(events: DetectionEvent[]): DetectionEvent[] {
  return events.filter(isEvent).slice(0, DETECTION_HISTORY_LIMIT);
}

export async function loadDetectionHistory(): Promise<DetectionEvent[]> {
  store = await Store.load(STORE_FILE, { defaults: { [STORE_KEY]: [] }, autoSave: 250 });
  return trimDetectionHistory((await store.get<unknown>(STORE_KEY) ?? []) as DetectionEvent[]);
}

export async function saveDetectionHistory(events: DetectionEvent[]): Promise<void> {
  if (!store) store = await Store.load(STORE_FILE, { defaults: { [STORE_KEY]: [] }, autoSave: 250 });
  await store.set(STORE_KEY, trimDetectionHistory(events));
  await store.save();
}
