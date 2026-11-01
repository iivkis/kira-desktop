/** A random but non-repeating flight plan for the calendar clock easter egg.
 *
 * Every click uses a new randomized direction for every numeral. Adjacent
 * numerals also choose different 45-degree sectors to avoid synchronized
 * movement. An individual numeral is explicitly forbidden from revisiting
 * its sector from the previous click, even when random() returns the same
 * sequence twice. No DOM, timers or GSAP dependencies: easy to regression-test.
 */
export interface ClockFlight {
  sector: number;
  x: number;
  y: number;
  rotation: number;
  scale: number;
  opacity: number;
}

const SECTORS = 8;
const directions = Array.from({ length: SECTORS }, (_, index) => index);

export function nextClockFlights(
  count: number,
  previous: readonly ClockFlight[] = [],
  random: () => number = Math.random,
): ClockFlight[] {
  const used = new Set<number>();
  const unit = () => Math.max(0, Math.min(0.9999999999999999, random()));
  return Array.from({ length: Math.max(0, Math.min(32, Math.floor(count))) }, (_, index) => {
    const oldSector = previous[index]?.sector;
    // Try to keep every digit in a distinct direction AND never repeat its
    // previous direction. More than eight digits can reuse sectors safely.
    const distinct = directions.filter(sector => !used.has(sector) && sector !== oldSector);
    const options = distinct.length ? distinct : directions.filter(sector => sector !== oldSector);
    const sector = options[Math.floor(unit() * options.length)];
    used.add(sector);

    // Randomize angle inside the selected sector and distance independently.
    const angle = (sector + 0.16 + unit() * 0.68) * Math.PI / 4;
    const distance = 24 + unit() * 23;
    const spin = (unit() < 0.5 ? -1 : 1) * (32 + unit() * 73);
    return {
      sector,
      x: Math.cos(angle) * distance,
      y: Math.sin(angle) * distance,
      rotation: spin,
      scale: 0.64 + unit() * 0.2,
      opacity: 0.48 + unit() * 0.29,
    };
  });
}
