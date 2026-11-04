<script lang="ts">
  import { onMount } from 'svelte';
  import { monitorLabel, UI_LOCALE } from '../ui-locale';
  import WindowControls from './WindowControls.svelte';
  import SystemCharts from './SystemCharts.svelte';
  import { pulseReading, tenetDisclosure, tenetFlowIndicator, tenetPanelArrival, tenetReflow, tenetReflowScope, tenetWidgetReveal } from '../motion';
  import {
    onSystemOverlayData, onSystemProcesses,
    type SystemOverlayData, type SystemProcesses,
  } from '../sdk/core';
  import {
    defaultSort, fractionLabel, isSampleFresh, nextSort, percentUsedPrecise, processMemoryLabel,
    sortedProcesses, systemHeadline, temperatureLabel, uptimeLabel, usageLabel, type SortField,
  } from '../system-model';

  let { onClose, onPopout, onCopy, onTab, generation }: {
    onClose: () => void;
    onPopout: () => void;
    onCopy: () => void;
    onTab: (tab: 'overview' | 'processes') => void;
    generation: number;
  } = $props();

  let status = $state<SystemOverlayData | null>(null);
  let processes = $state<SystemProcesses | null>(null);
  let active = $state<'overview' | 'processes'>('overview');
  let query = $state('');
  let sort = $state({ ...defaultSort });
  let refreshed = $state('');
  let lastSampleAt = $state<number | null>(null);
  let processesLastReceivedAt = $state<number | null>(null);
  let now = $state(Date.now());
  let visibleCount = $state(36);
  let processList: HTMLDivElement;
  const metrics = $derived(status?.metrics ?? null);
  const sampleFresh = $derived(isSampleFresh(lastSampleAt, now));
  // Keep expired metrics out of every numeric widget, not just the headline.
  const liveMetrics = $derived(sampleFresh ? metrics : null);
  // The process table has its own two-second native sampler. Expire old rows
  // independently from the overview telemetry when that worker stops.
  const processesFresh = $derived(isSampleFresh(processesLastReceivedAt, now, 8_000));
  const liveProcesses = $derived(processesFresh ? processes : null);
  const health = $derived(status === null ? 'ОЖИДАНИЕ ТЕЛЕМЕТРИИ' :
    !sampleFresh ? 'ДАННЫЕ УСТАРЕЛИ' : systemHeadline(metrics, status.core, status.niri));
  const swapPercent = $derived(percentUsedPrecise(liveMetrics?.swap_total_kib ?? null, liveMetrics?.swap_free_kib ?? null));
  const storagePercent = $derived(percentUsedPrecise(liveMetrics?.root_total_kib ?? null, liveMetrics?.root_available_kib ?? null));
  const matching = $derived(sortedProcesses(liveProcesses?.rows ?? [], query, sort));
  const visible = $derived(matching.slice(0, visibleCount));


  onMount(() => {
    const clock = window.setInterval(() => { now = Date.now(); }, 2_000);
    const stopMetrics = onSystemOverlayData(data => {
      const receivedAt = Date.now();
      status = data;
      lastSampleAt = receivedAt;
      now = receivedAt;
      refreshed = new Date(receivedAt).toLocaleTimeString(UI_LOCALE);
    });
    const stopProcesses = onSystemProcesses(data => {
      processes = data;
      processesLastReceivedAt = Date.now();
      now = processesLastReceivedAt;
    });
    return () => {
      window.clearInterval(clock);
      stopMetrics();
      stopProcesses();
      onTab('overview');
    };
  });

  function switchTab(tab: 'overview' | 'processes') {
    if (active === tab) return;
    active = tab;
    if (tab === 'processes') {
      processes = null;
      processesLastReceivedAt = null;
      visibleCount = 36;
    }
    onTab(tab);
  }

  function changeSort(field: SortField) {
    tenetReflow(processList, () => {
      sort = nextSort(sort, field);
      visibleCount = 36;
    });
  }

  function filterProcesses(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    tenetReflow(processList, () => {
      query = input.value;
      visibleCount = 36;
    });
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={keydown} />

<main data-kira-ui-root aria-label="Kira System Monitor"
  use:tenetWidgetReveal={{ contact: '[data-kira-contact]', direction: 1, sections: 2 }}
  class="kira-monitor-scroll relative flex h-auto min-h-0 w-full flex-col overflow-y-auto bg-kira-bg text-kira-text">
  <header data-kira-enter class="shrink-0 border-b border-kira-muted/15 bg-kira-bg px-5 pb-4 pt-5">
    <div class="flex items-start justify-between gap-3">
      <div class="border-l-2 border-kira-accent pl-3">
        <h1 data-kira-contact class="text-[25px] font-extrabold leading-tight tracking-tight">Монитор системы</h1>
      </div>
      <WindowControls {onClose} {onPopout} {generation} label="system monitor" />
    </div>
    {#if health !== 'ALL SYSTEMS NOMINAL'}
      <p class="mt-3 text-[11px] font-semibold text-kira-accent" role="status" aria-live="polite">{monitorLabel(health)}</p>
    {/if}
  </header>

  <nav data-kira-enter use:tenetFlowIndicator={{ active }} class="relative grid shrink-0 grid-cols-2 bg-kira-surface-soft" aria-label="Разделы монитора системы">
    <span data-kira-flow-indicator aria-hidden="true" class="pointer-events-none absolute bottom-0 left-0 z-10 h-0.5 bg-kira-accent"></span>
    <button data-kira-tab="overview" type="button" onclick={() => switchTab('overview')} aria-current={active === 'overview' ? 'page' : undefined}
      class={'border-b-2 border-transparent px-4 py-3 text-left text-[11px] font-bold tracking-wide focus-visible:outline-2 focus-visible:outline-kira-accent ' +
        (active === 'overview' ? 'text-kira-accent' : 'text-kira-muted hover:text-kira-text')}>
      ОБЗОР
    </button>
    <button data-kira-tab="processes" type="button" onclick={() => switchTab('processes')} aria-current={active === 'processes' ? 'page' : undefined}
      class={'border-b-2 border-transparent px-4 py-3 text-left text-[11px] font-bold tracking-wide focus-visible:outline-2 focus-visible:outline-kira-accent ' +
        (active === 'processes' ? 'text-kira-accent' : 'text-kira-muted hover:text-kira-text')}>
      ПРОЦЕССЫ
    </button>
  </nav>

  {#if active === 'overview'}
    <div use:tenetPanelArrival class="flex min-h-0 flex-1 flex-col">
      <section data-kira-enter class="bg-kira-surface px-5 pb-5 pt-4" aria-label="Телеметрия системы">
        <SystemCharts metrics={liveMetrics} {now} />
      </section>

      <section data-kira-enter class="bg-kira-bg px-5 py-4" aria-label="Дополнительные показатели ресурсов">
        <h2 class="mb-3 text-[12px] font-bold tracking-wide text-kira-text">ЁМКОСТЬ</h2>
        <div class="grid grid-cols-2 gap-3">
          <article class="min-w-0 bg-kira-surface-soft px-3 py-3">
            <h3 class="text-[12px] font-bold tracking-wide text-kira-muted">ОСНОВНОЙ ДИСК</h3>
            <div class="mt-2 text-[24px] font-extrabold leading-none tracking-tight tabular-nums">
              {usageLabel(liveMetrics?.root_total_kib ?? null, liveMetrics?.root_available_kib ?? null)}
            </div>
            {#if storagePercent !== null}
              <div role="progressbar" aria-label="Занято на основном диске" aria-valuemin={0} aria-valuemax={100}
                aria-valuenow={storagePercent} class="mt-3 h-1 bg-kira-muted/15">
                <div class="h-full bg-kira-accent" style:width={storagePercent + '%'}></div>
              </div>
            {:else}
              <div class="mt-3 h-1 bg-kira-muted/15" aria-label="Нет данных об использовании диска"></div>
            {/if}
            <p class="mt-2 truncate text-[11px] text-kira-muted"
              title={fractionLabel(liveMetrics?.root_total_kib ?? null, liveMetrics?.root_available_kib ?? null)}>
              {fractionLabel(liveMetrics?.root_total_kib ?? null, liveMetrics?.root_available_kib ?? null)}
            </p>
          </article>
          <article class="min-w-0 bg-kira-surface-soft px-3 py-3">
            <h3 class="text-[12px] font-bold tracking-wide text-kira-muted">ПАМЯТЬ SWAP</h3>
            <div class="mt-2 text-[24px] font-extrabold leading-none tracking-tight tabular-nums">
              {usageLabel(liveMetrics?.swap_total_kib ?? null, liveMetrics?.swap_free_kib ?? null)}
            </div>
            {#if swapPercent !== null}
              <div role="progressbar" aria-label="Использование swap" aria-valuemin={0} aria-valuemax={100}
                aria-valuenow={swapPercent} class="mt-3 h-1 bg-kira-muted/15">
                <div class="h-full bg-kira-accent" style:width={swapPercent + '%'}></div>
              </div>
            {:else}
              <div class="mt-3 h-1 bg-kira-muted/15" aria-label="Нет данных об использовании swap"></div>
            {/if}
            <p class="mt-2 truncate text-[11px] text-kira-muted"
              title={fractionLabel(liveMetrics?.swap_total_kib ?? null, liveMetrics?.swap_free_kib ?? null)}>
              {fractionLabel(liveMetrics?.swap_total_kib ?? null, liveMetrics?.swap_free_kib ?? null)}
            </p>
          </article>
        </div>
      </section>

      <section data-kira-enter class="bg-kira-surface-soft px-5 py-4"
        aria-label="Температуры оборудования и графика">
        <div class="flex items-center justify-between gap-3">
          <h2 class="text-[12px] font-bold tracking-[0.08em] text-kira-text">ОБОРУДОВАНИЕ</h2>
          <div class="flex items-baseline gap-2">
            <span class="text-[11px] font-bold tracking-wider text-kira-muted">CPU · ТЕМП</span>
            <span class="text-[14px] font-extrabold tabular-nums"
              use:pulseReading={temperatureLabel(liveMetrics?.cpu_temperature_c ?? null)}>
              {temperatureLabel(liveMetrics?.cpu_temperature_c ?? null)}
            </span>
          </div>
        </div>
        <div class="mt-3 space-y-2">
          {#each liveMetrics?.gpus ?? [] as gpu (gpu.label)}
            <div class="flex min-w-0 items-center justify-between gap-3 border-t border-kira-muted/15 pt-2 text-[11px]">
              <span class="min-w-0 flex-1 truncate font-semibold" title={gpu.label}>{gpu.label}</span>
              <div class="flex shrink-0 items-center gap-3">
                <span class="tabular-nums text-kira-muted" title="Загрузка GPU">
                  {gpu.utilization_percent === null ? 'НАГРУЗКА Н/Д' : gpu.utilization_percent + '%'}
                </span>
                <span class="font-bold tabular-nums" title="Температура GPU"
                  use:pulseReading={temperatureLabel(gpu.temperature_c)}>
                  {gpu.temperature_c === null ? 'ТЕМП Н/Д' : temperatureLabel(gpu.temperature_c)}
                </span>
              </div>
            </div>
          {:else}
            <p class="text-[11px] text-kira-muted">{liveMetrics === null ? 'ДАННЫЕ GPU НЕДОСТУПНЫ' : 'GPU НЕ ОБНАРУЖЕН'}</p>
          {/each}
        </div>
      </section>

      <details data-kira-enter use:tenetDisclosure class="group border-t border-kira-muted/15 bg-kira-bg px-5 py-3"
        aria-label="Дополнительные сведения о системе">
        <summary class="flex cursor-pointer list-none items-center justify-between gap-3 py-1
          text-[12px] font-bold tracking-[0.08em] text-kira-text
          focus-visible:outline-2 focus-visible:outline-kira-accent">
          <span>СВЕДЕНИЯ О СИСТЕМЕ</span>
          <span aria-hidden="true" class="text-sm text-kira-muted group-open:hidden">+</span>
          <span aria-hidden="true" class="hidden text-sm text-kira-muted group-open:inline">−</span>
        </summary>
        <div data-kira-disclosure-content class="mt-3 grid grid-cols-2 gap-x-5 gap-y-3 border-t border-kira-muted/15 pt-3">
          <div class="min-w-0">
            <div class="text-[11px] font-bold tracking-wider text-kira-muted">ВРЕМЯ РАБОТЫ</div>
            <div class="mt-1 truncate text-[11px] font-bold tabular-nums">
              {uptimeLabel(liveMetrics?.uptime_seconds ?? null)}
            </div>
          </div>
          <div class="min-w-0">
            <div class="text-[11px] font-bold tracking-wider text-kira-muted">НАГРУЗКА / 1 5 15</div>
            <div class="mt-1 truncate text-[11px] font-bold tabular-nums">
              {liveMetrics?.load?.map(n => n.toFixed(2)).join(' / ') ?? 'НЕДОСТУПНО'}
            </div>
          </div>
          <div class="col-span-2 flex items-center justify-between border-t border-kira-muted/15 pt-2">
            <span class="text-[11px] font-bold tracking-wider text-kira-muted">ПИТАНИЕ</span>
            <span class="text-right text-[11px] font-bold">
              {liveMetrics === null ? 'НЕДОСТУПНО' : liveMetrics.battery_percent == null ? 'НЕ ОБНАРУЖЕНО' :
                liveMetrics.battery_percent + '%' + (liveMetrics.battery_charging === true ? ' / ЗАРЯДКА' :
                  liveMetrics.battery_charging === false ? ' / РАЗРЯДКА' : '')}
            </span>
          </div>
        </div>
      </details>

      <footer data-kira-enter class="flex items-center justify-between gap-3 bg-kira-bg px-5 py-3">
        {#if status !== null && !sampleFresh}
          <span role="status" class="text-[11px] font-medium text-kira-muted">ПОСЛЕДНЕЕ ОБНОВЛЕНИЕ · {refreshed}</span>
        {/if}
        <button type="button" onclick={onCopy}
          class="px-1 py-2 text-[11px] font-bold tracking-wide text-kira-accent
            focus-visible:outline-2 focus-visible:outline-kira-accent">КОПИРОВАТЬ ОТЧЁТ</button>
      </footer>
    </div>
  {:else}
    <section use:tenetPanelArrival class="flex min-h-0 flex-1 flex-col bg-kira-surface px-4 py-4"
      aria-label="Процессы">
      <input data-kira-enter type="search" aria-label="Фильтр по названию процесса или PID"
        placeholder="Фильтр по имени или PID" value={query}
        oninput={filterProcesses}
        class="mb-3 min-h-9 w-full rounded bg-kira-bg px-3 py-2 text-xs text-kira-text outline-none
          placeholder:text-kira-muted focus-visible:outline-2 focus-visible:outline-kira-accent"/>
      <div data-kira-enter use:tenetFlowIndicator={{ active: sort.field }}
        class="relative grid grid-cols-[minmax(0,1fr)_42px_50px_65px_65px] gap-1 border-b border-kira-muted/20 pb-2">
        <span data-kira-flow-indicator aria-hidden="true"
          class="pointer-events-none absolute bottom-0 left-0 z-0 h-0.5 bg-kira-accent"></span>
        {#each [{field:'name',label:'ПРОЦЕСС'}, {field:'pid',label:'PID'}, {field:'cpu',label:'CPU'}, {field:'memory',label:'RSS'}, {field:'pss',label:'PSS'}] as column}
          <button data-kira-tab={column.field} type="button" onclick={() => changeSort(column.field as SortField)}
            aria-pressed={sort.field === column.field}
            class={'truncate text-right text-[11px] font-bold tracking-wide hover:text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent ' +
              (column.field === 'name' ? 'text-left ' : '') +
              (sort.field === column.field ? 'text-kira-accent' : 'text-kira-muted')}
            aria-label={'Сортировать по ' + column.label}
             title={column.field === 'memory' ? 'Память процесса (RSS)' :
               column.field === 'pss' ? 'Пропорциональная память процесса (PSS)' : 'Сортировать по ' + column.label}>
            {column.label}{sort.field === column.field ? (sort.descending ? ' ↓' : ' ↑') : ''}
          </button>
        {/each}
      </div>
      <div bind:this={processList} data-kira-enter use:tenetReflowScope class="kira-monitor-scroll min-h-0 flex-1 overflow-y-auto" aria-live="off">
        {#each visible as process (process.pid)}
          <div data-kira-reflow class="grid min-h-9 grid-cols-[minmax(0,1fr)_42px_50px_65px_65px] items-center gap-1 border-b border-kira-muted/10 text-[11px]"
            title={process.name + ' / PID ' + process.pid + ' / СОСТОЯНИЕ ' + process.state +
              ' / RSS ' + processMemoryLabel(process.rss_kib) + ' / PSS ' + processMemoryLabel(process.pss_kib)}>
            <span class="truncate font-semibold">{process.name}</span>
            <span class="text-right tabular-nums text-kira-muted">{process.pid}</span>
            <span class="text-right tabular-nums">{process.cpu_percent === null ? '—' : process.cpu_percent.toFixed(1) + '%'}</span>
            <span class="text-right tabular-nums">{processMemoryLabel(process.rss_kib)}</span>
            <span class="text-right tabular-nums">{processMemoryLabel(process.pss_kib)}</span>
          </div>
        {:else}
          <p class="py-9 text-center text-xs text-kira-muted">{liveProcesses?.error ?
            'ДАННЫЕ ПРОЦЕССОВ НЕДОСТУПНЫ: ' + liveProcesses.error :
            processes !== null && !processesFresh ? 'ДАННЫЕ ПРОЦЕССОВ УСТАРЕЛИ' :
            liveProcesses && liveProcesses.rows.length ? 'ПРОЦЕССЫ НЕ НАЙДЕНЫ' : 'СБОР ДАННЫХ…'}</p>
        {/each}
        {#if visible.length < matching.length && visible.length < 180}
          <button type="button" onclick={() => { visibleCount = Math.min(180, visibleCount + 36); }}
            class="mt-3 w-full border border-kira-muted/20 px-3 py-2 text-[11px] font-bold tracking-wider
              text-kira-accent hover:bg-kira-bg focus-visible:outline-2 focus-visible:outline-kira-accent">
            ПОКАЗАТЬ ЕЩЁ
          </button>
        {/if}
      </div>
      {#if query.trim() || liveProcesses?.truncated || visible.length < matching.length || (processes !== null && !processesFresh)}
        <p data-kira-enter class="shrink-0 pt-3 text-[11px] font-medium text-kira-muted" role="status">
          {visible.length} из {matching.length}
          {liveProcesses?.truncated ? ' · ДОСТИГНУТ ЛИМИТ СКАНИРОВАНИЯ' : ''}
          {visible.length === 180 && matching.length > 180 ? ' · УТОЧНИТЕ ФИЛЬТР' : ''}
        </p>
      {/if}
    </section>
  {/if}
</main>
