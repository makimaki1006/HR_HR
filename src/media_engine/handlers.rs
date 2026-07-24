//! キーワード需要ビューアのハンドラ群 (2026-07-24 HR_HR 統合)。
//!
//! 移植元 `job_media_engine_rs/src/main.rs` から**検索エンジン部のみ**を抽出
//! (引き継ぎ資料 4-2b の一覧に準拠。旧レポート系 /api/case・/api/report は移植しない)。
//!
//! HR_HR 統合での変更点:
//! - SerpApi の月次カウンタ・キャッシュは Turso 一次 ([`crate::media_engine::store`])
//! - Gemini はプロセス共通レートリミッタ (12回/分) を共有
//! - 重心 CSV の既定パスはアプリルート相対 `data/media_engine/…` (env で上書き可)
//!
//! 設計方針 (移植元の決定事項、遵守):
//! - ツールは断言しない (データの提示に留める)
//! - LLM に判断させない (/api/cluster は振り分けのみ、数値は Rust が実測値を引く)
//! - SerpApi は 1 レポート = 1 クエリ、消費は quota で明示

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::media_engine::config::{gemini_api_key, gemini_model, serpapi_key, GoogleAdsConfig};
use crate::media_engine::demand::{build_demand_map, Area, DemandMap};
use crate::media_engine::gemini;
use crate::media_engine::geo::{self, load_centroids};
use crate::media_engine::google_ads::{
    clamp_months_back, fetch_historical_metrics, fetch_historical_metrics_long,
    generate_keyword_forecast, generate_keyword_ideas, normalize_match_type, parse_forecast,
    parse_historical_metrics, parse_keyword_metrics, pick_geo_id_pref, suggest_geo_targets,
    valid_forecast_date, GeoPick, KeywordMetric, DEFAULT_MONTHS_BACK,
};
use crate::media_engine::keyword_cluster;
use crate::media_engine::keyword_demand::{
    concentration_from_metrics, concentration_json, recurring_peak_months, rotation_calendar,
    rotation_calendar_json, seasonality_summary, year_over_year,
};
use crate::media_engine::keywords::{build_volume_map, place_agnostic_keywords};
use crate::media_engine::media;
use crate::media_engine::serp::{self, organic_domains};
use crate::media_engine::serpapi;
use crate::media_engine::store;
use crate::AppState;

/// キーワード需要ビューアの UI ページ(同一オリジンで /api/keywords を叩く)。
pub async fn ui_keywords() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../../static/keywords.html"))
}

fn default_radius() -> f64 {
    30.0
}
fn default_max_areas() -> usize {
    12
}

/// 重心 CSV のパス(CENTROIDS_PATH 優先。既定は HR_HR アプリルート相対)。
fn centroids_path() -> PathBuf {
    match std::env::var("CENTROIDS_PATH") {
        Ok(p) if !p.is_empty() => PathBuf::from(p),
        _ => PathBuf::from("data/media_engine/municipality_centroids.csv"),
    }
}

fn default_noise_floor() -> i64 {
    50
}

/// `/api/keywords` の既定取得月数(Google Ads の期間指定なしと同じ 12ヶ月)。
fn default_months() -> u32 {
    DEFAULT_MONTHS_BACK
}

#[derive(Debug, Deserialize)]
pub struct KeywordsQuery {
    /// カンマ/改行区切りの複数キーワード。指定時はこれを優先。
    #[serde(default)]
    kw: Option<String>,
    /// 職種名。kw 未指定時に place_agnostic_keywords で地名なしKWを生成する。
    #[serde(default)]
    job: Option<String>,
    /// 任意の地域名(指定時のみ geo 解決して location_id を効かせる)。
    #[serde(default)]
    region: Option<String>,
    /// ローテカレンダーのノイズフロア(avg_monthly がこれ未満の KW を除外)。
    #[serde(default = "default_noise_floor")]
    noise_floor: i64,
    /// 取得する月次データの月数(既定 12、最大 48)。12 超で長期取得＋長期分析を付す。
    #[serde(default = "default_months")]
    months: u32,
}

/// kw(カンマ/改行区切り)を trim＋空除去＋順序保持で重複排除する。
fn split_keywords(raw: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    raw.split(['\n', ','])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .filter(|s| seen.insert(s.clone()))
        .collect()
}

pub async fn keywords_endpoint(Query(q): Query<KeywordsQuery>) -> Json<Value> {
    match run_keywords(q).await {
        Ok(v) => Json(v),
        Err(e) => Json(json!({"status": "error", "message": e.to_string()})),
    }
}

/// `GET /api/keywords`。Google Ads のみ・SerpApi 非依存でキーワード需要を返す。
async fn run_keywords(q: KeywordsQuery) -> anyhow::Result<Value> {
    let cfg = GoogleAdsConfig::from_env();
    let missing = cfg.missing();
    if !missing.is_empty() {
        return Ok(json!({
            "status": "missing_credentials",
            "message": "Google Ads の資格情報が未設定です",
            "missing": missing,
        }));
    }

    let keywords: Vec<String> = match q.kw.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => split_keywords(raw),
        None => match q.job.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(job) => place_agnostic_keywords(job),
            None => {
                return Ok(json!({
                    "status": "error",
                    "message": "kw(カンマ/改行区切り)または job のいずれかが必要です",
                }));
            }
        },
    };
    if keywords.is_empty() {
        return Ok(json!({"status": "error", "message": "有効なキーワードがありません"}));
    }

    let region = q.region.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let geo_pick = match region {
        Some(name) => resolve_geo(&cfg, name, None).await?,
        None => None,
    };
    let location_ids: Vec<String> =
        geo_pick.as_ref().map(|p| vec![p.id.clone()]).unwrap_or_default();

    let months = clamp_months_back(q.months);
    let long_range = months > DEFAULT_MONTHS_BACK;
    let resp = if long_range {
        fetch_historical_metrics_long(&cfg, &cfg.customer_id(), &keywords, &location_ids, months)
            .await?
    } else {
        fetch_historical_metrics(&cfg, &cfg.customer_id(), &keywords, &location_ids).await?
    };
    let metrics: Vec<KeywordMetric> = parse_keyword_metrics(&resp);

    let keyword_rows: Vec<Value> = metrics
        .iter()
        .map(|m| {
            json!({
                "keyword": m.keyword,
                "avg_monthly": m.avg_monthly,
                "monthly_12m": m.monthly_12m
                    .iter()
                    .map(|(mm, v)| json!({"month": mm, "search_volume": v}))
                    .collect::<Vec<_>>(),
                "months_count": m.monthly_12m.len(),
                "competition": m.competition,
                "competition_index": m.competition_index,
                "bid_low_yen": m.bid_low_yen,
                "bid_high_yen": m.bid_high_yen,
                "seasonality": seasonality_summary(m),
                "yoy": if long_range { year_over_year(m) } else { Value::Null },
                "recurring_peaks": if long_range { recurring_peak_months(m) } else { Value::Null },
            })
        })
        .collect();

    let rc = rotation_calendar(&metrics, q.noise_floor.max(0));
    let concentration = concentration_from_metrics(&metrics);

    Ok(json!({
        "status": "ok",
        "region": geo_pick.as_ref().map(|p| json!({
            "name": region,
            "geo_id": p.id,
            "geo_type": p.target_type,
            "canonical_name": p.canonical_name,
        })).unwrap_or(Value::Null),
        "noise_floor": q.noise_floor,
        "months": months,
        "keywords": keyword_rows,
        "rotation_calendar": rotation_calendar_json(&rc),
        "excluded_keywords": rc.excluded,
        "concentration": concentration_json(&concentration),
    }))
}

fn default_suggest_limit() -> usize {
    30
}

#[derive(Debug, Deserialize)]
pub struct SuggestQuery {
    #[serde(default)]
    seed: Option<String>,
    #[serde(default)]
    region: Option<String>,
    #[serde(default = "default_suggest_limit")]
    limit: usize,
    #[serde(default)]
    noise_floor: i64,
    #[serde(default)]
    exclude_brand: bool,
}

pub async fn suggest_endpoint(Query(q): Query<SuggestQuery>) -> Json<Value> {
    match run_suggest(q).await {
        Ok(v) => Json(v),
        Err(e) => Json(json!({"status": "error", "message": e.to_string()})),
    }
}

/// `GET /api/suggest`。Google Ads generateKeywordIdeas のみ・無料で関連キーワードを返す。
async fn run_suggest(q: SuggestQuery) -> anyhow::Result<Value> {
    let cfg = GoogleAdsConfig::from_env();
    let missing = cfg.missing();
    if !missing.is_empty() {
        return Ok(json!({
            "status": "missing_credentials",
            "message": "Google Ads の資格情報が未設定です",
            "missing": missing,
        }));
    }

    let seeds: Vec<String> = match q.seed.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => split_keywords(raw),
        None => {
            return Ok(json!({
                "status": "error",
                "message": "seed(カンマ/改行区切り)が必要です",
            }));
        }
    };
    if seeds.is_empty() {
        return Ok(json!({"status": "error", "message": "有効な seed がありません"}));
    }

    let region = q.region.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let geo_pick = match region {
        Some(name) => resolve_geo(&cfg, name, None).await?,
        None => None,
    };
    let location_ids: Vec<String> =
        geo_pick.as_ref().map(|p| vec![p.id.clone()]).unwrap_or_default();

    let resp = generate_keyword_ideas(&cfg, &cfg.customer_id(), &seeds, &location_ids).await?;
    let metrics: Vec<KeywordMetric> = parse_keyword_metrics(&resp);

    let mut sorted = metrics;
    sorted.sort_by(|a, b| b.avg_monthly.unwrap_or(-1).cmp(&a.avg_monthly.unwrap_or(-1)));

    let noise = q.noise_floor.max(0);
    let mut seen = std::collections::HashSet::new();
    let candidates: Vec<&KeywordMetric> = sorted
        .iter()
        .filter(|m| seen.insert(m.keyword.clone()))
        .filter(|m| m.avg_monthly.unwrap_or(0) >= noise)
        .collect();
    let brand_excluded = if q.exclude_brand {
        candidates.iter().filter(|m| m.is_brand).count()
    } else {
        0
    };
    let suggestions: Vec<Value> = candidates
        .iter()
        .filter(|m| !(q.exclude_brand && m.is_brand))
        .take(q.limit)
        .map(|m| {
            json!({
                "keyword": m.keyword,
                "avg_monthly": m.avg_monthly,
                "competition": m.competition,
                "competition_index": m.competition_index,
                "bid_low_yen": m.bid_low_yen,
                "bid_high_yen": m.bid_high_yen,
                "concepts": m.concepts
                    .iter()
                    .map(|(g, n)| json!({"group": g, "name": n}))
                    .collect::<Vec<_>>(),
                "is_brand": m.is_brand,
            })
        })
        .collect();

    Ok(json!({
        "status": "ok",
        "region": geo_pick.as_ref().map(|p| json!({
            "name": region,
            "geo_id": p.id,
            "geo_type": p.target_type,
            "canonical_name": p.canonical_name,
        })).unwrap_or(Value::Null),
        "seeds": seeds,
        "exclude_brand": q.exclude_brand,
        "brand_excluded_count": brand_excluded,
        "suggestions": suggestions,
    }))
}

fn default_max_cpc() -> f64 {
    300.0
}
fn default_daily_budget() -> f64 {
    5000.0
}

#[derive(Debug, Deserialize)]
pub struct ForecastQuery {
    #[serde(default)]
    kw: Option<String>,
    #[serde(default)]
    region: Option<String>,
    #[serde(default = "default_max_cpc")]
    max_cpc: f64,
    #[serde(default = "default_daily_budget")]
    daily_budget: f64,
    #[serde(default)]
    start: Option<String>,
    #[serde(default)]
    end: Option<String>,
    #[serde(default)]
    match_type: Option<String>,
}

pub async fn forecast_endpoint(Query(q): Query<ForecastQuery>) -> Json<Value> {
    match run_forecast(q).await {
        Ok(v) => Json(v),
        Err(e) => Json(json!({"status": "error", "message": e.to_string()})),
    }
}

/// `GET /api/forecast`。Google Ads generateKeywordForecastMetrics のみ・無料。
/// 期間(start/end)は必須(未来日でなければ Google 側が受け付けないため既定を置かない)。
async fn run_forecast(q: ForecastQuery) -> anyhow::Result<Value> {
    let cfg = GoogleAdsConfig::from_env();
    let missing = cfg.missing();
    if !missing.is_empty() {
        return Ok(json!({
            "status": "missing_credentials",
            "message": "Google Ads の資格情報が未設定です",
            "missing": missing,
        }));
    }

    let keywords: Vec<String> = match q.kw.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => split_keywords(raw),
        None => {
            return Ok(json!({
                "status": "error",
                "message": "kw(カンマ/改行区切り)が必要です",
            }));
        }
    };
    if keywords.is_empty() {
        return Ok(json!({"status": "error", "message": "有効なキーワードがありません"}));
    }

    let start = q.start.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let end = q.end.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let (start, end) = match (start, end) {
        (Some(s), Some(e)) => (s, e),
        _ => {
            return Ok(json!({
                "status": "error",
                "message": "start と end(yyyy-mm-dd)が必要です。予測期間は未来日である必要があり、履歴指標の最新月(常に過去月)からは導けないため既定値を置いていません",
            }));
        }
    };
    if !valid_forecast_date(start) || !valid_forecast_date(end) {
        return Ok(json!({
            "status": "error",
            "message": format!("start/end は yyyy-mm-dd 形式で指定してください(start={start}, end={end})"),
        }));
    }
    if start > end {
        return Ok(json!({
            "status": "error",
            "message": format!("start は end 以前である必要があります(start={start}, end={end})"),
        }));
    }

    let match_type = normalize_match_type(q.match_type.as_deref().unwrap_or(""));

    let region = q.region.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let geo_pick = match region {
        Some(name) => resolve_geo(&cfg, name, None).await?,
        None => None,
    };
    let location_ids: Vec<String> =
        geo_pick.as_ref().map(|p| vec![p.id.clone()]).unwrap_or_default();

    let resp = generate_keyword_forecast(
        &cfg,
        &cfg.customer_id(),
        &keywords,
        &location_ids,
        q.max_cpc,
        q.daily_budget,
        start,
        end,
        match_type,
    )
    .await?;
    let m = match parse_forecast(&resp) {
        Some(m) => m,
        None => {
            return Ok(json!({
                "status": "error",
                "message": "campaignForecastMetrics が応答に含まれていません",
                "raw": resp,
            }));
        }
    };

    Ok(json!({
        "status": "ok",
        "region": geo_pick.as_ref().map(|p| json!({
            "name": region,
            "geo_id": p.id,
            "geo_type": p.target_type,
            "canonical_name": p.canonical_name,
        })).unwrap_or(Value::Null),
        "keywords": keywords,
        "input": {
            "max_cpc_yen": q.max_cpc,
            "daily_budget_yen": q.daily_budget,
            "match_type": match_type,
            "start": start,
            "end": end,
        },
        "forecast": {
            "impressions": m.impressions,
            "clicks": m.clicks,
            "ctr": m.ctr,
            "average_cpc_yen": m.average_cpc_yen,
            "cost_yen": m.cost_yen,
            "conversions": m.conversions,
            "conversion_rate": m.conversion_rate,
            "average_cpa_yen": m.average_cpa_yen,
        },
        "caveat": "CV/CPAはGoogle側の一般推定。クリック・費用・CTRに比べ信頼度が低い",
    }))
}

/// 単一地名を Suggest→pick_geo_id_pref で解決する。
async fn resolve_geo(
    cfg: &GoogleAdsConfig,
    name: &str,
    prefer_prefecture: Option<&str>,
) -> anyhow::Result<Option<GeoPick>> {
    let cands = suggest_geo_targets(cfg, &[name.to_string()]).await?;
    Ok(pick_geo_id_pref(&cands, name, prefer_prefecture))
}

/// canonical("City,Prefecture,Japan")から県トークン(第2要素)を取り出す。
fn prefecture_of(canonical: Option<&str>) -> Option<String> {
    let parts: Vec<&str> = canonical?.split(',').map(str::trim).collect();
    parts.get(1).filter(|p| !p.is_empty()).map(|p| p.to_string())
}

/// 1 地域の需要合計(キーワード群の avgMonthlySearches 合計)を得る。
async fn area_volume(
    cfg: &GoogleAdsConfig,
    keywords: &[String],
    geo_id: &str,
) -> anyhow::Result<i64> {
    let resp =
        fetch_historical_metrics(cfg, &cfg.customer_id(), keywords, &[geo_id.to_string()]).await?;
    let rows = parse_historical_metrics(&resp);
    let mapped = build_volume_map(keywords, &rows);
    Ok(mapped.iter().filter_map(|(_, v)| v.clone().flatten()).sum())
}

/// 基準地→距離圏の地域別需要マップを組む共通処理。
async fn build_regional_demand(
    cfg: &GoogleAdsConfig,
    base: &str,
    keywords: &[String],
    radius_km: f64,
    max_areas: usize,
) -> anyhow::Result<(DemandMap, Option<GeoPick>)> {
    let centroids = load_centroids(&centroids_path())?;
    let base_pick = resolve_geo(cfg, base, None).await?;
    let base_prefecture =
        prefecture_of(base_pick.as_ref().and_then(|p| p.canonical_name.as_deref()));

    let nb = geo::neighbors_within(base, &centroids, radius_km, false);
    let cap = max_areas.max(1);
    let mut target_names: Vec<String> = vec![base.to_string()];
    for n in &nb.neighbors {
        target_names.push(n.name.clone());
    }
    target_names.truncate(cap);

    let mut id_map: HashMap<String, String> = HashMap::new();
    let mut vol_map: HashMap<String, i64> = HashMap::new();
    if let Some(p) = &base_pick {
        id_map.insert(base.to_string(), p.id.clone());
    }
    for name in &target_names {
        if !id_map.contains_key(name) {
            if let Some(p) = resolve_geo(cfg, name, base_prefecture.as_deref()).await? {
                id_map.insert(name.clone(), p.id);
            }
        }
    }
    let unique_ids: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        id_map.values().filter(|id| seen.insert((*id).clone())).cloned().collect()
    };
    // 連続照会は Google Ads のレート制限 (429) に当たるため 400ms 間隔を空ける。
    for (i, gid) in unique_ids.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        }
        let v = area_volume(cfg, keywords, gid).await?;
        vol_map.insert(gid.clone(), v);
    }
    let resolve = |name: &str| id_map.get(name).cloned();
    let volume = |_kws: &[String], geo_id: &str| vol_map.get(geo_id).copied().unwrap_or(0);
    let dm = build_demand_map(base, keywords, &centroids, radius_km, max_areas, resolve, volume);
    Ok((dm, base_pick))
}

#[derive(Debug, Deserialize)]
pub struct RegionsQuery {
    #[serde(default)]
    kw: Option<String>,
    #[serde(default)]
    job: Option<String>,
    base: String,
    #[serde(default = "default_radius")]
    radius_km: f64,
    #[serde(default = "default_max_areas")]
    max_areas: usize,
}

pub async fn regions_endpoint(Query(q): Query<RegionsQuery>) -> Json<Value> {
    match run_regions(q).await {
        Ok(v) => Json(v),
        Err(e) => Json(json!({"status": "error", "message": e.to_string()})),
    }
}

/// `GET /api/regions`。基準地の距離圏で地域別の検索需要を返す(Google Ads のみ・無料)。
async fn run_regions(q: RegionsQuery) -> anyhow::Result<Value> {
    let cfg = GoogleAdsConfig::from_env();
    let missing = cfg.missing();
    if !missing.is_empty() {
        return Ok(json!({
            "status": "missing_credentials",
            "message": "Google Ads の資格情報が未設定です",
            "missing": missing,
        }));
    }
    let base = q.base.trim();
    if base.is_empty() {
        return Ok(json!({"status": "error", "message": "base(基準地名)が必要です"}));
    }
    let keywords: Vec<String> = match q.kw.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => split_keywords(raw),
        None => match q.job.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(job) => place_agnostic_keywords(job),
            None => {
                return Ok(json!({"status": "error", "message": "kw または job が必要です"}));
            }
        },
    };
    if keywords.is_empty() {
        return Ok(json!({"status": "error", "message": "有効なキーワードがありません"}));
    }
    let (dm, base_pick) =
        build_regional_demand(&cfg, base, &keywords, q.radius_km, q.max_areas).await?;
    Ok(json!({
        "status": "ok",
        "base": base,
        "base_geo_id": base_pick.as_ref().map(|p| p.id.clone()),
        "base_geo_type": base_pick.as_ref().and_then(|p| p.target_type.clone()),
        "radius_km": q.radius_km,
        "keywords": keywords,
        "demand": demand_to_json(&dm),
    }))
}

#[derive(Debug, Deserialize)]
pub struct SerpQuery {
    /// SERP を引く 1 キーワード(複数渡されても先頭のみ使用)。
    kw: String,
    #[serde(default)]
    region: Option<String>,
    /// true のとき、同じ流れで AI 概要の本文も取得する(SERP 1 ＋ AI概要 1 ＝2消費)。
    #[serde(default)]
    with_answers: bool,
    /// 端末("mobile" / "desktop")。既定 "desktop"。device はキャッシュキーに含まれる。
    #[serde(default)]
    device: Option<String>,
}

pub async fn serp_endpoint(
    State(state): State<Arc<AppState>>,
    Query(q): Query<SerpQuery>,
) -> Json<Value> {
    match run_serp_endpoint(state, q).await {
        Ok(v) => Json(v),
        Err(e) => Json(json!({"status": "error", "message": e.to_string()})),
    }
}

/// `GET /api/serp`。**SerpApi を 1 クエリだけ**消費して媒体順位と広告枠を返す。
/// 1 レポート = 1 クエリの規律。消費は月次カウンタ (Turso) に記録して quota で返す。
async fn run_serp_endpoint(state: Arc<AppState>, q: SerpQuery) -> anyhow::Result<Value> {
    let key = serpapi_key();
    if key.is_empty() {
        return Ok(json!({"status": "missing_serpapi_key", "message": "SERPAPI_API_KEY が未設定です"}));
    }
    let turso = state.turso_db.clone();
    let kw = match split_keywords(&q.kw).into_iter().next() {
        Some(k) => k,
        None => return Ok(json!({"status": "error", "message": "kw(キーワード)が必要です"})),
    };
    // 地域: 日本語地名は SerpApi locations.json が受け付けないため、Google Ads の
    // GeoTarget Suggest で英語 canonical に変換してから引く。解決不能は全国 SERP に degrade。
    let mut location_note: Option<String> = None;
    let location: Option<String> = match q.region.as_deref().map(str::trim).filter(|s| !s.is_empty())
    {
        Some(name) => {
            let mut queries: Vec<String> = Vec::new();
            let cfg = GoogleAdsConfig::from_env();
            if cfg.missing().is_empty() {
                if let Ok(Some(pick)) = resolve_geo(&cfg, name, None).await {
                    if let Some(canon) = pick.canonical_name.as_deref() {
                        if let Some(city) = serp::city_from_canonical(canon) {
                            queries.push(city);
                        }
                        queries.push(canon.to_string());
                    }
                }
            }
            queries.push(name.to_string());
            let mut found = None;
            for qy in &queries {
                let locs = serpapi::fetch_locations(qy, 10).await.unwrap_or_default();
                if let Some(loc) = serp::pick_serp_location(&locs) {
                    found = Some(loc);
                    break;
                }
            }
            if found.is_none() {
                location_note =
                    Some(format!("「{name}」は SerpApi の地域名に解決できず、全国の検索結果です"));
            }
            found
        }
        None => None,
    };
    let device = match q.device.as_deref().map(str::trim).unwrap_or("") {
        "mobile" => "mobile",
        "tablet" => "tablet",
        _ => "desktop",
    };
    let mut payload =
        serpapi::search_device(&kw, location.as_deref(), 10, device, &key, turso.clone()).await?;
    let is_cache_hit = |p: &Value| {
        p.get("request_meta")
            .and_then(|m| m.get("local_cache_hit"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    let mut cache_hit = is_cache_hit(&payload);
    // AI概要の本文が欲しいのにキャッシュに本文もトークンも無いケースだけ取り直す。
    let needs_fresh_for_ai = q.with_answers
        && cache_hit
        && payload
            .get("ai_overview")
            .map(|ao| ao.get("text_blocks").is_none())
            .unwrap_or(false);
    if needs_fresh_for_ai {
        payload =
            serpapi::search_fresh_device(&kw, location.as_deref(), 10, device, &key, turso.clone())
                .await?;
        cache_hit = false;
    }
    let base_spent: i64 = if cache_hit { 0 } else { 1 };
    if !cache_hit {
        store::quota_increment(turso.clone(), 1).await;
    }
    let q_wants_answers = q.with_answers && !cache_hit;

    let index = media::default_index();
    let mut seen = std::collections::HashSet::new();
    let mut results: Vec<Value> = Vec::new();
    let mut dedup_note: Vec<Value> = Vec::new();
    let organic_total = payload
        .get("organic_results")
        .and_then(Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0);
    for (rank, host) in organic_domains(&payload) {
        let row = media::resolve_by_host(&host, &index);
        let media_id = row.map(|r| r.media_id.clone()).unwrap_or_else(|| host.clone());
        if !seen.insert(media_id.clone()) {
            let name = row.map(|r| r.media_name.clone()).unwrap_or_else(|| host.clone());
            dedup_note.push(json!({"rank": rank, "media_name": name}));
            continue;
        }
        let (media_name, known, specialized, job_scope) = match row {
            Some(r) => (r.media_name.clone(), true, r.is_specialized(), r.job_scope.clone()),
            None => (host.clone(), false, false, String::new()),
        };
        let detail = payload
            .get("organic_results")
            .and_then(Value::as_array)
            .and_then(|arr| {
                arr.iter().find(|r| r.get("position").and_then(Value::as_i64) == Some(rank))
            });
        let snippet =
            detail.and_then(|d| d.get("snippet")).and_then(Value::as_str).unwrap_or("");
        results.push(json!({
            "rank": rank, "domain": host, "media_id": media_id, "media_name": media_name,
            "known": known, "specialized": specialized, "job_scope": job_scope,
            "title": detail.and_then(|d| d.get("title")).and_then(Value::as_str).unwrap_or(""),
            "snippet": snippet,
            "link": detail.and_then(|d| d.get("link")).and_then(Value::as_str).unwrap_or(""),
            "listing_count": extract_listing_count(snippet),
        }));
    }
    let mut ads: Vec<Value> = Vec::new();
    if let Some(arr) = payload.get("ads").and_then(Value::as_array) {
        for (i, ad) in arr.iter().enumerate() {
            let link = ad
                .get("displayed_link")
                .or_else(|| ad.get("link"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let host = serp::host_of(link);
            let row = media::resolve_by_host(&host, &index);
            ads.push(json!({
                "rank": i + 1,
                "block_position": ad.get("block_position").and_then(Value::as_str).unwrap_or("top"),
                "domain": host,
                "media_name": row.map(|r| r.media_name.clone()).unwrap_or_else(|| host.clone()),
                "title": ad.get("title").and_then(Value::as_str).unwrap_or(""),
            }));
        }
    }
    let jobs = payload.get("jobs_results");
    let jobs_block = jobs.map(|j| {
        let list: Vec<Value> = j
            .get("jobs")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .take(10)
                    .map(|x| {
                        json!({
                            "title": x.get("title").and_then(Value::as_str).unwrap_or(""),
                            "company": x.get("company_name").and_then(Value::as_str).unwrap_or(""),
                            "location": x.get("location").and_then(Value::as_str).unwrap_or(""),
                            "via": x.get("via").and_then(Value::as_str).unwrap_or(""),
                            "extensions": x.get("detected_extensions").cloned().unwrap_or(Value::Null),
                            "apply_options": x.get("apply_options")
                                .and_then(Value::as_array)
                                .map(|a| a.iter().filter_map(|o| o.get("title").and_then(Value::as_str)).collect::<Vec<_>>())
                                .unwrap_or_default(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        json!({
            "count_text": j.get("link_text").and_then(Value::as_str).unwrap_or(""),
            "jobs": list,
        })
    });
    let related: Vec<String> = payload
        .get("related_searches")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.get("query").and_then(Value::as_str).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let key_for_tokens = key.clone();
    let mut extra_spent: i64 = 0;
    let questions: Vec<String> = payload
        .get("related_questions")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.get("question").and_then(Value::as_str).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let info = payload.get("search_information");
    let mut seen_filter = std::collections::HashSet::new();
    let filters: Vec<Value> = payload
        .get("filters")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|f| {
                    let name = f.get("name").and_then(Value::as_str)?;
                    if !seen_filter.insert(name.to_string()) {
                        return None;
                    }
                    let opts: Vec<String> = f
                        .get("options")
                        .and_then(Value::as_array)
                        .map(|o| {
                            o.iter()
                                .filter_map(|x| x.get("name").and_then(Value::as_str).map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    Some(json!({"name": name, "options": opts}))
                })
                .collect()
        })
        .unwrap_or_default();

    let ai_raw = payload.get("ai_overview");
    let has_ai_overview = ai_raw.is_some();
    let mut ai_overview = Value::Null;
    if let Some(ao) = ai_raw {
        if ao.get("text_blocks").is_some() {
            ai_overview = format_ai_overview(ao);
        } else if q_wants_answers {
            if let Some(tok) = ao.get("page_token").and_then(Value::as_str) {
                if let Ok(r) =
                    serpapi::fetch_by_token("google_ai_overview", "page_token", tok, &key_for_tokens)
                        .await
                {
                    extra_spent += 1;
                    if let Some(inner) = r.get("ai_overview") {
                        ai_overview = format_ai_overview(inner);
                    }
                }
            }
        }
    }

    if extra_spent > 0 {
        store::quota_increment(turso.clone(), extra_spent).await;
    }
    let total_spent = base_spent + extra_spent;
    let mut quota_out = store::quota_read(turso).await;
    if let Some(obj) = quota_out.as_object_mut() {
        obj.insert("spent_this_call".into(), json!(total_spent));
        if total_spent == 0 {
            obj.insert(
                "note".into(),
                json!("この結果はキャッシュから返したため枠を消費していません"),
            );
        }
    }

    Ok(json!({
        "status": "ok",
        "used_keyword": kw,
        "device": device,
        "location": location,
        "location_note": location_note,
        "results": results,
        "organic_total": organic_total,
        "deduped": dedup_note,
        "ads": ads,
        "jobs": jobs_block,
        "related_searches": related,
        "related_questions": questions,
        "ai_overview": ai_overview,
        "filters": filters,
        "total_results": info.and_then(|i| i.get("total_results")).cloned().unwrap_or(Value::Null),
        "has_ai_overview": has_ai_overview,
        "quota": quota_out,
    }))
}

/// AI概要の生 JSON を UI 向けに整形する(段落・見出し・リスト＋引用元)。
fn format_ai_overview(ao: &Value) -> Value {
    let blocks: Vec<Value> = ao
        .get("text_blocks")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|b| {
                    let ty = b.get("type").and_then(Value::as_str).unwrap_or("");
                    let items: Vec<String> = b
                        .get("list")
                        .and_then(Value::as_array)
                        .map(|l| {
                            l.iter()
                                .filter_map(|x| {
                                    let t = x.get("title").and_then(Value::as_str).unwrap_or("");
                                    let s = x.get("snippet").and_then(Value::as_str).unwrap_or("");
                                    let joined = format!("{t}{}{s}", if !t.is_empty() && !s.is_empty() { "：" } else { "" });
                                    if joined.trim().is_empty() { None } else { Some(joined) }
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    json!({
                        "type": ty,
                        "text": b.get("snippet").and_then(Value::as_str).unwrap_or(""),
                        "items": items,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let refs: Vec<Value> = ao
        .get("references")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|r| {
                    json!({
                        "title": r.get("title").and_then(Value::as_str).unwrap_or(""),
                        "source": r.get("source").and_then(Value::as_str)
                            .or_else(|| r.get("link").and_then(Value::as_str)).unwrap_or(""),
                        "link": r.get("link").and_then(Value::as_str).unwrap_or(""),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    json!({"blocks": blocks, "references": refs})
}

/// 説明文から「◯◯件」の掲載件数を取り出す(先頭一致・カンマ許容)。無ければ `None`。
fn extract_listing_count(snippet: &str) -> Option<i64> {
    let bytes = snippet.as_bytes();
    let ken = "件";
    let mut idx = 0;
    while let Some(pos) = snippet[idx..].find(ken) {
        let end = idx + pos;
        let mut start = end;
        while start > 0 {
            let c = bytes[start - 1];
            if c.is_ascii_digit() || c == b',' {
                start -= 1;
            } else {
                break;
            }
        }
        let num: String = snippet[start..end].chars().filter(|c| c.is_ascii_digit()).collect();
        if let Ok(v) = num.parse::<i64>() {
            if v > 0 {
                return Some(v);
            }
        }
        idx = end + ken.len();
    }
    None
}

#[derive(Debug, Deserialize)]
pub struct ClusterQuery {
    kw: String,
    /// 任意。`keyword:volume` のカンマ区切り(検索量の再表示用。LLM には数値を書かせない)。
    #[serde(default)]
    vol: Option<String>,
}

pub async fn cluster_endpoint(Query(q): Query<ClusterQuery>) -> Json<Value> {
    match run_cluster(q).await {
        Ok(v) => Json(v),
        Err(e) => Json(json!({"status": "error", "message": e.to_string()})),
    }
}

/// `GET /api/cluster`。候補キーワードを軸で分類する(Gemini・分類のみ)。
/// LLM には「与えた語を与えたカテゴリへ振り分ける」ことだけをさせる(捏造・欠落を構造で防ぐ)。
async fn run_cluster(q: ClusterQuery) -> anyhow::Result<Value> {
    let keywords = split_keywords(&q.kw);
    if keywords.is_empty() {
        return Ok(json!({"status": "error", "message": "kw(キーワード)が必要です"}));
    }
    let key = gemini_api_key();
    if key.is_empty() {
        return Ok(json!({"status": "missing_gemini_key", "message": "GEMINI_API_KEY が未設定です"}));
    }
    let vols = q.vol.as_deref().map(keyword_cluster::parse_volumes).unwrap_or_default();
    let source: Vec<(String, Option<i64>)> =
        keywords.iter().map(|k| (k.clone(), vols.get(k).copied())).collect();

    let categories = keyword_cluster::default_categories();
    let prompt = keyword_cluster::build_prompt(&keywords, &categories);
    let schema = keyword_cluster::response_schema();
    let model = gemini_model();
    match gemini::generate_json(&prompt, Some(&schema), &key, &model, 0.0).await {
        Ok(v) => {
            let merged = keyword_cluster::merge(&v, &source);
            Ok(json!({
                "status": "ok",
                "model": model,
                "categories": merged.get("categories").cloned().unwrap_or(Value::Null),
                "unassigned_count": merged.get("unassigned_count").cloned().unwrap_or(Value::Null),
                "hallucinated_count": merged.get("hallucinated_count").cloned().unwrap_or(Value::Null),
            }))
        }
        Err(e) => Ok(json!({"status": "error", "message": e.to_string()})),
    }
}

fn area_to_json(a: &Area) -> Value {
    json!({
        "name": a.name,
        "distance_km": a.distance_km,
        "is_base": a.is_base,
        "resolved": a.resolved,
        "geo_id": a.geo_id,
        "total_volume": a.total_volume,
    })
}

fn demand_to_json(dm: &DemandMap) -> Value {
    json!({
        "base": dm.base,
        "resolved": dm.resolved,
        "radius_km": dm.radius_km,
        "queried_count": dm.queried_count,
        "dropped_count": dm.dropped_count,
        "dropped_names": dm.dropped_names,
        "base_total_volume": dm.base_total_volume,
        "areas": dm.areas.iter().map(area_to_json).collect::<Vec<_>>(),
        "ranked": dm.ranked.iter().map(area_to_json).collect::<Vec<_>>(),
        "leakage": dm.leakage.iter().map(area_to_json).collect::<Vec<_>>(),
    })
}

/// キーワード需要ビューアが有効か (Google Ads の必須資格情報が揃っているか)。
/// タブリンクの表示可否に使う (未設定環境ではリンクごと出さないフラグ分離)。
pub fn media_engine_enabled() -> bool {
    GoogleAdsConfig::from_env().missing().is_empty()
}

#[cfg(test)]
mod listing_count_tests {
    use super::extract_listing_count;

    #[test]
    fn extracts_count_from_snippet() {
        assert_eq!(
            extract_listing_count("千葉県船橋市のドライバーの求人は7473件あります。"),
            Some(7473)
        );
        assert_eq!(extract_listing_count("求人を5,885件掲載中。"), Some(5885));
        assert_eq!(extract_listing_count("件数の記載がない説明文"), None);
        assert_eq!(extract_listing_count("508件 · アルバイト"), Some(508));
        assert_eq!(extract_listing_count("該当0件"), None);
    }

    #[test]
    fn split_keywords_dedups_and_trims() {
        let got = super::split_keywords(" 看護師 求人 ,\n介護 求人, 看護師 求人 ,, ");
        assert_eq!(got, vec!["看護師 求人".to_string(), "介護 求人".to_string()]);
    }
}
