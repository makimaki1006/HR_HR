//! 47 県横断比較ビューの HTML レンダリング

use super::fetch::{fetch_all_prefecture_kpi, ComparisonMetric, PrefectureKpi};
use crate::handlers::helpers::escape_html;
use crate::handlers::overview::{get_session_filters, render_no_db_data};
use crate::AppState;
use axum::extract::{Query, State};
use axum::response::Html;
use serde::Deserialize;
use serde_json::Value;
use std::fmt::Write as _;
use std::sync::Arc;
use tower_sessions::Session;

/// `/tab/comparison` のクエリパラメータ
#[derive(Debug, Deserialize, Default)]
pub struct ComparisonQuery {
    /// 表示する指標（既定 = `posting_count`）
    #[serde(default)]
    pub metric: Option<String>,
    /// `desc` (既定) または `asc`
    #[serde(default)]
    pub sort: Option<String>,
}

/// 47 都道府県横断比較タブ本体
pub async fn tab_comparison(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(q): Query<ComparisonQuery>,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let db = match &state.hw_db {
        Some(db) => db,
        None => return Html(render_no_db_data("47都道府県横断比較")),
    };

    let metric = ComparisonMetric::from_str(q.metric.as_deref().unwrap_or("posting_count"));
    let asc = matches!(q.sort.as_deref(), Some("asc"));

    // フィルタ用に大分類（job_types）を使用。中分類（industry_raws）は postings.job_type に
    // 直接マッピングできないため、ここでは job_types のみを採用（リサーチャー想定の最大単位）。
    let industry_filter = filters.job_types.clone();

    // 重い集計はブロッキング
    let cache_key = format!(
        "comparison_{}_{}_{}_{}",
        metric.as_str(),
        if asc { "asc" } else { "desc" },
        filters.industry_cache_key(),
        filters.prefecture, // ハイライト用（並びには影響しない）
    );

    if let Some(cached) = state.cache.get(&cache_key) {
        if let Some(html) = cached.as_str() {
            return Html(html.to_string());
        }
    }

    let db_clone = db.clone();
    let kpi_list = tokio::task::spawn_blocking(move || {
        fetch_all_prefecture_kpi(&db_clone, &industry_filter)
    })
    .await
    .unwrap_or_default();

    // ソート（指標の数値降順/昇順）
    let mut sorted = kpi_list.clone();
    sorted.sort_by(|a, b| {
        let av = metric.numeric_value(a);
        let bv = metric.numeric_value(b);
        if asc {
            av.partial_cmp(&bv).unwrap_or(std::cmp::Ordering::Equal)
        } else {
            bv.partial_cmp(&av).unwrap_or(std::cmp::Ordering::Equal)
        }
    });

    let html = render_comparison_html(&sorted, metric, asc, &filters.industry_label(), &filters.prefecture);
    state.cache.set(cache_key, Value::String(html.clone()));
    Html(html)
}

const ALL_METRICS: &[ComparisonMetric] = &[
    ComparisonMetric::PostingCount,
    ComparisonMetric::SalaryMinAvg,
    ComparisonMetric::SeishainRatio,
    ComparisonMetric::FacilityCount,
    ComparisonMetric::SalaryDisclosureRate,
];

/// 比較ビュー HTML を構築
fn render_comparison_html(
    sorted: &[PrefectureKpi],
    metric: ComparisonMetric,
    asc: bool,
    industry_label: &str,
    selected_pref: &str,
) -> String {
    let mut html = String::with_capacity(48_000);

    // ヘッダ + 指標切替
    html.push_str(r#"<div class="space-y-4">"#);
    html.push_str(r#"<div class="flex items-start justify-between flex-wrap gap-2">"#);
    write!(
        html,
        r#"<div>
            <h2 class="text-xl font-bold text-white">47都道府県 横断比較</h2>
            <p class="text-xs text-slate-500 mt-1">産業: <span class="text-blue-400">{ind}</span> ／ ハローワーク掲載求人データのみ。民間求人サイト (Indeed等) は含まれません。</p>
        </div>"#,
        ind = escape_html(industry_label)
    )
    .unwrap();
    html.push_str(r#"</div>"#);

    // 指標切替バー
    html.push_str(r#"<div class="flex flex-wrap gap-2 items-center bg-navy-800/40 border border-slate-700 rounded p-3">"#);
    html.push_str(r#"<span class="text-xs text-slate-400">指標:</span>"#);
    for m in ALL_METRICS {
        let active_cls = if *m == metric {
            "bg-blue-600 text-white"
        } else {
            "bg-slate-700/50 text-slate-300 hover:bg-slate-600"
        };
        write!(
            html,
            r##"<button class="px-3 py-1 rounded text-xs {active}" \
                hx-get="/tab/comparison?metric={mk}&sort={sort}" \
                hx-target="#content" hx-swap="innerHTML">{label}</button>"##,
            active = active_cls,
            mk = m.as_str(),
            sort = if asc { "asc" } else { "desc" },
            label = m.label()
        )
        .unwrap();
    }
    // ソート切替
    html.push_str(r#"<span class="text-xs text-slate-400 ml-4">並び順:</span>"#);
    let sort_label = if asc { "昇順 ↑" } else { "降順 ↓" };
    let next_sort = if asc { "desc" } else { "asc" };
    write!(
        html,
        r##"<button class="px-3 py-1 rounded text-xs bg-slate-700/50 text-slate-300 hover:bg-slate-600" \
            hx-get="/tab/comparison?metric={mk}&sort={ns}" \
            hx-target="#content" hx-swap="innerHTML">{sl}</button>"##,
        mk = metric.as_str(),
        ns = next_sort,
        sl = sort_label
    )
    .unwrap();
    // CSV ダウンロード（クライアント側で生成）
    html.push_str(r#"<button onclick="downloadComparisonCsv()" class="ml-auto px-3 py-1 rounded text-xs bg-emerald-700 text-white hover:bg-emerald-600">CSV ダウンロード</button>"#);
    html.push_str(r#"</div>"#);

    // === 統計サマリー ===
    let values: Vec<f64> = sorted.iter().map(|k| metric.numeric_value(k)).collect();
    let total_count: i64 = sorted.iter().map(|k| k.posting_count).sum();
    let max_v = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min_v = values
        .iter()
        .cloned()
        .filter(|v| *v > 0.0)
        .fold(f64::INFINITY, f64::min);
    let max_pref = sorted
        .iter()
        .find(|k| (metric.numeric_value(k) - max_v).abs() < f64::EPSILON)
        .map(|k| k.prefecture.clone())
        .unwrap_or_default();
    let min_pref = sorted
        .iter()
        .filter(|k| metric.numeric_value(k) > 0.0)
        .min_by(|a, b| {
            metric
                .numeric_value(a)
                .partial_cmp(&metric.numeric_value(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|k| k.prefecture.clone())
        .unwrap_or_default();
    let avg_v: f64 = if !values.is_empty() {
        values.iter().filter(|v| **v > 0.0).sum::<f64>()
            / values.iter().filter(|v| **v > 0.0).count().max(1) as f64
    } else {
        0.0
    };

    html.push_str(r#"<div class="grid grid-cols-2 md:grid-cols-4 gap-3">"#);
    write!(
        html,
        r#"<div class="bg-navy-800/40 border border-slate-700 rounded p-3">
            <div class="text-[10px] text-slate-500">全国合計求人</div>
            <div class="text-xl font-bold text-white mt-1">{total} <span class="text-xs text-slate-400">件</span></div>
        </div>
        <div class="bg-navy-800/40 border border-slate-700 rounded p-3">
            <div class="text-[10px] text-slate-500">最高 ({label})</div>
            <div class="text-base font-bold text-emerald-400 mt-1">{max_p}</div>
            <div class="text-xs text-slate-500">{max_v}</div>
        </div>
        <div class="bg-navy-800/40 border border-slate-700 rounded p-3">
            <div class="text-[10px] text-slate-500">最低 ({label})</div>
            <div class="text-base font-bold text-amber-400 mt-1">{min_p}</div>
            <div class="text-xs text-slate-500">{min_v}</div>
        </div>
        <div class="bg-navy-800/40 border border-slate-700 rounded p-3">
            <div class="text-[10px] text-slate-500">47県平均（除0）</div>
            <div class="text-xl font-bold text-blue-400 mt-1">{avg}</div>
        </div>"#,
        total = crate::handlers::helpers::format_number(total_count),
        label = metric.label(),
        max_p = escape_html(&max_pref),
        max_v = format_metric_value_simple(metric, max_v),
        min_p = escape_html(&min_pref),
        min_v = format_metric_value_simple(metric, min_v),
        avg = format_metric_value_simple(metric, avg_v),
    )
    .unwrap();
    html.push_str(r#"</div>"#);

    // === ECharts 横棒グラフ ===
    let chart_config = build_chart_config(sorted, metric, &max_pref, selected_pref);
    write!(
        html,
        r#"<div class="bg-navy-800/40 border border-slate-700 rounded p-3">
            <div class="text-xs text-slate-300 mb-2">{lbl}（47都道府県）</div>
            <div class="echart" data-chart-config='{cfg}' style="height:900px"></div>
        </div>"#,
        lbl = metric.label(),
        cfg = chart_config.replace('\'', "&#x27;"),
    )
    .unwrap();

    // === テーブル ===
    html.push_str(
        r#"<div class="bg-navy-800/40 border border-slate-700 rounded overflow-x-auto">
            <table class="w-full text-sm" id="comparison-table">
                <thead class="bg-navy-900/60 text-slate-300 text-xs">
                    <tr>
                        <th class="px-3 py-2 text-left">順位</th>
                        <th class="px-3 py-2 text-left">都道府県</th>
                        <th class="px-3 py-2 text-right">求人件数</th>
                        <th class="px-3 py-2 text-right">月給下限平均</th>
                        <th class="px-3 py-2 text-right">正社員比率</th>
                        <th class="px-3 py-2 text-right">事業所数</th>
                        <th class="px-3 py-2 text-right">給与開示率</th>
                        <th class="px-3 py-2 text-center">アクション</th>
                    </tr>
                </thead>
                <tbody>"#,
    );

    for (i, kpi) in sorted.iter().enumerate() {
        let highlight = if kpi.prefecture == selected_pref && !selected_pref.is_empty() {
            " bg-blue-900/30"
        } else if i % 2 == 0 {
            ""
        } else {
            " bg-navy-900/30"
        };
        write!(
            html,
            r#"<tr class="border-t border-slate-700{hl} hover:bg-blue-900/20">
                <td class="px-3 py-2 text-slate-400">{rank}</td>
                <td class="px-3 py-2 text-white font-medium">{pref}</td>
                <td class="px-3 py-2 text-right text-slate-200">{c1} 件</td>
                <td class="px-3 py-2 text-right text-slate-200">{c2}</td>
                <td class="px-3 py-2 text-right text-slate-200">{c3}%</td>
                <td class="px-3 py-2 text-right text-slate-200">{c4}</td>
                <td class="px-3 py-2 text-right text-slate-200">{c5}%</td>
                <td class="px-3 py-2 text-center">
                    <button onclick="navigateToKarte('{pref_attr}')" class="text-xs text-blue-400 hover:text-blue-300 underline">カルテへ</button>
                </td>
            </tr>"#,
            hl = highlight,
            rank = i + 1,
            pref = escape_html(&kpi.prefecture),
            pref_attr = escape_html(&kpi.prefecture),
            c1 = ComparisonMetric::PostingCount.format_value(kpi),
            c2 = if kpi.salary_min_avg > 0.0 {
                format!("{} 円", ComparisonMetric::SalaryMinAvg.format_value(kpi))
            } else {
                "-".to_string()
            },
            c3 = ComparisonMetric::SeishainRatio.format_value(kpi),
            c4 = ComparisonMetric::FacilityCount.format_value(kpi),
            c5 = ComparisonMetric::SalaryDisclosureRate.format_value(kpi),
        )
        .unwrap();
    }
    html.push_str("</tbody></table></div>");

    // === HW 限定性 + 因果非主張の注記 ===
    html.push_str(
        r#"<div class="text-[10px] text-slate-500 border-t border-slate-800 pt-2">
            出典: ハローワーク求人データ（hellowork.db）。<br>
            ※ 集計は単純な統計値であり、傾向を示すに留まります（因果関係を主張するものではありません）。<br>
            ※ HW 掲載求人のみを対象としており、Indeed・マイナビ等の民間サイト掲載求人は含まれません。
        </div>"#,
    );

    // === JS: CSV ダウンロード + カルテ遷移 ===
    html.push_str(
        r#"<script>
        function downloadComparisonCsv() {
            var rows = [['順位','都道府県','求人件数','月給下限平均(円)','正社員比率(%)','事業所数','給与開示率(%)']];
            document.querySelectorAll('#comparison-table tbody tr').forEach(function(tr, idx){
                var tds = tr.querySelectorAll('td');
                rows.push([
                    idx+1,
                    tds[1].textContent.trim(),
                    tds[2].textContent.replace(/[^0-9-]/g,'').trim(),
                    tds[3].textContent.replace(/[^0-9-]/g,'').trim(),
                    tds[4].textContent.replace('%','').trim(),
                    tds[5].textContent.replace(/[^0-9-]/g,'').trim(),
                    tds[6].textContent.replace('%','').trim()
                ]);
            });
            var bom = '﻿';
            var csv = bom + rows.map(function(r){ return r.map(function(c){ return '"'+String(c).replace(/"/g,'""')+'"'; }).join(','); }).join('\r\n');
            var blob = new Blob([csv], {type:'text/csv;charset=utf-8'});
            var url = URL.createObjectURL(blob);
            var a = document.createElement('a');
            a.href = url; a.download = 'prefecture_comparison.csv'; a.click();
            URL.revokeObjectURL(url);
        }
        function navigateToKarte(pref) {
            // 都道府県セレクタを切り替え、地域カルテタブへ移動
            var prefSel = document.getElementById('pref-select');
            if (prefSel) {
                prefSel.value = pref;
                // 既存の switchLocation 関数を呼ぶ（ある場合）
                if (typeof setPrefecture === 'function') {
                    setPrefecture(prefSel);
                } else {
                    fetch('/api/set_prefecture', {
                        method: 'POST',
                        headers: {'Content-Type': 'application/x-www-form-urlencoded'},
                        body: 'prefecture=' + encodeURIComponent(pref)
                    }).then(function(){
                        htmx.ajax('GET', '/tab/region_karte', {target:'#content', swap:'innerHTML'});
                    });
                    return;
                }
            }
            htmx.ajax('GET', '/tab/region_karte', {target:'#content', swap:'innerHTML'});
        }
        </script>"#,
    );

    html.push_str("</div>");
    html
}

/// ECharts 横棒グラフの設定 JSON を生成
fn build_chart_config(
    sorted: &[PrefectureKpi],
    metric: ComparisonMetric,
    max_pref: &str,
    selected_pref: &str,
) -> String {
    // ECharts は y 軸（カテゴリ）が下から上に積み上がるため、
    // 上に来てほしい順位 1 位を末尾に置く（並びを反転させる）
    let display: Vec<&PrefectureKpi> = sorted.iter().rev().collect();

    let prefs: Vec<String> = display
        .iter()
        .map(|k| escape_chart_str(&k.prefecture))
        .collect();

    let values: Vec<String> = display
        .iter()
        .map(|k| {
            let v = metric.numeric_value(k);
            if v.is_finite() && v != 0.0 {
                format!("{:.2}", v)
            } else {
                "0".to_string()
            }
        })
        .collect();

    // 各バーの色（最大値=緑、選択中の都道府県=青、その他=灰）
    let colors: Vec<String> = display
        .iter()
        .map(|k| {
            if k.prefecture == max_pref {
                "#10b981".to_string()
            } else if k.prefecture == selected_pref && !selected_pref.is_empty() {
                "#3b82f6".to_string()
            } else {
                "#64748b".to_string()
            }
        })
        .collect();

    // チャート用の data オブジェクト（color を per-item で渡す）
    let data_items: Vec<String> = values
        .iter()
        .zip(colors.iter())
        .map(|(v, c)| format!(r#"{{"value":{},"itemStyle":{{"color":"{}"}}}}"#, v, c))
        .collect();

    let unit = metric.unit();

    format!(
        r##"{{
            "tooltip": {{ "trigger": "axis", "axisPointer": {{ "type": "shadow" }} }},
            "grid": {{ "left": "70px", "right": "60px", "top": "10px", "bottom": "30px" }},
            "xAxis": {{ "type": "value", "name": "{unit}", "axisLabel": {{ "color": "#94a3b8" }} }},
            "yAxis": {{ "type": "category", "data": [{prefs}], "axisLabel": {{ "color": "#cbd5e1", "fontSize": 10 }} }},
            "series": [
                {{ "type": "bar", "name": "{label}", "data": [{data}], "label": {{ "show": false }} }}
            ]
        }}"##,
        unit = unit,
        label = metric.label(),
        prefs = prefs
            .iter()
            .map(|p| format!(r#""{}""#, p))
            .collect::<Vec<_>>()
            .join(","),
        data = data_items.join(","),
    )
}

/// チャート設定 JSON を HTML 属性値内に安全に埋め込むためのエスケープ。
///
/// JSON 文字列リテラル内で `<`, `>`, `&` を `<`, `>`, `&` に変換する。
/// これによりブラウザの HTML 属性パーサが `<script>` 等を要素として誤認することを防ぎ、
/// JSON.parse 時には元の文字列に復元される（XSS 二重防御）。
fn escape_chart_str(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}

/// 単純な数値→表示文字列（カードのサマリー用）
fn format_metric_value_simple(metric: ComparisonMetric, v: f64) -> String {
    if !v.is_finite() {
        return "-".to_string();
    }
    match metric {
        ComparisonMetric::PostingCount | ComparisonMetric::FacilityCount => {
            format!(
                "{} {}",
                crate::handlers::helpers::format_number(v.round() as i64),
                metric.unit()
            )
        }
        ComparisonMetric::SalaryMinAvg => {
            format!(
                "{} 円",
                crate::handlers::helpers::format_number(v.round() as i64)
            )
        }
        ComparisonMetric::SeishainRatio | ComparisonMetric::SalaryDisclosureRate => {
            format!("{:.1}%", v)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_chart_config_contains_47_prefs() {
        // 47 件のダミー KPI を作成
        let mut sorted = Vec::new();
        for (i, p) in crate::models::job_seeker::PREFECTURE_ORDER.iter().enumerate() {
            sorted.push(PrefectureKpi {
                prefecture: p.to_string(),
                posting_count: (i + 1) as i64 * 100,
                ..Default::default()
            });
        }
        let cfg = build_chart_config(&sorted, ComparisonMetric::PostingCount, "北海道", "");
        assert!(cfg.contains("北海道"), "chart config must include 北海道");
        assert!(cfg.contains("沖縄県"), "chart config must include 沖縄県");
        // 47 個の都道府県名が含まれているか
        let pref_count = crate::models::job_seeker::PREFECTURE_ORDER
            .iter()
            .filter(|p| cfg.contains(*p))
            .count();
        assert_eq!(
            pref_count, 47,
            "chart config must contain all 47 prefectures, got {}",
            pref_count
        );
    }

    #[test]
    fn render_html_is_safe_for_special_chars() {
        // 都道府県名に "<script>" のような怪しい文字が混入してもエスケープされる
        let kpi = vec![PrefectureKpi {
            prefecture: "<script>alert(1)</script>".to_string(),
            posting_count: 100,
            ..Default::default()
        }];
        let html = render_comparison_html(&kpi, ComparisonMetric::PostingCount, false, "全産業", "");
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    }
}
