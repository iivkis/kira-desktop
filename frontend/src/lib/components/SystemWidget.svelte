<script lang="ts">
  import { onMount } from 'svelte';
  import SystemCharts from './SystemCharts.svelte';
  import { pulseReading, revealStagger } from '../motion';
  import { tenetHoverLens } from '../tenet-hover-lens';
  import {
    onSystemOverlayData, onSystemProcesses,
    type SystemOverlayData, type SystemProcesses,
  } from '../sdk/core';
  import {
    defaultSort, fractionLabel, nextSort, processMemoryLabel,
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
  const metrics = $derived(status?.metrics ?? null);
  const health = $derived(systemHeadline(metrics, status?.core ?? false, status?.niri ?? false));
  const matching = $derived(sortedProcesses(processes?.rows ?? [], query, sort));
  const visible = $derived(matching.slice(0, 36));


  onMount(() => {
    const stopMetrics = onSystemOverlayData(data => {
      status = data;
      refreshed = new Date().toLocaleTimeString();
    });
    const stopProcesses = onSystemProcesses(data => { processes = data; });
    return () => {
      stopMetrics();
      stopProcesses();
      onTab('overview');
    };
  });

  function switchTab(tab: 'overview' | 'processes') {
    if (active === tab) return;
    active = tab;
    if (tab === 'processes') processes = null;
    onTab(tab);
  }

  function changeSort(field: SortField) {
    sort = nextSort(sort, field);
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={keydown} />

<main data-kira-ui-root aria-label="Kira System Monitor" use:tenetHoverLens
  class="kira-monitor-scroll flex h-auto min-h-0 w-full flex-col overflow-y-auto bg-kira-bg text-kira-text">
  <header class="shrink-0 bg-kira-bg px-5 pb-4 pt-5">
    <div class="flex items-start justify-between gap-3">
      <div>
        <div class="text-[10px] font-bold tracking-[0.12em] text-kira-accent">KIRA / SYSTEM MONITOR</div>
        <h1 class="mt-2 text-2xl font-extrabold">System overview</h1>
        <p class="mt-1 text-[11px] font-semibold tracking-wide"
          class:text-kira-accent={health === 'ALL SYSTEMS NOMINAL'}
          class:text-kira-muted={health !== 'ALL SYSTEMS NOMINAL'} aria-live="polite">{health}</p>
      </div>
      <button type="button" onclick={onClose} aria-label="Close system monitor" title="Close (Esc)"
        class="rounded px-2 py-1 text-xl text-kira-muted hover:text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent">×</button>
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
      <section data-kira-enter class="bg-kira-surface px-5 pb-4 pt-3" aria-label="Live system telemetry">
        <div class="mb-4 flex items-center gap-4 border-b border-kira-muted/15 pb-3">
          <div class="flex items-center gap-1.5">
            <span class="h-1.5 w-1.5 shrink-0 rounded-full"
              class:bg-kira-accent={status?.core}
              class:bg-kira-muted={!status?.core}></span>
            <span class="text-[9px] font-bold tracking-wider">CORE</span>
            <span class="text-[9px] text-kira-muted">{status?.core ? 'ONLINE' : 'OFFLINE'}</span>
          </div>
          <div class="flex items-center gap-1.5">
            <span class="h-1.5 w-1.5 shrink-0 rounded-full"
              class:bg-kira-accent={status?.niri}
              class:bg-kira-muted={!status?.niri}></span>
            <span class="text-[9px] font-bold tracking-wider">NIRI</span>
            <span class="text-[9px] text-kira-muted">{status?.niri ? 'ONLINE' : 'OFFLINE'}</span>
          </div>
        </div>
        <SystemCharts {metrics} />
      </section>

      <section data-kira-enter class="grid grid-cols-2 gap-px bg-kira-bg"
        aria-label="Secondary resource metrics">
        <div class="min-w-0 bg-kira-surface-soft px-5 py-3">
          <div class="text-[9px] font-bold tracking-[0.12em] text-kira-muted">SWAP / USED</div>
          <div class="mt-1 text-[23px] font-extrabold leading-tight tracking-tight tabular-nums">
            {usageLabel(metrics?.swap_total_kib ?? null, metrics?.swap_free_kib ?? null)}
          </div>
          <div class="mt-1 truncate text-[10px] text-kira-muted"
            title={fractionLabel(metrics?.swap_total_kib ?? null, metrics?.swap_free_kib ?? null)}>
            {fractionLabel(metrics?.swap_total_kib ?? null, metrics?.swap_free_kib ?? null)}
          </div>
        </div>
        <div class="min-w-0 bg-kira-surface-soft px-5 py-3">
          <div class="text-[9px] font-bold tracking-[0.12em] text-kira-muted">STORAGE / ROOT</div>
          <div class="mt-1 text-[23px] font-extrabold leading-tight tracking-tight tabular-nums">
            {usageLabel(metrics?.root_total_kib ?? null, metrics?.root_available_kib ?? null)}
          </div>
          <div class="mt-1 truncate text-[10px] text-kira-muted"
            title={fractionLabel(metrics?.root_total_kib ?? null, metrics?.root_available_kib ?? null)}>
            {fractionLabel(metrics?.root_total_kib ?? null, metrics?.root_available_kib ?? null)}
          </div>
        </div>
      </section>

      <section data-kira-enter class="space-y-2 bg-kira-bg px-5 py-3"
        aria-label="Hardware temperatures and graphics">
        <div class="flex items-center justify-between gap-3">
          <h2 class="text-[10px] font-bold tracking-[0.13em] text-kira-accent">HARDWARE / THERMALS</h2>
          <div class="flex items-baseline gap-2">
            <span class="text-[9px] font-semibold tracking-wider text-kira-muted">CPU</span>
            <span class="text-[14px] font-extrabold tabular-nums"
              use:pulseReading={temperatureLabel(metrics?.cpu_temperature_c ?? null)}>
              {temperatureLabel(metrics?.cpu_temperature_c ?? null)}
            </span>
          </div>
        </div>
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
      </section>

      <section data-kira-enter class="bg-kira-surface-soft px-5 py-3" aria-label="System details">
        <div class="grid grid-cols-2 gap-x-5 gap-y-3">
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
      </section>
      <footer data-kira-enter class="flex items-center justify-between gap-3 bg-kira-bg px-5 py-2.5">
        <span class="text-[9px] font-semibold tracking-wider text-kira-muted">
          LIVE / {refreshed || 'WAITING FOR DATA'}
        </span>
        <button type="button" onclick={onCopy}
          class="px-1 py-2 text-[10px] font-bold tracking-wide text-kira-accent
            focus-visible:outline-2 focus-visible:outline-kira-accent">COPY REPORT</button>
      </footer>
    </div>
  {:else}
    <section class="flex min-h-0 flex-1 flex-col bg-kira-surface px-4 py-4"
      aria-label="Processes" use:revealStagger>
      <h2 data-kira-enter class="mb-3 text-[10px] font-bold tracking-[0.12em] text-kira-accent">PROCESSES / HOST</h2>
      <input data-kira-enter type="search" aria-label="Filter by process name or PID" placeholder="Filter by process name or PID"
        bind:value={query} class="mb-3 min-h-9 w-full rounded bg-kira-bg px-3 py-2 text-xs text-kira-text outline-none placeholder:text-kira-muted focus-visible:outline-2 focus-visible:outline-kira-accent"/>
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
      <div data-kira-enter class="kira-monitor-scroll min-h-0 flex-1 overflow-y-auto" aria-live="polite">
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
      </div>
      <footer data-kira-enter class="shrink-0 space-y-2 pt-3 text-[10px] font-semibold text-kira-muted">
        <p>SHOWING {visible.length} OF {matching.length} MATCHES / {processes?.total ?? 0} SCANNED
          {processes?.truncated ? ' / LIMIT REACHED' : ''}</p>
        <p>READ ONLY / CPU % OF ENTIRE HOST / RSS RESIDENT / PSS PROPORTIONAL SHARE</p>
      </footer>
    </section>
  {/if}
</main>
