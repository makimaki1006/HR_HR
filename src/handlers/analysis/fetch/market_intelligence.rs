//! 採用マーケットインテリジェンス データアクセス層 (Phase 3 Step 1)
//!
//! 媒体分析レポート拡張のための市区町村単位の事前集計テーブル読取専用関数群。
//!
//! ## 対象テーブル (DDL: docs/survey_market_intelligence_phase0_2_schema.sql)
//!
//! | テーブル | 用途 | 状態 |
//! |---------|------|------|
//! | `municipality_recruiting_scores` | 配信優先度スコア | 未投入 (Phase 3 後続で計算投入) |
//! | `municipality_living_cost_proxy` | 生活コスト proxy | 未投入 |
//! | `commute_flow_summary` | 通勤OD要約 (TOP N 流入元) | 未投入 (`v2_external_commute_od` でフォールバック計算) |
//! | `municipality_occupation_population` | 市区町村×職業×年齢×性別 人口 | 未投入 |
//!
//! ## 設計方針
//!
//! - **READ-ONLY**: SELECT のみ。書き込みは Phase 3 後続スコープ。
//! - **table_exists チェック**: テーブル不在時は空 Vec 返却 (フェイルセーフ)
//! - **Turso 優先**: `query_turso_or_local` で Turso V2 を正本とする
//! - **DTO 未導入**: Step 1 では `Vec<Row>` (HashMap<String, Value>) を返す。型付き構造体は Step 2 で追加
//! - **Phase 0〜2 docs 準拠**: `SURVEY_MARKET_INTELLIGENCE_PHASE0_2_PREP.md` §5 Step 1 の関数群を実装
//!
//! 詳細仕様: `docs/SURVEY_MARKET_INTELLIGENCE_METRICS.md`

use serde_json::Value;
use std::collections::HashMap;

use super::super::super::helpers::table_exists;
use super::query_turso_or_local;

#[allow(dead_code)]
type Db = crate::db::local_sqlite::LocalDb;
#[allow(dead_code)]
type TursoDb = crate::db::turso_http::TursoDb;
#[allow(dead_code)]
type Row = HashMap<String, Value>;

/// `top_n` の安全上限 (悪意のある呼び出しによる過大クエリを防止)。
const MAX_TOP_N: usize = 100;

/// 市区町村別 × 職業グループ別の配信優先度スコアを取得する。
///
/// `municipality_recruiting_scores` テーブルから事前集計済のスコアを読む。
/// 詳細スキーマ: `docs/survey_market_intelligence_phase0_2_schema.sql` の同名テーブル。
///
/// # 引数
/// - `municipality_codes`: 取得対象の市区町村コード一覧 (空ならスコープなし → 空 Vec)
/// - `occupation_group_code`: 職業グループコード (空文字なら全職業)
///
/// # 戻り値
/// 各レコードのカラムを保持する `Vec<Row>`。テーブル不在 or 結果なしの場合は空 Vec。
pub(crate) fn fetch_recruiting_scores_by_municipalities(
    db: &Db,
    turso: Option<&TursoDb>,
    municipality_codes: &[&str],
    occupation_group_code: &str,
) -> Vec<Row> {
    if municipality_codes.is_empty() {
        return vec![];
    }
    // Turso/local いずれにも未投入時は空 Vec
    if !table_exists(db, "municipality_recruiting_scores") && turso.is_none() {
        return vec![];
    }

    let select_cols = "municipality_code, prefecture, municipality_name, \
                       occupation_group_code, occupation_group_name, \
                       target_population, adjacent_population, \
                       media_job_count, competitor_job_count, \
                       median_salary_yen, effective_wage_index, \
                       commute_reach_score, job_competition_score, \
                       establishment_competition_score, wage_competitiveness_score, \
                       living_cost_score, effective_wage_score, \
                       distribution_priority_score, \
                       scenario_conservative_population, \
                       scenario_standard_population, \
                       scenario_aggressive_population, \
                       source_year";

    let (sql, params): (String, Vec<String>) = if occupation_group_code.is_empty() {
        // ?1..?N に municipality_codes をバインド
        let placeholders: String = (1..=municipality_codes.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT {select_cols} \
             FROM municipality_recruiting_scores \
             WHERE municipality_code IN ({placeholders}) \
             ORDER BY distribution_priority_score DESC"
        );
        let params = municipality_codes.iter().map(|s| s.to_string()).collect();
        (sql, params)
    } else {
        // ?1 = occupation_group_code, ?2..?(N+1) = municipality_codes
        let placeholders: String = (2..=(municipality_codes.len() + 1))
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT {select_cols} \
             FROM municipality_recruiting_scores \
             WHERE occupation_group_code = ?1 \
               AND municipality_code IN ({placeholders}) \
             ORDER BY distribution_priority_score DESC"
        );
        let mut params: Vec<String> = vec![occupation_group_code.to_string()];
        params.extend(municipality_codes.iter().map(|s| s.to_string()));
        (sql, params)
    };

    query_turso_or_local(turso, db, &sql, &params, "municipality_recruiting_scores")
}

/// 市区町村別の生活コスト proxy データを取得する。
///
/// `municipality_living_cost_proxy` テーブルから単身向け/小世帯向け家賃 proxy・
/// 物価指数・地価などを取得する。
///
/// 詳細仕様: `docs/SURVEY_MARKET_INTELLIGENCE_METRICS.md` §7
pub(crate) fn fetch_living_cost_proxy(
    db: &Db,
    turso: Option<&TursoDb>,
    municipality_codes: &[&str],
) -> Vec<Row> {
    if municipality_codes.is_empty() {
        return vec![];
    }
    if !table_exists(db, "municipality_living_cost_proxy") && turso.is_none() {
        return vec![];
    }

    let placeholders: String = (1..=municipality_codes.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");

    let sql = format!(
        "SELECT municipality_code, prefecture, municipality_name, \
                single_household_rent_proxy, small_household_rent_proxy, \
                rent_per_square_meter, retail_price_index_proxy, \
                household_spending_annual_yen, land_price_residential_per_sqm, \
                housing_cost_rank, source_year \
         FROM municipality_living_cost_proxy \
         WHERE municipality_code IN ({placeholders}) \
         ORDER BY municipality_code"
    );

    let params: Vec<String> = municipality_codes.iter().map(|s| s.to_string()).collect();

    query_turso_or_local(turso, db, &sql, &params, "municipality_living_cost_proxy")
}

/// 目的地市区町村への通勤流入元 TOP N を取得する。
///
/// 優先順:
/// 1. `commute_flow_summary` (事前集計テーブル) があればそれを使う
/// 2. なければ `v2_external_commute_od` (国勢調査 OD 行列、実存) から動的に TOP N 計算
///
/// # 引数
/// - `dest_pref` / `dest_muni`: 目的地 (どちらか空なら空 Vec)
/// - `top_n`: 上位何件まで取るか (内部で `MAX_TOP_N` (=100) でクランプ)
///
/// # 戻り値
/// 流入元の各レコード。`commute_flow_summary` 経由なら `origin_municipality_code` 等が、
/// `v2_external_commute_od` 経由なら `origin_pref` / `origin_muni` / `total_commuters` 等が含まれる。
/// テーブル/データ不在時は空 Vec。
///
/// 詳細仕様: `docs/SURVEY_MARKET_INTELLIGENCE_METRICS.md` §6
pub(crate) fn fetch_commute_flow_summary(
    db: &Db,
    turso: Option<&TursoDb>,
    dest_pref: &str,
    dest_muni: &str,
    top_n: usize,
) -> Vec<Row> {
    if dest_pref.is_empty() || dest_muni.is_empty() {
        return vec![];
    }
    let limit = top_n.min(MAX_TOP_N).max(1);

    // 優先 1: 事前集計テーブル
    if table_exists(db, "commute_flow_summary") {
        let sql = format!(
            "SELECT origin_municipality_code, origin_prefecture, origin_municipality_name, \
                    flow_count, flow_share, target_origin_population, \
                    estimated_target_flow_conservative, \
                    estimated_target_flow_standard, \
                    estimated_target_flow_aggressive, \
                    rank_to_destination, source_year \
             FROM commute_flow_summary \
             WHERE destination_prefecture = ?1 AND destination_municipality_name = ?2 \
             ORDER BY rank_to_destination LIMIT {limit}"
        );
        let params = vec![dest_pref.to_string(), dest_muni.to_string()];
        return query_turso_or_local(turso, db, &sql, &params, "commute_flow_summary");
    }

    // 優先 2: v2_external_commute_od (国勢調査 OD) からフォールバック計算
    // ローカル table_exists チェックでローカル不在時は Turso 経由のみ試行
    let local_has = table_exists(db, "v2_external_commute_od");
    if !local_has && turso.is_none() {
        return vec![];
    }
    let sql = format!(
        "SELECT origin_pref, origin_muni, \
                total_commuters, male_commuters, female_commuters, reference_year \
         FROM v2_external_commute_od \
         WHERE dest_pref = ?1 AND dest_muni = ?2 \
           AND (origin_pref != dest_pref OR origin_muni != dest_muni) \
         ORDER BY total_commuters DESC LIMIT {limit}"
    );
    let params = vec![dest_pref.to_string(), dest_muni.to_string()];
    query_turso_or_local(turso, db, &sql, &params, "v2_external_commute_od")
}

/// 市区町村単位の職業×年齢×性別人口を取得する。
///
/// `municipality_occupation_population` テーブルから国勢調査由来の常住地ベース職業人口を取得する。
///
/// # 引数
/// - `municipality_code`: 取得対象の市区町村コード (空なら空 Vec)
/// - `basis`: `"resident"` (常住地) または `"workplace"` (従業地) (空文字なら両方)
/// - `occupation_codes`: 職業大分類コード一覧 (空なら全職業)
///
/// 詳細仕様: `docs/SURVEY_MARKET_INTELLIGENCE_METRICS.md` §3-§4
pub(crate) fn fetch_occupation_population(
    db: &Db,
    turso: Option<&TursoDb>,
    municipality_code: &str,
    basis: &str,
    occupation_codes: &[&str],
) -> Vec<Row> {
    if municipality_code.is_empty() {
        return vec![];
    }
    if !table_exists(db, "municipality_occupation_population") && turso.is_none() {
        return vec![];
    }

    // ベース WHERE: municipality_code (?1) + basis (?2、空なら省略)
    let mut params: Vec<String> = vec![municipality_code.to_string()];
    let mut where_clauses: Vec<String> = vec!["municipality_code = ?1".to_string()];

    if !basis.is_empty() {
        params.push(basis.to_string());
        where_clauses.push("basis = ?2".to_string());
    }

    // occupation_codes IN (...)
    if !occupation_codes.is_empty() {
        let start = params.len() + 1;
        let placeholders: String = (start..start + occupation_codes.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        where_clauses.push(format!("occupation_code IN ({placeholders})"));
        for c in occupation_codes {
            params.push(c.to_string());
        }
    }

    let where_sql = where_clauses.join(" AND ");
    let sql = format!(
        "SELECT municipality_code, prefecture, municipality_name, basis, \
                occupation_code, occupation_name, age_group, gender, \
                population, source_year \
         FROM municipality_occupation_population \
         WHERE {where_sql} \
         ORDER BY occupation_code, age_group, gender"
    );

    query_turso_or_local(turso, db, &sql, &params, "municipality_occupation_population")
}

// ============================================================
// テスト
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::local_sqlite::LocalDb;

    /// 既存パターン (`db/local_sqlite.rs:131` 流用) で空の一時 SQLite DB を作成。
    fn create_test_db() -> (tempfile::NamedTempFile, LocalDb) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let _ = rusqlite::Connection::open(path).unwrap();
        let db = LocalDb::new(path).unwrap();
        (tmp, db)
    }

    /// 全テーブルが空の状態で各 fetch 関数が空 Vec を返すこと。
    /// (テーブル不在時のフェイルセーフ動作確認)
    #[test]
    fn test_all_fetch_functions_return_empty_when_tables_missing() {
        let (_tmp, db) = create_test_db();

        // 全 fetch 関数: テーブル不在 + Turso None → 空 Vec
        assert!(
            fetch_recruiting_scores_by_municipalities(&db, None, &["01101"], "").is_empty(),
            "recruiting_scores: 空配列を期待"
        );
        assert!(
            fetch_living_cost_proxy(&db, None, &["01101"]).is_empty(),
            "living_cost_proxy: 空配列を期待"
        );
        assert!(
            fetch_commute_flow_summary(&db, None, "北海道", "札幌市", 10).is_empty(),
            "commute_flow_summary: 空配列を期待 (両テーブル不在)"
        );
        assert!(
            fetch_occupation_population(&db, None, "01101", "resident", &[]).is_empty(),
            "occupation_population: 空配列を期待"
        );
    }

    /// 必須引数が空の場合、early return で空 Vec を返すこと。
    #[test]
    fn test_required_args_empty_returns_empty() {
        let (_tmp, db) = create_test_db();

        // municipality_codes 空
        assert!(fetch_recruiting_scores_by_municipalities(&db, None, &[], "").is_empty());
        assert!(fetch_living_cost_proxy(&db, None, &[]).is_empty());

        // dest_pref / dest_muni 空
        assert!(fetch_commute_flow_summary(&db, None, "", "札幌市", 10).is_empty());
        assert!(fetch_commute_flow_summary(&db, None, "北海道", "", 10).is_empty());

        // municipality_code 空
        assert!(fetch_occupation_population(&db, None, "", "resident", &[]).is_empty());
    }

    /// `commute_flow_summary` 不在 + `v2_external_commute_od` 存在 →
    /// `v2_external_commute_od` から TOP N が取れること (フォールバック動作)。
    #[test]
    fn test_commute_flow_summary_falls_back_to_external_commute_od() {
        let (_tmp, db) = create_test_db();

        // v2_external_commute_od テーブルを作って 3 件投入
        db.execute(
            "CREATE TABLE v2_external_commute_od (
                origin_pref TEXT NOT NULL,
                origin_muni TEXT NOT NULL,
                dest_pref TEXT NOT NULL,
                dest_muni TEXT NOT NULL,
                total_commuters INTEGER NOT NULL,
                male_commuters INTEGER,
                female_commuters INTEGER,
                reference_year INTEGER,
                PRIMARY KEY (origin_pref, origin_muni, dest_pref, dest_muni)
            )",
            &[],
        )
        .expect("CREATE TABLE 失敗");

        for (op, om, dc) in [
            ("北海道", "小樽市", 5000_i64),
            ("北海道", "江別市", 8000_i64),
            ("北海道", "石狩市", 3000_i64),
            ("北海道", "札幌市", 100_i64), // self-loop は除外される想定
        ] {
            db.execute(
                "INSERT INTO v2_external_commute_od VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, 2020)",
                &[
                    &op as &dyn rusqlite::types::ToSql,
                    &om,
                    &"北海道",
                    &"札幌市",
                    &dc,
                ],
            )
            .expect("INSERT 失敗");
        }

        let rows = fetch_commute_flow_summary(&db, None, "北海道", "札幌市", 10);
        assert_eq!(rows.len(), 3, "self-loop 除外で 3 件期待 (実際: {})", rows.len());

        // 1 位は江別市 (total_commuters=8000)
        let first_origin_muni = rows[0]
            .get("origin_muni")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert_eq!(first_origin_muni, "江別市", "TOP1 流入元は江別市");
    }
}
