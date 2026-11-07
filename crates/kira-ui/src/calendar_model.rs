//! Strict Gregorian/ISO-week model ported from main's calendar-model.ts.
//! No locale-dependent timestamp math, timezone truncation or GTK state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CivilDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DayState {
    pub date: CivilDate,
    pub today: bool,
    pub selected: bool,
    pub outside: bool,
}

impl CivilDate {
    pub fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        if !(1..=9999).contains(&year)
            || !(1..=12).contains(&month)
            || day == 0
            || day > days_in_month(year, month)
        {
            return None;
        }
        Some(Self { year, month, day })
    }

    pub fn iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    pub fn shift_month(self, months: i32) -> Option<Self> {
        let offset = (self.year - 1) * 12 + i32::from(self.month) - 1;
        let next = offset.checked_add(months)?;
        if !(0..9999 * 12).contains(&next) {
            return None;
        }
        let year = next / 12 + 1;
        let month = u8::try_from(next % 12 + 1).ok()?;
        Self::new(year, month, self.day.min(days_in_month(year, month)))
    }

    pub fn shift_days(self, days: i32) -> Option<Self> {
        civil_from_days(days_from_civil(self) + i64::from(days))
    }

    /// ISO weekday: Monday 0, Sunday 6.
    pub fn weekday(self) -> u8 {
        (days_from_civil(self) + 3).rem_euclid(7) as u8
    }

    pub fn iso_week(self) -> (i32, u8) {
        let weekday = i64::from(self.weekday());
        let thursday = days_from_civil(self) + 3 - weekday;
        let week_year = civil_from_days(thursday).map_or(self.year, |date| date.year);
        let jan4 = Self::new(week_year, 1, 4).expect("valid ISO week year");
        let jan4_days = days_from_civil(jan4);
        let week_one_monday = jan4_days - i64::from(jan4.weekday());
        let week = (days_from_civil(self) - weekday - week_one_monday) / 7 + 1;
        (week_year, week as u8)
    }

    pub fn day_of_year(self) -> u16 {
        (days_from_civil(self)
            - days_from_civil(Self {
                year: self.year,
                month: 1,
                day: 1,
            })
            + 1) as u16
    }

    pub fn state(
        self,
        selected: Self,
        today: Self,
        visible_year: i32,
        visible_month: u8,
    ) -> DayState {
        DayState {
            date: self,
            today: self == today,
            selected: self == selected,
            outside: self.year != visible_year || self.month != visible_month,
        }
    }

    pub fn short_label(self) -> String {
        format!(
            "{} {}, {}",
            WEEKDAYS_LONG[self.weekday() as usize],
            self.day,
            MONTH_GENITIVE[usize::from(self.month - 1)]
        )
    }

    pub fn full_label(self) -> String {
        format!(
            "{} {} {} {} г.",
            WEEKDAYS_LONG[self.weekday() as usize],
            self.day,
            MONTH_GENITIVE[usize::from(self.month - 1)],
            self.year
        )
    }
}

pub const WEEKDAYS: [&str; 7] = ["ПН", "ВТ", "СР", "ЧТ", "ПТ", "СБ", "ВС"];
pub const WEEKDAYS_LONG: [&str; 7] = [
    "понедельник",
    "вторник",
    "среда",
    "четверг",
    "пятница",
    "суббота",
    "воскресенье",
];
pub const MONTHS: [&str; 12] = [
    "январь",
    "февраль",
    "март",
    "апрель",
    "май",
    "июнь",
    "июль",
    "август",
    "сентябрь",
    "октябрь",
    "ноябрь",
    "декабрь",
];
pub const MONTH_GENITIVE: [&str; 12] = [
    "января",
    "февраля",
    "марта",
    "апреля",
    "мая",
    "июня",
    "июля",
    "августа",
    "сентября",
    "октября",
    "ноября",
    "декабря",
];

pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

// Hinnant Gregorian serial-day conversion; 1970-01-01 is serial 0.
fn days_from_civil(date: CivilDate) -> i64 {
    let y = i64::from(date.year) - i64::from(date.month <= 2);
    let m = i64::from(date.month);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m_adj = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * m_adj + 2) / 5 + i64::from(date.day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(serial: i64) -> Option<CivilDate> {
    let z = serial + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    y += i64::from(m <= 2);
    CivilDate::new(
        i32::try_from(y).ok()?,
        u8::try_from(m).ok()?,
        u8::try_from(d).ok()?,
    )
}

/// Exactly six Monday-first rows with real adjacent-month dates.
pub fn month_grid(year: i32, month: u8) -> Option<[[Option<CivilDate>; 7]; 6]> {
    let first = CivilDate::new(year, month, 1)?;
    let start = i32::from(first.weekday());
    Some(std::array::from_fn(|week| {
        std::array::from_fn(|day| first.shift_days(week as i32 * 7 + day as i32 - start))
    }))
}

pub fn current_local_date() -> Option<CivilDate> {
    let now = gtk::glib::DateTime::now_local().ok()?;
    CivilDate::new(
        now.year(),
        u8::try_from(now.month()).ok()?,
        u8::try_from(now.day_of_month()).ok()?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn date(y: i32, m: u8, d: u8) -> CivilDate {
        CivilDate::new(y, m, d).unwrap()
    }

    #[test]
    fn main_independent_today_selected_and_adjacent_states() {
        let today = date(2026, 11, 2);
        let selected = date(2026, 11, 4);
        assert_eq!(today.state(selected, today, 2026, 11).today, true);
        assert!(!today.state(selected, today, 2026, 11).selected);
        assert!(date(2026, 10, 31).state(selected, today, 2026, 11).outside);
        assert!(today.state(today, today, 2026, 11).selected);
    }

    #[test]
    fn main_century_leap_rules() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(1900, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(days_in_month(2025, 2), 28);
    }

    #[test]
    fn main_six_monday_first_rows() {
        let grid = month_grid(2026, 11).unwrap();
        assert_eq!(grid[0][0].unwrap().iso(), "2026-10-26");
        assert_eq!(grid[0][6].unwrap().iso(), "2026-11-01");
        assert_eq!(grid[5][6].unwrap().iso(), "2026-12-06");
        assert_eq!(grid[0][0].unwrap().iso_week(), (2026, 44));
    }

    #[test]
    fn main_month_clamping_and_bounds() {
        assert_eq!(date(2024, 1, 31).shift_month(1), Some(date(2024, 2, 29)));
        assert_eq!(date(2025, 1, 31).shift_month(1), Some(date(2025, 2, 28)));
        assert_eq!(date(2026, 1, 15).shift_month(-1), Some(date(2025, 12, 15)));
        assert_eq!(date(1, 1, 1).shift_month(-1), None);
        assert_eq!(date(9999, 12, 31).shift_month(1), None);
    }

    #[test]
    fn main_iso_year_and_day_of_year() {
        assert_eq!(date(2021, 1, 1).iso_week(), (2020, 53));
        assert_eq!(date(2018, 12, 31).iso_week(), (2019, 1));
        assert_eq!(date(2026, 11, 1).day_of_year(), 305);
        assert_eq!(date(2024, 12, 31).day_of_year(), 366);
    }

    #[test]
    fn main_avoids_1900_offset_and_overflow() {
        assert_eq!(date(1, 1, 1).shift_days(1), Some(date(1, 1, 2)));
        assert_eq!(date(1, 1, 1).shift_days(-1), None);
        assert_eq!(date(9999, 12, 31).shift_days(1), None);
    }

    #[test]
    fn invalid_date_and_iso_format_are_safe() {
        assert_eq!(CivilDate::new(1900, 2, 29), None);
        assert_eq!(date(1, 1, 1).iso(), "0001-01-01");
        assert_eq!(date(2026, 11, 7).full_label(), "суббота 7 ноября 2026 г.");
    }
}
