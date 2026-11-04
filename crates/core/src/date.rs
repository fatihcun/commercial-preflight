use std::fmt;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::CommercialPreflightError;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Date {
    year: u16,
    month: u8,
    day: u8,
}

impl Date {
    pub fn new(year: u16, month: u8, day: u8) -> Result<Self, CommercialPreflightError> {
        if year == 0 || year > 9999 || !(1..=12).contains(&month) {
            return Err(CommercialPreflightError::InvalidDate(format!(
                "{year:04}-{month:02}-{day:02}"
            )));
        }
        let max_day = days_in_month(year, month);
        if day == 0 || day > max_day {
            return Err(CommercialPreflightError::InvalidDate(format!(
                "{year:04}-{month:02}-{day:02}"
            )));
        }
        Ok(Self { year, month, day })
    }

    pub fn year(self) -> u16 {
        self.year
    }

    pub fn month(self) -> u8 {
        self.month
    }

    pub fn day(self) -> u8 {
        self.day
    }

    pub fn today_utc() -> Result<Self, CommercialPreflightError> {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| CommercialPreflightError::Clock)?
            .as_secs();
        let days = i64::try_from(seconds / 86_400).map_err(|_| CommercialPreflightError::Clock)?;
        civil_from_days(days)
    }

    pub fn parse_supported(value: &str) -> Result<Self, CommercialPreflightError> {
        let value = value.trim();
        if value.len() >= 10 {
            let prefix = &value[..10];
            if prefix.as_bytes().get(4) == Some(&b'-') && prefix.as_bytes().get(7) == Some(&b'-') {
                return parse_parts(prefix, '-');
            }
        }
        if value.len() == 10 && value.as_bytes().get(2) == Some(&b'.') {
            return parse_day_first(value, '.');
        }
        if value.len() == 10 && value.as_bytes().get(2) == Some(&b'/') {
            return parse_day_first(value, '/');
        }
        Err(CommercialPreflightError::InvalidDate(value.to_owned()))
    }

    pub fn days_until(self, other: Self) -> i64 {
        ordinal(other) - ordinal(self)
    }
}

fn ordinal(date: Date) -> i64 {
    let year = i64::from(date.year) - 1;
    let mut days = year * 365 + year / 4 - year / 100 + year / 400;
    for month in 1..date.month {
        days += i64::from(days_in_month(date.year, month));
    }
    days + i64::from(date.day)
}

impl fmt::Display for Date {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:04}-{:02}-{:02}",
            self.year, self.month, self.day
        )
    }
}

impl FromStr for Date {
    type Err = CommercialPreflightError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse_supported(value)
    }
}

fn parse_parts(value: &str, separator: char) -> Result<Date, CommercialPreflightError> {
    let mut parts = value.split(separator);
    let year = parse_number(parts.next(), value)?;
    let month = parse_number(parts.next(), value)?;
    let day = parse_number(parts.next(), value)?;
    if parts.next().is_some() {
        return Err(CommercialPreflightError::InvalidDate(value.to_owned()));
    }
    Date::new(year, month, day)
}

fn parse_day_first(value: &str, separator: char) -> Result<Date, CommercialPreflightError> {
    let mut parts = value.split(separator);
    let day = parse_number(parts.next(), value)?;
    let month = parse_number(parts.next(), value)?;
    let year = parse_number(parts.next(), value)?;
    if parts.next().is_some() {
        return Err(CommercialPreflightError::InvalidDate(value.to_owned()));
    }
    Date::new(year, month, day)
}

fn parse_number<T: FromStr>(
    part: Option<&str>,
    original: &str,
) -> Result<T, CommercialPreflightError> {
    part.and_then(|part| part.parse().ok())
        .ok_or_else(|| CommercialPreflightError::InvalidDate(original.to_owned()))
}

fn is_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

// Converts days since 1970-01-01 to a Gregorian date.
fn civil_from_days(days_since_epoch: i64) -> Result<Date, CommercialPreflightError> {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    Date::new(
        u16::try_from(year).map_err(|_| CommercialPreflightError::Clock)?,
        u8::try_from(month).map_err(|_| CommercialPreflightError::Clock)?,
        u8::try_from(day).map_err(|_| CommercialPreflightError::Clock)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_formats() {
        let expected = Date::new(2026, 10, 30).expect("valid date");
        assert_eq!(Date::parse_supported("2026-10-30"), Ok(expected));
        assert_eq!(Date::parse_supported("30.10.2026"), Ok(expected));
        assert_eq!(Date::parse_supported("30/10/2026"), Ok(expected));
        assert_eq!(Date::parse_supported("2026-10-30T12:00:00"), Ok(expected));
    }

    #[test]
    fn rejects_invalid_dates() {
        assert!(Date::parse_supported("2026-02-29").is_err());
        assert!(Date::parse_supported("not-a-date").is_err());
    }

    #[test]
    fn unix_epoch_conversion_is_correct() {
        assert_eq!(
            civil_from_days(0).expect("epoch is valid"),
            Date::new(1970, 1, 1).expect("valid date")
        );
        assert_eq!(
            civil_from_days(20_756).expect("date is valid"),
            Date::new(2026, 10, 30).expect("valid date")
        );
    }

    #[test]
    fn calculates_validity_durations_across_months_and_leap_days() {
        let start = Date::new(2024, 2, 28).expect("valid date");
        let end = Date::new(2024, 3, 1).expect("valid date");
        assert_eq!(start.days_until(end), 2);
        assert_eq!(end.days_until(start), -2);
    }
}
