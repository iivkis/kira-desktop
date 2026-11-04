<script lang="ts">
  import { onMount } from 'svelte';

  let { onClose, onPopout, generation, label }: {
    onClose: () => void;
    onPopout: () => void;
    generation: number;
    label: string;
  } = $props();

  const localizedName = label === 'launcher' ? 'приложения'
    : label === 'calendar' ? 'календарь'
      : label === 'settings' ? 'настройки' : 'монитор системы';
  let detached = $state(false);
  let popoutFailed = $state(false);
  let responseTimer: number | null = null;

  function requestPopout() {
    popoutFailed = false;
    // A current Svelte bundle running inside an outdated native Shell can
    // expose the button without implementing the private pop-out protocol.
    if (!window.webkit?.messageHandlers?.kiraCalendarUi) {
      popoutFailed = true;
      return;
    }
    onPopout();
    if (responseTimer !== null) window.clearTimeout(responseTimer);
    responseTimer = window.setTimeout(() => {
      responseTimer = null;
      if (!detached) popoutFailed = true;
    }, 1800);
  }

  onMount(() => {
    const onDetached = (event: Event) => {
      const current = (event as CustomEvent<{ generation: number }>).detail?.generation;
      if (current === generation) {
        detached = true;
        popoutFailed = false;
        if (responseTimer !== null) window.clearTimeout(responseTimer);
        responseTimer = null;
      }
    };
    window.addEventListener('kira:overlay:detached', onDetached);
    return () => {
      window.removeEventListener('kira:overlay:detached', onDetached);
      if (responseTimer !== null) window.clearTimeout(responseTimer);
    };
  });
</script>

<div class="flex shrink-0 items-center gap-1" role="group" aria-label={'Управление окном: ' + localizedName}>
  {#if !detached}
    <button type="button" onclick={requestPopout}
      title={'Открыть ' + localizedName + ' отдельным окном Niri'}
      aria-label={'Вынести ' + localizedName + ' в отдельное окно'}
      class="rounded-sm p-2 text-kira-muted transition-colors hover:text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent">
      <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24"
        fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
        <rect x="3" y="8" width="13" height="13" rx="1"></rect>
        <path d="M12 3h9v9M21 3l-11 11"></path>
      </svg>
    </button>
  {/if}
  {#if popoutFailed}
    <span role="status" class="max-w-28 text-[10px] font-bold leading-tight text-kira-accent"
      title="Нативный Shell не подтвердил вынос окна. Обновите и полностью перезапустите kira-shell.">
      ПЕРЕЗАПУСТИТЬ SHELL
    </span>
  {/if}
  <button type="button" onclick={onClose} aria-label={'Закрыть ' + localizedName} title="Закрыть (Esc)"
    class="rounded-sm px-2 py-1 text-xl leading-none text-kira-muted transition-colors
      hover:text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent">×</button>
</div>
