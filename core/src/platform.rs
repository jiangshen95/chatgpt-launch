use crate::error::Error;
use crate::model::{AppDetection, ConnectionInfo};
use std::path::Path;
use std::process::Command;

/// Locate the installed ChatGPT desktop app for the current platform.
pub fn detect_app() -> Result<AppDetection, Error> {
    #[cfg(target_os = "macos")]
    {
        macos::detect()
    }
    #[cfg(target_os = "windows")]
    {
        windows::detect()
    }
    #[cfg(target_os = "linux")]
    {
        linux::detect()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err(Error::Platform("不支持的平台".into()))
    }
}

/// Best-effort observation of the launched app's open sockets.
pub fn observe_connections(pid: u32) -> Result<Vec<ConnectionInfo>, Error> {
    #[cfg(target_os = "macos")]
    {
        macos::observe(pid)
    }
    #[cfg(target_os = "windows")]
    {
        windows::observe(pid)
    }
    #[cfg(target_os = "linux")]
    {
        linux::observe(pid)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Err(Error::Platform("不支持的平台".into()))
    }
}

/// Whether any observed connection targets the loopback address on `port`
/// (i.e. the app is talking to a locally-running proxy).
pub fn proxy_in_use(conns: &[ConnectionInfo], port: u16) -> bool {
    let v4 = format!("127.0.0.1:{port}");
    let v6 = format!("[::1]:{port}");
    conns.iter().any(|c| {
        let remote = c.remote.trim();
        remote == v4 || remote == v6 || remote == format!("localhost:{port}")
    })
}

fn is_file(path: &str) -> bool {
    Path::new(path).is_file()
}

fn found(path: String, source: &str) -> AppDetection {
    AppDetection {
        path: Some(path),
        source: source.to_string(),
        message: "已找到".to_string(),
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    pub fn detect() -> Result<AppDetection, Error> {
        let mut candidates = vec!["/Applications/ChatGPT.app/Contents/MacOS/ChatGPT".to_string()];
        if let Ok(home) = std::env::var("HOME") {
            candidates.push(format!(
                "{home}/Applications/ChatGPT.app/Contents/MacOS/ChatGPT"
            ));
        }
        for c in &candidates {
            if is_file(c) {
                return Ok(found(c.clone(), "标准路径"));
            }
        }

        // Spotlight fallback by bundle id.
        if let Ok(out) = Command::new("mdfind")
            .args(["kMDItemCFBundleIdentifier == 'com.openai.chat'"])
            .output()
        {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let app = line.trim();
                if app.ends_with(".app") {
                    let bin = format!("{app}/Contents/MacOS/ChatGPT");
                    if is_file(&bin) {
                        return Ok(found(bin, "mdfind"));
                    }
                }
            }
        }

        Ok(AppDetection {
            path: None,
            source: "macos".into(),
            message: "未找到 ChatGPT.app，请在配置中手动指定可执行文件路径".into(),
        })
    }

    pub fn observe(pid: u32) -> Result<Vec<ConnectionInfo>, Error> {
        let out = Command::new("lsof")
            .args(["-nP", "-i", "-a", "-p", &pid.to_string()])
            .output()
            .map_err(|e| Error::Platform(format!("lsof 不可用: {e}")))?;

        let mut conns = Vec::new();
        for line in String::from_utf8_lossy(&out.stdout).lines().skip(1) {
            if let Some(idx) = line.find("->") {
                let left = &line[..idx];
                let right = &line[idx + 2..];
                let local = left.split_whitespace().next_back().unwrap_or("");
                let remote = right.split_whitespace().next().unwrap_or("");
                let state = right
                    .split('(')
                    .nth(1)
                    .and_then(|s| s.split(')').next())
                    .unwrap_or("");
                conns.push(ConnectionInfo {
                    local: local.to_string(),
                    remote: remote.to_string(),
                    state: state.to_string(),
                });
            }
        }
        Ok(conns)
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;

    pub fn detect() -> Result<AppDetection, Error> {
        let mut candidates: Vec<String> = Vec::new();
        for key in ["LOCALAPPDATA", "PROGRAMFILES", "PROGRAMFILES(X86)"] {
            if let Ok(base) = std::env::var(key) {
                candidates.push(format!("{base}\\Programs\\ChatGPT\\ChatGPT.exe"));
                candidates.push(format!("{base}\\OpenAI\\ChatGPT\\ChatGPT.exe"));
                candidates.push(format!("{base}\\ChatGPT\\ChatGPT.exe"));
            }
        }
        for c in &candidates {
            if is_file(c) {
                return Ok(found(c.clone(), "标准路径"));
            }
        }

        Ok(AppDetection {
            path: None,
            source: "windows".into(),
            message: "未找到 ChatGPT.exe，请在配置中手动指定应用路径".into(),
        })
    }

    pub fn observe(pid: u32) -> Result<Vec<ConnectionInfo>, Error> {
        let out = Command::new("netstat")
            .args(["-ano"])
            .output()
            .map_err(|e| Error::Platform(format!("netstat 不可用: {e}")))?;

        let pid_str = pid.to_string();
        let mut conns = Vec::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            // Proto, Local, Foreign, State, PID
            if fields.len() >= 5 && fields[4] == pid_str {
                conns.push(ConnectionInfo {
                    local: fields[1].to_string(),
                    remote: fields[2].to_string(),
                    state: fields[3].to_string(),
                });
            }
        }
        Ok(conns)
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;

    pub fn detect() -> Result<AppDetection, Error> {
        for name in ["chatgpt", "ChatGPT", "chatgpt-desktop"] {
            if let Ok(out) = Command::new("which").arg(name).output() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() && is_file(&p) {
                    return Ok(found(p, "which"));
                }
            }
        }

        let mut candidates: Vec<String> = Vec::new();
        if let Ok(home) = std::env::var("HOME") {
            candidates.push(format!("{home}/.local/bin/ChatGPT"));
            candidates.push(format!("{home}/.local/bin/chatgpt"));
            candidates.push(format!("{home}/Applications/ChatGPT"));
        }
        for extra in [
            "/opt/ChatGPT/ChatGPT",
            "/usr/bin/chatgpt",
            "/usr/bin/ChatGPT",
        ] {
            candidates.push(extra.to_string());
        }
        for c in &candidates {
            if is_file(c) {
                return Ok(found(c.clone(), "标准路径"));
            }
        }

        Ok(AppDetection {
            path: None,
            source: "linux".into(),
            message: "未找到 ChatGPT 可执行文件，请在配置中手动指定路径".into(),
        })
    }

    pub fn observe(pid: u32) -> Result<Vec<ConnectionInfo>, Error> {
        let out = Command::new("ss")
            .args(["-tnp"])
            .output()
            .map_err(|e| Error::Platform(format!("ss 不可用: {e}")))?;

        let needle = format!("pid={pid}");
        let mut conns = Vec::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if !line.contains(&needle) {
                continue;
            }
            let fields: Vec<&str> = line.split_whitespace().collect();
            // State, Recv-Q, Send-Q, Local, Peer, Process...
            if fields.len() >= 5 {
                conns.push(ConnectionInfo {
                    local: fields[3].to_string(),
                    remote: fields[4].to_string(),
                    state: fields[0].to_string(),
                });
            }
        }
        Ok(conns)
    }
}
