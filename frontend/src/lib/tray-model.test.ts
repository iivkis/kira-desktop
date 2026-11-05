import { describe, expect, it } from 'vitest';
import { EMPTY_TRAY, isTrayState, splitTray, type TrayItem } from './tray-model';

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

  it('rejects forged URLs, duplicate IDs, oversized lists and invalid statuses', () => {
    expect(isTrayState({ available: true, items: [item(1, false)] })).toBe(true);
    expect(isTrayState({ available: true, items: [item(1, false), item(1, true)] })).toBe(false);
    expect(isTrayState({ available: true, items: [{ ...item(1, false), icon: 'file:///etc/passwd' }] })).toBe(false);
    expect(isTrayState({ available: true, items: [{ ...item(1, false), status: 'Fake' }] })).toBe(false);
    expect(isTrayState({ available: true, items: Array.from({ length: 65 }, (_, i) => item(i + 1, false)) })).toBe(false);
  });
});
