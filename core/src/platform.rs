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

/// Best-effort observation of the launched app's open sockets, including its
/// child processes (Electron/Chromium apps keep their network sockets in
/// renderer/GPU/utility processes, not in the main process).
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

    /// BFS over the process tree starting at `pid`, returning each `(pid, name)`.
    fn related_processes(pid: u32) -> Vec<(u32, String)> {
        let mut seen: Vec<u32> = Vec::new();
        let mut queue: Vec<u32> = vec![pid];
        let mut out: Vec<(u32, String)> = Vec::new();
        while let Some(p) = queue.pop() {
            if seen.contains(&p) {
                continue;
            }
            seen.push(p);
            if let Some(name) = process_name(p) {
                out.push((p, name));
            }
            if let Ok(o) = Command::new("pgrep").args(["-P", &p.to_string()]).output() {
                for line in String::from_utf8_lossy(&o.stdout).lines() {
                    if let Ok(c) = line.trim().parse::<u32>() {
                        queue.push(c);
                    }
                }
            }
        }
        out
    }

    fn process_name(pid: u32) -> Option<String> {
        let out = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output()
            .ok()?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }

    pub fn observe(pid: u32) -> Result<Vec<ConnectionInfo>, Error> {
        let mut conns = Vec::new();
        for (p, name) in related_processes(pid) {
            let out = Command::new("lsof")
                .args(["-nP", "-i", "-a", "-p", &p.to_string()])
                .output()
                .map_err(|e| Error::Platform(format!("lsof 不可用: {e}")))?;

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
                        pid: Some(p),
                        process: Some(name.clone()),
                    });
                }
            }
        }
        Ok(conns)
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::collections::HashMap;

    pub fn detect() -> Result<AppDetection, Error> {
        // 1) 常规安装路径（非 Store 版本）。
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

        // 2) Microsoft Store（MSIX）版本：官方 Windows 版经 Store 安装，位于受保护的
        //    C:\Program Files\WindowsApps\ 下，只能通过包管理器（Get-AppxPackage）发现。
        if let Some(p) = store_exe_path() {
            return Ok(AppDetection {
                path: Some(p),
                source: "Microsoft Store".into(),
                message: "已找到（Microsoft Store 版）。其位于受保护的 WindowsApps 目录：若启动提示权限不足，请以管理员身份运行本工具".into(),
            });
        }

        Ok(AppDetection {
            path: None,
            source: "windows".into(),
            message: "未找到 ChatGPT / Codex（已检查标准路径与 Microsoft Store 安装），请在配置中手动指定应用路径".into(),
        })
    }

    /// 通过 PowerShell 包管理器发现 Microsoft Store 版 ChatGPT/Codex 的可执行文件。
    /// `Get-AppxPackage` 读取注册表、`Get-AppxPackageManifest` 读取包清单（均无需直接
    /// 访问受保护的 WindowsApps 目录），从而拿到权威的可执行文件名——覆盖 exe 位于
    /// `app\` 子目录、以及包名为 `OpenAI.Codex` 等非 "ChatGPT" 命名的情况。
    fn store_exe_path() -> Option<String> {
        let script = concat!(
            "$ErrorActionPreference='SilentlyContinue';",
            "Get-AppxPackage | Where-Object { $_.Name -match 'OpenAI|ChatGPT|Codex' } | ForEach-Object {",
            "  $pkg = $_;",
            "  $m = Get-AppxPackageManifest -Package $pkg;",
            "  $app = @($m.Package.Applications.Application)[0];",
            "  if ($app -and $app.Executable) { Write-Output ('PATH:' + (Join-Path $pkg.InstallLocation $app.Executable)) }",
            "  else {",
            "    $exe = Get-ChildItem -LiteralPath $pkg.InstallLocation -Recurse -Filter *.exe -ErrorAction SilentlyContinue | Select-Object -First 1;",
            "    if ($exe) { Write-Output ('PATH:' + $exe.FullName) }",
            "  }",
            "}",
        );

        let out = run_powershell(script)?;
        for line in out.lines() {
            let line = line.trim();
            if let Some(p) = line.strip_prefix("PATH:") {
                let p = p.trim().to_string();
                if !p.is_empty() {
                    return Some(p);
                }
            }
        }
        None
    }

    fn run_powershell(script: &str) -> Option<String> {
        let out = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Electron/Chromium 的主进程与 renderer/GPU/utility 子进程共用同一可执行文件名，
    /// 因此按进程名聚合即可覆盖整棵进程树。
    fn related_processes(pid: u32) -> Vec<(u32, String)> {
        let name = process_name(pid).unwrap_or_default();
        let mut pids: Vec<u32> = if name.is_empty() {
            Vec::new()
        } else {
            let script = format!(
                "Get-Process -Name '{name}' -ErrorAction SilentlyContinue | ForEach-Object {{ Write-Output $_.Id }}"
            );
            run_powershell(&script)
                .map(|out| {
                    out.lines()
                        .filter_map(|l| l.trim().parse::<u32>().ok())
                        .collect::<Vec<u32>>()
                })
                .unwrap_or_default()
        };
        if !pids.contains(&pid) {
            pids.push(pid);
        }
        let label = if name.is_empty() {
            "<unknown>".to_string()
        } else {
            name
        };
        pids.into_iter().map(|p| (p, label.clone())).collect()
    }

    fn process_name(pid: u32) -> Option<String> {
        let script = format!("(Get-Process -Id {pid} -ErrorAction SilentlyContinue).ProcessName");
        let out = run_powershell(&script)?;
        out.lines()
            .map(str::trim)
            .find(|s| !s.is_empty())
            .map(String::from)
    }

    pub fn observe(pid: u32) -> Result<Vec<ConnectionInfo>, Error> {
        let procs = related_processes(pid);
        let by_pid: HashMap<u32, String> = procs.into_iter().collect();

        let out = Command::new("netstat")
            .args(["-ano"])
            .output()
            .map_err(|e| Error::Platform(format!("netstat 不可用: {e}")))?;

        let mut conns = Vec::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            // Proto, Local, Foreign, State, PID (TCP only — UDP lines have no State).
            if fields.len() < 5 {
                continue;
            }
            let Ok(p) = fields[4].parse::<u32>() else {
                continue;
            };
            let Some(name) = by_pid.get(&p) else {
                continue;
            };
            conns.push(ConnectionInfo {
                local: fields[1].to_string(),
                remote: fields[2].to_string(),
                state: fields[3].to_string(),
                pid: Some(p),
                process: Some(name.clone()),
            });
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

    /// BFS over the process tree starting at `pid`, returning each `(pid, name)`.
    fn related_processes(pid: u32) -> Vec<(u32, String)> {
        let mut seen: Vec<u32> = Vec::new();
        let mut queue: Vec<u32> = vec![pid];
        let mut out: Vec<(u32, String)> = Vec::new();
        while let Some(p) = queue.pop() {
            if seen.contains(&p) {
                continue;
            }
            seen.push(p);
            if let Some(name) = process_name(p) {
                out.push((p, name));
            }
            if let Ok(o) = Command::new("pgrep").args(["-P", &p.to_string()]).output() {
                for line in String::from_utf8_lossy(&o.stdout).lines() {
                    if let Ok(c) = line.trim().parse::<u32>() {
                        queue.push(c);
                    }
                }
            }
        }
        out
    }

    fn process_name(pid: u32) -> Option<String> {
        let out = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output()
            .ok()?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }

    /// Parse the `users:(("name",pid=N,fd=M))` field of `ss -tnp`.
    pub(super) fn parse_ss_process(field: &str) -> Option<(String, u32)> {
        let i = field.find("users:((\"")?;
        let rest = &field[i + "users:((\"".len()..];
        let name_end = rest.find('"')?;
        let name = rest[..name_end].to_string();
        let after = &rest[name_end..];
        let pid_at = after.find("pid=")?;
        let after_pid = &after[pid_at + 4..];
        let end = after_pid
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after_pid.len());
        let p: u32 = after_pid[..end].parse().ok()?;
        Some((name, p))
    }

    pub fn observe(pid: u32) -> Result<Vec<ConnectionInfo>, Error> {
        let procs = related_processes(pid);
        let pids: Vec<u32> = procs.iter().map(|(p, _)| *p).collect();

        let out = Command::new("ss")
            .args(["-tnp"])
            .output()
            .map_err(|e| Error::Platform(format!("ss 不可用: {e}")))?;

        let mut conns = Vec::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            // State, Recv-Q, Send-Q, Local, Peer, Process...
            if fields.len() < 6 {
                continue;
            }
            let proc_field = fields
                .iter()
                .rev()
                .find(|f| f.contains("users:(("))
                .cloned();
            let Some((name, p)) = proc_field.and_then(parse_ss_process) else {
                continue;
            };
            if !pids.contains(&p) {
                continue;
            }
            conns.push(ConnectionInfo {
                local: fields[3].to_string(),
                remote: fields[4].to_string(),
                state: fields[0].to_string(),
                pid: Some(p),
                process: Some(name),
            });
        }
        Ok(conns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_in_use_detects_loopback_target() {
        let conns = vec![
            ConnectionInfo {
                local: "127.0.0.1:54321".into(),
                remote: "127.0.0.1:7890".into(),
                state: "ESTABLISHED".into(),
                pid: Some(1),
                process: Some("ChatGPT".into()),
            },
            ConnectionInfo {
                local: "127.0.0.1:54322".into(),
                remote: "1.2.3.4:443".into(),
                state: "ESTABLISHED".into(),
                pid: Some(2),
                process: Some("ChatGPT".into()),
            },
        ];
        assert!(proxy_in_use(&conns, 7890));
        assert!(!proxy_in_use(&conns, 1080));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parse_ss_process_field() {
        let (name, pid) = linux::parse_ss_process(r#"users:(("ChatGPT",pid=1234,fd=45))"#).unwrap();
        assert_eq!(name, "ChatGPT");
        assert_eq!(pid, 1234);
    }
}
