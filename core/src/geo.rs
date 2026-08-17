use crate::error::Error;
use crate::model::{ExitInfo, ProxyConfig, ProxyProtocol};
use std::time::Duration;

/// Look up the exit node's geolocation by making requests *through the proxy*
/// to neutral third-party endpoints. Never touches OpenAI endpoints.
pub fn lookup_exit(proxy: &ProxyConfig) -> Result<ExitInfo, Error> {
    let client = client_for(proxy)?;

    let providers: [Provider; 3] = [
        Provider {
            url: "https://ipinfo.io/json",
            source: "ipinfo.io",
            ip_keys: &["ip"],
            country_keys: &["country"],
            region_keys: &["region"],
            city_keys: &["city"],
            timezone_keys: &["timezone"],
        },
        Provider {
            url: "https://api.ip.sb/geoip",
            source: "ip.sb",
            ip_keys: &["ip"],
            country_keys: &["country_code", "country"],
            region_keys: &["region", "region_name"],
            city_keys: &["city"],
            timezone_keys: &["timezone"],
        },
        Provider {
            url: "https://ipapi.is/json",
            source: "ipapi.is",
            ip_keys: &["ip"],
            country_keys: &["country_code", "country"],
            region_keys: &["region", "region_code"],
            city_keys: &["city"],
            timezone_keys: &["timezone"],
        },
    ];

    let mut last_err: Option<Error> = None;
    for p in &providers {
        match fetch(&client, p) {
            Ok(info) => return Ok(info),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| Error::InvalidProxy("所有出口查询服务均失败".into())))
}

/// Build an HTTP client that routes all traffic through the given proxy.
pub fn client_for(proxy: &ProxyConfig) -> Result<reqwest::blocking::Client, Error> {
    let scheme = match proxy.protocol {
        ProxyProtocol::Socks5 => "socks5",
        ProxyProtocol::Http => "http",
        ProxyProtocol::Https => "https",
    };
    let base = format!("{scheme}://{}:{}", proxy.host, proxy.port);
    let mut p = reqwest::Proxy::all(&base).map_err(|e| Error::InvalidProxy(e.to_string()))?;
    if let (Some(user), Some(pass)) = (&proxy.username, &proxy.password) {
        p = p.basic_auth(user, pass);
    }

    reqwest::blocking::Client::builder()
        .proxy(p)
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(Error::Reqwest)
}

struct Provider {
    url: &'static str,
    source: &'static str,
    ip_keys: &'static [&'static str],
    country_keys: &'static [&'static str],
    region_keys: &'static [&'static str],
    city_keys: &'static [&'static str],
    timezone_keys: &'static [&'static str],
}

fn fetch(client: &reqwest::blocking::Client, p: &Provider) -> Result<ExitInfo, Error> {
    let resp = client.get(p.url).send()?;
    if !resp.status().is_success() {
        return Err(Error::InvalidProxy(format!(
            "{} 返回 {}",
            p.source,
            resp.status()
        )));
    }
    let text = resp.text()?;
    let v: serde_json::Value = serde_json::from_str(&text)?;

    let pick = |keys: &[&str]| -> String {
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
        ip: pick(p.ip_keys),
        country: pick(p.country_keys),
        region: pick(p.region_keys),
        city: pick(p.city_keys),
        timezone: pick(p.timezone_keys),
        source: p.source.to_string(),
    })
}

/// Map a 2-letter country code to a sensible default locale. Returns `None`
/// when there is no confident mapping (callers should then leave language unset).
pub fn language_for_country(country: &str) -> Option<String> {
    let c = country.trim().to_ascii_uppercase();
    let lang = match c.as_str() {
        "US" => "en-US",
        "GB" => "en-GB",
        "CA" => "en-CA",
        "AU" => "en-AU",
        "NZ" => "en-NZ",
        "IE" => "en-IE",
        "ZA" => "en-ZA",
        "SG" => "en-SG",
        "IN" => "en-IN",
        "HK" => "zh-HK",
        "TW" => "zh-TW",
        "CN" => "zh-CN",
        "JP" => "ja-JP",
        "KR" => "ko-KR",
        "DE" => "de-DE",
        "AT" => "de-AT",
        "CH" => "de-CH",
        "FR" => "fr-FR",
        "ES" => "es-ES",
        "MX" => "es-MX",
        "AR" => "es-AR",
        "BR" => "pt-BR",
        "PT" => "pt-PT",
        "IT" => "it-IT",
        "NL" => "nl-NL",
        "SE" => "sv-SE",
        "NO" => "nb-NO",
        "DK" => "da-DK",
        "FI" => "fi-FI",
        "PL" => "pl-PL",
        "TR" => "tr-TR",
        "RU" => "ru-RU",
        _ => return None,
    };
    Some(lang.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_mapping_is_stable() {
        assert_eq!(language_for_country("US"), Some("en-US".into()));
        assert_eq!(language_for_country("us"), Some("en-US".into()));
        assert_eq!(language_for_country("JP"), Some("ja-JP".into()));
        assert_eq!(language_for_country("XX"), None);
    }
}
