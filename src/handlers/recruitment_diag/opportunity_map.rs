//! Panel 7: 穴場 vs 激戦マップ（採用診断タブ）
//!
//! 市区町村単位で**採用難度スコア**を算出する：
//!
//! ```text
//! score = HW該当業種求人数 ÷ 昼間人口（国勢調査 昼夜間人口集計） × 10,000（人口1万人あたり）
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
//! - `score < 5.0` → 穴場（求人数が人口1万人あたり 5件未満）
//! - `5.0 <= score < 20.0` → 標準
//! - `score >= 20.0` → 激戦
//!
//! 分母は `v2_external_daytime_population` の昼間人口で、Panel 1 (Agoop 人流) とは出典が異なる。
//!
//! 閾値は定数として公開し、テストで逆証明する。

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_sessions::Session;
use ts_rs::TS;

use crate::db::local_sqlite::LocalDb;
use crate::db::turso_http::TursoDb;
use crate::AppState;

pub(crate) use super::insights::RdErrorNote;

// ======== Response Types ========

/// リクエストで受け取った絞り込み条件 (表示用にそのまま返す)。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdOpportunityFilters {
    /// 職種 (未指定なら null)。
    pub job_type: Option<String>,
    /// 雇用形態 (未指定なら null)。
    pub emp_type: Option<String>,
}

/// 市区町村 1 件分のスコア。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdOpportunityMunicipality {
    /// 市区町村名。
    pub name: String,
    /// 市区町村コード (解決できなければ null)。
    pub citycode: Option<u32>,
    /// HW 求人件数。
    pub hw_count: i64,
    /// 昼間人口 (人)。
    pub population: f64,
    /// 人口 1 万人あたり HW 求人数 (小数第 3 位に丸め)。
    pub score: f64,
    /// 穴場 / 標準 / 激戦。
    pub category: String,
}

/// 凡例: 穴場の区間。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdLegendOpportunity {
    /// 表示名。
    pub label: String,
    /// スコア上限 (この値未満)。
    pub max: f64,
    /// 表示色。
    pub color: String,
}

/// 凡例: 標準の区間。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdLegendStandard {
    /// 表示名。
    pub label: String,
    /// スコア下限 (この値以上)。
    pub min: f64,
    /// スコア上限 (この値未満)。
    pub max: f64,
    /// 表示色。
    pub color: String,
}

/// 凡例: 激戦の区間。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdLegendCompetitive {
    /// 表示名。
    pub label: String,
    /// スコア下限 (この値以上)。
    pub min: f64,
    /// 表示色。
    pub color: String,
}

/// 凡例 (レンジ説明用)。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdOpportunityLegend {
    /// 穴場。
    pub opportunity: RdLegendOpportunity,
    /// 標準。
    pub standard: RdLegendStandard,
    /// 激戦。
    pub competitive: RdLegendCompetitive,
    /// スコアの単位表記。
    pub unit: String,
}

impl RdOpportunityLegend {
    fn standard_legend() -> Self {
        Self {
            opportunity: RdLegendOpportunity {
                label: "穴場".to_string(),
                max: SCORE_OPPORTUNITY_MAX,
                color: "#3b82f6".to_string(),
            },
            standard: RdLegendStandard {
                label: "標準".to_string(),
                min: SCORE_OPPORTUNITY_MAX,
                max: SCORE_COMPETITIVE_MIN,
                color: "#f59e0b".to_string(),
            },
            competitive: RdLegendCompetitive {
                label: "激戦".to_string(),
                min: SCORE_COMPETITIVE_MIN,
                color: "#ef4444".to_string(),
            },
            unit: "人口1万人あたりHW求人数".to_string(),
        }
    }
}

/// Panel 7 成功時の本体。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdOpportunityMapResponse {
    /// 都道府県コード (1-47)。
    pub prefcode: i32,
    /// 受け取った絞り込み条件。
    pub filters: RdOpportunityFilters,
    /// 市区町村ごとのスコア (スコア降順)。
    pub municipalities: Vec<RdOpportunityMunicipality>,
    /// 凡例。
    pub legend: RdOpportunityLegend,
    /// HW 範囲・因果の注記。
    pub note: String,
}

/// Panel 7 のレスポンス。TS では `RdOpportunityMapResponse | RdErrorNote`。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(untagged)]
pub enum RdOpportunityMapResult {
    Ok(RdOpportunityMapResponse),
    Err(RdErrorNote),
}

// ======== 閾値定数（テスト可能にするため pub） ========

/// スコアの人口単位 (人口 1 万人あたり)
pub const SCORE_PER_POPULATION: f64 = 10_000.0;

/// 穴場判定閾値（人口 1 万人あたり HW求人数）
pub const SCORE_OPPORTUNITY_MAX: f64 = 5.0;

/// 激戦判定閾値（人口 1 万人あたり HW求人数）
pub const SCORE_COMPETITIVE_MIN: f64 = 20.0;

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
) -> Json<RdOpportunityMapResult> {
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

    Json(RdOpportunityMapResult::Ok(build_response(
        prefcode,
        params.job_type,
        params.emp_type,
        municipalities,
    )))
}

/// 成功本体を組み立てる。
fn build_response(
    prefcode: i32,
    job_type: Option<String>,
    emp_type: Option<String>,
    municipalities: Vec<RdOpportunityMunicipality>,
) -> RdOpportunityMapResponse {
    RdOpportunityMapResponse {
        prefcode,
        filters: RdOpportunityFilters { job_type, emp_type },
        municipalities,
        // 凡例（UI側でレンジ説明表示に使う）
        legend: RdOpportunityLegend::standard_legend(),
        note: "HW掲載求人のみ対象（全求人市場ではない）。スコアは比率指標であり因果関係を示すものではありません。".to_string(),
    }
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
) -> Vec<RdOpportunityMunicipality> {
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
        let score = compute_score(*hw_count, pop);
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
        .map(|m| RdOpportunityMunicipality {
            name: m.name.clone(),
            citycode: m.citycode,
            hw_count: m.hw_count,
            population: m.population,
            score: round3(m.score),
            category: m.category.clone(),
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

/// スコア = HW 求人数 ÷ 昼間人口 × 10,000 (人口 1 万人あたり。Panel 1 と同じ倍率)
pub fn compute_score(hw_count: i64, population: f64) -> f64 {
    (hw_count as f64) * SCORE_PER_POPULATION / population
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

fn error_response(msg: &str) -> RdOpportunityMapResult {
    RdOpportunityMapResult::Err(RdErrorNote::new(msg))
}

// ======== テスト ========

#[cfg(test)]
mod tests {
    use super::*;

    /// 分類境界を逆証明：閾値ちょうど・閾値の前後で category がどう遷移するか
    /// (スコアは人口 1 万人あたりの HW 求人数)
    #[test]
    fn classify_score_boundaries() {
        assert_eq!(SCORE_OPPORTUNITY_MAX, 5.0);
        assert_eq!(SCORE_COMPETITIVE_MIN, 20.0);

        // 穴場（score < 5.0）
        assert_eq!(classify_score(0.0), "穴場");
        assert_eq!(classify_score(1.0), "穴場");
        assert_eq!(classify_score(4.9999), "穴場");

        // 標準（5.0 <= score < 20.0）
        assert_eq!(classify_score(SCORE_OPPORTUNITY_MAX), "標準"); // 境界は標準側
        assert_eq!(classify_score(5.0), "標準");
        assert_eq!(classify_score(10.0), "標準");
        assert_eq!(classify_score(19.9999), "標準");

        // 激戦（score >= 20.0）
        assert_eq!(classify_score(SCORE_COMPETITIVE_MIN), "激戦"); // 境界は激戦側
        assert_eq!(classify_score(20.0), "激戦");
        assert_eq!(classify_score(50.0), "激戦");
        assert_eq!(classify_score(1000.0), "激戦");
    }

    /// 閾値の大小関係が保たれていることを確認（仕様崩れ防止）
    #[test]
    fn thresholds_monotonic() {
        assert!(SCORE_OPPORTUNITY_MAX < SCORE_COMPETITIVE_MIN);
        assert!(SCORE_OPPORTUNITY_MAX > 0.0);
        assert!(MIN_POPULATION_THRESHOLD > 0.0);
    }

    /// 具体的なダミー集計の逆証明 (人口 1 万人あたり)
    /// hw=10, pop=5000 の場合 score = 10/5000*10000 = 20.0 → 激戦
    /// hw=2,  pop=10000 の場合 score = 2/10000*10000 = 2.0 → 穴場
    /// hw=5,  pop=5000 の場合 score = 5/5000*10000 = 10.0 → 標準
    #[test]
    fn dummy_score_categorization() {
        let s1 = compute_score(10, 5000.0);
        assert!((s1 - 20.0).abs() < 1e-9, "s1={s1}");
        assert_eq!(classify_score(s1), "激戦");

        let s2 = compute_score(2, 10000.0);
        assert!((s2 - 2.0).abs() < 1e-9, "s2={s2}");
        assert_eq!(classify_score(s2), "穴場");

        let s3 = compute_score(5, 5000.0);
        assert!((s3 - 10.0).abs() < 1e-9, "s3={s3}");
        assert_eq!(classify_score(s3), "標準");
    }

    /// 境界ちょうどの求人・人口: hw=1,pop=2000 → 5.0 (標準) / hw=1,pop=500 → 20.0 (激戦)
    /// とその少し下・上 (旧: 千人あたり 0.5 / 2.0 と同じ求人・人口なら同じ区分)
    #[test]
    fn score_boundary_inputs() {
        assert_eq!(compute_score(1, 2000.0), 5.0);
        assert_eq!(classify_score(compute_score(1, 2000.0)), "標準");
        assert_eq!(classify_score(compute_score(1, 2001.0)), "穴場");
        assert_eq!(classify_score(compute_score(1, 1999.0)), "標準");
        assert_eq!(compute_score(1, 500.0), 20.0);
        assert_eq!(classify_score(compute_score(1, 500.0)), "激戦");
        assert_eq!(classify_score(compute_score(1, 501.0)), "標準");
        assert_eq!(classify_score(compute_score(1, 499.0)), "激戦");
    }

    /// 同じ hw / pop なら新しい区分は、整数比較で表した旧基準
    /// (hw/pop < 1/2000 → 穴場、< 1/500 → 標準、それ以上 → 激戦) と一致する。
    /// score は旧 (千人あたり) のちょうど 10 倍。
    #[test]
    fn category_unchanged_and_score_is_10x_old() {
        for hw in 0_i64..=40 {
            for pop in [
                1000.0_f64, 1001.0, 1250.0, 1999.0, 2000.0, 2001.0, 4000.0, 5000.0, 10000.0,
                20000.0, 100_000.0, 500.0, 499.0, 501.0,
            ] {
                // 旧基準 (千人あたり 0.5 / 2.0) を有理数で: hw/pop*1000 < 0.5 ⇔ hw*2000 < pop
                let old = if (hw as f64) * 2000.0 < pop {
                    "穴場"
                } else if (hw as f64) * 500.0 < pop {
                    "標準"
                } else {
                    "激戦"
                };
                let sc = compute_score(hw, pop);
                assert_eq!(classify_score(sc), old, "hw={hw} pop={pop} score={sc}");
                let old_score = (hw as f64) / pop * 1000.0;
                assert!(
                    (sc - old_score * 10.0).abs() < 1e-9 * (1.0 + sc.abs()),
                    "hw={hw} pop={pop}"
                );
            }
        }
    }

    /// 凡例の単位・しきい値 (人口 1 万人あたり)
    #[test]
    fn legend_is_per_10k() {
        let l = RdOpportunityLegend::standard_legend();
        assert_eq!(l.unit, "人口1万人あたりHW求人数");
        assert_eq!(l.opportunity.max, 5.0);
        assert_eq!(l.standard.min, 5.0);
        assert_eq!(l.standard.max, 20.0);
        assert_eq!(l.competitive.min, 20.0);
    }

    #[test]
    fn prefcode_name_lookup() {
        assert_eq!(prefcode_to_name(13), Some("東京都"));
        assert_eq!(prefcode_to_name(1), Some("北海道"));
        assert_eq!(prefcode_to_name(47), Some("沖縄県"));
        assert_eq!(prefcode_to_name(48), None);
        assert_eq!(prefcode_to_name(0), None);
    }

    // ======== 旧 json!() との等価テスト (Phase 1A-1) ========

    fn legacy_legend() -> serde_json::Value {
        serde_json::json!({
            "opportunity": { "label": "穴場", "max": SCORE_OPPORTUNITY_MAX, "color": "#3b82f6" },
            "standard":    { "label": "標準", "min": SCORE_OPPORTUNITY_MAX, "max": SCORE_COMPETITIVE_MIN, "color": "#f59e0b" },
            "competitive": { "label": "激戦", "min": SCORE_COMPETITIVE_MIN, "color": "#ef4444" },
            "unit": "人口1万人あたりHW求人数",
        })
    }

    fn legacy_muni(m: &MuniScore) -> serde_json::Value {
        serde_json::json!({
            "name": m.name,
            "citycode": m.citycode,
            "hw_count": m.hw_count,
            "population": m.population,
            "score": round3(m.score),
            "category": m.category,
        })
    }

    fn legacy_success(
        prefcode: i32,
        job_type: Option<String>,
        emp_type: Option<String>,
        municipalities: Vec<serde_json::Value>,
    ) -> serde_json::Value {
        serde_json::json!({
            "prefcode": prefcode,
            "filters": {
                "job_type": job_type,
                "emp_type": emp_type,
            },
            "municipalities": municipalities,
            "legend": legacy_legend(),
            "note": "HW掲載求人のみ対象（全求人市場ではない）。スコアは比率指標であり因果関係を示すものではありません。",
        })
    }

    fn legacy_error_response(msg: &str) -> serde_json::Value {
        serde_json::json!({
            "error": msg,
            "note": "HW掲載求人のみ対象（全求人市場ではない）。",
        })
    }

    fn s<T: serde::Serialize>(v: &T) -> String {
        serde_json::to_string(v).unwrap()
    }

    fn scores() -> Vec<MuniScore> {
        let mk = |name: &str, cc: Option<u32>, hw: i64, pop: f64, score: f64| MuniScore {
            name: name.to_string(),
            citycode: cc,
            hw_count: hw,
            population: pop,
            score,
            category: classify_score(score).to_string(),
        };
        vec![
            mk("A市", Some(13101), 10, 5000.0, 20.0),
            mk("B市", None, 0, 12345.5, 0.0),
            mk("C町", Some(0), 7, 1000.0, 12.3456789),
            mk("", None, 1, 99999.99, 4.999),
        ]
    }

    fn to_new(m: &MuniScore) -> RdOpportunityMunicipality {
        RdOpportunityMunicipality {
            name: m.name.clone(),
            citycode: m.citycode,
            hw_count: m.hw_count,
            population: m.population,
            score: round3(m.score),
            category: m.category.clone(),
        }
    }

    #[test]
    fn legend_matches_legacy_json() {
        assert_eq!(
            s(&RdOpportunityLegend::standard_legend()),
            s(&legacy_legend())
        );
    }

    #[test]
    fn municipality_matches_legacy_json() {
        for m in scores() {
            assert_eq!(s(&to_new(&m)), s(&legacy_muni(&m)));
        }
    }

    #[test]
    fn success_matches_legacy_json() {
        let cases: Vec<(i32, Option<&str>, Option<&str>, bool)> = vec![
            (13, Some("医療"), Some("正社員"), true),
            (1, None, None, true),
            (47, Some(""), Some(""), false),
        ];
        for (pc, jt, et, with) in cases {
            let ms = if with { scores() } else { vec![] };
            let new = build_response(
                pc,
                jt.map(String::from),
                et.map(String::from),
                ms.iter().map(to_new).collect(),
            );
            let legacy = legacy_success(
                pc,
                jt.map(String::from),
                et.map(String::from),
                ms.iter().map(legacy_muni).collect(),
            );
            assert_eq!(s(&RdOpportunityMapResult::Ok(new)), s(&legacy));
        }
    }

    #[test]
    fn error_matches_legacy_json() {
        for msg in [
            "DB未接続",
            "invalid prefcode: 0 (must be 1-47)",
            "prefcode 5 未対応",
            "",
        ] {
            assert_eq!(s(&error_response(msg)), s(&legacy_error_response(msg)));
        }
    }

    #[test]
    fn ts_decl_has_expected_fields() {
        let cfg = super::super::types::ts_config();
        let result = RdOpportunityMapResult::decl(&cfg);
        assert!(
            result.contains("RdOpportunityMapResponse | RdErrorNote"),
            "{result}"
        );
        let resp = RdOpportunityMapResponse::decl(&cfg);
        for f in [
            "prefcode: number",
            "filters: RdOpportunityFilters",
            "municipalities: Array<RdOpportunityMunicipality>",
            "legend: RdOpportunityLegend",
            "note: string",
        ] {
            assert!(resp.contains(f), "missing `{f}` in {resp}");
        }
        let m = RdOpportunityMunicipality::decl(&cfg);
        for f in [
            "citycode: number | null",
            "hw_count: number",
            "population: number",
            "score: number",
        ] {
            assert!(m.contains(f), "missing `{f}` in {m}");
        }
    }
}
