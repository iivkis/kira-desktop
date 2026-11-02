import { describe, expect, it } from 'vitest';
import { visibleWorkspaceWindow } from './workspace-window';
import type { Workspace } from './sdk/core';

function ws(i: number, focused = false): Workspace {
  return {
    id: String(i), idx: i, name: i === 3 ? 'edit' : null,
    output: 'DP-1', is_active: focused, is_focused: focused, is_urgent: false,
  };
}

describe('Niri workspace display window', () => {
  it('preserves the full Niri list when no workspaces are hidden', () => {
    const apps = [ws(1), ws(2, true)];
    expect(visibleWorkspaceWindow(apps, 8))
      .toEqual({ items: apps, before: 0, after: 0, beforeUrgent: false, afterUrgent: false });
  });
  it('always keeps real focus visible with honest hidden counts', () => {
    const entries = [1, 2, 3, 4, 5, 6].map(i => ws(i, i === 5));
    const visible = visibleWorkspaceWindow(entries, 3);
    expect(visible.items.map(item => item.id)).toEqual(['3', '4', '5']);
    expect(visible.before).toBe(2);
    expect(visible.after).toBe(1);
    expect(visible.beforeUrgent).toBe(false);
    expect(visible.afterUrgent).toBe(false);
  });
  it('surfaces urgent Niri workspaces hidden to either side without inventing windows', () => {
    const entries = [1, 2, 3, 4, 5, 6].map(i => ws(i, i === 4));
    entries[0].is_urgent = true;
    entries[5].is_urgent = true;
    const windowed = visibleWorkspaceWindow(entries, 2);
    expect(windowed.items.map(item => item.idx)).toEqual([3, 4]);
    expect(windowed.before).toBe(2);
    expect(windowed.after).toBe(2);
    expect(windowed.beforeUrgent).toBe(true);
    expect(windowed.afterUrgent).toBe(true);
    entries[0].is_urgent = false;
    expect(visibleWorkspaceWindow(entries, 2).beforeUrgent).toBe(false);
  });

  it('handles no focused workspace and bounds the configured limit', () => {
    const entries = [1, 2, 3, 4].map(i => ws(i));
    expect(visibleWorkspaceWindow(entries, 2)).toEqual({
      items: entries.slice(0, 2), before: 0, after: 2,
      beforeUrgent: false, afterUrgent: false,
    });
    expect(visibleWorkspaceWindow(entries, Number.NaN).items).toHaveLength(1);
    expect(visibleWorkspaceWindow(entries, 0).items).toHaveLength(1);
    expect(visibleWorkspaceWindow(entries, 999).items).toHaveLength(4);
    expect(entries).toHaveLength(4);
  });
});
