use std::time::{SystemTime, UNIX_EPOCH};

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

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
