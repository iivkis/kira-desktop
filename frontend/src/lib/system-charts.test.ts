import { describe, expect, it } from 'vitest';
import type { SystemMetrics } from './sdk/core';
import {
  activityAxis, appendPulse, appendTelemetry, chartMeters, gpuPeak,
  MAX_PULSE_POINTS, MAX_TELEMETRY_POINTS, telemetrySummary,
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
  it('keeps 120 genuine 500ms samples for one minute of fast pulse', () => {
    let history = [] as ReturnType<typeof appendPulse>;
    for (let i = 0; i < 145; i++) {
      history = appendPulse(history, {
        cpu_percent: i % 101, memory_total_kib: 1000, memory_available_kib: 600,
      }, i * 500);
    }
    expect(MAX_PULSE_POINTS).toBe(120);
    expect(history).toHaveLength(120);
    expect(history[0].time).toBe(12500);
    expect(history.at(-1)?.time).toBe(72000);
    expect(history.at(-1)?.ram).toBe(40);
  });

  it('never repeats 2s GPU samples across 500ms CPU/RAM pulses', () => {
    const sample = { cpu_percent: 5, memory_total_kib: 1000, memory_available_kib: 610 };
    let history = appendPulse([], sample, 500, 73);
    for (let i = 2; i <= 20; i++) history = appendPulse(history, sample, i * 500);
    expect(history.filter(point => point.gpu !== null)).toHaveLength(1);
    expect(history[0].gpu).toBe(73);
    expect(history.at(-1)?.gpu).toBeNull();
    const missing = appendPulse([], {
      cpu_percent: null, memory_total_kib: null, memory_available_kib: null,
    }, 500);
    expect(missing[0]).toEqual({ time: 500, cpu: null, ram: null, gpu: null });
  });

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

  it('keeps low CPU load visible while retaining an honest labeled zero origin', () => {
    const low = [2, 4, 5].map((cpu, time) => ({ time, cpu, ram: 39, gpu: null }));
    expect(activityAxis(low, 'cpu')).toEqual({ min: 0, max: 7 });
    expect(activityAxis(low, 'ram')).toEqual({ min: 37, max: 41 });
    expect(activityAxis(low, 'gpu')).toEqual({ min: 0, max: 100 });
    expect(activityAxis([{ time: 0, cpu: 91, ram: 99, gpu: 0 }], 'cpu'))
      .toEqual({ min: 89, max: 93 });
    expect(activityAxis([{ time: 0, cpu: 0, ram: 0, gpu: 0 }], 'ram'))
      .toEqual({ min: 0, max: 4 });
    expect(activityAxis([{ time: 0, cpu: null, ram: null, gpu: null }], 'cpu'))
      .toEqual({ min: 0, max: 100 });
  });

  it('gives identical CPU and RAM history the exact same adaptive scale', () => {
    const samples = [39.12, 39.25, 39.31].map((value, time) =>
      ({ time, cpu: value, ram: value, gpu: null }));
    expect(activityAxis(samples, 'cpu')).toEqual(activityAxis(samples, 'ram'));
  });

  it('preserves sub-percent RAM changes from each true 500ms pulse', () => {
    const a = appendPulse([], {
      cpu_percent: 5, memory_total_kib: 1_000_000, memory_available_kib: 608_800,
    }, 500);
    const b = appendPulse(a, {
      cpu_percent: 6, memory_total_kib: 1_000_000, memory_available_kib: 608_100,
    }, 1000);
    expect(b.map(point => point.cpu)).toEqual([5, 6]);
    expect(b.map(point => point.ram)).toEqual([39.12, 39.19]);
    expect(b.map(point => point.time)).toEqual([500, 1000]);
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
