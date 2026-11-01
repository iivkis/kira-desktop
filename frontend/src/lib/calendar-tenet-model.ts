/** Pure, inspectable time laws for Calendar's GSAP TENET scenes.
 * One event records its outbound path once; inverted playback runs the same
 * keyframes in the opposite chronology. Not a mirrored x-axis.
 */
export type FlowDirection = 1 | -1;
export interface FragmentFlight { x: number; y: number; rotation: number; }

export function temporalProgress(progress: number, direction: FlowDirection): number {
  const u = Number.isFinite(progress) ? Math.max(0, Math.min(1, progress)) : 0;
  return direction === 1 ? u : 1 - u;
}

/** Coordinates relative to the same contact point; saved for the event.
 * Changing month direction changes the outbound history, not the return.
 */
export function tenetFragmentsFor(direction: number): FragmentFlight[] {
  const sign = direction < 0 ? -1 : 1;
  return [
    { x: -39 * sign, y: -18, rotation: -65 },
    { x: 31 * sign, y: 29, rotation: 42 },
    { x: -14 * sign, y: 40, rotation: 80 },
    { x: 37 * sign, y: -32, rotation: -35 },
  ];
}

/** Same master cursor for both streams. Each uses its own chronology. */
export function dualTime(progress: number): { forward: number; inverted: number } {
  return { forward: temporalProgress(progress, 1), inverted: temporalProgress(progress, -1) };
}
