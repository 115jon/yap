//! Timestamps without a date-time crate: RFC 3339 parsing and the local
//! strings the message list shows. The UTC offset comes from the app (the
//! store itself has no clock and no time zone).

use std::fmt;

/// Returns the local UTC offset, in seconds, at a UTC instant (epoch ms).
/// The app passes one backed by the OS time zone, so DST is per instant.
#[derive(Clone, Copy)]
pub struct LocalOffset(pub fn(i64) -> i32);

impl LocalOffset {
    pub const UTC: Self = Self(|_| 0);

    pub fn at(self, utc_ms: i64) -> i32 {
        (self.0)(utc_ms)
    }
}

impl Default for LocalOffset {
    fn default() -> Self {
        Self::UTC
    }
}

impl fmt::Debug for LocalOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LocalOffset")
    }
}

// The offset source is environment, not state: two stores are equal whatever
// clock they were given.
impl PartialEq for LocalOffset {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

const MS_PER_DAY: i64 = 86_400_000;
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Days since 1970-01-01 for a proleptic Gregorian date (Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// (year, month 1-12, day 1-31) for days since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn number(s: &str) -> Option<i64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// `a` is a later instant than `b`. Both parse as RFC 3339 (the server
/// mixes `T` and space separators and offsets); unparseable ones fall back
/// to comparing the strings.
pub fn is_after(a: &str, b: &str) -> bool {
    match (parse_rfc3339(a), parse_rfc3339(b)) {
        (Some(a), Some(b)) => a > b,
        _ => a > b,
    }
}

/// Parses `YYYY-MM-DDTHH:MM:SS[.fff][Z|±HH:MM]` to epoch ms.
pub fn parse_rfc3339(s: &str) -> Option<i64> {
    let (date, rest) = s.split_once(['T', 't', ' '])?;
    let mut parts = date.splitn(3, '-');
    let year = number(parts.next()?)?;
    let month = number(parts.next()?)?;
    let day = number(parts.next()?)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let (clock, offset_s) = match rest.find(['Z', 'z', '+', '-']) {
        Some(at) => {
            let (clock, zone) = rest.split_at(at);
            let offset = match zone {
                "Z" | "z" => 0,
                _ => {
                    let sign = if zone.starts_with('-') { -1 } else { 1 };
                    let (h, m) = zone[1..].split_once(':')?;
                    sign * (number(h)? * 3600 + number(m)? * 60)
                }
            };
            (clock, offset)
        }
        None => (rest, 0),
    };
    let (clock, fraction) = clock.split_once('.').unwrap_or((clock, ""));
    let mut hms = clock.splitn(3, ':');
    let hour = number(hms.next()?)?;
    let minute = number(hms.next()?)?;
    let second = number(hms.next()?)?;
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let ms = match fraction.get(..fraction.len().min(3)) {
        Some("") | None => 0,
        Some(digits) => number(digits)? * 10_i64.pow(3 - digits.len() as u32),
    };
    let days = days_from_civil(year, month, day);
    Some(((days * 86_400 + hour * 3600 + minute * 60 + second - offset_s) * 1000) + ms)
}

/// Epoch ms as `YYYY-MM-DDTHH:MM:SS.fffZ`.
pub fn format_rfc3339(utc_ms: i64) -> String {
    let days = utc_ms.div_euclid(MS_PER_DAY);
    let in_day = utc_ms.rem_euclid(MS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    let (h, m, s, ms) = (
        in_day / 3_600_000,
        in_day / 60_000 % 60,
        in_day / 1000 % 60,
        in_day % 1000,
    );
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}.{ms:03}Z")
}

/// A message time as the list shows it, in local time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stamp {
    /// `h:mm AM`
    pub time: String,
    /// `MMM d`
    pub date: String,
    /// Local days since 1970-01-01, to compare with today.
    pub day: i64,
}

impl Stamp {
    pub fn new(utc_ms: i64, offset: LocalOffset) -> Self {
        let local = utc_ms + i64::from(offset.at(utc_ms)) * 1000;
        let day = local.div_euclid(MS_PER_DAY);
        let minutes = local.rem_euclid(MS_PER_DAY) / 60_000;
        let (hour, minute) = (minutes / 60, minutes % 60);
        let (_, month, date) = civil_from_days(day);
        let hour12 = match hour % 12 {
            0 => 12,
            h => h,
        };
        let meridiem = if hour < 12 { "AM" } else { "PM" };
        Self {
            time: format!("{hour12}:{minute:02} {meridiem}"),
            date: format!("{} {date}", MONTHS[(month - 1) as usize]),
            day,
        }
    }
}

const LONG_MONTHS: [&str; 12] = [
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

/// A local date: `Nov 5, 2026`, or `November 5, 2026` when `long`.
pub fn local_date(utc_ms: i64, offset: LocalOffset, long: bool) -> String {
    let (year, month, day) = civil_from_days(local_day(utc_ms, offset));
    let names = if long { &LONG_MONTHS } else { &MONTHS };
    format!("{} {day}, {year}", names[(month - 1) as usize])
}

/// How long ago, as the member list's tabs say it (MemberList.tsx
/// `formatRelativeTime`): `Just now`, `5m ago`, `3h ago`, `2d ago`, then the
/// local month and day (`Nov 5`). A time in the future reads as just now.
pub fn relative_time(utc_ms: i64, now_ms: i64, offset: LocalOffset) -> String {
    let minutes = (now_ms - utc_ms).div_euclid(60_000);
    let hours = minutes / 60;
    let days = hours / 24;
    if minutes < 1 {
        "Just now".to_owned()
    } else if minutes < 60 {
        format!("{minutes}m ago")
    } else if hours < 24 {
        format!("{hours}h ago")
    } else if days < 7 {
        format!("{days}d ago")
    } else {
        let (_, month, day) = civil_from_days(local_day(utc_ms, offset));
        format!("{} {day}", MONTHS[(month - 1) as usize])
    }
}

/// A local date and time: `Nov 5, 2026, 3:04 PM`.
pub fn local_date_time(utc_ms: i64, offset: LocalOffset) -> String {
    let stamp = Stamp::new(utc_ms, offset);
    format!("{}, {}", local_date(utc_ms, offset, false), stamp.time)
}

/// Today's local day number, for [`Stamp::day`].
pub fn local_day(utc_ms: i64, offset: LocalOffset) -> i64 {
    (utc_ms + i64::from(offset.at(utc_ms)) * 1000).div_euclid(MS_PER_DAY)
}

/// (year, month 1-12, day 1-31) of a local day number ([`local_day`]).
pub fn civil_date(day: i64) -> (i64, i64, i64) {
    civil_from_days(day)
}

/// The local day number of a calendar date (no validation).
pub fn day_number(year: i64, month: i64, day: i64) -> i64 {
    days_from_civil(year, month, day)
}

/// The UTC instant (epoch ms) of a local day's midnight, as
/// `new Date(y, m, d)` gives it.
pub fn local_midnight(day: i64, offset: LocalOffset) -> i64 {
    let local = day * MS_PER_DAY;
    let guess = local - i64::from(offset.at(local)) * 1000;
    local - i64::from(offset.at(guess)) * 1000
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_793_880_000_000; // 2026-11-05T12:00:00Z

    #[test]
    fn relative_time_under_a_minute_should_be_just_now() {
        assert_eq!(
            relative_time(NOW - 59_000, NOW, LocalOffset::UTC),
            "Just now"
        );
    }

    #[test]
    fn relative_time_should_count_minutes_then_hours_then_days() {
        let said: Vec<String> = [5 * 60_000, 3 * 3_600_000, 2 * MS_PER_DAY]
            .iter()
            .map(|ago| relative_time(NOW - ago, NOW, LocalOffset::UTC))
            .collect();
        assert_eq!(said, ["5m ago", "3h ago", "2d ago"]);
    }

    #[test]
    fn relative_time_after_a_week_should_be_the_local_month_and_day() {
        assert_eq!(
            relative_time(NOW - 8 * MS_PER_DAY, NOW, LocalOffset::UTC),
            "Oct 28"
        );
    }

    #[test]
    fn local_midnight_should_shift_by_the_offset() {
        let day = day_number(2026, 10, 2);
        assert_eq!(
            format_rfc3339(local_midnight(day, LocalOffset(|_| -5 * 3600))),
            "2026-10-02T05:00:00.000Z"
        );
    }

    #[test]
    fn parse_should_read_utc_with_milliseconds() {
        assert_eq!(
            parse_rfc3339("2026-11-04T12:00:05.000Z"),
            Some(1_793_793_605_000)
        );
    }

    #[test]
    fn parse_should_apply_an_offset() {
        assert_eq!(
            parse_rfc3339("2026-11-04T13:00:05+01:00"),
            parse_rfc3339("2026-11-04T12:00:05Z")
        );
    }

    #[test]
    fn parse_should_reject_garbage() {
        assert_eq!(parse_rfc3339("yesterday"), None);
    }

    #[test]
    fn format_should_round_trip() {
        let ms = 1_793_793_605_123;
        assert_eq!(parse_rfc3339(&format_rfc3339(ms)), Some(ms));
    }

    #[test]
    fn stamp_should_use_twelve_hour_time_and_short_month() {
        let ms = parse_rfc3339("2026-11-04T00:07:00Z").unwrap_or_default();
        let stamp = Stamp::new(ms, LocalOffset::UTC);
        assert_eq!(
            (stamp.time.as_str(), stamp.date.as_str()),
            ("12:07 AM", "Nov 4")
        );
    }

    #[test]
    fn date_time_should_read_like_the_audit_log() {
        let ms = parse_rfc3339("2026-11-04 23:22:59").unwrap_or_default();
        assert_eq!(
            local_date_time(ms, LocalOffset(|_| -6 * 3600)),
            "Nov 4, 2026, 5:22 PM"
        );
    }

    #[test]
    fn long_date_should_spell_the_month() {
        let ms = parse_rfc3339("2026-11-04T14:27:30.654Z").unwrap_or_default();
        assert_eq!(local_date(ms, LocalOffset::UTC, true), "November 4, 2026");
    }

    #[test]
    fn stamp_should_shift_to_local_time() {
        let ms = parse_rfc3339("2026-11-04T02:30:00Z").unwrap_or_default();
        let stamp = Stamp::new(ms, LocalOffset(|_| -5 * 3600));
        assert_eq!(
            (stamp.time.as_str(), stamp.date.as_str()),
            ("9:30 PM", "Nov 3")
        );
    }
}
