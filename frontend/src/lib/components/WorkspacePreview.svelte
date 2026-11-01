<script lang="ts">
  import type { WorkspaceState } from '../sdk/core';

  let { state, showNames, limit, onFocus, pending }: {
    state: WorkspaceState | null;
    showNames: boolean;
    limit: number;
    onFocus: (id: string) => void;
    pending: boolean;
  } = $props();

  // Follow Niri focus, not DOM position; do not lose the current workspace
  // when the configured visible-workspace limit is reached.
  let visible = $derived.by(() => {
    const all = state?.workspaces ?? [];
    const count = Math.max(1, Math.min(32, limit));
    const focused = Math.max(0, all.findIndex((item) => item.is_focused));
    return all.slice(Math.max(0, focused - count + 1), Math.max(0, focused - count + 1) + count);
  });
</script>

{#if !state}
  <span class="flex h-7 items-center whitespace-nowrap text-[10px] font-semibold text-kira-muted" role="status" aria-live="polite">NIRI…</span>
{:else if !state.connected}
  <span class="flex h-7 items-center whitespace-nowrap text-[10px] font-semibold text-kira-muted" role="status" aria-live="polite" title="Niri IPC disconnected">NIRI OFFLINE</span>
{:else if state.workspaces.length === 0}
  <span class="flex h-7 items-center whitespace-nowrap text-[10px] font-semibold text-kira-muted" role="status" aria-live="polite">NO WORKSPACES</span>
{:else}
  <ol class="flex max-w-[min(40vw,348px)] shrink-0 items-center gap-1 overflow-x-auto p-0 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden" aria-label="Niri workspaces">
    {#each visible as workspace (workspace.id)}
      <li
        class={`relative h-7 min-w-8 max-w-[90px] shrink-0 rounded-md text-[11px] font-bold tabular-nums transition-colors duration-150 motion-reduce:transition-none ${workspace.is_focused
          ? 'bg-kira-accent text-kira-on-accent'
          : workspace.is_active
            ? 'bg-kira-surface-soft text-kira-text'
            : 'text-kira-muted hover:bg-kira-surface-soft hover:text-kira-text'}`}
        data-focused={workspace.is_focused}
        data-urgent={workspace.is_urgent}
      >
        <button
          type="button"
          class={`grid h-full w-full cursor-pointer place-items-center rounded-[inherit] border-0 bg-transparent px-[9px] font-inherit text-inherit focus-visible:outline-2 focus-visible:outline-offset-2 disabled:cursor-progress ${workspace.is_focused ? 'focus-visible:outline-kira-on-accent focus-visible:-outline-offset-2' : 'focus-visible:outline-kira-accent'}`}
          aria-label={'Focus workspace ' + (workspace.name || workspace.idx)
            + (workspace.output ? ' on ' + workspace.output : '')}
          aria-pressed={workspace.is_focused}
          title={'Workspace ' + (workspace.name || workspace.idx)
            + (workspace.output ? ' on ' + workspace.output : '')
            + (workspace.is_focused ? ' (focused)' : workspace.is_active ? ' (active)' : '')
            + (workspace.is_urgent ? ' (urgent)' : '')}
          disabled={pending}
          onclick={() => onFocus(workspace.id)}
        >
          <span class="block max-w-[72px] truncate whitespace-nowrap">{showNames ? (workspace.name || workspace.idx) : workspace.idx}</span>
          {#if workspace.is_urgent}
            <span class={`pointer-events-none absolute bottom-[2px] left-1/2 h-[2px] w-2 -translate-x-1/2 ${workspace.is_focused ? 'bg-kira-on-accent' : 'bg-kira-accent'}`} aria-hidden="true"></span>
          {/if}
        </button>
      </li>
    {/each}
  </ol>
{/if}
