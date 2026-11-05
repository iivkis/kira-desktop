import { describe, expect, it } from 'vitest';
import { EMPTY_TRAY, isTrayState, splitTray, trayBarView, type TrayItem } from './tray-model';

const item = (id: number, pinned: boolean): TrayItem => ({
  id, key: 'app-' + id, title: 'Application ' + id, icon: null,
  status: 'Active', pinned,
});

describe('native tray state', () => {
  it('is empty rather than inventing icons while offline', () => {
    expect(isTrayState(EMPTY_TRAY)).toBe(true);
    expect(splitTray(EMPTY_TRAY.items)).toEqual({ pinned: [], hidden: [] });
  });

  it('partitions without mutating SNI registration order', () => {
    const original = [item(1, false), item(2, true), item(3, false), item(4, true)];
    const groups = splitTray(original);
    expect(groups.pinned.map(i => i.id)).toEqual([2, 4]);
    expect(groups.hidden.map(i => i.id)).toEqual([1, 3]);
    expect(original.map(i => i.id)).toEqual([1, 2, 3, 4]);
  });

  it('keeps clipped pinned items reachable and attention truthful', () => {
    const items = [item(1, true), item(2, true), item(3, false)];
    items[1].status = 'NeedsAttention';
    const view = trayBarView(items, 1);
    expect(view.visible.map(i => i.id)).toEqual([1]);
    expect(view.overflow.map(i => i.id)).toEqual([3, 2]);
    expect(view.needsAttention).toBe(true);
    expect(trayBarView(items, 2).needsAttention).toBe(false);
  });

  it('accepts bounded Unicode metadata without confusing UTF-16 and scalar length', () => {
    expect(isTrayState({
      available: true,
      items: [{ ...item(7, false), title: '😀'.repeat(120), key: '应用'.repeat(90) }],
    })).toBe(true);
    expect(isTrayState({
      available: true,
      items: [{ ...item(7, false), title: '😀'.repeat(121) }],
    })).toBe(false);
  });

  it('rejects forged URLs, duplicate IDs, oversized lists and invalid statuses', () => {
    expect(isTrayState({ available: true, items: [item(1, false)] })).toBe(true);
    expect(isTrayState({ available: true, items: [item(1, false), item(1, true)] })).toBe(false);
    expect(isTrayState({ available: true, items: [{ ...item(1, false), icon: 'file:///etc/passwd' }] })).toBe(false);
    expect(isTrayState({ available: true, items: [{ ...item(1, false), status: 'Fake' }] })).toBe(false);
    expect(isTrayState({
      available: true,
      items: [{ ...item(1, false), status: { toString() { throw Error('unsafe'); } } }],
    })).toBe(false);
    expect(isTrayState({ available: true, items: Array.from({ length: 65 }, (_, i) => item(i + 1, false)) })).toBe(false);
  });
});
