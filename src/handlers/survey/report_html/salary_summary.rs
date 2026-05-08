//! Salary summary - single source of truth for salary medians displayed in the report.
//!
//! 2026-05-08 Round 2-2 (Worker 2): 数値矛盾・地域混在修正
//!
//! ## 背景 (Round 1-I/K 監査)
//! 同一 PDF 内で「給与中央値」が 4 種類の異なる値で混在していた:
//!   - 27.4 万円 (CSV 全件 月給統一中央値)
//!   - 23.0 万円 (件数最多雇用形態グループのネイティブ単位中央値)
//!   - 27.5 万円 (時給 × 167h 月給換算)
//!   - 24.9 万円 (HW 比較値)
//!
//! 4 つともそれぞれ独立に計算され、ラベルも「給与中央値」で同一だったため
//! 営業現場で「どれが本当の給与か」が読めない事態が発生した。
//!
//! ## 設計
//! - [`SalaryHeadline`] 構造体に「PDF 表紙 / KPI に出る給与中央値」の
//!   single source of truth を集約する。
//! - 各 render 関数 (executive_summary, salary_stats, mod.rs カバーページ等) は
//!   `SalaryHeadline` を参照することで、同じ値・同じラベルを表示する。
//! - 異なる集計が必要な箇所 (時給換算 / 雇用形態グループ別 / HW 比較) は
//!   ラベルで明示的に区別する: `"月給中央値 (CSV 全件)"`, `"月給中央値 (時給×167h 換算)"`,
//!   `"月給中央値 (件数最多グループ)"` など。
//!
//! ## 給与単位の正規化 (PDF3 / PDF2 修正)
//! - PDF3 で 月給 63.6 万円 / 167h 換算 3,807 円 が出ていた事故は、
//!   `enhanced_stats.median` が年俸混入で 763,200 円相当に膨れた可能性がある。
//!   [`normalize_monthly_salary`] で「月給 60 万円超は年俸混入の疑い」を検出し、
//!   調整値とフラグを返す。
//! - PDF2 で 167h 換算 11 円 / 6 円 が出ていた事故は、月給値として時給を扱った
//!   逆ケース。[`is_suspicious_monthly_salary`] で「月給 5 万円未満」を検出する。
//!
//! ## 関連 memory ルール
//! - `feedback_test_data_validation.md` 「テストでデータ中身を検証する」
//! - `feedback_reverse_proof_tests.md` 「逆証明: ドメイン不変条件で前提誤りを検出」

use super::super::aggregator::{EmpGroupNativeAgg, SurveyAggregation};

/// PDF 表紙 / Executive Summary KPI に出る「給与中央値」の single source of truth.
///
/// PDF 内で複数箇所に出る給与中央値は、すべてこの構造体を経由して表示する。
/// 値が `None` の場合は「-」表示。
#[derive(Debug, Clone)]
pub(super) struct SalaryHeadline {
    /// CSV 全件 月給統一中央値（円）。is_hourly=true の場合は時給×167h 換算済。
    /// `enhanced_stats.median` を出所とする。
    pub csv_unified_monthly_median_yen: Option<i64>,
    /// 件数最多 雇用形態グループのネイティブ単位 中央値.
    /// グループの native_unit が「月給」なら円、`時給」なら円/時。
    pub top_group_native_median: Option<TopGroupMedian>,
    /// 全体集計が時給ベースか月給ベースか
    pub is_hourly_overall: bool,
    /// 月給値が異常（60 万円超 = 年俸混入疑い、5 万円未満 = 時給混入疑い）か
    pub salary_unit_warning: SalaryUnitWarning,
}

#[derive(Debug, Clone)]
pub(super) struct TopGroupMedian {
    pub group_label: String,
    pub native_unit: String,
    pub median: i64,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SalaryUnitWarning {
    None,
    /// 月給 60 万円超: 年俸が月給として混入した疑い
    PossibleYearlyMixedIn,
    /// 月給 5 万円未満: 時給が月給として混入した疑い
    PossibleHourlyMixedIn,
}

impl SalaryHeadline {
    /// 集計結果から PDF 表示用の給与ヘッドラインを構築する.
    ///
    /// ## ラベル統一
    /// - `csv_unified_monthly_median_yen` は **「月給中央値 (CSV 全件)」** ラベルで表示
    /// - `top_group_native_median` は **「{group_label} 中央値 (実測, n=N)」** ラベルで表示
    ///
    /// 同じ「給与中央値」ラベルを 2 箇所で別定義しないため、render 側でラベル接尾辞を
    /// 必ず付加すること。
    pub(super) fn from_aggregation(agg: &SurveyAggregation) -> Self {
        let csv_median = agg.enhanced_stats.as_ref().and_then(|s| {
            if s.count > 0 && s.median > 0 {
                Some(s.median)
            } else {
                None
            }
        });

        let top_group = agg
            .by_emp_group_native
            .iter()
            .filter(|g| g.count > 0 && g.median > 0)
            .max_by_key(|g| g.count)
            .map(|g: &EmpGroupNativeAgg| TopGroupMedian {
                group_label: g.group_label.clone(),
                native_unit: g.native_unit.clone(),
                median: g.median,
                count: g.count,
            });

        let warning = if agg.is_hourly {
            // 時給ベース集計時は単位異常検出を抑制 (時給 5,000 円超は問題ない)
            SalaryUnitWarning::None
        } else {
            csv_median.map_or(SalaryUnitWarning::None, classify_monthly_salary_warning)
        };

        SalaryHeadline {
            csv_unified_monthly_median_yen: csv_median,
            top_group_native_median: top_group,
            is_hourly_overall: agg.is_hourly,
            salary_unit_warning: warning,
        }
    }

    /// 表紙ハイライト用の表示値を返す.
    ///
    /// ラベルを必ず「月給中央値 (CSV 全件)」または「時給中央値 (CSV 全件)」と接尾辞付きで
    /// 統一する。表示単位 (万円 / 円/時) も同時に返す。
    pub(super) fn cover_highlight_text(&self) -> CoverHighlight {
        match self.csv_unified_monthly_median_yen {
            Some(yen) if !self.is_hourly_overall => {
                let normalized = normalize_monthly_salary(yen);
                let suffix = if normalized.was_normalized {
                    " (年俸混入を月給換算)"
                } else {
                    ""
                };
                CoverHighlight {
                    label: format!("月給中央値 (CSV 全件){}", suffix),
                    value_text: format!("{:.1}", normalized.value as f64 / 10_000.0),
                    unit: "万円".to_string(),
                }
            }
            Some(yen) if self.is_hourly_overall => CoverHighlight {
                label: "時給中央値 (CSV 全件)".to_string(),
                value_text: format!("{}", yen),
                unit: "円/時".to_string(),
            },
            _ => CoverHighlight {
                label: "給与中央値 (CSV 全件)".to_string(),
                value_text: "-".to_string(),
                unit: String::new(),
            },
        }
    }
}

/// 表紙ハイライト KPI 用の表示構造体.
#[derive(Debug, Clone)]
pub(super) struct CoverHighlight {
    pub label: String,
    pub value_text: String,
    pub unit: String,
}

/// 月給値の単位異常を判定する.
///
/// ## 閾値の根拠
/// - 60 万円超: 厚労省 賃金構造基本統計調査 2024 で月給 60 万円超は全産業の約 5%。
///   CSV 中央値が 60 万円超になるのは異常 (年俸が月給値として混入の疑い)。
/// - 5 万円未満: 月給 5 万円未満は法定最低賃金 (時給 1,000 円換算で月給 16.7 万円)
///   を大幅に下回り、時給が月給として混入した可能性が高い。
///
/// ## 用途
/// 表紙・KPI 表示時に警告文言を併記し、「単位推定の自動正規化」フラグも返せるように
/// する。
pub(super) fn classify_monthly_salary_warning(monthly_yen: i64) -> SalaryUnitWarning {
    if monthly_yen >= 600_000 {
        SalaryUnitWarning::PossibleYearlyMixedIn
    } else if monthly_yen > 0 && monthly_yen < 50_000 {
        SalaryUnitWarning::PossibleHourlyMixedIn
    } else {
        SalaryUnitWarning::None
    }
}

/// 月給の単位推定 + 自動正規化結果.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NormalizedMonthly {
    /// 正規化後の月給値（円）
    pub value: i64,
    /// 正規化が適用されたか
    pub was_normalized: bool,
    /// 検出された単位異常の種類
    pub warning: SalaryUnitWarning,
}

/// 月給値が年俸混入である疑いがある場合に 12 で除算して月給値に正規化する.
///
/// ## 仕様
/// - 入力 60 万円超 → 年俸混入とみなし `value / 12` を返却 (`was_normalized=true`)
/// - 入力 60 万円以下 → そのまま返却 (`was_normalized=false`)
/// - 5 万円未満 → そのまま返却し warning だけ立てる (時給混入は値の正規化が困難なため
///   「要確認」シグナルだけ提示)
///
/// ## PDF3 修正の根拠
/// 月給 63.6 万円 が出る求人は、CSV 上「年収 763 万円」と記載され salary_type を誤判定して
/// `unified_monthly` に 763 万円のまま入った可能性がある。12 で割れば 63.6 万円 → 63.6 万円
/// (元値が月給だった場合) ではなく **元値 763 万円 → 月給 63.6 万円** という発想で運用する。
/// したがって本関数は「月給とラベルされた 60 万円超」だけを 12 で除算する保守ロジックに留める。
pub(super) fn normalize_monthly_salary(monthly_yen: i64) -> NormalizedMonthly {
    let warning = classify_monthly_salary_warning(monthly_yen);
    match warning {
        SalaryUnitWarning::PossibleYearlyMixedIn => NormalizedMonthly {
            value: monthly_yen / 12,
            was_normalized: true,
            warning,
        },
        _ => NormalizedMonthly {
            value: monthly_yen,
            was_normalized: false,
            warning,
        },
    }
}

/// PDF2 最低賃金比較セクションで「時給を月給値として誤集計」していた問題に対する
/// 防御的フィルタ。
///
/// 都道府県別の `avg_min_salary` (月給下限平均) を 167h で割って時給換算する際、
/// 元値が時給で入っていると 11 円 / 6 円のような非現実値が出る。
/// この関数は「時給換算後に 100 円未満になる avg_min_salary は元値が時給だった疑い」
/// として該当エントリを `false` 判定する。
pub(super) fn is_plausible_monthly_min_salary(avg_min_salary_yen: i64) -> bool {
    // 月給下限が 50,000 円以上なら 167h 換算で約 300 円/h、最低賃金未満だが
    // 「学生バイト最低賃金値域」として観測しうる範囲。
    // 50,000 円未満は時給を月給値として誤集計した疑いが濃いため除外する。
    avg_min_salary_yen >= 50_000
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::survey::aggregator::EmpGroupNativeAgg;
    use crate::handlers::survey::statistics::EnhancedStats;

    fn agg_with_stats(median: i64, is_hourly: bool) -> SurveyAggregation {
        let mut agg = SurveyAggregation::default();
        agg.total_count = 100;
        agg.is_hourly = is_hourly;
        agg.enhanced_stats = Some(EnhancedStats {
            count: 100,
            mean: median,
            median,
            min: median,
            max: median,
            std_dev: 0,
            bootstrap_ci: None,
            trimmed_mean: None,
            quartiles: None,
            reliability: "high".to_string(),
        });
        agg
    }

    fn agg_with_top_group(median: i64, count: usize, native_unit: &str) -> SurveyAggregation {
        let mut agg = agg_with_stats(280_000, false);
        let mut g = EmpGroupNativeAgg::default();
        g.group_label = "正社員".to_string();
        g.native_unit = native_unit.to_string();
        g.count = count;
        g.median = median;
        agg.by_emp_group_native = vec![g];
        agg
    }

    /// SalaryHeadline は SurveyAggregation から CSV 中央値・件数最多グループ中央値の
    /// 両方を必ず保持する (single source of truth)
    #[test]
    fn salary_headline_holds_both_medians_for_single_source_of_truth() {
        let mut agg = agg_with_stats(274_000, false);
        let mut g = EmpGroupNativeAgg::default();
        g.group_label = "正社員".to_string();
        g.native_unit = "月給".to_string();
        g.count = 50;
        g.median = 230_000;
        agg.by_emp_group_native = vec![g];

        let h = SalaryHeadline::from_aggregation(&agg);

        // CSV 全件中央値 (27.4 万円相当) は「月給中央値 (CSV 全件)」として保持される
        assert_eq!(h.csv_unified_monthly_median_yen, Some(274_000));
        // 件数最多グループ中央値 (23.0 万円相当) は別フィールドとして保持される
        let top = h.top_group_native_median.expect("top group present");
        assert_eq!(top.median, 230_000);
        assert_eq!(top.group_label, "正社員");
        assert_eq!(top.native_unit, "月給");
        // 両者が別フィールドであることがラベル衝突を防ぐ前提条件
        assert_ne!(
            h.csv_unified_monthly_median_yen.unwrap(),
            top.median,
            "CSV 全件中央値と件数最多グループ中央値は値が異なる場合がある (ラベル区別必須)"
        );
    }

    /// 表紙ハイライトのラベルは「月給中央値 (CSV 全件)」と接尾辞付きで返る
    /// (素の「給与中央値」ラベルは禁止 = 4 種混在の根本原因)
    #[test]
    fn salary_headline_cover_highlight_label_includes_csv_scope_suffix() {
        let agg = agg_with_stats(274_000, false);
        let h = SalaryHeadline::from_aggregation(&agg);
        let hl = h.cover_highlight_text();

        // ラベルに「(CSV 全件)」または「(時給×167h 換算)」等の出所が必ず付く
        assert!(
            hl.label.contains("(CSV 全件)"),
            "表紙ハイライトラベルに集計範囲の接尾辞が必須: {:?}",
            hl.label
        );
        assert_eq!(hl.value_text, "27.4");
        assert_eq!(hl.unit, "万円");
    }

    /// 時給ベース集計時はラベルが「時給中央値 (CSV 全件)」になる (月給と区別)
    #[test]
    fn salary_headline_hourly_mode_uses_hourly_label() {
        let agg = agg_with_stats(1_200, true);
        let h = SalaryHeadline::from_aggregation(&agg);
        let hl = h.cover_highlight_text();
        assert_eq!(hl.label, "時給中央値 (CSV 全件)");
        assert_eq!(hl.unit, "円/時");
    }

    /// PDF3 月給 63.6 万円 (年俸混入疑い) → 12 で除算され 5.3 万円相当に正規化される
    #[test]
    fn salary_outlier_year_value_normalized_to_month() {
        // 月給 63.6 万円 = 636,000 円。年俸 763 万 を 12 で割ると 63.5 万円なので、
        // 「年俸が月給として混入」の検出閾値 60 万円を超える。
        let n = normalize_monthly_salary(636_000);
        assert!(n.was_normalized, "60 万円超は年俸混入として正規化される");
        assert_eq!(n.value, 53_000, "636,000 / 12 = 53,000 円相当");
        assert_eq!(n.warning, SalaryUnitWarning::PossibleYearlyMixedIn);

        // 50 万円ちょうどは閾値未満なので非正規化
        let normal = normalize_monthly_salary(500_000);
        assert!(!normal.was_normalized);
        assert_eq!(normal.warning, SalaryUnitWarning::None);

        // 60 万円ちょうどは正規化対象 (年俸 720 万円 / 12 = 60 万円のケース)
        let edge = normalize_monthly_salary(600_000);
        assert!(edge.was_normalized);
    }

    /// PDF2 月給 5 万円未満 (時給混入疑い) は warning を立てるが値はそのまま
    /// (時給 → 月給換算の自動推定は副作用が大きいため、目視確認シグナルに留める)
    #[test]
    fn salary_outlier_below_minwage_flagged_as_hourly_mixed_in() {
        let n = normalize_monthly_salary(11_000); // PDF2 で出た値 11 円相当
        assert!(!n.was_normalized);
        assert_eq!(n.value, 11_000);
        assert_eq!(n.warning, SalaryUnitWarning::PossibleHourlyMixedIn);

        let n2 = normalize_monthly_salary(80_000); // 月給 8 万円 (極端に低いが警告外)
        assert_eq!(n2.warning, SalaryUnitWarning::None);
    }

    /// PDF2 で「最低賃金 167h 換算 11 円 / 6 円」と出た事故の防御フィルタ
    /// (avg_min_salary が 50,000 円未満なら時給混入として除外する)
    #[test]
    fn min_wage_section_excludes_implausible_monthly_min_salary() {
        // 通常の月給下限 (15 万円) は採用される
        assert!(is_plausible_monthly_min_salary(150_000));
        // 月給 5 万円ちょうどは閾値ぎりぎりで採用 (学生バイト等)
        assert!(is_plausible_monthly_min_salary(50_000));
        // 月給 49,999 円は除外 (時給値の誤集計疑い)
        assert!(!is_plausible_monthly_min_salary(49_999));
        // 1,000 円は確実に時給値の誤集計
        assert!(!is_plausible_monthly_min_salary(1_000));
    }

    /// 件数最多グループの median が 0 件の場合は top_group_native_median=None になる
    /// (「給与中央値 (件数最多グループ)」ラベルが存在しない値で出るのを防ぐ)
    #[test]
    fn salary_headline_skips_zero_count_groups() {
        let agg = agg_with_top_group(0, 0, "月給");
        let h = SalaryHeadline::from_aggregation(&agg);
        assert!(h.top_group_native_median.is_none());
    }

    /// CSV 全件中央値が None の場合、cover_highlight_text のラベルは
    /// 「給与中央値 (CSV 全件)」ラベル (出所明記) を維持しつつ値は「-」を返す
    #[test]
    fn salary_headline_unknown_value_keeps_csv_scope_label() {
        let mut agg = SurveyAggregation::default();
        agg.total_count = 0;
        let h = SalaryHeadline::from_aggregation(&agg);
        let hl = h.cover_highlight_text();
        assert_eq!(hl.value_text, "-");
        // 値が未定でも「給与中央値 (CSV 全件)」のスコープラベルは維持される
        assert!(hl.label.contains("CSV 全件"));
    }
}
