//! 地域×業界分析タブ: ECharts JSON + 表 HTML 描画層。
//!
//! ## 設計方針
//! - competitive::external::wrap_panel と同等の見出し+出典脚注スタイルを踏襲。
//! - ECharts は competitive.html と同方式: `<div class="echart" data-chart-config='{...}'>`。
//!   afterSettle で window.initECharts が自動初期化する (app.js)。
//! - silent fallback 禁止: 件数 0 / データなしは明示メッセージ。
//! - XSS: SQL 由来文字列 (市区町村名・雇用形態名) は必ず escape_html。
//! - 出典明記: HW 掲載求人に基づく旨を脚注に記載 (MEMORY: feedback_hw_data_scope)。
//! - 中立表現: 「劣位/集中/縮小」評価語を使わない。順位・件数・中央値の事実記述のみ。

use std::fmt::Write as _;

use super::fetch::{
    CompanyPoint, EmpSalaryRow, JobTypeSalaryRow, MuniRankRow, PopulationPyramid, PyramidBand,
    RegionalFilter, SalaryHistogram, WageComparison,
};
use crate::handlers::competitive::escape_html;
use crate::handlers::overview::format_number;

/// 出典脚注 (全パネル共通)。
const SOURCE_NOTE: &str =
    "出典: ハローワーク掲載求人。給与は月給・有効レンジ (5万円以上) に基づく集計であり、当該地域の全求人市場を表すものではありません。";

/// 見出し + body + 出典脚注をまとめる共通ラッパ (competitive::external::wrap_panel 相当)。
///
/// `title` / `scope` は escape 済みで渡される想定だが二重 escape を避けるため
/// 呼び出し側で escape する。`body` は HTML 化済み。
fn wrap_panel(title: &str, scope_escaped: &str, body: &str) -> String {
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
        note = escape_html(SOURCE_NOTE),
    )
}

/// データなしメッセージ (silent fallback 禁止)。求人データ系パネル用。
fn no_data(label: &str) -> String {
    format!(
        r#"<div class="text-slate-400 text-sm py-3">{} に該当する求人データがありません。条件を変更してください。</div>"#,
        escape_html(label)
    )
}

/// データなしメッセージ (外部統計系パネル用。「求人」と書かない)。
fn no_data_external(label: &str) -> String {
    format!(
        r#"<div class="text-slate-400 text-sm py-3">{} に該当するデータがありません。条件を変更してください。</div>"#,
        escape_html(label)
    )
}

/// 1) 給与分布ヒストグラム (ECharts bar + 平均/中央値 markLine)。
pub(crate) fn render_salary_histogram(filter: &RegionalFilter, hist: &SalaryHistogram) -> String {
    let scope = filter.scope_label();
    if !hist.has_data || hist.count == 0 {
        return wrap_panel("給与分布ヒストグラム", &scope, &no_data("給与分布"));
    }

    // ECharts data-chart-config 用 JSON 配列を構築。
    // ラベル・値ともに数値/既知の "N万" 文字列のみ (ユーザ入力なし) だが、
    // JSON 文字列として安全に出力するため serde_json で組み立てる。
    let labels: Vec<serde_json::Value> = hist
        .bucket_labels
        .iter()
        .map(|s| serde_json::Value::String(s.clone()))
        .collect();
    let values: Vec<serde_json::Value> = hist
        .bucket_counts
        .iter()
        .map(|&c| serde_json::Value::from(c))
        .collect();
    let labels_json = serde_json::to_string(&labels).unwrap_or_else(|_| "[]".to_string());
    let values_json = serde_json::to_string(&values).unwrap_or_else(|_| "[]".to_string());

    // markLine: 平均 (橙) と中央値 (緑)。万円単位ラベル。
    let mean_man = hist.mean as f64 / 10_000.0;
    let median_man = hist.median as f64 / 10_000.0;

    // X 軸は "N万" カテゴリ。markLine は xAxis のカテゴリ index 上に出せないため
    // ここでは平均/中央値を凡例下のサマリで明示し、グラフ内 markLine は
    // 件数 (y 値) ではなく注記として markLine type=average を使わず、
    // サマリテキストで代替する (カテゴリ軸 + 値の markLine 整合のため)。
    // ただし要件 (平均/中央値 markLine) を満たすため、対応バケット index に
    // markLine (xAxis index) を引く。
    let bucket_width_man = 5.0_f64; // 5万円幅
    let start_man = hist
        .bucket_labels
        .first()
        .and_then(|s| s.trim_end_matches('万').parse::<f64>().ok())
        .unwrap_or(0.0);
    let mean_idx = ((mean_man - start_man) / bucket_width_man).max(0.0);
    let median_idx = ((median_man - start_man) / bucket_width_man).max(0.0);

    let config = format!(
        r##"{{
  "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}}},
  "grid": {{"left": "8%", "right": "5%", "bottom": "12%", "top": "8%"}},
  "xAxis": {{"type": "category", "data": {labels}, "name": "月給(円)", "axisLabel": {{"rotate": 45}}}},
  "yAxis": {{"type": "value", "name": "求人件数"}},
  "series": [{{
    "type": "bar",
    "data": {values},
    "itemStyle": {{"color": "#0072B2", "borderRadius": [4,4,0,0]}},
    "barWidth": "70%",
    "markLine": {{
      "symbol": "none",
      "data": [
        {{"xAxis": {mean_idx}, "name": "平均", "lineStyle": {{"color": "#E69F00", "type": "dashed", "width": 2}}, "label": {{"formatter": "平均 {mean_man:.1}万", "color": "#E69F00"}}}},
        {{"xAxis": {median_idx}, "name": "中央値", "lineStyle": {{"color": "#009E73", "type": "dashed", "width": 2}}, "label": {{"formatter": "中央値 {median_man:.1}万", "color": "#009E73", "position": "insideEndTop"}}}}
      ]
    }}
  }}]
}}"##,
        labels = labels_json,
        values = values_json,
        mean_idx = mean_idx,
        median_idx = median_idx,
        mean_man = mean_man,
        median_man = median_man,
    );

    let summary = format!(
        r#"<div class="grid grid-cols-3 gap-2 text-sm mb-2">
  <div class="stat-card"><div class="text-xs text-slate-400">集計件数</div>
    <div class="text-xl font-bold text-cyan-300">{cnt} 件</div></div>
  <div class="stat-card"><div class="text-xs text-slate-400">平均</div>
    <div class="text-xl font-bold text-amber-300">{mean}円</div></div>
  <div class="stat-card"><div class="text-xs text-slate-400">中央値</div>
    <div class="text-xl font-bold text-emerald-300">{median}円</div></div>
</div>"#,
        cnt = format_number(hist.count),
        mean = format_number(hist.mean),
        median = format_number(hist.median),
    );

    let body = format!(
        r#"{summary}<div class="echart" style="height:360px;" data-chart-config='{config}'></div>"#,
        summary = summary,
        config = config,
    );

    wrap_panel("給与分布ヒストグラム", &scope, &body)
}

/// 2) 市区町村別 求人数・給与中央値ランキング表。
pub(crate) fn render_muni_ranking(filter: &RegionalFilter, rows: &[MuniRankRow]) -> String {
    let scope = filter.scope_label();
    if rows.is_empty() {
        return wrap_panel(
            "市区町村別 求人数・給与中央値ランキング",
            &scope,
            &no_data("市区町村ランキング"),
        );
    }

    let mut table = String::new();
    table.push_str(
        r#"<div class="overflow-x-auto"><table class="data-table"><thead><tr>
          <th class="text-center" style="width:60px">順位</th>
          <th>市区町村</th>
          <th class="text-right">求人数</th>
          <th class="text-right">給与中央値 (月給)</th>
        </tr></thead><tbody>"#,
    );
    for (i, r) in rows.iter().enumerate() {
        let median = match r.median_salary {
            Some(v) => format!("{}円", format_number(v)),
            None => "-".to_string(),
        };
        write!(
            table,
            r#"<tr><td class="text-center">{rank}</td><td>{muni}</td><td class="text-right">{cnt}</td><td class="text-right">{median}</td></tr>"#,
            rank = i + 1,
            muni = escape_html(&r.municipality),
            cnt = format_number(r.count),
            median = median,
        )
        .unwrap();
    }
    table.push_str("</tbody></table></div>");

    let note = r#"<p class="text-xs text-slate-500">求人数の多い順 (上位50件)。中央値「-」は月給・有効レンジの該当求人が無いことを示します。業界を選択すると当該業界で絞り込みます。</p>"#;
    let body = format!("{}{}", table, note);

    wrap_panel("市区町村別 求人数・給与中央値ランキング", &scope, &body)
}

/// 3) 雇用形態別 給与統計表 (中央値 / 件数)。
pub(crate) fn render_emp_salary(filter: &RegionalFilter, rows: &[EmpSalaryRow]) -> String {
    let scope = filter.scope_label();
    if rows.is_empty() {
        return wrap_panel("雇用形態別 給与統計", &scope, &no_data("雇用形態別統計"));
    }

    let mut table = String::new();
    table.push_str(
        r#"<div class="overflow-x-auto"><table class="data-table"><thead><tr>
          <th>雇用形態</th>
          <th class="text-right">求人数</th>
          <th class="text-right">給与中央値 (月給)</th>
        </tr></thead><tbody>"#,
    );
    for r in rows {
        let median = match r.median_salary {
            Some(v) => format!("{}円", format_number(v)),
            None => "-".to_string(),
        };
        write!(
            table,
            r#"<tr><td>{emp}</td><td class="text-right">{cnt}</td><td class="text-right">{median}</td></tr>"#,
            emp = escape_html(&r.employment_type),
            cnt = format_number(r.count),
            median = median,
        )
        .unwrap();
    }
    table.push_str("</tbody></table></div>");

    let note = r#"<p class="text-xs text-slate-500">中央値「-」は月給・有効レンジの該当求人が無いことを示します。雇用形態 (正社員・契約社員・パート等) の区分は掲載元の表記に従います。</p>"#;
    let body = format!("{}{}", table, note);

    wrap_panel("雇用形態別 給与統計", &scope, &body)
}

/// 外部統計用の見出し + body + 任意出典脚注ラッパ。
/// HW 用 SOURCE_NOTE ではなく呼び出し側指定の出典を脚注にする。
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

/// 4) 業界別給与比較 (横棒 ECharts。当該業界をハイライト)。
pub(crate) fn render_job_type_salary(filter: &RegionalFilter, rows: &[JobTypeSalaryRow]) -> String {
    let scope = filter.scope_label();
    if rows.is_empty() {
        return wrap_panel("業界別 給与中央値比較", &scope, &no_data("業界別給与"));
    }

    // 中央値が算出できた業界のみグラフ化 (None は表で別途明示)。
    // ECharts horizontal bar: yAxis=category(業界), xAxis=value(中央値 万円)。
    // 件数降順で来るため、グラフは中央値の昇順に並べ替えると横棒が見やすい。
    let mut graphable: Vec<&JobTypeSalaryRow> =
        rows.iter().filter(|r| r.median_salary.is_some()).collect();
    graphable.sort_by_key(|r| r.median_salary.unwrap_or(0));

    let labels: Vec<serde_json::Value> = graphable
        .iter()
        .map(|r| serde_json::Value::String(escape_html(&r.job_type)))
        .collect();
    // 値はハイライト対象だけ色を変えるため itemStyle 付きオブジェクトで出す。
    let data: Vec<serde_json::Value> = graphable
        .iter()
        .map(|r| {
            let man = r.median_salary.unwrap_or(0) as f64 / 10_000.0;
            let color = if r.highlighted { "#E69F00" } else { "#0072B2" };
            serde_json::json!({
                "value": (man * 10.0).round() / 10.0,
                "itemStyle": {"color": color, "borderRadius": [0, 4, 4, 0]}
            })
        })
        .collect();
    let labels_json = serde_json::to_string(&labels).unwrap_or_else(|_| "[]".to_string());
    let data_json = serde_json::to_string(&data).unwrap_or_else(|_| "[]".to_string());
    let chart_h = (graphable.len() as i64 * 28 + 80).clamp(180, 720);

    let config = format!(
        r##"{{
  "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}, "valueFormatter": null}},
  "grid": {{"left": "2%", "right": "8%", "bottom": "4%", "top": "4%", "containLabel": true}},
  "xAxis": {{"type": "value", "name": "給与中央値(万円/月給)"}},
  "yAxis": {{"type": "category", "data": {labels}, "axisLabel": {{"fontSize": 11}}}},
  "series": [{{
    "type": "bar",
    "data": {data},
    "barWidth": "60%",
    "label": {{"show": true, "position": "right", "formatter": "{{c}}万", "fontSize": 10, "color": "#cbd5e1"}}
  }}]
}}"##,
        labels = labels_json,
        data = data_json,
    );

    let chart = if graphable.is_empty() {
        // 全業界で中央値算出不可 → グラフは出さず表のみ。
        String::new()
    } else {
        format!(
            r#"<div class="echart" style="height:{h}px;" data-chart-config='{config}'></div>"#,
            h = chart_h,
            config = config,
        )
    };

    // 補助表 (件数 + 中央値、ハイライト印)。
    let mut table = String::from(
        r#"<div class="overflow-x-auto mt-2"><table class="data-table"><thead><tr>
          <th>業界</th>
          <th class="text-right">求人数</th>
          <th class="text-right">給与中央値 (月給)</th>
        </tr></thead><tbody>"#,
    );
    for r in rows {
        let median = match r.median_salary {
            Some(v) => format!("{}円", format_number(v)),
            None => "-".to_string(),
        };
        let mark = if r.highlighted { " ★" } else { "" };
        write!(
            table,
            r#"<tr><td>{jt}{mark}</td><td class="text-right">{cnt}</td><td class="text-right">{median}</td></tr>"#,
            jt = escape_html(&r.job_type),
            mark = mark,
            cnt = format_number(r.count),
            median = median,
        )
        .unwrap();
    }
    table.push_str("</tbody></table></div>");

    let body = format!("{}{}", chart, table);
    wrap_panel("業界別 給与中央値比較", &scope, &body)
}

/// 5) 人口ピラミッド (男女別 5 歳階級。左=男性 右=女性 の横棒 ECharts)。
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

    // 年齢階級は若い順に並べ替え (age_group の先頭数値でソート)。
    let mut bands: Vec<&PyramidBand> = pyramid.bands.iter().collect();
    bands.sort_by_key(|b| age_sort_key(&b.age_group));

    let categories: Vec<serde_json::Value> = bands
        .iter()
        .map(|b| serde_json::Value::String(b.age_group.clone()))
        .collect();
    // 男性は負値で左側に描画 (ピラミッド慣例)、女性は正値で右側。
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
    // 先頭の連続数字を抽出。なければ大きい値 (末尾) 扱い。
    let digits: String = label.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<i64>().unwrap_or(9999)
}

/// 6) 最低賃金 vs 給与中央値 (都道府県粒度)。
pub(crate) fn render_wage_comparison(filter: &RegionalFilter, cmp: &WageComparison) -> String {
    let scope = filter.scope_label();
    let note = "出典: 厚生労働省 地域別最低賃金 (v2_external_minimum_wage、都道府県値) / 給与中央値はハローワーク掲載求人 (月給・有効レンジ)。時給換算は月173.8時間 (法定労働時間ベース) で算出した参考値です。";
    if !cmp.has_data {
        return wrap_panel_with_note(
            "最低賃金 vs 給与中央値",
            &scope,
            &no_data_external("最低賃金比較"),
            note,
        );
    }

    let min_wage_str = match cmp.hourly_min_wage {
        Some(w) => format!("{}円/時", format_number(w.round() as i64)),
        None => "-".to_string(),
    };
    let median_monthly_str = match cmp.median_monthly {
        Some(m) => format!("{}円/月", format_number(m)),
        None => "-".to_string(),
    };
    let median_hourly_str = match cmp.median_hourly {
        Some(h) => format!("{}円/時", format_number(h.round() as i64)),
        None => "-".to_string(),
    };

    // 「給与中央値(時給換算)が最賃をどれだけ上回るか」(中立記述)。
    let diff_html = match (cmp.hourly_min_wage, cmp.median_hourly) {
        (Some(w), Some(h)) if w > 0.0 => {
            let diff = h - w;
            let ratio = h / w;
            let sign = if diff >= 0.0 { "+" } else { "" };
            format!(
                r#"<div class="stat-card mt-2"><div class="text-xs text-slate-400">給与中央値(時給換算) と最低賃金の差</div>
  <div class="text-lg font-bold text-cyan-300">{sign}{diff}円/時 (最低賃金の {ratio:.2} 倍)</div></div>"#,
                sign = sign,
                diff = format_number(diff.round() as i64),
                ratio = ratio,
            )
        }
        _ => String::new(),
    };

    // ECharts: 最賃 vs 中央値(時給換算) の横棒比較。
    let chart = match (cmp.hourly_min_wage, cmp.median_hourly) {
        (Some(w), Some(h)) => {
            let config = format!(
                r##"{{
  "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}}},
  "grid": {{"left": "2%", "right": "10%", "bottom": "4%", "top": "8%", "containLabel": true}},
  "xAxis": {{"type": "value", "name": "円/時"}},
  "yAxis": {{"type": "category", "data": ["最低賃金(県値)", "給与中央値(時給換算)"], "axisLabel": {{"fontSize": 11}}}},
  "series": [{{
    "type": "bar",
    "data": [
      {{"value": {wage}, "itemStyle": {{"color": "#009E73"}}}},
      {{"value": {hourly}, "itemStyle": {{"color": "#0072B2"}}}}
    ],
    "barWidth": "50%",
    "label": {{"show": true, "position": "right", "formatter": "{{c}}円", "color": "#cbd5e1"}}
  }}]
}}"##,
                wage = w.round() as i64,
                hourly = h.round() as i64,
            );
            format!(
                r#"<div class="echart" style="height:200px;" data-chart-config='{config}'></div>"#,
                config = config,
            )
        }
        _ => String::new(),
    };

    let summary = format!(
        r#"<div class="grid grid-cols-3 gap-2 text-sm">
  <div class="stat-card"><div class="text-xs text-slate-400">最低賃金 (都道府県値)</div>
    <div class="text-base font-bold text-emerald-300">{min_wage}</div></div>
  <div class="stat-card"><div class="text-xs text-slate-400">給与中央値 (月給)</div>
    <div class="text-base font-bold text-cyan-300">{median_m}</div></div>
  <div class="stat-card"><div class="text-xs text-slate-400">給与中央値 (時給換算)</div>
    <div class="text-base font-bold text-blue-300">{median_h}</div></div>
</div>"#,
        min_wage = min_wage_str,
        median_m = median_monthly_str,
        median_h = median_hourly_str,
    );

    let count_note = format!(
        r#"<p class="text-xs text-slate-500">給与中央値の集計件数: {}件。最低賃金は都道府県単位の値であり、市区町村別の差はありません。</p>"#,
        format_number(cmp.count)
    );

    let body = format!("{}{}{}{}", summary, diff_html, chart, count_note);
    wrap_panel_with_note("最低賃金 vs 給与中央値", &scope, &body, note)
}

/// 7) 企業成長マトリックス (外部企業データの散布図: 成長率 × 従業員規模)。
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

    // ECharts scatter: x=成長率(%), y=従業員数 (対数表示は使わず線形)。
    // tooltip 用に企業名・業種も data に持たせる ([x, y, name, industry])。
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

    // tooltip formatter で企業名 (data[2]) と業種 (data[3]) を出す。
    // data-chart-config は JSON のため関数を埋められない → app.js 側の
    // 既定 formatter に委ねるが、企業名・業種を表に併記して補う。
    let chart = format!(
        r#"<div class="echart" style="height:340px;" data-chart-config='{config}'></div>"#,
        config = config,
    );

    // 補助表 (上位 規模順、最大 20 行)。
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

    #[test]
    fn histogram_no_data_shows_explicit_message() {
        // silent fallback 禁止: 空のヒストグラムは明示メッセージを返す
        let hist = SalaryHistogram {
            count: 0,
            bucket_labels: vec![],
            bucket_counts: vec![],
            mean: 0,
            median: 0,
            has_data: false,
        };
        let html = render_salary_histogram(&pref_filter(), &hist);
        assert!(html.contains("該当する求人データがありません"));
        assert!(!html.contains("data-chart-config"));
    }

    #[test]
    fn histogram_with_data_emits_echart_and_marklines() {
        let hist = SalaryHistogram {
            count: 3,
            bucket_labels: vec!["15万".into(), "20万".into()],
            bucket_counts: vec![1, 2],
            mean: 183_000,
            median: 200_000,
            has_data: true,
        };
        let html = render_salary_histogram(&pref_filter(), &hist);
        // ECharts 埋め込みと markLine (平均/中央値) を含む
        assert!(html.contains("data-chart-config"));
        assert!(html.contains("平均"));
        assert!(html.contains("中央値"));
        // サマリに件数が出る
        assert!(html.contains("3 件"));
    }

    #[test]
    fn muni_ranking_escapes_municipality_name() {
        // XSS: SQL 由来文字列を escape する
        let rows = vec![MuniRankRow {
            municipality: "<script>alert(1)</script>".into(),
            count: 10,
            median_salary: Some(200_000),
        }];
        let html = render_muni_ranking(&pref_filter(), &rows);
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn muni_ranking_empty_explicit_message() {
        let html = render_muni_ranking(&pref_filter(), &[]);
        assert!(html.contains("該当する求人データがありません"));
    }

    #[test]
    fn muni_ranking_none_median_shows_dash() {
        let rows = vec![MuniRankRow {
            municipality: "新宿区".into(),
            count: 5,
            median_salary: None,
        }];
        let html = render_muni_ranking(&pref_filter(), &rows);
        // 中央値なしは "-" で明示 (0 円と誤解させない)
        assert!(html.contains(">-<"));
    }

    #[test]
    fn emp_salary_escapes_and_shows_source_note() {
        let rows = vec![EmpSalaryRow {
            employment_type: "正社員".into(),
            count: 100,
            median_salary: Some(250_000),
        }];
        let html = render_emp_salary(&pref_filter(), &rows);
        assert!(html.contains("正社員"));
        // 出典 (HW 掲載求人) 明記
        assert!(html.contains("ハローワーク掲載求人"));
    }

    #[test]
    fn no_neutral_violation_words_in_static_notes() {
        // 中立表現監査: 評価語 (劣位/集中/縮小) を静的注記に含めない
        let rows = vec![EmpSalaryRow {
            employment_type: "パート".into(),
            count: 10,
            median_salary: None,
        }];
        let html = render_emp_salary(&pref_filter(), &rows);
        for banned in ["劣位", "集中", "縮小"] {
            assert!(!html.contains(banned), "banned word found: {banned}");
        }
    }

    #[test]
    fn display_spec_no_jobseeker_count_label() {
        // DISPLAY_SPEC §2: 求職者「人数」推定を出さない。
        // 表示ラベルに「求職者」「応募者数」等の人数推定語が無いこと。
        let rows = vec![MuniRankRow {
            municipality: "新宿区".into(),
            count: 5,
            median_salary: Some(200_000),
        }];
        let html = render_muni_ranking(&pref_filter(), &rows);
        assert!(!html.contains("求職者"));
        // 「求人数」(求人の件数) は許可
        assert!(html.contains("求人数"));
    }

    // --- Phase2 テスト ---

    fn job_filter() -> RegionalFilter {
        RegionalFilter {
            prefecture: "東京都".into(),
            municipality: "".into(),
            job_type: "医療，福祉".into(),
        }
    }

    // 4) 業界別給与比較
    #[test]
    fn job_type_salary_empty_explicit_message() {
        // silent fallback 禁止: 空入力は明示メッセージ
        let html = render_job_type_salary(&pref_filter(), &[]);
        assert!(html.contains("該当する求人データがありません"));
        assert!(!html.contains("data-chart-config"));
    }

    #[test]
    fn job_type_salary_highlights_and_escapes() {
        // データ妥当性: ハイライト印 ★ + 中央値表示 + XSS escape
        let rows = vec![
            JobTypeSalaryRow {
                job_type: "医療，福祉".into(),
                count: 100,
                median_salary: Some(250_000),
                highlighted: true,
            },
            JobTypeSalaryRow {
                job_type: "<script>x</script>".into(),
                count: 50,
                median_salary: Some(200_000),
                highlighted: false,
            },
        ];
        let html = render_job_type_salary(&job_filter(), &rows);
        assert!(html.contains("data-chart-config"));
        assert!(html.contains('★'), "highlighted job_type should be marked");
        // 中央値が表に出る
        assert!(html.contains("250,000円"));
        // XSS escape
        assert!(!html.contains("<script>x</script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn job_type_salary_no_neutral_violation() {
        let rows = vec![JobTypeSalaryRow {
            job_type: "建設業".into(),
            count: 10,
            median_salary: None,
            highlighted: false,
        }];
        let html = render_job_type_salary(&pref_filter(), &rows);
        for banned in ["劣位", "集中", "縮小"] {
            assert!(!html.contains(banned), "banned word: {banned}");
        }
    }

    // 5) 人口ピラミッド
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
        // データ妥当性: 年齢階級が昇順ソートされ、男女系列が分かれる
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
        // 男性は負値 (左側)、女性は正値 (右側)
        assert!(html.contains("男性") && html.contains("女性"));
        // 0〜4歳 が 85歳以上 より先に出る (昇順ソート)
        let idx_young = html.find("0〜4歳").unwrap();
        let idx_old = html.find("85歳以上").unwrap();
        assert!(idx_young < idx_old, "age groups should be sorted ascending");
    }

    #[test]
    fn pyramid_source_note_and_granularity() {
        // 出典明記 + 粒度明記 + 求職者人数でない旨
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
        assert!(html.contains("都道府県")); // 粒度
        assert!(html.contains("求人・求職の人数ではありません"));
    }

    // 6) 最低賃金 vs 給与中央値
    #[test]
    fn wage_comparison_empty_explicit_message() {
        let cmp = WageComparison {
            hourly_min_wage: None,
            median_monthly: None,
            median_hourly: None,
            count: 0,
            has_data: false,
        };
        let html = render_wage_comparison(&pref_filter(), &cmp);
        assert!(html.contains("該当するデータがありません"));
    }

    #[test]
    fn wage_comparison_shows_values_and_pref_level_note() {
        // データ妥当性: 最賃・中央値・時給換算が表示され、都道府県値である旨を明記
        let cmp = WageComparison {
            hourly_min_wage: Some(1113.0),
            median_monthly: Some(250_000),
            median_hourly: Some(250_000.0 / 173.8),
            count: 1234,
            has_data: true,
        };
        let html = render_wage_comparison(&pref_filter(), &cmp);
        assert!(html.contains("1,113円/時"));
        assert!(html.contains("250,000円/月"));
        // 都道府県値である明記
        assert!(html.contains("都道府県値") || html.contains("都道府県単位の値"));
    }

    #[test]
    fn wage_comparison_source_note() {
        let cmp = WageComparison {
            hourly_min_wage: Some(1000.0),
            median_monthly: Some(200_000),
            median_hourly: Some(200_000.0 / 173.8),
            count: 10,
            has_data: true,
        };
        let html = render_wage_comparison(&pref_filter(), &cmp);
        // 出典: 厚労省 最低賃金 + HW 求人
        assert!(html.contains("厚生労働省"));
        assert!(html.contains("ハローワーク掲載求人"));
    }

    #[test]
    fn wage_comparison_no_neutral_violation() {
        let cmp = WageComparison {
            hourly_min_wage: Some(1000.0),
            median_monthly: Some(180_000),
            median_hourly: Some(180_000.0 / 173.8),
            count: 5,
            has_data: true,
        };
        let html = render_wage_comparison(&pref_filter(), &cmp);
        for banned in ["劣位", "集中", "縮小"] {
            assert!(!html.contains(banned), "banned word: {banned}");
        }
    }

    // 7) 企業成長マトリックス
    #[test]
    fn company_matrix_empty_explicit_message() {
        let html = render_company_matrix(&pref_filter(), &[]);
        assert!(html.contains("該当するデータがありません"));
    }

    #[test]
    fn company_matrix_no_salesnow_brand_name() {
        // SalesNow 固有名を UI に出さない (「外部企業データ」表記)
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
        // データ妥当性: 企業名・従業員数・増減率が表に出る
        assert!(html.contains("テスト株式会社"));
        assert!(html.contains("data-chart-config"));
    }

    #[test]
    fn company_matrix_escapes_company_name() {
        // XSS: 企業名 escape
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
        // 相関≠因果: 注記に因果断定を含めない (「因果関係を示すものではありません」明記)
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
}
