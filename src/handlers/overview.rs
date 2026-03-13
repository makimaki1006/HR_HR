use axum::extract::State;
use axum::response::Html;
use serde_json::Value;
use std::sync::Arc;
use tower_sessions::Session;

use crate::auth::{
    SESSION_JOB_TYPE_KEY, SESSION_JOB_TYPES_KEY, SESSION_INDUSTRY_RAWS_KEY,
    SESSION_MUNICIPALITY_KEY, SESSION_PREFECTURE_KEY,
};
use crate::AppState;

/// セッションフィルタ（複数選択対応）
#[derive(Clone, Debug)]
pub struct SessionFilters {
    pub job_types: Vec<String>,
    pub industry_raws: Vec<String>,
    pub prefecture: String,
    pub municipality: String,
}

impl SessionFilters {
    /// 産業ラベル生成（具体名表示）
    /// 1-2件: 具体名をカンマ区切り（例: 「医療, 建設業」）
    /// 3件以上: 先頭名 + 「他N件」（例: 「医療 他2件」）
    /// 混合時（大分類+中分類）: 「医療 + 電気工事業 他1件」
    pub fn industry_label(&self) -> String {
        let jt_count = self.job_types.len();
        let ir_count = self.industry_raws.len();
        let total = jt_count + ir_count;

        if total == 0 {
            return "全産業".to_string();
        }

        // 全名前リストを構築（大分類優先）
        let mut all_names: Vec<&str> = Vec::with_capacity(total);
        for jt in &self.job_types { all_names.push(jt.as_str()); }
        for ir in &self.industry_raws { all_names.push(ir.as_str()); }

        if total <= 2 {
            all_names.join(", ")
        } else {
            format!("{} 他{}件", all_names[0], total - 1)
        }
    }

    /// ツールチップ用の全名前リスト
    pub fn industry_label_full(&self) -> String {
        let mut all_names: Vec<&str> = Vec::new();
        for jt in &self.job_types { all_names.push(jt.as_str()); }
        for ir in &self.industry_raws { all_names.push(ir.as_str()); }
        if all_names.is_empty() {
            "全産業".to_string()
        } else {
            all_names.join(", ")
        }
    }

    /// キャッシュキーの産業部分
    pub fn industry_cache_key(&self) -> String {
        let mut key = String::new();
        if !self.job_types.is_empty() {
            let mut sorted = self.job_types.clone();
            sorted.sort();
            key.push_str(&format!("jt:{}", sorted.join("+")));
        }
        if !self.industry_raws.is_empty() {
            if !key.is_empty() { key.push('|'); }
            let mut sorted = self.industry_raws.clone();
            sorted.sort();
            key.push_str(&format!("ir:{}", sorted.join("+")));
        }
        key
    }

    /// 「未分類」を除いた実質的な大分類リスト
    fn real_job_types(&self) -> Vec<String> {
        self.job_types.iter().filter(|jt| *jt != "未分類").cloned().collect()
    }

    /// 「未分類」が選択されているか
    fn has_unclassified(&self) -> bool {
        self.job_types.iter().any(|jt| jt == "未分類")
    }

    /// 産業フィルタSQL句（?N 位置パラメータ用）
    /// 両方指定時は OR 結合: (job_type IN (...) OR industry_raw IN (...))
    /// 「未分類」選択時は (industry_raw IS NULL OR industry_raw = '') を OR追加
    pub fn build_industry_clause(&self, mut idx: usize) -> (String, Vec<String>, usize) {
        let real_jt = self.real_job_types();
        let has_unclass = self.has_unclassified();
        let has_jt = !real_jt.is_empty();
        let has_ir = !self.industry_raws.is_empty();

        // フィルタなし
        if !has_jt && !has_ir && !has_unclass {
            return (String::new(), Vec::new(), idx);
        }

        let mut parts: Vec<String> = Vec::new();
        let mut params: Vec<String> = Vec::new();

        if has_jt {
            let ph: Vec<String> = real_jt.iter().map(|_| { idx += 1; format!("?{}", idx) }).collect();
            parts.push(format!("job_type IN ({})", ph.join(",")));
            params.extend(real_jt);
        }
        if has_ir {
            let ph: Vec<String> = self.industry_raws.iter().map(|_| { idx += 1; format!("?{}", idx) }).collect();
            parts.push(format!("industry_raw IN ({})", ph.join(",")));
            params.extend(self.industry_raws.iter().cloned());
        }
        if has_unclass {
            parts.push("(industry_raw IS NULL OR industry_raw = '')".to_string());
        }

        let clause = format!(" AND ({})", parts.join(" OR "));
        (clause, params, idx)
    }

    /// 産業フィルタSQL句（自動連番?パラメータ用 - jobmap等）
    /// 「未分類」選択時は (industry_raw IS NULL OR industry_raw = '') を OR追加
    pub fn append_industry_filter_str(&self, sql: &mut String, params: &mut Vec<String>) {
        let real_jt = self.real_job_types();
        let has_unclass = self.has_unclassified();
        let has_jt = !real_jt.is_empty();
        let has_ir = !self.industry_raws.is_empty();

        if !has_jt && !has_ir && !has_unclass {
            return;
        }

        let mut parts: Vec<String> = Vec::new();

        if has_jt {
            let ph = vec!["?"; real_jt.len()].join(",");
            parts.push(format!("job_type IN ({})", ph));
            params.extend(real_jt);
        }
        if has_ir {
            let ph = vec!["?"; self.industry_raws.len()].join(",");
            parts.push(format!("industry_raw IN ({})", ph));
            params.extend(self.industry_raws.iter().cloned());
        }
        if has_unclass {
            parts.push("(industry_raw IS NULL OR industry_raw = '')".to_string());
        }

        sql.push_str(&format!(" AND ({})", parts.join(" OR ")));
    }
}

/// JSONの配列文字列をVec<String>にパース
fn parse_json_array(s: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(s).unwrap_or_default()
}

/// セッションから共通フィルタ値を取得するヘルパー
pub async fn get_session_filters(session: &Session) -> SessionFilters {
    // 新キー（複数選択）→ 旧キー（単一）のフォールバック
    let job_types: Vec<String> = {
        let new_val: Option<String> = session.get(SESSION_JOB_TYPES_KEY).await.ok().flatten();
        if let Some(ref s) = new_val {
            let parsed = parse_json_array(s);
            if !parsed.is_empty() {
                parsed
            } else {
                // 旧キーからフォールバック
                let old: String = session.get(SESSION_JOB_TYPE_KEY).await.ok().flatten().unwrap_or_default();
                if old.is_empty() { Vec::new() } else { vec![old] }
            }
        } else {
            let old: String = session.get(SESSION_JOB_TYPE_KEY).await.ok().flatten().unwrap_or_default();
            if old.is_empty() { Vec::new() } else { vec![old] }
        }
    };

    let industry_raws: Vec<String> = {
        let val: Option<String> = session.get(SESSION_INDUSTRY_RAWS_KEY).await.ok().flatten();
        val.map(|s| parse_json_array(&s)).unwrap_or_default()
    };

    let prefecture: String = session
        .get(SESSION_PREFECTURE_KEY)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let municipality: String = session
        .get(SESSION_MUNICIPALITY_KEY)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();

    SessionFilters { job_types, industry_raws, prefecture, municipality }
}

/// SQLのWHERE句とパラメータインデックスを構築するヘルパー（hw_db用）
/// SessionFiltersの産業フィルタ+地域フィルタを構築
/// 戻り値: (WHERE句文字列, パラメータ値のVec)
pub fn build_filter_clause(
    filters: &SessionFilters,
    base_index: usize,
) -> (String, Vec<String>) {
    let mut clause = String::new();
    let mut params = Vec::new();

    // 産業フィルタ
    let (ind_clause, ind_params, mut idx) = filters.build_industry_clause(base_index);
    clause.push_str(&ind_clause);
    params.extend(ind_params);

    // 地域フィルタ
    if !filters.prefecture.is_empty() {
        idx += 1;
        clause.push_str(&format!(" AND prefecture = ?{}", idx));
        params.push(filters.prefecture.clone());
    }
    if !filters.municipality.is_empty() {
        idx += 1;
        clause.push_str(&format!(" AND municipality = ?{}", idx));
        params.push(filters.municipality.clone());
    }
    (clause, params)
}

/// SQLのWHERE句とパラメータインデックスを構築するヘルパー（location のみ、旧互換）
pub fn build_hw_location_filter(
    prefecture: &str,
    municipality: &str,
    base_index: usize,
) -> (String, Vec<String>) {
    let mut clause = String::new();
    let mut params = Vec::new();
    let mut idx = base_index;
    if !prefecture.is_empty() {
        idx += 1;
        clause.push_str(&format!(" AND prefecture = ?{}", idx));
        params.push(prefecture.to_string());
    }
    if !municipality.is_empty() {
        idx += 1;
        clause.push_str(&format!(" AND municipality = ?{}", idx));
        params.push(municipality.to_string());
    }
    (clause, params)
}

/// タブ1: 地域概況 - HTMXパーシャルHTML
pub async fn tab_overview(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let db = match &state.hw_db {
        Some(db) => db,
        None => {
            return Html(render_no_db_data("地域概況"));
        }
    };

    let cache_key = format!("overview_{}_{}_{}",
        filters.industry_cache_key(), filters.prefecture, filters.municipality);
    if let Some(cached) = state.cache.get(&cache_key) {
        if let Some(html) = cached.as_str() {
            return Html(html.to_string());
        }
    }

    let db = db.clone();
    let filters_clone = filters.clone();
    let stats = tokio::task::spawn_blocking(move || {
        fetch_overview_stats(&db, &filters_clone)
    }).await.unwrap_or_default();

    let location_label = make_location_label(&filters.prefecture, &filters.municipality);
    let industry_label = filters.industry_label();

    let html = render_overview(&industry_label, &stats, &location_label, &filters.prefecture);

    state.cache.set(cache_key, Value::String(html.clone()));
    Html(html)
}

/// DB未接続時のフォールバックHTML
pub fn render_no_db_data(tab_name: &str) -> String {
    format!(
        r#"<div class="p-8 text-center text-gray-400">
            <h2 class="text-2xl mb-4">{tab_name}</h2>
            <p>データベースが読み込まれていません。</p>
            <p class="text-sm mt-2">hellowork.db を配置してください。</p>
        </div>"#
    )
}

/// 地域ラベル生成
pub fn make_location_label(pref: &str, muni: &str) -> String {
    if pref.is_empty() {
        "全国".to_string()
    } else if muni.is_empty() {
        pref.to_string()
    } else {
        format!("{} {}", pref, muni)
    }
}

/// 概況統計データ
struct OverviewStats {
    total_postings: i64,
    facility_count: i64,
    avg_salary_min: f64,
    fulltime_count: i64,
    fulltime_rate: f64,
    /// 産業別求人数 (job_type, count)
    industry_dist: Vec<(String, i64)>,
    /// 職業大分類別 (occupation_major, count)
    occupation_dist: Vec<(String, i64)>,
    /// 雇用形態分布 (employment_type, count)
    employment_dist: Vec<(String, i64)>,
    /// 給与帯分布 (range_label, count)
    salary_ranges: Vec<(String, i64)>,
    /// 求人理由分布 (recruitment_reason, count)
    recruitment_reasons: Vec<(String, i64)>,
    /// 全国比較用
    national_total: i64,
    national_avg_salary_min: f64,
}

impl Default for OverviewStats {
    fn default() -> Self {
        Self {
            total_postings: 0,
            facility_count: 0,
            avg_salary_min: 0.0,
            fulltime_count: 0,
            fulltime_rate: 0.0,
            industry_dist: Vec::new(),
            occupation_dist: Vec::new(),
            employment_dist: Vec::new(),
            salary_ranges: Vec::new(),
            recruitment_reasons: Vec::new(),
            national_total: 0,
            national_avg_salary_min: 0.0,
        }
    }
}

/// postingsテーブルから概況統計を取得
///
/// 469K行のpostingsテーブルに対するクエリを最適化:
/// - クエリ1: 集約統計（総数・事業所数・平均給与・正社員数・雇用形態分布・求人理由分布）を1回のスキャンで取得
/// - クエリ2-4: GROUP BYクエリ（産業別・職業別・給与帯）は結果形状が異なるため個別実行
/// - クエリ5: 全国比較（都道府県選択時のみ、異なるWHERE句）
fn fetch_overview_stats(
    db: &crate::db::local_sqlite::LocalDb,
    filters: &SessionFilters,
) -> OverviewStats {
    let mut stats = OverviewStats::default();

    let (filter_clause, filter_params) = build_filter_clause(filters, 0);

    // bind_refs を一度だけ構築して全クエリで再利用
    let bind_refs: Vec<&dyn rusqlite::types::ToSql> =
        filter_params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();

    // 1. 集約統計 + 雇用形態分布 + 求人理由分布を1回のクエリで取得
    //    旧クエリ1(総数/事業所数/平均給与/正社員数) + 旧クエリ4(雇用形態) + 旧クエリ6(求人理由)を統合
    //    JSON_GROUP_ARRAY + JSON_OBJECT でグループ集計結果をサブクエリから取得
    {
        let sql = format!(
            "SELECT \
               COUNT(*) as cnt, \
               COUNT(DISTINCT facility_name) as fac_cnt, \
               AVG(NULLIF(salary_min, 0)) as avg_min, \
               SUM(CASE WHEN employment_type = '正社員' THEN 1 ELSE 0 END) as ft_cnt, \
               (\
                 SELECT JSON_GROUP_ARRAY(JSON_OBJECT('name', sub.employment_type, 'cnt', sub.c)) \
                 FROM (\
                   SELECT employment_type, COUNT(*) as c FROM postings \
                   WHERE 1=1{fc} AND employment_type IS NOT NULL AND employment_type != '' \
                   GROUP BY employment_type ORDER BY c DESC\
                 ) sub\
               ) as emp_dist_json, \
               (\
                 SELECT JSON_GROUP_ARRAY(JSON_OBJECT('name', sub2.recruitment_reason, 'cnt', sub2.c)) \
                 FROM (\
                   SELECT recruitment_reason, COUNT(*) as c FROM postings \
                   WHERE 1=1{fc} AND recruitment_reason IS NOT NULL AND recruitment_reason != '' \
                   GROUP BY recruitment_reason ORDER BY c DESC\
                 ) sub2\
               ) as reason_dist_json \
             FROM postings WHERE 1=1{fc}",
            fc = filter_clause,
        );

        // SQLite の ?N 位置パラメータは同一番号が複数出現しても同じ値にバインドされる
        // サブクエリ内で同じ ?1, ?2, ... を参照するためパラメータは1セットで十分
        if let Ok(rows) = db.query(&sql, &bind_refs) {
            if let Some(row) = rows.first() {
                stats.total_postings = get_i64(row, "cnt");
                stats.facility_count = get_i64(row, "fac_cnt");
                stats.avg_salary_min = get_f64(row, "avg_min");
                stats.fulltime_count = get_i64(row, "ft_cnt");
                stats.fulltime_rate = if stats.total_postings > 0 {
                    (stats.fulltime_count as f64 / stats.total_postings as f64) * 100.0
                } else {
                    0.0
                };

                // 雇用形態分布をJSONからパース
                let emp_json = get_str(row, "emp_dist_json");
                if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&emp_json) {
                    for item in &arr {
                        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let cnt = item.get("cnt").and_then(|v| v.as_i64()).unwrap_or(0);
                        if !name.is_empty() {
                            stats.employment_dist.push((name.to_string(), cnt));
                        }
                    }
                }

                // 求人理由分布をJSONからパース
                let reason_json = get_str(row, "reason_dist_json");
                if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&reason_json) {
                    for item in &arr {
                        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let cnt = item.get("cnt").and_then(|v| v.as_i64()).unwrap_or(0);
                        if !name.is_empty() {
                            stats.recruitment_reasons.push((name.to_string(), cnt));
                        }
                    }
                }
            }
        }
    }

    // 2. 産業別求人数 (GROUP BY job_type - 独自の結果形状)
    {
        let sql = format!(
            "SELECT job_type, COUNT(*) as cnt FROM postings \
             WHERE 1=1{filter_clause} AND job_type IS NOT NULL AND job_type != '' \
             GROUP BY job_type ORDER BY cnt DESC"
        );
        if let Ok(rows) = db.query(&sql, &bind_refs) {
            for row in &rows {
                let jt = get_str(row, "job_type");
                let cnt = get_i64(row, "cnt");
                if !jt.is_empty() {
                    stats.industry_dist.push((jt, cnt));
                }
            }
        }
    }

    // 3. 職業大分類別 (GROUP BY occupation_major - 独自の結果形状)
    {
        let sql = format!(
            "SELECT occupation_major, COUNT(*) as cnt FROM postings \
             WHERE 1=1{filter_clause} AND occupation_major IS NOT NULL AND occupation_major != '' \
             GROUP BY occupation_major ORDER BY cnt DESC LIMIT 15"
        );
        if let Ok(rows) = db.query(&sql, &bind_refs) {
            for row in &rows {
                let om = get_str(row, "occupation_major");
                let cnt = get_i64(row, "cnt");
                if !om.is_empty() {
                    stats.occupation_dist.push((om, cnt));
                }
            }
        }
    }

    // 4. 給与帯分布 (GROUP BY CASE式 + 追加WHERE条件 salary_min > 0)
    {
        let sql = format!(
            "SELECT \
               CASE \
                 WHEN salary_min < 150000 THEN '~15万' \
                 WHEN salary_min < 200000 THEN '15~20万' \
                 WHEN salary_min < 250000 THEN '20~25万' \
                 WHEN salary_min < 300000 THEN '25~30万' \
                 WHEN salary_min < 350000 THEN '30~35万' \
                 ELSE '35万~' \
               END as range_label, \
               COUNT(*) as cnt \
             FROM postings \
             WHERE 1=1{filter_clause} AND salary_min > 0 AND salary_type = '月給' \
             GROUP BY range_label ORDER BY MIN(salary_min)"
        );
        if let Ok(rows) = db.query(&sql, &bind_refs) {
            for row in &rows {
                let label = get_str(row, "range_label");
                let cnt = get_i64(row, "cnt");
                if !label.is_empty() {
                    stats.salary_ranges.push((label, cnt));
                }
            }
        }
    }

    // 5. 全国比較（都道府県選択時のみ、産業フィルタのみ適用）
    if !filters.prefecture.is_empty() {
        let national_filters = SessionFilters {
            job_types: filters.job_types.clone(),
            industry_raws: filters.industry_raws.clone(),
            prefecture: String::new(),
            municipality: String::new(),
        };
        let (jt_filter, jt_params) = build_filter_clause(&national_filters, 0);
        let sql = format!(
            "SELECT COUNT(*) as cnt, AVG(NULLIF(salary_min, 0)) as avg_min \
             FROM postings WHERE 1=1{jt_filter}"
        );
        let nat_refs: Vec<&dyn rusqlite::types::ToSql> =
            jt_params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
        if let Ok(rows) = db.query(&sql, &nat_refs) {
            if let Some(row) = rows.first() {
                stats.national_total = get_i64(row, "cnt");
                stats.national_avg_salary_min = get_f64(row, "avg_min");
            }
        }
    }

    stats
}

/// 3層比較パネルのHTML生成
fn build_comparison_section(
    stats: &OverviewStats,
    prefecture: &str,
    location_label: &str,
) -> String {
    if prefecture.is_empty() || stats.national_total == 0 {
        return String::new();
    }

    let region_label = location_label;

    fn bar_row(
        label: &str,
        nat_val: f64,
        region_val: f64,
        region_label: &str,
        unit: &str,
    ) -> String {
        let max_val = nat_val.max(region_val).max(0.001);
        let nat_pct = (nat_val / max_val * 100.0).round();
        let reg_pct = (region_val / max_val * 100.0).round();
        let diff = region_val - nat_val;
        let diff_sign = if diff > 0.0 { "+" } else { "" };
        let diff_color = if diff > 0.0 {
            "text-emerald-400"
        } else if diff < 0.0 {
            "text-rose-400"
        } else {
            "text-slate-400"
        };

        let region_label_short = if region_label.chars().count() > 5 {
            region_label.chars().take(5).collect::<String>() + "..."
        } else {
            region_label.to_string()
        };

        format!(
            r#"<div>
    <div class="text-xs text-slate-500 mb-1">{label}</div>
    <div class="flex items-center gap-2 text-sm">
        <span class="w-16 text-slate-400 shrink-0">全国</span>
        <div class="flex-1 bg-slate-700 rounded h-5 overflow-hidden">
            <div class="bg-blue-500/70 h-full rounded" style="width: {nat_pct}%"></div>
        </div>
        <span class="w-24 text-right text-slate-300">{nat_val:.0}{unit}</span>
    </div>
    <div class="flex items-center gap-2 text-sm mt-1">
        <span class="w-16 text-cyan-400 shrink-0 truncate" title="{region_label}">{region_label_short}</span>
        <div class="flex-1 bg-slate-700 rounded h-5 overflow-hidden">
            <div class="bg-cyan-500 h-full rounded" style="width: {reg_pct}%"></div>
        </div>
        <span class="w-24 text-right text-slate-300">{region_val:.0}{unit}</span>
    </div>
    <div class="text-right text-xs {diff_color} mt-0.5">差: {diff_sign}{diff_abs:.0}{unit}</div>
</div>"#,
            label = label,
            nat_pct = nat_pct,
            nat_val = nat_val,
            unit = unit,
            region_label = region_label,
            region_label_short = region_label_short,
            reg_pct = reg_pct,
            region_val = region_val,
            diff_color = diff_color,
            diff_sign = diff_sign,
            diff_abs = diff.abs(),
        )
    }

    let salary_bar = bar_row(
        "平均給与下限",
        stats.national_avg_salary_min,
        stats.avg_salary_min,
        region_label,
        "円",
    );

    format!(
        r#"<div class="stat-card border-l-4 border-cyan-600">
    <h3 class="text-sm text-slate-400 mb-4">全国 vs {region_label} 比較</h3>
    <div class="space-y-5">
        {salary_bar}
    </div>
</div>"#,
        region_label = region_label,
        salary_bar = salary_bar,
    )
}

/// HTMLレンダリング（インラインHTML生成）
fn render_overview(
    industry_label: &str,
    stats: &OverviewStats,
    location_label: &str,
    prefecture: &str,
) -> String {
    // 比較セクション
    let comparison_section = build_comparison_section(stats, prefecture, location_label);

    // 産業別横棒グラフ
    let industry_chart = build_horizontal_bar_chart(
        &stats.industry_dist,
        "産業別求人数",
        "#3B82F6",
        400,
    );

    // 職業大分類横棒グラフ
    let occupation_chart = build_horizontal_bar_chart(
        &stats.occupation_dist,
        "職業大分類別求人数",
        "#8B5CF6",
        400,
    );

    // 雇用形態ドーナツ
    let emp_colors = |e: &str| -> &str {
        match e {
            "正社員" | "正職員" => "#009E73",
            "パート" | "パートタイム" => "#CC79A7",
            "契約社員" | "契約職員" => "#56B4E9",
            "派遣" | "派遣社員" => "#8b5cf6",
            "業務委託" => "#E69F00",
            _ => "#999999",
        }
    };
    let emp_pie_data: Vec<String> = stats
        .employment_dist
        .iter()
        .map(|(e, v)| {
            format!(
                r#"{{"value": {}, "name": "{}", "itemStyle": {{"color": "{}"}}}}"#,
                v,
                e,
                emp_colors(e)
            )
        })
        .collect();

    // 給与帯棒グラフ
    let salary_labels: Vec<String> = stats
        .salary_ranges
        .iter()
        .map(|(l, _)| format!("\"{}\"", l))
        .collect();
    let salary_values: Vec<String> = stats.salary_ranges.iter().map(|(_, v)| v.to_string()).collect();

    // 求人理由ドーナツ
    let reason_pie_data: Vec<String> = stats
        .recruitment_reasons
        .iter()
        .take(10)
        .map(|(r, v)| format!(r#"{{"value": {}, "name": "{}"}}"#, v, r))
        .collect();

    // 正社員率の表示
    let ft_rate_display = format!("{:.1}%", stats.fulltime_rate);

    format!(
        r##"<div class="space-y-6">
    <h2 class="text-xl font-bold text-white">📊 地域概況 <span class="text-blue-400 text-base font-normal">{industry_label} / {location_label}</span></h2>

    {comparison_section}

    <!-- KPIカード -->
    <div class="grid-stats">
        <div class="stat-card">
            <div class="stat-value text-blue-400">{total_count}</div>
            <div class="stat-label">総求人数</div>
        </div>
        <div class="stat-card">
            <div class="stat-value text-emerald-400">{facility_count}</div>
            <div class="stat-label">事業所数</div>
        </div>
        <div class="stat-card">
            <div class="stat-value text-amber-400">{avg_salary}<span class="text-lg">円</span></div>
            <div class="stat-label">平均月給</div>
        </div>
        <div class="stat-card">
            <div class="stat-value text-cyan-400">{ft_rate}</div>
            <div class="stat-label">正社員率</div>
        </div>
    </div>

    <!-- 産業別 + 職業大分類 -->
    <div class="grid-charts">
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">産業別求人数</h3>
            {industry_chart}
        </div>
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">職業大分類別求人数</h3>
            {occupation_chart}
        </div>
    </div>

    <!-- 雇用形態 + 給与帯 -->
    <div class="grid-charts">
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">雇用形態分布</h3>
            <div class="echart" style="height:320px;" data-chart-config='{{
                "tooltip": {{"trigger": "item", "formatter": "{{b}}: {{c}}件 ({{d}}%)"}},
                "legend": {{"orient": "horizontal", "bottom": 0, "textStyle": {{"color": "#94a3b8", "fontSize": 11}}}},
                "series": [{{
                    "type": "pie",
                    "radius": ["40%", "70%"],
                    "center": ["50%", "48%"],
                    "avoidLabelOverlap": true,
                    "itemStyle": {{"borderRadius": 6, "borderColor": "#0f172a", "borderWidth": 2}},
                    "label": {{"show": true, "color": "#e2e8f0", "fontSize": 12, "formatter": "{{b}}\n{{d}}%"}},
                    "data": [{emp_pie_data}]
                }}]
            }}'></div>
        </div>
        <div class="stat-card">
            <h3 class="text-sm text-slate-400 mb-3">給与帯分布（月給下限ベース）</h3>
            <div class="echart" style="height:320px;" data-chart-config='{{
                "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}}},
                "xAxis": {{"type": "category", "data": [{salary_labels}]}},
                "yAxis": {{"type": "value", "name": "件数"}},
                "series": [{{
                    "type": "bar",
                    "data": [{salary_values}],
                    "itemStyle": {{"color": "#6366F1", "borderRadius": [4, 4, 0, 0]}},
                    "barWidth": "50%"
                }}]
            }}'></div>
        </div>
    </div>

    <!-- 求人理由 -->
    {reason_section}
</div>"##,
        industry_label = industry_label,
        location_label = location_label,
        comparison_section = comparison_section,
        total_count = format_number(stats.total_postings),
        facility_count = format_number(stats.facility_count),
        avg_salary = if stats.avg_salary_min > 0.0 {
            format!("{:.0}", stats.avg_salary_min)
        } else {
            "-".to_string()
        },
        ft_rate = ft_rate_display,
        industry_chart = industry_chart,
        occupation_chart = occupation_chart,
        emp_pie_data = emp_pie_data.join(","),
        salary_labels = salary_labels.join(","),
        salary_values = salary_values.join(","),
        reason_section = if !stats.recruitment_reasons.is_empty() {
            format!(
                r##"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-3">求人理由分布</h3>
        <div class="echart" style="height:350px;" data-chart-config='{{
            "tooltip": {{"trigger": "item", "formatter": "{{b}}: {{c}}件 ({{d}}%)"}},
            "legend": {{"orient": "horizontal", "bottom": 0, "textStyle": {{"color": "#94a3b8", "fontSize": 11}}}},
            "series": [{{
                "type": "pie",
                "radius": ["35%", "65%"],
                "center": ["60%", "50%"],
                "data": [{}]
            }}]
        }}'></div>
    </div>"##,
                reason_pie_data.join(",")
            )
        } else {
            String::new()
        },
    )
}

/// 横棒グラフの共通ビルダー
fn build_horizontal_bar_chart(
    data: &[(String, i64)],
    _title: &str,
    color: &str,
    height: u32,
) -> String {
    if data.is_empty() {
        return r##"<p class="text-slate-500 text-sm text-center py-12">データがありません</p>"##
            .to_string();
    }

    let labels: Vec<String> = data.iter().rev().map(|(l, _)| format!("\"{}\"", l)).collect();
    let values: Vec<String> = data.iter().rev().map(|(_, v)| v.to_string()).collect();

    format!(
        r##"<div class="echart" style="height:{height}px;" data-chart-config='{{
            "tooltip": {{"trigger": "axis", "axisPointer": {{"type": "shadow"}}}},
            "grid": {{"left": "25%", "right": "10%", "top": "5%", "bottom": "10%"}},
            "xAxis": {{"type": "value"}},
            "yAxis": {{"type": "category", "data": [{labels}]}},
            "series": [{{
                "type": "bar",
                "data": [{values}],
                "itemStyle": {{"color": "{color}", "borderRadius": [0, 8, 8, 0]}},
                "label": {{"show": true, "position": "right", "color": "#e2e8f0"}}
            }}]
        }}'></div>"##,
        height = height,
        labels = labels.join(","),
        values = values.join(","),
        color = color,
    )
}

// ヘルパー関数はhelpers.rsに統一、後方互換のため再エクスポート
pub use super::helpers::{escape_html, format_number, get_str, get_i64, get_f64, get_str_html};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number_basic() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(1), "1");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(1234567), "1,234,567");
    }

    #[test]
    fn test_format_number_negative() {
        assert_eq!(format_number(-1234), "-1,234");
    }

    #[test]
    fn test_get_str_exists() {
        let mut map = HashMap::new();
        map.insert("name".to_string(), Value::String("Alice".to_string()));
        assert_eq!(get_str(&map, "name"), "Alice");
    }

    #[test]
    fn test_get_str_missing() {
        let map = HashMap::new();
        assert_eq!(get_str(&map, "name"), "");
    }

    #[test]
    fn test_get_i64_integer() {
        let mut map = HashMap::new();
        map.insert("count".to_string(), serde_json::json!(42));
        assert_eq!(get_i64(&map, "count"), 42);
    }

    #[test]
    fn test_get_i64_float_conversion() {
        let mut map = HashMap::new();
        map.insert("count".to_string(), serde_json::json!(42.9));
        assert_eq!(get_i64(&map, "count"), 42);
    }

    #[test]
    fn test_get_i64_string_parse() {
        let mut map = HashMap::new();
        map.insert("count".to_string(), Value::String("100".to_string()));
        assert_eq!(get_i64(&map, "count"), 100);
    }

    #[test]
    fn test_get_i64_missing() {
        let map = HashMap::new();
        assert_eq!(get_i64(&map, "count"), 0);
    }

    #[test]
    fn test_get_f64_float() {
        let mut map = HashMap::new();
        map.insert("score".to_string(), serde_json::json!(3.14));
        assert!((get_f64(&map, "score") - 3.14).abs() < 0.001);
    }

    #[test]
    fn test_get_f64_missing() {
        let map = HashMap::new();
        assert_eq!(get_f64(&map, "score"), 0.0);
    }

    #[test]
    fn test_build_filter_clause_empty() {
        let filters = SessionFilters {
            job_types: vec![],
            industry_raws: vec![],
            prefecture: String::new(),
            municipality: String::new(),
        };
        let (clause, params) = build_filter_clause(&filters, 0);
        assert_eq!(clause, "");
        assert!(params.is_empty());
    }

    #[test]
    fn test_build_filter_clause_job_only() {
        let filters = SessionFilters {
            job_types: vec!["建設業".to_string()],
            industry_raws: vec![],
            prefecture: String::new(),
            municipality: String::new(),
        };
        let (clause, params) = build_filter_clause(&filters, 0);
        assert_eq!(clause, " AND (job_type IN (?1))");
        assert_eq!(params, vec!["建設業"]);
    }

    #[test]
    fn test_build_filter_clause_all() {
        let filters = SessionFilters {
            job_types: vec!["建設業".to_string()],
            industry_raws: vec![],
            prefecture: "東京都".to_string(),
            municipality: "新宿区".to_string(),
        };
        let (clause, params) = build_filter_clause(&filters, 0);
        assert_eq!(clause, " AND (job_type IN (?1)) AND prefecture = ?2 AND municipality = ?3");
        assert_eq!(params, vec!["建設業", "東京都", "新宿区"]);
    }

    #[test]
    fn test_build_filter_clause_industry_raws() {
        // 両方指定時は OR 結合
        let filters = SessionFilters {
            job_types: vec!["建設業".to_string()],
            industry_raws: vec!["病院".to_string(), "ソフトウェア業".to_string()],
            prefecture: "東京都".to_string(),
            municipality: String::new(),
        };
        let (clause, params) = build_filter_clause(&filters, 0);
        assert_eq!(clause, " AND (job_type IN (?1) OR industry_raw IN (?2,?3)) AND prefecture = ?4");
        assert_eq!(params, vec!["建設業", "病院", "ソフトウェア業", "東京都"]);

        // industry_rawsのみの場合
        let filters2 = SessionFilters {
            job_types: vec![],
            industry_raws: vec!["病院".to_string()],
            prefecture: String::new(),
            municipality: String::new(),
        };
        let (clause2, params2) = build_filter_clause(&filters2, 0);
        assert_eq!(clause2, " AND (industry_raw IN (?1))");
        assert_eq!(params2, vec!["病院"]);
    }

    #[test]
    fn test_build_filter_clause_multiple_job_types() {
        let filters = SessionFilters {
            job_types: vec!["建設業".to_string(), "医療".to_string()],
            industry_raws: vec![],
            prefecture: String::new(),
            municipality: String::new(),
        };
        let (clause, params) = build_filter_clause(&filters, 0);
        assert_eq!(clause, " AND (job_type IN (?1,?2))");
        assert_eq!(params, vec!["建設業", "医療"]);
    }

    #[test]
    fn test_build_filter_clause_unclassified() {
        // 「未分類」のみ選択
        let filters = SessionFilters {
            job_types: vec!["未分類".to_string()],
            industry_raws: vec![],
            prefecture: String::new(),
            municipality: String::new(),
        };
        let (clause, params) = build_filter_clause(&filters, 0);
        assert_eq!(clause, " AND ((industry_raw IS NULL OR industry_raw = ''))");
        assert!(params.is_empty());

        // 「未分類」+ 通常の大分類
        let filters2 = SessionFilters {
            job_types: vec!["建設業".to_string(), "未分類".to_string()],
            industry_raws: vec![],
            prefecture: String::new(),
            municipality: String::new(),
        };
        let (clause2, params2) = build_filter_clause(&filters2, 0);
        assert_eq!(clause2, " AND (job_type IN (?1) OR (industry_raw IS NULL OR industry_raw = ''))");
        assert_eq!(params2, vec!["建設業"]);
    }
}
