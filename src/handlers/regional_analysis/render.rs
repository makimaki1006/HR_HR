//! 地域分析タブ: ECharts JSON + 表 HTML 描画層。
//!
//! ## 設計方針
//! - 外部統計のみ (postings 依存なし)。
//! - ECharts は `<div class="echart" data-chart-config='{...}'>` 方式。
//! - silent fallback 禁止: 件数 0 / データなしは明示メッセージ。
//! - XSS: SQL 由来文字列は必ず escape_html。
//! - 出典明記: 各パネルに e-Stat / 国勢調査 / 厚労省等の出典を脚注に記載。
//! - 中立表現: 「劣位/集中/縮小」評価語を使わない。

use std::fmt::Write as _;

use super::fetch::{
    CompanyPoint, ForeignResidentRow, ForeignResidents, IndustryStructure, InternetUsage,
    JobOpeningsRatioData, LaborStatsRow, OccupationDist, OccupationRow, PopulationPyramid,
    PyramidBand, RegionalFilter, WageComparison,
};
use crate::handlers::competitive::escape_html;
use crate::handlers::overview::format_number;

/// 外部統計用の見出し + body + 任意出典脚注ラッパ。
fn wrap_panel_with_note(title: &str, scope_escaped: &str, body: &str, note: &str) -> String {
    format!(
        r#"<div class="stat-card space-y-2">
  <div class="flex items-center justify-between gap-2 flex-wrap">
    <h3 class="text-sm text-white font-semibold">{title} <span class="text-slate-400 text-xs font-normal">— {scope}</span></h3>
  </div>
  {body}
  <p class="text-xs text-slate-500">{note}</p>
</div>"#,
        title = escape_html(title),
        scope = scope_escaped,
        body = body,
        note = escape_html(note),
    )
}

/// データなしメッセージ (外部統計系パネル用)。
fn no_data_external(label: &str) -> String {
    format!(
        r#"<div class="text-slate-400 text-sm py-3">{} に該当するデータがありません。条件を変更してください。</div>"#,
        escape_html(label)
    )
}

// ============================================================
// 追加: e-Stat 3 系
// ============================================================

/// 有効求人倍率推移 (折れ線 ECharts)。
///
/// 出典: e-Stat 政府統計コード 00450091 (一般職業紹介状況)。
pub(crate) fn render_job_openings_ratio(
    filter: &RegionalFilter,
    data: &JobOpeningsRatioData,
) -> String {
    let scope = filter.scope_label();
    let note = "出典: e-Stat 政府統計コード 00450091 (一般職業紹介状況)。有効求人倍率 (全体)。当該都道府県の公共職業安定所管内における集計値です。";
    if !data.has_data || data.points.is_empty() {
        return wrap_panel_with_note(
            "有効求人倍率 推移",
            &scope,
            &no_data_external("有効求人倍率"),
            note,
        );
    }

    let years: Vec<serde_json::Value> = data
        .points
        .iter()
        .map(|p| serde_json::Value::String(format!("{}年度", p.year)))
        .collect();
    let ratios: Vec<serde_json::Value> = data
        .points
        .iter()
        .map(|p| serde_json::json!((p.ratio * 100.0).round() / 100.0))
        .collect();
    let years_json = serde_json::to_string(&years).unwrap_or_else(|_| "[]".to_string());
    let ratios_json = serde_json::to_string(&ratios).unwrap_or_else(|_| "[]".to_string());

    let config = format!(
        r##"{{
  "tooltip": {{"trigger": "axis"}},
  "grid": {{"left": "8%", "right": "5%", "bottom": "15%", "top": "8%"}},
  "xAxis": {{"type": "category", "data": {years}, "axisLabel": {{"rotate": 45, "fontSize": 10}}}},
  "yAxis": {{"type": "value", "name": "倍率", "min": 0}},
  "series": [{{
    "type": "line",
    "data": {ratios},
    "smooth": true,
    "symbol": "circle",
    "symbolSize": 6,
    "itemStyle": {{"color": "#0072B2"}},
    "lineStyle": {{"width": 2}},
    "markLine": {{
      "symbol": "none",
      "data": [{{"yAxis": 1.0, "name": "均衡水準", "lineStyle": {{"color": "#E69F00", "type": "dashed"}}, "label": {{"formatter": "1.0倍", "color": "#E69F00"}}}}]
    }}
  }}]
}}"##,
        years = years_json,
        ratios = ratios_json,
    );

    let body = format!(
        r#"<div class="echart" style="height:300px;" data-chart-config='{config}'></div>"#,
        config = config,
    );
    wrap_panel_with_note("有効求人倍率 推移", &scope, &body, note)
}

/// 労働統計指標カード行 (最新年度 1 行)。
///
/// 出典: e-Stat 社会人口統計体系 / 労働政策研究・研修機構。
pub(crate) fn render_labor_stats(
    filter: &RegionalFilter,
    row: Option<&LaborStatsRow>,
) -> String {
    let scope = filter.scope_label();
    let note = "出典: e-Stat 社会人口統計体系 (v2_external_labor_stats)。労働政策研究・研修機構。最新年度の値を表示。";
    let row = match row {
        Some(r) => r,
        None => {
            return wrap_panel_with_note(
                "労働統計指標",
                &scope,
                &no_data_external("労働統計"),
                note,
            );
        }
    };

    let fmt_opt_f64 = |v: Option<f64>, unit: &str| -> String {
        match v {
            Some(x) => format!("{:.2}{}", x, unit),
            None => "-".to_string(),
        }
    };
    let fmt_opt_salary = |v: Option<f64>| -> String {
        match v {
            Some(x) => format!("{}円", format_number(x.round() as i64)),
            None => "-".to_string(),
        }
    };

    let body = format!(
        r#"<p class="text-xs text-slate-400 mb-2">{fy}年度</p>
<div class="grid grid-cols-2 sm:grid-cols-3 gap-2 text-sm">
  <div class="stat-card"><div class="text-xs text-slate-400">完全失業率</div><div class="text-lg font-bold text-cyan-300">{unemp}</div></div>
  <div class="stat-card"><div class="text-xs text-slate-400">離職率</div><div class="text-lg font-bold text-blue-300">{sep}</div></div>
  <div class="stat-card"><div class="text-xs text-slate-400">月収(男性)</div><div class="text-lg font-bold text-white">{sal_m}</div></div>
  <div class="stat-card"><div class="text-xs text-slate-400">月収(女性)</div><div class="text-lg font-bold text-white">{sal_f}</div></div>
  <div class="stat-card"><div class="text-xs text-slate-400">所定内労働時間(男)</div><div class="text-lg font-bold text-slate-200">{wh_m}</div></div>
  <div class="stat-card"><div class="text-xs text-slate-400">所定内労働時間(女)</div><div class="text-lg font-bold text-slate-200">{wh_f}</div></div>
  <div class="stat-card"><div class="text-xs text-slate-400">パート時給(男)</div><div class="text-lg font-bold text-amber-300">{pt_m}</div></div>
  <div class="stat-card"><div class="text-xs text-slate-400">パート時給(女)</div><div class="text-lg font-bold text-amber-300">{pt_f}</div></div>
</div>"#,
        fy = row.fiscal_year,
        unemp = fmt_opt_f64(row.unemployment_rate, "%"),
        sep = fmt_opt_f64(row.separation_rate, "%"),
        sal_m = fmt_opt_salary(row.monthly_salary_male),
        sal_f = fmt_opt_salary(row.monthly_salary_female),
        wh_m = fmt_opt_f64(row.working_hours_male, "h"),
        wh_f = fmt_opt_f64(row.working_hours_female, "h"),
        pt_m = fmt_opt_salary(row.part_time_wage_male),
        pt_f = fmt_opt_salary(row.part_time_wage_female),
    );
    wrap_panel_with_note("労働統計指標", &scope, &body, note)
}

/// 産業構造テーブル (横棒グラフ + 表)。
///
/// 出典: 総務省統計局 国勢調査 / 経済センサス (v2_external_industry_structure)。
pub(crate) fn render_industry_structure(
    filter: &RegionalFilter,
    data: &IndustryStructure,
) -> String {
    let scope = filter.scope_label();
    let note = format!(
        "出典: 総務省統計局 国勢調査 (v2_external_industry_structure)。集計粒度: {}。集計不能コード除外。従業者数は当該地域の産業構成を示す参考値です。",
        data.granularity
    );
    if !data.has_data || data.rows.is_empty() {
        return wrap_panel_with_note(
            "産業構造",
            &scope,
            &no_data_external("産業構造"),
            &note,
        );
    }

    // 横棒グラフ: 従業者数の多い順 (昇順表示で視認性向上)。
    let mut asc: Vec<&super::fetch::IndustryStructureRow> = data.rows.iter().collect();
    asc.reverse();
    let labels: Vec<serde_json::Value> = asc
        .iter()
        .map(|r| serde_json::Value::String(escape_html(&r.industry)))
        .collect();
    let values: Vec<serde_json::Value> = asc
        .iter()
        .map(|r| serde_json::Value::from(r.employees))
        .collect();
    let labels_json = serde_json::to_string(&labels).unwrap_or_else(|_| "[]".to_string());
    let values_json = serde_json::to_string(&values).unwrap_or_else(|_| "[]".to_string());
    let chart_h = (asc.len() as i64 * 26 + 80).clamp(180, 600);

    let config = format!(
        r##"{{
  "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}}},
  "grid": {{"left": "2%", "right": "12%", "bottom": "4%", "top": "4%", "containLabel": true}},
  "xAxis": {{"type": "value", "name": "従業者数(人)"}},
  "yAxis": {{"type": "category", "data": {labels}, "axisLabel": {{"fontSize": 10}}}},
  "series": [{{"type": "bar", "data": {values}, "barWidth": "60%",
    "itemStyle": {{"color": "#0072B2", "borderRadius": [0, 4, 4, 0]}},
    "label": {{"show": true, "position": "right", "fontSize": 10, "color": "#cbd5e1"}}}}]
}}"##,
        labels = labels_json,
        values = values_json,
    );
    let chart = format!(
        r#"<div class="echart" style="height:{h}px;" data-chart-config='{config}'></div>"#,
        h = chart_h,
        config = config,
    );

    let mut table = String::from(
        r#"<div class="overflow-x-auto mt-2"><table class="data-table"><thead><tr>
          <th>産業</th><th class="text-right">従業者数</th><th class="text-right">構成比</th>
        </tr></thead><tbody>"#,
    );
    for r in &data.rows {
        let pct = if data.total > 0 {
            r.employees as f64 / data.total as f64 * 100.0
        } else {
            0.0
        };
        write!(
            table,
            r#"<tr><td>{ind}</td><td class="text-right">{emp}</td><td class="text-right">{pct:.1}%</td></tr>"#,
            ind = escape_html(&r.industry),
            emp = format_number(r.employees),
            pct = pct,
        )
        .unwrap();
    }
    write!(
        table,
        r#"<tr class="font-semibold"><td>合計</td><td class="text-right">{}</td><td class="text-right">100.0%</td></tr>"#,
        format_number(data.total)
    )
    .unwrap();
    table.push_str("</tbody></table></div>");

    let body = format!("{}{}", chart, table);
    wrap_panel_with_note("産業構造", &scope, &body, &note)
}

// ============================================================
// 既存維持: 人口ピラミッド
// ============================================================

/// 人口ピラミッド (男女別 5 歳階級。左=男性 右=女性 の横棒 ECharts)。
pub(crate) fn render_population_pyramid(
    filter: &RegionalFilter,
    pyramid: &PopulationPyramid,
) -> String {
    let scope = filter.scope_label();
    let note = format!(
        "出典: 国勢調査 (v2_external_population_pyramid)。集計粒度: {}。左 (青) = 男性 / 右 (橙) = 女性。これは地域の実人口統計であり、求人・求職の人数ではありません。",
        pyramid.granularity
    );
    if !pyramid.has_data || pyramid.bands.is_empty() {
        return wrap_panel_with_note(
            "人口ピラミッド",
            &scope,
            &no_data_external("人口ピラミッド"),
            &note,
        );
    }

    let mut bands: Vec<&PyramidBand> = pyramid.bands.iter().collect();
    bands.sort_by_key(|b| age_sort_key(&b.age_group));

    let categories: Vec<serde_json::Value> = bands
        .iter()
        .map(|b| serde_json::Value::String(b.age_group.clone()))
        .collect();
    let male: Vec<serde_json::Value> = bands
        .iter()
        .map(|b| serde_json::Value::from(-b.male_count))
        .collect();
    let female: Vec<serde_json::Value> = bands
        .iter()
        .map(|b| serde_json::Value::from(b.female_count))
        .collect();
    let cat_json = serde_json::to_string(&categories).unwrap_or_else(|_| "[]".to_string());
    let male_json = serde_json::to_string(&male).unwrap_or_else(|_| "[]".to_string());
    let female_json = serde_json::to_string(&female).unwrap_or_else(|_| "[]".to_string());
    let chart_h = (bands.len() as i64 * 22 + 80).clamp(200, 720);

    let config = format!(
        r##"{{
  "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}}},
  "legend": {{"data": ["男性", "女性"], "textStyle": {{"color": "#cbd5e1"}}, "top": 0}},
  "grid": {{"left": "2%", "right": "4%", "bottom": "3%", "top": "12%", "containLabel": true}},
  "xAxis": {{"type": "value", "axisLabel": {{"formatter": "{{value}}"}}}},
  "yAxis": {{"type": "category", "data": {cats}, "axisLabel": {{"fontSize": 10}}}},
  "series": [
    {{"name": "男性", "type": "bar", "stack": "total", "data": {male}, "itemStyle": {{"color": "#0072B2"}}}},
    {{"name": "女性", "type": "bar", "stack": "total", "data": {female}, "itemStyle": {{"color": "#E69F00"}}}}
  ]
}}"##,
        cats = cat_json,
        male = male_json,
        female = female_json,
    );

    let body = format!(
        r#"<div class="echart" style="height:{h}px;" data-chart-config='{config}'></div>"#,
        h = chart_h,
        config = config,
    );
    wrap_panel_with_note("人口ピラミッド", &scope, &body, &note)
}

/// age_group ("0〜4歳", "5〜9歳", "85歳以上" 等) を昇順ソートするキー。
fn age_sort_key(label: &str) -> i64 {
    let digits: String = label.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<i64>().unwrap_or(9999)
}

// ============================================================
// 既存維持: 最低賃金
// ============================================================

/// 最低賃金 (都道府県粒度)。
///
/// postings 由来の給与中央値を削除。最低賃金のみ表示。
pub(crate) fn render_wage_comparison(filter: &RegionalFilter, cmp: &WageComparison) -> String {
    let scope = filter.scope_label();
    let note = "出典: 厚生労働省 地域別最低賃金 (v2_external_minimum_wage、都道府県値)。最低賃金は都道府県単位の値であり、市区町村別の差はありません。";
    if !cmp.has_data {
        return wrap_panel_with_note(
            "地域別最低賃金",
            &scope,
            &no_data_external("最低賃金"),
            note,
        );
    }

    let min_wage_str = match cmp.hourly_min_wage {
        Some(w) => format!("{}円/時", format_number(w.round() as i64)),
        None => "-".to_string(),
    };

    let body = format!(
        r#"<div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
  <div class="stat-card"><div class="text-xs text-slate-400">最低賃金 (都道府県値・時給)</div>
    <div class="text-2xl font-bold text-emerald-300">{min_wage}</div></div>
</div>"#,
        min_wage = min_wage_str,
    );

    wrap_panel_with_note("地域別最低賃金", &scope, &body, note)
}

// ============================================================
// 既存維持: 企業成長マトリックス
// ============================================================

/// 企業成長マトリックス (外部企業データの散布図: 成長率 × 従業員規模)。
/// UI に "SalesNow" 固有名は出さない (「外部企業データ」表記)。
pub(crate) fn render_company_matrix(filter: &RegionalFilter, points: &[CompanyPoint]) -> String {
    let scope = filter.scope_label();
    let note = "出典: 外部企業データ (法人単位の従業員規模・増減率)。各点は1社を表します。増減率と規模の関係を示すものであり、因果関係を示すものではありません。";
    if points.is_empty() {
        return wrap_panel_with_note(
            "企業成長マトリックス (外部企業データ)",
            &scope,
            &no_data_external("企業成長マトリックス"),
            note,
        );
    }

    let data: Vec<serde_json::Value> = points
        .iter()
        .map(|p| {
            serde_json::json!([
                (p.growth_rate_1y * 10.0).round() / 10.0,
                p.employee_count,
                escape_html(&p.company_name),
                escape_html(&p.industry),
            ])
        })
        .collect();
    let data_json = serde_json::to_string(&data).unwrap_or_else(|_| "[]".to_string());

    let config = format!(
        r##"{{
  "tooltip": {{"trigger": "item", "formatter": null}},
  "grid": {{"left": "2%", "right": "5%", "bottom": "8%", "top": "6%", "containLabel": true}},
  "xAxis": {{"type": "value", "name": "従業員増減率(%/年)", "nameLocation": "middle", "nameGap": 28}},
  "yAxis": {{"type": "value", "name": "従業員数(人)"}},
  "series": [{{
    "type": "scatter",
    "symbolSize": 9,
    "itemStyle": {{"color": "#0072B2", "opacity": 0.6}},
    "data": {data}
  }}]
}}"##,
        data = data_json,
    );

    let chart = format!(
        r#"<div class="echart" style="height:340px;" data-chart-config='{config}'></div>"#,
        config = config,
    );

    let mut table = String::from(
        r#"<div class="overflow-x-auto mt-2"><table class="data-table"><thead><tr>
          <th>企業名</th>
          <th>業種</th>
          <th class="text-right">従業員数</th>
          <th class="text-right">増減率(%/年)</th>
        </tr></thead><tbody>"#,
    );
    for p in points.iter().take(20) {
        write!(
            table,
            r#"<tr><td>{name}</td><td>{ind}</td><td class="text-right">{emp}</td><td class="text-right">{rate:+.1}</td></tr>"#,
            name = escape_html(&p.company_name),
            ind = escape_html(&p.industry),
            emp = format_number(p.employee_count),
            rate = p.growth_rate_1y,
        )
        .unwrap();
    }
    table.push_str("</tbody></table></div>");

    let count_note = format!(
        r#"<p class="text-xs text-slate-500">対象企業数: {}社 (従業員数の多い順、上限あり)。表は上位20社。</p>"#,
        format_number(points.len() as i64)
    );

    let body = format!("{}{}{}", chart, table, count_note);
    wrap_panel_with_note("企業成長マトリックス (外部企業データ)", &scope, &body, note)
}

// ============================================================
// 既存維持: 在留外国人 / インターネット利用 / 職業別就業者
// ============================================================

/// 在留外国人 (在留資格別 横棒 + 表)。
pub(crate) fn render_foreign_residents(filter: &RegionalFilter, fr: &ForeignResidents) -> String {
    let scope = filter.scope_label();
    let note = "出典: 住民基本台帳 (SSDSE-A、都道府県値)。在留資格別の外国人数。外国人材の採用可能性・多文化対応ニーズの把握用であり、求人・求職の人数ではありません。";
    if !fr.has_data || fr.rows.is_empty() {
        return wrap_panel_with_note(
            "在留外国人 (在留資格別)",
            &scope,
            &no_data_external("在留外国人"),
            note,
        );
    }
    let mut asc: Vec<&ForeignResidentRow> = fr.rows.iter().take(12).collect();
    asc.reverse();
    let labels: Vec<serde_json::Value> = asc
        .iter()
        .map(|r| serde_json::Value::String(escape_html(&r.visa_status)))
        .collect();
    let data: Vec<serde_json::Value> = asc
        .iter()
        .map(|r| serde_json::Value::from(r.count))
        .collect();
    let labels_json = serde_json::to_string(&labels).unwrap_or_else(|_| "[]".to_string());
    let data_json = serde_json::to_string(&data).unwrap_or_else(|_| "[]".to_string());
    let chart_h = (asc.len() as i64 * 28 + 80).clamp(180, 460);
    let config = format!(
        r##"{{
  "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}}},
  "grid": {{"left": "2%", "right": "10%", "bottom": "4%", "top": "4%", "containLabel": true}},
  "xAxis": {{"type": "value", "name": "人数"}},
  "yAxis": {{"type": "category", "data": {labels}, "axisLabel": {{"fontSize": 11}}}},
  "series": [{{"type": "bar", "data": {data}, "barWidth": "60%", "itemStyle": {{"color": "#009E73", "borderRadius": [0, 4, 4, 0]}}, "label": {{"show": true, "position": "right", "fontSize": 10, "color": "#cbd5e1"}}}}]
}}"##,
        labels = labels_json,
        data = data_json,
    );
    let chart = format!(
        r#"<div class="echart" style="height:{h}px;" data-chart-config='{config}'></div>"#,
        h = chart_h,
        config = config,
    );
    let mut table = String::from(
        r#"<div class="overflow-x-auto mt-2"><table class="data-table"><thead><tr>
          <th>在留資格</th><th class="text-right">人数</th><th class="text-right">構成比</th>
        </tr></thead><tbody>"#,
    );
    for r in &fr.rows {
        let pct = if fr.total > 0 {
            r.count as f64 / fr.total as f64 * 100.0
        } else {
            0.0
        };
        write!(
            table,
            r#"<tr><td>{vs}</td><td class="text-right">{cnt}</td><td class="text-right">{pct:.1}%</td></tr>"#,
            vs = escape_html(&r.visa_status),
            cnt = format_number(r.count),
            pct = pct,
        )
        .unwrap();
    }
    write!(
        table,
        r#"<tr class="font-semibold"><td>合計</td><td class="text-right">{}</td><td class="text-right">100.0%</td></tr>"#,
        format_number(fr.total)
    )
    .unwrap();
    table.push_str("</tbody></table></div>");
    let body = format!("{}{}", chart, table);
    wrap_panel_with_note("在留外国人 (在留資格別)", &scope, &body, note)
}

/// インターネット利用 (利用率・スマホ保有率 スタットカード)。
pub(crate) fn render_internet_usage(filter: &RegionalFilter, iu: &InternetUsage) -> String {
    let scope = filter.scope_label();
    let note = "出典: 通信利用動向 (都道府県値)。採用チャネル (SNS / WEB 求人) の有効性を判断する参考指標です。";
    if !iu.has_data {
        return wrap_panel_with_note(
            "インターネット利用",
            &scope,
            &no_data_external("インターネット利用"),
            note,
        );
    }
    let fmt = |v: Option<f64>| match v {
        Some(x) => format!("{:.1}%", x),
        None => "-".to_string(),
    };
    let year = iu.year.map(|y| format!("{y}年")).unwrap_or_default();
    let body = format!(
        r#"<div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
  <div class="stat-card"><div class="text-xs text-slate-400">インターネット利用率 {year}</div><div class="text-2xl text-white font-bold">{usage}</div></div>
  <div class="stat-card"><div class="text-xs text-slate-400">スマートフォン保有率 {year}</div><div class="text-2xl text-white font-bold">{sp}</div></div>
</div>"#,
        year = escape_html(&year),
        usage = fmt(iu.usage_rate),
        sp = fmt(iu.smartphone_rate),
    );
    wrap_panel_with_note("インターネット利用", &scope, &body, note)
}

/// 職業別就業者 (従業地ベース実測、横棒 + 表)。
pub(crate) fn render_occupation(filter: &RegionalFilter, occ: &OccupationDist) -> String {
    let scope = filter.scope_label();
    let note = format!(
        "出典: 国勢調査 (従業地ベース・男女計)。集計粒度: {}。当該地域で働く人の職業構成 (就業者数) であり、求人・求職の人数ではありません。",
        occ.granularity
    );
    if !occ.has_data || occ.rows.is_empty() {
        return wrap_panel_with_note(
            "職業別就業者",
            &scope,
            &no_data_external("職業別就業者"),
            &note,
        );
    }
    let mut asc: Vec<&OccupationRow> = occ.rows.iter().collect();
    asc.reverse();
    let labels: Vec<serde_json::Value> = asc
        .iter()
        .map(|r| serde_json::Value::String(escape_html(&r.occupation)))
        .collect();
    let data: Vec<serde_json::Value> = asc
        .iter()
        .map(|r| serde_json::Value::from(r.population))
        .collect();
    let labels_json = serde_json::to_string(&labels).unwrap_or_else(|_| "[]".to_string());
    let data_json = serde_json::to_string(&data).unwrap_or_else(|_| "[]".to_string());
    let chart_h = (asc.len() as i64 * 28 + 80).clamp(180, 520);
    let config = format!(
        r##"{{
  "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}}},
  "grid": {{"left": "2%", "right": "12%", "bottom": "4%", "top": "4%", "containLabel": true}},
  "xAxis": {{"type": "value", "name": "就業者数(人)"}},
  "yAxis": {{"type": "category", "data": {labels}, "axisLabel": {{"fontSize": 11}}}},
  "series": [{{"type": "bar", "data": {data}, "barWidth": "60%", "itemStyle": {{"color": "#0072B2", "borderRadius": [0, 4, 4, 0]}}, "label": {{"show": true, "position": "right", "fontSize": 10, "color": "#cbd5e1"}}}}]
}}"##,
        labels = labels_json,
        data = data_json,
    );
    let chart = format!(
        r#"<div class="echart" style="height:{h}px;" data-chart-config='{config}'></div>"#,
        h = chart_h,
        config = config,
    );
    let mut table = String::from(
        r#"<div class="overflow-x-auto mt-2"><table class="data-table"><thead><tr>
          <th>職業</th><th class="text-right">就業者数</th><th class="text-right">構成比</th>
        </tr></thead><tbody>"#,
    );
    for r in &occ.rows {
        let pct = if occ.total > 0 {
            r.population as f64 / occ.total as f64 * 100.0
        } else {
            0.0
        };
        write!(
            table,
            r#"<tr><td>{occ}</td><td class="text-right">{pop}</td><td class="text-right">{pct:.1}%</td></tr>"#,
            occ = escape_html(&r.occupation),
            pop = format_number(r.population),
            pct = pct,
        )
        .unwrap();
    }
    write!(
        table,
        r#"<tr class="font-semibold"><td>合計</td><td class="text-right">{}</td><td class="text-right">100.0%</td></tr>"#,
        format_number(occ.total)
    )
    .unwrap();
    table.push_str("</tbody></table></div>");
    let body = format!("{}{}", chart, table);
    wrap_panel_with_note("職業別就業者", &scope, &body, &note)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pref_filter() -> RegionalFilter {
        RegionalFilter {
            prefecture: "東京都".into(),
            municipality: "".into(),
            job_type: "".into(),
        }
    }

    // ---- 在留外国人 ----

    #[test]
    fn foreign_residents_empty_shows_no_data_and_source() {
        let html = render_foreign_residents(&pref_filter(), &ForeignResidents::default());
        assert!(html.contains("住民基本台帳"));
        assert!(html.contains("該当するデータがありません"));
        assert!(!html.contains("SalesNow"));
    }

    #[test]
    fn foreign_residents_with_data_renders_chart_table_total() {
        let fr = ForeignResidents {
            rows: vec![
                ForeignResidentRow {
                    visa_status: "永住者".into(),
                    count: 1000,
                },
                ForeignResidentRow {
                    visa_status: "技能実習".into(),
                    count: 500,
                },
            ],
            total: 1500,
            survey_period: "2023".into(),
            has_data: true,
        };
        let html = render_foreign_residents(&pref_filter(), &fr);
        assert!(html.contains("data-chart-config"));
        assert!(html.contains("永住者"));
        assert!(html.contains("1,500"));
        assert!(html.contains("求人・求職の人数ではありません"));
    }

    #[test]
    fn foreign_residents_escapes_visa_status_xss() {
        let fr = ForeignResidents {
            rows: vec![ForeignResidentRow {
                visa_status: "<img src=x onerror=alert(1)>".into(),
                count: 10,
            }],
            total: 10,
            survey_period: String::new(),
            has_data: true,
        };
        let html = render_foreign_residents(&pref_filter(), &fr);
        assert!(!html.contains("<img src=x"));
    }

    // ---- インターネット利用 ----

    #[test]
    fn internet_usage_empty_and_with_data() {
        let empty = render_internet_usage(&pref_filter(), &InternetUsage::default());
        assert!(empty.contains("該当するデータがありません"));
        let iu = InternetUsage {
            usage_rate: Some(85.5),
            smartphone_rate: Some(70.2),
            year: Some(2023),
            has_data: true,
        };
        let html = render_internet_usage(&pref_filter(), &iu);
        assert!(html.contains("85.5%"));
        assert!(html.contains("70.2%"));
        assert!(html.contains("通信利用動向"));
    }

    // ---- 職業別就業者 ----

    #[test]
    fn occupation_empty_and_with_data() {
        let empty = render_occupation(&pref_filter(), &OccupationDist::default());
        assert!(empty.contains("該当するデータがありません"));
        let occ = OccupationDist {
            rows: vec![
                OccupationRow {
                    occupation: "事務従事者".into(),
                    population: 100_000,
                },
                OccupationRow {
                    occupation: "販売従事者".into(),
                    population: 50_000,
                },
            ],
            total: 150_000,
            granularity: "都道府県".into(),
            area_name: "東京都".into(),
            has_data: true,
        };
        let html = render_occupation(&pref_filter(), &occ);
        assert!(html.contains("data-chart-config"));
        assert!(html.contains("事務従事者"));
        assert!(html.contains("国勢調査"));
        assert!(html.contains("求人・求職の人数ではありません"));
    }

    #[test]
    fn occupation_escapes_xss() {
        let occ = OccupationDist {
            rows: vec![OccupationRow {
                occupation: "<script>x</script>".into(),
                population: 10,
            }],
            total: 10,
            granularity: "市区町村".into(),
            area_name: "x".into(),
            has_data: true,
        };
        let html = render_occupation(&pref_filter(), &occ);
        assert!(!html.contains("<script>x"));
    }

    // ---- 人口ピラミッド ----

    #[test]
    fn pyramid_empty_explicit_message() {
        let py = PopulationPyramid {
            bands: vec![],
            granularity: "都道府県".into(),
            area_name: "東京都".into(),
            has_data: false,
        };
        let html = render_population_pyramid(&pref_filter(), &py);
        assert!(html.contains("該当するデータがありません"));
        assert!(!html.contains("data-chart-config"));
    }

    #[test]
    fn pyramid_with_data_sorts_and_separates_sexes() {
        let py = PopulationPyramid {
            bands: vec![
                PyramidBand {
                    age_group: "85歳以上".into(),
                    male_count: 100,
                    female_count: 200,
                },
                PyramidBand {
                    age_group: "0〜4歳".into(),
                    male_count: 500,
                    female_count: 480,
                },
            ],
            granularity: "市区町村".into(),
            area_name: "東京都 新宿区".into(),
            has_data: true,
        };
        let html = render_population_pyramid(&pref_filter(), &py);
        assert!(html.contains("data-chart-config"));
        assert!(html.contains("男性") && html.contains("女性"));
        let idx_young = html.find("0〜4歳").unwrap();
        let idx_old = html.find("85歳以上").unwrap();
        assert!(idx_young < idx_old, "age groups should be sorted ascending");
    }

    #[test]
    fn pyramid_source_note_and_granularity() {
        let py = PopulationPyramid {
            bands: vec![PyramidBand {
                age_group: "20〜24歳".into(),
                male_count: 300,
                female_count: 320,
            }],
            granularity: "都道府県".into(),
            area_name: "東京都".into(),
            has_data: true,
        };
        let html = render_population_pyramid(&pref_filter(), &py);
        assert!(html.contains("国勢調査"));
        assert!(html.contains("都道府県"));
        assert!(html.contains("求人・求職の人数ではありません"));
    }

    // ---- 最低賃金 ----

    #[test]
    fn wage_comparison_empty_explicit_message() {
        let cmp = WageComparison {
            hourly_min_wage: None,
            has_data: false,
        };
        let html = render_wage_comparison(&pref_filter(), &cmp);
        assert!(html.contains("該当するデータがありません"));
    }

    #[test]
    fn wage_comparison_shows_min_wage_only() {
        let cmp = WageComparison {
            hourly_min_wage: Some(1113.0),
            has_data: true,
        };
        let html = render_wage_comparison(&pref_filter(), &cmp);
        assert!(html.contains("1,113円/時"));
        assert!(html.contains("厚生労働省"));
        // HW 由来の「給与中央値」は出ない
        assert!(!html.contains("ハローワーク掲載求人"));
    }

    #[test]
    fn wage_comparison_no_neutral_violation() {
        let cmp = WageComparison {
            hourly_min_wage: Some(1000.0),
            has_data: true,
        };
        let html = render_wage_comparison(&pref_filter(), &cmp);
        for banned in ["劣位", "集中", "縮小"] {
            assert!(!html.contains(banned), "banned word: {banned}");
        }
    }

    // ---- 企業成長マトリックス ----

    #[test]
    fn company_matrix_empty_explicit_message() {
        let html = render_company_matrix(&pref_filter(), &[]);
        assert!(html.contains("該当するデータがありません"));
    }

    #[test]
    fn company_matrix_no_salesnow_brand_name() {
        let points = vec![CompanyPoint {
            company_name: "テスト株式会社".into(),
            employee_count: 100,
            growth_rate_1y: 5.5,
            industry: "医療業".into(),
        }];
        let html = render_company_matrix(&pref_filter(), &points);
        assert!(
            !html.to_lowercase().contains("salesnow"),
            "SalesNow brand name must not appear"
        );
        assert!(html.contains("外部企業データ"));
        assert!(html.contains("テスト株式会社"));
        assert!(html.contains("data-chart-config"));
    }

    #[test]
    fn company_matrix_escapes_company_name() {
        let points = vec![CompanyPoint {
            company_name: "<img src=x onerror=1>".into(),
            employee_count: 50,
            growth_rate_1y: -2.0,
            industry: "建設業".into(),
        }];
        let html = render_company_matrix(&pref_filter(), &points);
        assert!(!html.contains("<img src=x"));
        assert!(html.contains("&lt;img"));
    }

    #[test]
    fn company_matrix_no_causation_claim_in_note() {
        let points = vec![CompanyPoint {
            company_name: "A社".into(),
            employee_count: 200,
            growth_rate_1y: 10.0,
            industry: "情報通信業".into(),
        }];
        let html = render_company_matrix(&pref_filter(), &points);
        assert!(html.contains("因果関係を示すものではありません"));
        for banned in ["劣位", "集中", "縮小"] {
            assert!(!html.contains(banned), "banned word: {banned}");
        }
    }

    // ---- 追加: e-Stat 3 系 ----

    #[test]
    fn job_openings_ratio_empty_explicit_message() {
        use super::super::fetch::JobOpeningsRatioData;
        let data = JobOpeningsRatioData {
            points: vec![],
            has_data: false,
        };
        let html = render_job_openings_ratio(&pref_filter(), &data);
        assert!(html.contains("該当するデータがありません"));
        assert!(!html.contains("data-chart-config"));
    }

    #[test]
    fn job_openings_ratio_with_data_emits_echart() {
        use super::super::fetch::{JobOpeningsRatioData, JobOpeningsRatioPoint};
        let data = JobOpeningsRatioData {
            points: vec![
                JobOpeningsRatioPoint { year: 2020, ratio: 1.05 },
                JobOpeningsRatioPoint { year: 2021, ratio: 1.12 },
            ],
            has_data: true,
        };
        let html = render_job_openings_ratio(&pref_filter(), &data);
        assert!(html.contains("data-chart-config"));
        assert!(html.contains("e-Stat"));
        assert!(html.contains("2020年度"));
    }

    #[test]
    fn labor_stats_none_shows_no_data() {
        let html = render_labor_stats(&pref_filter(), None);
        assert!(html.contains("該当するデータがありません"));
    }

    #[test]
    fn labor_stats_with_data_shows_values() {
        let row = LaborStatsRow {
            fiscal_year: 2022,
            unemployment_rate: Some(2.5),
            separation_rate: Some(15.0),
            monthly_salary_male: Some(350_000.0),
            monthly_salary_female: Some(250_000.0),
            working_hours_male: Some(165.0),
            working_hours_female: Some(145.0),
            part_time_wage_male: Some(1200.0),
            part_time_wage_female: Some(1050.0),
        };
        let html = render_labor_stats(&pref_filter(), Some(&row));
        assert!(html.contains("2022年度"));
        assert!(html.contains("2.50%"));
        assert!(html.contains("350,000円"));
        assert!(html.contains("e-Stat"));
    }

    #[test]
    fn industry_structure_empty_explicit_message() {
        let data = IndustryStructure {
            rows: vec![],
            total: 0,
            granularity: "都道府県".into(),
            has_data: false,
        };
        let html = render_industry_structure(&pref_filter(), &data);
        assert!(html.contains("該当するデータがありません"));
        assert!(!html.contains("data-chart-config"));
    }

    #[test]
    fn industry_structure_with_data_emits_chart_and_table() {
        use super::super::fetch::IndustryStructureRow;
        let data = IndustryStructure {
            rows: vec![
                IndustryStructureRow {
                    industry: "医療，福祉".into(),
                    employees: 100_000,
                },
                IndustryStructureRow {
                    industry: "製造業".into(),
                    employees: 80_000,
                },
            ],
            total: 180_000,
            granularity: "都道府県".into(),
            has_data: true,
        };
        let html = render_industry_structure(&pref_filter(), &data);
        assert!(html.contains("data-chart-config"));
        assert!(html.contains("医療，福祉"));
        assert!(html.contains("国勢調査"));
        // 因果・評価語なし
        for banned in ["劣位", "集中", "縮小"] {
            assert!(!html.contains(banned), "banned word: {banned}");
        }
    }

    #[test]
    fn industry_structure_escapes_xss() {
        use super::super::fetch::IndustryStructureRow;
        let data = IndustryStructure {
            rows: vec![IndustryStructureRow {
                industry: "<script>x</script>".into(),
                employees: 1,
            }],
            total: 1,
            granularity: "都道府県".into(),
            has_data: true,
        };
        let html = render_industry_structure(&pref_filter(), &data);
        assert!(!html.contains("<script>x"));
    }
}
