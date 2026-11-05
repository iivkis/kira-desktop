/** Trusted StatusNotifierItem snapshots. IDs are opaque, session-local native handles. */
export interface TrayItem {
  id: number;
  key: string;
  title: string;
  icon: string | null;
  status: 'Active' | 'Passive' | 'NeedsAttention';
  pinned: boolean;
}

export interface TrayState {
  available: boolean;
  items: TrayItem[];
}

export const EMPTY_TRAY: TrayState = { available: false, items: [] };

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** WebKit receives no arbitrary external URLs or client-supplied D-Bus names. */
export function isTrayState(value: unknown): value is TrayState {
  if (!record(value) || typeof value.available !== 'boolean' ||
      !Array.isArray(value.items) || value.items.length > 64) return false;
  const ids = new Set<number>();
  return value.items.every((item: unknown) => {
    if (!record(item) ||
        !Number.isSafeInteger(item.id) || (item.id as number) <= 0 ||
        ids.has(item.id as number) ||
        typeof item.key !== 'string' || item.key.length > 180 ||
        typeof item.title !== 'string' || item.title.length > 120 ||
        typeof item.pinned !== 'boolean' ||
        !['Active', 'Passive', 'NeedsAttention'].includes(String(item.status)) ||
        !(item.icon === null || (typeof item.icon === 'string' &&
          item.icon.length <= 190000 &&
          /^data:image\/(?:png|svg\+xml|jpeg|webp);base64,[A-Za-z0-9+/=]+$/.test(item.icon)))) return false;
    ids.add(item.id as number);
    return true;
  });
}

export function splitTray(items: TrayItem[]): { pinned: TrayItem[]; hidden: TrayItem[] } {
  return {
    pinned: items.filter(item => item.pinned),
    hidden: items.filter(item => !item.pinned),
  };
}

/** The bar may clip pinned icons, but those icons still belong to overflow. */
export function trayBarView(items: TrayItem[], limit: number): {
  visible: TrayItem[];
  overflow: TrayItem[];
  needsAttention: boolean;
} {
  const { pinned, hidden } = splitTray(items);
  const visible = pinned.slice(0, Math.max(0, Math.floor(limit)));
  const overflow = [...hidden, ...pinned.slice(visible.length)];
  return {
    visible,
    overflow,
    needsAttention: overflow.some(item => item.status === 'NeedsAttention'),
  };
}
