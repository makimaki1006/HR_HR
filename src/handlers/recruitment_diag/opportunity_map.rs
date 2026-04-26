//! Panel 7: 穴場 vs 激戦マップ（採用診断タブ）
//!
//! 市区町村単位で**採用難度スコア**を算出する：
//!
//! ```text
//! score = HW該当業種求人数 ÷ Agoop昼人口（人/千人） × 1000
//! ```
//!
//! スコアが高いほど「求人数に対して人口（＝潜在求職者母集団）が薄い」＝激戦、
//! 低いほど「競合求人が少ない＝穴場」と解釈する。
//!
//! # 設計原則（MEMORY 遵守）
//!
//! - **HW掲載求人のみ**: `postings` テーブル由来。全求人市場ではない（`feedback_hw_data_scope`）。
//! - **相関≠因果**: スコアはあくまで比率指標。実際の採用成否は別（`feedback_correlation_not_causation`）。
//! - **So What + アクション明示**: category で穴場/標準/激戦 3段階に離散化し、
//!   UI で示唆を出しやすくする（`feedback_hypothesis_driven`）。
//!
//! # 分類閾値
//!
//! Z-score ベースではなく、相対的に安定する**全国一般的な目安**を固定値で採用：
//!
//! - `score < 0.5` → 穴場（求人数が人口千人あたり 0.5件未満）
//! - `0.5 <= score < 2.0` → 標準
//! - `score >= 2.0` → 激戦
//!
//! 閾値は定数として公開し、テストで逆証明する。

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tower_sessions::Session;

use crate::db::local_sqlite::LocalDb;
use crate::db::turso_http::TursoDb;
use crate::AppState;

// ======== 閾値定数（テスト可能にするため pub） ========

/// 穴場判定閾値（人口千人あたり HW求人数）
pub const SCORE_OPPORTUNITY_MAX: f64 = 0.5;

/// 激戦判定閾値（人口千人あたり HW求人数）
pub const SCORE_COMPETITIVE_MIN: f64 = 2.0;

/// 集計結果が極端に少ない市区町村は除外（統計ノイズ防止）
pub const MIN_POPULATION_THRESHOLD: f64 = 1000.0;

// ======== Query Params ========

#[derive(Deserialize, Debug)]
pub struct OpportunityMapParams {
    /// 都道府県コード (1-47) 必須
    pub prefcode: i32,
    /// 職種フィルタ（postings.job_type と部分一致）
    #[serde(default)]
    pub job_type: Option<String>,
    /// 雇用形態フィルタ（postings.employment_type と完全一致）
    /// V2 標準では「正社員」
    #[serde(default)]
    pub emp_type: Option<String>,
}

// ======== Handler ========

/// GET /api/recruitment_diag/opportunity_map?prefcode=13&job_type=医療&emp_type=正社員
pub async fn opportunity_map(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<OpportunityMapParams>,
) -> Json<Value> {
    let db = match &state.hw_db {
        Some(d) => d.clone(),
        None => return Json(error_response("DB未接続")),
    };

    if !(1..=47).contains(&params.prefcode) {
        return Json(error_response(&format!(
            "invalid prefcode: {} (must be 1-47)",
            params.prefcode
        )));
    }

    let prefcode = params.prefcode;
    let pref_name = match prefcode_to_name(prefcode) {
        Some(n) => n.to_string(),
        None => return Json(error_response(&format!("prefcode {} 未対応", prefcode))),
    };
    let job_type = params.job_type.clone();
    let emp_type = params.emp_type.clone();

    let turso = state.turso_db.clone();

    let municipalities = tokio::task::spawn_blocking(move || {
        aggregate_opportunity(
            &db,
            turso.as_ref(),
            &pref_name,
            prefcode,
            job_type.as_deref(),
            emp_type.as_deref(),
        )
    })
    .await
    .unwrap_or_default();

    // 凡例（UI側でレンジ説明表示に使う）
    let legend = json!({
        "opportunity": { "label": "穴場", "max": SCORE_OPPORTUNITY_MAX, "color": "#3b82f6" },
        "standard":    { "label": "標準", "min": SCORE_OPPORTUNITY_MAX, "max": SCORE_COMPETITIVE_MIN, "color": "#f59e0b" },
        "competitive": { "label": "激戦", "min": SCORE_COMPETITIVE_MIN, "color": "#ef4444" },
        "unit": "人口千人あたりHW求人数",
    });

    Json(json!({
        "prefcode": prefcode,
        "filters": {
            "job_type": params.job_type,
            "emp_type": params.emp_type,
        },
        "municipalities": municipalities,
        "legend": legend,
        "note": "HW掲載求人のみ対象（全求人市場ではない）。スコアは比率指標であり因果関係を示すものではありません。",
    }))
}

// ======== 内部集計ロジック ========

/// 1市区町村分の集計結果
#[derive(Debug, Clone)]
struct MuniScore {
    name: String,
    citycode: Option<u32>,
    hw_count: i64,
    population: f64,
    score: f64,
    category: String,
}

fn aggregate_opportunity(
    db: &LocalDb,
    turso: Option<&TursoDb>,
    pref_name: &str,
    prefcode: i32,
    job_type: Option<&str>,
    emp_type: Option<&str>,
) -> Vec<Value> {
    // 1) HW求人件数を市区町村単位で集計
    let hw_map = collect_hw_counts_by_muni(db, pref_name, job_type, emp_type);
    if hw_map.is_empty() {
        return vec![];
    }

    // 2) 昼間人口を市区町村単位で取得
    let pop_map = collect_daytime_pop_by_muni(db, turso, pref_name);

    // 3) 結合＆スコア算出
    let mut result: Vec<MuniScore> = Vec::new();
    for (muni, hw_count) in hw_map.iter() {
        let pop = pop_map.get(muni).copied().unwrap_or(0.0);
        if pop < MIN_POPULATION_THRESHOLD {
            continue;
        }
        let score = (*hw_count as f64) / pop * 1000.0;
        let category = classify_score(score);
        let citycode = crate::geo::city_code::city_name_to_code(pref_name, muni);
        result.push(MuniScore {
            name: muni.clone(),
            citycode,
            hw_count: *hw_count,
            population: pop,
            score,
            category: category.to_string(),
        });
    }

    // スコア降順（激戦が上に）
    result.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let _ = prefcode; // 現状は pref_name でフィルタ、将来 prefcode 連携用に残す
    result
        .iter()
        .map(|m| {
            json!({
                "name": m.name,
                "citycode": m.citycode,
                "hw_count": m.hw_count,
                "population": m.population,
                "score": round3(m.score),
                "category": m.category,
            })
        })
        .collect()
}

/// HW求人件数を (municipality -> count) で取得
fn collect_hw_counts_by_muni(
    db: &LocalDb,
    pref_name: &str,
    job_type: Option<&str>,
    emp_type: Option<&str>,
) -> std::collections::HashMap<String, i64> {
    let mut sql = String::from(
        "SELECT municipality, COUNT(*) as cnt FROM postings \
         WHERE prefecture = ?1 AND municipality IS NOT NULL AND municipality != '' ",
    );
    let mut params: Vec<String> = vec![pref_name.to_string()];
    let mut idx = 2;
    if let Some(jt) = job_type {
        if !jt.is_empty() {
            sql.push_str(&format!("AND job_type LIKE ?{} ", idx));
            params.push(format!("%{}%", jt));
            idx += 1;
        }
    }
    if let Some(et) = emp_type {
        if !et.is_empty() {
            sql.push_str(&format!("AND employment_type = ?{} ", idx));
            params.push(et.to_string());
        }
    }
    sql.push_str("GROUP BY municipality");

    // postings はローカル SQLite のみ（Turso 未同期）。query_turso_or_local は
    // Turso 未定義テーブルで 0 件 → ローカルにフォールバックするため、
    // 第一引数に None を渡してローカル直接参照にする。
    let rows =
        super::super::analysis::fetch::query_turso_or_local(None, db, &sql, &params, "postings");
    let mut map = std::collections::HashMap::new();
    for r in rows {
        let muni = super::super::helpers::get_str(&r, "municipality");
        let cnt = super::super::helpers::get_i64(&r, "cnt");
        if !muni.is_empty() {
            map.insert(muni, cnt);
        }
    }
    map
}

/// 昼間人口を (municipality -> daytime_pop) で取得
fn collect_daytime_pop_by_muni(
    db: &LocalDb,
    turso: Option<&TursoDb>,
    pref_name: &str,
) -> std::collections::HashMap<String, f64> {
    let sql = "SELECT municipality, daytime_pop \
               FROM v2_external_daytime_population \
               WHERE prefecture = ?1 AND municipality IS NOT NULL AND municipality != ''";
    let params = vec![pref_name.to_string()];
    let rows = super::super::analysis::fetch::query_turso_or_local(
        turso,
        db,
        sql,
        &params,
        "v2_external_daytime_population",
    );
    let mut map = std::collections::HashMap::new();
    for r in rows {
        let muni = super::super::helpers::get_str(&r, "municipality");
        let pop = super::super::helpers::get_f64(&r, "daytime_pop");
        if !muni.is_empty() && pop > 0.0 {
            map.insert(muni, pop);
        }
    }
    map
}

/// スコアを穴場/標準/激戦に分類
pub fn classify_score(score: f64) -> &'static str {
    if score < SCORE_OPPORTUNITY_MAX {
        "穴場"
    } else if score < SCORE_COMPETITIVE_MIN {
        "標準"
    } else {
        "激戦"
    }
}

fn prefcode_to_name(prefcode: i32) -> Option<&'static str> {
    // 1-47 → 名称
    let map = crate::geo::pref_name_to_code();
    let target = format!("{:02}", prefcode);
    for (name, code) in map.iter() {
        if *code == target.as_str() {
            return Some(name);
        }
    }
    None
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn error_response(msg: &str) -> Value {
    json!({
        "error": msg,
        "note": "HW掲載求人のみ対象（全求人市場ではない）。",
    })
}

// ======== テスト ========

#[cfg(test)]
mod tests {
    use super::*;

    /// 分類境界を逆証明：閾値ちょうど・閾値の前後で category がどう遷移するか
    #[test]
    fn classify_score_boundaries() {
        // 穴場（score < 0.5）
        assert_eq!(classify_score(0.0), "穴場");
        assert_eq!(classify_score(0.1), "穴場");
        assert_eq!(classify_score(0.49999), "穴場");

        // 標準（0.5 <= score < 2.0）
        assert_eq!(classify_score(SCORE_OPPORTUNITY_MAX), "標準"); // 境界は標準側
        assert_eq!(classify_score(0.5), "標準");
        assert_eq!(classify_score(1.0), "標準");
        assert_eq!(classify_score(1.99999), "標準");

        // 激戦（score >= 2.0）
        assert_eq!(classify_score(SCORE_COMPETITIVE_MIN), "激戦"); // 境界は激戦側
        assert_eq!(classify_score(2.0), "激戦");
        assert_eq!(classify_score(5.0), "激戦");
        assert_eq!(classify_score(100.0), "激戦");
    }

    /// 閾値の大小関係が保たれていることを確認（仕様崩れ防止）
    #[test]
    fn thresholds_monotonic() {
        assert!(SCORE_OPPORTUNITY_MAX < SCORE_COMPETITIVE_MIN);
        assert!(SCORE_OPPORTUNITY_MAX > 0.0);
        assert!(MIN_POPULATION_THRESHOLD > 0.0);
    }

    /// 具体的なダミー集計の逆証明
    /// hw=10, pop=5000 の場合 score = 10/5000*1000 = 2.0 → 激戦
    /// hw=2,  pop=10000 の場合 score = 2/10000*1000 = 0.2 → 穴場
    /// hw=5,  pop=5000 の場合 score = 5/5000*1000 = 1.0 → 標準
    #[test]
    fn dummy_score_categorization() {
        let s1 = 10.0_f64 / 5000.0 * 1000.0;
        assert!((s1 - 2.0).abs() < 1e-9);
        assert_eq!(classify_score(s1), "激戦");

        let s2 = 2.0_f64 / 10000.0 * 1000.0;
        assert!((s2 - 0.2).abs() < 1e-9);
        assert_eq!(classify_score(s2), "穴場");

        let s3 = 5.0_f64 / 5000.0 * 1000.0;
        assert!((s3 - 1.0).abs() < 1e-9);
        assert_eq!(classify_score(s3), "標準");
    }

    #[test]
    fn prefcode_name_lookup() {
        assert_eq!(prefcode_to_name(13), Some("東京都"));
        assert_eq!(prefcode_to_name(1), Some("北海道"));
        assert_eq!(prefcode_to_name(47), Some("沖縄県"));
        assert_eq!(prefcode_to_name(48), None);
        assert_eq!(prefcode_to_name(0), None);
    }
}
