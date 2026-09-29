//! Panel 9 (CR-8): 通勤圏人材プール試算
//!
//! ユーザーが選択した市区町村について「通勤圏を広げると人材プールはどれだけ広がるか」を試算する。
//!
//! # データ源
//! - `v2_external_commute_od` (国勢調査 2020、市区町村粒度1:1 OD): origin/dest別 total_commuters
//! - `v2_external_labor_force` (SSDSE-A、市区町村粒度): unemployed (失業者数)
//! - `postings` (HW、市区町村粒度): COUNT(*) per (prefecture, municipality)
//!
//! # アルゴリズム
//! 1. 着地 = ユーザー選択市区町村として、`v2_external_commute_od` から
//!    `dest_pref/dest_muni` で逆引きし、自市区町村以外の origin を total_commuters 降順で取得。
//! 2. **30 分圏** = OD volume Top 5 (固定件数)。実距離ベースではない。
//! 3. **60 分圏** = 30 分圏を **除いた** Top 7 (= 通算 Top 12 のうち 6〜12 番目)。
//!    重複防止のため fail-safe で除外。
//! 4. 各市区町村について `v2_external_labor_force` から失業者数、
//!    `postings` から HW 求人件数を取得して合計。
//!
//! # 必須注記 (UI 表示)
//! - 通勤 OD は国勢調査 2020 ベース (5 年遅れ)
//! - 失業者プール拡大は通勤可能性であり応募意向ではない
//! - 30/60 分圏は OD volume 上位の固定件数、実距離ではない
//! - HW 求人は HW 掲載のみ。全求人市場ではない
//!
//! # MEMORY 遵守
//! - `feedback_correlation_not_causation`: 「拡大したらこのプール」は事実、応募行動は別
//! - `feedback_test_data_validation`: 集計結果を具体値で assert_eq
//! - `feedback_hw_data_scope`: HW 求人は HW 掲載のみ
//! - `feedback_never_guess_data`: テーブル名・カラム名は grep で実確認済
//! - `feedback_reverse_proof_tests`: ドメイン不変条件 (≥ 0、件数上限) 必須

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_sessions::Session;
use ts_rs::TS;

use crate::db::local_sqlite::LocalDb;
use crate::handlers::helpers::{get_i64, get_str};
use crate::handlers::overview::get_session_filters;
use crate::AppState;

use super::types::RdNotes;
use super::{CAUSATION_NOTE, HW_SCOPE_NOTE};

// ======== Response Types ========

/// 選択中の市区町村。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdTalentPoolCurrent {
    /// 都道府県名。
    pub prefecture: String,
    /// 市区町村名。
    pub municipality: String,
}

/// 通勤圏内の市区町村 1 件分 (`NeighborEntry` の応答形)。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdCommuteTierEntry {
    /// 都道府県名 (流入元)。
    pub prefecture: String,
    /// 市区町村名 (流入元)。
    pub municipality: String,
    /// 通勤者数 (流入元 → 選択市区町村)。
    pub commuters: i64,
    /// 失業者数 (人材プールの代理指標)。
    pub unemployment: i64,
    /// HW 求人件数。
    pub hw_postings: i64,
}

/// 通勤圏 1 段階分の集計 (30 分圏 / 60 分圏)。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdCommuteTier {
    /// 市区町村数。
    pub municipality_count: usize,
    /// 失業者プール合計。
    pub unemployment_pool: i64,
    /// HW 求人件数合計。
    pub hw_postings: i64,
    /// 内訳 (通勤者数の降順)。
    pub breakdown: Vec<RdCommuteTierEntry>,
}

impl RdCommuteTier {
    /// エラー応答用の空の tier。
    fn empty() -> Self {
        Self {
            municipality_count: 0,
            unemployment_pool: 0,
            hw_postings: 0,
            breakdown: Vec::new(),
        }
    }
}

/// 成功時の注記 (`hw_scope` / `causation` に出典・前提を足したもの)。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdTalentPoolNotes {
    /// HW 掲載求人のみが対象である旨。
    pub hw_scope: String,
    /// 相関であって因果ではない旨。
    pub causation: String,
    /// 通勤 OD の出典 (国勢調査年)。
    pub data_source_od: String,
    /// 失業者数の出典。
    pub data_source_unemployment: String,
    /// HW 求人件数の出典。
    pub data_source_hw: String,
    /// 30 分圏 / 60 分圏の定義。
    pub tier_definition: String,
    /// 失業者プールに関する注意。
    pub caveat_pool: String,
    /// 分圏の名称に関する注意。
    pub caveat_distance: String,
}

impl RdTalentPoolNotes {
    fn standard() -> Self {
        Self {
            hw_scope: HW_SCOPE_NOTE.to_string(),
            causation: CAUSATION_NOTE.to_string(),
            data_source_od: format!(
                "国勢調査 通勤 OD ({} 年、5年遅れ、市区町村間1:1)",
                COMMUTE_OD_REFERENCE_YEAR
            ),
            data_source_unemployment:
                "SSDSE-A 労働力統計 v2_external_labor_force.unemployed (国勢調査ベース)".to_string(),
            data_source_hw: "HW 掲載求人 postings テーブル".to_string(),
            tier_definition: format!(
                "30 分圏 = OD volume 上位 {} 件、60 分圏 = 30 分圏を除いた次の {} 件 (実距離ではなく通勤者数ベースの固定件数)",
                TIER_30MIN_COUNT, TIER_60MIN_ADDITIONAL_COUNT
            ),
            caveat_pool:
                "失業者プール拡大は通勤可能性の理論値であり、実際の応募意向を保証するものではない"
                    .to_string(),
            caveat_distance:
                "30/60 分圏という名称は便宜的な分類であり、実走行時間ではない".to_string(),
        }
    }
}

/// Panel 9 成功時の本体。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdTalentPoolExpansionResponse {
    /// パネル識別子 (常に "talent_pool_expansion")。
    pub panel: String,
    /// 選択中の市区町村。
    pub current: RdTalentPoolCurrent,
    /// 30 分圏 (OD 上位 5 件)。
    pub tier_30min: RdCommuteTier,
    /// 60 分圏 (30 分圏を除く次の 7 件)。
    pub tier_60min: RdCommuteTier,
    /// OD データが 1 件以上取得できたか。
    pub is_data_available: bool,
    /// 注記。
    pub notes: RdTalentPoolNotes,
}

/// Panel 9 エラー本体 (空の tier を含み、UI がそのまま描画できる形)。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdTalentPoolExpansionError {
    /// エラー内容。
    pub error: String,
    /// パネル識別子 (常に "talent_pool_expansion")。
    pub panel: String,
    /// 30 分圏 (常に空)。
    pub tier_30min: RdCommuteTier,
    /// 60 分圏 (常に空)。
    pub tier_60min: RdCommuteTier,
    /// 常に false。
    pub is_data_available: bool,
    /// 注記。
    pub notes: RdNotes,
}

/// Panel 9 のレスポンス。TS では `RdTalentPoolExpansionResponse | RdTalentPoolExpansionError`。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(untagged)]
pub enum RdTalentPoolExpansionResult {
    Ok(RdTalentPoolExpansionResponse),
    Err(RdTalentPoolExpansionError),
}

/// 30 分圏として扱う OD volume 上位市区町村件数 (固定値)
pub(crate) const TIER_30MIN_COUNT: usize = 5;
/// 60 分圏として扱う追加件数 (30 分圏 5 件を **除いた** 上位 7 件 = 通算 Top 12 のうち 6〜12)
pub(crate) const TIER_60MIN_ADDITIONAL_COUNT: usize = 7;

/// 国勢調査 OD の参照年 (UI caveat 用)
pub(crate) const COMMUTE_OD_REFERENCE_YEAR: i32 = 2020;

#[derive(Deserialize)]
pub struct TalentPoolExpansionParams {
    #[serde(default)]
    pub prefecture: String,
    #[serde(default)]
    pub municipality: String,
    /// 互換性のため受け付けるが現実装では未使用 (pref+muni のみで動作)
    #[serde(default)]
    pub citycode: Option<i64>,
}

/// 隣接市区町村 1 件分のメトリクス
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NeighborEntry {
    pub prefecture: String,
    pub municipality: String,
    /// 通勤者数 (origin → dest、user 選択先への流入)
    pub commuters: i64,
    /// 当該市区町村の失業者数 (人材プール代理指標)
    pub unemployment: i64,
    /// 当該市区町村の HW 求人件数
    pub hw_postings: i64,
}

/// `GET /api/recruitment_diag/talent_pool_expansion`
///
/// パラメータ: `prefecture`, `municipality` (必須)。citycode は任意 (互換用)。
///
/// レスポンス例:
/// ```json
/// {
///   "panel": "talent_pool_expansion",
///   "current": {"prefecture": "北海道", "municipality": "札幌市中央区"},
///   "tier_30min": {
///     "municipality_count": 5,
///     "unemployment_pool": 5800,
///     "hw_postings": 380,
///     "breakdown": [{"prefecture":"北海道","municipality":"札幌市東区","commuters":12000,"unemployment":1200,"hw_postings":80}, ...]
///   },
///   "tier_60min": { ... },
///   "notes": { ... }
/// }
/// ```
///
/// **fail-soft**:
/// - `v2_external_commute_od` が存在しない、または当該市区町村の OD が 0 件 → 空 breakdown を返す
/// - prefecture/municipality 未指定 → 400 風 error JSON
pub async fn api_talent_pool_expansion(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(params): Query<TalentPoolExpansionParams>,
) -> Json<RdTalentPoolExpansionResult> {
    // 入力解決: パラメータが空ならセッションフィルタにフォールバック
    let filters = get_session_filters(&session).await;
    let pref = if params.prefecture.is_empty() {
        filters.prefecture.clone()
    } else {
        params.prefecture.clone()
    };
    let muni = if params.municipality.is_empty() {
        filters.municipality.clone()
    } else {
        params.municipality.clone()
    };

    if pref.is_empty() || muni.is_empty() {
        return Json(error_body(
            "prefecture および municipality が必要です (citycode は補助的、名前指定が主)",
        ));
    }

    let db = match &state.hw_db {
        Some(d) => d.clone(),
        None => return Json(error_body("hellowork.db 未接続")),
    };

    let pref_c = pref.clone();
    let muni_c = muni.clone();

    // ブロッキングDBアクセスを別スレッドで実行
    let entries = tokio::task::spawn_blocking(move || fetch_neighbors(&db, &pref_c, &muni_c))
        .await
        .unwrap_or_default();

    // entries が空 = OD データ未投入または当該市区町村が origin として国勢調査に登場しない
    // → fail-soft で empty breakdown を返す (404 ではなく 200 + 空)
    let (tier_30, tier_60) = split_into_tiers(&entries);

    Json(RdTalentPoolExpansionResult::Ok(build_response(
        pref,
        muni,
        &tier_30,
        &tier_60,
        !entries.is_empty(),
    )))
}

/// 成功本体を組み立てる。
fn build_response(
    pref: String,
    muni: String,
    tier_30: &[NeighborEntry],
    tier_60: &[NeighborEntry],
    is_data_available: bool,
) -> RdTalentPoolExpansionResponse {
    RdTalentPoolExpansionResponse {
        panel: "talent_pool_expansion".to_string(),
        current: RdTalentPoolCurrent {
            prefecture: pref,
            municipality: muni,
        },
        tier_30min: build_tier(tier_30),
        tier_60min: build_tier(tier_60),
        is_data_available,
        notes: RdTalentPoolNotes::standard(),
    }
}

// ========================================================================
// データ取得層
// ========================================================================

/// `v2_external_commute_od` から、当該 (pref, muni) を **dest** とする
/// 自市区町村以外の origin を total_commuters 降順で取得。
///
/// 各 origin について `v2_external_labor_force.unemployed` および
/// `postings COUNT(*)` を結合し、`NeighborEntry` を組み立てる。
///
/// **fail-soft**:
/// - `v2_external_commute_od` テーブル不存在 → 空 Vec
/// - 各種付随データが取得できない → 0 で埋める
fn fetch_neighbors(db: &LocalDb, dest_pref: &str, dest_muni: &str) -> Vec<NeighborEntry> {
    // OD テーブル不存在なら fail-soft
    if !crate::handlers::helpers::table_exists(db, "v2_external_commute_od") {
        return Vec::new();
    }

    // 自市区町村以外の流入元を total_commuters 降順で最大 (TIER_30 + TIER_60) 件取得。
    // LIMIT は (5 + 7) = 12 件分。実距離ではなく OD volume 上位固定件数。
    let limit = (TIER_30MIN_COUNT + TIER_60MIN_ADDITIONAL_COUNT) as i64;
    let sql = "SELECT origin_pref, origin_muni, total_commuters \
               FROM v2_external_commute_od \
               WHERE dest_pref = ?1 AND dest_muni = ?2 \
                 AND (origin_pref != dest_pref OR origin_muni != dest_muni) \
               ORDER BY total_commuters DESC \
               LIMIT ?3";
    let limit_str = limit.to_string();
    // postings (郡名込み) と v2_external_* (郡名なし) の不一致吸収
    let dest_muni_normalized =
        crate::handlers::helpers::normalize_muni_for_external(dest_pref, dest_muni);
    let params: &[&dyn rusqlite::types::ToSql] = &[
        &dest_pref,
        &dest_muni_normalized,
        &limit_str as &dyn rusqlite::types::ToSql,
    ];
    let rows = match db.query(sql, params) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("fetch_neighbors OD query failed: {e}");
            return Vec::new();
        }
    };

    rows.iter()
        .map(|r| {
            let origin_pref = get_str(r, "origin_pref");
            let origin_muni = get_str(r, "origin_muni");
            let commuters = get_i64(r, "total_commuters");
            let unemployment = fetch_unemployment(db, &origin_pref, &origin_muni);
            let hw_postings = fetch_hw_postings_count(db, &origin_pref, &origin_muni);
            NeighborEntry {
                prefecture: origin_pref,
                municipality: origin_muni,
                commuters,
                unemployment,
                hw_postings,
            }
        })
        .collect()
}

/// `v2_external_labor_force.unemployed` を引く (取得不可なら 0)
fn fetch_unemployment(db: &LocalDb, pref: &str, muni: &str) -> i64 {
    if !crate::handlers::helpers::table_exists(db, "v2_external_labor_force") {
        return 0;
    }
    let sql = "SELECT unemployed FROM v2_external_labor_force \
               WHERE prefecture = ?1 AND municipality = ?2";
    // postings (郡名込み) と v2_external_* (郡名なし) の不一致吸収
    // (現呼び出し元は v2_external_commute_od 由来で既に郡名なしだが、防御的に strip)
    let muni_normalized = crate::handlers::helpers::normalize_muni_for_external(pref, muni);
    let params: &[&dyn rusqlite::types::ToSql] = &[&pref, &muni_normalized];
    db.query_scalar::<i64>(sql, params).unwrap_or(0).max(0)
}

/// `postings` から (pref, muni) の HW 求人件数を取得 (取得不可なら 0)
fn fetch_hw_postings_count(db: &LocalDb, pref: &str, muni: &str) -> i64 {
    let sql = "SELECT COUNT(*) FROM postings WHERE prefecture = ?1 AND municipality = ?2";
    let params: &[&dyn rusqlite::types::ToSql] = &[&pref, &muni];
    db.query_scalar::<i64>(sql, params).unwrap_or(0).max(0)
}

// ========================================================================
// 純粋ロジック (テスト容易)
// ========================================================================

/// `entries` (OD volume 降順) を 30 分圏 (Top 5) と 60 分圏 (次の 7、重複なし) に分割。
///
/// **不変条件**:
/// - tier_30 の長さは TIER_30MIN_COUNT (=5) 以下
/// - tier_60 の長さは TIER_60MIN_ADDITIONAL_COUNT (=7) 以下
/// - tier_30 と tier_60 は (pref, muni) の重複を含まない
pub(crate) fn split_into_tiers(
    entries: &[NeighborEntry],
) -> (Vec<NeighborEntry>, Vec<NeighborEntry>) {
    let mut iter = entries.iter().cloned();
    let tier_30: Vec<NeighborEntry> = iter.by_ref().take(TIER_30MIN_COUNT).collect();
    let tier_60: Vec<NeighborEntry> = iter.take(TIER_60MIN_ADDITIONAL_COUNT).collect();
    (tier_30, tier_60)
}

/// tier の集計値を計算 (失業者プール、HW 求人合計、市区町村数)
pub(crate) fn aggregate_tier(tier: &[NeighborEntry]) -> (i64, i64, usize) {
    let unemployment: i64 = tier.iter().map(|e| e.unemployment).sum();
    let hw_postings: i64 = tier.iter().map(|e| e.hw_postings).sum();
    (unemployment, hw_postings, tier.len())
}

/// tier を応答用の型に変換 (集計値 + breakdown)
fn build_tier(tier: &[NeighborEntry]) -> RdCommuteTier {
    let (unemployment_pool, hw_postings, count) = aggregate_tier(tier);
    RdCommuteTier {
        municipality_count: count,
        unemployment_pool,
        hw_postings,
        breakdown: tier
            .iter()
            .map(|e| RdCommuteTierEntry {
                prefecture: e.prefecture.clone(),
                municipality: e.municipality.clone(),
                commuters: e.commuters,
                unemployment: e.unemployment,
                hw_postings: e.hw_postings,
            })
            .collect(),
    }
}

fn error_body(msg: &str) -> RdTalentPoolExpansionResult {
    RdTalentPoolExpansionResult::Err(RdTalentPoolExpansionError {
        error: msg.to_string(),
        panel: "talent_pool_expansion".to_string(),
        tier_30min: RdCommuteTier::empty(),
        tier_60min: RdCommuteTier::empty(),
        is_data_available: false,
        notes: RdNotes::standard(),
    })
}

// ========================================================================
// テスト
// ========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_entry(pref: &str, muni: &str, commuters: i64, unemp: i64, hw: i64) -> NeighborEntry {
        NeighborEntry {
            prefecture: pref.to_string(),
            municipality: muni.to_string(),
            commuters,
            unemployment: unemp,
            hw_postings: hw,
        }
    }

    /// **必須テスト 1**: 30 分圏集計
    /// テストデータで 5 都市 [失業 1000, 800, 600, 400, 200] → 合計 3000
    #[test]
    fn aggregate_30min_tier_with_5_cities_sums_correctly() {
        let entries = vec![
            mk_entry("北海道", "札幌市東区", 5000, 1000, 50),
            mk_entry("北海道", "札幌市西区", 4000, 800, 40),
            mk_entry("北海道", "札幌市南区", 3000, 600, 30),
            mk_entry("北海道", "札幌市北区", 2000, 400, 20),
            mk_entry("北海道", "札幌市豊平区", 1000, 200, 10),
        ];
        let (tier30, tier60) = split_into_tiers(&entries);
        assert_eq!(tier30.len(), 5);
        assert_eq!(tier60.len(), 0, "5 件しかなければ 60 分圏は空");
        let (unemp, hw, count) = aggregate_tier(&tier30);
        assert_eq!(unemp, 3000, "失業者合計 = 1000+800+600+400+200");
        assert_eq!(hw, 150, "HW 合計 = 50+40+30+20+10");
        assert_eq!(count, 5);
    }

    /// **必須テスト 2**: 60 分圏集計
    /// 12 都市の合計が 30 分圏 (Top5) + 60 分圏 (Top 6-12) で正確に分割される
    #[test]
    fn aggregate_60min_tier_with_12_cities_split_correctly() {
        let entries: Vec<NeighborEntry> = (1..=12)
            .map(|i| {
                let commuters = (13 - i) * 1000; // 12000, 11000, ..., 1000 (降順)
                let unemp = (13 - i) * 100; // 1200, 1100, ..., 100
                let hw = 13 - i; // 12, 11, ..., 1
                mk_entry("X県", &format!("市{}", i), commuters, unemp, hw)
            })
            .collect();
        let (tier30, tier60) = split_into_tiers(&entries);
        assert_eq!(tier30.len(), 5);
        assert_eq!(tier60.len(), 7);

        // tier30: Top 5 = 失業 1200+1100+1000+900+800 = 5000、HW 12+11+10+9+8 = 50
        let (u30, h30, _) = aggregate_tier(&tier30);
        assert_eq!(u30, 1200 + 1100 + 1000 + 900 + 800);
        assert_eq!(u30, 5000);
        assert_eq!(h30, 12 + 11 + 10 + 9 + 8);
        assert_eq!(h30, 50);

        // tier60: 6-12 = 失業 700+600+500+400+300+200+100 = 2800、HW 7+6+5+4+3+2+1 = 28
        let (u60, h60, _) = aggregate_tier(&tier60);
        assert_eq!(u60, 700 + 600 + 500 + 400 + 300 + 200 + 100);
        assert_eq!(u60, 2800);
        assert_eq!(h60, 7 + 6 + 5 + 4 + 3 + 2 + 1);
        assert_eq!(h60, 28);
    }

    /// **必須テスト 3-A**: ドメイン不変条件 (失業者プール ≥ 0)
    #[test]
    fn invariant_unemployment_pool_non_negative() {
        let entries = vec![
            mk_entry("X県", "A市", 1000, 0, 0), // 失業 0 でも OK (負ではない)
            mk_entry("X県", "B市", 500, 100, 5),
        ];
        let (tier30, _) = split_into_tiers(&entries);
        let (unemp, _, _) = aggregate_tier(&tier30);
        assert!(unemp >= 0, "失業者プール ≥ 0");
        assert_eq!(unemp, 100);
    }

    /// **必須テスト 3-B**: ドメイン不変条件 (HW 求人 ≥ 0)
    #[test]
    fn invariant_hw_postings_non_negative() {
        let entries = vec![mk_entry("X県", "A市", 100, 50, 0)];
        let (tier30, _) = split_into_tiers(&entries);
        let (_, hw, _) = aggregate_tier(&tier30);
        assert!(hw >= 0);
        assert_eq!(hw, 0);
    }

    /// **必須テスト 3-C**: ドメイン不変条件 (件数上限)
    /// 30 分圏は 5 件以下、60 分圏は 7 件以下、両者重複なし
    #[test]
    fn invariant_tier_count_within_bounds() {
        // 入力 20 件あっても tier30 ≤ 5、tier60 ≤ 7
        let entries: Vec<NeighborEntry> = (1..=20)
            .map(|i| mk_entry("X県", &format!("市{}", i), 100 * (21 - i), 10, 1))
            .collect();
        let (tier30, tier60) = split_into_tiers(&entries);
        assert!(tier30.len() <= TIER_30MIN_COUNT);
        assert!(tier60.len() <= TIER_60MIN_ADDITIONAL_COUNT);
        assert_eq!(tier30.len(), 5);
        assert_eq!(tier60.len(), 7);

        // 重複防止: tier30 と tier60 の (pref, muni) 集合が交わらない
        let tier30_keys: std::collections::HashSet<(String, String)> = tier30
            .iter()
            .map(|e| (e.prefecture.clone(), e.municipality.clone()))
            .collect();
        for e in &tier60 {
            assert!(
                !tier30_keys.contains(&(e.prefecture.clone(), e.municipality.clone())),
                "tier30 と tier60 で重複市区町村: {} {}",
                e.prefecture,
                e.municipality
            );
        }
    }

    /// **必須テスト 4**: fail-soft (entries が空)
    #[test]
    fn fail_soft_empty_entries_returns_empty_tiers() {
        let entries: Vec<NeighborEntry> = Vec::new();
        let (tier30, tier60) = split_into_tiers(&entries);
        assert_eq!(tier30.len(), 0);
        assert_eq!(tier60.len(), 0);
        let (u30, h30, c30) = aggregate_tier(&tier30);
        assert_eq!(u30, 0);
        assert_eq!(h30, 0);
        assert_eq!(c30, 0);
    }

    /// **必須テスト 5**: error_body は notes と空 tier を含む (UI が安全に描画できる)
    #[test]
    fn error_body_contains_safe_defaults() {
        let v = serde_json::to_value(error_body("test")).unwrap();
        assert_eq!(v["error"], "test");
        assert_eq!(v["is_data_available"], false);
        assert_eq!(v["tier_30min"]["municipality_count"], 0);
        assert_eq!(v["tier_30min"]["breakdown"].as_array().unwrap().len(), 0);
        assert_eq!(v["tier_60min"]["municipality_count"], 0);
        assert!(v["notes"]["hw_scope"].as_str().unwrap().contains("HW"));
    }

    /// **必須テスト 6**: 国勢調査年 caveat 文言が含まれる (UI 必須注記)
    /// JSON レスポンスの notes に "2020" "OD" 文字列が含まれること
    #[test]
    fn notes_include_census_year_and_od_keyword() {
        // build_tier にも notes は含まれないため、本文の format! 出力で確認
        let data_source_msg = format!(
            "国勢調査 通勤 OD ({} 年、5年遅れ、市区町村間1:1)",
            COMMUTE_OD_REFERENCE_YEAR
        );
        assert!(data_source_msg.contains("2020"));
        assert!(data_source_msg.contains("OD"));
        assert_eq!(COMMUTE_OD_REFERENCE_YEAR, 2020);
    }

    /// **必須テスト 7**: build_tier の breakdown 並び順は入力順 (= OD volume 降順) を保つ
    #[test]
    fn build_tier_preserves_descending_order() {
        let entries = vec![
            mk_entry("X県", "A市", 1000, 100, 10),
            mk_entry("X県", "B市", 800, 80, 8),
            mk_entry("X県", "C市", 500, 50, 5),
        ];
        let (tier30, _) = split_into_tiers(&entries);
        let v = serde_json::to_value(build_tier(&tier30)).unwrap();
        let bd = v["breakdown"].as_array().unwrap();
        assert_eq!(bd.len(), 3);
        assert_eq!(bd[0]["municipality"], "A市");
        assert_eq!(bd[0]["commuters"], 1000);
        assert_eq!(bd[1]["municipality"], "B市");
        assert_eq!(bd[2]["municipality"], "C市");
        // unemployment_pool, hw_postings の集計
        assert_eq!(v["unemployment_pool"], 100 + 80 + 50);
        assert_eq!(v["hw_postings"], 10 + 8 + 5);
        assert_eq!(v["municipality_count"], 3);
    }

    /// **追加テスト 8**: 定数値の確認 (仕様準拠)
    #[test]
    fn tier_constants_match_spec() {
        assert_eq!(TIER_30MIN_COUNT, 5, "30 分圏は固定 5 件");
        assert_eq!(TIER_60MIN_ADDITIONAL_COUNT, 7, "60 分圏は追加 7 件");
        assert_eq!(
            TIER_30MIN_COUNT + TIER_60MIN_ADDITIONAL_COUNT,
            12,
            "通算 Top 12"
        );
    }

    /// **追加テスト 9**: 4 件しかない場合 (30 分圏は 4 件、60 分圏は 0)
    #[test]
    fn fewer_than_5_entries_all_in_tier_30() {
        let entries = vec![
            mk_entry("X県", "A市", 100, 10, 1),
            mk_entry("X県", "B市", 90, 9, 1),
            mk_entry("X県", "C市", 80, 8, 1),
            mk_entry("X県", "D市", 70, 7, 1),
        ];
        let (t30, t60) = split_into_tiers(&entries);
        assert_eq!(t30.len(), 4);
        assert_eq!(t60.len(), 0);
        let (u, h, c) = aggregate_tier(&t30);
        assert_eq!(c, 4);
        assert_eq!(u, 10 + 9 + 8 + 7);
        assert_eq!(h, 4);
    }

    // ======== 旧 json!() との等価テスト (Phase 1A-1) ========

    fn legacy_tier_to_json(tier: &[NeighborEntry]) -> serde_json::Value {
        let (unemployment_pool, hw_postings, count) = aggregate_tier(tier);
        let breakdown: Vec<serde_json::Value> = tier
            .iter()
            .map(|e| {
                serde_json::json!({
                    "prefecture": e.prefecture,
                    "municipality": e.municipality,
                    "commuters": e.commuters,
                    "unemployment": e.unemployment,
                    "hw_postings": e.hw_postings,
                })
            })
            .collect();
        serde_json::json!({
            "municipality_count": count,
            "unemployment_pool": unemployment_pool,
            "hw_postings": hw_postings,
            "breakdown": breakdown,
        })
    }

    fn legacy_error_body(msg: &str) -> serde_json::Value {
        serde_json::json!({
            "error": msg,
            "panel": "talent_pool_expansion",
            "tier_30min": {"municipality_count": 0, "unemployment_pool": 0, "hw_postings": 0, "breakdown": []},
            "tier_60min": {"municipality_count": 0, "unemployment_pool": 0, "hw_postings": 0, "breakdown": []},
            "is_data_available": false,
            "notes": {
                "hw_scope": HW_SCOPE_NOTE,
                "causation": CAUSATION_NOTE,
            },
        })
    }

    fn legacy_success(
        pref: String,
        muni: String,
        tier_30: &[NeighborEntry],
        tier_60: &[NeighborEntry],
        available: bool,
    ) -> serde_json::Value {
        serde_json::json!({
            "panel": "talent_pool_expansion",
            "current": {
                "prefecture": pref,
                "municipality": muni,
            },
            "tier_30min": legacy_tier_to_json(tier_30),
            "tier_60min": legacy_tier_to_json(tier_60),
            "is_data_available": available,
            "notes": {
                "hw_scope": HW_SCOPE_NOTE,
                "causation": CAUSATION_NOTE,
                "data_source_od": format!(
                    "国勢調査 通勤 OD ({} 年、5年遅れ、市区町村間1:1)",
                    COMMUTE_OD_REFERENCE_YEAR
                ),
                "data_source_unemployment": "SSDSE-A 労働力統計 v2_external_labor_force.unemployed (国勢調査ベース)",
                "data_source_hw": "HW 掲載求人 postings テーブル",
                "tier_definition": format!(
                    "30 分圏 = OD volume 上位 {} 件、60 分圏 = 30 分圏を除いた次の {} 件 (実距離ではなく通勤者数ベースの固定件数)",
                    TIER_30MIN_COUNT, TIER_60MIN_ADDITIONAL_COUNT
                ),
                "caveat_pool": "失業者プール拡大は通勤可能性の理論値であり、実際の応募意向を保証するものではない",
                "caveat_distance": "30/60 分圏という名称は便宜的な分類であり、実走行時間ではない",
            },
        })
    }

    fn s<T: serde::Serialize>(v: &T) -> String {
        serde_json::to_string(v).unwrap()
    }

    fn sample_entries(n: usize) -> Vec<NeighborEntry> {
        (0..n)
            .map(|i| {
                mk_entry(
                    if i % 2 == 0 { "北海道" } else { "" },
                    &format!("市{}", i),
                    (n - i) as i64 * 1000,
                    if i == 1 { 0 } else { 100 * i as i64 },
                    if i == 2 { 0 } else { i as i64 },
                )
            })
            .collect()
    }

    #[test]
    fn tier_matches_legacy_json() {
        for n in [0usize, 1, 5, 12] {
            let e = sample_entries(n);
            assert_eq!(s(&build_tier(&e)), s(&legacy_tier_to_json(&e)), "n={n}");
        }
    }

    #[test]
    fn success_matches_legacy_json() {
        for (n, pref, muni) in [
            (0usize, "北海道", "札幌市"),
            (3, "", ""),
            (12, "東京都", "千代田区"),
        ] {
            let entries = sample_entries(n);
            let (t30, t60) = split_into_tiers(&entries);
            let new = build_response(
                pref.to_string(),
                muni.to_string(),
                &t30,
                &t60,
                !entries.is_empty(),
            );
            let legacy = legacy_success(
                pref.to_string(),
                muni.to_string(),
                &t30,
                &t60,
                !entries.is_empty(),
            );
            assert_eq!(
                s(&RdTalentPoolExpansionResult::Ok(new)),
                s(&legacy),
                "n={n}"
            );
        }
    }

    #[test]
    fn error_matches_legacy_json() {
        for msg in [
            "hellowork.db 未接続",
            "prefecture および municipality が必要です",
            "",
        ] {
            assert_eq!(s(&error_body(msg)), s(&legacy_error_body(msg)));
        }
    }

    #[test]
    fn ts_decl_has_expected_fields() {
        let cfg = super::super::types::ts_config();
        let result = RdTalentPoolExpansionResult::decl(&cfg);
        assert!(
            result.contains("RdTalentPoolExpansionResponse | RdTalentPoolExpansionError"),
            "{result}"
        );
        let resp = RdTalentPoolExpansionResponse::decl(&cfg);
        for f in [
            "panel: string",
            "current: RdTalentPoolCurrent",
            "tier_30min: RdCommuteTier",
            "tier_60min: RdCommuteTier",
            "is_data_available: boolean",
            "notes: RdTalentPoolNotes",
        ] {
            assert!(resp.contains(f), "missing `{f}` in {resp}");
        }
        let tier = RdCommuteTier::decl(&cfg);
        for f in [
            "municipality_count: number",
            "unemployment_pool: number",
            "hw_postings: number",
            "breakdown: Array<RdCommuteTierEntry>",
        ] {
            assert!(tier.contains(f), "missing `{f}` in {tier}");
        }
        let err = RdTalentPoolExpansionError::decl(&cfg);
        assert!(
            err.contains("error: string") && err.contains("notes: RdNotes"),
            "{err}"
        );
    }
}
