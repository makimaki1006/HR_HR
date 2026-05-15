//! 示唆のHTML描画

use super::super::helpers::escape_html;
use super::helpers::*;

// ======== サブタブ描画 ========

/// サブタブ1: 採用構造分析
pub(crate) fn render_subtab_hiring(insights: &[Insight]) -> String {
    let filtered: Vec<_> = insights
        .iter()
        .filter(|i| i.category == InsightCategory::HiringStructure)
        .collect();
    render_insight_list(
        "採用構造分析",
        "なぜ採用が難しいのか、構造的な要因を分析します",
        &filtered,
    )
}

/// サブタブ2: 将来予測
pub(crate) fn render_subtab_forecast(insights: &[Insight]) -> String {
    let filtered: Vec<_> = insights
        .iter()
        .filter(|i| i.category == InsightCategory::Forecast)
        .collect();
    render_insight_list(
        "将来予測",
        "時系列データと人口動態から市場の方向性を予測します",
        &filtered,
    )
}

/// サブタブ3: 地域比較
pub(crate) fn render_subtab_regional(insights: &[Insight]) -> String {
    let filtered: Vec<_> = insights
        .iter()
        .filter(|i| i.category == InsightCategory::RegionalCompare)
        .collect();
    render_insight_list(
        "地域間比較",
        "他地域との比較から強み・弱みを可視化します",
        &filtered,
    )
}

/// サブタブ4: アクション提案
pub(crate) fn render_subtab_action(insights: &[Insight]) -> String {
    let filtered: Vec<_> = insights
        .iter()
        .filter(|i| i.category == InsightCategory::ActionProposal)
        .collect();
    let mut html = render_insight_list(
        "アクション提案",
        "データに基づく具体的な改善施策を提案します",
        &filtered,
    );
    html.push_str(&render_report_download_section());
    html
}

/// サブタブ5: 構造分析（Phase A、SSDSE-Aベース）
pub(crate) fn render_subtab_structural(insights: &[Insight]) -> String {
    let filtered: Vec<_> = insights
        .iter()
        .filter(|i| i.category == InsightCategory::StructuralContext)
        .collect();
    render_insight_list(
        "構造分析",
        "市区町村の世帯・労働力・医療福祉・教育・地理構造から、採用の背景にある構造的要因を示唆します",
        &filtered,
    )
}

/// 示唆カードのリスト描画
fn render_insight_list(title: &str, description: &str, insights: &[&Insight]) -> String {
    let mut html = String::with_capacity(4_000);

    write!(
        html,
        r#"<div class="space-y-4">
        <div class="mb-4">
            <h3 class="text-lg font-semibold text-white">{title}</h3>
            <p class="text-xs text-slate-500">{description}</p>
        </div>"#
    )
    .unwrap();

    if insights.is_empty() {
        html.push_str(
            r#"<div class="stat-card text-center py-8">
            <p class="text-slate-400 text-sm">該当する示唆はありません</p>
            <p class="text-slate-500 text-xs mt-1">地域を絞り込むとより詳細な分析が表示されます</p>
        </div>"#,
        );
    } else {
        for insight in insights {
            html.push_str(&render_insight_card(insight));
        }
        // HWデータソース注記
        html.push_str(r#"<div class="text-[10px] text-slate-600 mt-3">※ HW（ハローワーク）掲載求人に基づく分析です。IT・通信等のHW掲載が少ない産業は参考値としてご利用ください。</div>"#);
    }

    html.push_str("</div>");
    html
}

/// 単一の示唆カード描画
fn render_insight_card(insight: &Insight) -> String {
    let mut html = String::with_capacity(1_000);
    let bg = insight.severity.bg_class();
    let badge = insight.severity.badge_class();
    let label = insight.severity.label();

    write!(
        html,
        r#"<div class="rounded-lg border p-4 {bg}">
        <div class="flex items-start gap-3">
            <span class="px-2 py-0.5 rounded text-xs font-medium {badge}">{label}</span>
            <div class="flex-1 min-w-0">
                <h4 class="text-sm font-semibold text-white">{title}</h4>
                <p class="text-xs text-slate-300 mt-1">{body}</p>"#,
        title = escape_html(&insight.title),
        body = escape_html(&insight.body),
    )
    .unwrap();

    // エビデンス表示
    if !insight.evidence.is_empty() {
        html.push_str(r#"<div class="flex flex-wrap gap-3 mt-2">"#);
        for ev in &insight.evidence {
            let formatted_value = if ev.unit == "円" || ev.unit == "円/月" || ev.unit == "円/人"
            {
                format!("{:.0}{}", ev.value, ev.unit)
            } else if ev.unit == "%" || ev.unit == "%/月" {
                format!("{:.1}{}", ev.value * 100.0, ev.unit)
            } else if ev.unit == "人" || ev.unit == "件" || ev.unit == "件/千人" || ev.unit == "点"
            {
                format!("{:.0}{}", ev.value, ev.unit)
            } else {
                format!("{:.2}{}", ev.value, ev.unit)
            };
            write!(
                html,
                r#"<div class="text-xs">
                    <span class="text-slate-500">{metric}:</span>
                    <span class="text-white font-mono">{value}</span>
                    <span class="text-slate-600 ml-1">({context})</span>
                </div>"#,
                metric = escape_html(&ev.metric),
                value = escape_html(&formatted_value),
                context = escape_html(&ev.context),
            )
            .unwrap();
        }
        html.push_str("</div>");
    }

    // 関連タブリンク
    if !insight.related_tabs.is_empty() {
        html.push_str(r#"<div class="flex gap-2 mt-2">"#);
        for tab in &insight.related_tabs {
            let tab_label = match *tab {
                "overview" => "概況",
                "balance" => "需給",
                "workstyle" => "雇用形態",
                "analysis" => "分析",
                "trend" => "トレンド",
                "competitive" => "求人検索",
                "jobmap" => "地図",
                "diagnostic" => "診断",
                "insight" => "総合診断",
                "survey" => "媒体分析",
                _ => tab,
            };
            write!(html,
                r#"<a class="text-xs text-blue-400/60 hover:text-blue-300 cursor-pointer" onclick="navigateToTab('/tab/{tab}')">{tab_label}→</a>"#
            ).unwrap();
        }
        html.push_str("</div>");
    }

    html.push_str("</div></div></div>");
    html
}

// ======== レポートダウンロードセクション ========

/// レポートダウンロードボタン（総合診断タブの末尾に表示）
pub(crate) fn render_report_download_section() -> String {
    r#"<div class="stat-card mt-6">
        <h3 class="text-sm text-slate-400 mb-2">レポート出力</h3>
        <p class="text-xs text-slate-500 mb-3">分析結果をダウンロードできます。</p>
        <div class="flex gap-2 flex-wrap">
            <a href="/api/insight/report/xlsx" download
                class="px-4 py-2 bg-emerald-600 hover:bg-emerald-500 text-white rounded text-sm font-medium transition-colors inline-block">
                📊 Excel出力
            </a>
            <button onclick="downloadInsightReport()"
                class="px-4 py-2 bg-slate-600 hover:bg-slate-500 text-white rounded text-sm font-medium transition-colors">
                📄 JSON出力
            </button>
        </div>
    </div>"#.to_string()
}

// ======== ウィジェット描画 ========

/// 既存タブ用の小型ウィジェット（最大3件）
pub(crate) fn render_insight_widget_html(insights: &[&Insight]) -> String {
    let mut html = String::with_capacity(2_000);

    html.push_str(r#"<div class="space-y-2 mt-4 pt-4 border-t border-slate-700">
        <h4 class="text-xs font-semibold text-slate-400 flex items-center gap-1">
            <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
            </svg>
            関連する示唆
        </h4>"#);

    for insight in insights {
        let badge = insight.severity.badge_class();
        let label = insight.severity.label();
        write!(html,
            r#"<div class="flex items-start gap-2 p-2 rounded bg-slate-800/50">
                <span class="px-1.5 py-0.5 rounded text-[10px] font-medium {badge} shrink-0">{label}</span>
                <div class="min-w-0">
                    <p class="text-xs text-white font-medium truncate">{title}</p>
                    <p class="text-[10px] text-slate-400 line-clamp-2">{body}</p>
                </div>
            </div>"#,
            title = escape_html(&insight.title),
            body = escape_html(&insight.body),
        ).unwrap();
    }

    // 総合診断タブへの誘導
    html.push_str(r#"<a class="text-xs text-blue-400/60 hover:text-blue-300 cursor-pointer block text-right" onclick="navigateToTab('/tab/insight')">総合診断を見る →</a>"#);
    html.push_str("</div>");
    html
}

// ======== 統合レポートHTMLページ（PDF出力用） ========

use super::super::helpers::{format_number, get_f64, get_i64, get_str_ref};
use super::fetch::InsightContext;

use std::fmt::Write as _;
/// 完全なHTMLページとしてレポートを生成（/report/insight）
/// 7ページ構成: サマリー/市場/将来/課題/地域/アクション/付録
pub(crate) fn render_insight_report_page(
    insights: &[Insight],
    ctx: &InsightContext,
    pref: &str,
    muni: &str,
) -> String {
    let location = if !muni.is_empty() {
        format!("{} {}", pref, muni)
    } else if !pref.is_empty() {
        pref.to_string()
    } else {
        "全国".to_string()
    };
    let today = chrono::Local::now().format("%Y年%m月%d日").to_string();

    let critical = insights
        .iter()
        .filter(|i| i.severity == Severity::Critical)
        .count();
    let warning = insights
        .iter()
        .filter(|i| i.severity == Severity::Warning)
        .count();
    let info = insights
        .iter()
        .filter(|i| i.severity == Severity::Info)
        .count();
    let positive = insights
        .iter()
        .filter(|i| i.severity == Severity::Positive)
        .count();

    let summary = super::report::generate_executive_summary_text(insights, ctx);

    let mut html = String::with_capacity(32_000);

    // <!DOCTYPE html>
    html.push_str(r#"<!DOCTYPE html>
<html lang="ja">
<head>
<meta charset="UTF-8">
<title>総合診断レポート</title>
<style>
:root {
  --c-primary: #1a5276;
  --c-primary-light: #2874a6;
  --c-success: #059669;
  --c-danger: #dc2626;
  --c-warning: #f59e0b;
  --c-text: #1a1a2e;
  --c-text-muted: #888;
  --c-border: #e0e0e0;
  --c-bg-card: #f5f9ff;
  --bg: #ffffff;
  --text: #1a1a2e;
  --shadow-card: 0 1px 3px rgba(0,0,0,0.08);
  --radius: 6px;
}
body.theme-dark {
  --c-primary: #5b9bd5;
  --c-primary-light: #80b4e0;
  --c-text: #e6e6f0;
  --c-text-muted: #aaa;
  --c-border: #37415a;
  --c-bg-card: #232946;
  --bg: #1a1a2e;
  --text: #e6e6f0;
}
body.theme-dark { background: var(--bg) !important; color: var(--text) !important; }
body.theme-dark .flow-table th { background: #283350; }
body.theme-dark .flow-table tr:nth-child(even) { background: #20283d; }
body.theme-dark .flow-table td { color: var(--text); border-bottom-color: #2a3450; }
body.theme-dark .narrative { background: #232946; color: var(--text); }
body.theme-dark .chart-box { background: var(--bg); border-color: var(--c-border); }
body.theme-dark .notes { background: #232946; color: #bbb; }
body.theme-dark .insight-card.critical { background: #3a1f22; }
body.theme-dark .insight-card.warning { background: #3a2e1a; }
body.theme-dark .insight-card.info { background: #202a3a; }
body.theme-dark .insight-card.positive { background: #1f3a2a; }
body.theme-dark h1, body.theme-dark h2, body.theme-dark .section-title { color: var(--c-primary-light); }
@page {
  size: A4 landscape;
  margin: 12mm 10mm 18mm 10mm;
  @bottom-right { content: "Page " counter(page); font-size: 8px; color: #999; }
  @bottom-left { content: "F-A-C株式会社 | ハローワーク求人データ分析レポート"; font-size: 8px; color: #999; }
}
.cover-page {
  min-height: 180mm;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  text-align: center;
  padding: 20mm 15mm;
  page-break-after: always;
  border: 1px solid var(--c-border);
  border-radius: 8px;
  background: linear-gradient(135deg, var(--c-bg-card) 0%, var(--bg) 100%);
  position: relative;
  margin-bottom: 16px;
}
.cover-logo { width: 200px; height: 70px; display: flex; align-items: center; justify-content: center; color: var(--c-text-muted); font-size: 12px; border: 1px dashed var(--c-border); border-radius: 6px; margin-bottom: 30px; }
.cover-title { font-size: 28px; font-weight: 700; color: var(--c-primary); margin: 8px 0 6px; letter-spacing: 0.04em; }
.cover-sub { font-size: 16px; color: var(--text); margin-bottom: 24px; }
.cover-grade { font-size: 14px; color: var(--text); margin-bottom: 30px; padding: 8px 16px; border: 2px solid var(--c-primary); border-radius: 6px; display: inline-block; font-weight: bold; }
.cover-confidential { margin-top: 40px; font-size: 11px; color: var(--c-text-muted); border-top: 1px solid var(--c-border); padding-top: 14px; width: 70%; }
.cover-footer-cov { position: absolute; bottom: 10mm; left: 0; right: 0; font-size: 10px; color: var(--c-text-muted); }
.theme-toggle {
  position: fixed; top: 10px; right: 160px; z-index: 100;
  padding: 6px 12px; font-size: 12px; cursor: pointer;
  border: 1px solid var(--c-border); border-radius: 4px;
  background: var(--bg); color: var(--text);
}
.theme-toggle:focus { outline: 2px solid var(--c-primary); outline-offset: 2px; }
.screen-footer {
  margin-top: 24px; padding: 10px 16px;
  border-top: 1px solid var(--c-border);
  font-size: 10px; color: var(--c-text-muted);
  display: flex; justify-content: space-between;
}
* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: "Yu Gothic","Meiryo","Hiragino Sans",sans-serif; font-size: 11px; color: var(--text); background: var(--bg); padding: 20px; transition: background 0.2s, color 0.2s; }
h1 { font-size: 20px; color: var(--c-primary); border-bottom: 3px solid var(--c-primary); padding-bottom: 8px; margin-bottom: 6px; }
h2 { font-size: 14px; color: #2c3e50; margin: 16px 0 8px 0; border-bottom: 1px solid #ddd; padding-bottom: 4px; }
.subtitle { color: #666; font-size: 12px; margin-bottom: 12px; }
.grade-box { display: flex; align-items: center; gap: 16px; margin-bottom: 16px; padding: 16px; border: 2px solid; border-radius: 8px; }
.grade-circle { width: 60px; height: 60px; border-radius: 50%; display: flex; align-items: center; justify-content: center; font-size: 28px; font-weight: bold; color: #fff; flex-shrink: 0; }
.grade-detail { flex: 1; }
.grade-label { font-size: 16px; font-weight: bold; }
.grade-desc { font-size: 11px; color: #555; margin-top: 4px; line-height: 1.5; }
.findings-list { margin: 12px 0; padding-left: 20px; }
.findings-list li { font-size: 11px; line-height: 1.8; color: #333; }
.kpi-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(120px, 1fr)); gap: 10px; margin-bottom: 16px; }
.kpi-card { border: 1px solid var(--c-border); border-radius: var(--radius); padding: 12px; text-align: center; position: relative; overflow: hidden; transition: transform 0.2s, box-shadow 0.2s; background: #fff; box-shadow: var(--shadow-card); }
.kpi-card::before { content: ''; position: absolute; top: 0; left: 0; right: 0; height: 4px; background: linear-gradient(90deg, var(--c-primary), var(--c-primary-light)); }
.kpi-card:hover { transform: translateY(-2px); box-shadow: 0 4px 12px rgba(0,0,0,0.12); }
.kpi-value { font-size: 18px; font-weight: bold; }
.kpi-label { font-size: 9px; color: #666; margin-top: 3px; }
.kpi-subtitle { font-size: 8px; color: var(--c-text-muted); margin-top: 2px; }
.section-title { font-size: 15px; color: var(--c-primary); margin: 16px 0 8px 0; border-left: 4px solid var(--c-primary); padding-left: 8px; }
.section-question { font-size: 10px; color: #666; font-style: italic; margin-bottom: 10px; }
.narrative { background: #f8f9fa; border-left: 3px solid var(--c-primary); padding: 10px 14px; margin-bottom: 12px; font-size: 11px; line-height: 1.6; color: #444; }
.sortable-table th { cursor: pointer; user-select: none; position: relative; padding-right: 18px; }
.sortable-table th::after { content: '↕'; position: absolute; right: 4px; top: 50%; transform: translateY(-50%); font-size: 10px; color: #999; opacity: 0.5; }
.sortable-table th.sort-asc::after { content: '▲'; opacity: 1; color: var(--c-primary); }
.sortable-table th.sort-desc::after { content: '▼'; opacity: 1; color: var(--c-primary); }
.guide-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: 6px; margin: 8px 0 12px; }
.guide-item { background: var(--c-bg-card); border-left: 3px solid var(--c-primary-light); padding: 6px 8px; font-size: 9px; line-height: 1.4; }
.guide-item strong { color: var(--c-primary); display: block; margin-bottom: 2px; }
.chart-box { border: 1px solid #e2e8f0; border-radius: 6px; padding: 12px; margin-bottom: 12px; }
.chart-box h3 { font-size: 11px; color: #666; margin-bottom: 6px; }
.chart-interp { font-size: 9px; color: #666; margin-top: 6px; font-style: italic; border-top: 1px solid #eee; padding-top: 4px; }
.two-col { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
.insight-card { padding: 10px 14px; margin-bottom: 8px; border-radius: 0 4px 4px 0; }
.insight-card.critical { border-left: 6px solid #ef4444; background: #fef2f2; }
.insight-card.warning { border-left: 4px solid #f59e0b; background: #fffbeb; }
.insight-card.info { border-left: 2px solid #93c5fd; background: #f8fafc; font-size: 10px; }
.insight-card.positive { border-left: 2px solid #6ee7b7; background: #f0fdf4; font-size: 10px; }
.insight-title { font-weight: bold; margin-bottom: 3px; }
.insight-card.critical .insight-title { font-size: 13px; color: #dc2626; }
.insight-card.warning .insight-title { font-size: 12px; color: #92400e; }
.insight-card.info .insight-title { font-size: 11px; color: #1e40af; }
.insight-card.positive .insight-title { font-size: 11px; color: #065f46; }
.insight-body { font-size: 10px; color: #555; line-height: 1.5; }
.so-what { font-size: 10px; color: #1a5276; font-weight: bold; margin-top: 4px; }
.evidence { font-size: 8px; color: #999; margin-top: 3px; }
.flow-table { width: 100%; border-collapse: collapse; font-size: 11px; }
.flow-table th { background: #2c3e50; color: #fff; padding: 6px 8px; text-align: left; font-size: 10px; }
.flow-table td { padding: 5px 8px; border-bottom: 1px solid #eee; }
.flow-table tr:nth-child(even) { background: #f8f9fa; }
.metric-row { display: flex; justify-content: space-between; padding: 4px 0; border-bottom: 1px solid #f1f5f9; font-size: 10px; }
.metric-label { color: #64748b; }
.metric-value { font-weight: bold; }
.notes { margin-top: 16px; padding: 10px 14px; background: #f1f5f9; border-radius: 4px; font-size: 8px; color: #64748b; }
.print-btn { position: fixed; top: 10px; right: 10px; padding: 8px 16px; background: #2563eb; color: #fff; border: none; border-radius: 6px; cursor: pointer; font-size: 12px; z-index: 100; }
.report-page { page-break-after: always; }
.report-page:last-child { page-break-after: auto; }
@media print {
    -webkit-print-color-adjust: exact !important;
    print-color-adjust: exact !important;
    .print-btn { display: none; }
    .theme-toggle, .screen-footer, .no-print { display: none !important; }
    body, body.theme-dark { background: #fff !important; color: #333 !important; }
    body.theme-dark .flow-table th { background: #2c3e50 !important; color: #fff !important; }
    body.theme-dark .flow-table td { color: #333 !important; background: transparent !important; }
    body.theme-dark .flow-table tr:nth-child(even) { background: #f8f9fa !important; }
    body.theme-dark .narrative { background: #f8f9fa !important; color: #444 !important; }
    body.theme-dark .notes { background: #f1f5f9 !important; color: #64748b !important; }
    body.theme-dark .insight-card.critical { background: #fef2f2 !important; }
    body.theme-dark .insight-card.warning { background: #fffbeb !important; }
    body.theme-dark .insight-card.info { background: #f8fafc !important; }
    body.theme-dark .insight-card.positive { background: #f0fdf4 !important; }
    .cover-page { page-break-after: always; border: none !important; background: #fff !important; min-height: 85vh; }
    .no-break { page-break-inside: avoid; break-inside: avoid; }
    .chart-box { page-break-inside: avoid; break-inside: avoid; }
    .insight-card { page-break-inside: avoid; break-inside: avoid; }
    h2, .section-title { page-break-after: avoid; break-after: avoid; }
    .section-question { page-break-after: avoid; break-after: avoid; }
    .narrative { page-break-after: avoid; break-after: avoid; }
    body { padding: 0; }
    .report-page { min-height: auto; }
    .kpi-card { box-shadow: none !important; transform: none !important; }
    .sortable-table th::after { display: none; }
    .sortable-table th { cursor: default; padding-right: 8px; }
    thead { display: table-header-group; }
    .echart, [data-chart-config] { page-break-inside: avoid; break-inside: avoid; }
}
.metric-row { display: flex; justify-content: space-between; padding: 6px 0; border-bottom: 1px solid #f1f5f9; }
.metric-label { color: #64748b; font-size: 10px; }
.metric-value { font-weight: bold; font-size: 11px; }
</style>
<script src="https://cdn.jsdelivr.net/npm/echarts@5/dist/echarts.min.js"></script>
</head>
<body>
<button class="theme-toggle no-print" type="button" onclick="toggleTheme()" aria-label="ダークモード/ライトモードを切替">🌙 ダーク / ☀ ライト</button>
<button class="print-btn" onclick="window.print()" aria-label="印刷またはPDFで保存">印刷 / PDF保存</button>
"#);

    // 採用困難度グレード算出
    let vacancy_rate = ctx
        .vacancy
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")
        .map(|r| get_f64(r, "vacancy_rate"))
        .unwrap_or(0.0);
    let grade = super::report::compute_difficulty_grade(insights, vacancy_rate);
    let top_findings = super::report::extract_top_findings(insights);

    // ===== 表紙ページ =====
    write!(html,
        r#"<section class="cover-page" role="region" aria-labelledby="cover-title-id">
<div class="cover-title" id="cover-title-id">ハローワーク求人市場 総合診断レポート</div>
<div class="cover-sub">{location} &nbsp;|&nbsp; {today}</div>
<div class="cover-grade" style="border-color:{gcolor};color:{gcolor}">採用困難度: {gletter} {glabel}</div>
<div class="cover-confidential">この資料は機密情報です。外部への持ち出しは社内規定に従ってください。</div>
<div class="cover-footer-cov">F-A-C株式会社 &nbsp;|&nbsp; 生成日時: {today}</div>
</section>
"#,
        location = escape_html(&location),
        today = escape_html(&today),
        gcolor = grade.color,
        gletter = grade.letter,
        glabel = grade.label,
    ).unwrap();

    // ===== Page 1: エグゼクティブサマリー =====
    // タイトルは cover-page に出力済み、本文冒頭ではサブタイトル（地域+日付）のみ表示
    html.push_str("<section class=\"report-page\">");
    write!(html,
        r#"<div class="subtitle" style="font-size:14px;margin-bottom:12px;">{location} | {today}</div>"#
    ).unwrap();

    // 採用困難度グレード（大きく表示）
    write!(
        html,
        "<div class=\"grade-box\" style=\"border-color:{color}\">
            <div class=\"grade-circle\" style=\"background:{color}\">{letter}</div>
            <div class=\"grade-detail\">
                <div class=\"grade-label\">採用困難度: {label}</div>
                <div class=\"grade-desc\">{summary}</div>
            </div>
        </div>",
        color = grade.color,
        letter = grade.letter,
        label = grade.label,
        summary = escape_html(&summary),
    )
    .unwrap();

    // Top3発見
    if !top_findings.is_empty() {
        html.push_str("<h2>主な発見</h2><ol class=\"findings-list\">");
        for f in &top_findings {
            write!(html, "<li>{}</li>", escape_html(f)).unwrap();
        }
        html.push_str("</ol>");
    }

    // 推奨アクションTop3
    let actions: Vec<_> = insights
        .iter()
        .filter(|i| i.category == InsightCategory::ActionProposal)
        .take(3)
        .collect();
    if !actions.is_empty() {
        html.push_str("<h2>推奨アクション</h2><ol class=\"findings-list\">");
        for a in &actions {
            let so_what = super::report::generate_so_what(a);
            write!(
                html,
                "<li><strong>{}</strong> {}</li>",
                escape_html(&a.title),
                escape_html(&so_what)
            )
            .unwrap();
        }
        html.push_str("</ol>");
    }

    // KPIカード
    html.push_str(r#"<div class="kpi-grid">"#);

    // 求人数
    let total_postings: i64 = ctx
        .vacancy
        .iter()
        .map(|r| super::super::helpers::get_i64(r, "total_count"))
        .sum();
    report_kpi(
        &mut html,
        "総求人数",
        &format_number(total_postings),
        "#2563eb",
        "",
    );

    // 欠員率
    let vacancy_rate = ctx
        .vacancy
        .iter()
        .find(|r| super::super::helpers::get_str_ref(r, "emp_group") == "正社員")
        .map(|r| super::super::helpers::get_f64(r, "vacancy_rate"))
        .unwrap_or(0.0);
    let vr_color = if vacancy_rate > 0.3 {
        "#dc2626"
    } else if vacancy_rate > 0.2 {
        "#d97706"
    } else {
        "#059669"
    };
    let vr_sub = if vacancy_rate > super::report::NATIONAL_AVG_VACANCY_RATE {
        "▲ 全国平均以上"
    } else {
        "▼ 全国平均以下"
    };
    report_kpi(
        &mut html,
        "欠員補充率(正社員)",
        &format!("{:.1}%", vacancy_rate * 100.0),
        vr_color,
        vr_sub,
    );

    // 平均月給
    let avg_salary = ctx
        .cascade
        .iter()
        .find(|r| super::super::helpers::get_str_ref(r, "emp_group") == "正社員")
        .map(|r| super::super::helpers::get_f64(r, "avg_salary_min") as i64)
        .unwrap_or(0);
    if avg_salary > 0 {
        report_kpi(
            &mut html,
            "平均月給",
            &format!("{}円", format_number(avg_salary)),
            "#333",
            "",
        );
    } else {
        report_kpi(&mut html, "平均月給", "-", "#999", "");
    }

    // 通勤圏人口
    if ctx.commute_zone_total_pop > 0 {
        let zone_sub = format!(
            "{}市区町村 / {}県",
            ctx.commute_zone_count, ctx.commute_zone_pref_count
        );
        report_kpi(
            &mut html,
            "通勤圏人口",
            &format!("{}人", format_number(ctx.commute_zone_total_pop)),
            "#0891b2",
            &zone_sub,
        );
    } else {
        report_kpi(&mut html, "通勤圏人口", "-", "#999", "");
    }

    // 通勤流入
    if ctx.commute_inflow_total > 0 {
        let flow_sub = if ctx.commute_self_rate > 0.0 {
            format!("地元就業率 {:.0}%", ctx.commute_self_rate * 100.0)
        } else {
            String::new()
        };
        report_kpi(
            &mut html,
            "通勤流入数",
            &format!("{}人", format_number(ctx.commute_inflow_total)),
            "#7c3aed",
            &flow_sub,
        );
    } else {
        report_kpi(&mut html, "通勤流入数", "-", "#999", "");
    }
    html.push_str("</div>");

    // 示唆サマリーバッジ
    html.push_str(r#"<div class="badge-row">"#);
    if critical > 0 {
        write!(
            html,
            r#"<span class="badge badge-critical">重大 {}件</span>"#,
            critical
        )
        .unwrap();
    }
    if warning > 0 {
        write!(
            html,
            r#"<span class="badge badge-warning">注意 {}件</span>"#,
            warning
        )
        .unwrap();
    }
    if info > 0 {
        write!(
            html,
            r#"<span class="badge badge-info">情報 {}件</span>"#,
            info
        )
        .unwrap();
    }
    if positive > 0 {
        write!(
            html,
            r#"<span class="badge badge-positive">良好 {}件</span>"#,
            positive
        )
        .unwrap();
    }
    html.push_str("</div>");

    html.push_str("</section>"); // End Page 1

    // ===== Page 2: 市場概況 =====
    html.push_str("<section class=\"report-page\">");
    html.push_str("<div class=\"section-title\">市場概況</div>");
    html.push_str("<div class=\"section-question\">この地域の求人市場は今どういう状態か?</div>");
    html.push_str(r#"<div class="guide-grid">
<div class="guide-item"><strong>通勤フロー</strong>他県から働きに来る人の割合。高い=広域採用エリア。</div>
<div class="guide-item"><strong>地元就業率</strong>地元在住者で地元就業する割合。低い=外部流入依存。</div>
<div class="guide-item"><strong>流入元TOP3</strong>通勤者数が多い他地域。採用広告の配信対象として有効。</div>
</div>"#);

    // 通勤フローテーブル
    if !ctx.commute_inflow_top3.is_empty() {
        html.push_str(r#"<h2>通勤フロー（国勢調査実データ）</h2>"#);
        html.push_str(r#"<table class="sortable-table flow-table"><thead><tr><th>流入元</th><th>通勤者数</th></tr></thead><tbody>"#);
        for (p, m, c) in &ctx.commute_inflow_top3 {
            write!(
                html,
                "<tr><td>{}{}</td><td style=\"text-align:right\">{}</td></tr>",
                escape_html(p),
                escape_html(m),
                format_number(*c)
            )
            .unwrap();
        }
        html.push_str("</tbody></table>");
        if ctx.commute_self_rate > 0.0 {
            write!(
                html,
                r#"<div style="font-size:9px;color:#888;margin-top:4px">地元就業率: {:.1}%</div>"#,
                ctx.commute_self_rate * 100.0
            )
            .unwrap();
        }
    }

    html.push_str("</section>"); // End Page 2

    // ===== 追加セクションA: 地域経済環境 =====
    render_regional_economy_section(&mut html, ctx);

    // ===== 追加セクションB: 労働力の将来リスク =====
    render_labor_future_risk_section(&mut html, ctx);

    // ===== Page 3: チャート + 将来予測 =====
    html.push_str("<section class=\"report-page\">");
    html.push_str("<div class=\"section-title\">市場構造と将来予測</div>");
    html.push_str("<div class=\"section-question\">人材はこの先どうなるか?</div>");
    html.push_str(r#"<div class="guide-grid">
<div class="guide-item"><strong>産業別求人</strong>正社員求人数の多い業界TOP10。市場の主要プレーヤー。</div>
<div class="guide-item"><strong>人口ピラミッド</strong>性別×年齢の人口分布。労働力供給の将来予測に活用。</div>
<div class="guide-item"><strong>若年層比率</strong>15〜39歳の人口割合。低い=採用難易度が上昇する地域。</div>
</div>"#);

    // 給与帯分布（cascadeテーブルから）
    let salary_data: Vec<(String, i64)> = ctx
        .cascade
        .iter()
        .filter(|r| get_str_ref(r, "emp_group") == "正社員")
        .map(|r| {
            (
                escape_html(get_str_ref(r, "industry_raw")),
                get_i64(r, "posting_count"),
            )
        })
        .filter(|(_, c)| *c > 0)
        .take(10)
        .collect();

    if !salary_data.is_empty() {
        let labels: Vec<String> = salary_data
            .iter()
            .map(|(l, _)| format!("\"{}\"", l))
            .collect();
        let values: Vec<String> = salary_data.iter().map(|(_, v)| v.to_string()).collect();
        let chart_json = format!(
            "{{\"tooltip\":{{\"trigger\":\"axis\"}},\"xAxis\":{{\"type\":\"category\",\"data\":[{}],\"axisLabel\":{{\"rotate\":30,\"fontSize\":9}}}},\"yAxis\":{{\"type\":\"value\"}},\"series\":[{{\"type\":\"bar\",\"data\":[{}],\"itemStyle\":{{\"color\":\"#3b82f6\"}}}}]}}",
            labels.join(","), values.join(",")
        );
        let interp = super::report::interpret_industry_chart(&salary_data);
        html.push_str("<div class=\"chart-box no-break\">");
        html.push_str("<h3>産業別求人数（正社員 Top10）</h3>");
        write!(html, "<div class=\"report-chart\" style=\"width:100%;height:280px;\" data-chart-config='{}'></div>", chart_json).unwrap();
        if !interp.is_empty() {
            write!(
                html,
                "<div class=\"chart-interp\">{}</div>",
                escape_html(&interp)
            )
            .unwrap();
        }
        html.push_str("</div>");
    }

    // 蝶形ピラミッド（通勤圏 vs 選択市区町村）
    if !ctx.ext_pyramid.is_empty() {
        // ピラミッドデータ構築
        let ages: Vec<String> = ctx
            .ext_pyramid
            .iter()
            .map(|r| get_str_ref(r, "age_group").to_string())
            .collect();
        let local_male: Vec<String> = ctx
            .ext_pyramid
            .iter()
            .map(|r| format!("{}", -get_i64(r, "male_count")))
            .collect();
        let local_female: Vec<String> = ctx
            .ext_pyramid
            .iter()
            .map(|r| format!("{}", get_i64(r, "female_count")))
            .collect();
        let ages_json: Vec<String> = ages.iter().map(|a| format!("\"{}\"", a)).collect();

        let mut pyramid_json = String::with_capacity(1_000);
        pyramid_json
            .push_str("{\"tooltip\":{\"trigger\":\"axis\",\"axisPointer\":{\"type\":\"shadow\"}},");
        pyramid_json.push_str("\"legend\":{\"data\":[\"男性\",\"女性\"],\"textStyle\":{\"color\":\"#333\",\"fontSize\":10},\"bottom\":0},");
        pyramid_json.push_str("\"grid\":{\"left\":\"3%\",\"right\":\"3%\",\"top\":\"3%\",\"bottom\":\"12%\",\"containLabel\":true},");
        pyramid_json.push_str("\"xAxis\":{\"type\":\"value\"},");
        pyramid_json.push_str(&format!(
            "\"yAxis\":{{\"type\":\"category\",\"data\":[{}],\"axisTick\":{{\"show\":false}}}},",
            ages_json.join(",")
        ));
        pyramid_json.push_str(&format!("\"series\":[{{\"name\":\"男性\",\"type\":\"bar\",\"data\":[{}],\"itemStyle\":{{\"color\":\"#3b82f6\"}}}},", local_male.join(",")));
        pyramid_json.push_str(&format!("{{\"name\":\"女性\",\"type\":\"bar\",\"data\":[{}],\"itemStyle\":{{\"color\":\"#ec4899\"}}}}]", local_female.join(",")));
        pyramid_json.push('}');

        html.push_str("<div class=\"chart-box no-break\" style=\"margin-top:16px\">");
        html.push_str("<h3>人口ピラミッド（性別×年齢）</h3>");
        write!(html, "<div class=\"report-chart\" style=\"width:100%;height:400px;\" data-chart-config='{}'></div>", pyramid_json).unwrap();
        let pyramid_interp = super::report::interpret_pyramid(&ctx.ext_pyramid);
        if !pyramid_interp.is_empty() {
            write!(
                html,
                "<div class=\"chart-interp\">{}</div>",
                escape_html(&pyramid_interp)
            )
            .unwrap();
        }
        html.push_str(
            "<div style=\"font-size:8px;color:#999;margin-top:4px\">※外部統計データ(SSDSE-A)</div>",
        );
        html.push_str("</div>");
    }

    // 通勤フロー（サンキー）
    html.push_str("</section>"); // End Page 3

    if ctx.commute_inflow_total > 0 {
        // ===== Page 4: 通勤フロー =====
        html.push_str("<section class=\"report-page\">");
        html.push_str("<div class=\"section-title\">通勤フロー分析</div>");
        html.push_str("<div class=\"section-question\">実際に誰がどこから通勤しているか?</div>");
        html.push_str(r#"<div class="guide-grid">
<div class="guide-item"><strong>通勤流入</strong>他地域から当該地域へ通勤する人数。採用の供給源。</div>
<div class="guide-item"><strong>昼夜間人口比</strong>1.0超=昼間流入（都市型）、1.0未満=流出（ベッドタウン型）。</div>
<div class="guide-item"><strong>30km圏</strong>実質的な採用商圏。この範囲内の人口が採用対象。</div>
</div>"#);

        // 流入テーブル（詳細版）
        html.push_str(r#"<div class="two-col">"#);

        // 流入側
        html.push_str(
            r#"<div class="chart-box"><h3>通勤流入 Top10（この地域に通勤してくる人）</h3>"#,
        );
        html.push_str(r#"<table class="sortable-table flow-table"><thead><tr><th>流入元</th><th>通勤者数</th><th>男性</th><th>女性</th></tr></thead><tbody>"#);
        for (p, m, c) in &ctx.commute_inflow_top3 {
            write!(
                html,
                "<tr><td>{}{}</td><td style=\"text-align:right\">{}</td><td></td><td></td></tr>",
                escape_html(p),
                escape_html(m),
                format_number(*c)
            )
            .unwrap();
        }
        html.push_str("</tbody></table></div>");

        // 地元就業率+通勤圏メトリクス
        html.push_str(r#"<div class="chart-box"><h3>通勤圏の特徴</h3>"#);
        if ctx.commute_self_rate > 0.0 {
            report_metric(
                &mut html,
                "地元就業率",
                &format!("{:.1}%", ctx.commute_self_rate * 100.0),
            );
        }
        report_metric(
            &mut html,
            "通勤流入総数",
            &format!("{}人", format_number(ctx.commute_inflow_total)),
        );
        report_metric(
            &mut html,
            "通勤流出総数",
            &format!("{}人", format_number(ctx.commute_outflow_total)),
        );
        if ctx.commute_zone_count > 0 {
            report_metric(
                &mut html,
                "30km圏市区町村数",
                &format!(
                    "{}件 / {}県",
                    ctx.commute_zone_count, ctx.commute_zone_pref_count
                ),
            );
        }

        // 昼夜間人口比
        // 2026-05-15: DB column は `day_night_ratio` (% 単位 e.g. 96.42)。/100.0 で ratio 化
        if let Some(row) = ctx.ext_daytime_pop.first() {
            let ratio = get_f64(row, "day_night_ratio") / 100.0;
            if ratio > 0.0 {
                let label = if ratio > 1.05 {
                    "都市型（通勤流入）"
                } else if ratio < 0.95 {
                    "ベッドタウン型"
                } else {
                    "均衡型"
                };
                report_metric(
                    &mut html,
                    "昼夜間人口比",
                    &format!("{:.2} ({})", ratio, label),
                );
            }
        }
        html.push_str("</div>");
        html.push_str("</div>"); // two-col close
    }

    if ctx.commute_inflow_total > 0 {
        html.push_str("</section>"); // End Page 4 (commute)
    }

    // ===== Page 5+: 示唆詳細（章ごとに独立section） =====

    let categories = [
        (InsightCategory::HiringStructure, "第1章: 採用構造分析"),
        (InsightCategory::Forecast, "第2章: 将来予測"),
        (InsightCategory::RegionalCompare, "第3章: 地域比較"),
        (InsightCategory::ActionProposal, "第4章: 推奨アクション"),
        (InsightCategory::StructuralContext, "第5章: 構造分析"),
    ];

    let chapter_questions = [
        "この地域で採用が難しい構造的な原因は何か?",
        "市場はこの先どうなるか? 人材は確保できるか?",
        "他の地域と比較してどのような差分があるか?",
        "具体的に何をすべきか?",
        "地域の世帯・労働・医療・教育構造はどうなっているか?",
    ];

    for (idx, (cat, title)) in categories.iter().enumerate() {
        let filtered: Vec<_> = insights.iter().filter(|i| &i.category == cat).collect();
        if filtered.is_empty() {
            continue;
        }

        // 章ごとに独立したsection（印刷ページ分割）
        html.push_str("<section class=\"report-page\">");
        write!(html, r#"<h2>{title}</h2>"#).unwrap();
        write!(
            html,
            "<div class=\"section-question\">{}</div>",
            chapter_questions[idx]
        )
        .unwrap();
        html.push_str(r#"<div class="guide-grid">
<div class="guide-item"><strong>重大</strong>早急な対応が必要な課題。放置すると採用難が深刻化する可能性。</div>
<div class="guide-item"><strong>注意</strong>モニタリングが必要な項目。トレンド次第で重大化。</div>
<div class="guide-item"><strong>情報</strong>参考情報。戦略立案時の補助データとして活用。</div>
<div class="guide-item"><strong>良好</strong>現状で優位な領域。強みとして訴求可能。</div>
</div>"#);

        // 章ナラティブ（具体数値入り）
        let narrative = super::report::generate_chapter_narrative(cat, &filtered, ctx);
        write!(
            html,
            "<div class=\"narrative\">{}</div>",
            escape_html(&narrative)
        )
        .unwrap();

        for (card_idx, insight) in filtered.iter().enumerate() {
            // 5件ごとにpage-break挿入
            if card_idx > 0 && card_idx % 5 == 0 {
                html.push_str("</section><section class=\"report-page\">");
                write!(html, "<h2>{title}（続き）</h2>").unwrap();
            }

            let cls = match insight.severity {
                Severity::Critical => "critical",
                Severity::Warning => "warning",
                Severity::Info => "info",
                Severity::Positive => "positive",
            };
            write!(
                html,
                r#"<div class="insight-card {cls} no-break">
                    <div class="insight-title">[{}] {}</div>"#,
                escape_html(insight.severity.label()),
                escape_html(&insight.title),
            )
            .unwrap();

            // severity別の表示量制御
            match insight.severity {
                Severity::Critical | Severity::Warning => {
                    // 全項目表示
                    write!(
                        html,
                        r#"<div class="insight-body">{}</div>"#,
                        escape_html(&insight.body),
                    )
                    .unwrap();
                    if !insight.evidence.is_empty() {
                        let ev_text: Vec<String> = insight
                            .evidence
                            .iter()
                            .map(|e| format!("{}: {}", e.metric, e.context))
                            .collect();
                        write!(
                            html,
                            r#"<div class="evidence">{}</div>"#,
                            escape_html(&ev_text.join(" | "))
                        )
                        .unwrap();
                    }
                    let so_what = super::report::generate_so_what(insight);
                    if !so_what.is_empty() {
                        write!(
                            html,
                            r#"<div class="so-what">{}</div>"#,
                            escape_html(&so_what)
                        )
                        .unwrap();
                    }
                }
                Severity::Info => {
                    // 本文のみ（evidence非表示）
                    write!(html,
                        r#"<div class="insight-body" style="display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden">{}</div>"#,
                        escape_html(&insight.body),
                    ).unwrap();
                    let so_what = super::report::generate_so_what(insight);
                    if !so_what.is_empty() {
                        write!(
                            html,
                            r#"<div class="so-what">{}</div>"#,
                            escape_html(&so_what)
                        )
                        .unwrap();
                    }
                }
                Severity::Positive => {
                    // 本文1行のみ（so-what/evidence非表示）
                    write!(html,
                        r#"<div class="insight-body" style="display:-webkit-box;-webkit-line-clamp:1;-webkit-box-orient:vertical;overflow:hidden">{}</div>"#,
                        escape_html(&insight.body),
                    ).unwrap();
                }
            }
            html.push_str("</div>");
        }
        html.push_str("</section>"); // 章の終わり
    }

    // ===== 最終ページ: 注記 =====
    html.push_str("<section class=\"report-page\">");
    html.push_str(r#"<div class="notes">
        <strong>データソースと注意事項</strong><br>
        ・ハローワーク掲載求人に基づく分析です。IT・通信等のHW掲載が少ない産業は参考値としてご利用ください。<br>
        ・通勤フローは2020年国勢調査データに基づきます。<br>
        ・充足グレードはMLモデルによる推計値です。
    </div>"#);

    html.push_str("</section>"); // End notes page

    // ===== 画面用フッター =====
    write!(
        html,
        r#"<div class="screen-footer no-print">
<span>F-A-C株式会社 | ハローワーク求人データ分析レポート</span>
<span>生成日時: {today}</span>
</div>
"#,
        today = escape_html(&today)
    )
    .unwrap();

    // ECharts初期化スクリプト（SVGレンダラー + 印刷/リサイズ対応）
    html.push_str(r#"<script>
function toggleTheme() {
  document.body.classList.toggle('theme-dark');
  try {
    localStorage.setItem('report-theme',
      document.body.classList.contains('theme-dark') ? 'dark' : 'light');
  } catch(e) {}
}
(function() {
  try {
    if (localStorage.getItem('report-theme') === 'dark') {
      document.body.classList.add('theme-dark');
    }
  } catch(e) {}
})();
document.addEventListener('DOMContentLoaded', function() {
  // a11y: セクション/テーブルに role/aria を付与
  document.querySelectorAll('.report-page').forEach(function(s, i) {
    if (!s.getAttribute('role')) s.setAttribute('role', 'region');
    var h = s.querySelector('h1, h2, .section-title');
    if (h) {
      if (!h.id) h.id = 'rp-heading-' + i;
      s.setAttribute('aria-labelledby', h.id);
    }
  });
  document.querySelectorAll('.sortable-table').forEach(function(t) {
    t.setAttribute('role', 'grid');
    t.querySelectorAll('th').forEach(function(th) {
      th.setAttribute('aria-sort', 'none');
      th.setAttribute('tabindex', '0');
    });
  });
});
document.addEventListener('DOMContentLoaded', function() {
    var charts = [];
    document.querySelectorAll('.report-chart[data-chart-config]').forEach(function(el) {
        try {
            var config = JSON.parse(el.getAttribute('data-chart-config'));
            config.animation = false;
            config.backgroundColor = '#ffffff';
            var chart = echarts.init(el, null, { renderer: 'svg' });
            chart.setOption(config);
            charts.push(chart);
        } catch(e) { console.warn('Chart init error:', e); }
    });
    window.addEventListener('beforeprint', function() { charts.forEach(function(c) { c.resize(); }); });
    window.addEventListener('resize', function() { charts.forEach(function(c) { c.resize(); }); });
});
function initSortableTables() {
  document.querySelectorAll('.sortable-table').forEach(function(table) {
    table.querySelectorAll('th').forEach(function(th, colIdx) {
      th.addEventListener('click', function() {
        var tbody = table.querySelector('tbody');
        if (!tbody) return;
        var rows = Array.from(tbody.querySelectorAll('tr'));
        var isAsc = th.classList.contains('sort-asc');
        table.querySelectorAll('th').forEach(function(h) { h.classList.remove('sort-asc','sort-desc'); h.setAttribute('aria-sort','none'); });
        th.classList.add(isAsc ? 'sort-desc' : 'sort-asc');
        th.setAttribute('aria-sort', isAsc ? 'descending' : 'ascending');
        rows.sort(function(a,b) {
          var at = a.children[colIdx] ? a.children[colIdx].textContent.trim() : '';
          var bt = b.children[colIdx] ? b.children[colIdx].textContent.trim() : '';
          var an = parseFloat(at.replace(/[,件%万円倍+人]/g,''));
          var bn = parseFloat(bt.replace(/[,件%万円倍+人]/g,''));
          if (!isNaN(an) && !isNaN(bn)) return isAsc ? bn-an : an-bn;
          return isAsc ? bt.localeCompare(at,'ja') : at.localeCompare(bt,'ja');
        });
        rows.forEach(function(r) { tbody.appendChild(r); });
      });
    });
  });
}
document.addEventListener('DOMContentLoaded', initSortableTables);
</script>"#);

    html.push_str("</body></html>");
    html
}

fn report_metric(html: &mut String, label: &str, value: &str) {
    write!(html,
        r#"<div class="metric-row"><span class="metric-label">{label}</span><span class="metric-value">{value}</span></div>"#
    ).unwrap();
}

fn report_kpi(html: &mut String, label: &str, value: &str, color: &str, subtitle: &str) {
    write!(
        html,
        r#"<div class="kpi-card no-break">
            <div class="kpi-value" style="color:{color}">{value}</div>
            <div class="kpi-label">{label}</div>"#
    )
    .unwrap();
    if !subtitle.is_empty() {
        write!(html, r#"<div class="kpi-subtitle">{subtitle}</div>"#).unwrap();
    }
    html.push_str("</div>");
}

// ======== 追加セクション: 外部統計データ統合 ========

/// ECharts設定JSONをdata属性に埋め込んだチャートdivを生成
/// （既存の .report-chart[data-chart-config] 初期化スクリプトで描画される）
fn render_echarts_div(chart_json: &str, height_px: u32) -> String {
    // data属性内のシングルクオート衝突を回避
    let escaped = chart_json.replace('\'', "&#x27;");
    format!(
        r#"<div class="report-chart" style="width:100%;height:{}px;" data-chart-config='{}'></div>"#,
        height_px, escaped
    )
}

/// セクションA: 地域経済環境（Page 2の後に挿入）
/// - 事業所統計 TOP10（水平棒グラフ）
/// - 企業新陳代謝（開業率/廃業率 折れ線）
/// - 人口移動（転入超過KPI）
fn render_regional_economy_section(html: &mut String, ctx: &InsightContext) {
    // 3データ全て空ならセクション自体をスキップ
    if ctx.ext_establishments.is_empty()
        && ctx.ext_business_dynamics.is_empty()
        && ctx.ext_migration.is_empty()
    {
        return;
    }

    html.push_str("<section class=\"report-page\">");
    html.push_str("<div class=\"section-title\">地域経済環境</div>");
    html.push_str("<div class=\"section-question\">この地域の経済規模・新陳代謝・人口の流出入はどうなっているか?</div>");

    // --- 1. 事業所統計 TOP10 ---
    if !ctx.ext_establishments.is_empty() {
        let top: Vec<(String, i64)> = ctx
            .ext_establishments
            .iter()
            .map(|r| {
                // 2026-05-15: industry は SQL alias で industry_code (例: P85)、
                //   industry_name が日本語名。UI ラベルは industry_name を使う
                (
                    get_str_ref(r, "industry_name").to_string(),
                    get_i64(r, "establishment_count"),
                )
            })
            .filter(|(_, c)| *c > 0)
            .take(10)
            .collect();

        if !top.is_empty() {
            // 水平棒グラフ: 上位を上に表示するためデータを逆順に
            let mut ordered = top.clone();
            ordered.reverse();
            let categories = serde_json::Value::Array(
                ordered
                    .iter()
                    .map(|(n, _)| serde_json::Value::String(n.clone()))
                    .collect(),
            );
            let values = serde_json::Value::Array(
                ordered.iter().map(|(_, v)| serde_json::json!(*v)).collect(),
            );
            let chart = serde_json::json!({
                "tooltip": {"trigger": "axis", "axisPointer": {"type": "shadow"}},
                "grid": {"left": "3%", "right": "8%", "top": "3%", "bottom": "3%", "containLabel": true},
                "xAxis": {"type": "value"},
                "yAxis": {"type": "category", "data": categories, "axisLabel": {"fontSize": 9}},
                "series": [{
                    "type": "bar",
                    "data": values,
                    "itemStyle": {"color": "#1a5276"},
                    "label": {"show": true, "position": "right", "fontSize": 9}
                }]
            });

            html.push_str("<div class=\"chart-box no-break\">");
            html.push_str("<h3>産業別事業所数 TOP10</h3>");
            html.push_str(&render_echarts_div(&chart.to_string(), 320));
            let total: i64 = top.iter().map(|(_, v)| *v).sum();
            write!(html,
                r#"<div class="chart-interp">TOP10合計: {}件。事業所数が多い産業は雇用の受け皿となるが、同時に採用競合も多い。</div>"#,
                format_number(total)
            ).unwrap();
            html.push_str("</div>");
        }
    }

    // --- 2. 企業新陳代謝（開業率/廃業率） ---
    if !ctx.ext_business_dynamics.is_empty() {
        let years: Vec<String> = ctx
            .ext_business_dynamics
            .iter()
            .map(|r| get_str_ref(r, "fiscal_year").to_string())
            .collect();
        let opening: Vec<f64> = ctx
            .ext_business_dynamics
            .iter()
            .map(|r| get_f64(r, "opening_rate"))
            .collect();
        let closure: Vec<f64> = ctx
            .ext_business_dynamics
            .iter()
            .map(|r| get_f64(r, "closure_rate"))
            .collect();

        let has_data = opening.iter().any(|v| *v > 0.0) || closure.iter().any(|v| *v > 0.0);

        if has_data {
            let chart = serde_json::json!({
                "tooltip": {"trigger": "axis"},
                "legend": {"data": ["開業率", "廃業率"], "textStyle": {"fontSize": 10}, "top": 0},
                "grid": {"left": "3%", "right": "3%", "top": "18%", "bottom": "8%", "containLabel": true},
                "xAxis": {"type": "category", "data": years, "axisLabel": {"fontSize": 9}},
                "yAxis": {"type": "value", "axisLabel": {"formatter": "{value}%"}},
                "series": [
                    {"name": "開業率", "type": "line", "data": opening, "smooth": true, "itemStyle": {"color": "#059669"}},
                    {"name": "廃業率", "type": "line", "data": closure, "smooth": true, "itemStyle": {"color": "#dc2626"}}
                ]
            });

            html.push_str("<div class=\"chart-box no-break\">");
            html.push_str("<h3>企業新陳代謝（開業率 / 廃業率の推移）</h3>");
            html.push_str(&render_echarts_div(&chart.to_string(), 260));

            // 最新年度の開業-廃業ギャップを解釈
            let latest_open = opening.last().copied().unwrap_or(0.0);
            let latest_close = closure.last().copied().unwrap_or(0.0);
            let net = latest_open - latest_close;
            // feedback_correlation_not_causation.md 準拠:
            // 「成長市場」「減少局面」等の因果断定を避け、観測表現に統一。
            let interp = if net > 0.5 {
                format!(
                    "最新年度は開業率({:.1}%)が廃業率({:.1}%)を上回る傾向がみられ、新規参入が相対的に多い可能性があります（HW参考: 統計の業種範囲は地域全産業）。",
                    latest_open, latest_close
                )
            } else if net < -0.5 {
                format!("最新年度は廃業率({:.1}%)が開業率({:.1}%)を上回る傾向がみられ、既存企業の採用枠確保に留意が必要な可能性があります。", latest_close, latest_open)
            } else {
                format!(
                    "開業率{:.1}% / 廃業率{:.1}%でほぼ均衡している傾向がみられます。",
                    latest_open, latest_close
                )
            };
            write!(
                html,
                r#"<div class="chart-interp">{}</div>"#,
                escape_html(&interp)
            )
            .unwrap();
            html.push_str("</div>");
        }
    }

    // --- 3. 人口移動（転入・転出・社会増減） ---
    if let Some(row) = ctx.ext_migration.first() {
        let inflow = get_i64(row, "inflow");
        let outflow = get_i64(row, "outflow");
        let net = get_i64(row, "net_migration");
        let net_rate = get_f64(row, "net_migration_rate");

        if inflow > 0 || outflow > 0 {
            html.push_str(r#"<div class="chart-box no-break"><h3>人口移動（転入・転出）</h3>"#);
            html.push_str(r#"<div class="kpi-grid">"#);
            report_kpi(
                html,
                "転入数",
                &format!("{}人", format_number(inflow)),
                "#059669",
                "他地域から流入",
            );
            report_kpi(
                html,
                "転出数",
                &format!("{}人", format_number(outflow)),
                "#dc2626",
                "他地域へ流出",
            );
            let (net_color, net_label) = if net > 0 {
                ("#059669", "転入超過")
            } else if net < 0 {
                ("#dc2626", "転出超過")
            } else {
                ("#666", "均衡")
            };
            report_kpi(
                html,
                "社会増減",
                &format!("{:+}人", net),
                net_color,
                net_label,
            );
            if net_rate.abs() > 0.01 {
                report_kpi(
                    html,
                    "社会増減率",
                    &format!("{:+.2}‰", net_rate),
                    net_color,
                    "千人当たり",
                );
            }
            html.push_str("</div>");
            let interp = if net > 0 {
                "転入超過 = 労働力の外部流入があり、採用の供給源として期待できる。".to_string()
            } else if net < 0 {
                "転出超過 = 若年層流出の可能性。地域内採用が難化する構造要因。".to_string()
            } else {
                "人口移動は均衡。地域内の労働力供給が中心。".to_string()
            };
            write!(
                html,
                r#"<div class="chart-interp">{}</div>"#,
                escape_html(&interp)
            )
            .unwrap();
            html.push_str("</div>");
        }
    }

    // --- 4. 読み方ガイド ---
    html.push_str(r#"<div class="guide-grid">
<div class="guide-item"><strong>事業所数</strong>この地域に立地する事業所の数。多い=ビジネス集積地=採用競合も多い。</div>
<div class="guide-item"><strong>開業率</strong>新設事業所の登場率。高い=成長市場、新規求人の創出余地あり。</div>
<div class="guide-item"><strong>廃業率</strong>事業所の退出率。高い=競争激化または市場縮小の兆候。</div>
<div class="guide-item"><strong>社会増減</strong>転入-転出の差。プラス=人口流入地域、マイナス=流出地域。</div>
</div>"#);

    html.push_str("</section>");
}

/// セクションB: 労働力の将来リスク（Page 3の前に挿入）
/// - 介護需要（施設数 vs 高齢者数）
/// - 気象条件（降雪日数・日照時間）
/// - 家計支出（カテゴリ別構成比）
fn render_labor_future_risk_section(html: &mut String, ctx: &InsightContext) {
    if ctx.ext_care_demand.is_empty()
        && ctx.ext_climate.is_empty()
        && ctx.ext_household_spending.is_empty()
    {
        return;
    }

    html.push_str("<section class=\"report-page\">");
    html.push_str("<div class=\"section-title\">労働力の将来リスク</div>");
    html.push_str("<div class=\"section-question\">将来の労働需要・地域特性・生活コストにどんなリスクが潜むか?</div>");

    // --- 1. 介護需要（最新年度のスナップショット） ---
    if let Some(row) = ctx.ext_care_demand.last() {
        let nursing_home = get_i64(row, "nursing_home_count");
        let health_facility = get_i64(row, "health_facility_count");
        let home_care = get_i64(row, "home_care_offices");
        let day_service = get_i64(row, "day_service_offices");
        let pop_65 = get_i64(row, "pop_65_over");
        let pop_75 = get_i64(row, "pop_75_over");
        let rate_65 = get_f64(row, "pop_65_over_rate");
        let year = get_str_ref(row, "fiscal_year");

        let total_facilities = nursing_home + health_facility + home_care + day_service;

        if total_facilities > 0 || pop_65 > 0 {
            html.push_str(r#"<div class="chart-box no-break">"#);
            write!(html, r#"<h3>介護需要（{}年度）</h3>"#, escape_html(year)).unwrap();
            html.push_str(r#"<div class="two-col">"#);

            // 左: 施設数
            html.push_str(r#"<div>"#);
            if nursing_home > 0 {
                report_metric(
                    html,
                    "特養（特別養護老人ホーム）",
                    &format!("{}施設", format_number(nursing_home)),
                );
            }
            if health_facility > 0 {
                report_metric(
                    html,
                    "老健（介護老人保健施設）",
                    &format!("{}施設", format_number(health_facility)),
                );
            }
            if home_care > 0 {
                report_metric(
                    html,
                    "訪問介護事業所",
                    &format!("{}事業所", format_number(home_care)),
                );
            }
            if day_service > 0 {
                report_metric(
                    html,
                    "通所介護事業所",
                    &format!("{}事業所", format_number(day_service)),
                );
            }
            html.push_str(r#"</div>"#);

            // 右: 高齢者人口
            html.push_str(r#"<div>"#);
            if pop_65 > 0 {
                report_metric(
                    html,
                    "65歳以上人口",
                    &format!("{}人", format_number(pop_65)),
                );
            }
            if pop_75 > 0 {
                report_metric(
                    html,
                    "75歳以上人口",
                    &format!("{}人", format_number(pop_75)),
                );
            }
            if rate_65 > 0.0 {
                report_metric(html, "高齢化率", &format!("{:.1}%", rate_65));
            }
            // 施設あたり高齢者数
            if total_facilities > 0 && pop_65 > 0 {
                let per_facility = pop_65 / total_facilities;
                report_metric(
                    html,
                    "施設1件あたり65歳以上",
                    &format!("{}人", format_number(per_facility)),
                );
            }
            html.push_str(r#"</div>"#);

            html.push_str("</div>"); // two-col

            // feedback_correlation_not_causation.md 準拠:
            // 「拡大基調」「漸増の見込み」等の将来断定を避け、傾向観測 + 可能性表現に修正。
            let interp = if rate_65 >= 30.0 {
                "高齢化率30%超。介護職採用需要が相対的に高い可能性があり、中長期で供給逼迫が続く可能性に留意。"
            } else if rate_65 >= 25.0 {
                "高齢化率25%以上。介護需要が増加傾向となる可能性があり、採用競合との差別化が有効な可能性があります。"
            } else if rate_65 > 0.0 {
                "高齢化率は全国平均水準。介護採用需要は中長期で漸増する可能性があります。"
            } else {
                "介護需要の参考データ。施設数と人口規模から市場規模を把握できます。"
            };
            write!(
                html,
                r#"<div class="chart-interp">{}</div>"#,
                escape_html(interp)
            )
            .unwrap();
            html.push_str("</div>");
        }
    }

    // --- 2. 気象条件（最新年度） ---
    if let Some(row) = ctx.ext_climate.last() {
        let snow_days = get_f64(row, "snow_days");
        let sunshine = get_f64(row, "sunshine_hours");
        let avg_temp = get_f64(row, "avg_temperature");
        let precipitation = get_f64(row, "precipitation");
        let year = get_str_ref(row, "fiscal_year");

        if snow_days > 0.0 || sunshine > 0.0 || avg_temp.abs() > 0.01 {
            html.push_str(r#"<div class="chart-box no-break">"#);
            write!(
                html,
                r#"<h3>気象条件（{}年度・地域特性理解）</h3>"#,
                escape_html(year)
            )
            .unwrap();
            html.push_str(r#"<div class="kpi-grid">"#);
            if avg_temp.abs() > 0.01 {
                report_kpi(
                    html,
                    "年平均気温",
                    &format!("{:.1}℃", avg_temp),
                    "#2874a6",
                    "",
                );
            }
            if snow_days > 0.0 {
                let snow_color = if snow_days > 60.0 {
                    "#dc2626"
                } else if snow_days > 20.0 {
                    "#d97706"
                } else {
                    "#2874a6"
                };
                let snow_sub = if snow_days > 60.0 {
                    "多雪地域"
                } else if snow_days > 20.0 {
                    "一定の積雪"
                } else {
                    "少雪"
                };
                report_kpi(
                    html,
                    "降雪日数",
                    &format!("{:.0}日", snow_days),
                    snow_color,
                    snow_sub,
                );
            }
            if sunshine > 0.0 {
                report_kpi(
                    html,
                    "年間日照時間",
                    &format!("{:.0}h", sunshine),
                    "#f59e0b",
                    "",
                );
            }
            if precipitation > 0.0 {
                report_kpi(
                    html,
                    "年間降水量",
                    &format!("{:.0}mm", precipitation),
                    "#0891b2",
                    "",
                );
            }
            html.push_str("</div>");
            let interp = if snow_days > 60.0 {
                "多雪地域は冬季の通勤負荷が高く、通勤圏が狭くなりやすい。屋内勤務の訴求が有効。"
            } else if snow_days > 20.0 {
                "積雪期がある地域。冬季の通勤条件（除雪、車通勤可否等）が採用訴求ポイント。"
            } else {
                "気候条件による通勤制約は限定的。広域採用が設計しやすい。"
            };
            write!(
                html,
                r#"<div class="chart-interp">{}</div>"#,
                escape_html(interp)
            )
            .unwrap();
            html.push_str("</div>");
        }
    }

    // --- 3. 家計支出（カテゴリ別構成比） ---
    // 注: v2_external_household_spending は「消費支出」(親) と 10 サブカテゴリを両方
    //     持つため、親を含めると pie chart が 50% を占有してしまう。pie はサブカテゴリ
    //     のみで描画する (バグ修正 2026-04-27)。
    if !ctx.ext_household_spending.is_empty() {
        let items: Vec<(String, f64)> = ctx
            .ext_household_spending
            .iter()
            .filter(|r| get_str_ref(r, "category") != "消費支出")
            .map(|r| {
                (
                    get_str_ref(r, "category").to_string(),
                    get_f64(r, "monthly_amount"),
                )
            })
            .filter(|(_, v)| *v > 0.0)
            .take(8)
            .collect();

        if !items.is_empty() {
            let data: Vec<serde_json::Value> = items
                .iter()
                .map(|(name, value)| serde_json::json!({"name": name, "value": *value}))
                .collect();

            let chart = serde_json::json!({
                "tooltip": {"trigger": "item", "formatter": "{b}: {c}円 ({d}%)"},
                "legend": {"orient": "vertical", "right": 10, "top": "center", "textStyle": {"fontSize": 9}},
                "series": [{
                    "type": "pie",
                    "radius": ["40%", "70%"],
                    "center": ["35%", "50%"],
                    "data": data,
                    "label": {"show": true, "fontSize": 9, "formatter": "{b}\n{d}%"},
                    "itemStyle": {"borderRadius": 4, "borderColor": "#fff", "borderWidth": 1}
                }]
            });

            // 「消費支出」(親) を取得して合計表示に使う。無ければサブカテゴリ合計で代用
            let total: f64 = ctx
                .ext_household_spending
                .iter()
                .find(|r| get_str_ref(r, "category") == "消費支出")
                .map(|r| get_f64(r, "monthly_amount"))
                .unwrap_or_else(|| items.iter().map(|(_, v)| *v).sum());

            html.push_str("<div class=\"chart-box no-break\">");
            html.push_str("<h3>家計支出（カテゴリ別月額構成比）</h3>");
            html.push_str(&render_echarts_div(&chart.to_string(), 280));
            write!(html,
                r#"<div class="chart-interp">月額合計: 約{}円 (消費支出)。支出構成は地域の生活コスト水準を示し、賃金設計の基準となる。</div>"#,
                format_number(total as i64)
            ).unwrap();
            html.push_str("</div>");
        }
    }

    // --- 4. 読み方ガイド ---
    html.push_str(r#"<div class="guide-grid">
<div class="guide-item"><strong>高齢化率</strong>65歳以上人口比率。高い=介護需要増、労働力供給減のダブルリスク。</div>
<div class="guide-item"><strong>降雪日数</strong>冬季の通勤負荷の目安。多雪地域は実質通勤圏が縮小する。</div>
<div class="guide-item"><strong>日照時間</strong>気候快適性の指標。地域生活の魅力度の一要素。</div>
<div class="guide-item"><strong>家計支出</strong>地域の生活コスト水準。賃金設定と実質可処分所得の評価に使用。</div>
</div>"#);

    html.push_str("</section>");
}
