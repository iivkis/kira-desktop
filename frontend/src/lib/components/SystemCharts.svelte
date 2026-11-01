<script lang="ts">
  import { onMount } from 'svelte';
  // Tree-shaken Apache ECharts. SVG is better suited than a Canvas/WebGL
  // context to the compact, self-contained WebKitGTK overlay.
  import * as echarts from 'echarts/core';
  import { BarChart, LineChart } from 'echarts/charts';
  import { GridComponent } from 'echarts/components';
  import { SVGRenderer } from 'echarts/renderers';
  import type { EChartsType } from 'echarts/core';
  import type { SystemMetrics } from '../sdk/core';
  import {
    appendTelemetry, chartMeters, telemetrySummary,
    type Meter, type TelemetryPoint,
  } from '../system-charts';

  echarts.use([BarChart, LineChart, GridComponent, SVGRenderer]);

  let { metrics, meters }: { metrics: SystemMetrics | null; meters: Meter[] } = $props();

  let barsNode: HTMLDivElement;
  let trendNode: HTMLDivElement;
  let bars: EChartsType | null = null;
  let trend: EChartsType | null = null;
  let history: TelemetryPoint[] = [];
  let reduced = false;

  // Only the five approved brand colors. Both graphs use the same palette.
  const accent = '#fca311';
  const white = '#ffffff';
  const muted = '#e5e5e5';
  const prussian = '#14213d';

  const allMeters = $derived(chartMeters(meters, metrics));
  const chartHeight = $derived(12 + allMeters.length * 28);

  function render(): void {
    const rows = allMeters;
    bars?.setOption({
      animation: !reduced,
      animationDuration: reduced ? 0 : 500,
      animationDurationUpdate: reduced ? 0 : 420,
      animationEasingUpdate: 'cubicOut',
      grid: { left: 144, right: 34, top: 5, bottom: 5 },
      xAxis: {
        type: 'value', min: 0, max: 100,
        axisLabel: { show: false }, axisLine: { show: false },
        splitLine: { show: false }, axisTick: { show: false },
      },
      yAxis: {
        type: 'category', inverse: true,
        data: rows.map(row => row.title),
        axisLabel: {
          color: muted, fontSize: 10, width: 130, overflow: 'truncate',
          formatter: (label: string) => label.length > 22 ? label.slice(0, 20) + '…' : label,
        },
        axisLine: { show: false }, axisTick: { show: false },
      },
      series: [{
        type: 'bar',
        data: rows.map(row => row.percent),
        barWidth: 8,
        showBackground: true,
        backgroundStyle: { color: prussian, borderRadius: 3 },
        itemStyle: { color: accent, borderRadius: 3 },
        label: {
          show: true, position: 'right', color: white, fontSize: 10,
          formatter: (params: { value: unknown }) =>
            typeof params.value === 'number' ? params.value + '%' : '',
        },
        emphasis: { disabled: true },
        silent: true,
      }],
    }, { notMerge: true, lazyUpdate: false });

    const line = (name: string, color: string, field: 'cpu' | 'ram' | 'gpu') => ({
      name, type: 'line' as const, smooth: 0.32, silent: true,
      showSymbol: history.length < 3, symbol: 'circle', symbolSize: 5,
      connectNulls: false,
      data: history.map(point => point[field]),
      lineStyle: { color, width: 2 },
      itemStyle: { color },
      areaStyle: { color, opacity: field === 'cpu' ? 0.13 : 0.045 },
      animationDurationUpdate: reduced ? 0 : 460,
    });
    trend?.setOption({
      animation: !reduced,
      animationDuration: reduced ? 0 : 480,
      animationDurationUpdate: reduced ? 0 : 460,
      animationEasingUpdate: 'cubicOut',
      grid: { left: 25, right: 9, top: 12, bottom: 10 },
      xAxis: {
        type: 'category', boundaryGap: false,
        data: history.map(point => String(point.time)),
        axisLine: { show: false }, axisTick: { show: false },
        axisLabel: { show: false },
        splitLine: { show: false },
      },
      yAxis: {
        type: 'value', min: 0, max: 100, interval: 50,
        axisLine: { show: false }, axisTick: { show: false },
        axisLabel: { color: muted, fontSize: 9, formatter: '{value}' },
        splitLine: { lineStyle: { color: prussian, width: 1 } },
      },
      series: [
        line('CPU', accent, 'cpu'),
        line('RAM', white, 'ram'),
        line('GPU PEAK', muted, 'gpu'),
      ],
    }, { notMerge: true, lazyUpdate: false });
  }

  // The parent receives native samples every two seconds. No polling or
  // fabricated history is introduced by the chart component.
  $effect(() => {
    const current = metrics;
    const currentMeters = meters;
    void currentMeters;
    if (current) history = appendTelemetry(history, current, Date.now());
    render();
  });

  onMount(() => {
    const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
    reduced = motion.matches;
    bars = echarts.init(barsNode, undefined, { renderer: 'svg' });
    trend = echarts.init(trendNode, undefined, { renderer: 'svg' });
    const resize = () => { bars?.resize(); trend?.resize(); };
    const resized = new ResizeObserver(resize);
    resized.observe(barsNode);
    resized.observe(trendNode);
    const motionChanged = () => { reduced = motion.matches; render(); };
    motion.addEventListener('change', motionChanged);
    render();
    return () => {
      resized.disconnect();
      motion.removeEventListener('change', motionChanged);
      bars?.dispose();
      trend?.dispose();
      bars = null;
      trend = null;
      history = [];
    };
  });
</script>

<div class="space-y-3">
  <div class="flex items-center justify-between gap-2">
    <h3 class="text-[10px] font-bold tracking-[0.12em] text-kira-accent">UTILIZATION / LIVE</h3>
    <span class="text-[9px] font-semibold text-kira-muted">0–100%</span>
  </div>
  <div bind:this={barsNode} class="w-full" style:height={chartHeight + 'px'}
    role="img" aria-label={'Live utilization bars: ' +
      allMeters.map(m => m.title + ' ' + m.text).join(', ')}></div>
  <div class="flex items-center justify-between gap-2">
    <h3 class="text-[10px] font-bold tracking-[0.12em] text-kira-accent">ACTIVITY / LAST 60s</h3>
    <span class="text-[9px] text-kira-muted">2s SAMPLES</span>
  </div>
  <div class="flex gap-4 text-[9px] font-bold text-kira-muted" aria-hidden="true">
    <span><span class="text-kira-accent">●</span> CPU</span>
    <span><span class="text-kira-text">●</span> RAM</span>
    <span><span class="text-kira-muted">●</span> GPU PEAK</span>
  </div>
  <div bind:this={trendNode} class="h-[130px] w-full"
    role="img" aria-label={'Up to thirty actual samples of CPU, RAM and peak GPU utilization. ' +
      telemetrySummary(metrics)}></div>
</div>
