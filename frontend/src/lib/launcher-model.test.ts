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

  it('bounds keyboard selection to visible results and handles empty lists', () => {
    expect(nextLauncherSelection(0, 4, -1)).toBe(0);
    expect(nextLauncherSelection(3, 4, 1)).toBe(3);
    expect(nextLauncherSelection(1, 4, 1)).toBe(2);
    expect(nextLauncherSelection(0, 0, 1)).toBe(0);
    expect(nextLauncherSelection(0, 0, -1)).toBe(0);
  });
});
