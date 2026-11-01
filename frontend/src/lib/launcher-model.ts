import type { LauncherApp } from './sdk/core';

/** Search by desktop application's display name and description. The id stays
 * tied to the original native GIO catalog, even after filtering.
 */
export function filterLauncherApps(apps: readonly LauncherApp[], query: string): LauncherApp[] {
  const terms = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  if (!terms.length) return [...apps];
  return apps.filter(app => {
    const haystack = (app.name + ' ' + app.description).toLocaleLowerCase();
    return terms.every(term => haystack.includes(term));
  });
}

export function nextLauncherSelection(current: number, count: number, direction: -1 | 1): number {
  if (count <= 0) return 0;
  return Math.max(0, Math.min(count - 1, current + direction));
}
