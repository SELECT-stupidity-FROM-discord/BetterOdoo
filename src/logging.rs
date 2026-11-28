use crate::date;
use regex::Regex;
use std::{
    backtrace::Backtrace,
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{LazyLock, Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

const KEEP_FILES: usize = 10;
const TAIL_LINES: usize = 400;
const MAX_SHARE_BYTES: usize = 1 << 20;

struct State {
    file: Option<File>,
    tail: VecDeque<String>,
}

struct Logger {
    dir: PathBuf,
    session: PathBuf,
    state: Mutex<State>,
}

static LOGGER: OnceLock<Logger> = OnceLock::new();
static SECRETS: Mutex<(Vec<String>, Option<Regex>)> = Mutex::new((Vec::new(), None));

// Each rule is applied in order to every log line before it is written anywhere.
static RULES: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    let rule =
        |pattern: &str, with: &'static str| (Regex::new(pattern).expect("valid regex"), with);
    vec![
        rule(
            r"(?i)\b(authorization|set-cookie|cookie)\b\s*[:=]\s*[^\r\n]*",
            "${1}: <redacted>",
        ),
        rule(
            r"(?i)\b(bearer|basic)\s+[A-Za-z0-9._~+/=-]{6,}",
            "${1} <redacted>",
        ),
        rule(
            r#"(?i)\b(session_id|api[_-]?key|password|passwd|token|secret|login|database|db)(["']?\s*[:=]\s*["']?)[^\s"',;&})\]]+"#,
            "${1}${2}<redacted>",
        ),
        rule(
            r#"(?i)\b(https?|wss?)://[^/\s"'?#<>)]+([^\s"'?#<>)]*)(\?[^\s"'<>)]*)?"#,
            "${1}://<host>${2}",
        ),
        rule(
            r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+",
            "<email>",
        ),
        rule(r"\b(\d{1,3}\.){3}\d{1,3}\b", "<ip>"),
        rule(r"\b[A-Za-z0-9_-]{32,}\b", "<token>"),
        rule(r#"(?i)(/Users/|/home/)[^/\s"']+"#, "${1}<user>"),
        rule(r#"(?i)([A-Za-z]:\\Users\\)[^\\\s"']+"#, "${1}<user>"),
    ]
});

/// Tells the redactor an exact value (server host, login, key, cookie…) to scrub everywhere.
pub fn add_secret(value: &str) {
    let value = value.trim();
    if value.len() < 4 {
        return;
    }
    let mut guard = SECRETS.lock().unwrap_or_else(|e| e.into_inner());
    if guard.0.iter().any(|s| s == value) {
        return;
    }
    guard.0.push(value.to_string());
    let mut sorted: Vec<&String> = guard.0.iter().collect();
    sorted.sort_by_key(|s| std::cmp::Reverse(s.len()));
    let pattern = sorted
        .iter()
        .map(|s| regex::escape(s))
        .collect::<Vec<_>>()
        .join("|");
    guard.1 = Regex::new(&format!("(?i){pattern}")).ok();
}

/// Removes personal and sensitive details from text that may be shared.
pub fn redact(text: &str) -> String {
    let mut out = text.to_string();
    if let Some(re) = &SECRETS.lock().unwrap_or_else(|e| e.into_inner()).1 {
        out = re.replace_all(&out, "<redacted>").into_owned();
    }
    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"))
        && home.len() > 3
    {
        out = out.replace(&home, "~");
    }
    for (re, with) in RULES.iter() {
        out = re.replace_all(&out, *with).into_owned();
    }
    out
}

fn utc_now() -> (String, String) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (day, rest) = ((secs / 86_400) as i64, secs % 86_400);
    let time = (rest / 3600, rest % 3600 / 60, rest % 60);
    (
        date::iso(day),
        format!("{:02}:{:02}:{:02}", time.0, time.1, time.2),
    )
}

fn log_dir() -> PathBuf {
    let var = |k: &str| std::env::var(k).ok().map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        var("HOME").map(|h| h.join("Library/Logs"))
    } else if cfg!(windows) {
        var("LOCALAPPDATA")
    } else {
        var("XDG_STATE_HOME").or_else(|| var("HOME").map(|h| h.join(".local/state")))
    };
    base.unwrap_or_else(std::env::temp_dir)
        .join("BetterOdoo")
        .join("logs")
}

fn files(dir: &std::path::Path, prefix: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(prefix) && n.ends_with(".log"))
        })
        .collect();
    // Names embed a sortable timestamp, so newest comes first.
    found.sort_by(|a, b| b.cmp(a));
    found
}

impl Logger {
    fn push(&self, line: String) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(file) = &mut state.file {
            let _ = writeln!(file, "{line}");
        }
        state.tail.push_back(line);
        if state.tail.len() > TAIL_LINES {
            state.tail.pop_front();
        }
    }
}

impl log::Log for Logger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        // Our own code at debug level; dependencies only when they warn.
        m.target().starts_with("better_odoo") || m.level() <= log::Level::Warn
    }

    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let (day, time) = utc_now();
        self.push(redact(&format!(
            "{day}T{time}Z {:5} {}: {}",
            r.level(),
            r.target(),
            r.args()
        )));
    }

    fn flush(&self) {}
}

/// Starts the per-session runtime log and the crash reporter.
pub fn init() {
    let dir = log_dir();
    let _ = fs::create_dir_all(&dir);
    for kind in ["session-", "crash-"] {
        for old in files(&dir, kind).into_iter().skip(KEEP_FILES - 1) {
            let _ = fs::remove_file(old);
        }
    }
    let (day, time) = utc_now();
    let session = dir.join(format!("session-{day}-{}.log", time.replace(':', "")));
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&session)
        .ok();
    let logger = LOGGER.get_or_init(|| Logger {
        dir,
        session,
        state: Mutex::new(State {
            file,
            tail: VecDeque::new(),
        }),
    });
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Debug);
    }
    install_panic_hook();
    log::info!(
        "BetterOdoo {} starting on {} {}",
        crate::update::VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH
    );
}

fn crash_report(panic: &str, backtrace: &str, thread: &str, tail: &[String]) -> String {
    let (day, time) = utc_now();
    format!(
        "BetterOdoo crash report\nversion: {}\nos: {} {}\ntime: {day}T{time}Z\nthread: {thread}\n\n{}\n\nbacktrace:\n{}\n\nlast log lines:\n{}\n",
        crate::update::VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH,
        redact(panic),
        redact(backtrace),
        tail.join("\n"),
    )
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(logger) = LOGGER.get() {
            let tail: Vec<String> = logger
                .state
                .try_lock()
                .map(|s| s.tail.iter().cloned().collect())
                .unwrap_or_default();
            let thread = std::thread::current();
            let report = crash_report(
                &info.to_string(),
                &Backtrace::force_capture().to_string(),
                thread.name().unwrap_or("unnamed"),
                &tail,
            );
            let (day, time) = utc_now();
            let path = logger
                .dir
                .join(format!("crash-{day}-{}.log", time.replace(':', "")));
            let _ = fs::write(path, report);
        }
        previous(info);
    }));
}

/// Where this session's log is written.
pub fn session_path() -> Option<PathBuf> {
    LOGGER.get().map(|l| l.session.clone())
}

/// This session's log, already redacted, ready to paste into a bug report.
pub fn session_text() -> String {
    tail_of(session_path())
}

/// Crash logs from this and earlier runs, newest first.
pub fn crash_files() -> Vec<PathBuf> {
    LOGGER
        .get()
        .map(|l| files(&l.dir, "crash-"))
        .unwrap_or_default()
}

pub fn read_crash(path: &std::path::Path) -> String {
    tail_of(Some(path.to_path_buf()))
}

fn tail_of(path: Option<PathBuf>) -> String {
    let bytes = path.and_then(|p| fs::read(p).ok()).unwrap_or_default();
    let start = bytes.len().saturating_sub(MAX_SHARE_BYTES);
    String::from_utf8_lossy(&bytes[start..]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_server_and_keeps_the_request_path() {
        let out = redact(
            "POST https://acme-corp.odoo.com/web/dataset/call_kw/project.task/search_read?db=acme failed",
        );
        assert!(!out.contains("acme"));
        assert!(out.contains("/web/dataset/call_kw/project.task/search_read"));
    }

    #[test]
    fn hides_credentials_and_contact_details() {
        for leaky in [
            "sign in as jane.doe@example.com",
            "Cookie: session_id=abc123def456",
            "header Authorization: bearer sk_live_9f8e7d6c5b4a",
            "payload {\"login\": \"jane\", \"password\": \"s3cret!\"}",
            "api_key=0123456789abcdef0123456789abcdef01234567",
            "token 0123456789abcdef0123456789abcdef01234567 rejected",
            "connect to 192.168.1.20 refused",
        ] {
            let out = redact(leaky);
            for secret in [
                "jane",
                "abc123",
                "sk_live",
                "s3cret",
                "0123456789abcdef",
                "192.168",
            ] {
                assert!(!out.contains(secret), "{secret:?} survived in {out:?}");
            }
        }
    }

    #[test]
    fn hides_user_names_in_paths() {
        assert!(!redact("at /Users/jane/work/app/src/main.rs:10").contains("jane"));
        assert!(!redact(r"at C:\Users\jane\app\main.rs").contains("jane"));
        assert!(!redact("at /home/jane/app/main.rs").contains("jane"));
    }

    #[test]
    fn hides_registered_secrets_in_any_case() {
        add_secret("Hunter2-Pass");
        assert!(!redact("password was hunter2-PASS ok").contains("unter2"));
    }

    #[test]
    fn keeps_ordinary_messages_readable() {
        let line = "project.task.search_read failed: odoo: Access denied";
        assert_eq!(redact(line), line);
    }

    #[test]
    fn writes_session_and_crash_files_without_personal_data() {
        let home = std::env::temp_dir().join(format!("bo-log-test-{}", std::process::id()));
        // Safe here: this is the only test that touches the environment.
        unsafe {
            std::env::set_var("HOME", &home);
            std::env::set_var("XDG_STATE_HOME", &home);
            std::env::set_var("LOCALAPPDATA", &home);
        }
        init();
        log::info!("signed in as jane@example.com at https://acme-corp.odoo.com/web");
        let _ = std::panic::catch_unwind(|| panic!("boom for jane@example.com"));

        let session = std::fs::read_to_string(session_path().unwrap()).unwrap();
        assert!(session.contains("signed in as <email>"), "{session}");
        assert!(
            !session.contains("jane") && !session.contains("acme"),
            "{session}"
        );

        let crashes = crash_files();
        assert_eq!(crashes.len(), 1);
        let crash = read_crash(&crashes[0]);
        assert!(crash.contains("boom for <email>"), "{crash}");
        assert!(crash.contains("signed in as <email>"), "{crash}");
        assert!(!crash.contains("jane"), "{crash}");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn crash_report_is_redacted_and_labelled() {
        let report = crash_report(
            "panicked at /Users/jane/app/src/odoo.rs:5:1:\nbad row for jane@example.com",
            "0: better_odoo::odoo::tasks",
            "main",
            &["2026-01-01T00:00:00Z INFO x: started".into()],
        );
        assert!(report.contains("BetterOdoo crash report"));
        assert!(report.contains("better_odoo::odoo::tasks"));
        assert!(report.contains("started"));
        assert!(!report.contains("jane"));
    }
}
