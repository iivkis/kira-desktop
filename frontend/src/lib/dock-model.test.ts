import { describe, expect, it } from 'vitest';
import { dockApplications } from './dock-model';
import type { LauncherApp } from './sdk/core';

const apps: LauncherApp[] = [
  { id: 4, name: 'Files', description: '' },
  { id: 3, name: 'Firefox', description: '' },
  { id: 8, name: 'Terminal', description: '' },
];
describe('Dock catalog projection', () => {
  it('retains sorted native order for unconfigured preview', () => {
    expect(dockApplications(apps, [], 2).map(app => app.id)).toEqual([4, 3]);
    expect(dockApplications(apps, [], 80)).toHaveLength(3);
    expect(dockApplications(apps, [], 0)).toEqual([]);
  });
  it('honors explicit case-insensitive favorites without fabricating apps', () => {
    expect(dockApplications(apps, ['terminal', 'missing', 'Files', 'Terminal'])
      .map(app => app.id)).toEqual([8, 4]);
    expect(dockApplications(apps, ['missing'])).toEqual([]);
    expect(dockApplications(apps, ['terminal'], 0)).toEqual([]);
  });
});
