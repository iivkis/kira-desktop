/** TENET hover refraction geometry, independent of DOM/GSAP.
 * A mouse interaction records one bounded path. On pointer leave, the
 * INVERTED optical trace visits the same points in the opposite order.
 * The functional target never moves from its hit-testing rectangle.
 */
export interface LensPoint {
  x: number;
  y: number;
  dx: number;
  dy: number;
}

export const MAX_LENS_HISTORY = 16;

export function lensPoint(
  clientX: number, clientY: number, left: number, top: number,
  width: number, height: number,
): LensPoint {
  const w = Math.max(1, width);
  const h = Math.max(1, height);
  const nx = Math.max(0, Math.min(1, (clientX - left) / w));
  const ny = Math.max(0, Math.min(1, (clientY - top) / h));
  return {
    x: Math.round(nx * 1000) / 10,
    y: Math.round(ny * 1000) / 10,
    dx: Math.round((nx - 0.5) * 8 * 100) / 100,
    dy: Math.round((ny - 0.5) * 8 * 100) / 100,
  };
}

export function rememberLensPoint(
  history: readonly LensPoint[], next: LensPoint,
): LensPoint[] {
  const last = history[history.length - 1];
  if (last && last.x === next.x && last.y === next.y) return history.slice();
  return [...history, next].slice(-MAX_LENS_HISTORY);
}

/** A return uses the SAME stored coordinates; no newly synthesized path. */
export function inverseLensPath(history: readonly LensPoint[]): LensPoint[] {
  return [...history].reverse();
}
