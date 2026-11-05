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
  return Math.floor((total - available) / total * 100);
}

export function systemHealth(
  core: CoreConnectionStatus,
  version: string,
  niriConnected: boolean | null,
  metrics: SystemMetrics | null = null,
  freshness: 'fresh' | 'stale' = 'fresh',
): HealthSummary {
  const coreDetail = core === 'connected'
    ? 'Доступен' + (version ? ' (v' + version + ')' : '')
    : core === 'connecting' ? 'Подключение' : 'Недоступен';
  const niriDetail = niriConnected === null
    ? 'Ожидание первого снимка'
    : niriConnected ? 'Подключён' : 'Отключён';

  // Expired measurements must not appear as live resource readings, but a
  // transient sampler interruption does not warrant a prominent warning.
  const live = freshness === 'stale' ? null : metrics;
  const mem = live && percentUsed(live.memory_total_kib, live.memory_available_kib);
  const disk = live && percentUsed(live.root_total_kib, live.root_available_kib);
  const details = [
    'Kira Core: ' + coreDetail,
    'Niri IPC: ' + niriDetail,
    'CPU: ' + (!live ? 'Нет данных' :
      live.cpu_percent === null ? 'Измеряется' : live.cpu_percent + '%'),
    'Память: ' + (!live ? 'Нет данных' :
      mem === null ? 'Недоступно' : mem + '% занято'),
    'Диск: ' + (!live ? 'Нет данных' :
      disk === null ? 'Недоступно' : disk + '% занято'),
    'Батарея: ' + (!live ? 'Нет данных' :
      live.battery_percent == null ? 'Не обнаружена' : live.battery_percent + '%'),
  ].join('\n');
  if (core === 'unavailable') return { label: 'CORE OFF', phase: 'error', details };
  if (core === 'connecting') return { label: 'STARTING', phase: 'pending', details };
  if (niriConnected === false) return { label: 'NIRI OFF', phase: 'error', details };
  if (niriConnected === null) return { label: 'WAITING', phase: 'pending', details };
  if (freshness === 'stale') {
    return { label: 'CHECKING', phase: 'pending', details };
  }
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
  if (disk === null) return { label: 'PARTIAL', phase: 'pending', details };
  return { label: 'READY', phase: 'ready', details };
}
