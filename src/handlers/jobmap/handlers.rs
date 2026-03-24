use axum::extract::{Path, Query, State};
use axum::response::Html;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;
use tower_sessions::Session;

use crate::AppState;
use crate::geo::pref_name_to_code;
use crate::handlers::competitive::{build_option, escape_html};
use crate::handlers::overview::get_session_filters;

use super::fetch;
use super::render;
use super::stats;

#[derive(Deserialize)]
pub struct MarkerParams {
    #[serde(default)]
    pub prefecture: String,
    #[serde(default)]
    pub municipality: String,
    #[serde(default)]
    pub radius: Option<f64>,
    #[serde(default)]
    pub employment_type: String,
    #[serde(default)]
    pub salary_type: String,
    #[serde(default)]
    pub south: Option<f64>,
    #[serde(default)]
    pub north: Option<f64>,
    #[serde(default)]
    pub west: Option<f64>,
    #[serde(default)]
    pub east: Option<f64>,
}

#[derive(Deserialize)]
pub struct MuniParams {
    #[serde(default)]
    pub prefecture: String,
}

/// タブ6: 求人地図（初期ページ）
pub async fn tab_jobmap(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Html<String> {
    let filters = get_session_filters(&session).await;
    let industry_label = filters.industry_label();

    let geocoded_db = match &state.hw_db {
        Some(db) => db,
        None => {
            return Html(
                r#"<div class="p-8 text-center text-gray-400">
                    <h2 class="text-2xl mb-4">求人地図</h2>
                    <p>求人地図データベースが読み込まれていません。</p>
                    <p class="text-sm mt-2">hellowork.db を配置してください。</p>
                </div>"#
                    .to_string(),
            );
        }
    };

    // 選択産業のデータ存在チェック
    if !fetch::has_job_type_data(geocoded_db, &filters) {
        return Html(render::render_no_data_message(&industry_label));
    }

    let prefs = fetch::fetch_prefectures(geocoded_db, &filters);
    let pref_options: String = std::iter::once(build_option("", "-- 都道府県 --"))
        .chain(prefs.iter().map(|p| {
            if p == &filters.prefecture {
                format!(
                    r#"<option value="{}" selected>{}</option>"#,
                    escape_html(p),
                    escape_html(p)
                )
            } else {
                build_option(p, p)
            }
        }))
        .collect::<Vec<_>>()
        .join("\n");

    let html = render::render_jobmap_page(&industry_label, &filters.prefecture, &pref_options);
    Html(html)
}

/// マーカーJSON API
pub async fn jobmap_markers(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(params): Query<MarkerParams>,
) -> Json<serde_json::Value> {
    let filters = get_session_filters(&session).await;

    let geocoded_db = match &state.hw_db {
        Some(db) => db,
        None => return Json(serde_json::json!({"markers": [], "total": 0})),
    };

    // ビューポートboundsが全て指定されている場合は矩形検索
    if let (Some(south), Some(north), Some(west), Some(east)) =
        (params.south, params.north, params.west, params.east)
    {
        let (markers, total_available) = fetch::fetch_markers_by_bounds(
            geocoded_db,
            &filters,
            &params.employment_type,
            &params.salary_type,
            south,
            north,
            west,
            east,
        );
        return markers_to_json(&markers, None, total_available);
    }

    let session_pref = filters.prefecture.clone();

    let pref = if params.prefecture.is_empty() {
        &session_pref
    } else {
        &params.prefecture
    };

    if pref.is_empty() {
        return Json(serde_json::json!({
            "markers": [],
            "total": 0,
            "message": "都道府県を選択してください"
        }));
    }

    // GAS再現: 市区町村選択は必須
    if params.municipality.is_empty() {
        return Json(serde_json::json!({
            "markers": [],
            "total": 0,
            "message": "市区町村を選択してください"
        }));
    }

    let radius_km = params.radius.unwrap_or(10.0);

    // 市区町村中心座標を取得
    let center = state.hw_db.as_ref().and_then(|db| {
        fetch::get_muni_center(db, pref, &params.municipality)
            .or_else(|| {
                extract_parent_city(&params.municipality)
                    .and_then(|parent| fetch::get_muni_center(db, pref, &parent))
            })
    });

    let (markers, total_available) = if let Some((clat, clng)) = center {
        fetch::fetch_markers(
            geocoded_db,
            &filters,
            pref,
            "",
            &params.employment_type,
            &params.salary_type,
            clat,
            clng,
            radius_km,
        )
    } else {
        fetch::fetch_markers_by_pref(
            geocoded_db,
            &filters,
            pref,
            &params.municipality,
            &params.employment_type,
            &params.salary_type,
        )
    };

    markers_to_json(&markers, center, total_available)
}

/// 求人詳細カードHTML
pub async fn jobmap_detail(
    State(state): State<Arc<AppState>>,
    Path(posting_id): Path<i64>,
) -> Html<String> {
    let geocoded_db = match &state.hw_db {
        Some(db) => db,
        None => return Html("<p class='text-gray-400'>データなし</p>".to_string()),
    };

    match fetch::fetch_detail(geocoded_db, posting_id) {
        Some(detail) => Html(render::render_detail_card(&detail)),
        None => Html("<p class='text-gray-400'>求人が見つかりません</p>".to_string()),
    }
}

/// ピン留め統計API
pub async fn jobmap_stats(
    Json(req): Json<stats::StatsRequest>,
) -> Json<stats::StatsResult> {
    Json(stats::compute_stats(&req))
}

/// 都道府県→市区町村カスケード
pub async fn jobmap_municipalities(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(params): Query<MuniParams>,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let geocoded_db = match &state.hw_db {
        Some(db) => db,
        None => return Html(build_option("", "-- 市区町村 --")),
    };

    let munis = fetch::fetch_municipalities(geocoded_db, &filters, &params.prefecture);
    let options: String = std::iter::once(build_option("", "-- 市区町村 --"))
        .chain(munis.iter().map(|m| build_option(m, m)))
        .collect::<Vec<_>>()
        .join("\n");

    Html(options)
}

/// 求人詳細JSON API（ピンカード用、全フィールド返却）
pub async fn jobmap_detail_json(
    State(state): State<Arc<AppState>>,
    Path(posting_id): Path<i64>,
) -> Json<serde_json::Value> {
    let geocoded_db = match &state.hw_db {
        Some(db) => db,
        None => return Json(serde_json::json!({})),
    };

    match fetch::fetch_detail(geocoded_db, posting_id) {
        Some(d) => Json(serde_json::json!({
            "facility_name": d.facility_name,
            "job_type": d.job_type,
            "access": d.access,
            "employment_type": d.employment_type,
            "salary_type": d.salary_type,
            "salary_min": d.salary_min,
            "salary_max": d.salary_max,
            "headline": d.headline,
            "job_description": d.job_description,
            "requirements": d.requirements,
            "benefits": d.benefits,
            "working_hours": d.working_hours,
            "holidays": d.holidays,
            "tier3_label_short": d.tier3_label_short,
            "job_number": d.job_number,
            "hello_work_office": d.hello_work_office,
            "recruitment_reason": d.recruitment_reason,
        })),
        None => Json(serde_json::json!({})),
    }
}

// ===== 求職者データAPI（Tab 7 統合） =====

#[derive(Deserialize)]
pub struct SeekerParams {
    #[serde(default)]
    pub prefecture: String,
    #[serde(default)]
    pub municipality: String,
}

/// 求人マーカー + コロプレスJSON API: /api/jobmap/seekers
/// talentmapモジュール削除に伴い、postingsテーブルから直接集計
pub async fn jobmap_seekers(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(params): Query<SeekerParams>,
) -> Json<serde_json::Value> {
    let filters = get_session_filters(&session).await;

    let pref = if params.prefecture.is_empty() {
        &filters.prefecture
    } else {
        &params.prefecture
    };
    let muni = if params.municipality.is_empty() {
        &filters.municipality
    } else {
        &params.municipality
    };

    if pref.is_empty() {
        return Json(serde_json::json!({
            "markers": [],
            "choropleth": {},
            "total": 0,
            "message": "都道府県を選択してください"
        }));
    }

    let db = match &state.hw_db {
        Some(db) => db,
        None => return Json(serde_json::json!({
            "markers": [],
            "choropleth": {},
            "total": 0
        })),
    };

    // postingsテーブルから市区町村別の求人件数・平均座標を集計
    use crate::handlers::overview::{get_f64, get_i64};

    let mut actual_sql = format!(
        "SELECT municipality, COUNT(*) as cnt, \
         AVG(latitude) as avg_lat, AVG(longitude) as avg_lng \
         FROM postings \
         WHERE prefecture = ? AND latitude IS NOT NULL AND latitude != 0"
    );
    let mut actual_params: Vec<String> = vec![pref.to_string()];

    if !muni.is_empty() && muni != "すべて" {
        actual_sql.push_str(" AND municipality = ?");
        actual_params.push(muni.to_string());
    }
    // 産業フィルタ
    filters.append_industry_filter_str(&mut actual_sql, &mut actual_params);
    actual_sql.push_str(" GROUP BY municipality ORDER BY cnt DESC");

    let params_ref: Vec<&dyn rusqlite::types::ToSql> = actual_params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = match db.query(&actual_sql, &params_ref) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("jobmap_seekers query failed: {e}");
            return Json(serde_json::json!({
                "markers": [],
                "choropleth": {},
                "total": 0
            }));
        }
    };

    // マーカーJSON構築
    let mut markers = Vec::new();
    let mut choropleth = serde_json::Map::new();
    let mut total_count: i64 = 0;
    let max_count = rows.iter().map(|r| get_i64(r, "cnt")).max().unwrap_or(1).max(1);

    for row in &rows {
        let m_name = row.get("municipality").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let cnt = get_i64(row, "cnt");
        let avg_lat = get_f64(row, "avg_lat");
        let avg_lng = get_f64(row, "avg_lng");
        total_count += cnt;

        if avg_lat != 0.0 && avg_lng != 0.0 {
            markers.push(serde_json::json!({
                "municipality": m_name,
                "lat": avg_lat,
                "lng": avg_lng,
                "count": cnt,
            }));
        }

        // コロプレス用スタイル（件数に応じた色の濃淡）
        let intensity = (cnt as f64 / max_count as f64).min(1.0);
        let r_val = (59.0 + (59.0 * (1.0 - intensity))) as u8;
        let g_val = (130.0 + (125.0 * (1.0 - intensity))) as u8;
        let b_val = (246.0) as u8;
        let opacity = 0.2 + 0.6 * intensity;
        choropleth.insert(m_name, serde_json::json!({
            "fillColor": format!("rgb({},{},{})", r_val, g_val, b_val),
            "fillOpacity": opacity,
            "weight": 1,
            "color": "#475569",
        }));
    }

    // GeoJSON URL
    let geojson_url = {
        let code_map = pref_name_to_code();
        if let Some(code) = code_map.get(pref.as_str()) {
            // pref_code_to_romajiが利用できないので、コードのみで構築
            format!("/api/geojson/{}.json", code)
        } else {
            String::new()
        }
    };

    // 中心座標
    let (center_lat, center_lng) = if !markers.is_empty() {
        let sum_lat: f64 = markers.iter()
            .filter_map(|m| m.get("lat").and_then(|v| v.as_f64()))
            .sum();
        let sum_lng: f64 = markers.iter()
            .filter_map(|m| m.get("lng").and_then(|v| v.as_f64()))
            .sum();
        (sum_lat / markers.len() as f64, sum_lng / markers.len() as f64)
    } else {
        (36.5, 138.0)
    };

    Json(serde_json::json!({
        "markers": markers,
        "choropleth": choropleth,
        "geojsonUrl": geojson_url,
        "total": total_count,
        "center": {"lat": center_lat, "lng": center_lng}
    }))
}

/// 求人詳細サイドバーHTML API: /api/jobmap/seeker-detail
/// talentmapモジュール削除に伴い、postingsテーブルから直接集計
pub async fn jobmap_seeker_detail(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(params): Query<SeekerParams>,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let pref = if params.prefecture.is_empty() {
        &filters.prefecture
    } else {
        &params.prefecture
    };
    let muni = if params.municipality.is_empty() { "" } else { &params.municipality };

    if pref.is_empty() || muni.is_empty() {
        return Html(r#"<p class="text-gray-400 text-sm">市区町村を選択してください</p>"#.to_string());
    }

    let db = match &state.hw_db {
        Some(db) => db,
        None => return Html(r#"<p class="text-gray-400 text-sm">データベースなし</p>"#.to_string()),
    };

    use crate::handlers::overview::{get_f64, get_i64};

    // 基本統計
    let mut stats_sql = "SELECT COUNT(*) as cnt, \
                         AVG(salary_min) as avg_sal_min, AVG(salary_max) as avg_sal_max \
                         FROM postings \
                         WHERE prefecture = ? AND municipality = ?".to_string();
    let mut params_vec: Vec<String> = vec![pref.to_string(), muni.to_string()];
    filters.append_industry_filter_str(&mut stats_sql, &mut params_vec);

    let params_ref: Vec<&dyn rusqlite::types::ToSql> = params_vec
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let mut html = String::with_capacity(2048);
    html.push_str(r#"<div class="space-y-3 text-sm">"#);
    html.push_str(&format!(
        r#"<div class="text-lg font-bold text-white border-b border-gray-600 pb-1">{} {}</div>"#,
        escape_html(pref), escape_html(muni)
    ));

    if let Ok(rows) = db.query(&stats_sql, &params_ref) {
        if let Some(row) = rows.first() {
            let cnt = get_i64(row, "cnt");
            let avg_min = get_f64(row, "avg_sal_min");
            let avg_max = get_f64(row, "avg_sal_max");

            html.push_str(&format!(
                r#"<div class="grid grid-cols-2 gap-2">
  <div class="bg-gray-700/50 rounded p-2 text-center">
    <div class="text-xs text-gray-400">求人件数</div>
    <div class="text-xl font-bold text-blue-300">{}</div>
  </div>
  <div class="bg-gray-700/50 rounded p-2 text-center">
    <div class="text-xs text-gray-400">平均月給</div>
    <div class="text-sm font-bold text-yellow-300">{} - {}</div>
  </div>
</div>"#,
                cnt,
                format_yen_simple(avg_min as i64),
                format_yen_simple(avg_max as i64),
            ));
        }
    }

    // 雇用形態別
    let mut emp_sql = "SELECT employment_type, COUNT(*) as cnt FROM postings \
                       WHERE prefecture = ? AND municipality = ?".to_string();
    let mut emp_params: Vec<String> = vec![pref.to_string(), muni.to_string()];
    filters.append_industry_filter_str(&mut emp_sql, &mut emp_params);
    emp_sql.push_str(" AND employment_type IS NOT NULL AND employment_type != '' \
                       GROUP BY employment_type ORDER BY cnt DESC LIMIT 5");

    let emp_ref: Vec<&dyn rusqlite::types::ToSql> = emp_params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    if let Ok(rows) = db.query(&emp_sql, &emp_ref) {
        if !rows.is_empty() {
            html.push_str(r#"<div class="text-xs text-gray-400 mt-2">雇用形態</div>"#);
            for row in &rows {
                let emp = row.get("employment_type").and_then(|v| v.as_str()).unwrap_or("");
                let cnt = get_i64(row, "cnt");
                html.push_str(&format!(
                    r#"<div class="flex justify-between text-xs"><span class="text-gray-300">{}</span><span class="text-white font-medium">{}件</span></div>"#,
                    escape_html(emp), cnt
                ));
            }
        }
    }

    html.push_str("</div>");
    Html(html)
}

fn markers_to_json(
    markers: &[fetch::MarkerRow],
    _search_center: Option<(f64, f64)>,
    total_available: usize,
) -> Json<serde_json::Value> {
    let mut result = serde_json::json!({
        "markers": markers,
        "total": markers.len(),
        "totalAvailable": total_available,
    });

    // 常にマーカー重心を center として返す（自治体中心ではなく実データ位置基準）
    if !markers.is_empty() {
        let avg_lat: f64 = markers.iter().map(|m| m.lat).sum::<f64>() / markers.len() as f64;
        let avg_lng: f64 = markers.iter().map(|m| m.lng).sum::<f64>() / markers.len() as f64;
        result["center"] = serde_json::json!({"lat": avg_lat, "lng": avg_lng});
    }

    Json(result)
}

/// 政令指定都市の区名から親市名を抽出
fn extract_parent_city(municipality: &str) -> Option<String> {
    if let Some(shi_pos) = municipality.find('市') {
        let after_shi = &municipality[shi_pos + '市'.len_utf8()..];
        if after_shi.ends_with('区') && !after_shi.is_empty() {
            return Some(municipality[..shi_pos + '市'.len_utf8()].to_string());
        }
    }
    None
}

// ===== コロプレスオーバーレイAPI =====

#[derive(Deserialize)]
pub struct ChoroplethQuery {
    #[serde(default)]
    pub layer: String,
    #[serde(default)]
    pub prefecture: String,
}

/// コロプレスデータオーバーレイAPI: /api/jobmap/choropleth
/// layer パラメータに応じてローカルDB / Turso から市区町村別データを取得し、
/// 色分け情報を返す
pub async fn jobmap_choropleth(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(params): Query<ChoroplethQuery>,
) -> Json<serde_json::Value> {
    let filters = get_session_filters(&session).await;

    let pref = if params.prefecture.is_empty() {
        filters.prefecture.clone()
    } else {
        params.prefecture.clone()
    };

    if pref.is_empty() {
        return Json(serde_json::json!({
            "choropleth": {},
            "legend": [],
            "geojsonUrl": "",
            "error": "都道府県を選択してください"
        }));
    }

    let layer = if params.layer.is_empty() {
        "posting_count".to_string()
    } else {
        params.layer.clone()
    };

    // キャッシュチェック
    let cache_key = format!("choropleth_{}_{}", layer, pref);
    if let Some(cached) = state.cache.get(&cache_key) {
        return Json(cached);
    }

    let hw_db = state.hw_db.clone();
    let turso_db = state.turso_db.clone();
    let pref_clone = pref.clone();
    let layer_clone = layer.clone();
    let filters_clone = filters.clone();

    let result = tokio::task::spawn_blocking(move || {
        choropleth_data(&hw_db, turso_db.as_ref(), &pref_clone, &layer_clone, &filters_clone)
    })
    .await
    .unwrap_or_else(|_| serde_json::json!({
        "choropleth": {},
        "legend": [],
        "geojsonUrl": "",
        "error": "処理エラー"
    }));

    state.cache.set(cache_key, result.clone());
    Json(result)
}

/// コロプレスデータ構築（spawn_blocking 内で実行）
fn choropleth_data(
    hw_db: &Option<crate::db::local_sqlite::LocalDb>,
    turso: Option<&crate::db::turso_http::TursoDb>,
    pref: &str,
    layer: &str,
    filters: &crate::handlers::overview::SessionFilters,
) -> serde_json::Value {
    use crate::handlers::overview::{get_f64, get_i64};

    // 市区町村別の値を取得
    let muni_values: Vec<(String, f64)> = match layer {
        "posting_count" => {
            let db = match hw_db {
                Some(db) => db,
                None => return empty_choropleth(pref),
            };
            let mut sql = "SELECT municipality, COUNT(*) as cnt FROM postings \
                           WHERE prefecture = ? AND municipality != ''".to_string();
            let mut p: Vec<String> = vec![pref.to_string()];
            filters.append_industry_filter_str(&mut sql, &mut p);
            sql.push_str(" GROUP BY municipality");
            let refs: Vec<&dyn rusqlite::types::ToSql> =
                p.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
            db.query(&sql, &refs)
                .unwrap_or_default()
                .iter()
                .map(|r| {
                    let m = r.get("municipality").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let v = get_i64(r, "cnt") as f64;
                    (m, v)
                })
                .collect()
        }
        "avg_salary" => {
            let db = match hw_db {
                Some(db) => db,
                None => return empty_choropleth(pref),
            };
            let mut sql = "SELECT municipality, AVG(salary_min) as avg_sal FROM postings \
                           WHERE prefecture = ? AND municipality != '' AND salary_min > 0".to_string();
            let mut p: Vec<String> = vec![pref.to_string()];
            filters.append_industry_filter_str(&mut sql, &mut p);
            sql.push_str(" GROUP BY municipality");
            let refs: Vec<&dyn rusqlite::types::ToSql> =
                p.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
            db.query(&sql, &refs)
                .unwrap_or_default()
                .iter()
                .map(|r| {
                    let m = r.get("municipality").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let v = get_f64(r, "avg_sal");
                    (m, v)
                })
                .collect()
        }
        "day_night_ratio" => {
            // Turso優先、ローカルフォールバック
            let sql = "SELECT municipality, day_night_ratio \
                       FROM v2_external_daytime_population \
                       WHERE prefecture = ?1 AND municipality != ''";
            let params = vec![pref.to_string()];
            let rows = query_turso_or_local_choropleth(turso, hw_db, sql, &params, "v2_external_daytime_population");
            rows.iter()
                .map(|r| {
                    let m = r.get("municipality").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let v = get_f64(r, "day_night_ratio");
                    (m, v)
                })
                .collect()
        }
        "net_migration" => {
            let sql = "SELECT municipality, net_migration_rate \
                       FROM v2_external_migration \
                       WHERE prefecture = ?1 AND municipality != ''";
            let params = vec![pref.to_string()];
            let rows = query_turso_or_local_choropleth(turso, hw_db, sql, &params, "v2_external_migration");
            rows.iter()
                .map(|r| {
                    let m = r.get("municipality").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let v = get_f64(r, "net_migration_rate");
                    (m, v)
                })
                .collect()
        }
        "vacancy_rate" => {
            let db = match hw_db {
                Some(db) => db,
                None => return empty_choropleth(pref),
            };
            let sql = "SELECT municipality, vacancy_rate FROM v2_vacancy_rate \
                       WHERE prefecture = ?1 AND municipality != '' AND emp_group = '正社員'";
            let p: Vec<&dyn rusqlite::types::ToSql> = vec![&pref as &dyn rusqlite::types::ToSql];
            if !crate::handlers::helpers::table_exists(db, "v2_vacancy_rate") {
                return empty_choropleth(pref);
            }
            db.query(sql, &p)
                .unwrap_or_default()
                .iter()
                .map(|r| {
                    let m = r.get("municipality").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let v = get_f64(r, "vacancy_rate");
                    (m, v)
                })
                .collect()
        }
        "min_wage" => {
            // 都道府県単位の単一値 → 全市区町村に同じ値を設定
            let sql = "SELECT hourly_min_wage FROM v2_external_minimum_wage WHERE prefecture = ?1";
            let params = vec![pref.to_string()];
            let rows = query_turso_or_local_choropleth(turso, hw_db, sql, &params, "v2_external_minimum_wage");
            let wage = rows.first().map(|r| get_f64(r, "hourly_min_wage")).unwrap_or(0.0);
            if wage == 0.0 {
                return empty_choropleth(pref);
            }
            // 市区町村一覧を取得して同じ値を返す
            let db = match hw_db {
                Some(db) => db,
                None => return empty_choropleth(pref),
            };
            let muni_rows = db.query(
                "SELECT DISTINCT municipality FROM postings WHERE prefecture = ? AND municipality != ''",
                &[&pref as &dyn rusqlite::types::ToSql],
            ).unwrap_or_default();
            muni_rows.iter()
                .filter_map(|r| {
                    r.get("municipality").and_then(|v| v.as_str()).map(|m| (m.to_string(), wage))
                })
                .collect()
        }
        _ => {
            return serde_json::json!({
                "choropleth": {},
                "legend": [],
                "geojsonUrl": "",
                "error": format!("不明なレイヤー: {}", layer)
            });
        }
    };

    if muni_values.is_empty() {
        return empty_choropleth(pref);
    }

    // 色計算とJSON構築
    let is_diverging = matches!(layer, "day_night_ratio" | "net_migration");
    let center_value = match layer {
        "day_night_ratio" => 100.0,
        "net_migration" => 0.0,
        _ => 0.0,
    };

    let mut choropleth = serde_json::Map::new();

    if is_diverging {
        // ダイバージングカラーマップ: 中心値からの偏差で青/赤
        let max_deviation = muni_values.iter()
            .map(|(_, v)| (v - center_value).abs())
            .fold(0.0_f64, f64::max)
            .max(0.001);

        for (muni, val) in &muni_values {
            let deviation = val - center_value;
            let norm = (deviation / max_deviation).clamp(-1.0, 1.0);
            let (r_val, g_val, b_val) = if norm < 0.0 {
                // 青方向（中心値未満）
                let t = -norm;
                (
                    (255.0 * (1.0 - t) + 30.0 * t) as u8,
                    (255.0 * (1.0 - t) + 64.0 * t) as u8,
                    (255.0 * (1.0 - t) + 175.0 * t) as u8,
                )
            } else {
                // 赤方向（中心値超過）
                let t = norm;
                (
                    (255.0 * (1.0 - t) + 185.0 * t) as u8,
                    (255.0 * (1.0 - t) + 28.0 * t) as u8,
                    (255.0 * (1.0 - t) + 28.0 * t) as u8,
                )
            };
            let opacity = 0.3 + 0.5 * norm.abs();
            choropleth.insert(muni.clone(), serde_json::json!({
                "fillColor": format!("rgb({},{},{})", r_val, g_val, b_val),
                "fillOpacity": opacity,
                "weight": 1,
                "color": "#475569",
                "value": val,
            }));
        }
    } else {
        // グラデーションカラーマップ（薄→濃）
        let max_val = muni_values.iter()
            .map(|(_, v)| *v)
            .fold(0.0_f64, f64::max)
            .max(0.001);

        for (muni, val) in &muni_values {
            let intensity = (val / max_val).clamp(0.0, 1.0);
            // 薄い青→濃い青
            let r_val = (219.0 + (30.0 - 219.0) * intensity) as u8;
            let g_val = (234.0 + (64.0 - 234.0) * intensity) as u8;
            let b_val = (254.0 + (175.0 - 254.0) * intensity) as u8;
            let opacity = 0.2 + 0.6 * intensity;
            choropleth.insert(muni.clone(), serde_json::json!({
                "fillColor": format!("rgb({},{},{})", r_val, g_val, b_val),
                "fillOpacity": opacity,
                "weight": 1,
                "color": "#475569",
                "value": val,
            }));
        }
    }

    // 凡例生成
    let legend = build_legend(layer, &muni_values, is_diverging, center_value);

    // GeoJSON URL
    let geojson_url = {
        let code_map = pref_name_to_code();
        if let Some(code) = code_map.get(pref) {
            format!("/api/geojson/{}.json", code)
        } else {
            String::new()
        }
    };

    serde_json::json!({
        "choropleth": choropleth,
        "legend": legend,
        "geojsonUrl": geojson_url,
        "layer": layer,
        "prefecture": pref,
        "count": muni_values.len(),
    })
}

/// Turso優先→ローカルDBフォールバックのクエリ実行
fn query_turso_or_local_choropleth(
    turso: Option<&crate::db::turso_http::TursoDb>,
    hw_db: &Option<crate::db::local_sqlite::LocalDb>,
    sql: &str,
    params: &[String],
    local_table: &str,
) -> Vec<std::collections::HashMap<String, serde_json::Value>> {
    // Turso優先
    if let Some(tdb) = turso {
        let p: Vec<&dyn crate::db::turso_http::ToSqlTurso> =
            params.iter().map(|s| s as &dyn crate::db::turso_http::ToSqlTurso).collect();
        match tdb.query(sql, &p) {
            Ok(rows) if !rows.is_empty() => return rows,
            Ok(_) => {}
            Err(e) => {
                tracing::warn!("Turso choropleth query failed, falling back to local: {e}");
            }
        }
    }
    // ローカルDBフォールバック
    if let Some(db) = hw_db {
        if !crate::handlers::helpers::table_exists(db, local_table) {
            return vec![];
        }
        let p: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
        return db.query(sql, &p).unwrap_or_default();
    }
    vec![]
}

/// 凡例データ生成
fn build_legend(
    layer: &str,
    values: &[(String, f64)],
    is_diverging: bool,
    center_value: f64,
) -> Vec<serde_json::Value> {
    if values.is_empty() {
        return vec![];
    }

    let (label, unit) = match layer {
        "posting_count" => ("求人件数", "件"),
        "avg_salary" => ("平均月給", "円"),
        "day_night_ratio" => ("昼夜間人口比率", "%"),
        "net_migration" => ("純移動率", ""),
        "vacancy_rate" => ("欠員率", "%"),
        "min_wage" => ("最低賃金", "円/時"),
        _ => ("値", ""),
    };

    let vals: Vec<f64> = values.iter().map(|(_, v)| *v).collect();
    let min_val = vals.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_val = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    if is_diverging {
        vec![
            serde_json::json!({"color": "rgb(30,64,175)", "label": format!("{}{} 未満", label, center_value)}),
            serde_json::json!({"color": "rgb(255,255,255)", "label": format!("{}{}", label, center_value)}),
            serde_json::json!({"color": "rgb(185,28,28)", "label": format!("{}{} 超過", label, center_value)}),
        ]
    } else {
        let format_val = |v: f64| -> String {
            if v >= 10000.0 {
                format!("{:.1}万{}", v / 10000.0, unit)
            } else {
                format!("{:.0}{}", v, unit)
            }
        };
        vec![
            serde_json::json!({"color": "rgb(219,234,254)", "label": format_val(min_val)}),
            serde_json::json!({"color": "rgb(120,149,214)", "label": format_val((min_val + max_val) / 2.0)}),
            serde_json::json!({"color": "rgb(30,64,175)", "label": format_val(max_val)}),
        ]
    }
}

/// 空のコロプレスレスポンス
fn empty_choropleth(pref: &str) -> serde_json::Value {
    let geojson_url = {
        let code_map = pref_name_to_code();
        if let Some(code) = code_map.get(pref) {
            format!("/api/geojson/{}.json", code)
        } else {
            String::new()
        }
    };
    serde_json::json!({
        "choropleth": {},
        "legend": [],
        "geojsonUrl": geojson_url,
        "count": 0,
    })
}

/// 簡易金額フォーマット
fn format_yen_simple(n: i64) -> String {
    if n == 0 {
        return "-".to_string();
    }
    let man = n / 10000;
    if man > 0 {
        format!("{}万", man)
    } else {
        format!("{}円", n)
    }
}
