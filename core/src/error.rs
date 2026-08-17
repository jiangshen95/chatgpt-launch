use std::fmt;

/// Unified error type for the core library.
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Json(serde_json::Error),
    Reqwest(reqwest::Error),
    NotFound(String),
    AppNotFound(String),
    InvalidProxy(String),
    Platform(String),
    Other(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "IO 错误: {e}"),
            Error::Json(e) => write!(f, "JSON 错误: {e}"),
            Error::Reqwest(e) => write!(f, "网络错误: {e}"),
            Error::NotFound(id) => write!(f, "未找到配置: {id}"),
            Error::AppNotFound(msg) => write!(f, "{msg}"),
            Error::InvalidProxy(msg) => write!(f, "代理无效: {msg}"),
            Error::Platform(msg) => write!(f, "平台错误: {msg}"),
            Error::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Json(e) => Some(e),
            Error::Reqwest(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Reqwest(e)
    }
}
