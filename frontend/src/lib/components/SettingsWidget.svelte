<script lang="ts">
  import { onMount } from 'svelte';
  import { tenetFlowIndicator, tenetWidgetReveal } from '../motion';
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
  <header data-kira-enter class="flex shrink-0 items-center justify-between gap-3 border-b border-kira-muted/20 bg-kira-bg px-5 py-4">
    <div class="flex min-w-0 items-center gap-3 border-l-2 border-kira-accent pl-3">
      <span data-kira-contact class="text-[19px] font-black leading-none text-kira-accent" aria-hidden="true">K</span>
      <div class="min-w-0">
        <p class="text-[10px] font-extrabold tracking-[0.15em] text-kira-accent">KIRA / CONFIGURATION</p>
        <h1 class="mt-1 truncate text-[20px] font-extrabold leading-tight tracking-tight">Настройки системы</h1>
      </div>
    </div>
    <WindowControls {onClose} {onPopout} {generation} label="settings" />
  </header>

  <div data-kira-enter class="flex min-h-0 flex-1 overflow-hidden max-[720px]:flex-col">
    {#if detached}
      <aside class="flex w-[220px] shrink-0 flex-col overflow-y-auto border-r border-kira-muted/20 bg-kira-surface px-3 py-6 max-[720px]:hidden"
        aria-label="Разделы настроек">
        <div class="mb-6 px-3">
          <p class="text-[10px] font-extrabold tracking-[0.15em] text-kira-muted">РАЗДЕЛЫ</p>
          <p class="mt-2 text-[14px] font-extrabold text-kira-text">Управление</p>
        </div>
        <!-- One physical selection rail moves between real destinations.
             The current page and focus update before GSAP motion. -->
        <nav class="relative flex flex-col gap-1" aria-label="Разделы настроек"
          use:tenetFlowIndicator={{ active: section, axis: 'both' }}>
          <span data-kira-flow-indicator aria-hidden="true"
            class="pointer-events-none absolute left-0 top-0 z-0 border-l-[3px] border-kira-accent bg-kira-bg/35"></span>
          {#each SETTINGS_SECTIONS as item (item.id)}
            <button type="button" disabled={!item.available}
              data-kira-tab={item.id}
              onclick={() => navigate(item.id)}
              aria-current={section === item.id ? 'page' : undefined}
              class={['relative z-10 flex min-h-11 w-full items-center justify-between gap-2 px-4 text-left text-[13px] font-semibold focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-kira-accent',
                section === item.id ? 'text-kira-text' : 'text-kira-muted',
                item.available ? 'hover:text-kira-text' : 'cursor-not-allowed opacity-50'].join(' ')}>
              <span class="truncate">{item.label}</span>
              {#if !item.available}
                <span class="shrink-0 text-[10px] font-semibold text-kira-muted">Скоро</span>
              {/if}
            </button>
          {/each}
        </nav>
        <div class="mt-auto border-t border-kira-muted/20 px-3 pt-4 text-[11px] leading-relaxed text-kira-muted">
          Сведения локальны. Изменения дисплеев применяются и сохраняются отдельно.
        </div>
      </aside>
    {/if}

    <section class={['kira-settings-scroll min-w-0 flex-1 overflow-y-auto pb-7 pt-5', detached ? 'px-8 max-[720px]:px-5' : 'px-5'].join(' ')} aria-label="Содержимое настроек">
      <div class={detached ? 'mb-5 hidden max-[720px]:block' : 'mb-5'}>
          <label for="kira-settings-section" class="mb-2 block text-[10px] font-bold tracking-[0.16em] text-kira-muted">
            РАЗДЕЛ НАСТРОЕК
          </label>
          <div class="relative">
            <select id="kira-settings-section" aria-label="Раздел настроек" value={section} onchange={onSelect}
              class="w-full appearance-none rounded-none border border-kira-muted/40 bg-kira-surface px-3 py-3 pr-10 text-[13px] font-bold text-kira-text focus-visible:outline-2 focus-visible:outline-kira-accent">
              {#each SETTINGS_SECTIONS as item (item.id)}
                <option value={item.id} disabled={!item.available}>{item.label}{item.available ? '' : ' · скоро'}</option>
              {/each}
            </select>
            <span class="pointer-events-none absolute inset-y-0 right-3 flex items-center text-kira-accent" aria-hidden="true">
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="m6 9 6 6 6-6"/></svg>
            </span>
          </div>
      </div>

      {#if section === 'about'}
        <div class="mb-6 border-b border-kira-muted/20 pb-5">
          <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
            <span class="text-[10px] font-extrabold tracking-[0.15em] text-kira-accent">01 / SYSTEM IDENTITY</span>
            <span class="text-[10px] font-semibold tracking-wide text-kira-muted">ТОЛЬКО ЧТЕНИЕ</span>
          </div>
          <h2 class="text-[30px] font-black leading-tight tracking-tight">О системе</h2>
          <p class="mt-2 max-w-xl text-[12px] leading-relaxed text-kira-muted">
            Информация об операционной системе, устройстве и среде выполнения Kira Desktop.
          </p>
        </div>

        <!-- A single elevated fact, not a stack of ornamental cards. -->
        <div class="border-l-[3px] border-kira-accent bg-kira-surface px-5 py-5">
          <p class="text-[10px] font-bold tracking-[0.14em] text-kira-muted">ОПЕРАЦИОННАЯ СИСТЕМА</p>
          <p class="mt-2 break-words text-[23px] font-extrabold leading-snug tracking-tight text-kira-text">
            {about ? systemValue(about.os_name) : 'Получение данных…'}
          </p>
          <p class="mt-3 text-[10px] font-semibold tracking-[0.12em] text-kira-accent">LOCAL HOST / LINUX</p>
        </div>

        <div class="mb-2 mt-7 flex items-center justify-between gap-2">
          <h3 class="text-[14px] font-extrabold tracking-wide">Характеристики устройства</h3>
          <span class="text-[10px] font-bold tracking-widest text-kira-muted">08 / ФАКТОВ</span>
        </div>
        <dl class="grid grid-cols-1 gap-x-8 sm:grid-cols-2">
          {#each fields as field (field.key)}
            <div class="min-w-0 border-b border-kira-muted/20 py-3">
              <dt class="text-[11px] font-semibold tracking-wide text-kira-muted">{field.label}</dt>
              <dd class="mt-1.5 break-words text-[13px] font-bold leading-relaxed text-kira-text">
                {about ? field.value : 'Получение данных…'}
              </dd>
            </div>
          {/each}
        </dl>
        <p class="mt-5 border-l-2 border-kira-muted/30 pl-3 text-[11px] leading-relaxed text-kira-muted">
          Данные из локальной системы. Недоступные значения не подменяются приблизительными.
          Этот раздел не изменяет конфигурацию устройства.
        </p>
      {:else if section === 'displays'}
        <DisplaySettings {generation} />
      {/if}
    </section>
  </div>
</main>
