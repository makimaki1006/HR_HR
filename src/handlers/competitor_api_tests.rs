//! PR-2: `/api/competitor/*` (JSON API) の契約テスト。
//!
//! 外部への実呼び出しはしない。Google 広告 API は `include_google` を送らない (= 呼ばない)、
//! Indeed 採用市場・人口統計は DB なし (`bare_state`) の経路を使う。
//! 期待値は fixture (`scripts/make_competitor_fixtures.py`) の作り方から Python で独立に数えた値
//! (Rust の出力のコピーではない)。
//!
//! 同じ状態 (30 分保持・同時送信の拒否) はプロセス内の静的な保持なので、テストごとに別のメールを使う。
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path};
use axum::http::{header, Request, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;
use tower_sessions::{MemoryStore, Session, SessionManagerLayer};

use super::api::{
    in_flight, store, CompetitorErrorCode, InFlight, ReportStore, MAX_CSV_ROWS, REPORT_TTL_SECS,
};
use crate::auth::session::RateLimiter;
use crate::config::AppConfig;
use crate::db::cache::AppCache;
use crate::AppState;

fn bare_state() -> Arc<AppState> {
    let config = AppConfig::from_env();
    let cache = AppCache::new(config.cache_ttl_secs, config.cache_max_entries);
    let rate_limiter = RateLimiter::new(
        config.rate_limit_max_attempts,
        config.rate_limit_lockout_secs,
    );
    Arc::new(AppState {
        config,
        hw_db: None,
        indeed_db: None,
        turso_db: None,
        salesnow_db: None,
        scout_db: None,
        cache,
        rate_limiter,
        company_geo_cache: None,
        audit: None,
        google_oidc: None,
        hubspot: None,
    })
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/competitor")
            .join(name),
    )
    .unwrap()
}

async fn test_login(session: Session, Path(email): Path<String>) -> &'static str {
    session
        .insert(crate::auth::SESSION_USER_KEY, email)
        .await
        .unwrap();
    "ok"
}

/// 認証ミドルウェアの代わりにセッションだけ付けた小さなアプリ (認証・CSRF は build_app 側のテストで見る)。
fn app(body_limit: usize) -> Router {
    Router::new()
        .route("/test-login/{email}", get(test_login))
        .route("/api/competitor/options", get(super::api::api_options))
        .route(
            "/api/competitor/report",
            post(super::api::api_report).layer(DefaultBodyLimit::max(body_limit)),
        )
        .route("/api/competitor/pdf", post(super::api::api_pdf))
        .with_state(bare_state())
        .layer(SessionManagerLayer::new(MemoryStore::default()))
}

async fn login(app: &Router, email: &str) -> String {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/test-login/{email}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    res.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

fn multipart(csv: &[u8], fields: &[(&str, &str)]) -> Vec<u8> {
    let mut body: Vec<u8> = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(
            format!("--gb\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
                .as_bytes(),
        );
    }
    body.extend_from_slice(
        b"--gb\r\nContent-Disposition: form-data; name=\"csv_file\"; filename=\"jobs.csv\"\r\nContent-Type: text/csv\r\n\r\n",
    );
    body.extend_from_slice(csv);
    body.extend_from_slice(b"\r\n--gb--\r\n");
    body
}

fn form(source: &'static str, mode: &'static str) -> Vec<(&'static str, &'static str)> {
    vec![
        ("source_type", source),
        ("wage_mode", mode),
        ("top_n", "10"),
        ("survey_title", "API契約"),
    ]
}

async fn post_api(
    app: &Router,
    cookie: &str,
    csv: &[u8],
    fields: &[(&str, &str)],
) -> (StatusCode, Value) {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/competitor/report")
                .header(header::COOKIE, cookie)
                .header("content-type", "multipart/form-data; boundary=gb")
                .body(Body::from(multipart(csv, fields)))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let ct = res
        .headers()
        .get(header::CONTENT_TYPE)
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default();
    assert!(ct.starts_with("application/json"), "content-type: {ct}");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).expect("JSON"))
}

/// 行数だけを増やした合成 CSV (Indeed 一般形式)。1 行ごとに別の求人。
fn many_rows(n: usize) -> Vec<u8> {
    let mut csv = String::from("タイトル,会社名,勤務地,給与,雇用形態\n");
    for i in 0..n {
        csv.push_str(&format!(
            "職種{i:06},会社{i:06},大阪府大阪市北区,月給 25万円 ~ 30万円,正社員\n"
        ));
    }
    csv.into_bytes()
}

fn assert_error(v: &Value, code: &str) {
    assert_eq!(v["error"], code, "{v}");
    let msg = v["message"].as_str().unwrap();
    assert!(!msg.is_empty());
    // 内部語・外部入力の生エラー文を出さない
    for banned in [
        "UnequalLengths",
        "Error(",
        "panicked",
        "ヘッダー読み取りエラー",
        "csv::",
        "called `",
        "Utf8",
    ] {
        assert!(
            !msg.contains(banned),
            "{banned} が message に出ている: {msg}"
        );
    }
}

// ---------------------------------------------------------------- 契約テスト (具体値)

#[tokio::test]
async fn report_json_has_exact_values_for_monthly_fixture() {
    let app = app(20 * 1024 * 1024);
    let cookie = login(&app, "contract-monthly@example.test").await;
    let (status, v) = post_api(
        &app,
        &cookie,
        &fixture("sp_utf8.csv"),
        &form("indeed_sp", "monthly"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let r = &v["report"];
    assert_eq!(r["meta"]["title"], "API契約");
    assert_eq!(r["meta"]["unit"], "万円");
    assert_eq!(r["meta"]["is_hourly"], false);
    assert_eq!(r["meta"]["total_count"], 60); // 62 行 - 重複 2 行
    assert_eq!(r["meta"]["top_n_effective"], 10);
    assert_eq!(r["meta"]["top_n_requested"], "10");
    // 月給モードの分布・件数は時給の求人も月給換算して含む (重複排除後の 60 件すべてに給与がある)
    assert_eq!(r["meta"]["salary_parsed_count"], 60);
    assert_eq!(r["meta"]["salary_missing_count"], 0);
    assert_eq!(r["excel"]["decimals"], 2);
    // 給与表 (SP の月給 44 件): 下限 平均 26.98 / 中央値 27、上限 平均 31.93 / 中央値 32 (万円。Python で独立に計算)
    let table = r["excel"]["salary_table"].as_array().unwrap();
    assert_eq!(table[0]["label"], "平均値");
    assert_eq!(table[1]["label"], "中央値");
    assert_eq!(table[2]["label"], "最頻値");
    let near =
        |x: &Value, want: f64| assert!((x.as_f64().unwrap() - want).abs() < 0.005, "{x} != {want}");
    near(&table[0]["values"][0], 26.98);
    near(&table[0]["values"][1], 31.93);
    assert_eq!(table[1]["values"][0].as_f64(), Some(27.0));
    assert_eq!(table[1]["values"][1].as_f64(), Some(32.0));
    // 分布: 階級は数値の昇順で、件数の合計は給与のある 60 件 (階級ごとの具体値は時給モードのテストで固定)
    for key in ["upper", "lower"] {
        let bins = r["excel"]["histograms"][key].as_array().unwrap();
        let labels: Vec<u64> = bins
            .iter()
            .map(|b| b["label"].as_str().unwrap().parse().unwrap())
            .collect();
        assert!(labels.windows(2).all(|w| w[0] < w[1]), "{key}: {labels:?}");
        assert_eq!(
            bins.iter()
                .map(|b| b["count"].as_u64().unwrap())
                .sum::<u64>(),
            60,
            "{key}"
        );
    }
    // ワード (全体): 求人票のタグ列 + 人気バッジ (人気・超人気) を数える。件数の多い順、同数は語の昇順。分母は求人数 60
    let kw = r["excel"]["keyword_all"].as_array().unwrap();
    let head: Vec<(&str, u64)> = kw
        .iter()
        .map(|k| (k["word"].as_str().unwrap(), k["count"].as_u64().unwrap()))
        .collect();
    assert_eq!(
        head,
        vec![
            ("交通費支給", 18),
            ("昇給あり", 18),
            ("社会保険完備", 18),
            ("未経験歓迎", 17),
            ("賞与あり", 17),
            ("駅チカ", 16),
            ("人気", 14),
            ("超人気", 4)
        ]
    );
    assert_eq!(kw[0]["jobs"], 60);
    assert_eq!(kw[0]["share_pct"].as_f64(), Some(30.0));
    // 外部が無い経路: Google は呼んでいない、Indeed 市場・人口は DB なし (都道府県未選択)
    assert_eq!(r["google"]["status"], "not_requested");
    assert_eq!(r["indeed"]["status"], "unavailable");
    assert_eq!(r["population"]["status"], "unavailable");
    // report_id は 64 桁の 16 進、保持は 30 分
    let id = v["report_id"].as_str().unwrap();
    assert_eq!(id.len(), 64);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(v["expires_in_secs"], 1800);
}

#[tokio::test]
async fn report_json_hourly_uses_integer_yen_and_fifty_yen_bins() {
    let app = app(20 * 1024 * 1024);
    let cookie = login(&app, "contract-hourly@example.test").await;
    let (status, v) = post_api(
        &app,
        &cookie,
        &fixture("sp_utf8.csv"),
        &form("indeed_sp", "hourly"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let r = &v["report"];
    assert_eq!(r["meta"]["unit"], "円/時");
    assert_eq!(r["meta"]["is_hourly"], true);
    assert_eq!(r["excel"]["decimals"], 0);
    let table = r["excel"]["salary_table"].as_array().unwrap();
    // 時給 16 件: 下限 平均 1262.5 → 四捨五入して 1263、上限 平均 1409.375 → 1409 (円)。中央値 1255 / 1405
    assert_eq!(table[0]["values"][0].as_f64(), Some(1263.0));
    assert_eq!(table[0]["values"][1].as_f64(), Some(1409.0));
    assert_eq!(table[1]["values"][0].as_f64(), Some(1255.0));
    assert_eq!(table[1]["values"][1].as_f64(), Some(1405.0));
    let upper: Vec<(String, u64)> = r["excel"]["histograms"]["upper"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["label"].as_str().unwrap().to_owned(),
                b["count"].as_u64().unwrap(),
            )
        })
        .collect();
    let want = [
        ("1250", 1),
        ("1300", 4),
        ("1350", 2),
        ("1400", 4),
        ("1450", 1),
        ("1500", 3),
        ("1550", 1),
    ];
    assert_eq!(
        upper,
        want.iter()
            .map(|(l, c)| (l.to_string(), *c))
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn top_n_is_clamped_and_reported_back() {
    let app = app(20 * 1024 * 1024);
    let cookie = login(&app, "contract-topn@example.test").await;
    let mut f = form("indeed_sp", "monthly");
    f.retain(|(k, _)| *k != "top_n");
    f.push(("top_n", "9999"));
    let (status, v) = post_api(&app, &cookie, &fixture("sp_utf8.csv"), &f).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["report"]["meta"]["top_n_effective"], 200);
    assert_eq!(v["report"]["meta"]["top_n_requested"], "9999");
    let warnings = v["report"]["meta"]["warnings"].as_array().unwrap();
    assert!(
        warnings.iter().any(|w| w.as_str().unwrap().contains("200")),
        "丸めたことを warnings に出す: {warnings:?}"
    );
}

// ---------------------------------------------------------------- エラー (コード付き JSON)

#[tokio::test]
async fn input_errors_return_coded_json_without_internal_words() {
    let app = app(20 * 1024 * 1024);
    let cookie = login(&app, "errors-input@example.test").await;
    let csv = fixture("sp_utf8.csv");
    let (s, v) = post_api(&app, &cookie, b"", &form("indeed_sp", "monthly")).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_error(&v, "csv_missing");
    let (s, v) = post_api(&app, &cookie, &csv, &form("other", "monthly")).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_error(&v, "invalid_source_type");
    let (s, v) = post_api(&app, &cookie, &csv, &form("indeed_sp", "weekly")).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_error(&v, "invalid_wage_mode");
    let mut f = form("indeed_sp", "monthly");
    f.push(("prefecture", "存在県"));
    let (s, v) = post_api(&app, &cookie, &csv, &f).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_error(&v, "invalid_prefecture");
    let long = "あ".repeat(400); // 1,200 バイト > 1,000
    let mut f = form("indeed_sp", "monthly");
    f.push(("survey_title", &long));
    let (s, v) = post_api(&app, &cookie, &csv, &f).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_error(&v, "field_too_long");
}

/// ヘッダーだけの CSV は解析の段階で弾かれる (現行の挙動)。固定文の 422 で、内部語を出さない。
#[tokio::test]
async fn header_only_csv_is_422_csv_parse_failed() {
    let app = app(20 * 1024 * 1024);
    let cookie = login(&app, "errors-headeronly@example.test").await;
    let (s, v) = post_api(
        &app,
        &cookie,
        "タイトル,会社名,勤務地,給与,雇用形態
"
        .as_bytes(),
        &form("indeed", "monthly"),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_error(&v, "csv_parse_failed");
}

#[test]
fn no_indeed_jobs_maps_to_422_with_fixed_message() {
    let e = super::api::analysis_error(super::AnalyzeError::NoIndeed);
    assert_eq!(e.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_error(&serde_json::to_value(&e.body).unwrap(), "no_indeed_jobs");
}

#[tokio::test]
async fn unreadable_csv_never_echoes_library_error_text() {
    // 解析エラーの文字列 (内部ライブラリ由来) は画面に出さず固定文にする。
    use super::api::analysis_error;
    use super::AnalyzeError;
    let e = analysis_error(AnalyzeError::Parse(
        "ヘッダー読み取りエラー: CSV error: record 1 (line: 2, byte: 40): found record with 3 fields, but the previous record has 2 fields (UnequalLengths)".into(),
    ));
    assert_eq!(e.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(e.body.error, CompetitorErrorCode::CsvParseFailed);
    let v = serde_json::to_value(&e.body).unwrap();
    assert_error(&v, "csv_parse_failed");
    // 実際の壊れた入力 (バイナリ) でも 5xx や内部語にならない
    let app = app(20 * 1024 * 1024);
    let cookie = login(&app, "errors-garbage@example.test").await;
    let (s, v) = post_api(
        &app,
        &cookie,
        &[0u8, 159, 146, 150, 0, 1, 2, 3],
        &form("indeed", "monthly"),
    )
    .await;
    assert!(s.is_client_error(), "{s} {v}");
    assert!(v["error"].is_string());
    assert_error(&v, v["error"].as_str().unwrap());
}

#[tokio::test]
async fn csv_over_the_body_limit_is_413_csv_too_large() {
    let app = app(2048);
    let cookie = login(&app, "errors-413@example.test").await;
    let (s, v) = post_api(&app, &cookie, &many_rows(300), &form("indeed", "monthly")).await;
    assert_eq!(s, StatusCode::PAYLOAD_TOO_LARGE, "{v}");
    assert_error(&v, "csv_too_large");
}

// ---------------------------------------------------------------- 5 万行

#[test]
fn row_limit_constant_is_fifty_thousand() {
    assert_eq!(MAX_CSV_ROWS, 50_000);
}

/// 境界は純関数と、上限を小さく注入した経路で見る。本物の 5 万行を集計まで通すテストは
/// debug ビルドで数分かかり cargo の枠を占有するので作らない。
#[test]
fn row_limit_boundary_is_inclusive_in_the_pure_function() {
    use super::{check_row_limit, AnalyzeError};
    assert!(
        check_row_limit(MAX_CSV_ROWS, Some(MAX_CSV_ROWS)).is_ok(),
        "ちょうどは通る"
    );
    assert!(matches!(
        check_row_limit(MAX_CSV_ROWS + 1, Some(MAX_CSV_ROWS)),
        Err(AnalyzeError::TooManyRows(50_001))
    ));
    assert!(check_row_limit(0, Some(MAX_CSV_ROWS)).is_ok());
    assert!(
        check_row_limit(usize::MAX, None).is_ok(),
        "旧画面は上限なし"
    );
}

#[tokio::test]
async fn injected_small_limit_accepts_exactly_the_limit_and_rejects_one_more() {
    use super::{analyze, AnalyzeError};
    use crate::handlers::survey::upload::{UserSourceHint, WageMode};
    let ok = analyze(
        many_rows(3),
        UserSourceHint::Indeed,
        WageMode::Monthly,
        "",
        Some(3),
    )
    .await;
    assert_eq!(ok.expect("3 行 / 上限 3 は通る").total_count, 3);
    let ng = analyze(
        many_rows(4),
        UserSourceHint::Indeed,
        WageMode::Monthly,
        "",
        Some(3),
    )
    .await;
    assert!(
        matches!(ng, Err(AnalyzeError::TooManyRows(4))),
        "4 行 / 上限 3 は止める"
    );
    // 上限エラーは 422 csv_too_many_rows、文言に上限値を含み内部語を含まない
    let e = super::api::analysis_error(AnalyzeError::TooManyRows(50_001));
    assert_eq!(e.status, StatusCode::UNPROCESSABLE_ENTITY);
    let v = serde_json::to_value(&e.body).unwrap();
    assert_error(&v, "csv_too_many_rows");
    assert!(v["message"].as_str().unwrap().contains("50000"));
}

// ---------------------------------------------------------------- 30 分保持

#[test]
fn stored_report_expires_after_thirty_minutes() {
    assert_eq!(REPORT_TTL_SECS, 30 * 60);
    let s = ReportStore::new(Duration::from_secs(REPORT_TTL_SECS), 100);
    let t0 = Instant::now();
    let id = s.insert("a@example.test", sample_report("x"), t0);
    assert!(s.get("a@example.test", &id, t0).is_some());
    assert!(s
        .get(
            "a@example.test",
            &id,
            t0 + Duration::from_secs(29 * 60 + 59)
        )
        .is_some());
    assert!(
        s.get("a@example.test", &id, t0 + Duration::from_secs(30 * 60 + 1))
            .is_none(),
        "30 分を過ぎたら読めない"
    );
    // 期限切れは取り出しで消える (メモリに残さない)
    assert_eq!(s.len(), 0);
}

#[test]
fn stored_report_is_unreadable_by_other_users_and_unknown_ids() {
    let s = ReportStore::new(Duration::from_secs(REPORT_TTL_SECS), 100);
    let t0 = Instant::now();
    let id = s.insert("owner@example.test", sample_report("x"), t0);
    assert!(
        s.get("other@example.test", &id, t0).is_none(),
        "本人以外は読めない"
    );
    assert!(
        s.get("OWNER@example.test ", &id, t0).is_some(),
        "メールの大小・前後空白は同一人物"
    );
    assert!(s.get("owner@example.test", &"0".repeat(64), t0).is_none());
    assert!(s.get("owner@example.test", "", t0).is_none());
    assert!(
        s.get("owner@example.test", &id.to_uppercase(), t0)
            .is_none()
            || id == id.to_uppercase()
    );
    // 他人が読もうとしても本人の分は消えない
    assert!(s.get("owner@example.test", &id, t0).is_some());
}

#[test]
fn report_ids_are_unique_and_unguessable_length() {
    let s = ReportStore::new(Duration::from_secs(REPORT_TTL_SECS), 100);
    let t0 = Instant::now();
    let ids: std::collections::HashSet<String> = (0..50)
        .map(|_| s.insert("u@example.test", sample_report("x"), t0))
        .collect();
    assert_eq!(ids.len(), 50);
    assert!(ids.iter().all(|i| i.len() == 64));
}

#[test]
fn store_is_bounded_and_evicts_oldest() {
    let s = ReportStore::new(Duration::from_secs(REPORT_TTL_SECS), 3);
    let t0 = Instant::now();
    let a = s.insert("u@example.test", sample_report("a"), t0);
    let b = s.insert(
        "u@example.test",
        sample_report("b"),
        t0 + Duration::from_secs(1),
    );
    let c = s.insert(
        "u@example.test",
        sample_report("c"),
        t0 + Duration::from_secs(2),
    );
    let d = s.insert(
        "u@example.test",
        sample_report("d"),
        t0 + Duration::from_secs(3),
    );
    assert_eq!(s.len(), 3);
    let now = t0 + Duration::from_secs(4);
    assert!(
        s.get("u@example.test", &a, now).is_none(),
        "最も古い 1 件が追い出される"
    );
    for id in [&b, &c, &d] {
        assert!(s.get("u@example.test", id, now).is_some());
    }
}

#[tokio::test]
async fn report_id_from_the_api_is_readable_only_by_the_creator() {
    let app = app(20 * 1024 * 1024);
    let cookie = login(&app, "owner-http@example.test").await;
    let (s, v) = post_api(
        &app,
        &cookie,
        &fixture("sp_utf8.csv"),
        &form("indeed_sp", "monthly"),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let id = v["report_id"].as_str().unwrap();
    let now = Instant::now();
    let stored = store()
        .get("owner-http@example.test", id, now)
        .expect("本人は読める");
    assert_eq!(stored.meta.total_count, 60);
    assert!(store().get("someone-else@example.test", id, now).is_none());
}

// ---------------------------------------------------------------- 同時送信

#[test]
fn in_flight_guard_allows_one_per_user_and_releases_on_drop() {
    let f = InFlight::new();
    let a = f.try_acquire("a@example.test").expect("1 本目");
    assert!(f.try_acquire("a@example.test").is_none(), "2 本目は拒否");
    assert!(
        f.try_acquire(" A@example.test").is_none(),
        "大小・前後空白は同一人物"
    );
    let b = f.try_acquire("b@example.test").expect("別ユーザーは通る");
    drop(a);
    let again = f
        .try_acquire("a@example.test")
        .expect("1 本目が終われば通る");
    drop(again);
    drop(b);
    assert!(f.try_acquire("a@example.test").is_some());
}

#[test]
fn in_flight_guard_is_released_when_the_holder_panics() {
    let f = Arc::new(InFlight::new());
    let f2 = f.clone();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _g = f2.try_acquire("p@example.test").unwrap();
        panic!("1 本目が失敗");
    }));
    assert!(r.is_err());
    assert!(
        f.try_acquire("p@example.test").is_some(),
        "失敗しても解放される"
    );
}

#[tokio::test]
async fn concurrent_requests_from_one_user_let_exactly_one_through() {
    let app = app(20 * 1024 * 1024);
    let cookie = login(&app, "concurrent@example.test").await;
    let csv = many_rows(5_000);
    let f = form("indeed", "monthly");
    let (a, b) = tokio::join!(
        post_api(&app, &cookie, &csv, &f),
        post_api(&app, &cookie, &csv, &f)
    );
    let mut statuses = [a.0, b.0];
    statuses.sort();
    assert_eq!(
        statuses,
        [StatusCode::OK, StatusCode::TOO_MANY_REQUESTS],
        "{a:?} {b:?}"
    );
    let rejected = if a.0 == StatusCode::TOO_MANY_REQUESTS {
        &a.1
    } else {
        &b.1
    };
    assert_error(rejected, "report_in_progress");
    // 1 本目が終わった後は通る
    let (s, v) = post_api(
        &app,
        &cookie,
        &fixture("sp_utf8.csv"),
        &form("indeed_sp", "monthly"),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
}

#[tokio::test]
async fn failed_first_request_releases_the_slot() {
    let app = app(20 * 1024 * 1024);
    let cookie = login(&app, "failthenok@example.test").await;
    let (s, _) = post_api(
        &app,
        &cookie,
        &fixture("sp_utf8.csv"),
        &form("bad", "monthly"),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = post_api(&app, &cookie, b"", &form("indeed_sp", "monthly")).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, v) = post_api(
        &app,
        &cookie,
        &fixture("sp_utf8.csv"),
        &form("indeed_sp", "monthly"),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
}

#[tokio::test]
async fn a_held_slot_rejects_the_same_user_but_not_others() {
    let app = app(20 * 1024 * 1024);
    let cookie = login(&app, "held@example.test").await;
    let other = login(&app, "held-other@example.test").await;
    let g = in_flight().try_acquire("held@example.test").unwrap();
    let (s, v) = post_api(
        &app,
        &cookie,
        &fixture("sp_utf8.csv"),
        &form("indeed_sp", "monthly"),
    )
    .await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert_error(&v, "report_in_progress");
    let (s, _) = post_api(
        &app,
        &other,
        &fixture("sp_utf8.csv"),
        &form("indeed_sp", "monthly"),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "別ユーザーは止めない");
    drop(g);
    let (s, _) = post_api(
        &app,
        &cookie,
        &fixture("sp_utf8.csv"),
        &form("indeed_sp", "monthly"),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
}

// ---------------------------------------------------------------- options

#[tokio::test]
async fn options_lists_47_prefectures_and_reports_market_unavailable_without_db() {
    let app = app(1024);
    let cookie = login(&app, "options@example.test").await;
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/competitor/options")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(v["prefectures"].as_array().unwrap().len(), 47);
    assert_eq!(v["prefectures"][0], "北海道");
    assert_eq!(v["market_available"], false);
    assert_eq!(v["titles"], json!([]));
}

// ---------------------------------------------------------------- 認証・CSRF (本物のルーター)

#[tokio::test]
async fn api_routes_require_login_as_json_and_reject_foreign_or_headerless_writes() {
    let app = crate::build_app(bare_state());
    // 未ログイン: JSON の 401 (HTML の /login へは飛ばさない)
    for (method, path) in [
        ("GET", "/api/competitor/options"),
        ("POST", "/api/competitor/report"),
    ] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("origin", "https://hr-hw.onrender.com")
                    .header("accept", "application/json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{method} {path}");
        let v: Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(v["error"], "auth_required");
    }
    // 外部 Origin の POST は 403
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/competitor/report")
                .header("origin", "https://foreign.example")
                .header("accept", "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    // Origin も Referer も X-Requested-With も無い POST は 403
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/competitor/report")
                .header("accept", "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

// ---------------------------------------------------------------- 補助

fn sample_report(
    title: &str,
) -> crate::handlers::survey::report_html::competitor_model::CompetitorReport {
    use crate::handlers::survey::aggregator::{aggregate_records_with_mode, salary_fixture};
    use crate::handlers::survey::report_html::competitor_model::build_competitor_report;
    use crate::handlers::survey::upload::WageMode;
    let records = salary_fixture::records(
        "月給 25万円 ~ 30万円\n月給 35万円 ~ 40万円",
        "大阪府 大阪市",
    );
    let agg = aggregate_records_with_mode(&records, WageMode::Monthly);
    let none = json!({"status":"unavailable","message":"x"});
    build_competitor_report(&agg, 10, title, &none, &none, &none)
}

// ---------------------------------------------------------------- PDF (PR-3)
//
// 描画エンジン (Chromium) を使わないテストは、生成関数を差し替えた `pdf_response` で見る。
// 実 Chromium を使うテストは 1 本だけで `#[ignore]` (下記)。

use super::api::{classify_pdf_error, content_disposition, jst_date, pdf_filename, pdf_response};
use chrono::{NaiveDate, TimeZone, Utc};
use std::sync::atomic::{AtomicUsize, Ordering};

const GOOD_PDF: &[u8] = b"%PDF-1.4\nbody\n%%EOF\n";
const BUSY_MSG: &str = "PDF作成が混み合っています。少し時間をおいて再度お試しください。";
const TIMEOUT_MSG: &str = "PDFの作成に時間がかかっています。再度お試しください。";

fn day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 4).unwrap()
}

async fn body_json(res: axum::response::Response) -> (StatusCode, Value) {
    let status = res.status();
    let ct = res
        .headers()
        .get(header::CONTENT_TYPE)
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default();
    assert!(ct.starts_with("application/json"), "content-type: {ct}");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).expect("JSON"))
}

/// 生成関数の呼び出し回数を数える (呼ばれてはいけない場面の検査用)。
fn counting_ok(
    counter: &Arc<AtomicUsize>,
) -> impl FnOnce(String) -> std::future::Ready<Result<Vec<u8>, String>> {
    let c = counter.clone();
    move |_html| {
        c.fetch_add(1, Ordering::SeqCst);
        std::future::ready(Ok(GOOD_PDF.to_vec()))
    }
}

fn stored_report(store: &ReportStore, owner: &str, now: Instant) -> String {
    let mut r = sample_report("施設長");
    r.meta.prefecture = Some("大阪府".to_owned());
    store.insert(owner, r, now)
}

#[tokio::test]
async fn pdf_of_another_users_report_is_not_found_and_does_not_render() {
    let store = ReportStore::new(Duration::from_secs(1800), 10);
    let flight = InFlight::new();
    let now = Instant::now();
    let id = stored_report(&store, "owner@example.test", now);
    let calls = Arc::new(AtomicUsize::new(0));
    let res = pdf_response(
        &store,
        &flight,
        "other@example.test",
        &id,
        now,
        day(),
        counting_ok(&calls),
    )
    .await;
    let (status, v) = body_json(res).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{v}");
    assert_error(&v, "report_not_found");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn pdf_of_expired_report_is_not_found_but_exactly_at_expiry_still_works() {
    let store = ReportStore::new(Duration::from_secs(1800), 10);
    let flight = InFlight::new();
    let t0 = Instant::now();
    let id = stored_report(&store, "exp@example.test", t0);
    let calls = Arc::new(AtomicUsize::new(0));
    let at = t0 + Duration::from_secs(1800);
    let res = pdf_response(
        &store,
        &flight,
        "exp@example.test",
        &id,
        at,
        day(),
        counting_ok(&calls),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "ちょうど 30 分はまだ有効");
    let at = t0 + Duration::from_secs(1801);
    let res = pdf_response(
        &store,
        &flight,
        "exp@example.test",
        &id,
        at,
        day(),
        counting_ok(&calls),
    )
    .await;
    let (status, v) = body_json(res).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_error(&v, "report_not_found");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn pdf_with_unknown_or_malformed_id_is_not_found() {
    let store = ReportStore::new(Duration::from_secs(1800), 10);
    let flight = InFlight::new();
    let now = Instant::now();
    let _ = stored_report(&store, "m@example.test", now);
    let calls = Arc::new(AtomicUsize::new(0));
    let long = "a".repeat(100_000);
    let zeros = "0".repeat(64);
    let ids = [
        "",
        " ",
        zeros.as_str(),
        "../../etc/passwd",
        "..\\..\\x",
        "あいう",
        "abc\r\ndef",
        "'; DROP TABLE x;--",
        long.as_str(),
    ];
    for id in ids {
        let res = pdf_response(
            &store,
            &flight,
            "m@example.test",
            id,
            now,
            day(),
            counting_ok(&calls),
        )
        .await;
        let (status, v) = body_json(res).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "id={:?}",
            id.chars().take(20).collect::<String>()
        );
        assert_error(&v, "report_not_found");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn pdf_success_has_pdf_body_headers_and_japanese_filename() {
    let store = ReportStore::new(Duration::from_secs(1800), 10);
    let flight = InFlight::new();
    let now = Instant::now();
    let id = stored_report(&store, "Ok@Example.test", now);
    let seen = Arc::new(std::sync::Mutex::new(String::new()));
    let s2 = seen.clone();
    // メールの大文字小文字は区別しない (store と同じ扱い)
    let res = pdf_response(
        &store,
        &flight,
        "ok@example.test",
        &id,
        now,
        day(),
        move |html| {
            *s2.lock().unwrap() = html;
            std::future::ready(Ok(GOOD_PDF.to_vec()))
        },
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let h = res.headers().clone();
    assert_eq!(h[header::CONTENT_TYPE], "application/pdf");
    assert_eq!(h[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        h[header::CONTENT_DISPOSITION].to_str().unwrap(),
        "attachment; filename=\"competitor-report-2026-10-04.pdf\"; filename*=UTF-8''%E7%AB%B6%E5%90%88%E8%AA%BF%E6%9F%BB_%E5%A4%A7%E9%98%AA%E5%BA%9C_%E6%96%BD%E8%A8%AD%E9%95%B7_2026-10-04.pdf"
    );
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(bytes.ends_with(b"%%EOF\n"));
    // 保持中のレポートから作った HTML (Google の再取得ではない) が渡っている
    let html = seen.lock().unwrap().clone();
    assert!(html.contains("施設長"), "report title missing from html");
    assert!(html.contains("tabpanel"));
}

#[tokio::test]
async fn pdf_failure_is_json_and_same_report_id_can_be_retried() {
    let store = ReportStore::new(Duration::from_secs(1800), 10);
    let flight = InFlight::new();
    let now = Instant::now();
    let id = stored_report(&store, "retry@example.test", now);
    let cases = [
        (BUSY_MSG, StatusCode::SERVICE_UNAVAILABLE, "pdf_busy"),
        (TIMEOUT_MSG, StatusCode::GATEWAY_TIMEOUT, "pdf_timeout"),
        (
            "PDFを作成できませんでした。再度お試しください。",
            StatusCode::SERVICE_UNAVAILABLE,
            "pdf_failed",
        ),
        (
            "PDFの作成準備に失敗しました。",
            StatusCode::SERVICE_UNAVAILABLE,
            "pdf_failed",
        ),
        (
            "未知の文言 /tmp/secret chromium.log",
            StatusCode::SERVICE_UNAVAILABLE,
            "pdf_failed",
        ),
    ];
    for (msg, want_status, want_code) in cases {
        let res = pdf_response(
            &store,
            &flight,
            "retry@example.test",
            &id,
            now,
            day(),
            move |_| std::future::ready(Err::<Vec<u8>, String>(msg.to_owned())),
        )
        .await;
        let (status, v) = body_json(res).await;
        assert_eq!(status, want_status, "{msg}");
        assert_error(&v, want_code);
        // 生成関数の生の文言は出さない
        assert!(!v["message"].as_str().unwrap().contains("/tmp"));
        // 失敗しても枠は解放され、レポートは残り、同じ ID で再試行できる
        let calls = Arc::new(AtomicUsize::new(0));
        let ok = pdf_response(
            &store,
            &flight,
            "retry@example.test",
            &id,
            now,
            day(),
            counting_ok(&calls),
        )
        .await;
        assert_eq!(ok.status(), StatusCode::OK, "retry after {want_code}");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn pdf_output_that_is_not_a_complete_pdf_is_pdf_failed() {
    let store = ReportStore::new(Duration::from_secs(1800), 10);
    let flight = InFlight::new();
    let now = Instant::now();
    let id = stored_report(&store, "bad@example.test", now);
    for bytes in [
        b"<html>".to_vec(),
        b"%PDF-1.4 truncated".to_vec(),
        Vec::new(),
    ] {
        let b = bytes.clone();
        let res = pdf_response(
            &store,
            &flight,
            "bad@example.test",
            &id,
            now,
            day(),
            move |_| std::future::ready(Ok(b)),
        )
        .await;
        let (status, v) = body_json(res).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{bytes:?}");
        assert_error(&v, "pdf_failed");
    }
}

#[tokio::test]
async fn second_concurrent_pdf_for_same_user_is_429_and_other_user_is_not_blocked() {
    let store = ReportStore::new(Duration::from_secs(1800), 10);
    let flight = InFlight::new();
    let now = Instant::now();
    let id = stored_report(&store, "busy@example.test", now);
    let id2 = stored_report(&store, "free@example.test", now);
    let calls = Arc::new(AtomicUsize::new(0));
    {
        // 1 本目が作成中 (レポート作成中も同じ枠を使う)
        let _slot = flight.try_acquire("Busy@Example.test").unwrap();
        let res = pdf_response(
            &store,
            &flight,
            "busy@example.test",
            &id,
            now,
            day(),
            counting_ok(&calls),
        )
        .await;
        let (status, v) = body_json(res).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_error(&v, "report_in_progress");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let res = pdf_response(
            &store,
            &flight,
            "free@example.test",
            &id2,
            now,
            day(),
            counting_ok(&calls),
        )
        .await;
        assert_eq!(res.status(), StatusCode::OK);
    }
    // 1 本目が終われば通る
    let res = pdf_response(
        &store,
        &flight,
        "busy@example.test",
        &id,
        now,
        day(),
        counting_ok(&calls),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn truly_concurrent_pdfs_for_one_user_run_one_at_a_time() {
    let store = Arc::new(ReportStore::new(Duration::from_secs(1800), 10));
    let flight = Arc::new(InFlight::new());
    let now = Instant::now();
    let id = stored_report(&store, "race@example.test", now);
    let (started_tx, started_rx) = tokio::sync::oneshot::channel::<()>();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
    let (s1, f1, i1) = (store.clone(), flight.clone(), id.clone());
    let first = tokio::spawn(async move {
        pdf_response(
            &s1,
            &f1,
            "race@example.test",
            &i1,
            now,
            day(),
            move |_| async move {
                let _ = started_tx.send(());
                let _ = release_rx.await;
                Ok(GOOD_PDF.to_vec())
            },
        )
        .await
        .status()
    });
    started_rx.await.unwrap(); // 1 本目が生成の途中
    let calls = Arc::new(AtomicUsize::new(0));
    let second = pdf_response(
        &store,
        &flight,
        "race@example.test",
        &id,
        now,
        day(),
        counting_ok(&calls),
    )
    .await;
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
    release_tx.send(()).unwrap();
    assert_eq!(first.await.unwrap(), StatusCode::OK);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn pdf_error_classification_is_pinned_to_the_messages_in_competitor_pdf_rs() {
    // 描画側 (無改修) の固定文言が変わったらここが赤くなる。
    let src = include_str!("competitor_pdf.rs");
    for m in [BUSY_MSG, TIMEOUT_MSG] {
        assert!(src.contains(m), "competitor_pdf.rs から文言が消えた: {m}");
    }
    assert_eq!(
        classify_pdf_error(BUSY_MSG),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            CompetitorErrorCode::PdfBusy
        )
    );
    assert_eq!(
        classify_pdf_error(TIMEOUT_MSG),
        (StatusCode::GATEWAY_TIMEOUT, CompetitorErrorCode::PdfTimeout)
    );
    assert_eq!(
        classify_pdf_error("PDFを作成できませんでした。"),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            CompetitorErrorCode::PdfFailed
        )
    );
    assert_eq!(
        classify_pdf_error(""),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            CompetitorErrorCode::PdfFailed
        )
    );
}

fn meta_with(
    title: &str,
    pref: Option<&str>,
) -> crate::handlers::survey::report_html::competitor_model::ReportMeta {
    let mut m = sample_report("x").meta;
    m.title = title.to_owned();
    m.prefecture = pref.map(str::to_owned);
    m
}

#[test]
fn filename_follows_the_decided_pattern() {
    assert_eq!(
        pdf_filename(&meta_with("施設長", Some("大阪府")), day()),
        "競合調査_大阪府_施設長_2026-10-04.pdf"
    );
    // 都道府県なしは「全国」
    assert_eq!(
        pdf_filename(&meta_with("施設長", None), day()),
        "競合調査_全国_施設長_2026-10-04.pdf"
    );
    assert_eq!(
        pdf_filename(&meta_with("施設長", Some("")), day()),
        "競合調査_全国_施設長_2026-10-04.pdf"
    );
    // 調査名が空・記号だけでも壊れない
    assert_eq!(
        pdf_filename(&meta_with("", Some("大阪府")), day()),
        "競合調査_大阪府_2026-10-04.pdf"
    );
    assert_eq!(
        pdf_filename(&meta_with("///", Some("大阪府")), day()),
        "競合調査_大阪府_2026-10-04.pdf"
    );
}

#[test]
fn filename_replaces_dangerous_characters_and_limits_length() {
    let nasty = "a/b\\c\r\nd\"e\0f\tg:h*i?j<k>l|m;n%o#p\u{202e}q\u{200b}r s";
    let name = pdf_filename(&meta_with(nasty, Some("../大阪府")), day());
    for bad in [
        '/', '\\', '\r', '\n', '"', '\0', '\t', ':', '*', '?', '<', '>', '|', ';', '%', '#',
        '\u{202e}', '\u{200b}', ' ',
    ] {
        assert!(!name.contains(bad), "{bad:?} が残った: {name:?}");
    }
    assert!(!name.contains(".."), "{name:?}");
    assert!(name.starts_with("競合調査_"), "{name:?}");
    assert!(name.ends_with("_2026-10-04.pdf"), "{name:?}");
    // 調査名は 30 文字まで (危険文字は `_` に置換され、連続は 1 つにまとまる)
    assert_eq!(
        name,
        "競合調査_大阪府_a_b_c_d_e_f_g_h_i_j_k_l_m_n_o_2026-10-04.pdf"
    );
    let name = pdf_filename(
        &meta_with("x\u{202e}y\u{200b}z s%#&\u{feff}", Some("大阪府")),
        day(),
    );
    assert_eq!(name, "競合調査_大阪府_x_y_z_s_2026-10-04.pdf");
    // 超長
    let long = "あ".repeat(5000);
    let name = pdf_filename(&meta_with(&long, Some(&long)), day());
    assert!(name.chars().count() <= 100, "{}", name.chars().count());
    assert!(name.ends_with("_2026-10-04.pdf"));
    assert!(name.starts_with("競合調査_"));
}

#[test]
fn content_disposition_is_single_line_ascii_with_rfc5987_filename() {
    let v = content_disposition("競合調査_大阪府_施設長_2026-10-04.pdf");
    assert!(v.is_ascii());
    assert!(!v.contains('\n') && !v.contains('\r'));
    assert_eq!(
        v,
        "attachment; filename=\"competitor-report-2026-10-04.pdf\"; filename*=UTF-8''%E7%AB%B6%E5%90%88%E8%AA%BF%E6%9F%BB_%E5%A4%A7%E9%98%AA%E5%BA%9C_%E6%96%BD%E8%A8%AD%E9%95%B7_2026-10-04.pdf"
    );
    assert!(header::HeaderValue::from_str(&v).is_ok());
    // 日付を含まない名前でもフォールバックは固定名
    let v = content_disposition("report.pdf");
    assert!(v.contains("filename*=UTF-8''report.pdf"), "{v}");
}

#[test]
fn date_in_filename_is_jst() {
    // UTC 15:00 = JST 翌日 0:00
    let t = Utc.with_ymd_and_hms(2026, 10, 3, 15, 0, 0).unwrap();
    assert_eq!(jst_date(t), day());
    let t = Utc.with_ymd_and_hms(2026, 10, 3, 14, 59, 59).unwrap();
    assert_eq!(jst_date(t), NaiveDate::from_ymd_opt(2026, 10, 3).unwrap());
}

// ---- HTTP (ルーター経由)

async fn post_pdf(app: &Router, cookie: Option<&str>, body: &str) -> axum::response::Response {
    let mut req = Request::builder()
        .method("POST")
        .uri("/api/competitor/pdf")
        .header("content-type", "application/json");
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    app.clone()
        .oneshot(req.body(Body::from(body.to_owned())).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn pdf_http_requires_login_and_valid_body() {
    let app = app(1024 * 1024);
    let res = post_pdf(&app, None, "{}").await;
    let (status, v) = body_json(res).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(v["error"], "auth_required");
    let cookie = login(&app, "pdf-http-body@example.test").await;
    for body in ["", "not json", "{}", r#"{"report_id": 5}"#, "[]"] {
        let (status, v) = body_json(post_pdf(&app, Some(&cookie), body).await).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_error(&v, "invalid_request");
    }
    let (status, v) =
        body_json(post_pdf(&app, Some(&cookie), r#"{"report_id":"nope"}"#).await).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_error(&v, "report_not_found");
}

#[tokio::test]
async fn pdf_http_other_user_gets_not_found_and_broken_chromium_gives_json_error() {
    let app = app(1024 * 1024);
    let owner = "pdf-http-owner@example.test";
    let id = stored_report(store(), owner, Instant::now());
    let other_cookie = login(&app, "pdf-http-other@example.test").await;
    let body = format!(r#"{{"report_id":"{id}"}}"#);
    let (status, v) = body_json(post_pdf(&app, Some(&other_cookie), &body).await).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_error(&v, "report_not_found");

    // Chromium のパスを壊す。他のテストはこの環境変数を使わない (実 Chromium のテストは #[ignore])。
    let prev = std::env::var_os("PDF_CHROMIUM_PATH");
    std::env::set_var("PDF_CHROMIUM_PATH", "Z:/definitely/not/chromium.exe");
    let cookie = login(&app, owner).await;
    let (status, v) = body_json(post_pdf(&app, Some(&cookie), &body).await).await;
    // 再試行 (同じ report_id) も 404 にならず、同じ JSON エラーになる
    let (status2, v2) = body_json(post_pdf(&app, Some(&cookie), &body).await).await;
    match prev {
        Some(p) => std::env::set_var("PDF_CHROMIUM_PATH", p),
        None => std::env::remove_var("PDF_CHROMIUM_PATH"),
    }
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{v}");
    assert_error(&v, "pdf_failed");
    assert_eq!(status2, StatusCode::SERVICE_UNAVAILABLE, "{v2}");
    assert_error(&v2, "pdf_failed");
    assert!(
        store().get(owner, &id, Instant::now()).is_some(),
        "失敗してもレポートは残る"
    );
}

#[tokio::test]
#[ignore = "Requires a Chromium/Edge executable (PDF_CHROMIUM_PATH or the default path), node and playwright-core; run with --ignored"]
async fn pdf_http_real_chromium_returns_valid_pdf() {
    let app = app(1024 * 1024);
    let owner = "pdf-real@example.test";
    let id = stored_report(store(), owner, Instant::now());
    let cookie = login(&app, owner).await;
    let res = post_pdf(&app, Some(&cookie), &format!(r#"{{"report_id":"{id}"}}"#)).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()[header::CONTENT_TYPE], "application/pdf");
    let cd = res.headers()[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(cd.contains("filename*=UTF-8''"), "{cd}");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(bytes[bytes.len().saturating_sub(32)..]
        .windows(5)
        .any(|w| w == b"%%EOF"));
    assert!(bytes.len() > 10_000);
}
