use anyhow::{Result, anyhow};
use serde_json::{Value, json};

pub struct Odoo {
    url: String,
    agent: ureq::Agent,
    cookie: Option<String>,
    bearer: Option<(String, String)>,
    uid: i64,
}

#[derive(Clone, Debug)]
pub struct Task {
    pub id: i64,
    pub name: String,
    pub project: String,
    pub project_id: Option<i64>,
    pub parent: Option<(i64, String)>,
    pub sequence: i64,
    pub stage_id: Option<i64>,
    pub stage: String,
    pub state: String,
    pub assignees: Vec<String>,
    pub deadline: Option<String>,
    pub priority_high: bool,
    pub logged_hours: f64,
    pub done: bool,
}

#[derive(Clone, Debug)]
pub struct WorkItem {
    pub name: String,
    pub hours: f64,
    pub date: String,
    pub done: bool,
}

#[derive(Clone, Debug)]
pub struct Stage {
    pub id: i64,
    pub name: String,
    pub fold: bool,
}

#[derive(Clone, Debug)]
pub struct Change {
    pub field: String,
    pub old: String,
    pub new: String,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub author: String,
    pub date: String,
    pub body: String,
    pub changes: Vec<Change>,
}

#[derive(Clone, Debug)]
pub struct Timesheet {
    pub name: String,
    pub hours: f64,
    pub date: String,
    pub user: String,
}

#[derive(Clone)]
pub enum Auth {
    Password { login: String, password: String },
    ApiKey(String),
    Session(String),
}

#[derive(Clone)]
pub struct Login {
    pub url: String,
    pub db: String,
    pub auth: Auth,
}

impl Login {
    pub fn to_credentials(&self) -> (String, Vec<u8>) {
        let (kind, user, secret) = match &self.auth {
            Auth::Password { login, password } => ("password", login.as_str(), password),
            Auth::ApiKey(k) => ("api_key", "", k),
            Auth::Session(c) => ("session", "", c),
        };
        let user = [kind, &self.url, &self.db, user].join("\n");
        (user, secret.as_bytes().to_vec())
    }

    pub fn from_credentials(user: &str, secret: Vec<u8>) -> Option<Self> {
        let mut parts = user.splitn(4, '\n');
        let (kind, url, db, login) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
        let secret = String::from_utf8(secret).ok()?;
        let auth = match kind {
            "password" => Auth::Password {
                login: login.into(),
                password: secret,
            },
            "api_key" => Auth::ApiKey(secret),
            "session" => Auth::Session(secret),
            _ => return None,
        };
        Some(Self {
            url: url.into(),
            db: db.into(),
            auth,
        })
    }
}

pub struct Probe {
    pub url: String,
    pub version: String,
    pub db: String,
    pub sso: Option<&'static str>,
}

pub fn normalize_url(input: &str) -> String {
    let s = input.trim().trim_end_matches('/');
    if s.contains("://") {
        return s.into();
    }
    let local = s.starts_with("localhost") || s.starts_with("127.") || s.contains(".localhost");
    format!("{}://{s}", if local { "http" } else { "https" })
}

fn subdomain(url: &str) -> String {
    let host = url.split("://").last().unwrap_or(url);
    host.split(['.', ':', '/']).next().unwrap_or(host).into()
}

impl Odoo {
    fn bare(url: &str) -> Self {
        Self {
            url: normalize_url(url),
            agent: ureq::Agent::new_with_defaults(),
            cookie: None,
            bearer: None,
            uid: 0,
        }
    }

    pub fn probe(input: &str) -> Result<Probe> {
        let me = Self::bare(input);
        let version = me
            .version()
            .map_err(|_| anyhow!("No Odoo server answered at this address"))?;
        let db = me
            .rpc("/web/database/list", json!({}))
            .ok()
            .and_then(|v| v.as_array()?.first()?.as_str().map(String::from))
            .unwrap_or_else(|| subdomain(&me.url));
        let page = me
            .agent
            .get(format!("{}/web/login", me.url))
            .call()
            .ok()
            .and_then(|mut r| r.body_mut().read_to_string().ok())
            .unwrap_or_default()
            .to_lowercase();
        let sso = page.contains("auth_oauth").then(|| {
            if page.contains("google") {
                "Google"
            } else if page.contains("microsoft") {
                "Microsoft"
            } else {
                "SSO"
            }
        });
        Ok(Probe {
            url: me.url,
            version,
            db,
            sso,
        })
    }

    pub fn connect(login: &Login) -> Result<Self> {
        let mut me = Self::bare(&login.url);
        match &login.auth {
            Auth::Password {
                login: user,
                password,
            } => {
                let res = me.rpc(
                    "/web/session/authenticate",
                    json!({"db": login.db, "login": user, "password": password}),
                )?;
                me.uid = res["uid"].as_i64().ok_or_else(|| {
                    anyhow!("Wrong email or password. If you sign in with Google, Microsoft or 2FA, use a session cookie instead.")
                })?;
                return Ok(me);
            }
            Auth::ApiKey(key) => me.bearer = Some((key.clone(), login.db.clone())),
            Auth::Session(cookie) => me.cookie = Some(cookie.trim().into()),
        }
        let ctx = me
            .call("res.users", "context_get", json!({}))
            .map_err(|_| {
                anyhow!("Odoo rejected this sign-in. The key or cookie may be wrong or expired.")
            })?;
        me.uid = ctx["uid"].as_i64().unwrap_or(0);
        Ok(me)
    }

    pub fn workspace(&self) -> String {
        subdomain(&self.url)
    }

    pub fn version(&self) -> Result<String> {
        let info = self.rpc("/web/webclient/version_info", json!({}))?;
        let serie = info["server_serie"].as_str().unwrap_or("?");
        let serie = serie.rsplit('~').next().unwrap_or(serie);
        Ok(format!("Odoo {}", serie.trim_end_matches(".0")))
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
        let r = self.rpc(
            &format!("/web/dataset/call_kw/{model}/{method}"),
            json!({"model": model, "method": method, "args": args, "kwargs": kw}),
        );
        if let Err(e) = &r {
            eprintln!("{model}.{method}: {e}");
        }
        r
    }

    pub fn tasks(&self, domain: Value) -> Result<Vec<Task>> {
        let fields = [
            "name",
            "project_id",
            "parent_id",
            "sequence",
            "stage_id",
            "user_ids",
            "date_deadline",
            "priority",
            "effective_hours",
        ];
        let query = |extra: &[&str]| {
            let fields: Vec<&str> = fields.iter().chain(extra).copied().collect();
            self.call(
                "project.task",
                "search_read",
                json!({"domain": domain, "fields": fields, "order": "priority desc, date_deadline asc"}),
            )
        };
        let rows = query(&["state"]).or_else(|_| query(&[]))?;
        let stage_ids: Vec<i64> = rows
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r["stage_id"].get(0)?.as_i64())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let folded: Vec<i64> = self
            .call(
                "project.task.type",
                "read",
                json!({"ids": stage_ids, "fields": ["fold"]}),
            )
            .map(|v| {
                v.as_array()
                    .into_iter()
                    .flatten()
                    .filter(|s| s["fold"] == true)
                    .filter_map(|s| s["id"].as_i64())
                    .collect()
            })
            .unwrap_or_default();
        let ids: Vec<i64> = rows
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|r| r["user_ids"].as_array().cloned().unwrap_or_default())
            .filter_map(|v| v.as_i64())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let users = self.call("res.users", "read", json!({"ids": ids, "fields": ["name"]}))?;
        let name_of = |id: i64| {
            if id == self.uid {
                return "Me".to_string();
            }
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
                project_id: r["project_id"].get(0).and_then(|v| v.as_i64()),
                parent: r["parent_id"]
                    .get(0)
                    .and_then(|v| v.as_i64())
                    .map(|id| (id, m2o(&r["parent_id"]))),
                sequence: r["sequence"].as_i64().unwrap_or(0),
                stage_id: r["stage_id"].get(0).and_then(|v| v.as_i64()),
                stage: m2o(&r["stage_id"]),
                state: r["state"].as_str().unwrap_or("").into(),
                assignees: r["user_ids"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_i64())
                    .map(name_of)
                    .collect(),
                deadline: r["date_deadline"].as_str().map(Into::into),
                priority_high: r["priority"] == "1",
                logged_hours: r["effective_hours"].as_f64().unwrap_or(0.),
                done: matches!(r["state"].as_str(), Some("1_done" | "1_canceled"))
                    || r["stage_id"]
                        .get(0)
                        .and_then(|v| v.as_i64())
                        .is_some_and(|id| folded.contains(&id)),
            })
            .collect())
    }

    pub fn add_work(&self, parent: &Task, items: &[WorkItem]) -> Result<Option<String>> {
        let vals: Vec<Value> = items
            .iter()
            .map(|i| {
                json!({
                    "name": i.name,
                    "parent_id": parent.id,
                    "project_id": parent.project_id,
                    "user_ids": [[6, 0, [self.uid]]],
                })
            })
            .collect();
        let made = self.call("project.task", "create", json!({"vals_list": vals}))?;
        let mut problems = Vec::new();
        let done: Vec<i64> = made
            .as_array()
            .into_iter()
            .flatten()
            .zip(items)
            .filter(|(_, i)| i.done)
            .filter_map(|(id, _)| id.as_i64())
            .collect();
        if !done.is_empty()
            && let Err(e) = self.call(
                "project.task",
                "write",
                json!({"ids": done, "vals": {"state": "1_done"}}),
            )
        {
            problems.push(format!("couldn’t mark them done ({e})"));
        }
        let lines: Vec<Value> = items
            .iter()
            .filter(|i| i.hours > 0.)
            .map(|i| timesheet_vals(parent, i))
            .collect();
        if !lines.is_empty()
            && let Err(e) = self.call(
                "account.analytic.line",
                "create",
                json!({"vals_list": lines}),
            )
        {
            problems.push(format!("couldn’t log the time ({e})"));
        }
        Ok((!problems.is_empty()).then(|| {
            format!(
                "Sub-tasks were saved, but {}. Fix that in Odoo or the Timesheets tab; don’t add them again.",
                problems.join(" and ")
            )
        }))
    }

    pub fn add_timesheet(&self, task: &Task, item: &WorkItem) -> Result<()> {
        self.call(
            "account.analytic.line",
            "create",
            json!({"vals_list": [timesheet_vals(task, item)]}),
        )?;
        Ok(())
    }

    pub fn timesheets(&self, task: i64) -> Result<Vec<Timesheet>> {
        let rows = self.call(
            "account.analytic.line",
            "search_read",
            json!({
                "domain": [["task_id", "=", task]],
                "fields": ["name", "unit_amount", "date", "user_id"],
                "order": "date desc, id desc",
            }),
        )?;
        Ok(rows
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| Timesheet {
                name: r["name"].as_str().unwrap_or("").into(),
                hours: r["unit_amount"].as_f64().unwrap_or(0.),
                date: r["date"].as_str().unwrap_or("").into(),
                user: if r["user_id"].get(0).and_then(|v| v.as_i64()) == Some(self.uid) {
                    "Me".into()
                } else {
                    m2o(&r["user_id"])
                },
            })
            .collect())
    }

    pub fn base_url(&self) -> &str {
        &self.url
    }

    pub fn description(&self, task: i64) -> Result<String> {
        let rows = self.call(
            "project.task",
            "read",
            json!({"ids": [task], "fields": ["description"]}),
        )?;
        Ok(rows[0]["description"].as_str().unwrap_or("").to_string())
    }

    pub fn messages(&self, task: i64) -> Result<Vec<Message>> {
        let rows = self.call(
            "mail.message",
            "search_read",
            json!({
                "domain": [["model", "=", "project.task"], ["res_id", "=", task]],
                "fields": ["author_id", "date", "body", "tracking_value_ids"],
                "order": "id desc",
            }),
        )?;
        let ids: Vec<i64> = rows
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|r| {
                r["tracking_value_ids"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            })
            .filter_map(|v| v.as_i64())
            .collect();
        let tracking = if ids.is_empty() {
            json!([])
        } else {
            self.call(
                "mail.tracking.value",
                "read",
                json!({"ids": ids, "fields": [
                    "field_id", "old_value_char", "new_value_char", "old_value_datetime",
                    "new_value_datetime", "old_value_integer", "new_value_integer",
                    "old_value_float", "new_value_float",
                ]}),
            )
            .unwrap_or(json!([]))
        };
        let change = |t: &Value| {
            let pick = |side: &str| {
                ["char", "datetime", "integer", "float"]
                    .iter()
                    .map(|k| &t[format!("{side}_value_{k}")])
                    .find(|v| !v.is_null() && **v != json!(false) && **v != json!(""))
                    .map(|v| v.as_str().map_or(v.to_string(), str::to_string))
                    .unwrap_or("None".into())
            };
            Change {
                field: m2o(&t["field_id"]),
                old: pick("old"),
                new: pick("new"),
            }
        };
        Ok(rows
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| Message {
                author: m2o(&r["author_id"]),
                date: r["date"].as_str().unwrap_or("").into(),
                body: r["body"].as_str().unwrap_or("").into(),
                changes: r["tracking_value_ids"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|id| tracking.as_array()?.iter().find(|t| t["id"] == *id))
                    .map(change)
                    .collect(),
            })
            .collect())
    }

    pub fn post_message(&self, task: i64, text: &str, as_note: bool) -> Result<()> {
        let body = format!(
            "<p>{}</p>",
            text.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('\n', "<br>")
        );
        let subtype = if as_note {
            "mail.mt_note"
        } else {
            "mail.mt_comment"
        };
        self.call(
            "project.task",
            "message_post",
            json!({
                "ids": [task],
                "body": body,
                "message_type": "comment",
                "subtype_xmlid": subtype,
            }),
        )?;
        Ok(())
    }

    pub fn stages(&self, project: i64) -> Result<Vec<Stage>> {
        let rows = self.call(
            "project.task.type",
            "search_read",
            json!({
                "domain": [["project_ids", "in", [project]]],
                "fields": ["name", "fold"],
                "order": "sequence, id",
            }),
        )?;
        Ok(rows
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| {
                Some(Stage {
                    id: r["id"].as_i64()?,
                    name: r["name"].as_str()?.into(),
                    fold: r["fold"] == true,
                })
            })
            .collect())
    }

    pub fn set_stage(&self, task: i64, stage: i64) -> Result<()> {
        self.write(&[task], json!({"stage_id": stage}))
    }

    pub fn set_done(&self, task: i64, done: bool) -> Result<()> {
        let state = if done { "1_done" } else { "01_in_progress" };
        self.write(&[task], json!({"state": state}))
    }

    pub fn resequence(&self, order: &[i64]) -> Result<()> {
        for (i, id) in order.iter().enumerate() {
            self.write(&[*id], json!({"sequence": (i as i64 + 1) * 10}))?;
        }
        Ok(())
    }

    fn write(&self, ids: &[i64], vals: Value) -> Result<()> {
        self.call("project.task", "write", json!({"ids": ids, "vals": vals}))?;
        Ok(())
    }
}

fn timesheet_vals(task: &Task, item: &WorkItem) -> Value {
    json!({
        "task_id": task.id,
        "project_id": task.project_id,
        "name": item.name,
        "unit_amount": item.hours,
        "date": item.date,
    })
}

fn m2o(v: &Value) -> String {
    v.get(1).and_then(|s| s.as_str()).unwrap_or("—").into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_and_credentials() {
        assert_eq!(
            normalize_url(" yourco.odoo.com/ "),
            "https://yourco.odoo.com"
        );
        assert_eq!(normalize_url("localhost:8069"), "http://localhost:8069");
        assert_eq!(normalize_url("http://x.io"), "http://x.io");
        assert_eq!(subdomain("https://yourco.odoo.com"), "yourco");
        let login = Login {
            url: "https://yourco.odoo.com".into(),
            db: "yourco".into(),
            auth: Auth::Password {
                login: "me@yourco.com".into(),
                password: "p\nw".into(),
            },
        };
        let (user, secret) = login.to_credentials();
        let back = Login::from_credentials(&user, secret).unwrap();
        assert_eq!(
            (back.url.as_str(), back.db.as_str()),
            ("https://yourco.odoo.com", "yourco")
        );
        assert!(
            matches!(back.auth, Auth::Password { login, password } if login == "me@yourco.com" && password == "p\nw")
        );
        assert!(Login::from_credentials("bogus", vec![]).is_none());
    }
}
