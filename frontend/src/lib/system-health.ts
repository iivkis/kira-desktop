import type { SystemMetrics } from './sdk/core';

/** A system status reflects actual resource conditions as well as services.
 * Never imply the whole machine is healthy solely because Core responds.
 */
export type CoreConnectionStatus = 'connecting' | 'connected' | 'unavailable';
export type HealthPhase = 'ready' | 'pending' | 'warning' | 'error';

export interface HealthSummary {
  label: string;
  phase: HealthPhase;
  details: string;
}

function percentUsed(total: number | null, available: number | null): number | null {
  if (total === null || available === null || total <= 0 ||
      available < 0 || available > total) return null;
  return Math.round((total - available) / total * 100);
}

export function systemHealth(
  core: CoreConnectionStatus,
  version: string,
  niriConnected: boolean | null,
  metrics: SystemMetrics | null = null,
): HealthSummary {
  const coreDetail = core === 'connected'
    ? 'Online' + (version ? ' (v' + version + ')' : '')
    : core === 'connecting' ? 'Connecting' : 'Offline';
  const niriDetail = niriConnected === null
    ? 'Waiting for first snapshot'
    : niriConnected ? 'Connected' : 'Disconnected';

  const mem = metrics && percentUsed(metrics.memory_total_kib, metrics.memory_available_kib);
  const disk = metrics && percentUsed(metrics.root_total_kib, metrics.root_available_kib);
  const details = [
    'Kira Core: ' + coreDetail,
    'Niri IPC: ' + niriDetail,
    'CPU: ' + (metrics?.cpu_percent === null || !metrics ? 'Measuring' : metrics.cpu_percent + '%'),
    'Memory: ' + (mem === null ? 'Unavailable' : mem + '% used'),
    'Root disk: ' + (disk === null ? 'Unavailable' : disk + '% used'),
    'Battery: ' + (metrics?.battery_percent == null ? 'Not detected' : metrics.battery_percent + '%'),
  ].join('\n');
  if (core === 'unavailable') return { label: 'CORE OFF', phase: 'error', details };
  if (core === 'connecting') return { label: 'STARTING', phase: 'pending', details };
  if (niriConnected === false) return { label: 'NIRI OFF', phase: 'error', details };
  if (niriConnected === null) return { label: 'WAITING', phase: 'pending', details };
  if (!metrics || mem === null || metrics.cpu_percent === null) {
    return { label: 'CHECKING', phase: 'pending', details };
  }
  if (disk !== null && disk >= 90) return { label: 'LOW DISK', phase: 'warning', details };
  if (mem >= 90) return { label: 'LOW MEM', phase: 'warning', details };
  if (metrics.battery_percent !== null && metrics.battery_percent <= 15 &&
      metrics.battery_charging === false) {
    return { label: 'LOW BAT', phase: 'warning', details };
  }
  if (metrics.cpu_percent >= 95) return { label: 'BUSY', phase: 'warning', details };
  return { label: 'READY', phase: 'ready', details };
}
