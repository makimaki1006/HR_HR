//! React 画面 (`/app/{screen}`) 向けの JSON API (Phase 0-4, 2026-09-29)
//!
//! レスポンス型は `#[derive(Serialize, TS)]` の struct にし、`cargo test` で
//! `frontend/src/generated/*.ts` に TypeScript 型を書き出す (下の `tests::export_ts_bindings`)。
//! Rust 側のフィールド名を変えると生成物が変わり、フロントの `npm run typecheck` が落ちる。
//! CI (rust-test) は再生成後の `frontend/src/generated` に差分が無いことを検査する。
//!
//! 出力先は `#[ts(export)]` + 環境変数 `TS_RS_EXPORT_DIR` ではなく、テストの中で
//! `CARGO_MANIFEST_DIR` から組み立てる。`.cargo/` は .gitignore で各自のローカル設定置き場に
//! なっているため、そこに出力先を書くと環境によって `./bindings` に出てしまう。
//! 型を増やしたら `export_ts_bindings` に 1 行足す。
//!
//! 認証は `protected_routes` 側の `route_layer(auth_middleware)` に任せる (未ログインは 303 /login)。

use std::sync::Arc;

use axum::routing::get;
use axum::{Json, Router};
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use ts_rs::TS;

use crate::AppState;

/// `GET /api/app/ping` の応答。React 基盤の疎通確認 (ダミー画面) 専用で、業務データは返さない。
#[derive(Debug, Clone, Serialize, TS)]
pub struct AppPingResponse {
    /// 固定文言。
    pub message: String,
    /// サーバの現在時刻 (UTC、RFC 3339、秒精度。例: `2026-09-29T09:00:00Z`)。
    pub server_time: String,
}

/// `message` の固定値。テストとフロントの表示確認で使う。
pub const PING_MESSAGE: &str = "pong";

async fn ping() -> Json<AppPingResponse> {
    Json(AppPingResponse {
        message: PING_MESSAGE.to_string(),
        server_time: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
    })
}

/// `/api/app/*` のルータ。`protected_routes` に merge する。
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/app/ping", get(ping))
        // 2026-10-01: 都道府県・市区町村の一覧 (geo_api.rs)
        .merge(super::geo_api::router())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn pingはmessageとserver_timeだけを返す() {
        let app: Router = Router::new().route("/api/app/ping", get(ping));
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/api/app/ping")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ct = res
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(ct.starts_with("application/json"), "{ct}");
        let body = to_bytes(res.into_body(), 64 * 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let obj = v.as_object().unwrap();
        let keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        assert_eq!(keys, ["message", "server_time"]);
        assert_eq!(obj["message"], "pong");
        let t = obj["server_time"].as_str().unwrap();
        let parsed = chrono::DateTime::parse_from_rfc3339(t).expect(t);
        assert_eq!(parsed.offset().local_minus_utc(), 0, "{t}");
        assert!(t.ends_with('Z') && t.len() == 20, "{t}");
    }

    /// TS 型を `frontend/src/generated/` に書き出す。`cargo test --lib` で毎回走る。
    /// 生成物はコミットし、CI は再生成後の差分ゼロを検査する。
    #[test]
    fn export_ts_bindings() {
        let out_dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("frontend/src/generated");
        let cfg = ts_rs::Config::new().with_out_dir(&out_dir);
        AppPingResponse::export_all(&cfg).expect("AppPingResponse の TS 型を書き出せない");
        crate::handlers::crm_metadata::CrmMetadataResponse::export_all(&cfg)
            .expect("CrmMetadataResponse の TS 型を書き出せない");
        crate::crm::call_queue::CallQueueResponse::export_all(&cfg)
            .expect("CallQueueResponse の TS 型を書き出せない");
        crate::crm::call_queue::CallQueuePipelinesResponse::export_all(&cfg)
            .expect("CallQueuePipelinesResponse の TS 型を書き出せない");
        crate::crm::owners::CrmOwnersResponse::export_all(&cfg)
            .expect("CrmOwnersResponse の TS 型を書き出せない");
        crate::crm::workspace::WorkspaceResponse::export_all(&cfg)
            .expect("WorkspaceResponse の TS 型を書き出せない");
        // W8 求人票作成 (/app/jobgen): src/job_gen/contract.rs の要求・応答型
        crate::job_gen::contract::export_ts(&cfg).expect("jobgen 契約型の TS 型を書き出せない");
        crate::handlers::guide::GuideResponse::export_all(&cfg)
            .expect("GuideResponse の TS 型を書き出せない");
        // 競合調査 (/app/competitor): レポート本体 (CompetitorReport と依存型)・選択肢・エラー
        crate::handlers::competitor::CompetitorReportResponse::export_all(&cfg)
            .expect("CompetitorReportResponse の TS 型を書き出せない");
        crate::handlers::competitor::CompetitorOptions::export_all(&cfg)
            .expect("CompetitorOptions の TS 型を書き出せない");
        crate::handlers::competitor::CompetitorError::export_all(&cfg)
            .expect("CompetitorError の TS 型を書き出せない");
        let written = std::fs::read_to_string(out_dir.join("AppPingResponse.ts")).unwrap();
        assert!(
            written.contains("export type AppPingResponse = {"),
            "{written}"
        );
        crate::handlers::geo_api::GeoPrefectureOption::export_all(&cfg)
            .expect("GeoPrefectureOption の TS 型を書き出せない");
        crate::handlers::geo_api::GeoMunicipalityOption::export_all(&cfg)
            .expect("GeoMunicipalityOption の TS 型を書き出せない");
        // 2026-09-30: 共通基盤 (platform-team)。NavResponse は NavItem / NavGroup / NavKind も一緒に書き出す。
        crate::handlers::nav::NavResponse::export_all(&cfg)
            .expect("NavResponse の TS 型を書き出せない");
        crate::handlers::filters::FiltersCurrent::export_all(&cfg)
            .expect("FiltersCurrent の TS 型を書き出せない");
        for (file, head) in [
            ("NavResponse.ts", "export type NavResponse = {"),
            ("NavItem.ts", "export type NavItem = {"),
            ("NavGroup.ts", "export type NavGroup = {"),
            (
                "NavKind.ts",
                "export type NavKind = \"legacy_tab\" | \"page\" | \"app\";",
            ),
            ("FiltersCurrent.ts", "export type FiltersCurrent = {"),
        ] {
            let written = std::fs::read_to_string(out_dir.join(file)).unwrap();
            assert!(written.contains(head), "{file}: {written}");
        }
        let item = std::fs::read_to_string(out_dir.join("NavItem.ts")).unwrap();
        assert!(item.contains("hidden: boolean"), "{item}");
        assert!(item.contains("hidden_reason: string | null"), "{item}");
        assert!(item.contains("hidden_since: string | null"), "{item}");
        // W8 (2026-09-29): admin / my。依存型 (AccountRow 等) も export_all が一緒に書き出す。
        {
            use crate::handlers::admin::{
                AdminLoginFailuresResponse, AdminRoleChangeRequest, AdminRoleChangeResponse,
                AdminUsageResponse, AdminUserDetailResponse, AdminUsersResponse,
            };
            use crate::handlers::my::{
                MyActivityResponse, MyProfileResponse, MyProfileUpdateRequest,
            };
            AdminUsersResponse::export_all(&cfg).expect("AdminUsersResponse");
            AdminUserDetailResponse::export_all(&cfg).expect("AdminUserDetailResponse");
            AdminLoginFailuresResponse::export_all(&cfg).expect("AdminLoginFailuresResponse");
            AdminUsageResponse::export_all(&cfg).expect("AdminUsageResponse");
            AdminRoleChangeRequest::export_all(&cfg).expect("AdminRoleChangeRequest");
            AdminRoleChangeResponse::export_all(&cfg).expect("AdminRoleChangeResponse");
            MyProfileResponse::export_all(&cfg).expect("MyProfileResponse");
            MyActivityResponse::export_all(&cfg).expect("MyActivityResponse");
            MyProfileUpdateRequest::export_all(&cfg).expect("MyProfileUpdateRequest");
            for f in [
                "AccountRow.ts",
                "LoginSessionRow.ts",
                "ActivityLogRow.ts",
                "AdminUserKpi30d.ts",
                "AdminUsageEntry.ts",
            ] {
                assert!(
                    out_dir.join(f).is_file(),
                    "{f} が依存型として書き出されていない"
                );
            }
        }

        // 営業KPI (React 移行 W2、2026-09-30)。`GET /api/sales-kpi/data` の応答型と、その依存型
        // (SalesKpi* に前置して他画面と衝突させない)。i64 は JSON では普通の数なので bigint にしない。
        let cfg_number = ts_rs::Config::new()
            .with_out_dir(&out_dir)
            .with_large_int("number");
        crate::handlers::sales_kpi::SalesKpiData::export_all(&cfg_number)
            .expect("SalesKpiData の TS 型を書き出せない");
        let written = std::fs::read_to_string(out_dir.join("SalesKpiData.ts")).unwrap();
        assert!(
            written.contains("export type SalesKpiData = {"),
            "{written}"
        );
    }

    /// 生成される TS 型の中身。フィールド名・型が変わったらここも落ちる。
    #[test]
    fn ts型の宣言() {
        let decl = AppPingResponse::decl(&ts_rs::Config::default());
        assert!(decl.starts_with("type AppPingResponse = {"), "{decl}");
        assert!(decl.contains("message: string,"), "{decl}");
        assert!(decl.contains("server_time: string,"), "{decl}");
    }
}
