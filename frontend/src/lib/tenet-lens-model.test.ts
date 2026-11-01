import { describe, expect, it } from 'vitest';
import {
  inverseLensPath, lensPoint, MAX_LENS_HISTORY, rememberLensPoint,
} from './tenet-lens-model';

describe('TENET optical hover geometry', () => {
  it('maps the real pointer to its hitbox without moving the hitbox itself', () => {
    expect(lensPoint(150, 100, 100, 75, 100, 50))
      .toEqual({ x: 50, y: 50, dx: 0, dy: 0 });
    expect(lensPoint(100, 75, 100, 75, 100, 50))
      .toEqual({ x: 0, y: 0, dx: -4, dy: -4 });
    expect(lensPoint(200, 125, 100, 75, 100, 50))
      .toEqual({ x: 100, y: 100, dx: 4, dy: 4 });
    expect(lensPoint(-100, 2000, 100, 75, 100, 50))
      .toEqual({ x: 0, y: 100, dx: -4, dy: 4 });
    expect(lensPoint(1, 1, 1, 1, 0, 0))
      .toEqual({ x: 0, y: 0, dx: -4, dy: -4 });
  });

  it('remembers a bounded, deduplicated real pointer path', () => {
    let history = [] as ReturnType<typeof rememberLensPoint>;
    for (let i = 0; i < 48; i++) {
      const next = lensPoint(10 + i, 25, 0, 0, 100, 50);
      history = rememberLensPoint(history, next);
    }
    expect(history).toHaveLength(MAX_LENS_HISTORY);
    expect(history[0].x).toBe(42);
    expect(history.at(-1)?.x).toBe(57);
    expect(rememberLensPoint(history, history[history.length - 1])).toEqual(history);
  });

  it('replays the exact same visited points in reverse causal order', () => {
    const history = [
      lensPoint(10, 10, 0, 0, 100, 100),
      lensPoint(60, 20, 0, 0, 100, 100),
      lensPoint(80, 75, 0, 0, 100, 100),
    ];
    const inverted = inverseLensPath(history);
    expect(inverted).toEqual([history[2], history[1], history[0]]);
    expect(inverseLensPath(inverted)).toEqual(history);
    expect(history[0]).toEqual(lensPoint(10, 10, 0, 0, 100, 100));
    expect(inverseLensPath([])).toEqual([]);
  });
});
