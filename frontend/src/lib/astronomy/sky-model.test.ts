import { describe, expect, it } from 'vitest';
import { horizonPoint, localObservationDate, observerCoordinates, shortestSiderealDelta, siderealDegrees } from './sky-model';
import { STARS, CONSTELLATION_LABELS, CONSTELLATION_LINES } from './sky-catalog';

describe('offline sky chart / coordinate model', () => {
  it('keeps astronomical source data local and geographically bounded', () => {
    expect(STARS.length).toBeGreaterThan(2000);
    expect(CONSTELLATION_LINES.length).toBeGreaterThanOrEqual(88);
    expect(CONSTELLATION_LABELS.length).toBeGreaterThanOrEqual(88);
    for (const [ra, dec, mag] of STARS) {
      expect(ra).toBeGreaterThanOrEqual(0);
      expect(ra).toBeLessThan(360);
      expect(dec).toBeGreaterThanOrEqual(-90);
      expect(dec).toBeLessThanOrEqual(90);
      expect(mag).toBeLessThanOrEqual(5.35);
    }
  });

  it('refuses missing coordinates rather than silently inventing a home location', () => {
    expect(observerCoordinates('', '0')).toBeNull();
    expect(observerCoordinates('90.1', '0')).toBeNull();
    expect(observerCoordinates('51.4', '-180.1')).toBeNull();
    expect(observerCoordinates('foo', '0')).toBeNull();
    expect(observerCoordinates('51.4779', '0')).toEqual({ latitude: 51.4779, longitude: 0 });
  });

  it('uses selected civil day and local time without timezone guessing', () => {
    const date = localObservationDate({ year: 2026, month: 11, day: 3 }, '22:30');
    expect(date?.getFullYear()).toBe(2026);
    expect(date?.getMonth()).toBe(10);
    expect(date?.getDate()).toBe(3);
    expect(date?.getHours()).toBe(22);
    expect(date?.getMinutes()).toBe(30);
    expect(localObservationDate({ year: 2026, month: 11, day: 3 }, '27:00')).toBeNull();
    expect(localObservationDate({ year: 2500, month: 11, day: 3 }, '22:00')).toBeNull();
  });

  it('agrees with J2000 Greenwich mean sidereal angle', () => {
    expect(siderealDegrees(new Date('2000-01-01T12:00:00.000Z'), 0)).toBeCloseTo(280.46061837, 6);
  });

  it('puts the celestial equator overhead at the equator at transit', () => {
    const point = horizonPoint(45, 0, 0, 45);
    expect(point.altitude).toBeCloseTo(90, 8);
    expect(Math.hypot(point.x, point.y)).toBeCloseTo(0, 8);
    expect(horizonPoint(45, 0, 0, 135).altitude).toBeCloseTo(0, 8);
  });

  it('projects eastern and western horizons on the correct sides', () => {
    expect(horizonPoint(90, 0, 0, 0).x).toBeCloseTo(1, 8);
    expect(horizonPoint(270, 0, 0, 0).x).toBeCloseTo(-1, 8);
    expect(horizonPoint(0, -30, 50, 0).altitude).toBeCloseTo(10, 8);
  });

  it('rotates by shortest circular path across zero', () => {
    expect(shortestSiderealDelta(359, 1)).toBe(2);
    expect(shortestSiderealDelta(1, 359)).toBe(-2);
  });
});
