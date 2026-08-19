//! Sync the proxy configuration into Codex CLI's env file (`~/.codex/.env`).
//!
//! The Codex CLI (`codex`) loads `~/.codex/.env` to inject environment variables
//! into its `reqwest`-based control-plane HTTP clients. Writing the same proxy
//! here as the one used to launch the app keeps the CLI on the proxy too.

use crate::error::Error;
use std::path::{Path, PathBuf};

/// Proxy-related keys we manage (uppercase take precedence in Codex CLI; the
/// lowercase variants are also stripped so no stale value lingers).
const PROXY_KEYS: &[&str] = &[
    "HTTPS_PROXY",
    "HTTP_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "WS_PROXY",
    "WSS_PROXY",
    "https_proxy",
    "http_proxy",
    "all_proxy",
    "no_proxy",
    "ws_proxy",
    "wss_proxy",
];

fn home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

/// Default Codex CLI env file: `<home>/.codex/.env`.
pub fn default_path() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".codex").join(".env"))
}

/// Write the proxy configuration into `~/.codex/.env`.
pub fn sync_proxy(proxy_url: &str) -> Result<(), Error> {
    let path = default_path().ok_or_else(|| Error::Other("无法解析 HOME 目录".into()))?;
    sync_proxy_at(&path, proxy_url)
}

/// Testable core: merge the proxy keys into an existing/absent env file while
/// preserving every unrelated line.
pub fn sync_proxy_at(path: &Path, proxy_url: &str) -> Result<(), Error> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }

    let mut kept: Vec<String> = Vec::new();
    if path.exists() {
        let text = std::fs::read_to_string(path)?;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                kept.push(line.to_string());
                continue;
            }
            let key = trimmed.split('=').next().unwrap_or("").trim();
            if !PROXY_KEYS.contains(&key) {
                kept.push(line.to_string());
            }
        }
    }

    if !kept.is_empty() && !kept.last().is_some_and(|l| l.is_empty()) {
        kept.push(String::new());
    }
    kept.push("# --- proxy injected by chatgpt-launcher ---".to_string());
    for (k, v) in proxy_lines(proxy_url) {
        kept.push(format!("{k}={v}"));
    }

    let mut text = kept.join("\n");
    text.push('\n');
    std::fs::write(path, text)?;
    Ok(())
}

fn proxy_lines(proxy_url: &str) -> Vec<(&'static str, String)> {
    vec![
        ("HTTPS_PROXY", proxy_url.to_string()),
        ("HTTP_PROXY", proxy_url.to_string()),
        ("ALL_PROXY", proxy_url.to_string()),
        ("NO_PROXY", "localhost,127.0.0.1,::1".to_string()),
        ("WS_PROXY", proxy_url.to_string()),
        ("WSS_PROXY", proxy_url.to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A unique per-test directory under the system temp dir. Tests must only
    /// touch this directory — never the temp dir itself.
    fn tmp_root() -> PathBuf {
        std::env::temp_dir().join(format!("chatgpt-launcher-test-{}", uuid::Uuid::new_v4()))
    }

    fn cleanup(root: &Path) {
        // Safety guard: only remove our own per-test directory, never the temp dir.
        let is_ours = root
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("chatgpt-launcher-test-"));
        if is_ours {
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn writes_proxy_keys_to_new_file() {
        let root = tmp_root();
        let path = root.join(".codex").join(".env");
        sync_proxy_at(&path, "socks5://127.0.0.1:7890").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("HTTPS_PROXY=socks5://127.0.0.1:7890"));
        assert!(text.contains("ALL_PROXY=socks5://127.0.0.1:7890"));
        assert!(text.contains("NO_PROXY=localhost,127.0.0.1,::1"));
        cleanup(&root);
    }

    #[test]
    fn preserves_unrelated_lines_and_replaces_proxy() {
        let root = tmp_root();
        let path = root.join(".codex").join(".env");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "OPENAI_API_KEY=sk-test\nHTTP_PROXY=old\n# comment\n").unwrap();

        sync_proxy_at(&path, "http://127.0.0.1:8080").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();

        assert!(text.contains("OPENAI_API_KEY=sk-test"), "text: {text}");
        assert!(text.contains("# comment"), "text: {text}");
        assert!(
            text.contains("HTTPS_PROXY=http://127.0.0.1:8080"),
            "text: {text}"
        );
        assert!(!text.contains("HTTP_PROXY=old"), "text: {text}");
        cleanup(&root);
    }

    #[test]
    fn strips_stale_lowercase_proxy_keys() {
        let root = tmp_root();
        let path = root.join(".codex").join(".env");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "http_proxy=old-lower\nKEEP=1\n").unwrap();

        sync_proxy_at(&path, "http://127.0.0.1:8080").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("http_proxy"), "text: {text}");
        assert!(text.contains("KEEP=1"), "text: {text}");
        cleanup(&root);
    }
}
