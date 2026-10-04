//! PR-1: 旧 `/competitor` が出すレポート HTML(画面用)と PDF 用 HTML をバイト単位で固定する。
//!
//! 目的は「PR-2 で HTML を構造体から出す作りに変えても、出力が 1 バイトも変わらない」ことの検知。
//! 本番コードは何も変えていない(このファイルを子モジュールとして読み込む 1 行だけが competitor.rs にある)。
//! golden は `tests/fixtures/competitor/golden/*` に置く。
//!
//! 作り直すとき(意図して出力を変えたときだけ): `COMPETITOR_UPDATE_GOLDEN=1 cargo test --lib competitor`
//! 再生成した差分は目で確認してからコミットすること。
//!
//! 外部依存の扱い:
//! - Google 広告 API: ネットワークに出る `google_context` は呼ばない。応答の JSON を固定値で与える
//!   (形は `media_engine/handlers.rs` の実際の応答と同じ)。
//! - Indeed 採用市場 DB: `Snapshot` を手で組み、本物の `indeed_context` に通す(DB なしの経路は `None`)。
//! - 人口・賃金統計: 年齢別人口と最低賃金は固定 JSON。`population_context` は今日の日付で最低賃金が
//!   変わるので、日付に依存しない「都道府県未選択」の経路だけ本物を使う。
//!
//! 日時の正規化: 不要だった。画面用 HTML は現在時刻を含まない(`no_wall_clock_in_html` で確認)。
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use super::{indeed_context, pdf, population_context};
use crate::auth::session::RateLimiter;
use crate::config::AppConfig;
use crate::db::cache::AppCache;
use crate::handlers::survey::aggregator::aggregate_records_with_mode;
use crate::handlers::survey::report_html::render_competitor_report;
use crate::handlers::survey::upload::{parse_csv_bytes_with_hints, UserSourceHint, WageMode};
use crate::indeed::data::{PrefSeries, Series, Snapshot};
use crate::AppState;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/competitor")
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(name)).unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

/// golden と実出力をバイト単位で比べる。違えば最初の食い違い位置と前後を出して落ちる。
fn check_golden(name: &str, actual: &str) {
    let path = fixtures().join("golden").join(name);
    if std::env::var_os("COMPETITOR_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, actual.as_bytes()).unwrap();
        return;
    }
    let expected = std::fs::read(&path).unwrap_or_else(|e| {
        panic!("golden {name} がありません ({e}). COMPETITOR_UPDATE_GOLDEN=1 で採取")
    });
    assert!(
        expected.len() > 200,
        "golden {name} が空に近い ({} bytes)",
        expected.len()
    );
    let actual = actual.as_bytes();
    if expected == actual {
        return;
    }
    let at = expected
        .iter()
        .zip(actual.iter())
        .position(|(a, b)| a != b)
        .unwrap_or(expected.len().min(actual.len()));
    let around = |b: &[u8]| {
        String::from_utf8_lossy(&b[at.saturating_sub(60).min(b.len())..(at + 60).min(b.len())])
            .into_owned()
    };
    panic!(
        "golden {name} と不一致: 最初の差は byte {at} (golden {} bytes / 実際 {} bytes)\n  golden: ...{}...\n  actual: ...{}...",
        expected.len(),
        actual.len(),
        around(&expected),
        around(actual)
    );
}

// ---------------------------------------------------------------- 固定入力

fn snapshot() -> Snapshot {
    let series = Series {
        job: vec![Some(1200.0), Some(1180.0), None, Some(1250.0)],
        ctk: vec![Some(3400.0), Some(3300.0), Some(3100.0), None],
        emp: vec![Some(310.0), None, Some(295.0), Some(305.0)],
    };
    let mut snap = Snapshot::default();
    snap.meta.months = vec![
        "2026-05".into(),
        "2026-06".into(),
        "2026-07".into(),
        "2026-08".into(),
    ];
    snap.meta.latest = "2026-08".into();
    snap.meta.source = "合成テストデータ(Indeed採用市場の形のみ)".into();
    snap.meta.caveat = "検証用。実データではありません。".into();
    snap.meta.built_at = "2026-09-01".into();
    snap.by_title.insert("施設長".into(), series.clone());
    snap.by_pref.push(PrefSeries {
        prefecture: "大阪府".into(),
        title: "施設長".into(),
        series,
        mobile_pct: None,
    });
    snap
}

fn indeed_ok() -> Value {
    indeed_context(Some(&snapshot()), "施設長", "大阪府")
}
fn indeed_none() -> Value {
    indeed_context(None, "施設長", "大阪府")
}
fn indeed_no_series() -> Value {
    indeed_context(Some(&snapshot()), "存在しない職種", "大阪府")
}

fn google_ok() -> Value {
    let months = |base: i64| {
        (1..=12)
            .map(|m| json!({"month": format!("2025-{m:02}"), "search_volume": base + m * 10}))
            .collect::<Vec<_>>()
    };
    json!({"status":"ok","keyword":"施設長 求人","region":"大阪府",
      "demand":{"status":"ok","region":{"name":"大阪府","geo_id":"1","geo_type":"Province","canonical_name":"Osaka,Japan"},
        "noise_floor":0,"months":12,
        "keywords":[
          {"keyword":"施設長 求人","avg_monthly":320,"monthly_12m":months(300),"months_count":12,"competition":"HIGH","competition_index":80,"bid_low_yen":120,"bid_high_yen":480,"seasonality":null,"yoy":null,"recurring_peaks":null},
          {"keyword":"施設長 転職","avg_monthly":210,"monthly_12m":months(190),"months_count":12,"competition":"MEDIUM","competition_index":55,"bid_low_yen":90,"bid_high_yen":300,"seasonality":null,"yoy":null,"recurring_peaks":null}],
        "excluded_keywords":[]},
      "suggestions":{"status":"ok","suggestions":[
          {"keyword":"施設長 年収","avg_monthly":170},{"keyword":"施設長 資格","avg_monthly":90},{"keyword":"施設長 求人 大阪","avg_monthly":40}]}})
}
fn google_missing() -> Value {
    let m = json!({"status":"missing_credentials","message":"Google Ads の資格情報が未設定です","missing":["GOOGLE_ADS_DEVELOPER_TOKEN"]});
    json!({"status":"ok","keyword":"施設長 求人","region":"大阪府","demand":m,"suggestions":m})
}
fn google_error() -> Value {
    json!({"status":"ok","keyword":"施設長 求人","region":"大阪府",
      "demand":{"status":"error","message":"secret-token 認証に失敗しました"},
      "suggestions":{"status":"error","message":"secret-token upstream 500"}})
}
fn google_timeout() -> Value {
    json!({"status":"ok","keyword":"施設長 求人","region":"大阪府","demand":{"status":"timeout"},"suggestions":{"status":"timeout"}})
}
fn google_not_requested() -> Value {
    json!({"status":"not_requested", "message":"検索需要を取得するには検索語を指定し、Google広告APIの取得を選択してください。"})
}

fn population_ok() -> Value {
    json!({"status":"ok","region":"大阪府",
      "bands":(0..18).map(|i| json!({"age_group":format!("{}〜{}歳", i*5, i*5+4),"male_count":1000+i*37,"female_count":1100+i*41})).collect::<Vec<_>>(),
      "minimum_wage":1064,"minimum_wage_fiscal_year":2025,"minimum_wage_effective_date":"2025-10-16",
      "minimum_wage_source_url":"https://example.invalid/minimum-wage","minimum_wage_source":"official_csv",
      "minimum_wage_as_of":"2026-01-01",
      "labor":{"fiscal_year":2024,"unemployment_rate":2.5,"separation_rate":14.1}})
}
fn population_unavailable() -> Value {
    // 本物の経路(都道府県未選択)。日付に依存しない。
    population_context(&bare_state(), "")
}

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

struct Case {
    name: &'static str,
    csv: &'static str,
    source: UserSourceHint,
    mode: WageMode,
    top_n: usize,
    title: &'static str,
    indeed: fn() -> Value,
    google: fn() -> Value,
    pop: fn() -> Value,
}

impl Case {
    fn render(&self) -> String {
        let records = parse_csv_bytes_with_hints(&fixture(self.csv), Some("大阪府"), self.source)
            .expect("fixture CSV");
        let agg = aggregate_records_with_mode(&records, self.mode);
        render_competitor_report(
            &agg,
            self.top_n,
            self.title,
            &(self.indeed)(),
            &(self.google)(),
            &(self.pop)(),
        )
    }
}

fn cases() -> Vec<Case> {
    use UserSourceHint::{Indeed, IndeedSp};
    use WageMode::{Hourly, Monthly};
    let c = |name, csv, source, mode, top_n, title, indeed, google, pop| Case {
        name,
        csv,
        source,
        mode,
        top_n,
        title,
        indeed,
        google,
        pop,
    };
    vec![
        c(
            "r1_monthly_google_ok_indeed_ok_pop_ok",
            "sp_utf8.csv",
            IndeedSp,
            Monthly,
            10,
            "施設長 / 大阪府",
            indeed_ok as fn() -> Value,
            google_ok as fn() -> Value,
            population_ok as fn() -> Value,
        ),
        c(
            "r2_hourly_google_missing_indeed_none_pop_unavailable",
            "sp_utf8.csv",
            IndeedSp,
            Hourly,
            10,
            "",
            indeed_none,
            google_missing,
            population_unavailable,
        ),
        c(
            "r3_monthly_google_error_indeed_ok_pop_ok_escaped_title",
            "sp_utf8.csv",
            IndeedSp,
            Monthly,
            5,
            "<script>alert(1)</script>&\"競合\"",
            indeed_ok,
            google_error,
            population_ok,
        ),
        c(
            "r4_hourly_google_timeout_indeed_no_series_pop_ok",
            "sp_utf8.csv",
            IndeedSp,
            Hourly,
            3,
            "介護補助",
            indeed_no_series,
            google_timeout,
            population_ok,
        ),
        c(
            "r5_monthly_google_not_requested_indeed_ok_pop_unavailable",
            "sp_utf8.csv",
            IndeedSp,
            Monthly,
            45,
            "施設長",
            indeed_ok,
            google_not_requested,
            population_unavailable,
        ),
        c(
            "r6_hourly_google_ok_indeed_ok_pop_ok",
            "sp_utf8.csv",
            IndeedSp,
            Hourly,
            20,
            "介護補助 / 大阪府",
            indeed_ok,
            google_ok,
            population_ok,
        ),
        c(
            "r7_monthly_no_sp_data_all_external_missing",
            "plain_no_sp.csv",
            Indeed,
            Monthly,
            10,
            "SPなし",
            indeed_none,
            google_not_requested,
            population_unavailable,
        ),
        c(
            "r8_hourly_no_sp_data_google_ok",
            "plain_no_sp.csv",
            Indeed,
            Hourly,
            10,
            "SPなし時給",
            indeed_ok,
            google_ok,
            population_ok,
        ),
    ]
}

// ---------------------------------------------------------------- render 層 golden

#[test]
fn render_matches_golden_for_all_cases() {
    for c in cases() {
        check_golden(&format!("{}.html", c.name), &c.render());
    }
}

/// 同じ入力を繰り返し描画して同一であること(HashMap の並び順などで golden が不安定にならない)。
#[test]
fn render_is_deterministic_across_runs() {
    for c in cases() {
        let a = c.render();
        for _ in 0..4 {
            assert!(a == c.render(), "{} が実行ごとに変わる", c.name);
        }
    }
}

/// golden の中身が空振りしていない(エラーページや空レポートを固定していない)ことを具体値で確認する。
/// 期待値は fixture(scripts/make_competitor_fixtures.py)の作り方から数えたもの。
#[test]
fn golden_inputs_exercise_the_report_not_an_error_page() {
    let cs = cases();
    let by = |n: &str| cs.iter().find(|c| c.name.starts_with(n)).unwrap().render();
    let r1 = by("r1_");
    assert!(
        r1.contains(
            "<tr><th>集計対象</th><th>該当件数</th></tr><tr><td>CSV重複排除後</td><td>60</td>"
        ),
        "重複排除後の件数 = 62 行 - 重複 2 行(月給・時給のどちらのモードでも同じ)"
    );
    // 給与表の値は fixture の式から Python で独立に計算した値(Rust の出力のコピーではない)。
    // 月給 44 件: 下限 平均 26.98 / 中央値 27.00、上限 平均 31.93 / 中央値 32.00(万円)
    assert!(
        r1.contains("<th>平均値</th><td>26.98</td><td>31.93</td>"),
        "月給の平均"
    );
    assert!(
        r1.contains("<th>中央値</th><td>27.00</td><td>32.00</td>"),
        "月給の中央値"
    );
    assert!(r1.contains("施設長 / 大阪府"));
    assert!(
        r1.contains("施設長 転職") && r1.contains("施設長 年収"),
        "Google の関連語"
    );
    assert!(
        r1.contains("1,064") || r1.contains("1064"),
        "最低賃金(固定 JSON の値)"
    );
    assert!(
        r1.contains("0〜4歳") && r1.contains("85〜89歳"),
        "人口の年齢階級"
    );
    assert!(r1.contains("3400") || r1.contains("3,400"), "Indeed の ctk");
    let r2 = by("r2_");
    assert!(r2.contains("<td>CSV重複排除後</td><td>60</td>"));
    // 時給 16 件: 下限 平均 1262.5(表示は小数 2 桁の整数丸めで 1262.00)/ 中央値 1255、上限 平均 1409.375 / 中央値 1405
    assert!(
        r2.contains("<th>平均値</th><td>1262.00</td><td>1409.00</td>"),
        "時給の平均"
    );
    assert!(
        r2.contains("<th>中央値</th><td>1255.00</td><td>1405.00</td>"),
        "時給の中央値"
    );
    assert!(r2.contains("円/時"));
    assert!(r2.contains("Indeed競合調査"), "調査名が空のときの既定名");
    assert!(
        !r2.contains("GOOGLE_ADS_DEVELOPER_TOKEN"),
        "資格情報の変数名を画面に出さない"
    );
    let r3 = by("r3_");
    assert!(
        !r3.contains("secret-token"),
        "Google のエラー文を画面に出さない"
    );
    assert!(!r3.contains("<script>alert(1)</script>"));
    assert!(r3.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
}

/// 画面用 HTML に実行時刻が入っていない(= 正規化なしで byte 比較してよい)ことの確認。
#[test]
fn no_wall_clock_in_html() {
    let today = crate::minimum_wage::japan_today();
    let local = chrono::Local::now();
    let needles = [
        today.to_string(),
        local.format("%Y年%m月%d日").to_string(),
        local.format("%Y/%m/%d").to_string(),
    ];
    for c in cases() {
        let html = c.render();
        for needle in &needles {
            assert!(
                !html.contains(needle),
                "{}: {needle} が出力に入っている。正規化が必要",
                c.name
            );
        }
    }
}

// ---------------------------------------------------------------- PDF 用 HTML

#[test]
fn pdf_document_matches_golden_and_has_fixed_layout() {
    let cs = cases();
    let html = cs[0].render();
    let tabs_js = include_str!("../../static/js/competitor-tabs.js");
    assert!(
        html.contains(tabs_js),
        "前提: 画面用 HTML にはタブ JS が入っている"
    );
    let doc = pdf::document(&html);
    assert!(
        !doc.contains(tabs_js),
        "PDF 用 HTML からタブ JS が除去されている"
    );
    assert!(
        doc.contains("@page{size:A3 landscape;margin:8mm}"),
        "LAYOUT が入っている"
    );
    assert!(doc.contains("function fitPages()"), "FIT が入っている");
    assert!(doc.contains("default-src 'none'"), "CSP が入っている");
    assert_eq!(doc.matches("<head>").count(), 1);
    check_golden("pdf_document_r1.html", &doc);
    check_golden("pdf_document_r2.html", &pdf::document(&cs[1].render()));
}

// ---------------------------------------------------------------- ハンドラ層(multipart → CSV 解読 → 集計 → 描画)

async fn post_report(csv: &[u8], fields: &[(&str, &str)]) -> (StatusCode, String) {
    let app = axum::Router::new()
        .route("/report/competitor", post(super::report))
        .with_state(bare_state());
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
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/report/competitor")
                .header("content-type", "multipart/form-data; boundary=gb")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

fn form(source: &'static str, mode: &'static str) -> Vec<(&'static str, &'static str)> {
    vec![
        ("source_type", source),
        ("wage_mode", mode),
        ("top_n", "10"),
        ("survey_title", "ハンドラ golden"),
    ]
}

#[tokio::test]
async fn handler_output_is_identical_across_four_encodings_and_matches_golden() {
    let f = form("indeed_sp", "monthly");
    let (s0, utf8) = post_report(&fixture("sp_utf8.csv"), &f).await;
    assert_eq!(s0, StatusCode::OK);
    assert!(utf8.contains("<td>CSV重複排除後</td><td>60</td>"));
    assert!(utf8.contains("<th>中央値</th><td>27.00</td><td>32.00</td>"));
    check_golden("h1_sp_utf8_monthly.html", &utf8);
    for name in ["sp_utf8_bom.csv", "sp_sjis.csv", "sp_utf16le.csv"] {
        let (status, html) = post_report(&fixture(name), &f).await;
        assert_eq!(status, StatusCode::OK, "{name}");
        assert!(
            html == utf8,
            "{name} の出力が UTF-8 版と違う(文字コード判定の差)"
        );
    }
    // 4 種が実際に別のバイト列であること(同じファイルを 4 回送っていない)
    assert_eq!(&fixture("sp_utf8_bom.csv")[..3], b"\xef\xbb\xbf");
    assert_eq!(&fixture("sp_utf16le.csv")[..2], b"\xff\xfe");
    assert!(
        std::str::from_utf8(&fixture("sp_sjis.csv")).is_err(),
        "Shift-JIS 版は UTF-8 として不正"
    );
}

#[tokio::test]
async fn handler_hourly_no_sp_and_error_pages_match_golden() {
    let (s, html) = post_report(&fixture("sp_utf8.csv"), &form("indeed_sp", "hourly")).await;
    assert_eq!(s, StatusCode::OK);
    assert!(html.contains("<td>CSV重複排除後</td><td>60</td>"));
    assert!(html.contains("<th>中央値</th><td>1255.00</td><td>1405.00</td>"));
    check_golden("h2_sp_utf8_hourly.html", &html);

    let (s, html) = post_report(&fixture("plain_no_sp.csv"), &form("indeed", "monthly")).await;
    assert_eq!(s, StatusCode::OK);
    check_golden("h3_plain_no_sp_monthly.html", &html);

    // 列欠落(給与列なし)は 200 のレポート、空ファイル・条件不正はエラーページ(400)。文面も固定する。
    let (s, html) = post_report(
        &fixture("sp_missing_salary_column.csv"),
        &form("indeed_sp", "monthly"),
    )
    .await;
    // 現行の挙動: 給与列が無くても 200 でレポートを返す(給与は空・件数だけ出る)。PR-2 はこれを変えない。
    assert_eq!(s, StatusCode::OK);
    check_golden("h4_missing_salary_column_200.html", &html);
    let (s, html) = post_report(b"", &form("indeed_sp", "monthly")).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    check_golden("h5_empty_csv_400.html", &html);
    let (s, html) = post_report(&fixture("sp_utf8.csv"), &form("other", "monthly")).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    check_golden("h6_bad_source_400.html", &html);
    let (s, html) = post_report(&fixture("sp_utf8.csv"), &form("indeed_sp", "weekly")).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    check_golden("h7_bad_wage_mode_400.html", &html);
    let mut f = form("indeed_sp", "monthly");
    f.push(("prefecture", "存在県"));
    let (s, html) = post_report(&fixture("sp_utf8.csv"), &f).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    check_golden("h8_bad_prefecture_400.html", &html);
}

/// 1 万行(合成)。レポートの大きさは行数に比例しないので golden は 1 本。
#[tokio::test]
async fn handler_ten_thousand_rows_matches_golden() {
    let mut csv = String::from("css-1hwmqh1,css-bxyec3 href,css-bxyec3,css-14qk2ra,css-18rxko3,css-18rxko3 (2),jobsearch-JobCard-tag,css-1vlebyu,css-u74ql7\n");
    for i in 0..10_000u32 {
        let lo = 18 + (i * 7) % 20;
        let pop = if i % 13 == 0 {
            "超人気"
        } else if i % 5 == 0 {
            "人気"
        } else {
            ""
        };
        csv.push_str(&format!(
            "正社員,https://example.com/{i},職種{i:05},会社{c:04},大阪府大阪市北区,月給 {lo}万円 ~ {hi}万円,賞与あり、昇給あり,年間休日{d}日,{pop}\n",
            c = i % 2500,
            hi = lo + 4 + i % 6,
            d = 100 + i % 30
        ));
    }
    let (s, html) = post_report(csv.as_bytes(), &form("indeed_sp", "monthly")).await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        html.contains("<td>CSV重複排除後</td><td>10,000</td>"),
        "1 万件"
    );
    check_golden("h9_ten_thousand_rows.html", &html);
}
