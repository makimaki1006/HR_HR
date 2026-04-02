use axum::extract::State;
use axum::response::Html;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tower_sessions::Session;

use crate::AppState;
use crate::db::local_sqlite::LocalDb;

use super::overview::{
    build_filter_clause, format_number, get_f64, get_i64, get_session_filters, get_str,
    make_location_label, render_no_db_data, SessionFilters,
};

/// タブ2: 企業分析
pub async fn tab_balance(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let db = match &state.hw_db {
        Some(db) => db,
        None => return Html(render_no_db_data("企業分析")),
    };

    let cache_key = format!("balance_{}_{}_{}", filters.industry_cache_key(), filters.prefecture, filters.municipality);
    if let Some(cached) = state.cache.get(&cache_key) {
        if let Some(html) = cached.as_str() {
            return Html(html.to_string());
        }
    }

    let db = db.clone();
    let filters_clone = filters.clone();
    let stats = tokio::task::spawn_blocking(move || {
        fetch_balance(&db, &filters_clone)
    }).await.unwrap_or_default();

    let mut html = render_balance(&filters, &stats);
    html.push_str(r#"<div hx-get="/api/insight/widget/balance" hx-trigger="load" hx-swap="innerHTML"></div>"#);
    state.cache.set(cache_key, Value::String(html.clone()));
    Html(html)
}

struct BalanceStats {
    /// 従業員規模分布 (band_label, count)
    employee_size_dist: Vec<(String, i64)>,
    /// 資本金分布 (band_label, count)
    capital_dist: Vec<(String, i64)>,
    /// 設立年代分布 (era_label, count)
    founding_era_dist: Vec<(String, i64)>,
    /// 女性従業員比率ヒストグラム (range, count)
    female_ratio_hist: Vec<(String, i64)>,
    /// パート比率ヒストグラム (range, count)
    parttime_ratio_hist: Vec<(String, i64)>,
    /// 産業×従業員規模クロス (job_type, size_band, count) - 上位5産業
    industry_size_cross: Vec<(String, String, i64)>,
    /// 上位5産業名
    top_industries: Vec<String>,
    /// 従業員規模バンド名
    size_bands: Vec<String>,
    /// KPI
    total_postings: i64,
    total_facilities: i64,
    avg_employee_count: f64,     // 平均
    median_employee_count: f64,  // 中央値
    mode_employee_count: i64,    // 最頻値
}

impl Default for BalanceStats {
    fn default() -> Self {
        Self {
            employee_size_dist: Vec::new(),
            capital_dist: Vec::new(),
            founding_era_dist: Vec::new(),
            female_ratio_hist: Vec::new(),
            parttime_ratio_hist: Vec::new(),
            industry_size_cross: Vec::new(),
            top_industries: Vec::new(),
            size_bands: Vec::new(),
            total_postings: 0,
            total_facilities: 0,
            avg_employee_count: 0.0,
            median_employee_count: 0.0,
            mode_employee_count: 0,
        }
    }
}

fn fetch_balance(
    db: &LocalDb,
    filters: &SessionFilters,
) -> BalanceStats {
    let mut stats = BalanceStats::default();
    let (filter_clause, filter_params) = build_filter_clause(filters, 0);

    let mk_bind = || -> Vec<&dyn rusqlite::types::ToSql> {
        filter_params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect()
    };

    // 0. KPI基本 + 従業員数（平均/中央値/最頻値の3指標）
    {
        let sql = format!(
            "SELECT COUNT(*) as cnt, COUNT(DISTINCT facility_name) as fac_cnt, \
             AVG(NULLIF(employee_count, 0)) as avg_emp \
             FROM postings WHERE 1=1{filter_clause}"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            if let Some(row) = rows.first() {
                stats.total_postings = get_i64(row, "cnt");
                stats.total_facilities = get_i64(row, "fac_cnt");
                stats.avg_employee_count = get_f64(row, "avg_emp");
            }
        }
        // 中央値（SQLiteにMEDIAN関数がないためOFFSETで計算）
        let median_sql = format!(
            "SELECT employee_count FROM postings \
             WHERE 1=1{filter_clause} AND employee_count > 0 \
             ORDER BY employee_count \
             LIMIT 1 OFFSET (SELECT COUNT(*)/2 FROM postings WHERE 1=1{filter_clause} AND employee_count > 0)"
        );
        if let Ok(rows) = db.query(&median_sql, &mk_bind()) {
            if let Some(row) = rows.first() {
                stats.median_employee_count = get_f64(row, "employee_count");
            }
        }
        // 最頻値
        let mode_sql = format!(
            "SELECT employee_count, COUNT(*) as freq FROM postings \
             WHERE 1=1{filter_clause} AND employee_count > 0 \
             GROUP BY employee_count ORDER BY freq DESC LIMIT 1"
        );
        if let Ok(rows) = db.query(&mode_sql, &mk_bind()) {
            if let Some(row) = rows.first() {
                stats.mode_employee_count = get_i64(row, "employee_count");
            }
        }
    }

    // 1. 従業員規模分布
    {
        let sql = format!(
            "SELECT CASE \
               WHEN employee_count <= 5 THEN '~5人' \
               WHEN employee_count <= 20 THEN '6~20人' \
               WHEN employee_count <= 50 THEN '21~50人' \
               WHEN employee_count <= 100 THEN '51~100人' \
               WHEN employee_count <= 300 THEN '101~300人' \
               ELSE '300人~' \
             END as band, COUNT(*) as cnt \
             FROM postings WHERE 1=1{filter_clause} AND employee_count > 0 \
             GROUP BY band ORDER BY MIN(employee_count)"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            for row in &rows {
                let band = get_str(row, "band");
                let cnt = get_i64(row, "cnt");
                if !band.is_empty() {
                    stats.employee_size_dist.push((band, cnt));
                }
            }
        }
    }

    // 2. 資本金分布
    {
        let sql = format!(
            "SELECT CASE \
               WHEN capital <= 100 THEN '~100万' \
               WHEN capital <= 500 THEN '~500万' \
               WHEN capital <= 1000 THEN '~1000万' \
               WHEN capital <= 5000 THEN '~5000万' \
               WHEN capital <= 10000 THEN '~1億' \
               ELSE '1億~' \
             END as band, COUNT(*) as cnt \
             FROM postings WHERE 1=1{filter_clause} AND capital > 0 \
             GROUP BY band ORDER BY MIN(capital)"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            for row in &rows {
                let band = get_str(row, "band");
                let cnt = get_i64(row, "cnt");
                if !band.is_empty() {
                    stats.capital_dist.push((band, cnt));
                }
            }
        }
    }

    // 3. 設立年代分布
    {
        let sql = format!(
            "SELECT CASE \
               WHEN founding_year <= 1970 THEN '~1970' \
               WHEN founding_year <= 1980 THEN '1971~1980' \
               WHEN founding_year <= 1990 THEN '1981~1990' \
               WHEN founding_year <= 2000 THEN '1991~2000' \
               WHEN founding_year <= 2010 THEN '2001~2010' \
               WHEN founding_year <= 2020 THEN '2011~2020' \
               ELSE '2021~' \
             END as era, COUNT(*) as cnt \
             FROM postings WHERE 1=1{filter_clause} AND founding_year > 0 \
             GROUP BY era ORDER BY MIN(founding_year)"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            for row in &rows {
                let era = get_str(row, "era");
                let cnt = get_i64(row, "cnt");
                if !era.is_empty() {
                    stats.founding_era_dist.push((era, cnt));
                }
            }
        }
    }

    // 4. 女性従業員比率ヒストグラム
    {
        let sql = format!(
            "SELECT CASE \
               WHEN CAST(employee_count_female AS REAL) / employee_count < 0.1 THEN '~10%' \
               WHEN CAST(employee_count_female AS REAL) / employee_count < 0.2 THEN '10~20%' \
               WHEN CAST(employee_count_female AS REAL) / employee_count < 0.3 THEN '20~30%' \
               WHEN CAST(employee_count_female AS REAL) / employee_count < 0.5 THEN '30~50%' \
               WHEN CAST(employee_count_female AS REAL) / employee_count < 0.7 THEN '50~70%' \
               ELSE '70%~' \
             END as rng, COUNT(*) as cnt \
             FROM postings WHERE 1=1{filter_clause} AND employee_count > 0 AND employee_count_female > 0 \
             GROUP BY rng ORDER BY MIN(CAST(employee_count_female AS REAL) / employee_count)"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            for row in &rows {
                let rng = get_str(row, "rng");
                let cnt = get_i64(row, "cnt");
                if !rng.is_empty() {
                    stats.female_ratio_hist.push((rng, cnt));
                }
            }
        }
    }

    // 5. パート比率ヒストグラム
    {
        let sql = format!(
            "SELECT CASE \
               WHEN CAST(employee_count_parttime AS REAL) / employee_count < 0.1 THEN '~10%' \
               WHEN CAST(employee_count_parttime AS REAL) / employee_count < 0.2 THEN '10~20%' \
               WHEN CAST(employee_count_parttime AS REAL) / employee_count < 0.3 THEN '20~30%' \
               WHEN CAST(employee_count_parttime AS REAL) / employee_count < 0.5 THEN '30~50%' \
               WHEN CAST(employee_count_parttime AS REAL) / employee_count < 0.7 THEN '50~70%' \
               ELSE '70%~' \
             END as rng, COUNT(*) as cnt \
             FROM postings WHERE 1=1{filter_clause} AND employee_count > 0 AND employee_count_parttime > 0 \
             GROUP BY rng ORDER BY MIN(CAST(employee_count_parttime AS REAL) / employee_count)"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            for row in &rows {
                let rng = get_str(row, "rng");
                let cnt = get_i64(row, "cnt");
                if !rng.is_empty() {
                    stats.parttime_ratio_hist.push((rng, cnt));
                }
            }
        }
    }

    // 6. 産業×従業員規模クロス（上位5産業）
    {
        // 上位5産業を取得
        let sql_top = format!(
            "SELECT job_type, COUNT(*) as cnt FROM postings \
             WHERE 1=1{filter_clause} AND job_type IS NOT NULL AND job_type != '' \
             GROUP BY job_type ORDER BY cnt DESC LIMIT 5"
        );
        if let Ok(rows) = db.query(&sql_top, &mk_bind()) {
            for row in &rows {
                let jt = get_str(row, "job_type");
                if !jt.is_empty() {
                    stats.top_industries.push(jt);
                }
            }
        }

        // クロス集計
        let size_bands_list = vec![
            "~5人", "6~20人", "21~50人", "51~100人", "101~300人", "300人~",
        ];
        stats.size_bands = size_bands_list.iter().map(|s| s.to_string()).collect();

        let sql_cross = format!(
            "SELECT job_type, \
               CASE \
                 WHEN employee_count <= 5 THEN '~5人' \
                 WHEN employee_count <= 20 THEN '6~20人' \
                 WHEN employee_count <= 50 THEN '21~50人' \
                 WHEN employee_count <= 100 THEN '51~100人' \
                 WHEN employee_count <= 300 THEN '101~300人' \
                 ELSE '300人~' \
               END as band, COUNT(*) as cnt \
             FROM postings WHERE 1=1{filter_clause} AND employee_count > 0 \
             AND job_type IS NOT NULL AND job_type != '' \
             GROUP BY job_type, band"
        );
        if let Ok(rows) = db.query(&sql_cross, &mk_bind()) {
            for row in &rows {
                let jt = get_str(row, "job_type");
                let band = get_str(row, "band");
                let cnt = get_i64(row, "cnt");
                if !jt.is_empty() && !band.is_empty() && stats.top_industries.contains(&jt) {
                    stats.industry_size_cross.push((jt, band, cnt));
                }
            }
        }
    }

    stats
}

fn render_balance(
    filters: &SessionFilters,
    stats: &BalanceStats,
) -> String {
    let location_label = make_location_label(&filters.prefecture, &filters.municipality);
    let industry_label = filters.industry_label();

    // KPIカード
    let kpi_cards = format!(
        r##"<div class="grid-stats">
    <div class="stat-card">
        <div class="stat-value text-blue-400">{}</div>
        <div class="stat-label">総求人数</div>
    </div>
    <div class="stat-card">
        <div class="stat-value text-emerald-400">{}</div>
        <div class="stat-label">事業所数</div>
    </div>
    <div class="stat-card">
        <div class="stat-value text-amber-400">{}<span class="text-lg">人</span></div>
        <div class="stat-label">従業員数（中央値）</div>
    </div>
    <div class="stat-card">
        <div class="stat-value text-cyan-400">{}<span class="text-lg">人</span></div>
        <div class="stat-label">従業員数（平均）</div>
    </div>
    <div class="stat-card">
        <div class="stat-value text-purple-400">{}<span class="text-lg">人</span></div>
        <div class="stat-label">従業員数（最頻値）</div>
    </div>
</div>"##,
        format_number(stats.total_postings),
        format_number(stats.total_facilities),
        if stats.median_employee_count > 0.0 { format!("{:.0}", stats.median_employee_count) } else { "-".to_string() },
        if stats.avg_employee_count > 0.0 { format!("{:.0}", stats.avg_employee_count) } else { "-".to_string() },
        if stats.mode_employee_count > 0 { format_number(stats.mode_employee_count) } else { "-".to_string() },
    );

    // 従業員規模分布
    let emp_size_chart = build_bar_chart(&stats.employee_size_dist, "#3B82F6", 320);

    // 資本金分布
    let capital_chart = build_bar_chart(&stats.capital_dist, "#10B981", 320);

    // 設立年代分布
    let founding_chart = build_bar_chart(&stats.founding_era_dist, "#F59E0B", 320);

    // 女性従業員比率
    let female_ratio_chart = build_bar_chart(&stats.female_ratio_hist, "#EC4899", 280);

    // パート比率
    let parttime_ratio_chart = build_bar_chart(&stats.parttime_ratio_hist, "#8B5CF6", 280);

    // 産業×従業員規模クロス（スタックバー）
    let cross_chart = build_industry_size_cross(
        &stats.top_industries,
        &stats.size_bands,
        &stats.industry_size_cross,
    );

    format!(
        r##"<div class="space-y-6">
    <h2 class="text-xl font-bold text-white">🏢 企業分析 <span class="text-blue-400 text-base font-normal">{industry_label} / {location_label}</span></h2>
    <p class="text-xs text-slate-500 -mt-4">事業所の規模・資本金・設立年代・従業員構成を分析します</p>

    {kpi_cards}

    <div class="grid-charts">
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">従業員規模分布</h3>
            {emp_size_chart}
        </div>
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">資本金分布（万円）</h3>
            {capital_chart}
        </div>
    </div>

    <div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-3">設立年代分布</h3>
        {founding_chart}
    </div>

    <div class="grid-charts">
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">女性従業員比率</h3>
            <p class="text-xs text-slate-500 mb-2">従業員に占める女性の割合の分布</p>
            {female_ratio_chart}
        </div>
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">パート比率</h3>
            <p class="text-xs text-slate-500 mb-2">従業員に占めるパートの割合の分布</p>
            {parttime_ratio_chart}
        </div>
    </div>

    <div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-3">産業×従業員規模クロス（上位5産業）</h3>
        {cross_chart}
    </div>

</div>"##,
        industry_label = industry_label,
        location_label = location_label,
        kpi_cards = kpi_cards,
        emp_size_chart = emp_size_chart,
        capital_chart = capital_chart,
        founding_chart = founding_chart,
        female_ratio_chart = female_ratio_chart,
        parttime_ratio_chart = parttime_ratio_chart,
        cross_chart = cross_chart,
    )
}

/// 汎用棒グラフビルダー
fn build_bar_chart(data: &[(String, i64)], color: &str, height: u32) -> String {
    if data.is_empty() {
        return r##"<p class="text-slate-500 text-sm text-center py-12">データがありません</p>"##
            .to_string();
    }
    let labels: Vec<String> = data.iter().map(|(l, _)| format!("\"{}\"", l)).collect();
    let values: Vec<String> = data.iter().map(|(_, v)| v.to_string()).collect();

    format!(
        r##"<div class="echart" style="height:{height}px;" data-chart-config='{{
            "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}}},
            "grid": {{"left": "3%", "right": "4%", "bottom": "8%", "top": "5%", "containLabel": true}},
            "xAxis": {{"type": "category", "data": [{labels}], "axisLabel": {{"rotate": 30}}}},
            "yAxis": {{"type": "value"}},
            "series": [{{
                "type": "bar",
                "data": [{values}],
                "itemStyle": {{"color": "{color}", "borderRadius": [4, 4, 0, 0]}},
                "label": {{"show": true, "position": "top", "color": "#e2e8f0", "fontSize": 11}}
            }}]
        }}'></div>"##,
        height = height,
        labels = labels.join(","),
        values = values.join(","),
        color = color,
    )
}

/// 産業×従業員規模のスタック横棒グラフ
fn build_industry_size_cross(
    top_industries: &[String],
    size_bands: &[String],
    cross_data: &[(String, String, i64)],
) -> String {
    if top_industries.is_empty() || cross_data.is_empty() {
        return r##"<p class="text-slate-500 text-sm text-center py-12">データがありません</p>"##
            .to_string();
    }

    // ピボットテーブル構築
    let mut pivot: HashMap<(&str, &str), i64> = HashMap::new();
    for (jt, band, cnt) in cross_data {
        pivot.insert((jt.as_str(), band.as_str()), *cnt);
    }

    let band_colors = [
        "#3B82F6", "#10B981", "#F59E0B", "#EF4444", "#8B5CF6", "#EC4899",
    ];

    let industry_labels: Vec<String> = top_industries
        .iter()
        .rev()
        .map(|s| format!("\"{}\"", s))
        .collect();

    let series: Vec<String> = size_bands
        .iter()
        .enumerate()
        .map(|(i, band)| {
            let data: Vec<String> = top_industries
                .iter()
                .rev()
                .map(|jt| {
                    let val = pivot.get(&(jt.as_str(), band.as_str())).copied().unwrap_or(0);
                    val.to_string()
                })
                .collect();
            let color = band_colors.get(i).unwrap_or(&"#999");
            format!(
                r##"{{"name": "{band}", "type": "bar", "stack": "total", "data": [{data}], "itemStyle": {{"color": "{color}"}}}}"##,
                band = band,
                data = data.join(","),
                color = color,
            )
        })
        .collect();

    format!(
        r##"<div class="echart" style="height:400px;" data-chart-config='{{
            "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}}},
            "legend": {{"data": [{legend}], "top": "0%"}},
            "grid": {{"left": "20%", "right": "5%", "top": "12%", "bottom": "5%"}},
            "xAxis": {{"type": "value"}},
            "yAxis": {{"type": "category", "data": [{labels}]}},
            "series": [{series}]
        }}'></div>"##,
        legend = size_bands
            .iter()
            .map(|s| format!("\"{}\"", s))
            .collect::<Vec<_>>()
            .join(","),
        labels = industry_labels.join(","),
        series = series.join(","),
    )
}
