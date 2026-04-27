//! 分割: report_html/region.rs (物理移動・内容変更なし)

#![allow(unused_imports, dead_code)]

use super::super::super::company::fetch::NearbyCompany;
use super::super::super::helpers::{escape_html, format_number, get_f64, get_str_ref, Row};
use super::super::super::insight::fetch::InsightContext;
use super::super::aggregator::{
    CompanyAgg, EmpTypeSalary, ScatterPoint, SurveyAggregation, TagSalaryAgg,
};
use super::super::hw_enrichment::HwAreaEnrichment;
use super::super::job_seeker::JobSeekerAnalysis;
use serde_json::json;

use super::helpers::*;

pub(super) fn render_section_region(html: &mut String, agg: &SurveyAggregation) {
    if agg.by_prefecture.is_empty() {
        return;
    }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>地域分析（都道府県）</h2>\n");
    // So What 行: 件数の多い都道府県と割合を 1 行で提示
    if let Some((top_pref, top_count)) = agg.by_prefecture.first() {
        let pct = if agg.total_count > 0 {
            *top_count as f64 / agg.total_count as f64 * 100.0
        } else {
            0.0
        };
        html.push_str(&format!(
            "<p class=\"section-sowhat\">\u{203B} 件数が最も多いのは「{}」で全体の {:.1}%（件数の多い順に整理）。</p>\n",
            escape_html(top_pref),
            pct
        ));
    }
    html.push_str(
        "<p class=\"section-xref\">関連: Section 7（市区町村）/ Section 8（最低賃金）</p>\n",
    );

    render_figure_caption(html, "表 6-1", "都道府県別 求人件数 Top 10");
    html.push_str("<table class=\"sortable-table zebra\">\n<thead><tr><th>#</th><th>都道府県</th><th style=\"text-align:right\">件数</th><th style=\"text-align:right\">割合</th></tr></thead>\n<tbody>\n");
    let total = agg.total_count.max(1);
    for (i, (pref, count)) in agg.by_prefecture.iter().take(10).enumerate() {
        let pct = *count as f64 / total as f64 * 100.0;
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{:.1}%</td></tr>\n",
            i + 1,
            escape_html(pref),
            format_number(*count as i64),
            pct,
        ));
    }
    html.push_str("</tbody></table>\n");

    // 残りの都道府県数を注記
    if agg.by_prefecture.len() > 10 {
        html.push_str(&format!(
            "<p class=\"note\">※ 他{}都道府県のデータは省略しています</p>\n",
            agg.by_prefecture.len() - 10
        ));
    }

    // 簡易ヒートマップ（Top 10 都道府県を 4 段階の濃度で可視化）
    if let Some((_, top_count)) = agg.by_prefecture.first() {
        let max_c = (*top_count as f64).max(1.0);
        render_figure_caption(
            html,
            "図 6-1",
            "都道府県別 求人件数ヒートマップ（Top 10、濃度=件数）",
        );
        html.push_str(
            "<div class=\"heatmap-grid\" role=\"img\" aria-label=\"都道府県別件数ヒートマップ\">\n",
        );
        for (pref, count) in agg.by_prefecture.iter().take(10) {
            let ratio = *count as f64 / max_c;
            let cls = if ratio >= 0.75 {
                "h-4"
            } else if ratio >= 0.5 {
                "h-3"
            } else if ratio >= 0.25 {
                "h-2"
            } else {
                "h-1"
            };
            html.push_str(&format!(
                "<div class=\"heatmap-cell {}\"><div style=\"font-weight:700;\">{}</div>\
                 <div style=\"font-size:8pt;\">{}件</div></div>\n",
                cls,
                escape_html(pref),
                format_number(*count as i64),
            ));
        }
        // 余白を埋める（10 セル未満の場合）
        let placeholder_count = 10usize.saturating_sub(agg.by_prefecture.len());
        for _ in 0..placeholder_count {
            html.push_str("<div class=\"heatmap-cell h-empty\">-</div>\n");
        }
        html.push_str("</div>\n");
        html.push_str(
            "<div class=\"heatmap-legend\">濃度: \
            <span class=\"swatch\" style=\"background:#dbeafe\"></span>少 \
            <span class=\"swatch\" style=\"background:#93c5fd\"></span> \
            <span class=\"swatch\" style=\"background:#3b82f6\"></span> \
            <span class=\"swatch\" style=\"background:#1e40af\"></span>多</div>\n",
        );
        render_read_hint(
            html,
            "色の濃い県ほど求人件数が多く、当 CSV データのカバレッジ重心です。色が薄い県は本レポートでの統計信頼性が下がる点に留意してください。",
        );
    }

    render_section_bridge(
        html,
        "次セクションでは、都道府県を市区町村レベルに掘り下げ、給与水準を併せて確認します。",
    );

    html.push_str("</div>\n");
}

// =====================================================================
// Impl-1 (2026-04-26): 媒体分析データ活用 #18 / D-3 / D-4 印刷版
// 配置: Section 6 (都道府県) と Section 7 (市区町村) の間に
//       「地域特性 補足」を追加。データソース: InsightContext のみで完結。
// =====================================================================

const REGION_EXTRAS_NATIONAL_AGING_PCT: f64 = 29.0;

fn classify_habitable_density(density: f64) -> &'static str {
    if density >= 5_000.0 {
        "都市型"
    } else if density >= 1_000.0 {
        "中間型"
    } else {
        "郊外型"
    }
}

fn calc_aging_rate_from_pyramid(pyramid: &[Row]) -> f64 {
    if pyramid.is_empty() {
        return 0.0;
    }
    let mut total: i64 = 0;
    let mut elderly: i64 = 0;
    for r in pyramid {
        let grp = get_str_ref(r, "age_group");
        let m = get_i64_local(r, "male_count");
        let f = get_i64_local(r, "female_count");
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

fn get_i64_local(row: &Row, key: &str) -> i64 {
    super::super::super::helpers::get_i64(row, key)
}

/// 案 #18 + D-4: 可住地密度 + 高齢化率 KPI セクション（印刷版）
///
/// 必須注記:
/// - #18: 「可住地密度は地理特性。求人配信戦略との因果ではなく傾向参照」
/// - D-4: 「65 歳以上人口比率。労働人口希少性の参考指標」
pub(super) fn render_section_region_extras(html: &mut String, ctx: &InsightContext) {
    // 可住地密度
    let geo_density = ctx
        .ext_geography
        .first()
        .map(|r| {
            let d = get_f64(r, "habitable_density_per_km2");
            if d > 0.0 {
                d
            } else {
                let pop = get_i64_local(r, "total_population") as f64;
                let area = get_f64(r, "habitable_area_km2");
                if pop > 0.0 && area > 0.0 {
                    pop / area
                } else {
                    0.0
                }
            }
        })
        .unwrap_or(0.0);

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
        return;
    }

    html.push_str("<div class=\"section\" data-testid=\"region-extras-section\">\n");
    html.push_str("<h2>地域特性 補足（地理 / 人口構成）</h2>\n");

    render_figure_caption(html, "図 6-3", "可住地密度・高齢化率 KPI（参考指標）");

    html.push_str("<div class=\"kpi-grid\">\n");

    // 可住地密度
    if geo_density > 0.0 {
        let cls = classify_habitable_density(geo_density);
        let val_text = format!("{} 人/km²", format_number(geo_density.round() as i64));
        let compare = format!("分類: {}（基準: 5000+ 都市 / 1000+ 中間 / 未満 郊外）", cls);
        render_kpi_card_v2(html, "", "可住地密度", &val_text, "", &compare, "", cls);
    }

    // 高齢化率
    if aging_rate > 0.0 {
        let diff = aging_rate - REGION_EXTRAS_NATIONAL_AGING_PCT;
        let sign = if diff > 0.0 { "+" } else { "" };
        let compare = format!(
            "全国 {:.0}% / 差分 {}{:.1}pt",
            REGION_EXTRAS_NATIONAL_AGING_PCT, sign, diff
        );
        let status = if aging_rate >= 35.0 {
            "warn"
        } else if aging_rate <= 22.0 {
            "good"
        } else {
            ""
        };
        render_kpi_card_v2(
            html,
            "",
            "高齢化率（65+ 人口比率）",
            &format!("{:.1}", aging_rate),
            "%",
            &compare,
            status,
            "",
        );
    }

    html.push_str("</div>\n");

    render_read_hint(
        html,
        "可住地密度は地理特性で、都市型は媒体配信を狭域集中、郊外型は広域配信が向く傾向があります。\
         高齢化率は労働人口希少性の参考指標で、全国平均（29%）との差分で地域特性を把握します。\
         どちらも因果ではなく傾向参照値であり、職種・条件マッチングが本質的要因です。",
    );

    html.push_str("</div>\n");
}

/// 案 D-3: 産業別就業者構成 Top10 セクション（印刷版・データ受け渡し型）
///
/// `industry_rows` は handlers 側で fetch_industry_structure から得た上位 10 行。
/// 印刷版 render_survey_report_page の現行 API では受け取れないため、
/// 直接呼び出しは Tab UI 経由（integration.rs）で完結し、印刷版では非表示。
/// 関数自体は将来の API 拡張時に流用可能なよう pub(super) として用意のみ。
#[allow(dead_code)]
pub(super) fn render_section_industry_structure(
    html: &mut String,
    industry_rows: &[Row],
    pref: &str,
) {
    if industry_rows.is_empty() {
        return;
    }
    // 集計行 (AS=全産業 / AR=全産業(公務除く) / CR=非農林漁業(公務除く)) を除外
    // これらを含めると合計が 3 倍以上になり構成比が誤る (バグ修正 2026-04-27)
    let is_aggregate = |code: &str| matches!(code, "AS" | "AR" | "CR");
    let detail_rows: Vec<&Row> = industry_rows
        .iter()
        .filter(|r| !is_aggregate(get_str_ref(r, "industry_code")))
        .collect();
    let total: i64 = detail_rows
        .iter()
        .map(|r| get_i64_local(r, "employees_total"))
        .sum();
    if total <= 0 {
        return;
    }

    html.push_str("<div class=\"section\" data-testid=\"industry-structure-print\">\n");
    html.push_str(&format!(
        "<h2>地域の産業構成（{} 就業者 Top 10）</h2>\n",
        super::super::super::helpers::escape_html(pref)
    ));
    render_figure_caption(html, "表 6-2", "産業別就業者数 Top 10（国勢調査 2020）");

    html.push_str(
        "<table class=\"sortable-table zebra\">\n<thead><tr><th>#</th><th>産業</th>\
        <th style=\"text-align:right\">就業者数</th><th style=\"text-align:right\">構成比</th></tr></thead>\n<tbody>\n",
    );
    for (i, r) in detail_rows.iter().take(10).enumerate() {
        let name = get_str_ref(r, "industry_name");
        let emp = get_i64_local(r, "employees_total");
        let pct = emp as f64 / total as f64 * 100.0;
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{:.1}%</td></tr>\n",
            i + 1,
            super::super::super::helpers::escape_html(name),
            format_number(emp),
            pct,
        ));
    }
    html.push_str("</tbody></table>\n");

    render_read_hint(
        html,
        "産業分類は国勢調査 2020 ベース。HW industry_raw とは粒度が異なる可能性があります。\
         産業別就業者数と採用容易性に相関が見られる場合がありますが、職種・条件マッチングが本質的要因です。",
    );

    html.push_str("</div>\n");
}

#[cfg(test)]
mod impl1_print_tests {
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

    fn empty_ctx() -> InsightContext {
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
            ext_industry_employees: vec![],
            hw_industry_counts: vec![],
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

    /// 印刷版 #18 + D-4: 地域特性補足セクション具体値テスト
    #[test]
    fn impl1_print_region_extras_renders_density_and_aging() {
        let ctx = InsightContext {
            ext_geography: vec![make_row(&[
                ("habitable_density_per_km2", json!(8_500.0)),
                ("habitable_area_km2", json!(11.66)),
                ("total_population", json!(60_000_i64)),
            ])],
            ext_population: vec![make_row(&[("aging_rate", json!(33.5))])],
            ..empty_ctx()
        };

        let mut html = String::new();
        render_section_region_extras(&mut html, &ctx);

        assert!(
            html.contains("data-testid=\"region-extras-section\""),
            "印刷版 region-extras section 必須"
        );
        assert!(html.contains("図 6-3"), "図番号 6-3 必須");
        // 具体値: 8500
        assert!(html.contains("8,500"), "可住地密度 8500 表示");
        // 都市型分類 (>=5000)
        assert!(html.contains("都市型"), "都市型 (>=5000) 分類");
        // 高齢化率 33.5% / 全国 29% 比較 +4.5pt
        assert!(html.contains("33.5"), "高齢化率 33.5 表示");
        assert!(html.contains("全国 29%"), "全国比較");
        assert!(html.contains("+4.5pt"), "差分 +4.5pt 具体値検証");
        // 必須注記
        assert!(html.contains("可住地密度は地理特性"), "#18 必須注記");
        assert!(html.contains("労働人口希少性の参考指標"), "D-4 必須注記");
    }

    /// 産業構造印刷版 D-3: Top10 表 + 構成比
    #[test]
    fn impl1_print_industry_section_renders_top10_with_pct() {
        let mk = |name: &str, emp: i64| -> Row {
            make_row(&[
                ("industry_name", json!(name)),
                ("employees_total", json!(emp)),
            ])
        };
        let rows = vec![
            mk("医療,福祉", 100_000),
            mk("製造業", 80_000),
            mk("卸売業,小売業", 60_000),
        ];

        let mut html = String::new();
        render_section_industry_structure(&mut html, &rows, "東京都");

        assert!(
            html.contains("data-testid=\"industry-structure-print\""),
            "印刷版 industry セクション ID 必須"
        );
        assert!(html.contains("表 6-2"), "表番号 6-2 必須");
        // total = 240_000、医療福祉 100_000 → 41.7%
        assert!(html.contains("41.7%"), "医療福祉構成比 41.7% 具体値");
        assert!(html.contains("100,000"), "医療福祉就業者数 100,000 具体値");
        // 必須注記
        assert!(html.contains("国勢調査 2020"), "D-3 必須注記");
        assert!(
            html.contains("職種・条件マッチングが本質的要因"),
            "因果ではない注記"
        );
    }

    /// 印刷版 fail-soft: 全空 ctx ではセクション非表示
    #[test]
    fn impl1_print_region_extras_hidden_when_no_data() {
        let ctx = empty_ctx();
        let mut html = String::new();
        render_section_region_extras(&mut html, &ctx);
        assert!(html.is_empty(), "全空時は印刷版セクション非表示");
    }
}

pub(super) fn render_section_municipality_salary(html: &mut String, agg: &SurveyAggregation) {
    if agg.by_municipality_salary.is_empty() {
        return;
    }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>地域分析（市区町村）</h2>\n");
    // So What: 件数の多い市区町村の給与水準が最も高い先
    if let Some(top_hi_salary) = agg
        .by_municipality_salary
        .iter()
        .take(15)
        .max_by_key(|m| m.avg_salary)
    {
        html.push_str(&format!(
            "<p class=\"section-sowhat\">\u{203B} 件数の多い 15 市区町村のうち、平均月給が最も高いのは\
             「{} {}」で {}（同名異県を避けるため都道府県併記）。</p>\n",
            escape_html(&top_hi_salary.prefecture),
            escape_html(&top_hi_salary.name),
            escape_html(&format_man_yen(top_hi_salary.avg_salary))
        ));
    }
    html.push_str(
        "<p style=\"font-size:9pt;color:#555;margin:0 0 8px;\">\
        <strong>【読み方ガイド】</strong>掲載件数の多い市区町村の給与水準を比較。\
        同じ都道府県内でも市区町村により給与差があります。\
    </p>\n",
    );

    // 同名市区町村の判定（伊達市・府中市など）
    use std::collections::HashMap;
    let mut name_count: HashMap<String, usize> = HashMap::new();
    for m in agg.by_municipality_salary.iter().take(15) {
        *name_count.entry(m.name.clone()).or_insert(0) += 1;
    }

    render_figure_caption(
        html,
        "表 7-1",
        "市区町村別 給与水準 Top 15（同名市区町村マーク付き）",
    );
    html.push_str(
        "<table class=\"sortable-table zebra\">\n<thead><tr><th>#</th><th>市区町村</th><th>都道府県</th>\
        <th style=\"text-align:right\">件数</th><th style=\"text-align:right\">平均月給</th>\
        <th style=\"text-align:right\">中央値</th></tr></thead>\n<tbody>\n",
    );
    for (i, m) in agg.by_municipality_salary.iter().take(15).enumerate() {
        let dup_marker = if name_count.get(&m.name).copied().unwrap_or(0) > 1 {
            " <span title=\"同名市区町村あり\" style=\"color:#f59e0b;font-weight:700;font-size:9pt;\">\u{26A0} 同名</span>"
        } else {
            ""
        };
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}{}</td><td style=\"font-size:10px;color:#666\">{}</td>\
             <td class=\"num\">{}件</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>\n",
            i + 1,
            escape_html(&m.name),
            dup_marker,
            escape_html(&m.prefecture),
            m.count,
            format_man_yen(m.avg_salary),
            format_man_yen(m.median_salary),
        ));
    }
    html.push_str("</tbody></table>\n");

    render_read_hint(
        html,
        "「\u{26A0} 同名」マークは、伊達市（北海道/福島県）や府中市（東京都/広島県）のように同名の市区町村が複数存在することを示します。都道府県と組み合わせて判定してください。",
    );

    render_section_bridge(
        html,
        "次セクションでは、雇用形態（正社員/パート/派遣）の構成と単位の異なる給与を別々に確認します。",
    );

    html.push_str("</div>\n");
}
