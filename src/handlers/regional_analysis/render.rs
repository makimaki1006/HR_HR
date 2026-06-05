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

use super::fetch::{EmpSalaryRow, MuniRankRow, RegionalFilter, SalaryHistogram};
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

/// データなしメッセージ (silent fallback 禁止)。
fn no_data(label: &str) -> String {
    format!(
        r#"<div class="text-slate-400 text-sm py-3">{} に該当する求人データがありません。条件を変更してください。</div>"#,
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
}
