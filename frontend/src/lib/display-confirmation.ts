/** Convert a native Unix-epoch rollback deadline into the WebKit monotonic
 * clock once, at event receipt. Subsequent Date.now() or timezone changes
 * cannot move the displayed countdown relative to Rust's Instant watchdog.
 * This is presentation only: Rust remains the authority for confirm/revert.
 */
export function confirmationDeadlineMonotonic(
  deadlineUnixMs: number | null,
  receivedUnixMs: number,
  receivedMonotonicMs: number,
): number | null {
  if (deadlineUnixMs === null || !Number.isFinite(deadlineUnixMs) ||
      !Number.isFinite(receivedUnixMs) || !Number.isFinite(receivedMonotonicMs)) {
    return null;
  }
  // The native session lasts at most 15 seconds. A bad wall clock or delayed
  // event may shorten the visible period, never grant extra rollback time.
  const remainingMs = Math.max(0, Math.min(15_000, deadlineUnixMs - receivedUnixMs));
  return receivedMonotonicMs + remainingMs;
}

export function confirmationSecondsRemaining(
  deadlineMonotonicMs: number | null,
  monotonicNow: number,
): number | null {
  if (deadlineMonotonicMs === null || !Number.isFinite(deadlineMonotonicMs) ||
      !Number.isFinite(monotonicNow)) return null;
  return Math.max(0, Math.ceil((deadlineMonotonicMs - monotonicNow) / 1_000));
}
