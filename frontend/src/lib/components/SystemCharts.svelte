<script lang="ts">
  import { onMount } from 'svelte';
  import * as echarts from 'echarts/core';
  import { GaugeChart, LineChart } from 'echarts/charts';
  import { GridComponent } from 'echarts/components';
  import { SVGRenderer } from 'echarts/renderers';
  import type { EChartsType } from 'echarts/core';
  import { onSystemPulse, type SystemMetrics, type SystemPulse } from '../sdk/core';
  import { surfaceColors } from '../surface-colors';
  import { isSampleFresh, liveUtilization } from '../system-model';
  import {
    activityAxis, activitySeries, activityTimeDomain, alignedActivity, appendPulse, gpuPeak, recentPulseHistory,
    type TelemetryPoint,
  } from '../system-charts';

  echarts.use([GaugeChart, LineChart, GridComponent, SVGRenderer]);

  let { metrics, now }: { metrics: SystemMetrics | null; now: number } = $props();

  let gaugeNode: HTMLDivElement;
  let trendNode: HTMLDivElement;
  let gauges: EChartsType | null = null;
  let trend: EChartsType | null = null;
  let reduced = false;
  let palette = surfaceColors();
  let history: TelemetryPoint[] = [];
  let pulse = $state<SystemPulse | null>(null);
  let lastPulseAt = $state<number | null>(null);
  let lastPulse: SystemPulse | null = null;
  let lastMetrics: SystemMetrics | null = null;
  let pendingGpu: number | null | undefined;

  // The parent clock tick invalidates this derived value every two seconds,
  // while a new pulse invalidates it immediately. Use a monotonic timestamp
  // for both sides so changing the wall clock cannot keep dead data "fresh".
  const pulseFresh = $derived.by(() => {
    if (!Number.isFinite(now)) return false;
    return isSampleFresh(lastPulseAt, performance.now(), 2_500);
  });
  const usage = $derived(liveUtilization(pulse, pulseFresh, metrics));
  const cpu = $derived(usage.cpu);
  const ram = $derived(usage.ram);
  const gpuAvailable = $derived(metrics !== null && gpuPeak(metrics) !== null);
  const trendHeight = $derived(gpuAvailable ? 296 : 202);
  const dialLabel = (value: number | null, precise = false) =>
    value === null ? '—' : (precise ? value.toFixed(1) : String(value)) + '%';

  function draw(): void {
    const { accent, muted } = palette;
    const white = palette.text;
    const prussian = palette.surface;
    const dial = (name: string, value: number | null, center: string, color: string) => ({
      name, type: 'gauge' as const, center: [center, '50%'],
      radius: '73%', min: 0, max: 100, startAngle: 90, endAngle: -270,
      clockwise: true, silent: true, animation: !reduced,
      progress: { show: value !== null, width: 8, roundCap: true, itemStyle: { color } },
      axisLine: { roundCap: true, lineStyle: { width: 8, color: [[1, prussian]] } },
      pointer: { show: false }, anchor: { show: false },
      axisTick: { show: false }, splitLine: { show: false },
      axisLabel: { show: false }, title: { show: false }, detail: { show: false },
      data: value === null ? [] : [{ value }],
    });
    gauges?.setOption({
      animation: !reduced,
      animationDuration: reduced ? 0 : 350,
      animationDurationUpdate: reduced ? 0 : 180,
      animationEasingUpdate: 'cubicOut',
      series: [
        dial('CPU', cpu, '25%', accent),
        dial('RAM', ram, '75%', white),
      ],
    }, { lazyUpdate: true });

    // Old readings leave the chart even if the native stream has stopped.
    const syncedHistory = alignedActivity(recentPulseHistory(history, performance.now()));
    const timeDomain = activityTimeDomain(syncedHistory);
    const panels: { field: 'cpu' | 'ram' | 'gpu'; name: string; color: string }[] = [
      { field: 'cpu', name: 'АКТИВНОСТЬ CPU', color: accent },
      { field: 'ram', name: 'АКТИВНОСТЬ RAM', color: white },
    ];
    if (gpuAvailable) panels.push({ field: 'gpu', name: 'АКТИВНОСТЬ GPU', color: muted });

    trend?.setOption({
      animation: !reduced,
      animationDuration: reduced ? 0 : 350,
      animationDurationUpdate: reduced ? 0 : 180,
      animationEasingUpdate: 'cubicOut',
      tooltip: { show: false },
      grid: panels.map((panel, i) => ({
        id: 'grid-' + panel.field,
        left: 41, right: 8, top: i * 96 + 28, height: 56,
      })),
      xAxis: panels.map((panel, i) => ({
        id: 'x-' + panel.field,
        type: 'time' as const, gridIndex: i, boundaryGap: false,
        min: timeDomain.min, max: timeDomain.max,
        axisLine: { show: false }, axisTick: { show: false },
        axisLabel: { show: false }, splitLine: { show: false },
      })),
      yAxis: panels.map((panel, i) => {
        const axis = activityAxis(syncedHistory, panel.field);
        return {
          id: 'y-' + panel.field,
          type: 'value' as const, gridIndex: i,
          min: axis.min, max: axis.max, interval: (axis.max - axis.min) / 2,
          axisLine: { show: false }, axisTick: { show: false },
          axisLabel: {
            color: muted, fontSize: 11, margin: 7,
            formatter: (v: number) => Number(v.toFixed(1)) + '%',
          },
          splitLine: { lineStyle: { color: prussian, width: 1 } },
        };
      }),
      series: panels.map((panel, i) => ({
        id: 'series-' + panel.field,
        name: panel.name, type: 'line' as const,
        xAxisIndex: i, yAxisIndex: i,
        smooth: 0.18, silent: true,
        showSymbol: syncedHistory.length < 3, symbolSize: 5,
        connectNulls: panel.field === 'gpu',
        data: activitySeries(syncedHistory, panel.field),
        lineStyle: { color: panel.color, width: 2.5 },
        itemStyle: { color: panel.color },
        areaStyle: { color: panel.color, opacity: panel.field === 'cpu' ? 0.18 : 0.09 },
      })),
    }, { replaceMerge: ['grid', 'xAxis', 'yAxis', 'series'], lazyUpdate: true });
  }

  // One history point per actual native pulse. No 500ms JS timer and no
  // GPU replication between slow readings. The bounded list is one minute.
  $effect(() => {
    // The parent refresh tick forces expiration redraws when no new native
    // pulse arrives. A boolean freshness derived value alone may stay false
    // and would not invalidate an old ECharts history view.
    const refreshTick = now;
    if (!Number.isFinite(refreshTick)) return;
    const current = metrics;
    const sample = pulse;
    if (current === null) {
      // Drop any slow GPU reading that was awaiting a fast pulse before the
      // overlay metrics went stale. Never attach it to a later new pulse.
      pendingGpu = undefined;
      lastMetrics = null;
    } else if (current !== lastMetrics) {
      pendingGpu = gpuPeak(current);
      lastMetrics = current;
    }
    if (sample && sample !== lastPulse) {
      // The chart axis is elapsed-time only; monotonic time keeps the
      // recorded trajectory stable when the wall clock is adjusted.
      history = appendPulse(history, sample, performance.now(), pendingGpu ?? null);
      pendingGpu = undefined;
      lastPulse = sample;
    }
    draw();
  });

  onMount(() => {
    // CSS role lookup is only necessary when the view or theme changes;
    // avoid recalculating computed styles for every 500ms native pulse.
    palette = surfaceColors(gaugeNode);
    const theme = new MutationObserver(() => {
      palette = surfaceColors(gaugeNode);
      draw();
    });
    theme.observe(document.documentElement, {
      attributes: true, attributeFilter: ['class', 'style'],
    });
    const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
    reduced = motion.matches;
    const stopPulse = onSystemPulse(sample => {
      lastPulseAt = performance.now();
      pulse = sample;
    });
    gauges = echarts.init(gaugeNode, undefined, { renderer: 'svg' });
    trend = echarts.init(trendNode, undefined, { renderer: 'svg' });
    const resize = () => { gauges?.resize(); trend?.resize(); };
    const observer = new ResizeObserver(resize);
    observer.observe(gaugeNode);
    observer.observe(trendNode);
    const onMotionChange = () => { reduced = motion.matches; draw(); };
    motion.addEventListener('change', onMotionChange);
    draw();
    return () => {
      stopPulse();
      observer.disconnect();
      theme.disconnect();
      motion.removeEventListener('change', onMotionChange);
      gauges?.dispose();
      trend?.dispose();
      gauges = null;
      trend = null;
      history = [];
      lastPulse = null;
      lastPulseAt = null;
      lastMetrics = null;
      pendingGpu = undefined;
    };
  });
</script>

<div class="space-y-5">
  <section aria-label="Загрузка процессора и использование памяти">
    {#if pulse !== null && !pulseFresh}
      <p class="mb-2 text-[10px] font-semibold text-kira-muted" role="status">ИМПУЛЬС УСТАРЕЛ</p>
    {/if}
    <div class="relative h-[182px] bg-kira-bg">
      <div bind:this={gaugeNode} class="absolute inset-0" aria-hidden="true"></div>
      <div class="pointer-events-none absolute inset-0 grid grid-cols-2 divide-x divide-kira-muted/15">
        <div class="relative flex min-w-0 flex-col items-center justify-center">
          <div class="text-[36px] font-extrabold leading-none tracking-[-0.06em] tabular-nums text-kira-accent"
            aria-label={'Загрузка CPU: ' + (cpu === null ? 'нет данных' : cpu + '%')}>
            {dialLabel(cpu)}
          </div>
          <div class="mt-2 text-[10px] font-bold tracking-[0.12em] text-kira-muted">CPU</div>
        </div>
        <div class="relative flex min-w-0 flex-col items-center justify-center">
          <div class="text-[36px] font-extrabold leading-none tracking-[-0.06em] tabular-nums text-kira-text"
            aria-label={'Использование RAM: ' + (ram === null ? 'нет данных' : ram.toFixed(1) + '%')}>
            {dialLabel(ram, true)}
          </div>
          <div class="mt-2 text-[10px] font-bold tracking-[0.12em] text-kira-muted">RAM</div>
        </div>
      </div>
    </div>
  </section>

  <section aria-label="История нагрузки ресурсов">
    <h2 class="text-[12px] font-bold tracking-wide text-kira-text">ИСТОРИЯ НАГРУЗКИ</h2>
    <div class="relative mt-3">
      <div bind:this={trendNode} style:height={trendHeight + 'px'} class="w-full"
        role="img" aria-label={'Измеренная история нагрузки с адаптивными шкалами. ' +
          'CPU ' + (cpu === null ? 'нет данных' : cpu + '%') +
          ', RAM ' + (ram === null ? 'нет данных' : ram + '%') +
          (pulse !== null && !pulseFresh ? '. Последний импульс устарел.' : '')}></div>
      <div class="pointer-events-none absolute inset-0" aria-hidden="true">
        <div class="absolute left-9 top-0 text-[10px] font-bold tracking-wide text-kira-accent">CPU</div>
        <div class="absolute left-9 top-[96px] text-[10px] font-bold tracking-wide text-kira-text">RAM</div>
        {#if gpuAvailable}
          <div class="absolute left-9 top-[192px] text-[10px] font-bold tracking-wide text-kira-muted">GPU</div>
        {/if}
      </div>
    </div>
    <!-- The time domain spans real samples only, never synthetic 60-second data. -->
    <div class="mt-1 flex justify-between text-[11px] font-medium text-kira-muted" aria-hidden="true">
      <span>До 60 с назад</span>
      <span>Последний замер</span>
    </div>
  </section>
</div>
