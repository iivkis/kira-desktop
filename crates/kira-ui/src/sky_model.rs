//! Pure horizontal sky projection ported from frozen main's sky-model.ts.
//! This finder chart is illustrative, not precision astrometry. J2000 stars
//! are not corrected for precession, proper motion, refraction or twilight.
use crate::calendar_model::CivilDate;

const DEG: f64 = std::f64::consts::PI / 180.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Observer {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyPoint {
    pub x: f64,
    pub y: f64,
    pub altitude: f64,
}

pub fn observer_coordinates(latitude: &str, longitude: &str) -> Option<Observer> {
    let latitude = latitude.trim();
    let longitude = longitude.trim();
    if latitude.is_empty() || longitude.is_empty() {
        return None;
    }
    let lat = latitude.parse::<f64>().ok()?;
    let lon = longitude.parse::<f64>().ok()?;
    if !lat.is_finite() || !lon.is_finite()
        || !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon)
    {
        return None;
    }
    Some(Observer { latitude: lat, longitude: lon })
}

pub fn clock_hour_minute(clock: &str) -> Option<(i32, i32)> {
    let bytes = clock.as_bytes();
    if bytes.len() != 5 || bytes[2] != b':'
        || ![0, 1, 3, 4].iter().all(|&i| bytes[i].is_ascii_digit())
    {
        return None;
    }
    let hour = i32::from(bytes[0] - b'0') * 10 + i32::from(bytes[1] - b'0');
    let minute = i32::from(bytes[3] - b'0') * 10 + i32::from(bytes[4] - b'0');
    (hour < 24 && minute < 60).then_some((hour, minute))
}

/// Date and wall-clock time use the *operating system's local timezone*,
/// not a timezone inferred from user-entered coordinates. Explicitly reject
/// nonexistent times (DST jumps) and unsupported years.
pub fn local_observation_unix(date: CivilDate, clock: &str) -> Option<i64> {
    if !(1900..=2100).contains(&date.year) {
        return None;
    }
    let (hour, minute) = clock_hour_minute(clock)?;
    let instant = gtk::glib::DateTime::from_local(
        date.year, i32::from(date.month), i32::from(date.day), hour, minute, 0.0,
    ).ok()?;
    if instant.year() != date.year
        || instant.month() != i32::from(date.month)
        || instant.day_of_month() != i32::from(date.day)
        || instant.hour() != hour || instant.minute() != minute
    {
        return None;
    }
    Some(instant.to_unix())
}

/// Greenwich mean sidereal angle at unix seconds, plus east-positive
/// longitude, normalized to [0, 360). Astronomical Almanac approximation.
pub fn sidereal_degrees(unix_seconds: i64, longitude_east: f64) -> f64 {
    let jd = unix_seconds as f64 / 86400.0 + 2440587.5;
    let d = jd - 2451545.0;
    let t = d / 36525.0;
    (280.46061837 + 360.98564736629 * d
        + 0.000387933 * t * t - t * t * t / 38710000.0
        + longitude_east).rem_euclid(360.0)
}

/// Zenith is (0,0); visible horizon is radius one, north up, east right.
pub fn horizon_point(
    ra: f64, dec_degrees: f64, latitude_degrees: f64, local_sidereal_degrees: f64,
) -> SkyPoint {
    let hour_angle = (local_sidereal_degrees - ra) * DEG;
    let dec = dec_degrees * DEG;
    let lat = latitude_degrees * DEG;
    let east = -dec.cos() * hour_angle.sin();
    let north = dec.sin() * lat.cos() - dec.cos() * lat.sin() * hour_angle.cos();
    let sin_altitude = lat.sin() * dec.sin() + lat.cos() * dec.cos() * hour_angle.cos();
    let altitude = sin_altitude.clamp(-1.0, 1.0).asin() / DEG;
    let az = east.atan2(north);
    let radius = (90.0 - altitude) / 90.0;
    SkyPoint { x: radius * az.sin(), y: -radius * az.cos(), altitude }
}

/// Retarget interrupted sidereal animation on the shortest circular route.
pub fn shortest_sidereal_delta(from: f64, to: f64) -> f64 {
    (to - from + 540.0).rem_euclid(360.0) - 180.0
}

#[cfg(test)]
mod tests {
    use super::*;
    fn near(a: f64, b: f64) { assert!((a-b).abs() < 1e-7, "{a} != {b}"); }

    #[test]
    fn coordinates_and_local_time_fail_closed() {
        assert_eq!(observer_coordinates("51.4779","0"), Some(Observer {
            latitude: 51.4779, longitude: 0.0
        }));
        for (a, b) in [("", "0"), ("90.1", "0"), ("51", "-180.1"),
            ("NaN", "0"), ("inf", "1"), ("foo", "0")] {
            assert!(observer_coordinates(a, b).is_none());
        }
        assert_eq!(clock_hour_minute("22:30"), Some((22,30)));
        for time in ["27:00", "9:00", "12:60", "aa:bb", ""] {
            assert!(clock_hour_minute(time).is_none());
        }
        assert!(local_observation_unix(CivilDate::new(2500,1,1).unwrap(),"22:00").is_none());
        assert!(local_observation_unix(CivilDate::new(2026,11,3).unwrap(),"27:00").is_none());
        assert!(local_observation_unix(CivilDate::new(2026,11,3).unwrap(),"22:30").is_some());
    }

    #[test]
    fn source_sidereal_and_cardinal_projection_match_j2000() {
        near(sidereal_degrees(946728000, 0.0), 280.46061837);
        let transit = horizon_point(45.0, 0.0, 0.0, 45.0);
        near(transit.altitude, 90.0);
        near(transit.x.hypot(transit.y), 0.0);
        near(horizon_point(45.0, 0.0, 0.0, 135.0).altitude, 0.0);
        near(horizon_point(90.0, 0.0, 0.0, 0.0).x, 1.0);
        near(horizon_point(270.0, 0.0, 0.0, 0.0).x, -1.0);
        near(horizon_point(0.0, -30.0, 50.0, 0.0).altitude, 10.0);
    }

    #[test]
    fn shortest_rotation_crosses_zero_without_long_spin() {
        near(shortest_sidereal_delta(359.0,1.0), 2.0);
        near(shortest_sidereal_delta(1.0,359.0), -2.0);
    }
}
