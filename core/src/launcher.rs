use crate::error::Error;
use crate::geo;
use crate::model::{ExitInfo, LaunchResult, Profile};
use crate::platform;
use std::process::Command;

/// Remote-debugging port used by diagnostic mode. Bound to 127.0.0.1 only.
pub const DEBUG_PORT: u16 = 9224;

pub struct Resolved {
    pub timezone: Option<String>,
    pub language: Option<String>,
    pub exit_info: Option<ExitInfo>,
}

/// Resolve the concrete timezone/language for a profile. Each is only resolved
/// when the corresponding injection toggle is on; when a field is set to "auto"
/// (empty), a neutral exit-node lookup is performed *through the proxy*. Lookup
/// failures degrade gracefully (the field stays unset) instead of blocking launch.
pub fn resolve(profile: &Profile) -> Result<Resolved, Error> {
    let want_tz = profile.injection.inject_timezone;
    let want_lang = profile.injection.inject_language;

    let need_tz = want_tz && profile.timezone.as_deref().unwrap_or("").is_empty();
    let need_lang = want_lang && profile.language.as_deref().unwrap_or("").is_empty();

    let exit_info = if need_tz || need_lang {
        geo::lookup_exit(&profile.proxy).ok()
    } else {
        None
    };

    let timezone = if want_tz {
        match &profile.timezone {
            Some(tz) if !tz.is_empty() => Some(tz.clone()),
            _ => exit_info
                .as_ref()
                .map(|e| e.timezone.clone())
                .filter(|t| !t.is_empty()),
        }
    } else {
        None
    };

    let language = if want_lang {
        match &profile.language {
            Some(l) if !l.is_empty() => Some(l.clone()),
            _ => exit_info
                .as_ref()
                .and_then(|e| geo::language_for_country(&e.country)),
        }
    } else {
        None
    };

    Ok(Resolved {
        timezone,
        language,
        exit_info,
    })
}

pub fn launch(profile: &Profile, diagnostic_mode: bool) -> Result<LaunchResult, Error> {
    let app_path = match profile.app_path.as_deref().map(str::trim) {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => platform::detect_app()?.path.ok_or_else(|| {
            Error::AppNotFound("未找到 ChatGPT 桌面版，请在配置中手动指定应用可执行文件路径".into())
        })?,
    };

    let resolved = resolve(profile)?;
    let proxy_url = profile.proxy.url();

    // Keep Codex CLI on the same proxy by syncing its env file. Best-effort:
    // a failure here must not block the app launch.
    let (codex_env_synced, codex_env_note) = if profile.injection.sync_codex_env {
        match crate::codex_env::sync_proxy(&proxy_url) {
            Ok(()) => (true, None),
            Err(e) => (false, Some(format!("同步 ~/.codex/.env 失败: {e}"))),
        }
    } else {
        (false, None)
    };

    let mut cmd = Command::new(&app_path);
    for (k, v) in build_env(
        &proxy_url,
        resolved.timezone.as_deref(),
        resolved.language.as_deref(),
    ) {
        cmd.env(k, v);
    }
    for arg in build_args(
        &proxy_url,
        resolved.timezone.as_deref(),
        resolved.language.as_deref(),
        diagnostic_mode,
        profile.injection.leak_protection,
    ) {
        cmd.arg(arg);
    }

    let child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied && is_windowsapps_path(&app_path) {
            Error::Other(format!(
                "启动失败：ChatGPT（Microsoft Store 版）位于受保护的 WindowsApps 目录，当前权限无法直接启动。\n\
                 请以管理员身份运行本工具后重试（管理员可正常注入代理/时区/语言）。\n\
                 底层错误: {e}"
            ))
        } else {
            Error::Other(format!("启动失败 ({app_path}): {e}"))
        }
    })?;
    let pid = child.id();

    Ok(LaunchResult {
        pid,
        app_path,
        proxy_url,
        timezone: resolved.timezone,
        language: resolved.language,
        diagnostic_mode,
        debug_port: diagnostic_mode.then_some(DEBUG_PORT),
        exit_info: resolved.exit_info,
        codex_env_synced,
        codex_env_note,
    })
}

/// Microsoft Store 版应用位于受保护的 WindowsApps 目录，普通权限直接 CreateProcess
/// 会被拒绝（ERROR_ACCESS_DENIED）；以管理员身份运行可绕过，并正常注入环境变量/参数。
fn is_windowsapps_path(path: &str) -> bool {
    path.to_ascii_lowercase().contains("windowsapps")
}

fn build_env(
    proxy_url: &str,
    timezone: Option<&str>,
    language: Option<&str>,
) -> Vec<(String, String)> {
    let mut env = vec![
        ("HTTPS_PROXY".to_string(), proxy_url.to_string()),
        ("HTTP_PROXY".to_string(), proxy_url.to_string()),
        ("ALL_PROXY".to_string(), proxy_url.to_string()),
        (
            "NO_PROXY".to_string(),
            "localhost,127.0.0.1,::1".to_string(),
        ),
    ];
    if let Some(tz) = timezone {
        env.push(("TZ".to_string(), tz.to_string()));
    }
    if let Some(lang) = language {
        env.push(("LANG".to_string(), lang.to_string()));
        env.push(("LC_ALL".to_string(), lang.to_string()));
    }
    env
}

#[cfg_attr(not(target_os = "windows"), allow(unused_variables))]
fn build_args(
    proxy_url: &str,
    timezone: Option<&str>,
    language: Option<&str>,
    diagnostic_mode: bool,
    leak_protection: bool,
) -> Vec<String> {
    let mut args = vec![format!("--proxy-server={proxy_url}")];
    if leak_protection {
        // Prevent real-IP / DNS leakage around the proxy:
        // - WebRTC must not use non-proxied UDP; otherwise an `RTCPeerConnection`
        //   reveals the host's real public IP via ICE candidates to any page.
        //   Set both the default and the forced policy — some Chromium builds
        //   only honour one of the two.
        // - Disable QUIC (HTTP/3 UDP) so it cannot bypass the TCP proxy and leak
        //   the real network path / resolver.
        args.push("--webrtc-ip-handling-policy=disable_non_proxied_udp".to_string());
        args.push("--force-webrtc-ip-handling-policy=disable_non_proxied_udp".to_string());
        args.push("--disable-quic".to_string());
    }
    if let Some(lang) = language {
        args.push(format!("--lang={lang}"));
        // `--lang` only sets the UI locale; `navigator.language` and the request
        // `Accept-Language` header follow the accept-languages list, which on
        // Windows otherwise falls back to the OS locale. Setting both keeps the
        // visible locale and the HTTP language header consistent with the exit.
        args.push(format!("--accept-lang={lang}"));
    }
    // Chromium on Windows ignores the TZ environment variable (Chromium issue 40200249),
    // so pass the timezone as an explicit Chromium switch as well. On other platforms the
    // TZ env var is honored, and this switch is not needed.
    #[cfg(target_os = "windows")]
    if let Some(tz) = timezone {
        args.push(format!("--timezone-for-testing={tz}"));
    }
    if diagnostic_mode {
        args.push(format!("--remote-debugging-port={DEBUG_PORT}"));
        args.push("--remote-debugging-address=127.0.0.1".to_string());
        // Chromium 111+ rejects DevTools WebSocket connections whose Origin is not
        // explicitly allowed; the CDP probe needs this to connect programmatically.
        args.push("--remote-allow-origins=*".to_string());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Profile, ProxyConfig, ProxyProtocol};

    fn p() -> Profile {
        Profile::new(
            "t".into(),
            ProxyConfig {
                protocol: ProxyProtocol::Socks5,
                host: "127.0.0.1".into(),
                port: 7890,
                username: None,
                password: None,
            },
        )
    }

    #[test]
    fn env_includes_proxy_and_no_proxy() {
        let env = build_env("socks5://127.0.0.1:7890", None, None);
        assert!(env
            .iter()
            .any(|(k, v)| k == "HTTPS_PROXY" && v == "socks5://127.0.0.1:7890"));
        assert!(env.iter().any(|(k, _)| k == "NO_PROXY"));
        assert!(!env.iter().any(|(k, _)| k == "TZ"));
    }

    #[test]
    fn env_includes_tz_and_lang() {
        let env = build_env("socks5://h:1", Some("America/Los_Angeles"), Some("en-US"));
        assert!(env
            .iter()
            .any(|(k, v)| k == "TZ" && v == "America/Los_Angeles"));
        assert!(env.iter().any(|(k, v)| k == "LANG" && v == "en-US"));
        assert!(env.iter().any(|(k, v)| k == "LC_ALL" && v == "en-US"));
    }

    #[test]
    fn args_include_proxy_and_lang() {
        let args = build_args("socks5://127.0.0.1:7890", None, Some("en-US"), false, true);
        assert!(args.contains(&"--proxy-server=socks5://127.0.0.1:7890".to_string()));
        assert!(args.contains(&"--lang=en-US".to_string()));
        assert!(args.contains(&"--accept-lang=en-US".to_string()));
        assert!(!args.iter().any(|a| a.contains("remote-debugging")));
    }

    #[test]
    fn args_include_leak_prevention_flags() {
        let args = build_args("socks5://127.0.0.1:7890", None, None, false, true);
        assert!(args.contains(&"--webrtc-ip-handling-policy=disable_non_proxied_udp".to_string()));
        assert!(
            args.contains(&"--force-webrtc-ip-handling-policy=disable_non_proxied_udp".to_string())
        );
        assert!(args.contains(&"--disable-quic".to_string()));
    }

    #[test]
    fn args_omit_leak_prevention_when_disabled() {
        let args = build_args("socks5://127.0.0.1:7890", None, None, false, false);
        assert!(!args.iter().any(|a| a.contains("webrtc")));
        assert!(!args.iter().any(|a| a.contains("quic")));
    }

    #[test]
    fn args_add_debug_port_and_allow_origins_when_diagnostic() {
        let args = build_args("socks5://127.0.0.1:7890", None, None, true, true);
        assert!(args.contains(&format!("--remote-debugging-port={DEBUG_PORT}")));
        assert!(args.contains(&"--remote-debugging-address=127.0.0.1".to_string()));
        assert!(args.contains(&"--remote-allow-origins=*".to_string()));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn args_add_timezone_switch_on_windows() {
        let args = build_args(
            "socks5://127.0.0.1:7890",
            Some("America/Los_Angeles"),
            None,
            false,
            true,
        );
        assert!(args.contains(&"--timezone-for-testing=America/Los_Angeles".to_string()));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn args_omit_timezone_switch_off_windows() {
        let args = build_args(
            "socks5://127.0.0.1:7890",
            Some("America/Los_Angeles"),
            None,
            false,
            true,
        );
        assert!(!args.iter().any(|a| a.contains("timezone-for-testing")));
    }

    #[test]
    fn resolve_keeps_fixed_values_without_network() {
        let mut profile = p();
        profile.timezone = Some("America/New_York".into());
        profile.language = Some("en-US".into());
        profile.injection.inject_timezone = true;
        profile.injection.inject_language = true;
        let r = resolve(&profile).unwrap();
        assert_eq!(r.timezone.as_deref(), Some("America/New_York"));
        assert_eq!(r.language.as_deref(), Some("en-US"));
        assert!(r.exit_info.is_none());
    }

    #[test]
    fn resolve_skips_injection_when_disabled() {
        let mut profile = p();
        profile.timezone = Some("America/New_York".into());
        profile.language = Some("en-US".into());
        // injection defaults to off
        let r = resolve(&profile).unwrap();
        assert!(r.timezone.is_none());
        assert!(r.language.is_none());
        assert!(r.exit_info.is_none());
    }
}
