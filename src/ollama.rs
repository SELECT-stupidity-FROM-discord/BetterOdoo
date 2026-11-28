use crate::ai;
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader};

#[derive(Clone)]
pub struct Model {
    pub name: String,
    pub size: u64,
}

pub fn root() -> String {
    let base = ai::base();
    base.trim_end_matches('/')
        .trim_end_matches("/v1")
        .to_string()
}

pub fn installed() -> Result<Vec<Model>> {
    let res: Value = ureq::get(format!("{}/api/tags", root()))
        .call()?
        .body_mut()
        .read_json()?;
    let mut models: Vec<Model> = res["models"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| {
                    Some(Model {
                        name: m["name"].as_str()?.to_string(),
                        size: m["size"].as_u64().unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    models.sort_by_key(|m| m.name.to_lowercase());
    Ok(models)
}

pub fn remove(name: &str) -> Result<()> {
    ureq::delete(format!("{}/api/delete", root()))
        .force_send_body()
        .send_json(json!({"model": name}))?;
    Ok(())
}

pub fn pull(name: &str, mut progress: impl FnMut(&str, f32)) -> Result<()> {
    let res = ureq::post(format!("{}/api/pull", root()))
        .send_json(json!({"model": name, "stream": true}))?;
    for line in BufReader::new(res.into_body().into_reader()).lines() {
        let v: Value = serde_json::from_str(&line?)?;
        if let Some(e) = v["error"].as_str() {
            return Err(anyhow!("{e}"));
        }
        let fraction = match (v["completed"].as_u64(), v["total"].as_u64()) {
            (Some(c), Some(t)) if t > 0 => c as f32 / t as f32,
            _ => 0.,
        };
        progress(v["status"].as_str().unwrap_or(""), fraction);
        if v["status"] == "success" {
            return Ok(());
        }
    }
    Err(anyhow!("download ended early"))
}

pub fn size_label(bytes: u64) -> String {
    let gb = bytes as f64 / 1e9;
    if gb >= 1. {
        format!("{gb:.1} GB")
    } else {
        format!("{} MB", (bytes as f64 / 1e6).round())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(size_label(4_683_087_332), "4.7 GB");
        assert_eq!(size_label(274_000_000), "274 MB");
    }
}
