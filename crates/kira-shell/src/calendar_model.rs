//! Pure Gregorian date arithmetic for Kira's own date picker.
//! No GTK state or time-zone conversion is used in the calendar grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CivilDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl CivilDate {
    pub fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        if !(1..=9999).contains(&year) || !(1..=12).contains(&month) {
            return None;
        }
        if day == 0 || day > days_in_month(year, month) {
            return None;
        }
        Some(Self { year, month, day })
    }

    pub fn days(self) -> i64 {
        let year = self.year - i32::from(self.month <= 2);
        let era = year.div_euclid(400);
        let yoe = year - era * 400;
        let month = i32::from(self.month);
        let mp = month + if month > 2 { -3 } else { 9 };
        let doy = (153 * mp + 2) / 5 + i32::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        i64::from(era) * 146097 + i64::from(doe) - 719468
    }

    pub fn from_days(days: i64) -> Option<Self> {
        let z = days + 719468;
        let era = z.div_euclid(146097);
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let mut year = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = mp + if mp < 10 { 3 } else { -9 };
        year += i64::from(month <= 2);
        let year = i32::try_from(year).ok()?;
        Self::new(year, u8::try_from(month).ok()?, u8::try_from(day).ok()?)
    }

    /// Monday = 0, Sunday = 6.
    pub fn weekday(self) -> i64 {
        (self.days() + 3).rem_euclid(7)
    }

    pub fn shift_days(self, offset: i64) -> Option<Self> {
        Self::from_days(self.days() + offset)
    }

    pub fn day_of_year(self) -> u16 {
        (self.days() - Self::new(self.year, 1, 1).expect("valid year").days() + 1) as u16
    }

    pub fn iso_week(self) -> (i32, u8) {
        // The ISO week-year is the Gregorian year of this week's Thursday.
        let thursday = self.days() + 3 - self.weekday();
        let iso_year = Self::from_days(thursday).map_or(self.year, |d| d.year);
        let jan4 = Self::new(iso_year, 1, 4).expect("valid ISO year");
        let week1_monday = jan4.days() - jan4.weekday();
        (
            iso_year,
            u8::try_from((self.days() - self.weekday() - week1_monday) / 7 + 1).unwrap_or(1),
        )
    }

    pub fn iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 31,
    }
}

pub fn shift_month(year: i32, month: u8, months: i32) -> Option<(i32, u8)> {
    let serial = (year - 1) * 12 + i32::from(month) - 1 + months;
    if !(0..9999 * 12).contains(&serial) {
        return None;
    }
    Some((serial / 12 + 1, (serial % 12 + 1) as u8))
}

/// Always six stable Monday-first rows; partial weeks show adjoining dates.
pub fn month_grid(year: i32, month: u8) -> [[Option<CivilDate>; 7]; 6] {
    let first = CivilDate::new(year, month, 1).expect("valid calendar month");
    let monday = first.days() - first.weekday();
    std::array::from_fn(|row| {
        std::array::from_fn(|column| CivilDate::from_days(monday + (row * 7 + column) as i64))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leap_years_and_century_rules() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(1900, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
        assert!(CivilDate::new(2025, 2, 29).is_none());
    }

    #[test]
    fn dates_round_trip_across_gregorian_boundaries() {
        for date in [
            (1, 1, 1),
            (1900, 3, 1),
            (1970, 1, 1),
            (2000, 2, 29),
            (2026, 11, 1),
            (9999, 12, 31),
        ] {
            let original = CivilDate::new(date.0, date.1, date.2).unwrap();
            assert_eq!(CivilDate::from_days(original.days()), Some(original));
        }
        assert_eq!(CivilDate::new(2026, 11, 1).unwrap().weekday(), 6);
    }

    #[test]
    fn monday_first_month_grid_and_adjacent_days() {
        let grid = month_grid(2026, 11);
        assert_eq!(grid[0][0].unwrap().iso(), "2026-10-26");
        assert_eq!(grid[0][6].unwrap().iso(), "2026-11-01");
        assert_eq!(grid[5][6].unwrap().iso(), "2026-12-06");
        assert_eq!(grid[0][0].unwrap().iso_week(), (2026, 44));
    }

    #[test]
    fn iso_week_year_crossing_and_day_of_year() {
        assert_eq!(CivilDate::new(2021, 1, 1).unwrap().iso_week(), (2020, 53));
        assert_eq!(CivilDate::new(2018, 12, 31).unwrap().iso_week(), (2019, 1));
        assert_eq!(CivilDate::new(2026, 11, 1).unwrap().day_of_year(), 305);
        assert_eq!(CivilDate::new(2024, 12, 31).unwrap().day_of_year(), 366);
    }

    #[test]
    fn month_navigation_handles_years_and_limits() {
        assert_eq!(shift_month(2026, 1, -1), Some((2025, 12)));
        assert_eq!(shift_month(2026, 12, 1), Some((2027, 1)));
        assert_eq!(shift_month(2026, 11, -12), Some((2025, 11)));
        assert_eq!(shift_month(1, 1, -1), None);
        assert_eq!(shift_month(9999, 12, 1), None);
    }
}
