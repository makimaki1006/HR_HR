use axum::{
    extract::{Path, Query, State},
    response::{Html, Json},
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;
use tower_sessions::Session;

use super::overview::{format_number, get_i64, get_str};
use crate::models::job_seeker::PREFECTURE_ORDER;
use crate::AppState;

#[derive(Deserialize)]
pub struct GeoJsonQuery {
    pub pref: Option<String>,
}

/// GeoJSON API: /api/geojson/:filename（キャッシュ付き）
pub async fn get_geojson(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> Json<Value> {
    // キャッシュチェック
    let cache_key = format!("geojson_{}", filename);
    if let Some(cached) = state.cache.get(&cache_key) {
        return Json(cached);
    }

    let geojson_dir = "static/geojson";
    let path = format!("{geojson_dir}/{filename}");

    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<Value>(&content) {
            Ok(json) => {
                state.cache.set(cache_key, json.clone());
                Json(json)
            }
            Err(_) => Json(Value::Null),
        },
        Err(_) => Json(Value::Null),
    }
}

#[derive(Deserialize)]
pub struct MarkersQuery {
    pub job_type: Option<String>,
}

/// マーカーAPI: /api/markers?job_type=建設業
pub async fn get_markers(
    State(state): State<Arc<AppState>>,
    Query(params): Query<MarkersQuery>,
) -> Json<Value> {
    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return Json(serde_json::json!([])),
    };

    let job_type = params.job_type.clone();

    let rows = tokio::task::spawn_blocking(move || {
        // job_type指定時のみフィルタ
        let (sql, bind_strings) = if let Some(ref jt) = job_type {
            if !jt.is_empty() {
                (
                    "SELECT prefecture, COUNT(*) as cnt FROM postings WHERE job_type = ?1 GROUP BY prefecture ORDER BY cnt DESC".to_string(),
                    vec![jt.clone()],
                )
            } else {
                (
                    "SELECT prefecture, COUNT(*) as cnt FROM postings GROUP BY prefecture ORDER BY cnt DESC".to_string(),
                    Vec::new(),
                )
            }
        } else {
            (
                "SELECT prefecture, COUNT(*) as cnt FROM postings GROUP BY prefecture ORDER BY cnt DESC".to_string(),
                Vec::new(),
            )
        };

        let bind_refs: Vec<&dyn rusqlite::types::ToSql> =
            bind_strings.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();

        db.query(&sql, &bind_refs).unwrap_or_default()
    }).await.unwrap_or_default();

    // 都道府県→緯度経度マッピング
    let pref_coords: Vec<(&str, f64, f64)> = vec![
        ("北海道", 43.06, 141.35),
        ("青森県", 40.82, 140.74),
        ("岩手県", 39.70, 141.15),
        ("宮城県", 38.27, 140.87),
        ("秋田県", 39.72, 140.10),
        ("山形県", 38.24, 140.34),
        ("福島県", 37.75, 140.47),
        ("茨城県", 36.34, 140.45),
        ("栃木県", 36.57, 139.88),
        ("群馬県", 36.39, 139.06),
        ("埼玉県", 35.86, 139.65),
        ("千葉県", 35.61, 140.12),
        ("東京都", 35.69, 139.69),
        ("神奈川県", 35.45, 139.64),
        ("新潟県", 37.90, 139.02),
        ("富山県", 36.70, 137.21),
        ("石川県", 36.59, 136.63),
        ("福井県", 36.07, 136.22),
        ("山梨県", 35.66, 138.57),
        ("長野県", 36.24, 138.18),
        ("岐阜県", 35.39, 136.72),
        ("静岡県", 34.98, 138.38),
        ("愛知県", 35.18, 136.91),
        ("三重県", 34.73, 136.51),
        ("滋賀県", 35.00, 135.87),
        ("京都府", 35.02, 135.76),
        ("大阪府", 34.69, 135.52),
        ("兵庫県", 34.69, 135.18),
        ("奈良県", 34.69, 135.83),
        ("和歌山県", 34.23, 135.17),
        ("鳥取県", 35.50, 134.24),
        ("島根県", 35.47, 133.05),
        ("岡山県", 34.66, 133.93),
        ("広島県", 34.40, 132.46),
        ("山口県", 34.19, 131.47),
        ("徳島県", 34.07, 134.56),
        ("香川県", 34.34, 134.04),
        ("愛媛県", 33.84, 132.77),
        ("高知県", 33.56, 133.53),
        ("福岡県", 33.61, 130.42),
        ("佐賀県", 33.25, 130.30),
        ("長崎県", 32.74, 129.87),
        ("熊本県", 32.79, 130.74),
        ("大分県", 33.24, 131.61),
        ("宮崎県", 31.91, 131.42),
        ("鹿児島県", 31.56, 130.56),
        ("沖縄県", 26.21, 127.68),
    ];

    let mut markers: Vec<Value> = Vec::new();
    for row in &rows {
        let pref = row.get("prefecture").and_then(|v| v.as_str()).unwrap_or("");
        let cnt = row.get("cnt").and_then(|v| v.as_i64()).unwrap_or(0);

        if let Some((_, lat, lng)) = pref_coords.iter().find(|(name, _, _)| *name == pref) {
            markers.push(serde_json::json!({
                "name": pref,
                "lat": lat,
                "lng": lng,
                "count": cnt
            }));
        }
    }

    Json(Value::Array(markers))
}

#[derive(Deserialize)]
pub struct PrefecturesQuery {
    pub job_type: Option<String>,
}

/// 都道府県一覧API（job_typeフィルタなし）
pub async fn get_prefectures(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(_params): Query<PrefecturesQuery>,
) -> Html<String> {
    let mut prefs = if let Some(db) = &state.hw_db {
        let db = db.clone();
        tokio::task::spawn_blocking(move || {
            db.query(
                "SELECT DISTINCT prefecture FROM postings WHERE prefecture IS NOT NULL AND prefecture != ''",
                &[],
            ).unwrap_or_default()
            .iter()
            .filter_map(|r| r.get("prefecture").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .collect::<Vec<String>>()
        }).await.unwrap_or_default()
    } else {
        Vec::new()
    };

    // JIS北→南順にソート
    prefs.sort_by_key(|p| {
        PREFECTURE_ORDER
            .iter()
            .position(|&o| o == p.as_str())
            .unwrap_or(99)
    });

    let html: String = prefs
        .iter()
        .map(|p| super::competitive::build_option(p, p))
        .collect::<Vec<_>>()
        .join("\n");

    Html(html)
}

#[derive(Deserialize)]
pub struct MunicipalitiesCascadeQuery {
    pub prefecture: Option<String>,
}

/// 市区町村カスケードAPI（job_typeフィルタなし）
pub async fn get_municipalities_cascade(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<MunicipalitiesCascadeQuery>,
) -> Html<String> {
    let prefecture = params.prefecture.as_deref().unwrap_or("");
    if prefecture.is_empty() {
        return Html(String::new());
    }

    let munis = if let Some(db) = &state.hw_db {
        let db = db.clone();
        let pref = prefecture.to_string();
        tokio::task::spawn_blocking(move || {
            db.query(
                "SELECT DISTINCT municipality FROM postings WHERE prefecture = ?1 AND municipality IS NOT NULL AND municipality != '' ORDER BY municipality",
                &[&pref as &dyn rusqlite::types::ToSql],
            ).unwrap_or_default()
            .iter()
            .filter_map(|r| r.get("municipality").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .collect::<Vec<String>>()
        }).await.unwrap_or_default()
    } else {
        Vec::new()
    };

    let html: String = munis
        .iter()
        .map(
            |m| match crate::geo::city_code::city_name_to_code(prefecture, m) {
                Some(code) => super::competitive::build_option_with_data(
                    m,
                    m,
                    &[("citycode", code.to_string())],
                ),
                None => super::competitive::build_option(m, m),
            },
        )
        .collect::<Vec<_>>()
        .join("\n");

    Html(html)
}

#[derive(Deserialize)]
pub struct IndustriesQuery {
    pub prefecture: Option<String>,
    pub municipality: Option<String>,
}

/// 産業一覧API（地域フィルタ付き、件数表示）
pub async fn get_industries(
    State(state): State<Arc<AppState>>,
    Query(params): Query<IndustriesQuery>,
) -> Html<String> {
    let prefecture = params.prefecture.as_deref().unwrap_or("").to_string();
    let municipality = params.municipality.as_deref().unwrap_or("").to_string();

    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return Html(String::new()),
    };

    let rows = tokio::task::spawn_blocking(move || {
        let (loc_filter, loc_params) =
            super::overview::build_hw_location_filter(&prefecture, &municipality, 0);
        let sql = format!(
            "SELECT job_type, COUNT(*) as cnt FROM postings \
             WHERE 1=1{loc_filter} AND job_type IS NOT NULL AND job_type != '' \
             GROUP BY job_type ORDER BY cnt DESC"
        );
        let bind_refs: Vec<&dyn rusqlite::types::ToSql> = loc_params
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();

        db.query(&sql, &bind_refs).unwrap_or_default()
    })
    .await
    .unwrap_or_default();

    let html: String = rows
        .iter()
        .filter_map(|r| {
            let jt = get_str(r, "job_type");
            let cnt = get_i64(r, "cnt");
            if jt.is_empty() {
                None
            } else {
                Some(format!(
                    r#"<option value="{}">{} ({})</option>"#,
                    jt,
                    jt,
                    format_number(cnt)
                ))
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    Html(html)
}

/// 産業ツリーAPI: /api/industry_tree?prefecture=&municipality=
/// JSON: [{ "major": "建設業", "major_count": N, "subs": [{"name": "...", "count": N}, ...] }, ...]
/// 「未分類」カテゴリも含む（industry_rawがNULL/空の求人）
/// レスポンスはキャッシュ（TTL: AppCacheのデフォルトTTLに従う）
pub async fn get_industry_tree(
    State(state): State<Arc<AppState>>,
    Query(params): Query<IndustriesQuery>,
) -> Json<Value> {
    let prefecture = params.prefecture.as_deref().unwrap_or("").to_string();
    let municipality = params.municipality.as_deref().unwrap_or("").to_string();

    // キャッシュチェック
    let cache_key = format!("industry_tree_{}_{}", prefecture, municipality);
    if let Some(cached) = state.cache.get(&cache_key) {
        return Json(cached);
    }

    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return Json(Value::Array(vec![])),
    };

    // 全DBクエリをspawn_blockingで実行
    let json_result = tokio::task::spawn_blocking(move || {
        let (loc_filter, loc_params) =
            super::overview::build_hw_location_filter(&prefecture, &municipality, 0);

        // 分類済み求人（job_type + industry_raw が両方あるもの）
        let sql = format!(
            "SELECT job_type, industry_raw, COUNT(*) as cnt FROM postings \
             WHERE 1=1{loc_filter} \
             AND job_type IS NOT NULL AND job_type != '' \
             AND industry_raw IS NOT NULL AND industry_raw != '' \
             GROUP BY job_type, industry_raw \
             ORDER BY job_type, cnt DESC"
        );
        let bind_refs: Vec<&dyn rusqlite::types::ToSql> = loc_params
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();

        let rows = db.query(&sql, &bind_refs).unwrap_or_default();

        // BTreeMapで大分類グルーピング
        let mut tree: BTreeMap<String, (i64, Vec<(String, i64)>)> = BTreeMap::new();
        for row in &rows {
            let major = get_str(row, "job_type");
            let sub = get_str(row, "industry_raw");
            let cnt = get_i64(row, "cnt");
            if major.is_empty() || sub.is_empty() {
                continue;
            }
            let entry = tree.entry(major).or_insert((0, Vec::new()));
            entry.0 += cnt;
            entry.1.push((sub, cnt));
        }

        // 未分類カテゴリ: industry_raw が NULL/空 の求人数
        let unclass_sql = format!(
            "SELECT COUNT(*) as cnt FROM postings \
             WHERE 1=1{loc_filter} \
             AND (industry_raw IS NULL OR industry_raw = '')"
        );
        let unclass_refs: Vec<&dyn rusqlite::types::ToSql> = loc_params
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        let unclass_count = db
            .query(&unclass_sql, &unclass_refs)
            .ok()
            .and_then(|rows| {
                rows.first()
                    .and_then(|r| r.get("cnt").and_then(|v| v.as_i64()))
            })
            .unwrap_or(0);

        // 件数降順ソート
        let mut sorted: Vec<_> = tree.into_iter().collect();
        sorted.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));

        let mut result: Vec<Value> = sorted
            .into_iter()
            .map(|(major, (major_count, subs))| {
                let sub_arr: Vec<Value> = subs
                    .into_iter()
                    .map(|(name, count)| serde_json::json!({"name": name, "count": count}))
                    .collect();
                serde_json::json!({
                    "major": major,
                    "major_count": major_count,
                    "subs": sub_arr
                })
            })
            .collect();

        // 未分類カテゴリを末尾に追加（件数 > 0 の場合のみ）
        if unclass_count > 0 {
            result.push(serde_json::json!({
                "major": "未分類",
                "major_count": unclass_count,
                "subs": []
            }));
        }

        Value::Array(result)
    })
    .await
    .unwrap_or_else(|_| Value::Array(vec![]));

    state.cache.set(cache_key, json_result.clone());
    Json(json_result)
}
