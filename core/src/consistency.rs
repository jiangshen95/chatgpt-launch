use crate::geo::language_for_country;
use crate::model::{ConsistencyResult, ExitInfo, Profile};

/// Compare a profile against the observed exit node and surface risk hints.
///
/// This is deliberately a *hint*, not an authoritative verdict: the mapping is
/// coarse (country -> timezone prefix), and unknown countries simply produce no
/// warning rather than a false positive.
pub fn check(profile: &Profile, exit: &ExitInfo) -> ConsistencyResult {
    let mut warnings = Vec::new();

    if let Some(tz) = profile.timezone.as_deref().filter(|t| !t.is_empty()) {
        if let Some(prefixes) = tz_prefixes_for_country(&exit.country) {
            if !prefixes.iter().any(|p| tz.starts_with(p)) {
                warnings.push(format!(
                    "时区 {tz} 与出口国家 {} 不一致，建议改为该地区的时区",
                    exit.country
                ));
            }
        }
    }

    if let Some(lang) = profile.language.as_deref().filter(|l| !l.is_empty()) {
        if let Some(expected) = language_for_country(&exit.country) {
            if !lang_region_matches(lang, &expected) {
                warnings.push(format!(
                    "语言 {lang} 与出口国家 {} 不一致（建议 {}）",
                    exit.country, expected
                ));
            }
        }
    }

    ConsistencyResult {
        ok: warnings.is_empty(),
        warnings,
    }
}

fn lang_region_matches(lang: &str, expected: &str) -> bool {
    let lr = lang.split('-').nth(1).unwrap_or("");
    let er = expected.split('-').nth(1).unwrap_or("");
    lr.eq_ignore_ascii_case(er)
}

fn tz_prefixes_for_country(country: &str) -> Option<&'static [&'static str]> {
    match country.trim().to_ascii_uppercase().as_str() {
        "US" | "CA" | "MX" | "BR" | "AR" | "CL" | "CO" | "PE" => Some(&["America/"]),
        "GB" | "IE" | "PT" | "ES" | "FR" | "DE" | "IT" | "NL" | "BE" | "AT" | "CH" | "SE"
        | "NO" | "DK" | "FI" | "PL" | "CZ" | "GR" | "RO" | "HU" | "TR" | "RU" | "UA" => {
            Some(&["Europe/"])
        }
        "JP" | "KR" | "CN" | "TW" | "HK" | "SG" | "TH" | "VN" | "PH" | "IN" | "ID" | "MY" => {
            Some(&["Asia/"])
        }
        "AU" => Some(&["Australia/"]),
        "NZ" => Some(&["Pacific/"]),
        "ZA" | "NG" | "KE" | "EG" | "MA" => Some(&["Africa/"]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Profile, ProxyConfig, ProxyProtocol};

    fn profile(tz: Option<&str>, lang: Option<&str>) -> Profile {
        let mut p = Profile::new(
            "test".into(),
            ProxyConfig {
                protocol: ProxyProtocol::Socks5,
                host: "127.0.0.1".into(),
                port: 1080,
                username: None,
                password: None,
            },
        );
        p.timezone = tz.map(String::from);
        p.language = lang.map(String::from);
        p
    }

    fn us_exit() -> ExitInfo {
        ExitInfo {
            ip: "8.8.8.8".into(),
            country: "US".into(),
            region: "California".into(),
            city: "Los Angeles".into(),
            timezone: "America/Los_Angeles".into(),
            source: "test".into(),
        }
    }

    #[test]
    fn matching_config_has_no_warnings() {
        let p = profile(Some("America/Los_Angeles"), Some("en-US"));
        let r = check(&p, &us_exit());
        assert!(r.ok, "unexpected warnings: {:?}", r.warnings);
    }

    #[test]
    fn mismatched_timezone_warns() {
        let p = profile(Some("Asia/Shanghai"), Some("en-US"));
        let r = check(&p, &us_exit());
        assert!(!r.ok);
        assert!(r.warnings.iter().any(|w| w.contains("时区")));
    }

    #[test]
    fn mismatched_language_warns() {
        let p = profile(Some("America/Los_Angeles"), Some("zh-CN"));
        let r = check(&p, &us_exit());
        assert!(!r.ok);
        assert!(r.warnings.iter().any(|w| w.contains("语言")));
    }
}
