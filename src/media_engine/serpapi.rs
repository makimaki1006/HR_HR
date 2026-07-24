//! SerpApi リクエスト構築(Phase 2)。
//!
//! Python 版 `serpapi_client.build_search_params` の移植。
//! ここでは HTTP 送出はせず、日本向け既定値を持つパラメータ(JSON)を組み立てるだけ。
//! レスポンス解析は既存 [`crate::media_engine::serp::organic_domains`] を再利用する。

use serde_json::{Map, Value};
use unicode_normalization::UnicodeNormalization;

/// 日本向け既定値(Python 版 JAPAN_DEFAULTS と一致)。
pub const GOOGLE_DOMAIN: &str = "google.co.jp";
pub const HL: &str = "ja";
pub const GL: &str = "jp";

/// SerpApi 検索エンドポイント(Python 版 `API_BASE` と一致)。
pub const API_BASE: &str = "https://serpapi.com/search.json";
/// SerpApi 地名メタデータ(Python 版 `LOCATIONS_URL` と一致)。キー不要・枠消費なし。
pub const LOCATIONS_URL: &str = "https://serpapi.com/locations.json";

/// クエリ/地名の表記ゆれ吸収(Python 版 `_normalize_text` と一致)。
///
/// NFKC 正規化(全角英数・全角空白の統一)→ 連続空白を 1 つに圧縮 → 前後空白除去。
pub fn normalize_text(value: &str) -> String {
    let nfkc: String = value.nfkc().collect();
    nfkc.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// SerpApi 検索パラメータを組み立てる(Python 版 `build_search_params` と一致)。
///
/// - engine 既定 "google"、device 既定 "desktop"、num 既定 10。
/// - engine=="google" のときのみ num/device を付ける。
/// - location は正規化後に空でなければ付ける。
pub fn build_search_params(query: &str, location: Option<&str>, num: i64) -> Value {
    build_search_params_full(query, "google", location, "desktop", num)
}

/// engine/device も指定するフル版。
pub fn build_search_params_full(
    query: &str,
    engine: &str,
    location: Option<&str>,
    device: &str,
    num: i64,
) -> Value {
    let mut params = Map::new();
    params.insert("engine".into(), Value::String(engine.to_string()));
    params.insert("q".into(), Value::String(normalize_text(query)));
    params.insert("google_domain".into(), Value::String(GOOGLE_DOMAIN.into()));
    params.insert("hl".into(), Value::String(HL.into()));
    params.insert("gl".into(), Value::String(GL.into()));
    if engine == "google" {
        params.insert("num".into(), Value::from(num));
        params.insert("device".into(), Value::String(device.to_string()));
    }
    if let Some(loc) = location {
        let loc = normalize_text(loc);
        if !loc.is_empty() {
            params.insert("location".into(), Value::String(loc));
        }
    }
    Value::Object(params)
}

// 2026-07-24 HR_HR 統合: キャッシュの読み書きは store.rs (Turso 一次 + ローカル
// ファイル副次) へ移した。Render 無料プランのディスク非永続対策 (ユーザー決定)。

/// キャッシュキー(Python `_cache_key` と同一: api_key を除くパラメータをキー昇順で
/// JSON 化し SHA-1)。同じ問い合わせを二重に買わないための鍵。
pub fn cache_key(params: &Value) -> String {
    let mut entries: Vec<(String, Value)> = params
        .as_object()
        .map(|o| {
            o.iter()
                .filter(|(k, _)| k.as_str() != "api_key")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        })
        .unwrap_or_default();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut map = Map::new();
    for (k, v) in entries {
        map.insert(k, v);
    }
    // Python は ensure_ascii=False・区切りが ", " / ": " の既定 dumps。
    let canon = serde_json::to_string(&Value::Object(map))
        .unwrap_or_default()
        .replace("\":", "\": ")
        .replace(",\"", ", \"");
    sha1_hex(canon.as_bytes())
}

/// 依存を増やさないための最小 SHA-1(RFC 3174)。
fn sha1_hex(data: &[u8]) -> String {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let ml = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&ml.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let tmp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = tmp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

/// SerpApi 検索を 1 回実行し、生 JSON を返す。
///
/// **同一パラメータの応答はローカルキャッシュから返し、枠を消費しない**
/// (Python `serpapi_client.search` と同じキー・同じ置き場を共有)。
/// キャッシュから返した場合は `request_meta.local_cache_hit = true` が付く。
pub async fn search(
    query: &str,
    location: Option<&str>,
    num: i64,
    api_key: &str,
    turso: Option<crate::db::turso_http::TursoDb>,
) -> anyhow::Result<serde_json::Value> {
    search_device(query, location, num, "desktop", api_key, turso).await
}

/// device("desktop"/"mobile"/"tablet")を指定して検索する。
///
/// 求職者の主戦場はスマホなので、媒体順位は mobile でも確認できるようにする。
/// device はキャッシュキーに含まれるため、desktop と mobile は別々にキャッシュされる
/// (＝一方を取っても他方は改めて 1 クエリ必要)。
pub async fn search_device(
    query: &str,
    location: Option<&str>,
    num: i64,
    device: &str,
    api_key: &str,
    turso: Option<crate::db::turso_http::TursoDb>,
) -> anyhow::Result<serde_json::Value> {
    let params = build_search_params_full(query, "google", location, device, num);
    // 同一問い合わせは買い直さない(キーは Python 実装と同一。置き場は Turso 一次)。
    let key = cache_key(&params);
    if let Some(mut cached) = crate::media_engine::store::cache_get(turso.clone(), &key).await {
        if let Some(obj) = cached.as_object_mut() {
            let meta = obj
                .entry("request_meta")
                .or_insert_with(|| Value::Object(Map::new()));
            if let Some(m) = meta.as_object_mut() {
                m.insert("local_cache_hit".into(), Value::Bool(true));
            }
        }
        return Ok(cached);
    }
    let payload = fetch_search(&params, api_key).await?;
    // 買った応答は保存して次回以降は無料で再利用する。
    crate::media_engine::store::cache_put(turso, &key, &payload).await;
    Ok(payload)
}

/// キャッシュを読まずに必ず実取得する(枠を 1 消費)。取得結果はキャッシュへ保存する。
///
/// 用途: キャッシュ応答に page_token が無い/古いために付随情報(AI概要の本文など)が
/// 取れないとき、トークンを新鮮にするために取り直す。
pub async fn search_fresh(
    query: &str,
    location: Option<&str>,
    num: i64,
    api_key: &str,
    turso: Option<crate::db::turso_http::TursoDb>,
) -> anyhow::Result<serde_json::Value> {
    search_fresh_device(query, location, num, "desktop", api_key, turso).await
}

/// device 指定でキャッシュを読まずに実取得する。
pub async fn search_fresh_device(
    query: &str,
    location: Option<&str>,
    num: i64,
    device: &str,
    api_key: &str,
    turso: Option<crate::db::turso_http::TursoDb>,
) -> anyhow::Result<serde_json::Value> {
    let params = build_search_params_full(query, "google", location, device, num);
    let key = cache_key(&params);
    let payload = fetch_search(&params, api_key).await?;
    crate::media_engine::store::cache_put(turso, &key, &payload).await;
    Ok(payload)
}

/// 実際に SerpApi へ GET する(キャッシュ判定なし)。[`search`]/[`search_fresh`] の共通部。
async fn fetch_search(params: &Value, api_key: &str) -> anyhow::Result<serde_json::Value> {
    let mut pairs: Vec<(String, String)> = Vec::new();
    if let Some(obj) = params.as_object() {
        for (k, v) in obj {
            let s = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            pairs.push((k.clone(), s));
        }
    }
    pairs.push(("api_key".to_string(), api_key.to_string()));
    let client = reqwest::Client::new();
    let resp = client.get(API_BASE).query(&pairs).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "SerpApi search failed: HTTP {status}: {}",
            body.chars().take(1200).collect::<String>()
        );
    }
    Ok(resp.json().await?)
}

/// page_token 系の継続取得(AI概要・関連質問の展開など)を 1 回叩く。
///
/// トークンは短時間で失効するため、**元の SERP を取った直後に**呼ぶこと。
/// `token_param` は "page_token" か "next_page_token"、`engine` は
/// "google_ai_overview" / "google_related_questions" 等。キャッシュはしない
/// (トークンが毎回変わるためキーが一致しない)。
pub async fn fetch_by_token(
    engine: &str,
    token_param: &str,
    token: &str,
    api_key: &str,
) -> anyhow::Result<serde_json::Value> {
    let client = reqwest::Client::new();
    let pairs = [
        ("engine", engine),
        (token_param, token),
        ("api_key", api_key),
    ];
    let resp = client.get(API_BASE).query(&pairs).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "SerpApi {engine} failed: HTTP {status}: {}",
            body.chars().take(600).collect::<String>()
        );
    }
    Ok(resp.json().await?)
}

/// locations.json レスポンス(配列)を GeoLocation へ解析する(純粋)。
///
/// Python `serpapi_client.fetch_locations` は生の配列(dict)を返し、
/// `media_ranking.resolve_serp_location` が `country_code` / `canonical_name` を読む。
/// ここでは Rust 側で使う 2 フィールドだけ取り出す(欠測はスキップ)。
pub fn parse_locations(payload: &serde_json::Value) -> Vec<crate::media_engine::serp::GeoLocation> {
    let mut out = Vec::new();
    let arr = match payload.as_array() {
        Some(a) => a,
        None => return out,
    };
    for item in arr {
        let canonical = item
            .get("canonical_name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if canonical.is_empty() {
            continue;
        }
        let country = item
            .get("country_code")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        out.push(crate::media_engine::serp::GeoLocation {
            country_code: country,
            canonical_name: canonical,
        });
    }
    out
}

/// SerpApi locations.json を叩き地名候補を返す(Phase 2b、キー不要・枠消費なし)。
///
/// GET `https://serpapi.com/locations.json?q=<query>&limit=<limit>`。Python
/// `fetch_locations` と一致。解析済み [`crate::media_engine::serp::GeoLocation`] を返す。
pub async fn fetch_locations(
    query: &str,
    limit: i64,
) -> anyhow::Result<Vec<crate::media_engine::serp::GeoLocation>> {
    let client = reqwest::Client::new();
    let resp = client
        .get(LOCATIONS_URL)
        .query(&[("q", query.to_string()), ("limit", limit.to_string())])
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "SerpApi locations failed: HTTP {status}: {}",
            body.chars().take(1200).collect::<String>()
        );
    }
    let payload: Value = resp.json().await?;
    Ok(parse_locations(&payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_match_japan() {
        let p = build_search_params("看護師 求人", None, 10);
        assert_eq!(p["engine"], "google");
        assert_eq!(p["q"], "看護師 求人");
        assert_eq!(p["google_domain"], "google.co.jp");
        assert_eq!(p["hl"], "ja");
        assert_eq!(p["gl"], "jp");
        assert_eq!(p["num"], 10);
        assert_eq!(p["device"], "desktop");
        // location 未指定なら鍵ごと無し
        assert!(p.get("location").is_none());
    }

    #[test]
    fn normalize_absorbs_fullwidth_and_extra_spaces() {
        // 全角英数→半角、全角空白/連続空白→単一、前後trim
        assert_eq!(normalize_text("  Tokyo,  Japan "), "Tokyo, Japan");
        assert_eq!(normalize_text("ＩＴ　求人"), "IT 求人");
        let p = build_search_params("看護師　　求人 ", Some(" 東京都　千代田区 "), 10);
        assert_eq!(p["q"], "看護師 求人");
        assert_eq!(p["location"], "東京都 千代田区");
    }

    #[test]
    fn empty_location_after_normalize_is_omitted() {
        let p = build_search_params("介護 求人", Some("   "), 10);
        assert!(p.get("location").is_none());
    }

    #[test]
    fn non_google_engine_omits_num_device() {
        let p = build_search_params_full("看護師", "google_jobs", None, "desktop", 10);
        assert!(p.get("num").is_none());
        assert!(p.get("device").is_none());
    }

    #[test]
    fn organic_domains_roundtrip_via_serp() {
        // レスポンス解析は既存 serp::organic_domains を再利用できることの確認。
        let payload = json!({
            "organic_results": [
                {"position": 1, "link": "https://jp.indeed.com/q"},
                {"position": 2, "link": "https://townwork.net/a"}
            ]
        });
        let got = crate::media_engine::serp::organic_domains(&payload);
        assert_eq!(got[0], (1, "jp.indeed.com".to_string()));
        assert_eq!(got[1], (2, "townwork.net".to_string()));
    }

    #[test]
    fn parse_locations_extracts_country_and_canonical() {
        // locations.json は配列。canonical_name 欠測はスキップ。
        let payload = json!([
            {"country_code": "JP", "canonical_name": "Takasaki,Gunma,Japan"},
            {"country_code": "US", "canonical_name": "Takasaki,US"},
            {"country_code": "JP"}
        ]);
        let got = parse_locations(&payload);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].country_code, "JP");
        assert_eq!(got[0].canonical_name, "Takasaki,Gunma,Japan");
        // 非配列は空。
        assert!(parse_locations(&json!({})).is_empty());
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    /// Python 実装が作った実キャッシュのファイル名と、Rust の [`cache_key`] が一致すること。
    /// (一致しないと同じ問い合わせを二重に買ってしまう)
    #[test]
    fn cache_key_matches_python_cache_files() {
        let dir = std::path::PathBuf::from("../data/serpapi_cache");
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => return, // キャッシュが無い環境ではスキップ
        };
        let mut checked = 0;
        for ent in entries.flatten() {
            let path = ent.path();
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) if s.len() == 40 => s.to_string(), // sha1 hex のみ
                _ => continue,
            };
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let v: Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(_) => continue,
            };
            // 保存された request_meta.params から鍵を再計算する
            let params = match v.get("request_meta").and_then(|m| m.get("params")) {
                Some(p) => p.clone(),
                None => continue,
            };
            assert_eq!(cache_key(&params), stem, "キャッシュキー不一致: {:?}", path);
            checked += 1;
            if checked >= 5 {
                break;
            }
        }
    }
}
