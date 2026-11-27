use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::env::var;

fn chat(system: &str, user: &str) -> Result<Value> {
    let base = var("AI_BASE_URL").unwrap_or("http://localhost:11434/v1".into());
    let model = var("AI_MODEL").unwrap_or("llama3.2".into());
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

pub fn subtasks(title: &str) -> Result<Vec<String>> {
    let v = chat(
        r#"Break the task into 3-7 concrete sub-tasks. Reply only {"subtasks": ["..."]}."#,
        title,
    )?;
    Ok(v["subtasks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s.as_str().map(Into::into))
        .collect())
}

#[allow(dead_code)]
pub fn timesheet_lines(text: &str, tasks: &[(i64, String)]) -> Result<Vec<(i64, f64, String)>> {
    let list: String = tasks.iter().map(|(id, n)| format!("{id}: {n}\n")).collect();
    let v = chat(
        &format!(
            r#"Map the user's work log to these tasks:
{list}Reply only {{"lines": [{{"task_id": int, "hours": number, "description": "..."}}]}}. Use only listed ids."#
        ),
        text,
    )?;
    let known_task = |id: i64| tasks.iter().any(|(t, _)| *t == id);
    Ok(v["lines"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| {
            Some((
                l["task_id"].as_i64()?,
                l["hours"].as_f64()?,
                l["description"].as_str()?.into(),
            ))
        })
        .filter(|(id, h, _)| known_task(*id) && *h > 0.0 && *h <= 24.0)
        .collect())
}
