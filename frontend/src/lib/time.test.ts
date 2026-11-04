import { describe, expect, it } from 'vitest';
import { formatClock } from './time';

describe('formatClock', () => {
  it('formats a UTC clock using 24-hour time', () => {
    expect(formatClock(new Date('2026-10-31T21:04:00Z'), 'UTC')).toBe('21:04');
  });

  it('treats its second argument as an IANA time zone, never a UI locale', () => {
    const instant = new Date('2026-11-04T12:07:00Z');
    expect(formatClock(instant, 'Europe/Helsinki')).toBe('14:07');
    expect(formatClock(instant, 'Asia/Tokyo', true)).toBe('21:07:00');
  });
});
