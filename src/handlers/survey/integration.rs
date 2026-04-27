//! CSV × HW × 外部統計 統合レポート描画

use super::super::company::fetch::NearbyCompany;
use super::super::helpers::{escape_html, format_number, get_f64, get_i64, get_str_ref, Row};
use super::super::insight::fetch::InsightContext;
use super::super::insight::helpers::{Insight, Severity};
use super::hw_enrichment::HwAreaEnrichment;

use std::fmt::Write as _;

/// 媒体分析タブ拡張データ（Impl-1 #6/#18/D-3/D-4 用）
///
/// `render_integration` 互換性のため、Default で空構造体を返せる設計。
/// 全 vec 空ならば各拡張セクションは描画されない（fail-soft）。
///
/// `feedback_correlation_not_causation.md` 準拠: スコア / KPI は傾向参照で
/// 因果を主張しないことを各セクションの注記で明示する。
#[derive(Debug, Default, Clone)]
pub(crate) struct SurveyExtensionData {
    /// 案 #6: 主要 3 都道府県の region_benchmark 行（pref → 正社員行）
    /// 各 Row は salary_competitiveness / job_market_tightness / wage_compliance /
    /// working_age_ratio / population_growth / labor_fluidity 等の 0-1 正規化スコアを持つ。
    ///
    /// 2026-04-26 ユーザー指摘により、都道府県粒度では媒体分析の参考にならないと判断。
    /// 主たるレーダー表示は `top3_municipality_benchmark` を使用するが、
    /// 後方互換のため本フィールドは維持（既存テスト互換）。
    pub top3_region_benchmark: Vec<(String, Row)>,
    /// 案 D-3: dominant 都道府県の産業別就業者構成 Top10（v2_external_industry_structure）
    /// 注記: schema 上 prefecture_code のみ対応のため都道府県粒度のみ。
    pub industry_structure_top10: Vec<Row>,
    /// 2026-04-26: CSV 上位 3 市区町村の region_benchmark 行 (label, row, is_pref_fallback)。
    /// label は "{pref} {muni}" もしくは "{pref} {muni} (県値参考)" 形式。
    /// is_pref_fallback=true の場合、市区町村粒度データなしで都道府県値を流用。
    /// 媒体分析タブの主役レーダーチャート (「主要都市 6 軸ベンチマーク」)。
    pub top3_municipality_benchmark: Vec<(String, Row, bool)>,
    /// 2026-04-26: CSV 件数上位 N 市区町村のヒートマップデータ
    /// `(prefecture, municipality, count)` の Vec。47 県ヒートマップを置換する。
    pub top_municipalities_heatmap: Vec<(String, String, usize)>,
}

/// 統合レポートHTML生成（後方互換: SurveyExtensionData 無し）
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
    render_integration_with_ext(
        pref,
        muni,
        insights,
        ctx,
        companies,
        hw_enrichments,
        &SurveyExtensionData::default(),
    )
}

/// 統合レポートHTML生成（拡張版: 媒体分析データ活用 #6/#18/D-3/D-4）
///
/// 拡張データ `ext` が空であれば旧 render_integration と同一動作。
pub(crate) fn render_integration_with_ext(
    pref: &str,
    muni: &str,
    insights: &[Insight],
    ctx: &InsightContext,
    companies: &[NearbyCompany],
    hw_enrichments: &[HwAreaEnrichment],
    ext: &SurveyExtensionData,
) -> String {
    let mut html = String::with_capacity(12_000);
    let location = if !muni.is_empty() {
        format!("{} {}", pref, muni)
    } else {
        pref.to_string()
    };

    write!(html,
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
    ).unwrap();

    // HWデータセクション
    html.push_str(&render_hw_section(ctx));

    // 地域×HW データ連携セクション（CSV の pref/muni ペア × HW DB 突合）
    html.push_str(&render_hw_area_enrichment_section(hw_enrichments, ctx));

    // 外部統計セクション
    html.push_str(&render_external_section(ctx));

    // 2026-04-26: 主要市区町村ヒートマップ（CSV 件数 Top N）
    // ユーザー指摘「都道府県単位の集計はあまり参考にならない」に対応し、
    // CSV 上の主要市区町村に絞ったヒートマップを最初に表示する。
    html.push_str(&render_municipality_heatmap_section(
        &ext.top_municipalities_heatmap,
    ));

    // 2026-04-26: 主要市区町村 6 軸ベンチマーク レーダー（top3_municipality_benchmark あり時）
    // 都道府県粒度より高い解像度を提供する主役レーダー。
    html.push_str(&render_municipality_benchmark_radar_section(
        &ext.top3_municipality_benchmark,
    ));

    // Impl-1 #6: 主要地域 6 軸ベンチマーク レーダーチャート（top3_region_benchmark あり時のみ）
    // ※ 後方互換維持: top3_municipality_benchmark がある場合は冗長になるため都道府県版は非表示
    if ext.top3_municipality_benchmark.is_empty() {
        html.push_str(&render_region_benchmark_radar_section(
            &ext.top3_region_benchmark,
        ));
    }

    // Impl-1 #18 / D-4: 可住地密度 + 高齢化率 KPI カード（地域特性補足）
    html.push_str(&render_geography_aging_section(ctx));

    // Impl-1 D-3: 産業別就業者構成 Top10（dominant 都道府県）
    html.push_str(&render_industry_structure_section(
        &ext.industry_structure_top10,
        pref,
    ));

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
                    write!(
                        html,
                        r#"<div class="text-xs text-slate-500 mt-2">平均掲載日数: {:.0}日</div>"#,
                        days
                    )
                    .unwrap();
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
    write!(
        html,
        r#"<div class="text-sm text-white mb-3">傾向: <span class="font-bold {color}">{label}</span>
            <span class="text-xs text-slate-400 ml-2">(重大{c}件 / 注意{w}件 / 良好{p}件)</span>
        </div>"#,
        label = tendency.0,
        color = tendency.1,
        c = critical,
        w = warning,
        p = positive,
    )
    .unwrap();

    // 上位5件の示唆をカード表示
    for insight in insights.iter().take(5) {
        let badge = insight.severity.badge_class();
        let bg = insight.severity.bg_class();
        let label = insight.severity.label();
        write!(html,
            r#"<div class="flex items-start gap-2 p-2.5 rounded border {bg} mb-2">
                <span class="px-2 py-0.5 rounded text-[10px] font-medium {badge} shrink-0">{label}</span>
                <div class="min-w-0 flex-1">
                    <p class="text-xs text-white font-medium">{}</p>
                    <p class="text-[10px] text-slate-400 mt-0.5">{}</p>
                </div>
            </div>"#,
            escape_html(&insight.title),
            escape_html(&insight.body),
        ).unwrap();
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
        write!(
            html,
            r#"<p class="text-slate-500 text-xs">{}に該当する企業データはありません</p>"#,
            escape_html(location)
        )
        .unwrap();
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

    write!(html,
        r#"<div class="text-xs text-slate-300 mb-3">{}の企業 <span class="text-white font-bold">{}社</span>{}（うちHW求人あり: {}社）</div>"#,
        escape_html(location), total, escape_html(&ind_text), with_hw
    ).unwrap();

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

        write!(
            html,
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
        )
        .unwrap();
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
    // 2026-04-26 監査 Q1.3: DB 側 vacancy_rate は 0-1 比率なので
    //   VacancyRatePct::from_ratio() で % 単位 Newtype に明示変換する
    let vacancy_hint: Option<crate::handlers::types::VacancyRatePct> = ctx
        .vacancy
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")
        .map(|r| get_f64(r, "vacancy_rate"))
        .filter(|v| *v > 0.0)
        .map(crate::handlers::types::VacancyRatePct::from_ratio);

    write!(html,
        r#"<div class="text-xs text-slate-400 mb-3">
            CSV に含まれる {n} 件の地域について、HW DB から現在掲載件数を市区町村粒度で突合しています。
        </div>"#,
        n = enrichments.len()
    ).unwrap();

    // D-2 監査 Q1.1 / Q3.3 対応 / feedback_hw_data_scope.md 準拠:
    //   旧 UI は posting_change_3m / 1y を市区町村行に表示していたが、
    //   ts_turso_counts は都道府県粒度しか持たないため、同一都道府県内の
    //   全市区町村が同じ値（粒度詐称）になっていた。
    //   印刷版は P0 #4 で削除済だが、Tab UI は残存していたため、ここで列を削除し、
    //   推移は別カードで都道府県代表値として明示分離する。
    let pref_summary = build_pref_change_summary(enrichments);
    if !pref_summary.is_empty() {
        html.push_str(&pref_summary);
    }

    html.push_str(
        r#"<div class="overflow-x-auto"><table class="w-full text-xs min-w-[480px]" data-testid="hw-area-enrichment-table">
        <thead><tr class="text-slate-400 border-b border-slate-700">
            <th class="text-left py-1.5 px-2">都道府県</th>
            <th class="text-left py-1.5 px-2">市区町村</th>
            <th class="text-right py-1.5 px-2">HW現在掲載件数</th>
            <th class="text-right py-1.5 px-2">欠員率（都道府県）</th>
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

        // 欠員率は enrichment 固有値を優先、無ければ ctx の都道府県値でヒント表示
        // 2026-04-26 監査 Q1.3: VacancyRatePct (% 単位) で統一済
        // 注: 欠員率は都道府県粒度の参考値であり、市区町村単位の差は反映しない
        let vacancy_html = match e.vacancy_rate_pct.or(vacancy_hint) {
            Some(vp) if vp.as_f64() > 0.0 => {
                let v = vp.as_f64();
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

        write!(
            html,
            r#"<tr class="border-b border-slate-800 hover:bg-slate-800/50">
                <td class="py-1.5 px-2 text-slate-200">{}</td>
                <td class="py-1.5 px-2 text-white">{}</td>
                <td class="py-1.5 px-2 text-right">{}</td>
                <td class="py-1.5 px-2 text-right">{}</td>
            </tr>"#,
            escape_html(&e.prefecture),
            escape_html(&e.municipality),
            count_html,
            vacancy_html,
        )
        .unwrap();
    }

    html.push_str("</tbody></table></div>");

    if enrichments.len() > 30 {
        write!(html,
            r#"<div class="text-[11px] text-slate-600 mt-2">※ 掲載件数上位 30 件を表示（全 {} 件中）</div>"#,
            enrichments.len()
        ).unwrap();
    }

    html.push_str(
        r#"<div class="text-[11px] text-slate-600 mt-3 border-t border-slate-800 pt-2">
        ※ HW現在掲載件数は市区町村粒度の値ですが、3ヶ月／1年の求人件数推移と欠員率は HW 時系列 DB / 外部統計の都道府県粒度のため、市区町村別の差分は反映していません。
        市区町村行に都道府県値をコピー表示すると粒度の誤誘導となるため、推移は上部の都道府県別カードに分離しています。
        本セクションはハローワーク掲載求人のみを対象としており、全求人市場の動向ではありません。
        集計値は傾向の観測であり、因果関係を主張するものではありません。
    </div>"#,
    );
    html.push_str("</section>");
    html
}

/// 都道府県粒度の posting_change_3m/1y を「市区町村テーブルとは分離した」カード群として描画。
///
/// D-2 監査 Q1.1 / Q3.3 対応:
///   旧仕様では市区町村テーブルの各行に都道府県値をコピー表示していたため、
///   同一都道府県内の muni 行で値が同一になる粒度詐称が発生していた。
///   ここでは pref ごとに 1 つのカードを描画し、都道府県粒度であることを
///   ラベルで明示する。
///
/// feedback_hw_data_scope.md 準拠:
///   暴走値はサニティチェックで除外済み（hw_enrichment::sanitize_change_pct）。
fn build_pref_change_summary(enrichments: &[HwAreaEnrichment]) -> String {
    use std::collections::BTreeMap;
    // pref → (3m, 1y) を最初に出てきた値で固定（同 pref 内で同値前提）
    let mut by_pref: BTreeMap<String, (Option<f64>, Option<f64>)> = BTreeMap::new();
    for e in enrichments {
        if e.prefecture.is_empty() {
            continue;
        }
        by_pref
            .entry(e.prefecture.clone())
            .or_insert((e.posting_change_3m_pct, e.posting_change_1y_pct));
    }
    // 推移データがどれか 1 つでも存在する pref のみ表示
    let visible: Vec<(&String, &(Option<f64>, Option<f64>))> = by_pref
        .iter()
        .filter(|(_, (c3, c1))| c3.is_some() || c1.is_some())
        .collect();
    if visible.is_empty() {
        return String::new();
    }
    let mut html = String::with_capacity(1_500);
    html.push_str(
        r#"<div class="mb-3 p-3 rounded bg-slate-800/40 border border-slate-700" data-testid="hw-pref-trend-card">
            <div class="text-[11px] text-slate-400 mb-2">
                <strong class="text-slate-200">都道府県粒度の参考値</strong>
                ／ HW 時系列 DB は都道府県単位の集計のため、市区町村別の差分は反映していません。
                ※ ETL 初期スナップショットによるノイズ（暴走値）はサニティチェックで除外しています。
            </div>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-2 text-xs">"#,
    );
    for (pref, (c3, c1)) in visible.iter().take(8) {
        let c3_text = match c3 {
            Some(v) => format!("{:+.1}%", v),
            None => "—".to_string(),
        };
        let c1_text = match c1 {
            Some(v) => format!("{:+.1}%", v),
            None => "—".to_string(),
        };
        write!(
            html,
            r#"<div class="p-2 rounded bg-slate-900/50">
                <div class="text-slate-300 font-medium">{pref}</div>
                <div class="text-[11px] text-slate-400 mt-0.5">3ヶ月推移: <span class="text-white">{c3}</span> ／ 1年推移: <span class="text-white">{c1}</span></div>
            </div>"#,
            pref = escape_html(pref),
            c3 = c3_text,
            c1 = c1_text,
        )
        .unwrap();
    }
    html.push_str("</div></div>");
    html
}

/// 推移率(%)セル生成: None / 0 / +/- の区別と定性ラベル併記
///
/// D-2 監査 Q1.1 対応: 市区町村テーブルから列削除済 (Tab UI からも削除)。
/// 関数自体は今後の都道府県別カード等での再利用を想定し残置。
#[allow(dead_code)]
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
    write!(
        html,
        r#"<div class="text-center p-2 bg-slate-800/50 rounded">
            <div class="text-sm font-bold {color}">{}</div>
            <div class="text-[10px] text-slate-500">{}</div>
        </div>"#,
        escape_html(value),
        escape_html(label),
    )
    .unwrap();
}

// =====================================================================
// Impl-1 (2026-04-26): 媒体分析データ活用 #6 / #18 / D-3 / D-4
// =====================================================================

/// 6 軸レーダーチャート用の軸定義
///
/// region_benchmark テーブルのカラム名 → 表示ラベル。スコアは 0-1 正規化値で、
/// 表示時に 100 倍する。
const REGION_BENCHMARK_RADAR_AXES: &[(&str, &str)] = &[
    ("salary_competitiveness", "給与競争力"),
    ("job_market_tightness", "求人量"),
    ("posting_freshness", "充足度"),
    ("working_age_ratio", "人口動態"),
    ("wage_compliance", "賃金遵守"),
    ("industry_diversity", "産業多様性"),
];

/// 0-1 スコアを 0-100 に変換し、範囲外を clamp。
fn radar_score_to_pct(score: f64) -> f64 {
    if !score.is_finite() {
        return 0.0;
    }
    (score * 100.0).clamp(0.0, 100.0)
}

/// 案 #6: 主要地域 6 軸ベンチマーク レーダーチャート
///
/// CSV 上位 3 都道府県の region_benchmark 6 軸スコアを ECharts radar で重ね描き。
/// データが空 / 1 県のみの場合はセクション非表示（fail-soft）。
///
/// 必須注記: 「6 軸スコアは相対値。地域間の戦略的優劣ではなく特性の違いを示す」
fn render_region_benchmark_radar_section(top3: &[(String, Row)]) -> String {
    if top3.is_empty() {
        return String::new();
    }

    let mut html = String::with_capacity(2_500);
    html.push_str(r#"<section class="stat-card" data-testid="region-benchmark-radar"><h4 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-purple-500 pl-2">主要地域 6 軸ベンチマーク</h4>"#);

    write!(
        html,
        r#"<div class="text-xs text-slate-400 mb-2">
            CSV 上位 {n} 都道府県を 6 軸（給与 / 求人量 / 充足 / 人口動態 / 賃金遵守 / 産業多様性）でレーダー比較。\
            軸ごとに 0-100 で正規化された相対値を重ね描きしています。
        </div>"#,
        n = top3.len()
    )
    .unwrap();

    // ECharts radar config 構築
    let indicators: Vec<serde_json::Value> = REGION_BENCHMARK_RADAR_AXES
        .iter()
        .map(|(_, label)| serde_json::json!({"name": label, "max": 100}))
        .collect();

    let colors = ["#a855f7", "#22d3ee", "#f59e0b"]; // 紫 / シアン / アンバー
    let series_data: Vec<serde_json::Value> = top3
        .iter()
        .enumerate()
        .map(|(i, (pref, row))| {
            let values: Vec<f64> = REGION_BENCHMARK_RADAR_AXES
                .iter()
                .map(|(col, _)| radar_score_to_pct(get_f64(row, col)))
                .collect();
            serde_json::json!({
                "name": pref,
                "value": values,
                "lineStyle": {"color": colors[i % colors.len()], "width": 2},
                "areaStyle": {"color": colors[i % colors.len()], "opacity": 0.15},
            })
        })
        .collect();

    let legend_names: Vec<&String> = top3.iter().map(|(p, _)| p).collect();

    let config = serde_json::json!({
        "tooltip": {"trigger": "item"},
        "legend": {
            "data": legend_names,
            "textStyle": {"color": "#cbd5e1", "fontSize": 11},
            "bottom": 0,
        },
        "radar": {
            "indicator": indicators,
            "shape": "polygon",
            "splitNumber": 4,
            "axisName": {"color": "#94a3b8", "fontSize": 11},
            "splitLine": {"lineStyle": {"color": "#334155"}},
            "splitArea": {"areaStyle": {"color": ["#1e293b80", "#0f172a80"]}},
            "axisLine": {"lineStyle": {"color": "#475569"}},
        },
        "series": [{
            "type": "radar",
            "data": series_data,
        }],
    });
    let cfg_str = config.to_string().replace('\'', "&#39;");

    write!(
        html,
        r#"<div class="echart" style="height:340px;width:100%;" data-chart-config='{}'></div>"#,
        cfg_str
    )
    .unwrap();

    // 凡例（テキストフォールバック: チャート非対応環境向け）
    html.push_str(r#"<div class="grid grid-cols-1 md:grid-cols-3 gap-2 mt-2 text-[11px]">"#);
    for (i, (pref, row)) in top3.iter().enumerate() {
        let color = colors[i % colors.len()];
        let scores: Vec<String> = REGION_BENCHMARK_RADAR_AXES
            .iter()
            .map(|(col, label)| {
                let v = radar_score_to_pct(get_f64(row, col));
                format!("{}={:.0}", label, v)
            })
            .collect();
        write!(
            html,
            r#"<div class="p-2 rounded bg-slate-800/40">
                <div class="font-semibold" style="color:{color}">{pref}</div>
                <div class="text-[10px] text-slate-400 mt-1">{scores}</div>
            </div>"#,
            color = color,
            pref = escape_html(pref),
            scores = escape_html(&scores.join(" / "))
        )
        .unwrap();
    }
    html.push_str("</div>");

    // 図番号 + 必須注記
    html.push_str(
        r#"<div class="text-[11px] text-slate-500 mt-2">図 6-2: 主要地域 6 軸ベンチマーク</div>"#,
    );
    html.push_str(
        r#"<div class="text-[11px] text-slate-600 mt-2 border-t border-slate-800 pt-2">
        6 軸スコアは相対値です。地域間の戦略的優劣ではなく特性の違いを示す傾向参照値であり、因果関係や採用成功の保証ではありません。
        スコープは HW 掲載求人および外部統計に基づきます。
    </div>"#,
    );
    html.push_str("</section>");
    html
}

/// 2026-04-26: 主要市区町村 6 軸ベンチマーク レーダーチャート
///
/// 媒体分析タブの主役レーダー。CSV 件数上位 3 市区町村の region_benchmark を重ね描き。
/// 市区町村粒度データが無い地域は都道府県値で fallback し、ラベルに「(県値参考)」を併記。
///
/// ユーザー指摘 (2026-04-26):
/// > 都道府県単位の集計データはあまり参考にならない
/// → 市区町村粒度に切替。
fn render_municipality_benchmark_radar_section(top3: &[(String, Row, bool)]) -> String {
    if top3.is_empty() {
        return String::new();
    }

    let mut html = String::with_capacity(2_500);
    html.push_str(r#"<section class="stat-card" data-testid="municipality-benchmark-radar"><h4 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-pink-500 pl-2">主要都市 6 軸ベンチマーク（市区町村粒度）</h4>"#);

    let has_fallback = top3.iter().any(|(_, _, fb)| *fb);
    write!(
        html,
        r#"<div class="text-xs text-slate-400 mb-2">
            CSV 件数 上位 {n} 市区町村を 6 軸（給与 / 求人量 / 充足 / 人口動態 / 賃金遵守 / 産業多様性）でレーダー比較。\
            軸ごとに 0-100 で正規化された相対値です。{fb}
        </div>"#,
        n = top3.len(),
        fb = if has_fallback {
            "<br>※ 一部地域は市区町村粒度データなしのため都道府県値で代用しています（ラベル末尾「(県値参考)」）。"
        } else {
            ""
        },
    )
    .unwrap();

    let indicators: Vec<serde_json::Value> = REGION_BENCHMARK_RADAR_AXES
        .iter()
        .map(|(_, label)| serde_json::json!({"name": label, "max": 100}))
        .collect();

    let colors = ["#ec4899", "#10b981", "#3b82f6"]; // ピンク / 緑 / 青
    let series_data: Vec<serde_json::Value> = top3
        .iter()
        .enumerate()
        .map(|(i, (label, row, _fb))| {
            let values: Vec<f64> = REGION_BENCHMARK_RADAR_AXES
                .iter()
                .map(|(col, _)| radar_score_to_pct(get_f64(row, col)))
                .collect();
            serde_json::json!({
                "name": label,
                "value": values,
                "lineStyle": {"color": colors[i % colors.len()], "width": 2},
                "areaStyle": {"color": colors[i % colors.len()], "opacity": 0.15},
            })
        })
        .collect();

    let legend_names: Vec<&String> = top3.iter().map(|(label, _, _)| label).collect();

    let config = serde_json::json!({
        "tooltip": {"trigger": "item"},
        "legend": {
            "data": legend_names,
            "textStyle": {"color": "#cbd5e1", "fontSize": 11},
            "bottom": 0,
        },
        "radar": {
            "indicator": indicators,
            "shape": "polygon",
            "splitNumber": 4,
            "axisName": {"color": "#94a3b8", "fontSize": 11},
            "splitLine": {"lineStyle": {"color": "#334155"}},
            "splitArea": {"areaStyle": {"color": ["#1e293b80", "#0f172a80"]}},
            "axisLine": {"lineStyle": {"color": "#475569"}},
        },
        "series": [{
            "type": "radar",
            "data": series_data,
        }],
    });
    let cfg_str = config.to_string().replace('\'', "&#39;");

    write!(
        html,
        r#"<div class="echart" style="height:340px;width:100%;" data-chart-config='{}'></div>"#,
        cfg_str
    )
    .unwrap();

    // 凡例（テキストフォールバック）
    html.push_str(r#"<div class="grid grid-cols-1 md:grid-cols-3 gap-2 mt-2 text-[11px]">"#);
    for (i, (label, row, fb)) in top3.iter().enumerate() {
        let color = colors[i % colors.len()];
        let scores: Vec<String> = REGION_BENCHMARK_RADAR_AXES
            .iter()
            .map(|(col, lbl)| {
                let v = radar_score_to_pct(get_f64(row, col));
                format!("{}={:.0}", lbl, v)
            })
            .collect();
        let fb_marker = if *fb {
            r#"<span class="text-amber-400 ml-1">[県値参考]</span>"#
        } else {
            ""
        };
        write!(
            html,
            r#"<div class="p-2 rounded bg-slate-800/40">
                <div class="font-semibold" style="color:{color}">{label}{fb_marker}</div>
                <div class="text-[10px] text-slate-400 mt-1">{scores}</div>
            </div>"#,
            color = color,
            label = escape_html(label),
            fb_marker = fb_marker,
            scores = escape_html(&scores.join(" / "))
        )
        .unwrap();
    }
    html.push_str("</div>");

    html.push_str(
        r#"<div class="text-[11px] text-slate-500 mt-2">図 6-2M: 主要都市 6 軸ベンチマーク（市区町村粒度）</div>"#,
    );
    html.push_str(
        r#"<div class="text-[11px] text-slate-600 mt-2 border-t border-slate-800 pt-2">
        6 軸スコアは相対値です。市区町村間の戦略的優劣ではなく特性の違いを示す傾向参照値であり、因果関係や採用成功の保証ではありません。
        スコープは HW 掲載求人および外部統計（市区町村粒度）に基づきます。
    </div>"#,
    );
    html.push_str("</section>");
    html
}

/// 2026-04-26: CSV 主要市区町村ヒートマップ
///
/// CSV 件数 上位 N 市区町村を件数順にカラーグラデーションで表示。
/// 47 県ヒートマップを置換。「都道府県単位は参考にならない」というユーザー指摘に対応。
fn render_municipality_heatmap_section(top: &[(String, String, usize)]) -> String {
    if top.is_empty() {
        return String::new();
    }

    let max_count = top.iter().map(|(_, _, c)| *c).max().unwrap_or(1).max(1);

    let mut html = String::with_capacity(2_000);
    html.push_str(r#"<section class="stat-card" data-testid="municipality-heatmap"><h4 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-rose-500 pl-2">主要市区町村ヒートマップ（CSV 件数）</h4>"#);

    write!(
        html,
        r#"<div class="text-xs text-slate-400 mb-3">
            CSV に登場する件数 Top {n} の市区町村を件数順に色分け表示。\
            濃い色ほど CSV 件数が多い地域です。媒体分析の主たる対象地域は本セクションを優先参照してください。
        </div>"#,
        n = top.len(),
    )
    .unwrap();

    // グリッド形式: 6 列 × 5 行 (最大 30 セル)
    html.push_str(r#"<div class="grid grid-cols-3 sm:grid-cols-4 md:grid-cols-5 lg:grid-cols-6 gap-2" data-testid="muni-heatmap-grid">"#);
    for (pref, muni, count) in top {
        let intensity = (*count as f64 / max_count as f64).clamp(0.05, 1.0);
        // bgColor: rose-500 を intensity で透明度調整
        let alpha = 0.15 + 0.7 * intensity;
        let text_color = if intensity > 0.5 { "#fff" } else { "#fcd1d8" };
        write!(
            html,
            r#"<div class="rounded p-2 text-center" style="background:rgba(244,63,94,{alpha:.2});color:{tc};" data-testid="muni-heatmap-cell">
                <div class="text-[10px] truncate" title="{pref_full}">{pref_short}</div>
                <div class="text-xs font-semibold truncate" title="{muni_full}">{muni}</div>
                <div class="text-sm font-bold mt-1">{count}</div>
            </div>"#,
            alpha = alpha,
            tc = text_color,
            pref_full = escape_html(pref),
            // 都道府県名末尾の "都/道/府/県" を省略してコンパクト化
            pref_short = escape_html(pref.trim_end_matches(['都', '道', '府', '県'])),
            muni_full = escape_html(muni),
            muni = escape_html(muni),
            count = format_number(*count as i64),
        )
        .unwrap();
    }
    html.push_str("</div>");

    html.push_str(
        r#"<div class="text-[11px] text-slate-500 mt-3">図 6-1M: 主要市区町村ヒートマップ</div>"#,
    );
    html.push_str(
        r#"<div class="text-[11px] text-slate-600 mt-2 border-t border-slate-800 pt-2">
        件数は CSV 内の出現回数（ハローワーク全体ではありません）。重複削除後の値で、件数=求人広告本数の概算値です。
    </div>"#,
    );
    html.push_str("</section>");
    html
}

/// 都市分類ラベル（可住地密度 人/km²）
fn classify_habitable_density(density: f64) -> &'static str {
    if density >= 5_000.0 {
        "都市型"
    } else if density >= 1_000.0 {
        "中間型"
    } else {
        "郊外型"
    }
}

/// 65+ 人口比率 を pyramid 行から計算（karte の calc_elderly_rate_from_pyramid と同等ロジック）
fn calc_aging_rate_from_pyramid(pyramid: &[Row]) -> f64 {
    if pyramid.is_empty() {
        return 0.0;
    }
    let mut total: i64 = 0;
    let mut elderly: i64 = 0;
    for r in pyramid {
        let grp = get_str_ref(r, "age_group");
        let m = get_i64(r, "male_count");
        let f = get_i64(r, "female_count");
        total += m + f;
        if grp == "65-74" || grp == "75+" || grp == "70-79" || grp == "80+" {
            elderly += m + f;
        } else if grp == "60-69" {
            elderly += (m + f) / 2;
        }
    }
    if total > 0 {
        (elderly as f64 / total as f64) * 100.0
    } else {
        0.0
    }
}

/// 案 #18 + D-4: 可住地密度 + 高齢化率 KPI カード
///
/// 単独 KPI では意味が薄いため、可住地密度には都市分類ラベルを併記し、
/// 高齢化率には全国（pyramid 全国合算が無いため代替値）との差分を併記する。
///
/// 必須注記:
/// - #18: 「可住地密度は地理特性。求人配信戦略との因果ではなく傾向参照」
/// - D-4: 「65 歳以上人口比率。労働人口希少性の参考指標」
fn render_geography_aging_section(ctx: &InsightContext) -> String {
    // データの可用性チェック
    let geo_density = ctx
        .ext_geography
        .first()
        .map(|r| {
            let d = get_f64(r, "habitable_density_per_km2");
            if d > 0.0 {
                d
            } else {
                let pop = get_i64(r, "total_population") as f64;
                let area = get_f64(r, "habitable_area_km2");
                if pop > 0.0 && area > 0.0 {
                    pop / area
                } else {
                    0.0
                }
            }
        })
        .unwrap_or(0.0);
    // 高齢化率: ext_population.aging_rate を優先、無ければ pyramid から再計算
    let aging_rate_from_pop = ctx
        .ext_population
        .first()
        .map(|r| get_f64(r, "aging_rate"))
        .unwrap_or(0.0);
    let aging_rate = if aging_rate_from_pop > 0.0 {
        aging_rate_from_pop
    } else {
        calc_aging_rate_from_pyramid(&ctx.ext_pyramid)
    };

    if geo_density <= 0.0 && aging_rate <= 0.0 {
        return String::new();
    }

    // 全国比較: 全国平均値（参考、定数）
    // 出典: 総務省 人口推計 2023 - 全国の高齢化率は約 29.0%
    const NATIONAL_AGING_RATE_PCT: f64 = 29.0;

    let mut html = String::with_capacity(1_500);
    html.push_str(
        r#"<section class="stat-card" data-testid="geo-aging-kpi"><h4 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-emerald-500 pl-2">地域特性 補足（地理 / 人口構成）</h4>"#,
    );
    html.push_str(r#"<div class="grid grid-cols-1 md:grid-cols-2 gap-3">"#);

    // 可住地密度カード
    if geo_density > 0.0 {
        let cls = classify_habitable_density(geo_density);
        let color = match cls {
            "都市型" => "text-pink-400",
            "中間型" => "text-amber-400",
            _ => "text-cyan-400",
        };
        write!(
            html,
            r#"<div class="p-3 bg-slate-800/50 rounded" data-testid="habitable-density-card">
                <div class="text-[10px] text-slate-500 mb-1">可住地密度</div>
                <div class="text-lg font-bold {color}">{val} 人/km<sup>2</sup></div>
                <div class="text-xs text-slate-400 mt-1" data-testid="city-class-label">分類: <span class="font-semibold {color}">{cls}</span></div>
                <div class="text-[10px] text-slate-500 mt-1">基準: 5000+ 都市型 / 1000+ 中間型 / それ未満 郊外型</div>
            </div>"#,
            val = format_number(geo_density.round() as i64),
            cls = cls,
            color = color,
        )
        .unwrap();
    }

    // 高齢化率カード
    if aging_rate > 0.0 {
        let diff = aging_rate - NATIONAL_AGING_RATE_PCT;
        let (sign, color) = if diff > 0.0 {
            ("+", "text-amber-400")
        } else if diff < 0.0 {
            ("", "text-emerald-400")
        } else {
            ("", "text-slate-300")
        };
        write!(
            html,
            r#"<div class="p-3 bg-slate-800/50 rounded" data-testid="aging-rate-card">
                <div class="text-[10px] text-slate-500 mb-1">高齢化率（65+ 人口比率）</div>
                <div class="text-lg font-bold {color}">{val:.1}%</div>
                <div class="text-xs text-slate-400 mt-1" data-testid="national-compare">全国 {nat:.0}% / 差分 <span class="{color}">{sign}{diff:.1}pt</span></div>
                <div class="text-[10px] text-slate-500 mt-1">労働人口希少性の参考指標</div>
            </div>"#,
            val = aging_rate,
            nat = NATIONAL_AGING_RATE_PCT,
            sign = sign,
            diff = diff,
            color = color,
        )
        .unwrap();
    }

    html.push_str("</div>");
    html.push_str(
        r#"<div class="text-[11px] text-slate-600 mt-3 border-t border-slate-800 pt-2">
        可住地密度は地理特性、求人配信戦略との因果ではなく傾向参照です。\
        高齢化率は 65 歳以上人口比率で労働人口希少性の参考指標です。\
        全国値は総務省 人口推計（2023）に基づく定数です。
    </div>"#,
    );
    html.push_str("</section>");
    html
}

/// 案 D-3: 産業別就業者構成 Top10
///
/// dominant 都道府県の v2_external_industry_structure から取得した
/// industry_code / industry_name / employees_total をベースに横バー形式で表示。
///
/// 必須注記: 「産業分類は国勢調査 2020 ベース。HW industry_raw と粒度が異なる可能性」
fn render_industry_structure_section(rows: &[Row], pref: &str) -> String {
    if rows.is_empty() {
        return String::new();
    }
    // 合計就業者数
    let total: i64 = rows.iter().map(|r| get_i64(r, "employees_total")).sum();
    if total <= 0 {
        return String::new();
    }

    let mut html = String::with_capacity(2_000);
    html.push_str(
        r#"<section class="stat-card" data-testid="industry-structure-section"><h4 class="text-sm font-semibold text-slate-200 mb-3 border-l-4 border-cyan-500 pl-2">地域の産業構成（就業者 Top 10）</h4>"#,
    );
    write!(
        html,
        r#"<div class="text-xs text-slate-400 mb-2">{pref} の産業別就業者数（国勢調査ベース）。CSV 求人地域に対する人材プールサイズの参考値です。</div>"#,
        pref = escape_html(pref)
    )
    .unwrap();

    // 横バーで上位 10
    html.push_str(r#"<div class="space-y-1.5" data-testid="industry-bars">"#);
    let max_v = rows
        .iter()
        .take(10)
        .map(|r| get_i64(r, "employees_total"))
        .max()
        .unwrap_or(1)
        .max(1);
    for r in rows.iter().take(10) {
        let name = get_str_ref(r, "industry_name");
        let emp = get_i64(r, "employees_total");
        if emp <= 0 {
            continue;
        }
        let pct = emp as f64 / total as f64 * 100.0;
        let width_pct = (emp as f64 / max_v as f64 * 100.0).clamp(0.0, 100.0);
        write!(
            html,
            r#"<div class="flex items-center gap-2 text-xs">
                <div class="w-32 text-slate-300 truncate" title="{name_attr}">{name}</div>
                <div class="flex-1 h-4 bg-slate-800 rounded overflow-hidden">
                    <div class="h-4 bg-cyan-600" style="width:{w:.1}%"></div>
                </div>
                <div class="w-24 text-right text-slate-400 tabular-nums">{val} ({pct:.1}%)</div>
            </div>"#,
            name = escape_html(name),
            name_attr = escape_html(name),
            w = width_pct,
            val = format_number(emp),
            pct = pct,
        )
        .unwrap();
    }
    html.push_str("</div>");

    html.push_str(
        r#"<div class="text-[11px] text-slate-600 mt-3 border-t border-slate-800 pt-2">
        産業分類は国勢調査 2020 ベース。HW industry_raw と粒度が異なる可能性があるため、HW 求人の業種比率と直接の対応は保証されません。\
        産業別就業者数と採用容易性に相関が見られる場合がありますが、職種・条件マッチングが本質的要因です。
    </div>"#,
    );
    html.push_str("</section>");
    html
}

#[cfg(test)]
mod fixb_tests {
    //! Fix-B (D-2 監査 Q1.1 / Q3.3) 逆証明テスト群
    //!
    //! 旧 Tab UI は posting_change_3m_pct / posting_change_1y_pct を市区町村テーブルの
    //! 列として表示していたため、同一都道府県内の muni 行で値が同一になる粒度詐称
    //! が発生していた。Fix-B で列を削除し、推移は都道府県カードに分離した。

    use super::*;
    use crate::handlers::types::VacancyRatePct;

    fn empty_ctx() -> InsightContext {
        // テスト用の最小 InsightContext。InsightContext は外部依存が多いため
        // ここでは Default が使えるかを確認しつつ、ベースラインで構築する。
        // 実 fixture は survey/report_html_qa_test.rs の mock_empty_insight_ctx を参照。
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
            // Impl-3: ライフスタイル特性 (P-1/P-2)
            ext_social_life: vec![],
            ext_internet_usage: vec![],
            ext_households: vec![],
            ext_vital: vec![],
            ext_labor_force: vec![],
            ext_medical_welfare: vec![],
            ext_education_facilities: vec![],
            ext_geography: vec![],
            ext_education: vec![],
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

    fn build_two_muni_same_pref() -> Vec<HwAreaEnrichment> {
        vec![
            HwAreaEnrichment {
                prefecture: "東京都".to_string(),
                municipality: "千代田区".to_string(),
                hw_posting_count: 100,
                posting_change_3m_pct: Some(12.3),
                posting_change_1y_pct: Some(8.5),
                vacancy_rate_pct: Some(VacancyRatePct::from_ratio(0.15)),
            },
            HwAreaEnrichment {
                prefecture: "東京都".to_string(),
                municipality: "中央区".to_string(),
                hw_posting_count: 80,
                posting_change_3m_pct: Some(12.3), // 同一県なので同じ値
                posting_change_1y_pct: Some(8.5),
                vacancy_rate_pct: Some(VacancyRatePct::from_ratio(0.15)),
            },
        ]
    }

    /// Tab UI のテーブルから「3ヶ月推移」「1年推移」列が消えていること
    #[test]
    fn fixb_tab_table_no_3m_or_1y_columns() {
        let ctx = empty_ctx();
        let enrichments = build_two_muni_same_pref();
        let html = render_hw_area_enrichment_section(&enrichments, &ctx);

        // テーブル本体だけを切り出し（pref カードと混同しないため）
        let table_html = html
            .split("data-testid=\"hw-area-enrichment-table\"")
            .nth(1)
            .unwrap_or("")
            .split("</table>")
            .next()
            .unwrap_or("");

        assert!(
            !table_html.contains("3ヶ月推移"),
            "市区町村テーブルから「3ヶ月推移」列が削除されているべき (D-2 Q1.1)"
        );
        assert!(
            !table_html.contains("1年推移"),
            "市区町村テーブルから「1年推移」列が削除されているべき (D-2 Q1.1)"
        );
    }

    /// 都道府県別の推移カードが分離表示されている
    #[test]
    fn fixb_pref_trend_separated_into_card() {
        let ctx = empty_ctx();
        let enrichments = build_two_muni_same_pref();
        let html = render_hw_area_enrichment_section(&enrichments, &ctx);

        assert!(
            html.contains("hw-pref-trend-card"),
            "都道府県別推移カードが必須 (D-2 Q1.1 構造的解決)"
        );
        assert!(
            html.contains("都道府県粒度の参考値"),
            "粒度を明示するラベルが必須"
        );
        assert!(
            html.contains("市区町村別の差分は反映していません"),
            "市区町村粒度詐称を防ぐ注記が必須"
        );
    }

    /// 推移データなし（None）の muni のみの場合は pref カード自体が出ない
    #[test]
    fn fixb_pref_card_hidden_when_no_change_data() {
        let no_change = vec![HwAreaEnrichment {
            prefecture: "東京都".to_string(),
            municipality: "千代田区".to_string(),
            hw_posting_count: 100,
            posting_change_3m_pct: None,
            posting_change_1y_pct: None,
            vacancy_rate_pct: None,
        }];
        let summary = build_pref_change_summary(&no_change);
        assert!(
            summary.is_empty(),
            "推移データなしのとき pref カードは出ないべき"
        );
    }

    /// 同一県の 2 muni を入力すると pref カードは 1 つだけ
    #[test]
    fn fixb_pref_card_dedupes_within_same_prefecture() {
        let enrichments = build_two_muni_same_pref();
        let summary = build_pref_change_summary(&enrichments);

        // 「東京都」が pref ラベルとして 1 回だけ出る（カードラベル）
        // grid 内の "<div class=\"text-slate-300 font-medium\">東京都</div>" の出現数
        let pref_label_count = summary
            .matches(r#"<div class="text-slate-300 font-medium">東京都</div>"#)
            .count();
        assert_eq!(
            pref_label_count, 1,
            "同一県の複数 muni でも pref カードは 1 つに統合されるべき"
        );
    }

    /// HW 限定スコープと因果非主張の注記が必ず含まれる
    #[test]
    fn fixb_section_has_hw_scope_and_no_causation_note() {
        let ctx = empty_ctx();
        let enrichments = build_two_muni_same_pref();
        let html = render_hw_area_enrichment_section(&enrichments, &ctx);

        assert!(
            html.contains("ハローワーク掲載求人のみ")
                || html.contains("HW 掲載求人のみ")
                || html.contains("ハローワーク掲載"),
            "HW 限定スコープの注記が必須 (feedback_hw_data_scope.md)"
        );
        assert!(
            html.contains("因果関係を主張するものではありません"),
            "因果非主張の注記が必須 (feedback_correlation_not_causation.md)"
        );
    }
}

// =====================================================================
// Impl-1 (2026-04-26): 媒体分析データ活用 #6 / #18 / D-3 / D-4 contract tests
//
// 設計:
// - 各案で「具体値検証」(feedback_test_data_validation / reverse_proof_tests 準拠)
// - 必須注記・図番号・data-testid を逆証明形式で検証
// - 各案 2 件以上、合計 8 件以上
// =====================================================================
#[cfg(test)]
mod impl1_contract_tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    fn make_row(pairs: &[(&str, serde_json::Value)]) -> Row {
        let mut m: Row = HashMap::new();
        for (k, v) in pairs {
            m.insert((*k).to_string(), v.clone());
        }
        m
    }

    fn ctx_with_geography_and_pyramid() -> InsightContext {
        // 東京都千代田区相当: 高密度 + 高齢化中位
        let geo_row = make_row(&[
            ("habitable_density_per_km2", json!(5_200.0)),
            ("habitable_area_km2", json!(11.66)),
            ("total_population", json!(60_000_i64)),
        ]);
        // ext_population.aging_rate = 0 → pyramid から再計算
        // pyramid: 0-14: 100, 15-64: 600, 65-74: 200, 75+: 100 → 65+ = 300/1000 = 30.0%
        let p1 = make_row(&[
            ("age_group", json!("0-14")),
            ("male_count", json!(50_i64)),
            ("female_count", json!(50_i64)),
        ]);
        let p2 = make_row(&[
            ("age_group", json!("15-64")),
            ("male_count", json!(300_i64)),
            ("female_count", json!(300_i64)),
        ]);
        let p3 = make_row(&[
            ("age_group", json!("65-74")),
            ("male_count", json!(100_i64)),
            ("female_count", json!(100_i64)),
        ]);
        let p4 = make_row(&[
            ("age_group", json!("75+")),
            ("male_count", json!(50_i64)),
            ("female_count", json!(50_i64)),
        ]);

        InsightContext {
            ext_geography: vec![geo_row],
            ext_pyramid: vec![p1, p2, p3, p4],
            ..empty_ctx_for_impl1()
        }
    }

    fn empty_ctx_for_impl1() -> InsightContext {
        // empty_ctx() を fixb_tests から流用したいが private のため再構築
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
            ext_social_life: vec![],
            ext_internet_usage: vec![],
            ext_households: vec![],
            ext_vital: vec![],
            ext_labor_force: vec![],
            ext_medical_welfare: vec![],
            ext_education_facilities: vec![],
            ext_geography: vec![],
            ext_education: vec![],
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

    // ------ 案 #6: 主要地域 6 軸ベンチマーク レーダーチャート ------

    #[test]
    fn impl1_radar_section_emits_6axis_data_with_3_prefs() {
        let mk_score_row =
            |emp: &str, sal: f64, tight: f64, fresh: f64, wa: f64, wage: f64, div: f64| -> Row {
                make_row(&[
                    ("emp_group", json!(emp)),
                    ("salary_competitiveness", json!(sal)),
                    ("job_market_tightness", json!(tight)),
                    ("posting_freshness", json!(fresh)),
                    ("working_age_ratio", json!(wa)),
                    ("wage_compliance", json!(wage)),
                    ("industry_diversity", json!(div)),
                ])
            };
        let top3 = vec![
            (
                "東京都".to_string(),
                mk_score_row("正社員", 0.85, 0.78, 0.72, 0.92, 0.95, 0.88),
            ),
            (
                "神奈川県".to_string(),
                mk_score_row("正社員", 0.70, 0.65, 0.60, 0.85, 0.90, 0.75),
            ),
            (
                "千葉県".to_string(),
                mk_score_row("正社員", 0.60, 0.55, 0.50, 0.80, 0.88, 0.65),
            ),
        ];
        let html = render_region_benchmark_radar_section(&top3);

        // セクションが描画される
        assert!(
            html.contains("data-testid=\"region-benchmark-radar\""),
            "radar section data-testid 必須"
        );
        // 図番号
        assert!(html.contains("図 6-2"), "図 6-2 番号必須");
        // 6 軸ラベル
        for axis in &[
            "給与競争力",
            "求人量",
            "充足度",
            "人口動態",
            "賃金遵守",
            "産業多様性",
        ] {
            assert!(html.contains(axis), "軸ラベル {} が必須", axis);
        }
        // 3 県名すべて表示
        assert!(html.contains("東京都"), "1 番手県");
        assert!(html.contains("神奈川県"), "2 番手県");
        assert!(html.contains("千葉県"), "3 番手県");

        // ECharts data-chart-config に radar series + 6 軸 indicator
        assert!(
            html.contains("data-chart-config="),
            "ECharts config 属性必須"
        );
        assert!(html.contains("\"type\":\"radar\""), "radar series type");
        assert!(
            html.contains("\"max\":100"),
            "indicator max=100 (0-100 正規化)"
        );

        // 必須注記 (memory feedback_correlation_not_causation 準拠)
        assert!(
            html.contains("地域間の戦略的優劣ではなく特性の違い"),
            "因果ではなく特性の違いの注記必須"
        );
        assert!(
            html.contains("因果関係や採用成功の保証ではありません"),
            "因果非主張の注記必須"
        );

        // 具体値検証: 東京都の給与競争力 0.85 → 85 (0-100 変換)
        // 凡例フォールバックに「給与競争力=85」が出る
        assert!(
            html.contains("給与競争力=85"),
            "東京都 salary_competitiveness 0.85→85 表示"
        );
        assert!(
            html.contains("人口動態=92"),
            "東京都 working_age_ratio 0.92→92 表示"
        );
    }

    #[test]
    fn impl1_radar_section_hidden_when_top3_empty() {
        let html = render_region_benchmark_radar_section(&[]);
        assert!(html.is_empty(), "top3 空時は radar セクション非表示");
    }

    #[test]
    fn impl1_radar_score_clamps_invalid_values() {
        // 範囲外の値が入力されても 0-100 に clamp されることを保証
        assert_eq!(radar_score_to_pct(0.5), 50.0);
        assert_eq!(radar_score_to_pct(0.0), 0.0);
        assert_eq!(radar_score_to_pct(1.0), 100.0);
        assert_eq!(radar_score_to_pct(1.5), 100.0, "上限 clamp");
        assert_eq!(radar_score_to_pct(-0.1), 0.0, "下限 clamp");
        assert_eq!(radar_score_to_pct(f64::NAN), 0.0, "NaN は 0 fallback");
    }

    // ------ 案 #18: 可住地密度 + 都市分類 ------

    #[test]
    fn impl1_habitable_density_kpi_with_city_class() {
        let ctx = ctx_with_geography_and_pyramid();
        let html = render_geography_aging_section(&ctx);

        assert!(
            html.contains("data-testid=\"geo-aging-kpi\""),
            "geo-aging KPI セクション必須"
        );
        assert!(
            html.contains("data-testid=\"habitable-density-card\""),
            "可住地密度カード必須"
        );
        assert!(
            html.contains("data-testid=\"city-class-label\""),
            "都市分類ラベル必須"
        );
        // 具体値: 5200 → "5,200" + 都市型 (>=5000)
        assert!(html.contains("5,200"), "可住地密度 5200 人/km² 表示");
        assert!(html.contains("都市型"), "5200 ≥ 5000 ⇒ 都市型分類");
        // 必須注記
        assert!(
            html.contains("可住地密度は地理特性") && html.contains("傾向参照"),
            "#18 必須注記: 地理特性、因果ではなく傾向参照"
        );
    }

    #[test]
    fn impl1_habitable_density_classification_thresholds() {
        // 境界値の逆証明: 5000 / 1000 をまたぐと分類が変わる
        assert_eq!(classify_habitable_density(5_000.0), "都市型");
        assert_eq!(classify_habitable_density(4_999.0), "中間型");
        assert_eq!(classify_habitable_density(1_000.0), "中間型");
        assert_eq!(classify_habitable_density(999.9), "郊外型");
        assert_eq!(classify_habitable_density(0.0), "郊外型");
    }

    // ------ 案 D-3: 産業別就業者構成 ------

    #[test]
    fn impl1_industry_section_top10_with_pct() {
        let mk = |name: &str, emp: i64| -> Row {
            make_row(&[
                ("industry_code", json!("XX")),
                ("industry_name", json!(name)),
                ("employees_total", json!(emp)),
            ])
        };
        let rows = vec![
            mk("医療,福祉", 200_000),
            mk("製造業", 150_000),
            mk("卸売業,小売業", 120_000),
            mk("建設業", 80_000),
            mk("サービス業", 60_000),
        ];
        let html = render_industry_structure_section(&rows, "東京都");

        // セクションテストID
        assert!(
            html.contains("data-testid=\"industry-structure-section\""),
            "産業構成セクション ID 必須"
        );
        assert!(
            html.contains("data-testid=\"industry-bars\""),
            "産業バーリスト必須"
        );
        // 上位 5 産業すべて表示
        for n in &[
            "医療,福祉",
            "製造業",
            "卸売業,小売業",
            "建設業",
            "サービス業",
        ] {
            assert!(html.contains(n), "産業名 {} 必須", n);
        }
        // 具体値: 200,000 (35.1%) - 200000/610000=32.79% → 構成比表示
        // total = 610000、医療福祉 = 200000 → 32.8%
        assert!(
            html.contains("200,000"),
            "医療福祉 employees_total 200000 表示"
        );
        // 都道府県名表示
        assert!(html.contains("東京都"), "対象都道府県表示");
        // 必須注記
        assert!(
            html.contains("国勢調査 2020 ベース"),
            "D-3 必須注記: 国勢調査 2020 ベース"
        );
        assert!(
            html.contains("HW industry_raw と粒度が異なる可能性"),
            "粒度差異の注記必須"
        );
        assert!(
            html.contains("職種・条件マッチングが本質的要因"),
            "因果ではない旨"
        );
    }

    #[test]
    fn impl1_industry_section_hidden_when_empty_or_zero_total() {
        // 空入力は非表示
        assert_eq!(
            render_industry_structure_section(&[], "東京都"),
            String::new()
        );
        // total 0 (employees_total 全行 0) も非表示（fail-soft 逆証明）
        let zero_rows = vec![make_row(&[
            ("industry_name", json!("---")),
            ("employees_total", json!(0_i64)),
        ])];
        assert_eq!(
            render_industry_structure_section(&zero_rows, "東京都"),
            String::new()
        );
    }

    // ------ 案 D-4: 高齢化率 KPI ------

    #[test]
    fn impl1_aging_rate_kpi_with_national_compare() {
        let ctx = ctx_with_geography_and_pyramid();
        let html = render_geography_aging_section(&ctx);

        assert!(
            html.contains("data-testid=\"aging-rate-card\""),
            "aging KPI カード必須"
        );
        assert!(
            html.contains("data-testid=\"national-compare\""),
            "全国比較ラベル必須"
        );
        // 具体値検証: pyramid から (200+100)/1000 = 30.0%
        assert!(html.contains("30.0%"), "計算済み高齢化率 30.0% 表示");
        // 全国 29% との差分 +1.0pt 表示
        assert!(html.contains("全国 29%"), "全国 29% 表示");
        assert!(
            html.contains("+1.0pt"),
            "30 - 29 = +1.0pt 差分（具体値検証）"
        );
        // 必須注記
        assert!(
            html.contains("65 歳以上人口比率") || html.contains("65+ 人口比率"),
            "D-4 必須注記: 65 歳以上 = 高齢化率定義"
        );
        assert!(
            html.contains("労働人口希少性の参考指標"),
            "D-4 必須注記: 参考指標"
        );
    }

    #[test]
    fn impl1_aging_rate_calc_specific_values_reverse_proof() {
        // 逆証明: 0-14:100, 15-64:600, 65-74:200, 75+:100 → elderly = 300, total = 1000
        let p1 = make_row(&[
            ("age_group", json!("0-14")),
            ("male_count", json!(50_i64)),
            ("female_count", json!(50_i64)),
        ]);
        let p2 = make_row(&[
            ("age_group", json!("15-64")),
            ("male_count", json!(300_i64)),
            ("female_count", json!(300_i64)),
        ]);
        let p3 = make_row(&[
            ("age_group", json!("65-74")),
            ("male_count", json!(100_i64)),
            ("female_count", json!(100_i64)),
        ]);
        let p4 = make_row(&[
            ("age_group", json!("75+")),
            ("male_count", json!(50_i64)),
            ("female_count", json!(50_i64)),
        ]);
        let rate = calc_aging_rate_from_pyramid(&[p1, p2, p3, p4]);
        assert!(
            (rate - 30.0).abs() < 0.01,
            "30.0% であるべきだが {} だった",
            rate
        );

        // 空入力 → 0.0
        assert_eq!(calc_aging_rate_from_pyramid(&[]), 0.0);
    }

    #[test]
    fn impl1_geo_aging_section_hidden_when_no_data() {
        let ctx = empty_ctx_for_impl1();
        let html = render_geography_aging_section(&ctx);
        assert!(
            html.is_empty(),
            "geography / pyramid 全空時はセクション非表示 (fail-soft 逆証明)"
        );
    }

    // ------ 共通: SurveyExtensionData Default + render_integration 互換 ------

    #[test]
    fn impl1_survey_extension_data_default_is_empty() {
        let ext = SurveyExtensionData::default();
        assert!(ext.top3_region_benchmark.is_empty(), "Default は空の top3");
        assert!(
            ext.industry_structure_top10.is_empty(),
            "Default は空の industry"
        );
    }

    #[test]
    fn impl1_render_integration_legacy_signature_still_works() {
        // 旧 render_integration が SurveyExtensionData::default() を内部で使い、
        // 既存挙動を保つことを保証（後方互換の逆証明）
        let ctx = empty_ctx_for_impl1();
        let html = render_integration("東京都", "千代田区", &[], &ctx, &[], &[]);
        // HW統合分析 見出しは出る
        assert!(html.contains("HW統合分析"), "見出しは旧と同じ");
        // 拡張セクションは出ない（データなし、ext default empty）
        assert!(
            !html.contains("data-testid=\"region-benchmark-radar\""),
            "ext default 時は radar 出ない"
        );
        assert!(
            !html.contains("data-testid=\"industry-structure-section\""),
            "ext default 時は industry 出ない"
        );
    }

    // ====================================================================
    // 2026-04-26 Granularity: 市区町村粒度 contract tests (10 件以上)
    // ユーザー指摘「都道府県単位は参考にならない」に対応した
    // 市区町村レーダー / ヒートマップ / 注記の逆証明テスト群
    // ====================================================================

    /// 逆証明: 市区町村ヒートマップ section は top_municipalities が空ならば描画しない
    #[test]
    fn granularity_heatmap_hidden_when_empty() {
        let html = render_municipality_heatmap_section(&[]);
        assert!(html.is_empty(), "空 Vec ではヒートマップ非表示");
    }

    /// 逆証明: 市区町村ヒートマップ section に CSV 件数 Top N が表示される
    #[test]
    fn granularity_heatmap_shows_top_municipalities_with_counts() {
        let top = vec![
            ("東京都".to_string(), "千代田区".to_string(), 100),
            ("東京都".to_string(), "新宿区".to_string(), 80),
            ("神奈川県".to_string(), "横浜市".to_string(), 60),
        ];
        let html = render_municipality_heatmap_section(&top);

        assert!(
            html.contains("data-testid=\"municipality-heatmap\""),
            "ヒートマップ data-testid 必須"
        );
        assert!(
            html.contains("data-testid=\"muni-heatmap-grid\""),
            "ヒートマップグリッド data-testid 必須"
        );
        // 全市区町村名表示
        assert!(html.contains("千代田区"), "千代田区が表示される");
        assert!(html.contains("新宿区"), "新宿区が表示される");
        assert!(html.contains("横浜市"), "横浜市が表示される");
        // 件数表示
        assert!(html.contains("100"), "件数 100 表示");
        assert!(html.contains("80"), "件数 80 表示");
        assert!(html.contains("60"), "件数 60 表示");
        // 図番号
        assert!(html.contains("図 6-1M"), "図 6-1M (市区町村ヒートマップ)");
    }

    /// 逆証明: 市区町村ヒートマップは件数が多い地域ほど濃い色 (alpha 値で判定)
    #[test]
    fn granularity_heatmap_color_intensity_by_count() {
        let top = vec![
            ("東京都".to_string(), "高件数市".to_string(), 1000),
            ("東京都".to_string(), "低件数市".to_string(), 50),
        ];
        let html = render_municipality_heatmap_section(&top);
        // 1000 件は alpha = 0.15 + 0.7 = 0.85
        assert!(
            html.contains("rgba(244,63,94,0.85)") || html.contains("rgba(244,63,94,0.84)"),
            "高件数の alpha 0.85 付近"
        );
        // 50 件は alpha = 0.15 + 0.7 * (50/1000) = 0.185
        assert!(
            html.contains("rgba(244,63,94,0.18)") || html.contains("rgba(244,63,94,0.19)"),
            "低件数の alpha 0.185 付近"
        );
    }

    /// 逆証明: 市区町村レーダー section は top3 が空なら非表示
    #[test]
    fn granularity_municipality_radar_hidden_when_empty() {
        let html = render_municipality_benchmark_radar_section(&[]);
        assert!(html.is_empty(), "空 Vec で市区町村レーダー非表示");
    }

    /// 逆証明: 市区町村レーダーが 6 軸データを ECharts 形式で描画
    #[test]
    fn granularity_municipality_radar_emits_6axis_data() {
        let mk_row = |sal: f64, tight: f64, fresh: f64, wa: f64, wage: f64, div: f64| {
            make_row(&[
                ("emp_group", json!("正社員")),
                ("salary_competitiveness", json!(sal)),
                ("job_market_tightness", json!(tight)),
                ("posting_freshness", json!(fresh)),
                ("working_age_ratio", json!(wa)),
                ("wage_compliance", json!(wage)),
                ("industry_diversity", json!(div)),
            ])
        };
        let top3 = vec![
            (
                "東京都 千代田区".to_string(),
                mk_row(0.85, 0.78, 0.72, 0.92, 0.95, 0.88),
                false,
            ),
            (
                "東京都 新宿区".to_string(),
                mk_row(0.70, 0.65, 0.60, 0.85, 0.90, 0.75),
                false,
            ),
            (
                "神奈川県 横浜市".to_string(),
                mk_row(0.60, 0.55, 0.50, 0.80, 0.88, 0.65),
                false,
            ),
        ];
        let html = render_municipality_benchmark_radar_section(&top3);

        assert!(
            html.contains("data-testid=\"municipality-benchmark-radar\""),
            "radar data-testid 必須"
        );
        assert!(
            html.contains("市区町村粒度"),
            "市区町村粒度であることを明示"
        );
        // 6 軸
        for axis in &[
            "給与競争力",
            "求人量",
            "充足度",
            "人口動態",
            "賃金遵守",
            "産業多様性",
        ] {
            assert!(html.contains(axis), "軸 {} 必須", axis);
        }
        // 3 市区町村のラベル表示
        assert!(html.contains("千代田区"), "千代田区ラベル");
        assert!(html.contains("新宿区"), "新宿区ラベル");
        assert!(html.contains("横浜市"), "横浜市ラベル");
        // ECharts 識別属性
        assert!(
            html.contains("data-chart-config"),
            "ECharts data-chart-config"
        );
        assert!(html.contains("\"type\":\"radar\""), "radar series type");
        // 図番号 (M suffix で都道府県版と区別)
        assert!(html.contains("図 6-2M"), "図 6-2M (市区町村粒度) 必須");
    }

    /// 逆証明: fallback 注記が表示される (一部地域が県値参考の場合)
    #[test]
    fn granularity_municipality_radar_shows_fallback_note() {
        let mk_row = || {
            make_row(&[
                ("emp_group", json!("正社員")),
                ("salary_competitiveness", json!(0.5)),
            ])
        };
        let top3 = vec![
            ("東京都 千代田区".to_string(), mk_row(), false),
            ("東京都 新宿区 (県値参考)".to_string(), mk_row(), true),
        ];
        let html = render_municipality_benchmark_radar_section(&top3);

        assert!(
            html.contains("一部地域は市区町村粒度データなし") || html.contains("県値参考"),
            "fallback 注記または県値参考ラベルが必要"
        );
        assert!(
            html.contains("[県値参考]"),
            "凡例に [県値参考] マーカー必須"
        );
    }

    /// 逆証明: 全件 fallback ではない場合は注記なし
    #[test]
    fn granularity_municipality_radar_no_fallback_note_when_all_municipal() {
        let mk_row = || {
            make_row(&[
                ("emp_group", json!("正社員")),
                ("salary_competitiveness", json!(0.5)),
            ])
        };
        let top3 = vec![
            ("東京都 千代田区".to_string(), mk_row(), false),
            ("東京都 新宿区".to_string(), mk_row(), false),
        ];
        let html = render_municipality_benchmark_radar_section(&top3);
        assert!(
            !html.contains("一部地域は市区町村粒度データなし"),
            "全件市区町村粒度なら fallback 注記出ない"
        );
    }

    /// 逆証明: SurveyExtensionData に top3_municipality_benchmark がある場合、
    /// render_integration_with_ext は市区町村レーダーを描画
    #[test]
    fn granularity_extension_data_municipality_benchmark_renders() {
        let ctx = empty_ctx_for_impl1();
        let mut ext = SurveyExtensionData::default();
        ext.top3_municipality_benchmark = vec![(
            "東京都 千代田区".to_string(),
            make_row(&[
                ("emp_group", json!("正社員")),
                ("salary_competitiveness", json!(0.85)),
            ]),
            false,
        )];
        let html = render_integration_with_ext("東京都", "千代田区", &[], &ctx, &[], &[], &ext);
        assert!(
            html.contains("data-testid=\"municipality-benchmark-radar\""),
            "市区町村レーダーが描画される"
        );
    }

    /// 逆証明: top3_municipality_benchmark がある時、都道府県レーダーは抑制される (重複回避)
    #[test]
    fn granularity_municipality_benchmark_suppresses_prefecture_radar() {
        let ctx = empty_ctx_for_impl1();
        let mut ext = SurveyExtensionData::default();
        // 両方ある状態
        ext.top3_municipality_benchmark = vec![(
            "東京都 千代田区".to_string(),
            make_row(&[
                ("emp_group", json!("正社員")),
                ("salary_competitiveness", json!(0.5)),
            ]),
            false,
        )];
        ext.top3_region_benchmark = vec![(
            "東京都".to_string(),
            make_row(&[
                ("emp_group", json!("正社員")),
                ("salary_competitiveness", json!(0.5)),
            ]),
        )];
        let html = render_integration_with_ext("東京都", "千代田区", &[], &ctx, &[], &[], &ext);
        // 市区町村は出る
        assert!(
            html.contains("data-testid=\"municipality-benchmark-radar\""),
            "市区町村レーダーは出る"
        );
        // 都道府県は出ない (冗長のため)
        assert!(
            !html.contains("data-testid=\"region-benchmark-radar\""),
            "市区町村ある時は都道府県レーダー抑制 (冗長回避)"
        );
    }

    /// 逆証明: top_municipalities_heatmap がある場合、ヒートマップ section が描画される
    #[test]
    fn granularity_extension_data_heatmap_renders() {
        let ctx = empty_ctx_for_impl1();
        let mut ext = SurveyExtensionData::default();
        ext.top_municipalities_heatmap = vec![
            ("東京都".to_string(), "千代田区".to_string(), 100),
            ("神奈川県".to_string(), "横浜市".to_string(), 80),
        ];
        let html = render_integration_with_ext("東京都", "千代田区", &[], &ctx, &[], &[], &ext);
        assert!(
            html.contains("data-testid=\"municipality-heatmap\""),
            "ext.top_municipalities_heatmap で市区町村ヒートマップ描画"
        );
        assert!(html.contains("千代田区"));
        assert!(html.contains("横浜市"));
    }

    /// 逆証明: 都道府県名末尾 "都/道/府/県" が省略されてコンパクト表示される
    #[test]
    fn granularity_heatmap_pref_short_form() {
        let top = vec![
            ("北海道".to_string(), "札幌市".to_string(), 50),
            ("東京都".to_string(), "千代田区".to_string(), 100),
            ("京都府".to_string(), "京都市".to_string(), 30),
            ("沖縄県".to_string(), "那覇市".to_string(), 20),
        ];
        let html = render_municipality_heatmap_section(&top);
        // pref_short は "東京" "京都" "北海" "沖縄" など
        // ※ "京都府京都市" の "京都" は短縮された都道府県名と市名で同じになるが、
        //   独立した表示要素のため div で区切られる
        assert!(html.contains(">東京<"), "東京 (短縮) 表示");
        assert!(html.contains(">沖縄<"), "沖縄 (短縮) 表示");
    }

    /// 逆証明: SurveyExtensionData に新フィールドが追加され、Default が空 Vec
    #[test]
    fn granularity_extension_data_defaults_empty() {
        let ext = SurveyExtensionData::default();
        assert!(ext.top3_municipality_benchmark.is_empty(), "Default は空");
        assert!(ext.top_municipalities_heatmap.is_empty(), "Default は空");
    }
}
