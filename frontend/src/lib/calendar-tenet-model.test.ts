import { describe, expect, it } from 'vitest';
import { dualTime, temporalProgress, tenetFragmentsFor } from './calendar-tenet-model';
import { nextClockFlights } from './clock-flight';

describe('TENET Calendar temporal contract', () => {
  it('advances simultaneous forward and inverted flows from a single cursor', () => {
    expect(dualTime(0)).toEqual({ forward: 0, inverted: 1 });
    expect(dualTime(0.25)).toEqual({ forward: 0.25, inverted: 0.75 });
    expect(dualTime(0.5)).toEqual({ forward: 0.5, inverted: 0.5 });
    expect(dualTime(1)).toEqual({ forward: 1, inverted: 0 });
    expect(temporalProgress(-5, -1)).toBe(1);
    expect(temporalProgress(5, 1)).toBe(1);
    expect(temporalProgress(Number.NaN, 1)).toBe(0);
  });

  it('inverts one remembered flight without recalculating coordinates', () => {
    const outward = tenetFragmentsFor(1);
    const inbound = [...outward].reverse().reverse();
    expect(inbound).toEqual(outward);
    for (const flight of outward) {
      // The very same recorded flight becomes a return when time is reversed.
      const center = { x: 0, y: 0 };
      const position = (progress: number) => ({
        x: center.x + progress * flight.x,
        y: center.y + progress * flight.y,
        rotation: progress * flight.rotation,
      });
      for (const t of [0, 0.1, 0.5, 0.9, 1]) {
        expect(position(dualTime(t).inverted))
          .toEqual(position(dualTime(1 - t).forward));
      }
      // IEEE-754 -0 is physically identical to +0; compare magnitudes.
      expect(Math.abs(position(0).x)).toBe(0);
      expect(Math.abs(position(0).y)).toBe(0);
      expect(Math.abs(position(0).rotation)).toBe(0);
    }
  });

  it('changes outbound history when month direction changes, not the contact point', () => {
    const forward = tenetFragmentsFor(1);
    const backward = tenetFragmentsFor(-1);
    expect(forward).toHaveLength(4);
    expect(backward).toHaveLength(4);
    for (let i = 0; i < forward.length; i++) {
      expect(backward[i].x).toBe(-forward[i].x);
      expect(backward[i].y).toBe(forward[i].y);
      expect(backward[i].rotation).toBe(forward[i].rotation);
    }
  });

  it('randomizes only between clock events, never while reversing a single one', () => {
    const first = nextClockFlights(6, [], () => 0.2);
    // Saved outbound positions are reused as-is on the return.
    const memory = [...first];
    expect(memory).toEqual(first);
    const second = nextClockFlights(6, first, () => 0.2);
    expect(second.every((flight, i) => flight.sector !== first[i].sector)).toBe(true);
    expect(first).toEqual(memory);
  });
});
