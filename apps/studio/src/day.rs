//! Calendar days: a set's date, a rehearsal's, today's, and how the pages
//! name them.

/// A calendar day, as days since 1970-01-01.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Day(pub i32);

pub const DAY: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
pub const MON: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
pub const MONTH: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Today, by this device's clock (UTC).
pub fn today() -> Day {
    let secs = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    Day((secs / 86_400) as i32)
}

impl Day {
    /// The day of a civil date (`m` 1–12).
    pub const fn ymd(y: i32, m: u32, d: u32) -> Day {
        let y = if m <= 2 { y - 1 } else { y };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = (y - era * 400) as u32;
        let mp = if m > 2 { m - 3 } else { m + 9 };
        let doy = (153 * mp + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        Day(era * 146097 + doe as i32 - 719468)
    }

    /// Year, month (1–12) and day of the month.
    pub fn civil(self) -> (i32, u32, u32) {
        let z = self.0 + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = (z - era * 146097) as u32;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe as i32 + era * 400 + i32::from(m <= 2);
        (y, m, d)
    }

    /// 0 for Sunday.
    pub fn weekday(self) -> usize {
        (self.0 + 4).rem_euclid(7) as usize
    }
    pub fn month0(self) -> usize {
        self.civil().1 as usize - 1
    }
    pub fn mday(self) -> u32 {
        self.civil().2
    }
    pub fn year(self) -> i32 {
        self.civil().0
    }
    pub fn iso(self) -> String {
        let (y, m, d) = self.civil();
        format!("{y:04}-{m:02}-{d:02}")
    }
    pub fn plus(self, n: i32) -> Day {
        Day(self.0 + n)
    }
    /// Days from today: 0 today, negative before.
    pub fn from_today(self) -> i32 {
        self.0 - today().0
    }
    pub fn dow3(self) -> &'static str {
        &DAY[self.weekday()][..3]
    }
    /// "Thu Nov 19".
    pub fn short(self) -> String {
        format!("{} {} {}", self.dow3(), MON[self.month0()], self.mday())
    }
    /// Today, Tomorrow, Yesterday, a weekday this week, else the short date.
    pub fn when(self) -> String {
        match self.from_today() {
            0 => "Today".into(),
            1 => "Tomorrow".into(),
            -1 => "Yesterday".into(),
            n if n > 0 && n < 7 => DAY[self.weekday()].into(),
            _ => self.short(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_day_round_trips_through_its_civil_date() {
        let d = Day::ymd(2026, 11, 27);
        assert_eq!(d.civil(), (2026, 11, 27));
        assert_eq!(d.iso(), "2026-11-27");
        assert_eq!(DAY[d.weekday()], "Friday");
        assert_eq!(d.plus(1).short(), "Sat Nov 28");
    }

    #[test]
    fn today_is_the_clock_s() {
        // Not a fixed prototype date: a day after Studio was written.
        assert!(today() > Day::ymd(2026, 1, 1));
    }
}
