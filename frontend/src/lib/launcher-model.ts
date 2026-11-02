import type { LauncherApp } from './sdk/core';

/** Search by desktop application's display name and description. The id stays
 * tied to the original native GIO catalog, even after filtering.
 */
export function filterLauncherApps(apps: readonly LauncherApp[], query: string): LauncherApp[] {
  const terms = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  if (!terms.length) return [...apps];
  const joined = terms.join(' ');
  // Preserve native GIO IDs and source order among equal-relevance hits.
  return apps.map((app, index) => {
    const name = app.name.toLocaleLowerCase();
    const haystack = (name + ' ' + app.description).toLocaleLowerCase();
    if (!terms.every(term => haystack.includes(term))) return null;
    const rank = name === joined ? 0
      : name.startsWith(joined) ? 1
        : name.split(/\s+/).some(word => word.startsWith(joined)) ? 2
          : name.includes(joined) ? 3
            : terms.every(term => name.includes(term)) ? 4 : 5;
    return { app, index, rank };
  }).filter((entry): entry is { app: LauncherApp; index: number; rank: number } => entry !== null)
    .sort((a, b) => a.rank - b.rank || a.index - b.index)
    .map(({ app }) => app);
}

export function nextLauncherSelection(current: number, count: number, direction: -1 | 1): number {
  if (count <= 0) return 0;
  return Math.max(0, Math.min(count - 1, current + direction));
}
