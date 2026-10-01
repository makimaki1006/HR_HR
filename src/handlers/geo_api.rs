//! 都道府県・市区町村の一覧 (React 画面用 JSON API、2026-10-01)
//!
//! 旧 HTML 版 (`/api/prefectures` / `/api/municipalities_cascade`、`handlers::api`) と
//! 同じ DB 取得・並べ替えの関数 (`list_prefectures` / `list_municipalities`) を共有する。
//! HTML 版と JSON 版でロジックを複製しない。
//!
//! - `GET /api/app/geo/prefectures` → `GeoPrefectureOption[]`
//! - `GET /api/app/geo/municipalities?prefecture=<名前>` → `GeoMunicipalityOption[]`
//!
//! 認証は `protected_routes` 側 (app_api::router を merge している箇所) に任せる。

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::db::local_sqlite::LocalDb;
use crate::models::job_seeker::PREFECTURE_ORDER;
use crate::AppState;

/// 都道府県の選択肢。`prefcode` は JIS 都道府県コード (北海道=1 … 沖縄=47)。
/// `PREFECTURE_ORDER` に無い名前は null。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct GeoPrefectureOption {
    pub name: String,
    pub prefcode: Option<u8>,
}

/// 市区町村の選択肢。`citycode` は市区町村マスタ (`geo::city_code`) に無ければ null。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct GeoMunicipalityOption {
    pub name: String,
    pub citycode: Option<u32>,
}

/// postings の DISTINCT prefecture を JIS 北→南順に並べる。未知の名前は末尾。
pub(crate) fn list_prefectures(db: &LocalDb) -> Vec<String> {
    let mut prefs = db
        .query(
            "SELECT DISTINCT prefecture FROM postings WHERE prefecture IS NOT NULL AND prefecture != ''",
            &[],
        )
        .unwrap_or_default()
        .iter()
        .filter_map(|r| {
            r.get("prefecture")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect::<Vec<String>>();
    prefs.sort_by_key(|p| {
        PREFECTURE_ORDER
            .iter()
            .position(|&o| o == p.as_str())
            .unwrap_or(99)
    });
    prefs
}

/// 指定都道府県の市区町村 (ORDER BY municipality) と citycode。
pub(crate) fn list_municipalities(db: &LocalDb, pref: &str) -> Vec<(String, Option<u32>)> {
    db.query(
        "SELECT DISTINCT municipality FROM postings WHERE prefecture = ?1 AND municipality IS NOT NULL AND municipality != '' ORDER BY municipality",
        &[&pref as &dyn rusqlite::types::ToSql],
    )
    .unwrap_or_default()
    .iter()
    .filter_map(|r| r.get("municipality").and_then(|v| v.as_str()))
    .map(|m| (m.to_string(), crate::geo::city_code::city_name_to_code(pref, m)))
    .collect()
}

/// 非同期ラッパ。DB 未接続なら空。
pub(crate) async fn fetch_prefectures(db: Option<&LocalDb>) -> Vec<String> {
    match db {
        Some(db) => {
            let db = db.clone();
            tokio::task::spawn_blocking(move || list_prefectures(&db))
                .await
                .unwrap_or_default()
        }
        None => Vec::new(),
    }
}

/// 非同期ラッパ。DB 未接続なら空。
pub(crate) async fn fetch_municipalities(
    db: Option<&LocalDb>,
    pref: &str,
) -> Vec<(String, Option<u32>)> {
    match db {
        Some(db) => {
            let db = db.clone();
            let pref = pref.to_string();
            tokio::task::spawn_blocking(move || list_municipalities(&db, &pref))
                .await
                .unwrap_or_default()
        }
        None => Vec::new(),
    }
}

/// JIS 都道府県コード。`PREFECTURE_ORDER` は JIS 順なので位置 + 1。
fn prefcode_of(name: &str) -> Option<u8> {
    PREFECTURE_ORDER
        .iter()
        .position(|&o| o == name)
        .map(|i| (i + 1) as u8)
}

async fn get_prefectures(State(state): State<Arc<AppState>>) -> Json<Vec<GeoPrefectureOption>> {
    let prefs = fetch_prefectures(state.hw_db.as_ref()).await;
    Json(
        prefs
            .into_iter()
            .map(|name| GeoPrefectureOption {
                prefcode: prefcode_of(&name),
                name,
            })
            .collect(),
    )
}

#[derive(Deserialize)]
struct MunicipalitiesQuery {
    prefecture: Option<String>,
}

async fn get_municipalities(
    State(state): State<Arc<AppState>>,
    Query(params): Query<MunicipalitiesQuery>,
) -> Json<Vec<GeoMunicipalityOption>> {
    let pref = params.prefecture.as_deref().unwrap_or("");
    if pref.is_empty() {
        return Json(Vec::new());
    }
    let munis = fetch_municipalities(state.hw_db.as_ref(), pref).await;
    Json(
        munis
            .into_iter()
            .map(|(name, citycode)| GeoMunicipalityOption { name, citycode })
            .collect(),
    )
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/app/geo/prefectures", get(get_prefectures))
        .route("/api/app/geo/municipalities", get(get_municipalities))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefcodeはpref_name_to_codeと全47件で一致する() {
        let m = crate::geo::pref_name_to_code();
        for (i, name) in PREFECTURE_ORDER.iter().enumerate() {
            let expect: u8 = m.get(name).unwrap().parse().unwrap();
            assert_eq!(prefcode_of(name), Some(expect), "{name}");
            assert_eq!(expect as usize, i + 1, "{name}");
        }
        assert_eq!(prefcode_of("架空県"), None);
    }

    #[test]
    fn ts型の宣言() {
        let cfg = ts_rs::Config::default();
        let p = GeoPrefectureOption::decl(&cfg);
        assert!(p.contains("name: string,"), "{p}");
        assert!(p.contains("prefcode: number | null,"), "{p}");
        let m = GeoMunicipalityOption::decl(&cfg);
        assert!(m.contains("name: string,"), "{m}");
        assert!(m.contains("citycode: number | null,"), "{m}");
    }
}
