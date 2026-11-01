<script lang="ts">
  import { onMount } from 'svelte';
  import * as echarts from 'echarts/core';
  import { GaugeChart, LineChart } from 'echarts/charts';
  import { GridComponent } from 'echarts/components';
  import { SVGRenderer } from 'echarts/renderers';
  import type { EChartsType } from 'echarts/core';
  import type { SystemMetrics } from '../sdk/core';
  import { percentUsed } from '../system-model';
  import {
    activityAxis, appendTelemetry, gpuPeak, telemetrySummary,
    type TelemetryPoint,
  } from '../system-charts';

  echarts.use([GaugeChart, LineChart, GridComponent, SVGRenderer]);

  let { metrics }: { metrics: SystemMetrics | null } = $props();

  let gaugeNode: HTMLDivElement;
  let trendNode: HTMLDivElement;
  let gauges: EChartsType | null = null;
  let trend: EChartsType | null = null;
  let reduced = false;
  let history: TelemetryPoint[] = [];
  let lastSample: SystemMetrics | null = null;

  const accent = '#fca311';
  const white = '#ffffff';
  const muted = '#e5e5e5';
  const prussian = '#14213d';

  const ram = $derived(metrics
    ? percentUsed(metrics.memory_total_kib, metrics.memory_available_kib) : null);
  const gpuAvailable = $derived(metrics !== null && gpuPeak(metrics) !== null);
  const trendHeight = $derived(gpuAvailable ? 296 : 202);
  const dialLabel = (value: number | null) => value === null ? '—' : value + '%';

  function draw(): void {
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
      animationDuration: reduced ? 0 : 700,
      animationDurationUpdate: reduced ? 0 : 530,
      animationEasingUpdate: 'cubicOut',
      series: [
        dial('CPU', metrics?.cpu_percent ?? null, '25%', accent),
        dial('RAM', ram, '75%', white),
      ],
    }, { notMerge: true });

    const panels: { field: 'cpu' | 'ram' | 'gpu'; name: string; color: string }[] = [
      { field: 'cpu', name: 'CPU ACTIVITY', color: accent },
      { field: 'ram', name: 'MEMORY ACTIVITY', color: white },
    ];
    if (gpuAvailable) panels.push({ field: 'gpu', name: 'GPU ACTIVITY', color: muted });

    trend?.setOption({
      animation: !reduced,
      animationDuration: reduced ? 0 : 460,
      animationDurationUpdate: reduced ? 0 : 420,
      animationEasingUpdate: 'cubicOut',
      tooltip: { show: false },
      grid: panels.map((_, i) => ({
        left: 34, right: 8, top: i * 96 + 28, height: 56,
      })),
      xAxis: panels.map((_, i) => ({
        type: 'category' as const, gridIndex: i, boundaryGap: false,
        data: history.map(point => String(point.time)),
        axisLine: { show: false }, axisTick: { show: false },
        axisLabel: { show: false }, splitLine: { show: false },
      })),
      yAxis: panels.map((panel, i) => {
        const axis = activityAxis(history, panel.field);
        return {
          type: 'value' as const, gridIndex: i,
          min: axis.min, max: axis.max, splitNumber: 2,
          axisLine: { show: false }, axisTick: { show: false },
          axisLabel: {
            color: muted, fontSize: 9, margin: 7,
            formatter: (v: number) => v === axis.min || v === axis.max ? v + '%' : '',
          },
          splitLine: { lineStyle: { color: prussian, width: 1 } },
        };
      }),
      series: panels.map((panel, i) => ({
        name: panel.name, type: 'line' as const,
        xAxisIndex: i, yAxisIndex: i,
        smooth: 0.27, silent: true,
        showSymbol: history.length < 3, symbolSize: 5,
        connectNulls: false,
        data: history.map(point => point[panel.field]),
        lineStyle: { color: panel.color, width: 2.5 },
        itemStyle: { color: panel.color },
        areaStyle: { color: panel.color, opacity: panel.field === 'cpu' ? 0.18 : 0.09 },
      })),
    }, { notMerge: true });
  }

  // One history point per *native* sample, not per chart resize or DOM update.
  $effect(() => {
    const sample = metrics;
    if (sample && sample !== lastSample) {
      history = appendTelemetry(history, sample, Date.now());
      lastSample = sample;
    }
    draw();
  });

  onMount(() => {
    const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
    reduced = motion.matches;
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
      observer.disconnect();
      motion.removeEventListener('change', onMotionChange);
      gauges?.dispose();
      trend?.dispose();
      gauges = null;
      trend = null;
      history = [];
      lastSample = null;
    };
  });
</script>

<div class="space-y-4">
  <section aria-label="CPU and memory utilization">
    <div class="flex items-center justify-between">
      <h2 class="text-[10px] font-bold tracking-[0.15em] text-kira-accent">SYSTEM / PULSE</h2>
      <span class="text-[9px] font-semibold tracking-widest text-kira-muted">LIVE · 2S</span>
    </div>
    <div class="relative mt-1 h-[164px]">
      <div bind:this={gaugeNode} class="absolute inset-0" aria-hidden="true"></div>
      <div class="pointer-events-none absolute inset-0 grid grid-cols-2">
        <div class="flex flex-col items-center justify-center">
          <div class="text-[37px] font-extrabold leading-none tracking-[-0.07em] tabular-nums text-kira-accent"
            aria-label={'CPU utilization ' + (metrics?.cpu_percent === null || !metrics ? 'unavailable' : metrics.cpu_percent + ' percent')}>
            {dialLabel(metrics?.cpu_percent ?? null)}
          </div>
          <div class="mt-2 text-[10px] font-bold tracking-[0.14em] text-kira-muted">PROCESSOR</div>
        </div>
        <div class="flex flex-col items-center justify-center">
          <div class="text-[37px] font-extrabold leading-none tracking-[-0.07em] tabular-nums"
            aria-label={'Memory utilization ' + (ram === null ? 'unavailable' : ram + ' percent')}>
            {dialLabel(ram)}
          </div>
          <div class="mt-2 text-[10px] font-bold tracking-[0.14em] text-kira-muted">MEMORY</div>
        </div>
      </div>
    </div>
  </section>
  <section aria-label="Live resource activity">
    <div class="flex items-center justify-between">
      <h2 class="text-[10px] font-bold tracking-[0.15em] text-kira-accent">ACTIVITY / LIVE</h2>
      <span class="text-[9px] font-semibold text-kira-muted">LAST 60S · ADAPTIVE SCALE</span>
    </div>
    <div class="relative mt-3">
      <div bind:this={trendNode} style:height={trendHeight + 'px'} class="w-full"
        role="img" aria-label={'Measured history with individually labelled adaptive percentage scales. ' +
          telemetrySummary(metrics)}></div>
      <div class="pointer-events-none absolute inset-0" aria-hidden="true">
        <div class="absolute left-9 top-0 text-[10px] font-bold tracking-wide text-kira-accent">CPU</div>
        <div class="absolute left-9 top-[96px] text-[10px] font-bold tracking-wide text-kira-text">RAM</div>
        {#if gpuAvailable}
          <div class="absolute left-9 top-[192px] text-[10px] font-bold tracking-wide text-kira-muted">GPU</div>
        {/if}
      </div>
    </div>
  </section>
</div>
