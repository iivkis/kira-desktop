import { describe, expect, it } from 'vitest';
import { bytesLabel, defaultSort, fractionLabel, nextSort, percentUsed, processMemoryLabel, sortedProcesses, systemHeadline, temperatureLabel, usageLabel, isSampleFresh } from './system-model';
import type { ProcessRow, SystemMetrics } from './sdk/core';

const processes: ProcessRow[] = [
  { pid: 3, name: 'zebra', state: 'R', cpu_percent: 20, rss_kib: 4000, pss_kib: 1200 },
  { pid: 1, name: 'alpha', state: 'S', cpu_percent: 80, rss_kib: 1000, pss_kib: 800 },
  { pid: 2, name: 'beta', state: 'S', cpu_percent: 50, rss_kib: 2000, pss_kib: 1300 },
  { pid: 4, name: 'missing', state: 'S', cpu_percent: null, rss_kib: null, pss_kib: null },
];

describe('Svelte System Monitor model', () => {
  it('never confuses a missing or stopped native stream with live telemetry', () => {
    expect(isSampleFresh(null, 10_000)).toBe(false);
    expect(isSampleFresh(5_000, 10_000)).toBe(true);
    expect(isSampleFresh(4_000, 10_000)).toBe(true);
    expect(isSampleFresh(3_999, 10_000)).toBe(false);
    expect(isSampleFresh(10_001, 10_000)).toBe(false);
    expect(isSampleFresh(9_000, Number.NaN)).toBe(false);
    expect(isSampleFresh(7_500, 10_000, 2_500)).toBe(true);
    expect(isSampleFresh(7_499, 10_000, 2_500)).toBe(false);
  });

  it('sorts each column in both directions and keeps missing metrics last', () => {
    for (const [field, ascending, descending] of [
      ['name', [1, 2, 4, 3], [3, 4, 2, 1]],
      ['pid', [1, 2, 3, 4], [4, 3, 2, 1]],
      ['cpu', [3, 2, 1, 4], [1, 2, 3, 4]],
      ['memory', [1, 2, 3, 4], [3, 2, 1, 4]],
      ['pss', [1, 3, 2, 4], [2, 3, 1, 4]],
    ] as const) {
      expect(sortedProcesses(processes, '', { field, descending: false }).map(row => row.pid)).toEqual(ascending);
      expect(sortedProcesses(processes, '', { field, descending: true }).map(row => row.pid)).toEqual(descending);
    }
    expect(nextSort(defaultSort, 'cpu')).toEqual({ field: 'cpu', descending: false });
    expect(nextSort(defaultSort, 'memory')).toEqual({ field: 'memory', descending: true });
    expect(nextSort(defaultSort, 'pss')).toEqual({ field: 'pss', descending: true });
    expect(nextSort(defaultSort, 'name')).toEqual({ field: 'name', descending: false });
  });

  it('filters processes by case-insensitive name and PID', () => {
    expect(sortedProcesses(processes, 'ALP', defaultSort).map(row => row.pid)).toEqual([1]);
    expect(sortedProcesses(processes, ' 3 ', defaultSort).map(row => row.pid)).toEqual([3]);
    expect(sortedProcesses(processes, 'nonsense', defaultSort)).toEqual([]);
    expect(processes.map(row => row.pid)).toEqual([3, 1, 2, 4]);
  });

  it('distinguishes unavailable metrics from real zero', () => {
    expect(percentUsed(null, 0)).toBeNull();
    expect(percentUsed(0, 0)).toBeNull();
    expect(percentUsed(100, 150)).toBeNull();
    expect(percentUsed(100, 0)).toBe(100);
    expect(usageLabel(null, null)).toBe('UNAVAILABLE');
    expect(usageLabel(0, 0)).toBe('NOT CONFIGURED');
    expect(usageLabel(100000, 100000)).toBe('0%');
    expect(usageLabel(7700000, 7679000)).toBe('0.3%');
    expect(usageLabel(1000000, 999999)).toBe('<0.1%');
    expect(usageLabel(100, 60)).toBe('40%');
    expect(usageLabel(100, 120)).toBe('UNAVAILABLE');

    expect(fractionLabel(null, null)).toBe('UNAVAILABLE');
    expect(bytesLabel(1_048_576)).toBe('1.0 GiB');
    expect(processMemoryLabel(null)).toBe('—');
    expect(processMemoryLabel(0)).toBe('0 KiB');
    expect(processMemoryLabel(512)).toBe('512 KiB');
    expect(temperatureLabel(null)).toBe('UNAVAILABLE');
    expect(temperatureLabel(57.125)).toBe('57.1 °C');
  });

  it('prioritizes offline, storage and memory pressure', () => {
    const m: SystemMetrics = {
      cpu_percent: 12, cpu_temperature_c: null, gpus: [],
      memory_total_kib: 100, memory_available_kib: 80,
      swap_total_kib: 0, swap_free_kib: 0,
      root_total_kib: 100, root_available_kib: 80, uptime_seconds: 3600,
      load: [0, 0, 0], battery_percent: null, battery_charging: null,
    };
    expect(systemHeadline(m, false, true)).toBe('CORE OFFLINE');
    expect(systemHeadline(m, true, false)).toBe('NIRI DISCONNECTED');
    expect(systemHeadline(m, true, true)).toBe('ALL SYSTEMS NOMINAL');
    expect(systemHeadline({ ...m, root_available_kib: 1 }, true, true)).toBe('LOW STORAGE');
    expect(systemHeadline({ ...m, memory_available_kib: 1 }, true, true)).toBe('MEMORY PRESSURE');
  });
});
