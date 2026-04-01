//! 示唆のHTML描画

use super::helpers::*;
use super::super::helpers::escape_html;

// ======== サブタブ描画 ========

/// サブタブ1: 採用構造分析
pub(crate) fn render_subtab_hiring(insights: &[Insight]) -> String {
    let filtered: Vec<_> = insights.iter()
        .filter(|i| i.category == InsightCategory::HiringStructure)
        .collect();
    render_insight_list("採用構造分析", "なぜ採用が難しいのか、構造的な要因を分析します", &filtered)
}

/// サブタブ2: 将来予測
pub(crate) fn render_subtab_forecast(insights: &[Insight]) -> String {
    let filtered: Vec<_> = insights.iter()
        .filter(|i| i.category == InsightCategory::Forecast)
        .collect();
    render_insight_list("将来予測", "時系列データと人口動態から市場の方向性を予測します", &filtered)
}

/// サブタブ3: 地域比較
pub(crate) fn render_subtab_regional(insights: &[Insight]) -> String {
    let filtered: Vec<_> = insights.iter()
        .filter(|i| i.category == InsightCategory::RegionalCompare)
        .collect();
    render_insight_list("地域間比較", "他地域との比較から強み・弱みを可視化します", &filtered)
}

/// サブタブ4: アクション提案
pub(crate) fn render_subtab_action(insights: &[Insight]) -> String {
    let filtered: Vec<_> = insights.iter()
        .filter(|i| i.category == InsightCategory::ActionProposal)
        .collect();
    let mut html = render_insight_list("アクション提案", "データに基づく具体的な改善施策を提案します", &filtered);
    html.push_str(&render_report_download_section());
    html
}

/// 示唆カードのリスト描画
fn render_insight_list(title: &str, description: &str, insights: &[&Insight]) -> String {
    let mut html = String::with_capacity(4_000);

    html.push_str(&format!(
        r#"<div class="space-y-4">
        <div class="mb-4">
            <h3 class="text-lg font-semibold text-white">{title}</h3>
            <p class="text-xs text-slate-500">{description}</p>
        </div>"#
    ));

    if insights.is_empty() {
        html.push_str(r#"<div class="stat-card text-center py-8">
            <p class="text-slate-400 text-sm">該当する示唆はありません</p>
            <p class="text-slate-500 text-xs mt-1">地域を絞り込むとより詳細な分析が表示されます</p>
        </div>"#);
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

    html.push_str(&format!(
        r#"<div class="rounded-lg border p-4 {bg}">
        <div class="flex items-start gap-3">
            <span class="px-2 py-0.5 rounded text-xs font-medium {badge}">{label}</span>
            <div class="flex-1 min-w-0">
                <h4 class="text-sm font-semibold text-white">{title}</h4>
                <p class="text-xs text-slate-300 mt-1">{body}</p>"#,
        title = escape_html(&insight.title),
        body = escape_html(&insight.body),
    ));

    // エビデンス表示
    if !insight.evidence.is_empty() {
        html.push_str(r#"<div class="flex flex-wrap gap-3 mt-2">"#);
        for ev in &insight.evidence {
            let formatted_value = if ev.unit == "円" || ev.unit == "円/月" || ev.unit == "円/人" {
                format!("{:.0}{}", ev.value, ev.unit)
            } else if ev.unit == "%" || ev.unit == "%/月" {
                format!("{:.1}{}", ev.value * 100.0, ev.unit)
            } else if ev.unit == "人" || ev.unit == "件" || ev.unit == "件/千人" || ev.unit == "点" {
                format!("{:.0}{}", ev.value, ev.unit)
            } else {
                format!("{:.2}{}", ev.value, ev.unit)
            };
            html.push_str(&format!(
                r#"<div class="text-xs">
                    <span class="text-slate-500">{metric}:</span>
                    <span class="text-white font-mono">{value}</span>
                    <span class="text-slate-600 ml-1">({context})</span>
                </div>"#,
                metric = escape_html(&ev.metric),
                value = escape_html(&formatted_value),
                context = escape_html(&ev.context),
            ));
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
                "competitive" => "競合",
                "jobmap" => "地図",
                "diagnostic" => "診断",
                "insight" => "総合診断",
                "survey" => "競合調査",
                _ => tab,
            };
            html.push_str(&format!(
                r#"<a class="text-xs text-blue-400/60 hover:text-blue-300 cursor-pointer" onclick="navigateToTab('/tab/{tab}')">{tab_label}→</a>"#
            ));
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
        html.push_str(&format!(
            r#"<div class="flex items-start gap-2 p-2 rounded bg-slate-800/50">
                <span class="px-1.5 py-0.5 rounded text-[10px] font-medium {badge} shrink-0">{label}</span>
                <div class="min-w-0">
                    <p class="text-xs text-white font-medium truncate">{title}</p>
                    <p class="text-[10px] text-slate-400 line-clamp-2">{body}</p>
                </div>
            </div>"#,
            title = escape_html(&insight.title),
            body = escape_html(&insight.body),
        ));
    }

    // 総合診断タブへの誘導
    html.push_str(r#"<a class="text-xs text-blue-400/60 hover:text-blue-300 cursor-pointer block text-right" onclick="navigateToTab('/tab/insight')">総合診断を見る →</a>"#);
    html.push_str("</div>");
    html
}

// ======== 統合レポートHTMLページ（PDF出力用） ========

use super::super::helpers::{format_number, get_f64, get_i64, get_str_ref};
use super::fetch::InsightContext;

/// 完全なHTMLページとしてレポートを生成（/report/insight）
/// 7ページ構成: サマリー/市場/将来/課題/地域/アクション/付録
pub(crate) fn render_insight_report_page(
    insights: &[Insight],
    ctx: &InsightContext,
    pref: &str,
    muni: &str,
) -> String {
    let location = if !muni.is_empty() { format!("{} {}", pref, muni) }
        else if !pref.is_empty() { pref.to_string() }
        else { "全国".to_string() };
    let today = chrono::Local::now().format("%Y年%m月%d日").to_string();

    let critical = insights.iter().filter(|i| i.severity == Severity::Critical).count();
    let warning = insights.iter().filter(|i| i.severity == Severity::Warning).count();
    let info = insights.iter().filter(|i| i.severity == Severity::Info).count();
    let positive = insights.iter().filter(|i| i.severity == Severity::Positive).count();

    let summary = super::report::generate_executive_summary_text(insights);

    let mut html = String::with_capacity(32_000);

    // <!DOCTYPE html>
    html.push_str(r#"<!DOCTYPE html>
<html lang="ja">
<head>
<meta charset="UTF-8">
<title>総合診断レポート</title>
<style>
@page { size: A4 landscape; margin: 12mm 10mm 15mm 10mm; }
* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: "Yu Gothic","Meiryo","Hiragino Sans",sans-serif; font-size: 11px; color: #333; background: #fff; padding: 20px; }
h1 { font-size: 20px; color: #1a5276; border-bottom: 3px solid #1a5276; padding-bottom: 8px; margin-bottom: 6px; }
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
.kpi-card { border: 1px solid #ddd; border-radius: 6px; padding: 12px; text-align: center; }
.kpi-value { font-size: 18px; font-weight: bold; }
.kpi-label { font-size: 9px; color: #666; margin-top: 3px; }
.section-title { font-size: 15px; color: #1a5276; margin: 16px 0 8px 0; border-left: 4px solid #1a5276; padding-left: 8px; }
.section-question { font-size: 10px; color: #666; font-style: italic; margin-bottom: 10px; }
.narrative { background: #f8f9fa; border-left: 3px solid #1a5276; padding: 10px 14px; margin-bottom: 12px; font-size: 11px; line-height: 1.6; color: #444; }
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
    .print-btn { display: none; }
    .no-break { page-break-inside: avoid; break-inside: avoid; }
    body { padding: 0; }
    .report-page { min-height: auto; }
}
.metric-row { display: flex; justify-content: space-between; padding: 6px 0; border-bottom: 1px solid #f1f5f9; }
.metric-label { color: #64748b; font-size: 10px; }
.metric-value { font-weight: bold; font-size: 11px; }
</style>
<script src="https://cdn.jsdelivr.net/npm/echarts@5/dist/echarts.min.js"></script>
</head>
<body>
<button class="print-btn" onclick="window.print()">印刷 / PDF保存</button>
"#);

    // 採用困難度グレード算出
    let vacancy_rate = ctx.vacancy.iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")
        .map(|r| get_f64(r, "vacancy_rate"))
        .unwrap_or(0.0);
    let grade = super::report::compute_difficulty_grade(insights, vacancy_rate);
    let top_findings = super::report::extract_top_findings(insights);

    // ===== Page 1: エグゼクティブサマリー =====
    html.push_str("<section class=\"report-page\">");
    html.push_str(&format!(
        r#"<h1>ハローワーク求人市場 総合診断レポート</h1>
        <div class="subtitle">{location} | {today}</div>"#
    ));

    // 採用困難度グレード（大きく表示）
    html.push_str(&format!(
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
    ));

    // Top3発見
    if !top_findings.is_empty() {
        html.push_str("<h2>主な発見</h2><ol class=\"findings-list\">");
        for f in &top_findings {
            html.push_str(&format!("<li>{}</li>", escape_html(f)));
        }
        html.push_str("</ol>");
    }

    // 推奨アクションTop3
    let actions: Vec<_> = insights.iter()
        .filter(|i| i.category == InsightCategory::ActionProposal)
        .take(3)
        .collect();
    if !actions.is_empty() {
        html.push_str("<h2>推奨アクション</h2><ol class=\"findings-list\">");
        for a in &actions {
            let so_what = super::report::generate_so_what(a);
            html.push_str(&format!("<li><strong>{}</strong> {}</li>",
                escape_html(&a.title), escape_html(&so_what)));
        }
        html.push_str("</ol>");
    }

    // KPIカード
    html.push_str(r#"<div class="kpi-grid">"#);

    // 求人数
    let total_postings: i64 = ctx.vacancy.iter()
        .map(|r| super::super::helpers::get_i64(r, "total_count"))
        .sum();
    report_kpi(&mut html, "総求人数", &format_number(total_postings), "#2563eb");

    // 欠員率
    let vacancy_rate = ctx.vacancy.iter()
        .find(|r| super::super::helpers::get_str_ref(r, "emp_group") == "正社員")
        .map(|r| super::super::helpers::get_f64(r, "vacancy_rate"))
        .unwrap_or(0.0);
    let vr_color = if vacancy_rate > 0.3 { "#dc2626" } else if vacancy_rate > 0.2 { "#d97706" } else { "#059669" };
    report_kpi(&mut html, "欠員率(正社員)", &format!("{:.1}%", vacancy_rate * 100.0), vr_color);

    // 平均月給
    let avg_salary = ctx.cascade.iter()
        .find(|r| super::super::helpers::get_str_ref(r, "emp_group") == "正社員")
        .map(|r| super::super::helpers::get_f64(r, "avg_salary_min") as i64)
        .unwrap_or(0);
    if avg_salary > 0 {
        report_kpi(&mut html, "平均月給", &format!("{}円", format_number(avg_salary)), "#333");
    } else {
        report_kpi(&mut html, "平均月給", "-", "#999");
    }

    // 通勤圏人口
    if ctx.commute_zone_total_pop > 0 {
        report_kpi(&mut html, "通勤圏人口", &format!("{}人", format_number(ctx.commute_zone_total_pop)), "#0891b2");
    } else {
        report_kpi(&mut html, "通勤圏人口", "-", "#999");
    }

    // 通勤流入
    if ctx.commute_inflow_total > 0 {
        report_kpi(&mut html, "通勤流入数", &format!("{}人", format_number(ctx.commute_inflow_total)), "#7c3aed");
    } else {
        report_kpi(&mut html, "通勤流入数", "-", "#999");
    }
    html.push_str("</div>");

    // 示唆サマリーバッジ
    html.push_str(r#"<div class="badge-row">"#);
    if critical > 0 { html.push_str(&format!(r#"<span class="badge badge-critical">重大 {}件</span>"#, critical)); }
    if warning > 0 { html.push_str(&format!(r#"<span class="badge badge-warning">注意 {}件</span>"#, warning)); }
    if info > 0 { html.push_str(&format!(r#"<span class="badge badge-info">情報 {}件</span>"#, info)); }
    if positive > 0 { html.push_str(&format!(r#"<span class="badge badge-positive">良好 {}件</span>"#, positive)); }
    html.push_str("</div>");

    html.push_str("</section>"); // End Page 1

    // ===== Page 2: 市場概況 =====
    html.push_str("<section class=\"report-page\">");
    html.push_str("<div class=\"section-title\">市場概況</div>");
    html.push_str("<div class=\"section-question\">この地域の求人市場は今どういう状態か?</div>");

    // 通勤フローテーブル
    if !ctx.commute_inflow_top3.is_empty() {
        html.push_str(r#"<h2>通勤フロー（国勢調査実データ）</h2>"#);
        html.push_str(r#"<table class="flow-table"><thead><tr><th>流入元</th><th>通勤者数</th></tr></thead><tbody>"#);
        for (p, m, c) in &ctx.commute_inflow_top3 {
            html.push_str(&format!(
                "<tr><td>{}{}</td><td style=\"text-align:right\">{}</td></tr>",
                escape_html(p), escape_html(m), format_number(*c)
            ));
        }
        html.push_str("</tbody></table>");
        if ctx.commute_self_rate > 0.0 {
            html.push_str(&format!(
                r#"<div style="font-size:9px;color:#888;margin-top:4px">地元就業率: {:.1}%</div>"#,
                ctx.commute_self_rate * 100.0
            ));
        }
    }

    html.push_str("</section>"); // End Page 2

    // ===== Page 3: チャート + 将来予測 =====
    html.push_str("<section class=\"report-page\">");
    html.push_str("<div class=\"section-title\">市場構造と将来予測</div>");
    html.push_str("<div class=\"section-question\">人材はこの先どうなるか?</div>");

    // 給与帯分布（cascadeテーブルから）
    let salary_data: Vec<(String, i64)> = ctx.cascade.iter()
        .filter(|r| get_str_ref(r, "emp_group") == "正社員")
        .map(|r| (escape_html(&get_str_ref(r, "industry_raw").to_string()), get_i64(r, "posting_count")))
        .filter(|(_,c)| *c > 0)
        .take(10)
        .collect();

    if !salary_data.is_empty() {
        let labels: Vec<String> = salary_data.iter().map(|(l,_)| format!("\"{}\"", l)).collect();
        let values: Vec<String> = salary_data.iter().map(|(_,v)| v.to_string()).collect();
        let chart_json = format!(
            "{{\"tooltip\":{{\"trigger\":\"axis\"}},\"xAxis\":{{\"type\":\"category\",\"data\":[{}],\"axisLabel\":{{\"rotate\":30,\"fontSize\":9}}}},\"yAxis\":{{\"type\":\"value\"}},\"series\":[{{\"type\":\"bar\",\"data\":[{}],\"itemStyle\":{{\"color\":\"#3b82f6\"}}}}]}}",
            labels.join(","), values.join(",")
        );
        let interp = super::report::interpret_industry_chart(&salary_data);
        html.push_str("<div class=\"chart-box no-break\">");
        html.push_str("<h3>産業別求人数（正社員 Top10）</h3>");
        html.push_str(&format!("<div class=\"report-chart\" style=\"width:100%;height:280px;\" data-chart-config='{}'></div>", chart_json));
        if !interp.is_empty() {
            html.push_str(&format!("<div class=\"chart-interp\">{}</div>", escape_html(&interp)));
        }
        html.push_str("</div>");
    }

    // 蝶形ピラミッド（通勤圏 vs 選択市区町村）
    if !ctx.ext_pyramid.is_empty() {
        // ピラミッドデータ構築
        let ages: Vec<String> = ctx.ext_pyramid.iter()
            .map(|r| get_str_ref(r, "age_group").to_string())
            .collect();
        let local_male: Vec<String> = ctx.ext_pyramid.iter()
            .map(|r| format!("{}", -get_i64(r, "male_count")))
            .collect();
        let local_female: Vec<String> = ctx.ext_pyramid.iter()
            .map(|r| format!("{}", get_i64(r, "female_count")))
            .collect();
        let ages_json: Vec<String> = ages.iter().map(|a| format!("\"{}\"", a)).collect();

        let mut pyramid_json = String::with_capacity(1_000);
        pyramid_json.push_str("{\"tooltip\":{\"trigger\":\"axis\",\"axisPointer\":{\"type\":\"shadow\"}},");
        pyramid_json.push_str("\"legend\":{\"data\":[\"男性\",\"女性\"],\"textStyle\":{\"color\":\"#333\",\"fontSize\":10},\"bottom\":0},");
        pyramid_json.push_str("\"grid\":{\"left\":\"3%\",\"right\":\"3%\",\"top\":\"3%\",\"bottom\":\"12%\",\"containLabel\":true},");
        pyramid_json.push_str("\"xAxis\":{\"type\":\"value\"},");
        pyramid_json.push_str(&format!("\"yAxis\":{{\"type\":\"category\",\"data\":[{}],\"axisTick\":{{\"show\":false}}}},", ages_json.join(",")));
        pyramid_json.push_str(&format!("\"series\":[{{\"name\":\"男性\",\"type\":\"bar\",\"data\":[{}],\"itemStyle\":{{\"color\":\"#3b82f6\"}}}},", local_male.join(",")));
        pyramid_json.push_str(&format!("{{\"name\":\"女性\",\"type\":\"bar\",\"data\":[{}],\"itemStyle\":{{\"color\":\"#ec4899\"}}}}]", local_female.join(",")));
        pyramid_json.push_str("}");

        html.push_str("<div class=\"chart-box no-break\" style=\"margin-top:16px\">");
        html.push_str("<h3>人口ピラミッド（性別×年齢）</h3>");
        html.push_str(&format!("<div class=\"report-chart\" style=\"width:100%;height:400px;\" data-chart-config='{}'></div>", pyramid_json));
        let pyramid_interp = super::report::interpret_pyramid(&ctx.ext_pyramid);
        if !pyramid_interp.is_empty() {
            html.push_str(&format!("<div class=\"chart-interp\">{}</div>", escape_html(&pyramid_interp)));
        }
        html.push_str("<div style=\"font-size:8px;color:#999;margin-top:4px\">※外部統計データ(SSDSE-A)</div>");
        html.push_str("</div>");
    }

    // 通勤フロー（サンキー）
    html.push_str("</section>"); // End Page 3

    if ctx.commute_inflow_total > 0 {
        // ===== Page 4: 通勤フロー =====
        html.push_str("<section class=\"report-page\">");
        html.push_str("<div class=\"section-title\">通勤フロー分析</div>");
        html.push_str("<div class=\"section-question\">実際に誰がどこから通勤しているか?</div>");

        // 流入テーブル（詳細版）
        html.push_str(r#"<div class="two-col">"#);

        // 流入側
        html.push_str(r#"<div class="chart-box"><h3>通勤流入 Top10（この地域に通勤してくる人）</h3>"#);
        html.push_str(r#"<table class="flow-table"><thead><tr><th>流入元</th><th>通勤者数</th><th>男性</th><th>女性</th></tr></thead><tbody>"#);
        for (p, m, c) in &ctx.commute_inflow_top3 {
            html.push_str(&format!(
                "<tr><td>{}{}</td><td style=\"text-align:right\">{}</td><td></td><td></td></tr>",
                escape_html(p), escape_html(m), format_number(*c)
            ));
        }
        html.push_str("</tbody></table></div>");

        // 地元就業率+通勤圏メトリクス
        html.push_str(r#"<div class="chart-box"><h3>通勤圏の特徴</h3>"#);
        if ctx.commute_self_rate > 0.0 {
            report_metric(&mut html, "地元就業率", &format!("{:.1}%", ctx.commute_self_rate * 100.0));
        }
        report_metric(&mut html, "通勤流入総数", &format!("{}人", format_number(ctx.commute_inflow_total)));
        report_metric(&mut html, "通勤流出総数", &format!("{}人", format_number(ctx.commute_outflow_total)));
        if ctx.commute_zone_count > 0 {
            report_metric(&mut html, "30km圏市区町村数", &format!("{}件 / {}県", ctx.commute_zone_count, ctx.commute_zone_pref_count));
        }

        // 昼夜間人口比
        if let Some(row) = ctx.ext_daytime_pop.first() {
            let ratio = get_f64(row, "daytime_ratio");
            if ratio > 0.0 {
                let label = if ratio > 1.05 { "都市型（通勤流入）" } else if ratio < 0.95 { "ベッドタウン型" } else { "均衡型" };
                report_metric(&mut html, "昼夜間人口比", &format!("{:.2} ({})", ratio, label));
            }
        }
        html.push_str("</div>");
        html.push_str("</div>"); // two-col close
    }

    if ctx.commute_inflow_total > 0 {
        html.push_str("</section>"); // End Page 4 (commute)
    }

    // ===== Page 5-6: 示唆詳細 =====
    html.push_str("<section class=\"report-page\">");

    // 示唆一覧（4章構成）
    let categories = [
        (InsightCategory::HiringStructure, "第1章: 採用構造分析"),
        (InsightCategory::Forecast, "第2章: 将来予測"),
        (InsightCategory::RegionalCompare, "第3章: 地域比較"),
        (InsightCategory::ActionProposal, "第4章: 推奨アクション"),
    ];

    let chapter_questions = [
        "この地域で採用が難しい構造的な原因は何か?",
        "市場はこの先どうなるか? 人材は確保できるか?",
        "他の地域と比べて優位か劣位か?",
        "具体的に何をすべきか?",
    ];

    for (idx, (cat, title)) in categories.iter().enumerate() {
        let filtered: Vec<_> = insights.iter().filter(|i| &i.category == cat).collect();
        if filtered.is_empty() { continue; }

        html.push_str(&format!(r#"<h2>{title}</h2>"#));
        html.push_str(&format!("<div class=\"section-question\">{}</div>", chapter_questions[idx]));

        // ナラティブ（章の概要テキスト）
        if let Some(top) = filtered.first() {
            let narrative = if filtered.len() == 1 {
                format!("この章では1件の分析結果を報告します。最も重要な点は「{}」です。", top.title)
            } else {
                let critical_count = filtered.iter().filter(|i| i.severity == Severity::Critical || i.severity == Severity::Warning).count();
                if critical_count > 0 {
                    format!("{}件の分析結果のうち、注意が必要な項目が{}件あります。最優先は「{}」です。",
                        filtered.len(), critical_count, top.title)
                } else {
                    format!("{}件の分析結果があります。全体的に良好な状態です。", filtered.len())
                }
            };
            html.push_str(&format!("<div class=\"narrative\">{}</div>", escape_html(&narrative)));
        }
        for insight in &filtered {
            let cls = match insight.severity {
                Severity::Critical => "critical",
                Severity::Warning => "warning",
                Severity::Info => "info",
                Severity::Positive => "positive",
            };
            html.push_str(&format!(
                r#"<div class="insight-card {cls} no-break">
                    <div class="insight-title">[{}] {}</div>
                    <div class="insight-body">{}</div>"#,
                escape_html(insight.severity.label()),
                escape_html(&insight.title),
                escape_html(&insight.body),
            ));
            if !insight.evidence.is_empty() {
                let ev_text: Vec<String> = insight.evidence.iter()
                    .map(|e| format!("{}: {}", e.metric, e.context))
                    .collect();
                html.push_str(&format!(r#"<div class="evidence">{}</div>"#, escape_html(&ev_text.join(" | "))));
            }
            // So What? テキスト
            let so_what = super::report::generate_so_what(insight);
            if !so_what.is_empty() {
                html.push_str(&format!(r#"<div class="so-what">{}</div>"#, escape_html(&so_what)));
            }
            html.push_str("</div>");
        }
    }

    html.push_str("</section>"); // End insight pages

    // ===== 最終ページ: 注記 =====
    html.push_str("<section class=\"report-page\">");
    html.push_str(r#"<div class="notes">
        <strong>データソースと注意事項</strong><br>
        ・ハローワーク掲載求人に基づく分析です。IT・通信等のHW掲載が少ない産業は参考値としてご利用ください。<br>
        ・通勤フローは2020年国勢調査データに基づきます。<br>
        ・充足グレードはMLモデルによる推計値です。
    </div>"#);

    html.push_str("</section>"); // End notes page

    // ECharts初期化スクリプト
    html.push_str(r#"<script>
document.addEventListener('DOMContentLoaded', function() {
    document.querySelectorAll('.report-chart[data-chart-config]').forEach(function(el) {
        try {
            var config = JSON.parse(el.getAttribute('data-chart-config'));
            config.animation = false;
            config.backgroundColor = '#ffffff';
            var chart = echarts.init(el, null);
            chart.setOption(config);
        } catch(e) { console.warn('Chart init error:', e); }
    });
});
</script>"#);

    html.push_str("</body></html>");
    html
}

fn report_metric(html: &mut String, label: &str, value: &str) {
    html.push_str(&format!(
        r#"<div class="metric-row"><span class="metric-label">{label}</span><span class="metric-value">{value}</span></div>"#
    ));
}

fn report_kpi(html: &mut String, label: &str, value: &str, color: &str) {
    html.push_str(&format!(
        r#"<div class="kpi-card no-break">
            <div class="kpi-value" style="color:{color}">{value}</div>
            <div class="kpi-label">{label}</div>
        </div>"#
    ));
}
