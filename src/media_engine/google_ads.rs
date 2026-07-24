//! Google Ads KeywordPlanIdeaService リクエスト構築とレスポンス解析(Phase 2)。
//!
//! Python 版 `google_ads_keyword_plan.build_generate_keyword_historical_metrics_request`
//! の移植。ここでは HTTP 送出はせず、リクエスト(JSON)組み立てと
//! レスポンス JSON→`Vec<(String, Option<i64>)>` 解析だけを行う。
//! 解析結果は [`crate::media_engine::keywords::build_volume_map`] に食わせられる形。

use serde_json::{Map, Value};

/// 既定言語 ID(日本語)。Python 版 `language_id="1005"` と一致。
pub const DEFAULT_LANGUAGE_ID: &str = "1005";
/// 既定ネットワーク。SERP を Google 単独に揃えるため AND_PARTNERS を使わない。
pub const DEFAULT_NETWORK: &str = "GOOGLE_SEARCH";

/// 履歴指標リクエストの構築エラー。
#[derive(Debug, PartialEq, Eq)]
pub enum RequestError {
    NoKeywords,
    TooManyKeywords(usize),
    TooManyGeoTargets(usize),
    /// 期間指定の月が 1..=12 の外(Google Ads の月名に変換できない)。
    InvalidMonth(u32),
    /// 予測期間の日付が yyyy-mm-dd でない、または start > end。
    InvalidDate(String),
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestError::NoKeywords => write!(f, "keywords is required"),
            RequestError::TooManyKeywords(n) => write!(
                f,
                "Google Ads historical metrics accepts at most 10,000 keywords (got {n})"
            ),
            RequestError::TooManyGeoTargets(n) => write!(
                f,
                "Google Ads historical metrics accepts at most 10 geo targets (got {n})"
            ),
            RequestError::InvalidMonth(m) => {
                write!(f, "month must be 1..=12 (got {m})")
            }
            RequestError::InvalidDate(d) => {
                write!(f, "forecast period date must be yyyy-mm-dd and start<=end (got {d})")
            }
        }
    }
}

impl std::error::Error for RequestError {}

/// GenerateKeywordHistoricalMetrics リクエストを組み立てる。
///
/// Python 版と同じく: キーワードを strip → 空除去 → 順序保持で重複排除。
/// キー: customerId / keywords / language(languageConstants/..) /
/// geoTargetConstants(geoTargetConstants/..) / keywordPlanNetwork。
pub fn build_historical_metrics_request(
    customer_id: &str,
    keywords: &[String],
    location_ids: &[String],
    language_id: &str,
    network: &str,
) -> Result<Value, RequestError> {
    // strip + 空除去 + 順序保持の重複排除(Python の dict.fromkeys と一致)。
    let mut seen = std::collections::HashSet::new();
    let cleaned: Vec<String> = keywords
        .iter()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .filter(|k| seen.insert(k.clone()))
        .collect();

    if cleaned.is_empty() {
        return Err(RequestError::NoKeywords);
    }
    if cleaned.len() > 10_000 {
        return Err(RequestError::TooManyKeywords(cleaned.len()));
    }
    if location_ids.len() > 10 {
        return Err(RequestError::TooManyGeoTargets(location_ids.len()));
    }

    let mut req = Map::new();
    req.insert("customerId".into(), Value::String(customer_id.to_string()));
    req.insert(
        "keywords".into(),
        Value::Array(cleaned.into_iter().map(Value::String).collect()),
    );
    req.insert(
        "language".into(),
        Value::String(format!("languageConstants/{language_id}")),
    );
    req.insert(
        "geoTargetConstants".into(),
        Value::Array(
            location_ids
                .iter()
                .map(|id| Value::String(format!("geoTargetConstants/{id}")))
                .collect(),
        ),
    );
    req.insert(
        "keywordPlanNetwork".into(),
        Value::String(network.to_string()),
    );
    Ok(Value::Object(req))
}

/// 既定値(language=1005, network=GOOGLE_SEARCH)で構築する薄いラッパ。
pub fn build_historical_metrics_request_default(
    customer_id: &str,
    keywords: &[String],
    location_ids: &[String],
) -> Result<Value, RequestError> {
    build_historical_metrics_request(
        customer_id,
        keywords,
        location_ids,
        DEFAULT_LANGUAGE_ID,
        DEFAULT_NETWORK,
    )
}

// ---------------------------------------------------------------------------
// 過去最大 4 年(48ヶ月)の月次データ — historicalMetricsOptions.yearMonthRange
// ---------------------------------------------------------------------------

/// Google Ads の履歴指標が遡れる上限(実測 48ヶ月 = 4 年)。
pub const MAX_MONTHS_BACK: u32 = 48;
/// 期間指定なしのときに Google Ads が返す既定の月数。
pub const DEFAULT_MONTHS_BACK: u32 = 12;

/// months_back を 1..=[`MAX_MONTHS_BACK`] に丸める。
pub fn clamp_months_back(months_back: u32) -> u32 {
    months_back.clamp(1, MAX_MONTHS_BACK)
}

/// 月番号(1-12)→ Google Ads の月名(英大文字)。範囲外は `None`。
/// [`month_name_to_num`] の逆写像。
pub fn month_num_to_name(month: u32) -> Option<&'static str> {
    match month {
        1 => Some("JANUARY"),
        2 => Some("FEBRUARY"),
        3 => Some("MARCH"),
        4 => Some("APRIL"),
        5 => Some("MAY"),
        6 => Some("JUNE"),
        7 => Some("JULY"),
        8 => Some("AUGUST"),
        9 => Some("SEPTEMBER"),
        10 => Some("OCTOBER"),
        11 => Some("NOVEMBER"),
        12 => Some("DECEMBER"),
        _ => None,
    }
}

/// 終端年月と遡る月数から開始年月を求める(終端月を含めて months_back ヶ月)。
///
/// 例: end=(2026,7), months_back=48 → start=(2022,8)(2022-08..2026-07 の 48ヶ月)。
/// months_back は [`clamp_months_back`] で 1..=48 に丸める。
pub fn range_start_from_end(end_year: i32, end_month: u32, months_back: u32) -> (i32, u32) {
    let n = clamp_months_back(months_back) as i64;
    // 月を「西暦*12 + (月-1)」の通し番号にして減算する(年跨ぎを安全に扱う)。
    let total = end_year as i64 * 12 + (end_month as i64 - 1) - (n - 1);
    let year = total.div_euclid(12) as i32;
    let month = total.rem_euclid(12) as u32 + 1;
    (year, month)
}

/// 期間指定つきの GenerateKeywordHistoricalMetrics リクエストを組み立てる。
///
/// [`build_historical_metrics_request`] の出力に
/// `historicalMetricsOptions.yearMonthRange.{start,end}.{year, month}` を足すだけ
/// (year=数値、month=英大文字の月名)。期間を付けない従来の関数は一切変えない。
pub fn build_historical_metrics_request_range(
    customer_id: &str,
    keywords: &[String],
    location_ids: &[String],
    language_id: &str,
    network: &str,
    start: (i32, u32),
    end: (i32, u32),
) -> Result<Value, RequestError> {
    let mut req =
        build_historical_metrics_request(customer_id, keywords, location_ids, language_id, network)?;
    let start_name = month_num_to_name(start.1).ok_or(RequestError::InvalidMonth(start.1))?;
    let end_name = month_num_to_name(end.1).ok_or(RequestError::InvalidMonth(end.1))?;
    if let Some(obj) = req.as_object_mut() {
        obj.insert(
            "historicalMetricsOptions".into(),
            serde_json::json!({
                "yearMonthRange": {
                    "start": {"year": start.0, "month": start_name},
                    "end":   {"year": end.0,   "month": end_name},
                }
            }),
        );
    }
    Ok(req)
}

/// 既定値(language=1005, network=GOOGLE_SEARCH)＋期間指定で構築する薄いラッパ。
///
/// `end`(終端年月)を含めて `months_back` ヶ月ぶんを要求する。
pub fn build_historical_metrics_request_range_default(
    customer_id: &str,
    keywords: &[String],
    location_ids: &[String],
    end: (i32, u32),
    months_back: u32,
) -> Result<Value, RequestError> {
    let start = range_start_from_end(end.0, end.1, months_back);
    build_historical_metrics_request_range(
        customer_id,
        keywords,
        location_ids,
        DEFAULT_LANGUAGE_ID,
        DEFAULT_NETWORK,
        start,
        end,
    )
}

/// 応答に含まれる月次データのうち最新の年月を返す(`(year, month)`)。
///
/// 現在時刻に依存せず「API が実際に返した最新月」を期間指定の終端に使うための関数。
/// Google Ads の月次データは公開に遅延があり、暦上の当月・前月がまだ無いことがあるため、
/// 時計から終端を決めるより安全。月次が 1 件も無ければ `None`。
pub fn latest_month_in_payload(payload: &Value) -> Option<(i32, u32)> {
    let results = payload.get("results").and_then(Value::as_array)?;
    let mut best: Option<(i32, u32)> = None;
    for item in results {
        let m = item
            .get("keywordMetrics")
            .or_else(|| item.get("keywordIdeaMetrics"));
        let rows = match m
            .and_then(|x| x.get("monthlySearchVolumes"))
            .and_then(Value::as_array)
        {
            Some(r) => r,
            None => continue,
        };
        for row in rows {
            let year = row
                .get("year")
                .and_then(|y| y.as_i64().or_else(|| y.as_str()?.trim().parse::<i64>().ok()));
            let month = row
                .get("month")
                .and_then(Value::as_str)
                .and_then(month_name_to_num)
                .and_then(|mm| mm.parse::<u32>().ok());
            if let (Some(y), Some(mm)) = (year, month) {
                let cand = (y as i32, mm);
                if best.map(|b| cand > b).unwrap_or(true) {
                    best = Some(cand);
                }
            }
        }
    }
    best
}

/// レスポンス(results[].text + keywordMetrics.avgMonthlySearches)を
/// `Vec<(String, Option<i64>)>` に解析する。
///
/// avgMonthlySearches は文字列("22200")でも数値でも受ける(Google Ads REST は
/// int64 を JSON 文字列で返すため)。欠損・非数値は `None`。
pub fn parse_historical_metrics(payload: &Value) -> Vec<(String, Option<i64>)> {
    let mut out = Vec::new();
    let results = match payload.get("results").and_then(Value::as_array) {
        Some(r) => r,
        None => return out,
    };
    for item in results {
        let text = match item.get("text").and_then(Value::as_str) {
            Some(t) => t.to_string(),
            None => continue,
        };
        let volume = item
            .get("keywordMetrics")
            .and_then(|m| m.get("avgMonthlySearches"))
            .and_then(parse_int64);
        out.push((text, volume));
    }
    out
}

/// JSON 値から i64 を取り出す(int64 は文字列で来る場合がある)。
fn parse_int64(v: &Value) -> Option<i64> {
    if let Some(n) = v.as_i64() {
        return Some(n);
    }
    if let Some(s) = v.as_str() {
        return s.trim().parse::<i64>().ok();
    }
    None
}

/// Google Ads の月名(英大文字)→ 2桁の月("01".."12")。未知名は `None`。
fn month_name_to_num(name: &str) -> Option<&'static str> {
    match name.trim().to_uppercase().as_str() {
        "JANUARY" => Some("01"),
        "FEBRUARY" => Some("02"),
        "MARCH" => Some("03"),
        "APRIL" => Some("04"),
        "MAY" => Some("05"),
        "JUNE" => Some("06"),
        "JULY" => Some("07"),
        "AUGUST" => Some("08"),
        "SEPTEMBER" => Some("09"),
        "OCTOBER" => Some("10"),
        "NOVEMBER" => Some("11"),
        "DECEMBER" => Some("12"),
        _ => None,
    }
}

/// 1 キーワードの Google Ads 履歴指標(月次内訳＋競合度＋CPC を含む)。
///
/// `parse_historical_metrics` が avgMonthlySearches のみを返すのに対し、こちらは同じ
/// generateKeywordHistoricalMetrics 応答の keywordMetrics から月別ボリューム・競合度・
/// 入札単価まで取り出す。int64 系(avgMonthlySearches / competitionIndex / bid micros /
/// monthlySearches)は文字列でも数値でも受ける([`parse_int64`] 流用)。
#[derive(Debug, Clone, PartialEq)]
pub struct KeywordMetric {
    pub keyword: String,
    pub avg_monthly: Option<i64>,
    /// (「YYYY-MM」, 検索数) の並び(応答順を保持)。
    pub monthly_12m: Vec<(String, i64)>,
    /// 競合度 enum("LOW"/"MEDIUM"/"HIGH"/"UNSPECIFIED" 等)。欠損は空文字。
    pub competition: String,
    /// 競合指数(0-100)。
    pub competition_index: Option<i64>,
    /// ページ上部入札単価(下限)を円換算(micros ÷ 1,000,000)。
    pub bid_low_yen: Option<f64>,
    /// ページ上部入札単価(上限)を円換算(micros ÷ 1,000,000)。
    pub bid_high_yen: Option<f64>,
    /// 概念タグ(keywordAnnotations.concepts)。`(グループ名, 概念名)` の並び(応答順)。
    /// 例: `("サービス", "タクシー")` / `("ノンブランド", "ノンブランド")`。
    /// generateKeywordIdeas に `keywordAnnotation:["KEYWORD_CONCEPT"]` を付けた時のみ入る。
    pub concepts: Vec<(String, String)>,
    /// ブランド指名キーワードか([`concept_group_is_brand`] の判定)。概念タグが無ければ false。
    pub is_brand: bool,
}

/// 概念グループがブランド系か判定する。
///
/// 判定根拠の優先順位:
/// 1. `conceptGroup.type`(Google Ads `ConceptGroupTypeEnum`)。`BRAND` / `OTHER_BRANDS` は
///    ブランド、`NON_BRAND` は非ブランド。ここで決まればロケール非依存で最も堅い。
/// 2. type が欠落/`UNSPECIFIED`/`UNKNOWN` のときだけグループ名で判定する。日本語ロケールでは
///    "ノンブランド" / "他のブランド" のように返るため、まず「ノンブランド(non-brand)」を
///    除外し、残りが "ブランド"(brand)を含むならブランド系とみなす。"サービス" 等の
///    非ブランド系グループ名は false のまま(名前ホワイトリスト方式)。
pub fn concept_group_is_brand(group_name: &str, group_type: Option<&str>) -> bool {
    if let Some(t) = group_type {
        match t.trim().to_uppercase().as_str() {
            "BRAND" | "OTHER_BRANDS" => return true,
            "NON_BRAND" | "NON_BRANDS" => return false,
            // UNSPECIFIED / UNKNOWN / 空 は名前判定へフォールバック。
            _ => {}
        }
    }
    let n = group_name.trim().to_lowercase();
    if n.contains("ノンブランド")
        || n.contains("非ブランド")
        || n.contains("non-brand")
        || n.contains("non brand")
        || n.contains("nonbrand")
    {
        return false;
    }
    n.contains("ブランド") || n.contains("brand")
}

/// 応答(results[].keywordMetrics または keywordIdeaMetrics)を [`KeywordMetric`] 配列へ解析する。
///
/// generateKeywordHistoricalMetrics 応答は `keywordMetrics`、generateKeywordIdeas 応答は
/// `keywordIdeaMetrics`(同じサブフィールド)を持つ。どちらのキーでも拾う。
/// monthlySearchVolumes の各行 {month:"MARCH", year:"2025", monthlySearches:"1234"} を
/// ("2025-03", 1234) へ(月名→2桁、[`month_name_to_num`])。lowTopOfPageBidMicros /
/// highTopOfPageBidMicros は micros なので ÷1_000_000 で円へ。text が無い行はスキップ。
pub fn parse_keyword_metrics(payload: &Value) -> Vec<KeywordMetric> {
    let mut out = Vec::new();
    let results = match payload.get("results").and_then(Value::as_array) {
        Some(r) => r,
        None => return out,
    };
    for item in results {
        let keyword = match item.get("text").and_then(Value::as_str) {
            Some(t) => t.to_string(),
            None => continue,
        };
        // 履歴指標は keywordMetrics、generateKeywordIdeas 応答は keywordIdeaMetrics に
        // 同じサブフィールドを持つ。キー名だけが異なるので両対応で拾う。
        let m = item
            .get("keywordMetrics")
            .or_else(|| item.get("keywordIdeaMetrics"));
        let avg_monthly = m.and_then(|x| x.get("avgMonthlySearches")).and_then(parse_int64);
        let competition = m
            .and_then(|x| x.get("competition"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let competition_index = m.and_then(|x| x.get("competitionIndex")).and_then(parse_int64);
        let bid_low_yen = m
            .and_then(|x| x.get("lowTopOfPageBidMicros"))
            .and_then(parse_int64)
            .map(|v| v as f64 / 1_000_000.0);
        let bid_high_yen = m
            .and_then(|x| x.get("highTopOfPageBidMicros"))
            .and_then(parse_int64)
            .map(|v| v as f64 / 1_000_000.0);

        let mut monthly_12m = Vec::new();
        if let Some(rows) = m
            .and_then(|x| x.get("monthlySearchVolumes"))
            .and_then(Value::as_array)
        {
            for row in rows {
                let month_name = row.get("month").and_then(Value::as_str);
                // year は文字列("2025")でも数値でも来る。
                let year = row.get("year").and_then(|y| {
                    y.as_str()
                        .map(str::to_string)
                        .or_else(|| y.as_i64().map(|n| n.to_string()))
                });
                let vol = row.get("monthlySearches").and_then(parse_int64);
                if let (Some(mn), Some(yr), Some(v)) = (month_name, year, vol) {
                    if let Some(mm) = month_name_to_num(mn) {
                        monthly_12m.push((format!("{yr}-{mm}"), v));
                    }
                }
            }
        }

        // 概念タグ(keywordAnnotations.concepts[])。generateKeywordIdeas で
        // keywordAnnotation:["KEYWORD_CONCEPT"] を要求したときのみ入る(無ければ空)。
        let mut concepts: Vec<(String, String)> = Vec::new();
        let mut is_brand = false;
        if let Some(rows) = item
            .get("keywordAnnotations")
            .and_then(|a| a.get("concepts"))
            .and_then(Value::as_array)
        {
            for c in rows {
                let name = c
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let group = c.get("conceptGroup");
                let group_name = group
                    .and_then(|g| g.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let group_type = group.and_then(|g| g.get("type")).and_then(Value::as_str);
                if name.is_empty() && group_name.is_empty() {
                    continue;
                }
                if concept_group_is_brand(&group_name, group_type) {
                    is_brand = true;
                }
                concepts.push((group_name, name));
            }
        }

        out.push(KeywordMetric {
            keyword,
            avg_monthly,
            monthly_12m,
            competition,
            competition_index,
            bid_low_yen,
            bid_high_yen,
            concepts,
            is_brand,
        });
    }
    out
}

// ===========================================================================
// Phase 2b: 実 API クライアント(OAuth / 履歴指標 / GeoTarget Suggest)。
// Python 版 google_ads_keyword_volume.py / geo_resolver.py の HTTP 詳細に一致。
// ===========================================================================

use crate::media_engine::config::GoogleAdsConfig;

/// OAuth トークンエンドポイント。
pub const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
/// Google Ads API ルート。
pub const GOOGLE_ADS_API_ROOT: &str = "https://googleads.googleapis.com";

/// GeoTargetConstants:suggest の 1 候補(Python `suggest_geo_targets` の返却要素)。
#[derive(Debug, Clone, PartialEq)]
pub struct GeoCandidate {
    pub id: Option<String>,
    pub name: Option<String>,
    pub canonical_name: Option<String>,
    pub target_type: Option<String>,
    pub status: Option<String>,
    pub reach: Option<i64>,
}

/// 解決結果(選ばれた地域)。
#[derive(Debug, Clone, PartialEq)]
pub struct GeoPick {
    pub id: String,
    pub name: Option<String>,
    pub canonical_name: Option<String>,
    pub target_type: Option<String>,
    pub reach: Option<i64>,
}

/// 種別の優先順位(小さいほど優先)。Python `_TARGET_TYPE_RANK` と一致。市区を最上位。
fn target_type_rank(target_type: Option<&str>) -> i32 {
    match target_type {
        Some("City") => 0,
        Some("Postal Code") => 1,
        Some("Neighborhood") => 2,
        Some("Prefecture") => 3,
        Some("Region") => 4,
        Some("Country") => 5,
        _ => 99,
    }
}

/// suggest レスポンス JSON を候補配列へ解析する。
pub fn parse_geo_candidates(payload: &Value) -> Vec<GeoCandidate> {
    let mut out = Vec::new();
    let suggestions = match payload
        .get("geoTargetConstantSuggestions")
        .and_then(Value::as_array)
    {
        Some(s) => s,
        None => return out,
    };
    for suggestion in suggestions {
        let gt = suggestion.get("geoTargetConstant");
        let field = |k: &str| {
            gt.and_then(|g| g.get(k))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        out.push(GeoCandidate {
            id: field("id"),
            name: field("name"),
            canonical_name: field("canonicalName"),
            target_type: field("targetType"),
            status: field("status"),
            reach: suggestion.get("reach").and_then(parse_int64),
        });
    }
    out
}

/// 候補から最適な地域IDを選ぶ(純粋関数、Python `resolve_geo_id` と同じ挙動)。
///
/// ENABLED かつ id を持つ候補のみ対象。選択優先度は ①クエリと完全一致(候補 name)
/// → ②種別ランク(City を最優先) → ③reach 大。同点は入力の先勝ち。
///
/// 県ヒント無し(従来挙動)。県ヒント付きは [`pick_geo_id_pref`] を使う。
pub fn pick_geo_id(candidates: &[GeoCandidate], query: &str) -> Option<GeoPick> {
    pick_geo_id_pref(candidates, query, None)
}

/// 県ヒント付きで最適な地域IDを選ぶ(Python `resolve_geo_id` の `prefer_prefecture_en`)。
///
/// 選択優先度は ①クエリと完全一致(候補 name) → ②県一致(prefer_prefecture 指定時、
/// canonical に県トークンを含む候補) → ③種別ランク(City 最優先) → ④reach 大。
/// 同名市区が複数県に存在するとき(府中市=東京/広島 等)、reach 最大の別県への誤解決を
/// 防ぐため、近隣は基準の県を渡して曖昧性を解消する。prefer_prefecture が None/空なら
/// 県一致キーは常に一定(=従来挙動)。
pub fn pick_geo_id_pref(
    candidates: &[GeoCandidate],
    query: &str,
    prefer_prefecture: Option<&str>,
) -> Option<GeoPick> {
    let enabled: Vec<&GeoCandidate> = candidates
        .iter()
        .filter(|c| c.status.as_deref() == Some("ENABLED") && c.id.is_some())
        .collect();
    if enabled.is_empty() {
        return None;
    }
    let pref = prefer_prefecture.unwrap_or("").trim().to_lowercase();
    // min_by_key は同点時に先頭要素を返す(Python の min と一致)。
    let best = enabled
        .iter()
        .min_by_key(|c| {
            let exact = if c.name.as_deref() == Some(query) { 0 } else { 1 };
            // 県一致(指定時のみ有効)。canonical に県トークンを含む候補を優先。
            let pref_match = if !pref.is_empty()
                && c.canonical_name
                    .as_deref()
                    .map(|cn| cn.to_lowercase().contains(&pref))
                    .unwrap_or(false)
            {
                0
            } else {
                1
            };
            let type_rank = target_type_rank(c.target_type.as_deref());
            let reach = c.reach.unwrap_or(0);
            (exact, pref_match, type_rank, -reach)
        })
        .copied()?;
    Some(GeoPick {
        id: best.id.clone().unwrap_or_default(),
        name: best.name.clone(),
        canonical_name: best.canonical_name.clone(),
        target_type: best.target_type.clone(),
        reach: best.reach,
    })
}

/// レスポンスが成功系でなければ本文つきで anyhow エラーにする。
async fn error_for_body(resp: reqwest::Response, ctx: &str) -> anyhow::Result<reqwest::Response> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    anyhow::bail!("{ctx}: HTTP {status}: {}", body.chars().take(1200).collect::<String>())
}

/// 429(RESOURCE_EXHAUSTED)の本文から `retryDelay` の秒数を取り出す(例 "4s")。
/// 取れない場合は `None`(呼び出し側が既定の待機にフォールバックする)。
pub fn parse_retry_delay_secs(body: &str) -> Option<u64> {
    let v: Value = serde_json::from_str(body).ok()?;
    let errors = v
        .get("error")?
        .get("details")?
        .as_array()?
        .iter()
        .find_map(|d| d.get("errors").and_then(Value::as_array))?;
    let s = errors.iter().find_map(|e| {
        e.get("details")
            .and_then(|d| d.get("quotaErrorDetails"))
            .and_then(|q| q.get("retryDelay"))
            .and_then(Value::as_str)
    })?;
    s.trim_end_matches('s').trim().parse::<u64>().ok()
}

/// レート制限(429)に当たったら待って再試行する POST。
///
/// Google Ads は "Requests per service per method" のアカウント単位レートを持ち、
/// 地域別に連続照会すると 429 を返す。サーバ指定の retryDelay を尊重し、
/// 指定が無ければ指数バックオフ(4s, 8s, 16s)で最大 [`MAX_RATE_RETRIES`] 回まで再試行する。
const MAX_RATE_RETRIES: u32 = 3;
async fn post_json_with_retry(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    cfg: &GoogleAdsConfig,
    body: &Value,
    ctx: &str,
) -> anyhow::Result<reqwest::Response> {
    let mut attempt = 0u32;
    loop {
        let resp = client
            .post(url)
            .header("Authorization", format!("Bearer {token}"))
            .header("developer-token", &cfg.developer_token)
            .header("login-customer-id", cfg.login_customer_id.replace('-', ""))
            .json(body)
            .send()
            .await?;
        if resp.status().as_u16() != 429 || attempt >= MAX_RATE_RETRIES {
            return error_for_body(resp, ctx).await;
        }
        let text = resp.text().await.unwrap_or_default();
        let wait = parse_retry_delay_secs(&text).unwrap_or(4 << attempt);
        tracing::warn!("{ctx}: レート制限(429)。{wait}秒待って再試行します(試行{})", attempt + 1);
        tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
        attempt += 1;
    }
}

/// OAuth リフレッシュトークンから access_token を得る(Python `refresh_access_token`)。
pub async fn refresh_access_token(cfg: &GoogleAdsConfig) -> anyhow::Result<String> {
    let mut form: Vec<(&str, &str)> = vec![
        ("client_id", cfg.client_id.as_str()),
        ("refresh_token", cfg.refresh_token.as_str()),
        ("grant_type", "refresh_token"),
    ];
    if !cfg.client_secret.is_empty() {
        form.push(("client_secret", cfg.client_secret.as_str()));
    }
    let client = reqwest::Client::new();
    let resp = client.post(TOKEN_URL).form(&form).send().await?;
    let resp = error_for_body(resp, "OAuth token refresh failed").await?;
    let payload: Value = resp.json().await?;
    payload
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("OAuth token refresh response did not include access_token"))
}

/// GenerateKeywordHistoricalMetrics を実行し、生 JSON を返す。
///
/// body は [`build_historical_metrics_request_default`] の出力から customerId を除いたもの。
/// customer_id / login-customer-id はハイフンを除去する(Python と一致)。
pub async fn fetch_historical_metrics(
    cfg: &GoogleAdsConfig,
    customer_id: &str,
    keywords: &[String],
    location_ids: &[String],
) -> anyhow::Result<Value> {
    let cid = customer_id.replace('-', "");
    let body = build_historical_metrics_request_default(&cid, keywords, location_ids)?;
    post_historical_metrics(cfg, &cid, body).await
}

/// 組み立て済み body で generateKeywordHistoricalMetrics を叩く共通部分。
/// customerId は URL パスにのみ現れるため body からは取り除く。
async fn post_historical_metrics(
    cfg: &GoogleAdsConfig,
    cid: &str,
    mut body: Value,
) -> anyhow::Result<Value> {
    let token = refresh_access_token(cfg).await?;
    if let Some(obj) = body.as_object_mut() {
        obj.remove("customerId");
    }
    let url = format!(
        "{GOOGLE_ADS_API_ROOT}/{}/customers/{cid}:generateKeywordHistoricalMetrics",
        cfg.api_version
    );
    let client = reqwest::Client::new();
    // 地域別に連続照会するとアカウント単位のレート制限(429)に当たるため、
    // サーバ指定の retryDelay を尊重して再試行する。
    let resp = post_json_with_retry(
        &client,
        &url,
        &token,
        cfg,
        &body,
        "generateKeywordHistoricalMetrics failed",
    )
    .await?;
    Ok(resp.json().await?)
}

/// 期間指定つきで GenerateKeywordHistoricalMetrics を実行する(最大 48ヶ月)。
///
/// `end_year` / `end_month` は終端年月(この月を含む)。現在時刻をこの関数の中では
/// 取得しない(呼び出し側が終端を決める = テスト可能・時計非依存)。
pub async fn fetch_historical_metrics_range(
    cfg: &GoogleAdsConfig,
    customer_id: &str,
    keywords: &[String],
    location_ids: &[String],
    end_year: i32,
    end_month: u32,
    months_back: u32,
) -> anyhow::Result<Value> {
    let cid = customer_id.replace('-', "");
    let body = build_historical_metrics_request_range_default(
        &cid,
        keywords,
        location_ids,
        (end_year, end_month),
        months_back,
    )?;
    post_historical_metrics(cfg, &cid, body).await
}

/// 長期(最大 48ヶ月)の月次データを取る。終端年月は **API が返した最新月** を使う。
///
/// 手順: ①期間指定なしで 1 回叩く(=既定 12ヶ月、最新月が分かる)→ ②その最新月を終端に
/// `months_back` ヶ月を要求して叩き直す。months_back が既定 12 以下、または月次が
/// 1 件も返らなかった場合は ① の応答をそのまま返す(2 回目は叩かない)。
///
/// 時計(`SystemTime::now`)を使わないのは、Google Ads の月次データに公開遅延があり
/// 暦上の当月・前月がまだ存在しないことがあるため(存在しない月を終端に指定すると
/// API エラーになりうる)。API の実データを基準にすれば遅延に自動追従する。
pub async fn fetch_historical_metrics_long(
    cfg: &GoogleAdsConfig,
    customer_id: &str,
    keywords: &[String],
    location_ids: &[String],
    months_back: u32,
) -> anyhow::Result<Value> {
    let base = fetch_historical_metrics(cfg, customer_id, keywords, location_ids).await?;
    let n = clamp_months_back(months_back);
    if n <= DEFAULT_MONTHS_BACK {
        return Ok(base);
    }
    let (ey, em) = match latest_month_in_payload(&base) {
        Some(v) => v,
        None => return Ok(base), // 月次が無ければ基準月を決められない。
    };
    fetch_historical_metrics_range(cfg, customer_id, keywords, location_ids, ey, em, n).await
}

/// GenerateKeywordIdeas を実行し、生 JSON({"results":[...]})を返す。
///
/// 関連キーワードのサジェスト(検索量順の材料)を Google Ads のみ・無料で得るための呼び出し。
/// [`fetch_historical_metrics`] と同じ認証・ヘッダ・クライアント様式。
/// customerId は body に**入れない**(URL パスの `customers/{cid}` にのみ現れる)。
/// body: language(languageConstants/1005)/ geoTargetConstants / keywordPlanNetwork
/// (GOOGLE_SEARCH_AND_PARTNERS)/ keywordSeed.keywords。location_ids 空なら
/// geoTargetConstants は空配列。nextPageToken があればページ送りで results を全連結する。
pub async fn generate_keyword_ideas(
    cfg: &GoogleAdsConfig,
    customer_id: &str,
    seeds: &[String],
    location_ids: &[String],
) -> anyhow::Result<Value> {
    // seed を strip → 空除去 → 順序保持で重複排除(履歴指標と同じ規律)。
    let mut seen = std::collections::HashSet::new();
    let cleaned: Vec<String> = seeds
        .iter()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .filter(|k| seen.insert(k.clone()))
        .collect();
    if cleaned.is_empty() {
        return Err(RequestError::NoKeywords.into());
    }

    let token = refresh_access_token(cfg).await?;
    let cid = customer_id.replace('-', "");
    let url = format!(
        "{GOOGLE_ADS_API_ROOT}/{}/customers/{cid}:generateKeywordIdeas",
        cfg.api_version
    );
    let geo_targets: Vec<Value> = location_ids
        .iter()
        .map(|id| Value::String(format!("geoTargetConstants/{id}")))
        .collect();
    let client = reqwest::Client::new();

    let mut all_results: Vec<Value> = Vec::new();
    let mut page_token: Option<String> = None;
    loop {
        let mut body = serde_json::json!({
            "language": format!("languageConstants/{DEFAULT_LANGUAGE_ID}"),
            "geoTargetConstants": geo_targets.clone(),
            "keywordPlanNetwork": "GOOGLE_SEARCH_AND_PARTNERS",
            // 概念タグ(業種/サービス/ブランド区分)を各結果に付けさせる。追加課金なし。
            "keywordAnnotation": ["KEYWORD_CONCEPT"],
            "keywordSeed": {"keywords": cleaned.clone()},
        });
        if let (Some(obj), Some(tok)) = (body.as_object_mut(), page_token.as_ref()) {
            obj.insert("pageToken".into(), Value::String(tok.clone()));
        }
        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("developer-token", &cfg.developer_token)
            .header("login-customer-id", cfg.login_customer_id.replace('-', ""))
            .json(&body)
            .send()
            .await?;
        let resp = error_for_body(resp, "generateKeywordIdeas failed").await?;
        let payload: Value = resp.json().await?;
        if let Some(rows) = payload.get("results").and_then(Value::as_array) {
            all_results.extend(rows.iter().cloned());
        }
        let next = payload
            .get("nextPageToken")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        match next {
            Some(tok) => page_token = Some(tok),
            None => break,
        }
    }
    Ok(serde_json::json!({"results": all_results}))
}

/// GeoTargetConstants:suggest を実行し、候補配列を返す(Python `suggest_geo_targets`)。
///
/// headers: developer-token / Authorization、login-customer-id は任意。
/// body={"locale":"ja","countryCode":"JP","locationNames":{"names":[...]}}。
pub async fn suggest_geo_targets(
    cfg: &GoogleAdsConfig,
    names: &[String],
) -> anyhow::Result<Vec<GeoCandidate>> {
    let cleaned: Vec<String> = names
        .iter()
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .collect();
    if cleaned.is_empty() {
        return Ok(Vec::new());
    }
    let token = refresh_access_token(cfg).await?;
    let url = format!(
        "{GOOGLE_ADS_API_ROOT}/{}/geoTargetConstants:suggest",
        cfg.api_version
    );
    let body = serde_json::json!({
        "locale": "ja",
        "countryCode": "JP",
        "locationNames": {"names": cleaned},
    });
    let client = reqwest::Client::new();
    let mut builder = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("developer-token", &cfg.developer_token)
        .json(&body);
    if !cfg.login_customer_id.is_empty() {
        builder = builder.header("login-customer-id", cfg.login_customer_id.replace('-', ""));
    }
    let resp = builder.send().await?;
    let resp = error_for_body(resp, "geoTargetConstants:suggest failed").await?;
    let payload: Value = resp.json().await?;
    Ok(parse_geo_candidates(&payload))
}

// ===========================================================================
// 出稿予測指標(KeywordPlanIdeaService.GenerateKeywordForecastMetrics)。
// Google Ads のみ・無料。SerpApi は使わない。
// ===========================================================================

/// JPY の micros 換算係数。**1 円 = 1,000,000 micros**(実 API で確認済み)。
pub const MICROS_PER_YEN: f64 = 1_000_000.0;
/// 入札単価 micros の下限(= 1 円)。これ未満は rangeError TOO_LOW で弾かれる。
pub const MIN_BID_MICROS: i64 = 1_000_000;
/// 予測の既定マッチタイプ。
pub const DEFAULT_MATCH_TYPE: &str = "PHRASE";
/// 使えるマッチタイプ(これ以外が来たら [`DEFAULT_MATCH_TYPE`] に落とす)。
pub const ALLOWED_MATCH_TYPES: [&str; 3] = ["PHRASE", "EXACT", "BROAD"];

/// 円 → micros(四捨五入した整数)。負値・非有限は 0。
pub fn yen_to_micros(yen: f64) -> i64 {
    if !yen.is_finite() || yen <= 0.0 {
        return 0;
    }
    (yen * MICROS_PER_YEN).round() as i64
}

/// micros → 円。
pub fn micros_to_yen(micros: f64) -> f64 {
    micros / MICROS_PER_YEN
}

/// マッチタイプの正規化(大文字化 → 許可リスト外は既定 PHRASE)。
pub fn normalize_match_type(raw: &str) -> &'static str {
    let up = raw.trim().to_ascii_uppercase();
    for allowed in ALLOWED_MATCH_TYPES {
        if up == allowed {
            return allowed;
        }
    }
    DEFAULT_MATCH_TYPE
}

/// `yyyy-mm-dd` の形か(桁数と範囲のみ。暦の妥当性=2月30日等までは見ない)。
pub fn valid_forecast_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    if !b
        .iter()
        .enumerate()
        .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    let month: u32 = s[5..7].parse().unwrap_or(0);
    let day: u32 = s[8..10].parse().unwrap_or(0);
    (1..=12).contains(&month) && (1..=31).contains(&day)
}

/// GenerateKeywordForecastMetrics のリクエスト body を組み立てる(HTTP は送らない)。
///
/// 実 API で 200 が返った形に一致させる:
/// - `forecastPeriod` は `{"startDate","endDate"}` を**直下**に置く
///   (dateInterval / dateRange というネストは存在しない)。
/// - 金額は **1 円 = 1,000,000 micros**。maxCpcBidMicros の下限は 1,000,000。
/// - customerId は body に入れない(URL パスの `customers/{cid}` にのみ現れる)。
pub fn build_forecast_request(
    keywords: &[String],
    location_ids: &[String],
    max_cpc_yen: f64,
    daily_budget_yen: f64,
    start_date: &str,
    end_date: &str,
    match_type: &str,
) -> Result<Value, RequestError> {
    // strip → 空除去 → 順序保持の重複排除(履歴指標と同じ規律)。
    let mut seen = std::collections::HashSet::new();
    let cleaned: Vec<String> = keywords
        .iter()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .filter(|k| seen.insert(k.clone()))
        .collect();
    if cleaned.is_empty() {
        return Err(RequestError::NoKeywords);
    }
    if location_ids.len() > 10 {
        return Err(RequestError::TooManyGeoTargets(location_ids.len()));
    }
    if !valid_forecast_date(start_date) {
        return Err(RequestError::InvalidDate(start_date.to_string()));
    }
    if !valid_forecast_date(end_date) {
        return Err(RequestError::InvalidDate(end_date.to_string()));
    }
    if start_date > end_date {
        return Err(RequestError::InvalidDate(format!(
            "{start_date} > {end_date}"
        )));
    }

    let mt = normalize_match_type(match_type);
    let max_cpc_micros = yen_to_micros(max_cpc_yen).max(MIN_BID_MICROS);
    let budget_micros = yen_to_micros(daily_budget_yen).max(MIN_BID_MICROS);

    let biddable: Vec<Value> = cleaned
        .iter()
        .map(|k| {
            serde_json::json!({
                "keyword": {"text": k, "matchType": mt},
                "maxCpcBidMicros": max_cpc_micros.to_string(),
            })
        })
        .collect();
    let geo_modifiers: Vec<Value> = location_ids
        .iter()
        .map(|id| serde_json::json!({"geoTargetConstant": format!("geoTargetConstants/{id}")}))
        .collect();

    Ok(serde_json::json!({
        "campaign": {
            "adGroups": [{"biddableKeywords": biddable}],
            "geoModifiers": geo_modifiers,
            "languageConstants": [format!("languageConstants/{DEFAULT_LANGUAGE_ID}")],
            "keywordPlanNetwork": DEFAULT_NETWORK,
            "biddingStrategy": {
                "manualCpcBiddingStrategy": {
                    "dailyBudgetMicros": budget_micros.to_string(),
                    "maxCpcBidMicros": max_cpc_micros.to_string(),
                }
            },
        },
        "forecastPeriod": {"startDate": start_date, "endDate": end_date},
    }))
}

/// 予測指標(micros はすべて円へ換算済み)。
#[derive(Debug, Clone, PartialEq)]
pub struct ForecastMetrics {
    pub impressions: f64,
    pub clicks: f64,
    pub ctr: f64,
    pub average_cpc_yen: f64,
    pub cost_yen: f64,
    /// 以下 3 つは Google 側の一般推定。クリック・費用・CTR より信頼度が低い。
    pub conversions: Option<f64>,
    pub conversion_rate: Option<f64>,
    pub average_cpa_yen: Option<f64>,
}

/// JSON の数値を寛容に f64 で読む(REST は int64 を文字列で返すため)。
fn as_f64_loose(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// GenerateKeywordForecastMetrics のレスポンスを [`ForecastMetrics`] に解析する。
///
/// `campaignForecastMetrics` が無ければ `None`。必須系(impressions/clicks/ctr/
/// averageCpcMicros/costMicros)が欠けている場合は 0.0 として扱う(API は 0 予測時に
/// フィールドごと省くことがあるため)。micros は ÷1,000,000 で円に直す。
pub fn parse_forecast(payload: &Value) -> Option<ForecastMetrics> {
    let m = payload.get("campaignForecastMetrics")?;
    if !m.is_object() {
        return None;
    }
    Some(ForecastMetrics {
        impressions: as_f64_loose(m.get("impressions")).unwrap_or(0.0),
        clicks: as_f64_loose(m.get("clicks")).unwrap_or(0.0),
        ctr: as_f64_loose(m.get("clickThroughRate")).unwrap_or(0.0),
        average_cpc_yen: as_f64_loose(m.get("averageCpcMicros"))
            .map(micros_to_yen)
            .unwrap_or(0.0),
        cost_yen: as_f64_loose(m.get("costMicros"))
            .map(micros_to_yen)
            .unwrap_or(0.0),
        conversions: as_f64_loose(m.get("conversions")),
        conversion_rate: as_f64_loose(m.get("conversionRate")),
        average_cpa_yen: as_f64_loose(m.get("averageCpaMicros")).map(micros_to_yen),
    })
}

/// GenerateKeywordForecastMetrics を実行し、生 JSON を返す。
///
/// 認証・ヘッダは [`fetch_historical_metrics`] と同一(Authorization Bearer /
/// developer-token / login-customer-id はハイフン除去)。金額は円で受け取り
/// micros(×1,000,000)へ変換する。エラーは握りつぶさず Google 側 body 付きで返す。
pub async fn generate_keyword_forecast(
    cfg: &GoogleAdsConfig,
    customer_id: &str,
    keywords: &[String],
    location_ids: &[String],
    max_cpc_yen: f64,
    daily_budget_yen: f64,
    start_date: &str,
    end_date: &str,
    match_type: &str,
) -> anyhow::Result<Value> {
    let body = build_forecast_request(
        keywords,
        location_ids,
        max_cpc_yen,
        daily_budget_yen,
        start_date,
        end_date,
        match_type,
    )?;
    let token = refresh_access_token(cfg).await?;
    let cid = customer_id.replace('-', "");
    let url = format!(
        "{GOOGLE_ADS_API_ROOT}/{}/customers/{cid}:generateKeywordForecastMetrics",
        cfg.api_version
    );
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("developer-token", &cfg.developer_token)
        .header("login-customer-id", cfg.login_customer_id.replace('-', ""))
        .json(&body)
        .send()
        .await?;
    let resp = error_for_body(resp, "generateKeywordForecastMetrics failed").await?;
    Ok(resp.json().await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_shape_matches_python() {
        let kws = vec!["看護師 求人".to_string(), "介護 求人".to_string()];
        let geos = vec!["1009307".to_string()];
        let req = build_historical_metrics_request_default("1234567890", &kws, &geos).unwrap();
        assert_eq!(req["customerId"], "1234567890");
        assert_eq!(req["keywords"][0], "看護師 求人");
        assert_eq!(req["keywords"][1], "介護 求人");
        assert_eq!(req["language"], "languageConstants/1005");
        assert_eq!(req["geoTargetConstants"][0], "geoTargetConstants/1009307");
        assert_eq!(req["keywordPlanNetwork"], "GOOGLE_SEARCH");
    }

    #[test]
    fn keywords_are_stripped_and_deduped_in_order() {
        let kws = vec![
            "  看護師 求人 ".to_string(),
            "介護 求人".to_string(),
            "看護師 求人".to_string(), // 重複(trim後)
            "   ".to_string(),         // 空
        ];
        let req = build_historical_metrics_request_default("c", &kws, &[]).unwrap();
        let arr = req["keywords"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0], "看護師 求人");
        assert_eq!(arr[1], "介護 求人");
    }

    #[test]
    fn empty_keywords_errors() {
        let kws = vec!["   ".to_string()];
        assert_eq!(
            build_historical_metrics_request_default("c", &kws, &[]),
            Err(RequestError::NoKeywords)
        );
    }

    #[test]
    fn too_many_geo_targets_errors() {
        let kws = vec!["看護師 求人".to_string()];
        let geos: Vec<String> = (0..11).map(|i| i.to_string()).collect();
        assert_eq!(
            build_historical_metrics_request_default("c", &kws, &geos),
            Err(RequestError::TooManyGeoTargets(11))
        );
    }

    #[test]
    fn parse_handles_string_and_numeric_volumes() {
        let payload = json!({
            "results": [
                {"text": "看護師 求人", "keywordMetrics": {"avgMonthlySearches": "22200"}},
                {"text": "介護 求人",   "keywordMetrics": {"avgMonthlySearches": 5400}},
                {"text": "存在しない語", "keywordMetrics": {}},
                {"text": "指標欠落語"}
            ]
        });
        let got = parse_historical_metrics(&payload);
        assert_eq!(got.len(), 4);
        assert_eq!(got[0], ("看護師 求人".to_string(), Some(22200)));
        assert_eq!(got[1], ("介護 求人".to_string(), Some(5400)));
        assert_eq!(got[2], ("存在しない語".to_string(), None));
        assert_eq!(got[3], ("指標欠落語".to_string(), None));
    }

    #[test]
    fn parse_feeds_build_volume_map() {
        // parse_historical_metrics の出力を keywords::build_volume_map に食わせられる。
        let payload = json!({
            "results": [
                {"text": "看護 師 求人", "keywordMetrics": {"avgMonthlySearches": "1000"}}
            ]
        });
        let rows = parse_historical_metrics(&payload); // Vec<(String, Option<i64>)>
        // build_volume_map は (String, V) を取るので Option を V として渡す。
        let requested = vec!["看護師 求人".to_string()];
        let mapped = crate::media_engine::keywords::build_volume_map(&requested, &rows);
        // 再トークナイズ吸収で Some(Some(1000)) に一致。
        assert_eq!(mapped[0].1, Some(Some(1000)));
    }

    #[test]
    fn empty_payload_yields_empty() {
        assert!(parse_historical_metrics(&json!({})).is_empty());
        assert!(parse_historical_metrics(&json!({"results": []})).is_empty());
    }

    // --- parse_keyword_metrics(月次内訳＋競合度＋CPC)---

    #[test]
    fn parse_keyword_metrics_extracts_all_fields() {
        // monthlySearchVolumes / competition / competitionIndex / bid micros を含む応答。
        // int64 系は文字列(monthlySearches / competitionIndex / bid)でも数値でも受ける。
        let payload = json!({
            "results": [
                {
                    "text": "看護師 求人",
                    "keywordMetrics": {
                        "avgMonthlySearches": "22200",
                        "competition": "HIGH",
                        "competitionIndex": "73",
                        "lowTopOfPageBidMicros": "150000000",   // 150 円
                        "highTopOfPageBidMicros": 480500000_i64, // 480.5 円(数値)
                        "monthlySearchVolumes": [
                            {"month": "MARCH",   "year": "2025", "monthlySearches": "1234"},
                            {"month": "APRIL",   "year": 2025,   "monthlySearches": 5600},
                            {"month": "DECEMBER","year": "2025", "monthlySearches": "999"}
                        ]
                    }
                }
            ]
        });
        let got = parse_keyword_metrics(&payload);
        assert_eq!(got.len(), 1);
        let m = &got[0];
        assert_eq!(m.keyword, "看護師 求人");
        assert_eq!(m.avg_monthly, Some(22200));
        assert_eq!(m.competition, "HIGH");
        assert_eq!(m.competition_index, Some(73));
        assert_eq!(m.bid_low_yen, Some(150.0));
        assert_eq!(m.bid_high_yen, Some(480.5));
        assert_eq!(
            m.monthly_12m,
            vec![
                ("2025-03".to_string(), 1234),
                ("2025-04".to_string(), 5600), // year が数値でも文字列化
                ("2025-12".to_string(), 999),
            ]
        );
    }

    #[test]
    fn parse_keyword_metrics_handles_missing_optionals() {
        // keywordMetrics が空/欠損でもクラッシュせず None/空で返す。
        let payload = json!({
            "results": [
                {"text": "指標なし語", "keywordMetrics": {}},
                {"text": "keywordMetrics 欠落語"},
                {"keywordMetrics": {"avgMonthlySearches": 10}} // text 欠落 → スキップ
            ]
        });
        let got = parse_keyword_metrics(&payload);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].keyword, "指標なし語");
        assert_eq!(got[0].avg_monthly, None);
        assert_eq!(got[0].competition, "");
        assert_eq!(got[0].competition_index, None);
        assert_eq!(got[0].bid_low_yen, None);
        assert!(got[0].monthly_12m.is_empty());
        assert_eq!(got[1].keyword, "keywordMetrics 欠落語");
    }

    #[test]
    fn parse_keyword_metrics_empty_payload() {
        assert!(parse_keyword_metrics(&json!({})).is_empty());
        assert!(parse_keyword_metrics(&json!({"results": []})).is_empty());
    }

    #[test]
    fn parse_keyword_metrics_reads_keyword_idea_metrics_key() {
        // generateKeywordIdeas 応答は keywordMetrics ではなく keywordIdeaMetrics に指標を持つ。
        // 同じ parse_keyword_metrics が avgMonthlySearches / competition / bid を抽出できること。
        let payload = json!({
            "results": [
                {
                    "text": "看護師 求人 東京",
                    "keywordIdeaMetrics": {
                        "avgMonthlySearches": "8100",
                        "competition": "MEDIUM",
                        "competitionIndex": "42",
                        "lowTopOfPageBidMicros": "120000000",   // 120 円
                        "highTopOfPageBidMicros": 350000000_i64, // 350 円
                        "monthlySearchVolumes": [
                            {"month": "JANUARY", "year": "2025", "monthlySearches": "700"}
                        ]
                    }
                },
                {
                    // 履歴指標側キーが残っていても従来どおり拾える(後方互換)。
                    "text": "看護師 求人",
                    "keywordMetrics": {"avgMonthlySearches": 22200}
                }
            ]
        });
        let got = parse_keyword_metrics(&payload);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].keyword, "看護師 求人 東京");
        assert_eq!(got[0].avg_monthly, Some(8100));
        assert_eq!(got[0].competition, "MEDIUM");
        assert_eq!(got[0].competition_index, Some(42));
        assert_eq!(got[0].bid_low_yen, Some(120.0));
        assert_eq!(got[0].bid_high_yen, Some(350.0));
        assert_eq!(got[0].monthly_12m, vec![("2025-01".to_string(), 700)]);
        // keywordMetrics(履歴指標キー)も引き続き読める。
        assert_eq!(got[1].avg_monthly, Some(22200));
    }

    // --- 期間指定(最大48ヶ月)---

    #[test]
    fn month_name_round_trip() {
        for m in 1..=12u32 {
            let name = month_num_to_name(m).unwrap();
            let num = month_name_to_num(name).unwrap();
            assert_eq!(num.parse::<u32>().unwrap(), m, "{name}");
        }
        assert!(month_num_to_name(0).is_none());
        assert!(month_num_to_name(13).is_none());
    }

    #[test]
    fn range_start_counts_end_month_inclusive() {
        // 48ヶ月: 2022-08 .. 2026-07(終端を含む)。
        assert_eq!(range_start_from_end(2026, 7, 48), (2022, 8));
        // 12ヶ月: 2025-08 .. 2026-07。
        assert_eq!(range_start_from_end(2026, 7, 12), (2025, 8));
        // 1ヶ月は同月。
        assert_eq!(range_start_from_end(2026, 7, 1), (2026, 7));
        // 年跨ぎ(1月終端)。
        assert_eq!(range_start_from_end(2026, 1, 13), (2025, 1));
        // 上限クランプ(49→48 と同じ)。
        assert_eq!(range_start_from_end(2026, 7, 999), range_start_from_end(2026, 7, 48));
        // 下限クランプ(0→1)。
        assert_eq!(range_start_from_end(2026, 7, 0), (2026, 7));
    }

    #[test]
    fn range_request_adds_year_month_range_only() {
        let kws = vec!["看護師 求人".to_string()];
        let geos = vec!["1009307".to_string()];
        let req =
            build_historical_metrics_request_range_default("1234567890", &kws, &geos, (2026, 7), 48)
                .unwrap();
        // 既存キーは従来どおり(後方互換)。
        assert_eq!(req["customerId"], "1234567890");
        assert_eq!(req["keywords"][0], "看護師 求人");
        assert_eq!(req["language"], "languageConstants/1005");
        assert_eq!(req["geoTargetConstants"][0], "geoTargetConstants/1009307");
        assert_eq!(req["keywordPlanNetwork"], "GOOGLE_SEARCH");
        // 追加分: year=数値、month=英大文字の月名。
        let r = &req["historicalMetricsOptions"]["yearMonthRange"];
        assert_eq!(r["start"]["year"], 2022);
        assert_eq!(r["start"]["month"], "AUGUST");
        assert_eq!(r["end"]["year"], 2026);
        assert_eq!(r["end"]["month"], "JULY");
        // 期間なしの従来リクエストには historicalMetricsOptions は付かない。
        let plain = build_historical_metrics_request_default("1234567890", &kws, &geos).unwrap();
        assert!(plain.get("historicalMetricsOptions").is_none());
    }

    #[test]
    fn range_request_rejects_invalid_month() {
        let kws = vec!["a".to_string()];
        assert_eq!(
            build_historical_metrics_request_range(
                "c", &kws, &[], DEFAULT_LANGUAGE_ID, DEFAULT_NETWORK, (2026, 0), (2026, 7)
            ),
            Err(RequestError::InvalidMonth(0))
        );
        assert_eq!(
            build_historical_metrics_request_range(
                "c", &kws, &[], DEFAULT_LANGUAGE_ID, DEFAULT_NETWORK, (2026, 1), (2026, 13)
            ),
            Err(RequestError::InvalidMonth(13))
        );
    }

    #[test]
    fn latest_month_in_payload_picks_max_across_keywords() {
        let payload = json!({
            "results": [
                {"text": "a", "keywordMetrics": {"monthlySearchVolumes": [
                    {"month": "NOVEMBER", "year": "2025", "monthlySearches": "1"},
                    {"month": "JANUARY",  "year": 2026,   "monthlySearches": 2}
                ]}},
                {"text": "b", "keywordIdeaMetrics": {"monthlySearchVolumes": [
                    {"month": "JUNE", "year": "2026", "monthlySearches": "3"}
                ]}},
                {"text": "c", "keywordMetrics": {}}
            ]
        });
        assert_eq!(latest_month_in_payload(&payload), Some((2026, 6)));
        // 月次が無ければ None(0 埋めしない)。
        assert_eq!(latest_month_in_payload(&json!({"results": []})), None);
        assert_eq!(latest_month_in_payload(&json!({})), None);
    }

    #[test]
    fn parse_keyword_metrics_keeps_all_48_months() {
        // 48ヶ月分の月次をそのまま全件保持する(件数の切り詰めをしない)。
        let mut rows = Vec::new();
        for i in 0..48u32 {
            let (y, m) = range_start_from_end(2026, 7, 48 - i);
            rows.push(json!({
                "month": month_num_to_name(m).unwrap(),
                "year": y,
                "monthlySearches": (i as i64) * 10,
            }));
        }
        let payload = json!({"results": [{"text": "kw", "keywordMetrics": {
            "avgMonthlySearches": 100, "monthlySearchVolumes": rows
        }}]});
        let got = parse_keyword_metrics(&payload);
        assert_eq!(got[0].monthly_12m.len(), 48);
        assert_eq!(got[0].monthly_12m[0].0, "2022-08");
        assert_eq!(got[0].monthly_12m[47].0, "2026-07");
    }

    // --- 概念タグ(keywordAnnotations)---

    #[test]
    fn concept_group_is_brand_prefers_type_enum() {
        // type が最優先(ロケール非依存)。
        assert!(concept_group_is_brand("何らかの群", Some("BRAND")));
        assert!(concept_group_is_brand("何らかの群", Some("OTHER_BRANDS")));
        assert!(!concept_group_is_brand("他のブランド", Some("NON_BRAND")));
        // type 欠落/UNSPECIFIED は名前判定へフォールバック。
        assert!(concept_group_is_brand("他のブランド", None));
        assert!(concept_group_is_brand("他のブランド", Some("UNSPECIFIED")));
        assert!(!concept_group_is_brand("ノンブランド", None));
        assert!(!concept_group_is_brand("サービス", None));
        assert!(!concept_group_is_brand("職種", None));
        // 英語ロケール。
        assert!(concept_group_is_brand("Other brands", None));
        assert!(!concept_group_is_brand("Non-Brands", None));
    }

    #[test]
    fn parse_keyword_metrics_extracts_concepts_and_brand_flag() {
        let payload = json!({
            "results": [
                {
                    "text": "タクシー 求人",
                    "keywordIdeaMetrics": {"avgMonthlySearches": "1000"},
                    "keywordAnnotations": {"concepts": [
                        {"name": "タクシー", "conceptGroup": {"name": "サービス", "type": "OTHER"}},
                        {"name": "ノンブランド", "conceptGroup": {"name": "ノンブランド", "type": "NON_BRAND"}}
                    ]}
                },
                {
                    "text": "日本交通 求人",
                    "keywordIdeaMetrics": {"avgMonthlySearches": "500"},
                    "keywordAnnotations": {"concepts": [
                        {"name": "日本交通", "conceptGroup": {"name": "他のブランド"}}
                    ]}
                },
                {
                    // 概念タグ無し(履歴指標応答等)→ 空 + is_brand=false(後方互換)。
                    "text": "介護 求人",
                    "keywordMetrics": {"avgMonthlySearches": "300"}
                }
            ]
        });
        let got = parse_keyword_metrics(&payload);
        assert_eq!(got.len(), 3);
        assert_eq!(
            got[0].concepts,
            vec![
                ("サービス".to_string(), "タクシー".to_string()),
                ("ノンブランド".to_string(), "ノンブランド".to_string()),
            ]
        );
        assert!(!got[0].is_brand);
        assert_eq!(got[1].concepts, vec![("他のブランド".to_string(), "日本交通".to_string())]);
        assert!(got[1].is_brand, "type 欠落でもグループ名『他のブランド』でブランド判定");
        assert!(got[2].concepts.is_empty());
        assert!(!got[2].is_brand);
    }

    // --- GeoTargetConstants:suggest 解析 & pick_geo_id(純粋部分) ---

    fn cand(
        id: &str,
        name: &str,
        target_type: &str,
        status: &str,
        reach: i64,
    ) -> GeoCandidate {
        GeoCandidate {
            id: Some(id.to_string()),
            name: Some(name.to_string()),
            canonical_name: Some(format!("{name},Japan")),
            target_type: Some(target_type.to_string()),
            status: Some(status.to_string()),
            reach: Some(reach),
        }
    }

    #[test]
    fn parse_geo_candidates_reads_fields_and_reach_string() {
        // reach は int64=文字列で来ることがある。
        let payload = json!({
            "geoTargetConstantSuggestions": [
                {
                    "reach": "123456",
                    "geoTargetConstant": {
                        "id": "1009540", "name": "Osaka",
                        "canonicalName": "Osaka,Osaka,Japan",
                        "targetType": "City", "status": "ENABLED"
                    }
                }
            ]
        });
        let got = parse_geo_candidates(&payload);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id.as_deref(), Some("1009540"));
        assert_eq!(got[0].reach, Some(123456));
        assert_eq!(got[0].target_type.as_deref(), Some("City"));
    }

    #[test]
    fn pick_prefers_exact_name_over_type_and_reach() {
        // 「東京都」→ 完全一致の Prefecture を選ぶ(reach 大の City より優先)。
        let cands = vec![
            cand("1009307", "Tokyo", "City", "ENABLED", 9_000_000),
            cand("20636", "東京都", "Prefecture", "ENABLED", 1_000),
        ];
        let pick = pick_geo_id(&cands, "東京都").unwrap();
        assert_eq!(pick.id, "20636");
        assert_eq!(pick.target_type.as_deref(), Some("Prefecture"));
    }

    #[test]
    fn pick_city_over_prefecture_on_type_when_no_exact() {
        // 完全一致が無ければ種別ランクで City を優先。
        let cands = vec![
            cand("P", "近隣県", "Prefecture", "ENABLED", 5_000_000),
            cand("C", "近隣市", "City", "ENABLED", 100),
        ];
        let pick = pick_geo_id(&cands, "存在しない語").unwrap();
        assert_eq!(pick.id, "C");
    }

    #[test]
    fn pick_reach_breaks_type_ties() {
        // 同種別・非完全一致なら reach 大を選ぶ。
        let cands = vec![
            cand("small", "市A", "City", "ENABLED", 100),
            cand("big", "市B", "City", "ENABLED", 900),
        ];
        let pick = pick_geo_id(&cands, "query").unwrap();
        assert_eq!(pick.id, "big");
    }

    #[test]
    fn pick_ignores_disabled_and_missing_id() {
        let cands = vec![
            cand("x", "渋谷区", "City", "REMOVED", 1000), // 無効
            GeoCandidate {
                id: None,
                name: Some("渋谷区".into()),
                canonical_name: None,
                target_type: Some("City".into()),
                status: Some("ENABLED".into()),
                reach: Some(9999),
            }, // id 欠落
            cand("1009308", "渋谷区", "City", "ENABLED", 500),
        ];
        let pick = pick_geo_id(&cands, "渋谷区").unwrap();
        assert_eq!(pick.id, "1009308");
    }

    #[test]
    fn pick_none_when_no_enabled() {
        let cands = vec![cand("x", "y", "City", "REMOVED", 1)];
        assert!(pick_geo_id(&cands, "y").is_none());
        assert!(pick_geo_id(&[], "y").is_none());
    }

    // --- 県ヒント(prefer_prefecture)---

    fn cand_canon(
        id: &str,
        name: &str,
        canonical: &str,
        target_type: &str,
        reach: i64,
    ) -> GeoCandidate {
        GeoCandidate {
            id: Some(id.to_string()),
            name: Some(name.to_string()),
            canonical_name: Some(canonical.to_string()),
            target_type: Some(target_type.to_string()),
            status: Some("ENABLED".to_string()),
            reach: Some(reach),
        }
    }

    #[test]
    fn pref_hint_disambiguates_same_name_city_across_prefectures() {
        // 「府中市」= 東京(reach 大)/ 広島。県ヒント Hiroshima で広島側を選ぶ。
        let cands = vec![
            cand_canon("tokyo_fuchu", "府中市", "Fuchu,Tokyo,Japan", "City", 900_000),
            cand_canon("hiroshima_fuchu", "府中市", "Fuchu,Hiroshima,Japan", "City", 50_000),
        ];
        // 県ヒント無し → 完全一致同士で reach 大(東京)。
        let no_hint = pick_geo_id(&cands, "府中市").unwrap();
        assert_eq!(no_hint.id, "tokyo_fuchu");
        // 県ヒント Hiroshima → 広島側。
        let hinted = pick_geo_id_pref(&cands, "府中市", Some("Hiroshima")).unwrap();
        assert_eq!(hinted.id, "hiroshima_fuchu");
    }

    #[test]
    fn pref_hint_ranks_below_exact_match_and_above_type() {
        // 県一致は完全一致の「次」・種別の「前」。
        // 候補: ①完全一致だが別県 City ②非完全一致だが県一致 Prefecture
        // → 完全一致(①)が勝つ(pref より exact 優先)。
        let cands = vec![
            cand_canon("exact_other", "高崎市", "Takasaki,Nagano,Japan", "City", 100),
            cand_canon("pref_pref", "群馬", "Gunma,Japan", "Prefecture", 100),
        ];
        let pick = pick_geo_id_pref(&cands, "高崎市", Some("Gunma")).unwrap();
        assert_eq!(pick.id, "exact_other");

        // 完全一致が無い場合、県一致が種別ランクより優先される。
        // ①県一致 Prefecture(type_rank=3) ②県不一致 City(type_rank=0)
        // → 県一致(①)が勝つ。
        let cands2 = vec![
            cand_canon("pref_match_pref", "近隣県都市", "X,Gunma,Japan", "Prefecture", 100),
            cand_canon("other_city", "近隣別県市", "Y,Saitama,Japan", "City", 100),
        ];
        let pick2 = pick_geo_id_pref(&cands2, "存在しない語", Some("Gunma")).unwrap();
        assert_eq!(pick2.id, "pref_match_pref");
    }

    #[test]
    fn pref_hint_none_matches_legacy_behavior() {
        // 県ヒント None は従来の pick_geo_id と同一。
        let cands = vec![
            cand("small", "市A", "City", "ENABLED", 100),
            cand("big", "市B", "City", "ENABLED", 900),
        ];
        assert_eq!(
            pick_geo_id_pref(&cands, "query", None),
            pick_geo_id(&cands, "query")
        );
    }

    // ---------------------------------------------------------------------
    // 出稿予測指標(generateKeywordForecastMetrics)
    // ---------------------------------------------------------------------

    /// 実 API が 200 で返した形のモック応答。
    fn forecast_payload() -> Value {
        json!({"campaignForecastMetrics": {
            "impressions": 26868.06,
            "clickThroughRate": 0.036,
            "averageCpcMicros": "159912496",
            "clicks": 969.28,
            "costMicros": "155000000000",
            "conversions": 8.27,
            "conversionRate": 0.0085,
            "averageCpaMicros": "18723322488"
        }})
    }

    #[test]
    fn yen_to_micros_uses_one_million_per_yen() {
        // JPY は 1 円 = 1,000,000 micros。300 円 → "300000000"。
        assert_eq!(yen_to_micros(300.0), 300_000_000);
        assert_eq!(yen_to_micros(1.0), MIN_BID_MICROS);
        assert_eq!(yen_to_micros(5000.0), 5_000_000_000);
        assert_eq!(yen_to_micros(0.5), 500_000);
        // 非有限・負値は 0(呼び出し側で下限クランプする)。
        assert_eq!(yen_to_micros(-3.0), 0);
        assert_eq!(yen_to_micros(f64::NAN), 0);
    }

    #[test]
    fn micros_to_yen_roundtrips() {
        assert_eq!(micros_to_yen(300_000_000.0), 300.0);
        assert_eq!(micros_to_yen(yen_to_micros(1234.0) as f64), 1234.0);
    }

    #[test]
    fn parse_forecast_extracts_all_fields_in_yen() {
        let m = parse_forecast(&forecast_payload()).unwrap();
        assert!((m.impressions - 26868.06).abs() < 1e-6);
        assert!((m.clicks - 969.28).abs() < 1e-6);
        assert!((m.ctr - 0.036).abs() < 1e-9);
        // costMicros 155,000,000,000 → 155,000 円。
        assert!((m.cost_yen - 155_000.0).abs() < 1e-6);
        // averageCpcMicros 159,912,496 → 約 160 円。
        assert!((m.average_cpc_yen - 159.912496).abs() < 1e-6);
        assert!((m.average_cpc_yen - 160.0).abs() < 0.1);
        assert!((m.conversions.unwrap() - 8.27).abs() < 1e-6);
        assert!((m.conversion_rate.unwrap() - 0.0085).abs() < 1e-9);
        // averageCpaMicros 18,723,322,488 → 約 18,723 円。
        assert!((m.average_cpa_yen.unwrap() - 18_723.322488).abs() < 1e-6);
    }

    #[test]
    fn parse_forecast_handles_missing_and_optional_fields() {
        // campaignForecastMetrics が無ければ None。
        assert!(parse_forecast(&json!({})).is_none());
        assert!(parse_forecast(&json!({"campaignForecastMetrics": "x"})).is_none());
        // CV 系が無ければ None、必須系が無ければ 0.0。
        let m = parse_forecast(&json!({"campaignForecastMetrics": {"clicks": 5.0}})).unwrap();
        assert_eq!(m.clicks, 5.0);
        assert_eq!(m.impressions, 0.0);
        assert_eq!(m.cost_yen, 0.0);
        assert!(m.conversions.is_none());
        assert!(m.average_cpa_yen.is_none());
    }

    #[test]
    fn forecast_request_matches_verified_shape() {
        let kws = vec!["ドライバー 求人".to_string()];
        let geos = vec!["2392".to_string()];
        let req =
            build_forecast_request(&kws, &geos, 300.0, 5000.0, "2026-08-01", "2026-08-31", "PHRASE")
                .unwrap();
        let bk = &req["campaign"]["adGroups"][0]["biddableKeywords"][0];
        assert_eq!(bk["keyword"]["text"], "ドライバー 求人");
        assert_eq!(bk["keyword"]["matchType"], "PHRASE");
        assert_eq!(bk["maxCpcBidMicros"], "300000000");
        assert_eq!(
            req["campaign"]["geoModifiers"][0]["geoTargetConstant"],
            "geoTargetConstants/2392"
        );
        assert_eq!(req["campaign"]["languageConstants"][0], "languageConstants/1005");
        assert_eq!(req["campaign"]["keywordPlanNetwork"], "GOOGLE_SEARCH");
        let bs = &req["campaign"]["biddingStrategy"]["manualCpcBiddingStrategy"];
        assert_eq!(bs["dailyBudgetMicros"], "5000000000");
        assert_eq!(bs["maxCpcBidMicros"], "300000000");
        // forecastPeriod は startDate/endDate を直下に置く(ネストしない)。
        assert_eq!(req["forecastPeriod"]["startDate"], "2026-08-01");
        assert_eq!(req["forecastPeriod"]["endDate"], "2026-08-31");
        assert!(req["forecastPeriod"].get("dateInterval").is_none());
        assert!(req["forecastPeriod"].get("dateRange").is_none());
        // customerId は body に入れない(URL パスにのみ現れる)。
        assert!(req.get("customerId").is_none());
    }

    #[test]
    fn forecast_request_dedupes_keywords_and_clamps_min_bid() {
        let kws = vec![
            "  ドライバー 求人 ".to_string(),
            "ドライバー 求人".to_string(),
            "".to_string(),
            "介護 求人".to_string(),
        ];
        // 0.4 円は下限 1 円(1,000,000 micros)へクランプ。
        let req = build_forecast_request(&kws, &[], 0.4, 0.0, "2026-08-01", "2026-08-31", "").unwrap();
        let bks = req["campaign"]["adGroups"][0]["biddableKeywords"]
            .as_array()
            .unwrap();
        assert_eq!(bks.len(), 2);
        assert_eq!(bks[0]["keyword"]["text"], "ドライバー 求人");
        assert_eq!(bks[1]["keyword"]["text"], "介護 求人");
        assert_eq!(bks[0]["maxCpcBidMicros"], "1000000");
        assert_eq!(bks[0]["keyword"]["matchType"], "PHRASE"); // 空指定は既定 PHRASE
        assert_eq!(
            req["campaign"]["biddingStrategy"]["manualCpcBiddingStrategy"]["dailyBudgetMicros"],
            "1000000"
        );
        // 地域未指定は geoModifiers 空配列(全国)。
        assert_eq!(req["campaign"]["geoModifiers"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn forecast_request_rejects_bad_input() {
        assert_eq!(
            build_forecast_request(&[], &[], 300.0, 5000.0, "2026-08-01", "2026-08-31", "PHRASE"),
            Err(RequestError::NoKeywords)
        );
        let kws = vec!["a".to_string()];
        assert_eq!(
            build_forecast_request(&kws, &[], 300.0, 5000.0, "2026/08/01", "2026-08-31", "PHRASE"),
            Err(RequestError::InvalidDate("2026/08/01".to_string()))
        );
        assert_eq!(
            build_forecast_request(&kws, &[], 300.0, 5000.0, "2026-08-01", "2026-13-01", "PHRASE"),
            Err(RequestError::InvalidDate("2026-13-01".to_string()))
        );
        // start > end。
        assert_eq!(
            build_forecast_request(&kws, &[], 300.0, 5000.0, "2026-09-01", "2026-08-31", "PHRASE"),
            Err(RequestError::InvalidDate("2026-09-01 > 2026-08-31".to_string()))
        );
        let geos: Vec<String> = (0..11).map(|i| i.to_string()).collect();
        assert_eq!(
            build_forecast_request(&kws, &geos, 300.0, 5000.0, "2026-08-01", "2026-08-31", "PHRASE"),
            Err(RequestError::TooManyGeoTargets(11))
        );
    }

    #[test]
    fn match_type_is_normalized() {
        assert_eq!(normalize_match_type("exact"), "EXACT");
        assert_eq!(normalize_match_type(" broad "), "BROAD");
        assert_eq!(normalize_match_type("PHRASE"), "PHRASE");
        assert_eq!(normalize_match_type("なにこれ"), "PHRASE");
        assert_eq!(normalize_match_type(""), "PHRASE");
    }

    #[test]
    fn forecast_date_validation() {
        assert!(valid_forecast_date("2026-08-01"));
        assert!(valid_forecast_date("2026-12-31"));
        assert!(!valid_forecast_date("2026-00-01"));
        assert!(!valid_forecast_date("2026-08-32"));
        assert!(!valid_forecast_date("26-08-01"));
        assert!(!valid_forecast_date("2026-8-1"));
        assert!(!valid_forecast_date(""));
    }
}
