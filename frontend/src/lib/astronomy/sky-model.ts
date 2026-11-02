import type { CivilDate } from '../calendar-model';

/**
 * Lightweight fixed-star horizontal coordinates. The offline catalogue is
 * J2000; the sidereal-time expression is the standard mean GMST polynomial
 * (USNO Astronomical Almanac). Deliberately no geolocation permission, API,
 * network or third-party runtime required. No precession, refraction or
 * proper-motion correction: this is a visual finder chart, not astrometry.
 */
const DEG = Math.PI / 180;
export interface Observer { latitude: number; longitude: number }
export interface SkyPoint { x: number; y: number; altitude: number }

export function observerCoordinates(latitude: string, longitude: string): Observer | null {
  if (!latitude.trim() || !longitude.trim()) return null;
  const lat = Number(latitude);
  const lon = Number(longitude);
  if (!Number.isFinite(lat) || !Number.isFinite(lon) || lat < -90 || lat > 90 || lon < -180 || lon > 180) return null;
  return { latitude: lat, longitude: lon };
}

/** User-selected civil date at an explicitly selected local wall-clock time. */
export function localObservationDate(date: CivilDate, clock: string): Date | null {
  const match = /^([01]\d|2[0-3]):([0-5]\d)$/.exec(clock);
  if (!match || date.year < 1900 || date.year > 2100) return null;
  const result = new Date(0);
  result.setFullYear(date.year, date.month - 1, date.day);
  result.setHours(Number(match[1]), Number(match[2]), 0, 0);
  if (!Number.isFinite(result.getTime()) ||
      result.getFullYear() !== date.year || result.getMonth() !== date.month - 1 ||
      result.getDate() !== date.day || result.getHours() !== Number(match[1])) return null;
  return result;
}

/** Approximate Greenwich mean sidereal angle in degrees, normalized 0..360. */
export function siderealDegrees(when: Date, longitudeEastDegrees: number): number {
  const jd = when.getTime() / 86400000 + 2440587.5;
  const d = jd - 2451545.0;
  const t = d / 36525;
  const deg = 280.46061837 + 360.98564736629 * d
    + 0.000387933 * t * t - t * t * t / 38710000 + longitudeEastDegrees;
  return ((deg % 360) + 360) % 360;
}

/**
 * Azimuthal equidistant projection of the visible celestial hemisphere.
 * North = up, east = right; zenith=(0,0), horizon radius=1.
 * Angles are fixed J2000 RA/Dec degrees and observer local sidereal angle.
 */
export function horizonPoint(
  raDegrees: number, declinationDegrees: number,
  latitudeDegrees: number, localSiderealDegrees: number,
): SkyPoint {
  const hourAngle = (localSiderealDegrees - raDegrees) * DEG;
  const dec = declinationDegrees * DEG;
  const lat = latitudeDegrees * DEG;
  const east = -Math.cos(dec) * Math.sin(hourAngle);
  const north = Math.sin(dec) * Math.cos(lat) - Math.cos(dec) * Math.sin(lat) * Math.cos(hourAngle);
  const sinAltitude = Math.sin(lat) * Math.sin(dec) + Math.cos(lat) * Math.cos(dec) * Math.cos(hourAngle);
  const altitude = Math.asin(Math.max(-1, Math.min(1, sinAltitude))) / DEG;
  const az = Math.atan2(east, north);
  const radius = (90 - altitude) / 90;
  return { x: radius * Math.sin(az), y: -radius * Math.cos(az), altitude };
}

/** Smallest circular movement, so transitions don't spin 360 degrees. */
export function shortestSiderealDelta(from: number, to: number): number {
  return ((to - from + 540) % 360 + 360) % 360 - 180;
}
