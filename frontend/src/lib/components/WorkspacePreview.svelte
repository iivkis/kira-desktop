<script lang="ts">
  import { tick } from 'svelte';
  import type { WorkspaceState } from '../sdk/core';
  import { visibleWorkspaceWindow } from '../workspace-window';
  import { tenetFlowIndicator, tenetScrollIntoView, tenetScrollScope } from '../motion';

  let { state, showNames, limit, onFocus, pending }: {
    state: WorkspaceState | null;
    showNames: boolean;
    limit: number;
    onFocus: (id: string) => void;
    pending: boolean;
  } = $props();

  const windowed = $derived(visibleWorkspaceWindow(state?.workspaces ?? [], limit));
  const focusedId = $derived(windowed.items.find(item => item.is_focused)?.id ?? null);
  let list = $state<HTMLOListElement | undefined>(undefined);
  let previousFocus: string | null = null;

  // The model guarantees inclusion; WebKit's fixed-width horizontal
  // viewport still needs explicit pixel visibility after Niri changes focus.
  $effect(() => {
    const id = focusedId;
    if (id === null) return;
    const animate = previousFocus !== null && previousFocus !== id;
    previousFocus = id;
    let cancelled = false;
    void tick().then(() => {
      if (cancelled || !list) return;
      const match = Array.from(list.querySelectorAll<HTMLElement>('[data-workspace-id]'))
        .find(item => item.dataset.workspaceId === id);
      if (match) tenetScrollIntoView(list, match, animate);
    });
    return () => { cancelled = true; };
  });

</script>

{#if !state}
  <span class="flex h-7 items-center whitespace-nowrap text-[10px] font-semibold tracking-wide text-kira-muted"
    role="status" aria-live="polite">NIRI: ПОДКЛЮЧЕНИЕ…</span>
{:else if !state.connected}
  <span class="flex h-7 items-center whitespace-nowrap text-[10px] font-semibold tracking-wide text-kira-muted"
    role="status" aria-live="polite" title="Нет соединения с Niri IPC">NIRI НЕДОСТУПЕН</span>
{:else if state.workspaces.length === 0}
  <span class="flex h-7 items-center whitespace-nowrap text-[10px] font-semibold tracking-wide text-kira-muted"
    role="status" aria-live="polite">НЕТ РАБОЧИХ СТОЛОВ</span>
{:else}
  <div class="flex min-w-0 shrink items-center gap-1" data-workspace-strip>
    {#if windowed.before > 0}
      <span class="shrink-0 text-[10px] font-semibold tabular-nums"
        class:text-kira-accent={windowed.beforeUrgent}
        class:text-kira-muted={!windowed.beforeUrgent}
        role={windowed.beforeUrgent ? 'status' : undefined}
        title={windowed.before + ' предыдущих рабочих пространств скрыто' + (windowed.beforeUrgent ? ' / срочный сигнал' : '')}
        aria-label={windowed.before + ' предыдущих рабочих пространств не показано' + (windowed.beforeUrgent ? ', скрыто срочное пространство' : '')}>
        −{windowed.before}
      </span>
    {/if}
    <ol bind:this={list} use:tenetFlowIndicator={{ active: focusedId ?? '', axis: 'both' }}
      use:tenetScrollScope
      class="relative flex max-w-[min(45vw,416px)] min-w-0 shrink items-center gap-1 overflow-x-auto p-0 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
      aria-label="Рабочие пространства Niri">
      <li data-kira-flow-indicator aria-hidden="true"
        class="pointer-events-none absolute left-0 top-0 z-0 list-none rounded-[2px] bg-kira-accent"></li>
      {#each windowed.items as workspace (workspace.id)}
        <li
          class={['relative z-10 h-[29px] min-w-[31px] max-w-[108px] shrink-0 rounded-[2px] text-[11px] font-extrabold tabular-nums transition-colors duration-150 motion-reduce:transition-none',
            workspace.is_focused
              ? 'text-kira-on-accent'
              : workspace.is_active
                ? 'bg-kira-surface-soft text-kira-text'
                : 'text-kira-muted hover:bg-kira-surface-soft hover:text-kira-text'].join(' ')}
          data-focused={workspace.is_focused}
          data-workspace-id={workspace.id}
          data-kira-tab={workspace.id}
          data-urgent={workspace.is_urgent}
        >
          <button
            type="button"
            class={['grid h-full w-full cursor-pointer place-items-center rounded-[inherit] border-0 bg-transparent px-[10px] font-inherit text-inherit focus-visible:outline-2 disabled:cursor-progress',
              workspace.is_focused
                ? 'focus-visible:outline-kira-on-accent focus-visible:-outline-offset-2'
                : 'focus-visible:outline-kira-accent focus-visible:outline-offset-1'].join(' ')}
            aria-label={'Переключиться на пространство ' + (workspace.name || workspace.idx)
              + (workspace.output ? ' на экране ' + workspace.output : '')
              + (workspace.is_urgent ? ', требует внимания' : '')}
            aria-pressed={workspace.is_focused}
            title={'Пространство ' + (workspace.name || workspace.idx)
              + (workspace.output ? ' на экране ' + workspace.output : '')
              + (workspace.is_focused ? ' (в фокусе)' : workspace.is_active ? ' (активно)' : '')
              + (workspace.is_urgent ? ' (требует внимания)' : '')}
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
      <span class="shrink-0 text-[10px] font-semibold tabular-nums"
        class:text-kira-accent={windowed.afterUrgent}
        class:text-kira-muted={!windowed.afterUrgent}
        role={windowed.afterUrgent ? 'status' : undefined}
        title={windowed.after + ' последующих рабочих пространств скрыто' + (windowed.afterUrgent ? ' / срочный сигнал' : '')}
        aria-label={windowed.after + ' последующих рабочих пространств не показано' + (windowed.afterUrgent ? ', скрыто срочное пространство' : '')}>
        +{windowed.after}
      </span>
    {/if}
  </div>
{/if}
