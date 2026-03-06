use axum::extract::{Query, State};
use axum::response::Html;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tower_sessions::Session;

use crate::AppState;
use crate::handlers::competitive::escape_html;
use crate::handlers::overview::{get_session_filters, SessionFilters};

/// 地域フィルタ（産業+都道府県+市区町村）のWHERE句と連番?パラメータを構築
fn build_region_filter(filters: &SessionFilters, pref: &str, muni: &str) -> (String, Vec<String>) {
    let mut clause = String::from("1=1");
    let mut params: Vec<String> = Vec::new();
    filters.append_industry_filter_str(&mut clause, &mut params);
    clause.push_str(" AND prefecture = ?");
    params.push(pref.to_string());
    clause.push_str(" AND municipality = ?");
    params.push(muni.to_string());
    (clause, params)
}

#[derive(Deserialize)]
pub struct RegionParams {
    #[serde(default)]
    pub prefecture: String,
    #[serde(default)]
    pub municipality: String,
}

// --- 1. 地域サマリー（postingsテーブルから集計） ---

pub async fn region_summary(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(params): Query<RegionParams>,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    if params.prefecture.is_empty() || params.municipality.is_empty() {
        return Html(r#"<p class="text-gray-400 text-xs">地域を選択してください</p>"#.to_string());
    }

    let db = match &state.hw_db {
        Some(db) => db,
        None => return Html(r#"<p class="text-gray-400 text-xs">データベースなし</p>"#.to_string()),
    };

    let (where_clause, filter_params) = build_region_filter(&filters, &params.prefecture, &params.municipality);

    // postingsテーブルから求人件数・給与統計を集計
    let sql = format!(
        "SELECT COUNT(*) as cnt, \
         AVG(salary_min) as avg_sal_min, AVG(salary_max) as avg_sal_max \
         FROM postings WHERE {where_clause}"
    );
    let bind: Vec<&dyn rusqlite::types::ToSql> = filter_params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();

    let rows = match db.query(&sql, &bind) {
        Ok(r) => r,
        Err(_) => {
            return Html(r#"<p class="text-gray-400 text-xs">データ取得エラー</p>"#.to_string());
        }
    };

    if rows.is_empty() {
        return Html(format!(
            r#"<p class="text-gray-400 text-xs">{}の{}データがありません</p>"#,
            escape_html(&params.municipality),
            escape_html(&filters.industry_label())
        ));
    }

    let row = &rows[0];
    let posting_count = get_i64(row, "cnt");
    let avg_sal_min = get_f64(row, "avg_sal_min");
    let avg_sal_max = get_f64(row, "avg_sal_max");

    // 雇用形態別件数
    let emp_sql = format!(
        "SELECT employment_type, COUNT(*) as cnt FROM postings \
         WHERE {where_clause} \
         AND employment_type IS NOT NULL AND employment_type != '' \
         GROUP BY employment_type ORDER BY cnt DESC LIMIT 3"
    );
    let emp_bind: Vec<&dyn rusqlite::types::ToSql> = filter_params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    let emp_info = if let Ok(emp_rows) = db.query(&emp_sql, &emp_bind) {
        emp_rows.iter().map(|r| {
            format!("{}: {}件", get_str(r, "employment_type"), get_i64(r, "cnt"))
        }).collect::<Vec<_>>().join(", ")
    } else {
        String::new()
    };

    let html = format!(
        r#"<div class="grid grid-cols-2 gap-2 text-xs">
  <div class="bg-gray-700/50 rounded p-2 text-center">
    <div class="text-gray-400">求人件数</div>
    <div class="text-lg font-bold text-blue-300">{}</div>
  </div>
  <div class="bg-gray-700/50 rounded p-2 text-center">
    <div class="text-gray-400">平均給与下限</div>
    <div class="text-lg font-bold text-yellow-300">{}</div>
  </div>
  <div class="bg-gray-700/50 rounded p-2 text-center">
    <div class="text-gray-400">平均給与上限</div>
    <div class="text-lg font-bold text-green-300">{}</div>
  </div>
  <div class="bg-gray-700/50 rounded p-2 col-span-2 text-center">
    <div class="text-gray-400">主要雇用形態</div>
    <div class="text-sm font-bold text-purple-300">{}</div>
  </div>
</div>"#,
        posting_count,
        format_yen(avg_sal_min as i64),
        format_yen(avg_sal_max as i64),
        if emp_info.is_empty() { "データなし".to_string() } else { emp_info }
    );

    Html(html)
}

// --- 2. 雇用形態別求人（postingsテーブルから集計） ---

pub async fn region_age_gender(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(params): Query<RegionParams>,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    if params.prefecture.is_empty() || params.municipality.is_empty() {
        return Html(r#"<p class="text-gray-400 text-xs">地域を選択してください</p>"#.to_string());
    }

    let db = match &state.hw_db {
        Some(db) => db,
        None => return Html(r#"<p class="text-gray-400 text-xs">データベースなし</p>"#.to_string()),
    };

    let (where_clause, filter_params) = build_region_filter(&filters, &params.prefecture, &params.municipality);

    // postingsテーブルから雇用形態別・給与区分別の件数を集計
    let sql = format!(
        "SELECT employment_type, COUNT(*) as cnt FROM postings \
         WHERE {where_clause} \
         AND employment_type IS NOT NULL AND employment_type != '' \
         GROUP BY employment_type ORDER BY cnt DESC"
    );
    let bind: Vec<&dyn rusqlite::types::ToSql> = filter_params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();

    let rows = match db.query(&sql, &bind) {
        Ok(r) => r,
        Err(_) => {
            return Html(r#"<p class="text-gray-400 text-xs">データ取得エラー</p>"#.to_string());
        }
    };

    if rows.is_empty() {
        return Html(r#"<p class="text-gray-400 text-xs">雇用形態データなし</p>"#.to_string());
    }

    // EChartsのデータ構築（横棒グラフ）
    let mut categories = Vec::new();
    let mut values = Vec::new();

    for row in &rows {
        let et = get_str(row, "employment_type");
        let cnt = get_i64(row, "cnt");
        categories.push(et);
        values.push(cnt);
    }

    let cats_json = categories
        .iter()
        .map(|c| format!("'{}'", escape_html(c)))
        .collect::<Vec<_>>()
        .join(",");
    let vals_json = values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",");

    // ユニークIDを生成（ECharts描画用）
    let chart_id = format!("region-emp-type-{}", rand_id());

    let html = format!(
        r#"<div id="{chart_id}" style="width:100%;height:220px;"></div>
<script>
(function(){{
  var el = document.getElementById('{chart_id}');
  if (!el || typeof echarts === 'undefined') return;
  var ch = echarts.init(el, 'dark');
  ch.setOption({{
    tooltip: {{ trigger: 'axis', axisPointer: {{ type: 'shadow' }} }},
    grid: {{ left: 80, right: 20, top: 10, bottom: 20 }},
    xAxis: {{ type: 'value' }},
    yAxis: {{ type: 'category', data: [{cats_json}], axisTick: {{ show: false }} }},
    series: [
      {{ name: '求人件数', type: 'bar', data: [{vals_json}],
         itemStyle: {{ color: '#3b82f6' }}, barWidth: '60%' }}
    ]
  }});
  new ResizeObserver(function(){{ ch.resize(); }}).observe(el);
}})();
</script>"#
    );

    Html(html)
}

// --- 3. 求人統計（postingsテーブル） ---

pub async fn region_posting_stats(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(params): Query<RegionParams>,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let geocoded_db = match &state.hw_db {
        Some(db) => db,
        None => {
            return Html(r#"<p class="text-gray-400 text-xs">求人DBなし</p>"#.to_string());
        }
    };

    if params.prefecture.is_empty() || params.municipality.is_empty() {
        return Html(r#"<p class="text-gray-400 text-xs">地域を選択してください</p>"#.to_string());
    }

    let (where_clause, filter_params) = build_region_filter(&filters, &params.prefecture, &params.municipality);
    let mk_bind = || -> Vec<&dyn rusqlite::types::ToSql> {
        filter_params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect()
    };

    let mut html = String::with_capacity(2048);

    // 雇用形態別件数
    let emp_sql = format!("SELECT employment_type, COUNT(*) as cnt FROM postings WHERE {where_clause} GROUP BY employment_type ORDER BY cnt DESC");
    let emp_rows = geocoded_db.query(&emp_sql, &mk_bind());

    html.push_str(r#"<div class="space-y-3 text-xs">"#);

    // 雇用形態テーブル
    html.push_str(r#"<div><div class="text-gray-400 mb-1 font-medium">雇用形態</div>"#);
    html.push_str(r#"<table class="w-full"><tbody>"#);
    if let Ok(rows) = &emp_rows {
        for row in rows {
            let emp = get_str(row, "employment_type");
            let cnt = get_i64(row, "cnt");
            html.push_str(&format!(
                r#"<tr><td class="text-gray-300 py-0.5">{}</td><td class="text-right text-white font-medium">{}件</td></tr>"#,
                escape_html(&emp),
                cnt
            ));
        }
    }
    html.push_str("</tbody></table></div>");

    // 給与統計
    let salary_sql = format!("SELECT salary_type, AVG(salary_min) as avg_min, AVG(salary_max) as avg_max, MIN(salary_min) as min_min, MAX(salary_max) as max_max, COUNT(*) as cnt FROM postings WHERE {where_clause} AND salary_min > 0 GROUP BY salary_type");
    let salary_rows = geocoded_db.query(&salary_sql, &mk_bind());

    html.push_str(r#"<div><div class="text-gray-400 mb-1 font-medium">給与レンジ</div>"#);
    html.push_str(r#"<table class="w-full"><thead><tr class="text-gray-500"><th class="text-left">区分</th><th class="text-right">平均下限</th><th class="text-right">平均上限</th><th class="text-right">件</th></tr></thead><tbody>"#);
    if let Ok(rows) = &salary_rows {
        for row in rows {
            let st = get_str(row, "salary_type");
            let avg_min = get_f64(row, "avg_min");
            let avg_max = get_f64(row, "avg_max");
            let cnt = get_i64(row, "cnt");
            html.push_str(&format!(
                r#"<tr><td class="text-gray-300 py-0.5">{}</td><td class="text-right text-yellow-300">{}</td><td class="text-right text-yellow-300">{}</td><td class="text-right text-white">{}</td></tr>"#,
                escape_html(&st),
                format_yen(avg_min as i64),
                format_yen(avg_max as i64),
                cnt
            ));
        }
    }
    html.push_str("</tbody></table></div>");

    // 産業別TOP5
    let ind_sql = format!("SELECT job_type, COUNT(*) as cnt FROM postings WHERE {where_clause} AND job_type != '' GROUP BY job_type ORDER BY cnt DESC LIMIT 5");
    let ind_rows = geocoded_db.query(&ind_sql, &mk_bind());

    html.push_str(r#"<div><div class="text-gray-400 mb-1 font-medium">産業別 TOP5</div>"#);
    if let Ok(rows) = &ind_rows {
        let max_cnt = rows.first().map(|r| get_i64(r, "cnt")).unwrap_or(1).max(1);
        for row in rows {
            let ind = get_str(row, "job_type");
            let cnt = get_i64(row, "cnt");
            let pct = (cnt as f64 / max_cnt as f64) * 100.0;
            html.push_str(&format!(
                r#"<div class="flex items-center gap-2 py-0.5">
  <span class="text-gray-300 w-32 truncate" title="{full}">{label}</span>
  <div class="flex-1 bg-gray-700 rounded h-3">
    <div class="bg-blue-500 rounded h-3" style="width:{pct:.0}%"></div>
  </div>
  <span class="text-white w-8 text-right">{cnt}</span>
</div>"#,
                full = escape_html(&ind),
                label = escape_html(&truncate(&ind, 16)),
                pct = pct,
                cnt = cnt
            ));
        }
    }
    html.push_str("</div>");

    html.push_str("</div>");

    Html(html)
}

// --- 4. セグメント分析（postingsテーブル） ---

pub async fn region_segments(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(params): Query<RegionParams>,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let geocoded_db = match &state.hw_db {
        Some(db) => db,
        None => {
            return Html(r#"<p class="text-gray-400 text-xs">求人DBなし</p>"#.to_string());
        }
    };

    if params.prefecture.is_empty() || params.municipality.is_empty() {
        return Html(r#"<p class="text-gray-400 text-xs">地域を選択してください</p>"#.to_string());
    }

    let (where_clause, filter_params) = build_region_filter(&filters, &params.prefecture, &params.municipality);
    let mk_bind = || -> Vec<&dyn rusqlite::types::ToSql> {
        filter_params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect()
    };

    let mut html = String::with_capacity(2048);
    html.push_str(r#"<div class="space-y-3 text-xs">"#);

    // Tier3セグメント分布TOP10
    let tier3_sql = format!("SELECT tier3_label_short, COUNT(*) as cnt FROM postings WHERE {where_clause} AND tier3_label_short != '' GROUP BY tier3_label_short ORDER BY cnt DESC LIMIT 10");
    let tier3_rows = geocoded_db.query(&tier3_sql, &mk_bind());

    html.push_str(r#"<div><div class="text-gray-400 mb-1 font-medium">求人セグメント TOP10</div>"#);
    if let Ok(rows) = &tier3_rows {
        if rows.is_empty() {
            html.push_str(r#"<p class="text-gray-500">データなし</p>"#);
        } else {
            let max_cnt = rows.first().map(|r| get_i64(r, "cnt")).unwrap_or(1).max(1);
            for row in rows {
                let label = get_str(row, "tier3_label_short");
                let cnt = get_i64(row, "cnt");
                let pct = (cnt as f64 / max_cnt as f64) * 100.0;
                html.push_str(&format!(
                    r#"<div class="flex items-center gap-2 py-0.5">
  <span class="text-gray-300 w-36 truncate" title="{full}">{short}</span>
  <div class="flex-1 bg-gray-700 rounded h-3">
    <div class="bg-emerald-500 rounded h-3" style="width:{pct:.0}%"></div>
  </div>
  <span class="text-white w-8 text-right">{cnt}</span>
</div>"#,
                    full = escape_html(&label),
                    short = escape_html(&truncate(&label, 20)),
                    pct = pct,
                    cnt = cnt
                ));
            }
        }
    }
    html.push_str("</div>");

    // 雇用形態別分布
    let emp_sql = format!("SELECT employment_type, COUNT(*) as cnt FROM postings WHERE {where_clause} AND employment_type != '' GROUP BY employment_type ORDER BY cnt DESC LIMIT 10");
    let emp_rows = geocoded_db.query(&emp_sql, &mk_bind());

    html.push_str(r#"<div><div class="text-gray-400 mb-1 font-medium">雇用形態別分布</div>"#);
    if let Ok(rows) = &emp_rows {
        if rows.is_empty() {
            html.push_str(r#"<p class="text-gray-500">データなし</p>"#);
        } else {
            let max_cnt = rows.first().map(|r| get_i64(r, "cnt")).unwrap_or(1).max(1);
            for row in rows {
                let label = get_str(row, "employment_type");
                let cnt = get_i64(row, "cnt");
                let pct = (cnt as f64 / max_cnt as f64) * 100.0;
                html.push_str(&format!(
                    r#"<div class="flex items-center gap-2 py-0.5">
  <span class="text-gray-300 w-36 truncate" title="{full}">{short}</span>
  <div class="flex-1 bg-gray-700 rounded h-3">
    <div class="bg-amber-500 rounded h-3" style="width:{pct:.0}%"></div>
  </div>
  <span class="text-white w-8 text-right">{cnt}</span>
</div>"#,
                    full = escape_html(&label),
                    short = escape_html(&truncate(&label, 20)),
                    pct = pct,
                    cnt = cnt
                ));
            }
        }
    }
    html.push_str("</div>");

    html.push_str("</div>");

    Html(html)
}

// --- ヘルパー ---

fn get_i64(row: &HashMap<String, serde_json::Value>, key: &str) -> i64 {
    row.get(key)
        .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
        .unwrap_or(0)
}

fn get_f64(row: &HashMap<String, serde_json::Value>, key: &str) -> f64 {
    row.get(key)
        .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)))
        .unwrap_or(0.0)
}

fn get_str(row: &HashMap<String, serde_json::Value>, key: &str) -> String {
    row.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn format_yen(n: i64) -> String {
    if n == 0 {
        return "\u{2212}".to_string(); // −
    }
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    format!("\u{00a5}{}", result.chars().rev().collect::<String>())
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars - 1).collect();
        format!("{}…", truncated)
    }
}

/// 簡易ユニークID生成（チャート要素の衝突回避用）
fn rand_id() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    t % 1_000_000
}
