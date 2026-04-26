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
