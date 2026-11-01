import type { ProcessRow, SystemMetrics } from './sdk/core';

export type SortField = 'name' | 'pid' | 'cpu' | 'memory';
export type SortState = { field: SortField; descending: boolean };

export const defaultSort: SortState = { field: 'cpu', descending: true };

export function nextSort(current: SortState, field: SortField): SortState {
  return current.field === field
    ? { field, descending: !current.descending }
    : { field, descending: field === 'cpu' || field === 'memory' };
}

export function sortedProcesses(rows: ProcessRow[], query: string, state: SortState): ProcessRow[] {
  const q = query.trim().toLocaleLowerCase();
  const compareUnknown = (a: number | null, b: number | null): number => {
    if (a === null) return b === null ? 0 : 1;
    if (b === null) return -1;
    return state.descending ? b - a : a - b;
  };
  return rows.filter(row => !q || row.name.toLocaleLowerCase().includes(q) || String(row.pid).includes(q))
    .sort((a, b) => {
      let delta: number;
      switch (state.field) {
        case 'name':
          delta = a.name.toLocaleLowerCase().localeCompare(b.name.toLocaleLowerCase());
          if (state.descending) delta = -delta;
          break;
        case 'pid':
          delta = state.descending ? b.pid - a.pid : a.pid - b.pid;
          break;
        case 'cpu':
          delta = compareUnknown(a.cpu_percent, b.cpu_percent);
          break;
        case 'memory':
          delta = compareUnknown(a.rss_kib, b.rss_kib);
          break;
      }
      return delta || a.pid - b.pid;
    });
}

export function percentUsed(total: number | null, available: number | null): number | null {
  if (total === null || available === null || total <= 0 || available > total) return null;
  return Math.floor(100 * (total - available) / total);
}

export function bytesLabel(kib: number): string {
  return kib >= 1_048_576 ? (kib / 1_048_576).toFixed(1) + ' GiB' : Math.round(kib / 1024) + ' MiB';
}

export function fractionLabel(total: number | null, available: number | null): string {
  return total === null || available === null || available > total ? 'UNAVAILABLE'
    : bytesLabel(total - available) + ' / ' + bytesLabel(total);
}

export function systemHeadline(metrics: SystemMetrics | null, core: boolean, niri: boolean): string {
  if (!core) return 'CORE OFFLINE';
  if (!niri) return 'NIRI DISCONNECTED';
  if (!metrics) return 'MEASURING';
  if ((percentUsed(metrics.root_total_kib, metrics.root_available_kib) ?? 0) >= 90) return 'LOW STORAGE';
  if ((percentUsed(metrics.memory_total_kib, metrics.memory_available_kib) ?? 0) >= 90) return 'MEMORY PRESSURE';
  if (metrics.battery_percent !== null && metrics.battery_percent <= 15 && metrics.battery_charging === false) return 'LOW BATTERY';
  if (metrics.cpu_percent !== null && metrics.cpu_percent >= 95) return 'SYSTEM BUSY';
  if (metrics.cpu_percent === null || percentUsed(metrics.memory_total_kib, metrics.memory_available_kib) === null) return 'MEASURING';
  if (metrics.root_total_kib === null) return 'PARTIAL DATA';
  return 'ALL SYSTEMS NOMINAL';
}

export function uptimeLabel(seconds: number | null): string {
  if (seconds === null) return 'UNAVAILABLE';
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return (days ? days + 'd ' : '') + hours + 'h ' + minutes + 'm';
}
