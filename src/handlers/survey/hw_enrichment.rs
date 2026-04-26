//! 媒体分析: CSV 住所 × HW データベース連携
//!
//! CSV からパースされた (prefecture, municipality) ペアごとに、HW ローカル DB
//! (`postings`) と HW 時系列 DB (`ts_turso_counts`) を照合し、
//! 各エリアの HW 求人数・3ヶ月増員率・1年増員率を算出する。
//!
//! ## 指標の定義
//! - `hw_posting_count`: HW に現在掲載されている求人件数（postings テーブル直接カウント、**市区町村粒度**）
//! - `posting_change_3m_pct`: 過去3ヶ月の HW 求人件数変化率 (%)。
//!   ts_turso_counts 由来。最新月と 3ヶ月前の比較。
//!   **🔴 注意: ts_turso_counts は都道府県粒度しか持たないため、
//!   同一都道府県内の全市区町村が同じ値を示す。市区町村単位の動向ではない**。
//! - `posting_change_1y_pct`: 過去1年の HW 求人件数変化率 (%)。同上、12ヶ月前比較。
//!   **🔴 同様に都道府県粒度の値を市区町村に流し込んでいる**。
//! - `vacancy_rate_external`: 外部統計由来の欠員補充率（求人理由が「欠員補充」の比率）。
//!   InsightContext を利用する場合は呼び出し元で設定する想定。
//!
//! ## MEMORY 遵守 (feedback_hw_data_scope)
//! - HW掲載求人のみの指標。市場全体ではない。
//! - 3ヶ月/1年の比較は季節要因を含む。
//! - 「増員率」はサンプル件数変動の可能性を含む（業界フィルタ時は
//!   ts_turso_salary 由来のサンプル件数のため、より慎重な解釈が必要）。
//! - **粒度の不一致**: posting_change_*m_pct は都道府県単位の集計値であり、
//!   市区町村独自の動向を表すものではない。UI 表示時に明記すること。

use crate::db::local_sqlite::LocalDb;
use crate::db::turso_http::TursoDb;
use crate::handlers::helpers::{get_f64, get_i64};
use crate::handlers::types::VacancyRatePct;
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
    /// 欠員補充率（% 単位 = `VacancyRatePct`）— 外部統計由来。None は未取得。
    ///
    /// **2026-04-26 監査 Q1.3 修正**:
    /// 以前は `Option<f64>` で 0-1 比率と 0-100% が UI/補填経路で混在していた。
    /// `VacancyRatePct` Newtype で **% 単位（0-100）** に固定。
    /// DB 側 `vacancy_rate` カラム（0-1 比率）からの代入は
    /// `VacancyRatePct::from_ratio()` で必ず変換すること。
    pub vacancy_rate_pct: Option<VacancyRatePct>,
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
                .filter_map(|(p, _)| {
                    if set.insert(p.clone()) {
                        Some(p.clone())
                    } else {
                        None
                    }
                })
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

/// ts_turso_counts の初期スナップショットによる「+374%」級の暴走値を抑止する閾値。
///
/// 求人件数の月次/年次変動が ±200% を超える地域は、現実の市場動向ではなく
/// ETL 初期月のサンプリングカバレッジ不安定（feedback_hw_data_scope.md 参照）に
/// 起因する可能性が極めて高いため、None として欠損扱いする。
///
/// D-2 監査 Q1.2 対応:
/// - 経済指標（CPI 等）の典型的な月次/年次変動は ±20% 以内に収まる
/// - +374% のような値は ts_turso_counts の初期スナップショットでのみ観測される
/// - 値の妥当性検証なしに UI に流すと誤誘導になるため、サニティチェックで除去
pub(crate) const POSTING_CHANGE_SANITY_LIMIT: f64 = 200.0;

/// スナップショット数の最小要件（これ未満では時系列比較不能）
pub(crate) const MIN_SNAPSHOTS_FOR_3M: usize = 4;
pub(crate) const MIN_SNAPSHOTS_FOR_1Y: usize = 13;

/// 値が現実離れしていないかチェック。NaN/Inf も None 化。
///
/// D-2 監査 Q1.2 対応 / feedback_hw_data_scope.md 準拠:
/// - NaN / +Inf / -Inf は None
/// - |value| > 200% は ETL 初期ノイズとして None
pub(crate) fn sanitize_change_pct(value: Option<f64>) -> Option<f64> {
    let v = value?;
    if !v.is_finite() {
        return None;
    }
    if v.abs() > POSTING_CHANGE_SANITY_LIMIT {
        return None;
    }
    Some(v)
}

/// 都道府県単位の 3ヶ月・1年 posting 件数変化率 (%)
///
/// 注意: ts_turso_counts は posting_count（正社員/パート/その他合計）の snapshot を持つ。
/// 最新 snapshot と 3ヶ月前（または1年前）snapshot を比較して変化率を算出。
///
/// D-2 監査 Q1.2 対応:
/// - スナップショット数が不足する場合 (< 4 / < 13) は None
/// - 計算結果の絶対値が ±200% を超える場合は ETL 初期ノイズとみなして None
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
    // スナップショット数が時系列比較に必要な数を満たさない場合は None
    let change_3m = if rows.len() >= MIN_SNAPSHOTS_FOR_3M {
        rows.get(3)
            .map(|r| get_f64(r, "total"))
            .filter(|v| *v > 0.0)
            .map(|prev| (latest - prev) / prev * 100.0)
    } else {
        None
    };
    let change_1y = if rows.len() >= MIN_SNAPSHOTS_FOR_1Y {
        rows.get(12)
            .map(|r| get_f64(r, "total"))
            .filter(|v| *v > 0.0)
            .map(|prev| (latest - prev) / prev * 100.0)
    } else {
        None
    };
    // サニティチェック: 暴走値（±200%超）/ NaN / Inf は ETL 初期ノイズとして除外
    (
        sanitize_change_pct(change_3m),
        sanitize_change_pct(change_1y),
    )
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

    // ========================================================
    // D-2 監査 Q1.2 対応: sanitize_change_pct テスト群
    // feedback_test_data_validation.md / feedback_reverse_proof_tests.md 準拠
    // ========================================================

    /// 暴走値 +374.3% は None になる（実観測された ETL 初期ノイズの再現）
    #[test]
    fn sanitize_rejects_374_percent_runaway_value() {
        let result = sanitize_change_pct(Some(374.3));
        assert_eq!(
            result, None,
            "+374.3% は ETL 初期ノイズとして None 化されるべき"
        );
    }

    /// 暴走値 -90% も同様に None になる（しきい値 200% を超えないが、
    /// 別の問題: ここでは 200% 以内なので Some として通すのが正解）
    #[test]
    fn sanitize_keeps_minus_90_percent_within_limit() {
        // -90% は ±200% 以内なので Some のまま通す（市場崩壊的だが計算上は妥当範囲）
        let result = sanitize_change_pct(Some(-90.0));
        assert_eq!(result, Some(-90.0));
    }

    /// しきい値ちょうど 200% は通す
    #[test]
    fn sanitize_passes_exactly_200_percent() {
        assert_eq!(sanitize_change_pct(Some(200.0)), Some(200.0));
        assert_eq!(sanitize_change_pct(Some(-200.0)), Some(-200.0));
    }

    /// 200.01% は弾く
    #[test]
    fn sanitize_rejects_just_over_200_percent() {
        assert_eq!(sanitize_change_pct(Some(200.01)), None);
        assert_eq!(sanitize_change_pct(Some(-200.01)), None);
    }

    /// NaN / Inf は弾く
    #[test]
    fn sanitize_rejects_nan_and_inf() {
        assert_eq!(sanitize_change_pct(Some(f64::NAN)), None);
        assert_eq!(sanitize_change_pct(Some(f64::INFINITY)), None);
        assert_eq!(sanitize_change_pct(Some(f64::NEG_INFINITY)), None);
    }

    /// 通常値（±15% 等）はそのまま通す
    #[test]
    fn sanitize_passes_normal_values() {
        assert_eq!(sanitize_change_pct(Some(15.5)), Some(15.5));
        assert_eq!(sanitize_change_pct(Some(-7.2)), Some(-7.2));
        assert_eq!(sanitize_change_pct(Some(0.0)), Some(0.0));
    }

    /// None はそのまま None
    #[test]
    fn sanitize_none_passthrough() {
        assert_eq!(sanitize_change_pct(None), None);
    }

    /// 定数値の妥当性検証: しきい値が 200, 最小スナップショットが 4/13
    #[test]
    fn constants_are_documented_values() {
        assert_eq!(POSTING_CHANGE_SANITY_LIMIT, 200.0);
        assert_eq!(MIN_SNAPSHOTS_FOR_3M, 4);
        assert_eq!(MIN_SNAPSHOTS_FOR_1Y, 13);
    }
}
