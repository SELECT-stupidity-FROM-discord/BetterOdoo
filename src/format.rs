use crate::theme::*;

pub fn hours(h: f64) -> String {
    if h <= 0. {
        return "—".into();
    }
    let total = (h * 60.).round() as i64;
    match (total / 60, total % 60) {
        (h, 0) => format!("{h}h"),
        (0, m) => format!("{m}m"),
        (h, m) => format!("{h}h {m}m"),
    }
}

pub fn stage_tone(stage: &str) -> Tone {
    let s = stage.to_lowercase();
    if s.contains("progress") {
        BLUE
    } else if s.contains("review") || s.contains("test") {
        PURPLE
    } else if s.contains("done") || s.contains("complet") {
        GREEN
    } else {
        GRAY
    }
}

pub fn short_name(name: &str) -> &str {
    name.split('@').next().unwrap_or(name)
}

pub fn person_tone(people: &[String], name: &str) -> Tone {
    PEOPLE[people.iter().position(|p| p == name).unwrap_or(0) % PEOPLE.len()]
}

pub fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

pub fn parse_hours(text: &str) -> Option<f64> {
    let t = text.trim().to_lowercase();
    if t.is_empty() {
        return Some(0.);
    }
    let value = if let Some((h, m)) = t.split_once(':') {
        h.trim().parse::<f64>().ok()? + m.trim().parse::<f64>().ok()? / 60.
    } else if t.contains('h') || t.contains('m') {
        let (h, rest) = t.split_once('h').unwrap_or(("", &t));
        let h = if h.is_empty() {
            0.
        } else {
            h.trim().parse::<f64>().ok()?
        };
        let rest = rest.trim().trim_end_matches('m').trim();
        let m = if rest.is_empty() {
            0.
        } else {
            rest.parse::<f64>().ok()?
        };
        h + m / 60.
    } else {
        t.parse::<f64>().ok()?
    };
    (0. ..=24.).contains(&value).then_some(value)
}

pub fn hours_text(h: f64) -> String {
    if h <= 0. {
        return String::new();
    }
    let s = format!("{h:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hours_are_read_leniently() {
        assert_eq!(parse_hours("1.5"), Some(1.5));
        assert_eq!(parse_hours("2h"), Some(2.));
        assert_eq!(parse_hours("1h 30m"), Some(1.5));
        assert_eq!(parse_hours("45m"), Some(0.75));
        assert_eq!(parse_hours("1:30"), Some(1.5));
        assert_eq!(parse_hours(""), Some(0.));
        assert_eq!(parse_hours("abc"), None);
        assert_eq!(parse_hours("-1"), None);
        assert_eq!(parse_hours("30"), None);
    }

    #[test]
    fn hours_round_trip_to_text() {
        assert_eq!(hours_text(1.5), "1.5");
        assert_eq!(hours_text(3.), "3");
        assert_eq!(hours_text(0.), "");
    }
}
