/** Bounded real native telemetry for Apache ECharts (500ms CPU/RAM, 2s GPU).
 * This module is intentionally DOM-independent so the data rules are testable.
 */
import type { SystemMetrics, SystemPulse } from './sdk/core';
import { percentUsedPrecise } from './system-model';

export interface Meter {
  title: string;
  percent: number | null;
  text: string;
}

export interface TelemetryPoint {
  time: number;
  cpu: number | null;
  ram: number | null;
  gpu: number | null;
}

export const MAX_TELEMETRY_POINTS = 30; // Legacy 2-second samples.
export const MAX_PULSE_POINTS = 120; // 500ms * 120 = 60 seconds.

export function gpuPeak(metrics: SystemMetrics): number | null {
  const values = metrics.gpus
    .map(gpu => gpu.utilization_percent)
    .filter((value): value is number => value !== null);
  return values.length ? Math.max(...values) : null;
}

export function chartMeters(meters: readonly Meter[], metrics: SystemMetrics | null): Meter[] {
  if (!metrics) return meters.slice();
  return [
    ...meters,
    ...metrics.gpus.map(gpu => ({
      title: gpu.label,
      percent: gpu.utilization_percent,
      text: gpu.utilization_percent === null ? 'UNAVAILABLE' : gpu.utilization_percent + '%',
    })),
  ];
}

/** Null samples are missing measurements, never zero. No synthetic points. */
export function appendTelemetry(
  history: readonly TelemetryPoint[],
  metrics: SystemMetrics,
  time: number,
): TelemetryPoint[] {
  const next = [...history, {
    time,
    cpu: metrics.cpu_percent,
    ram: percentUsedPrecise(metrics.memory_total_kib, metrics.memory_available_kib),
    gpu: gpuPeak(metrics),
  }];
  return next.slice(-MAX_TELEMETRY_POINTS);
}

/** One real sample per native 500ms pulse. GPU is populated only on the
 * next pulse after a genuine two-second GPU reading, never interpolated or
 * multiplied into twenty fictitious 500ms measurements.
 */
export function appendPulse(
  history: readonly TelemetryPoint[],
  pulse: SystemPulse,
  time: number,
  gpuReading: number | null = null,
): TelemetryPoint[] {
  const next = [...history, {
    time,
    cpu: pulse.cpu_percent,
    ram: percentUsedPrecise(pulse.memory_total_kib, pulse.memory_available_kib),
    gpu: gpuReading,
  }];
  return next.slice(-MAX_PULSE_POINTS);
}

/** CPU and RAM use the SAME bounded adaptive-window rule.
 * CPU was previously zero-anchored while RAM was zoomed; that made the
 * plots behave differently despite identical 500ms pulse events.
 * The per-lane min/max are always printed alongside the plot, so zoom
 * never claims a larger change than actually occurred.
 */
export function activityAxis(
  samples: readonly TelemetryPoint[],
  field: 'cpu' | 'ram' | 'gpu',
): { min: number; max: number } {
  const values = samples
    .map(point => point[field])
    .filter((value): value is number =>
      value !== null && Number.isFinite(value) && value >= 0 && value <= 100);
  if (!values.length) return { min: 0, max: 100 };
  const low = Math.min(...values);
  const high = Math.max(...values);
  const spread = high - low;
  const padding = Math.max(2, spread * 0.25);
  let min = Math.max(0, Math.floor((low - padding) * 10) / 10);
  let max = Math.min(100, Math.ceil((high + padding) * 10) / 10);
  // Keep a minimum four-point interval without falsely exceeding 0–100%.
  if (max - min < 4) {
    min = Math.max(0, Math.floor((low - 2) * 10) / 10);
    max = Math.min(100, Math.max(min + 4, Math.ceil((high + 2) * 10) / 10));
    min = Math.max(0, Math.min(min, max - 4));
  }
  return { min, max };
}

/** Return an accessible live label without depending on SVG rendering. */
export function telemetrySummary(metrics: SystemMetrics | null): string {
  if (!metrics) return 'Waiting for system telemetry';
  const value = (percent: number | null) => percent === null ? 'unavailable' : percent + '%';
  return [
    'CPU ' + value(metrics.cpu_percent),
    'RAM ' + value(percentUsedPrecise(metrics.memory_total_kib, metrics.memory_available_kib)),
    'GPU peak ' + value(gpuPeak(metrics)),
  ].join(', ');
}
