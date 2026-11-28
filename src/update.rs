use anyhow::Result;
use serde_json::Value;

pub const NAME: &str = "BetterOdoo";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPO_URL: &str = "https://github.com/SELECT-stupidity-FROM-discord/BetterOdoo";

pub enum Check {
    UpToDate,
    Available { tag: String, url: String },
    NoReleases,
}

// Fetches the newest GitHub release and compares it with this build.
pub fn latest() -> Result<Check> {
    let api = REPO_URL.replace("github.com/", "api.github.com/repos/") + "/releases/latest";
    let res = ureq::get(api)
        .header("User-Agent", NAME)
        .header("Accept", "application/vnd.github+json")
        .call();
    let release: Value = match res {
        Ok(mut r) => r.body_mut().read_json()?,
        Err(ureq::Error::StatusCode(404)) => return Ok(Check::NoReleases),
        Err(e) => return Err(e.into()),
    };
    let tag = release["tag_name"].as_str().unwrap_or_default().to_string();
    if is_newer(&tag, VERSION) {
        let url = release["html_url"].as_str().unwrap_or(REPO_URL).to_string();
        Ok(Check::Available { tag, url })
    } else {
        Ok(Check::UpToDate)
    }
}

fn parts(v: &str) -> [u64; 3] {
    let mut out = [0; 3];
    let nums = v
        .trim_start_matches('v')
        .split(['.', '-', '+'])
        .map_while(|p| p.parse::<u64>().ok());
    for (slot, n) in out.iter_mut().zip(nums) {
        *slot = n;
    }
    out
}

fn is_newer(remote: &str, local: &str) -> bool {
    parts(remote) > parts(local)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("0.1", "0.1.0"));
        assert!(!is_newer("v0.1.0", "0.1.1"));
    }
}
