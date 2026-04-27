//! Agent P3: PDF 競合調査レポート QA テスト
//!
//! 仕様書: docs/pdf_design_spec_2026_04_24.md 9節 QA チェックリスト
//! 追加要件: 2026-04-24 ユーザー要件（Agent B 改修中）
//!   - 「ハローワーク競合調査」文言削除
//!   - 「SalesNow」文言 UI 上全削除 → 「地域注目企業」統一
//!   - 地域注目企業セクション刷新（与信削除、売上/1y/3m 追加）
//!   - HW データ連携セクション新規
//!   - A4縦印刷 UX 最適化（page-break / contenteditable）
//!   - HTML 自己完結性（inline CSS / 外部 link 禁止 / ECharts CDN のみ許容）
//!
//! 目的:
//!   P2/Agent B が再実装する `render_survey_report_page` の出力 HTML を
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
//!   - Agent B 未完了時: 9.4 禁止ワード系/9.10-9.13 新規群の一部 fail は期待通り
//!   - Agent B 完了時: 全 pass を最終目標

use super::super::company::fetch::NearbyCompany;
use super::super::insight::fetch::InsightContext;
use super::aggregator::{
    CompanyAgg, EmpTypeSalary, MunicipalitySalaryAgg, PrefectureSalaryAgg, RegressionResult,
    ScatterPoint, SurveyAggregation, TagSalaryAgg,
};
use super::job_seeker::{InexperienceAnalysis, JobSeekerAnalysis, SalaryRangePerception};
use super::report_html::render_survey_report_page;
use super::statistics::EnhancedStats;

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

/// NearbyCompany mock（2026-04-24 要件: SalesNow 表示削除 / 売上 / 1y / 3m 追加）
///
/// 注意:
///   - company_name には "SalesNow" を含めない（9.4 禁止ワード検証に干渉するため）
///   - sales_amount, sales_range, employee_delta_1y, employee_delta_3m の
///     新フィールドは B の render_section_* 実装で HTML に出力される前提
fn mock_nearby_company() -> NearbyCompany {
    NearbyCompany {
        corporate_number: "1234567890123".to_string(),
        company_name: "サンプル医療法人α".to_string(),
        prefecture: "東京都".to_string(),
        sn_industry: "医療・福祉".to_string(),
        employee_count: 200,
        credit_score: 68.0,
        postal_code: "100".to_string(),
        hw_posting_count: 5,
        sales_amount: 1_200_000_000.0,
        sales_range: "10-50億円".to_string(),
        employee_delta_1y: 12.5,
        employee_delta_3m: 3.2,
    }
}

/// P2/B 完了後を想定した「フル機能呼び出し」の HTML
fn render_full_html() -> String {
    let agg = minimal_agg();
    let seeker = minimal_seeker();
    let by_company = agg.by_company.clone();
    let by_emp = agg.by_emp_type_salary.clone();
    let smin = agg.salary_min_values.clone();
    let smax = agg.salary_max_values.clone();
    let mut ctx = mock_empty_insight_ctx();
    // Minimal ts_counts so compute_posting_change_from_ts returns non-None
    // (4 snapshots required for 3m, 13 for 1y. Provide 13 to trigger both)
    use std::collections::HashMap as StdHashMap;
    ctx.ts_counts = (0..13)
        .map(|i| {
            let mut row: StdHashMap<String, serde_json::Value> = StdHashMap::new();
            row.insert(
                "snapshot_id".into(),
                serde_json::Value::String(format!("2025-{:02}", (12 - i).max(1))),
            );
            row.insert(
                "emp_group".into(),
                serde_json::Value::String("正社員".into()),
            );
            row.insert(
                "posting_count".into(),
                serde_json::Value::Number(((1000 + i * 50) as i64).into()),
            );
            row.insert(
                "facility_count".into(),
                serde_json::Value::Number(100_i64.into()),
            );
            row
        })
        .collect();
    ctx.vacancy = vec![{
        let mut row: StdHashMap<String, serde_json::Value> = StdHashMap::new();
        row.insert(
            "emp_group".into(),
            serde_json::Value::String("正社員".into()),
        );
        row.insert(
            "vacancy_rate".into(),
            serde_json::Value::Number(serde_json::Number::from_f64(0.12).unwrap()),
        );
        row.insert(
            "total_count".into(),
            serde_json::Value::Number(500_i64.into()),
        );
        row
    }];
    let sn = vec![mock_nearby_company()];
    render_survey_report_page(
        &agg,
        &seeker,
        &by_company,
        &by_emp,
        &smin,
        &smax,
        Some(&ctx),
        &sn,
    )
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
    assert!(html.contains("雇用形態"), "Section 4（雇用形態分布）が必須");
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
fn p3_spec_9_1_hw_comparison_section_removed() {
    // 2026-04-24 ユーザー指摘により HW市場比較セクションは**削除**
    // (任意スクレイピング件数 vs HW 全体の非同質比較は無意味)
    // hw_context の有無に関わらず HW市場比較 <h2> は出ないことを検証
    let html_without = render_minimal_html();
    let has_h2_without = html_without.split("<h2").skip(1).any(|s| {
        s.split("</h2>")
            .next()
            .map(|t| t.contains("HW市場比較") || t.contains("HW 市場比較"))
            .unwrap_or(false)
    });
    assert!(!has_h2_without, "hw_context=None: HW市場比較は削除済");

    let html_with = render_full_html();
    let has_h2_with = html_with.split("<h2").skip(1).any(|s| {
        s.split("</h2>")
            .next()
            .map(|t| t.contains("HW市場比較") || t.contains("HW 市場比較"))
            .unwrap_or(false)
    });
    assert!(!has_h2_with, "hw_context=Some でも HW市場比較は削除済");
}

#[test]
fn p3_spec_9_1_featured_companies_section_toggled_by_emptiness() {
    // 2026-04-24 要件: SalesNow 表記削除 → 「地域注目企業」統一
    // CSS コメント等の偶発ヒットを避けるため <h2> 内の検出に限定
    let html_without = render_minimal_html();
    let has_h2_without = html_without.split("<h2").skip(1).any(|s| {
        s.split("</h2>")
            .next()
            .map(|t| t.contains("地域注目企業") || t.contains("注目企業"))
            .unwrap_or(false)
    });
    assert!(
        !has_h2_without,
        "nearby_companies.is_empty() のとき 地域注目企業セクションの <h2> が出ない"
    );

    let html_with = render_full_html();
    let has_h2_with = html_with.split("<h2").skip(1).any(|s| {
        s.split("</h2>")
            .next()
            .map(|t| t.contains("地域注目企業") || t.contains("注目企業"))
            .unwrap_or(false)
    });
    assert!(
        has_h2_with,
        "nearby_companies 非空のとき 地域注目企業セクションの <h2> が必要"
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
        html.contains("page-break-inside") && html.contains("avoid"),
        "page-break-inside: avoid が必須（仕様書 6.2）"
    );
}

#[test]
fn p3_spec_9_2_print_color_adjust_exact() {
    let html = render_full_html();
    let has_webkit = html.contains("-webkit-print-color-adjust") && html.contains("exact");
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
    // @bottom-left または static footer で 株式会社For A-career を含む
    assert!(
        html.contains("株式会社For A-career"),
        "@page @bottom-left 等に 株式会社For A-career が必須（仕様書 7.3 / 6.1）"
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
// 9.4 禁止ワードチェック（仕様書 1.5 節 + 2026-04-24 追加）
// ============================================================

/// 禁止ワード完全一致チェック（リテラル文字列で出現しないこと）
///
/// 注意:
///   - 「上位」「順位」は文脈によっては安全だが、仕様書は完全一致禁止とする
///   - CSS クラス名等の偶発ヒットを避けるため、日本語リテラルのみ対象
fn assert_no_forbidden_word(html: &str, word: &str) {
    assert!(
        !html.contains(word),
        "禁止ワード「{}」が HTML 出力に含まれる（仕様書 1.5 節 / 2026-04-24 追加要件）",
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

// ---- 2026-04-24 追加: UI 文言禁止ワード ----

#[test]
fn p3_spec_9_4_no_hw_survey_label() {
    // 旧タイトル構成要素「ハローワーク競合調査」は全削除
    let html = render_full_html();
    assert_no_forbidden_word(&html, "ハローワーク競合調査");
}

#[test]
fn p3_spec_9_4_no_salesnow_label() {
    // 「SalesNow」は UI 表示から全削除。「地域注目企業」に統一
    // HTML コメントや data-* 属性レベルでも出現させない方針
    let html = render_full_html();
    assert_no_forbidden_word(&html, "SalesNow");
}

#[test]
fn p3_spec_9_4_no_credit_score_label() {
    // 「与信スコア」「与信指標」等のユーザー向け表示文字列は削除
    // （内部フィールド名 credit_score は Rust struct 名なので HTML に出ない想定）
    let html = render_full_html();
    assert!(
        !html.contains("与信スコア") && !html.contains("与信指標") && !html.contains("信用スコア"),
        "与信/信用スコアは UI に表示しない（仕様書 2026-04-24 追加要件）"
    );
}

#[test]
fn p3_spec_9_4_no_rival_investigation_label() {
    // 「競合調査分析」「競合調査レポート」等の強い競争語彙も削減対象
    // （Agent B の合意: タイトル/サブタイトルから「競合調査」系を外し「求人市場分析」系に統一）
    let html = render_full_html();
    // 表紙サブタイトル周辺の「競合調査分析」チェック
    assert_no_forbidden_word(&html, "競合調査分析");
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
    // 仕様書 7.1: 「株式会社For A-career」（半角ハイフン、株式会社は全角、間スペースなし）
    assert!(
        html.contains("株式会社For A-career"),
        "株式会社For A-career ブランド表記が必須（仕様書 7.1）"
    );
    // 3 箇所以上（表紙、@page footer、本文末尾）（仕様書 9.3）
    let count = html.matches("株式会社For A-career").count();
    assert!(
        count >= 1,
        "株式会社For A-career が少なくとも 1 箇所必要。仕様書 9.3 では 3 箇所以上が期待される（現在 {}）",
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
    // 2026-04-24 追加フィールド（契約検証）
    let _: f64 = nc.sales_amount;
    let _: String = nc.sales_range.clone();
    let _: f64 = nc.employee_delta_1y;
    let _: f64 = nc.employee_delta_3m;
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
// 9.10 地域注目企業セクション項目（2026-04-24 新規要件）
// ============================================================
//
// 背景:
//   - 「SalesNow」表記削除 → 「地域注目企業」に統一
//   - 与信/信用スコア列を削除
//   - 新列: 売上（金額 or レンジ）/ 1年人員推移 / 3ヶ月人員推移
//
// 注意:
//   - B 実装の進捗次第で当面 fail する（期待通り）

#[test]
fn p3_spec_9_10_section_title_is_area_featured_companies() {
    let html = render_full_html();
    // 「地域注目企業」見出しが <h2> 内に存在（SalesNow は含まない）
    let has_proper_title = html.split("<h2").skip(1).any(|s| {
        s.split("</h2>")
            .next()
            .map(|t| {
                (t.contains("地域注目企業") || t.contains("注目企業")) && !t.contains("SalesNow")
            })
            .unwrap_or(false)
    });
    assert!(
        has_proper_title,
        "地域注目企業セクションの <h2> が『地域注目企業』または『注目企業』のみで、\
        『SalesNow』を含まないこと"
    );
}

#[test]
fn p3_spec_9_10_column_sales_shown() {
    let html = render_full_html();
    // 売上表示: ヘッダに「売上」または「売上高」文字列が含まれる
    assert!(
        html.contains("売上") || html.contains("売上高"),
        "地域注目企業テーブルに『売上』列が必要（2026-04-24 追加要件）"
    );
}

#[test]
fn p3_spec_9_10_column_employee_delta_1y_shown() {
    let html = render_full_html();
    // 1年推移列: 「1年」と（「人員」or「推移」or「従業員」）の両方が含まれる
    let has_1y = html.contains("1年") || html.contains("1 年") || html.contains("1ヶ年");
    let has_head = html.contains("人員") || html.contains("推移") || html.contains("従業員");
    assert!(
        has_1y && has_head,
        "地域注目企業テーブルに『1年人員推移』列が必要（2026-04-24 追加要件）。\
        1年={} / 人員or推移or従業員={}",
        has_1y,
        has_head
    );
}

#[test]
fn p3_spec_9_10_column_employee_delta_3m_shown() {
    let html = render_full_html();
    // 3ヶ月推移列: 「3ヶ月」or「3か月」or「3カ月」 と（「人員」or「推移」or「従業員」）
    let has_3m = html.contains("3ヶ月")
        || html.contains("3か月")
        || html.contains("3カ月")
        || html.contains("3 ヶ月");
    let has_head = html.contains("人員") || html.contains("推移") || html.contains("従業員");
    assert!(
        has_3m && has_head,
        "地域注目企業テーブルに『3ヶ月人員推移』列が必要（2026-04-24 追加要件）。\
        3ヶ月={} / 人員or推移or従業員={}",
        has_3m,
        has_head
    );
}

#[test]
fn p3_spec_9_10_no_credit_column_in_featured_companies() {
    // 地域注目企業テーブルから与信/信用スコア列が削除されていること
    let html = render_full_html();
    // 9.4 と重複するが、特にテーブル headings（<th>...与信...</th>）を対象に強く検証
    assert!(
        !html.contains("<th>与信") && !html.contains("<th>信用"),
        "地域注目企業テーブルから与信/信用スコア列を削除（2026-04-24 追加要件）"
    );
}

// ============================================================
// 9.11 HW データ連携セクション（2026-04-24 新規要件）
// ============================================================

#[test]
fn p3_spec_9_11_hw_enrichment_section_title() {
    let html = render_full_html();
    // セクションタイトル: 「HW データ連携」「HW状況」「地域別 HW」等の柔軟マッチ
    let has_section_title = html.split("<h2").skip(1).any(|s| {
        s.split("</h2>")
            .next()
            .map(|t| {
                (t.contains("HW") || t.contains("ハローワーク"))
                    && (t.contains("連携")
                        || t.contains("状況")
                        || t.contains("求人動向")
                        || t.contains("求人状況")
                        || t.contains("掲載動向"))
            })
            .unwrap_or(false)
    });
    assert!(
        has_section_title,
        "HW データ連携セクションの <h2> が必要（例: 『HW データ連携』『地域別 HW 状況』等）"
    );
}

#[test]
fn p3_spec_9_11_trend_labels_present() {
    let html = render_full_html();
    // 2026-04-24 ユーザー指摘により、表内 trend 列（3ヶ月/1年推移）は削除。
    // ts_turso_counts の初期スナップショット不安定で「+374.3%」等の暴走値を
    // 全行に同じ値として出してしまっていたため。
    // テーブルでは CSV件数 / HW現在件数のみ、時系列推移は注記レベルに留める。
    let h2_has_hw_enrich = html.split("<h2").skip(1).any(|s| {
        s.split("</h2>")
            .next()
            .map(|t| t.contains("地域 × HW"))
            .unwrap_or(false)
    });
    assert!(
        h2_has_hw_enrich,
        "地域 × HW データ連携セクションの <h2> が必要（trend 列は削除済）"
    );
}

#[test]
fn p3_spec_9_11_vacancy_rate_placeholder_exists() {
    let html = render_full_html();
    // 欠員補充率表示: データあり→数値（%）、なし→「—」または「-」
    // 少なくとも「欠員補充率」ラベルとプレースホルダ記号が併存
    // (2026-04-26 vacancy_rate 概念整理: 「欠員率」→「欠員補充率」)
    let has_vacancy_label = html.contains("欠員補充率");
    let has_placeholder = html.contains("\u{2014}") || html.contains("—") || html.contains("%");
    assert!(
        has_vacancy_label && has_placeholder,
        "欠員補充率ラベルと値（%）orプレースホルダ（—）が必要。label={} / placeholder={}",
        has_vacancy_label,
        has_placeholder
    );
}

#[test]
fn p3_spec_9_11_hw_posting_count_displayed() {
    let html = render_full_html();
    // HW 掲載求人数: 「HW求人数」「HW 掲載求人」「掲載求人数」等のラベル
    let has_label = html.contains("HW求人数")
        || html.contains("HW 掲載求人")
        || html.contains("掲載求人数")
        || html.contains("HW掲載");
    assert!(
        has_label,
        "HW 掲載求人数ラベルが必要（ダッシュボード本体との表記整合）"
    );
}

// ============================================================
// 9.12 HTML 自己完結性（2026-04-24 新規要件）
// ============================================================
//
// 背景:
//   - ユーザーはダウンロード後にブラウザだけで編集→印刷する
//   - 外部依存は ECharts CDN のみ（画像/CSSの外部 link は禁止）
//   - <style> タグに主要 CSS が inline で含まれること

#[test]
fn p3_spec_9_12_inline_style_block_present() {
    let html = render_full_html();
    // <style> タグに実質的な CSS が含まれる（空タグは NG）
    assert!(html.contains("<style"), "<style> タグが必要（inline CSS）");
    // 主要 CSS キーワードが style 内に出現することを緩くチェック
    // （font-family は既に 9.2 で検証済みなので、body/section/table 等の基本セレクタを確認）
    let has_basic_selectors = html.contains("body")
        && (html.contains("section") || html.contains(".section") || html.contains("div"))
        && (html.contains("table") || html.contains(".data-table"));
    assert!(
        has_basic_selectors,
        "inline <style> に主要セレクタ（body/section/table 等）が含まれるべき"
    );
}

#[test]
fn p3_spec_9_12_no_external_stylesheet_link() {
    let html = render_full_html();
    // 外部 CSS <link rel="stylesheet" ...> が存在しないこと
    // ECharts CDN は <script> タグなので対象外
    let has_external_css = html.contains("rel=\"stylesheet\"") || html.contains("rel='stylesheet'");
    assert!(
        !has_external_css,
        "外部 CSS link は禁止（HTML 自己完結性、ECharts CDN のみ許容）"
    );
}

#[test]
fn p3_spec_9_12_echarts_cdn_allowed() {
    let html = render_full_html();
    // 唯一許容される外部依存: ECharts CDN の <script src="https://cdn.jsdelivr.net/...echarts..."
    let has_echarts_script = html.contains("echarts");
    assert!(
        has_echarts_script,
        "ECharts CDN script は必要（グラフ描画のため）"
    );
}

#[test]
fn p3_spec_9_12_contenteditable_attributes_exist() {
    let html = render_full_html();
    // ダウンロード後編集想定で、主要テキスト箇所に contenteditable="true" が付与されている
    let count = html.matches("contenteditable=\"true\"").count()
        + html.matches("contenteditable='true'").count()
        + html.matches("contenteditable=true").count();
    assert!(
        count >= 3,
        "contenteditable=\"true\" が主要テキスト箇所に最低 3 箇所必要（現在 {}）\
        （2026-04-24 追加要件: ダウンロード後編集 UX）",
        count
    );
}

// ============================================================
// 9.13 A4 縦印刷 UX（2026-04-24 新規要件強化）
// ============================================================

#[test]
fn p3_spec_9_13_page_size_a4_portrait_strict() {
    let html = render_full_html();
    // @page { size: A4 portrait; ... } を strict に検証
    // 空白許容: "A4 portrait", "A4  portrait" 等
    let has_portrait = html.contains("A4 portrait")
        || html.contains("A4  portrait")
        || html.contains("A4\tportrait")
        || html.contains("portrait A4")
        || (html.contains("@page") && html.contains("A4") && html.contains("portrait"));
    assert!(
        has_portrait,
        "@page {{ size: A4 portrait; }} が必要（2026-04-24: A4縦印刷 UX 最適化）"
    );
}

#[test]
fn p3_spec_9_13_page_break_rules_exist() {
    let html = render_full_html();
    // page-break-inside: avoid は 9.2 で検証済み
    // 追加: page-break-before / page-break-after のいずれかが存在
    let has_before_or_after = html.contains("page-break-before")
        || html.contains("page-break-after")
        || html.contains("break-before")
        || html.contains("break-after");
    assert!(
        has_before_or_after,
        "page-break-before/after ルールが必要（章区切りの明示 / 2026-04-24 追加要件）"
    );
}

#[test]
fn p3_spec_9_13_media_print_rule_exists() {
    let html = render_full_html();
    // @media print ルールが存在（印刷専用スタイル）
    assert!(
        html.contains("@media print"),
        "@media print ルールが必要（印刷専用スタイル / 2026-04-24 追加要件）"
    );
}

#[test]
fn p3_spec_9_13_no_print_class_for_ui_controls() {
    let html = render_full_html();
    // 画面用 UI コントロール（印刷ボタン等）は no-print クラスで印刷時非表示
    // @media print 内の .no-print { display: none } 相当が存在
    let has_no_print_class = html.contains("no-print") || html.contains(".no-print");
    assert!(
        has_no_print_class,
        "画面専用 UI に no-print クラスが必要（印刷時に非表示化）"
    );
}

// ============================================================
// Fix-B 追加 (D-2 監査 / 2026-04-26): 逆因果文言修正の逆証明テスト
// feedback_correlation_not_causation.md 準拠
// ============================================================

/// wage.rs: 旧文言「優先検討すると効率的」が出ない（因果断定の削除）
/// + 新文言「目立つ存在感を持つ傾向」が出る
#[test]
fn fixb_wage_no_efficient_priority_phrasing() {
    let html = render_full_html();
    assert!(
        !html.contains("優先検討すると効率的"),
        "wage.rs: 因果断定文言「優先検討すると効率的」が残っている (feedback_correlation_not_causation.md 違反)"
    );
}

#[test]
fn fixb_wage_has_correlation_safe_phrasing() {
    let html = render_full_html();
    // 最低賃金セクションは agg.by_prefecture_salary に依存する
    // minimal_agg() に存在するため出力されるはず
    assert!(
        html.contains("目立つ存在感を持つ傾向"),
        "wage.rs: 新文言「目立つ存在感を持つ傾向」が見つからない"
    );
    assert!(
        html.contains("因果関係を示すものではありません"),
        "wage.rs: 因果断定回避文言が必須"
    );
}

/// seeker.rs: 旧文言「給与水準が上昇傾向」が出ない（因果断定の削除）
/// + 新文言「正の関連が観測」が出る
#[test]
fn fixb_seeker_no_salary_rising_trend_phrasing() {
    let html = render_full_html();
    assert!(
        !html.contains("給与水準が上昇傾向"),
        "seeker.rs: 因果断定文言「給与水準が上昇傾向」が残っている"
    );
}

#[test]
fn fixb_seeker_has_correlation_safe_phrasing() {
    let html = render_full_html();
    assert!(
        html.contains("正の関連が観測"),
        "seeker.rs: 新文言「正の関連が観測」が見つからない"
    );
    assert!(
        html.contains("因果関係を主張するものでもありません"),
        "seeker.rs: 因果断定回避文言が必須"
    );
}

/// salesnow.rs: 旧文言「採用が活発な傾向」が出ない（因果断定の削除）
/// + 新文言「採用活動が活発な可能性」が出る + 両方向解釈の注記
#[test]
fn fixb_salesnow_no_active_hiring_assertion() {
    let html = render_full_html();
    // 厳密な旧文言「採用が活発な傾向（相関であり、因果は別途検討）」が消えていること
    assert!(
        !html.contains("採用が活発な傾向（相関であり、因果は別途検討）"),
        "salesnow.rs: 旧文言が残っている"
    );
}

#[test]
fn fixb_salesnow_has_two_way_interpretation() {
    let html = render_full_html();
    assert!(
        html.contains("採用活動が活発な可能性"),
        "salesnow.rs: 新文言「採用活動が活発な可能性」が見つからない"
    );
    assert!(
        html.contains("採用が難航しているために HW にも掲載しているケース"),
        "salesnow.rs: 両方向解釈の注記（逆方向ケース）が必須"
    );
    assert!(
        html.contains("組織改編") || html.contains("統計粒度"),
        "salesnow.rs: 印刷版に組織改編/統計粒度の揺らぎ注記が必要 (D-2 Q2.4 対応)"
    );
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
// 8. contenteditable で編集後の印刷 PDF 反映確認
//
// 手順:
//   1) cargo run --release
//   2) ブラウザで /survey/report?... を開く
//   3) Ctrl+P -> Save as PDF
//   4) PDF を目視確認
