use std::time::{SystemTime, UNIX_EPOCH};

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const FULL_MONTHS: [&str; 12] = [
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartialDate {
    pub year: i64,
    pub month: i64,
    pub day: Option<i64>,
}

pub struct Due {
    pub label: String,
    pub overdue: bool,
}

// ponytail: UTC day boundary; use the user's Odoo tz once settings exist
pub fn today() -> i64 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    secs as i64 / 86_400
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn iso(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn weekday(days: i64) -> &'static str {
    WEEKDAYS[(days + 4).rem_euclid(7) as usize]
}

pub fn is_valid(s: &str) -> bool {
    parse(s).is_some_and(|(y, m, d)| iso(days_from_civil(y, m, d)) == s)
}

pub fn days_from_iso(s: &str) -> Option<i64> {
    let (y, m, d) = parse(s)?;
    Some(days_from_civil(y, m, d))
}

pub fn week_start(days: i64) -> i64 {
    days - (days + 3).rem_euclid(7)
}

pub fn month_start(days: i64) -> i64 {
    let (y, m, _) = parts(days);
    days_from_civil(y, m, 1)
}

pub fn parts(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

pub fn from_ymd(year: i64, month: i64, day: i64) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let days = days_from_civil(year, month, day);
    (parts(days) == (year, month, day)).then_some(days)
}

pub fn month_name(month: i64) -> &'static str {
    FULL_MONTHS
        .get((month - 1).clamp(0, 11) as usize)
        .copied()
        .unwrap_or("")
}

pub fn add_months(year: i64, month: i64, delta: i64) -> (i64, i64) {
    let total = year * 12 + (month - 1) + delta;
    (total.div_euclid(12), total.rem_euclid(12) + 1)
}

pub fn calendar_cells(year: i64, month: i64) -> [i64; 42] {
    let first = days_from_civil(year, month, 1);
    let offset = (first + 3).rem_euclid(7); // Monday = 0 .. Sunday = 6
    let start = first - offset;
    std::array::from_fn(|i| start + i as i64)
}

fn parse_month_word(word: &str) -> Option<i64> {
    if word.len() < 3 {
        return None;
    }
    let w = word.to_lowercase();
    FULL_MONTHS
        .iter()
        .position(|m| m.to_lowercase().starts_with(&w))
        .map(|i| i as i64 + 1)
}

pub fn parse_partial(input: &str, today: i64, view_year: i64, view_month: i64) -> Option<PartialDate> {
    let raw = input.trim();
    if raw.is_empty() {
        return None;
    }
    let lower = raw.to_lowercase();
    match lower.as_str() {
        "today" | "tod" => {
            let (year, month, _) = parts(today);
            return Some(PartialDate {
                year,
                month,
                day: Some(today),
            });
        }
        "tomorrow" | "tom" => {
            let d = today + 1;
            let (year, month, _) = parts(d);
            return Some(PartialDate {
                year,
                month,
                day: Some(d),
            });
        }
        "yesterday" | "yes" => {
            let d = today - 1;
            let (year, month, _) = parts(d);
            return Some(PartialDate {
                year,
                month,
                day: Some(d),
            });
        }
        _ => {}
    }

    // Numeric ISO-like: YYYY, YYYY-MM, YYYY-MM-DD (also accepts '/' or '.')
    let num_parts: Vec<&str> = raw
        .split(['-', '/', '.'])
        .filter(|s| !s.is_empty())
        .collect();
    if !num_parts.is_empty() && num_parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
        let first = num_parts[0];
        if first.len() == 4 {
            let year = first.parse::<i64>().ok()?;
            if !(1900..=2200).contains(&year) {
                return None;
            }
            if num_parts.len() == 1 {
                return Some(PartialDate {
                    year,
                    month: view_month,
                    day: None,
                });
            }
            let month = num_parts[1].parse::<i64>().ok()?;
            if !(1..=12).contains(&month) {
                return None;
            }
            if num_parts.len() == 2 {
                return Some(PartialDate {
                    year,
                    month,
                    day: None,
                });
            }
            let d_num = num_parts[2].parse::<i64>().ok()?;
            return Some(PartialDate {
                year,
                month,
                day: from_ymd(year, month, d_num),
            });
        }
    }

    // Natural month names: "dec", "dec 15", "15 dec", "dec 15 2027", "15 dec 2027", "2027 dec"
    let tokens: Vec<&str> = raw
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | '-' | '/'))
        .filter(|s| !s.is_empty())
        .collect();
    let mut month = None;
    let mut year = None;
    let mut day_num = None;
    for tok in tokens {
        if let Some(m) = parse_month_word(tok)
            && month.is_none()
        {
            month = Some(m);
        } else if let Ok(n) = tok.parse::<i64>() {
            if tok.len() == 4 && (1900..=2200).contains(&n) && year.is_none() {
                year = Some(n);
            } else if (1..=31).contains(&n) && day_num.is_none() {
                day_num = Some(n);
            } else {
                return None;
            }
        } else {
            return None;
        }
    }
    let m = month?;
    let y = year.unwrap_or(view_year);
    let day = day_num.and_then(|d| from_ymd(y, m, d));
    Some(PartialDate {
        year: y,
        month: m,
        day,
    })
}

fn parse(s: &str) -> Option<(i64, i64, i64)> {
    let mut it = s.get(..10)?.split('-').map(|p| p.parse::<i64>().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

pub fn due(deadline: Option<&str>, today: i64) -> Due {
    let Some((y, m, d)) = deadline.and_then(parse) else {
        return Due {
            label: "—".into(),
            overdue: false,
        };
    };
    let days = days_from_civil(y, m, d);
    let label = match days - today {
        0 => "Today".into(),
        -1 => "Yesterday".into(),
        1 => "Tomorrow".into(),
        _ => format!(
            "{} {} {}",
            WEEKDAYS[(days + 4).rem_euclid(7) as usize],
            d,
            MONTHS[(m - 1) as usize]
        ),
    };
    Due {
        label,
        overdue: days < today,
    }
}

fn utc_offset_minutes() -> i64 {
    static OFFSET: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *OFFSET.get_or_init(|| {
        // ponytail: shells out to `date`; Windows falls back to UTC
        let out = std::process::Command::new("date").arg("+%z").output().ok();
        let z = out.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        z.filter(|z| z.len() == 5)
            .and_then(|z| {
                let sign = if z.starts_with('-') { -1 } else { 1 };
                Some(
                    sign * (z.get(1..3)?.parse::<i64>().ok()? * 60
                        + z.get(3..5)?.parse::<i64>().ok()?),
                )
            })
            .unwrap_or(0)
    })
}

// Odoo UTC "YYYY-MM-DD HH:MM:SS" to (local day label, local time label).
pub fn stamp(utc: &str) -> (String, String) {
    let Some((y, m, d)) = parse(utc) else {
        return (String::new(), String::new());
    };
    let clock = |i: usize| {
        utc.get(i..i + 2)
            .and_then(|p| p.parse::<i64>().ok())
            .unwrap_or(0)
    };
    let total = clock(11) * 60 + clock(14) + utc_offset_minutes();
    let days = days_from_civil(y, m, d) + total.div_euclid(1440);
    let minutes = total.rem_euclid(1440);
    let (y, m, d) = parse(&iso(days)).unwrap_or((y, m, d));
    let hour = minutes / 60;
    let time = format!(
        "{}:{:02} {}",
        if hour % 12 == 0 { 12 } else { hour % 12 },
        minutes % 60,
        if hour < 12 { "am" } else { "pm" }
    );
    (format!("{d} {} {y}", MONTHS[(m - 1) as usize]), time)
}

pub fn days_since(stamp: &str, today: i64) -> Option<i64> {
    let (y, m, d) = parse(stamp)?;
    Some((today - days_from_civil(y, m, d)).max(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        let today = days_from_civil(2026, 11, 28);
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(due(Some("2026-11-29"), today).label, "Tomorrow");
        assert_eq!(due(Some("2026-12-04 00:00:00"), today).label, "Fri 4 Dec");
        let late = due(Some("2026-11-27"), today);
        assert!(late.overdue && late.label == "Yesterday");
        assert_eq!(due(None, today).label, "—");
        assert_eq!(due(Some("garbage"), today).label, "—");
        assert_eq!(iso(today), "2026-11-28");
        assert_eq!(iso(days_from_civil(2024, 2, 29)), "2024-02-29");
        assert_eq!(weekday(today), "Sat");
        assert!(is_valid("2026-11-28") && !is_valid("2026-02-30") && !is_valid("tomorrow"));
        assert_eq!(days_from_iso("2026-11-28"), Some(today));
        assert_eq!(iso(week_start(today)), "2026-11-23");
        assert_eq!(iso(week_start(days_from_civil(2026, 11, 23))), "2026-11-23");
        assert_eq!(iso(week_start(days_from_civil(2026, 11, 29))), "2026-11-23");
        assert_eq!(iso(month_start(today)), "2026-11-01");
        assert_eq!(add_months(2026, 12, 1), (2027, 1));
        assert_eq!(add_months(2026, 1, -1), (2025, 12));
        assert_eq!(
            parse_partial("2027", today, 2026, 11),
            Some(PartialDate {
                year: 2027,
                month: 11,
                day: None
            })
        );
        assert_eq!(
            parse_partial("2027-03", today, 2026, 11),
            Some(PartialDate {
                year: 2027,
                month: 3,
                day: None
            })
        );
        assert_eq!(
            parse_partial("2027-03-15", today, 2026, 11),
            Some(PartialDate {
                year: 2027,
                month: 3,
                day: from_ymd(2027, 3, 15)
            })
        );
        assert_eq!(
            parse_partial("dec 25", today, 2026, 11),
            Some(PartialDate {
                year: 2026,
                month: 12,
                day: from_ymd(2026, 12, 25)
            })
        );
        assert_eq!(
            parse_partial("15 Jan 2027", today, 2026, 11),
            Some(PartialDate {
                year: 2027,
                month: 1,
                day: from_ymd(2027, 1, 15)
            })
        );
        assert_eq!(
            parse_partial("tomorrow", today, 2026, 11),
            Some(PartialDate {
                year: 2026,
                month: 11,
                day: Some(today + 1)
            })
        );
    }
}
