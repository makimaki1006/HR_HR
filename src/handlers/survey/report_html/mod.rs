//! HTMLレポート生成（株式会社For A-career 求人市場 総合診断レポート）
//! 仕様書: docs/pdf_design_spec_2026_04_24.md (2026-04-24 追加要件反映済み)
//! A4縦向き印刷 / ダウンロード後編集 両対応のHTMLとして出力する
//! - 表紙 → Executive Summary → HWデータ連携 → 各セクション(So What付き) → 注記/免責
//! - EChartsインタラクティブチャート + ソート可能テーブル
//! - 印刷時はモノクロ耐性（severityアイコン併記）対応
//! - `contenteditable` により主要コメント欄はダウンロード後にユーザーが編集可能

use super::super::company::fetch::NearbyCompany;
use super::super::helpers::{escape_html, format_number};
use super::super::insight::fetch::InsightContext;
#[cfg(test)]
use super::aggregator::ScatterPoint;
use super::aggregator::{CompanyAgg, EmpTypeSalary, SurveyAggregation};
use super::hw_enrichment::HwAreaEnrichment;
use super::job_seeker::JobSeekerAnalysis;

// ======== サブモジュール宣言 (大規模ファイル分割: C-2) ========
mod demographics;
mod employment;
mod executive_summary;
mod helpers;
mod hw_enrichment;
pub(crate) mod industry_mismatch;
mod lifestyle;
mod market_tightness;
mod notes;
mod region;
mod regional_compare;
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
use demographics::render_section_demographics;
use employment::render_section_emp_group_native;
use employment::render_section_employment;
use helpers::{
    compose_target_region, render_dv2_cover_highlights, render_dv2_section_badge, render_scripts,
};
use industry_mismatch::render_section_industry_mismatch;
use industry_mismatch::render_section_industry_mismatch_csv;
use lifestyle::render_section_lifestyle;
use market_tightness::render_section_market_tightness;
use market_tightness::render_section_market_tightness_with_variant;
use notes::render_section_notes;
use region::render_section_municipality_salary;
use region::render_section_region;
use region::render_section_region_extras;
use salary_stats::render_section_salary_stats;
use salesnow::render_section_company_segments;
use salesnow::render_section_company_segments_with_industry;
use salesnow::render_section_salesnow_companies;
use scatter::render_section_scatter;
use seeker::render_section_job_seeker;
use style::render_css;
use wage::render_section_company;
use wage::render_section_household_vs_salary;
use wage::render_section_min_wage;
use wage::render_section_tag_salary;

// テストモジュールが helpers / scatter 等の関数を直接呼び出すための再エクスポート
#[cfg(test)]
use helpers::*;
#[cfg(test)]
use scatter::*;

/// レポートバリアント (2026-04-29 追加)
///
/// # バリアント
/// - `Full`: HW データ併載 (既存仕様、デフォルト)
/// - `Public`: HW 最小化 + 公開オープンデータ + 地域競合比較を強化
///
/// # 設計意図
/// HW データの公開言及を抑制したい運用と、HW 比較を含む既存ワークフローを
/// 共存させるため、URL クエリ `?variant=full|public` で切替可能とする。
/// 各 section 関数は `ReportVariant` を受け取り、自身の出し分けを判断する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportVariant {
    /// 既存仕様: HW データ併載
    Full,
    /// 公開データ中心: HW 最小化、オープンデータと地域比較を強化
    Public,
}

impl ReportVariant {
    /// クエリ文字列から ReportVariant を解決
    pub fn from_query(s: Option<&str>) -> Self {
        match s {
            Some("public") => Self::Public,
            _ => Self::Full,
        }
    }

    /// クエリ文字列に変換 (URL 切替リンク用)
    pub fn as_query(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Public => "public",
        }
    }

    /// 表示名
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Full => "HW併載版",
            Self::Public => "公開データ中心版",
        }
    }

    /// HW セクションを表示するか
    pub fn show_hw_sections(self) -> bool {
        matches!(self, Self::Full)
    }

    /// アイコン (絵文字)
    pub fn icon(self) -> &'static str {
        match self {
            Self::Full => "\u{1F3E2}",   // 🏢
            Self::Public => "\u{1F30D}", // 🌍
        }
    }

    /// 反対バリアント (切替リンク用)
    pub fn alternative(self) -> Self {
        match self {
            Self::Full => Self::Public,
            Self::Public => Self::Full,
        }
    }

    /// 想定読者・コンテキスト説明
    pub fn description(self) -> &'static str {
        match self {
            Self::Full => "ハローワーク掲載求人と統合分析を含む完全版（社内分析向け）",
            Self::Public => "e-Stat等の公開データを主軸とした版（対外提案向け）",
        }
    }
}

/// バリアントインジケータ + 切替リンク HTML を生成
///
/// 印刷レポート上部 (web view のみ表示) に「現在のバリアント表示 + 切替リンク」を出力。
/// 印刷時は `.no-print` クラス + `@media print` の両方で非表示化。
///
/// # 設計意図
/// - ユーザーが現在どちらのバリアントを閲覧しているか即座に判別可能にする
/// - ワンクリックで反対バリアントへ切替できる導線を提供
/// - 同じ CSV から異なる視点で 2 バリアント生成可能なことを明示
fn render_variant_indicator(variant: ReportVariant) -> String {
    let alt = variant.alternative();
    let mut html = String::with_capacity(1_200);
    html.push_str(
        "<div class=\"variant-indicator no-print\" role=\"region\" aria-label=\"PDF出力モード切替\">\n",
    );
    html.push_str("<div class=\"variant-indicator-inner\">\n");
    html.push_str(&format!(
        "<span class=\"variant-current\"><span class=\"variant-icon\" aria-hidden=\"true\">{icon}</span>現在: <strong>{name}</strong></span>\n",
        icon = variant.icon(),
        name = escape_html(variant.display_name()),
    ));
    html.push_str(&format!(
        "<span class=\"variant-desc\">{}</span>\n",
        escape_html(variant.description())
    ));
    // 切替リンク: JS で session_id 等の URL パラメータを保持しつつ variant のみ書き換え
    html.push_str(&format!(
        "<a href=\"?variant={tv}\" class=\"variant-switch-link\" \
         data-target-variant=\"{tv}\" \
         onclick=\"switchReportVariant(event, '{tv}')\" \
         aria-label=\"PDF出力モードを{name}に切替\" \
         title=\"同じCSVから異なる視点で生成。両バリアントを試して比較できます\">\
         <span aria-hidden=\"true\">{icon}</span> {name} に切替 \u{2192}\
         </a>\n",
        tv = alt.as_query(),
        name = escape_html(alt.display_name()),
        icon = alt.icon(),
    ));
    html.push_str("</div>\n");
    html.push_str("</div>\n");
    // 切替スクリプト: 現在の URL から variant のみ差し替えて再読み込み
    // （session_id 等の他パラメータを保持するため URL API を利用）
    html.push_str(
        "<script>\n\
         function switchReportVariant(ev, target) {\n\
           if (ev) ev.preventDefault();\n\
           try {\n\
             var url = new URL(window.location.href);\n\
             url.searchParams.set('variant', target);\n\
             window.location.href = url.toString();\n\
           } catch (e) {\n\
             window.location.search = '?variant=' + encodeURIComponent(target);\n\
           }\n\
           return false;\n\
         }\n\
         </script>\n",
    );
    html
}

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
    render_survey_report_page_with_municipalities(
        agg,
        seeker,
        by_company,
        by_emp_type_salary,
        salary_min_values,
        salary_max_values,
        hw_context,
        salesnow_companies,
        hw_enrichment_map,
        &[],
    )
}

/// 2026-04-26 Granularity: 市区町村別デモグラフィック付き拡張版
///
/// ユーザー指摘「都道府県単位は参考にならない」に対応。
/// `municipality_demographics` に CSV 上位 N 市区町村のピラミッド・労働力・教育施設等を渡す。
/// 空 Vec ならデフォルト (都道府県粒度のみ) で動作 (後方互換)。
pub(crate) fn render_survey_report_page_with_municipalities(
    agg: &SurveyAggregation,
    seeker: &JobSeekerAnalysis,
    by_company: &[CompanyAgg],
    by_emp_type_salary: &[EmpTypeSalary],
    salary_min_values: &[i64],
    salary_max_values: &[i64],
    hw_context: Option<&InsightContext>,
    salesnow_companies: &[NearbyCompany],
    hw_enrichment_map: &std::collections::HashMap<String, HwAreaEnrichment>,
    municipality_demographics: &[super::granularity::MunicipalityDemographics],
) -> String {
    // デフォルトは Full バリアント (後方互換)
    render_survey_report_page_with_variant(
        agg,
        seeker,
        by_company,
        by_emp_type_salary,
        salary_min_values,
        salary_max_values,
        hw_context,
        salesnow_companies,
        hw_enrichment_map,
        municipality_demographics,
        ReportVariant::Full,
    )
}

/// 2026-04-29: バリアント切替対応版
///
/// `variant` で `Full` (HW併載) / `Public` (公開データ中心) を切替。
/// 既存の `render_survey_report_page_with_municipalities` は本関数を Full で呼ぶ薄いラッパ。
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_survey_report_page_with_variant(
    agg: &SurveyAggregation,
    seeker: &JobSeekerAnalysis,
    by_company: &[CompanyAgg],
    by_emp_type_salary: &[EmpTypeSalary],
    salary_min_values: &[i64],
    salary_max_values: &[i64],
    hw_context: Option<&InsightContext>,
    salesnow_companies: &[NearbyCompany],
    hw_enrichment_map: &std::collections::HashMap<String, HwAreaEnrichment>,
    municipality_demographics: &[super::granularity::MunicipalityDemographics],
    variant: ReportVariant,
) -> String {
    // 4 セグメント未指定時は空 (後方互換)
    let empty_segments = super::super::company::fetch::RegionalCompanySegments::default();
    render_survey_report_page_with_variant_v2(
        agg,
        seeker,
        by_company,
        by_emp_type_salary,
        salary_min_values,
        salary_max_values,
        hw_context,
        salesnow_companies,
        &empty_segments,
        hw_enrichment_map,
        municipality_demographics,
        variant,
    )
}

/// 2026-04-29 v2: 4 セグメント企業 (大手 / 中堅 / 急成長 / 採用活発) 対応版
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_survey_report_page_with_variant_v2(
    agg: &SurveyAggregation,
    seeker: &JobSeekerAnalysis,
    by_company: &[CompanyAgg],
    by_emp_type_salary: &[EmpTypeSalary],
    salary_min_values: &[i64],
    salary_max_values: &[i64],
    hw_context: Option<&InsightContext>,
    salesnow_companies: &[NearbyCompany],
    salesnow_segments: &super::super::company::fetch::RegionalCompanySegments,
    hw_enrichment_map: &std::collections::HashMap<String, HwAreaEnrichment>,
    municipality_demographics: &[super::granularity::MunicipalityDemographics],
    variant: ReportVariant,
) -> String {
    // v3 を業界フィルタなしで呼ぶ薄いラッパ (後方互換)
    let empty_industry_segments = super::super::company::fetch::RegionalCompanySegments::default();
    render_survey_report_page_with_variant_v3(
        agg,
        seeker,
        by_company,
        by_emp_type_salary,
        salary_min_values,
        salary_max_values,
        hw_context,
        salesnow_companies,
        salesnow_segments,
        &empty_industry_segments,
        None,
        hw_enrichment_map,
        municipality_demographics,
        variant,
    )
}

/// 2026-04-29 v3: 業界フィルタ対応版 (全業界 + 同業界 両方併記)
///
/// 業界フィルタが指定されている場合、salesnow_segments_industry に同業界版を渡す。
/// 未指定の場合、`salesnow_segments_industry` は空 + `industry_filter` は None。
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_survey_report_page_with_variant_v3(
    agg: &SurveyAggregation,
    seeker: &JobSeekerAnalysis,
    by_company: &[CompanyAgg],
    by_emp_type_salary: &[EmpTypeSalary],
    salary_min_values: &[i64],
    salary_max_values: &[i64],
    hw_context: Option<&InsightContext>,
    salesnow_companies: &[NearbyCompany],
    salesnow_segments: &super::super::company::fetch::RegionalCompanySegments,
    salesnow_segments_industry: &super::super::company::fetch::RegionalCompanySegments,
    industry_filter: Option<&str>,
    hw_enrichment_map: &std::collections::HashMap<String, HwAreaEnrichment>,
    municipality_demographics: &[super::granularity::MunicipalityDemographics],
    variant: ReportVariant,
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

    // --- バリアントインジケータ + 切替リンク (2026-04-29) ---
    // web view では現在のバリアントと切替リンクを表示。印刷時は .no-print で非表示。
    html.push_str(&render_variant_indicator(variant));

    // --- 表紙ページ (Section 0 / 仕様書 7.2) ---
    // 2026-04-24: 「競合調査分析」文言を全削除。タイトルは「求人市場 総合診断レポート」に統一。
    // 2026-04-26 Design v2: 3 段構成（タイトル / 対象 / ハイライト KPI）の刷新版表紙を
    //   既存 cover-page の前に追加。既存はテスト互換のため維持。
    let today_short = chrono::Local::now().format("%Y年%m月").to_string();
    let target_region = compose_target_region(agg);

    // dv2 表紙（刷新版: 印刷時の主役）
    html.push_str(
        "<section class=\"dv2-cover\" role=\"region\" aria-labelledby=\"dv2-cover-title\">\n",
    );
    // 上段ヘッダー: ブランド + 生成メタ
    html.push_str("<div class=\"dv2-cover-header\">\n");
    html.push_str("<div class=\"dv2-cover-brand\">株式会社For A-career</div>\n");
    html.push_str(&format!(
        "<div class=\"dv2-cover-meta\">{} 版</div>\n",
        escape_html(&today_short)
    ));
    html.push_str("</div>\n");
    // 中央: タイトル + 対象
    html.push_str("<div class=\"dv2-cover-main\">\n");
    html.push_str("<div>\n");
    html.push_str("<div class=\"dv2-cover-title-accent\" aria-hidden=\"true\"></div>\n");
    html.push_str(
        "<h1 id=\"dv2-cover-title\" class=\"dv2-cover-title\">求人市場<br>総合診断レポート</h1>\n",
    );
    html.push_str(
        "<p class=\"dv2-cover-subtitle\">ハローワーク掲載求人 + アップロード CSV クロス分析</p>\n",
    );
    html.push_str("</div>\n");
    html.push_str(&format!(
        "<div class=\"dv2-cover-target\">対象: {}</div>\n",
        escape_html(&target_region)
    ));

    // 下段: ハイライト 3 KPI
    let hl_count = format_number(agg.total_count as i64);
    let hl_region = target_region.clone();
    let hl_median = match &agg.enhanced_stats {
        Some(s) if s.count > 0 => {
            if agg.is_hourly {
                format!("{}", format_number(s.median))
            } else {
                format!("{:.1}", s.median as f64 / 10_000.0)
            }
        }
        _ => "-".to_string(),
    };
    let hl_median_unit = match &agg.enhanced_stats {
        Some(s) if s.count > 0 => {
            if agg.is_hourly {
                "円/時"
            } else {
                "万円"
            }
        }
        _ => "",
    };
    render_dv2_cover_highlights(
        &mut html,
        &[
            ("サンプル件数", &hl_count, "件"),
            ("主要地域", &hl_region, ""),
            ("給与中央値", &hl_median, hl_median_unit),
        ],
    );
    html.push_str("</div>\n"); // /dv2-cover-main

    // フッター: 機密 + 生成日時
    html.push_str("<div class=\"dv2-cover-footer\">\n");
    html.push_str(
        "<span>この資料は機密情報です。外部への持ち出しは社内規定に従ってください。</span>\n",
    );
    html.push_str(&format!(
        "<span>生成日時: {}</span>\n",
        escape_html(&now)
    ));
    html.push_str("</div>\n");
    html.push_str("</section>\n");

    // 既存 cover-page (テスト互換のため維持。印刷時は dv2-cover が page-break-after で先に描画され
    // 続く既存表紙が次ページに重ねて出るのを避けるため、画面表示のみにする)
    html.push_str("<style>@media print { .cover-page.cover-legacy { display: none !important; } }</style>\n");
    html.push_str(
        "<section class=\"cover-page cover-legacy no-print-cover\" role=\"region\" aria-labelledby=\"cover-title\" aria-hidden=\"true\" style=\"display:none\">\n",
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
    // 2026-04-29 (variant): Public バリアントでは HW 言及を最小化するため非表示。
    if variant.show_hw_sections() {
        if let Some(ctx) = hw_context {
            render_section_hw_enrichment(&mut html, agg, ctx, hw_enrichment_map);
        }
    }

    // --- Section 1 補助: サマリー(旧) は Executive Summary に統合済み ---
    // 「サマリー」見出しはテスト互換のため Executive Summary 内で維持
    render_section_summary(&mut html, agg);

    // --- Section 2: HW 市場比較 ---
    // 2026-04-24 ユーザー指摘により削除:
    //   「アップロード CSV 件数 VS ハローワークデータ」という
    //   非同質データ比較は無意味。雇用形態構成比・最低賃金比較の "媒体" 値も
    //   出どころ不明の誤誘導になるため、HW 市場比較セクション自体を非表示化。
    //   HW 側の補完数値は Section 3 (地域×HW データ連携) と Exec Summary で
    //   参考値として併記するに留める。
    let _ = hw_context;

    // --- Section 3: 給与分布 統計 ---
    render_section_salary_stats(&mut html, agg, salary_min_values, salary_max_values);

    // --- Section 4MT: 採用市場 逼迫度 ---
    // 4 軸 (有効求人倍率 / HW 欠員補充率 / 失業率 / 離職率) の複合指標
    // 2026-04-29 (variant): Public バリアントでは HW 欠員補充率を除外する
    //   バージョンに切替 (signature 互換のため variant を渡す)
    render_section_market_tightness_with_variant(&mut html, hw_context, variant);

    // --- Section 4B (CR-9 / 2026-04-27): 産業ミスマッチ警戒 ---
    // 地域就業者構成 (国勢調査) と HW 求人産業構成のギャップを表で可視化。
    // 2026-04-29 (variant): Public では HW 求人を CSV 媒体掲載分に置換するため、
    //   variant=Public のときは別 section (CSV vs 国勢調査) を使う。
    if let Some(ctx) = hw_context {
        match variant {
            ReportVariant::Full => {
                render_section_industry_mismatch(
                    &mut html,
                    &ctx.ext_industry_employees,
                    &ctx.hw_industry_counts,
                );
            }
            ReportVariant::Public => {
                // CSV 媒体掲載 vs 国勢調査
                render_section_industry_mismatch_csv(
                    &mut html,
                    &ctx.ext_industry_employees,
                    agg,
                );
            }
        }
    }

    // --- Section 4P (2026-04-29): 対象地域 vs 競合地域 多面比較 (Public 専用) ---
    // CSV 件数 + 外部統計 (デモグラ × サイコグラ × ジオグラ) の 3 軸で対象地域を全国平均と
    // 比較し、媒体ミックス・訴求軸選定の参考材料として提示する。
    // HW 求人データを一切使用せず、Public バリアント (HW 言及最小化) でのみ表示。
    if matches!(variant, ReportVariant::Public) {
        if let Some(ctx) = hw_context {
            regional_compare::render_section_regional_compare(&mut html, ctx, agg);
        }
    }

    // --- Section 3D (Impl-2 案 D-1/D-2/#10/#17): 人材デモグラフィック ---
    // 年齢層ピラミッド + 学歴分布 + 採用候補プール (失業者) + 教育施設密度を
    // 1 つの section で「対象地域の労働力候補者」の俯瞰として表示。
    // hw_context が None もしくは関連データ全空なら非表示。
    if let Some(ctx) = hw_context {
        render_section_demographics(&mut html, ctx);
    }

    // --- Section 3D-M (2026-04-26 Granularity): 主要市区町村別 デモグラフィック ---
    // ユーザー指摘「都道府県単位は参考にならない」に対応。
    // CSV 件数上位 3 市区町村について、市区町村粒度の年齢ピラミッド / 失業者 / 教育施設を
    // 横並びカードで表示する。municipality_demographics が空ならスキップ。
    if !municipality_demographics.is_empty() {
        demographics::render_section_demographics_by_municipality(
            &mut html,
            municipality_demographics,
        );
    }

    // --- Section 4: 雇用形態分布 ---
    render_section_employment(&mut html, agg, by_emp_type_salary);

    // --- Section 4B: 雇用形態グループ別 ネイティブ単位集計 (2026-04-24 Phase 2) ---
    // 正社員 → 月給, パート → 時給 を並列表示
    render_section_emp_group_native(&mut html, agg);

    // --- Section 5: 給与の相関分析（散布図） ---
    render_section_scatter(&mut html, agg);

    // --- Section 6: 地域分析（都道府県） ---
    render_section_region(&mut html, agg);

    // --- Section 6 補助 (Impl-1 案 #18 / D-4): 地域特性 補足（地理 / 人口構成） ---
    // 可住地密度 + 都市分類 + 高齢化率 KPI。ctx が無い、もしくは関連データ全空なら非表示。
    if let Some(ctx) = hw_context {
        render_section_region_extras(&mut html, ctx);
    }

    // --- Section 7: 地域分析（市区町村） ---
    render_section_municipality_salary(&mut html, agg);

    // --- Section 8: 最低賃金比較 ---
    render_section_min_wage(&mut html, agg);

    // --- Section 8 補助 (Impl-3 案 #8): 世帯所得 vs CSV 給与競争力（図 8-2） ---
    // 最低賃金比較（表 8-1: 法定下限）に対し、世帯月平均支出（実生活コスト）との
    // 比率を補完表示する。hw_context が無い、または ext_household_spending が空なら非表示。
    render_section_household_vs_salary(&mut html, agg, hw_context);

    // --- Section 8B (Impl-3 案 P-1/P-2): ライフスタイル特性 ---
    // 社会生活参加率（v2_external_social_life）と
    // ネット利用率（v2_external_internet_usage）から
    // オフ活動量・オンライン媒体適合度を提示。
    render_section_lifestyle(&mut html, hw_context);

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

    // --- Section 12B (2026-04-29): SalesNow 4 セグメント (規模上位/中規模/人員拡大/求人積極) ---
    // ユーザー指摘:
    // > 業界絞込/絞らない の両方を表示したい (異業種ベンチマーク + 同業界比較 を併記)
    //
    // 業界指定時: 全業界版 + 同業界版 の両方を並列表示
    // 業界未指定時: 全業界版のみ
    if !salesnow_segments.is_empty() {
        render_section_company_segments_with_industry(
            &mut html,
            salesnow_segments,
            salesnow_segments_industry,
            industry_filter,
        );
    }

    // --- Section 13: 注記・出典・免責 (必須) ---
    render_section_notes(&mut html, &now);

    // --- 画面下部フッター（印刷時は @page footer を使用） ---
    html.push_str("<div class=\"screen-footer no-print\">\n");
    html.push_str("<span>株式会社For A-career | 求人市場 総合診断レポート</span>\n");
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
    /// (アップロード CSV 件数 vs HW 全体の非同質比較は無意味)
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
            ext_education: vec![],
            ext_industry_employees: vec![],
            hw_industry_counts: vec![],
            // Impl-3: ライフスタイル
            ext_social_life: vec![],
            ext_internet_usage: vec![],
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
        // 注記カテゴリのテキスト見出し (絵文字は装飾削除済、見出しテキストで識別)
        assert!(html.contains("データソース"), "データソース見出し");
        // ⚠️ スコープ制約 (警告アイコンは機能的に残す)
        assert!(html.contains("\u{26A0}\u{FE0F}"));
        assert!(html.contains("スコープ制約"), "スコープ制約見出し");
        // 統計手法 (絵文字削除、テキスト見出しで識別)
        assert!(html.contains("統計手法"), "統計手法見出し");
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

// ============================================================
// Readability 強化（2026-04-26）: 見やすさ徹底改善 contract tests
//
// PDF 15 ページ → 10-12 ページへの圧縮を狙う CSS / 構造変更を
// 機械検証する。情報を減らさず、見やすさを上げる方針。
//   - <details> による折りたたみ
//   - 章番号統一・印刷時の重複 KPI 非表示
//   - page-break-before / break-after
//   - 注記のフッター集約ポインタ
// ============================================================
#[cfg(test)]
mod readability_contract_tests {
    use super::super::aggregator::SurveyAggregation;
    use super::super::job_seeker::JobSeekerAnalysis;
    use super::render_survey_report_page;

    fn render_minimal() -> String {
        let agg = SurveyAggregation::default();
        let seeker = JobSeekerAnalysis::default();
        render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[])
    }

    /// (1) Executive Summary に折りたたみ details が存在する
    #[test]
    fn readability_collapsible_guide_present() {
        let html = render_minimal();
        assert!(
            html.contains("<details class=\"collapsible-guide\""),
            "<details class=\"collapsible-guide\"> による折りたたみが必須"
        );
        assert!(
            html.contains("クリックで開閉") || html.contains("クリックで展開"),
            "summary テキストにクリックヒントが必須"
        );
    }

    /// (2) 重複 KPI grid が legacy class でマークされている（印刷時非表示）
    #[test]
    fn readability_legacy_kpi_grid_marked() {
        let html = render_minimal();
        assert!(
            html.contains("exec-kpi-grid-legacy"),
            "旧 KPI grid に exec-kpi-grid-legacy class が必須（印刷時非表示マーカー）"
        );
        // 印刷時非表示 CSS が含まれること
        assert!(
            html.contains(".exec-kpi-grid-legacy { display: none !important; }"),
            "印刷時に旧 KPI を非表示にする CSS rule が必須"
        );
    }

    /// (3) Executive Summary に強制改ページが設定されている
    #[test]
    fn readability_executive_summary_page_break() {
        let html = render_minimal();
        assert!(
            html.contains("page-break-before: always") && html.contains("page-break-after: always"),
            "Executive Summary 1 ページ完結のための page-break ルールが必須"
        );
    }

    /// (4) 注記フッター集約のためのポインタが存在
    #[test]
    fn readability_notes_pointer_present() {
        let html = render_minimal();
        assert!(
            html.contains("notes-pointer"),
            "「詳細は注記参照」notes-pointer class が必須"
        );
        assert!(
            html.contains("第6章 注記"),
            "フッター注記への明示参照が必須（情報削除ではなく集約）"
        );
    }

    /// (5) 章番号統一: 主要 section が「第N章」プレフィックスで始まる
    #[test]
    fn readability_chapter_numbering_consistent() {
        let html = render_minimal();
        // 注記セクションは第6章として統一済み
        assert!(
            html.contains("第6章 注記"),
            "注記セクションに『第6章』プレフィックスが必須"
        );
        // 章番号統一用の chapter-num CSS class が定義されている
        assert!(
            html.contains(".chapter-num") || html.contains("chapter-num"),
            "章番号統一用の chapter-num CSS class が必須"
        );
    }

    /// (6) 印刷時のフォント・余白調整 CSS が存在
    #[test]
    fn readability_print_typography_optimized() {
        let html = render_minimal();
        // 印刷時 font-size 10pt
        assert!(
            html.contains("font-size: 10pt !important"),
            "印刷時の font-size 圧縮（10pt）が必須"
        );
        // 印刷時 dark theme は light に強制
        assert!(
            html.contains("color-scheme: light !important"),
            "印刷時 dark theme を light に強制する CSS が必須"
        );
    }

    /// (7) zebra stripe コントラスト強化
    #[test]
    fn readability_zebra_stripe_enhanced() {
        let html = render_minimal();
        // 既存の薄い #fafafa から #f3f6fb / #eef3fa に強化
        assert!(
            html.contains("#eef3fa") || html.contains("#f3f6fb"),
            "zebra stripe のコントラスト強化色 (#eef3fa / #f3f6fb) が必須"
        );
    }

    /// (8) 折りたたみ details は印刷時に強制展開される
    #[test]
    fn readability_details_open_on_print() {
        let html = render_minimal();
        // 印刷時 summary 非表示 + details-body は強制表示
        assert!(
            html.contains("details.collapsible-guide > summary { display: none; }"),
            "印刷時に summary を非表示にする CSS が必須（本文は強制表示）"
        );
    }

    /// (9) KPI 強調クラス (kpi-emphasized) が定義されている
    #[test]
    fn readability_kpi_emphasized_class_defined() {
        let html = render_minimal();
        assert!(
            html.contains(".kpi-emphasized"),
            "主要 KPI 強調用 .kpi-emphasized CSS class 定義が必須"
        );
    }

    /// (10) 注記情報は削除ではなく折りたたみ集約（feedback_correlation_not_causation 準拠）
    #[test]
    fn readability_no_information_loss() {
        let html = render_minimal();
        // 因果≠相関の警告は維持
        assert!(
            html.contains("相関") && (html.contains("因果") || html.contains("仮説")),
            "相関≠因果の注記は折りたたみ後も維持必須"
        );
        // HW スコープ警告も維持
        assert!(
            html.contains("掲載") || html.contains("代表"),
            "HW スコープ警告は折りたたみ後も維持必須"
        );
    }

    /// (11) 図表とキャプションの分離防止
    #[test]
    fn readability_figure_with_caption_class_defined() {
        let html = render_minimal();
        assert!(
            html.contains(".figure-with-caption") || html.contains("figure-with-caption"),
            "図表+キャプションを分離させない figure-with-caption class 定義が必須"
        );
    }

    /// (12) 既存テスト互換確認: section-howto は引き続き出力される
    #[test]
    fn readability_preserves_legacy_howto_for_tests() {
        let agg = {
            let mut a = SurveyAggregation::default();
            a.total_count = 100;
            a
        };
        let seeker = JobSeekerAnalysis::default();
        let html = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[]);
        // 折りたたみ追加後も既存 section-howto は出力（テスト互換）
        assert!(
            html.contains("section-howto"),
            "既存 section-howto class は互換のため維持"
        );
        assert!(
            html.contains("このページの読み方"),
            "「このページの読み方」テキストは維持"
        );
    }
}

// ============================================================
// Design v2 強化（2026-04-26）: コンサル提案資料品質の刷新版 contract tests
//
// プロフェッショナルなビジュアル刷新を機械検証する。
//   - CSS variables (--dv2-*)
//   - dv2-* 名前空間 class
//   - Section 番号バッジ
//   - 印刷時 design-v2 強制適用
//   - SVG inline icon
//   - 表紙刷新（3 段構成）
// ============================================================
#[cfg(test)]
mod design_v2_contract_tests {
    use super::super::aggregator::SurveyAggregation;
    use super::super::job_seeker::JobSeekerAnalysis;
    use super::helpers::{
        render_dv2_data_bar, render_dv2_icon, render_dv2_progress_bar, render_dv2_trend,
    };
    use super::render_survey_report_page;

    fn render_minimal() -> String {
        let agg = SurveyAggregation::default();
        let seeker = JobSeekerAnalysis::default();
        render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[])
    }

    fn render_with_data() -> String {
        let mut agg = SurveyAggregation::default();
        agg.total_count = 250;
        agg.dominant_prefecture = Some("東京都".to_string());
        agg.dominant_municipality = Some("千代田区".to_string());
        let seeker = JobSeekerAnalysis::default();
        render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[])
    }

    /// (1) CSS variables --dv2-* が定義されている
    #[test]
    fn dv2_css_variables_defined() {
        let html = render_minimal();
        assert!(
            html.contains("--dv2-bg:") && html.contains("--dv2-accent:"),
            "dv2 CSS variables (--dv2-bg / --dv2-accent) が必須"
        );
        assert!(
            html.contains("--dv2-good:") && html.contains("--dv2-warn:") && html.contains("--dv2-crit:"),
            "severity 色変数 (good/warn/crit) が必須"
        );
        assert!(
            html.contains("--dv2-fs-display") && html.contains("--dv2-fs-body"),
            "タイポグラフィ階層変数 (display/body) が必須"
        );
    }

    /// (2) dv2 表紙が 3 段構成 (header / main / footer) で出力される
    #[test]
    fn dv2_cover_three_section_layout() {
        let html = render_minimal();
        assert!(
            html.contains("class=\"dv2-cover\""),
            "dv2-cover クラスが必須"
        );
        assert!(
            html.contains("dv2-cover-header") && html.contains("dv2-cover-main") && html.contains("dv2-cover-footer"),
            "3 段構成 (header / main / footer) が必須"
        );
        assert!(
            html.contains("dv2-cover-title") && html.contains("求人市場"),
            "dv2-cover-title に「求人市場」タイトルが必須"
        );
        assert!(
            html.contains("dv2-cover-subtitle"),
            "dv2-cover-subtitle 副題が必須"
        );
        assert!(
            html.contains("dv2-cover-target"),
            "dv2-cover-target 対象地域が必須"
        );
    }

    /// (3) dv2 表紙ハイライト 3 KPI が含まれる
    #[test]
    fn dv2_cover_has_three_highlight_kpis() {
        let html = render_with_data();
        assert!(
            html.contains("dv2-cover-highlights"),
            "dv2-cover-highlights ラッパーが必須"
        );
        let hl_count = html.matches("class=\"dv2-cover-hl\"").count();
        assert!(
            hl_count >= 3,
            "ハイライト KPI が 3 件以上必須（実測: {}）",
            hl_count
        );
        assert!(html.contains("サンプル件数"), "サンプル件数ハイライト");
        assert!(html.contains("主要地域"), "主要地域ハイライト");
        assert!(html.contains("給与中央値"), "給与中央値ハイライト");
    }

    /// (4) Section 番号バッジが Executive Summary に付与されている
    #[test]
    fn dv2_section_badge_on_exec_summary() {
        let html = render_minimal();
        assert!(
            html.contains("class=\"dv2-section-badge\""),
            "dv2-section-badge class が必須"
        );
        assert!(
            html.contains(">01<"),
            "Executive Summary の Section 番号「01」が必須"
        );
        assert!(
            html.contains("class=\"dv2-section-heading\""),
            "dv2-section-heading ラッパーが必須"
        );
    }

    /// (5) dv2 KPI カードクラスの CSS が定義されている
    #[test]
    fn dv2_kpi_card_css_defined() {
        let html = render_minimal();
        assert!(
            html.contains(".dv2-kpi-card"),
            ".dv2-kpi-card CSS rule が必須"
        );
        assert!(
            html.contains("dv2-kpi-large"),
            "dv2-kpi-large (主要 KPI 強調) が必須"
        );
        assert!(
            html.contains("data-status=\"good\"") || html.contains("[data-status=\"good\"]"),
            "data-status による severity 色分けが必須"
        );
    }

    /// (6) 印刷時に dv2 が主役として有効化される CSS
    /// 2026-04-30: @page 重複定義を撤去したため、L42 の単一定義 (margin: 10mm 8mm 12mm 8mm)
    /// と L46-55 のフッター定義を確認する。横幅 8mm で本文幅 194mm を確保。
    #[test]
    fn dv2_print_mode_activated() {
        let html = render_minimal();
        assert!(
            html.contains("margin: 10mm 8mm 12mm 8mm"),
            "印刷時 A4 余白 (上 10mm / 左右 8mm / 下 12mm) が必須"
        );
        assert!(
            html.contains("@bottom-left") && html.contains("求人市場 総合診断レポート"),
            "印刷時の bottom-left footer (会社名 + レポート名) が必須"
        );
        assert!(
            html.contains("@bottom-right") && html.contains("counter(page)"),
            "印刷時の bottom-right footer (ページ番号) が必須"
        );
    }

    /// (7) dv2 helpers: SVG inline icon (4 種) が描画できる
    #[test]
    fn dv2_svg_inline_icons_render() {
        let check = render_dv2_icon("check");
        let warn = render_dv2_icon("warn");
        let crit = render_dv2_icon("crit");
        let info = render_dv2_icon("info");
        assert!(check.contains("<svg") && check.contains("dv2-icon-check"));
        assert!(warn.contains("<svg") && warn.contains("dv2-icon-warn"));
        assert!(crit.contains("<svg") && crit.contains("dv2-icon-crit"));
        assert!(info.contains("<svg") && info.contains("dv2-icon-info"));
        assert!(check.contains("aria-hidden=\"true\""));
        assert!(check.contains("<path"));
    }

    /// (8) dv2 データバー: value/max からパーセント width を計算
    #[test]
    fn dv2_data_bar_renders_correct_percentage() {
        let bar = render_dv2_data_bar(50.0, 100.0, "");
        assert!(
            bar.contains("width:50.0%"),
            "50/100 → 50.0% の width が必須"
        );
        assert!(bar.contains("dv2-databar"));
        let bar_good = render_dv2_data_bar(75.0, 100.0, "good");
        assert!(
            bar_good.contains("data-tone=\"good\""),
            "tone=good 属性が必須"
        );
        let bar_zero = render_dv2_data_bar(50.0, 0.0, "");
        assert!(
            bar_zero.contains("width:0.0%"),
            "max=0 のとき width=0% にフォールバック"
        );
    }

    /// (9) dv2 進捗バー: aria-valuenow / aria-valuemax 等の a11y 属性
    #[test]
    fn dv2_progress_bar_has_a11y_attributes() {
        let mut html = String::new();
        render_dv2_progress_bar(&mut html, 65.0, "65%");
        assert!(html.contains("role=\"progressbar\""), "role=progressbar");
        assert!(html.contains("aria-valuenow=\"65\""), "aria-valuenow=65");
        assert!(html.contains("aria-valuemin=\"0\""), "aria-valuemin");
        assert!(html.contains("aria-valuemax=\"100\""), "aria-valuemax");
        assert!(html.contains("dv2-progress-fill"), "fill 要素");
        assert!(html.contains(">65%<"), "ラベル表示");
    }

    /// (10) dv2 トレンド矢印: up/down/flat の 3 種
    #[test]
    fn dv2_trend_arrows_three_directions() {
        let up = render_dv2_trend("up", "+5.2%");
        let down = render_dv2_trend("down", "-3.1%");
        let flat = render_dv2_trend("flat", "±0.0%");
        assert!(up.contains("\u{2191}"), "↑ (U+2191) が必須");
        assert!(down.contains("\u{2193}"), "↓ (U+2193) が必須");
        assert!(flat.contains("\u{2192}"), "→ (U+2192) が必須");
        assert!(up.contains("dv2-trend-up"));
        assert!(down.contains("dv2-trend-down"));
        assert!(flat.contains("dv2-trend-flat"));
        assert!(up.contains("aria-label=\"上昇\""));
    }

    /// (11) Indigo accent カラー (#4f46e5) が CSS に存在
    #[test]
    fn dv2_accent_color_indigo_defined() {
        let html = render_minimal();
        assert!(
            html.contains("#4f46e5") || html.contains("#4F46E5"),
            "indigo accent color (#4f46e5) が必須"
        );
    }

    /// (12) 既存 cover-page は印刷時非表示にされる
    #[test]
    fn dv2_legacy_cover_hidden_in_print() {
        let html = render_minimal();
        assert!(
            html.contains("cover-legacy"),
            "既存 cover-page は cover-legacy class でマーキング"
        );
        assert!(
            html.contains(".cover-page.cover-legacy { display: none !important; }"),
            "印刷時の legacy 表紙非表示 CSS が必須"
        );
    }

    /// (13) タイポグラフィ階層: 4 階層の font-size が CSS variable 化されている
    #[test]
    fn dv2_typography_four_tier_hierarchy() {
        let html = render_minimal();
        assert!(html.contains("--dv2-fs-display"), "Display 階層");
        assert!(html.contains("--dv2-fs-heading"), "Heading 階層");
        assert!(html.contains("--dv2-fs-body"), "Body 階層");
        assert!(html.contains("--dv2-fs-caption"), "Caption 階層");
        assert!(
            html.contains("tabular-nums"),
            "tabular-nums (等幅数字) が必須"
        );
    }

    /// (14) dv2 アクセントバー (タイトル下の 4px 縦線)
    #[test]
    fn dv2_cover_title_accent_bar_present() {
        let html = render_minimal();
        assert!(
            html.contains("dv2-cover-title-accent"),
            "dv2-cover-title-accent (タイトル装飾バー) が必須"
        );
    }

    /// (15) memory ルール準拠: 因果断定回避 + HW スコープは維持
    #[test]
    fn dv2_preserves_memory_rules() {
        let html = render_minimal();
        assert!(
            html.contains("相関") && (html.contains("因果") || html.contains("仮説")),
            "相関≠因果の注記は刷新後も維持必須"
        );
        assert!(
            html.contains("掲載") || html.contains("代表"),
            "HW スコープ警告は刷新後も維持必須"
        );
    }

    /// (16) dv2 アクションカード CSS が定義されている
    #[test]
    fn dv2_action_card_css_defined() {
        let html = render_minimal();
        assert!(
            html.contains(".dv2-action-card"),
            ".dv2-action-card CSS が必須"
        );
        assert!(
            html.contains("data-priority=\"now\"") || html.contains("[data-priority=\"now\"]"),
            "data-priority による優先度色分けが必須"
        );
    }

    /// (17) Noto Sans JP が印刷時に指定される
    #[test]
    fn dv2_print_typography_noto_sans_jp() {
        let html = render_minimal();
        assert!(
            html.contains("Noto Sans JP"),
            "Noto Sans JP (印刷時の本文フォント) が必須"
        );
    }

    /// (18) 既存 908 テスト互換: 主要 KPI ラベルが維持されている
    #[test]
    fn dv2_preserves_existing_kpi_labels() {
        let html = render_with_data();
        assert!(html.contains("サンプル件数"), "サンプル件数 (互換)");
        assert!(html.contains("主要地域"), "主要地域 (互換)");
        assert!(html.contains("主要雇用形態"), "主要雇用形態 (互換)");
        assert!(html.contains("給与中央値"), "給与中央値 (互換)");
        assert!(html.contains("新着比率"), "新着比率 (互換)");
    }
}

// =============================================================================
// テスト: バリアントインジケータ (2026-04-29)
// =============================================================================

#[cfg(test)]
mod variant_indicator_tests {
    use super::*;

    /// タスク 2: 印刷レポート出力 (Full variant) に「現在: HW併載版」表記
    #[test]
    fn variant_indicator_full_shows_current_label() {
        let html = render_variant_indicator(ReportVariant::Full);
        assert!(
            html.contains("現在:"),
            "Full バリアントインジケータに「現在:」表記必須"
        );
        assert!(
            html.contains("HW併載版"),
            "Full バリアントインジケータに「HW併載版」表記必須"
        );
        // 反対バリアント切替リンク
        assert!(
            html.contains("公開データ中心版"),
            "Full バリアントから「公開データ中心版」へ切替リンク必須"
        );
        assert!(
            html.contains("variant=public"),
            "切替リンクの URL は variant=public"
        );
    }

    /// タスク 2: 印刷レポート出力 (Public variant) に「現在: 公開データ中心版」表記
    #[test]
    fn variant_indicator_public_shows_current_label() {
        let html = render_variant_indicator(ReportVariant::Public);
        assert!(
            html.contains("現在:"),
            "Public バリアントインジケータに「現在:」表記必須"
        );
        assert!(
            html.contains("公開データ中心版"),
            "Public バリアントインジケータに「公開データ中心版」表記必須"
        );
        // 反対バリアント切替リンク
        assert!(
            html.contains("HW併載版"),
            "Public バリアントから「HW併載版」へ切替リンク必須"
        );
        assert!(
            html.contains("variant=full"),
            "切替リンクの URL は variant=full"
        );
    }

    /// .no-print クラスで印刷時非表示が保証されている
    #[test]
    fn variant_indicator_is_hidden_in_print() {
        let html_full = render_variant_indicator(ReportVariant::Full);
        let html_public = render_variant_indicator(ReportVariant::Public);
        assert!(
            html_full.contains("no-print"),
            "Full インジケータは .no-print クラスを持つ"
        );
        assert!(
            html_public.contains("no-print"),
            "Public インジケータは .no-print クラスを持つ"
        );
    }

    /// アクセシビリティ: aria-label が両バリアントで適切に設定されている
    #[test]
    fn variant_indicator_has_accessibility_labels() {
        let html_full = render_variant_indicator(ReportVariant::Full);
        let html_public = render_variant_indicator(ReportVariant::Public);
        assert!(
            html_full.contains("aria-label=\"PDF出力モード切替\""),
            "Full インジケータに region の aria-label 必須"
        );
        assert!(
            html_public.contains("aria-label=\"PDF出力モード切替\""),
            "Public インジケータに region の aria-label 必須"
        );
        // 切替リンクにも aria-label
        assert!(
            html_full.contains("PDF出力モードを公開データ中心版に切替"),
            "Full の切替リンク aria-label 必須"
        );
        assert!(
            html_public.contains("PDF出力モードを HW併載版に切替")
                || html_public.contains("PDF出力モードをHW併載版に切替"),
            "Public の切替リンク aria-label 必須"
        );
    }

    /// 想定読者の説明テキストが含まれる (タスク 4)
    #[test]
    fn variant_indicator_describes_target_audience() {
        let html_full = render_variant_indicator(ReportVariant::Full);
        let html_public = render_variant_indicator(ReportVariant::Public);
        assert!(
            html_full.contains("社内分析向け"),
            "Full は「社内分析向け」と説明"
        );
        assert!(
            html_public.contains("対外提案向け"),
            "Public は「対外提案向け」と説明"
        );
    }

    /// ReportVariant の補助メソッド検証
    #[test]
    fn variant_alternative_swaps_correctly() {
        assert_eq!(ReportVariant::Full.alternative(), ReportVariant::Public);
        assert_eq!(ReportVariant::Public.alternative(), ReportVariant::Full);
    }
}
