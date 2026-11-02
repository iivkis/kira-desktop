import type { LauncherApp } from './sdk/core';

/** Real GIO apps only. Without explicit pins, show the bounded catalog
 * head, not invented favorites or pretend running-window status.
 */
export function dockApplications(
  apps: readonly LauncherApp[], favorites: readonly string[], limit = 8,
): LauncherApp[] {
  const count = Number.isFinite(limit) ? Math.max(0, Math.min(8, Math.trunc(limit))) : 0;
  if (!favorites.length) return apps.slice(0, count);
  const seen = new Set<number>();
  const result: LauncherApp[] = [];
  for (const favorite of favorites) {
    const name = favorite.trim().toLocaleLowerCase();
    if (!name) continue;
    const app = apps.find(row => row.name.toLocaleLowerCase() === name);
    if (app && !seen.has(app.id)) {
      seen.add(app.id);
      result.push(app);
      if (result.length >= count) break;
    }
  }
  return result.slice(0, count);
}
