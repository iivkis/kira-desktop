import { describe, expect, it } from 'vitest';
import { bytesLabel, defaultSort, fractionLabel, nextSort, percentUsed, sortedProcesses, systemHeadline } from './system-model';
import type { ProcessRow, SystemMetrics } from './sdk/core';

const processes: ProcessRow[] = [
  { pid: 3, name: 'zebra', state: 'R', cpu_percent: 20, rss_kib: 4000 },
  { pid: 1, name: 'alpha', state: 'S', cpu_percent: 80, rss_kib: 1000 },
  { pid: 2, name: 'beta', state: 'S', cpu_percent: 50, rss_kib: 2000 },
  { pid: 4, name: 'missing', state: 'S', cpu_percent: null, rss_kib: null },
];

describe('Svelte System Monitor model', () => {
  it('sorts each column in both directions and keeps missing metrics last', () => {
    for (const [field, ascending, descending] of [
      ['name', [1, 2, 4, 3], [3, 4, 2, 1]],
      ['pid', [1, 2, 3, 4], [4, 3, 2, 1]],
      ['cpu', [3, 2, 1, 4], [1, 2, 3, 4]],
      ['memory', [1, 2, 3, 4], [3, 2, 1, 4]],
    ] as const) {
      expect(sortedProcesses(processes, '', { field, descending: false }).map(row => row.pid)).toEqual(ascending);
      expect(sortedProcesses(processes, '', { field, descending: true }).map(row => row.pid)).toEqual(descending);
    }
    expect(nextSort(defaultSort, 'cpu')).toEqual({ field: 'cpu', descending: false });
    expect(nextSort(defaultSort, 'memory')).toEqual({ field: 'memory', descending: true });
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
    expect(fractionLabel(null, null)).toBe('UNAVAILABLE');
    expect(bytesLabel(1_048_576)).toBe('1.0 GiB');
  });

  it('prioritizes offline, storage and memory pressure', () => {
    const m: SystemMetrics = {
      cpu_percent: 12, memory_total_kib: 100, memory_available_kib: 80,
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
