use crate::date;
use crate::odoo::WorkItem;
use crate::settings;
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::env::var;

pub const DEFAULT_MODEL: &str = "qwen2.5:7b";

pub fn model() -> String {
    settings::model()
        .or_else(|| var("AI_MODEL").ok())
        .unwrap_or(DEFAULT_MODEL.into())
}

pub fn base() -> String {
    var("AI_BASE_URL").unwrap_or("http://localhost:11434/v1".into())
}

fn chat(system: &str, user: &str) -> Result<Value> {
    request(system, user).inspect_err(|e| log::warn!("AI request failed: {e}"))
}

fn request(system: &str, user: &str) -> Result<Value> {
    let base = base();
    let model = model();
    let mut req = ureq::post(format!("{base}/chat/completions"));
    if let Ok(k) = var("AI_API_KEY") {
        req = req.header("Authorization", &format!("Bearer {k}"));
    }
    let mut res: Value = req
        .send_json(json!({
            "model": model,
            "response_format": {"type": "json_object"},
            "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
        }))?
        .body_mut()
        .read_json()?;
    let text = res["choices"][0]["message"]["content"].take();
    Ok(serde_json::from_str(
        text.as_str().ok_or(anyhow!("empty AI reply"))?,
    )?)
}

pub fn work_items(task: &str, log: &str, today: i64) -> Result<Vec<WorkItem>> {
    let today_iso = date::iso(today);
    let system = format!(
        r#"You turn a developer's work log into sub-tasks of the Odoo task "{task}".
Today is {weekday} {today_iso}.
Split the log into separate pieces of work. For each piece give:
- name: clean up the user's own wording only enough to read clearly, without over-summarizing or losing intricate details (preserve exact ticket IDs such as TC-ABF-12, specific components, endpoints, error/edge cases, root causes, and technical qualifiers)
- hours: time spent as a number, 0 if not stated
- date: the day it was done as YYYY-MM-DD, resolving words like "yesterday"; default {today_iso}
- done: true unless the log says it is unfinished
Reply only {{"items": [{{"name": "...", "hours": 0, "date": "YYYY-MM-DD", "done": true}}]}}."#,
        weekday = date::weekday(today),
    );
    parse_items(&chat(&system, log)?, log, &today_iso)
}

pub fn revise_items(
    task: &str,
    current: &[WorkItem],
    instruction: &str,
    today: i64,
) -> Result<Vec<WorkItem>> {
    let today_iso = date::iso(today);
    let system = format!(
        r#"You edit a draft list of sub-tasks for the Odoo task "{task}".
Today is {weekday} {today_iso}.
You get the current draft as JSON and an instruction. Apply the instruction and return the full updated list.
Keep every item the instruction does not mention exactly as it is. When editing an item, only simplify phrasing where requested and preserve intricate technical details and exact ticket IDs such as TC-ABF-12.
Item fields: name, hours (number), date (YYYY-MM-DD), done (true/false).
Reply only {{"items": [{{"name": "...", "hours": 0, "date": "YYYY-MM-DD", "done": true}}]}}."#,
        weekday = date::weekday(today),
    );
    let draft: Vec<Value> = current
        .iter()
        .map(|i| json!({"name": i.name, "hours": i.hours, "date": i.date, "done": i.done}))
        .collect();
    let user = format!(
        "Current draft:\n{}\n\nInstruction: {instruction}",
        json!({"items": draft})
    );
    let known = format!(
        "{user}\n{}",
        current
            .iter()
            .map(|i| i.name.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    parse_items(&chat(&system, &user)?, &known, &today_iso)
}

fn parse_items(v: &Value, known: &str, today_iso: &str) -> Result<Vec<WorkItem>> {
    let items: Vec<WorkItem> = v["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|i| {
            let name = i["name"].as_str()?.trim();
            let name = keep_known_ids(name, known);
            (!name.is_empty()).then(|| WorkItem {
                name: name.chars().take(320).collect(),
                hours: i["hours"]
                    .as_f64()
                    .filter(|h| h.is_finite() && (0.0..=24.0).contains(h))
                    .unwrap_or(0.),
                date: i["date"]
                    .as_str()
                    .filter(|d| date::is_valid(d))
                    .map_or(today_iso.to_string(), String::from),
                done: i["done"].as_bool().unwrap_or(true),
            })
        })
        .collect();
    if items.is_empty() {
        return Err(anyhow!("The AI couldn't find any work in that text"));
    }
    Ok(items)
}

fn keep_known_ids(name: &str, log: &str) -> String {
    let looks_like_id = |w: &str| {
        w.contains('-')
            && w.chars().any(|c| c.is_ascii_digit())
            && w.chars().any(|c| c.is_ascii_uppercase())
    };
    name.split_whitespace()
        .filter(|w| {
            let bare = w.trim_matches(|c: char| !c.is_alphanumeric());
            !looks_like_id(bare) || log.contains(bare)
        })
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches([',', ' ', '-', ':'])
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invented_ticket_ids_are_dropped() {
        let log = "TC-ABF-12 fixed the trip text";
        assert_eq!(
            keep_known_ids("Fixed trip text, TC-ABF-12", log),
            "Fixed trip text, TC-ABF-12"
        );
        assert_eq!(
            keep_known_ids("Booking alarm loop, TC-ABF-ABF-13", log),
            "Booking alarm loop"
        );
        assert_eq!(
            keep_known_ids("Wire up iOS-17 push", "wire up push"),
            "Wire up push"
        );
        assert_eq!(
            keep_known_ids("Use e-mail, Not-an-id", log),
            "Use e-mail, Not-an-id"
        );
    }
}
