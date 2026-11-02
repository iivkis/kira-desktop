<script lang="ts">
  import { onMount } from 'svelte';
  import { tenetWidgetReveal } from '../motion';
  import { onSettingsAbout, type SettingsAbout } from '../sdk/core';
  import { SETTINGS_SECTIONS, sectionIsAvailable, formatInstalledMemory, systemValue, type SettingsSectionId } from '../settings-model';
  import WindowControls from './WindowControls.svelte';
  import DisplaySettings from './DisplaySettings.svelte';

  let { onClose, onPopout, generation }: {
    onClose: () => void;
    onPopout: () => void;
    generation: number;
  } = $props();

  let detached = $state(false);
  let section = $state<SettingsSectionId>('about');
  let about = $state<SettingsAbout | null>(null);

  const fields = $derived([
    { key: 'kernel', label: 'Версия ядра', value: systemValue(about?.kernel) },
    { key: 'architecture', label: 'Архитектура', value: systemValue(about?.architecture) },
    { key: 'hostname', label: 'Имя устройства', value: systemValue(about?.hostname) },
    { key: 'cpu_model', label: 'Процессор', value: systemValue(about?.cpu_model) },
    { key: 'cpu_threads', label: 'Доступные логические потоки', value: about?.cpu_threads == null ? 'Недоступно' : String(about.cpu_threads) },
    { key: 'memory', label: 'Оперативная память', value: formatInstalledMemory(about?.memory_total_kib ?? null) },
    { key: 'session', label: 'Тип сессии', value: systemValue(about?.session) },
    { key: 'shell_version', label: 'Версия Kira Shell', value: systemValue(about?.shell_version) },
  ]);

  function navigate(id: string) {
    if (sectionIsAvailable(id)) section = id;
  }

  function onSelect(event: Event) {
    navigate((event.currentTarget as HTMLSelectElement).value);
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      onClose();
    }
  }

  onMount(() => {
    const stop = onSettingsAbout(generation, snapshot => { about = snapshot; });
    const onDetached = (event: Event) => {
      const current = (event as CustomEvent<{ generation: number }>).detail?.generation;
      if (current === generation) detached = true;
    };
    window.addEventListener('kira:overlay:detached', onDetached);
    return () => {
      stop();
      window.removeEventListener('kira:overlay:detached', onDetached);
    };
  });
</script>

<svelte:window onkeydown={onKeydown} />

<main data-kira-ui-root aria-label="Kira Settings" data-settings-mode={detached ? 'window' : 'widget'}
  use:tenetWidgetReveal={{ contact: '[data-kira-contact]', direction: 1, sections: 2 }}
  class="kira-settings relative flex min-h-0 w-full flex-col overflow-hidden bg-kira-bg text-kira-text">
  <header data-kira-enter class="flex shrink-0 items-center justify-between gap-3 border-b border-kira-muted/20 bg-kira-bg px-5 py-3">
    <div class="flex min-w-0 items-center gap-3">
      <span data-kira-contact class="flex size-9 shrink-0 items-center justify-center border border-kira-accent/70 bg-kira-accent/10 text-[16px] font-black text-kira-accent" aria-hidden="true">K</span>
      <div class="min-w-0">
        <p class="text-[9px] font-extrabold tracking-[0.20em] text-kira-accent">KIRA / CONFIGURATION</p>
        <h1 class="mt-0.5 truncate text-[16px] font-bold tracking-tight">Настройки системы</h1>
      </div>
    </div>
    <WindowControls {onClose} {onPopout} {generation} label="settings" />
  </header>

  <div data-kira-enter class="flex min-h-0 flex-1 overflow-hidden">
    {#if detached}
      <aside class="flex w-[220px] shrink-0 flex-col border-r border-kira-muted/20 bg-kira-surface-soft px-3 py-6"
        aria-label="Разделы настроек">
        <div class="mb-6 px-3">
          <p class="text-[9px] font-extrabold tracking-[0.19em] text-kira-muted">ПАРАМЕТРЫ</p>
          <p class="mt-2 text-[12px] font-semibold text-kira-text">Навигация</p>
        </div>
        <nav class="flex flex-col gap-1" aria-label="Разделы настроек">
          {#each SETTINGS_SECTIONS as item (item.id)}
            <button type="button" disabled={!item.available}
              onclick={() => navigate(item.id)}
              aria-current={section === item.id ? 'page' : undefined}
              class={['flex min-h-11 w-full items-center justify-between gap-2 border-l-2 px-3 text-left text-[12px] font-semibold transition-colors motion-reduce:transition-none',
                section === item.id ? 'border-kira-accent bg-kira-surface text-kira-text' : 'border-transparent text-kira-muted',
                item.available ? 'hover:bg-kira-surface' : 'cursor-not-allowed opacity-55'].join(' ')}>
              <span class="truncate">{item.label}</span>
              {#if !item.available}
                <span class="shrink-0 text-[8px] font-semibold uppercase tracking-wider">Скоро</span>
              {:else if section === item.id}
                <span aria-hidden="true" class="size-1.5 shrink-0 bg-kira-accent"></span>
              {/if}
            </button>
          {/each}
        </nav>
        <div class="mt-auto border-t border-kira-muted/20 px-3 pt-4 text-[10px] leading-relaxed text-kira-muted">
          Данные устройства. Локально и только для чтения.
        </div>
      </aside>
    {/if}

    <section class={['kira-settings-scroll min-w-0 flex-1 overflow-y-auto pb-7 pt-5', detached ? 'px-8' : 'px-5'].join(' ')} aria-label="Содержимое настроек">
      {#if !detached}
        <div class="mb-5">
          <label for="kira-settings-section" class="mb-2 block text-[10px] font-bold tracking-[0.16em] text-kira-muted">
            РАЗДЕЛ НАСТРОЕК
          </label>
          <div class="relative">
            <select id="kira-settings-section" aria-label="Раздел настроек" value={section} onchange={onSelect}
              class="w-full appearance-none rounded-none border border-kira-muted/40 bg-kira-surface px-3 py-3 pr-10 text-[12px] font-bold text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent">
              {#each SETTINGS_SECTIONS as item (item.id)}
                <option value={item.id} disabled={!item.available}>{item.label}{item.available ? '' : ' · скоро'}</option>
              {/each}
            </select>
            <span class="pointer-events-none absolute inset-y-0 right-3 flex items-center text-kira-accent" aria-hidden="true">
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="m6 9 6 6 6-6"/></svg>
            </span>
          </div>
        </div>
      {/if}

      {#if section === 'about'}
        <div class="mb-6 border-b border-kira-muted/20 pb-5">
          <div class="mb-3 flex items-center justify-between gap-3">
            <span class="text-[10px] font-bold tracking-[0.16em] text-kira-accent">01 / SYSTEM IDENTITY</span>
            <span class="border border-kira-muted/30 px-2 py-1 text-[9px] font-semibold tracking-[0.1em] text-kira-muted">READ ONLY</span>
          </div>
          <h2 class="text-[30px] font-black leading-tight tracking-tight">О системе</h2>
          <p class="mt-2 max-w-xl text-[12px] leading-relaxed text-kira-muted">
            Информация об операционной системе, устройстве и среде выполнения Kira Desktop.
          </p>
        </div>

        <div class="mb-5 border-l-[3px] border-kira-accent bg-kira-surface px-5 py-5">
          <p class="text-[10px] font-bold tracking-[0.14em] text-kira-muted">ОПЕРАЦИОННАЯ СИСТЕМА</p>
          <p class="mt-2 break-words text-[21px] font-extrabold leading-snug tracking-tight text-kira-text">
            {about ? systemValue(about.os_name) : 'Получение данных…'}
          </p>
          <p class="mt-3 text-[10px] font-semibold tracking-[0.12em] text-kira-accent">LOCAL HOST / LINUX</p>
        </div>

        <div class="mb-3 flex items-center justify-between">
          <h3 class="text-[12px] font-extrabold tracking-wide">Характеристики устройства</h3>
          <span class="text-[9px] font-bold tracking-widest text-kira-muted">DETAILS / 08</span>
        </div>
        <dl class="grid grid-cols-1 gap-px border border-kira-muted/20 bg-kira-muted/20 sm:grid-cols-2">
          {#each fields as field (field.key)}
            <div class="min-w-0 bg-kira-bg px-4 py-3">
              <dt class="text-[10px] font-semibold tracking-wide text-kira-muted">{field.label}</dt>
              <dd class="mt-1.5 break-words text-[12px] font-bold leading-relaxed text-kira-text">
                {about ? field.value : 'Получение данных…'}
              </dd>
            </div>
          {/each}
        </dl>
        <p class="mt-4 text-[10px] leading-relaxed text-kira-muted">
          Данные получены из локального ядра и системных файлов. Недоступные значения не подменяются приблизительными.
        </p>
      {:else if section === 'displays'}
        <DisplaySettings {generation} />
      {/if}
    </section>
  </div>
</main>
