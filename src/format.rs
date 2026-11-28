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
    if s.contains("cancel")
        || s.contains("block")
        || s.contains("reject")
        || s.contains("fail")
    {
        STAGE_RED
    } else if s.contains("done")
        || s.contains("complet")
        || s.contains("clos")
        || s.contains("resolv")
        || s.contains("approv")
    {
        STAGE_GREEN
    } else if s.contains("prod") || s.contains("deploy") || s.contains("live") || s.contains("releas")
    {
        STAGE_CYAN
    } else if s.contains("uat")
        || s.contains("test")
        || s.contains("review")
        || s.contains("qa")
        || s.contains("verif")
    {
        STAGE_PURPLE
    } else if s.contains("progress") || s.contains("doing") || s.contains("active") {
        STAGE_BLUE
    } else if s.contains("dev") || s.contains("build") || s.contains("implement") {
        STAGE_AMBER
    } else if s.contains("intern") || s.contains("stag") || s.contains("design") {
        STAGE_PINK
    } else if s.contains("wait") || s.contains("hold") || s.contains("pend") || s.contains("pause")
    {
        STAGE_ORANGE
    } else if s.contains("new")
        || s.contains("backlog")
        || s.contains("inbox")
        || s.contains("draft")
        || s.contains("todo")
        || s.contains("to do")
        || s == "—"
    {
        STAGE_GRAY
    } else {
        const FALLBACK: [Tone; 6] = [
            STAGE_BLUE,
            STAGE_AMBER,
            STAGE_PURPLE,
            STAGE_CYAN,
            STAGE_PINK,
            STAGE_ORANGE,
        ];
        let hash = s
            .bytes()
            .fold(0usize, |acc, b| acc.wrapping_mul(31).wrapping_add(b as usize));
        FALLBACK[hash % FALLBACK.len()]
    }
}

pub fn stage_dot(stage: &str) -> u32 {
    match stage_tone(stage).fg {
        0x5f5e5b => 0x9b9a97, // warm stone
        0x2d688a => 0x3d84b0, // slate blue
        0x8f5b1e => 0xc28334, // warm ochre
        0x6f4d8e => 0x8b62b0, // muted plum
        0x2f6f6a => 0x3f8f88, // eucalyptus teal
        0x3a6f4f => 0x4a8f65, // sage green
        0x9e3f3d => 0xc45452, // muted crimson
        0x8f456b => 0xb8618c, // dusty rose
        0x9a521b => 0xc97232, // terracotta
        fg => fg,
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

    #[test]
    fn stage_tones_are_distinct() {
        let stages = [
            "Development",
            "Production",
            "Completed",
            "UAT/Testing",
            "New",
            "In Progress",
            "Cancelled",
            "Internal",
        ];
        let colors: std::collections::HashSet<u32> =
            stages.iter().map(|s| stage_tone(s).fg).collect();
        assert_eq!(colors.len(), stages.len());
    }
}
