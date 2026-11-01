import { describe, expect, it } from 'vitest';
import { dayOfYear, daysInMonth, isoDate, isoWeek, monthGrid, shiftDays, shiftMonth } from './calendar-model';

describe('Svelte Calendar Gregorian model', () => {
  it('respects leap years, including century rules', () => {
    expect(daysInMonth(2024, 2)).toBe(29);
    expect(daysInMonth(1900, 2)).toBe(28);
    expect(daysInMonth(2000, 2)).toBe(29);
    expect(daysInMonth(2025, 2)).toBe(28);
  });
  it('keeps six Monday-first rows with adjacent days', () => {
    const grid = monthGrid(2026, 11);
    expect(grid).toHaveLength(6);
    expect(grid.every((row) => row.length === 7)).toBe(true);
    expect(isoDate(grid[0][0]!)).toBe('2026-10-26');
    expect(isoDate(grid[0][6]!)).toBe('2026-11-01');
    expect(isoDate(grid[5][6]!)).toBe('2026-12-06');
    expect(isoWeek(grid[0][0]!)).toEqual({ year: 2026, week: 44 });
  });
  it('clamps selection on short months and respects year bounds', () => {
    expect(shiftMonth({ year: 2024, month: 1, day: 31 }, 1)).toEqual({ year: 2024, month: 2, day: 29 });
    expect(shiftMonth({ year: 2025, month: 1, day: 31 }, 1)).toEqual({ year: 2025, month: 2, day: 28 });
    expect(shiftMonth({ year: 2026, month: 1, day: 15 }, -1)).toEqual({ year: 2025, month: 12, day: 15 });
    expect(shiftMonth({ year: 1, month: 1, day: 1 }, -1)).toBeNull();
    expect(shiftMonth({ year: 9999, month: 12, day: 31 }, 1)).toBeNull();
  });
  it('calculates ISO week years and day-of-year', () => {
    expect(isoWeek({ year: 2021, month: 1, day: 1 })).toEqual({ year: 2020, week: 53 });
    expect(isoWeek({ year: 2018, month: 12, day: 31 })).toEqual({ year: 2019, week: 1 });
    expect(dayOfYear({ year: 2026, month: 11, day: 1 })).toBe(305);
    expect(dayOfYear({ year: 2024, month: 12, day: 31 })).toBe(366);
  });
  it('avoids the JavaScript Date year-1900 trap', () => {
    expect(isoDate(shiftDays({ year: 1, month: 1, day: 1 }, 1)!)).toBe('0001-01-02');
    expect(shiftDays({ year: 1, month: 1, day: 1 }, -1)).toBeNull();
    expect(shiftDays({ year: 9999, month: 12, day: 31 }, 1)).toBeNull();
  });
});
