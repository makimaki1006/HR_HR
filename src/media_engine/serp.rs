//! SERP(検索結果)側の純粋ロジック。
//!
//! Python 版 `serpapi_client.organic_domains` / `media_ranking.resolve_serp_location`
//! の移植。HTTP 取得は Phase 2。ここは payload(JSON)や候補リストからの抽出のみ。

use serde_json::Value;

/// URL からホスト名を取り出す(小文字化、先頭 www. 除去)。
pub fn host_of(url: &str) -> String {
    let after_scheme = url.split("://").nth(1).unwrap_or(url);
    let authority = after_scheme.split('/').next().unwrap_or("");
    let authority = authority.split('@').last().unwrap_or(authority); // userinfo 除去
    let host_port = authority.split('?').next().unwrap_or(authority);
    let host = host_port.split(':').next().unwrap_or(host_port).to_lowercase();
    host.strip_prefix("www.").unwrap_or(&host).to_string()
}

/// organic_results から (順位, ホスト) を抽出する。
pub fn organic_domains(payload: &Value) -> Vec<(i64, String)> {
    let mut out: Vec<(i64, String)> = Vec::new();
    if let Some(results) = payload.get("organic_results").and_then(Value::as_array) {
        for item in results {
            let link = item.get("link").and_then(Value::as_str).unwrap_or("");
            let host = host_of(link);
            if host.is_empty() {
                continue;
            }
            let rank = item
                .get("position")
                .and_then(Value::as_i64)
                .unwrap_or((out.len() + 1) as i64);
            out.push((rank, host));
        }
    }
    out
}

/// SerpApi の地名候補(country_code, canonical_name)。
#[derive(Debug, Clone)]
pub struct GeoLocation {
    pub country_code: String,
    pub canonical_name: String,
}

/// Google Ads の canonical("Takasaki,Gunma,Japan")から市名("Takasaki")を取る。
pub fn city_from_canonical(geo_canonical: &str) -> Option<String> {
    let city = geo_canonical.split(',').next()?.trim();
    if city.is_empty() {
        None
    } else {
        Some(city.to_string())
    }
}

/// 候補から SerpApi の location 名を選ぶ(JP 一致のみ。無ければ None=全国SERP)。
///
/// Python `media_ranking.resolve_serp_location` と一致。非JPへフォールバックすると
/// 別国の地域でSERPを叩き媒体順位が別国のものになるため、JP 一致が無ければ location
/// 無し(全国)に落とす。
pub fn pick_serp_location(matches: &[GeoLocation]) -> Option<String> {
    matches
        .iter()
        .find(|m| m.country_code == "JP")
        .map(|m| m.canonical_name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn host_of_strips_scheme_www_port() {
        assert_eq!(host_of("https://www.baitoru.com/kanto/x"), "baitoru.com");
        assert_eq!(host_of("https://jp.indeed.com:443/q-x?a=1"), "jp.indeed.com");
        assert_eq!(host_of("http://job-medley.com/hh/pref13/"), "job-medley.com");
    }

    #[test]
    fn organic_domains_extracts_rank_and_host() {
        let payload = json!({
            "organic_results": [
                {"position": 1, "link": "https://doraever.jp/x"},
                {"position": 2, "link": "https://jp.indeed.com/q"},
                {"link": "https://townwork.net/a"} // position 無し → 連番
            ]
        });
        let got = organic_domains(&payload);
        assert_eq!(got[0], (1, "doraever.jp".to_string()));
        assert_eq!(got[1], (2, "jp.indeed.com".to_string()));
        assert_eq!(got[2], (3, "townwork.net".to_string()));
    }

    #[test]
    fn city_and_location_pick() {
        assert_eq!(city_from_canonical("Takasaki,Gunma,Japan").as_deref(), Some("Takasaki"));
        let matches = vec![
            GeoLocation { country_code: "US".into(), canonical_name: "Takasaki, X".into() },
            GeoLocation { country_code: "JP".into(), canonical_name: "Takasaki, Gunma, Japan".into() },
        ];
        assert_eq!(pick_serp_location(&matches).as_deref(), Some("Takasaki, Gunma, Japan"));
    }

    #[test]
    fn pick_serp_location_none_when_no_jp() {
        // JP 一致が無ければ None(=全国SERP)。非JPにフォールバックしない。
        let matches = vec![
            GeoLocation { country_code: "US".into(), canonical_name: "Springfield, US".into() },
            GeoLocation { country_code: "GB".into(), canonical_name: "London, UK".into() },
        ];
        assert_eq!(pick_serp_location(&matches), None);
        assert_eq!(pick_serp_location(&[]), None);
    }

    #[test]
    fn city_from_canonical_empty_is_none() {
        assert_eq!(city_from_canonical(""), None);
        assert_eq!(city_from_canonical(" , X"), None);
    }
}
