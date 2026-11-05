<script lang="ts">
  import { onMount } from 'svelte';
  import { tenetWidgetReveal, tenetReflow, tenetReflowScope } from '../motion';
  import { EMPTY_TRAY, splitTray, type TrayState } from '../tray-model';
  import { onTrayUpdate, requestTraySnapshot, trayAction } from '../sdk/core';

  let { onClose, generation }: { onClose: () => void; generation: number } = $props();
  let state = $state<TrayState>(EMPTY_TRAY);
  let list: HTMLDivElement;
  const groups = $derived(splitTray(state.items));

  onMount(() => {
    const stop = onTrayUpdate(next => {
      if (list) tenetReflow(list, () => { state = next; });
      else state = next;
    });
    requestTraySnapshot(generation);
    return stop;
  });

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Escape') { event.preventDefault(); onClose(); }
  }

  function togglePin(id: number) {
    trayAction(id, 'pin', generation);
  }
  function activate(id: number) {
    trayAction(id, 'activate', generation);
  }
  function secondary(event: MouseEvent, id: number) {
    event.preventDefault();
    trayAction(id, 'secondary', generation);
  }
</script>

<svelte:window onkeydown={keydown} />
<main data-kira-ui-root aria-label="Kira Tray" role="dialog" aria-modal="false"
  use:tenetWidgetReveal={{ contact: '[data-kira-contact]', direction: 1 }}
  class="relative flex min-h-0 w-full flex-col overflow-hidden bg-kira-bg text-kira-text">
  <header data-kira-enter class="flex items-center justify-between gap-3 px-4 pb-3 pt-4">
    <h1 data-kira-contact class="border-l-2 border-kira-accent pl-3 text-base font-extrabold">Скрытые значки</h1>
    <button type="button" title="Закрыть трей" aria-label="Закрыть трей" onclick={onClose}
      class="flex size-7 items-center justify-center text-lg text-kira-muted hover:text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent">×</button>
  </header>
  <div data-kira-enter bind:this={list} use:tenetReflowScope
    class="kira-tray-scroll max-h-[360px] min-h-0 overflow-y-auto bg-kira-surface px-3 py-3"
    role="group" aria-label="Значки приложений">
    {#each [...groups.hidden, ...groups.pinned] as item (item.id)}
      <div data-kira-reflow class="mb-1 flex min-h-11 items-center gap-2">
        <button type="button" onclick={() => activate(item.id)}
          oncontextmenu={(event) => secondary(event, item.id)}
          title={item.title}
          class="flex min-w-0 flex-1 items-center gap-3 rounded-[2px] p-1.5 text-left hover:bg-kira-bg/25 focus-visible:outline-2 focus-visible:outline-kira-accent">
          <span class="flex size-7 shrink-0 items-center justify-center bg-kira-bg text-kira-muted" aria-hidden="true">
            {#if item.icon}
              <img src={item.icon} alt="" class="size-6 object-contain" draggable="false" />
            {:else}
              <span class="text-xs font-semibold uppercase">{item.title.slice(0, 1)}</span>
            {/if}
          </span>
          <span class="min-w-0 flex-1 truncate text-[12px] font-medium">{item.title}</span>
          {#if item.status === 'NeedsAttention'}
            <span class="size-1.5 shrink-0 bg-kira-accent" aria-label="Требует внимания"></span>
          {/if}
        </button>
        <button type="button" onclick={() => togglePin(item.id)}
          title={item.pinned ? 'Убрать в трей' : 'Закрепить в Top Bar'}
          aria-label={(item.pinned ? 'Открепить ' : 'Закрепить ') + item.title}
          aria-pressed={item.pinned}
          class={'flex size-8 shrink-0 items-center justify-center rounded-[2px] focus-visible:outline-2 focus-visible:outline-kira-accent ' +
            (item.pinned ? 'bg-kira-accent text-kira-on-accent' : 'text-kira-muted hover:bg-kira-bg/20 hover:text-kira-text')}>
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor"
            stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M9 3h6l-1 6 3 3v2H7v-2l3-3-1-6ZM12 14v7"/>
          </svg>
        </button>
      </div>
    {:else}
      <p class="px-2 py-6 text-center text-[12px] text-kira-muted" role="status">
        {state.available ? 'Приложения не добавили значки в трей' : 'Служба системного трея недоступна'}
      </p>
    {/each}
    {#if groups.pinned.length > 0}
      <p class="mt-3 border-t border-kira-muted/15 px-2 pt-3 text-[11px] text-kira-muted">
        Закреплённые значки остаются в верхней панели.
      </p>
    {/if}
  </div>
</main>
