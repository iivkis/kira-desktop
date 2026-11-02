<script lang="ts">
  import type { WorkspaceState } from '../sdk/core';
  import { visibleWorkspaceWindow } from '../workspace-window';

  let { state, showNames, limit, onFocus, pending }: {
    state: WorkspaceState | null;
    showNames: boolean;
    limit: number;
    onFocus: (id: string) => void;
    pending: boolean;
  } = $props();

  const windowed = $derived(visibleWorkspaceWindow(state?.workspaces ?? [], limit));

</script>

{#if !state}
  <span class="flex h-7 items-center whitespace-nowrap text-[10px] font-semibold tracking-wide text-kira-muted"
    role="status" aria-live="polite">NIRI…</span>
{:else if !state.connected}
  <span class="flex h-7 items-center whitespace-nowrap text-[10px] font-semibold tracking-wide text-kira-muted"
    role="status" aria-live="polite" title="Niri IPC disconnected">NIRI OFFLINE</span>
{:else if state.workspaces.length === 0}
  <span class="flex h-7 items-center whitespace-nowrap text-[10px] font-semibold tracking-wide text-kira-muted"
    role="status" aria-live="polite">NO WORKSPACES</span>
{:else}
  <div class="flex min-w-0 shrink items-center gap-1" data-workspace-strip>
    {#if windowed.before > 0}
      <span class="shrink-0 text-[10px] font-semibold tabular-nums text-kira-muted/80"
        title={windowed.before + ' preceding Niri workspaces hidden'}
        aria-label={windowed.before + ' preceding workspaces not shown'}>
        −{windowed.before}
      </span>
    {/if}
    <ol class="flex max-w-[min(45vw,416px)] min-w-0 shrink items-center gap-1 overflow-x-auto p-0 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
      aria-label="Niri workspaces">
      {#each windowed.items as workspace (workspace.id)}
        <li
          class={['relative h-[29px] min-w-[31px] max-w-[108px] shrink-0 rounded-[2px] text-[11px] font-extrabold tabular-nums transition-colors duration-150 motion-reduce:transition-none',
            workspace.is_focused
              ? 'bg-kira-accent text-kira-on-accent'
              : workspace.is_active
                ? 'bg-kira-surface-soft text-kira-text'
                : 'text-kira-muted hover:bg-kira-surface-soft hover:text-kira-text'].join(' ')}
          data-focused={workspace.is_focused}
          data-urgent={workspace.is_urgent}
        >
          <button
            type="button"
            class={['grid h-full w-full cursor-pointer place-items-center rounded-[inherit] border-0 bg-transparent px-[10px] font-inherit text-inherit focus-visible:outline-2 disabled:cursor-progress',
              workspace.is_focused
                ? 'focus-visible:outline-kira-on-accent focus-visible:-outline-offset-2'
                : 'focus-visible:outline-kira-accent focus-visible:outline-offset-1'].join(' ')}
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
            <span class="block max-w-[86px] truncate whitespace-nowrap">
              {showNames ? (workspace.name || workspace.idx) : workspace.idx}
            </span>
            {#if workspace.is_urgent}
              <span aria-hidden="true"
                class={['pointer-events-none absolute right-[3px] top-[3px] h-[4px] w-[4px]',
                  workspace.is_focused ? 'bg-kira-on-accent' : 'bg-kira-accent'].join(' ')}></span>
            {/if}
          </button>
        </li>
      {/each}
    </ol>
    {#if windowed.after > 0}
      <span class="shrink-0 text-[10px] font-semibold tabular-nums text-kira-muted/80"
        title={windowed.after + ' following Niri workspaces hidden'}
        aria-label={windowed.after + ' following workspaces not shown'}>
        +{windowed.after}
      </span>
    {/if}
  </div>
{/if}
