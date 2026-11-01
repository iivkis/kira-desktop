import { describe, expect, it } from 'vitest';
import type { SystemMetrics } from './sdk/core';
import {
  appendTelemetry, chartMeters, gpuPeak, MAX_TELEMETRY_POINTS, telemetrySummary,
} from './system-charts';

const metrics: SystemMetrics = {
  cpu_percent: 31,
  cpu_temperature_c: 58,
  gpus: [
    { label: 'iGPU / Intel (card0)', utilization_percent: null, temperature_c: 48 },
    { label: 'GPU / AMD (card1)', utilization_percent: 72, temperature_c: 64 },
  ],
  memory_total_kib: 1000, memory_available_kib: 200,
  swap_total_kib: 0, swap_free_kib: 0,
  root_total_kib: 1000, root_available_kib: 500,
  uptime_seconds: 300, load: [0.1, 0.2, 0.3],
  battery_percent: null, battery_charging: null,
};

describe('System Monitor ECharts telemetry model', () => {
  it('charts only real samples and caps history at thirty points', () => {
    let samples = [] as ReturnType<typeof appendTelemetry>;
    for (let i = 0; i < 50; i++) {
      samples = appendTelemetry(samples, metrics, 1000 + i * 2000);
    }
    expect(samples).toHaveLength(MAX_TELEMETRY_POINTS);
    expect(samples[0].time).toBe(41_000);
    expect(samples.at(-1)).toEqual({ time: 99_000, cpu: 31, ram: 80, gpu: 72 });
  });

  it('never synthesizes zero when GPU, CPU or memory measurements are missing', () => {
    const absent = {
      ...metrics,
      cpu_percent: null,
      memory_available_kib: null,
      gpus: metrics.gpus.map(gpu => ({ ...gpu, utilization_percent: null })),
    };
    expect(gpuPeak(absent)).toBeNull();
    expect(appendTelemetry([], absent, 1)).toEqual([
      { time: 1, cpu: null, ram: null, gpu: null },
    ]);
    expect(telemetrySummary(absent)).toContain('GPU peak unavailable');
  });

  it('adds one bar per detected GPU, retaining unknown readings', () => {
    const rows = chartMeters([{ title: 'CPU', percent: 31, text: '31%' }], metrics);
    expect(rows).toHaveLength(3);
    expect(rows.map(row => row.percent)).toEqual([31, null, 72]);
    expect(rows[1].text).toBe('UNAVAILABLE');
    expect(chartMeters([], null)).toEqual([]);
    expect(telemetrySummary(null)).toBe('Waiting for system telemetry');
  });
});
