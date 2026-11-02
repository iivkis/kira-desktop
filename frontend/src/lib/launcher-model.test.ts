import { describe, expect, it } from 'vitest';
import { filterLauncherApps, nextLauncherSelection } from './launcher-model';
import type { LauncherApp } from './sdk/core';

const apps: LauncherApp[] = [
  { id: 7, name: 'Firefox', description: 'Web browser' },
  { id: 2, name: 'Files', description: 'Browse folders' },
  { id: 21, name: 'Terminal', description: 'Command line' },
];

describe('launcher model', () => {
  it('searches display names and descriptions without losing native GIO identity', () => {
    expect(filterLauncherApps(apps, 'fire').map(app => app.id)).toEqual([7]);
    expect(filterLauncherApps(apps, 'browse folders').map(app => app.id)).toEqual([2]);
    expect(filterLauncherApps(apps, 'WEB BROWSER').map(app => app.id)).toEqual([7]);
    expect(filterLauncherApps(apps, 'terminal command').map(app => app.id)).toEqual([21]);
    expect(filterLauncherApps(apps, 'unavailable')).toEqual([]);
    expect(filterLauncherApps(apps, '')).toEqual(apps);
    expect(apps[0].id).toBe(7);
  });

  it('ranks exact and prefix app names ahead of description-only hits', () => {
    const entries: LauncherApp[] = [
      { id: 10, name: 'Browser tools', description: 'Utilities' },
      { id: 11, name: 'WebKit diagnostics', description: 'Browser testing' },
      { id: 12, name: 'Browser', description: 'The browser' },
      { id: 13, name: 'My browser', description: 'Utility browser' },
      { id: 14, name: 'Browser editor', description: 'Window browser' },
      { id: 15, name: 'Cool Browser', description: 'Browser tool' },
    ];
    expect(filterLauncherApps(entries, 'browser').map(app => app.id))
      .toEqual([12, 10, 14, 13, 15, 11]);
    expect(filterLauncherApps(entries, 'browser editor').map(app => app.id)).toEqual([14]);
    expect(filterLauncherApps(entries, '').map(app => app.id))
      .toEqual([10, 11, 12, 13, 14, 15]);
    expect(entries[0].id).toBe(10);
  });

  it('bounds keyboard selection to visible results and handles empty lists', () => {
    expect(nextLauncherSelection(0, 4, -1)).toBe(0);
    expect(nextLauncherSelection(3, 4, 1)).toBe(3);
    expect(nextLauncherSelection(1, 4, 1)).toBe(2);
    expect(nextLauncherSelection(0, 0, 1)).toBe(0);
    expect(nextLauncherSelection(0, 0, -1)).toBe(0);
  });
});
