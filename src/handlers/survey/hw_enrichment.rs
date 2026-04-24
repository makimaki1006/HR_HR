//! 媒体分析: CSV 住所 × HW データベース連携
//!
//! CSV からパースされた (prefecture, municipality) ペアごとに、HW ローカル DB
//! (`postings`) と HW 時系列 DB (`ts_turso_counts`) を照合し、
//! 各エリアの HW 求人数・3ヶ月増員率・1年増員率を算出する。
//!
//! ## 指標の定義
//! - `hw_posting_count`: HW に現在掲載されている求人件数（postings テーブル直接カウント）
//! - `posting_change_3m_pct`: 過去3ヶ月の HW 求人件数変化率 (%)。
//!   ts_turso_counts 由来。最新月と 3ヶ月前の比較。
//! - `posting_change_1y_pct`: 過去1年の HW 求人件数変化率 (%)。同上、12ヶ月前比較。
//! - `vacancy_rate_external`: 外部統計由来の欠員率（InsightContext を利用する場合）。
//!   本モジュールでは直接取得せず、呼び出し元で設定する想定。
//!
//! ## MEMORY 遵守 (feedback_hw_data_scope)
//! - HW掲載求人のみの指標。市場全体ではない。
//! - 3ヶ月/1年の比較は季節要因を含む。
//! - 「増員率」はサンプル件数変動の可能性を含む（業界フィルタ時は
//!   ts_turso_salary 由来のサンプル件数のため、より慎重な解釈が必要）。

use crate::db::local_sqlite::LocalDb;
use crate::db::turso_http::TursoDb;
use crate::handlers::helpers::{get_f64, get_i64};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 単一 (prefecture, municipality) ペアに対する HW 連携指標
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HwAreaEnrichment {
    pub prefecture: String,
    pub municipality: String,
    /// HW 現在掲載求人数（postings テーブル）
    pub hw_posting_count: i64,
    /// 3ヶ月前比の求人件数変化率 (%)。None は時系列データ欠如。
    pub posting_change_3m_pct: Option<f64>,
    /// 1年前比の求人件数変化率 (%)。
    pub posting_change_1y_pct: Option<f64>,
    /// 欠員率 (%) — 外部統計由来。None は未取得。
    /// 呼び出し元で InsightContext から補填する想定（仕様書 4 節 Section H 参照）。
    pub vacancy_rate_pct: Option<f64>,
}

impl HwAreaEnrichment {
    /// 増員率ラベル（人事担当向け定性表現）
    pub fn change_label_3m(&self) -> &'static str {
        match self.posting_change_3m_pct {
            None => "—",
            Some(v) if v > 15.0 => "大きく増加",
            Some(v) if v > 3.0 => "緩やかに増加",
            Some(v) if v >= -3.0 => "横ばい",
            Some(v) if v >= -15.0 => "緩やかに減少",
            Some(_) => "大きく減少",
        }
    }

    pub fn change_label_1y(&self) -> &'static str {
        match self.posting_change_1y_pct {
            None => "—",
            Some(v) if v > 30.0 => "大きく増加",
            Some(v) if v > 10.0 => "緩やかに増加",
            Some(v) if v >= -10.0 => "横ばい",
            Some(v) if v >= -30.0 => "緩やかに減少",
            Some(_) => "大きく減少",
        }
    }
}

/// 複数の (pref, muni) ペアをまとめて enrich（N+1 クエリ回避）
///
/// # Args
/// - `db`: ローカル SQLite (postings テーブル用)
/// - `turso`: Turso 時系列テーブル用（None なら時系列データは None のまま）
/// - `pref_muni_pairs`: (prefecture, municipality) 配列
///
/// # Returns
/// key = `"{prefecture}:{municipality}"` の HashMap
pub fn enrich_areas(
    db: &LocalDb,
    turso: Option<&TursoDb>,
    pref_muni_pairs: &[(String, String)],
) -> HashMap<String, HwAreaEnrichment> {
    let mut result: HashMap<String, HwAreaEnrichment> = HashMap::new();

    // 重複除去（同一 pref+muni の複数行が CSV にあっても HW 側は1回だけ問い合わせる）
    let unique: Vec<(String, String)> = {
        let mut seen = std::collections::HashSet::new();
        pref_muni_pairs
            .iter()
            .filter(|(p, m)| !p.is_empty() && !m.is_empty())
            .filter(|pm| seen.insert(((*pm).0.clone(), (*pm).1.clone())))
            .cloned()
            .collect()
    };

    // 1) postings テーブルから現在求人件数を一括取得
    for (pref, muni) in &unique {
        let key = format!("{}:{}", pref, muni);
        let count = fetch_hw_posting_count(db, pref, muni);
        let entry = result.entry(key).or_insert_with(|| HwAreaEnrichment {
            prefecture: pref.clone(),
            municipality: muni.clone(),
            ..Default::default()
        });
        entry.hw_posting_count = count;
    }

    // 2) Turso 時系列から 3m/1y 変化率を取得（Turso 無ければスキップ）
    if let Some(t) = turso {
        // 都道府県単位でキャッシュ化（ts_turso_counts は prefecture+emp_group でのみ取得可能、
        // municipality は今のところ利用不可。呼び出し元で pref 集計値を使う前提）
        let prefs: Vec<String> = {
            let mut set = std::collections::HashSet::new();
            unique
                .iter()
                .filter_map(|(p, _)| if set.insert(p.clone()) { Some(p.clone()) } else { None })
                .collect()
        };
        let mut pref_changes: HashMap<String, (Option<f64>, Option<f64>)> = HashMap::new();
        for pref in &prefs {
            let (change_3m, change_1y) = fetch_pref_posting_changes(t, pref);
            pref_changes.insert(pref.clone(), (change_3m, change_1y));
        }
        for (pref, muni) in &unique {
            let key = format!("{}:{}", pref, muni);
            if let Some((c3, c1)) = pref_changes.get(pref) {
                if let Some(entry) = result.get_mut(&key) {
                    entry.posting_change_3m_pct = *c3;
                    entry.posting_change_1y_pct = *c1;
                }
            }
        }
    }

    result
}

/// 単一 (pref, muni) の HW 求人件数
fn fetch_hw_posting_count(db: &LocalDb, pref: &str, muni: &str) -> i64 {
    let sql = "SELECT COUNT(*) as cnt FROM postings WHERE prefecture = ?1 AND municipality = ?2";
    let params: Vec<&dyn rusqlite::types::ToSql> = vec![&pref, &muni];
    db.query(sql, &params)
        .ok()
        .and_then(|rows| rows.first().map(|r| get_i64(r, "cnt")))
        .unwrap_or(0)
}

/// 都道府県単位の 3ヶ月・1年 posting 件数変化率 (%)
///
/// 注意: ts_turso_counts は posting_count（正社員/パート/その他合計）の snapshot を持つ。
/// 最新 snapshot と 3ヶ月前（または1年前）snapshot を比較して変化率を算出。
///
/// Return: (change_3m_pct, change_1y_pct)
fn fetch_pref_posting_changes(turso: &TursoDb, pref: &str) -> (Option<f64>, Option<f64>) {
    let sql = "SELECT snapshot_id, SUM(posting_count) as total \
               FROM ts_turso_counts \
               WHERE prefecture = ?1 \
               GROUP BY snapshot_id \
               ORDER BY snapshot_id DESC \
               LIMIT 14";
    let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> = vec![&pref];
    let rows = match turso.query(sql, &params) {
        Ok(r) => r,
        Err(_) => return (None, None),
    };
    if rows.is_empty() {
        return (None, None);
    }
    // 最新から降順に取得済。最新 = rows[0], 3ヶ月前 = rows[3], 1年前 = rows[12] 近似
    let latest = get_f64(rows.first().unwrap(), "total");
    if latest <= 0.0 {
        return (None, None);
    }
    let change_3m = rows
        .get(3)
        .map(|r| get_f64(r, "total"))
        .filter(|v| *v > 0.0)
        .map(|prev| (latest - prev) / prev * 100.0);
    let change_1y = rows
        .get(12)
        .map(|r| get_f64(r, "total"))
        .filter(|v| *v > 0.0)
        .map(|prev| (latest - prev) / prev * 100.0);
    (change_3m, change_1y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_label_3m_boundaries() {
        let mut e = HwAreaEnrichment::default();
        e.posting_change_3m_pct = Some(20.0);
        assert_eq!(e.change_label_3m(), "大きく増加");
        e.posting_change_3m_pct = Some(5.0);
        assert_eq!(e.change_label_3m(), "緩やかに増加");
        e.posting_change_3m_pct = Some(0.0);
        assert_eq!(e.change_label_3m(), "横ばい");
        e.posting_change_3m_pct = Some(-5.0);
        assert_eq!(e.change_label_3m(), "緩やかに減少");
        e.posting_change_3m_pct = Some(-20.0);
        assert_eq!(e.change_label_3m(), "大きく減少");
        e.posting_change_3m_pct = None;
        assert_eq!(e.change_label_3m(), "—");
    }

    #[test]
    fn change_label_1y_boundaries() {
        let mut e = HwAreaEnrichment::default();
        e.posting_change_1y_pct = Some(40.0);
        assert_eq!(e.change_label_1y(), "大きく増加");
        e.posting_change_1y_pct = Some(15.0);
        assert_eq!(e.change_label_1y(), "緩やかに増加");
        e.posting_change_1y_pct = Some(0.0);
        assert_eq!(e.change_label_1y(), "横ばい");
        e.posting_change_1y_pct = Some(-20.0);
        assert_eq!(e.change_label_1y(), "緩やかに減少");
        e.posting_change_1y_pct = Some(-40.0);
        assert_eq!(e.change_label_1y(), "大きく減少");
    }
}
