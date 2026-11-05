<script lang="ts">
  import { onMount } from 'svelte';
  import { tenetWidgetReveal, tenetReflow, tenetReflowScope } from '../motion';
  import { EMPTY_TRAY, EMPTY_TRAY_MENU, splitTray, type TrayState, type TrayMenu } from '../tray-model';
  import { onTrayUpdate, onTrayMenuUpdate, requestTraySnapshot, trayAction, clickTrayMenu, dismissTrayMenu } from '../sdk/core';

  let { onClose, generation }: { onClose: () => void; generation: number } = $props();
  let trayState: TrayState = $state(EMPTY_TRAY);
  let currentMenu: TrayMenu = $state(EMPTY_TRAY_MENU);
  let list = $state<HTMLDivElement | undefined>(undefined);
  const groups = $derived(splitTray(trayState.items));

  onMount(() => {
    const stop = onTrayUpdate(next => {
      if (list?.isConnected) tenetReflow(list, () => { trayState = next; });
      else trayState = next;
    });
    const stopMenu = onTrayMenuUpdate(next => { currentMenu = next; });
    requestTraySnapshot(generation);
    return () => { stop(); stopMenu(); };
  });

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      if (currentMenu.handle !== 0) dismissTrayMenu(generation);
      else onClose();
    }
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
  function clickMenuItem(id: number) {
    if (currentMenu.handle !== 0) clickTrayMenu(generation, currentMenu.handle, id);
  }
</script>

<svelte:window onkeydown={keydown} />
<main data-kira-ui-root aria-label="Kira Tray" role="dialog" aria-modal="false"
  use:tenetWidgetReveal={{ contact: '[data-kira-contact]', direction: 1 }}
  class="relative flex min-h-0 w-full flex-col overflow-hidden bg-kira-bg text-kira-text">
  <header data-kira-enter class="flex items-center justify-between gap-3 px-4 pb-3 pt-4">
    <h1 data-kira-contact class="border-l-2 border-kira-accent pl-3 text-base font-extrabold">
      {currentMenu.handle !== 0 ? 'Меню приложения' : 'Скрытые значки'}
    </h1>
    <button type="button" title="Закрыть трей" aria-label="Закрыть трей" onclick={onClose}
      class="flex size-7 items-center justify-center text-lg text-kira-muted hover:text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent">×</button>
  </header>
  {#if currentMenu.handle !== 0}
    <section data-kira-enter class="kira-tray-scroll max-h-[420px] min-h-0 overflow-y-auto bg-kira-surface px-3 py-3"
      aria-label={'Меню ' + currentMenu.title}>
      <button type="button" onclick={() => dismissTrayMenu(generation)}
        class="mb-2 flex w-full items-center gap-2 border-b border-kira-muted/20 px-2 py-2 text-left text-xs text-kira-muted hover:text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent"
        aria-label="Назад к значкам трея">
        <span aria-hidden="true">‹</span>
        <span>Назад · {currentMenu.title}</span>
      </button>
      <div role="menu" aria-label={'Действия ' + currentMenu.title}>
        {#each currentMenu.items as entry, i (i)}
          {#if entry.separator}
            <div role="separator" class="my-2 border-t border-kira-muted/20"></div>
          {:else if entry.has_children}
            <p class="py-2 text-[11px] font-extrabold uppercase tracking-wide text-kira-muted"
              style={'padding-left: ' + (8 + entry.depth * 12) + 'px'}>
              {entry.label}
            </p>
          {:else}
            <button type="button" role="menuitem"
              disabled={!entry.enabled}
              title={entry.label}
              aria-label={entry.label + (entry.checked ? ', выбрано' : '')}
              onclick={() => clickMenuItem(entry.id)}
              style={'padding-left: ' + (8 + entry.depth * 12) + 'px'}
              class="flex min-h-9 w-full items-center gap-2 rounded-[2px] py-1.5 pr-2 text-left text-[12px] hover:bg-kira-bg/20 focus-visible:outline-2 focus-visible:outline-kira-accent disabled:cursor-not-allowed disabled:opacity-40">
              <span class="w-4 shrink-0 text-center text-kira-accent" aria-hidden="true">
                {entry.checked ? (entry.radio ? '●' : '✓') : ''}
              </span>
              <span class="min-w-0 flex-1 truncate">{entry.label}</span>
            </button>
          {/if}
        {:else}
          <p class="px-2 py-5 text-center text-[12px] text-kira-muted" role="status">
            У приложения пока нет доступных действий
          </p>
        {/each}
      </div>
    </section>
  {:else}
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
        {trayState.available ? 'Приложения не добавили значки в трей' : 'Служба системного трея недоступна'}
      </p>
    {/each}
    {#if groups.pinned.length > 0}
      <p class="mt-3 border-t border-kira-muted/15 px-2 pt-3 text-[11px] text-kira-muted">
        Закреплённые значки доступны в панели или через кнопку переполнения.
      </p>
    {/if}
  </div>
  {/if}
</main>
