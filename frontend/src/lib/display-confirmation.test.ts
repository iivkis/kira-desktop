import { describe, expect, it } from 'vitest';
import { confirmationDeadlineMonotonic, confirmationSecondsRemaining } from './display-confirmation';

describe('native display rollback countdown', () => {
  it('anchors the native epoch deadline once to monotonic time', () => {
    const deadline = confirmationDeadlineMonotonic(1_015_000, 1_000_000, 500);
    expect(deadline).toBe(15_500);
    expect(confirmationSecondsRemaining(deadline, 500)).toBe(15);
    expect(confirmationSecondsRemaining(deadline, 4_501)).toBe(11);
    expect(confirmationSecondsRemaining(deadline, 15_501)).toBe(0);
    // Wall-clock jumps are irrelevant after receipt: the visible clock
    // uses only the fixed monotonic deadline and current monotonic time.
    expect(confirmationSecondsRemaining(deadline, 4_501)).toBe(11);
  });

  it('does not restart a timer when a native event arrives late', () => {
    const late = confirmationDeadlineMonotonic(1_015_000, 1_012_000, 500);
    expect(confirmationSecondsRemaining(late, 500)).toBe(3);
    expect(confirmationSecondsRemaining(late, 3_500)).toBe(0);
    expect(confirmationSecondsRemaining(late, 7_000)).toBe(0);
  });

  it('caps untrusted durations and handles old shells without a deadline', () => {
    const clamped = confirmationDeadlineMonotonic(999_999, 0, 300);
    expect(confirmationSecondsRemaining(clamped, 300)).toBe(15);
    expect(confirmationSecondsRemaining(
      confirmationDeadlineMonotonic(1_000, 2_000, 300), 300,
    )).toBe(0);
    expect(confirmationDeadlineMonotonic(null, 0, 300)).toBeNull();
    expect(confirmationDeadlineMonotonic(10, Number.NaN, 1)).toBeNull();
    expect(confirmationSecondsRemaining(null, 0)).toBeNull();
    expect(confirmationSecondsRemaining(1000, Number.POSITIVE_INFINITY)).toBeNull();
  });
});
