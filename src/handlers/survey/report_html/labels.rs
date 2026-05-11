//! Centralized label constants to prevent duplicate labels with different definitions.
//!
//! 2026-05-08 Round 2-2 (Worker 2): 数値矛盾・地域混在修正
//!
//! ## 背景 (Round 1-K 監査)
//! 「重点配信候補 (S+A)」と「配信検証候補 (スコア160+)」が PDF 内で同じ「重点配信候補」
//! ラベルで 2 箇所に出力され、数値が異なる (0 件 vs 11 件) ため読者が矛盾と誤読した。
//! priority カテゴリ (S/A/B/C/D) と合成 score (0-200) は独立軸であり、本来別ラベル.
//!
//! ## Round 10 Phase 1B 後 (2026-05-11) の閾値変更
//! Round 9 P2-G でスコアスケールが 0..200 に統一され、KPI 「配信検証候補」の閾値が
//! 80 → 160 に変更された。本ファイルの定数も合わせて改名・文言更新済み.
//!
//! ## 設計
//! - 各ラベルを定数化することで、PDF 内で 2 箇所に同じラベル文字列が出るのを防ぐ.
//! - 単体テストで「ラベル文字列が衝突しない」「定義が一意」であることを保証.
//!
//! ## 関連 memory ルール
//! - `feedback_unit_consistency_audit.md` 「単位の一貫性監査」と同質の問題.
//!   「ラベルの一貫性監査」もコードベース全体で 1 つに統一する.

/// 配信候補に関するラベル定数群.
pub(super) mod distribution_candidates {
    /// ヒーロー Card 1: priority カテゴリ S および A の市区町村数.
    /// 算出: `recruiting_scores.iter().filter(|s| priority IN ('S','A')).count()`
    pub const PRIORITY_SA_LABEL: &str = "重点配信候補 (S + A)";

    /// KPI: 合成スコア distribution_priority_score >= 160 の市区町村数 (Round 9 P2-G で 80→160 統一).
    /// 算出: `recruiting_scores.iter().filter(|s| s.distribution_priority_score >= 160.0).count()`
    pub const SCORE_160_PLUS_LABEL: &str = "配信検証候補 (スコア160+)";
    pub const SCORE_160_PLUS_TITLE: &str = "配信検証候補";
    pub const SCORE_160_PLUS_UNIT: &str = "件 (スコア160+)";

    /// PDF 内補注: 2 つのラベルの違いを 1 行で説明する文.
    pub const DISTINCTION_NOTE: &str =
        "重点配信候補 (S+A) はカテゴリ分類、配信検証候補 (スコア160+) は合成スコアによる別軸の集計です。";
}

/// 給与ヘッドライン用ラベル.
/// SalaryHeadline と組み合わせて、PDF 内で「給与中央値」が複数値出ても
/// 必ず接尾辞で集計範囲が区別されるようにする.
pub(super) mod salary_labels {
    /// CSV 全件 月給統一中央値 (enhanced_stats.median 由来).
    pub const CSV_ALL_MONTHLY: &str = "月給中央値 (CSV 全件)";
    /// CSV 全件 時給統一中央値 (is_hourly=true 時).
    pub const CSV_ALL_HOURLY: &str = "時給中央値 (CSV 全件)";
    /// 件数最多 雇用形態グループのネイティブ単位中央値.
    pub const TOP_GROUP_NATIVE: &str = "中央値 (件数最多グループ・実測)";
    /// HW 比較用の市場中央値.
    pub const HW_MARKET: &str = "月給中央値 (HW 市場参考値)";
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 「重点配信候補 (S+A)」と「配信検証候補 (スコア160+)」のラベルが一意で
    /// 衝突しない (Round 1-K の根本原因対策の逆証明)
    #[test]
    fn priority_sa_vs_score160_have_distinct_labels() {
        let sa = distribution_candidates::PRIORITY_SA_LABEL;
        let score = distribution_candidates::SCORE_160_PLUS_LABEL;
        let score_title = distribution_candidates::SCORE_160_PLUS_TITLE;
        let score_unit = distribution_candidates::SCORE_160_PLUS_UNIT;
        assert_ne!(
            sa, score,
            "PRIORITY_SA と SCORE_160_PLUS のラベルは別文字列"
        );
        // 一方が他方の prefix にもなっていないこと (PDF 内の grep を考慮)
        assert!(!sa.contains(score));
        assert!(!score.contains(sa));
        // 「重点配信候補 (S + A)」と「配信検証候補」が PDF grep で区別可能
        assert!(sa.contains("S") && sa.contains("A"));
        assert!(score.contains("160"));
        assert!(score.contains(score_title));
        assert!(score_unit.contains("160"));
    }

    /// 給与系ラベルがすべて異なる (PDF 内で 4 種類が衝突しないことを保証)
    #[test]
    fn salary_labels_are_distinct() {
        let labels = [
            salary_labels::CSV_ALL_MONTHLY,
            salary_labels::CSV_ALL_HOURLY,
            salary_labels::TOP_GROUP_NATIVE,
            salary_labels::HW_MARKET,
        ];
        let mut sorted: Vec<&&str> = labels.iter().collect();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            labels.len(),
            "給与中央値の表示ラベルは集計範囲ごとに異なる文字列で区別される"
        );
        // 「給与中央値」だけの素ラベルが含まれないこと (4 種混在の根本原因)
        for l in &labels {
            assert!(
                l.contains("(") && l.contains(")"),
                "給与ラベルは必ず括弧書きで集計範囲を明記する: {}",
                l
            );
        }
    }
}
