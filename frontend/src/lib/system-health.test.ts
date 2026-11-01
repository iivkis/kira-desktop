import { describe, expect, it } from 'vitest';
import { systemHealth } from './system-health';

describe('system summary from real Core and Niri signals', () => {
  it('is ready only when Core and Niri are both connected', () => {
    const ready = systemHealth('connected', '0.1.0', true);
    expect(ready.phase).toBe('ready');
    expect(ready.label).toBe('READY');
    expect(ready.details).toContain('Kira Core: Online (v0.1.0)');
    expect(ready.details).toContain('Niri IPC: Connected');
  });

  it('shows connection progress instead of claiming system readiness', () => {
    expect(systemHealth('connecting', '', null)).toMatchObject({
      label: 'STARTING', phase: 'pending',
    });
    expect(systemHealth('connected', '0.1.0', null)).toMatchObject({
      label: 'WAITING', phase: 'pending',
    });
  });

  it('prioritizes precise degraded signals', () => {
    const offlineCore = systemHealth('unavailable', '', false);
    expect(offlineCore).toMatchObject({ label: 'CORE OFF', phase: 'error' });
    expect(offlineCore.details).toContain('Niri IPC: Disconnected');
    expect(systemHealth('connected', '0.1.0', false)).toMatchObject({
      label: 'NIRI OFF', phase: 'error',
    });
  });

  it('does not fabricate a version when Core has not supplied one', () => {
    expect(systemHealth('connected', '', true).details).toContain('Kira Core: Online\n');
  });
});
