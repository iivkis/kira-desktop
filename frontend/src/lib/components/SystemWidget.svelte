<script lang="ts">
  import { onMount } from 'svelte';
  import SystemCharts from './SystemCharts.svelte';
  import { pulseReading, revealStagger } from '../motion';
  import {
    onSystemOverlayData, onSystemProcesses,
    type SystemOverlayData, type SystemProcesses,
  } from '../sdk/core';
  import {
    defaultSort, fractionLabel, isSampleFresh, nextSort, percentUsedPrecise, processMemoryLabel,
    sortedProcesses, systemHeadline, temperatureLabel, uptimeLabel, usageLabel, type SortField,
  } from '../system-model';

  let { onClose, onCopy, onTab }: {
    onClose: () => void;
    onCopy: () => void;
    onTab: (tab: 'overview' | 'processes') => void;
  } = $props();

  let status = $state<SystemOverlayData | null>(null);
  let processes = $state<SystemProcesses | null>(null);
  let active = $state<'overview' | 'processes'>('overview');
  let query = $state('');
  let sort = $state({ ...defaultSort });
  let refreshed = $state('');
  let lastSampleAt = $state<number | null>(null);
  let now = $state(Date.now());
  let visibleCount = $state(36);
  const metrics = $derived(status?.metrics ?? null);
  const sampleFresh = $derived(isSampleFresh(lastSampleAt, now));
  const health = $derived(status === null ? 'AWAITING TELEMETRY' :
    !sampleFresh ? 'TELEMETRY STALE' : systemHeadline(metrics, status.core, status.niri));
  const swapPercent = $derived(percentUsedPrecise(metrics?.swap_total_kib ?? null, metrics?.swap_free_kib ?? null));
  const storagePercent = $derived(percentUsedPrecise(metrics?.root_total_kib ?? null, metrics?.root_available_kib ?? null));
  const matching = $derived(sortedProcesses(processes?.rows ?? [], query, sort));
  const visible = $derived(matching.slice(0, visibleCount));


  onMount(() => {
    const clock = window.setInterval(() => { now = Date.now(); }, 2_000);
    const stopMetrics = onSystemOverlayData(data => {
      const receivedAt = Date.now();
      status = data;
      lastSampleAt = receivedAt;
      now = receivedAt;
      refreshed = new Date(receivedAt).toLocaleTimeString();
    });
    const stopProcesses = onSystemProcesses(data => { processes = data; });
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
      visibleCount = 36;
    }
    onTab(tab);
  }

  function changeSort(field: SortField) {
    sort = nextSort(sort, field);
    visibleCount = 36;
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
  class="kira-monitor-scroll flex h-auto min-h-0 w-full flex-col overflow-y-auto bg-kira-bg text-kira-text">
  <header class="shrink-0 border-b border-kira-muted/15 bg-kira-bg px-5 pb-4 pt-5">
    <div class="flex items-start justify-between gap-3">
      <div class="border-l-2 border-kira-accent pl-3">
        <div class="text-[10px] font-bold tracking-[0.15em] text-kira-accent">KIRA / HOST INSTRUMENTS</div>
        <h1 class="mt-2 text-[25px] font-extrabold leading-none tracking-tight">System monitor</h1>
        <p class="mt-2 text-[10px] font-medium tracking-wide text-kira-muted">LOCAL TELEMETRY / READ ONLY</p>
      </div>
      <button type="button" onclick={onClose} aria-label="Close system monitor" title="Close (Esc)"
        class="rounded px-2 py-1 text-xl text-kira-muted hover:text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent">×</button>
    </div>
    <div class="mt-4 flex min-w-0 items-center justify-between gap-3 border-t border-kira-muted/15 pt-3"
      role="status" aria-live="polite">
      <div class="flex min-w-0 items-center gap-2">
        <span class="h-1.5 w-1.5 shrink-0 rounded-full"
          class:bg-kira-accent={status !== null && sampleFresh && health === 'ALL SYSTEMS NOMINAL'}
          class:bg-kira-muted={status === null || !sampleFresh || health !== 'ALL SYSTEMS NOMINAL'}></span>
        <span class="truncate text-[10px] font-bold tracking-wide"
          class:text-kira-accent={health === 'ALL SYSTEMS NOMINAL'}
          class:text-kira-muted={health !== 'ALL SYSTEMS NOMINAL'}>{health}</span>
      </div>
      <span class="shrink-0 text-right text-[9px] font-semibold tracking-wide text-kira-muted">
        {status === null ? 'NO SAMPLE' : sampleFresh ? 'CURRENT SAMPLE' : 'LAST SAMPLE / STALE'}
      </span>
    </div>
  </header>

  <nav class="grid shrink-0 grid-cols-2 bg-kira-surface-soft" aria-label="System monitor sections">
    <button type="button" onclick={() => switchTab('overview')} aria-current={active === 'overview' ? 'page' : undefined}
      class={'border-b-2 px-4 py-3 text-left text-[11px] font-bold tracking-wide focus-visible:outline-2 focus-visible:outline-kira-accent ' +
        (active === 'overview' ? 'border-kira-accent text-kira-accent' : 'border-transparent text-kira-muted hover:text-kira-text')}>
      01 / OVERVIEW
    </button>
    <button type="button" onclick={() => switchTab('processes')} aria-current={active === 'processes' ? 'page' : undefined}
      class={'border-b-2 px-4 py-3 text-left text-[11px] font-bold tracking-wide focus-visible:outline-2 focus-visible:outline-kira-accent ' +
        (active === 'processes' ? 'border-kira-accent text-kira-accent' : 'border-transparent text-kira-muted hover:text-kira-text')}>
      02 / PROCESSES
    </button>
  </nav>

  {#if active === 'overview'}
    <div class="flex min-h-0 flex-1 flex-col" use:revealStagger>
      <section data-kira-enter class="bg-kira-surface px-5 pb-5 pt-4" aria-label="Live system telemetry">
        <div class="mb-4 flex items-center justify-between gap-2 border-b border-kira-muted/15 pb-3">
          <div class="flex items-center gap-2">
            <span class="text-[9px] font-bold tracking-wider text-kira-muted">CORE</span>
            <span class="text-[9px] font-bold tracking-wide"
              class:text-kira-accent={status?.core} class:text-kira-muted={!status?.core}>
              {status === null ? 'UNKNOWN' : status.core ? 'ONLINE' : 'OFFLINE'}
            </span>
          </div>
          <div class="flex items-center gap-2">
            <span class="text-[9px] font-bold tracking-wider text-kira-muted">NIRI</span>
            <span class="text-[9px] font-bold tracking-wide"
              class:text-kira-accent={status?.niri} class:text-kira-muted={!status?.niri}>
              {status === null ? 'UNKNOWN' : status.niri ? 'ONLINE' : 'OFFLINE'}
            </span>
          </div>
          <span class="text-[9px] font-semibold tracking-wide text-kira-muted">NATIVE FEED</span>
        </div>
        <SystemCharts {metrics} {now} />
      </section>

      <section data-kira-enter class="bg-kira-bg px-5 py-4" aria-label="Secondary resource metrics">
        <div class="mb-3 flex items-center justify-between gap-2">
          <h2 class="text-[10px] font-extrabold tracking-[0.15em] text-kira-accent">03 / CAPACITY</h2>
          <span class="text-[9px] font-semibold tracking-wide text-kira-muted">USED / TOTAL</span>
        </div>
        <div class="grid grid-cols-2 gap-3">
          <article class="min-w-0 border border-kira-muted/15 bg-kira-surface-soft px-3 py-3">
            <h3 class="text-[9px] font-bold tracking-[0.12em] text-kira-muted">ROOT STORAGE</h3>
            <div class="mt-2 text-[24px] font-extrabold leading-none tracking-tight tabular-nums">
              {usageLabel(metrics?.root_total_kib ?? null, metrics?.root_available_kib ?? null)}
            </div>
            {#if storagePercent !== null}
              <div role="progressbar" aria-label="Root storage used" aria-valuemin={0} aria-valuemax={100}
                aria-valuenow={storagePercent} class="mt-3 h-1 bg-kira-muted/15">
                <div class="h-full bg-kira-accent" style:width={storagePercent + '%'}></div>
              </div>
            {:else}
              <div class="mt-3 h-1 bg-kira-muted/15" aria-label="Root storage usage unavailable"></div>
            {/if}
            <p class="mt-2 truncate text-[10px] text-kira-muted"
              title={fractionLabel(metrics?.root_total_kib ?? null, metrics?.root_available_kib ?? null)}>
              {fractionLabel(metrics?.root_total_kib ?? null, metrics?.root_available_kib ?? null)}
            </p>
          </article>
          <article class="min-w-0 border border-kira-muted/15 bg-kira-surface-soft px-3 py-3">
            <h3 class="text-[9px] font-bold tracking-[0.12em] text-kira-muted">SWAP MEMORY</h3>
            <div class="mt-2 text-[24px] font-extrabold leading-none tracking-tight tabular-nums">
              {usageLabel(metrics?.swap_total_kib ?? null, metrics?.swap_free_kib ?? null)}
            </div>
            {#if swapPercent !== null}
              <div role="progressbar" aria-label="Swap memory used" aria-valuemin={0} aria-valuemax={100}
                aria-valuenow={swapPercent} class="mt-3 h-1 bg-kira-muted/15">
                <div class="h-full bg-kira-accent" style:width={swapPercent + '%'}></div>
              </div>
            {:else}
              <div class="mt-3 h-1 bg-kira-muted/15" aria-label="Swap memory usage unavailable"></div>
            {/if}
            <p class="mt-2 truncate text-[10px] text-kira-muted"
              title={fractionLabel(metrics?.swap_total_kib ?? null, metrics?.swap_free_kib ?? null)}>
              {fractionLabel(metrics?.swap_total_kib ?? null, metrics?.swap_free_kib ?? null)}
            </p>
          </article>
        </div>
      </section>

      <section data-kira-enter class="bg-kira-surface-soft px-5 py-4"
        aria-label="Hardware temperatures and graphics">
        <div class="flex items-center justify-between gap-3">
          <h2 class="text-[10px] font-extrabold tracking-[0.15em] text-kira-accent">04 / HARDWARE</h2>
          <div class="flex items-baseline gap-2">
            <span class="text-[9px] font-bold tracking-wider text-kira-muted">CPU TEMP</span>
            <span class="text-[14px] font-extrabold tabular-nums"
              use:pulseReading={temperatureLabel(metrics?.cpu_temperature_c ?? null)}>
              {temperatureLabel(metrics?.cpu_temperature_c ?? null)}
            </span>
          </div>
        </div>
        <div class="mt-3 space-y-2">
          {#each metrics?.gpus ?? [] as gpu (gpu.label)}
            <div class="flex min-w-0 items-center justify-between gap-3 border-t border-kira-muted/15 pt-2 text-[11px]">
              <span class="min-w-0 flex-1 truncate font-semibold" title={gpu.label}>{gpu.label}</span>
              <div class="flex shrink-0 items-center gap-3">
                <span class="tabular-nums text-kira-muted" title="GPU utilization">
                  {gpu.utilization_percent === null ? 'LOAD N/A' : gpu.utilization_percent + '%'}
                </span>
                <span class="font-bold tabular-nums" title="GPU temperature"
                  use:pulseReading={temperatureLabel(gpu.temperature_c)}>
                  {gpu.temperature_c === null ? 'TEMP N/A' : temperatureLabel(gpu.temperature_c)}
                </span>
              </div>
            </div>
          {:else}
            <p class="text-[10px] text-kira-muted">NO GPU DETECTED</p>
          {/each}
        </div>
      </section>

      <details data-kira-enter class="group border-t border-kira-muted/15 bg-kira-bg px-5 py-3"
        aria-label="Additional host diagnostics">
        <summary class="flex cursor-pointer list-none items-center justify-between gap-3 py-1
          text-[10px] font-extrabold tracking-[0.12em] text-kira-accent
          focus-visible:outline-2 focus-visible:outline-kira-accent">
          <span>05 / HOST DETAILS</span>
          <span class="text-[9px] font-semibold text-kira-muted group-open:hidden">EXPAND +</span>
          <span class="hidden text-[9px] font-semibold text-kira-muted group-open:inline">COLLAPSE −</span>
        </summary>
        <div class="mt-3 grid grid-cols-2 gap-x-5 gap-y-3 border-t border-kira-muted/15 pt-3">
          <div class="min-w-0">
            <div class="text-[9px] font-bold tracking-wider text-kira-muted">UPTIME</div>
            <div class="mt-1 truncate text-[11px] font-bold tabular-nums">
              {uptimeLabel(metrics?.uptime_seconds ?? null)}
            </div>
          </div>
          <div class="min-w-0">
            <div class="text-[9px] font-bold tracking-wider text-kira-muted">LOAD / 1 5 15</div>
            <div class="mt-1 truncate text-[11px] font-bold tabular-nums">
              {metrics?.load?.map(n => n.toFixed(2)).join(' / ') ?? 'UNAVAILABLE'}
            </div>
          </div>
          <div class="col-span-2 flex items-center justify-between border-t border-kira-muted/15 pt-2">
            <span class="text-[9px] font-bold tracking-wider text-kira-muted">POWER</span>
            <span class="text-right text-[10px] font-bold">
              {metrics?.battery_percent == null ? 'NOT DETECTED' :
                metrics.battery_percent + '%' + (metrics.battery_charging === true ? ' / CHARGING' :
                  metrics.battery_charging === false ? ' / DISCHARGING' : '')}
            </span>
          </div>
        </div>
      </details>

      <footer data-kira-enter class="flex items-center justify-between gap-3 bg-kira-bg px-5 py-3">
        <span class="text-[9px] font-semibold tracking-wider text-kira-muted">
          {status === null ? 'WAITING FOR DATA' : sampleFresh ? 'UPDATED / ' + refreshed : 'LAST UPDATE / ' + refreshed + ' / STALE'}
        </span>
        <button type="button" onclick={onCopy}
          class="px-1 py-2 text-[10px] font-bold tracking-wide text-kira-accent
            focus-visible:outline-2 focus-visible:outline-kira-accent">COPY REPORT</button>
      </footer>
    </div>
  {:else}
    <section class="flex min-h-0 flex-1 flex-col bg-kira-surface px-4 py-4"
      aria-label="Processes" use:revealStagger>
      <div data-kira-enter class="mb-3 flex items-center justify-between gap-2">
        <h2 class="text-[10px] font-bold tracking-[0.12em] text-kira-accent">PROCESSES / HOST</h2>
        <span class="text-[9px] font-semibold tracking-wide text-kira-muted">READ ONLY</span>
      </div>
      <input data-kira-enter type="search" aria-label="Filter by process name or PID"
        placeholder="Filter by process name or PID" bind:value={query}
        oninput={() => { visibleCount = 36; }}
        class="mb-3 min-h-9 w-full rounded bg-kira-bg px-3 py-2 text-xs text-kira-text outline-none
          placeholder:text-kira-muted focus-visible:outline-2 focus-visible:outline-kira-accent"/>
      <div data-kira-enter class="grid grid-cols-[minmax(0,1fr)_42px_50px_65px_65px] gap-1 border-b border-kira-muted/20 pb-2">
        {#each [{field:'name',label:'PROCESS'}, {field:'pid',label:'PID'}, {field:'cpu',label:'CPU'}, {field:'memory',label:'RSS'}, {field:'pss',label:'PSS'}] as column}
          <button type="button" onclick={() => changeSort(column.field as SortField)}
            class={'truncate text-right text-[10px] font-bold tracking-wide hover:text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent ' +
              (column.field === 'name' ? 'text-left ' : '') +
              (sort.field === column.field ? 'text-kira-accent' : 'text-kira-muted')}
            aria-label={'Sort by ' + column.label}>
            {column.label}{sort.field === column.field ? (sort.descending ? ' ↓' : ' ↑') : ''}
          </button>
        {/each}
      </div>
      <div data-kira-enter class="kira-monitor-scroll min-h-0 flex-1 overflow-y-auto" aria-live="off">
        {#each visible as process (process.pid)}
          <div class="grid min-h-9 grid-cols-[minmax(0,1fr)_42px_50px_65px_65px] items-center gap-1 border-b border-kira-muted/10 text-[11px]"
            title={process.name + ' / PID ' + process.pid + ' / STATE ' + process.state +
              ' / RSS ' + processMemoryLabel(process.rss_kib) + ' / PSS ' + processMemoryLabel(process.pss_kib)}>
            <span class="truncate font-semibold">{process.name}</span>
            <span class="text-right tabular-nums text-kira-muted">{process.pid}</span>
            <span class="text-right tabular-nums">{process.cpu_percent === null ? '—' : process.cpu_percent.toFixed(1) + '%'}</span>
            <span class="text-right tabular-nums">{processMemoryLabel(process.rss_kib)}</span>
            <span class="text-right tabular-nums">{processMemoryLabel(process.pss_kib)}</span>
          </div>
        {:else}
          <p class="py-9 text-center text-xs text-kira-muted">{processes?.error ?
            'PROCESS DATA UNAVAILABLE: ' + processes.error :
            processes && processes.rows.length ? 'NO MATCHING PROCESSES' : 'SAMPLING / PLEASE WAIT'}</p>
        {/each}
        {#if visible.length < matching.length && visible.length < 180}
          <button type="button" onclick={() => { visibleCount = Math.min(180, visibleCount + 36); }}
            class="mt-3 w-full border border-kira-muted/20 px-3 py-2 text-[10px] font-bold tracking-wider
              text-kira-accent hover:bg-kira-bg focus-visible:outline-2 focus-visible:outline-kira-accent">
            SHOW 36 MORE / {matching.length - visible.length} REMAINING
          </button>
        {/if}
      </div>
      <footer data-kira-enter class="shrink-0 space-y-2 pt-3 text-[10px] font-semibold text-kira-muted">
        <p>SHOWING {visible.length} OF {matching.length} MATCHES / {processes?.total ?? 0} SCANNED
          {processes?.truncated ? ' / SCAN LIMIT REACHED' : ''}
          {visible.length === 180 && matching.length > 180 ? ' / REFINE FILTER FOR MORE' : ''}</p>
        <p>CPU % OF ENTIRE HOST / RSS RESIDENT / PSS PROPORTIONAL SHARE</p>
      </footer>
    </section>
  {/if}
</main>
