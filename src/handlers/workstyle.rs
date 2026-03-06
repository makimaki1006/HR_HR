use axum::extract::State;
use axum::response::Html;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tower_sessions::Session;

use crate::AppState;
use crate::db::local_sqlite::LocalDb;

use super::overview::{
    build_filter_clause, format_number, get_i64, get_session_filters, get_str,
    make_location_label, render_no_db_data, SessionFilters,
};

/// タブ3: 求人条件分析
pub async fn tab_workstyle(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let db = match &state.hw_db {
        Some(db) => db,
        None => return Html(render_no_db_data("求人条件")),
    };

    let cache_key = format!("workstyle_{}_{}", filters.industry_cache_key(), filters.prefecture);
    if let Some(cached) = state.cache.get(&cache_key) {
        if let Some(html) = cached.as_str() {
            return Html(html.to_string());
        }
    }

    let stats = fetch_workstyle(db, &filters);
    let html = render_workstyle(&filters, &stats);
    state.cache.set(cache_key, Value::String(html.clone()));
    Html(html)
}

struct WorkstyleStats {
    total_postings: i64,
    /// 雇用形態分布 (employment_type, count)
    distribution: Vec<(String, i64)>,
    /// 雇用形態×給与区分 (employment_type, salary_type, count)
    salary_cross: Vec<(String, String, i64)>,
    /// 賞与あり率
    bonus_rate: f64,
    bonus_count: i64,
    /// 昇給あり率
    raise_rate: f64,
    raise_count: i64,
    /// 退職金あり率
    retirement_rate: f64,
    retirement_count: i64,
    /// 社会保険加入率 (insurance_name, rate, count)
    insurance_rates: Vec<(String, f64, i64)>,
    /// 週休二日制分布 (label, count)
    weekly_holiday_dist: Vec<(String, i64)>,
    /// 年間休日分布 (range, count)
    annual_holiday_hist: Vec<(String, i64)>,
    /// 月平均残業時間分布 (range, count)
    overtime_hist: Vec<(String, i64)>,
    /// テレワーク対応率
    telework_rate: f64,
    telework_count: i64,
    /// 託児施設あり率
    childcare_rate: f64,
    childcare_count: i64,
    /// 入居住宅あり率
    housing_rate: f64,
    housing_count: i64,
}

impl Default for WorkstyleStats {
    fn default() -> Self {
        Self {
            total_postings: 0,
            distribution: Vec::new(),
            salary_cross: Vec::new(),
            bonus_rate: 0.0,
            bonus_count: 0,
            raise_rate: 0.0,
            raise_count: 0,
            retirement_rate: 0.0,
            retirement_count: 0,
            insurance_rates: Vec::new(),
            weekly_holiday_dist: Vec::new(),
            annual_holiday_hist: Vec::new(),
            overtime_hist: Vec::new(),
            telework_rate: 0.0,
            telework_count: 0,
            childcare_rate: 0.0,
            childcare_count: 0,
            housing_rate: 0.0,
            housing_count: 0,
        }
    }
}

fn fetch_workstyle(
    db: &LocalDb,
    filters: &SessionFilters,
) -> WorkstyleStats {
    let mut stats = WorkstyleStats::default();
    let (filter_clause, filter_params) = build_filter_clause(filters, 0);

    let mk_bind = || -> Vec<&dyn rusqlite::types::ToSql> {
        filter_params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect()
    };

    // 0. 総件数
    {
        let sql = format!("SELECT COUNT(*) as cnt FROM postings WHERE 1=1{filter_clause}");
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            if let Some(row) = rows.first() {
                stats.total_postings = get_i64(row, "cnt");
            }
        }
    }

    // 1. 雇用形態分布
    {
        let sql = format!(
            "SELECT employment_type, COUNT(*) as cnt FROM postings \
             WHERE 1=1{filter_clause} AND employment_type IS NOT NULL AND employment_type != '' \
             GROUP BY employment_type ORDER BY cnt DESC"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            for row in &rows {
                let et = get_str(row, "employment_type");
                let cnt = get_i64(row, "cnt");
                if !et.is_empty() {
                    stats.distribution.push((et, cnt));
                }
            }
        }
    }

    // 2. 雇用形態×給与区分
    {
        let sql = format!(
            "SELECT employment_type, salary_type, COUNT(*) as cnt FROM postings \
             WHERE 1=1{filter_clause} AND employment_type IS NOT NULL AND employment_type != '' \
             AND salary_type IS NOT NULL AND salary_type != '' \
             GROUP BY employment_type, salary_type ORDER BY employment_type, cnt DESC"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            for row in &rows {
                let et = get_str(row, "employment_type");
                let st = get_str(row, "salary_type");
                let cnt = get_i64(row, "cnt");
                if !et.is_empty() && !st.is_empty() {
                    stats.salary_cross.push((et, st, cnt));
                }
            }
        }
    }

    let total = stats.total_postings.max(1) as f64;

    // 3. 賞与あり率（bonus_code または has_賞与）
    {
        let sql = format!(
            "SELECT SUM(CASE WHEN bonus_code IS NOT NULL AND bonus_code != '' AND bonus_code != '0' THEN 1 \
                              WHEN \"has_賞与\" = 1 THEN 1 ELSE 0 END) as cnt \
             FROM postings WHERE 1=1{filter_clause}"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            if let Some(row) = rows.first() {
                stats.bonus_count = get_i64(row, "cnt");
                stats.bonus_rate = stats.bonus_count as f64 / total * 100.0;
            }
        }
    }

    // 4. 昇給あり率（raise_code または has_昇給）
    {
        let sql = format!(
            "SELECT SUM(CASE WHEN raise_code IS NOT NULL AND raise_code != '' AND raise_code != '0' THEN 1 \
                              WHEN \"has_昇給\" = 1 THEN 1 ELSE 0 END) as cnt \
             FROM postings WHERE 1=1{filter_clause}"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            if let Some(row) = rows.first() {
                stats.raise_count = get_i64(row, "cnt");
                stats.raise_rate = stats.raise_count as f64 / total * 100.0;
            }
        }
    }

    // 5. 退職金あり率（retirement_age_code または has_退職金）
    {
        let sql = format!(
            "SELECT SUM(CASE WHEN retirement_age_code IS NOT NULL AND retirement_age_code != '' AND retirement_age_code != '0' THEN 1 \
                              WHEN \"has_退職金\" = 1 THEN 1 ELSE 0 END) as cnt \
             FROM postings WHERE 1=1{filter_clause}"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            if let Some(row) = rows.first() {
                stats.retirement_count = get_i64(row, "cnt");
                stats.retirement_rate = stats.retirement_count as f64 / total * 100.0;
            }
        }
    }

    // 6. 社会保険加入率
    {
        let insurance_cols = [
            ("insurance_employment", "雇用保険"),
            ("insurance_workers_comp", "労災保険"),
            ("insurance_health", "健康保険"),
            ("insurance_pension", "厚生年金"),
        ];
        for (col, label) in &insurance_cols {
            let sql = format!(
                "SELECT SUM(CASE WHEN {col} = 1 OR {col} = '1' THEN 1 ELSE 0 END) as cnt \
                 FROM postings WHERE 1=1{filter_clause}"
            );
            if let Ok(rows) = db.query(&sql, &mk_bind()) {
                if let Some(row) = rows.first() {
                    let cnt = get_i64(row, "cnt");
                    let rate = cnt as f64 / total * 100.0;
                    stats.insurance_rates.push((label.to_string(), rate, cnt));
                }
            }
        }
    }

    // 7. 週休二日制分布
    {
        let sql = format!(
            "SELECT CASE \
               WHEN weekly_holiday_code = '1' THEN '完全週休二日制' \
               WHEN weekly_holiday_code = '2' THEN '週休二日制' \
               WHEN weekly_holiday_code = '3' THEN 'その他' \
               WHEN weekly_holiday_code IS NULL OR weekly_holiday_code = '' THEN '不明' \
               ELSE weekly_holiday_code \
             END as label, COUNT(*) as cnt \
             FROM postings WHERE 1=1{filter_clause} \
             GROUP BY label ORDER BY cnt DESC"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            for row in &rows {
                let label = get_str(row, "label");
                let cnt = get_i64(row, "cnt");
                if !label.is_empty() {
                    stats.weekly_holiday_dist.push((label, cnt));
                }
            }
        }
    }

    // 8. 年間休日分布
    {
        let sql = format!(
            "SELECT CASE \
               WHEN annual_holidays < 80 THEN '~80日' \
               WHEN annual_holidays < 100 THEN '80~100日' \
               WHEN annual_holidays < 110 THEN '100~110日' \
               WHEN annual_holidays < 120 THEN '110~120日' \
               WHEN annual_holidays < 130 THEN '120~130日' \
               ELSE '130日~' \
             END as rng, COUNT(*) as cnt \
             FROM postings WHERE 1=1{filter_clause} AND annual_holidays > 0 \
             GROUP BY rng ORDER BY MIN(annual_holidays)"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            for row in &rows {
                let rng = get_str(row, "rng");
                let cnt = get_i64(row, "cnt");
                if !rng.is_empty() {
                    stats.annual_holiday_hist.push((rng, cnt));
                }
            }
        }
    }

    // 9. 月平均残業時間分布
    {
        let sql = format!(
            "SELECT CASE \
               WHEN overtime_monthly < 5 THEN '~5h' \
               WHEN overtime_monthly < 10 THEN '5~10h' \
               WHEN overtime_monthly < 20 THEN '10~20h' \
               WHEN overtime_monthly < 30 THEN '20~30h' \
               WHEN overtime_monthly < 45 THEN '30~45h' \
               ELSE '45h~' \
             END as rng, COUNT(*) as cnt \
             FROM postings WHERE 1=1{filter_clause} AND overtime_monthly > 0 \
             GROUP BY rng ORDER BY MIN(overtime_monthly)"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            for row in &rows {
                let rng = get_str(row, "rng");
                let cnt = get_i64(row, "cnt");
                if !rng.is_empty() {
                    stats.overtime_hist.push((rng, cnt));
                }
            }
        }
    }

    // 10. テレワーク対応率
    {
        let sql = format!(
            "SELECT SUM(CASE WHEN telework_code IS NOT NULL AND telework_code != '' AND telework_code != '0' THEN 1 \
                              WHEN \"has_在宅勤務\" = 1 THEN 1 ELSE 0 END) as cnt \
             FROM postings WHERE 1=1{filter_clause}"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            if let Some(row) = rows.first() {
                stats.telework_count = get_i64(row, "cnt");
                stats.telework_rate = stats.telework_count as f64 / total * 100.0;
            }
        }
    }

    // 11. 託児施設あり率
    {
        let sql = format!(
            "SELECT SUM(CASE WHEN childcare_facility = 1 OR childcare_facility = '1' THEN 1 ELSE 0 END) as cnt \
             FROM postings WHERE 1=1{filter_clause}"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            if let Some(row) = rows.first() {
                stats.childcare_count = get_i64(row, "cnt");
                stats.childcare_rate = stats.childcare_count as f64 / total * 100.0;
            }
        }
    }

    // 12. 入居住宅あり率
    {
        let sql = format!(
            "SELECT SUM(CASE WHEN housing_available = 1 OR housing_available = '1' THEN 1 ELSE 0 END) as cnt \
             FROM postings WHERE 1=1{filter_clause}"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            if let Some(row) = rows.first() {
                stats.housing_count = get_i64(row, "cnt");
                stats.housing_rate = stats.housing_count as f64 / total * 100.0;
            }
        }
    }

    stats
}

fn render_workstyle(
    filters: &SessionFilters,
    stats: &WorkstyleStats,
) -> String {
    let location_label = make_location_label(&filters.prefecture, &filters.municipality);
    let industry_label = filters.industry_label();

    // 雇用形態ドーナツ
    let ws_colors = |ws: &str| -> &str {
        match ws {
            "正社員" | "正職員" => "#009E73",
            "パート" | "パートタイム" => "#CC79A7",
            "契約社員" | "契約職員" => "#56B4E9",
            "派遣" | "派遣社員" => "#E69F00",
            "業務委託" => "#8b5cf6",
            "嘱託" | "嘱託社員" => "#F0E442",
            _ => "#999999",
        }
    };

    let total: i64 = stats.distribution.iter().map(|(_, c)| c).sum();

    let ws_pie: Vec<String> = stats
        .distribution
        .iter()
        .map(|(w, v)| {
            format!(
                r#"{{"value": {}, "name": "{}", "itemStyle": {{"color": "{}"}}}}"#,
                v,
                w,
                ws_colors(w)
            )
        })
        .collect();

    // KPIカード
    let kpi_cards = stats
        .distribution
        .iter()
        .map(|(ws, cnt)| {
            let pct = if total > 0 {
                (*cnt as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            let color = ws_colors(ws);
            format!(
                r#"<div class="stat-card" style="border-left: 4px solid {};">
                <div class="text-sm font-semibold text-white">{}</div>
                <div class="text-xs text-slate-400">{}件 ({:.1}%)</div>
            </div>"#,
                color,
                ws,
                format_number(*cnt),
                pct
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    // 雇用形態×給与区分 スタック棒グラフ
    let emp_types: Vec<&str> = stats
        .distribution
        .iter()
        .take(5)
        .map(|(s, _)| s.as_str())
        .collect();

    let salary_types_set: Vec<String> = {
        let mut seen = Vec::new();
        for (_, st, _) in &stats.salary_cross {
            if !seen.contains(st) {
                seen.push(st.clone());
            }
        }
        seen
    };

    let salary_type_colors = |st: &str| -> &str {
        match st {
            "月給" => "#009E73",
            "時給" => "#D55E00",
            "日給" => "#56B4E9",
            "年俸" | "年収" => "#E69F00",
            _ => "#666666",
        }
    };

    let mut sal_pivot: HashMap<(&str, &str), i64> = HashMap::new();
    for (et, st, cnt) in &stats.salary_cross {
        sal_pivot.insert((et.as_str(), st.as_str()), *cnt);
    }

    let mut et_totals: HashMap<&str, i64> = HashMap::new();
    for (et, _, cnt) in &stats.salary_cross {
        *et_totals.entry(et.as_str()).or_insert(0) += cnt;
    }

    let age_series: Vec<String> = salary_types_set
        .iter()
        .map(|st| {
            let data: Vec<String> = emp_types
                .iter()
                .map(|et| {
                    let val = sal_pivot.get(&(*et, st.as_str())).copied().unwrap_or(0);
                    let t = et_totals.get(et).copied().unwrap_or(1).max(1);
                    let pct = (val as f64 / t as f64) * 100.0;
                    format!("{:.1}", pct)
                })
                .collect();
            let color = salary_type_colors(st);
            format!(
                r##"{{"name": "{}", "type": "bar", "stack": "total", "data": [{}], "itemStyle": {{"color": "{}"}}, "label": {{"show": true, "formatter": "{{c}}%", "color": "#fff", "fontSize": 10}}}}"##,
                st,
                data.join(","),
                color
            )
        })
        .collect();

    let emp_labels: Vec<String> = emp_types.iter().map(|s| format!("\"{}\"", s)).collect();

    // 社会保険加入率 横棒グラフ
    let insurance_labels: Vec<String> = stats
        .insurance_rates
        .iter()
        .rev()
        .map(|(l, _, _)| format!("\"{}\"", l))
        .collect();
    let insurance_values: Vec<String> = stats
        .insurance_rates
        .iter()
        .rev()
        .map(|(_, r, _)| format!("{:.1}", r))
        .collect();

    // 週休二日制ドーナツ
    let weekly_pie: Vec<String> = stats
        .weekly_holiday_dist
        .iter()
        .map(|(l, v)| format!(r#"{{"value": {}, "name": "{}"}}"#, v, l))
        .collect();

    // 年間休日ヒストグラム
    let holiday_labels: Vec<String> = stats
        .annual_holiday_hist
        .iter()
        .map(|(l, _)| format!("\"{}\"", l))
        .collect();
    let holiday_values: Vec<String> = stats
        .annual_holiday_hist
        .iter()
        .map(|(_, v)| v.to_string())
        .collect();

    // 残業時間ヒストグラム
    let overtime_labels: Vec<String> = stats
        .overtime_hist
        .iter()
        .map(|(l, _)| format!("\"{}\"", l))
        .collect();
    let overtime_values: Vec<String> = stats
        .overtime_hist
        .iter()
        .map(|(_, v)| v.to_string())
        .collect();

    format!(
        r##"<div class="space-y-6">
    <h2 class="text-xl font-bold text-white">💰 求人条件 <span class="text-blue-400 text-base font-normal">{industry_label} / {location_label}</span></h2>
    <p class="text-xs text-slate-500 -mt-4">給与・福利厚生・休日・勤務条件の分析</p>

    <!-- 雇用形態分布ドーナツ + KPIカード -->
    <div class="flex flex-col md:flex-row gap-4">
        <div class="stat-card flex-1">
            <h3 class="text-sm text-slate-400 mb-3">雇用形態分布</h3>
            <div class="echart" style="height:300px;" data-chart-config='{{
                "tooltip": {{"trigger": "item", "formatter": "{{b}}: {{c}}件 ({{d}}%)"}},
                "legend": {{"orient": "horizontal", "bottom": "0%"}},
                "series": [{{
                    "type": "pie",
                    "radius": ["40%", "70%"],
                    "center": ["50%", "48%"],
                    "avoidLabelOverlap": true,
                    "itemStyle": {{"borderRadius": 6, "borderColor": "#0f172a", "borderWidth": 2}},
                    "label": {{"show": true, "color": "#e2e8f0", "fontSize": 12, "formatter": "{{b}}\n{{d}}%"}},
                    "data": [{ws_pie}]
                }}]
            }}'></div>
        </div>
        <div class="flex flex-col gap-2" style="flex: 0 0 220px;">
            {kpi_cards}
        </div>
    </div>

    <!-- 賞与・昇給・退職金 KPI -->
    <div class="grid-stats">
        <div class="stat-card">
            <div class="stat-value text-emerald-400">{bonus_rate:.1}%</div>
            <div class="stat-label">賞与あり率（{bonus_count}件）</div>
        </div>
        <div class="stat-card">
            <div class="stat-value text-blue-400">{raise_rate:.1}%</div>
            <div class="stat-label">昇給あり率（{raise_count}件）</div>
        </div>
        <div class="stat-card">
            <div class="stat-value text-amber-400">{retirement_rate:.1}%</div>
            <div class="stat-label">退職金あり率（{retirement_count}件）</div>
        </div>
    </div>

    <!-- 雇用形態 × 給与帯 -->
    <div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">雇用形態 × 給与区分</h3>
        <div class="echart" style="height:350px;" data-chart-config='{{
            "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}}},
            "legend": {{"top": "0%", "itemGap": 15}},
            "grid": {{"left": "3%", "right": "4%", "bottom": "3%", "top": "15%", "containLabel": true}},
            "xAxis": {{"type": "category", "data": [{emp_labels}]}},
            "yAxis": {{"type": "value", "max": 100, "axisLabel": {{"formatter": "{{value}}%"}}}},
            "series": [{age_series}]
        }}'></div>
    </div>

    <!-- 社会保険加入率 -->
    <div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-3">社会保険加入率</h3>
        <div class="echart" style="height:250px;" data-chart-config='{{
            "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}, "formatter": "{{b}}: {{c}}%"}},
            "grid": {{"left": "20%", "right": "10%", "top": "5%", "bottom": "5%"}},
            "xAxis": {{"type": "value", "max": 100, "axisLabel": {{"formatter": "{{value}}%"}}}},
            "yAxis": {{"type": "category", "data": [{insurance_labels}]}},
            "series": [{{
                "type": "bar",
                "data": [{insurance_values}],
                "itemStyle": {{"color": "#10B981", "borderRadius": [0, 8, 8, 0]}},
                "label": {{"show": true, "position": "right", "formatter": "{{c}}%", "color": "#e2e8f0"}}
            }}]
        }}'></div>
    </div>

    <div class="grid-charts">
        <!-- 週休二日制ドーナツ -->
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">週休二日制の割合</h3>
            <div class="echart" style="height:300px;" data-chart-config='{{
                "tooltip": {{"trigger": "item", "formatter": "{{b}}: {{c}}件 ({{d}}%)"}},
                "series": [{{
                    "type": "pie",
                    "radius": ["35%", "65%"],
                    "data": [{weekly_pie}]
                }}]
            }}'></div>
        </div>
        <!-- 年間休日分布 -->
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">年間休日分布</h3>
            <div class="echart" style="height:300px;" data-chart-config='{{
                "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}}},
                "xAxis": {{"type": "category", "data": [{holiday_labels}], "axisLabel": {{"rotate": 20}}}},
                "yAxis": {{"type": "value"}},
                "series": [{{
                    "type": "bar",
                    "data": [{holiday_values}],
                    "itemStyle": {{"color": "#6366F1", "borderRadius": [4, 4, 0, 0]}}
                }}]
            }}'></div>
        </div>
    </div>

    <div class="grid-charts">
        <!-- 月平均残業時間 -->
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">月平均残業時間分布</h3>
            <div class="echart" style="height:300px;" data-chart-config='{{
                "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}}},
                "xAxis": {{"type": "category", "data": [{overtime_labels}]}},
                "yAxis": {{"type": "value"}},
                "series": [{{
                    "type": "bar",
                    "data": [{overtime_values}],
                    "itemStyle": {{"color": "#F59E0B", "borderRadius": [4, 4, 0, 0]}}
                }}]
            }}'></div>
        </div>
        <!-- テレワーク・福利厚生KPI -->
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">その他の福利厚生</h3>
            <div class="space-y-4 mt-4">
                <div class="flex items-center justify-between p-3 rounded-lg" style="background: rgba(99,102,241,0.1);">
                    <span class="text-white font-medium">テレワーク対応</span>
                    <span class="text-2xl font-bold" style="color: #6366F1;">{telework_rate:.1}%</span>
                </div>
                <div class="flex items-center justify-between p-3 rounded-lg" style="background: rgba(236,72,153,0.1);">
                    <span class="text-white font-medium">託児施設あり</span>
                    <span class="text-2xl font-bold" style="color: #EC4899;">{childcare_rate:.1}%</span>
                </div>
                <div class="flex items-center justify-between p-3 rounded-lg" style="background: rgba(16,185,129,0.1);">
                    <span class="text-white font-medium">入居住宅あり</span>
                    <span class="text-2xl font-bold" style="color: #10B981;">{housing_rate:.1}%</span>
                </div>
            </div>
        </div>
    </div>
</div>"##,
        industry_label = industry_label,
        location_label = location_label,
        ws_pie = ws_pie.join(","),
        kpi_cards = kpi_cards,
        bonus_rate = stats.bonus_rate,
        bonus_count = format_number(stats.bonus_count),
        raise_rate = stats.raise_rate,
        raise_count = format_number(stats.raise_count),
        retirement_rate = stats.retirement_rate,
        retirement_count = format_number(stats.retirement_count),
        emp_labels = emp_labels.join(","),
        age_series = age_series.join(","),
        insurance_labels = insurance_labels.join(","),
        insurance_values = insurance_values.join(","),
        weekly_pie = weekly_pie.join(","),
        holiday_labels = holiday_labels.join(","),
        holiday_values = holiday_values.join(","),
        overtime_labels = overtime_labels.join(","),
        overtime_values = overtime_values.join(","),
        telework_rate = stats.telework_rate,
        childcare_rate = stats.childcare_rate,
        housing_rate = stats.housing_rate,
    )
}
