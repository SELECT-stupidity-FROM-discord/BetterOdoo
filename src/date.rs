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
    }
}
