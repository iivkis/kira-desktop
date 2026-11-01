import { describe, expect, it } from 'vitest';
import { systemHealth } from './system-health';
import type { SystemMetrics } from './sdk/core';

const normal: SystemMetrics = {
  cpu_percent: 24, memory_total_kib: 1000, memory_available_kib: 580,
  swap_total_kib: 0, swap_free_kib: 0,
  root_total_kib: 1000, root_available_kib: 500,
  uptime_seconds: 3600, load: [0.2, 0.3, 0.1],
  battery_percent: null, battery_charging: null,
};

describe('read-only system health status', () => {
  it('does not call the whole system READY merely because Core and Niri work', () => {
    expect(systemHealth('connected', '0.1.0', true)).toMatchObject({ label: 'CHECKING', phase: 'pending' });
    expect(systemHealth('connected', '0.1.0', true, normal)).toMatchObject({ label: 'READY', phase: 'ready' });
  });

  it('prioritizes offline services before resource warnings', () => {
    expect(systemHealth('connecting', '', null, normal).label).toBe('STARTING');
    expect(systemHealth('unavailable', '', false, normal)).toMatchObject({ label: 'CORE OFF', phase: 'error' });
    expect(systemHealth('connected', '', false, normal)).toMatchObject({ label: 'NIRI OFF', phase: 'error' });
    expect(systemHealth('connected', '', null, normal).label).toBe('WAITING');
  });

  it('shows actual memory, disk and battery pressure', () => {
    expect(systemHealth('connected', '', true, { ...normal, root_available_kib: 50 }).label).toBe('LOW DISK');
    expect(systemHealth('connected', '', true, { ...normal, memory_available_kib: 50 }).label).toBe('LOW MEM');
    expect(systemHealth('connected', '', true, { ...normal, battery_percent: 10, battery_charging: false }).label).toBe('LOW BAT');
    expect(systemHealth('connected', '', true, { ...normal, battery_percent: 10, battery_charging: true }).label).toBe('READY');
    expect(systemHealth('connected', '', true, { ...normal, cpu_percent: 98 }).label).toBe('BUSY');
    expect(systemHealth('connected', '', true, { ...normal, root_total_kib: null }).label).toBe('PARTIAL');
  });

  it('never renders an unknown metric as zero', () => {
    const value = systemHealth('connected', '0.1.0', true, { ...normal, cpu_percent: null });
    expect(value.label).toBe('CHECKING');
    expect(value.details).toContain('CPU: Measuring');
    expect(value.details).toContain('Kira Core: Online (v0.1.0)');
  });
});
