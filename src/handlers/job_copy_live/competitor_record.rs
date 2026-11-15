//! 競合調査の結果を、HubSpot の求人 (0-420) に月ごとに残す。
//!
//! - `PUT /api/job-copy/listings/{id}/competitor-record`: 競合調査の画面が保持中 (30 分) のレポートの ID を渡す。
//!   サーバーがレポートから月ごとの 1 件 (`CompetitorMonth`) を写して作り、求人の `competitor_monthly`
//!   (JSON の配列) に足す。同じ月は置き換える。人が読む 5 項目 (`competitor_last_date` ほか) も同じ PATCH で上書きする。
//! - HubSpot の呼び出しは、求人の読み取り 1 回 (今の記録と自社の給与) と PATCH 1 回。
//! - 数字はレポートから写すだけ。自社の給与は求人の本文の基本給与 (円) を、競合の給与の単位 (万円 / 円) に合わせるだけ。
//! - 元の CSV とレポートの全体は残さない (`csv_url` は常に null)。新しい DB・表・環境変数は無い。
//! - `competitor_monthly` は HubSpot の文字数の上限 (65,536 字) があるので、超えそうなら書かずに止める。
use super::{authorized_user, client_error, listings, valid_id, Access, ReadError, Record};
use crate::handlers::survey::report_html::competitor_model::CompetitorReport;
use crate::hubspot::client::{COMPETITOR_PROPERTIES, COMPETITOR_PROPERTY_MISSING};
use crate::{hubspot::HubSpotError, AppState};
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    Extension, Json,
};
use chrono::{DateTime, FixedOffset, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;
use tower_sessions::Session;
use ts_rs::TS;
use unicode_normalization::UnicodeNormalization;

/// 月ごとの記録 (JSON の配列) を持つ項目。
pub(super) const MONTHLY: &str = COMPETITOR_PROPERTIES[0];
/// HubSpot のテキストの項目は 65,536 字まで (2026-11-16 に HubSpot の資料で確認)。少し手前で止める。
pub(super) const MONTHLY_MAX_CHARS: usize = 65_000;
/// よく出る言葉を残す数。
const KEYWORDS: usize = 5;

/// 月ごとの記録の 1 件。HubSpot の求人の `competitor_monthly` に、この形の配列で入る。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CompetitorMonth {
    /// "2026-11" (JST)。同じ月の記録は 1 件だけ。
    pub month: String,
    /// 記録した日時 (JST、RFC 3339)。
    pub run_at: String,
    /// 記録した人のメール。
    pub by: String,
    pub conditions: CompetitorConditions,
    pub count: CompetitorCount,
    pub salary: CompetitorSalary,
    /// 保存した時点の自社の求人の給与。読めない・単位が合わないときは null。
    pub own_salary: Option<CompetitorOwnSalary>,
    /// よく出る言葉の上位 5 つ (言葉・件数)。
    pub keywords_top5: Vec<(String, u32)>,
    /// 元の CSV の場所。残さないので常に null。
    pub csv_url: Option<String>,
}
/// 調べたときの条件。月ごとに違うと比べられないので必ず残す。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CompetitorConditions {
    pub title: String,
    pub employment_type: Option<String>,
    pub prefecture: Option<String>,
    pub municipality: Option<String>,
    pub top_n: u32,
    pub source: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CompetitorCount {
    pub total: u32,
    pub salary_parsed: u32,
    /// 人気の表示がある求人。人気の表示が無い CSV では null。
    pub popular: Option<u32>,
    pub super_popular: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CompetitorSalary {
    /// "万円" (月給) または "円/時" (時給)。
    pub unit: String,
    pub all: CompetitorSalaryStats,
    pub popular: CompetitorSalaryStats,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CompetitorSalaryStats {
    pub min_mean: Option<f64>,
    pub min_median: Option<f64>,
    pub max_mean: Option<f64>,
    pub max_median: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CompetitorOwnSalary {
    pub min: Option<f64>,
    pub max: Option<f64>,
}
/// 保存の応答。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct CompetitorRecordSaved {
    pub listing_id: String,
    pub listing_name: Option<String>,
    pub month: String,
    /// 同じ月の記録があって置き換えたか。
    pub replaced: bool,
    /// 保存後の記録の数 (月の数)。
    pub months: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    report_id: String,
    /// 競合調査の画面で選んだ CSV の媒体 (`indeed_sp` / `indeed`)。条件として残す。
    source_type: String,
}

fn err(status: StatusCode, code: &'static str) -> ReadError {
    ReadError(status, code)
}
fn source_label(source_type: &str) -> Option<&'static str> {
    match source_type {
        "indeed_sp" => Some("Indeed SP の一覧CSV"),
        "indeed" => Some("Indeed PC の一覧CSV"),
        _ => None,
    }
}
fn jst(now: DateTime<Utc>) -> DateTime<FixedOffset> {
    now.with_timezone(&FixedOffset::east_opt(9 * 3600).expect("JST"))
}

/// 求人の画面に渡す月ごとの記録 (古い月から)。空・読めない値・形の違う 1 件は出さない (画面を壊さない)。
pub(super) fn monthly(record: &Record) -> Vec<CompetitorMonth> {
    let Some(raw) = record.value(MONTHLY) else {
        return Vec::new();
    };
    let mut rows: Vec<CompetitorMonth> = serde_json::from_str::<Vec<Value>>(raw)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| serde_json::from_value(value).ok())
        .collect();
    rows.sort_by(|a, b| a.month.cmp(&b.month));
    rows
}

/// 本文の金額の文字 (全角・桁区切りあり) を数にする。数字だけでなければ None。
fn yen(text: Option<&str>) -> Option<f64> {
    let normalized: String = text?.nfkc().filter(|c| *c != ',').collect();
    let value = normalized.trim();
    (!value.is_empty() && value.bytes().all(|c| c.is_ascii_digit()))
        .then(|| value.parse::<f64>().ok())
        .flatten()
}
/// 自社の給与を、競合の給与の単位に合わせる。月給どうし (万円)・時給どうし (円) のときだけ。
fn own_salary(record: &Record, is_hourly: bool) -> Option<CompetitorOwnSalary> {
    let (kind, min, max) = listings::own_salary_text(record)?;
    let wanted = if is_hourly { "時給" } else { "月給" };
    if kind.as_deref() != Some(wanted) {
        return None;
    }
    let convert = |text: Option<String>| {
        yen(text.as_deref()).map(|v| {
            if is_hourly {
                v
            } else {
                (v / 100.0).round() / 100.0
            }
        })
    };
    let (min, max) = (convert(min), convert(max));
    (min.is_some() || max.is_some()).then_some(CompetitorOwnSalary { min, max })
}

/// レポートから月ごとの 1 件を写す。新しい集計はしない。
pub(super) fn build_month(
    report: &CompetitorReport,
    source: &str,
    by: &str,
    own_salary: Option<CompetitorOwnSalary>,
    now: DateTime<Utc>,
) -> CompetitorMonth {
    let at = jst(now);
    let row = |label: &str| {
        report
            .excel
            .salary_table
            .iter()
            .find(|row| row.label == label)
            .map(|row| row.values.clone())
            .unwrap_or_default()
    };
    let (mean, median) = (row("平均値"), row("中央値"));
    // 各行は [総合下限, 総合上限, 人気下限, 人気上限]。
    let stats = |min: usize, max: usize| CompetitorSalaryStats {
        min_mean: mean.get(min).copied().flatten(),
        min_median: median.get(min).copied().flatten(),
        max_mean: mean.get(max).copied().flatten(),
        max_median: median.get(max).copied().flatten(),
    };
    let meta = &report.meta;
    CompetitorMonth {
        month: at.format("%Y-%m").to_string(),
        run_at: at.to_rfc3339_opts(SecondsFormat::Secs, false),
        by: by.to_owned(),
        conditions: CompetitorConditions {
            title: meta.title.clone(),
            employment_type: meta.employment_type.clone(),
            prefecture: meta.prefecture.clone(),
            municipality: meta.municipality.clone(),
            top_n: meta.top_n_effective,
            source: source.to_owned(),
        },
        count: CompetitorCount {
            total: meta.total_count,
            salary_parsed: meta.salary_parsed_count,
            popular: report.popularity_split.as_ref().map(|p| p.popular),
            super_popular: report.popularity_split.as_ref().map(|p| p.super_popular),
        },
        salary: CompetitorSalary {
            unit: meta.unit.clone(),
            all: stats(0, 1),
            popular: stats(2, 3),
        },
        own_salary,
        keywords_top5: report
            .excel
            .keyword_all
            .iter()
            .take(KEYWORDS)
            .map(|row| (row.word.clone(), row.count))
            .collect(),
        csv_url: None,
    }
}

#[derive(Debug, PartialEq)]
pub(super) enum MergeError {
    /// 今の値が JSON の配列として読めない (上書きすると過去の記録を失うので止める)。
    Unreadable,
    /// 足すと文字数の上限を超える。
    TooLarge,
}
/// 今の値に 1 件を足す (同じ月は置き換え、月の順に並べる)。返すのは (書く値, 置き換えたか, 月の数)。
pub(super) fn merge(
    existing: Option<&str>,
    entry: &CompetitorMonth,
    max_chars: usize,
) -> Result<(String, bool, usize), MergeError> {
    let mut rows: Vec<Value> = match existing.map(str::trim).filter(|raw| !raw.is_empty()) {
        None => Vec::new(),
        Some(raw) => serde_json::from_str(raw).map_err(|_| MergeError::Unreadable)?,
    };
    let month_of = |row: &Value| row.get("month").and_then(Value::as_str).map(str::to_owned);
    if rows.iter().any(|row| month_of(row).is_none()) {
        return Err(MergeError::Unreadable);
    }
    let before = rows.len();
    rows.retain(|row| month_of(row).as_deref() != Some(entry.month.as_str()));
    let replaced = rows.len() != before;
    rows.push(serde_json::to_value(entry).map_err(|_| MergeError::Unreadable)?);
    rows.sort_by_key(|row| month_of(row).unwrap_or_default());
    let text = serde_json::to_string(&rows).map_err(|_| MergeError::Unreadable)?;
    if text.chars().count() > max_chars {
        return Err(MergeError::TooLarge);
    }
    Ok((text, replaced, rows.len()))
}

fn amount(value: f64, hourly: bool) -> String {
    if hourly {
        format!("{}円", value.round())
    } else {
        format!("{value:.1}万円")
    }
}
/// 人が読む 1 行。例「競合 86件・給与中央値 23.5万円（自社 22.0万円）・人気 12件」。
pub(super) fn summary_line(entry: &CompetitorMonth) -> String {
    let hourly = entry.salary.unit != "万円";
    let mut parts = vec![format!("競合 {}件", entry.count.total)];
    if let Some(median) = entry.salary.all.min_median {
        let own = entry
            .own_salary
            .as_ref()
            .and_then(|own| own.min)
            .map(|own| format!("（自社 {}）", amount(own, hourly)))
            .unwrap_or_default();
        parts.push(format!("給与中央値 {}{own}", amount(median, hourly)));
    }
    if let Some(popular) = entry.count.popular {
        parts.push(format!("人気 {popular}件"));
    }
    parts.join("・")
}
fn number(value: f64) -> String {
    // 23.5 / 1200 のように、余分な 0 を付けない。
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}
/// PATCH で書く 6 項目。値が無い項目は空にする (前の月の値を残さない)。
pub(super) fn properties(
    entry: &CompetitorMonth,
    monthly: String,
) -> BTreeMap<String, Option<String>> {
    let median = entry.salary.all.min_median;
    let own = entry.own_salary.as_ref().and_then(|own| own.min);
    // 自社の給与(下限) − 競合の給与の中央値(下限)。
    let gap = own
        .zip(median)
        .map(|(own, median)| ((own - median) * 100.0).round() / 100.0);
    let values = [
        Some(monthly),
        Some(entry.run_at.chars().take(10).collect()),
        Some(entry.count.total.to_string()),
        median.map(number),
        gap.map(number),
        Some(summary_line(entry)),
    ];
    COMPETITOR_PROPERTIES
        .iter()
        .zip(values)
        .map(|(name, value)| ((*name).to_owned(), value))
        .collect()
}

fn hubspot_error(e: HubSpotError) -> ReadError {
    match e {
        HubSpotError::Decode(m) if m == COMPETITOR_PROPERTY_MISSING => {
            err(StatusCode::CONFLICT, "competitor_property_missing")
        }
        HubSpotError::Busy => err(StatusCode::SERVICE_UNAVAILABLE, "hubspot_busy"),
        HubSpotError::NotFound => err(StatusCode::NOT_FOUND, "listing_not_found"),
        HubSpotError::Auth { .. } => err(StatusCode::FORBIDDEN, "hubspot_scope_denied"),
        HubSpotError::RateLimited => err(StatusCode::TOO_MANY_REQUESTS, "hubspot_rate_limited"),
        _ => err(StatusCode::BAD_GATEWAY, "hubspot_write_failed"),
    }
}

type Reply = (
    [(header::HeaderName, &'static str); 1],
    Json<CompetitorRecordSaved>,
);

pub(super) async fn put(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<Request>,
) -> Result<Reply, ReadError> {
    save(&state, &access, &session, &id, &headers, req, Utc::now()).await
}

/// 時刻を引数にした本体 (テストで月を決める)。
async fn save(
    state: &AppState,
    access: &Access,
    session: &Session,
    id: &str,
    headers: &HeaderMap,
    req: Request,
    now: DateTime<Utc>,
) -> Result<Reply, ReadError> {
    let user = authorized_user(state, access, session).await?;
    valid_id(id)?;
    if headers
        .get("x-requested-with")
        .and_then(|v| v.to_str().ok())
        != Some("fetch")
    {
        return Err(err(StatusCode::FORBIDDEN, "csrf_required"));
    }
    let source = source_label(&req.source_type)
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "competitor_source_unknown"))?;
    // 本人が作って保持中のレポートだけ。期限切れ・他人の ID は区別しない。
    let report = crate::handlers::competitor::stored_report(&user, &req.report_id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "competitor_report_not_found"))?;
    let service = access.service.as_deref().ok_or(ReadError(
        StatusCode::SERVICE_UNAVAILABLE,
        "hubspot_not_configured",
    ))?;
    // 今の記録と自社の給与を 1 回で読む (ほかの読み取りと相乗りしない)。
    let found = service
        .hubspot()
        .get_listing_fresh(
            id,
            &[
                "hs_name",
                "id_hrhakkaa",
                "id_airwork",
                "id_reclogi",
                "reclogi_moto_hr_kyuujin_id",
                "kyuujin_status",
                "hrh_kyuujinhyou_honbun",
                "shigotonaiyou",
                "kyuuyokeitai",
                MONTHLY,
            ],
        )
        .await
        .map_err(|e| match e {
            HubSpotError::NotFound => err(StatusCode::NOT_FOUND, "listing_not_found"),
            other => client_error(other),
        })?;
    let record = Record {
        id: found.id,
        properties: found.properties,
    };
    let entry = build_month(
        &report,
        source,
        &user,
        own_salary(&record, report.meta.is_hourly),
        now,
    );
    let (text, replaced, months) = merge(record.value(MONTHLY), &entry, MONTHLY_MAX_CHARS)
        .map_err(|e| match e {
            MergeError::Unreadable => err(
                StatusCode::UNPROCESSABLE_ENTITY,
                "competitor_record_unreadable",
            ),
            MergeError::TooLarge => err(
                StatusCode::UNPROCESSABLE_ENTITY,
                "competitor_record_too_large",
            ),
        })?;
    service
        .hubspot()
        .patch_listing_competitor(id, &properties(&entry, text))
        .await
        .map_err(hubspot_error)?;
    service.detail_cache.forget_listing(id);
    tracing::info!(listing = %id, month = %entry.month, replaced, months, "job_copy_competitor_record_saved");
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(CompetitorRecordSaved {
            listing_id: id.to_owned(),
            listing_name: record.value("hs_name").map(str::to_owned),
            month: entry.month,
            replaced,
            months: months as u32,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::State as MockState, response::IntoResponse, routing::get, Router};
    use chrono::TimeZone;
    use serde_json::json;
    use std::collections::BTreeSet;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Remote {
        reads: usize,
        patches: Vec<Value>,
        property_exists: bool,
        properties: serde_json::Map<String, Value>,
    }
    async fn read_remote(MockState(remote): MockState<Arc<Mutex<Remote>>>) -> Json<Value> {
        let mut remote = remote.lock().unwrap();
        remote.reads += 1;
        Json(json!({"id":"30","properties":remote.properties}))
    }
    async fn write_remote(
        MockState(remote): MockState<Arc<Mutex<Remote>>>,
        Json(payload): Json<Value>,
    ) -> axum::response::Response {
        let mut remote = remote.lock().unwrap();
        remote.patches.push(payload.clone());
        if !remote.property_exists {
            return (StatusCode::BAD_REQUEST, Json(json!({"status":"error"}))).into_response();
        }
        for (key, value) in payload["properties"].as_object().unwrap() {
            remote.properties.insert(key.clone(), value.clone());
        }
        Json(json!({"id":"30","properties":payload["properties"]})).into_response()
    }
    async fn property_definition(
        MockState(remote): MockState<Arc<Mutex<Remote>>>,
    ) -> axum::response::Response {
        if remote.lock().unwrap().property_exists {
            Json(json!({"name":"competitor_monthly"})).into_response()
        } else {
            (StatusCode::NOT_FOUND, Json(json!({}))).into_response()
        }
    }
    struct Fixture {
        state: Arc<AppState>,
        access: Access,
        session: Session,
        remote: Arc<Mutex<Remote>>,
    }
    const USER: &str = "writer@example.test";
    async fn session_for(email: Option<&str>) -> Session {
        let session = Session::new(None, Arc::new(tower_sessions::MemoryStore::default()), None);
        if let Some(email) = email {
            session
                .insert(crate::auth::SESSION_USER_KEY, email)
                .await
                .unwrap();
            session
                .insert(
                    crate::auth::SESSION_LOGIN_METHOD_KEY,
                    crate::auth::LOGIN_METHOD_GOOGLE_OIDC,
                )
                .await
                .unwrap();
        }
        session
    }
    /// 月給 21.3 万円〜25 万円の HRハッカーの求人 (架空)。
    fn listing_properties(monthly: Option<&str>) -> serde_json::Map<String, Value> {
        let mut props = json!({
            "hs_name": "顧客A 配送スタッフ",
            "id_hrhakkaa": "HR-30",
            "hrh_kyuujinhyou_honbun": "給与形態：月給\n基本給与 最小：213,000\n基本給与 最大：250000",
        });
        if let Some(monthly) = monthly {
            props["competitor_monthly"] = json!(monthly);
        }
        props.as_object().unwrap().clone()
    }
    async fn fixture(property_exists: bool, monthly: Option<&str>) -> Fixture {
        let remote = Arc::new(Mutex::new(Remote {
            property_exists,
            properties: listing_properties(monthly),
            ..Default::default()
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new()
            .route(
                "/crm/v3/objects/0-420/30",
                get(read_remote).patch(write_remote),
            )
            .route(
                "/crm/v3/properties/0-420/competitor_monthly",
                get(property_definition),
            )
            .with_state(remote.clone());
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let access = Access {
            service: Some(Arc::new(super::super::JobReadService::for_test(base))),
            images: None,
            drive_listings: BTreeSet::new(),
            drive_config_error: None,
            snapshot_reader: None,
            snapshot_cache: Arc::new(tokio::sync::Mutex::new(None)),
            resolved_jobs: Arc::new(tokio::sync::Mutex::new(std::collections::BTreeMap::new())),
            moc_drive: Ok(None),
            moc_path: None,
        };
        Fixture {
            state: super::super::integration_tests::moc_state(),
            access,
            session: session_for(Some(USER)).await,
            remote,
        }
    }
    fn headers(csrf: bool) -> HeaderMap {
        let mut h = HeaderMap::new();
        if csrf {
            h.insert("x-requested-with", "fetch".parse().unwrap());
        }
        h
    }
    fn november() -> DateTime<Utc> {
        // JST 2026-11-15 16:20
        Utc.with_ymd_and_hms(2026, 11, 15, 7, 20, 0).unwrap()
    }
    async fn call(
        f: &Fixture,
        session: &Session,
        id: &str,
        csrf: bool,
        report_id: &str,
        now: DateTime<Utc>,
    ) -> Result<CompetitorRecordSaved, (StatusCode, &'static str)> {
        save(
            &f.state,
            &f.access,
            session,
            id,
            &headers(csrf),
            Request {
                report_id: report_id.to_owned(),
                source_type: "indeed_sp".into(),
            },
            now,
        )
        .await
        .map(|(_, Json(v))| v)
        .map_err(|ReadError(status, code)| (status, code))
    }
    fn report_id() -> String {
        crate::handlers::competitor::store_report_for_test(USER, "配送 大阪")
    }
    fn stored(f: &Fixture) -> Vec<Value> {
        let raw = f.remote.lock().unwrap().properties["competitor_monthly"]
            .as_str()
            .unwrap()
            .to_owned();
        serde_json::from_str(&raw).unwrap()
    }

    #[tokio::test]
    async fn the_first_record_writes_the_month_and_the_five_readable_fields_in_one_patch() {
        let f = fixture(true, None).await;
        let saved = call(&f, &f.session, "30", true, &report_id(), november())
            .await
            .unwrap();
        assert_eq!(
            saved,
            CompetitorRecordSaved {
                listing_id: "30".into(),
                listing_name: Some("顧客A 配送スタッフ".into()),
                month: "2026-11".into(),
                replaced: false,
                months: 1,
            }
        );
        let remote = f.remote.lock().unwrap();
        assert_eq!(
            (remote.reads, remote.patches.len()),
            (1, 1),
            "読む 1 回・書く 1 回"
        );
        let props = remote.patches[0]["properties"].as_object().unwrap();
        assert_eq!(
            props.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            COMPETITOR_PROPERTIES.into_iter().collect::<BTreeSet<_>>()
        );
        // sample_report: 月給 25〜30 万円と 35〜40 万円の 2 件。下限の中央値は 30 万円。自社の下限は 21.3 万円。
        assert_eq!(props["competitor_last_date"], "2026-11-15");
        assert_eq!(props["competitor_count"], "2");
        assert_eq!(props["competitor_salary_median"], "30");
        assert_eq!(props["competitor_salary_gap"], "-8.7");
        assert_eq!(
            props["competitor_summary"],
            "競合 2件・給与中央値 30.0万円（自社 21.3万円）・人気 0件"
        );
        let rows: Vec<Value> =
            serde_json::from_str(props["competitor_monthly"].as_str().unwrap()).unwrap();
        assert_eq!(
            rows,
            vec![json!({
                "month": "2026-11",
                "run_at": "2026-11-15T16:20:00+09:00",
                "by": USER,
                "conditions": {"title": "配送 大阪", "employment_type": "不明", "prefecture": "大阪府",
                               "municipality": "大阪市", "top_n": 10, "source": "Indeed SP の一覧CSV"},
                "count": {"total": 2, "salary_parsed": 2, "popular": 0, "super_popular": 0},
                "salary": {"unit": "万円",
                           "all": {"min_mean": 30.0, "min_median": 30.0, "max_mean": 35.0, "max_median": 35.0},
                           "popular": {"min_mean": null, "min_median": null, "max_mean": null, "max_median": null}},
                "own_salary": {"min": 21.3, "max": 25.0},
                "keywords_top5": [],
                "csv_url": null,
            })]
        );
    }

    #[tokio::test]
    async fn the_same_month_is_replaced_and_a_new_month_is_added_in_order() {
        let old = json!([
            {"month":"2026-11","run_at":"2026-11-02T10:00:00+09:00","by":"old@example.test","count":{"total":99}},
            {"month":"2026-09","run_at":"2026-09-10T10:00:00+09:00","by":"old@example.test","count":{"total":7}}
        ])
        .to_string();
        let f = fixture(true, Some(&old)).await;
        let saved = call(&f, &f.session, "30", true, &report_id(), november())
            .await
            .unwrap();
        assert_eq!((saved.replaced, saved.months), (true, 2));
        let rows = stored(&f);
        assert_eq!(
            rows.iter()
                .map(|r| r["month"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["2026-09", "2026-11"]
        );
        assert_eq!(rows[0]["count"]["total"], 7, "ほかの月は触らない");
        assert_eq!(
            rows[1]["count"]["total"], 2,
            "同じ月は新しい記録に置き換わる"
        );
        assert_eq!(rows[1]["by"], USER);
        // 次の月 (JST 12/1 0:30 = UTC 11/30 15:30) は足される。
        let december = Utc.with_ymd_and_hms(2026, 11, 30, 15, 30, 0).unwrap();
        let saved = call(&f, &f.session, "30", true, &report_id(), december)
            .await
            .unwrap();
        assert_eq!(
            (saved.month.as_str(), saved.replaced, saved.months),
            ("2026-12", false, 3)
        );
        assert_eq!(
            stored(&f)
                .iter()
                .map(|r| r["month"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["2026-09", "2026-11", "2026-12"]
        );
    }

    #[test]
    fn merge_stops_before_the_character_limit_and_on_unreadable_values() {
        let entry = build_month(
            &crate::handlers::competitor::api_tests::sample_report("配送"),
            "Indeed SP の一覧CSV",
            USER,
            None,
            november(),
        );
        let one = merge(None, &entry, MONTHLY_MAX_CHARS).unwrap().0;
        let size = one.chars().count();
        // 1 件ちょうどなら入り、1 字足りなければ止まる。
        assert!(merge(None, &entry, size).is_ok());
        assert_eq!(merge(None, &entry, size - 1), Err(MergeError::TooLarge));
        // ほかの月がすでに上限近くまであるときも止まる (文字数で数える。バイトではない)。
        let filler =
            json!([{"month":"2026-01","memo":"あ".repeat(MONTHLY_MAX_CHARS - size)}]).to_string();
        assert_eq!(
            merge(Some(&filler), &entry, MONTHLY_MAX_CHARS),
            Err(MergeError::TooLarge)
        );
        for broken in ["not json", "{\"month\":\"2026-10\"}", "[{\"no_month\":1}]"] {
            assert_eq!(
                merge(Some(broken), &entry, MONTHLY_MAX_CHARS),
                Err(MergeError::Unreadable),
                "{broken}"
            );
        }
        assert_eq!(merge(Some("  "), &entry, MONTHLY_MAX_CHARS).unwrap().2, 1);
    }

    #[tokio::test]
    async fn a_record_over_the_limit_or_unreadable_is_never_written() {
        let filler = json!([{"month":"2026-01","memo":"あ".repeat(MONTHLY_MAX_CHARS)}]).to_string();
        for (existing, code) in [
            (filler.as_str(), "competitor_record_too_large"),
            ("not json", "competitor_record_unreadable"),
        ] {
            let f = fixture(true, Some(existing)).await;
            let e = call(&f, &f.session, "30", true, &report_id(), november())
                .await
                .unwrap_err();
            assert_eq!(e, (StatusCode::UNPROCESSABLE_ENTITY, code));
            assert!(f.remote.lock().unwrap().patches.is_empty());
        }
    }

    #[tokio::test]
    async fn a_bad_listing_id_or_an_unknown_report_never_reaches_hubspot() {
        let f = fixture(true, None).await;
        let id = report_id();
        for listing in ["", "abc", "30x"] {
            let e = call(&f, &f.session, listing, true, &id, november())
                .await
                .unwrap_err();
            assert_eq!(
                e,
                (StatusCode::BAD_REQUEST, "invalid_record_id"),
                "{listing:?}"
            );
        }
        // 存在しない ID と、ほかの人が作ったレポートは区別しない。
        let others =
            crate::handlers::competitor::store_report_for_test("someone@example.test", "x");
        for report in ["missing", others.as_str()] {
            let e = call(&f, &f.session, "30", true, report, november())
                .await
                .unwrap_err();
            assert_eq!(e, (StatusCode::NOT_FOUND, "competitor_report_not_found"));
        }
        let remote = f.remote.lock().unwrap();
        assert_eq!((remote.reads, remote.patches.len()), (0, 0));
    }

    #[tokio::test]
    async fn anonymous_other_domains_and_missing_csrf_never_write() {
        let f = fixture(true, None).await;
        let id = report_id();
        let e = call(&f, &session_for(None).await, "30", true, &id, november())
            .await
            .unwrap_err();
        assert_eq!(e.0, StatusCode::UNAUTHORIZED);
        let outsider = session_for(Some("other@other.test")).await;
        let e = call(&f, &outsider, "30", true, &id, november())
            .await
            .unwrap_err();
        assert_eq!(e.0, StatusCode::FORBIDDEN);
        let e = call(&f, &f.session, "30", false, &id, november())
            .await
            .unwrap_err();
        assert_eq!(e, (StatusCode::FORBIDDEN, "csrf_required"));
        let remote = f.remote.lock().unwrap();
        assert_eq!((remote.reads, remote.patches.len()), (0, 0));
    }

    #[tokio::test]
    async fn missing_hubspot_properties_give_the_friendly_error() {
        let f = fixture(false, None).await;
        let e = call(&f, &f.session, "30", true, &report_id(), november())
            .await
            .unwrap_err();
        assert_eq!(e, (StatusCode::CONFLICT, "competitor_property_missing"));
    }

    #[test]
    fn own_salary_is_only_used_when_the_unit_matches_and_the_text_is_a_number() {
        let rec = |body: &str| -> Record {
            serde_json::from_value(
                json!({"id":"1","properties":{"id_hrhakkaa":"HR-1","hrh_kyuujinhyou_honbun":body}}),
            )
            .unwrap()
        };
        let monthly = rec("給与形態：月給\n基本給与 最小：２２０，０００\n基本給与 最大：応相談");
        assert_eq!(
            own_salary(&monthly, false),
            Some(CompetitorOwnSalary {
                min: Some(22.0),
                max: None
            })
        );
        // 月給の求人を、時給の競合と比べない。
        assert_eq!(own_salary(&monthly, true), None);
        let hourly = rec("給与形態：時給\n基本給与 最小：1200");
        assert_eq!(
            own_salary(&hourly, true),
            Some(CompetitorOwnSalary {
                min: Some(1200.0),
                max: None
            })
        );
        assert_eq!(own_salary(&rec("仕事内容：配送"), false), None);
    }

    #[test]
    fn the_listing_read_returns_valid_months_in_order_and_drops_broken_ones() {
        let entry = build_month(
            &crate::handlers::competitor::api_tests::sample_report("配送"),
            "Indeed SP の一覧CSV",
            USER,
            None,
            november(),
        );
        let mut earlier = entry.clone();
        earlier.month = "2026-09".into();
        let raw = json!([entry, {"month":"2026-10","broken":true}, earlier]).to_string();
        let rec = |value: Value| -> Record {
            serde_json::from_value(json!({"id":"1","properties":{"competitor_monthly":value}}))
                .unwrap()
        };
        let rows = monthly(&rec(json!(raw)));
        assert_eq!(
            rows.iter().map(|r| r.month.as_str()).collect::<Vec<_>>(),
            ["2026-09", "2026-11"]
        );
        assert!(monthly(&rec(json!("not json"))).is_empty());
        assert!(monthly(&rec(json!(null))).is_empty());
    }

    #[test]
    fn the_summary_line_follows_the_unit_and_omits_what_is_missing() {
        let mut entry = build_month(
            &crate::handlers::competitor::api_tests::sample_report("配送"),
            "Indeed SP の一覧CSV",
            USER,
            Some(CompetitorOwnSalary {
                min: Some(22.0),
                max: None,
            }),
            november(),
        );
        entry.count.total = 86;
        entry.count.popular = Some(12);
        entry.salary.all.min_median = Some(23.5);
        assert_eq!(
            summary_line(&entry),
            "競合 86件・給与中央値 23.5万円（自社 22.0万円）・人気 12件"
        );
        entry.salary.unit = "円/時".into();
        entry.salary.all.min_median = Some(1250.0);
        entry.own_salary = None;
        entry.count.popular = None;
        assert_eq!(summary_line(&entry), "競合 86件・給与中央値 1250円");
    }
}
