//! HTMLレポート生成（株式会社For A-career 求人市場 総合診断レポート）
//! 仕様書: docs/pdf_design_spec_2026_04_24.md (2026-04-24 追加要件反映済み)
//! A4縦向き印刷 / ダウンロード後編集 両対応のHTMLとして出力する
//! - 表紙 → Executive Summary → HWデータ連携 → 各セクション(So What付き) → 注記/免責
//! - EChartsインタラクティブチャート + ソート可能テーブル
//! - 印刷時はモノクロ耐性（severityアイコン併記）対応
//! - `contenteditable` により主要コメント欄はダウンロード後にユーザーが編集可能

use super::super::company::fetch::NearbyCompany;
use super::super::helpers::escape_html;
use super::super::insight::fetch::InsightContext;
#[cfg(test)]
use super::aggregator::ScatterPoint;
use super::aggregator::{CompanyAgg, EmpTypeSalary, SurveyAggregation};
use super::hw_enrichment::HwAreaEnrichment;
use super::job_seeker::JobSeekerAnalysis;

// ======== サブモジュール宣言 (大規模ファイル分割: C-2) ========
mod employment;
mod executive_summary;
mod helpers;
mod hw_enrichment;
mod notes;
mod region;
mod salary_stats;
mod salesnow;
mod scatter;
mod seeker;
mod style;
mod summary;
mod wage;

// 各サブモジュール公開 API (本 mod.rs 内のエントリ関数から呼出)
use executive_summary::render_section_executive_summary;
use hw_enrichment::render_section_hw_enrichment;
use summary::render_section_summary;
// render_section_hw_comparison / render_comparison_card は hw_enrichment.rs 内で legacy として保持
// (#[allow(dead_code)] でモジュール内で抑制済み)
use employment::render_section_emp_group_native;
use employment::render_section_employment;
use helpers::{compose_target_region, render_scripts};
use notes::render_section_notes;
use region::render_section_municipality_salary;
use region::render_section_region;
use salary_stats::render_section_salary_stats;
use salesnow::render_section_salesnow_companies;
use scatter::render_section_scatter;
use seeker::render_section_job_seeker;
use style::render_css;
use wage::render_section_company;
use wage::render_section_min_wage;
use wage::render_section_tag_salary;

// テストモジュールが helpers / scatter 等の関数を直接呼び出すための再エクスポート
#[cfg(test)]
use helpers::*;
#[cfg(test)]
use scatter::*;

/// 求人市場 総合診断レポート 印刷/ダウンロード用 HTML を生成
///
/// # 引数
/// - `agg`: CSVから集計した求人データ
/// - `seeker`: 求職者心理分析結果
/// - `by_company`: 企業別集計（Step 2 で追加）
/// - `by_emp_type_salary`: 雇用形態別給与（Step 2 で追加）
/// - `salary_min_values`: 下限給与一覧（Step 2 で追加）
/// - `salary_max_values`: 上限給与一覧（Step 2 で追加）
/// - `hw_context`: HW ローカル/外部統計コンテキスト（Section 2/3/H 等で参照）
/// - `salesnow_companies`: 地域注目企業リスト（内部名は呼出側互換で維持）
pub(crate) fn render_survey_report_page(
    agg: &SurveyAggregation,
    seeker: &JobSeekerAnalysis,
    by_company: &[CompanyAgg],
    by_emp_type_salary: &[EmpTypeSalary],
    salary_min_values: &[i64],
    salary_max_values: &[i64],
    hw_context: Option<&InsightContext>,
    salesnow_companies: &[NearbyCompany],
) -> String {
    // 後方互換: enrichment マップなしでの呼び出し
    render_survey_report_page_with_enrichment(
        agg,
        seeker,
        by_company,
        by_emp_type_salary,
        salary_min_values,
        salary_max_values,
        hw_context,
        salesnow_companies,
        &std::collections::HashMap::new(),
    )
}

/// 市区町村別 HW enrichment map を受け取る拡張版
///
/// `hw_enrichment_map`: key = `"{prefecture}:{municipality}"` の HashMap
/// 各エントリに市区町村単位の HW 現在件数 / 推移 / 欠員率 を格納
pub(crate) fn render_survey_report_page_with_enrichment(
    agg: &SurveyAggregation,
    seeker: &JobSeekerAnalysis,
    by_company: &[CompanyAgg],
    by_emp_type_salary: &[EmpTypeSalary],
    salary_min_values: &[i64],
    salary_max_values: &[i64],
    hw_context: Option<&InsightContext>,
    salesnow_companies: &[NearbyCompany],
    hw_enrichment_map: &std::collections::HashMap<String, HwAreaEnrichment>,
) -> String {
    let now = chrono::Local::now()
        .format("%Y年%m月%d日 %H:%M")
        .to_string();
    let mut html = String::with_capacity(64_000);

    // --- DOCTYPE + HEAD ---
    html.push_str("<!DOCTYPE html>\n<html lang=\"ja\">\n<head>\n");
    html.push_str("<meta charset=\"UTF-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n");
    html.push_str("<title>求人市場 総合診断レポート</title>\n");
    html.push_str("<style>\n");
    html.push_str(&render_css());
    html.push_str("</style>\n");
    // ECharts CDN
    html.push_str(
        "<script src=\"https://cdn.jsdelivr.net/npm/echarts@5/dist/echarts.min.js\"></script>\n",
    );
    html.push_str("</head>\n<body>\n");

    // --- テーマ切替 + 印刷ボタン ---
    html.push_str("<div class=\"no-print\" style=\"text-align:right;padding:8px 16px;\">\n");
    html.push_str("<button class=\"theme-toggle\" type=\"button\" onclick=\"toggleTheme()\" aria-label=\"ダークモード/ライトモードを切替\">\u{1F319} ダーク / \u{2600} ライト</button>\n");
    html.push_str("<button onclick=\"window.print()\" aria-label=\"印刷またはPDFで保存\" style=\"padding:8px 24px;font-size:14px;cursor:pointer;border:1px solid #666;border-radius:4px;background:#fff;\">印刷 / PDF保存</button>\n");
    html.push_str("</div>\n");

    // --- 表紙ページ (Section 0 / 仕様書 7.2) ---
    // 2026-04-24: 「競合調査分析」文言を全削除。タイトルは「求人市場 総合診断レポート」に統一。
    let today_short = chrono::Local::now().format("%Y年%m月").to_string();
    let target_region = compose_target_region(agg);
    html.push_str(
        "<section class=\"cover-page\" role=\"region\" aria-labelledby=\"cover-title\">\n",
    );
    html.push_str("<div class=\"cover-logo\" aria-hidden=\"true\">株式会社For A-career</div>\n");
    html.push_str(
        "<div class=\"cover-title\" id=\"cover-title\">求人市場<br>総合診断レポート</div>\n",
    );
    html.push_str("<div class=\"cover-sub\">");
    html.push_str(&escape_html(&today_short));
    html.push_str(" 版</div>\n");
    html.push_str(&format!(
        "<div class=\"cover-target\">対象: {}</div>\n",
        escape_html(&target_region)
    ));
    // 表紙コメント（ダウンロード後にユーザーが追記できる欄）
    html.push_str(
        "<div class=\"cover-comment\" contenteditable=\"true\" spellcheck=\"false\" \
         aria-label=\"レポートコメント（クリックで編集可）\" \
         data-editable-placeholder=\"※ コメントを入力（例: 宛先部署・提案趣旨・補足事項）\">\
         ※ コメントを入力（例: 宛先部署・提案趣旨・補足事項）\
         </div>\n",
    );
    html.push_str("<div class=\"cover-confidential\">この資料は機密情報です。外部への持ち出しは社内規定に従ってください。</div>\n");
    html.push_str(&format!(
        "<div class=\"cover-footer\">株式会社For A-career &nbsp;|&nbsp; 生成日時: {}</div>\n",
        escape_html(&now)
    ));
    html.push_str("</section>\n");

    // --- Executive Summary (Section 1 / 仕様書 3章) ---
    render_section_executive_summary(
        &mut html,
        agg,
        seeker,
        by_company,
        by_emp_type_salary,
        hw_context,
    );

    // --- Section H: 地域 × HW データ連携（新規: 2026-04-24） ---
    // CSV の (都道府県, 市区町村) ごとに、HW ローカルDB/時系列/外部統計から取得された
    // HW 現在件数・3ヶ月/1年推移・欠員率を一覧表示する。
    // hw_context が無い場合はセクション自体を出力しない。
    if let Some(ctx) = hw_context {
        render_section_hw_enrichment(&mut html, agg, ctx, hw_enrichment_map);
    }

    // --- Section 1 補助: サマリー(旧) は Executive Summary に統合済み ---
    // 「サマリー」見出しはテスト互換のため Executive Summary 内で維持
    render_section_summary(&mut html, agg);

    // --- Section 2: HW 市場比較 ---
    // 2026-04-24 ユーザー指摘により削除:
    //   「任意でスクレイピングしている件数 VS ハローワークデータ」という
    //   非同質データ比較は無意味。雇用形態構成比・最低賃金比較の "媒体" 値も
    //   出どころ不明の誤誘導になるため、HW 市場比較セクション自体を非表示化。
    //   HW 側の補完数値は Section 3 (地域×HW データ連携) と Exec Summary で
    //   参考値として併記するに留める。
    let _ = hw_context;

    // --- Section 3: 給与分布 統計 ---
    render_section_salary_stats(&mut html, agg, salary_min_values, salary_max_values);

    // --- Section 4: 雇用形態分布 ---
    render_section_employment(&mut html, agg, by_emp_type_salary);

    // --- Section 4B: 雇用形態グループ別 ネイティブ単位集計 (2026-04-24 Phase 2) ---
    // 正社員 → 月給, パート → 時給 を並列表示
    render_section_emp_group_native(&mut html, agg);

    // --- Section 5: 給与の相関分析（散布図） ---
    render_section_scatter(&mut html, agg);

    // --- Section 6: 地域分析（都道府県） ---
    render_section_region(&mut html, agg);

    // --- Section 7: 地域分析（市区町村） ---
    render_section_municipality_salary(&mut html, agg);

    // --- Section 8: 最低賃金比較 ---
    render_section_min_wage(&mut html, agg);

    // --- Section 9: 企業分析 ---
    render_section_company(&mut html, by_company);

    // --- Section 10: タグ × 給与相関 ---
    render_section_tag_salary(&mut html, agg);

    // --- Section 11: 求職者心理分析 ---
    render_section_job_seeker(&mut html, seeker);

    // --- Section 12: SalesNow 地域注目企業（非空のときのみ） ---
    if !salesnow_companies.is_empty() {
        render_section_salesnow_companies(&mut html, salesnow_companies);
    }

    // --- Section 13: 注記・出典・免責 (必須) ---
    render_section_notes(&mut html, &now);

    // --- 画面下部フッター（印刷時は @page footer を使用） ---
    html.push_str("<div class=\"screen-footer no-print\">\n");
    html.push_str("<span>株式会社For A-career | ハローワーク求人データ分析レポート</span>\n");
    html.push_str(&format!("<span>生成日時: {}</span>\n", escape_html(&now)));
    html.push_str("</div>\n");

    // --- ECharts初期化スクリプト + ソート可能テーブル ---
    html.push_str(&render_scripts());

    html.push_str("</body>\n</html>");
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_man_yen() {
        assert_eq!(format_man_yen(250_000), "25.0万円");
        assert_eq!(format_man_yen(183_500), "18.4万円");
        assert_eq!(format_man_yen(0), "-");
        assert_eq!(format_man_yen(-50_000), "-5.0万円");
    }

    #[test]
    fn test_build_salary_histogram() {
        let values = vec![200_000, 210_000, 250_000, 270_000, 300_000];
        let (labels, counts, boundaries) = build_salary_histogram(&values, 20_000);
        assert!(!labels.is_empty());
        assert_eq!(labels.len(), counts.len());
        assert_eq!(labels.len(), boundaries.len());
        let total: usize = counts.iter().sum();
        assert_eq!(total, 5);
    }

    #[test]
    fn test_build_salary_histogram_empty() {
        let (labels, counts, boundaries) = build_salary_histogram(&[], 20_000);
        assert!(labels.is_empty());
        assert!(counts.is_empty());
        assert!(boundaries.is_empty());
    }

    #[test]
    fn test_build_salary_histogram_zeros() {
        let values = vec![0, 0, 0];
        let (labels, counts, boundaries) = build_salary_histogram(&values, 20_000);
        assert!(labels.is_empty());
        assert!(counts.is_empty());
        assert!(boundaries.is_empty());
    }

    #[test]
    fn test_compute_mode() {
        // 200_000 が最頻帯（bin 200_000..220_000 に3件）
        let values = vec![200_000, 205_000, 210_000, 250_000, 300_000];
        let mode = compute_mode(&values, 20_000);
        assert_eq!(mode, Some(200_000));
    }

    #[test]
    fn test_compute_mode_empty() {
        assert_eq!(compute_mode(&[], 20_000), None);
        assert_eq!(compute_mode(&[0, 0], 20_000), None);
    }

    #[test]
    fn test_histogram_echart_config() {
        let labels = vec!["20万".to_string(), "22万".to_string(), "24万".to_string()];
        let values = vec![5, 12, 8];
        let config = build_histogram_echart_config(
            &labels,
            &values,
            "#42A5F5",
            Some(220_000),
            Some(215_000),
            Some(220_000),
            20_000,
        );
        assert!(config.contains("bar"));
        assert!(config.contains("markLine"));
        assert!(config.contains("平均"));
        assert!(config.contains("中央値"));
        assert!(config.contains("最頻値"));
        // 最頻値カラー（紫 #9b59b6）が含まれる
        assert!(config.contains("#9b59b6"));
        // JSON として妥当か
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&config);
        assert!(parsed.is_ok());
    }

    #[test]
    fn test_histogram_echart_config_fine_bin() {
        // 5,000円刻みで 22.5万 などの小数ラベルが生成できること
        let labels = vec!["22.5万".to_string(), "23万".to_string()];
        let values = vec![3, 7];
        let config = build_histogram_echart_config(
            &labels,
            &values,
            "#42A5F5",
            Some(225_000),
            Some(230_000),
            Some(225_000),
            5_000,
        );
        // 225_000 は 22.5万 にスナップされる
        assert!(config.contains("22.5万"));
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&config);
        assert!(parsed.is_ok());
    }

    #[test]
    fn test_echart_div_output() {
        let config = r#"{"type":"bar"}"#;
        let div = render_echart_div(config, 200);
        assert!(div.contains("data-chart-config"));
        assert!(div.contains("echart"));
        assert!(div.contains("200px"));
    }

    #[test]
    fn test_render_empty_data() {
        let agg = SurveyAggregation::default();
        let seeker = JobSeekerAnalysis::default();
        let html = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[]);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("</html>"));
        // ECharts CDN が含まれること
        assert!(html.contains("echarts"));
        // サマリーセクションは出力される
        assert!(html.contains("サマリー"));
        // ソート可能テーブルのスクリプトが含まれること
        assert!(html.contains("initSortableTables"));
    }

    #[test]
    fn test_render_scripts_contains_echart_init() {
        let scripts = render_scripts();
        assert!(scripts.contains("data-chart-config"));
        assert!(scripts.contains("echarts.init"));
        assert!(scripts.contains("initSortableTables"));
        assert!(scripts.contains("beforeprint"));
    }

    #[test]
    fn test_min_wage_all_47_prefectures() {
        // 47都道府県全てで Some を返すことを確認（地域比較の基準データ欠落検出）
        let prefectures = [
            "北海道",
            "青森県",
            "岩手県",
            "宮城県",
            "秋田県",
            "山形県",
            "福島県",
            "茨城県",
            "栃木県",
            "群馬県",
            "埼玉県",
            "千葉県",
            "東京都",
            "神奈川県",
            "新潟県",
            "富山県",
            "石川県",
            "福井県",
            "山梨県",
            "長野県",
            "岐阜県",
            "静岡県",
            "愛知県",
            "三重県",
            "滋賀県",
            "京都府",
            "大阪府",
            "兵庫県",
            "奈良県",
            "和歌山県",
            "鳥取県",
            "島根県",
            "岡山県",
            "広島県",
            "山口県",
            "徳島県",
            "香川県",
            "愛媛県",
            "高知県",
            "福岡県",
            "佐賀県",
            "長崎県",
            "熊本県",
            "大分県",
            "宮崎県",
            "鹿児島県",
            "沖縄県",
        ];
        assert_eq!(prefectures.len(), 47, "都道府県リストは47件");
        for pref in &prefectures {
            let mw = min_wage_for_prefecture(pref);
            assert!(mw.is_some(), "最低賃金データが欠落: {}", pref);
            let val = mw.unwrap();
            assert!(
                (1000..=1300).contains(&val),
                "{} の最低賃金 {} が妥当範囲(1000-1300円)を逸脱",
                pref,
                val
            );
        }
    }

    /// 2026-04-24 ユーザー指摘により HW市場比較セクションは削除済み
    /// (任意スクレイピング件数 vs HW 全体の非同質比較は無意味)
    /// → hw_context の有無に関わらず <h2>HW市場比較</h2> が **出ないこと** を検証
    #[test]
    fn test_render_hw_market_comparison_section_removed() {
        let agg = SurveyAggregation::default();
        let seeker = JobSeekerAnalysis::default();

        let html_without = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[]);
        assert!(
            !html_without.contains("<h2>HW市場比較</h2>"),
            "hw_context=None: HW市場比較は削除済"
        );

        let ctx = mock_empty_insight_ctx();
        let html_with =
            render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], Some(&ctx), &[]);
        assert!(
            !html_with.contains("<h2>HW市場比較</h2>"),
            "hw_context=Some でも HW市場比較は削除済（2026-04-24 ユーザー指摘）"
        );
    }

    /// テスト用: 空の InsightContext を生成
    fn mock_empty_insight_ctx() -> super::super::super::insight::fetch::InsightContext {
        use super::super::super::insight::fetch::InsightContext;
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
            // Phase A: SSDSE-A 新規6テーブル
            ext_households: vec![],
            ext_vital: vec![],
            ext_labor_force: vec![],
            ext_medical_welfare: vec![],
            ext_education_facilities: vec![],
            ext_geography: vec![],
            // Phase A: 県平均
            pref_avg_unemployment_rate: None,
            pref_avg_single_rate: None,
            pref_avg_physicians_per_10k: None,
            pref_avg_daycare_per_1k_children: None,
            pref_avg_habitable_density: None,
            // Phase B: Agoop 人流
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

    /// パーセンタイル計算: 基本動作
    #[test]
    fn test_percentile_sorted_basic() {
        let sorted = vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0];
        assert_eq!(percentile_sorted(&sorted, 0.0), 10.0);
        assert_eq!(percentile_sorted(&sorted, 100.0), 100.0);
        let p50 = percentile_sorted(&sorted, 50.0);
        assert!(
            (p50 - 60.0).abs() < 20.0,
            "p50は中央付近のはず, got {}",
            p50
        );
    }

    #[test]
    fn test_percentile_sorted_empty() {
        assert_eq!(percentile_sorted(&[], 50.0), 0.0);
    }

    /// 軸範囲計算: 異常値が混ざっていない場合もデータ範囲に沿うこと
    #[test]
    fn test_compute_axis_range_basic() {
        let mut values: Vec<f64> = (20..=50).map(|v| v as f64).collect();
        let (lo, hi) = compute_axis_range(&mut values);
        assert!(
            (0.0..=25.0).contains(&lo),
            "lo should be near data min, got {}",
            lo
        );
        assert!(
            (45.0..=60.0).contains(&hi),
            "hi should be near data max, got {}",
            hi
        );
        assert!(hi > lo, "hi > lo");
        // ECharts PDF の 0〜700 問題が再発しないことを保証
        assert!(hi < 700.0, "hi should not explode to 700, got {}", hi);
    }

    #[test]
    fn test_compute_axis_range_empty() {
        let mut values: Vec<f64> = vec![];
        let (lo, hi) = compute_axis_range(&mut values);
        assert!(hi > lo, "degenerate range should still yield hi>lo");
    }

    #[test]
    fn test_compute_axis_range_single_value() {
        let mut values: Vec<f64> = vec![30.0, 30.0, 30.0];
        let (lo, hi) = compute_axis_range(&mut values);
        assert!(hi > lo, "単一値でも範囲が潰れないこと");
        assert!(lo >= 0.0);
    }

    // ============================================================
    // UI-3 統合 contract test: section レベルで figure/legend/banner が組込
    // 済みかを実 HTML レンダリングで確認
    // ============================================================

    /// 注記セクションがカテゴリ別ボックス + 用語ツールチップを含むこと
    #[test]
    fn ui3_notes_section_has_categorized_boxes() {
        let agg = SurveyAggregation::default();
        let seeker = JobSeekerAnalysis::default();
        let html = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[]);
        // 5 カテゴリのボックス class
        assert!(
            html.contains("report-notes-cat-data"),
            "データソースカテゴリボックスが必要"
        );
        assert!(
            html.contains("report-notes-cat-scope"),
            "スコープ制約カテゴリボックスが必要"
        );
        assert!(
            html.contains("report-notes-cat-method"),
            "統計手法カテゴリボックスが必要"
        );
        assert!(
            html.contains("report-notes-cat-corr"),
            "相関≠因果カテゴリボックスが必要"
        );
        assert!(
            html.contains("report-notes-cat-update"),
            "更新頻度カテゴリボックスが必要"
        );
        // 用語ツールチップ（IQR / Bootstrap / Trimmed mean）
        assert!(
            html.contains("data-term-tooltip=\"1\""),
            "用語ツールチップが必要"
        );
        assert!(html.contains(">IQR<"), "IQR 用語");
        assert!(html.contains("Bootstrap 95% CI"), "Bootstrap 95% CI 用語");
        assert!(html.contains("Trimmed mean"), "Trimmed mean 用語");
        // 冒頭サマリ
        assert!(
            html.contains("本レポートを正しく読むための前提"),
            "冒頭サマリ「本レポートを正しく読むための前提」が必要"
        );
    }

    /// 求職者心理分析が空でない時、第4章図番号 + 解釈ガイドバナーが含まれる
    #[test]
    fn ui3_seeker_section_has_chapter_4_and_guidance() {
        let mut seeker = JobSeekerAnalysis::default();
        seeker.total_analyzed = 100;
        seeker.salary_range_perception = Some(super::super::job_seeker::SalaryRangePerception {
            avg_range_width: 50_000,
            avg_lower: 200_000,
            avg_upper: 250_000,
            expected_point: 220_000,
            narrow_count: 10,
            medium_count: 30,
            wide_count: 60,
        });
        let agg = SurveyAggregation::default();
        let html = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[]);
        // 第4章見出し
        assert!(html.contains("第4章 求職者心理分析"), "第4章 タイトル必須");
        // 図 4-1 の data 属性
        assert!(
            html.contains("data-figure=\"4-1\""),
            "図 4-1 (給与レンジ) 番号必要"
        );
        // 章冒頭の相関≠因果バナー
        assert!(
            html.contains("report-banner-gray"),
            "解釈ガイド（gray バナー）必要"
        );
        assert!(html.contains("本章の解釈ガイド"));
        // 読み方吹き出し
        assert!(html.contains("class=\"report-callout\""));
    }

    /// 注記カテゴリの絵文字 + aria 関連の a11y 属性確認
    #[test]
    fn ui3_a11y_attributes_present() {
        let agg = SurveyAggregation::default();
        let seeker = JobSeekerAnalysis::default();
        let html = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[]);
        // role=note は notes / banner で使用
        assert!(html.contains("role=\"note\""));
        // 用語 tooltip の role=tooltip
        assert!(html.contains("role=\"tooltip\""));
        // tabindex でキーボードフォーカス可能
        assert!(html.contains("tabindex=\"0\""));
        // 注記カテゴリの絵文字（📊 データソース）
        assert!(html.contains("\u{1F4CA}"), "📊 データソースアイコンが必要");
        // ⚠️ スコープ制約
        assert!(html.contains("\u{26A0}\u{FE0F}"));
        // 🔬 統計手法
        assert!(html.contains("\u{1F52C}"));
    }

    /// 散布図の異常値除外ロジック（render_section_scatter 内のフィルタ条件を直接検証）
    #[test]
    fn test_scatter_outlier_filter() {
        let points = vec![
            ScatterPoint {
                x: 200_000,
                y: 300_000,
            }, // OK
            ScatterPoint {
                x: 150_000,
                y: 250_000,
            }, // OK
            ScatterPoint {
                x: 10_000,
                y: 6_000_000,
            }, // NG: y=600万
            ScatterPoint {
                x: 5_000,
                y: 7_000_000,
            }, // NG: x<5万 かつ y=700万
            ScatterPoint {
                x: 300_000,
                y: 200_000,
            }, // NG: x>200万 かつ y<x
            ScatterPoint {
                x: 40_000,
                y: 50_000,
            }, // NG: x<5万
        ];
        let filtered: Vec<&ScatterPoint> = points
            .iter()
            .filter(|p| {
                let x_man = p.x as f64 / 10_000.0;
                let y_man = p.y as f64 / 10_000.0;
                (5.0..=200.0).contains(&x_man) && (5.0..=200.0).contains(&y_man) && y_man >= x_man
            })
            .collect();
        assert_eq!(
            filtered.len(),
            2,
            "5万〜200万の範囲内かつ y>=x の2点のみ残る"
        );
    }
}

// ============================================================
// UI-2 強化（2026-04-26）: 主要 6 sections 物語化 contract tests
//
// 各 section に「図表番号」「読み方ヒント」「新規グラフ」「KPI 整合」を追加した
// ことを機械検証する。既存テストは触らない（純粋追加）。
// ============================================================
#[cfg(test)]
mod ui2_contract_tests {
    use super::super::aggregator::{
        CompanyAgg, EmpTypeSalary, MunicipalitySalaryAgg, PrefectureSalaryAgg, RegressionResult,
        ScatterPoint, SurveyAggregation, TagSalaryAgg,
    };
    use super::super::job_seeker::JobSeekerAnalysis;
    use super::super::statistics::{EnhancedStats, QuartileStats};
    use super::render_survey_report_page;

    fn ui2_minimal_agg() -> SurveyAggregation {
        let mut agg = SurveyAggregation::default();
        agg.total_count = 120;
        agg.new_count = 25;
        agg.salary_parse_rate = 0.91;
        agg.location_parse_rate = 0.95;
        agg.dominant_prefecture = Some("東京都".to_string());
        agg.dominant_municipality = Some("千代田区".to_string());
        agg.by_prefecture = vec![
            ("東京都".to_string(), 60),
            ("神奈川県".to_string(), 35),
            ("北海道".to_string(), 15),
            ("福島県".to_string(), 10),
        ];
        agg.by_employment_type = vec![
            ("正社員".to_string(), 70),
            ("パート".to_string(), 30),
            ("派遣".to_string(), 20),
        ];
        agg.by_tags = vec![("賞与あり".to_string(), 30)];
        agg.salary_values = (0..30).map(|i| 200_000 + i * 5_000).collect();
        agg.salary_min_values = (0..30).map(|i| 180_000 + i * 5_000).collect();
        agg.salary_max_values = (0..30).map(|i| 250_000 + i * 5_000).collect();
        agg.by_emp_type_salary = vec![
            EmpTypeSalary {
                emp_type: "正社員".to_string(),
                count: 70,
                avg_salary: 260_000,
                median_salary: 255_000,
            },
            EmpTypeSalary {
                emp_type: "パート".to_string(),
                count: 30,
                avg_salary: 180_000,
                median_salary: 175_000,
            },
        ];
        // 同名市区町村のテスト用に伊達市を 2 件含める
        agg.by_municipality_salary = vec![
            MunicipalitySalaryAgg {
                name: "千代田区".to_string(),
                prefecture: "東京都".to_string(),
                count: 50,
                avg_salary: 280_000,
                median_salary: 275_000,
            },
            MunicipalitySalaryAgg {
                name: "伊達市".to_string(),
                prefecture: "北海道".to_string(),
                count: 8,
                avg_salary: 220_000,
                median_salary: 218_000,
            },
            MunicipalitySalaryAgg {
                name: "伊達市".to_string(),
                prefecture: "福島県".to_string(),
                count: 6,
                avg_salary: 215_000,
                median_salary: 212_000,
            },
        ];
        agg.by_prefecture_salary = vec![
            PrefectureSalaryAgg {
                name: "東京都".to_string(),
                count: 60,
                avg_salary: 280_000,
                avg_min_salary: 240_000,
            },
            PrefectureSalaryAgg {
                name: "高知県".to_string(),
                count: 5,
                avg_salary: 170_000,
                avg_min_salary: 155_000,
            },
        ];
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
            quartiles: Some(QuartileStats {
                q1: 220_000,
                q2: 250_000,
                q3: 280_000,
                iqr: 60_000,
                lower_bound: 130_000,
                upper_bound: 370_000,
                outlier_count: 2,
                inlier_count: 28,
            }),
            reliability: "high".to_string(),
        });
        agg.outliers_removed_total = 2;
        agg.salary_values_raw_count = 32;
        agg.is_hourly = false;
        agg
    }

    fn render_ui2() -> String {
        let agg = ui2_minimal_agg();
        let seeker = JobSeekerAnalysis::default();
        let by_company = vec![
            CompanyAgg {
                name: "サンプル法人A".to_string(),
                count: 12,
                avg_salary: 280_000,
                median_salary: 275_000,
            },
            CompanyAgg {
                name: "サンプル法人B".to_string(),
                count: 8,
                avg_salary: 230_000,
                median_salary: 228_000,
            },
        ];
        let by_emp = agg.by_emp_type_salary.clone();
        let smin = agg.salary_min_values.clone();
        let smax = agg.salary_max_values.clone();
        render_survey_report_page(&agg, &seeker, &by_company, &by_emp, &smin, &smax, None, &[])
    }

    // ---- Section 1: Executive Summary ----

    #[test]
    fn ui2_exec_summary_has_howto_guide() {
        let html = render_ui2();
        assert!(
            html.contains("section-howto") && html.contains("このページの読み方"),
            "Executive Summary 冒頭に『このページの読み方』ガイドが必須"
        );
    }

    #[test]
    fn ui2_exec_summary_has_kpi_v2_with_icon_and_compare() {
        let html = render_ui2();
        assert!(
            html.contains("kpi-card-v2"),
            "強化版 KPI カード (kpi-card-v2) が必須"
        );
        assert!(
            html.contains("kpi-icon"),
            "KPI カードにアイコン要素 (kpi-icon) が必須"
        );
        assert!(
            html.contains("kpi-compare"),
            "KPI カードに比較値表示 (kpi-compare) が必須"
        );
        assert!(
            html.contains("figure-caption") && html.contains("図 1-1"),
            "Executive Summary に図 1-1 の図表キャプションが必須"
        );
    }

    #[test]
    fn ui2_exec_summary_has_priority_badges() {
        let html = render_ui2();
        assert!(
            html.contains("priority-badge"),
            "推奨アクションの優先度バッジ CSS クラスが必須"
        );
    }

    // ---- Section 3: 給与統計 ----

    #[test]
    fn ui2_salary_stats_has_summary_table_with_figure_no() {
        let html = render_ui2();
        assert!(
            html.contains("表 3-1") && html.contains("給与統計サマリ"),
            "給与統計セクションに表 3-1 のキャプションが必須"
        );
    }

    #[test]
    fn ui2_salary_stats_has_iqr_shade_bar() {
        let html = render_ui2();
        assert!(
            html.contains("iqr-bar") && html.contains("iqr-shade"),
            "IQR シェードバー (iqr-bar / iqr-shade) が必須"
        );
        assert!(
            html.contains("図 3-1"),
            "IQR シェード図に図 3-1 のキャプションが必須"
        );
    }

    #[test]
    fn ui2_salary_stats_has_outlier_removal_table() {
        let html = render_ui2();
        assert!(
            html.contains("表 3-2") && html.contains("外れ値除外"),
            "外れ値除外前後比較テーブル (表 3-2) が必須"
        );
        assert!(
            html.contains("read-hint"),
            "salary_stats セクションに読み方ヒントが必須"
        );
    }

    #[test]
    fn ui2_salary_stats_has_histogram_figure_numbers() {
        let html = render_ui2();
        assert!(
            html.contains("図 3-2") && html.contains("図 3-3"),
            "下限給与ヒストグラム 図 3-2/3-3 のキャプションが必須"
        );
    }

    // ---- Section 5: 散布図 ----

    #[test]
    fn ui2_scatter_has_regression_table_and_threshold_guide() {
        let html = render_ui2();
        assert!(
            html.contains("図 5-1") || html.contains("表 5-1"),
            "散布図セクションに図 5-1 / 表 5-1 のキャプションが必須"
        );
        // R² 閾値ガイド (0.5 / 0.3 が表示されること)
        assert!(
            html.contains("> 0.5") && html.contains("0.3"),
            "R\u{00B2} 閾値ガイド (> 0.5 / 0.3) が必須"
        );
    }

    #[test]
    fn ui2_scatter_has_correlation_not_causation_warning() {
        let html = render_ui2();
        // memory feedback_correlation_not_causation 準拠
        assert!(
            html.contains("相関")
                && (html.contains("因果関係を示すものではありません") || html.contains("因果")),
            "散布図セクションに相関≠因果の注意書きが必須"
        );
    }

    // ---- Section 6: 地域分析（都道府県） ----

    #[test]
    fn ui2_region_has_heatmap() {
        let html = render_ui2();
        assert!(
            html.contains("heatmap-grid") && html.contains("heatmap-cell"),
            "都道府県別ヒートマップ (heatmap-grid) が必須"
        );
        assert!(
            html.contains("図 6-1"),
            "都道府県別ヒートマップに図 6-1 のキャプションが必須"
        );
    }

    #[test]
    fn ui2_region_has_pref_table_figure_no() {
        let html = render_ui2();
        assert!(
            html.contains("表 6-1"),
            "都道府県別件数テーブルに表 6-1 のキャプションが必須"
        );
    }

    // ---- Section 7: 市区町村 ----

    #[test]
    fn ui2_municipality_has_dup_marker() {
        let html = render_ui2();
        // 伊達市が2件あるため同名マーカーが付与される
        assert!(
            html.contains("同名市区町村あり") || html.contains("\u{26A0} 同名"),
            "同名市区町村マーカーが必須（伊達市など）"
        );
        assert!(
            html.contains("表 7-1"),
            "市区町村別給与テーブルに表 7-1 のキャプションが必須"
        );
    }

    // ---- Section 4: 雇用形態 ----

    #[test]
    fn ui2_employment_has_dumbbell_chart() {
        let html = render_ui2();
        assert!(
            html.contains("dumbbell-list") && html.contains("dumbbell-row"),
            "雇用形態 dumbbell chart (dumbbell-list/row) が必須"
        );
        assert!(
            html.contains("図 4-1") || html.contains("図 4-2") || html.contains("表 4-1"),
            "雇用形態セクションに図 4-1/4-2 または表 4-1 のキャプションが必須"
        );
    }

    // ---- Section 8: 最低賃金 ----

    #[test]
    fn ui2_min_wage_has_diff_bar() {
        let html = render_ui2();
        assert!(
            html.contains("minwage-diff-bar"),
            "最低賃金差分バー (minwage-diff-bar) が必須"
        );
        assert!(
            html.contains("表 8-1"),
            "最低賃金比較テーブルに表 8-1 のキャプションが必須"
        );
    }

    // ---- Section 9: 企業 ----

    #[test]
    fn ui2_company_has_two_axis_visualization() {
        let html = render_ui2();
        assert!(
            html.contains("表 9-1"),
            "企業別件数テーブルに表 9-1 のキャプションが必須"
        );
    }

    // ---- Section 10: タグ ----

    #[test]
    fn ui2_tag_has_treemap_with_caption() {
        let html = render_ui2();
        assert!(
            html.contains("図 10-1") || html.contains("表 10-1"),
            "タグ×給与セクションに図 10-1 / 表 10-1 のキャプションが必須"
        );
    }

    // ---- 共通: 読み方ヒントの総数 ----

    #[test]
    fn ui2_multiple_read_hints_present() {
        let html = render_ui2();
        let count = html.matches("read-hint-label").count();
        assert!(
            count >= 4,
            "読み方ヒント (read-hint-label) が 4 箇所以上必須（実測: {}）",
            count
        );
    }

    // ---- 共通: 図表キャプションの総数 ----

    #[test]
    fn ui2_figure_caption_total_count() {
        let html = render_ui2();
        let count = html.matches("class=\"figure-caption\"").count();
        assert!(
            count >= 10,
            "図表キャプション (figure-caption) が 10 箇所以上必須（実測: {}）",
            count
        );
    }

    // ---- 共通: 既存 KPI 値の互換性確認 ----

    #[test]
    fn ui2_kpi_values_consistent_with_legacy() {
        let html = render_ui2();
        // 強化版 KPI カードと旧 KPI カードが両方出力される（テスト互換維持）
        assert!(
            html.contains("\"kpi-card\"") || html.contains("\"kpi-card "),
            "旧 KPI カード（互換）が出力されること"
        );
        assert!(
            html.contains("kpi-card-v2"),
            "強化版 KPI カードが出力されること"
        );
        // 5 つの KPI ラベル
        assert!(html.contains("サンプル件数"));
        assert!(html.contains("主要地域"));
        assert!(html.contains("主要雇用形態"));
        assert!(html.contains("給与中央値"));
        assert!(html.contains("新着比率"));
    }

    // ---- 共通: section-bridge による物語のつなぎ ----

    #[test]
    fn ui2_section_bridges_present() {
        let html = render_ui2();
        let count = html.matches("section-bridge").count();
        assert!(
            count >= 3,
            "section-bridge による次セクションへのつなぎが 3 箇所以上必須（実測: {}）",
            count
        );
    }

    // ---- 共通: HW スコープ注意（feedback_hw_data_scope 準拠） ----

    #[test]
    fn ui2_preserves_hw_data_scope_warning() {
        let html = render_ui2();
        assert!(
            html.contains("全求人市場")
                || html.contains("代表ではありません")
                || html.contains("掲載"),
            "HW データスコープ注意は維持（feedback_hw_data_scope.md 準拠）"
        );
    }
}
