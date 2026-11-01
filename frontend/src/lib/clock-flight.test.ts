import { describe, expect, it } from 'vitest';
import { nextClockFlights } from './clock-flight';

describe('Calendar GSAP clock chaos flight plans', () => {
  it('sends six numerals into six distinct directions on every click', () => {
    const plan = nextClockFlights(6, [], () => 0.4);
    expect(plan).toHaveLength(6);
    expect(new Set(plan.map(digit => digit.sector)).size).toBe(6);
    for (const digit of plan) {
      expect(digit.sector).toBeGreaterThanOrEqual(0);
      expect(digit.sector).toBeLessThan(8);
      expect(Math.hypot(digit.x, digit.y)).toBeGreaterThanOrEqual(24);
      expect(Math.hypot(digit.x, digit.y)).toBeLessThan(47);
      expect(Math.abs(digit.rotation)).toBeGreaterThanOrEqual(32);
      expect(digit.scale).toBeGreaterThanOrEqual(0.64);
      expect(digit.opacity).toBeGreaterThanOrEqual(0.48);
    }
  });

  it('never sends the same numeral into its previous direction', () => {
    // Even pathological RNG returning identical values on every draw must
    // produce a different sector for each digit on consecutive clicks.
    let previous = nextClockFlights(6, [], () => 0);
    for (let click = 0; click < 30; click++) {
      const next = nextClockFlights(6, previous, () => 0);
      expect(new Set(next.map(digit => digit.sector)).size).toBe(6);
      expect(next.every((digit, i) => digit.sector !== previous[i].sector)).toBe(true);
      previous = next;
    }
  });

  it('changes angle, distance and spin when the random seed changes', () => {
    const previous = nextClockFlights(6, [], () => 0.2);
    const left = nextClockFlights(6, previous, () => 0.1);
    const right = nextClockFlights(6, previous, () => 0.9);
    expect(left.map(({ x, y, rotation }) => [x, y, rotation]))
      .not.toEqual(right.map(({ x, y, rotation }) => [x, y, rotation]));
    expect(left.every((digit, i) => digit.sector !== previous[i].sector)).toBe(true);
    expect(right.every((digit, i) => digit.sector !== previous[i].sector)).toBe(true);
  });

  it('handles empty glyph lists and repeated clicks with more than eight glyphs', () => {
    expect(nextClockFlights(0)).toEqual([]);
    expect(nextClockFlights(-2)).toEqual([]);
    const old = nextClockFlights(10, [], () => 0.5);
    const next = nextClockFlights(10, old, () => 0.5);
    expect(next).toHaveLength(10);
    expect(next.every((digit, i) => digit.sector !== old[i].sector)).toBe(true);
  });
});
