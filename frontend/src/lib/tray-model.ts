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
        typeof item.key !== 'string' || Array.from(item.key).length > 240 ||
        typeof item.title !== 'string' || Array.from(item.title).length > 120 ||
        typeof item.pinned !== 'boolean' ||
        typeof item.status !== 'string' ||
        !['Active', 'Passive', 'NeedsAttention'].includes(item.status) ||
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

/** Subset of app-owned D-Bus menus rendered by the trusted tray host. */
export interface TrayMenuEntry {
  id: number;
  label: string;
  depth: number;
  enabled: boolean;
  separator: boolean;
  checked: boolean;
  radio: boolean;
  has_children: boolean;
}

export interface TrayMenu {
  handle: number;
  title: string;
  items: TrayMenuEntry[];
}

export const EMPTY_TRAY_MENU: TrayMenu = { handle: 0, title: '', items: [] };

export function isTrayMenu(value: unknown): value is TrayMenu {
  if (!record(value) || !Number.isSafeInteger(value.handle) ||
      (value.handle as number) < 0 || (value.handle as number) > 0xffffffff ||
      typeof value.title !== 'string' || Array.from(value.title).length > 120 ||
      !Array.isArray(value.items) || value.items.length > 80 ||
      ((value.handle as number) === 0 && value.items.length !== 0)) return false;
  return value.items.every((entry: unknown) =>
    record(entry) && Number.isSafeInteger(entry.id) &&
    (entry.id as number) > 0 && (entry.id as number) <= 0x7fffffff &&
    Number.isSafeInteger(entry.depth) &&
    (entry.depth as number) >= 0 && (entry.depth as number) <= 2 &&
    typeof entry.label === 'string' && Array.from(entry.label).length <= 120 &&
    typeof entry.enabled === 'boolean' &&
    typeof entry.separator === 'boolean' &&
    typeof entry.checked === 'boolean' &&
    typeof entry.radio === 'boolean' &&
    typeof entry.has_children === 'boolean',
  );
}
