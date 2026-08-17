use serde::{Deserialize, Serialize};

/// Proxy protocol choices exposed to the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyProtocol {
    Socks5,
    Http,
    Https,
}

impl ProxyProtocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProxyProtocol::Socks5 => "socks5",
            ProxyProtocol::Http => "http",
            ProxyProtocol::Https => "https",
        }
    }

    pub fn default_port(&self) -> u16 {
        match self {
            ProxyProtocol::Socks5 => 1080,
            ProxyProtocol::Http => 8080,
            ProxyProtocol::Https => 8443,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConfig {
    pub protocol: ProxyProtocol,
    pub host: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

impl ProxyConfig {
    /// Full proxy URL with credentials embedded, suitable for process args / env vars.
    pub fn url(&self) -> String {
        let auth = match (&self.username, &self.password) {
            (Some(u), Some(p)) => format!("{u}:{p}@"),
            (Some(u), None) => format!("{u}@"),
            _ => String::new(),
        };
        format!(
            "{}://{}{}:{}",
            self.protocol.as_str(),
            auth,
            self.host,
            self.port
        )
    }

    /// Proxy URL without credentials (for reqwest `Proxy::all` + `basic_auth`).
    pub fn url_no_auth(&self) -> String {
        format!("{}://{}:{}", self.protocol.as_str(), self.host, self.port)
    }
}

/// Telemetry toggles. Reserved for a future release — stored but NOT applied yet.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryToggles {
    #[serde(default)]
    pub disable_sentry: bool,
    #[serde(default)]
    pub disable_statsig: bool,
    #[serde(default)]
    pub disable_sparkle: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// Empty id means "create new" when coming from the UI.
    pub id: String,
    pub name: String,
    pub proxy: ProxyConfig,
    /// IANA timezone, e.g. `America/Los_Angeles`. `None` = auto-detect via proxy at launch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    /// BCP-47 locale, e.g. `en-US`. `None` = auto-detect via proxy at launch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default)]
    pub telemetry: TelemetryToggles,
    /// Manual override of the ChatGPT binary path. `None` = auto-detect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_path: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Profile {
    pub fn new(name: String, proxy: ProxyConfig) -> Self {
        let now = now_ms();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            proxy,
            timezone: None,
            language: None,
            telemetry: TelemetryToggles::default(),
            app_path: None,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Geolocation / exit-node information learned from a neutral endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitInfo {
    pub ip: String,
    pub country: String,
    pub region: String,
    pub city: String,
    pub timezone: String,
    pub source: String,
}

/// Result of a connection test, including risk-consistency hints.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestReport {
    pub exit: ExitInfo,
    pub consistency: ConsistencyResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsistencyResult {
    pub ok: bool,
    pub warnings: Vec<String>,
}

/// A single observed socket connection of the launched app.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub local: String,
    pub remote: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResult {
    pub pid: u32,
    pub app_path: String,
    pub proxy_url: String,
    pub timezone: Option<String>,
    pub language: Option<String>,
    pub diagnostic_mode: bool,
    pub debug_port: Option<u16>,
    pub exit_info: Option<ExitInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppDetection {
    pub path: Option<String>,
    pub source: String,
    pub message: String,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
