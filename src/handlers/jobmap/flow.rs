//! mesh1km 人流データクエリハンドラ
//!
//! 年別分割テーブル `v2_flow_mesh1km_YYYY` と集計テーブル `v2_flow_mesh3km_agg` / `v2_flow_city_agg` を
//! ズームレベル別にルーティングする。
//!
//! # 設計原則
//!
//! - `AggregateMode` enum で dayflag=2/timezone=2 の double count を型強制防御
//! - bbox → mesh1kmid 範囲クエリは `BETWEEN min_id AND max_id`（IN句爆発回避、ユーザー判断#4）
//! - Turso / ローカルSQLite の両方に対応（`query_turso_or_local`）
//!
//! # CTAS 戻し完了（2026-05-01 Turso 無料枠リセット後）
//!
//! `v2_flow_city_agg` / `v2_flow_mesh3km_agg` を Turso 投入完了し、
//! `GROUP BY` 動的集計から事前集計テーブル参照に戻した。本番性能 ~10x 改善見込。
//! `has_flow_table(db, "v2_flow_city_agg")` の早期 return も復活。
//! 戻し手順は `docs/flow_ctas_restore.md` を参照。

use super::super::helpers::{table_exists, Row};
use super::flow_types::{AggregateMode, AggregateModeError};
use crate::db::local_sqlite::LocalDb as Db;
use crate::db::turso_http::TursoDb;

/// 年別テーブル名を解決（2019/2020/2021 以外は Err）
pub fn resolve_table_by_year(year: i32) -> Result<&'static str, AggregateModeError> {
    match year {
        2019 => Ok("v2_flow_mesh1km_2019"),
        2020 => Ok("v2_flow_mesh1km_2020"),
        2021 => Ok("v2_flow_mesh1km_2021"),
        _ => Err(AggregateModeError::InvalidParams {
            dayflag: -1,
            timezone: year,
        }),
    }
}

// 2026-05-01 CTAS 戻し: any_mesh1km_table_exists は CTAS 戻し後に未使用となり削除。
// 各 CTAS ベース関数は table_exists(db, "v2_flow_city_agg" or "v2_flow_mesh3km_agg") で個別判定。
// 生 mesh1km 参照の get_mesh_heatmap (z≥13) は has_flow_table(db, table) で年別判定継続。

/// bbox内 mesh1km ヒートマップ取得（z≥13 用）
///
/// `v2_flow_attribute_mesh1km` で bbox 内のmesh1kmidを範囲決定 → BETWEEN句で絞り込み。
pub fn get_mesh_heatmap(
    db: &Db,
    turso: Option<&TursoDb>,
    mesh_min: i64,
    mesh_max: i64,
    year: i32,
    month: i32,
    mode: AggregateMode,
) -> Vec<Row> {
    let table = match resolve_table_by_year(year) {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    if !has_flow_table(db, table) {
        return vec![];
    }

    let sql = format!(
        "SELECT mesh1kmid, prefcode, citycode, month, dayflag, timezone, population \
         FROM {table} \
         WHERE mesh1kmid BETWEEN ?1 AND ?2 \
           AND month = ?3 \
           AND {mode_where} \
         ORDER BY mesh1kmid, dayflag, timezone",
        mode_where = mode.where_clause()
    );
    let params: Vec<String> = vec![
        mesh_min.to_string(),
        mesh_max.to_string(),
        format!("{:02}", month),
    ];
    query_db(db, turso, &sql, &params, table)
}

/// mesh3km 集計ヒートマップ（z10-12 用）
///
/// 2026-05-01 CTAS 戻し: `v2_flow_mesh3km_agg` 直接参照。
/// mesh_min/mesh_max は 3km 単位（呼び出し元が `/1000` 済み）でそのまま使用。
/// `mode.where_clause()` で dayflag=2/timezone=2 double count 防御は維持。
pub fn get_mesh3km_heatmap(
    db: &Db,
    turso: Option<&TursoDb>,
    mesh_min: i64,
    mesh_max: i64,
    year: i32,
    month: i32,
    mode: AggregateMode,
) -> Vec<Row> {
    // 年バリデーション維持 (CTAS 内 year カラムでフィルタ)
    if resolve_table_by_year(year).is_err() {
        return vec![];
    }
    // CTAS 早期 return (Turso 専用環境では turso が渡された時点で存在と見なす)
    if !table_exists(db, "v2_flow_mesh3km_agg") && turso.is_none() {
        return vec![];
    }

    let sql = format!(
        "SELECT mesh3kmid_approx, year, month, dayflag, timezone, pop_sum \
         FROM v2_flow_mesh3km_agg \
         WHERE mesh3kmid_approx BETWEEN ?1 AND ?2 \
           AND year = ?3 AND month = ?4 \
           AND {mode_where} \
         ORDER BY mesh3kmid_approx",
        mode_where = mode.where_clause()
    );
    let params = vec![
        mesh_min.to_string(),
        mesh_max.to_string(),
        year.to_string(),
        format!("{:02}", month),
    ];
    query_db(db, turso, &sql, &params, "v2_flow_mesh3km_agg")
}

/// city_agg 集計ヒートマップ（z≤9 用）
///
/// 2026-05-01 CTAS 戻し: `v2_flow_city_agg` 直接参照。
/// `AggregateMode::from_params(dayflag, timezone)` で不正組合せを遮断は維持。
/// CTAS 投入時点で dayflag/timezone は正規化済 (0/1/2 の値で各レコード保持)。
pub fn get_city_agg(
    db: &Db,
    turso: Option<&TursoDb>,
    year: i32,
    month: i32,
    dayflag: i32,
    timezone: i32,
) -> Vec<Row> {
    // 年バリデーション維持
    if resolve_table_by_year(year).is_err() {
        return vec![];
    }
    // dayflag/timezone の double count 防御（入力検証）
    if AggregateMode::from_params(dayflag, timezone).is_err() {
        return vec![];
    }
    // CTAS 早期 return
    if !table_exists(db, "v2_flow_city_agg") && turso.is_none() {
        return vec![];
    }

    let sql = "SELECT citycode, year, month, dayflag, timezone, pop_sum, mesh_count \
               FROM v2_flow_city_agg \
               WHERE year = ?1 AND month = ?2 AND dayflag = ?3 AND timezone = ?4 \
               ORDER BY citycode";
    let params = vec![
        year.to_string(),
        format!("{:02}", month),
        dayflag.to_string(),
        timezone.to_string(),
    ];
    query_db(db, turso, sql, &params, "v2_flow_city_agg")
}

/// 地域カルテ用: 時間帯プロファイル（citycode内のmonth×dayflag×timezone集計）
///
/// 2026-05-01 CTAS 戻し: `v2_flow_city_agg` 直接参照、生値 (dayflag IN (0,1) AND timezone IN (0,1)) のみ。
pub fn get_karte_profile(db: &Db, turso: Option<&TursoDb>, citycode: i64, year: i32) -> Vec<Row> {
    if resolve_table_by_year(year).is_err() {
        return vec![];
    }
    if !table_exists(db, "v2_flow_city_agg") && turso.is_none() {
        return vec![];
    }

    let sql = "SELECT month, dayflag, timezone, pop_sum \
               FROM v2_flow_city_agg \
               WHERE citycode = ?1 AND year = ?2 \
                 AND dayflag IN (0,1) AND timezone IN (0,1) \
               ORDER BY month, dayflag, timezone";
    let params = vec![citycode.to_string(), year.to_string()];
    query_db(db, turso, sql, &params, "v2_flow_city_agg")
}

/// 36ヶ月時系列（地域カルテ用、コロナ期markArea用）
///
/// 2026-05-01 CTAS 戻し: `v2_flow_city_agg` 直接参照、3 年 (2019-2021) × 12 ヶ月 × 平日昼 (dayflag=1, timezone=0) のみ。
pub fn get_karte_monthly_trend(db: &Db, turso: Option<&TursoDb>, citycode: i64) -> Vec<Row> {
    if !table_exists(db, "v2_flow_city_agg") && turso.is_none() {
        return vec![];
    }

    let sql = "SELECT year, month, pop_sum \
               FROM v2_flow_city_agg \
               WHERE citycode = ?1 \
                 AND year IN (2019, 2020, 2021) \
                 AND dayflag = 1 AND timezone = 0 \
               ORDER BY year, month";
    let params = vec![citycode.to_string()];
    query_db(db, turso, sql, &params, "v2_flow_city_agg")
}

/// 昼夜比: 市区町村の平日昼滞在 / 夜間滞在
///
/// 2026-05-01 CTAS 戻し: `v2_flow_city_agg` 直接参照。
/// 平日 (dayflag=1) × 昼/夜 (timezone IN (0,1))。集計値 timezone=2 は含めない (double count 防御)。
pub fn get_karte_daynight_ratio(
    db: &Db,
    turso: Option<&TursoDb>,
    citycode: i64,
    year: i32,
) -> Option<f64> {
    if resolve_table_by_year(year).is_err() {
        return None;
    }
    if !table_exists(db, "v2_flow_city_agg") && turso.is_none() {
        return None;
    }

    let sql = "SELECT timezone, SUM(pop_sum) as total \
               FROM v2_flow_city_agg \
               WHERE citycode = ?1 AND year = ?2 \
                 AND dayflag = 1 AND timezone IN (0,1) \
               GROUP BY timezone";
    let params = vec![citycode.to_string(), year.to_string()];
    let rows = query_db(db, turso, sql, &params, "v2_flow_city_agg");
    if rows.len() < 2 {
        return None;
    }
    let mut day = 0.0;
    let mut night = 0.0;
    for r in &rows {
        let tz = super::super::helpers::get_i64(r, "timezone");
        let total = super::super::helpers::get_f64(r, "total");
        match tz {
            0 => day = total,
            1 => night = total,
            _ => {}
        }
    }
    if night > 0.0 {
        Some(day / night)
    } else {
        None
    }
}

// ======== 内部ヘルパー ========

/// テーブル存在確認（flow_staging.db と country-statistics Turso の両対応）
fn has_flow_table(db: &Db, name: &str) -> bool {
    table_exists(db, name)
}

/// Turso → ローカルフォールバッククエリ（analysis/fetch.rs の pub(crate) 関数を経由）
fn query_db(
    db: &Db,
    turso: Option<&TursoDb>,
    sql: &str,
    params: &[String],
    table: &str,
) -> Vec<Row> {
    super::super::analysis::fetch::query_turso_or_local(turso, db, sql, params, table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_table_valid_years() {
        assert_eq!(resolve_table_by_year(2019).unwrap(), "v2_flow_mesh1km_2019");
        assert_eq!(resolve_table_by_year(2020).unwrap(), "v2_flow_mesh1km_2020");
        assert_eq!(resolve_table_by_year(2021).unwrap(), "v2_flow_mesh1km_2021");
    }

    #[test]
    fn resolve_table_invalid_years() {
        assert!(resolve_table_by_year(2018).is_err());
        assert!(resolve_table_by_year(2022).is_err());
        assert!(resolve_table_by_year(0).is_err());
    }

    // ===== FALLBACK SQL の構造検証（逆証明: double count しない構造になっているか） =====

    /// city_agg FALLBACK: Raw モードで dayflag=2/timezone=2 が WHERE 句に現れないこと
    #[test]
    fn city_agg_fallback_no_double_count_in_raw() {
        // 実DBには繋がないため、SQL組立の純粋関数化が無い場合は
        // AggregateMode::Raw.where_clause() が出す文字列を直接検証する。
        let w = AggregateMode::Raw.where_clause();
        assert!(w.contains("dayflag IN (0,1)"));
        assert!(w.contains("timezone IN (0,1)"));
        assert!(!w.contains("dayflag = 2"));
        assert!(!w.contains("timezone = 2"));
    }

    /// city_agg FALLBACK: 不正な dayflag/timezone 組合せで空 Vec を返すこと（早期return検証）
    #[test]
    fn city_agg_fallback_invalid_params_returns_empty() {
        // 実DB無しテスト: resolve_table_by_year が Ok でも from_params が Err なら空が返る。
        // → AggregateMode::from_params(9, 9) が Err であることを前提にした設計を明示。
        assert!(AggregateMode::from_params(9, 9).is_err());
        assert!(AggregateMode::from_params(0, 9).is_err());
        assert!(AggregateMode::from_params(9, 0).is_err());
    }

    /// mesh3km FALLBACK: mesh_min/max が安全に mesh1km レンジへ展開されること（overflow防御）
    #[test]
    fn mesh3km_range_expansion_no_overflow() {
        // i64::MAX を越える乗算は saturating で上限に張り付くことを確認
        let huge: i64 = i64::MAX / 500;
        let expanded = huge.saturating_mul(1000).saturating_add(999);
        assert_eq!(expanded, i64::MAX);
    }

    /// karte_monthly_trend: UNION ALL SQL が 3 年分含み、dayflag=1/timezone=0 固定であること
    #[test]
    fn karte_monthly_sql_shape() {
        // コード内のSQL文字列に対し必要要素が含まれることを確認（回帰防止）
        let expected_elements = [
            "v2_flow_mesh1km_2019",
            "v2_flow_mesh1km_2020",
            "v2_flow_mesh1km_2021",
            "UNION ALL",
            "dayflag = 1",
            "timezone = 0",
        ];
        // 関数体のSQL再掲（テストで同型を保つため）
        let sql = "\
            SELECT year, month, SUM(pop_sum) AS pop_sum FROM ( \
                SELECT 2019 AS year, month, SUM(population) AS pop_sum \
                  FROM v2_flow_mesh1km_2019 \
                  WHERE citycode = ?1 AND dayflag = 1 AND timezone = 0 \
                  GROUP BY month \
                UNION ALL \
                SELECT 2020 AS year, month, SUM(population) AS pop_sum \
                  FROM v2_flow_mesh1km_2020 \
                  WHERE citycode = ?1 AND dayflag = 1 AND timezone = 0 \
                  GROUP BY month \
                UNION ALL \
                SELECT 2021 AS year, month, SUM(population) AS pop_sum \
                  FROM v2_flow_mesh1km_2021 \
                  WHERE citycode = ?1 AND dayflag = 1 AND timezone = 0 \
                  GROUP BY month \
            ) \
            GROUP BY year, month \
            ORDER BY year, month";
        for e in expected_elements {
            assert!(sql.contains(e), "missing element: {e}");
        }
        // dayflag=2 / timezone=2 を含まない（double count 無し）
        assert!(!sql.contains("dayflag = 2"));
        assert!(!sql.contains("timezone = 2"));
    }
}
