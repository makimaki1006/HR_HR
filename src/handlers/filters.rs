//! ヘッダーフィルタの読み出し API (React 向け、2026-09-30)
//!
//! - `GET /api/filters/current`: session に入っている都道府県 / 市区町村 / 職種 / 産業を JSON で返す。
//!   未選択は `""` / `[]`。読む session キーと意味は `overview::get_session_filters` と同じ
//!   (新キー → 旧キーのフォールバック込み)。書き込みは既存の `POST /api/set_*` のまま。
//! - `resolve_filters()`: 「URL クエリ (`pref` / `muni` / `ind` / `jt`) があればクエリ優先、
//!   無ければ session」の helper。React 側 (`frontend/src/shell/useFilterState.ts`) の
//!   起動時の決め方と同じ規則にしてある。React 向けの JSON API はこれで読む (計画書 §2.2)。
//!   `ind` と `jt` はカンマ区切り (React の `splitList` と同じ: trim して空要素は捨てる)。

use std::sync::Arc;

use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use ts_rs::TS;

use super::overview::{get_session_filters, SessionFilters};
use crate::AppState;

/// `GET /api/filters/current` の応答。React 側 `frontend/src/shell/types.ts` の `FiltersCurrent` と同じ形。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, TS)]
pub struct FiltersCurrent {
    /// 未選択は `""` (全国)。
    pub prefecture: String,
    /// 未選択は `""`。
    pub municipality: String,
    /// 職種 (大分類)。未選択は `[]`。
    pub job_types: Vec<String>,
    /// 産業 (industry_raw)。未選択は `[]`。
    pub industry_raws: Vec<String>,
}

impl From<SessionFilters> for FiltersCurrent {
    fn from(s: SessionFilters) -> Self {
        Self {
            prefecture: s.prefecture,
            municipality: s.municipality,
            job_types: s.job_types,
            industry_raws: s.industry_raws,
        }
    }
}

impl From<&FiltersCurrent> for SessionFilters {
    fn from(f: &FiltersCurrent) -> Self {
        Self {
            job_types: f.job_types.clone(),
            industry_raws: f.industry_raws.clone(),
            prefecture: f.prefecture.clone(),
            municipality: f.municipality.clone(),
        }
    }
}

/// URL クエリのフィルタ (`?pref=東京都&muni=千代田区&ind=a,b&jt=x,y`)。
/// `None` はキーが無いこと。`Some("")` はキーがあって空 (= 解除)。
/// `axum::extract::Query<FilterQuery>` でそのまま受けられる。
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct FilterQuery {
    pub pref: Option<String>,
    pub muni: Option<String>,
    /// industry_raws。カンマ区切り。
    pub ind: Option<String>,
    /// job_types。カンマ区切り。
    pub jt: Option<String>,
}

impl FilterQuery {
    /// 4 キーのどれかがある。
    pub fn is_present(&self) -> bool {
        self.pref.is_some() || self.muni.is_some() || self.ind.is_some() || self.jt.is_some()
    }
}

/// カンマ区切り → 配列。trim して空要素は捨てる (React の `splitList` と同じ)。
pub fn split_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// クエリがあればクエリ、無ければ session。キーごとに決める。
///
/// `pref` だけが来て `muni` が無いときは市区町村を空にする
/// (`set_prefecture` が市区町村をリセットするのと同じ。React の `useFilterState` と同じ規則)。
pub fn resolve_filters(session_filters: &SessionFilters, query: &FilterQuery) -> FiltersCurrent {
    let prefecture = query
        .pref
        .clone()
        .unwrap_or_else(|| session_filters.prefecture.clone());
    let municipality = match (&query.muni, &query.pref) {
        (Some(m), _) => m.clone(),
        (None, Some(_)) => String::new(),
        (None, None) => session_filters.municipality.clone(),
    };
    let job_types = query
        .jt
        .as_deref()
        .map(split_list)
        .unwrap_or_else(|| session_filters.job_types.clone());
    let industry_raws = query
        .ind
        .as_deref()
        .map(split_list)
        .unwrap_or_else(|| session_filters.industry_raws.clone());
    FiltersCurrent {
        prefecture,
        municipality,
        job_types,
        industry_raws,
    }
}

/// session の現在値 (`get_session_filters` と同じ意味)。
pub async fn current_filters(session: &Session) -> FiltersCurrent {
    get_session_filters(session).await.into()
}

/// session + クエリを解決した値。React 向け JSON API はこれを使う (wave-b 向け)。
pub async fn effective_filters(session: &Session, query: &FilterQuery) -> FiltersCurrent {
    resolve_filters(&get_session_filters(session).await, query)
}

/// `GET /api/filters/current`。要ログイン (`protected_routes` の `auth_middleware` 配下)。
pub async fn api_filters_current(session: Session) -> Json<FiltersCurrent> {
    Json(current_filters(&session).await)
}

/// `/api/filters/*` のルータ。`protected_routes` に merge する。
pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/filters/current", get(api_filters_current))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use axum::routing::post;
    use tower::ServiceExt;
    use tower_sessions::{MemoryStore, SessionManagerLayer};

    fn session(pref: &str, muni: &str, jt: &[&str], ind: &[&str]) -> SessionFilters {
        SessionFilters {
            job_types: jt.iter().map(|s| s.to_string()).collect(),
            industry_raws: ind.iter().map(|s| s.to_string()).collect(),
            prefecture: pref.to_string(),
            municipality: muni.to_string(),
        }
    }

    fn q(
        pref: Option<&str>,
        muni: Option<&str>,
        ind: Option<&str>,
        jt: Option<&str>,
    ) -> FilterQuery {
        FilterQuery {
            pref: pref.map(str::to_string),
            muni: muni.map(str::to_string),
            ind: ind.map(str::to_string),
            jt: jt.map(str::to_string),
        }
    }

    #[test]
    fn クエリが無ければsessionの値() {
        let s = session("東京都", "千代田区", &["医療"], &["病院"]);
        let f = resolve_filters(&s, &FilterQuery::default());
        assert_eq!(
            f,
            FiltersCurrent {
                prefecture: "東京都".into(),
                municipality: "千代田区".into(),
                job_types: vec!["医療".into()],
                industry_raws: vec!["病院".into()],
            }
        );
        // 未選択は "" / []
        let f = resolve_filters(&session("", "", &[], &[]), &FilterQuery::default());
        assert_eq!(f, FiltersCurrent::default());
        assert!(!FilterQuery::default().is_present());
    }

    #[test]
    fn クエリがあればキーごとにクエリ優先() {
        let s = session("東京都", "千代田区", &["医療"], &["病院"]);
        // pref だけ → muni は空になる (set_prefecture と同じ)、jt/ind は session のまま
        let f = resolve_filters(&s, &q(Some("北海道"), None, None, None));
        assert_eq!(f.prefecture, "北海道");
        assert_eq!(f.municipality, "");
        assert_eq!(f.job_types, vec!["医療"]);
        assert_eq!(f.industry_raws, vec!["病院"]);
        // pref + muni
        let f = resolve_filters(&s, &q(Some("北海道"), Some("札幌市"), None, None));
        assert_eq!(
            (f.prefecture.as_str(), f.municipality.as_str()),
            ("北海道", "札幌市")
        );
        // muni だけ → pref は session
        let f = resolve_filters(&s, &q(None, Some("港区"), None, None));
        assert_eq!(
            (f.prefecture.as_str(), f.municipality.as_str()),
            ("東京都", "港区")
        );
        // pref= (空) → 全国に解除、muni も空
        let f = resolve_filters(&s, &q(Some(""), None, None, None));
        assert_eq!((f.prefecture.as_str(), f.municipality.as_str()), ("", ""));
        // ind / jt はカンマ区切り、空要素は捨てる、trim する
        let f = resolve_filters(&s, &q(None, None, Some("建設業, 製造業,,"), Some(" 介護 ")));
        assert_eq!(f.industry_raws, vec!["建設業", "製造業"]);
        assert_eq!(f.job_types, vec!["介護"]);
        assert_eq!(f.prefecture, "東京都");
        // ind= (空) → 解除。jt はキーが無いので session
        let f = resolve_filters(&s, &q(None, None, Some(""), None));
        assert!(f.industry_raws.is_empty());
        assert_eq!(f.job_types, vec!["医療"]);
        assert!(q(None, None, Some(""), None).is_present());
    }

    #[test]
    fn split_listの規則() {
        assert_eq!(split_list(""), Vec::<String>::new());
        assert_eq!(split_list(","), Vec::<String>::new());
        assert_eq!(split_list("a"), vec!["a"]);
        assert_eq!(split_list(" a , b ,, c"), vec!["a", "b", "c"]);
    }

    #[test]
    fn filter_queryはaxumのqueryで受けられる() {
        let parse = |s: &str| {
            let uri: axum::http::Uri = format!("/x?{s}").parse().unwrap();
            axum::extract::Query::<FilterQuery>::try_from_uri(&uri)
                .unwrap()
                .0
        };
        assert_eq!(parse(""), FilterQuery::default());
        assert_eq!(
            parse("pref=%E6%9D%B1%E4%BA%AC%E9%83%BD&ind=a%2Cb&jt="),
            q(Some("東京都"), None, Some("a,b"), Some(""))
        );
    }

    #[test]
    fn ts型の宣言はreact側のtypes_tsと同じフィールド() {
        let decl = FiltersCurrent::decl(&ts_rs::Config::default());
        assert!(decl.starts_with("type FiltersCurrent = {"), "{decl}");
        for field in [
            "prefecture: string,",
            "municipality: string,",
            "job_types: Array<string>,",
            "industry_raws: Array<string>,",
        ] {
            assert!(decl.contains(field), "{field} が無い: {decl}");
        }
    }

    /// session に値を入れる小さなルート + 本物の `api_filters_current`。
    async fn set_session(session: Session, body: String) -> &'static str {
        use crate::auth::{
            SESSION_INDUSTRY_RAWS_KEY, SESSION_JOB_TYPES_KEY, SESSION_JOB_TYPE_KEY,
            SESSION_MUNICIPALITY_KEY, SESSION_PREFECTURE_KEY,
        };
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        for (key, name) in [
            (SESSION_PREFECTURE_KEY, "prefecture"),
            (SESSION_MUNICIPALITY_KEY, "municipality"),
            (SESSION_JOB_TYPES_KEY, "job_types_json"),
            (SESSION_INDUSTRY_RAWS_KEY, "industry_raws_json"),
            (SESSION_JOB_TYPE_KEY, "job_type"),
        ] {
            if let Some(s) = v.get(name).and_then(|x| x.as_str()) {
                session.insert(key, s.to_string()).await.unwrap();
            }
        }
        "OK"
    }

    fn app() -> axum::Router {
        axum::Router::new()
            .route("/api/filters/current", get(api_filters_current))
            .route("/_set", post(set_session))
            .layer(SessionManagerLayer::new(MemoryStore::default()))
    }

    async fn get_json(app: &axum::Router, cookie: Option<&str>) -> (StatusCode, serde_json::Value) {
        let mut b = Request::builder().uri("/api/filters/current");
        if let Some(c) = cookie {
            b = b.header("cookie", c);
        }
        let res = app
            .clone()
            .oneshot(b.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let ct = res
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(ct.starts_with("application/json"), "{ct}");
        let body = to_bytes(res.into_body(), 64 * 1024).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test]
    async fn api_filters_currentは未選択なら空文字と空配列() {
        let (status, v) = get_json(&app(), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            v,
            serde_json::json!({"prefecture": "", "municipality": "", "job_types": [], "industry_raws": []})
        );
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            ["prefecture", "municipality", "job_types", "industry_raws"]
        );
    }

    #[tokio::test]
    async fn api_filters_currentはsessionの値をget_session_filtersと同じ意味で返す() {
        let app = app();
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/_set")
                    .body(Body::from(
                        r#"{"prefecture":"東京都","municipality":"千代田区","job_types_json":"[\"医療\",\"介護\"]","industry_raws_json":"[\"病院\"]"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let cookie = res.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let (status, v) = get_json(&app, Some(&cookie)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            v,
            serde_json::json!({"prefecture": "東京都", "municipality": "千代田区", "job_types": ["医療", "介護"], "industry_raws": ["病院"]})
        );

        // 旧キー (単一 job_type) だけのセッション → 新キーが空なら旧キーへフォールバック (get_session_filters と同じ)
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/_set")
                    .body(Body::from(r#"{"job_type":"建設","job_types_json":"[]"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        let cookie = res.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let (_, v) = get_json(&app, Some(&cookie)).await;
        assert_eq!(v["job_types"], serde_json::json!(["建設"]));
        assert_eq!(v["prefecture"], "");
    }
}
