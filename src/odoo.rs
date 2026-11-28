use crate::html::{self, Block};
use anyhow::{Result, anyhow};
use serde_json::{Value, json};

pub struct Odoo {
    url: String,
    agent: ureq::Agent,
    cookie: Option<String>,
    bearer: Option<(String, String)>,
    uid: i64,
    partner_id: Option<i64>,
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
    pub stage_since: Option<String>,
    pub user_ids: Vec<i64>,
    pub tags: Vec<(i64, String)>,
    pub partner: Option<(i64, String)>,
    pub creator: Option<String>,
    pub creator_id: Option<i64>,
    pub personal_stage: Option<String>,
}

impl Task {
    pub fn assigned_by(&self, who: &str) -> bool {
        if self.creator.as_deref() != Some(who) {
            return false;
        }
        match self.creator_id {
            Some(cid) => self.user_ids.iter().any(|&id| id != cid),
            None => self.assignees.iter().any(|a| a != who),
        }
    }
}

#[derive(Clone, Debug)]
pub struct WorkItem {
    pub name: String,
    pub hours: f64,
    pub date: String,
    pub done: bool,
}

#[derive(Clone, Debug, Default)]
pub struct NewTaskInput {
    pub name: String,
    pub description: Option<String>,
    pub project_id: Option<i64>,
    pub stage_id: Option<i64>,
    pub deadline: Option<String>,
    pub priority_high: bool,
    pub user_ids: Vec<i64>,
    pub tag_ids: Vec<i64>,
    pub partner_id: Option<i64>,
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
    pub is_me: bool,
    pub date: String,
    pub body: Vec<Block>,
    pub changes: Vec<Change>,
}

#[derive(Clone, Debug)]
pub struct Timesheet {
    pub id: i64,
    pub name: String,
    pub hours: f64,
    pub date: String,
    pub user: String,
    pub task: Option<(i64, String)>,
    pub project: String,
}

#[derive(Clone, Debug)]
pub struct Employee {
    pub id: i64,
    pub name: String,
    pub job_title: String,
    pub department: String,
    pub email: String,
    pub phone: String,
    pub mobile: String,
    pub manager: Option<String>,
    pub location: Option<String>,
    pub user_id: Option<i64>,
    pub user_name: Option<String>,
    pub presence: Option<String>,
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
            partner_id: None,
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
                me.partner_id = res["partner_id"].as_i64();
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
        if me.uid <= 0 {
            return Err(anyhow!(
                "Odoo rejected this sign-in. The key or cookie may be wrong or expired."
            ));
        }
        me.partner_id = me
            .call(
                "res.users",
                "read",
                json!({"ids": [me.uid], "fields": ["partner_id"]}),
            )
            .ok()
            .and_then(|rows| rows[0]["partner_id"].get(0)?.as_i64());
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

    fn send_with_retry<F>(&self, mut make_req: F) -> Result<Value>
    where
        F: FnMut() -> Result<Value>,
    {
        let mut last_err = None;
        for delay_ms in [0u64, 500, 1500] {
            if delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            }
            match make_req() {
                Ok(v) => return Ok(v),
                Err(e) => {
                    let msg = e.to_string();
                    let transient = msg.contains("http status: 502")
                        || msg.contains("http status: 503")
                        || msg.contains("http status: 504")
                        || msg.contains("http status: 429");
                    if !transient {
                        return Err(e);
                    }
                    last_err = Some(e);
                }
            }
        }
        Err(anyhow!(
            "Odoo server is temporarily unavailable ({}). Try again in a moment.",
            last_err
                .map(|e| e.to_string())
                .unwrap_or_else(|| "gateway error".into())
        ))
    }

    fn rpc(&self, path: &str, params: Value) -> Result<Value> {
        let body = json!({"jsonrpc": "2.0", "method": "call", "params": params});
        let url = format!("{}{path}", self.url);
        let mut v = self.send_with_retry(|| {
            let mut req = self.agent.post(&url);
            if let Some(c) = &self.cookie {
                req = req.header("Cookie", &format!("session_id={c}"));
            }
            Ok(req.send_json(&body)?.body_mut().read_json()?)
        })?;
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
            let url = format!("{}/json/2/{model}/{method}", self.url);
            return self.send_with_retry(|| {
                let mut req = self
                    .agent
                    .post(&url)
                    .header("Authorization", &format!("bearer {key}"));
                if !db.is_empty() {
                    req = req.header("X-Odoo-Database", db);
                }
                Ok(req.send_json(&kw)?.body_mut().read_json()?)
            });
        }
        let mut take = |key: &str| kw.as_object_mut().and_then(|o| o.remove(key));
        let args = take("ids")
            .or_else(|| take("vals_list"))
            .map_or(json!([]), |first| json!([first]));
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
        self.read_tasks(domain, false)
    }

    pub fn todos(&self) -> Result<Vec<Task>> {
        self.read_tasks(
            json!([["project_id", "=", false], ["parent_id", "=", false]]),
            true,
        )
    }

    fn read_tasks(&self, domain: Value, personal_stage: bool) -> Result<Vec<Task>> {
        let fields = [
            "name",
            "project_id",
            "parent_id",
            "sequence",
            "stage_id",
            "user_ids",
            "create_uid",
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
        let rows = if personal_stage {
            try_odoo_fallback(
                query(&[
                    "state",
                    "date_last_stage_update",
                    "tag_ids",
                    "partner_id",
                    "personal_stage_type_id",
                ]),
                || query(&["state", "date_last_stage_update", "tag_ids", "partner_id"]),
            )
        } else {
            query(&["state", "date_last_stage_update", "tag_ids", "partner_id"])
        };
        let rows = try_odoo_fallback(rows, || query(&["state", "date_last_stage_update"]));
        let rows = try_odoo_fallback(rows, || query(&["state"]));
        let rows = try_odoo_fallback(rows, || query(&[]))?;
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
        let tag_ids: Vec<i64> = rows
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|r| r["tag_ids"].as_array().cloned().unwrap_or_default())
            .filter_map(|v| v.as_i64())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let tag_names = if tag_ids.is_empty() {
            json!([])
        } else {
            self.call(
                "project.tags",
                "read",
                json!({"ids": tag_ids, "fields": ["name"]}),
            )
            .unwrap_or(json!([]))
        };
        let tag_name = |id: i64| {
            tag_names
                .as_array()
                .into_iter()
                .flatten()
                .find(|t| t["id"] == id)
                .and_then(|t| t["name"].as_str())
                .unwrap_or("?")
                .to_string()
        };
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
                stage_since: r["date_last_stage_update"].as_str().map(Into::into),
                user_ids: r["user_ids"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_i64())
                    .collect(),
                tags: r["tag_ids"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_i64())
                    .map(|id| (id, tag_name(id)))
                    .collect(),
                partner: r["partner_id"]
                    .get(0)
                    .and_then(|v| v.as_i64())
                    .map(|id| (id, m2o(&r["partner_id"]))),
                creator: r["create_uid"].get(0).and_then(|v| v.as_i64()).map(|id| {
                    if id == self.uid {
                        "Me".to_string()
                    } else {
                        m2o(&r["create_uid"])
                    }
                }),
                creator_id: r["create_uid"].get(0).and_then(|v| v.as_i64()),
                personal_stage: r["personal_stage_type_id"]
                    .get(1)
                    .and_then(|s| s.as_str())
                    .map(Into::into),
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
        self.read_timesheets(json!([["task_id", "=", task]]), None)
    }

    pub fn all_timesheets(&self) -> Result<Vec<Timesheet>> {
        self.read_timesheets(json!([["project_id", "!=", false]]), Some(200))
    }

    fn read_timesheets(&self, domain: Value, limit: Option<usize>) -> Result<Vec<Timesheet>> {
        let mut kw = json!({
            "domain": domain,
            "fields": ["name", "unit_amount", "date", "user_id", "task_id", "project_id"],
            "order": "date desc, id desc",
        });
        if let Some(n) = limit {
            kw["limit"] = json!(n);
        }
        let rows = self.call("account.analytic.line", "search_read", kw)?;
        Ok(rows
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| Timesheet {
                id: r["id"].as_i64().unwrap_or(0),
                name: r["name"].as_str().unwrap_or("").into(),
                hours: r["unit_amount"].as_f64().unwrap_or(0.),
                date: r["date"].as_str().unwrap_or("").into(),
                user: if r["user_id"].get(0).and_then(|v| v.as_i64()) == Some(self.uid) {
                    "Me".into()
                } else {
                    m2o(&r["user_id"])
                },
                task: r["task_id"]
                    .get(0)
                    .and_then(|v| v.as_i64())
                    .map(|id| (id, m2o(&r["task_id"]))),
                project: m2o(&r["project_id"]),
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
        if self.bearer.is_none()
            && let Ok(res) = self.rpc(
                "/mail/thread/messages",
                json!({
                    "thread_model": "project.task",
                    "thread_id": task,
                    "limit": 50,
                }),
            )
            && let Some(parsed) = parse_thread_messages(&res, &self.url, self.partner_id)
        {
            return Ok(parsed);
        }
        let rows = self.call(
            "mail.message",
            "search_read",
            json!({
                "domain": [["model", "=", "project.task"], ["res_id", "=", task]],
                "fields": ["author_id", "date", "body"],
                "order": "id desc",
            }),
        )?;
        let rows = rows.as_array().cloned().unwrap_or_default();
        let ids: Vec<i64> = rows.iter().filter_map(|r| r["id"].as_i64()).collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let tracking = self
            .call(
                "mail.tracking.value",
                "search_read",
                json!({
                    "domain": [["mail_message_id", "in", ids]],
                    "fields": [
                        "mail_message_id", "field_id", "old_value_char", "new_value_char",
                        "old_value_text", "new_value_text",
                        "old_value_datetime", "new_value_datetime", "old_value_integer",
                        "new_value_integer", "old_value_float", "new_value_float",
                    ],
                }),
            )
            .unwrap_or(json!([]));
        let tracking = tracking.as_array().cloned().unwrap_or_default();
        let pick = |t: &Value, side: &str| {
            ["char", "text", "datetime", "integer", "float"]
                .iter()
                .map(|k| &t[format!("{side}_value_{k}")])
                .find(|v| !v.is_null() && **v != json!(false) && **v != json!(""))
                .map_or("None".into(), |v| {
                    v.as_str().map_or(v.to_string(), str::to_string)
                })
        };
        Ok(rows
            .iter()
            .map(|r| Message {
                author: m2o(&r["author_id"]),
                is_me: self.partner_id.is_some()
                    && r["author_id"].get(0).and_then(|v| v.as_i64()) == self.partner_id,
                date: r["date"].as_str().unwrap_or("").into(),
                body: html::parse(r["body"].as_str().unwrap_or(""), &self.url),
                changes: tracking
                    .iter()
                    .filter(|t| t["mail_message_id"][0] == r["id"])
                    .map(|t| Change {
                        field: m2o(&t["field_id"]),
                        old: pick(t, "old"),
                        new: pick(t, "new"),
                    })
                    .collect(),
            })
            .filter(|m| !m.body.is_empty() || !m.changes.is_empty())
            .collect())
    }

    pub fn attachments(&self, task: i64) -> Result<Vec<Block>> {
        let rows = self.call(
            "ir.attachment",
            "search_read",
            json!({
                "domain": [["res_model", "=", "project.task"], ["res_id", "=", task]],
                "fields": ["name"],
            }),
        )?;
        Ok(rows
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| Block::Attachment {
                label: r["name"].as_str().unwrap_or("Attachment").into(),
                url: format!("{}/web/content/{}?download=true", self.url, r["id"]),
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

    pub fn set_priority(&self, task: i64, high: bool) -> Result<()> {
        self.write(&[task], json!({"priority": if high { "1" } else { "0" }}))
    }

    pub fn create_task(&self, input: &NewTaskInput) -> Result<()> {
        let assignees = if input.user_ids.is_empty() {
            vec![self.uid]
        } else {
            input.user_ids.clone()
        };
        let mut vals = json!({
            "name": input.name,
            "user_ids": [[6, 0, assignees]],
        });
        if let Some(desc) = input.description.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            let body = format!(
                "<p>{}</p>",
                desc.replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
                    .replace('\n', "<br>")
            );
            vals["description"] = json!(body);
        }
        if let Some(p) = input.project_id {
            vals["project_id"] = json!(p);
        }
        if let Some(s) = input.stage_id {
            vals["stage_id"] = json!(s);
        }
        if let Some(d) = &input.deadline {
            vals["date_deadline"] = json!(d);
        }
        if input.priority_high {
            vals["priority"] = json!("1");
        }
        if !input.tag_ids.is_empty() {
            vals["tag_ids"] = json!([[6, 0, input.tag_ids]]);
        }
        if let Some(partner) = input.partner_id {
            vals["partner_id"] = json!(partner);
        }
        self.call("project.task", "create", json!({"vals_list": [vals]}))?;
        Ok(())
    }

    pub fn employees(&self) -> Result<Vec<Employee>> {
        let base = [
            "name",
            "job_title",
            "department_id",
            "work_email",
            "work_phone",
            "parent_id",
            "user_id",
        ];
        let query = |model: &str, extra: &[&str]| {
            let fields: Vec<&str> = base.iter().chain(extra).copied().collect();
            self.call(
                model,
                "search_read",
                json!({
                    "domain": [],
                    "fields": fields,
                    "order": "name asc",
                }),
            )
        };
        let rows = try_odoo_fallback(
            query(
                "hr.employee.public",
                &["mobile_phone", "work_location_id", "hr_presence_state"],
            ),
            || query("hr.employee.public", &["mobile_phone"]),
        );
        let rows = try_odoo_fallback(rows, || query("hr.employee.public", &[]));
        let rows = try_odoo_fallback(rows, || {
            query(
                "hr.employee",
                &["mobile_phone", "work_location_id", "hr_presence_state"],
            )
        });
        let rows = try_odoo_fallback(rows, || query("hr.employee", &["mobile_phone"]));
        let rows = try_odoo_fallback(rows, || query("hr.employee", &[]))?;
        let text = |v: &Value| v.as_str().unwrap_or("").trim().to_string();
        Ok(rows
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| {
                let user_id = r["user_id"].get(0).and_then(|v| v.as_i64());
                let user_name = user_id.map(|id| {
                    if id == self.uid {
                        "Me".to_string()
                    } else {
                        m2o(&r["user_id"])
                    }
                });
                Employee {
                    id: r["id"].as_i64().unwrap_or(0),
                    name: text(&r["name"]),
                    job_title: text(&r["job_title"]),
                    department: r["department_id"]
                        .get(1)
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .into(),
                    email: text(&r["work_email"]),
                    phone: text(&r["work_phone"]),
                    mobile: text(&r["mobile_phone"]),
                    manager: r["parent_id"]
                        .get(1)
                        .and_then(|s| s.as_str())
                        .map(Into::into),
                    location: r["work_location_id"]
                        .get(1)
                        .and_then(|s| s.as_str())
                        .map(Into::into),
                    user_id,
                    user_name,
                    presence: r["hr_presence_state"].as_str().map(Into::into),
                }
            })
            .collect())
    }

    pub fn resequence(&self, order: &[i64]) -> Result<()> {
        for (i, id) in order.iter().enumerate() {
            self.write(&[*id], json!({"sequence": (i as i64 + 1) * 10}))?;
        }
        Ok(())
    }

    pub fn options(&self, model: &str, query: &str) -> Result<Vec<(i64, String)>> {
        let mut domain = vec![json!(["name", "ilike", query])];
        match model {
            "res.users" => domain.push(json!(["share", "=", false])),
            "project.project" => domain.push(json!(["active", "=", true])),
            _ => {}
        }
        let rows = self.call(
            model,
            "search_read",
            json!({"domain": domain, "fields": ["name"], "limit": 25, "order": "name"}),
        )?;
        Ok(rows
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| Some((r["id"].as_i64()?, r["name"].as_str()?.to_string())))
            .collect())
    }

    pub fn set_fields(&self, id: i64, vals: Value) -> Result<()> {
        self.write(&[id], vals)
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

fn try_odoo_fallback<T>(first: Result<T>, next: impl FnOnce() -> Result<T>) -> Result<T> {
    match first {
        Ok(v) => Ok(v),
        Err(e) if e.to_string().starts_with("odoo:") => next(),
        Err(e) => Err(e),
    }
}

fn format_tracking_val(v: &Value) -> String {
    if v.is_null() || *v == json!(false) || *v == json!("") {
        return "None".into();
    }
    if let Some(obj) = v.as_object() {
        if let Some(inner) = obj.get("value") {
            return format_tracking_val(inner);
        }
        if let Some(name) = obj.get("name").and_then(|s| s.as_str()) {
            return name.to_string();
        }
    }
    v.as_str().map_or_else(|| v.to_string(), str::to_string)
}

fn parse_tracking_entry(t: &Value) -> Option<Change> {
    let field = t["fieldInfo"]["desc"]
        .as_str()
        .or_else(|| t["field_desc"].as_str())
        .or_else(|| t["changedField"].as_str())
        .or_else(|| t["field"].get("desc").and_then(|v| v.as_str()))
        .or_else(|| t["field_id"].get(1).and_then(|v| v.as_str()))
        .or_else(|| t["field"].as_str())?
        .to_string();
    let pick_side = |keys: &[&str]| {
        keys.iter()
            .map(|k| &t[*k])
            .find(|v| !v.is_null() && **v != json!(false) && **v != json!(""))
            .map_or_else(|| "None".into(), format_tracking_val)
    };
    let old = pick_side(&[
        "oldValue",
        "old_value_char",
        "old_value_text",
        "old_value_datetime",
        "old_value_integer",
        "old_value_float",
        "old_value",
    ]);
    let new = pick_side(&[
        "newValue",
        "new_value_char",
        "new_value_text",
        "new_value_datetime",
        "new_value_integer",
        "new_value_float",
        "new_value",
    ]);
    Some(Change { field, old, new })
}

fn parse_thread_messages(
    res: &Value,
    base_url: &str,
    me_partner_id: Option<i64>,
) -> Option<Vec<Message>> {
    let author_pid = |m: &Value| {
        m["author_id"]
            .as_i64()
            .or_else(|| m["author_id"].get(0).and_then(|v| v.as_i64()))
            .or_else(|| m["author"].get("id").and_then(|v| v.as_i64()))
            .or_else(|| m["author_id"].get("id").and_then(|v| v.as_i64()))
    };
    // Odoo 18/19/20 Store format: { data: { "mail.message": [...], "res.partner": [...], "mail.tracking.value": [...] } }
    if let Some(data) = res.get("data").and_then(|d| d.as_object())
        && let Some(raw_msgs) = data.get("mail.message").and_then(|m| m.as_array())
    {
        let partners: std::collections::HashMap<i64, String> = data
            .get("res.partner")
            .and_then(|p| p.as_array())
            .into_iter()
            .flatten()
            .filter_map(|p| Some((p["id"].as_i64()?, p["name"].as_str()?.to_string())))
            .collect();
        let tracking_by_id: std::collections::HashMap<i64, &Value> = data
            .get("mail.tracking.value")
            .and_then(|t| t.as_array())
            .into_iter()
            .flatten()
            .filter_map(|t| Some((t["id"].as_i64()?, t)))
            .collect();
        let mut msgs: Vec<(i64, Message)> = raw_msgs
            .iter()
            .map(|m| {
                let id = m["id"].as_i64().unwrap_or(0);
                let pid = author_pid(m);
                let author = pid
                    .and_then(|pid| partners.get(&pid).cloned())
                    .or_else(|| {
                        m["author"]
                            .get("name")
                            .and_then(|s| s.as_str())
                            .map(String::from)
                    })
                    .or_else(|| {
                        m["author_id"]
                            .get(1)
                            .and_then(|s| s.as_str())
                            .map(String::from)
                    })
                    .or_else(|| {
                        m["author_id"]
                            .get("name")
                            .and_then(|s| s.as_str())
                            .map(String::from)
                    })
                    .unwrap_or_else(|| "System".into());
                let raw_body = m["body"]
                    .as_str()
                    .or_else(|| m["body"].as_array().and_then(|a| a.get(1)?.as_str()))
                    .unwrap_or("");
                let tracking_list = m["trackingValues"]
                    .as_array()
                    .or_else(|| m["tracking_value_ids"].as_array());
                let changes: Vec<Change> = tracking_list
                    .into_iter()
                    .flatten()
                    .filter_map(|entry| {
                        if let Some(tid) = entry.as_i64() {
                            tracking_by_id
                                .get(&tid)
                                .and_then(|t| parse_tracking_entry(t))
                        } else {
                            parse_tracking_entry(entry)
                        }
                    })
                    .collect();
                (
                    id,
                    Message {
                        author,
                        is_me: me_partner_id.is_some() && pid == me_partner_id,
                        date: m["date"].as_str().unwrap_or("").into(),
                        body: html::parse(raw_body, base_url),
                        changes,
                    },
                )
            })
            .filter(|(_, m)| !m.body.is_empty() || !m.changes.is_empty())
            .collect();
        msgs.sort_by_key(|(id, _)| std::cmp::Reverse(*id));
        return Some(msgs.into_iter().map(|(_, m)| m).collect());
    }

    // Inline format: { messages: [...] } or [...]
    let raw_msgs = res
        .get("messages")
        .and_then(|m| m.as_array())
        .or_else(|| res.as_array())?;
    let mut msgs: Vec<(i64, Message)> = raw_msgs
        .iter()
        .map(|m| {
            let id = m["id"].as_i64().unwrap_or(0);
            let pid = author_pid(m);
            let author = m["author"]
                .get("name")
                .and_then(|s| s.as_str())
                .map(String::from)
                .or_else(|| {
                    m["author_id"]
                        .get(1)
                        .and_then(|s| s.as_str())
                        .map(String::from)
                })
                .unwrap_or_else(|| "System".into());
            let raw_body = m["body"]
                .as_str()
                .or_else(|| m["body"].as_array().and_then(|a| a.get(1)?.as_str()))
                .unwrap_or("");
            let changes = m["trackingValues"]
                .as_array()
                .or_else(|| m["tracking_value_ids"].as_array())
                .into_iter()
                .flatten()
                .filter_map(parse_tracking_entry)
                .collect();
            (
                id,
                Message {
                    author,
                    is_me: me_partner_id.is_some() && pid == me_partner_id,
                    date: m["date"].as_str().unwrap_or("").into(),
                    body: html::parse(raw_body, base_url),
                    changes,
                },
            )
        })
        .filter(|(_, m)| !m.body.is_empty() || !m.changes.is_empty())
        .collect();
    msgs.sort_by_key(|(id, _)| std::cmp::Reverse(*id));
    Some(msgs.into_iter().map(|(_, m)| m).collect())
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

    #[test]
    fn assigned_by_matches_tasks_created_for_others() {
        let task = |creator: Option<&str>,
                    creator_id: Option<i64>,
                    user_ids: Vec<i64>,
                    assignees: Vec<&str>| Task {
            id: 1,
            name: "T".into(),
            project: "P".into(),
            project_id: Some(1),
            parent: None,
            sequence: 0,
            stage_id: None,
            stage: "Open".into(),
            state: "".into(),
            assignees: assignees.into_iter().map(String::from).collect(),
            deadline: None,
            priority_high: false,
            logged_hours: 0.,
            done: false,
            stage_since: None,
            user_ids,
            tags: vec![],
            partner: None,
            creator: creator.map(String::from),
            creator_id,
            personal_stage: None,
        };
        assert!(task(Some("Me"), Some(1), vec![2], vec!["Alice"]).assigned_by("Me"));
        assert!(task(Some("Me"), Some(1), vec![1, 2], vec!["Me", "Alice"]).assigned_by("Me"));
        assert!(!task(Some("Me"), Some(1), vec![1], vec!["Me"]).assigned_by("Me"));
        assert!(!task(Some("Me"), Some(1), vec![], vec![]).assigned_by("Me"));
        assert!(task(Some("Alice"), Some(2), vec![3], vec!["Bob"]).assigned_by("Alice"));
        assert!(!task(Some("Alice"), Some(2), vec![3], vec!["Bob"]).assigned_by("Me"));
        assert!(!task(Some("Alice"), Some(2), vec![2], vec!["Alice"]).assigned_by("Alice"));
        assert!(!task(None, None, vec![2], vec!["Alice"]).assigned_by("Me"));
        assert!(task(Some("Me"), None, vec![], vec!["Alice"]).assigned_by("Me"));
        assert!(!task(Some("Me"), None, vec![], vec!["Me"]).assigned_by("Me"));
    }

    #[test]
    fn parses_odoo_store_thread_messages_with_tracking() {
        let payload = json!({
            "data": {
                "res.partner": [
                    {"id": 7, "name": "Alice"}
                ],
                "mail.tracking.value": [
                    {
                        "id": 99,
                        "fieldInfo": {"desc": "Stage"},
                        "oldValue": {"value": "Development"},
                        "newValue": {"value": "Production"}
                    }
                ],
                "mail.message": [
                    {
                        "id": 10,
                        "author_id": 7,
                        "date": "2026-11-28 06:00:00",
                        "body": ["markup", ""],
                        "trackingValues": [99]
                    },
                    {
                        "id": 11,
                        "author_id": 7,
                        "date": "2026-11-28 06:05:00",
                        "body": "<p>Looks good</p>",
                        "trackingValues": []
                    }
                ]
            }
        });
        let msgs = parse_thread_messages(&payload, "https://acme.odoo.com", Some(7)).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].author, "Alice");
        assert!(msgs[0].is_me);
        assert!(!msgs[0].body.is_empty());
        assert_eq!(msgs[1].changes.len(), 1);
        assert_eq!(msgs[1].changes[0].field, "Stage");
        assert_eq!(msgs[1].changes[0].old, "Development");
        assert_eq!(msgs[1].changes[0].new, "Production");
    }
}
