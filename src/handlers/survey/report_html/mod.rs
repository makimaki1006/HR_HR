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
mod labels;
mod region_filter;
mod salary_summary;
// Phase 3 Step 3: 採用マーケットインテリジェンス HTML セクション群
pub(crate) mod industry_mismatch;
mod industry_salary;
mod occupation_salary;
mod lifestyle;
mod market_intelligence;
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

#[cfg(test)]
mod invariant_tests;

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
use industry_salary::render_section_industry_salary;
use occupation_salary::render_section_occupation_salary;
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
    /// Phase 3: 採用マーケットインテリジェンス拡張版
    ///
    /// Full と同じ HW セクションを残しつつ、配信地域ランキング・通勤流入元・
    /// 生活コスト補正後給与魅力度などの追加セクションを表示する (Step 3 で実装予定)。
    ///
    /// 詳細: `docs/SURVEY_MARKET_INTELLIGENCE_PHASE0_2_PREP.md` §5 Step 4
    MarketIntelligence,
}

impl ReportVariant {
    /// クエリ文字列から ReportVariant を解決。
    ///
    /// 既存挙動を完全維持: 未指定 / 不明値 / `"full"` はすべて `Full` にフォールバック。
    pub fn from_query(s: Option<&str>) -> Self {
        match s {
            Some("public") => Self::Public,
            Some("market_intelligence") => Self::MarketIntelligence,
            _ => Self::Full,
        }
    }

    /// クエリ文字列に変換 (URL 切替リンク用)
    pub fn as_query(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Public => "public",
            Self::MarketIntelligence => "market_intelligence",
        }
    }

    /// 表示名
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Full => "HW併載版",
            Self::Public => "公開データ中心版",
            Self::MarketIntelligence => "採用マーケットインテリジェンス版",
        }
    }

    /// HW セクションを表示するか
    ///
    /// 2026-05-08 Round 2-1 仕様変更:
    /// - `Full`: HW 併載 (社内分析向け、HW データを軸に表示)
    /// - `Public`: HW 非表示 (対外提案向け、e-Stat 等の公開データ中心)
    /// - `MarketIntelligence`: **HW 非表示** (採用コンサル拡張版、HW 言及最小化)
    ///
    /// 旧仕様 (`Full | MarketIntelligence` で true) は Round 1-L 監査で
    /// 通常導線 PDF (アクションバー = MI variant) に HW セクションが 7 系統混入する
    /// 問題が確認されたため、ユーザー判断で MI = Public 系 + 採用コンサル拡張に再定義。
    /// 詳細: `docs/PDF_DATA_SOURCE_MIXING_AUDIT_2026_05_08.md`
    pub fn show_hw_sections(self) -> bool {
        matches!(self, Self::Full)
    }

    /// 採用マーケットインテリジェンスセクション (Phase 3 Step 3 で追加予定) を表示するか。
    ///
    /// Step 4 ではこのフックメソッドを定義するのみで、HTML 側の参照はまだしない。
    /// Step 3 でこのメソッドを `if variant.show_market_intelligence_sections() { ... }` で参照する。
    pub fn show_market_intelligence_sections(self) -> bool {
        matches!(self, Self::MarketIntelligence)
    }

    /// アイコン (絵文字)
    pub fn icon(self) -> &'static str {
        match self {
            Self::Full => "\u{1F3E2}",               // 🏢
            Self::Public => "\u{1F30D}",             // 🌍
            Self::MarketIntelligence => "\u{1F4CA}", // 📊
        }
    }

    /// 反対バリアント (切替リンク用)
    ///
    /// 既存挙動 (`Full ↔ Public`) を維持。`MarketIntelligence` の場合は `Full` に戻す
    /// (一般版へのフォールバック)。これにより既存 2 値の `alternative()` テストが影響を受けない。
    pub fn alternative(self) -> Self {
        match self {
            Self::Full => Self::Public,
            Self::Public => Self::Full,
            Self::MarketIntelligence => Self::Full,
        }
    }

    /// 想定読者・コンテキスト説明
    pub fn description(self) -> &'static str {
        match self {
            Self::Full => "ハローワーク掲載求人と統合分析を含む完全版（社内分析向け）",
            Self::Public => "e-Stat等の公開データを主軸とした版（対外提案向け）",
            Self::MarketIntelligence => {
                "採用ターゲット分析を含む拡張版（媒体分析・配信地域提案向け）"
            }
        }
    }
}

/// テーマ切替インジケータ + 全 3 テーマ (default / v8 / v7a) への切替リンクを生成。
///
/// 現在のテーマを表示し、他の 2 テーマへ切替できるリンクを並べる。
/// `?theme=` クエリパラメータのみ書き換え、他のクエリ (session_id 等) は保持。
/// 印刷時は `.no-print` クラスで非表示。
fn render_theme_indicator(current: ReportTheme) -> String {
    let all = [
        ReportTheme::Default,
        ReportTheme::V8WorkingPaper,
        ReportTheme::V7aEditorial,
    ];
    let mut html = String::with_capacity(1_500);
    html.push_str(
        "<div class=\"theme-indicator no-print\" role=\"region\" aria-label=\"レポートデザインテーマ切替\" \
         style=\"max-width:880px;margin:8px auto;padding:10px 14px;border:1px solid #cbd5e1;border-radius:6px;background:#f8fafc;font-size:13px;\">\n"
    );
    html.push_str(&format!(
        "<div style=\"display:flex;align-items:center;gap:12px;flex-wrap:wrap;\">\n\
         <span><strong>デザインテーマ:</strong> 現在 <strong style=\"color:#1e3a8a;\">{name}</strong> ({desc})</span>\n",
        name = escape_html(current.display_name()),
        desc = escape_html(current.description()),
    ));
    html.push_str("<span aria-hidden=\"true\" style=\"color:#94a3b8;\">|</span>\n");
    html.push_str("<span>切替:</span>\n");
    for t in all {
        if t == current {
            html.push_str(&format!(
                "<span style=\"padding:3px 10px;border-radius:4px;background:#1e3a8a;color:#fff;font-weight:700;\">{name}</span>\n",
                name = escape_html(t.display_name()),
            ));
        } else {
            html.push_str(&format!(
                "<a href=\"?theme={tq}\" data-target-theme=\"{tq}\" \
                 onclick=\"switchReportTheme(event,'{tq}')\" \
                 aria-label=\"テーマを{name}に切替\" \
                 style=\"padding:3px 10px;border-radius:4px;background:#fff;color:#1e3a8a;border:1px solid #cbd5e1;text-decoration:none;\">{name}</a>\n",
                tq = t.as_query(),
                name = escape_html(t.display_name()),
            ));
        }
    }
    html.push_str("</div>\n</div>\n");
    // テーマ切替スクリプト: variant と同様に他のクエリパラメータを保持しつつ theme のみ書き換え
    html.push_str(
        "<script>\n\
         function switchReportTheme(ev, target) {\n\
           if (ev) ev.preventDefault();\n\
           try {\n\
             var url = new URL(window.location.href);\n\
             url.searchParams.set('theme', target);\n\
             window.location.href = url.toString();\n\
           } catch (e) {\n\
             window.location.search = '?theme=' + encodeURIComponent(target);\n\
           }\n\
           return false;\n\
         }\n\
         </script>\n",
    );
    html
}

/// テーマ別 CSS を生成 (2026-05-01 追加)
///
/// マークアップは共通で CSS のみ差し替えるため、各テーマは既存 CSS の **後ろに** 追加する形で
/// ルールを上書きする。これにより既存挙動 (Default) は完全維持され、テーマ指定時のみ
/// 上書きが効く。
fn render_css_for_theme(theme: ReportTheme) -> String {
    let mut css = style::render_css();
    match theme {
        ReportTheme::Default => {}
        ReportTheme::V8WorkingPaper => css.push_str(&style::render_theme_v8_workingpaper()),
        ReportTheme::V7aEditorial => css.push_str(&style::render_theme_v7a_editorial()),
    }
    css
}

/// レポートデザインテーマ (2026-05-01 追加)
///
/// 同じ CSV 分析結果を異なるデザインで出力するための切替軸。
/// `ReportVariant` (内容の出し分け) と直交する軸として共存する。
///
/// マークアップ構造はテーマに関わらず共通。CSS のみで見た目を切り替えるため、
/// テストはテーマごとに走らせる必要はなく、Default テーマで担保する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportTheme {
    /// 既存スタイル (デフォルト)
    Default,
    /// Statistical Working Paper 風 (Noto Serif JP + 勝色 + 罫線 severity)
    V8WorkingPaper,
    /// Editorial 風
    V7aEditorial,
}

impl ReportTheme {
    /// クエリ文字列から ReportTheme を解決
    pub fn from_query(s: Option<&str>) -> Self {
        match s {
            Some("v8") | Some("v8_workingpaper") => Self::V8WorkingPaper,
            Some("v7a") | Some("v7a_editorial") => Self::V7aEditorial,
            _ => Self::Default,
        }
    }

    /// クエリ文字列に変換 (URL 切替リンク用)
    pub fn as_query(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::V8WorkingPaper => "v8",
            Self::V7aEditorial => "v7a",
        }
    }

    /// 表示名
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Default => "標準デザイン",
            Self::V8WorkingPaper => "Working Paper 版",
            Self::V7aEditorial => "Editorial 版",
        }
    }

    /// 短い説明
    pub fn description(self) -> &'static str {
        match self {
            Self::Default => "既存のレポートデザイン",
            Self::V8WorkingPaper => "公的統計報告書スタイル (Noto Serif JP + 勝色 + 罫線 severity)",
            Self::V7aEditorial => "編集記事スタイル (大見出し + 余白重視)",
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
    // P0-1 (2026-05-06): MarketIntelligence variant への補助導線は媒体分析タブの
    // アクションバー (render.rs) に「採用コンサルレポート PDF」ボタンとして配置済み。
    // ここ (variant_indicator) に MI 切替リンクを追加すると Full/Public report の
    // 出力に MI 用語が混入し既存 variant_isolation 設計に違反する (T2483 isolation tests)。
    // したがって意図的に MI 切替リンクは追加しない (媒体分析タブから流入する設計)。
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
///
/// theme=Default で従来挙動。v8/v7a 等は CSS のみ差し替えで見た目を切替 (2026-05-01 追加)。
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
    // 後方互換: theme 未指定の呼び出しは Default テーマ。
    render_survey_report_page_with_variant_v3_themed(
        agg,
        seeker,
        by_company,
        by_emp_type_salary,
        salary_min_values,
        salary_max_values,
        hw_context,
        salesnow_companies,
        salesnow_segments,
        salesnow_segments_industry,
        industry_filter,
        hw_enrichment_map,
        municipality_demographics,
        variant,
        ReportTheme::Default,
        None,
        None,
    )
}

/// v3 + theme 対応版 (2026-05-01 追加)
///
/// 同じ CSV 分析結果を異なるデザインで出力するため、現場で見た目を比較可能にする。
/// マークアップ構造は theme に依存せず共通。CSS だけが切り替わる。
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_survey_report_page_with_variant_v3_themed(
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
    theme: ReportTheme,
    // Phase 3 Step 5 Phase 5 (2026-05-04): MarketIntelligence variant 専用 fetch 用 DB 参照。
    // 既存の Full / Public 経路は `None` を渡しても従来通り `default()` で動作する。
    db: Option<&crate::db::local_sqlite::LocalDb>,
    turso: Option<&crate::db::turso_http::TursoDb>,
) -> String {
    let now = chrono::Local::now()
        .format("%Y年%m月%d日 %H:%M")
        .to_string();
    let mut html = String::with_capacity(64_000);

    // --- DOCTYPE + HEAD ---
    html.push_str("<!DOCTYPE html>\n<html lang=\"ja\" data-theme=\"");
    html.push_str(theme.as_query());
    html.push_str("\">\n<head>\n");
    html.push_str("<meta charset=\"UTF-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n");
    html.push_str("<title>求人市場 総合診断レポート</title>\n");
    html.push_str("<style>\n");
    html.push_str(&render_css_for_theme(theme));
    html.push_str("</style>\n");
    // ECharts CDN
    html.push_str(
        "<script src=\"https://cdn.jsdelivr.net/npm/echarts@5/dist/echarts.min.js\"></script>\n",
    );
    html.push_str("</head>\n<body>\n");

    // --- テーマ切替 + 印刷ボタン ---
    html.push_str("<div class=\"no-print\" style=\"text-align:right;padding:8px 16px;\">\n");
    html.push_str("<button class=\"theme-toggle\" type=\"button\" onclick=\"toggleTheme()\" aria-label=\"ダークモード/ライトモードを切替\">\u{1F319} ダーク / \u{2600} ライト</button>\n");
    /* P0-2 (2026-05-06): 印刷ボタンクリック時、ECharts インスタンスを resize() してから
     * window.print() を呼ぶ。これにより印刷時のチャート見切れを防ぐ。
     * `_echarts_instance_` 属性は ECharts が init 時に自動付与する DOM marker。
     * echarts.getInstanceByDom() で外部から chart instance を取得可能。 */
    html.push_str("<button onclick=\"(function(){try{document.querySelectorAll('[_echarts_instance_]').forEach(function(el){var c=window.echarts&&window.echarts.getInstanceByDom(el);if(c)c.resize();});}catch(e){}setTimeout(function(){window.print();},50);})()\" aria-label=\"印刷またはPDFで保存\" style=\"padding:8px 24px;font-size:14px;cursor:pointer;border:1px solid #666;border-radius:4px;background:#fff;\">印刷 / PDF保存</button>\n");
    html.push_str("</div>\n");

    // --- バリアントインジケータ + 切替リンク (2026-04-29) ---
    // web view では現在のバリアントと切替リンクを表示。印刷時は .no-print で非表示。
    html.push_str(&render_variant_indicator(variant));

    // --- テーマ切替リンク (2026-05-01) ---
    // 現場で旧/新デザインを比較できるよう、3 テーマ (default / v8 / v7a) を切替可能にする。
    html.push_str(&render_theme_indicator(theme));

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
    // 2026-05-08 Round 2-1: cover subtitle を variant 別に切替。
    // MI / Public は HW 言及最小化、Full は HW 併載を明示。
    let cover_subtitle = match variant {
        ReportVariant::Full => "ハローワーク掲載求人 + アップロード CSV クロス分析",
        ReportVariant::MarketIntelligence => {
            "アップロード CSV + 公開統計による採用市場・ターゲット分析"
        }
        ReportVariant::Public => "アップロード CSV + 公開統計クロス分析",
    };
    html.push_str(&format!(
        "<p class=\"dv2-cover-subtitle\">{}</p>\n",
        escape_html(cover_subtitle)
    ));
    html.push_str("</div>\n");
    html.push_str(&format!(
        "<div class=\"dv2-cover-target\">対象: {}</div>\n",
        escape_html(&target_region)
    ));

    // 下段: ハイライト 3 KPI
    // 2026-05-08 Round 2-2: SalaryHeadline (single source of truth) 経由で表示し、
    // PDF 内に「給与中央値」が複数値で出る矛盾を防ぐ。ラベルには集計範囲の接尾辞
    // (CSV 全件 / 時給×167h 換算 / 件数最多グループ) が必ず付く。
    let hl_count = format_number(agg.total_count as i64);
    let hl_region = target_region.clone();
    let salary_headline = salary_summary::SalaryHeadline::from_aggregation(agg);
    let cover_hl = salary_headline.cover_highlight_text();
    render_dv2_cover_highlights(
        &mut html,
        &[
            ("サンプル件数", &hl_count, "件"),
            ("主要地域", &hl_region, ""),
            (
                cover_hl.label.as_str(),
                cover_hl.value_text.as_str(),
                cover_hl.unit.as_str(),
            ),
        ],
    );
    html.push_str("</div>\n"); // /dv2-cover-main

    // フッター: 機密 + 生成日時
    html.push_str("<div class=\"dv2-cover-footer\">\n");
    html.push_str(
        "<span>この資料は機密情報です。外部への持ち出しは社内規定に従ってください。</span>\n",
    );
    html.push_str(&format!("<span>生成日時: {}</span>\n", escape_html(&now)));
    html.push_str("</div>\n");
    html.push_str("</section>\n");

    // 既存 cover-page (テスト互換のため維持。印刷時は dv2-cover が page-break-after で先に描画され
    // 続く既存表紙が次ページに重ねて出るのを避けるため、画面表示のみにする)
    html.push_str(
        "<style>@media print { .cover-page.cover-legacy { display: none !important; } }</style>\n",
    );
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
    // 2026-05-08 Round 2-1: variant 引数を追加し、Full 以外では HW 比較系の
    //   優先アクション (給与ギャップ / 雇用形態構成差) を出さないように切替。
    render_section_executive_summary(
        &mut html,
        agg,
        seeker,
        by_company,
        by_emp_type_salary,
        hw_context,
        variant,
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
    // 2026-05-08 Round 2-1: MarketIntelligence も HW 言及最小化方針のため
    //   Public と同じ 3 軸版 (HW 欠員補充率除外) に切替。
    //   _with_variant の内部分岐 (Full|MI → 4 軸 / Public → 3 軸) は触らず、
    //   MI 経路は明示的に Public 用 render を直接呼ぶ。
    if matches!(variant, ReportVariant::MarketIntelligence) {
        market_tightness::render_section_market_tightness_public(&mut html, hw_context);
    } else {
        render_section_market_tightness_with_variant(&mut html, hw_context, variant);
    }

    // --- Section 4B (CR-9 / 2026-04-27): 産業ミスマッチ警戒 ---
    // 地域就業者構成 (国勢調査) と HW 求人産業構成のギャップを表で可視化。
    // 2026-04-29 (variant): Public では HW 求人を CSV 媒体掲載分に置換するため、
    //   variant=Public のときは別 section (CSV vs 国勢調査) を使う。
    // 2026-05-08 Round 2-1: MarketIntelligence も HW 言及最小化方針のため
    //   Public と同じ CSV 経路 (CSV 媒体掲載 vs 国勢調査) に統一。
    if let Some(ctx) = hw_context {
        match variant {
            ReportVariant::Full => {
                render_section_industry_mismatch(
                    &mut html,
                    &ctx.ext_industry_employees,
                    &ctx.hw_industry_counts,
                );
            }
            ReportVariant::Public | ReportVariant::MarketIntelligence => {
                // CSV 媒体掲載 vs 国勢調査
                render_section_industry_mismatch_csv(&mut html, &ctx.ext_industry_employees, agg);
            }
        }
    }

    // --- Section 4B-2 (Round 3-A / 2026-05-06): 産業別就業者 Top10 (国勢調査 2020) ---
    // Round 2-4 セグメント接続監査の P0-3 消化:
    //   region.rs:269 の `render_section_industry_structure` (実装 + unit test 完備) は
    //   印刷版 render パイプラインから呼ばれていなかった。MI variant に 1 行で接続し、
    //   表 6-2「産業別就業者 Top10」を有効化する。
    // データ source は e-Stat 国勢調査 2020 (`v2_external_industry_structure`) で公的統計のみ。
    // HW 求人データ非依存のため Round 2-1/2.5/2.7-B の HW 言及最小化方針と整合。
    // Full / Public は Tab UI 経由 (integration.rs:139) で既に表示済みのため、
    // 印刷経路では MI variant に限定して章追加 (regression 防止)。
    if matches!(variant, ReportVariant::MarketIntelligence) {
        if let Some(ctx) = hw_context {
            let pref = agg
                .dominant_prefecture
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or("対象地域");
            region::render_section_industry_structure(
                &mut html,
                &ctx.ext_industry_employees,
                pref,
            );
        }
    }

    // --- Section 4B-3 (Round 3-B / 2026-05-06): 業界別 給与水準（CSV 推定）---
    // Round 1-E 完全欠落 Top 2 (業界×給与) と Round 2-4 真の未実装 #8 を消化。
    // CSV 由来の `agg.by_company` / `by_tag_salary` を業界大分類で再集計し、
    // 件数 Top 10 業界の加重平均給与を表で提示する。MI variant 専用 (HW 言及最小化方針)。
    // 業界推定は industry_mismatch::map_keyword_to_major_industry を再利用 (新規ロジックなし)。
    if matches!(variant, ReportVariant::MarketIntelligence) {
        render_section_industry_salary(&mut html, agg);
    }

    // --- Section 4B-4 (Round 3-C / 2026-05-09): 職種推定グループ別 給与参考 ---
    // Round 1-E 完全欠落 Top 1 (職種×給与) と Round 2-4 真の未実装 #7 を消化。
    // CSV 由来の `agg.by_tag_salary` (主) / `by_company` (補) を職種推定グループで
    // 再集計し、件数 Top 10 職種の加重平均給与を表で提示する。MI variant 専用。
    // 職種推定は occupation_salary::map_keyword_to_occupation_group (10 グループ)
    // で実施 (industry_mismatch の産業大分類 12 種とは別軸、医療福祉を 6 系に細分化)。
    if matches!(variant, ReportVariant::MarketIntelligence) {
        render_section_occupation_salary(&mut html, agg);
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
    // 2026-05-08 Round 2.5: MarketIntelligence では非表示。
    //   「観測指標」列は HW 求人 + 人員推移 の合成値、注記には HW industry_mapping や
    //   「HW にも掲載」等の HW 文言が密に入っているため、章ごと非表示で HW 言及最小化方針に統一。
    //   Full / Public は既存挙動維持 (regression 防止)。
    if !salesnow_companies.is_empty() && !matches!(variant, ReportVariant::MarketIntelligence) {
        render_section_salesnow_companies(&mut html, salesnow_companies);
    }

    // --- Section 12B (2026-04-29): SalesNow 4 セグメント (規模上位/中規模/人員拡大/求人積極) ---
    // ユーザー指摘:
    // > 業界絞込/絞らない の両方を表示したい (異業種ベンチマーク + 同業界比較 を併記)
    //
    // 業界指定時: 全業界版 + 同業界版 の両方を並列表示
    // 業界未指定時: 全業界版のみ
    //
    // 2026-05-08 Round 2.5: MarketIntelligence では非表示。
    //   「HW 求人継続率」列・「求人積極期 (HW 5 件以上)」セグメント・takeaway 文の
    //   「HW 求人継続率は規模帯間で〜」等が HW データ前提のため、章ごと非表示。
    //   Full / Public は既存挙動維持 (regression 防止)。
    if !salesnow_segments.is_empty() && !matches!(variant, ReportVariant::MarketIntelligence) {
        render_section_company_segments_with_industry(
            &mut html,
            salesnow_segments,
            salesnow_segments_industry,
            industry_filter,
        );
    }

    // --- Phase 3 Step 3 (2026-05-04): 採用マーケットインテリジェンス (variant=market_intelligence 専用) ---
    //
    // `?variant=market_intelligence` のときだけ追加表示する 5 セクション
    // (結論サマリー / 配信地域ランキング / 人材供給 / 給与・生活コスト / 母集団レンジ + 通勤流入元補助)。
    // 既存 Full / Public variant では出力に一切影響しない。
    //
    // 現状: 事前集計テーブル (municipality_recruiting_scores 等) が未投入のため
    // データは空 = placeholder 表示。実データ接続は Phase 3 Step 5+ で対応予定
    // (target_municipalities 抽出 + build_market_intelligence_data の handlers.rs 統合)。
    if variant.show_market_intelligence_sections() {
        // Phase 3 Step 5 Phase 5 (2026-05-04): MarketIntelligence variant 限定で実 fetch を呼ぶ。
        // db が None の場合 (テスト経路など) は従来通り default() にフォールバック。
        // target_municipalities (= 市区町村コード) は handlers.rs では未解決なので空で渡し、
        // dest_pref/dest_muni のみ CSV TOP1 から導出して commute_flow_summary を活性化する。
        // (target_municipalities が空でも、dest_pref/dest_muni があれば早期 return しない設計)
        let mi_data = if let Some(db_ref) = db {
            // Phase 5.5 (2026-05-04): agg から (pref, name) を集めて JIS 5 桁 code を解決し、
            // target_municipalities を完全活性化する。aggregate 行は除外 (area_level='unit')。
            let mut pairs_owned: Vec<(String, String)> = agg
                .by_municipality_salary
                .iter()
                .filter(|m| !m.prefecture.is_empty() && !m.name.is_empty())
                .map(|m| (m.prefecture.clone(), m.name.clone()))
                .collect();
            pairs_owned.sort_unstable();
            pairs_owned.dedup();
            const MAX_TARGETS: usize = 20;
            if pairs_owned.len() > MAX_TARGETS {
                pairs_owned.truncate(MAX_TARGETS);
            }
            let pairs: Vec<(&str, &str)> = pairs_owned
                .iter()
                .map(|(p, n)| (p.as_str(), n.as_str()))
                .collect();

            let resolved_rows =
                super::super::analysis::fetch::fetch_code_master_by_names(db_ref, turso, &pairs);
            let target_codes_owned: Vec<String> = resolved_rows
                .iter()
                .filter_map(|row| {
                    row.get("municipality_code")
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                })
                .collect();
            let target_codes: Vec<&str> = target_codes_owned.iter().map(|s| s.as_str()).collect();

            let (dest_pref, dest_muni) = agg
                .by_municipality_salary
                .iter()
                .find(|m| !m.prefecture.is_empty() && !m.name.is_empty())
                .map(|m| (m.prefecture.as_str(), m.name.as_str()))
                .unwrap_or(("", ""));
            let mut data = market_intelligence::build_market_intelligence_data(
                db_ref,
                turso,
                &target_codes,
                "",
                dest_pref,
                dest_muni,
                10,
            );
            // Round 8 P1-1 (2026-05-10): CSV 由来の自治体集計を mi_data に inject。
            // 4 象限図 (CSV 求人数 × 国勢調査 employees_total) で使う。
            data.csv_municipalities = agg
                .by_municipality_salary
                .iter()
                .filter(|m| !m.prefecture.is_empty() && !m.name.is_empty())
                .map(|m| super::super::analysis::fetch::CsvMunicipalityCell {
                    prefecture: m.prefecture.clone(),
                    name: m.name.clone(),
                    count: m.count,
                    median_salary: m.median_salary,
                })
                .collect();
            data
        } else {
            super::super::analysis::fetch::SurveyMarketIntelligenceData::default()
        };
        market_intelligence::render_section_market_intelligence(&mut html, &mi_data);
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
        // GAS 風 最頻値カラー（青 #3b82f6）が含まれる
        assert!(config.contains("#3b82f6"));
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

    /// PDF印刷時の重なり防止: 中央値/平均/最頻値の label position が異なる位置に
    /// 分散して配置されていることを検証する。
    #[test]
    fn test_histogram_marklines_use_distinct_label_positions() {
        let labels = vec!["20万".to_string(), "22万".to_string(), "24万".to_string()];
        let values = vec![5, 12, 8];
        let config = build_histogram_echart_config(
            &labels,
            &values,
            "#42A5F5",
            Some(220_000), // mean
            Some(220_000), // median (近接値で重なりが起きやすい状況を再現)
            Some(220_000), // mode
            20_000,
        );

        let parsed: serde_json::Value =
            serde_json::from_str(&config).expect("config must be valid JSON");

        let series = parsed["series"][0]["markLine"]["data"]
            .as_array()
            .expect("markLine.data must be array");
        assert_eq!(series.len(), 3, "中央値・平均・最頻値の3線が存在すること");

        // 各 markLine の name と label.position の対応を収集
        let mut positions: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for ml in series {
            let name = ml["name"].as_str().expect("name").to_string();
            let pos = ml["label"]["position"]
                .as_str()
                .expect("label.position must be set")
                .to_string();
            positions.insert(name, pos);
        }

        let median_pos = positions.get("中央値").expect("中央値 must exist");
        let mean_pos = positions.get("平均").expect("平均 must exist");
        let mode_pos = positions.get("最頻値").expect("最頻値 must exist");

        // 3つすべての label position が異なる
        assert_ne!(median_pos, mean_pos, "中央値と平均の position は異なる");
        assert_ne!(mean_pos, mode_pos, "平均と最頻値の position は異なる");
        assert_ne!(median_pos, mode_pos, "中央値と最頻値の position は異なる");

        // 期待値: 縦方向に分散（end / insideEndTop / insideEndBottom）
        assert_eq!(median_pos, "end");
        assert_eq!(mean_pos, "insideEndTop");
        assert_eq!(mode_pos, "insideEndBottom");
    }

    /// ラベル文字（中央値 / 平均 / 最頻値）を削除しないことを保証する回帰テスト。
    #[test]
    fn test_histogram_marklines_preserve_all_three_labels() {
        let labels = vec!["20万".to_string(), "22万".to_string()];
        let values = vec![3, 7];
        let config = build_histogram_echart_config(
            &labels,
            &values,
            "#42A5F5",
            Some(210_000),
            Some(215_000),
            Some(220_000),
            20_000,
        );

        let parsed: serde_json::Value =
            serde_json::from_str(&config).expect("config must be valid JSON");
        let series = parsed["series"][0]["markLine"]["data"]
            .as_array()
            .expect("markLine.data must be array");

        let names: Vec<&str> = series.iter().filter_map(|ml| ml["name"].as_str()).collect();
        assert!(names.contains(&"中央値"), "中央値ラベルが残っていること");
        assert!(names.contains(&"平均"), "平均ラベルが残っていること");
        assert!(names.contains(&"最頻値"), "最頻値ラベルが残っていること");

        // 各 markLine の label.formatter も削除されていない
        for ml in series {
            let formatter = ml["label"]["formatter"].as_str().unwrap_or("");
            assert!(
                !formatter.is_empty(),
                "label.formatter が削除されていない (name={:?})",
                ml["name"]
            );
        }
    }

    /// distance（オフセット距離）も統計種別ごとに異なる値が設定されていることを検証。
    /// 同じ position を使うフォールバック実装で重なりが残らないことを保証する。
    #[test]
    fn test_histogram_marklines_use_distinct_label_distances() {
        let labels = vec!["20万".to_string(), "22万".to_string()];
        let values = vec![5, 8];
        let config = build_histogram_echart_config(
            &labels,
            &values,
            "#42A5F5",
            Some(220_000),
            Some(220_000),
            Some(220_000),
            20_000,
        );
        let parsed: serde_json::Value = serde_json::from_str(&config).unwrap();
        let series = parsed["series"][0]["markLine"]["data"]
            .as_array()
            .expect("markLine.data must be array");

        let mut distances: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();
        for ml in series {
            let name = ml["name"].as_str().unwrap().to_string();
            let dist = ml["label"]["distance"]
                .as_i64()
                .expect("label.distance must be set as integer");
            distances.insert(name, dist);
        }

        let med = *distances.get("中央値").unwrap();
        let avg = *distances.get("平均").unwrap();
        let mod_ = *distances.get("最頻値").unwrap();
        // 3つの distance が全て異なる
        assert_ne!(med, avg);
        assert_ne!(avg, mod_);
        assert_ne!(med, mod_);
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

    // ============================================================
    // Round 2-3 P0 グラフ修正（GAS 風 markLine + 軸表示 + radar center 等）
    // ============================================================

    /// 中央値=緑 / 平均=赤 / 最頻値=青 の GAS 風バッジ色が反映されていること
    #[test]
    fn histogram_marklines_use_distinct_badge_colors() {
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
        let parsed: serde_json::Value = serde_json::from_str(&config).unwrap();
        let series = parsed["series"][0]["markLine"]["data"]
            .as_array()
            .expect("markLine.data must be array");

        let mut color_by_name: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for ml in series {
            let name = ml["name"].as_str().unwrap().to_string();
            let line_color = ml["lineStyle"]["color"].as_str().unwrap().to_string();
            let bg_color = ml["label"]["backgroundColor"]
                .as_str()
                .expect("label.backgroundColor が GAS 風バッジ用に設定されていること")
                .to_string();
            // line color と badge color が同一であること（同色での視覚的一貫性）
            assert_eq!(line_color, bg_color, "line と badge が同色 ({})", name);
            color_by_name.insert(name, bg_color);
        }

        assert_eq!(
            color_by_name.get("中央値").map(String::as_str),
            Some("#22c55e")
        );
        assert_eq!(
            color_by_name.get("平均").map(String::as_str),
            Some("#ef4444")
        );
        assert_eq!(
            color_by_name.get("最頻値").map(String::as_str),
            Some("#3b82f6")
        );
    }

    /// markLine label が「中央値 23.0万」のように値を含む文字列であること
    #[test]
    fn histogram_marklines_include_value_in_label() {
        let labels = vec!["20万".to_string(), "22万".to_string()];
        let values = vec![3, 7];
        let config = build_histogram_echart_config(
            &labels,
            &values,
            "#42A5F5",
            Some(233_000),
            Some(230_000),
            Some(200_000),
            10_000,
        );
        let parsed: serde_json::Value = serde_json::from_str(&config).unwrap();
        let series = parsed["series"][0]["markLine"]["data"].as_array().unwrap();

        let mut formatter_by_name: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for ml in series {
            let name = ml["name"].as_str().unwrap().to_string();
            let f = ml["label"]["formatter"].as_str().unwrap().to_string();
            formatter_by_name.insert(name, f);
        }

        let median_label = formatter_by_name.get("中央値").unwrap();
        let mean_label = formatter_by_name.get("平均").unwrap();
        let mode_label = formatter_by_name.get("最頻値").unwrap();

        // ラベルが「ラベル名 + 数値万」の形式
        assert!(
            median_label.starts_with("中央値 ") && median_label.contains("万"),
            "中央値 label に値が含まれる: {}",
            median_label
        );
        assert!(
            mean_label.starts_with("平均 ") && mean_label.contains("万"),
            "平均 label に値が含まれる: {}",
            mean_label
        );
        assert!(
            mode_label.starts_with("最頻値 ") && mode_label.contains("万"),
            "最頻値 label に値が含まれる: {}",
            mode_label
        );
    }

    /// markLine label がバッジ風 (backgroundColor + borderRadius + padding + 白文字 + bold)
    #[test]
    fn histogram_marklines_render_badge_style_label() {
        let labels = vec!["20万".to_string(), "22万".to_string()];
        let values = vec![3, 7];
        let config = build_histogram_echart_config(
            &labels,
            &values,
            "#42A5F5",
            Some(220_000),
            Some(220_000),
            Some(220_000),
            20_000,
        );
        let parsed: serde_json::Value = serde_json::from_str(&config).unwrap();
        let series = parsed["series"][0]["markLine"]["data"].as_array().unwrap();

        for ml in series {
            let label = &ml["label"];
            assert!(
                label["backgroundColor"].is_string(),
                "backgroundColor が string ({})",
                ml["name"]
            );
            assert_eq!(
                label["color"].as_str(),
                Some("#ffffff"),
                "label の文字色は白 ({})",
                ml["name"]
            );
            assert!(
                label["borderRadius"].as_i64().unwrap_or(0) > 0,
                "borderRadius が 0 超 ({})",
                ml["name"]
            );
            assert!(
                label["padding"].is_array(),
                "padding が array ({})",
                ml["name"]
            );
            assert_eq!(
                label["fontWeight"].as_str(),
                Some("bold"),
                "fontWeight が bold ({})",
                ml["name"]
            );
        }
    }

    /// yAxis.min == 0 (棒高さ誇張防止)
    #[test]
    fn histogram_yaxis_min_is_zero() {
        let labels = vec!["20万".to_string(), "22万".to_string()];
        let values = vec![3, 7];
        let config = build_histogram_echart_config(
            &labels,
            &values,
            "#42A5F5",
            Some(220_000),
            Some(220_000),
            Some(220_000),
            20_000,
        );
        let parsed: serde_json::Value = serde_json::from_str(&config).unwrap();
        assert_eq!(
            parsed["yAxis"]["min"].as_i64(),
            Some(0),
            "yAxis.min が 0 に強制されていること"
        );
    }

    // ============================================================
    // Round 2.7-AC: yAxis 0 強制 bulletproof 化 + ラベル近接統合
    // ============================================================

    /// 全 histogram builder で yAxis.scale が明示的に false (ECharts auto-scale 罠回避)
    #[test]
    fn histogram_yaxis_scale_is_false_explicitly() {
        let labels = vec!["20万".to_string(), "22万".to_string()];
        let values = vec![3, 7];
        let config = build_histogram_echart_config(
            &labels,
            &values,
            "#42A5F5",
            Some(220_000),
            Some(220_000),
            Some(220_000),
            20_000,
        );
        let parsed: serde_json::Value = serde_json::from_str(&config).unwrap();
        assert_eq!(
            parsed["yAxis"]["scale"].as_bool(),
            Some(false),
            "yAxis.scale が false に明示されていること (auto-scale で min:0 が無視される罠の回避)"
        );
    }

    /// yAxis.minInterval が 1 (件数なので小数 tick 抑止)
    #[test]
    fn histogram_yaxis_min_interval_is_one() {
        let labels = vec!["20万".to_string(), "22万".to_string()];
        let values = vec![3, 7];
        let config = build_histogram_echart_config(
            &labels,
            &values,
            "#42A5F5",
            Some(220_000),
            Some(220_000),
            Some(220_000),
            20_000,
        );
        let parsed: serde_json::Value = serde_json::from_str(&config).unwrap();
        assert_eq!(
            parsed["yAxis"]["minInterval"].as_i64(),
            Some(1),
            "yAxis.minInterval が 1 (件数 = 整数なので小数 tick を抑止)"
        );
    }

    /// 3 値が近接 (差 ≤ bin_size * 2) のとき統合ラベル (graphic) が出力される
    #[test]
    fn histogram_uses_combined_label_when_stats_are_close() {
        // 中央値 230,000 / 平均 225,000 / 最頻値 240,000 (差 15,000、bin=10,000)
        // 差 / bin = 1.5 ≤ 2 → 近接
        let labels = vec!["22万".to_string(), "23万".to_string(), "24万".to_string()];
        let values = vec![5, 12, 8];
        let config = build_histogram_echart_config(
            &labels,
            &values,
            "#42A5F5",
            Some(225_000),
            Some(230_000),
            Some(240_000),
            10_000,
        );
        let parsed: serde_json::Value = serde_json::from_str(&config).unwrap();

        let graphic = parsed["graphic"]
            .as_array()
            .expect("graphic が配列であること");
        assert!(
            !graphic.is_empty(),
            "近接時は graphic に統合カードが含まれる"
        );

        // markLine ラベルは show=false (graphic と二重表示にならない)
        let ml = parsed["series"][0]["markLine"]["data"].as_array().unwrap();
        for entry in ml {
            assert_eq!(
                entry["label"]["show"].as_bool(),
                Some(false),
                "近接時は markLine label が非表示 ({})",
                entry["name"]
            );
        }

        // graphic に 3 統計値の名称が含まれること
        let graphic_str = serde_json::to_string(&graphic).unwrap();
        assert!(graphic_str.contains("中央値"), "graphic に中央値が含まれる");
        assert!(graphic_str.contains("平均"), "graphic に平均が含まれる");
        assert!(graphic_str.contains("最頻値"), "graphic に最頻値が含まれる");
    }

    /// 3 値が離れている (差 > bin_size * 2) のとき従来の position 分散ラベルを維持
    #[test]
    fn histogram_uses_distributed_labels_when_stats_are_far() {
        // 中央値 200,000 / 平均 300,000 / 最頻値 500,000 (差 300,000、bin=10,000)
        // 差 / bin = 30 ≫ 2 → 離れている
        let labels = vec!["20万".to_string(), "30万".to_string(), "50万".to_string()];
        let values = vec![3, 7, 2];
        let config = build_histogram_echart_config(
            &labels,
            &values,
            "#42A5F5",
            Some(300_000),
            Some(200_000),
            Some(500_000),
            10_000,
        );
        let parsed: serde_json::Value = serde_json::from_str(&config).unwrap();

        let graphic = parsed["graphic"].as_array().unwrap();
        assert!(graphic.is_empty(), "離れているとき graphic は空");

        // markLine ラベルは show=true (従来通り表示)
        let ml = parsed["series"][0]["markLine"]["data"].as_array().unwrap();
        for entry in ml {
            assert_eq!(
                entry["label"]["show"].as_bool(),
                Some(true),
                "離れているときは markLine label を表示 ({})",
                entry["name"]
            );
        }
    }

    /// stats_are_close ヘルパー単体: 境界条件
    #[test]
    fn stats_are_close_boundary_conditions() {
        // 1 値のみ → false
        assert!(!stats_are_close(Some(100), None, None, 10));
        // 同値 → true
        assert!(stats_are_close(Some(100), Some(100), Some(100), 10));
        // 差 = bin_size * 2 (境界) → true
        assert!(stats_are_close(Some(100), Some(120), Some(110), 10));
        // 差 > bin_size * 2 → false
        assert!(!stats_are_close(Some(100), Some(121), Some(110), 10));
        // bin_size = 0 → false (defensive)
        assert!(!stats_are_close(Some(100), Some(100), Some(100), 0));
    }

    /// scatter.rs に xAxis.show / axisLine.show / axisTick.show が記述されていること
    #[test]
    fn scatter_source_contains_axis_show_directives() {
        let src = include_str!("scatter.rs");
        assert!(
            src.contains("\"show\": true"),
            "scatter.rs に show:true 指定があること"
        );
        assert!(
            src.contains("\"axisLine\""),
            "scatter.rs に axisLine 指定があること"
        );
        assert!(
            src.contains("\"axisTick\""),
            "scatter.rs に axisTick 指定があること"
        );
    }

    /// market_tightness.rs / regional_compare.rs の radar に center が指定され中央寄せであること
    #[test]
    fn radar_center_is_centered() {
        let mt_src = include_str!("market_tightness.rs");
        let rc_src = include_str!("regional_compare.rs");
        // どちらも center=["50%", "55%"] を含むこと
        for (name, src) in [
            ("market_tightness.rs", mt_src),
            ("regional_compare.rs", rc_src),
        ] {
            assert!(
                src.contains("\"center\": [\"50%\", \"55%\"]")
                    || src.contains("\"center\":[\"50%\",\"55%\"]"),
                "{} radar に center=[50%,55%] が指定されていること",
                name
            );
        }
    }

    /// employment.rs ドーナツに minAngle=5 が指定されていること
    #[test]
    fn donut_employment_has_min_angle() {
        let src = include_str!("employment.rs");
        assert!(
            src.contains("\"minAngle\": 5") || src.contains("\"minAngle\":5"),
            "employment.rs ドーナツに minAngle=5 が指定されていること"
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

    /// 2026-04-30: LLM 視覚レビュー用 HTML ダンプ。
    /// `cargo test --lib dump_report_html_for_review -- --ignored --nocapture` で生成。
    #[test]
    #[ignore = "manual: HTML dump for visual review"]
    fn dump_report_html_for_review() {
        use std::io::Write;
        let html = render_with_data();
        let path = std::env::temp_dir().join("review_report.html");
        let mut f = std::fs::File::create(&path).expect("create file");
        f.write_all(html.as_bytes()).expect("write html");
        println!("WROTE {} ({} bytes)", path.display(), html.len());
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
            html.contains("--dv2-good:")
                && html.contains("--dv2-warn:")
                && html.contains("--dv2-crit:"),
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
            html.contains("dv2-cover-header")
                && html.contains("dv2-cover-main")
                && html.contains("dv2-cover-footer"),
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

    // ============================================================
    // Phase 3 Step 4: MarketIntelligence variant
    // ============================================================

    /// `?variant=market_intelligence` が `MarketIntelligence` に解決されること。
    #[test]
    fn variant_query_market_intelligence_resolves() {
        assert_eq!(
            ReportVariant::from_query(Some("market_intelligence")),
            ReportVariant::MarketIntelligence
        );
    }

    /// 既存挙動完全維持: `public` / `full` / `None` / 不明値の解釈が変わらないこと。
    #[test]
    fn variant_query_existing_behavior_preserved() {
        // public は既存どおり
        assert_eq!(
            ReportVariant::from_query(Some("public")),
            ReportVariant::Public
        );
        // 未指定は default として Full
        assert_eq!(ReportVariant::from_query(None), ReportVariant::Full);
        // "full" 明示も Full (既存)
        assert_eq!(ReportVariant::from_query(Some("full")), ReportVariant::Full);
        // 不明値も Full フォールバック (既存)
        assert_eq!(
            ReportVariant::from_query(Some("invalid_value_xyz")),
            ReportVariant::Full
        );
        assert_eq!(ReportVariant::from_query(Some("")), ReportVariant::Full);
    }

    /// `MarketIntelligence` の `as_query()` / `display_name()` / `description()` が定義されていること。
    #[test]
    fn variant_market_intelligence_metadata_defined() {
        let v = ReportVariant::MarketIntelligence;
        assert_eq!(v.as_query(), "market_intelligence");
        assert!(!v.display_name().is_empty(), "display_name 必須");
        assert!(!v.description().is_empty(), "description 必須");
        assert!(!v.icon().is_empty(), "icon 必須");
        // ラウンドトリップ: as_query → from_query で同じ variant に戻る
        assert_eq!(
            ReportVariant::from_query(Some(v.as_query())),
            ReportVariant::MarketIntelligence
        );
    }

    /// 2026-05-08 Round 2-1 仕様変更:
    /// `MarketIntelligence` は HW セクションを表示しない (Public と同じ動作)。
    /// HW 併載は Full のみ。MI は対外提案向け (HW 言及最小化)。
    /// 詳細: `docs/PDF_DATA_SOURCE_MIXING_AUDIT_2026_05_08.md`
    #[test]
    fn variant_market_intelligence_does_not_show_hw_sections() {
        assert!(
            !ReportVariant::MarketIntelligence.show_hw_sections(),
            "MarketIntelligence は HW セクションを表示しない設計 (Round 2-1)"
        );
        // Full は HW 併載維持 (regression 防止)
        assert!(
            ReportVariant::Full.show_hw_sections(),
            "Full は HW 併載維持 (regression 防止)"
        );
        // Public は既存挙動維持
        assert!(
            !ReportVariant::Public.show_hw_sections(),
            "Public は既存挙動維持 (HW 非表示)"
        );
    }

    /// `show_market_intelligence_sections()` フックが MarketIntelligence のときのみ true。
    #[test]
    fn variant_show_market_intelligence_sections_flag() {
        assert!(ReportVariant::MarketIntelligence.show_market_intelligence_sections());
        assert!(!ReportVariant::Full.show_market_intelligence_sections());
        assert!(!ReportVariant::Public.show_market_intelligence_sections());
    }

    /// `MarketIntelligence.alternative()` は `Full` (一般版に戻る)。
    /// 既存 `Full ↔ Public` は影響を受けないこと。
    #[test]
    fn variant_market_intelligence_alternative_returns_full() {
        assert_eq!(
            ReportVariant::MarketIntelligence.alternative(),
            ReportVariant::Full
        );
        // 既存挙動維持
        assert_eq!(ReportVariant::Full.alternative(), ReportVariant::Public);
        assert_eq!(ReportVariant::Public.alternative(), ReportVariant::Full);
    }

    /// P0-1 (2026-05-06): variant_indicator は MI 用語を出力しない (variant isolation 維持)。
    /// MI 動線は媒体分析タブのアクションバー (render.rs の `data-variant="market_intelligence"`
    /// ボタン) に集約され、レポート画面 (Full/Public) の variant_indicator には
    /// MI への切替リンクを設置しない設計。
    #[test]
    fn variant_indicator_full_does_not_emit_market_intelligence_term() {
        let html = render_variant_indicator(ReportVariant::Full);
        assert!(
            !html.contains("採用マーケットインテリジェンス"),
            "Full の variant_indicator に MI 用語が混入してはならない (variant isolation)"
        );
        assert!(
            !html.contains("variant=market_intelligence"),
            "Full の variant_indicator に MI への切替リンクは置かない設計"
        );
    }

    #[test]
    fn variant_indicator_public_does_not_emit_market_intelligence_term() {
        let html = render_variant_indicator(ReportVariant::Public);
        assert!(
            !html.contains("採用マーケットインテリジェンス"),
            "Public の variant_indicator に MI 用語が混入してはならない (variant isolation)"
        );
        assert!(
            !html.contains("variant=market_intelligence"),
            "Public の variant_indicator に MI への切替リンクは置かない設計"
        );
    }

    /// `?theme=` クエリパラメータと `?variant=` パーサが独立であること
    /// (theme=v8 のときも variant 解釈が干渉しない)。
    ///
    /// `ReportTheme::from_query` と `ReportVariant::from_query` は別関数で、
    /// 入力文字列も互いに重複しないことを確認する。
    #[test]
    fn variant_and_theme_parsers_are_independent() {
        // theme クエリ値で variant を呼んでも Full フォールバックする
        assert_eq!(ReportVariant::from_query(Some("v8")), ReportVariant::Full);
        assert_eq!(ReportVariant::from_query(Some("v7a")), ReportVariant::Full);
        assert_eq!(
            ReportVariant::from_query(Some("default")),
            ReportVariant::Full
        );
        // 逆方向: variant クエリ値で theme を呼んでも default フォールバックする
        assert_eq!(
            ReportTheme::from_query(Some("market_intelligence")),
            ReportTheme::Default
        );
        assert_eq!(
            ReportTheme::from_query(Some("public")),
            ReportTheme::Default
        );
        assert_eq!(ReportTheme::from_query(Some("full")), ReportTheme::Default);
    }

    /// `as_query()` のラウンドトリップ完全性 (3 variant すべて)。
    #[test]
    fn variant_query_roundtrip_all_variants() {
        for v in [
            ReportVariant::Full,
            ReportVariant::Public,
            ReportVariant::MarketIntelligence,
        ] {
            let q = v.as_query();
            assert_eq!(
                ReportVariant::from_query(Some(q)),
                v,
                "ラウンドトリップ失敗: {q}"
            );
        }
    }

    // ============================================================
    // Phase 3 Step 3: MarketIntelligence セクション表示分岐
    // ============================================================

    use super::super::super::analysis::fetch::SurveyMarketIntelligenceData;
    use super::market_intelligence;

    /// `variant=market_intelligence` のときのみ採用マーケットインテリジェンスセクションが
    /// HTML に追加される (variant=full / public では追加されない) ことを検証する。
    ///
    /// `render_survey_report_page_with_variant_v3_themed` パイプライン全体は引数が多く
    /// 型準備が重いため、`if variant.show_market_intelligence_sections() {...}` の
    /// 分岐ロジックを直接テストする。
    #[test]
    fn test_market_intelligence_section_only_in_market_intelligence_variant() {
        let data = SurveyMarketIntelligenceData::default();

        // Full variant: フラグ false なので render しない → 空 HTML
        let mut html_full = String::new();
        if ReportVariant::Full.show_market_intelligence_sections() {
            market_intelligence::render_section_market_intelligence(&mut html_full, &data);
        }
        assert!(
            html_full.is_empty(),
            "Full では新セクションが追加されないこと (実際: {} chars)",
            html_full.len()
        );

        // Public variant: 同じく false → 空 HTML
        let mut html_public = String::new();
        if ReportVariant::Public.show_market_intelligence_sections() {
            market_intelligence::render_section_market_intelligence(&mut html_public, &data);
        }
        assert!(
            html_public.is_empty(),
            "Public でも新セクションが追加されないこと"
        );

        // MarketIntelligence variant: フラグ true → セクションが追加される
        let mut html_mi = String::new();
        if ReportVariant::MarketIntelligence.show_market_intelligence_sections() {
            market_intelligence::render_section_market_intelligence(&mut html_mi, &data);
        }
        assert!(
            html_mi.contains("採用マーケットインテリジェンス"),
            "MarketIntelligence では親セクション heading 必須"
        );
        assert!(html_mi.contains("結論サマリー"), "結論サマリーカード必須");
        assert!(
            html_mi.contains("配信地域ランキング"),
            "配信地域ランキング必須"
        );
        // Empty legacy talent-supply section is omitted to avoid internal fallback wording.
        assert!(
            html_mi.contains("給与・生活コスト比較"),
            "給与・生活コスト比較必須"
        );
        assert!(
            html_mi.contains("母集団レンジ"),
            "保守/標準/強気 母集団レンジ必須"
        );
    }

    /// MarketIntelligence セクションが空データでも panic せず placeholder を返すこと。
    #[test]
    fn test_market_intelligence_empty_data_does_not_panic() {
        let mut html = String::new();
        let data = SurveyMarketIntelligenceData::default();
        market_intelligence::render_section_market_intelligence(&mut html, &data);
        // 5 セクション + 1 補助で placeholder が複数出る
        // P0 (2026-05-06): prefix を「データ準備中」→「該当なし」に変更
        assert!(html.contains("該当なし"));
    }

    // =========================================================================
    // Phase 3 Step 5 Phase 5 (2026-05-04): mod.rs 統合 + signature ripple
    //
    // `_v3_themed` に追加した `db: Option<&LocalDb>` / `turso: Option<&TursoDb>` 経路で、
    // MarketIntelligence variant のときだけ実 fetch (build_market_intelligence_data) を呼ぶ
    // ガード分岐を検証する。Full / Public では呼ばれず、default() フォールバックのままになる。
    // =========================================================================

    /// MarketIntelligence variant + db=None で render を呼ぶと、default() フォールバックで
    /// Step 5 セクション特有のラベル (mi-empty placeholder 等) が HTML に含まれる。
    ///
    /// 実 fetch を経由しないため、既存テストの空 HTML 期待値とほぼ同等になるが、
    /// セクション root と placeholder は出力される。
    #[test]
    fn market_intelligence_variant_invokes_build_data() {
        let agg = SurveyAggregation::default();
        let seeker = JobSeekerAnalysis::default();
        let empty_segments =
            super::super::super::company::fetch::RegionalCompanySegments::default();
        let empty_map = std::collections::HashMap::new();
        let html = render_survey_report_page_with_variant_v3_themed(
            &agg,
            &seeker,
            &[],
            &[],
            &[],
            &[],
            None,
            &[],
            &empty_segments,
            &empty_segments,
            None,
            &empty_map,
            &[],
            ReportVariant::MarketIntelligence,
            ReportTheme::Default,
            None, // db: 未接続経路 → default() フォールバック
            None, // turso: 同上
        );
        // Step 5 セクション特有の文字列のいずれかを含むこと
        // (Phase 4 で実装された親 wrapper 又は placeholder)
        let has_mi_marker = html.contains("採用マーケットインテリジェンス")
            || html.contains("配信地域ランキング")
            || html.contains("該当なし");
        assert!(
            has_mi_marker,
            "MarketIntelligence variant では Step 5 セクションが描画されること"
        );
    }

    /// Full variant では Step 5 専用ラベル (Phase 4 新規) が一切出ない。
    #[test]
    fn full_variant_does_not_invoke_step5_sections() {
        let agg = SurveyAggregation::default();
        let seeker = JobSeekerAnalysis::default();
        let empty_segments =
            super::super::super::company::fetch::RegionalCompanySegments::default();
        let empty_map = std::collections::HashMap::new();
        let html = render_survey_report_page_with_variant_v3_themed(
            &agg,
            &seeker,
            &[],
            &[],
            &[],
            &[],
            None,
            &[],
            &empty_segments,
            &empty_segments,
            None,
            &empty_map,
            &[],
            ReportVariant::Full,
            ReportTheme::Default,
            None,
            None,
        );
        assert!(
            !html.contains("採用マーケットインテリジェンス"),
            "Full では MI 親セクション heading が出てはならない"
        );
        assert!(
            !html.contains("配信地域ランキング"),
            "Full では MI 配信地域ランキングが出てはならない"
        );
    }

    /// Public variant でも Step 5 セクションは出ない。
    #[test]
    fn public_variant_does_not_invoke_step5_sections() {
        let agg = SurveyAggregation::default();
        let seeker = JobSeekerAnalysis::default();
        let empty_segments =
            super::super::super::company::fetch::RegionalCompanySegments::default();
        let empty_map = std::collections::HashMap::new();
        let html = render_survey_report_page_with_variant_v3_themed(
            &agg,
            &seeker,
            &[],
            &[],
            &[],
            &[],
            None,
            &[],
            &empty_segments,
            &empty_segments,
            None,
            &empty_map,
            &[],
            ReportVariant::Public,
            ReportTheme::Default,
            None,
            None,
        );
        assert!(
            !html.contains("採用マーケットインテリジェンス"),
            "Public では MI 親セクション heading が出てはならない"
        );
        assert!(
            !html.contains("配信地域ランキング"),
            "Public では MI 配信地域ランキングが出てはならない"
        );
    }

    /// db=None で MarketIntelligence variant を呼ぶと、build_market_intelligence_data は
    /// 経由せず default() で fallback する (副作用なし、panic なし)。
    /// Full / Public 同様、新セクションは描画される (default データのため placeholder 中心)。
    #[test]
    fn variant_guard_falls_back_to_default_for_non_mi() {
        // db=None の場合、MarketIntelligence variant でも fetch をスキップして default()
        // となる。HTML には親セクション + placeholder のみ。
        let agg = SurveyAggregation::default();
        let seeker = JobSeekerAnalysis::default();
        let empty_segments =
            super::super::super::company::fetch::RegionalCompanySegments::default();
        let empty_map = std::collections::HashMap::new();
        let html = render_survey_report_page_with_variant_v3_themed(
            &agg,
            &seeker,
            &[],
            &[],
            &[],
            &[],
            None,
            &[],
            &empty_segments,
            &empty_segments,
            None,
            &empty_map,
            &[],
            ReportVariant::MarketIntelligence,
            ReportTheme::Default,
            None,
            None,
        );
        // default fallback: 親セクションは出るが、データなし placeholder
        assert!(html.contains("採用マーケットインテリジェンス"));
        assert!(
            html.contains("該当なし"),
            "default() フォールバック時は placeholder が出ること"
        );
    }

    /// Phase 5.5 (2026-05-04): MarketIntelligence variant + 実 db に対して
    /// agg.by_municipality_salary の (pref, name) が code 解決されて
    /// build_market_intelligence_data に渡されることを smoke レベルで検証する。
    ///
    /// 設計: in-memory DB に `municipality_code_master` (area_level 列入り) のみを投入。
    /// 下流 4 fetch のテーブルは存在しないため空 Vec フォールバックになるが、
    /// code 解決が走った結果 build が呼ばれ panic しないことを確認する。
    #[test]
    fn market_intelligence_variant_resolves_codes_smoke() {
        use crate::db::local_sqlite::LocalDb;
        use crate::handlers::survey::aggregator::MunicipalitySalaryAgg;

        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let _ = rusqlite::Connection::open(path).unwrap();
        let db = LocalDb::new(path).unwrap();
        db.execute(
            "CREATE TABLE municipality_code_master (
                municipality_code TEXT PRIMARY KEY,
                municipality_name TEXT,
                prefecture TEXT,
                area_type TEXT,
                area_level TEXT,
                parent_code TEXT
            )",
            &[],
        )
        .unwrap();
        db.execute(
            "INSERT INTO municipality_code_master VALUES \
             ('13104', '新宿区', '東京都', 'special_ward', 'unit', '13100'), \
             ('13103', '港区', '東京都', 'special_ward', 'unit', '13100')",
            &[],
        )
        .unwrap();

        let mut agg = SurveyAggregation::default();
        agg.by_municipality_salary = vec![
            MunicipalitySalaryAgg {
                name: "新宿区".to_string(),
                prefecture: "東京都".to_string(),
                count: 5,
                avg_salary: 250_000,
                median_salary: 240_000,
            },
            MunicipalitySalaryAgg {
                name: "港区".to_string(),
                prefecture: "東京都".to_string(),
                count: 3,
                avg_salary: 300_000,
                median_salary: 280_000,
            },
        ];
        let seeker = JobSeekerAnalysis::default();
        let empty_segments =
            super::super::super::company::fetch::RegionalCompanySegments::default();
        let empty_map = std::collections::HashMap::new();

        let html = render_survey_report_page_with_variant_v3_themed(
            &agg,
            &seeker,
            &[],
            &[],
            &[],
            &[],
            None,
            &[],
            &empty_segments,
            &empty_segments,
            None,
            &empty_map,
            &[],
            ReportVariant::MarketIntelligence,
            ReportTheme::Default,
            Some(&db),
            None,
        );
        // パニックせず、MI 親セクションが出力されること
        assert!(
            html.contains("採用マーケットインテリジェンス"),
            "code 解決が走っても MI セクションは描画される"
        );
    }

    /// Full variant では (実 db を渡しても) code 解決ロジックを通らず副作用なし。
    #[test]
    fn full_variant_does_not_resolve_codes_with_db() {
        use crate::db::local_sqlite::LocalDb;
        use crate::handlers::survey::aggregator::MunicipalitySalaryAgg;

        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        let _ = rusqlite::Connection::open(path).unwrap();
        let db = LocalDb::new(path).unwrap();

        let mut agg = SurveyAggregation::default();
        agg.by_municipality_salary = vec![MunicipalitySalaryAgg {
            name: "新宿区".to_string(),
            prefecture: "東京都".to_string(),
            count: 5,
            avg_salary: 250_000,
            median_salary: 240_000,
        }];
        let seeker = JobSeekerAnalysis::default();
        let empty_segments =
            super::super::super::company::fetch::RegionalCompanySegments::default();
        let empty_map = std::collections::HashMap::new();

        let html = render_survey_report_page_with_variant_v3_themed(
            &agg,
            &seeker,
            &[],
            &[],
            &[],
            &[],
            None,
            &[],
            &empty_segments,
            &empty_segments,
            None,
            &empty_map,
            &[],
            ReportVariant::Full,
            ReportTheme::Default,
            Some(&db),
            None,
        );
        assert!(
            !html.contains("採用マーケットインテリジェンス"),
            "Full variant では code 解決も MI セクションも実行されない"
        );
    }

    /// 既存 `render_survey_report_page` (Full variant 相当の古い関数) の
    /// 出力に MarketIntelligence セクションが含まれないこと (既存挙動完全維持)。
    #[test]
    fn test_existing_render_does_not_emit_market_intelligence() {
        let agg = SurveyAggregation::default();
        let seeker = JobSeekerAnalysis::default();
        let html = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[]);
        // 既存 render は variant 引数を取らないため新セクションは絶対に出ない
        assert!(
            !html.contains("採用マーケットインテリジェンス"),
            "既存 render に新セクションが混入してはならない"
        );
        assert!(
            !html.contains("配信地域ランキング"),
            "既存 render に新セクションが混入してはならない"
        );
    }

    // ========================================================================
    // 2026-05-08 Round 2-1: HW セクション混入防止テスト群
    //
    // ユーザー判断 (Round 1-L 監査結果):
    // - MarketIntelligence variant は HW 言及最小化 (Public 系 + 採用コンサル拡張)
    // - Full は HW 併載維持 (社内分析向け)
    // - Public は既存挙動維持 (HW 言及最小化)
    //
    // これらのテストは、show_hw_sections() の guard 変更が章レベルで
    // 期待通り効くこと、および regression を起こしていないことを検証する。
    // ========================================================================

    /// HW 関連用語のプリセット (cover subtitle / Section H heading / 4 軸 KPI / 産業 vs HW)
    /// MI / Public 出力にこれらが含まれないことを検証するために使用。
    fn hw_forbidden_terms_for_mi() -> &'static [&'static str] {
        &[
            // 表紙サブタイトル (Round 1-L #1)
            "ハローワーク掲載求人 + アップロード CSV クロス分析",
            // Section H 見出し (Round 1-L #3)
            "地域 × HW データ連携",
            // Section 4 KPI (Round 1-L #4)
            "HW 欠員補充率",
            // Section 4B 表ヘッダ (Round 1-L #5)
            "HW 求人構成比",
            // Exec Summary HW 比較 (Round 1-L #2)
            "HW 市場",
        ]
    }

    /// 共通テストヘルパ: variant + theme で minimal render を生成
    fn render_for_variant_r2_1(variant: ReportVariant) -> String {
        let agg = SurveyAggregation::default();
        let seeker = JobSeekerAnalysis::default();
        let empty_segments =
            super::super::super::company::fetch::RegionalCompanySegments::default();
        let empty_map = std::collections::HashMap::new();
        render_survey_report_page_with_variant_v3_themed(
            &agg,
            &seeker,
            &[],
            &[],
            &[],
            &[],
            None,
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
        )
    }

    /// MI variant では `show_hw_sections() == false` (新仕様)
    #[test]
    fn mi_variant_does_not_show_hw_sections() {
        assert!(
            !ReportVariant::MarketIntelligence.show_hw_sections(),
            "Round 2-1 仕様: MI は HW セクションを表示しない"
        );
    }

    /// Full variant では `show_hw_sections() == true` (regression 防止)
    #[test]
    fn full_variant_still_shows_hw_sections() {
        assert!(
            ReportVariant::Full.show_hw_sections(),
            "Full は HW 併載維持 (regression 防止)"
        );
    }

    /// Public variant では `show_hw_sections() == false` (既存挙動維持)
    #[test]
    fn public_variant_does_not_show_hw_sections_regression() {
        assert!(
            !ReportVariant::Public.show_hw_sections(),
            "Public は既存挙動維持: HW 非表示"
        );
    }

    /// MI variant の HTML 出力に Round 1-L 検出の P0 HW 用語 5 系統が含まれない。
    ///
    /// 検証対象 (cover subtitle / Section H heading / 4MT KPI / 4B header / Exec HW 比較):
    /// `docs/PDF_DATA_SOURCE_MIXING_AUDIT_2026_05_08.md` §4.1
    #[test]
    fn mi_variant_html_output_does_not_contain_hw_p0_terms() {
        let html = render_for_variant_r2_1(ReportVariant::MarketIntelligence);
        for term in hw_forbidden_terms_for_mi() {
            assert!(
                !html.contains(term),
                "MI variant 出力に Round 1-L P0 HW 用語が混入: '{}'",
                term
            );
        }
        // cover subtitle が MI 専用文言に切替わっていること (positive 証明)
        assert!(
            html.contains("採用市場・ターゲット分析") || html.contains("公開統計クロス分析"),
            "MI 用 cover subtitle が出力されていること"
        );
    }

    /// Full variant の HTML 出力には HW 用語が含まれる (HW 併載維持の逆証明)。
    ///
    /// hw_context が None の minimal render でも、cover subtitle は variant 連動で
    /// 「ハローワーク掲載求人 + アップロード CSV クロス分析」になる。
    #[test]
    fn full_variant_html_output_contains_hw_subtitle() {
        let html = render_for_variant_r2_1(ReportVariant::Full);
        assert!(
            html.contains("ハローワーク掲載求人 + アップロード CSV クロス分析"),
            "Full variant の cover subtitle に HW 文言が出ること (HW 併載維持の逆証明)"
        );
    }

    /// Public variant の HTML 出力には HW P0 用語が含まれない (既存挙動維持の逆証明)。
    #[test]
    fn public_variant_html_output_does_not_contain_hw_p0_terms() {
        let html = render_for_variant_r2_1(ReportVariant::Public);
        for term in hw_forbidden_terms_for_mi() {
            assert!(
                !html.contains(term),
                "Public variant 出力に Round 1-L P0 HW 用語が混入: '{}'",
                term
            );
        }
    }

    /// cover subtitle が variant 別に切り替わる (3 variant の差分が出ること)。
    #[test]
    fn cover_subtitle_differs_by_variant() {
        let html_full = render_for_variant_r2_1(ReportVariant::Full);
        let html_mi = render_for_variant_r2_1(ReportVariant::MarketIntelligence);
        let html_public = render_for_variant_r2_1(ReportVariant::Public);

        // Full のみ HW 文言を含む
        assert!(html_full.contains("ハローワーク掲載求人"));
        assert!(!html_mi.contains("ハローワーク掲載求人"));
        assert!(!html_public.contains("ハローワーク掲載求人"));
    }

    // ========================================================================
    // 2026-05-08 Round 2.5: salesnow セクション (第 12 章 / 第 12B 章) HW 混入防止
    //
    // Round 2-1 で残った 2 系統 (salesnow_companies / company_segments) を MI variant で
    // 章ごと非表示にする変更の regression 防止テスト。
    //
    // - Full は salesnow セクションを表示する (HW 列含む全カラム維持)
    // - MI は salesnow セクションを章ごと非表示 (HW 言及最小化方針)
    // - Public は既存挙動維持 (Round 2-1 で salesnow を Public 経路で出していた場合は維持)
    // ========================================================================

    /// salesnow テスト用のミニデータ (1 社) を作る
    fn salesnow_test_company() -> super::super::super::company::fetch::NearbyCompany {
        super::super::super::company::fetch::NearbyCompany {
            corporate_number: "1234567890123".to_string(),
            company_name: "テスト株式会社".to_string(),
            prefecture: "東京都".to_string(),
            sn_industry: "医療・福祉".to_string(),
            employee_count: 500,
            credit_score: 0.0,
            postal_code: "100-0001".to_string(),
            hw_posting_count: 7,
            sales_amount: 5.0e8,
            sales_range: "5億円以上".to_string(),
            employee_delta_1y: 3.5,
            employee_delta_3m: 0.8,
        }
    }

    /// segments テスト用ミニデータ (large に 1 社) を作る
    fn salesnow_test_segments() -> super::super::super::company::fetch::RegionalCompanySegments {
        let mut s = super::super::super::company::fetch::RegionalCompanySegments::default();
        s.pool_size = 1;
        s.large.push(salesnow_test_company());
        s
    }

    /// salesnow データありで variant 別 render するヘルパ
    fn render_for_variant_r25_with_salesnow(variant: ReportVariant) -> String {
        let agg = SurveyAggregation::default();
        let seeker = JobSeekerAnalysis::default();
        let companies = vec![salesnow_test_company()];
        let segments = salesnow_test_segments();
        let empty_segments_industry =
            super::super::super::company::fetch::RegionalCompanySegments::default();
        let empty_map = std::collections::HashMap::new();
        render_survey_report_page_with_variant_v3_themed(
            &agg,
            &seeker,
            &[],
            &[],
            &[],
            &[],
            None,
            &companies,
            &segments,
            &empty_segments_industry,
            None,
            &empty_map,
            &[],
            variant,
            ReportTheme::Default,
            None,
            None,
        )
    }

    /// Round 2.5 検出対象の HW 用語 (salesnow 章テーブル / 注記 / takeaway 内)
    ///
    /// スコープ: salesnow.rs の 2 系統 (第 12 章 salesnow_companies / 第 12B 章 company_segments)。
    /// notes.rs の汎用 footer (variant 非依存) に残る "HW 公開求人" は別ラウンド対応とし、
    /// 本リストには含めない。
    fn hw_forbidden_terms_for_salesnow_r25() -> &'static [&'static str] {
        &[
            // 第 12 章 (salesnow_companies) のテーブル列ヘッダ
            "HW求人数",
            // 第 12 章 注記
            "HW industry_mapping",
            "HW にも掲載",
            // 第 12B 章 (company_segments) のテーブル列ヘッダ
            "HW 求人継続率",
            // 第 12B 章 セグメントラベル
            "求人積極期 (HW",
            "ハローワークで 5 件以上",
        ]
    }

    /// Round 2.5: MI variant の salesnow 章は HW 用語を出さない (章ごと非表示)
    #[test]
    fn mi_variant_salesnow_section_excludes_hw_terms() {
        let html = render_for_variant_r25_with_salesnow(ReportVariant::MarketIntelligence);
        for term in hw_forbidden_terms_for_salesnow_r25() {
            assert!(
                !html.contains(term),
                "MI variant 出力に Round 2.5 salesnow 章 HW 用語が混入: '{}'",
                term
            );
        }
    }

    /// Round 2.5: Full variant では salesnow 章 HW 列が維持される (regression 防止)
    #[test]
    fn full_variant_salesnow_section_still_shows_hw_columns() {
        let html = render_for_variant_r25_with_salesnow(ReportVariant::Full);
        // Full は salesnow 章を表示する (companies テーブルの HW 列ヘッダが出る)
        assert!(
            html.contains("HW求人数"),
            "Full variant では salesnow テーブルに HW 求人数列が維持されるはず"
        );
        // 第 12B 章 (segments) も large に 1 社入れているので構造サマリは出る
        assert!(
            html.contains("HW 求人継続率"),
            "Full variant では segments テーブルに HW 求人継続率列が維持されるはず"
        );
    }

    /// Round 2.5: MI variant では salesnow 章自体が出力されない (章ごと非表示の証明)
    /// 章タイトル (h2 見出し) を直接検証することで、章レベルで非表示になっていることを確認。
    #[test]
    fn mi_variant_salesnow_chapter_is_hidden() {
        let html = render_for_variant_r25_with_salesnow(ReportVariant::MarketIntelligence);
        // 第 5 章 地域注目企業 / 地域企業 ベンチマーク のいずれの h2 タイトルも出ない
        assert!(
            !html.contains("第5章 地域注目企業"),
            "MI では第 12 章 (地域注目企業) は章ごと非表示"
        );
        assert!(
            !html.contains("第5章 地域企業 ベンチマーク"),
            "MI では第 12B 章 (地域企業ベンチマーク) は章ごと非表示"
        );
    }

    /// Round 2.5: Full では salesnow 章タイトルが表示される (regression 防止)
    #[test]
    fn full_variant_salesnow_chapter_is_visible() {
        let html = render_for_variant_r25_with_salesnow(ReportVariant::Full);
        assert!(
            html.contains("第5章 地域注目企業"),
            "Full では第 12 章 (地域注目企業) が表示されるはず"
        );
        assert!(
            html.contains("第5章 地域企業 ベンチマーク"),
            "Full では第 12B 章 (地域企業ベンチマーク) が表示されるはず"
        );
    }

    /// Round 2.5 + Round 2-1 統合: MI variant の HTML 出力に
    /// 9 系統の HW 用語 (Round 2-1 の 5 系統 + Round 2.5 の 7 系統 = 重複除いて約 9 種) が
    /// すべて含まれないこと。
    #[test]
    fn mi_variant_html_excludes_all_hw_phrases_combined() {
        let html = render_for_variant_r25_with_salesnow(ReportVariant::MarketIntelligence);
        // Round 2-1 系統 (cover subtitle / 4MT KPI / 4B header / Exec HW 比較)
        for term in hw_forbidden_terms_for_mi() {
            assert!(
                !html.contains(term),
                "MI 出力に Round 2-1 P0 HW 用語が混入: '{}'",
                term
            );
        }
        // Round 2.5 系統 (salesnow 章テーブル列 / 注記 / takeaway)
        for term in hw_forbidden_terms_for_salesnow_r25() {
            assert!(
                !html.contains(term),
                "MI 出力に Round 2.5 salesnow 章 HW 用語が混入: '{}'",
                term
            );
        }
    }

    // =========================================================================
    // Round 3-A (2026-05-06): 産業構成 Top10 セクション (region.rs:269) を
    // MI variant の通常導線 PDF (render_survey_report_page_with_variant_v3_themed)
    // に接続する追加分の検証。Round 2-4 監査 P0-3 の消化。
    // =========================================================================

    /// 最小 InsightContext を構築するヘルパ (ext_industry_employees のみ設定可能)
    fn ctx_with_industry_rows(
        rows: Vec<super::super::super::helpers::Row>,
    ) -> super::super::super::insight::fetch::InsightContext {
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
            ext_industry_employees: rows,
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

    /// 産業別就業者 Top10 用のサンプル Row 3 件 (集計行は混入させない)
    fn sample_industry_rows() -> Vec<super::super::super::helpers::Row> {
        use serde_json::json;
        use std::collections::HashMap;
        let mut out = Vec::new();
        for (code, name, emp) in [
            ("P", "医療,福祉", 120_000_i64),
            ("E", "製造業", 90_000_i64),
            ("I", "卸売業,小売業", 60_000_i64),
        ] {
            let mut m: super::super::super::helpers::Row = HashMap::new();
            m.insert("industry_code".to_string(), json!(code));
            m.insert("industry_name".to_string(), json!(name));
            m.insert("employees_total".to_string(), json!(emp));
            out.push(m);
        }
        out
    }

    /// MI variant + ext_industry_employees あり → 産業構成 Top10 セクションが出力される
    #[test]
    fn round3a_industry_structure_section_appears_in_mi_variant() {
        let mut agg = SurveyAggregation::default();
        agg.dominant_prefecture = Some("東京都".to_string());
        let seeker = JobSeekerAnalysis::default();
        let ctx = ctx_with_industry_rows(sample_industry_rows());
        let empty_segments =
            super::super::super::company::fetch::RegionalCompanySegments::default();
        let empty_map = std::collections::HashMap::new();
        let html = render_survey_report_page_with_variant_v3_themed(
            &agg,
            &seeker,
            &[],
            &[],
            &[],
            &[],
            Some(&ctx),
            &[],
            &empty_segments,
            &empty_segments,
            None,
            &empty_map,
            &[],
            ReportVariant::MarketIntelligence,
            ReportTheme::Default,
            None,
            None,
        );
        assert!(
            html.contains("data-testid=\"industry-structure-print\""),
            "MI variant では産業構成 Top10 セクション (Round 3-A 接続) が出力されるはず"
        );
        assert!(
            html.contains("表 6-2"),
            "産業構成 Top10 の図番号 6-2 が出力されること"
        );
        assert!(
            html.contains("医療,福祉"),
            "サンプル産業名 (Top1) が表に表示されること"
        );
        assert!(
            html.contains("国勢調査 2020"),
            "data source は e-Stat 国勢調査 2020 (中立ラベル) であること"
        );
    }

    /// Full / Public variant では Round 3-A 経路は発火しない (Tab UI 経由は別系統)。
    /// 印刷経路の章追加を MI に限定したことの regression 防止。
    #[test]
    fn round3a_industry_structure_section_skipped_in_full_and_public_print_path() {
        for variant in [ReportVariant::Full, ReportVariant::Public] {
            let mut agg = SurveyAggregation::default();
            agg.dominant_prefecture = Some("東京都".to_string());
            let seeker = JobSeekerAnalysis::default();
            let ctx = ctx_with_industry_rows(sample_industry_rows());
            let empty_segments =
                super::super::super::company::fetch::RegionalCompanySegments::default();
            let empty_map = std::collections::HashMap::new();
            let html = render_survey_report_page_with_variant_v3_themed(
                &agg,
                &seeker,
                &[],
                &[],
                &[],
                &[],
                Some(&ctx),
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
            );
            // Round 3-A の testid (印刷経路の region::render_section_industry_structure) は
            // MI 限定。Full/Public は Tab UI 経由で別途表示済みのため重複を避ける。
            assert!(
                !html.contains("data-testid=\"industry-structure-print\""),
                "{:?} variant の印刷経路では Round 3-A 接続は発火しないはず",
                variant
            );
        }
    }

    /// Round 3-A 追加章の data source ラベルは公的統計 (HW 連想語不混入)
    #[test]
    fn round3a_industry_structure_section_uses_neutral_data_source_label() {
        let mut agg = SurveyAggregation::default();
        agg.dominant_prefecture = Some("東京都".to_string());
        let seeker = JobSeekerAnalysis::default();
        let ctx = ctx_with_industry_rows(sample_industry_rows());
        let empty_segments =
            super::super::super::company::fetch::RegionalCompanySegments::default();
        let empty_map = std::collections::HashMap::new();
        let html = render_survey_report_page_with_variant_v3_themed(
            &agg,
            &seeker,
            &[],
            &[],
            &[],
            &[],
            Some(&ctx),
            &[],
            &empty_segments,
            &empty_segments,
            None,
            &empty_map,
            &[],
            ReportVariant::MarketIntelligence,
            ReportTheme::Default,
            None,
            None,
        );
        // Round 3-A 章の section 内では HW 連想語 (HW 求人 / 欠員補充率 / industry_mapping) を出さない
        // (region::render_section_industry_structure の注記は「HW industry_raw とは粒度が異なる可能性」
        //  という警告文を含むため、章単体ではなく特定の HW テーブル列名のみ検査する)
        let section_marker = "data-testid=\"industry-structure-print\"";
        let start = html
            .find(section_marker)
            .expect("Round 3-A セクションが出力されている前提");
        // section の終端を直近の </div> 検索で確定 (簡易: 最初の section close)
        let after = &html[start..];
        // 章単体に「HW求人数」「HW 欠員補充率」「HW 求人継続率」等の数値ラベルが含まれていないこと
        for hw_label in ["HW求人数", "HW 欠員補充率", "HW 求人継続率"] {
            assert!(
                !after[..after.len().min(4_000)].contains(hw_label),
                "Round 3-A 章に HW 数値ラベル '{}' が混入してはならない",
                hw_label
            );
        }
    }
}
