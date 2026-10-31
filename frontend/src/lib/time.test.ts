import { describe, expect, it } from 'vitest';
import { formatClock } from './time';

describe('formatClock', () => {
  it('formats a UTC clock using 24-hour time', () => {
    expect(formatClock(new Date('2026-10-31T21:04:00Z'), 'UTC')).toBe('21:04');
  });
});
