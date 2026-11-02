import type { Workspace } from './sdk/core';

/** Niri's focused workspace must remain visible when the KDL limit hides
 * some entries. Counts refer only to omitted workspaces, never windows.
 */
export interface WorkspaceWindow {
  items: Workspace[];
  before: number;
  after: number;
}

export function visibleWorkspaceWindow(
  workspaces: readonly Workspace[], limit: number,
): WorkspaceWindow {
  const count = Number.isFinite(limit)
    ? Math.max(1, Math.min(32, Math.trunc(limit))) : 1;
  const focus = workspaces.findIndex(workspace => workspace.is_focused);
  const start = Math.min(
    Math.max(0, focus - count + 1),
    Math.max(0, workspaces.length - count),
  );
  const items = workspaces.slice(start, start + count);
  return {
    items,
    before: start,
    after: Math.max(0, workspaces.length - start - items.length),
  };
}
