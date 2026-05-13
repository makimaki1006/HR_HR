//! サブタブ7: 通勤圏分析（30km圏内の都道府県またぎ市区町村 + 性別×年齢ピラミッド）
//!
//! - render_subtab_7: 公開エントリポイント
//! - build_commute_sankey / build_butterfly_pyramid: ECharts JSON 構築
//! - kpi: KPI カード描画ヘルパ（subtab7 内のみで使用）

use serde_json::Value;
use std::collections::HashMap;

use super::super::super::helpers::{escape_html, format_number, get_f64, get_i64};
use super::super::fetch::{
    fetch_commute_inflow, fetch_commute_outflow, fetch_self_commute_rate, CommuteFlow,
};
use super::super::helpers::get_str;

type Row = HashMap<String, Value>;

/// 通勤圏分析（30km圏内の都道府県またぎ市区町村 + 性別×年齢ピラミッド）
pub(crate) fn render_subtab_7(
    db: &crate::db::local_sqlite::LocalDb,
    turso: Option<&crate::db::turso_http::TursoDb>,
    pref: &str,
    muni: &str,
) -> String {
    use super::super::fetch::{
        fetch_commute_zone, fetch_commute_zone_pyramid, fetch_population_pyramid,
        fetch_spatial_mismatch,
    };

    let mut html = String::with_capacity(8_000);
    html.push_str(r#"<div class="space-y-4">"#);

    if muni.is_empty() {
        html.push_str(r#"<div class="stat-card text-center py-8">
            <p class="text-slate-400 text-sm">市区町村を選択すると通勤圏分析が表示されます</p>
            <p class="text-slate-500 text-xs mt-1">30km圏内の隣接市区町村（都道府県またぎ）を自動抽出します</p>
        </div></div>"#);
        return html;
    }

    // 30km圏内の市区町村を抽出
    let zone = fetch_commute_zone(db, pref, muni, 30.0);
    if zone.is_empty() {
        html.push_str(r#"<div class="stat-card"><p class="text-slate-400 text-sm">通勤圏データを取得できませんでした</p></div></div>"#);
        return html;
    }

    // 通勤圏ピラミッド集約
    let zone_pyramid = fetch_commute_zone_pyramid(db, turso, &zone);
    // 選択市区町村単体のピラミッド
    let local_pyramid = fetch_population_pyramid(db, turso, pref, muni);
    // 空間ミスマッチ
    let spatial = fetch_spatial_mismatch(db, pref, muni);

    // 都道府県カウント
    let mut pref_set = std::collections::HashSet::new();
    for m in &zone {
        pref_set.insert(m.prefecture.as_str());
    }
    let pref_count = pref_set.len();

    // 通勤圏人口集計
    let mut zone_total_pop: i64 = 0;
    let mut zone_working_age: i64 = 0;
    let mut zone_elderly: i64 = 0;
    for row in &zone_pyramid {
        let male = get_i64(row, "male_count");
        let female = get_i64(row, "female_count");
        let total = male + female;
        zone_total_pop += total;
        let age = get_str(row, "age_group");
        match age {
            "15-19" | "20-24" | "25-29" | "30-34" | "35-39" | "40-44" | "45-49" | "50-54"
            | "55-59" | "60-64" | "10-19" | "20-29" | "30-39" | "40-49" | "50-59" | "60-69" => {
                zone_working_age += total
            }
            _ => {}
        }
        match age {
            "65-69" | "70-74" | "75-79" | "80-84" | "85+" | "70-79" | "80+" => {
                zone_elderly += total
            }
            _ => {}
        }
    }
    let aging_rate = if zone_total_pop > 0 {
        zone_elderly as f64 / zone_total_pop as f64
    } else {
        0.0
    };

    // ヘッダー
    html.push_str(&format!(
        r#"<h3 class="text-lg font-semibold text-white">🌐 通勤圏分析 <span class="text-blue-400 text-base">{pref} {muni}</span> の30km圏内</h3>
        <p class="text-xs text-slate-500">圏内市区町村: {}件（{}県にまたがる）</p>"#,
        zone.len(), pref_count
    ));

    // KPIカード
    html.push_str(r#"<div class="grid grid-cols-2 md:grid-cols-4 gap-3">"#);
    kpi(
        &mut html,
        "圏内総人口",
        &format!("{}人", format_number(zone_total_pop)),
        "text-blue-400",
    );
    kpi(
        &mut html,
        "生産年齢人口",
        &format!("{}人", format_number(zone_working_age)),
        "text-emerald-400",
    );
    kpi(
        &mut html,
        "高齢化率",
        &format!("{:.1}%", aging_rate * 100.0),
        if aging_rate > 0.30 {
            "text-red-400"
        } else if aging_rate > 0.25 {
            "text-amber-400"
        } else {
            "text-green-400"
        },
    );
    kpi(
        &mut html,
        "対象市区町村",
        &format!("{}件 / {}県", zone.len(), pref_count),
        "text-cyan-400",
    );
    html.push_str("</div>");

    // 蝶形ピラミッドチャート
    if !zone_pyramid.is_empty() {
        let chart = build_butterfly_pyramid(&zone_pyramid, &local_pyramid, muni);
        html.push_str(&format!(
            r#"<div class="stat-card">
                <h4 class="text-sm text-slate-400 mb-2">性別×年齢 人口ピラミッド（通勤圏 vs {muni}）</h4>
                <div class="echart" style="height:500px;" data-chart-config='{chart}'></div>
                <div class="text-xs text-slate-600 mt-1">※通勤圏(30km圏内)の全市区町村人口を合算</div>
            </div>"#
        ));
    }

    // 空間ミスマッチ情報
    if !spatial.is_empty() {
        html.push_str(
            r#"<div class="stat-card"><h4 class="text-sm text-slate-400 mb-2">通勤圏求人市場</h4>"#,
        );
        html.push_str(r#"<div class="grid grid-cols-2 md:grid-cols-4 gap-3">"#);
        // 正社員優先、なければfirst()にフォールバック
        let (sm_row, is_fallback) =
            if let Some(row) = spatial.iter().find(|r| get_str(r, "emp_group") == "正社員") {
                (Some(row), false)
            } else {
                (spatial.first(), true)
            };
        if let Some(row) = sm_row {
            if is_fallback {
                let grp = get_str(row, "emp_group");
                html.push_str(&format!(r#"<p class="text-xs text-amber-400 mb-2">※正社員データなし。{}のデータを表示</p>"#, escape_html(grp)));
            }
            let acc30 = get_i64(row, "accessible_postings_30km");
            let local = get_i64(row, "posting_count");
            let gap = get_f64(row, "salary_gap_vs_accessible");
            let iso = get_f64(row, "isolation_score");
            kpi(
                &mut html,
                "30km圏内求人数",
                &format_number(acc30),
                "text-blue-400",
            );
            kpi(&mut html, "地元求人数", &format_number(local), "text-white");
            kpi(
                &mut html,
                "給与差(対圏内)",
                &format!("{:+.0}円", gap),
                if gap < 0.0 {
                    "text-red-400"
                } else {
                    "text-green-400"
                },
            );
            kpi(
                &mut html,
                "孤立スコア",
                &format!("{:.2}", iso),
                if iso > 0.5 {
                    "text-red-400"
                } else {
                    "text-green-400"
                },
            );
        }
        html.push_str("</div></div>");
    }

    // 圏内市区町村テーブル
    html.push_str(
        r#"<div class="stat-card"><h4 class="text-sm text-slate-400 mb-2">圏内市区町村一覧</h4>"#,
    );
    html.push_str(r#"<div class="overflow-x-auto max-h-80"><table class="w-full text-xs">"#);
    html.push_str(
        r#"<thead><tr class="text-slate-500 border-b border-slate-700">
        <th class="text-left py-1 px-2">県</th>
        <th class="text-left py-1 px-2">市区町村</th>
        <th class="text-right py-1 px-2">距離</th>
    </tr></thead><tbody>"#,
    );
    for m in zone.iter().take(50) {
        let is_self = m.prefecture == pref && m.municipality == muni;
        let style = if is_self {
            r#" class="text-blue-400 font-medium""#
        } else {
            ""
        };
        html.push_str(&format!(
            r#"<tr class="border-b border-slate-800"><td class="py-1 px-2"{style}>{}</td><td class="py-1 px-2"{style}>{}</td><td class="text-right py-1 px-2">{:.1}km</td></tr>"#,
            escape_html(&m.prefecture), escape_html(&m.municipality), m.distance_km
        ));
    }
    html.push_str("</tbody></table></div></div>");

    // ======== 通勤フロー（実データ: 国勢調査OD） ========
    let inflow = fetch_commute_inflow(db, pref, muni);
    let outflow = fetch_commute_outflow(db, pref, muni);

    if !inflow.is_empty() || !outflow.is_empty() {
        let self_rate = fetch_self_commute_rate(db, pref, muni);

        // サンキーダイアグラム
        html.push_str(r#"<div class="stat-card"><h4 class="text-sm text-slate-400 mb-2">🔄 通勤フロー（国勢調査実データ）</h4>"#);

        if !inflow.is_empty() || !outflow.is_empty() {
            let sankey = build_commute_sankey(&inflow, &outflow, muni);
            html.push_str(&format!(
                r#"<div class="echart" style="height:400px;" data-chart-config='{sankey}'></div>"#
            ));
        }

        // 地元就業率
        html.push_str(&format!(
            r#"<div class="text-xs text-slate-500 mt-2">地元就業率: {:.1}%（住民のうち同市区町村内で働く人の割合）※2020年国勢調査</div>"#,
            self_rate * 100.0
        ));
        html.push_str("</div>");

        // 流入元複合テーブル
        if !inflow.is_empty() {
            html.push_str(r#"<div class="stat-card"><h4 class="text-sm text-slate-400 mb-2">📥 通勤流入元 Top 10（実フロー）</h4>"#);
            html.push_str(r#"<div class="overflow-x-auto"><table class="w-full text-xs">"#);
            html.push_str(
                r#"<thead><tr class="text-slate-500 border-b border-slate-700">
                <th class="text-left py-1 px-2">流入元</th>
                <th class="text-right py-1 px-2">通勤者数</th>
                <th class="text-right py-1 px-2">男性</th>
                <th class="text-right py-1 px-2">女性</th>
            </tr></thead><tbody>"#,
            );
            for f in inflow.iter().take(10) {
                let loc = format!(
                    "{}{}",
                    escape_html(&f.partner_pref),
                    escape_html(&f.partner_muni)
                );
                let cross_pref = if f.partner_pref != pref { " 🔀" } else { "" };
                html.push_str(&format!(
                    r#"<tr class="border-b border-slate-800">
                        <td class="py-1 px-2 text-white">{loc}{cross_pref}</td>
                        <td class="text-right py-1 px-2 text-blue-400 font-mono">{}</td>
                        <td class="text-right py-1 px-2 text-slate-400">{}</td>
                        <td class="text-right py-1 px-2 text-slate-400">{}</td>
                    </tr>"#,
                    format_number(f.total_commuters),
                    format_number(f.male_commuters),
                    format_number(f.female_commuters),
                ));
            }
            html.push_str("</tbody></table></div>");
            html.push_str(r#"<div class="text-xs text-slate-600 mt-1">🔀 = 都道府県またぎ ※2020年国勢調査</div>"#);
            html.push_str("</div>");
        }
    }

    html.push_str("</div>");
    html
}

/// サンキーダイアグラムECharts JSON生成
fn build_commute_sankey(
    inflow: &[CommuteFlow],
    outflow: &[CommuteFlow],
    center_name: &str,
) -> String {
    use serde_json::json;

    let mut node_values: Vec<serde_json::Value> = vec![json!({"name": center_name})];
    let mut link_values: Vec<serde_json::Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // 流入（左→中央）
    for f in inflow.iter().take(10) {
        let name = format!("{}{}", f.partner_pref, f.partner_muni);
        if !seen.contains(&name) {
            node_values.push(json!({"name": &name}));
            seen.insert(name.clone());
        }
        link_values.push(json!({
            "source": &name,
            "target": center_name,
            "value": f.total_commuters
        }));
    }

    // 流出（中央→右）
    for f in outflow.iter().take(10) {
        let name = format!("{}{}(流出)", f.partner_pref, f.partner_muni);
        if !seen.contains(&name) {
            node_values.push(json!({"name": &name}));
            seen.insert(name.clone());
        }
        link_values.push(json!({
            "source": center_name,
            "target": &name,
            "value": f.total_commuters
        }));
    }

    let config = json!({
        "tooltip": {"trigger": "item"},
        "series": [{
            "type": "sankey",
            "layout": "none",
            "emphasis": {"focus": "adjacency"},
            "nodeAlign": "justify",
            "data": node_values,
            "links": link_values,
            "lineStyle": {"color": "gradient", "curveness": 0.5},
            "label": {"color": "#e2e8f0", "fontSize": 10}
        }]
    });
    config.to_string().replace('\'', "&#x27;")
}

/// 蝶形ピラミッドECharts JSON生成
fn build_butterfly_pyramid(zone: &[Row], local: &[Row], muni_name: &str) -> String {
    use serde_json::json;

    let ages: Vec<String> = zone
        .iter()
        .map(|r| get_str(r, "age_group").to_string())
        .collect();

    let zone_male: Vec<i64> = zone.iter().map(|r| -get_i64(r, "male_count")).collect();
    let zone_female: Vec<i64> = zone.iter().map(|r| get_i64(r, "female_count")).collect();

    // ローカルピラミッド（年齢順にマッチング）
    let local_map: std::collections::HashMap<String, (i64, i64)> = local
        .iter()
        .map(|r| {
            let age = get_str(r, "age_group").to_string();
            let m = get_i64(r, "male_count");
            let f = get_i64(r, "female_count");
            (age, (m, f))
        })
        .collect();

    let local_male: Vec<i64> = ages
        .iter()
        .map(|a| -local_map.get(a).map(|(m, _)| *m).unwrap_or(0))
        .collect();
    let local_female: Vec<i64> = ages
        .iter()
        .map(|a| local_map.get(a).map(|(_, f)| *f).unwrap_or(0))
        .collect();

    let legend_male_local = format!("男性({})", muni_name);
    let legend_female_local = format!("女性({})", muni_name);

    let config = json!({
        "tooltip": {"trigger": "axis", "axisPointer": {"type": "shadow"}},
        "legend": {
            "data": ["男性(通勤圏)", "女性(通勤圏)", &legend_male_local, &legend_female_local],
            "textStyle": {"color": "#94a3b8", "fontSize": 10},
            "bottom": 0
        },
        "grid": {"left": "3%", "right": "3%", "top": "3%", "bottom": "12%", "containLabel": true},
        "xAxis": {"type": "value"},
        "yAxis": {"type": "category", "data": ages, "axisTick": {"show": false}},
        "series": [
            {
                "name": "男性(通勤圏)",
                "type": "bar",
                "data": zone_male,
                "itemStyle": {"color": "rgba(59,130,246,0.7)"}
            },
            {
                "name": "女性(通勤圏)",
                "type": "bar",
                "data": zone_female,
                "itemStyle": {"color": "rgba(236,72,153,0.7)"}
            },
            {
                "name": &legend_male_local,
                "type": "bar",
                "data": local_male,
                "barGap": "-100%",
                "itemStyle": {"color": "rgba(59,130,246,0.3)"}
            },
            {
                "name": &legend_female_local,
                "type": "bar",
                "data": local_female,
                "barGap": "-100%",
                "itemStyle": {"color": "rgba(236,72,153,0.3)"}
            }
        ]
    });
    config.to_string().replace('\'', "&#x27;")
}

/// KPIカード描画ヘルパ（subtab7 内のみで使用）
fn kpi(html: &mut String, label: &str, value: &str, color: &str) {
    html.push_str(&format!(
        r#"<div class="stat-card text-center">
            <div class="text-lg font-bold {color}">{value}</div>
            <div class="text-xs text-slate-500">{label}</div>
        </div>"#
    ));
}
