/** Bounded, honest, two-second system telemetry for Apache ECharts.
 * This module is intentionally DOM-independent so the data rules are testable.
 */
import type { SystemMetrics } from './sdk/core';
import { percentUsed } from './system-model';

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

export const MAX_TELEMETRY_POINTS = 30; // At two-second cadence, up to one minute.

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
    ram: percentUsed(metrics.memory_total_kib, metrics.memory_available_kib),
    gpu: gpuPeak(metrics),
  }];
  return next.slice(-MAX_TELEMETRY_POINTS);
}

/** Explicit, labelled local axis ranges make low activity readable without
 * pretending the chart spans 0–100%. Missing samples cannot create fake peaks.
 * CPU/GPU retain a true zero origin; RAM focuses on its measured range.
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
  if (field !== 'ram') {
    const max = Math.min(100, Math.max(15, Math.ceil(Math.max(...values) * 1.25 / 5) * 5));
    return { min: 0, max };
  }
  const minSample = Math.min(...values);
  const maxSample = Math.max(...values);
  const min = Math.max(0, Math.floor((minSample - 3) / 5) * 5);
  const max = Math.min(100, Math.max(min + 5, Math.ceil((maxSample + 3) / 5) * 5));
  return { min, max };
}

/** Return an accessible live label without depending on SVG rendering. */
export function telemetrySummary(metrics: SystemMetrics | null): string {
  if (!metrics) return 'Waiting for system telemetry';
  const value = (percent: number | null) => percent === null ? 'unavailable' : percent + '%';
  return [
    'CPU ' + value(metrics.cpu_percent),
    'RAM ' + value(percentUsed(metrics.memory_total_kib, metrics.memory_available_kib)),
    'GPU peak ' + value(gpuPeak(metrics)),
  ].join(', ');
}
