/** Derive an honest status from both existing, real Core and Niri signals.
 * No extra polling or optimistic connection indicators.
 */
export type CoreConnectionStatus = 'connecting' | 'connected' | 'unavailable';
export type HealthPhase = 'ready' | 'pending' | 'error';

export interface HealthSummary {
  label: string;
  phase: HealthPhase;
  details: string;
}

export function systemHealth(
  core: CoreConnectionStatus,
  version: string,
  niriConnected: boolean | null,
): HealthSummary {
  const coreDetail = core === 'connected'
    ? 'Online' + (version ? ' (v' + version + ')' : '')
    : core === 'connecting' ? 'Connecting' : 'Offline';
  const niriDetail = niriConnected === null
    ? 'Waiting for first snapshot'
    : niriConnected ? 'Connected' : 'Disconnected';

  const details = 'Kira Core: ' + coreDetail + '\nNiri IPC: ' + niriDetail;
  if (core === 'unavailable') return { label: 'CORE OFF', phase: 'error', details };
  if (core === 'connecting') return { label: 'STARTING', phase: 'pending', details };
  if (niriConnected === false) return { label: 'NIRI OFF', phase: 'error', details };
  if (niriConnected === null) return { label: 'WAITING', phase: 'pending', details };
  return { label: 'READY', phase: 'ready', details };
}
