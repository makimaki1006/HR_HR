//! fromto_city 人流データクエリハンドラ（地方ブロック4区分）
//!
//! from_area の4区分:
//! - 0: 同一市区町村
//! - 1: 同一都道府県の別市区町村
//! - 2: 同一地方ブロックの別都道府県
//! - 3: 異なる地方ブロック
//!
//! 個別市区町村間1:1 ODは取得不可（Agoop制約）。細粒度ODは `v2_external_commute_od`（国勢調査）を使用。

use super::super::helpers::{table_exists, Row};
use crate::db::local_sqlite::LocalDb as Db;
use crate::db::turso_http::TursoDb;

const FROMTO_TABLE: &str = "v2_flow_fromto_city";

/// 着地市区町村への流入TOP（from_area別）
pub fn get_fromto_top(
    db: &Db,
    turso: Option<&TursoDb>,
    citycode: i64,
    year: i32,
    month: i32,
    dayflag: i32,
    timezone: i32,
    limit: i32,
) -> Vec<Row> {
    if !table_exists(db, FROMTO_TABLE) {
        return vec![];
    }
    let sql = format!(
        "SELECT from_area, population \
         FROM {FROMTO_TABLE} \
         WHERE citycode = ?1 AND year = ?2 AND month = ?3 \
           AND dayflag = ?4 AND timezone = ?5 \
         ORDER BY population DESC \
         LIMIT ?6"
    );
    let params = vec![
        citycode.to_string(),
        year.to_string(),
        format!("{:02}", month),
        dayflag.to_string(),
        timezone.to_string(),
        limit.to_string(),
    ];
    super::super::analysis::fetch::query_turso_or_local(turso, db, &sql, &params, FROMTO_TABLE)
}

/// 地域カルテ用: from_area 4区分の総計内訳（年次、平日昼のみ）
pub fn get_inflow_breakdown(
    db: &Db,
    turso: Option<&TursoDb>,
    citycode: i64,
    year: i32,
) -> Vec<Row> {
    if !table_exists(db, FROMTO_TABLE) {
        return vec![];
    }
    // 平日昼（dayflag=1, timezone=0）のみで集計。全月平均として SUM/12。
    let sql = format!(
        "SELECT from_area, SUM(population) as total_population \
         FROM {FROMTO_TABLE} \
         WHERE citycode = ?1 AND year = ?2 AND dayflag = 1 AND timezone = 0 \
         GROUP BY from_area \
         ORDER BY from_area"
    );
    let params = vec![citycode.to_string(), year.to_string()];
    super::super::analysis::fetch::query_turso_or_local(turso, db, &sql, &params, FROMTO_TABLE)
}

/// from_area コードの日本語ラベル
pub fn from_area_label(code: i64) -> &'static str {
    match code {
        0 => "同一市区町村",
        1 => "同一都道府県の別市区町村",
        2 => "同一地方ブロックの別都道府県",
        3 => "異なる地方ブロック",
        _ => "不明",
    }
}

/// from_area コードの短縮ラベル（サンキー図のノード名用）
pub fn from_area_short_label(code: i64) -> &'static str {
    match code {
        0 => "同市区町村",
        1 => "同県別市",
        2 => "同地方別県",
        3 => "異地方",
        _ => "不明",
    }
}

/// 流入元サンキー図用: 特定月・時間帯の from_area 別流入量
///
/// 🔴 非集計値 (dayflag=0 or 1, timezone=0 or 1) のみ使用。
///     dayflag=2/timezone=2 の集計値は呼び出し側で渡さない想定（double-count防止）。
pub fn get_inflow_sankey(
    db: &Db,
    turso: Option<&TursoDb>,
    citycode: i64,
    year: i32,
    month: i32,
    dayflag: i32,
    timezone: i32,
) -> Vec<Row> {
    if !table_exists(db, FROMTO_TABLE) {
        return vec![];
    }
    let sql = format!(
        "SELECT from_area, SUM(population) as total_population \
         FROM {FROMTO_TABLE} \
         WHERE citycode = ?1 AND year = ?2 AND month = ?3 \
           AND dayflag = ?4 AND timezone = ?5 \
         GROUP BY from_area \
         ORDER BY from_area"
    );
    let params = vec![
        citycode.to_string(),
        year.to_string(),
        format!("{:02}", month),
        dayflag.to_string(),
        timezone.to_string(),
    ];
    super::super::analysis::fetch::query_turso_or_local(turso, db, &sql, &params, FROMTO_TABLE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_area_labels() {
        assert_eq!(from_area_label(0), "同一市区町村");
        assert_eq!(from_area_label(1), "同一都道府県の別市区町村");
        assert_eq!(from_area_label(2), "同一地方ブロックの別都道府県");
        assert_eq!(from_area_label(3), "異なる地方ブロック");
        assert_eq!(from_area_label(99), "不明");
    }
}
