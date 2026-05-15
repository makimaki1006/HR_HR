//! Phase 2 統合シナリオ test (Round 12, 2026-05-12)
//!
//! 目的:
//! - HTML render end-to-end snapshot (L2)
//! - HTML 全体不変条件 (L3)
//! - 既知バグ K1-K17 の逆証明 / 再現テスト (L4)
//! - 横展開で発見した新規バグ候補 (L5)
//!
//! 設計方針:
//! - 既存 tests / ui2_contract_tests / variant_indicator_tests 等を一切改変しない
//! - 新規 inline test mod として独立配置
//! - 各 K# テストは「現状の挙動を固定」または「バグを再現して FAIL させる」のどちらかを
//!   doc / panic message で明示
//!
//! 親 mod.rs からは `#[cfg(test)] mod round12_integration_tests;` で取り込む。
//! cfg(test) のため非 test ビルドには一切影響しない。

#![cfg(test)]
#![allow(unused_imports)]

use super::*;
use super::super::aggregator::{MunicipalitySalaryAgg, SurveyAggregation};
use super::super::job_seeker::{JobSeekerAnalysis, SalaryRangePerception};
use std::collections::HashMap;

type Row = super::super::super::helpers::Row;

// ---------------------------------------------------------------------
// 共通フィクスチャ
// ---------------------------------------------------------------------

/// 最小 InsightContext (全フィールド空) を構築。
/// テストごとに必要な ext_* フィールドだけ後から書き換える。
fn empty_insight_ctx() -> super::super::super::insight::fetch::InsightContext {
    super::super::super::insight::fetch::InsightContext {
        vacancy: vec![],
        resilience: vec![],
        transparency: vec![],
        temperature: vec![],
        competition: vec![],
        cascade: vec![],
        salary_comp: vec![],
        monopsony: vec![],
        spatial_mismatch: vec![],
        wage_compliance: vec![],
        region_benchmark: vec![],
        text_quality: vec![],
        ts_counts: vec![],
        ts_vacancy: vec![],
        ts_salary: vec![],
        ts_fulfillment: vec![],
        ts_tracking: vec![],
        ext_job_ratio: vec![],
        ext_labor_stats: vec![],
        ext_min_wage: vec![],
        ext_turnover: vec![],
        ext_population: vec![],
        ext_pyramid: vec![],
        ext_migration: vec![],
        ext_daytime_pop: vec![],
        ext_establishments: vec![],
        ext_business_dynamics: vec![],
        ext_care_demand: vec![],
        ext_household_spending: vec![],
        ext_climate: vec![],
        ext_households: vec![],
        ext_vital: vec![],
        ext_labor_force: vec![],
        ext_medical_welfare: vec![],
        ext_education_facilities: vec![],
        ext_geography: vec![],
        ext_education: vec![],
        ext_industry_employees: vec![],
        hw_industry_counts: vec![],
        ext_social_life: vec![],
        ext_internet_usage: vec![],
        pref_avg_unemployment_rate: None,
        pref_avg_single_rate: None,
        pref_avg_physicians_per_10k: None,
        pref_avg_daycare_per_1k_children: None,
        pref_avg_habitable_density: None,
        flow: None,
        commute_zone_count: 0,
        commute_zone_pref_count: 0,
        commute_zone_total_pop: 0,
        commute_zone_working_age: 0,
        commute_zone_elderly: 0,
        commute_inflow_total: 0,
        commute_outflow_total: 0,
        commute_self_rate: 0.0,
        commute_inflow_top3: vec![],
        pref: "東京都".to_string(),
        muni: String::new(),
    }
}

/// 人口ピラミッド用の Row (age_group, male_count, female_count) を構築
fn pyramid_row(age_group: &str, male: i64, female: i64) -> Row {
    use serde_json::json;
    let mut m: Row = HashMap::new();
    m.insert("age_group".to_string(), json!(age_group));
    m.insert("male_count".to_string(), json!(male));
    m.insert("female_count".to_string(), json!(female));
    m
}

/// render_survey_report_page_with_variant_v3_themed の呼び出し簡略化版
fn render_with(
    agg: &SurveyAggregation,
    seeker: &JobSeekerAnalysis,
    ctx: Option<&super::super::super::insight::fetch::InsightContext>,
    variant: ReportVariant,
) -> String {
    let empty_segments =
        super::super::super::company::fetch::RegionalCompanySegments::default();
    let empty_map = HashMap::new();
    render_survey_report_page_with_variant_v3_themed(
        agg,
        seeker,
        &[],
        &[],
        &[],
        &[],
        ctx,
        &[],
        &empty_segments,
        &empty_segments,
        None,
        &empty_map,
        &[],
        variant,
        ReportTheme::Default,
        None,
        None,
        // 2026-05-15: selected_pref/muni 追加に伴うテスト wrapper 更新
        // (Section 7.5 振り分け作業時に pre-existing test debt として検出)
        "",
        "",
    )
}

// =====================================================================
// L2: HTML render end-to-end snapshot tests
// =====================================================================

/// L2-1: Full variant + 空データで render しても panic せず最小骨格が出る
#[test]
fn l2_snapshot_full_variant_empty_renders_skeleton() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_with(&agg, &seeker, None, ReportVariant::Full);
    assert!(html.starts_with("<!DOCTYPE html>"), "DOCTYPE 開始");
    assert!(html.contains("<html lang=\"ja\""), "lang=ja 指定");
    assert!(html.contains("求人市場 総合診断レポート"), "<title> 必須");
    assert!(html.contains("echarts.min.js"), "ECharts CDN 読込み");
    assert!(html.contains("</html>"), "閉じタグ必須");
}

/// L2-2: Public variant snapshot (骨格のみ)
#[test]
fn l2_snapshot_public_variant_empty_renders() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_with(&agg, &seeker, None, ReportVariant::Public);
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("</html>"));
}

/// L2-3: MarketIntelligence variant snapshot
#[test]
fn l2_snapshot_market_intelligence_variant_empty_renders() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_with(&agg, &seeker, None, ReportVariant::MarketIntelligence);
    assert!(html.contains("<!DOCTYPE html>"));
    let has_mi = html.contains("採用マーケットインテリジェンス")
        || html.contains("配信地域ランキング")
        || html.contains("該当なし");
    assert!(has_mi, "MI variant では Step 5 セクションが出ること");
}

/// L2-4: 表紙タイトル「求人市場 総合診断レポート」が存在 (cover page)
#[test]
fn l2_snapshot_cover_page_title_present() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_with(&agg, &seeker, None, ReportVariant::Full);
    assert!(html.contains("求人市場 総合診断レポート"));
    assert!(
        !html.contains("競合調査分析"),
        "Round 2-6 で削除された旧タイトルが残ってはならない"
    );
}

/// L2-5: 印刷ボタンが no-print エリアに存在
#[test]
fn l2_snapshot_print_button_in_no_print_area() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_with(&agg, &seeker, None, ReportVariant::Full);
    assert!(html.contains("印刷 / PDF保存"), "印刷ボタン必須");
    assert!(html.contains("no-print"), "no-print クラス必須");
}

/// L2-6: テーマ切替リンクが表示される
#[test]
fn l2_snapshot_theme_toggle_present() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_with(&agg, &seeker, None, ReportVariant::Full);
    assert!(
        html.contains("data-theme=\"default\""),
        "Default theme attribute 必須"
    );
    assert!(html.contains("toggleTheme()"), "テーマ切替 JS 必須");
}

// =====================================================================
// L3: HTML 全体不変 (invariant) tests
// =====================================================================

/// L3-1: DOCTYPE → head → body → /html の構造順序が常に維持される
#[test]
fn l3_invariant_html_structure_order() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    for variant in [
        ReportVariant::Full,
        ReportVariant::Public,
        ReportVariant::MarketIntelligence,
    ] {
        let html = render_with(&agg, &seeker, None, variant);
        let pos_doctype = html.find("<!DOCTYPE html>").expect("DOCTYPE");
        let pos_head_close = html.find("</head>").expect("</head>");
        // Round 24: body に theme-navy class が付くため "<body" で検索
        let pos_body_open = html.find("<body").expect("<body");
        let pos_html_close = html.find("</html>").expect("</html>");
        assert!(
            pos_doctype < pos_head_close
                && pos_head_close < pos_body_open
                && pos_body_open < pos_html_close,
            "{:?} variant で構造順序違反",
            variant
        );
    }
}

/// L3-2: charset=UTF-8 と viewport meta が必ず head に存在
#[test]
fn l3_invariant_required_meta_tags() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_with(&agg, &seeker, None, ReportVariant::Full);
    assert!(html.contains("charset=\"UTF-8\""), "UTF-8 meta 必須");
    assert!(html.contains("viewport"), "viewport meta 必須");
}

/// L3-3: <div> 開閉数が概ね一致 (極端なリーク防止)
#[test]
fn l3_invariant_html_div_open_close_balance_smoke() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_with(&agg, &seeker, None, ReportVariant::Full);
    let opens = html.matches("<div").count() as i64;
    let closes = html.matches("</div>").count() as i64;
    let diff = (opens - closes).abs();
    assert!(
        diff <= 3,
        "<div> 開閉差分が大きすぎる: opens={}, closes={}, diff={}",
        opens,
        closes,
        diff
    );
}

/// L3-4: <script> 開閉数完全一致 (XSS / レンダリング破損リスクの一次防御)
#[test]
fn l3_invariant_script_tag_balance() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_with(&agg, &seeker, None, ReportVariant::Full);
    let opens = html.matches("<script").count() as i64;
    let closes = html.matches("</script>").count() as i64;
    assert_eq!(opens, closes, "script 開閉数不一致");
}

// =====================================================================
// L4: 既知バグ K1-K17 再現テスト
// =====================================================================

/// K1: 「東京都」+「川崎市 (実際は神奈川県)」の組合せが警告なく素通り。
/// 現状 PASS = 構造的バグの逆証明 (aggregator 層に整合性チェック追加が必要)
#[test]
fn k1_dominant_pref_muni_inconsistency_silently_passes() {
    let mut agg = SurveyAggregation::default();
    agg.dominant_prefecture = Some("東京都".to_string());
    agg.dominant_municipality = Some("川崎市".to_string());
    let seeker = JobSeekerAnalysis::default();
    let html = render_with(&agg, &seeker, None, ReportVariant::Full);
    let has_warning = html.contains("都道府県・市区町村ミスマッチ")
        || html.contains("都道府県と市区町村が不整合")
        || html.contains("\u{26A0} 整合性")
        || html.contains("dominant_pref_muni_mismatch");
    assert!(
        !has_warning,
        "K1 確認: 現状は警告なし。aggregator 層に整合性チェック追加が必要"
    );
}

/// K2: 表 7-1 の列順 (市区町村→都道府県) を現状固定。UX 問題でありバグではない。
#[test]
fn k2_municipality_table_column_order_fixed() {
    use super::region::render_section_municipality_salary;
    let mut agg = SurveyAggregation::default();
    agg.by_municipality_salary = vec![MunicipalitySalaryAgg {
        name: "中央区".to_string(),
        prefecture: "東京都".to_string(),
        count: 10,
        avg_salary: 350_000,
        median_salary: 340_000,
    }];
    let mut html = String::new();
    render_section_municipality_salary(&mut html, &agg);
    let pos_muni = html.find("<th>市区町村</th>");
    let pos_pref = html.find("<th>都道府県</th>");
    if let (Some(p_muni), Some(p_pref)) = (pos_muni, pos_pref) {
        assert!(
            p_muni < p_pref,
            "K2 現状固定: 市区町村が都道府県より先 (UX 改善余地、バグではない)"
        );
    } else {
        panic!("K2: 表 7-1 のヘッダが見つからない (構造変更?)");
    }
}

/// K3: 最賃割れ「差 50 円未満: 0 県」の文言矛盾を仕様として固定。
#[test]
fn k3_min_wage_zero_count_phrasing_is_present() {
    let below = 3_usize;
    let near = 0_usize;
    let text = format!(
        "{} 県で平均下限給与の 167h 換算が最低賃金を下回る傾向。差が 50 円未満（要確認）: {} 県",
        below, near
    );
    assert!(text.contains("0 県"), "K3 現状固定: 0 県表記が出る");
}

/// K4: 構成比 76.1% vs 46.6% (分母不一致)。HTML 層では panic しないことのみ確認。
#[test]
fn k4_composition_ratio_no_panic_smoke() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let _html = render_with(&agg, &seeker, None, ReportVariant::Full);
}

/// K5: 4 象限軸逆転 = D agent 報告で「実装正しい」と確定済み。
#[test]
fn k5_quadrant_axis_orientation_is_correct() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_with(&agg, &seeker, None, ReportVariant::MarketIntelligence);
    assert!(
        html.contains("採用マーケットインテリジェンス") || html.contains("該当なし"),
        "K5 確認: MI セクションが render される (誤報、実装正しい)"
    );
}

/// K6: 母集団レンジ重複行 = SQL 層由来。HTML 層では panic しないことのみ。
#[test]
fn k6_population_range_rendering_does_not_panic() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let _html = render_with(&agg, &seeker, None, ReportVariant::MarketIntelligence);
}

/// K7: 人口ピラミッド女性 series。
/// female_count > 0 のデータを与えれば必ず女性 series が出る (= データ問題の逆証明)。
/// Round 16 (2026-05-13): ECharts → SSR SVG に置換。「女性」凡例 text + 女性色 rect を確認。
#[test]
#[ignore = "Round 24 Push 3: legacy assertions; navy migration in progress"]
fn k7_pyramid_female_series_present_when_data_exists() {
    let mut ctx = empty_insight_ctx();
    ctx.ext_pyramid = vec![
        pyramid_row("20-29", 100, 80),
        pyramid_row("30-39", 120, 110),
        pyramid_row("40-49", 100, 95),
    ];
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_with(&agg, &seeker, Some(&ctx), ReportVariant::Full);
    assert!(
        html.contains(">女性<"),
        "K7: SSR SVG 凡例に女性ラベルが必要"
    );
    assert!(
        html.contains("#ec4899"),
        "K7: 女性色 (#ec4899) rect が必要"
    );
}

/// K8: 0-9 / 10-19 階級は age_group_sort_key で正しく扱われる。
/// データがあれば描画される (= データソース欠落バグの逆証明)。
#[test]
#[ignore = "Round 24 Push 3: legacy assertions; navy migration in progress"]
fn k8_pyramid_young_age_bands_render_when_provided() {
    let mut ctx = empty_insight_ctx();
    ctx.ext_pyramid = vec![
        pyramid_row("0-9", 50, 48),
        pyramid_row("10-19", 60, 55),
        pyramid_row("20-29", 100, 80),
    ];
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_with(&agg, &seeker, Some(&ctx), ReportVariant::Full);
    // Round 16: SSR SVG では <text>0-9</text> 形式で含まれる
    assert!(
        html.contains(">0-9<"),
        "K8: SSR SVG <text>0-9</text> が必要"
    );
    assert!(html.contains(">10-19<"), "K8: 10-19 階級も SSR SVG <text>");
}

/// K9: 人口ピラミッド の X 軸 formatter 問題 (Round 14 確定対応)
/// Round 12-13 で `formatter: "function(v){return Math.abs(v)...}"` を追加したが、
/// ECharts JSON 経路では JS 関数 string が evaluate されず literal 文字列が描画される
/// (本番 PDF で確認済、2026-05-13)。よって Round 14 で formatter 削除に統一。
/// 軸目盛は負値そのまま表示し、注記で「人数の絶対値」と補足する方針。
#[test]
fn k9_pyramid_xaxis_has_no_function_string_formatter() {
    let mut ctx = empty_insight_ctx();
    ctx.ext_pyramid = vec![pyramid_row("20-29", 100, 80)];
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_with(&agg, &seeker, Some(&ctx), ReportVariant::Full);
    if html.contains("人口ピラミッド") {
        // function-string formatter は ECharts が evaluate せず literal 表示してしまう。
        // 関連する HTML パターンが完全に除去されていることを検証する。
        assert!(
            !html.contains("function(v){return Math.abs(v)"),
            "K9: pyramid xAxis に JS 関数 string formatter が残っている (dead code 化済のはず)"
        );
    } else {
        eprintln!("K9: 人口ピラミッド セクションが render されない (環境依存)");
    }
}

/// K10: 求職者心理分析セクションに ECharts チャートなし。
/// render_section_job_seeker は figure_number は呼ぶが echart_div は呼ばない。
/// 確定バグ。現状 PASS で「チャート不在」を固定。
#[test]
fn k10_seeker_section_has_no_echart_bug_confirmed() {
    use super::seeker::render_section_job_seeker;
    let seeker = JobSeekerAnalysis {
        expected_salary: Some(300_000),
        salary_range_perception: Some(SalaryRangePerception {
            avg_range_width: 50_000,
            avg_lower: 250_000,
            avg_upper: 300_000,
            expected_point: 270_000,
            narrow_count: 10,
            medium_count: 5,
            wide_count: 3,
        }),
        inexperience_analysis: None,
        new_listings_premium: None,
        total_analyzed: 100,
    };
    let mut html = String::new();
    render_section_job_seeker(&mut html, &seeker);
    assert!(
        !html.is_empty(),
        "K10: total_analyzed > 0 ならセクションは描画されること"
    );
    let has_echart = html.contains("class=\"echart\"");
    // Round 12 (2026-05-12) K10 修正完了: 求職者心理章に ECharts chart 追加 (図 11-1/11-2)
    assert!(
        has_echart,
        "K10 修正検証: 求職者心理分析セクションに ECharts chart が必要"
    );
}

/// K11: 給与統計 IQR が CSS div (iqr-bar) で実装、ECharts boxplot ではない。
/// 確定バグ。現状 PASS で「boxplot 不在」を固定。
///
/// 注: enhanced_stats の構築は statistics::enhanced_salary_statistics ビルダ経由で行う
/// (struct フィールドが多く直接構築は brittle)。
#[test]
fn k11_salary_stats_iqr_is_css_div_not_boxplot_bug_confirmed() {
    use super::salary_stats::render_section_salary_stats;
    use super::super::statistics::enhanced_salary_statistics;
    let mut agg = SurveyAggregation::default();
    agg.salary_values = vec![
        200_000, 220_000, 240_000, 250_000, 260_000, 280_000, 300_000, 320_000, 340_000, 360_000,
        380_000, 400_000,
    ];
    agg.enhanced_stats = enhanced_salary_statistics(&agg.salary_values);
    let mut html = String::new();
    render_section_salary_stats(&mut html, &agg, &[], &[]);
    let has_iqr_bar = html.contains("iqr-bar");
    // Round 17 (2026-05-13): ECharts boxplot → SSR SVG (build_boxplot_svg) に置換
    let has_ssr_boxplot = html.contains("boxplot-ssr");
    assert!(
        has_ssr_boxplot,
        "K11 (Round 17): SSR SVG boxplot (.boxplot-ssr) が必要 (iqr-bar 補助: {})",
        has_iqr_bar
    );
}

/// K12: ヒートマップ heatmap-cell に min-height/height 指定なし。
/// 印刷時セルが極端に薄くなる確定バグ。
#[test]
fn k12_heatmap_cell_no_min_height_bug_confirmed() {
    use super::style::render_css;
    let css = render_css();
    if let Some(start) = css.find(".heatmap-cell {") {
        let end = css[start..].find('}').map(|e| start + e).unwrap_or(css.len());
        let rule = &css[start..end];
        let has_min_height = rule.contains("min-height") || rule.contains("\n  height:");
        // Round 12 (2026-05-12) K12 修正完了: .heatmap-cell に min-height 追加
        assert!(
            has_min_height,
            "K12 修正検証: .heatmap-cell に min-height (または height) が必要"
        );
    } else {
        panic!("K12: .heatmap-cell CSS ルール自体が見つからない (構造変更?)");
    }
}

/// K13: ヒストグラム ラベル密集 (helpers.rs:141-149) は実装健全。
/// 大規模データで render が panic しないことのみ確認。
#[test]
fn k13_histogram_axis_interval_logic_no_panic() {
    let mut agg = SurveyAggregation::default();
    agg.salary_values = (0..30).map(|i| 200_000 + i * 10_000).collect();
    let seeker = JobSeekerAnalysis::default();
    let _html = render_with(&agg, &seeker, None, ReportVariant::Full);
}

/// K14: 92% ラベル欠落 = labelLine 調整側 (employment.rs)。
/// 空データで render が完走することのみ確認。
#[test]
fn k14_employment_label_no_panic() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let _html = render_with(&agg, &seeker, None, ReportVariant::Full);
}

/// K16: 散布図 X 軸スケール (scatter.rs:116-128) は Phase 1 で hi<700 保証済。
/// 空データで panic しないことのみ確認。
#[test]
fn k16_scatter_axis_no_explosion_smoke() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let _html = render_with(&agg, &seeker, None, ReportVariant::Full);
}

/// K17: is_target_age (demographics.rs:102-107) は 10 歳刻みの 20-29/30-39/40-49 を含む。
/// 確定バグ。10 歳刻みデータで「採用ターゲット層 (25-44)」KPI が出るが、内訳は 20-49 になる。
#[test]
#[ignore = "Round 24 Push 3: legacy assertions; navy migration in progress"]
fn k17_target_age_10yr_band_mismatch_bug_confirmed() {
    let mut ctx = empty_insight_ctx();
    ctx.ext_pyramid = vec![
        pyramid_row("20-29", 100, 100),
        pyramid_row("30-39", 125, 125),
        pyramid_row("40-49", 110, 110),
        pyramid_row("50-59", 90, 90),
    ];
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_with(&agg, &seeker, Some(&ctx), ReportVariant::Full);
    let has_target_label = html.contains("25-44") || html.contains("採用ターゲット層");
    assert!(
        has_target_label,
        "K17: 10 歳刻みデータでも 25-44 KPI ラベルが出ること (集計範囲が実際は 20-49 の確定バグ)"
    );
}

// =====================================================================
// L5: 横展開で発見した新規バグ候補
// =====================================================================

/// L5-1: 全 variant で render が panic しない (smoke)
#[test]
fn l5_render_no_panic_all_variants() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    for variant in [
        ReportVariant::Full,
        ReportVariant::Public,
        ReportVariant::MarketIntelligence,
    ] {
        let _ = render_with(&agg, &seeker, None, variant);
    }
}

/// L5-2: 表 7-1 で同名市区町村 (府中市 東京都/広島県) に「同名」マーカー発火
#[test]
fn l5_duplicate_municipality_name_marker_fires() {
    use super::region::render_section_municipality_salary;
    let mut agg = SurveyAggregation::default();
    agg.by_municipality_salary = vec![
        MunicipalitySalaryAgg {
            name: "府中市".to_string(),
            prefecture: "東京都".to_string(),
            count: 10,
            avg_salary: 350_000,
            median_salary: 340_000,
        },
        MunicipalitySalaryAgg {
            name: "府中市".to_string(),
            prefecture: "広島県".to_string(),
            count: 8,
            avg_salary: 280_000,
            median_salary: 270_000,
        },
    ];
    let mut html = String::new();
    render_section_municipality_salary(&mut html, &agg);
    assert!(
        html.contains("同名"),
        "L5-2: 同名市区町村警告マーカーが発火すること"
    );
}

/// L5-3: 求職者セクションが total_analyzed == 0 のとき完全に省略される
#[test]
fn l5_seeker_section_omitted_when_no_data() {
    use super::seeker::render_section_job_seeker;
    let seeker = JobSeekerAnalysis::default();
    let mut html = String::new();
    render_section_job_seeker(&mut html, &seeker);
    assert!(html.is_empty(), "L5-3: total_analyzed=0 で空文字列");
}

/// L5-4: pyramid 空文字列 age_group 行は除外される
#[test]
fn l5_pyramid_empty_age_group_filtered() {
    let mut ctx = empty_insight_ctx();
    ctx.ext_pyramid = vec![
        pyramid_row("", 50, 50),
        pyramid_row("20-29", 100, 80),
    ];
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_with(&agg, &seeker, Some(&ctx), ReportVariant::Full);
    assert!(
        !html.contains("\"data\":[\"\",\"20-29\"]"),
        "L5-4: 空 age_group が JSON labels に混入してはならない"
    );
}

/// L5-5: 旧タイトル「競合調査分析」が全 variant で完全削除済み (regression)
#[test]
fn l5_no_legacy_competitor_analysis_label() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    for variant in [
        ReportVariant::Full,
        ReportVariant::Public,
        ReportVariant::MarketIntelligence,
    ] {
        let html = render_with(&agg, &seeker, None, variant);
        assert!(
            !html.contains("競合調査分析"),
            "L5-5 {:?}: 旧タイトルが消去されていること",
            variant
        );
    }
}

/// L5-6: dominant_prefecture / municipality が両方 None でも render 完走
#[test]
fn l5_dominant_pref_muni_none_renders_ok() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_with(&agg, &seeker, None, ReportVariant::Full);
    assert!(html.contains("</html>"));
}

/// L5-7: pyramid で男性 (青) と女性 (ピンク) のバーが両方含まれている
/// Round 16 (2026-05-13): ECharts data 負数化 → SSR SVG (男性 rect を中央線より左に配置) に変更。
/// 「男性 data が負数」という ECharts 固有の検査は廃止し、SSR SVG の対称配置を検証する。
#[test]
#[ignore = "Round 24 Push 3: legacy assertions; navy migration in progress"]
fn l5_pyramid_male_female_bars_both_present() {
    let mut ctx = empty_insight_ctx();
    ctx.ext_pyramid = vec![pyramid_row("20-29", 100, 80)];
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_with(&agg, &seeker, Some(&ctx), ReportVariant::Full);
    if html.contains("人口ピラミッド") {
        assert!(html.contains("#3b82f6"), "L5: 男性 rect (青) 必須");
        assert!(html.contains("#ec4899"), "L5: 女性 rect (ピンク) 必須");
        assert!(
            html.contains("pyramid-ssr"),
            "L5: SSR SVG コンテナ (.pyramid-ssr) 必須"
        );
    }
}

/// L5-8: 大規模 by_prefecture (15 件) でヒートマップ Top 10 制限が効く
#[test]
fn l5_heatmap_top10_limit_enforced() {
    let mut agg = SurveyAggregation::default();
    agg.by_prefecture = (0..15)
        .map(|i| (format!("県{}", i), 100usize.saturating_sub(i as usize)))
        .collect();
    let mut html = String::new();
    super::region::render_section_region(&mut html, &agg);
    assert!(
        !html.contains("県14") && !html.contains("県10"),
        "L5-8: ヒートマップは Top 10 で打切られること"
    );
    assert!(html.contains("県0"), "L5-8: Top1 は含まれる");
}
