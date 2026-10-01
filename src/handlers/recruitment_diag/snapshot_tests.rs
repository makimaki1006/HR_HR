//! ゴールデン・スナップショット: 採用診断 9 API の JSON 応答を文字列で固定する
//!
//! ## 目的
//!
//! Phase 1A で 9 本のハンドラの応答を `json!()` から `#[derive(Serialize, TS)]` の struct に
//! 置き換える。置き換えの前後で JSON が 1 バイトも変わらないこと (キー順・整数/小数の別・
//! null の出し方を含む) を、置き換え前のコードで作ったゴールデンファイルとの
//! **文字列完全一致** で保証する。
//!
//! ## 使い方
//!
//! - 通常実行: `cargo test --lib recruitment_diag::snapshot_tests`
//!   → `testdata/snapshots/{panel}__{scenario}.json` と完全一致を比較する。
//! - 更新: 環境変数 `UPDATE_RD_SNAPSHOTS=1` を付けて実行したときだけファイルを書き出す。
//!   ゴールデンの更新は「応答を意図的に変えた」ときだけ行うこと。
//!
//! ## 比較方法
//!
//! - 応答本体 (`Json(x)` の `x`) を `serde_json::to_string_pretty(&x)` で直列化し、末尾に `\n` を 1 つ付ける。
//!   `to_value` を経由しないので、struct 化後は struct の直列化結果がそのまま比較される。
//! - ゴールデン読み込み時のみ `\r\n` → `\n` に正規化する (Windows の git autocrlf 対策)。
//! - 時刻・乱数など非決定値は現行 9 API の応答に含まれないため、マスク処理はしていない。
//!   Panel 7 (opportunity_map) は内部で HashMap を使うが、応答はスコア降順ソート済みで、
//!   フィクスチャはスコアが重複しないように作ってある (同点だと順序が非決定になる)。
//! - 注意: Panel 3 (inflow) の `total_population` は流入行 0 件のとき `-0.0` になる
//!   (空イテレータの `f64` `sum()` が -0.0 を返すため)。struct 化でも同じ計算を保つこと。
//!
//! ## フィクスチャ
//!
//! - base: `contract_tests::create_test_hw_db` (postings 12 件のみ。外部・人流テーブル無し)
//! - rich: base + 本ファイルの `create_rich_hw_db` で追加するローカルテーブル
//!   (Turso 無しでもローカル fallback で読まれる人流・外部統計テーブル)。

#![cfg(test)]

use super::contract_tests::{create_test_hw_db, empty_session, test_app_state, test_app_state_opt};
use super::*;
use crate::db::local_sqlite::LocalDb;
use axum::extract::{Query, State};
use serde::Serialize;
use tempfile::NamedTempFile;
use tower_sessions::Session;

// ======================================================================
// 比較ヘルパ
// ======================================================================

fn snapshot_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/handlers/recruitment_diag/testdata/snapshots")
}

/// 応答本体を pretty JSON で直列化し、ゴールデンと文字列完全一致で比較する。
fn assert_snapshot<T: Serialize>(name: &str, body: &T) {
    let mut actual = serde_json::to_string_pretty(body).expect("serialize response");
    actual.push('\n');

    let path = snapshot_dir().join(format!("{name}.json"));
    if std::env::var("UPDATE_RD_SNAPSHOTS").as_deref() == Ok("1") {
        std::fs::create_dir_all(snapshot_dir()).expect("create snapshot dir");
        std::fs::write(&path, &actual).expect("write snapshot");
        return;
    }

    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| {
            panic!(
                "snapshot {} を読めない ({e})。UPDATE_RD_SNAPSHOTS=1 で生成すること",
                path.display()
            )
        })
        .replace("\r\n", "\n");
    assert!(
        actual == expected,
        "snapshot mismatch: {name}\n--- expected ({})\n{expected}\n--- actual\n{actual}",
        path.display()
    );
}

// ======================================================================
// rich フィクスチャ
// ======================================================================

/// base フィクスチャに、Turso 無しでローカル fallback される人流・外部統計テーブルを足す。
///
/// 追加内容 (具体値はゴールデン目視確認の根拠):
/// - postings: 岩手県 飲食業 正社員 を 宮古市 3 / 花巻市 5 / 北上市 1 / 遠野市 1 件
///   → 岩手県 飲食業 正社員 は 盛岡市 10 件と合わせて 20 件、全国 飲食業 正社員 も 20 件
/// - v2_flow_mesh1km_2021 (月平均 = SUM / 月数、2 か月分):
///   - 3201 盛岡市   昼 60,000 / 夜 50,000 (昼夜比 1.2 = 均衡型、観光地補正なし)
///   - 13101 千代田区 昼 300,000 / 夜 60,000 (昼夜比 5.0 = 観光地補正あり)
///   - 3202 宮古市   昼 8,000 / 夜 10,000 (昼夜比 0.8 = ベッドタウン型)
///   - 3203 大船渡市 昼 5,000 / 夜 無し
///   - 3205 花巻市   昼 20,000 / 夜 20,000
/// - v2_flow_fromto_city (2021, 平日昼):
///   - 3201: 0=9,000 / 1=500 / 2=300 / 3=200 (同市区町村 90%)
///   - 13101: 0=4,000 / 1=2,000 / 2=2,000 / 3=2,000 (異地方 20%)
///   - 3202: 0=5,000 / 1=4,000 / 2=800 / 3=200 (どちらの閾値にも該当しない)
/// - v2_external_daytime_population (岩手県): 盛岡市 300,000 / 宮古市 2,000 / 花巻市 2,000 / 北上市 500
/// - v2_external_commute_od (着地 = 岩手県盛岡市): 13 origin + 自市区町村 1 行 + 着地違い 1 行
/// - v2_external_labor_force (岩手県): 滝沢市 1,500 / 花巻市 1,200 / 紫波町 600 / 宮古市 900 /
///   北上市 1,100 / 一関市 1,300
/// - v2_vacancy_rate / v2_transparency_score (岩手県盛岡市 正社員 1 行ずつ):
///   欠員補充率 0.25 (HS-1 閾値 0.20 以上)、平均開示率 0.30 (HS-3 閾値 0.50 未満)
fn create_rich_hw_db() -> (NamedTempFile, LocalDb) {
    let (tmp, db) = create_test_hw_db();
    let conn = rusqlite::Connection::open(tmp.path()).unwrap();
    conn.execute_batch(
        r#"
        INSERT INTO postings (job_type, prefecture, municipality, employment_type)
        VALUES
            ('飲食業', '岩手県', '宮古市', '正社員'),
            ('飲食業', '岩手県', '宮古市', '正社員'),
            ('飲食業', '岩手県', '宮古市', '正社員'),
            ('飲食業', '岩手県', '花巻市', '正社員'),
            ('飲食業', '岩手県', '花巻市', '正社員'),
            ('飲食業', '岩手県', '花巻市', '正社員'),
            ('飲食業', '岩手県', '花巻市', '正社員'),
            ('飲食業', '岩手県', '花巻市', '正社員'),
            ('飲食業', '岩手県', '北上市', '正社員'),
            ('飲食業', '岩手県', '遠野市', '正社員');

        CREATE TABLE v2_flow_mesh1km_2021 (
            mesh1kmid INTEGER,
            citycode INTEGER,
            month INTEGER,
            dayflag INTEGER,
            timezone INTEGER,
            population REAL
        );
        INSERT INTO v2_flow_mesh1km_2021 VALUES
            (1, 3201, 1, 1, 0, 40000), (2, 3201, 1, 1, 0, 20000),
            (1, 3201, 2, 1, 0, 40000), (2, 3201, 2, 1, 0, 20000),
            (1, 3201, 1, 1, 1, 50000), (1, 3201, 2, 1, 1, 50000),
            (1, 3201, 1, 0, 0, 999999), (1, 3201, 1, 2, 2, 999999),
            (3, 13101, 1, 1, 0, 300000), (3, 13101, 2, 1, 0, 300000),
            (3, 13101, 1, 1, 1, 60000), (3, 13101, 2, 1, 1, 60000),
            (4, 3202, 1, 1, 0, 8000), (4, 3202, 2, 1, 0, 8000),
            (4, 3202, 1, 1, 1, 10000), (4, 3202, 2, 1, 1, 10000),
            (5, 3203, 1, 1, 0, 5000), (5, 3203, 2, 1, 0, 5000),
            (6, 3205, 1, 1, 0, 20000), (6, 3205, 2, 1, 0, 20000),
            (6, 3205, 1, 1, 1, 20000), (6, 3205, 2, 1, 1, 20000);

        CREATE TABLE v2_flow_fromto_city (
            citycode INTEGER,
            year INTEGER,
            month INTEGER,
            dayflag INTEGER,
            timezone INTEGER,
            from_area INTEGER,
            population REAL
        );
        INSERT INTO v2_flow_fromto_city VALUES
            (3201, 2021, 1, 1, 0, 0, 9000), (3201, 2021, 1, 1, 0, 1, 500),
            (3201, 2021, 1, 1, 0, 2, 300), (3201, 2021, 1, 1, 0, 3, 200),
            (3201, 2021, 1, 1, 1, 0, 777777), (3201, 2020, 1, 1, 0, 0, 777777),
            (13101, 2021, 1, 1, 0, 0, 4000), (13101, 2021, 1, 1, 0, 1, 2000),
            (13101, 2021, 1, 1, 0, 2, 2000), (13101, 2021, 1, 1, 0, 3, 2000),
            (3202, 2021, 1, 1, 0, 0, 5000), (3202, 2021, 1, 1, 0, 1, 4000),
            (3202, 2021, 1, 1, 0, 2, 800), (3202, 2021, 1, 1, 0, 3, 200);

        CREATE TABLE v2_external_daytime_population (
            prefecture TEXT,
            municipality TEXT,
            daytime_pop REAL
        );
        INSERT INTO v2_external_daytime_population VALUES
            ('岩手県', '盛岡市', 300000),
            ('岩手県', '宮古市', 2000),
            ('岩手県', '花巻市', 2000),
            ('岩手県', '北上市', 500),
            ('東京都', '千代田区', 850000);

        CREATE TABLE v2_external_commute_od (
            origin_pref TEXT,
            origin_muni TEXT,
            dest_pref TEXT,
            dest_muni TEXT,
            total_commuters INTEGER
        );
        INSERT INTO v2_external_commute_od VALUES
            ('岩手県', '盛岡市', '岩手県', '盛岡市', 99999),
            ('岩手県', '滝沢市', '岩手県', '盛岡市', 13000),
            ('岩手県', '花巻市', '岩手県', '盛岡市', 12000),
            ('岩手県', '紫波町', '岩手県', '盛岡市', 11000),
            ('岩手県', '矢巾町', '岩手県', '盛岡市', 10000),
            ('岩手県', '宮古市', '岩手県', '盛岡市', 9000),
            ('岩手県', '北上市', '岩手県', '盛岡市', 8000),
            ('岩手県', '雫石町', '岩手県', '盛岡市', 7000),
            ('岩手県', '八幡平市', '岩手県', '盛岡市', 6000),
            ('岩手県', '一関市', '岩手県', '盛岡市', 5000),
            ('岩手県', '奥州市', '岩手県', '盛岡市', 4000),
            ('岩手県', '遠野市', '岩手県', '盛岡市', 3000),
            ('岩手県', '岩手町', '岩手県', '盛岡市', 2000),
            ('岩手県', '釜石市', '岩手県', '盛岡市', 1000),
            ('岩手県', '盛岡市', '岩手県', '宮古市', 500);

        CREATE TABLE v2_external_labor_force (
            prefecture TEXT,
            municipality TEXT,
            unemployed INTEGER
        );
        INSERT INTO v2_external_labor_force VALUES
            ('岩手県', '滝沢市', 1500),
            ('岩手県', '花巻市', 1200),
            ('岩手県', '紫波町', 600),
            ('岩手県', '宮古市', 900),
            ('岩手県', '北上市', 1100),
            ('岩手県', '一関市', 1300);

        -- Panel 8 用: insight engine の HS-1 / HS-3 を発火させる最小データ
        CREATE TABLE v2_vacancy_rate (
            prefecture TEXT, municipality TEXT, industry_raw TEXT, emp_group TEXT,
            total_count INTEGER, vacancy_count INTEGER, growth_count INTEGER,
            new_facility_count INTEGER, vacancy_rate REAL, growth_rate REAL
        );
        INSERT INTO v2_vacancy_rate VALUES
            ('岩手県', '盛岡市', '', '正社員', 100, 25, 10, 2, 0.25, 0.10);

        CREATE TABLE v2_transparency_score (
            prefecture TEXT, municipality TEXT, industry_raw TEXT, emp_group TEXT,
            total_count INTEGER, avg_transparency REAL, median_transparency REAL,
            disclosure_annual_holidays REAL, disclosure_bonus_months REAL,
            disclosure_employee_count REAL, disclosure_capital REAL, disclosure_overtime REAL,
            disclosure_female_ratio REAL, disclosure_parttime_ratio REAL,
            disclosure_founding_year REAL
        );
        INSERT INTO v2_transparency_score VALUES
            ('岩手県', '盛岡市', '', '正社員', 100, 0.30, 0.25,
             0.9, 0.6, 0.5, 0.4, 0.1, 0.2, 0.3, 0.35);
        "#,
    )
    .unwrap();
    drop(conn);
    (tmp, db)
}

async fn session_with_area(pref: &str, muni: &str) -> Session {
    let s = empty_session().await;
    s.insert(crate::auth::SESSION_PREFECTURE_KEY, pref.to_string())
        .await
        .unwrap();
    s.insert(crate::auth::SESSION_MUNICIPALITY_KEY, muni.to_string())
        .await
        .unwrap();
    s
}

// ======================================================================
// Panel 1: difficulty
// ======================================================================

fn difficulty_params(
    job_type: &str,
    emp_type: &str,
    pref: &str,
    muni: &str,
    citycode: Option<i64>,
) -> handlers::DifficultyParams {
    handlers::DifficultyParams {
        job_type: job_type.to_string(),
        emp_type: emp_type.to_string(),
        prefecture: pref.to_string(),
        municipality: muni.to_string(),
        prefcode: None,
        citycode,
    }
}

async fn run_difficulty(name: &str, rich: bool, params: handlers::DifficultyParams) {
    let (_tmp, db) = if rich {
        create_rich_hw_db()
    } else {
        create_test_hw_db()
    };
    let state = test_app_state(db);
    let resp =
        handlers::api_difficulty_score(State(state), empty_session().await, Query(params)).await;
    assert_snapshot(name, &resp.0);
}

#[tokio::test]
async fn snap_difficulty() {
    // base: 人流テーブル無し → population 0 → rank 0「人口データ不足」
    run_difficulty(
        "difficulty__base_no_flow",
        false,
        difficulty_params("飲食業", "正社員", "岩手県", "盛岡市", None),
    )
    .await;
    // 該当求人 0 件 → rank 0「データ不足」
    run_difficulty(
        "difficulty__no_matching_postings",
        false,
        difficulty_params("医療", "正社員", "岩手県", "盛岡市", None),
    )
    .await;
    // 全パラメータ空 → 全国全件 12 件、citycode null
    run_difficulty(
        "difficulty__empty_params",
        false,
        difficulty_params("", "", "", "", None),
    )
    .await;
    // 未知の雇用形態 → expand_employment_type が空 = 全雇用形態
    run_difficulty(
        "difficulty__unknown_emp_type",
        false,
        difficulty_params("飲食業", "アルバイト", "岩手県", "盛岡市", None),
    )
    .await;
    // rich: 盛岡市 10 件 / 昼 60,000 → score 1.667 rank 2「穏やか」
    run_difficulty(
        "difficulty__rich_rank2_mild",
        true,
        difficulty_params("飲食業", "正社員", "岩手県", "盛岡市", None),
    )
    .await;
    // rich: citycode 3205 (昼 20,000) → 10 件で score 5.0 rank 3「平均的」
    run_difficulty(
        "difficulty__rich_rank3_average",
        true,
        difficulty_params("飲食業", "正社員", "岩手県", "盛岡市", Some(3205)),
    )
    .await;
    // rich: citycode 3202 (昼 8,000 / 夜 10,000) → score 12.5 rank 4「激戦」
    run_difficulty(
        "difficulty__rich_rank4_heavy",
        true,
        difficulty_params("飲食業", "正社員", "岩手県", "盛岡市", Some(3202)),
    )
    .await;
    // rich: citycode 3203 (昼 5,000 / 夜 無し) → score 20.0 rank 5「超激戦」、昼夜比 0
    run_difficulty(
        "difficulty__rich_rank5_super_heavy_no_night",
        true,
        difficulty_params("飲食業", "正社員", "岩手県", "盛岡市", Some(3203)),
    )
    .await;
    // rich: 千代田区 (昼夜比 5.0) → 観光地補正、分母 60,000、2 件で score 0.333 rank 1「穴場」
    run_difficulty(
        "difficulty__rich_tourist_rank1",
        true,
        difficulty_params("製造業", "正社員", "東京都", "千代田区", None),
    )
    .await;
}

#[tokio::test]
async fn snap_difficulty_session_fallback() {
    // パラメータ空 + セッションに 岩手県/盛岡市 → セッション値で解決
    let (_tmp, db) = create_rich_hw_db();
    let state = test_app_state(db);
    let session = session_with_area("岩手県", "盛岡市").await;
    let resp = handlers::api_difficulty_score(
        State(state),
        session,
        Query(difficulty_params("飲食業", "正社員", "", "", None)),
    )
    .await;
    assert_snapshot("difficulty__session_fallback", &resp.0);
}

#[tokio::test]
async fn snap_difficulty_no_db() {
    let state = test_app_state_opt(None);
    let resp = handlers::api_difficulty_score(
        State(state),
        empty_session().await,
        Query(difficulty_params(
            "飲食業",
            "正社員",
            "岩手県",
            "盛岡市",
            None,
        )),
    )
    .await;
    assert_snapshot("difficulty__no_db", &resp.0);
}

// ======================================================================
// Panel 2: talent_pool
// ======================================================================

fn talent_pool_params(
    pref: &str,
    muni: &str,
    citycode: Option<i64>,
    year: Option<i32>,
) -> handlers::TalentPoolParams {
    handlers::TalentPoolParams {
        prefecture: pref.to_string(),
        municipality: muni.to_string(),
        citycode,
        year,
    }
}

async fn run_talent_pool(
    name: &str,
    db: Option<LocalDb>,
    session: Session,
    params: handlers::TalentPoolParams,
) {
    let state = test_app_state_opt(db);
    let resp = handlers::api_talent_pool(State(state), session, Query(params)).await;
    assert_snapshot(name, &resp.0);
}

#[tokio::test]
async fn snap_talent_pool() {
    // base: 人流テーブル無し → 0/0「未投入」
    let (_t, db) = create_test_hw_db();
    run_talent_pool(
        "talent_pool__base_no_flow",
        Some(db),
        empty_session().await,
        talent_pool_params("岩手県", "盛岡市", None, None),
    )
    .await;

    // rich 盛岡市: 昼 60,000 / 夜 50,000 → 昼夜比 1.2 均衡型
    let (_t, db) = create_rich_hw_db();
    run_talent_pool(
        "talent_pool__rich_balanced",
        Some(db),
        empty_session().await,
        talent_pool_params("岩手県", "盛岡市", None, None),
    )
    .await;

    // rich 千代田区: 昼夜比 5.0 → 流入超過型
    let (_t, db) = create_rich_hw_db();
    run_talent_pool(
        "talent_pool__rich_inflow_excess",
        Some(db),
        empty_session().await,
        talent_pool_params("東京都", "千代田区", Some(13101), None),
    )
    .await;

    // rich 宮古市: 昼夜比 0.8 → ベッドタウン型
    let (_t, db) = create_rich_hw_db();
    run_talent_pool(
        "talent_pool__rich_bedtown",
        Some(db),
        empty_session().await,
        talent_pool_params("", "", Some(3202), None),
    )
    .await;

    // rich 大船渡市: 夜間データ無し
    let (_t, db) = create_rich_hw_db();
    run_talent_pool(
        "talent_pool__rich_no_night",
        Some(db),
        empty_session().await,
        talent_pool_params("", "", Some(3203), None),
    )
    .await;

    // rich だが year=2020 (テーブル無し) → 0/0
    let (_t, db) = create_rich_hw_db();
    run_talent_pool(
        "talent_pool__year_2020_no_table",
        Some(db),
        empty_session().await,
        talent_pool_params("岩手県", "盛岡市", None, Some(2020)),
    )
    .await;

    // year=2018 (未対応年) → 0/0
    let (_t, db) = create_rich_hw_db();
    run_talent_pool(
        "talent_pool__year_2018_unsupported",
        Some(db),
        empty_session().await,
        talent_pool_params("岩手県", "盛岡市", None, Some(2018)),
    )
    .await;

    // citycode も pref+muni も無し → error
    let (_t, db) = create_test_hw_db();
    run_talent_pool(
        "talent_pool__missing_area",
        Some(db),
        empty_session().await,
        talent_pool_params("", "", None, None),
    )
    .await;

    // 名前から citycode を解決できない → error
    let (_t, db) = create_test_hw_db();
    run_talent_pool(
        "talent_pool__unknown_municipality",
        Some(db),
        empty_session().await,
        talent_pool_params("岩手県", "存在しない市", None, None),
    )
    .await;

    // DB 未接続 (citycode 解決後に判定される)
    run_talent_pool(
        "talent_pool__no_db",
        None,
        empty_session().await,
        talent_pool_params("岩手県", "盛岡市", None, None),
    )
    .await;

    // セッション fallback (rich)
    let (_t, db) = create_rich_hw_db();
    run_talent_pool(
        "talent_pool__session_fallback",
        Some(db),
        session_with_area("岩手県", "盛岡市").await,
        talent_pool_params("", "", None, None),
    )
    .await;
}

// ======================================================================
// Panel 3: inflow
// ======================================================================

fn inflow_params(pref: &str, muni: &str, citycode: Option<i64>) -> handlers::InflowParams {
    handlers::InflowParams {
        prefecture: pref.to_string(),
        municipality: muni.to_string(),
        citycode,
        year: None,
    }
}

async fn run_inflow(name: &str, db: Option<LocalDb>, params: handlers::InflowParams) {
    let state = test_app_state_opt(db);
    let resp =
        handlers::api_inflow_analysis(State(state), empty_session().await, Query(params)).await;
    assert_snapshot(name, &resp.0);
}

#[tokio::test]
async fn snap_inflow() {
    // base: fromto テーブル無し → breakdown [] / total 0
    let (_t, db) = create_test_hw_db();
    run_inflow(
        "inflow__base_no_table",
        Some(db),
        inflow_params("岩手県", "盛岡市", Some(3201)),
    )
    .await;

    // rich 盛岡市: 同市区町村 90% → 地域限定
    let (_t, db) = create_rich_hw_db();
    run_inflow(
        "inflow__rich_local_dominant",
        Some(db),
        inflow_params("岩手県", "盛岡市", None),
    )
    .await;

    // rich 千代田区: 異地方 20% → 広域
    let (_t, db) = create_rich_hw_db();
    run_inflow(
        "inflow__rich_wide_area",
        Some(db),
        inflow_params("東京都", "千代田区", None),
    )
    .await;

    // rich 宮古市: 同市 50% / 同県別市 40% → 通勤圏内
    let (_t, db) = create_rich_hw_db();
    run_inflow(
        "inflow__rich_commute_zone",
        Some(db),
        inflow_params("", "", Some(3202)),
    )
    .await;

    // rich だがデータの無い citycode → breakdown [] / total 0
    let (_t, db) = create_rich_hw_db();
    run_inflow(
        "inflow__rich_no_rows_for_city",
        Some(db),
        inflow_params("", "", Some(3205)),
    )
    .await;

    // citycode 無し → error
    let (_t, db) = create_test_hw_db();
    run_inflow(
        "inflow__missing_area",
        Some(db),
        inflow_params("", "", None),
    )
    .await;

    // DB 未接続
    run_inflow(
        "inflow__no_db",
        None,
        inflow_params("岩手県", "盛岡市", Some(3201)),
    )
    .await;
}

// ======================================================================
// Panel 4: competitors (Turso SalesNow 必須。Turso 無しでは未接続エラーのみ到達可能)
// ======================================================================

#[tokio::test]
async fn snap_competitors() {
    let (_t, db) = create_test_hw_db();
    let state = test_app_state(db);
    let q = competitors::CompetitorsQuery {
        job_type: "飲食業".to_string(),
        prefcode: Some(3),
        municipality: "盛岡市".to_string(),
        limit: Some(100),
    };
    let resp = competitors::competitors(State(state), Query(q)).await;
    assert_snapshot("competitors__no_salesnow", &resp.0);
}

// ======================================================================
// Panel 5: condition_gap
// ======================================================================

fn gap_query(
    job_type: &str,
    emp_type: &str,
    prefcode: Option<i32>,
    muni: &str,
    company: Option<(f64, f64, f64)>,
) -> condition_gap::ConditionGapQuery {
    condition_gap::ConditionGapQuery {
        job_type: job_type.to_string(),
        emp_type: emp_type.to_string(),
        prefcode,
        municipality: muni.to_string(),
        company_salary_min: company.map(|c| c.0),
        company_bonus_months: company.map(|c| c.1),
        company_annual_holidays: company.map(|c| c.2),
    }
}

async fn run_gap(name: &str, db: Option<LocalDb>, q: condition_gap::ConditionGapQuery) {
    let state = test_app_state_opt(db);
    let resp = condition_gap::condition_gap(State(state), Query(q)).await;
    assert_snapshot(name, &resp.0);
}

#[tokio::test]
async fn snap_condition_gap() {
    // 自社 22 万 / 賞与 3.0 / 年休 115 → 業界中央値より上回る
    let (_t, db) = create_test_hw_db();
    run_gap(
        "condition_gap__success_above",
        Some(db),
        gap_query(
            "飲食業",
            "正社員",
            Some(3),
            "盛岡市",
            Some((220_000.0, 3.0, 115.0)),
        ),
    )
    .await;

    // 自社 18 万 / 賞与 1.0 / 年休 100 → 下回る
    let (_t, db) = create_test_hw_db();
    run_gap(
        "condition_gap__success_below",
        Some(db),
        gap_query(
            "飲食業",
            "正社員",
            Some(3),
            "盛岡市",
            Some((180_000.0, 1.0, 100.0)),
        ),
    )
    .await;

    // 自社条件 = 業界中央値ちょうど (月給 210,000 / 賞与 2.5 / 年休 110)
    let (_t, db) = create_test_hw_db();
    run_gap(
        "condition_gap__success_equal",
        Some(db),
        gap_query(
            "飲食業",
            "正社員",
            Some(3),
            "盛岡市",
            Some((210_000.0, 2.5, 110.0)),
        ),
    )
    .await;

    // 自社条件未入力 → 自社年収 0
    let (_t, db) = create_test_hw_db();
    run_gap(
        "condition_gap__no_company_input",
        Some(db),
        gap_query("飲食業", "正社員", Some(3), "盛岡市", None),
    )
    .await;

    // prefcode 無し + 市区町村無し → 全国 (全業界は東京 2 件を含む 12 件)
    let (_t, db) = create_test_hw_db();
    run_gap(
        "condition_gap__national",
        Some(db),
        gap_query("飲食業", "", None, "", Some((220_000.0, 3.0, 115.0))),
    )
    .await;

    // prefcode 範囲外 (99) → prefecture "" 扱い
    let (_t, db) = create_test_hw_db();
    run_gap(
        "condition_gap__prefcode_out_of_range",
        Some(db),
        gap_query(
            "飲食業",
            "正社員",
            Some(99),
            "",
            Some((220_000.0, 3.0, 115.0)),
        ),
    )
    .await;

    // パート → DB 値に展開されるがフィクスチャに該当無し → sample 0
    let (_t, db) = create_test_hw_db();
    run_gap(
        "condition_gap__part_no_samples",
        Some(db),
        gap_query(
            "飲食業",
            "パート",
            Some(3),
            "盛岡市",
            Some((220_000.0, 3.0, 115.0)),
        ),
    )
    .await;

    // 未知の雇用形態はそのまま一致検索 → sample 0
    let (_t, db) = create_test_hw_db();
    run_gap(
        "condition_gap__unknown_emp_type",
        Some(db),
        gap_query(
            "飲食業",
            "アルバイト",
            Some(3),
            "盛岡市",
            Some((220_000.0, 3.0, 115.0)),
        ),
    )
    .await;

    // DB 未接続
    run_gap(
        "condition_gap__no_db",
        None,
        gap_query(
            "飲食業",
            "正社員",
            Some(3),
            "盛岡市",
            Some((220_000.0, 3.0, 115.0)),
        ),
    )
    .await;
}

// ======================================================================
// Panel 6: market_trend (Turso 必須。Turso 無しでは未接続エラーのみ到達可能)
// ======================================================================

#[tokio::test]
async fn snap_market_trend() {
    let (_t, db) = create_test_hw_db();
    let state = test_app_state(db);
    let q = market_trend::MarketTrendQuery {
        job_type: "飲食業".to_string(),
        emp_type: "正社員".to_string(),
        prefcode: Some(3),
        months: None,
    };
    let resp = market_trend::market_trend(State(state), Query(q)).await;
    assert_snapshot("market_trend__no_turso", &resp.0);
}

// ======================================================================
// Panel 7: opportunity_map
// ======================================================================

fn opp_params(
    prefcode: i32,
    job_type: Option<&str>,
    emp_type: Option<&str>,
) -> opportunity_map::OpportunityMapParams {
    opportunity_map::OpportunityMapParams {
        prefcode,
        job_type: job_type.map(str::to_string),
        emp_type: emp_type.map(str::to_string),
    }
}

async fn run_opp(name: &str, db: Option<LocalDb>, p: opportunity_map::OpportunityMapParams) {
    let state = test_app_state_opt(db);
    let resp =
        opportunity_map::opportunity_map(State(state), empty_session().await, Query(p)).await;
    assert_snapshot(name, &resp.0);
}

#[tokio::test]
async fn snap_opportunity_map() {
    // base: 昼間人口テーブル無し → municipalities []
    let (_t, db) = create_test_hw_db();
    run_opp(
        "opportunity_map__base_no_pop_table",
        Some(db),
        opp_params(3, Some("飲食業"), Some("正社員")),
    )
    .await;

    // rich: 花巻市 2.5 激戦 / 宮古市 1.5 標準 / 盛岡市 0.033 穴場。北上市 (人口 500) と遠野市 (人口無し) は除外
    let (_t, db) = create_rich_hw_db();
    run_opp(
        "opportunity_map__rich_three_categories",
        Some(db),
        opp_params(3, Some("飲食業"), Some("正社員")),
    )
    .await;

    // rich: フィルタ無し (job_type/emp_type None)
    let (_t, db) = create_rich_hw_db();
    run_opp(
        "opportunity_map__rich_no_filters",
        Some(db),
        opp_params(3, None, None),
    )
    .await;

    // rich: 空文字フィルタ (Some("")) → フィルタ無しと同じ集計だが filters に "" が出る
    let (_t, db) = create_rich_hw_db();
    run_opp(
        "opportunity_map__rich_empty_string_filters",
        Some(db),
        opp_params(3, Some(""), Some("")),
    )
    .await;

    // rich: 雇用形態「パート」は展開されず完全一致 → 0 件 → []
    let (_t, db) = create_rich_hw_db();
    run_opp(
        "opportunity_map__rich_part_unexpanded",
        Some(db),
        opp_params(3, Some("飲食業"), Some("パート")),
    )
    .await;

    // rich: 求人の無い都道府県 (沖縄県) → []
    let (_t, db) = create_rich_hw_db();
    run_opp(
        "opportunity_map__rich_no_postings_pref",
        Some(db),
        opp_params(47, Some("飲食業"), Some("正社員")),
    )
    .await;

    // prefcode 範囲外
    let (_t, db) = create_test_hw_db();
    run_opp(
        "opportunity_map__invalid_prefcode_0",
        Some(db),
        opp_params(0, Some("飲食業"), Some("正社員")),
    )
    .await;
    let (_t, db) = create_test_hw_db();
    run_opp(
        "opportunity_map__invalid_prefcode_48",
        Some(db),
        opp_params(48, None, None),
    )
    .await;

    // DB 未接続 (prefcode 検証より先に判定される)
    run_opp(
        "opportunity_map__no_db",
        None,
        opp_params(0, Some("飲食業"), Some("正社員")),
    )
    .await;
}

// ======================================================================
// Panel 8: insights
// ======================================================================

fn insights_params(
    prefcode: i32,
    citycode: Option<u32>,
    job_type: Option<&str>,
    emp_type: Option<&str>,
) -> insights::InsightsParams {
    insights::InsightsParams {
        prefcode,
        citycode,
        job_type: job_type.map(str::to_string),
        emp_type: emp_type.map(str::to_string),
    }
}

async fn run_insights(name: &str, db: Option<LocalDb>, p: insights::InsightsParams) {
    let state = test_app_state_opt(db);
    let resp = insights::insights(State(state), empty_session().await, Query(p)).await;
    assert_snapshot(name, &resp.0);
}

#[tokio::test]
async fn snap_insights() {
    // base: 岩手県 盛岡市
    let (_t, db) = create_test_hw_db();
    run_insights(
        "insights__base_city",
        Some(db),
        insights_params(3, Some(3201), Some("飲食業"), Some("正社員")),
    )
    .await;

    // base: 県全体 (citycode 無し、job/emp も None)
    let (_t, db) = create_test_hw_db();
    run_insights(
        "insights__base_pref_only",
        Some(db),
        insights_params(3, None, None, None),
    )
    .await;

    // citycode が別の都道府県 (東京都 + 盛岡市 3201) → municipality ""
    let (_t, db) = create_test_hw_db();
    run_insights(
        "insights__citycode_other_pref",
        Some(db),
        insights_params(13, Some(3201), Some("製造業"), Some("正社員")),
    )
    .await;

    // rich: 岩手県 盛岡市 (人流・外部テーブルあり)
    let (_t, db) = create_rich_hw_db();
    run_insights(
        "insights__rich_city",
        Some(db),
        insights_params(3, Some(3201), Some("飲食業"), Some("正社員")),
    )
    .await;

    // prefcode 範囲外
    let (_t, db) = create_test_hw_db();
    run_insights(
        "insights__invalid_prefcode",
        Some(db),
        insights_params(99, None, None, None),
    )
    .await;

    // DB 未接続
    run_insights(
        "insights__no_db",
        None,
        insights_params(3, Some(3201), Some("飲食業"), Some("正社員")),
    )
    .await;
}

// ======================================================================
// Panel 9: talent_pool_expansion
// ======================================================================

fn tpe_params(pref: &str, muni: &str) -> talent_pool_expansion::TalentPoolExpansionParams {
    talent_pool_expansion::TalentPoolExpansionParams {
        prefecture: pref.to_string(),
        municipality: muni.to_string(),
        citycode: None,
    }
}

async fn run_tpe(
    name: &str,
    db: Option<LocalDb>,
    session: Session,
    p: talent_pool_expansion::TalentPoolExpansionParams,
) {
    let state = test_app_state_opt(db);
    let resp =
        talent_pool_expansion::api_talent_pool_expansion(State(state), session, Query(p)).await;
    assert_snapshot(name, &resp.0);
}

#[tokio::test]
async fn snap_talent_pool_expansion() {
    // base: OD テーブル無し → is_data_available false、両 tier 空
    let (_t, db) = create_test_hw_db();
    run_tpe(
        "talent_pool_expansion__base_no_od_table",
        Some(db),
        empty_session().await,
        tpe_params("岩手県", "盛岡市"),
    )
    .await;

    // rich: 13 origin のうち上位 12 件 → 30 分圏 5 / 60 分圏 7
    //   30 分圏: 失業者 1500+1200+600+0+900 = 4200、HW 花巻 5 + 宮古 3 = 8
    //   60 分圏: 失業者 北上 1100 + 一関 1300 = 2400、HW 北上 1 + 遠野 1 = 2
    let (_t, db) = create_rich_hw_db();
    run_tpe(
        "talent_pool_expansion__rich_two_tiers",
        Some(db),
        empty_session().await,
        tpe_params("岩手県", "盛岡市"),
    )
    .await;

    // rich: 着地 宮古市 → origin 盛岡市 1 件のみ (30 分圏 1 / 60 分圏 0)
    let (_t, db) = create_rich_hw_db();
    run_tpe(
        "talent_pool_expansion__rich_single_origin",
        Some(db),
        empty_session().await,
        tpe_params("岩手県", "宮古市"),
    )
    .await;

    // rich: OD 行の無い着地 → is_data_available false
    let (_t, db) = create_rich_hw_db();
    run_tpe(
        "talent_pool_expansion__rich_no_od_rows",
        Some(db),
        empty_session().await,
        tpe_params("岩手県", "遠野市"),
    )
    .await;

    // セッション fallback (rich)
    let (_t, db) = create_rich_hw_db();
    run_tpe(
        "talent_pool_expansion__session_fallback",
        Some(db),
        session_with_area("岩手県", "盛岡市").await,
        tpe_params("", ""),
    )
    .await;

    // 市区町村欠落 → error
    let (_t, db) = create_test_hw_db();
    run_tpe(
        "talent_pool_expansion__missing_municipality",
        Some(db),
        empty_session().await,
        tpe_params("岩手県", ""),
    )
    .await;

    // DB 未接続
    run_tpe(
        "talent_pool_expansion__no_db",
        None,
        empty_session().await,
        tpe_params("岩手県", "盛岡市"),
    )
    .await;
}
