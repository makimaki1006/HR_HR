//! CSV × HW × 外部統計 統合レポート描画

use super::super::company::fetch::NearbyCompany;
use super::super::helpers::{escape_html, format_number, get_f64, get_str_ref};
use super::super::insight::fetch::InsightContext;
use super::super::insight::helpers::{Insight, Severity};
use super::hw_enrichment::HwAreaEnrichment;

/// 統合レポートHTML生成
///
/// # Args
/// - `hw_enrichments`: CSV に含まれる pref/muni ペアごとの HW 連携指標
///   （空スライスなら「地域×HW データ連携」セクションは非表示）
pub(crate) fn render_integration(
    pref: &str,
    muni: &str,
    insights: &[Insight],
    ctx: &InsightContext,
    companies: &[NearbyCompany],
    hw_enrichments: &[HwAreaEnrichment],
) -> String {
    let mut html = String::with_capacity(12_000);
    let location = if !muni.is_empty() {
        format!("{} {}", pref, muni)
    } else {
        pref.to_string()
    };

    html.push_str(&format!(
        r#"<div class="space-y-4 mt-6" id="survey-integration">
        <section class="stat-card border-l-4 border-blue-500">
            <div class="flex items-start justify-between flex-wrap gap-3">
                <div>
                    <h3 class="text-lg font-bold text-white">HW統合分析
                        <span class="text-blue-400 text-base font-normal ml-2">{}</span>
                    </h3>
                    <p class="text-xs text-slate-400 mt-1">
                        アップロードCSVの主要地域に対して、HW求人・外部統計・企業データを突き合わせた参考比較です。
                    </p>
                </div>
                <div class="text-xs text-slate-500 text-right">
                    <div>スコープ: HW掲載求人のみ</div>
                    <div>外部統計: e-Stat / SSDSE-A</div>
                </div>
            </div>
        </section>"#,
        escape_html(&location)
    ));

    // HWデータセクション
    html.push_str(&render_hw_section(ctx));

    // 地域×HW データ連携セクション（CSV の pref/muni ペア × HW DB 突合）
    html.push_str(&render_hw_area_enrichment_section(hw_enrichments, ctx));

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
    html.push_str(r#"<section class="stat-card"><h4 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-blue-500 pl-2">ハローワーク求人市場</h4>"#);

    if ctx.vacancy.is_empty() && ctx.cascade.is_empty() {
        html.push_str(r#"<p class="text-slate-500 text-xs">この地域のHWデータはありません</p>"#);
    } else {
        html.push_str(r#"<div class="grid grid-cols-2 md:grid-cols-4 gap-3">"#);
        // 正社員の欠員率
        if let Some(row) = ctx
            .vacancy
            .iter()
            .find(|r| get_str_ref(r, "emp_group") == "正社員")
        {
            let total = get_f64(row, "total_count") as i64;
            let vacancy_rate = get_f64(row, "vacancy_rate");
            kpi_card(
                &mut html,
                "HW求人数",
                &format_number(total),
                "text-blue-400",
            );
            let vr_color = if vacancy_rate > 0.3 {
                "text-red-400"
            } else if vacancy_rate > 0.2 {
                "text-amber-400"
            } else {
                "text-green-400"
            };
            kpi_card(
                &mut html,
                "欠員率(正社員)",
                &format!("{:.1}%", vacancy_rate * 100.0),
                vr_color,
            );
        }
        // カスケードから給与
        if let Some(row) = ctx
            .cascade
            .iter()
            .find(|r| get_str_ref(r, "emp_group") == "正社員")
        {
            let avg_sal = get_f64(row, "avg_salary_min") as i64;
            let holidays = get_f64(row, "avg_annual_holidays") as i64;
            if avg_sal > 0 {
                kpi_card(
                    &mut html,
                    "HW平均月給",
                    &format!("{}円", format_number(avg_sal)),
                    "text-white",
                );
            }
            if holidays > 0 {
                kpi_card(
                    &mut html,
                    "HW平均休日",
                    &format!("{}日", holidays),
                    "text-white",
                );
            }
        }
        html.push_str("</div>");

        // 充足トレンド
        if !ctx.ts_fulfillment.is_empty() {
            if let Some(last) = ctx.ts_fulfillment.last() {
                let days = get_f64(last, "avg_listing_days");
                if days > 0.0 {
                    html.push_str(&format!(
                        r#"<div class="text-xs text-slate-500 mt-2">平均掲載日数: {:.0}日</div>"#,
                        days
                    ));
                }
            }
        }
    }
    html.push_str(r#"<div class="text-[11px] text-slate-600 mt-3 border-t border-slate-800 pt-2">HW掲載求人のみを対象とした集計です。IT・通信等でHW掲載が少ない産業では参考値となります。</div>"#);
    html.push_str("</section>");
    html
}

/// 外部統計セクション
fn render_external_section(ctx: &InsightContext) -> String {
    let mut html = String::with_capacity(2_000);
    html.push_str(r#"<section class="stat-card"><h4 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-emerald-500 pl-2">地域特性（外部統計）</h4>"#);

    let mut has_data = false;
    html.push_str(r#"<div class="grid grid-cols-2 md:grid-cols-3 gap-3">"#);

    // 人口
    if let Some(row) = ctx.ext_population.first() {
        let pop = get_f64(row, "total_population") as i64;
        if pop > 0 {
            kpi_card(
                &mut html,
                "総人口",
                &format!("{}人", format_number(pop)),
                "text-white",
            );
            has_data = true;
        }
    }

    // 昼夜間人口比
    if let Some(row) = ctx.ext_daytime_pop.first() {
        let ratio = get_f64(row, "daytime_ratio");
        if ratio > 0.0 {
            let label = if ratio > 1.05 {
                "都市型（通勤流入）"
            } else if ratio < 0.95 {
                "ベッドタウン型（通勤流出）"
            } else {
                "均衡型"
            };
            kpi_card(
                &mut html,
                &format!("昼夜間人口比 ({})", label),
                &format!("{:.2}", ratio),
                "text-cyan-400",
            );
            has_data = true;
        }
    }

    // 転入転出
    if let Some(row) = ctx.ext_migration.first() {
        let in_m = get_f64(row, "in_migration") as i64;
        let out_m = get_f64(row, "out_migration") as i64;
        let net = in_m - out_m;
        let color = if net > 0 {
            "text-green-400"
        } else {
            "text-red-400"
        };
        kpi_card(&mut html, "純移動数", &format!("{:+}人", net), color);
        has_data = true;
    }

    // 有効求人倍率
    if let Some(row) = ctx.ext_job_ratio.last() {
        let ratio = get_f64(row, "ratio_total");
        if ratio > 0.0 {
            let color = if ratio > 1.5 {
                "text-red-400"
            } else if ratio > 1.0 {
                "text-amber-400"
            } else {
                "text-green-400"
            };
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
        html.push_str(
            r#"<p class="text-slate-500 text-xs">この地域の外部統計データはありません</p>"#,
        );
    } else {
        html.push_str(
            r#"<div class="text-[11px] text-slate-600 mt-3 border-t border-slate-800 pt-2">出典: e-Stat / SSDSE-A。地域特性と採用難度には相関が見られますが、因果関係を示すものではありません。</div>"#,
        );
    }

    html.push_str("</section>");
    html
}

/// insight示唆セクション
fn render_insights_section(insights: &[Insight]) -> String {
    if insights.is_empty() {
        return String::new();
    }

    let mut html = String::with_capacity(2_000);
    html.push_str(
        r#"<section class="stat-card"><h4 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-amber-500 pl-2">自動診断の示唆</h4>"#,
    );

    let critical = insights
        .iter()
        .filter(|i| i.severity == Severity::Critical)
        .count();
    let warning = insights
        .iter()
        .filter(|i| i.severity == Severity::Warning)
        .count();
    let positive = insights
        .iter()
        .filter(|i| i.severity == Severity::Positive)
        .count();

    // 傾向サマリ（「評価」ではなく「傾向」として記述）
    let tendency = if critical >= 2 {
        ("採用課題の傾向が強く見られる", "text-red-400")
    } else if critical >= 1 || warning >= 3 {
        ("注意を要する傾向が見られる", "text-amber-400")
    } else if positive >= 2 {
        ("比較的良好な傾向が見られる", "text-emerald-400")
    } else {
        ("標準的な傾向", "text-slate-300")
    };
    html.push_str(&format!(
        r#"<div class="text-sm text-white mb-3">傾向: <span class="font-bold {color}">{label}</span>
            <span class="text-xs text-slate-400 ml-2">(重大{c}件 / 注意{w}件 / 良好{p}件)</span>
        </div>"#,
        label = tendency.0,
        color = tendency.1,
        c = critical,
        w = warning,
        p = positive,
    ));

    // 上位5件の示唆をカード表示
    for insight in insights.iter().take(5) {
        let badge = insight.severity.badge_class();
        let bg = insight.severity.bg_class();
        let label = insight.severity.label();
        html.push_str(&format!(
            r#"<div class="flex items-start gap-2 p-2.5 rounded border {bg} mb-2">
                <span class="px-2 py-0.5 rounded text-[10px] font-medium {badge} shrink-0">{label}</span>
                <div class="min-w-0 flex-1">
                    <p class="text-xs text-white font-medium">{}</p>
                    <p class="text-[10px] text-slate-400 mt-0.5">{}</p>
                </div>
            </div>"#,
            escape_html(&insight.title),
            escape_html(&insight.body),
        ));
    }

    html.push_str(r#"<div class="text-[11px] text-slate-600 mt-3 border-t border-slate-800 pt-2">
        示唆はHW掲載求人と外部統計に基づく相対的観察です。採用判断の唯一の根拠とせず、詳細分析タブでの個別検証を推奨します。
    </div>"#);
    html.push_str("</section>");
    html
}

/// 地域注目企業セクション
/// （旧: SalesNow 表示。ラベルから SalesNow 文言を削除し、
///  与信スコア → 売上 / 1年人員推移 / 3ヶ月人員推移 に置換）
fn render_companies_section(companies: &[NearbyCompany], location: &str) -> String {
    let mut html = String::with_capacity(3_000);
    html.push_str(r#"<section class="stat-card"><h4 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-emerald-500 pl-2">地域注目企業</h4>"#);

    if companies.is_empty() {
        html.push_str(&format!(
            r#"<p class="text-slate-500 text-xs">{}に該当する企業データはありません</p>"#,
            escape_html(location)
        ));
        html.push_str("</section>");
        return html;
    }

    // サマリー
    let total = companies.len();
    let with_hw = companies.iter().filter(|c| c.hw_posting_count > 0).count();
    let industries: Vec<&str> = {
        let mut inds: Vec<&str> = companies
            .iter()
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

    // テーブル（モバイル対応: overflow-x-auto + 最小幅確保）
    html.push_str(
        r#"<div class="overflow-x-auto"><table class="w-full text-xs min-w-[720px]">
        <thead><tr class="text-slate-400 border-b border-slate-700">
            <th class="text-left py-1.5 px-2">企業名</th>
            <th class="text-left py-1.5 px-2">業種</th>
            <th class="text-right py-1.5 px-2">従業員数</th>
            <th class="text-right py-1.5 px-2">売上</th>
            <th class="text-right py-1.5 px-2">1年人員推移</th>
            <th class="text-right py-1.5 px-2">3ヶ月人員推移</th>
            <th class="text-right py-1.5 px-2">HW求人</th>
            <th class="text-center py-1.5 px-2">詳細</th>
        </tr></thead><tbody>"#,
    );

    for c in companies.iter().take(50) {
        let hw_badge = if c.hw_posting_count > 0 {
            format!(
                r#"<span class="text-blue-400 font-medium">{}</span>"#,
                c.hw_posting_count
            )
        } else {
            r#"<span class="text-slate-600">-</span>"#.to_string()
        };

        let emp_text = if c.employee_count > 0 {
            format_number(c.employee_count)
        } else {
            "-".to_string()
        };

        // 売上: sales_range（ラベル）優先、なければ sales_amount 円表記
        let sales_text = if !c.sales_range.is_empty() {
            escape_html(&c.sales_range)
        } else if c.sales_amount > 0.0 {
            format!("{}円", format_number(c.sales_amount as i64))
        } else {
            "-".to_string()
        };

        // 人員推移（%）: 符号付き・色分け
        let delta_1y_html = render_delta_cell(c.employee_delta_1y);
        let delta_3m_html = render_delta_cell(c.employee_delta_3m);

        html.push_str(&format!(
            r##"<tr class="border-b border-slate-800 hover:bg-slate-800/50">
                <td class="py-1.5 px-2 text-white">{}</td>
                <td class="py-1.5 px-2 text-slate-400">{}</td>
                <td class="py-1.5 px-2 text-right text-slate-300">{}</td>
                <td class="py-1.5 px-2 text-right text-slate-300">{}</td>
                <td class="py-1.5 px-2 text-right">{}</td>
                <td class="py-1.5 px-2 text-right">{}</td>
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
            sales_text,
            delta_1y_html,
            delta_3m_html,
            hw_badge,
            escape_html(&c.corporate_number),
        ));
    }

    html.push_str("</tbody></table></div>");
    html.push_str(r#"<div class="text-[11px] text-slate-600 mt-3 border-t border-slate-800 pt-2">地域注目企業（従業員数降順）。売上・人員推移は外部企業DB由来の参考値で、直近の組織改編や統計粒度による揺らぎを含みます。</div>"#);
    html.push_str("</section>");
    html
}

/// 人員推移(%)セル生成: 符号付き + 色分け
fn render_delta_cell(delta_pct: f64) -> String {
    // 0.0 も「変化なし」として表示する（データ欠損はフェッチ層で 0.0 想定）
    if delta_pct.abs() < 0.05 {
        return r#"<span class="text-slate-500">±0.0%</span>"#.to_string();
    }
    let color = if delta_pct > 0.0 {
        "text-emerald-400"
    } else {
        "text-red-400"
    };
    format!(
        r#"<span class="{color} font-medium">{sign}{val:.1}%</span>"#,
        color = color,
        sign = if delta_pct > 0.0 { "+" } else { "" },
        val = delta_pct,
    )
}

/// 地域×HW データ連携セクション
/// CSV に含まれる (pref, muni) ペアごとに HW DB と突合した結果を表示
fn render_hw_area_enrichment_section(
    enrichments: &[HwAreaEnrichment],
    ctx: &InsightContext,
) -> String {
    let mut html = String::with_capacity(3_000);
    html.push_str(r#"<section class="stat-card">
        <h4 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-cyan-500 pl-2">地域×HW データ連携</h4>"#);

    if enrichments.is_empty() {
        html.push_str(
            r#"<p class="text-slate-500 text-xs">CSV から地域（都道府県＋市区町村）を特定できなかったため、地域別 HW 連携は表示できません。</p>
            </section>"#,
        );
        return html;
    }

    // 外部統計由来の欠員率（正社員）を pref 単位で補填用に抽出
    // InsightContext.vacancy は単一地域のものなので、同一都道府県の全行に同じ値を当てる簡易運用
    let vacancy_hint = ctx
        .vacancy
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")
        .map(|r| get_f64(r, "vacancy_rate") * 100.0);

    html.push_str(&format!(
        r#"<div class="text-xs text-slate-400 mb-3">
            CSV に含まれる {n} 件の地域について、HW DB から現在掲載件数・過去3ヶ月／1年の推移を突合しています。
            欠員率は外部統計由来（参考値）。
        </div>"#,
        n = enrichments.len()
    ));

    html.push_str(
        r#"<div class="overflow-x-auto"><table class="w-full text-xs min-w-[640px]">
        <thead><tr class="text-slate-400 border-b border-slate-700">
            <th class="text-left py-1.5 px-2">都道府県</th>
            <th class="text-left py-1.5 px-2">市区町村</th>
            <th class="text-right py-1.5 px-2">HW現在掲載件数</th>
            <th class="text-right py-1.5 px-2">3ヶ月推移</th>
            <th class="text-right py-1.5 px-2">1年推移</th>
            <th class="text-right py-1.5 px-2">欠員率</th>
        </tr></thead><tbody>"#,
    );

    // 件数降順で上位 30 件まで表示
    let mut sorted: Vec<&HwAreaEnrichment> = enrichments.iter().collect();
    sorted.sort_by(|a, b| b.hw_posting_count.cmp(&a.hw_posting_count));

    for e in sorted.iter().take(30) {
        let count_html = if e.hw_posting_count > 0 {
            format!(
                r#"<span class="text-blue-400 font-medium">{}</span>"#,
                format_number(e.hw_posting_count)
            )
        } else {
            r#"<span class="text-slate-600">-</span>"#.to_string()
        };

        let change_3m_html = render_pct_change_cell(e.posting_change_3m_pct, e.change_label_3m());
        let change_1y_html = render_pct_change_cell(e.posting_change_1y_pct, e.change_label_1y());

        // 欠員率は enrichment 固有値を優先、無ければ ctx の都道府県値でヒント表示
        let vacancy_html = match e.vacancy_rate_pct.or(vacancy_hint) {
            Some(v) if v > 0.0 => {
                let color = if v > 30.0 {
                    "text-red-400"
                } else if v > 20.0 {
                    "text-amber-400"
                } else {
                    "text-emerald-400"
                };
                format!(r#"<span class="{}">{:.1}%</span>"#, color, v)
            }
            _ => r#"<span class="text-slate-600">-</span>"#.to_string(),
        };

        html.push_str(&format!(
            r#"<tr class="border-b border-slate-800 hover:bg-slate-800/50">
                <td class="py-1.5 px-2 text-slate-200">{}</td>
                <td class="py-1.5 px-2 text-white">{}</td>
                <td class="py-1.5 px-2 text-right">{}</td>
                <td class="py-1.5 px-2 text-right">{}</td>
                <td class="py-1.5 px-2 text-right">{}</td>
                <td class="py-1.5 px-2 text-right">{}</td>
            </tr>"#,
            escape_html(&e.prefecture),
            escape_html(&e.municipality),
            count_html,
            change_3m_html,
            change_1y_html,
            vacancy_html,
        ));
    }

    html.push_str("</tbody></table></div>");

    if enrichments.len() > 30 {
        html.push_str(&format!(
            r#"<div class="text-[11px] text-slate-600 mt-2">※ 掲載件数上位 30 件を表示（全 {} 件中）</div>"#,
            enrichments.len()
        ));
    }

    html.push_str(r#"<div class="text-[11px] text-slate-600 mt-3 border-t border-slate-800 pt-2">
        3ヶ月／1年推移は HW 時系列 DB 由来（都道府県×雇用形態グループの月次推移）。
        欠員率は外部統計由来の参考値（市区町村粒度で欠損する場合があります）。
        HW掲載求人のみを対象としており、全求人市場の動向ではありません。
    </div>"#);
    html.push_str("</section>");
    html
}

/// 推移率(%)セル生成: None / 0 / +/- の区別と定性ラベル併記
fn render_pct_change_cell(pct: Option<f64>, label: &str) -> String {
    match pct {
        None => r#"<span class="text-slate-600">-</span>"#.to_string(),
        Some(v) => {
            let color = if v > 3.0 {
                "text-emerald-400"
            } else if v < -3.0 {
                "text-red-400"
            } else {
                "text-slate-300"
            };
            let sign = if v > 0.0 { "+" } else { "" };
            format!(
                r#"<div class="{color} font-medium">{sign}{v:.1}%</div>
                <div class="text-[10px] text-slate-500">{label}</div>"#,
                color = color,
                sign = sign,
                v = v,
                label = escape_html(label),
            )
        }
    }
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
