//! 採用診断タブ (Recruitment Diagnostics)
//!
//! HW利用企業の人事/採用コンサル向け統合ダッシュボード。
//! 業種 × エリア × 自社条件を入力 → 全データ源横断の採用戦略レポートを表示する。
//!
//! # ルート
//! - `GET /tab/recruitment_diag` → タブ骨格HTML（HTMX swap target）
//! - `GET /api/recruitment_diag/difficulty` → Panel 1: 採用難度スコア（A担当）
//! - `GET /api/recruitment_diag/talent_pool` → Panel 2: 人材プール診断（A担当）
//! - `GET /api/recruitment_diag/inflow`     → Panel 3: 流入元分析（A担当）
//!
//! Panel 4-8 は他 Agent が同じ `/api/recruitment_diag/*` 名前空間で実装。
//!
//! # データ源
//! - `postings` (ローカル SQLite): HW求人 469k件
//! - `v2_flow_mesh1km_YYYY` (Turso): Agoop 滞在人口
//! - `v2_flow_fromto_city` (Turso): Agoop 流入元 4区分（83%投入済）
//! - `v2_external_*` (Turso): 外部統計
//! - `v2_hw_timeseries_*` (Turso): HW時系列
//! - `salesnow_*` (Turso): SalesNow 198K 社
//!
//! # MEMORY 遵守
//! - `feedback_hw_data_scope.md`: HW注意書きを全レスポンスに含める
//! - `feedback_correlation_not_causation.md`: 「傾向」「可能性」に留める
//! - `feedback_hypothesis_driven.md`: So What + アクション提案を返す

mod fetch;
mod handlers;
mod render;

#[cfg(test)]
mod contract_tests;

// Panel 4-6 (担当B)
pub mod competitors;
pub mod condition_gap;
pub mod market_trend;

// Panel 7-8 (担当C)
pub mod insights;
pub mod opportunity_map;

pub use handlers::{
    api_difficulty_score, api_inflow_analysis, api_talent_pool, tab_recruitment_diag,
};

// Panel 4-6 ハンドラ (担当B)
pub use competitors::competitors as api_competitors;
pub use condition_gap::condition_gap as api_condition_gap;
pub use market_trend::market_trend as api_market_trend;

// Panel 7-8 ハンドラ (担当C)
pub use insights::insights as api_insights;
pub use opportunity_map::opportunity_map as api_opportunity_map;

/// 採用診断 API 全体で返す HW データ範囲に関する標準注意書き。
/// （`feedback_hw_data_scope.md` 遵守）
pub(crate) const HW_SCOPE_NOTE: &str =
    "本分析は HW（ハローワーク）掲載求人のみを対象とする。HW非掲載の求人・民間媒体を含まないため、\
     全求人市場の実態を示すものではない。";

/// 示唆の因果関係に関する標準注意書き。
/// （`feedback_correlation_not_causation.md` 遵守）
pub(crate) const CAUSATION_NOTE: &str =
    "本スコア・示唆は相関に基づく『傾向』の提示であり、因果関係を保証するものではない。";

/// 3 統合雇用形態 → postings.employment_type IN 句の内訳。
///
/// - `正社員`      → `"正社員"`
/// - `パート`      → `"パート労働者"`, `"有期雇用派遣パート"`, `"無期雇用派遣パート"`
/// - `その他`      → `"正社員以外"`, `"派遣"` 他
///
/// UI からは 3 区分で受け取り、本関数で DB 実値群に展開する。
pub(crate) fn expand_employment_type(ui_value: &str) -> Vec<&'static str> {
    match ui_value {
        "正社員" => vec!["正社員"],
        "パート" => vec!["パート労働者", "有期雇用派遣パート", "無期雇用派遣パート"],
        "その他" => vec!["正社員以外", "派遣", "契約社員"],
        _ => vec![], // 空 = 全雇用形態
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hw_scope_note_nonempty() {
        assert!(!HW_SCOPE_NOTE.is_empty());
        assert!(HW_SCOPE_NOTE.contains("HW"));
    }

    #[test]
    fn expand_employment_regular() {
        // 正社員 → 正社員のみ
        let v = expand_employment_type("正社員");
        assert_eq!(v, vec!["正社員"]);
    }

    #[test]
    fn expand_employment_part() {
        // パート → 3 種類に展開（派遣パート含む）
        let v = expand_employment_type("パート");
        assert_eq!(v.len(), 3);
        assert!(v.contains(&"パート労働者"));
        assert!(v.contains(&"有期雇用派遣パート"));
        assert!(v.contains(&"無期雇用派遣パート"));
    }

    #[test]
    fn expand_employment_other() {
        // その他 → 正社員以外＋派遣
        let v = expand_employment_type("その他");
        assert!(v.contains(&"正社員以外"));
        assert!(v.contains(&"派遣"));
    }

    #[test]
    fn expand_employment_empty() {
        // 空文字 → 空 Vec（= 全雇用形態フィルタなし）
        assert!(expand_employment_type("").is_empty());
        assert!(expand_employment_type("不明な値").is_empty());
    }
}
