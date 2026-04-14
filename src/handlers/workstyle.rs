use axum::extract::State;
use axum::response::Html;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tower_sessions::Session;

use crate::db::local_sqlite::LocalDb;
use crate::AppState;

use super::overview::{
    build_filter_clause, cross_nav, format_number, get_i64, get_session_filters, get_str,
    make_location_label, render_no_db_data, SessionFilters,
};

/// タブ3: 求人条件分析
pub async fn tab_workstyle(State(state): State<Arc<AppState>>, session: Session) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let db = match &state.hw_db {
        Some(db) => db,
        None => return Html(render_no_db_data("求人条件")),
    };

    let cache_key = format!(
        "workstyle_{}_{}_{}",
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
    let stats = tokio::task::spawn_blocking(move || fetch_workstyle(&db, &filters_clone))
        .await
        .unwrap_or_default();

    let mut html = render_workstyle(&filters, &stats);
    html.push_str(r#"<div class="text-[10px] text-slate-600 mt-4 border-t border-slate-800 pt-2">出典: ハローワーク掲載求人データ / 外部統計: e-Stat API / SSDSE-A（総務省統計局）</div>"#);
    html.push_str(r#"<div hx-get="/api/insight/widget/workstyle" hx-trigger="load" hx-swap="innerHTML"></div>"#);
    state.cache.set(cache_key, Value::String(html.clone()));
    Html(html)
}

/// 市場概況タブ用: 条件分析セクションHTML生成（fetch + render）
pub(crate) fn build_workstyle_html(db: &LocalDb, filters: &SessionFilters) -> String {
    let stats = fetch_workstyle(db, filters);
    render_workstyle(filters, &stats)
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
    /// 休日テキスト分布 (text, count)
    holiday_text_dist: Vec<(String, i64)>,
    /// 福利厚生一覧 (label, count, rate%)
    benefits_list: Vec<(String, i64, f64)>,
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
            holiday_text_dist: Vec::new(),
            benefits_list: Vec::new(),
        }
    }
}

fn fetch_workstyle(db: &LocalDb, filters: &SessionFilters) -> WorkstyleStats {
    let mut stats = WorkstyleStats::default();
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

    // 3-5. 賞与・昇給・退職金あり率（3クエリを1クエリに統合）
    {
        let sql = format!(
            "SELECT \
               SUM(CASE WHEN bonus_code IS NOT NULL AND bonus_code != '' AND bonus_code != '0' THEN 1 \
                        WHEN \"has_賞与\" = 1 THEN 1 ELSE 0 END) as bonus_cnt, \
               SUM(CASE WHEN raise_code IS NOT NULL AND raise_code != '' AND raise_code != '0' THEN 1 \
                        WHEN \"has_昇給\" = 1 THEN 1 ELSE 0 END) as raise_cnt, \
               SUM(CASE WHEN retirement_age_code IS NOT NULL AND retirement_age_code != '' AND retirement_age_code != '0' THEN 1 \
                        WHEN \"has_退職金\" = 1 THEN 1 ELSE 0 END) as retirement_cnt \
             FROM postings WHERE 1=1{filter_clause}"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            if let Some(row) = rows.first() {
                stats.bonus_count = get_i64(row, "bonus_cnt");
                stats.bonus_rate = stats.bonus_count as f64 / total * 100.0;
                stats.raise_count = get_i64(row, "raise_cnt");
                stats.raise_rate = stats.raise_count as f64 / total * 100.0;
                stats.retirement_count = get_i64(row, "retirement_cnt");
                stats.retirement_rate = stats.retirement_count as f64 / total * 100.0;
            }
        }
    }

    // 6. 社会保険加入率（4クエリを1クエリに統合）
    {
        let sql = format!(
            "SELECT \
               SUM(CASE WHEN insurance_employment = 1 OR insurance_employment = '1' THEN 1 ELSE 0 END) as emp_ins, \
               SUM(CASE WHEN insurance_workers_comp = 1 OR insurance_workers_comp = '1' THEN 1 ELSE 0 END) as workers_ins, \
               SUM(CASE WHEN insurance_health = 1 OR insurance_health = '1' THEN 1 ELSE 0 END) as health_ins, \
               SUM(CASE WHEN insurance_pension = 1 OR insurance_pension = '1' THEN 1 ELSE 0 END) as pension_ins \
             FROM postings WHERE 1=1{filter_clause}"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            if let Some(row) = rows.first() {
                let insurance_results = [
                    ("雇用保険", get_i64(row, "emp_ins")),
                    ("労災保険", get_i64(row, "workers_ins")),
                    ("健康保険", get_i64(row, "health_ins")),
                    ("厚生年金", get_i64(row, "pension_ins")),
                ];
                for (label, cnt) in &insurance_results {
                    let rate = *cnt as f64 / total * 100.0;
                    stats.insurance_rates.push((label.to_string(), rate, *cnt));
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

    // 7b. 休日テキスト詳細分布（上位10パターン）
    {
        let sql = format!(
            "SELECT holiday_text, COUNT(*) as cnt \
             FROM postings WHERE 1=1{filter_clause} AND holiday_text IS NOT NULL AND holiday_text != '' \
             GROUP BY holiday_text ORDER BY cnt DESC LIMIT 10"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            for row in &rows {
                let text = get_str(row, "holiday_text");
                let cnt = get_i64(row, "cnt");
                if !text.is_empty() {
                    stats.holiday_text_dist.push((text, cnt));
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

    // 10-12. テレワーク・託児施設・入居住宅あり率（3クエリを1クエリに統合）
    {
        let sql = format!(
            "SELECT \
               SUM(CASE WHEN telework_code IS NOT NULL AND telework_code != '' AND telework_code != '0' THEN 1 \
                        WHEN \"has_在宅勤務\" = 1 THEN 1 ELSE 0 END) as telework_cnt, \
               SUM(CASE WHEN childcare_facility = 1 OR childcare_facility = '1' THEN 1 ELSE 0 END) as childcare_cnt, \
               SUM(CASE WHEN housing_available = 1 OR housing_available = '1' THEN 1 ELSE 0 END) as housing_cnt \
             FROM postings WHERE 1=1{filter_clause}"
        );
        if let Ok(rows) = db.query(&sql, &mk_bind()) {
            if let Some(row) = rows.first() {
                stats.telework_count = get_i64(row, "telework_cnt");
                stats.telework_rate = stats.telework_count as f64 / total * 100.0;
                stats.childcare_count = get_i64(row, "childcare_cnt");
                stats.childcare_rate = stats.childcare_count as f64 / total * 100.0;
                stats.housing_count = get_i64(row, "housing_cnt");
                stats.housing_rate = stats.housing_count as f64 / total * 100.0;
            }
        }
    }

    // 13. 福利厚生一覧（has_*カラム全16項目）
    {
        let benefits_cols = [
            ("has_社会保険", "社会保険"),
            ("has_雇用保険", "雇用保険"),
            ("has_健康保険", "健康保険"),
            ("has_厚生年金", "厚生年金"),
            ("has_退職金", "退職金"),
            ("has_賞与", "賞与"),
            ("has_昇給", "昇給"),
            ("has_育児休業", "育児休業"),
            ("has_介護休暇", "介護休暇"),
            ("has_在宅勤務", "在宅勤務"),
            ("has_マイカー通勤", "マイカー通勤可"),
            ("has_週休二日", "週休二日制"),
            ("has_年休120以上", "年間休日120日以上"),
            ("has_外国人雇用", "外国人雇用実績"),
            ("has_定年制", "定年制"),
            ("has_再雇用制度", "再雇用制度"),
        ];
        for (col, label) in &benefits_cols {
            let sql = format!(
                "SELECT SUM(CASE WHEN \"{col}\" = 1 THEN 1 ELSE 0 END) as cnt FROM postings WHERE 1=1{filter_clause}"
            );
            if let Ok(rows) = db.query(&sql, &mk_bind()) {
                if let Some(row) = rows.first() {
                    let cnt = get_i64(row, "cnt");
                    let rate = if total > 0.0 {
                        cnt as f64 / total * 100.0
                    } else {
                        0.0
                    };
                    stats.benefits_list.push((label.to_string(), cnt, rate));
                }
            }
        }
        stats
            .benefits_list
            .sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    }

    stats
}

fn render_workstyle(filters: &SessionFilters, stats: &WorkstyleStats) -> String {
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

    let ws_pie_data: Vec<serde_json::Value> = stats
        .distribution
        .iter()
        .map(|(w, v)| json!({"value": v, "name": w, "itemStyle": {"color": ws_colors(w)}}))
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

    let age_series_data: Vec<serde_json::Value> = salary_types_set
        .iter()
        .map(|st| {
            let data: Vec<f64> = emp_types
                .iter()
                .map(|et| {
                    let val = sal_pivot.get(&(*et, st.as_str())).copied().unwrap_or(0);
                    let t = et_totals.get(et).copied().unwrap_or(1).max(1);
                    let pct = (val as f64 / t as f64) * 100.0;
                    (pct * 10.0).round() / 10.0
                })
                .collect();
            let color = salary_type_colors(st);
            json!({
                "name": st,
                "type": "bar",
                "stack": "total",
                "data": data,
                "itemStyle": {"color": color},
                "label": {"show": true, "formatter": "{c}%", "color": "#fff", "fontSize": 10}
            })
        })
        .collect();

    let emp_labels_data: Vec<&str> = emp_types.clone();

    // 社会保険加入率 横棒グラフ
    let insurance_labels_data: Vec<&str> = stats
        .insurance_rates
        .iter()
        .rev()
        .map(|(l, _, _)| l.as_str())
        .collect();
    let insurance_values_data: Vec<f64> = stats
        .insurance_rates
        .iter()
        .rev()
        .map(|(_, r, _)| (*r * 10.0).round() / 10.0)
        .collect();

    // 週休二日制ドーナツ
    let weekly_pie_data: Vec<serde_json::Value> = stats
        .weekly_holiday_dist
        .iter()
        .map(|(l, v)| json!({"value": v, "name": l}))
        .collect();

    // 年間休日ヒストグラム
    let holiday_labels_data: Vec<&str> = stats
        .annual_holiday_hist
        .iter()
        .map(|(l, _)| l.as_str())
        .collect();
    let holiday_values_data: Vec<i64> = stats.annual_holiday_hist.iter().map(|(_, v)| *v).collect();

    // 残業時間ヒストグラム
    let overtime_labels_data: Vec<&str> = stats
        .overtime_hist
        .iter()
        .map(|(l, _)| l.as_str())
        .collect();
    let overtime_values_data: Vec<i64> = stats.overtime_hist.iter().map(|(_, v)| *v).collect();

    // アクセシビリティ: 各チャートのaria-label生成
    let ws_pie_aria = {
        let total: i64 = stats.distribution.iter().map(|(_, v)| *v).sum();
        let top3: Vec<String> = stats
            .distribution
            .iter()
            .take(3)
            .map(|(name, val)| {
                if total > 0 {
                    format!("{} {:.1}%", name, *val as f64 / total as f64 * 100.0)
                } else {
                    format!("{} {}件", name, val)
                }
            })
            .collect();
        format!("雇用形態分布: {}", top3.join("、"))
    };

    let salary_cross_aria = {
        let top_types: Vec<&str> = emp_types.iter().take(3).copied().collect();
        format!("雇用形態別給与区分: {}", top_types.join("、"))
    };

    let insurance_aria = {
        let top3: Vec<String> = stats
            .insurance_rates
            .iter()
            .take(3)
            .map(|(l, r, _)| format!("{} {:.1}%", l, r))
            .collect();
        format!("社会保険加入率: {}", top3.join("、"))
    };

    let weekly_pie_aria = {
        let total: i64 = stats.weekly_holiday_dist.iter().map(|(_, v)| *v).sum();
        let top3: Vec<String> = stats
            .weekly_holiday_dist
            .iter()
            .take(3)
            .map(|(name, val)| {
                if total > 0 {
                    format!("{} {:.1}%", name, *val as f64 / total as f64 * 100.0)
                } else {
                    format!("{} {}件", name, val)
                }
            })
            .collect();
        format!("週休二日制の割合: {}", top3.join("、"))
    };

    let holiday_aria = {
        let top3: Vec<String> = stats
            .annual_holiday_hist
            .iter()
            .take(3)
            .map(|(l, v)| format!("{} {}件", l, v))
            .collect();
        if top3.is_empty() {
            "年間休日分布".to_string()
        } else {
            format!("年間休日分布: {}", top3.join("、"))
        }
    };

    let overtime_aria = {
        let top3: Vec<String> = stats
            .overtime_hist
            .iter()
            .take(3)
            .map(|(l, v)| format!("{} {}件", l, v))
            .collect();
        if top3.is_empty() {
            "月平均残業時間分布".to_string()
        } else {
            format!("月平均残業時間分布: {}", top3.join("、"))
        }
    };

    // --- チャート設定をserde_jsonで安全に生成 ---
    let ws_pie_config = json!({
        "tooltip": {"trigger": "item", "formatter": "{b}: {c}件 ({d}%)"},
        "legend": {"orient": "horizontal", "bottom": 0, "textStyle": {"color": "#94a3b8", "fontSize": 11}},
        "series": [{
            "type": "pie",
            "radius": ["40%", "70%"],
            "center": ["50%", "48%"],
            "avoidLabelOverlap": true,
            "itemStyle": {"borderRadius": 6, "borderColor": "#0f172a", "borderWidth": 2},
            "label": {"show": true, "color": "#e2e8f0", "fontSize": 12, "formatter": "{b}\n{d}%"},
            "data": ws_pie_data
        }]
    }).to_string().replace('\'', "&#39;");

    let salary_cross_config = json!({
        "tooltip": {"trigger": "axis", "axisPointer": {"type": "shadow"}},
        "legend": {"top": 0, "textStyle": {"color": "#94a3b8", "fontSize": 11}},
        "grid": {"left": "3%", "right": "4%", "bottom": "3%", "top": "15%", "containLabel": true},
        "xAxis": {"type": "category", "data": emp_labels_data},
        "yAxis": {"type": "value", "max": 100, "axisLabel": {"formatter": "{value}%"}},
        "series": age_series_data
    })
    .to_string()
    .replace('\'', "&#39;");

    let insurance_config = json!({
        "tooltip": {"trigger": "axis", "axisPointer": {"type": "shadow"}, "formatter": "{b}: {c}%"},
        "grid": {"left": "20%", "right": "10%", "top": "5%", "bottom": "5%"},
        "xAxis": {"type": "value", "max": 100, "axisLabel": {"formatter": "{value}%"}},
        "yAxis": {"type": "category", "data": insurance_labels_data},
        "series": [{
            "type": "bar",
            "data": insurance_values_data,
            "itemStyle": {"color": "#10B981", "borderRadius": [0, 8, 8, 0]},
            "label": {"show": true, "position": "right", "formatter": "{c}%", "color": "#e2e8f0"}
        }]
    })
    .to_string()
    .replace('\'', "&#39;");

    let weekly_pie_config = json!({
        "tooltip": {"trigger": "item", "formatter": "{b}: {c}件 ({d}%)"},
        "series": [{
            "type": "pie",
            "radius": ["35%", "65%"],
            "data": weekly_pie_data
        }]
    })
    .to_string()
    .replace('\'', "&#39;");

    let holiday_config = json!({
        "tooltip": {"trigger": "axis", "axisPointer": {"type": "shadow"}},
        "xAxis": {"type": "category", "data": holiday_labels_data, "axisLabel": {"rotate": 20}},
        "yAxis": {"type": "value"},
        "series": [{
            "type": "bar",
            "data": holiday_values_data,
            "itemStyle": {"color": "#6366F1", "borderRadius": [4, 4, 0, 0]}
        }]
    })
    .to_string()
    .replace('\'', "&#39;");

    let overtime_config = json!({
        "tooltip": {"trigger": "axis", "axisPointer": {"type": "shadow"}},
        "grid": {"left": "3%", "right": "4%", "bottom": "3%", "top": "15%", "containLabel": true},
        "xAxis": {"type": "category", "data": overtime_labels_data},
        "yAxis": {"type": "value"},
        "series": [{
            "type": "bar",
            "data": overtime_values_data,
            "itemStyle": {"color": "#F59E0B", "borderRadius": [4, 4, 0, 0]}
        }]
    })
    .to_string()
    .replace('\'', "&#39;");

    format!(
        r##"<div class="space-y-6">
    <h2 class="text-xl font-bold text-white">💰 求人条件 <span class="text-blue-400 text-base font-normal">{industry_label} / {location_label}</span></h2>
    <p class="text-xs text-slate-500 -mt-4">給与・福利厚生・休日・勤務条件の分析</p>

    <!-- 雇用形態分布ドーナツ + KPIカード -->
    <div class="flex flex-col md:flex-row gap-4">
        <div class="stat-card flex-1">
            <h3 class="text-sm text-slate-400 mb-3">雇用形態分布</h3>
            <div class="echart" role="img" aria-label="{ws_pie_aria}" style="height:300px;" data-chart-config='{ws_pie_config}'></div>
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
        <h3 class="text-sm text-slate-400 mb-1">雇用形態 × 給与区分 {nav_salary}</h3>
        <div class="echart" role="img" aria-label="{salary_cross_aria}" style="height:350px;" data-chart-config='{salary_cross_config}'></div>
    </div>

    <!-- 社会保険加入率 -->
    <div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-3">社会保険加入率</h3>
        <div class="echart" role="img" aria-label="{insurance_aria}" style="height:250px;" data-chart-config='{insurance_config}'></div>
    </div>

    <div class="grid-charts">
        <!-- 週休二日制ドーナツ -->
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">週休二日制の割合</h3>
            <div class="echart" role="img" aria-label="{weekly_pie_aria}" style="height:300px;" data-chart-config='{weekly_pie_config}'></div>
        </div>
        <!-- 休日パターン詳細 -->
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">休日パターン Top10</h3>
            <div class="space-y-1">
                {holiday_text_bars}
            </div>
        </div>
    </div>
    <div class="grid-charts">
        <!-- 年間休日分布 -->
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">年間休日分布</h3>
            <div class="echart" role="img" aria-label="{holiday_aria}" style="height:300px;" data-chart-config='{holiday_config}'></div>
        </div>
    </div>

    <div class="grid-charts">
        <!-- 月平均残業時間 -->
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">月平均残業時間分布</h3>
            {overtime_chart}
        </div>
        <!-- 福利厚生一覧 -->
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">福利厚生一覧</h3>
            <div class="space-y-1 mt-2">
                {benefits_bars}
            </div>
        </div>
    </div>
</div>"##,
        industry_label = industry_label,
        location_label = location_label,
        nav_salary = cross_nav("/tab/analysis", "給与構造・競争力分析"),
        ws_pie_aria = ws_pie_aria,
        ws_pie_config = ws_pie_config,
        salary_cross_aria = salary_cross_aria,
        salary_cross_config = salary_cross_config,
        insurance_aria = insurance_aria,
        insurance_config = insurance_config,
        weekly_pie_aria = weekly_pie_aria,
        weekly_pie_config = weekly_pie_config,
        holiday_aria = holiday_aria,
        holiday_config = holiday_config,
        kpi_cards = kpi_cards,
        bonus_rate = stats.bonus_rate,
        bonus_count = format_number(stats.bonus_count),
        raise_rate = stats.raise_rate,
        raise_count = format_number(stats.raise_count),
        retirement_rate = stats.retirement_rate,
        retirement_count = format_number(stats.retirement_count),
        holiday_text_bars = {
            let max_cnt = stats
                .holiday_text_dist
                .first()
                .map(|(_, c)| *c)
                .unwrap_or(1)
                .max(1);
            let mut bars = String::new();
            for (text, cnt) in &stats.holiday_text_dist {
                let w = (*cnt as f64 / max_cnt as f64 * 100.0).min(100.0);
                bars.push_str(&format!(
                    "<div class=\"flex items-center gap-2\">\
                     <div class=\"w-28 text-xs text-slate-400 text-right flex-shrink-0 truncate\">{}</div>\
                     <div class=\"flex-1 rounded-full h-4 relative overflow-hidden\" style=\"background:rgba(15,23,42,0.6)\">\
                       <div class=\"h-full rounded-full\" style=\"width:{w:.1}%;background:#8b5cf6\"></div>\
                     </div>\
                     <div class=\"w-16 text-xs text-slate-300 text-right\">{}</div>\
                    </div>",
                    super::helpers::escape_html(text),
                    super::helpers::format_number(*cnt),
                ));
            }
            if bars.is_empty() {
                "<p class=\"text-slate-500 text-sm\">データなし</p>".to_string()
            } else {
                bars
            }
        },
        overtime_chart = if overtime_labels_data.is_empty() {
            r#"<p class="text-slate-500 text-sm text-center py-8">残業時間データがありません</p>"#
                .to_string()
        } else {
            format!(
                "<div class=\"echart\" role=\"img\" aria-label=\"{}\" style=\"height:300px;\" data-chart-config='{}'></div>",
                overtime_aria, overtime_config
            )
        },
        benefits_bars = {
            let mut bars = String::new();
            for (label, _cnt, rate) in &stats.benefits_list {
                let w = rate.min(100.0);
                let color = if *rate > 70.0 {
                    "#22c55e"
                } else if *rate > 40.0 {
                    "#3b82f6"
                } else {
                    "#64748b"
                };
                bars.push_str(&format!(
                    "<div class=\"flex items-center gap-2\">\
                     <div class=\"w-28 text-xs text-slate-400 text-right flex-shrink-0\">{label}</div>\
                     <div class=\"flex-1 rounded-full h-4 relative overflow-hidden\" style=\"background:rgba(15,23,42,0.6)\">\
                       <div class=\"h-full rounded-full\" style=\"width:{w:.1}%;background:{color}\"></div>\
                     </div>\
                     <div class=\"w-12 text-xs text-slate-300 text-right\">{rate:.1}%</div>\
                    </div>"
                ));
            }
            bars
        },
    )
}
