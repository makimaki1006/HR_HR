//! CSV × HW × 外部統計 統合レポート描画

use super::super::company::fetch::NearbyCompany;
use super::super::helpers::{escape_html, format_number, get_f64, get_str_ref};
use super::super::insight::fetch::InsightContext;
use super::super::insight::helpers::{Insight, Severity};

/// 統合レポートHTML生成
pub(crate) fn render_integration(
    pref: &str,
    muni: &str,
    insights: &[Insight],
    ctx: &InsightContext,
    companies: &[NearbyCompany],
) -> String {
    let mut html = String::with_capacity(12_000);
    let location = if !muni.is_empty() { format!("{} {}", pref, muni) } else { pref.to_string() };

    html.push_str(&format!(
        r#"<div class="space-y-4 mt-4">
        <h3 class="text-lg font-bold text-white">🔗 統合分析: <span class="text-blue-400">{}</span></h3>
        <p class="text-xs text-slate-500">HW求人データ・外部統計データ・企業データとの統合分析結果</p>"#,
        escape_html(&location)
    ));

    // HWデータセクション
    html.push_str(&render_hw_section(ctx));

    // 外部統計セクション
    html.push_str(&render_external_section(ctx));

    // 該当地域の企業データセクション
    html.push_str(&render_companies_section(companies, &location));

    // insight示唆セクション
    html.push_str(&render_insights_section(insights));

    html.push_str("</div>");
    html
}

/// HW求人データセクション
fn render_hw_section(ctx: &InsightContext) -> String {
    let mut html = String::with_capacity(2_000);
    html.push_str(r#"<div class="stat-card"><h4 class="text-sm text-slate-400 mb-3">📋 ハローワーク求人市場</h4>"#);

    if ctx.vacancy.is_empty() && ctx.cascade.is_empty() {
        html.push_str(r#"<p class="text-slate-500 text-xs">この地域のHWデータはありません</p>"#);
    } else {
        html.push_str(r#"<div class="grid grid-cols-2 md:grid-cols-4 gap-3">"#);
        // 正社員の欠員率
        if let Some(row) = ctx.vacancy.iter().find(|r| get_str_ref(r, "emp_group") == "正社員") {
            let total = get_f64(row, "total_count") as i64;
            let vacancy_rate = get_f64(row, "vacancy_rate");
            kpi_card(&mut html, "HW求人数", &format_number(total), "text-blue-400");
            let vr_color = if vacancy_rate > 0.3 { "text-red-400" } else if vacancy_rate > 0.2 { "text-amber-400" } else { "text-green-400" };
            kpi_card(&mut html, "欠員率(正社員)", &format!("{:.1}%", vacancy_rate * 100.0), vr_color);
        }
        // カスケードから給与
        if let Some(row) = ctx.cascade.iter().find(|r| get_str_ref(r, "emp_group") == "正社員") {
            let avg_sal = get_f64(row, "avg_salary_min") as i64;
            let holidays = get_f64(row, "avg_annual_holidays") as i64;
            if avg_sal > 0 { kpi_card(&mut html, "HW平均月給", &format!("{}円", format_number(avg_sal)), "text-white"); }
            if holidays > 0 { kpi_card(&mut html, "HW平均休日", &format!("{}日", holidays), "text-white"); }
        }
        html.push_str("</div>");

        // 充足トレンド
        if !ctx.ts_fulfillment.is_empty() {
            if let Some(last) = ctx.ts_fulfillment.last() {
                let days = get_f64(last, "avg_listing_days");
                if days > 0.0 {
                    html.push_str(&format!(
                        r#"<div class="text-xs text-slate-500 mt-2">平均掲載日数: {:.0}日</div>"#, days
                    ));
                }
            }
        }
    }
    html.push_str(r#"<div class="text-xs text-slate-600 mt-2">※HW掲載求人に基づく分析。IT・通信等のHW掲載が少ない産業は参考値。</div>"#);
    html.push_str("</div>");
    html
}

/// 外部統計セクション
fn render_external_section(ctx: &InsightContext) -> String {
    let mut html = String::with_capacity(2_000);
    html.push_str(r#"<div class="stat-card"><h4 class="text-sm text-slate-400 mb-3">🏙️ 地域特性（外部統計）</h4>"#);

    let mut has_data = false;
    html.push_str(r#"<div class="grid grid-cols-2 md:grid-cols-3 gap-3">"#);

    // 人口
    if let Some(row) = ctx.ext_population.first() {
        let pop = get_f64(row, "total_population") as i64;
        if pop > 0 {
            kpi_card(&mut html, "総人口", &format!("{}人", format_number(pop)), "text-white");
            has_data = true;
        }
    }

    // 昼夜間人口比
    if let Some(row) = ctx.ext_daytime_pop.first() {
        let ratio = get_f64(row, "daytime_ratio");
        if ratio > 0.0 {
            let label = if ratio > 1.05 { "都市型（通勤流入）" } else if ratio < 0.95 { "ベッドタウン型（通勤流出）" } else { "均衡型" };
            kpi_card(&mut html, &format!("昼夜間人口比 ({})", label), &format!("{:.2}", ratio), "text-cyan-400");
            has_data = true;
        }
    }

    // 転入転出
    if let Some(row) = ctx.ext_migration.first() {
        let in_m = get_f64(row, "in_migration") as i64;
        let out_m = get_f64(row, "out_migration") as i64;
        let net = in_m - out_m;
        let color = if net > 0 { "text-green-400" } else { "text-red-400" };
        kpi_card(&mut html, "純移動数", &format!("{:+}人", net), color);
        has_data = true;
    }

    // 有効求人倍率
    if let Some(row) = ctx.ext_job_ratio.last() {
        let ratio = get_f64(row, "ratio_total");
        if ratio > 0.0 {
            let color = if ratio > 1.5 { "text-red-400" } else if ratio > 1.0 { "text-amber-400" } else { "text-green-400" };
            kpi_card(&mut html, "有効求人倍率", &format!("{:.2}倍", ratio), color);
            has_data = true;
        }
    }

    // 離職率
    if let Some(row) = ctx.ext_turnover.last() {
        let sep = get_f64(row, "separation_rate");
        if sep > 0.0 {
            kpi_card(&mut html, "離職率", &format!("{:.1}%", sep), "text-white");
            has_data = true;
        }
    }

    html.push_str("</div>");

    if !has_data {
        html.push_str(r#"<p class="text-slate-500 text-xs">この地域の外部統計データはありません</p>"#);
    } else {
        html.push_str(r#"<div class="text-xs text-slate-600 mt-2">※外部統計データ（e-Stat / SSDSE-A）</div>"#);
    }

    html.push_str("</div>");
    html
}

/// insight示唆セクション
fn render_insights_section(insights: &[Insight]) -> String {
    if insights.is_empty() {
        return String::new();
    }

    let mut html = String::with_capacity(2_000);
    html.push_str(r#"<div class="stat-card"><h4 class="text-sm text-slate-400 mb-3">💡 自動診断結果</h4>"#);

    let critical = insights.iter().filter(|i| i.severity == Severity::Critical).count();
    let warning = insights.iter().filter(|i| i.severity == Severity::Warning).count();
    let positive = insights.iter().filter(|i| i.severity == Severity::Positive).count();

    // サマリー
    let assessment = if critical >= 2 { "深刻な課題あり" }
        else if critical >= 1 || warning >= 3 { "注意が必要" }
        else if positive >= 2 { "比較的良好" }
        else { "標準的" };
    html.push_str(&format!(
        r#"<div class="text-sm text-white mb-3">総合評価: <span class="font-bold">{}</span> (重大{}件 / 注意{}件 / 良好{}件)</div>"#,
        assessment, critical, warning, positive
    ));

    // 上位5件の示唆をカード表示
    for insight in insights.iter().take(5) {
        let badge = insight.severity.badge_class();
        let label = insight.severity.label();
        html.push_str(&format!(
            r#"<div class="flex items-start gap-2 p-2 rounded bg-slate-800/50 mb-2">
                <span class="px-1.5 py-0.5 rounded text-[10px] font-medium {badge} shrink-0">{label}</span>
                <div class="min-w-0">
                    <p class="text-xs text-white font-medium">{}</p>
                    <p class="text-[10px] text-slate-400 line-clamp-2">{}</p>
                </div>
            </div>"#,
            escape_html(&insight.title),
            escape_html(&insight.body),
        ));
    }

    html.push_str("</div>");
    html
}

/// 該当地域の企業セクション
fn render_companies_section(companies: &[NearbyCompany], location: &str) -> String {
    let mut html = String::with_capacity(3_000);
    html.push_str(r#"<div class="stat-card"><h4 class="text-sm text-slate-400 mb-3">🏢 該当地域の企業データ</h4>"#);

    if companies.is_empty() {
        html.push_str(&format!(
            r#"<p class="text-slate-500 text-xs">{}に該当する企業データはありません</p>"#,
            escape_html(location)
        ));
        html.push_str("</div>");
        return html;
    }

    // サマリー
    let total = companies.len();
    let with_hw = companies.iter().filter(|c| c.hw_posting_count > 0).count();
    let industries: Vec<&str> = {
        let mut inds: Vec<&str> = companies.iter()
            .map(|c| c.sn_industry.as_str())
            .filter(|s| !s.is_empty())
            .collect();
        inds.sort();
        inds.dedup();
        inds.truncate(5);
        inds
    };
    let ind_text = if industries.is_empty() {
        String::new()
    } else {
        format!(" / {}", industries.join("・"))
    };

    html.push_str(&format!(
        r#"<div class="text-xs text-slate-300 mb-3">{}の企業 <span class="text-white font-bold">{}社</span>{}（うちHW求人あり: {}社）</div>"#,
        escape_html(location), total, escape_html(&ind_text), with_hw
    ));

    // テーブル
    html.push_str(
        r#"<div class="overflow-x-auto"><table class="w-full text-xs">
        <thead><tr class="text-slate-400 border-b border-slate-700">
            <th class="text-left py-1.5 px-2">企業名</th>
            <th class="text-left py-1.5 px-2">業種</th>
            <th class="text-right py-1.5 px-2">従業員数</th>
            <th class="text-right py-1.5 px-2">信用スコア</th>
            <th class="text-right py-1.5 px-2">HW求人</th>
            <th class="text-center py-1.5 px-2">詳細</th>
        </tr></thead><tbody>"#
    );

    for c in companies.iter().take(30) {
        let score_color = if c.credit_score >= 70.0 {
            "text-green-400"
        } else if c.credit_score >= 50.0 {
            "text-amber-400"
        } else if c.credit_score > 0.0 {
            "text-red-400"
        } else {
            "text-slate-500"
        };

        let score_text = if c.credit_score > 0.0 {
            format!("{:.0}", c.credit_score)
        } else {
            "-".to_string()
        };

        let hw_badge = if c.hw_posting_count > 0 {
            format!(r#"<span class="text-blue-400 font-medium">{}</span>"#, c.hw_posting_count)
        } else {
            r#"<span class="text-slate-600">-</span>"#.to_string()
        };

        let emp_text = if c.employee_count > 0 {
            format_number(c.employee_count)
        } else {
            "-".to_string()
        };

        html.push_str(&format!(
            r##"<tr class="border-b border-slate-800 hover:bg-slate-800/50">
                <td class="py-1.5 px-2 text-white">{}</td>
                <td class="py-1.5 px-2 text-slate-400">{}</td>
                <td class="py-1.5 px-2 text-right text-slate-300">{}</td>
                <td class="py-1.5 px-2 text-right {}">{}
                </td>
                <td class="py-1.5 px-2 text-right">{}</td>
                <td class="py-1.5 px-2 text-center">
                    <button class="text-blue-400 hover:text-blue-300 text-[10px]"
                        hx-get="/api/company/profile/{}" hx-target="#content" hx-swap="innerHTML">
                        詳細
                    </button>
                </td>
            </tr>"##,
            escape_html(&c.company_name),
            escape_html(&c.sn_industry),
            emp_text,
            score_color, score_text,
            hw_badge,
            escape_html(&c.corporate_number),
        ));
    }

    html.push_str("</tbody></table></div>");
    html.push_str(r#"<div class="text-xs text-slate-600 mt-2">※企業データベースに基づく。従業員数降順。</div>"#);
    html.push_str("</div>");
    html
}

fn kpi_card(html: &mut String, label: &str, value: &str, color: &str) {
    html.push_str(&format!(
        r#"<div class="text-center p-2 bg-slate-800/50 rounded">
            <div class="text-sm font-bold {color}">{}</div>
            <div class="text-[10px] text-slate-500">{}</div>
        </div>"#,
        escape_html(value),
        escape_html(label),
    ));
}
