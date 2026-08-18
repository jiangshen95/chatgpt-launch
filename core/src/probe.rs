//! CDP (Chrome DevTools Protocol) probe against a ChatGPT desktop app launched
//! in diagnostic mode (`--remote-debugging-port`).
//!
//! The probe only talks to the loopback debugging endpoint — never to OpenAI.
//! It reads what the app *itself* actually sees:
//!
//! * the renderer's timezone (`Intl.DateTimeFormat().resolvedOptions().timeZone`),
//! * the renderer's language (`navigator.language`) and local time,
//! * the app's real egress IP/geo by navigating a throwaway target to a neutral
//!   echo endpoint (`ipinfo.io/json`) through the app's own network stack.

use crate::error::Error;
use crate::model::ExitInfo;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// A single debuggable target exposed by the CDP HTTP endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CdpTarget {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub url: String,
    pub ws_url: String,
}

/// Result of probing a running (diagnostic-mode) ChatGPT app.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppProbe {
    /// Whether the CDP HTTP endpoint responded at all.
    pub reachable: bool,
    /// Browser/Electron version string, when available.
    pub browser: Option<String>,
    /// Debuggable targets currently exposed.
    pub targets: Vec<CdpTarget>,
    /// Timezone the renderer actually reports.
    pub timezone: Option<String>,
    /// Language the renderer actually reports.
    pub language: Option<String>,
    /// The renderer's own local time string (`new Date().toString()`).
    pub local_time: Option<String>,
    /// Actual egress observed through the app's network stack.
    pub exit: Option<ExitInfo>,
    /// Non-fatal diagnostics / caveats.
    pub hints: Vec<String>,
}

const PROBE_ENDPOINT: &str = "https://ipinfo.io/json";

/// Single round-trip snapshot of what the renderer reports. Returned as a JSON
/// string so a partial failure (e.g. `navigator` absent in a worker) still yields
/// the fields that are available.
const SNAPSHOT_EXPR: &str = r#"(() => {
  try {
    return JSON.stringify({
      tz: Intl.DateTimeFormat().resolvedOptions().timeZone || "",
      lang: (typeof navigator !== "undefined" && navigator.language) || "",
      now: new Date().toString()
    });
  } catch (e) {
    return JSON.stringify({ error: String(e) });
  }
})()"#;

#[derive(Debug, Default)]
struct Snapshot {
    timezone: Option<String>,
    language: Option<String>,
    local_time: Option<String>,
}

fn parse_snapshot(v: &serde_json::Value) -> Option<Snapshot> {
    let s = v.as_str()?;
    let j: serde_json::Value = serde_json::from_str(s).ok()?;
    if j.get("error").is_some() {
        return None;
    }
    let get = |k: &str| {
        j.get(k)
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from)
    };
    Some(Snapshot {
        timezone: get("tz"),
        language: get("lang"),
        local_time: get("now"),
    })
}

/// Read the renderer snapshot from any page target (falling back to any target
/// that exposes a WebSocket URL). Timezone is process-wide, so any renderer in
/// the same app is a valid source.
fn read_snapshot(targets: &[CdpTarget]) -> Result<Snapshot, String> {
    let mut candidates: Vec<&CdpTarget> = targets.iter().filter(|t| t.kind == "page").collect();
    if candidates.is_empty() {
        candidates = targets.iter().filter(|t| !t.ws_url.is_empty()).collect();
    }
    if candidates.is_empty() {
        return Err("未找到可用的 CDP target（页面可能尚未加载，可稍后重试）".to_string());
    }

    let mut last_err = "目标未返回有效快照".to_string();
    for t in candidates {
        let mut session = match CdpSession::connect(&t.ws_url) {
            Ok(s) => s,
            Err(e) => {
                last_err = e;
                continue;
            }
        };
        // The renderer's execution context may not be ready immediately after
        // launch; give each target a few short attempts before moving on.
        for _ in 0..3 {
            match session.eval(SNAPSHOT_EXPR) {
                Ok(v) => {
                    if let Some(snap) = parse_snapshot(&v) {
                        return Ok(snap);
                    }
                    last_err = format!("目标「{}」未返回有效快照", t.url);
                }
                Err(e) => last_err = e,
            }
            std::thread::sleep(Duration::from_millis(300));
        }
    }
    Err(last_err)
}

/// Run the full probe against a ChatGPT app exposing CDP on `port` (loopback).
pub fn probe(port: u16) -> Result<AppProbe, Error> {
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    let hints = Vec::new();

    let (reachable, browser) = match client.get(format!("{base}/json/version")).send() {
        Ok(resp) if resp.status().is_success() => {
            let v: serde_json::Value = resp
                .text()
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or(serde_json::Value::Null);
            let browser = v.get("Browser").and_then(|x| x.as_str()).map(String::from);
            (true, browser)
        }
        _ => (false, None),
    };

    let targets = match client.get(format!("{base}/json/list")).send() {
        Ok(resp) if resp.status().is_success() => resp
            .text()
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .map(|v| parse_targets(&v))
            .unwrap_or_default(),
        _ => Vec::new(),
    };

    let mut out = AppProbe {
        reachable,
        browser,
        targets: targets.clone(),
        timezone: None,
        language: None,
        local_time: None,
        exit: None,
        hints,
    };

    if !reachable {
        out.hints.push(format!(
            "无法访问 127.0.0.1:{port}：请确认已勾选「诊断模式」并成功启动，且 ChatGPT 进程仍在运行"
        ));
        return Ok(out);
    }

    // 1) Read the renderer's own timezone / language / local time. The main
    // window may be exposed under a non-obvious URL (or an empty one while it
    // is still loading), so probe every page target instead of guessing its URL.
    match read_snapshot(&targets) {
        Ok(s) => {
            out.timezone = s.timezone;
            out.language = s.language;
            out.local_time = s.local_time;
        }
        Err(e) => out
            .hints
            .push(format!("未能读取应用内时区/语言/本地时间：{e}")),
    }

    // 2) Actual egress through the app's own network stack.
    match probe_exit(&client, &base) {
        Ok(exit) => out.exit = Some(exit),
        Err(e) => out.hints.push(format!("实测出口失败: {e}")),
    }

    Ok(out)
}

/// Create a throwaway CDP target navigated to a neutral echo endpoint, read the
/// JSON body, and parse it. The request goes through the app's real network
/// stack (proxy included), so it reflects the app's actual egress.
fn probe_exit(client: &reqwest::blocking::Client, base: &str) -> Result<ExitInfo, String> {
    let resp = client
        .put(format!("{base}/json/new?{PROBE_ENDPOINT}"))
        .send()
        .map_err(|e| format!("创建探针 target 失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("创建探针 target 失败: HTTP {}", resp.status()));
    }
    let text = resp.text().map_err(|e| format!("读取 target 失败: {e}"))?;
    let t: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("解析 target 失败: {e}"))?;
    let id = t
        .get("id")
        .and_then(|x| x.as_str())
        .ok_or("target 响应缺少 id")?
        .to_string();
    let ws_url = t
        .get("webSocketDebuggerUrl")
        .and_then(|x| x.as_str())
        .ok_or("target 响应缺少 webSocketDebuggerUrl")?
        .to_string();

    let body = poll_body(&ws_url, Duration::from_secs(12))?;
    // Best-effort cleanup of the throwaway target.
    let _ = client.get(format!("{base}/json/close/{id}")).send();

    parse_ipinfo(&body)
}

fn poll_body(ws_url: &str, deadline: Duration) -> Result<String, String> {
    let mut session = CdpSession::connect(ws_url)?;
    let start = Instant::now();
    let mut last_err = String::new();
    while start.elapsed() < deadline {
        match session.eval("document.body ? (document.body.innerText || '') : ''") {
            Ok(v) if v.as_str().is_some_and(|s| !s.trim().is_empty()) => {
                return Ok(v.as_str().unwrap().to_string());
            }
            Ok(_) => {
                std::thread::sleep(Duration::from_millis(400));
            }
            Err(e) => {
                last_err = e;
                std::thread::sleep(Duration::from_millis(400));
            }
        }
    }
    Err(format!("页面加载超时: {last_err}"))
}

fn parse_targets(v: &serde_json::Value) -> Vec<CdpTarget> {
    let Some(arr) = v.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|t| {
            Some(CdpTarget {
                id: t.get("id")?.as_str()?.to_string(),
                kind: t.get("type")?.as_str().unwrap_or("").to_string(),
                title: t.get("title")?.as_str().unwrap_or("").to_string(),
                url: t.get("url")?.as_str().unwrap_or("").to_string(),
                ws_url: t
                    .get("webSocketDebuggerUrl")?
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect()
}

fn parse_ipinfo(text: &str) -> Result<ExitInfo, String> {
    let v: serde_json::Value =
        serde_json::from_str(text.trim()).map_err(|e| format!("ipinfo 返回非 JSON: {e}"))?;
    let get = |keys: &[&str]| -> String {
        keys.iter()
            .find_map(|k| {
                v.get(k)
                    .and_then(|x| x.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default()
    };
    Ok(ExitInfo {
        ip: get(&["ip"]),
        country: get(&["country"]),
        region: get(&["region"]),
        city: get(&["city"]),
        timezone: get(&["timezone"]),
        source: "ipinfo.io (ChatGPT 实测)".to_string(),
    })
}

/// A minimal, blocking CDP client over a single WebSocket connection.
type WsStream = tungstenite::stream::MaybeTlsStream<std::net::TcpStream>;

struct CdpSession {
    ws: tungstenite::WebSocket<WsStream>,
    next_id: u64,
}

impl CdpSession {
    fn connect(ws_url: &str) -> Result<Self, String> {
        let (mut ws, _) = tungstenite::connect(ws_url).map_err(|e| {
            let s = e.to_string();
            if s.contains("403") || s.contains("401") || s.contains("Forbidden") {
                format!(
                    "CDP WebSocket 握手被拒（{e}）：请用带 --remote-allow-origins=* 的诊断模式重启 ChatGPT"
                )
            } else {
                format!("CDP WebSocket 连接失败: {e}")
            }
        })?;
        // The probe only ever uses plain ws:// (loopback), so the stream is the
        // plain TCP variant; bound blocking reads so a hung target can't stall us.
        if let tungstenite::stream::MaybeTlsStream::Plain(sock) = ws.get_mut() {
            let _ = sock.set_read_timeout(Some(Duration::from_secs(8)));
            let _ = sock.set_write_timeout(Some(Duration::from_secs(8)));
        }
        Ok(Self { ws, next_id: 1 })
    }

    fn eval(&mut self, expression: &str) -> Result<serde_json::Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let payload = serde_json::json!({
            "id": id,
            "method": "Runtime.evaluate",
            "params": {
                "expression": expression,
                "returnByValue": true,
                "awaitPromise": true,
            },
        });
        self.ws
            .send(tungstenite::Message::Text(payload.to_string().into()))
            .map_err(|e| format!("CDP 发送失败: {e}"))?;

        loop {
            let msg = self
                .ws
                .read()
                .map_err(|e| format!("CDP 读取失败/超时: {e}"))?;
            if let tungstenite::Message::Text(t) = msg {
                let v: serde_json::Value =
                    serde_json::from_str(&*t).unwrap_or(serde_json::Value::Null);
                if v.get("id").and_then(|x| x.as_u64()) == Some(id) {
                    if let Some(err) = v.get("error") {
                        return Err(format!("CDP 错误: {err}"));
                    }
                    return Ok(v
                        .pointer("/result/result/value")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_target_list() {
        let v = serde_json::json!([
            {
                "id": "A1",
                "type": "page",
                "title": "ChatGPT",
                "url": "https://chatgpt.com/",
                "webSocketDebuggerUrl": "ws://127.0.0.1:9224/devtools/page/A1"
            }
        ]);
        let targets = parse_targets(&v);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].id, "A1");
    }

    #[test]
    fn parses_snapshot() {
        let v = serde_json::Value::String(
            r#"{"tz":"America/Los_Angeles","lang":"en-US","now":"Sat Jan 01 2022 00:00:00"}"#
                .into(),
        );
        let snap = parse_snapshot(&v).unwrap();
        assert_eq!(snap.timezone.as_deref(), Some("America/Los_Angeles"));
        assert_eq!(snap.language.as_deref(), Some("en-US"));
        assert!(snap.local_time.is_some());
    }

    #[test]
    fn snapshot_parses_partial_fields() {
        let v = serde_json::Value::String(r#"{"tz":"Asia/Tokyo"}"#.into());
        let snap = parse_snapshot(&v).unwrap();
        assert_eq!(snap.timezone.as_deref(), Some("Asia/Tokyo"));
        assert!(snap.language.is_none());
        assert!(snap.local_time.is_none());
    }

    #[test]
    fn snapshot_rejects_error_payload() {
        let v = serde_json::Value::String(r#"{"error":"boom"}"#.into());
        assert!(parse_snapshot(&v).is_none());
    }

    #[test]
    fn parses_ipinfo_json() {
        let body = r#"{"ip":"1.2.3.4","city":"Los Angeles","region":"California","country":"US","loc":"34.0,-118.0","timezone":"America/Los_Angeles"}"#;
        let exit = parse_ipinfo(body).unwrap();
        assert_eq!(exit.ip, "1.2.3.4");
        assert_eq!(exit.country, "US");
        assert_eq!(exit.timezone, "America/Los_Angeles");
        assert_eq!(exit.source, "ipinfo.io (ChatGPT 实测)");
    }
}
