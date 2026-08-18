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

    // 1) Read the renderer's own timezone / language / local time.
    if let Some(page) = targets
        .iter()
        .find(|t| t.kind == "page" && is_chat_target(t))
    {
        let mut session = match CdpSession::connect(&page.ws_url) {
            Ok(s) => s,
            Err(e) => {
                out.hints.push(e);
                return Ok(out);
            }
        };
        out.timezone = session
            .eval("Intl.DateTimeFormat().resolvedOptions().timeZone")
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .filter(|s| !s.is_empty());
        out.language = session
            .eval("navigator.language")
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .filter(|s| !s.is_empty());
        out.local_time = session
            .eval("new Date().toString()")
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .filter(|s| !s.is_empty());
    } else {
        out.hints
            .push("未找到 ChatGPT 页面 target（页面可能尚未加载，可稍后重试）".to_string());
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

fn is_chat_target(t: &CdpTarget) -> bool {
    let u = t.url.to_ascii_lowercase();
    u.contains("chatgpt") || u.contains("openai") || u.contains("chat.")
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
        assert!(is_chat_target(&targets[0]));
    }

    #[test]
    fn chat_target_detection() {
        assert!(is_chat_target(&CdpTarget {
            id: "x".into(),
            kind: "page".into(),
            title: "ChatGPT".into(),
            url: "https://chatgpt.com/".into(),
            ws_url: "".into(),
        }));
        assert!(!is_chat_target(&CdpTarget {
            id: "x".into(),
            kind: "page".into(),
            title: "other".into(),
            url: "https://example.com/".into(),
            ws_url: "".into(),
        }));
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
