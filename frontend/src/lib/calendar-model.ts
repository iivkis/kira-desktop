/** Pure Gregorian calendar model for the Svelte widget.
 * UTC arithmetic avoids DST skips and Date's 1900 offset for years 0–99.
 */
export interface CivilDate {
  year: number;
  month: number;
  day: number;
}
const DAY_MS = 86_400_000;

function utcDate(date: CivilDate): Date {
  const value = new Date(0);
  value.setUTCHours(12, 0, 0, 0);
  value.setUTCFullYear(date.year, date.month - 1, date.day);
  return value;
}
function civilDate(date: Date): CivilDate | null {
  const year = date.getUTCFullYear();
  if (year < 1 || year > 9999) return null;
  return { year, month: date.getUTCMonth() + 1, day: date.getUTCDate() };
}
export function todayCivilDate(date = new Date()): CivilDate {
  return { year: date.getFullYear(), month: date.getMonth() + 1, day: date.getDate() };
}
export function isoDate(date: CivilDate): string {
  return [String(date.year).padStart(4, '0'), String(date.month).padStart(2, '0'), String(date.day).padStart(2, '0')].join('-');
}
export function daysInMonth(year: number, month: number): number {
  if (month < 1 || month > 12 || year < 1 || year > 9999) throw new RangeError('Invalid calendar month');
  if (month === 2) return year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0) ? 29 : 28;
  return [4, 6, 9, 11].includes(month) ? 30 : 31;
}
export function shiftDays(date: CivilDate, days: number): CivilDate | null {
  const moved = utcDate(date);
  moved.setUTCDate(moved.getUTCDate() + days);
  return civilDate(moved);
}
export function shiftMonth(date: CivilDate, months: number): CivilDate | null {
  const index = (date.year - 1) * 12 + date.month - 1 + months;
  if (!Number.isInteger(months) || index < 0 || index >= 9999 * 12) return null;
  const year = Math.floor(index / 12) + 1;
  const month = index % 12 + 1;
  return { year, month, day: Math.min(date.day, daysInMonth(year, month)) };
}
/** Monday-first, always six rows, with dates in adjacent months. */
export function monthGrid(year: number, month: number): (CivilDate | null)[][] {
  daysInMonth(year, month);
  const first = utcDate({ year, month, day: 1 });
  const start = -((first.getUTCDay() + 6) % 7);
  return Array.from({ length: 6 }, (_, week) =>
    Array.from({ length: 7 }, (_, day) => shiftDays({ year, month, day: 1 }, start + week * 7 + day)));
}
export function dayOfYear(date: CivilDate): number {
  return Math.round((utcDate(date).getTime() - utcDate({ year: date.year, month: 1, day: 1 }).getTime()) / DAY_MS) + 1;
}
export function isoWeek(date: CivilDate): { year: number; week: number } {
  const weekday = (utcDate(date).getUTCDay() + 6) % 7;
  const thursday = shiftDays(date, 3 - weekday);
  const isoYear = thursday?.year ?? date.year;
  const jan4 = utcDate({ year: isoYear, month: 1, day: 4 });
  jan4.setUTCDate(jan4.getUTCDate() - (jan4.getUTCDay() + 6) % 7);
  const monday = utcDate(date);
  monday.setUTCDate(monday.getUTCDate() - weekday);
  return { year: isoYear, week: Math.round((monday.getTime() - jan4.getTime()) / (7 * DAY_MS)) + 1 };
}
/** Date labels are localized; arithmetic stays independent of the user's zone. */
export function localDateLabel(date: CivilDate, locale?: string, options?: Intl.DateTimeFormatOptions): string {
  const value = new Date(0);
  value.setFullYear(date.year, date.month - 1, date.day);
  value.setHours(12, 0, 0, 0);
  return new Intl.DateTimeFormat(locale, options ?? { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' }).format(value);
}

/** A single localized, actual wall-clock value drives the calendar time and
 * its decorative GSAP digit rolls. Avoid separate clocks for the animation.
 */
export function localClockText(value: Date, locale?: string): string {
  return new Intl.DateTimeFormat(locale, {
    hour: '2-digit', minute: '2-digit', second: '2-digit', hourCycle: 'h23',
  }).format(value);
}

export function clockGlyphs(value: string): { char: string; digit: boolean }[] {
  return Array.from(value, char => ({
    char,
    digit: /\p{Decimal_Number}/u.test(char),
  }));
}
