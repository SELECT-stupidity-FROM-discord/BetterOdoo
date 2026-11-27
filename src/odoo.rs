use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::env::var;

pub struct Odoo {
    url: String,
    agent: ureq::Agent,
    cookie: Option<String>,
    bearer: Option<(String, String)>,
}

#[derive(Clone, Debug)]
pub struct Task {
    pub id: i64,
    pub name: String,
    pub project: String,
    pub stage: String,
    pub assignees: Vec<String>,
    pub deadline: Option<String>,
    pub priority_high: bool,
}

impl Odoo {
    pub fn from_env() -> Result<Self> {
        let env = |k: &str| var(k).map_err(|_| anyhow!("set {k} (see PLAN.md, Sign-in)"));
        let db = var("ODOO_DB").unwrap_or_default();
        let mut me = Self {
            url: env("ODOO_URL")?.trim_end_matches('/').into(),
            agent: ureq::Agent::new_with_defaults(),
            cookie: None,
            bearer: None,
        };
        if let Ok(key) = var("ODOO_API_KEY") {
            me.bearer = Some((key, db));
        } else if let Ok(s) = var("ODOO_SESSION") {
            me.cookie = Some(s);
        } else {
            let res = me.rpc(
                "/web/session/authenticate",
                json!({"db": db, "login": env("ODOO_LOGIN")?, "password": env("ODOO_PASSWORD")?}),
            )?;
            if res["uid"].as_i64().is_none() {
                return Err(anyhow!(
                    "password login failed. SSO or 2FA account? Use ODOO_SESSION instead"
                ));
            }
            return Ok(me);
        }
        me.call("res.users", "context_get", json!({}))
            .map_err(|e| anyhow!("sign-in rejected (expired cookie or bad key?): {e}"))?;
        Ok(me)
    }

    fn rpc(&self, path: &str, params: Value) -> Result<Value> {
        let body = json!({"jsonrpc": "2.0", "method": "call", "params": params});
        let mut req = self.agent.post(format!("{}{path}", self.url));
        if let Some(c) = &self.cookie {
            req = req.header("Cookie", &format!("session_id={c}"));
        }
        let mut v: Value = req.send_json(&body)?.body_mut().read_json()?;
        if let Some(e) = v.get("error") {
            return Err(anyhow!(
                "odoo: {}",
                e["data"]["message"].as_str().unwrap_or(&e.to_string())
            ));
        }
        Ok(v["result"].take())
    }

    pub fn call(&self, model: &str, method: &str, mut kw: Value) -> Result<Value> {
        if let Some((key, db)) = &self.bearer {
            let mut req = self
                .agent
                .post(format!("{}/json/2/{model}/{method}", self.url))
                .header("Authorization", &format!("bearer {key}"));
            if !db.is_empty() {
                req = req.header("X-Odoo-Database", db);
            }
            return Ok(req.send_json(&kw)?.body_mut().read_json()?);
        }
        let args = kw
            .as_object_mut()
            .and_then(|o| o.remove("ids"))
            .map_or(json!([]), |ids| json!([ids]));
        self.rpc(
            &format!("/web/dataset/call_kw/{model}/{method}"),
            json!({"model": model, "method": method, "args": args, "kwargs": kw}),
        )
    }

    pub fn tasks(&self, domain: Value) -> Result<Vec<Task>> {
        let rows = self.call("project.task", "search_read", json!({
            "domain": domain,
            "fields": ["name", "project_id", "stage_id", "user_ids", "date_deadline", "priority"],
            "limit": 200,
            "order": "priority desc, date_deadline asc",
        }))?;
        let ids: Vec<i64> = rows
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|r| r["user_ids"].as_array().cloned().unwrap_or_default())
            .filter_map(|v| v.as_i64())
            .collect();
        let users = self.call("res.users", "read", json!({"ids": ids, "fields": ["name"]}))?;
        let name_of = |id: i64| {
            users
                .as_array()
                .into_iter()
                .flatten()
                .find(|u| u["id"] == id)
                .and_then(|u| u["name"].as_str())
                .unwrap_or("?")
                .to_string()
        };
        Ok(rows
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| Task {
                id: r["id"].as_i64().unwrap_or(0),
                name: r["name"].as_str().unwrap_or("").into(),
                project: m2o(&r["project_id"]),
                stage: m2o(&r["stage_id"]),
                assignees: r["user_ids"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_i64())
                    .map(name_of)
                    .collect(),
                deadline: r["date_deadline"].as_str().map(Into::into),
                priority_high: r["priority"] == "1",
            })
            .collect())
    }

    pub fn create_subtasks(&self, parent: i64, names: &[String]) -> Result<()> {
        let vals: Vec<Value> = names
            .iter()
            .map(|n| json!({"name": n, "parent_id": parent}))
            .collect();
        self.call("project.task", "create", json!({"vals_list": vals}))?;
        Ok(())
    }
}

fn m2o(v: &Value) -> String {
    v.get(1).and_then(|s| s.as_str()).unwrap_or("—").into()
}
