use axum::extract::State;
use axum::response::Html;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tower_sessions::Session;

use crate::AppState;

use super::overview::{
    build_filter_clause, get_i64, get_session_filters, get_str, make_location_label,
    render_no_db_data, SessionFilters,
};

/// タブ4: 採用動向 - HTMXパーシャルHTML
pub async fn tab_demographics(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let db = match &state.hw_db {
        Some(db) => db,
        None => return Html(render_no_db_data("採用動向")),
    };

    let cache_key = format!(
        "demographics_{}_{}_{}",
        filters.industry_cache_key(),
        filters.prefecture,
        filters.municipality
    );
    if let Some(cached) = state.cache.get(&cache_key) {
        if let Some(html) = cached.as_str() {
            return Html(html.to_string());
        }
    }

    let db = db.clone();
    let filters_clone = filters.clone();
    let stats = tokio::task::spawn_blocking(move || fetch_demographics(&db, &filters_clone))
        .await
        .unwrap_or_default();

    let html = render_demographics(&filters, &stats);
    state.cache.set(cache_key, Value::String(html.clone()));
    Html(html)
}

/// 市場概況タブ用: 採用動向セクションHTML生成（fetch + render）
pub(crate) fn build_demographics_html(
    db: &crate::db::local_sqlite::LocalDb,
    filters: &SessionFilters,
) -> String {
    let stats = fetch_demographics(db, filters);
    render_demographics(filters, &stats)
}

#[derive(Default)]
struct DemoStats {
    total_postings: i64,
    /// 求人理由内訳 (recruitment_reason, count)
    recruitment_reasons: Vec<(String, i64)>,
    /// 求人理由×産業クロス top5 reason × top5 industry
    reason_industry_cross: Vec<(String, String, i64)>,
    top_reasons: Vec<String>,
    top_industries_for_cross: Vec<String>,
    /// 募集人数分布 (range, count)
    recruitment_count_dist: Vec<(String, i64)>,
    /// 年齢制限分布 (label, count)
    age_restriction_dist: Vec<(String, i64)>,
    /// 学歴要件分布 (education_required, count)
    education_dist: Vec<(String, i64)>,
    /// 必要資格TOP20 (license, count)
    license_top: Vec<(String, i64)>,
    /// 選考方法分布 (selection_method, count)
    selection_method_dist: Vec<(String, i64)>,
    /// 試用期間分布 (range, count)
    trial_period_dist: Vec<(String, i64)>,
}

fn fetch_demographics(
    db: &crate::db::local_sqlite::LocalDb,
    filters: &SessionFilters,
) -> DemoStats {
    let mut stats = DemoStats::default();
    let (filter_clause, filter_params) = build_filter_clause(filters, 0);

    let mk_bind = || -> Vec<&dyn rusqlite::types::ToSql> {
        filter_params
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect()
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

    // 1. 求人理由内訳
    {
        let sql = format!(
            "SELECT recruitment_reason, COUNT(*) as cnt FROM postings \
             WHERE 1=1{filter_clause} AND recruitment_reason IS NOT NULL AND recruitment_reason != '' \
             GROUP BY recruitment_reason ORDER BY cnt DESC"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            for row in &rows {
                let reason = get_str(row, "recruitment_reason");
                let cnt = get_i64(row, "cnt");
                if !reason.is_empty() {
                    stats.recruitment_reasons.push((reason, cnt));
                }
            }
        }
    }

    // 2. 求人理由×産業クロス
    {
        // 上位5理由
        stats.top_reasons = stats
            .recruitment_reasons
            .iter()
            .take(5)
            .map(|(r, _)| r.clone())
            .collect();

        // 上位5産業
        let sql_ind = format!(
            "SELECT job_type, COUNT(*) as cnt FROM postings \
             WHERE 1=1{filter_clause} AND job_type IS NOT NULL AND job_type != '' \
             GROUP BY job_type ORDER BY cnt DESC LIMIT 5"
        );
        if let Ok(rows) = db.query(&sql_ind, &mk_bind()) {
            for row in &rows {
                let jt = get_str(row, "job_type");
                if !jt.is_empty() {
                    stats.top_industries_for_cross.push(jt);
                }
            }
        }

        // クロス集計
        if !stats.top_reasons.is_empty() && !stats.top_industries_for_cross.is_empty() {
            let sql = format!(
                "SELECT recruitment_reason, job_type, COUNT(*) as cnt FROM postings \
                 WHERE 1=1{filter_clause} \
                 AND recruitment_reason IS NOT NULL AND recruitment_reason != '' \
                 AND job_type IS NOT NULL AND job_type != '' \
                 GROUP BY recruitment_reason, job_type"
            );
            if let Ok(rows) = db.query(&sql, &mk_bind()) {
                for row in &rows {
                    let reason = get_str(row, "recruitment_reason");
                    let jt = get_str(row, "job_type");
                    let cnt = get_i64(row, "cnt");
                    if stats.top_reasons.contains(&reason)
                        && stats.top_industries_for_cross.contains(&jt)
                    {
                        stats.reason_industry_cross.push((reason, jt, cnt));
                    }
                }
            }
        }
    }

    // 3. 募集人数分布
    {
        let sql = format!(
            "SELECT CASE \
               WHEN recruitment_count = 1 THEN '1人' \
               WHEN recruitment_count <= 3 THEN '2〜3人' \
               WHEN recruitment_count <= 5 THEN '4〜5人' \
               WHEN recruitment_count <= 10 THEN '6〜10人' \
               ELSE '11人〜' \
             END as rng, COUNT(*) as cnt \
             FROM postings WHERE 1=1{filter_clause} AND recruitment_count > 0 \
             GROUP BY rng ORDER BY MIN(recruitment_count)"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            for row in &rows {
                let rng = get_str(row, "rng");
                let cnt = get_i64(row, "cnt");
                if !rng.is_empty() {
                    stats.recruitment_count_dist.push((rng, cnt));
                }
            }
        }
    }

    // 4. 年齢制限分布
    {
        let sql = format!(
            "SELECT CASE \
               WHEN (age_min IS NULL OR age_min = 0) AND (age_max IS NULL OR age_max = 0) THEN '制限なし' \
               WHEN age_max > 0 AND (age_min IS NULL OR age_min = 0) THEN '上限あり' \
               WHEN age_min > 0 AND (age_max IS NULL OR age_max = 0) THEN '下限あり' \
               ELSE '上下限あり' \
             END as label, COUNT(*) as cnt \
             FROM postings WHERE 1=1{filter_clause} \
             GROUP BY label ORDER BY cnt DESC"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            for row in &rows {
                let label = get_str(row, "label");
                let cnt = get_i64(row, "cnt");
                if !label.is_empty() {
                    stats.age_restriction_dist.push((label, cnt));
                }
            }
        }
    }

    // 5. 学歴要件分布
    {
        let sql = format!(
            "SELECT education_required, COUNT(*) as cnt FROM postings \
             WHERE 1=1{filter_clause} AND education_required IS NOT NULL AND education_required != '' \
             GROUP BY education_required ORDER BY cnt DESC"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            for row in &rows {
                let ed = get_str(row, "education_required");
                let cnt = get_i64(row, "cnt");
                if !ed.is_empty() {
                    stats.education_dist.push((ed, cnt));
                }
            }
        }
    }

    // 6. 必要資格TOP20（license_1, license_2, license_3を1回のスキャンで集約）
    {
        let sql = format!(
            "SELECT license, COUNT(*) as total FROM ( \
               SELECT license_1 as license FROM postings \
                 WHERE 1=1{filter_clause} AND license_1 IS NOT NULL AND license_1 != '' \
               UNION ALL \
               SELECT license_2 FROM postings \
                 WHERE 1=1{filter_clause} AND license_2 IS NOT NULL AND license_2 != '' \
               UNION ALL \
               SELECT license_3 FROM postings \
                 WHERE 1=1{filter_clause} AND license_3 IS NOT NULL AND license_3 != '' \
             ) GROUP BY license ORDER BY total DESC LIMIT 20"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            for row in &rows {
                let lic = get_str(row, "license");
                let cnt = get_i64(row, "total");
                if !lic.is_empty() {
                    stats.license_top.push((lic, cnt));
                }
            }
        }
    }

    // 7. 選考方法分布
    {
        let sql = format!(
            "SELECT selection_method, COUNT(*) as cnt FROM postings \
             WHERE 1=1{filter_clause} AND selection_method IS NOT NULL AND selection_method != '' \
             GROUP BY selection_method ORDER BY cnt DESC LIMIT 10"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            for row in &rows {
                let method = get_str(row, "selection_method");
                let cnt = get_i64(row, "cnt");
                if !method.is_empty() {
                    stats.selection_method_dist.push((method, cnt));
                }
            }
        }
    }

    // 8. 試用期間分布
    {
        let sql = format!(
            "SELECT CASE \
               WHEN trial_period IS NULL OR trial_period = '' OR trial_period = '0' THEN 'なし' \
               WHEN trial_period_months <= 1 THEN '1ヶ月以下' \
               WHEN trial_period_months <= 3 THEN '2〜3ヶ月' \
               WHEN trial_period_months <= 6 THEN '4〜6ヶ月' \
               ELSE '6ヶ月超' \
             END as rng, COUNT(*) as cnt \
             FROM postings WHERE 1=1{filter_clause} \
             GROUP BY rng ORDER BY cnt DESC"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            for row in &rows {
                let rng = get_str(row, "rng");
                let cnt = get_i64(row, "cnt");
                if !rng.is_empty() {
                    stats.trial_period_dist.push((rng, cnt));
                }
            }
        }
    }

    stats
}

fn render_demographics(filters: &SessionFilters, stats: &DemoStats) -> String {
    let location_label = make_location_label(&filters.prefecture, &filters.municipality);
    let industry_label = filters.industry_label();

    // 求人理由ドーナツ
    let reason_pie: Vec<String> = stats
        .recruitment_reasons
        .iter()
        .take(10)
        .map(|(r, v)| format!(r#"{{"value": {}, "name": "{}"}}"#, v, r))
        .collect();

    // 求人理由×産業クロス（スタック棒グラフ）
    let cross_chart = build_reason_industry_cross(
        &stats.top_reasons,
        &stats.top_industries_for_cross,
        &stats.reason_industry_cross,
    );

    // 募集人数分布
    let recruit_labels: Vec<String> = stats
        .recruitment_count_dist
        .iter()
        .map(|(l, _)| format!("\"{}\"", l))
        .collect();
    let recruit_values: Vec<String> = stats
        .recruitment_count_dist
        .iter()
        .map(|(_, v)| v.to_string())
        .collect();

    // 年齢制限ドーナツ
    let age_pie: Vec<String> = stats
        .age_restriction_dist
        .iter()
        .map(|(l, v)| format!(r#"{{"value": {}, "name": "{}"}}"#, v, l))
        .collect();

    // 学歴要件棒グラフ
    let edu_labels: Vec<String> = stats
        .education_dist
        .iter()
        .rev()
        .map(|(l, _)| format!("\"{}\"", l))
        .collect();
    let edu_values: Vec<String> = stats
        .education_dist
        .iter()
        .rev()
        .map(|(_, v)| v.to_string())
        .collect();

    // 必要資格TOP20（横棒）
    let license_labels: Vec<String> = stats
        .license_top
        .iter()
        .rev()
        .map(|(l, _)| format!("\"{}\"", l))
        .collect();
    let license_values: Vec<String> = stats
        .license_top
        .iter()
        .rev()
        .map(|(_, v)| v.to_string())
        .collect();

    // 選考方法棒グラフ
    let selection_labels: Vec<String> = stats
        .selection_method_dist
        .iter()
        .rev()
        .map(|(l, _)| format!("\"{}\"", l))
        .collect();
    let selection_values: Vec<String> = stats
        .selection_method_dist
        .iter()
        .rev()
        .map(|(_, v)| v.to_string())
        .collect();

    // 試用期間分布
    let trial_labels: Vec<String> = stats
        .trial_period_dist
        .iter()
        .map(|(l, _)| format!("\"{}\"", l))
        .collect();
    let trial_values: Vec<String> = stats
        .trial_period_dist
        .iter()
        .map(|(_, v)| v.to_string())
        .collect();

    format!(
        r##"<div class="space-y-6">
    <h2 class="text-xl font-bold text-white">📋 採用動向 <span class="text-blue-400 text-base font-normal">{industry_label} / {location_label}</span></h2>
    <p class="text-xs text-slate-500 -mt-4">求人理由・募集人数・応募条件・選考方法の分析</p>

    <!-- 求人理由 + 年齢制限 -->
    <div class="grid-charts">
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">求人理由内訳</h3>
            <div class="echart" style="height:350px;" data-chart-config='{{
                "tooltip": {{"trigger": "item", "formatter": "{{b}}: {{c}}件 ({{d}}%)"}},
                "legend": {{"orient": "horizontal", "bottom": 0, "textStyle": {{"color": "#94a3b8", "fontSize": 11}}}},
                "series": [{{
                    "type": "pie",
                    "radius": ["35%", "65%"],
                    "center": ["60%", "50%"],
                    "data": [{reason_pie}]
                }}]
            }}'></div>
        </div>
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">年齢制限の分布</h3>
            <div class="echart" style="height:350px;" data-chart-config='{{
                "tooltip": {{"trigger": "item", "formatter": "{{b}}: {{c}}件 ({{d}}%)"}},
                "series": [{{
                    "type": "pie",
                    "radius": ["35%", "65%"],
                    "data": [{age_pie}]
                }}]
            }}'></div>
        </div>
    </div>

    <!-- 求人理由×産業クロス -->
    <div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-3">求人理由×産業クロス（上位5）</h3>
        {cross_chart}
    </div>

    <!-- 募集人数分布 + 試用期間分布 -->
    <div class="grid-charts">
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">募集人数の分布</h3>
            <div class="echart" style="height:300px;" data-chart-config='{{
                "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}}},
                "xAxis": {{"type": "category", "data": [{recruit_labels}]}},
                "yAxis": {{"type": "value"}},
                "series": [{{
                    "type": "bar",
                    "data": [{recruit_values}],
                    "itemStyle": {{"color": "#3B82F6", "borderRadius": [4, 4, 0, 0]}},
                    "label": {{"show": true, "position": "top", "color": "#e2e8f0"}}
                }}]
            }}'></div>
        </div>
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">試用期間の分布</h3>
            <div class="echart" style="height:300px;" data-chart-config='{{
                "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}}},
                "xAxis": {{"type": "category", "data": [{trial_labels}]}},
                "yAxis": {{"type": "value"}},
                "series": [{{
                    "type": "bar",
                    "data": [{trial_values}],
                    "itemStyle": {{"color": "#F59E0B", "borderRadius": [4, 4, 0, 0]}},
                    "label": {{"show": true, "position": "top", "color": "#e2e8f0"}}
                }}]
            }}'></div>
        </div>
    </div>

    <!-- 学歴要件 + 選考方法 -->
    <div class="grid-charts">
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">学歴要件分布</h3>
            <div class="echart" style="height:300px;" data-chart-config='{{
                "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}}},
                "grid": {{"left": "25%", "right": "10%", "top": "5%", "bottom": "5%"}},
                "xAxis": {{"type": "value"}},
                "yAxis": {{"type": "category", "data": [{edu_labels}]}},
                "series": [{{
                    "type": "bar",
                    "data": [{edu_values}],
                    "itemStyle": {{"color": "#10B981", "borderRadius": [0, 8, 8, 0]}},
                    "label": {{"show": true, "position": "right", "color": "#e2e8f0"}}
                }}]
            }}'></div>
        </div>
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">選考方法の分布</h3>
            <div class="echart" style="height:300px;" data-chart-config='{{
                "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}}},
                "grid": {{"left": "25%", "right": "10%", "top": "5%", "bottom": "5%"}},
                "xAxis": {{"type": "value"}},
                "yAxis": {{"type": "category", "data": [{selection_labels}]}},
                "series": [{{
                    "type": "bar",
                    "data": [{selection_values}],
                    "itemStyle": {{"color": "#8B5CF6", "borderRadius": [0, 8, 8, 0]}},
                    "label": {{"show": true, "position": "right", "color": "#e2e8f0"}}
                }}]
            }}'></div>
        </div>
    </div>

    <!-- 必要資格TOP20 -->
    <div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-3">必要資格TOP20</h3>
        <div class="echart" style="height:500px;" data-chart-config='{{
            "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}}},
            "grid": {{"left": "30%", "right": "10%", "top": "3%", "bottom": "5%"}},
            "xAxis": {{"type": "value"}},
            "yAxis": {{"type": "category", "data": [{license_labels}]}},
            "series": [{{
                "type": "bar",
                "data": [{license_values}],
                "itemStyle": {{"color": "#EC4899", "borderRadius": [0, 8, 8, 0]}},
                "label": {{"show": true, "position": "right", "color": "#e2e8f0"}}
            }}]
        }}'></div>
    </div>
</div>"##,
        industry_label = industry_label,
        location_label = location_label,
        reason_pie = reason_pie.join(","),
        age_pie = age_pie.join(","),
        cross_chart = cross_chart,
        recruit_labels = recruit_labels.join(","),
        recruit_values = recruit_values.join(","),
        trial_labels = trial_labels.join(","),
        trial_values = trial_values.join(","),
        edu_labels = edu_labels.join(","),
        edu_values = edu_values.join(","),
        selection_labels = selection_labels.join(","),
        selection_values = selection_values.join(","),
        license_labels = license_labels.join(","),
        license_values = license_values.join(","),
    )
}

/// 求人理由×産業のスタック棒グラフ
fn build_reason_industry_cross(
    top_reasons: &[String],
    top_industries: &[String],
    cross_data: &[(String, String, i64)],
) -> String {
    if top_reasons.is_empty() || top_industries.is_empty() || cross_data.is_empty() {
        return r##"<p class="text-slate-500 text-sm text-center py-12">データがありません</p>"##
            .to_string();
    }

    let mut pivot: HashMap<(&str, &str), i64> = HashMap::new();
    for (reason, jt, cnt) in cross_data {
        pivot.insert((reason.as_str(), jt.as_str()), *cnt);
    }

    let industry_colors = ["#3B82F6", "#10B981", "#F59E0B", "#EF4444", "#8B5CF6"];

    let reason_labels: Vec<String> = top_reasons
        .iter()
        .rev()
        .map(|s| format!("\"{}\"", s))
        .collect();

    let series: Vec<String> = top_industries
        .iter()
        .enumerate()
        .map(|(i, jt)| {
            let data: Vec<String> = top_reasons
                .iter()
                .rev()
                .map(|reason| {
                    let val = pivot
                        .get(&(reason.as_str(), jt.as_str()))
                        .copied()
                        .unwrap_or(0);
                    val.to_string()
                })
                .collect();
            let color = industry_colors.get(i).unwrap_or(&"#999");
            format!(
                r##"{{"name": "{jt}", "type": "bar", "stack": "total", "data": [{data}], "itemStyle": {{"color": "{color}"}}}}"##,
                jt = jt,
                data = data.join(","),
                color = color,
            )
        })
        .collect();

    format!(
        r##"<div class="echart" style="height:350px;" data-chart-config='{{
            "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}}},
            "legend": {{"data": [{legend}], "top": "0%"}},
            "grid": {{"left": "20%", "right": "5%", "top": "12%", "bottom": "5%"}},
            "xAxis": {{"type": "value"}},
            "yAxis": {{"type": "category", "data": [{labels}]}},
            "series": [{series}]
        }}'></div>"##,
        legend = top_industries
            .iter()
            .map(|s| format!("\"{}\"", s))
            .collect::<Vec<_>>()
            .join(","),
        labels = reason_labels.join(","),
        series = series.join(","),
    )
}
