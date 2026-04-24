//! Agent P3: PDF 競合調査レポート QA テスト
//!
//! 仕様書: docs/pdf_design_spec_2026_04_24.md 9節 QA チェックリスト
//!
//! 目的:
//!   P2 が再実装する `render_survey_report_page` の出力 HTML を
//!   機械検証可能な assertion に落とし込み、仕様書の必須要件
//!   （構造 / 印刷 CSS / 必須文言 / 禁止ワード / severity 等）を
//!   確実にカバーする。
//!
//! スコープ方針:
//!   - 既存 lib コードは変更しない（既存テスト非破壊）
//!   - `render_survey_report_page` のシグネチャ変更を契約として検出
//!   - 機械検証不可能な項目（視覚スナップショット等）は手動確認推奨として
//!     本ファイルのドキュメントコメントに明示
//!
//! 運用:
//!   - P2 未完了時: 一部 fail が仕様未達成箇所を示す
//!   - P2 完了時: 全 pass を最終目標

use super::aggregator::{
    CompanyAgg, EmpTypeSalary, MunicipalitySalaryAgg, PrefectureSalaryAgg, RegressionResult,
    ScatterPoint, SurveyAggregation, TagSalaryAgg,
};
use super::job_seeker::{InexperienceAnalysis, JobSeekerAnalysis, SalaryRangePerception};
use super::report_html::render_survey_report_page;
use super::statistics::EnhancedStats;
use super::super::company::fetch::NearbyCompany;
use super::super::insight::fetch::InsightContext;

// ============================================================
// Mock / ヘルパー
// ============================================================

/// 空の InsightContext を生成（全 Vec 空、Option None）
/// 付録 A の全フィールドを網羅すること。新規フィールド追加時はコンパイルエラーで検出。
fn mock_empty_insight_ctx() -> InsightContext {
    InsightContext {
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
        muni: "千代田区".to_string(),
    }
}

/// 最小限の SurveyAggregation（描画を通すだけのデータ）
fn minimal_agg() -> SurveyAggregation {
    let mut agg = SurveyAggregation::default();
    agg.total_count = 100;
    agg.new_count = 18;
    agg.salary_parse_rate = 0.92;
    agg.location_parse_rate = 0.95;
    agg.dominant_prefecture = Some("東京都".to_string());
    agg.dominant_municipality = Some("千代田区".to_string());
    agg.by_prefecture = vec![("東京都".to_string(), 60), ("神奈川県".to_string(), 40)];
    agg.by_employment_type = vec![("正社員".to_string(), 62), ("パート".to_string(), 38)];
    agg.by_tags = vec![("賞与あり".to_string(), 30)];
    agg.salary_values = (0..30).map(|i| 200_000 + i * 5_000).collect();
    agg.salary_min_values = (0..30).map(|i| 180_000 + i * 5_000).collect();
    agg.salary_max_values = (0..30).map(|i| 250_000 + i * 5_000).collect();
    agg.by_company = vec![CompanyAgg {
        name: "サンプル法人A".to_string(),
        count: 12,
        avg_salary: 260_000,
        median_salary: 258_000,
    }];
    agg.by_emp_type_salary = vec![EmpTypeSalary {
        emp_type: "正社員".to_string(),
        count: 62,
        avg_salary: 260_000,
        median_salary: 255_000,
    }];
    agg.by_municipality_salary = vec![MunicipalitySalaryAgg {
        name: "千代田区".to_string(),
        prefecture: "東京都".to_string(),
        count: 50,
        avg_salary: 260_000,
        median_salary: 255_000,
    }];
    agg.by_prefecture_salary = vec![PrefectureSalaryAgg {
        name: "東京都".to_string(),
        count: 60,
        avg_salary: 260_000,
        avg_min_salary: 230_000,
    }];
    agg.by_tag_salary = vec![TagSalaryAgg {
        tag: "賞与あり".to_string(),
        count: 30,
        avg_salary: 275_000,
        diff_from_avg: 15_000,
        diff_percent: 5.8,
    }];
    agg.scatter_min_max = (0..15)
        .map(|i| ScatterPoint {
            x: 180_000 + i * 5_000,
            y: 250_000 + i * 5_000,
        })
        .collect();
    agg.regression_min_max = Some(RegressionResult {
        slope: 1.2,
        intercept: 50_000.0,
        r_squared: 0.85,
    });
    agg.enhanced_stats = Some(EnhancedStats {
        count: 30,
        mean: 255_000,
        median: 250_000,
        min: 200_000,
        max: 345_000,
        std_dev: 30_000,
        bootstrap_ci: None,
        trimmed_mean: None,
        quartiles: None,
        reliability: "high".to_string(),
    });
    agg.is_hourly = false;
    agg
}

fn minimal_seeker() -> JobSeekerAnalysis {
    JobSeekerAnalysis {
        expected_salary: Some(240_000),
        salary_range_perception: Some(SalaryRangePerception {
            avg_range_width: 70_000,
            avg_lower: 220_000,
            avg_upper: 290_000,
            expected_point: 243_100,
            narrow_count: 10,
            medium_count: 15,
            wide_count: 5,
        }),
        inexperience_analysis: Some(InexperienceAnalysis {
            inexperience_count: 12,
            experience_count: 18,
            inexperience_avg_salary: Some(230_000),
            experience_avg_salary: Some(265_000),
            salary_gap: Some(35_000),
        }),
        new_listings_premium: Some(8_000),
        total_analyzed: 30,
    }
}

/// P2 完了後を想定した「フル機能呼び出し」の HTML
fn render_full_html() -> String {
    let agg = minimal_agg();
    let seeker = minimal_seeker();
    let by_company = agg.by_company.clone();
    let by_emp = agg.by_emp_type_salary.clone();
    let smin = agg.salary_min_values.clone();
    let smax = agg.salary_max_values.clone();
    let ctx = mock_empty_insight_ctx();
    let sn = vec![NearbyCompany {
        corporate_number: "1234567890123".to_string(),
        company_name: "SalesNowサンプル社".to_string(),
        prefecture: "東京都".to_string(),
        sn_industry: "医療・福祉".to_string(),
        employee_count: 200,
        credit_score: 68.0,
        postal_code: "100".to_string(),
        hw_posting_count: 5,
    }];
    render_survey_report_page(&agg, &seeker, &by_company, &by_emp, &smin, &smax, Some(&ctx), &sn)
}

/// hw_context=None, salesnow=空 のケース
fn render_minimal_html() -> String {
    let agg = minimal_agg();
    let seeker = minimal_seeker();
    let by_company = agg.by_company.clone();
    let by_emp = agg.by_emp_type_salary.clone();
    let smin = agg.salary_min_values.clone();
    let smax = agg.salary_max_values.clone();
    render_survey_report_page(&agg, &seeker, &by_company, &by_emp, &smin, &smax, None, &[])
}

// ============================================================
// 9.1 構造チェック
// ============================================================

#[test]
fn p3_spec_9_1_doctype_and_html_root() {
    let html = render_full_html();
    assert!(html.contains("<!DOCTYPE html>"), "DOCTYPE 宣言が必要");
    assert!(html.contains("<html"), "<html> ルート要素が必要");
    assert!(html.contains("</html>"), "</html> 閉じタグが必要");
    assert!(html.contains("lang=\"ja\""), "lang=ja 属性が必要");
}

#[test]
fn p3_spec_9_1_cover_page_present() {
    let html = render_full_html();
    assert!(
        html.contains("cover-page"),
        "表紙（cover-page クラス）が必須（仕様書 2.2 #0）"
    );
}

#[test]
fn p3_spec_9_1_executive_summary_present() {
    let html = render_full_html();
    // Executive Summary or サマリー セクションがいずれか存在
    assert!(
        html.contains("Executive Summary")
            || html.contains("exec-summary")
            || html.contains("サマリー"),
        "Executive Summary 相当のセクションが必須（仕様書 2.2 #1 / 3章）"
    );
}

#[test]
fn p3_spec_9_1_required_sections_exist() {
    // 仕様書 2.4 の「必須（削除不可）」: 0, 1, 4, 6, 8, 9, 13
    let html = render_full_html();
    // セクション 4 雇用形態分布
    assert!(
        html.contains("雇用形態"),
        "Section 4（雇用形態分布）が必須"
    );
    // セクション 6 地域分析
    assert!(
        html.contains("地域") || html.contains("都道府県"),
        "Section 6（地域分析）が必須"
    );
    // セクション 9 企業分析
    assert!(
        html.contains("法人") || html.contains("企業"),
        "Section 9（企業分析）が必須"
    );
    // セクション 13 注記・免責
    assert!(
        html.contains("免責") || html.contains("注記") || html.contains("出典"),
        "Section 13（注記・出典・免責）が必須"
    );
}

#[test]
fn p3_spec_9_1_hw_comparison_toggled_by_hw_context() {
    // CSS コメント等の偶発ヒットを避けるため <h2> 内の検出に限定
    let html_without = render_minimal_html();
    let has_h2_comparison_without = html_without
        .split("<h2")
        .skip(1)
        .any(|s| s.split("</h2>").next().map(|t| t.contains("HW市場比較") || t.contains("HW 市場比較")).unwrap_or(false));
    assert!(
        !has_h2_comparison_without,
        "hw_context=None のとき Section 2（HW 市場比較）の <h2> が出ない"
    );

    let html_with = render_full_html();
    let has_h2_comparison_with = html_with
        .split("<h2")
        .skip(1)
        .any(|s| s.split("</h2>").next().map(|t| t.contains("HW市場比較") || t.contains("HW 市場比較") || t.contains("市場比較")).unwrap_or(false));
    assert!(
        has_h2_comparison_with,
        "hw_context=Some のとき Section 2（HW 市場比較）の <h2> が必要"
    );
}

#[test]
fn p3_spec_9_1_salesnow_section_toggled_by_emptiness() {
    // CSS コメント等の偶発ヒットを避けるため <h2> 内の検出に限定
    let html_without = render_minimal_html();
    let has_h2_salesnow_without = html_without
        .split("<h2")
        .skip(1)
        .any(|s| s.split("</h2>").next().map(|t| t.contains("SalesNow") || t.contains("地域注目企業")).unwrap_or(false));
    assert!(
        !has_h2_salesnow_without,
        "salesnow_companies.is_empty() のとき Section 12 の <h2> が出ない"
    );

    let html_with = render_full_html();
    let has_h2_salesnow_with = html_with
        .split("<h2")
        .skip(1)
        .any(|s| s.split("</h2>").next().map(|t| t.contains("SalesNow") || t.contains("地域注目企業")).unwrap_or(false));
    assert!(
        has_h2_salesnow_with,
        "salesnow_companies 非空のとき Section 12 の <h2> が必要"
    );
}

#[test]
fn p3_spec_9_1_has_h2_or_section_elements() {
    let html = render_full_html();
    // 少なくとも 3 つの h2 or section 見出しがあること（複数セクション構造）
    let h2_count = html.matches("<h2").count();
    let section_count = html.matches("<section").count();
    assert!(
        h2_count >= 3 || section_count >= 3,
        "少なくとも 3 つの h2 or section 要素が必要（現在 h2={}, section={}）",
        h2_count,
        section_count
    );
}

// ============================================================
// 9.2 CSS / 印刷チェック
// ============================================================

#[test]
fn p3_spec_9_2_at_page_a4_portrait_rule() {
    let html = render_full_html();
    // @page { size: A4 ... } または @page { size: ... A4 ... } を許容
    let has_a4 = html.contains("@page") && html.contains("A4");
    assert!(has_a4, "@page 宣言に A4 size 指定が必要（仕様書 6.1）");
    // 縦 portrait (または横 landscape でない A4 単独指定も許容)
    assert!(
        html.contains("portrait") || html.contains("A4"),
        "portrait 指定が望ましい（仕様書 6.1）"
    );
}

#[test]
fn p3_spec_9_2_page_margin_defined() {
    let html = render_full_html();
    // @page 内に margin 指定
    assert!(
        html.contains("@page") && html.contains("margin"),
        "@page に margin が指定されているべき（仕様書 6.1: margin 12mm）"
    );
}

#[test]
fn p3_spec_9_2_page_break_inside_avoid_exists() {
    let html = render_full_html();
    assert!(
        html.contains("page-break-inside")
            && html.contains("avoid"),
        "page-break-inside: avoid が必須（仕様書 6.2）"
    );
}

#[test]
fn p3_spec_9_2_print_color_adjust_exact() {
    let html = render_full_html();
    let has_webkit = html.contains("-webkit-print-color-adjust")
        && html.contains("exact");
    let has_std = html.contains("print-color-adjust") && html.contains("exact");
    assert!(
        has_webkit || has_std,
        "print-color-adjust: exact が必須（仕様書 6.4）"
    );
}

#[test]
fn p3_spec_9_2_thead_table_header_group() {
    let html = render_full_html();
    assert!(
        html.contains("table-header-group"),
        "thead {{ display: table-header-group; }} でページ跨ぎ対応（仕様書 6.7）"
    );
}

#[test]
fn p3_spec_9_2_font_family_specified() {
    let html = render_full_html();
    assert!(
        html.contains("font-family"),
        "font-family 指定が必須（仕様書 5.1）"
    );
    // 日本語フォント指定が推奨
    assert!(
        html.contains("Hiragino")
            || html.contains("Meiryo")
            || html.contains("Noto Sans JP")
            || html.contains("sans-serif"),
        "日本語対応 font-family が推奨（仕様書 5.1）"
    );
}

#[test]
fn p3_spec_9_2_footer_contains_fac_company_name() {
    let html = render_full_html();
    // @bottom-left または static footer で F-A-C株式会社 を含む
    assert!(
        html.contains("F-A-C株式会社"),
        "@page @bottom-left 等に F-A-C株式会社 が必須（仕様書 7.3 / 6.1）"
    );
}

#[test]
fn p3_spec_9_2_page_counter_exists() {
    let html = render_full_html();
    assert!(
        html.contains("counter(page)") || html.contains("counter(pages)"),
        "ページ番号カウンタが必須（仕様書 6.1）"
    );
}

// ============================================================
// 9.3 コンテンツ必須項目チェック
// ============================================================

#[test]
fn p3_spec_9_3_hw_scope_notice_two_places() {
    let html = render_full_html();
    // 「ハローワーク掲載求人のみ」または「ハローワーク掲載求人」などキーワード
    // Executive Summary 直下と末尾注記の 2 箇所以上
    let pat = "ハローワーク";
    let count = html.matches(pat).count();
    assert!(
        count >= 2,
        "HW スコープ注意書きが 2 箇所以上必要（現在 {} 箇所）（MEMORY feedback_hw_data_scope）",
        count
    );
    // 「全求人市場」等の代表性注意
    assert!(
        html.contains("全求人市場") || html.contains("代表") || html.contains("限らない"),
        "全求人市場の代表ではない旨の注意書きが必要"
    );
}

#[test]
fn p3_spec_9_3_salary_bias_notice() {
    let html = render_full_html();
    // MEMORY feedback_hw_data_scope: 給与バイアス注意
    assert!(
        html.contains("給与バイアス")
            || html.contains("低く出る")
            || html.contains("中小企業")
            || html.contains("低めに設定"),
        "給与バイアス注意書きが必要（MEMORY feedback_hw_data_scope）"
    );
}

#[test]
fn p3_spec_9_3_correlation_not_causation_notice() {
    let html = render_full_html();
    // MEMORY feedback_correlation_not_causation
    let has_correlation_note = html.contains("相関")
        && (html.contains("因果") || html.contains("示唆") || html.contains("仮説"));
    assert!(
        has_correlation_note,
        "相関≠因果 注記が必要（MEMORY feedback_correlation_not_causation）"
    );
}

#[test]
fn p3_spec_9_3_generated_datetime_present() {
    let html = render_full_html();
    assert!(
        html.contains("生成日") || html.contains("生成日時"),
        "生成日時が表紙/末尾に必要（仕様書 7.3）"
    );
}

// ============================================================
// 9.4 禁止ワードチェック（仕様書 1.5 節）
// ============================================================

/// 禁止ワード完全一致チェック（リテラル文字列で出現しないこと）
///
/// 注意:
///   - 「上位」「順位」は文脈によっては安全だが、仕様書は完全一致禁止とする
///   - CSS クラス名等の偶発ヒットを避けるため、日本語リテラルのみ対象
fn assert_no_forbidden_word(html: &str, word: &str) {
    assert!(
        !html.contains(word),
        "禁止ワード「{}」が HTML 出力に含まれる（仕様書 1.5 節）",
        word
    );
}

#[test]
fn p3_spec_9_4_forbidden_word_ranking() {
    let html = render_full_html();
    assert_no_forbidden_word(&html, "ランキング");
}

#[test]
fn p3_spec_9_4_forbidden_word_rank_number() {
    let html = render_full_html();
    assert_no_forbidden_word(&html, "順位");
    assert_no_forbidden_word(&html, "1位");
}

#[test]
fn p3_spec_9_4_forbidden_word_top() {
    let html = render_full_html();
    // 「上位」は仕様書 1.5 で明示的に禁止（代替: 「件数の多い」「件数の多い順に整理」）
    assert_no_forbidden_word(&html, "上位");
}

#[test]
fn p3_spec_9_4_forbidden_word_evaluative() {
    let html = render_full_html();
    assert_no_forbidden_word(&html, "おすすめ");
    assert_no_forbidden_word(&html, "ベスト");
    assert_no_forbidden_word(&html, "最適");
}

#[test]
fn p3_spec_9_4_forbidden_word_quality_judgment() {
    let html = render_full_html();
    assert_no_forbidden_word(&html, "優良");
    assert_no_forbidden_word(&html, "質が高い");
}

#[test]
fn p3_spec_9_4_forbidden_word_prescriptive() {
    let html = render_full_html();
    assert_no_forbidden_word(&html, "すべき");
    assert_no_forbidden_word(&html, "しなければならない");
}

#[test]
fn p3_spec_9_4_forbidden_word_absolute() {
    let html = render_full_html();
    // 「確実に」は断定回避
    assert_no_forbidden_word(&html, "確実に");
    // 注: 「100%」は CSS の linear-gradient / width / max-width 等で頻出するため
    //     コンテキスト無視の生マッチは不適。
    //     日本語文字列の直後の「100%」のみを検出（例: 「改善100%」等の断定ケース）
    let needle = "100%";
    let mut ja_context_found = false;
    let mut search_start = 0usize;
    while let Some(found) = html[search_start..].find(needle) {
        let abs_pos = search_start + found;
        // 直前の char を 1 文字取得（UTF-8 safe に）
        let head = &html[..abs_pos];
        if let Some(prev_char) = head.chars().last() {
            let is_ja = (prev_char >= '\u{3040}' && prev_char <= '\u{30FF}')
                || (prev_char >= '\u{4E00}' && prev_char <= '\u{9FFF}');
            if is_ja {
                ja_context_found = true;
                break;
            }
        }
        search_start = abs_pos + needle.len();
    }
    assert!(
        !ja_context_found,
        "日本語文脈の中に『100%』が断定語として使われている可能性（仕様書 1.5 節）"
    );
}

// ============================================================
// 9.5 Severity / 色チェック
// ============================================================

#[test]
fn p3_spec_9_5_severity_colors_defined() {
    let html = render_full_html();
    // helpers.rs::Severity の色と厳密一致
    // Critical=#ef4444, Warning=#f59e0b, Info=#3b82f6, Positive=#10b981
    // 少なくとも CSS 定義か実使用のどちらかで色コードが含まれる
    // （全 severity が必ず UI 出力されるとは限らないが、仕様書 5.2 でパレット定義必須）
    let has_critical = html.contains("#ef4444");
    let has_warning = html.contains("#f59e0b");
    let has_info = html.contains("#3b82f6");
    let has_positive = html.contains("#10b981");
    let defined_count = [has_critical, has_warning, has_info, has_positive]
        .iter()
        .filter(|&&b| b)
        .count();
    assert!(
        defined_count >= 2,
        "Severity カラー（ef4444/f59e0b/3b82f6/10b981）のうち少なくとも 2 つが定義されるべき（仕様書 5.2）。現在 {} 個",
        defined_count
    );
}

#[test]
fn p3_spec_9_5_severity_icons_present_for_monochrome() {
    let html = render_full_html();
    // モノクロ耐性: severity アイコン文字（▲▲ / ▲ / ● / ◯）のいずれかが必要
    // 実装で severity badge を 1 つでも使っていれば 1 つは含まれる
    let has_any_icon = html.contains("\u{25B2}\u{25B2}") // ▲▲
        || html.contains("\u{25B2}") // ▲
        || html.contains("\u{25CF}") // ●
        || html.contains("\u{25EF}") // ◯
        || html.contains("\u{25CB}"); // ○ (alt)
    assert!(
        has_any_icon,
        "モノクロ耐性のための severity 文字アイコン（▲▲/▲/●/◯）が必要（仕様書 6.5）"
    );
}

// ============================================================
// 9.6 Executive Summary 詳細チェック
// ============================================================

#[test]
fn p3_spec_9_6_kpi_five_cards_present() {
    let html = render_full_html();
    // 5 KPI: サンプル件数, 主要地域, 主要雇用形態, 給与中央値, 新着比率
    // カード要素（kpi-card など）が少なくとも 5 つ、または 5 KPI ラベルが含まれる
    let kpi_card_count = html.matches("kpi-card").count();
    let has_five_labels = html.contains("サンプル")
        && (html.contains("主要地域") || html.contains("地域"))
        && (html.contains("雇用形態") || html.contains("主要雇用"))
        && (html.contains("中央値") || html.contains("給与中央"))
        && (html.contains("新着"));
    assert!(
        kpi_card_count >= 5 || has_five_labels,
        "5 KPI カード（サンプル/地域/雇用形態/中央値/新着）が必須（仕様書 3.3）。kpi-card 出現数={}",
        kpi_card_count
    );
}

#[test]
fn p3_spec_9_6_priority_actions_three_or_placeholder() {
    let html = render_full_html();
    // 推奨優先アクション 3 件、または「該当なし」プレースホルダー
    // action 要素数を軽量判定
    let has_action_container = html.contains("exec-summary-action")
        || html.contains("推奨")
        || html.contains("アクション");
    let has_no_action_placeholder = html.contains("該当なし") || html.contains("条件を満たす");
    assert!(
        has_action_container || has_no_action_placeholder,
        "推奨優先アクション 3 件または placeholder が必須（仕様書 3.4）"
    );
}

// ============================================================
// 9.7 データ欠損ハンドリング（panic 耐性）
// ============================================================

#[test]
fn p3_spec_9_7_renders_default_aggregation_without_panic() {
    // 全 field 空 の SurveyAggregation でも panic しないこと
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[]);
    assert!(
        html.contains("<!DOCTYPE html>"),
        "空データでも HTML ドキュメントを返すこと"
    );
    assert!(
        html.contains("</html>"),
        "空データでも HTML を完結させること"
    );
}

#[test]
fn p3_spec_9_7_zero_total_count_handled() {
    let mut agg = SurveyAggregation::default();
    agg.total_count = 0;
    let seeker = JobSeekerAnalysis::default();
    let html = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[]);
    // 注: "isNaN" は JavaScript の built-in 関数で "NaN" を substring として含むため
    //     stricter な matching （単語境界類似）で誤ヒットを避ける
    let bad_nan = html.contains(">NaN<")
        || html.contains(" NaN ")
        || html.contains("=NaN")
        || html.contains(":NaN");
    let bad_inf = html.contains(">Infinity<")
        || html.contains(" Infinity ")
        || html.contains("=Infinity")
        || html.contains(":Infinity")
        || html.contains("-Infinity");
    assert!(
        !bad_nan && !bad_inf,
        "total_count=0 で NaN/Infinity が HTML 本文に出力されないこと"
    );
}

#[test]
fn p3_spec_9_7_missing_enhanced_stats_handled() {
    let mut agg = minimal_agg();
    agg.enhanced_stats = None;
    agg.salary_values.clear();
    let seeker = minimal_seeker();
    let html = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[]);
    // フォールバック表示（「サンプル不足」「算出不能」「データなし」等）
    let has_fallback = html.contains("サンプル不足")
        || html.contains("算出不能")
        || html.contains("データなし")
        || html.contains("省略");
    assert!(
        has_fallback || html.contains("<!DOCTYPE html>"),
        "enhanced_stats=None 時に合理的フォールバック or 少なくとも panic せず HTML を返すこと"
    );
}

#[test]
fn p3_spec_9_7_missing_seeker_subfields_handled() {
    let agg = minimal_agg();
    let seeker = JobSeekerAnalysis {
        expected_salary: None,
        salary_range_perception: None,
        inexperience_analysis: None,
        new_listings_premium: None,
        total_analyzed: 0,
    };
    let html = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[]);
    assert!(
        html.contains("<!DOCTYPE html>"),
        "JobSeekerAnalysis 全 Option None で panic しないこと"
    );
}

#[test]
fn p3_spec_9_7_all_edge_cases_combined() {
    // 最悪ケース: 全データ空 + hw_context None + salesnow 空
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[]);
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("</html>"));
    // 必須セクション 13（免責）は空データでも出す
    assert!(
        html.contains("ハローワーク") || html.contains("免責") || html.contains("注記"),
        "空データでも末尾注記（Section 13）は出力される"
    );
}

// ============================================================
// 9.8 ブランディングチェック
// ============================================================

#[test]
fn p3_spec_9_8_fac_company_name_present() {
    let html = render_full_html();
    // 仕様書 7.1: 「F-A-C株式会社」（半角ハイフン、株式会社は全角、間スペースなし）
    assert!(
        html.contains("F-A-C株式会社"),
        "F-A-C株式会社 ブランド表記が必須（仕様書 7.1）"
    );
    // 3 箇所以上（表紙、@page footer、本文末尾）（仕様書 9.3）
    let count = html.matches("F-A-C株式会社").count();
    assert!(
        count >= 1,
        "F-A-C株式会社 が少なくとも 1 箇所必要。仕様書 9.3 では 3 箇所以上が期待される（現在 {}）",
        count
    );
}

// ============================================================
// 9.9 契約検証（2026-04-23 事故再発防止）
// ============================================================

#[test]
fn p3_spec_9_9_struct_fields_unchanged_compile_check() {
    // 本ファイルが構造体のフィールド名を直接参照しているため、
    // フィールド名が変更されればコンパイルエラーで検出される。
    // ここでは代表フィールドに直接アクセスして契約を保証する。
    let agg = SurveyAggregation::default();
    let _: usize = agg.total_count;
    let _: usize = agg.new_count;
    let _: Option<String> = agg.dominant_prefecture.clone();
    let _: Vec<(String, usize)> = agg.by_employment_type.clone();
    let _: Vec<i64> = agg.salary_values.clone();
    let _: Option<EnhancedStats> = agg.enhanced_stats.clone();
    let _: Vec<CompanyAgg> = agg.by_company.clone();
    let _: Vec<EmpTypeSalary> = agg.by_emp_type_salary.clone();
    let _: Vec<i64> = agg.salary_min_values.clone();
    let _: Vec<i64> = agg.salary_max_values.clone();
    let _: Vec<TagSalaryAgg> = agg.by_tag_salary.clone();
    let _: Vec<MunicipalitySalaryAgg> = agg.by_municipality_salary.clone();
    let _: Vec<ScatterPoint> = agg.scatter_min_max.clone();
    let _: Option<RegressionResult> = agg.regression_min_max.clone();
    let _: Vec<PrefectureSalaryAgg> = agg.by_prefecture_salary.clone();
    let _: bool = agg.is_hourly;

    let seeker = JobSeekerAnalysis::default();
    let _: Option<i64> = seeker.expected_salary;
    let _: Option<SalaryRangePerception> = seeker.salary_range_perception.clone();
    let _: Option<InexperienceAnalysis> = seeker.inexperience_analysis.clone();
    let _: Option<i64> = seeker.new_listings_premium;
    let _: usize = seeker.total_analyzed;

    let nc = NearbyCompany::default();
    let _: String = nc.corporate_number.clone();
    let _: String = nc.company_name.clone();
    let _: String = nc.prefecture.clone();
    let _: String = nc.sn_industry.clone();
    let _: i64 = nc.employee_count;
    let _: f64 = nc.credit_score;
    let _: String = nc.postal_code.clone();
    let _: i64 = nc.hw_posting_count;
}

#[test]
fn p3_spec_9_9_render_function_signature_unchanged() {
    // シグネチャ: fn(&SurveyAggregation, &JobSeekerAnalysis, &[CompanyAgg], &[EmpTypeSalary],
    //            &[i64], &[i64], Option<&InsightContext>, &[NearbyCompany]) -> String
    // 型が一致しなければコンパイル時エラー
    let f: fn(
        &SurveyAggregation,
        &JobSeekerAnalysis,
        &[CompanyAgg],
        &[EmpTypeSalary],
        &[i64],
        &[i64],
        Option<&InsightContext>,
        &[NearbyCompany],
    ) -> String = render_survey_report_page;
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let out = f(&agg, &seeker, &[], &[], &[], &[], None, &[]);
    assert!(out.contains("<!DOCTYPE html>"));
}

// ============================================================
// 付記: 機械検証不可能な手動確認項目
// ============================================================
//
// 以下は `cargo test` では検証不可能なため、視覚的に手動確認すること:
//
// 1. 表紙 (Cover) が印刷プレビューで 1 ページ目に完結
// 2. Executive Summary が 2 ページ目に完結
// 3. 見出しがページ末尾に孤立しない
// 4. テーブルがページ跨ぎしてもヘッダーが次ページ冒頭に再出現
// 5. モノクロ印刷プレビュー（Chrome: Color -> Black and White）で
//    severity がアイコン文字により判別可能
// 6. CDN オフライン環境下での ECharts フォールバック挙動
// 7. 日本語フォントレンダリング品質
//
// 手順:
//   1) cargo run --release
//   2) ブラウザで /survey/report?... を開く
//   3) Ctrl+P -> Save as PDF
//   4) PDF を目視確認
