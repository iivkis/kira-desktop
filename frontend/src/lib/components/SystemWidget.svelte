<script lang="ts">
  import { onMount } from 'svelte';
  import {
    onSystemOverlayData, onSystemProcesses,
    type SystemOverlayData, type SystemProcesses,
  } from '../sdk/core';
  import {
    bytesLabel, defaultSort, fractionLabel, nextSort, percentUsed,
    sortedProcesses, systemHeadline, uptimeLabel, type SortField,
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
  const meters = $derived([
    { title: 'CPU / ALL CORES', percent: metrics?.cpu_percent ?? null,
      text: metrics?.cpu_percent === null || !metrics ? 'MEASURING' : metrics.cpu_percent + '%' },
    { title: 'MEMORY / RAM',
      percent: percentUsed(metrics?.memory_total_kib ?? null, metrics?.memory_available_kib ?? null),
      text: fractionLabel(metrics?.memory_total_kib ?? null, metrics?.memory_available_kib ?? null) },
    { title: 'SWAP',
      percent: percentUsed(metrics?.swap_total_kib ?? null, metrics?.swap_free_kib ?? null),
      text: metrics?.swap_total_kib === 0 ? 'NOT CONFIGURED'
        : fractionLabel(metrics?.swap_total_kib ?? null, metrics?.swap_free_kib ?? null) },
    { title: 'STORAGE / ROOT',
      percent: percentUsed(metrics?.root_total_kib ?? null, metrics?.root_available_kib ?? null),
      text: fractionLabel(metrics?.root_total_kib ?? null, metrics?.root_available_kib ?? null) },
  ]);

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

<main data-kira-ui-root aria-label="Kira System Monitor"
  class="flex h-full min-h-0 w-full flex-col overflow-y-auto bg-kira-bg text-kira-text">
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
    <div class="flex min-h-0 flex-1 flex-col">
      <section class="space-y-2 bg-kira-bg px-5 py-4" aria-label="Connections">
        <h2 class="text-[10px] font-bold tracking-[0.12em] text-kira-accent">CONNECTIONS / REAL-TIME</h2>
        <div class="flex justify-between gap-3 text-xs"><span class="text-kira-muted">KIRA CORE</span>
          <span class="font-bold">{status?.core ? 'CONNECTED' : 'OFFLINE'}</span></div>
        <div class="flex justify-between gap-3 text-xs"><span class="text-kira-muted">NIRI IPC</span>
          <span class="font-bold">{status?.niri ? 'CONNECTED' : 'DISCONNECTED'}</span></div>
      </section>

      <section class="space-y-4 bg-kira-surface px-5 py-4" aria-label="Resource usage">
        <h2 class="text-[10px] font-bold tracking-[0.12em] text-kira-accent">RESOURCES / LIVE TELEMETRY</h2>
        {#each meters as meter (meter.title)}
          <div class="space-y-2">
            <div class="flex items-center justify-between gap-2 text-[11px]">
              <span class="font-semibold text-kira-muted">{meter.title}</span>
              <span class="text-right font-bold tabular-nums">{meter.text}</span>
            </div>
            {#if meter.percent !== null}
              <div class="h-1.5 w-full overflow-hidden rounded-sm bg-kira-bg" role="progressbar"
                aria-label={meter.title} aria-valuenow={meter.percent} aria-valuemin="0" aria-valuemax="100">
                <div class="h-full bg-kira-accent" style:width={meter.percent + '%'}></div>
              </div>
            {/if}
          </div>
        {/each}
      </section>

      <section class="space-y-3 bg-kira-bg px-5 py-4" aria-label="Operating system details">
        <h2 class="text-[10px] font-bold tracking-[0.12em] text-kira-accent">OPERATING SYSTEM / CONTEXT</h2>
        <div class="flex justify-between gap-3 text-[11px]"><span class="text-kira-muted">UPTIME</span>
          <span class="font-bold tabular-nums">{uptimeLabel(metrics?.uptime_seconds ?? null)}</span></div>
        <div class="flex justify-between gap-3 text-[11px]"><span class="text-kira-muted">LOAD / 1 5 15</span>
          <span class="font-bold tabular-nums">{metrics?.load?.map(n => n.toFixed(2)).join(' / ') ?? 'UNAVAILABLE'}</span></div>
        <div class="flex justify-between gap-3 text-[11px]"><span class="text-kira-muted">POWER</span>
          <span class="text-right font-bold">{metrics?.battery_percent == null ? 'NOT DETECTED' :
            metrics.battery_percent + '%' + (metrics.battery_charging === true ? ' / CHARGING' :
              metrics.battery_charging === false ? ' / DISCHARGING' : '')}</span></div>
        <div class="flex items-center justify-between gap-2 border-t border-kira-muted/20 pt-3">
          <span class="text-[10px] font-semibold text-kira-muted">LIVE / {refreshed || 'WAITING'}</span>
          <button type="button" onclick={onCopy}
            class="px-1 py-2 text-[11px] font-bold text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent">COPY REPORT</button>
        </div>
      </section>
    </div>
  {:else}
    <section class="flex min-h-0 flex-1 flex-col bg-kira-surface px-4 py-4" aria-label="Processes">
      <h2 class="mb-3 text-[10px] font-bold tracking-[0.12em] text-kira-accent">PROCESSES / HOST</h2>
      <input type="search" aria-label="Filter by process name or PID" placeholder="Filter by process name or PID"
        bind:value={query} class="mb-3 min-h-9 w-full rounded bg-kira-bg px-3 py-2 text-xs text-kira-text outline-none placeholder:text-kira-muted focus-visible:outline-2 focus-visible:outline-kira-accent"/>
      <div class="grid grid-cols-[minmax(0,1fr)_55px_65px_76px] gap-1 border-b border-kira-muted/20 pb-2">
        {#each [{field:'name',label:'PROCESS'}, {field:'pid',label:'PID'}, {field:'cpu',label:'CPU'}, {field:'memory',label:'RAM'}] as column}
          <button type="button" onclick={() => changeSort(column.field as SortField)}
            class={'truncate text-right text-[10px] font-bold tracking-wide hover:text-kira-accent focus-visible:outline-2 focus-visible:outline-kira-accent ' +
              (column.field === 'name' ? 'text-left ' : '') +
              (sort.field === column.field ? 'text-kira-accent' : 'text-kira-muted')}
            aria-label={'Sort by ' + column.label}>
            {column.label}{sort.field === column.field ? (sort.descending ? ' ↓' : ' ↑') : ''}
          </button>
        {/each}
      </div>
      <div class="min-h-0 flex-1 overflow-y-auto" aria-live="polite">
        {#each visible as process (process.pid)}
          <div class="grid min-h-9 grid-cols-[minmax(0,1fr)_55px_65px_76px] items-center gap-1 border-b border-kira-muted/10 text-[11px]"
            title={process.name + ' / PID ' + process.pid + ' / STATE ' + process.state}>
            <span class="truncate font-semibold">{process.name}</span>
            <span class="text-right tabular-nums text-kira-muted">{process.pid}</span>
            <span class="text-right tabular-nums">{process.cpu_percent === null ? '—' : process.cpu_percent.toFixed(1) + '%'}</span>
            <span class="text-right tabular-nums">{process.rss_kib === null ? '—' : bytesLabel(process.rss_kib)}</span>
          </div>
        {:else}
          <p class="py-9 text-center text-xs text-kira-muted">{processes?.error ?
            'PROCESS DATA UNAVAILABLE: ' + processes.error :
            processes && processes.rows.length ? 'NO MATCHING PROCESSES' : 'SAMPLING / PLEASE WAIT'}</p>
        {/each}
      </div>
      <footer class="shrink-0 space-y-2 pt-3 text-[10px] font-semibold text-kira-muted">
        <p>SHOWING {visible.length} OF {matching.length} MATCHES / {processes?.total ?? 0} SCANNED
          {processes?.truncated ? ' / LIMIT REACHED' : ''}</p>
        <p>READ ONLY / CPU % OF ENTIRE HOST / RSS MEMORY</p>
      </footer>
    </section>
  {/if}
</main>
