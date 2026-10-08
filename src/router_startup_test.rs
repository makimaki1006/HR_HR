//! Router を組み立てられることを確かめる。
//!
//! ------------------------------------------------------------------
//! なぜこのテストが要るのか（2026-09-07 に本番が21時間出せなかった経緯）
//! ------------------------------------------------------------------
//! マージのときに `.merge(handlers::indeed::router())` が2行残り、
//! 同じパスに2つハンドラが登録された状態で main に入った。
//! 行の位置が違ったので git は衝突と判定せず、両方を採用した。
//!
//! axum は同一パスの二重登録を **Router の組み立て時に panic** で弾く:
//!
//!   thread 'main' panicked at src/lib.rs:668:10:
//!   Overlapping method route. Handler for `GET /tab/indeed` already exists
//!
//! ここが問題で、**`cargo build` も `cargo test` も通ってしまう**。
//! 実際そのとき 3,225 件のテストが全部通っていた。`build_app()` は
//! 起動時にしか呼ばれないので、起動して初めて落ちる。
//! Render はビルドに成功し、起動に失敗し、仕様どおりデプロイを取り消して
//! 古い版に戻し続けた。ログ上は「healthy」なので、何時間も気づけなかった。
//!
//! このテストは `build_app()` を呼ぶだけ。それだけで上の panic を
//! `cargo test` の段階で捕まえられる。
//!
//! ルートを足すときは、ここが通ることを確認すること。

#[cfg(test)]
mod tests {
    use axum::{
        body::{to_bytes, Body},
        http::{header, HeaderMap, Request, StatusCode},
    };
    use std::sync::Arc;
    use tower::ServiceExt;

    use crate::config::AppConfig;
    use crate::db::cache::AppCache;
    use crate::{build_app, AppState};

    fn assert_security_headers(headers: &HeaderMap) {
        let policy = headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
        assert!(policy.contains("default-src 'self'"));
        assert!(policy.contains("connect-src 'self'"));
        assert!(policy.contains("frame-ancestors 'self'"));
        // 外部 iframe は https のページだけ (Zoom Phone Smart Embed と、架電画面の中央で開くリンク)。
        // ワイルドカード・http: ・data: は許可しない
        assert!(policy.contains("frame-src 'self' https://applications.zoom.us https:;"));
        assert!(!policy.contains("frame-src *"));
        let frame_src = policy
            .split(';')
            .map(str::trim)
            .find(|d| d.starts_with("frame-src "))
            .expect("frame-src がある");
        assert_eq!(
            frame_src,
            "frame-src 'self' https://applications.zoom.us https:"
        );
        for loose in ["*", "http:", "data:", "blob:", "'unsafe-inline'"] {
            assert!(
                !frame_src.split_whitespace().any(|t| t == loose),
                "frame-src に {loose} を入れない"
            );
        }
        // 枠の中に開くのは外部ページだけ。この画面自体を外部から枠に入れることは引き続き禁止
        assert!(policy.contains("frame-ancestors 'self'"));
        assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(headers[header::X_FRAME_OPTIONS], "DENY");
        assert_eq!(
            headers[header::REFERRER_POLICY],
            "strict-origin-when-cross-origin"
        );
        assert_eq!(
            headers[header::STRICT_TRANSPORT_SECURITY],
            "max-age=31536000; includeSubDomains"
        );
    }

    /// Exercise the complete application stack without credentials or env changes.
    #[tokio::test]
    async fn job_copy_shell_requires_existing_login_in_full_app() {
        let response = build_app(minimal_state())
            .oneshot(
                Request::builder()
                    .uri("/app/job-copy")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers()[header::LOCATION], "/login");
        assert_security_headers(response.headers());
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        assert!(body.is_empty(), "Unauthenticated shell must expose no data");
    }

    /// Valid query extraction must still stop before MOC or Drive configuration.
    #[tokio::test]
    async fn job_copy_private_reads_require_login_in_full_app() {
        let app = build_app(minimal_state());
        for path in [
            "/api/job-copy/moc",
            "/api/job-copy/listing-status",
            "/api/job-copy/image?company_id=10&listing_id=30&manifest_id=40&slot=1",
        ] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
            assert!(!response.headers().contains_key(header::LOCATION));
            assert_security_headers(response.headers());
            let body = to_bytes(response.into_body(), 1024).await.unwrap();
            let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(json, serde_json::json!({"code": "login_required"}));
        }
    }

    /// DB も外部サービスも無い、最小の状態。

    /// DB も外部サービスも無い、最小の状態。
    /// ルーティングの組み立てだけを見るので、中身は空でよい。
    fn minimal_state() -> Arc<AppState> {
        Arc::new(AppState {
            // AppConfig に Default は無いので、全項目を空で埋める。
            // ルーティングの組み立てには設定値を使わない。
            config: AppConfig {
                port: 0,
                auth_password: String::new(),
                auth_password_hash: String::new(),
                external_passwords: Vec::new(),
                allowed_domains: Vec::new(),
                allowed_domains_extra: Vec::new(),
                hellowork_db_path: String::new(),
                indeed_db_path: String::new(),
                cache_ttl_secs: 60,
                cache_max_entries: 10,
                rate_limit_max_attempts: 5,
                rate_limit_lockout_secs: 60,
                audit_turso_url: String::new(),
                audit_turso_token: String::new(),
                audit_ip_salt: String::new(),
                admin_emails: Vec::new(),
                turso_external_url: String::new(),
                turso_external_token: String::new(),
                salesnow_turso_url: String::new(),
                salesnow_turso_token: String::new(),
                scout_turso_url: String::new(),
                scout_turso_token: String::new(),
            },
            hw_db: None,
            indeed_db: None,
            turso_db: None,
            salesnow_db: None,
            scout_db: None,
            cache: AppCache::new(60, 10),
            rate_limiter: crate::auth::session::RateLimiter::new(5, 60),
            company_geo_cache: None,
            audit: None,
            google_oidc: None,
            hubspot: None,
        })
    }

    /// 🔴 これが落ちたら、本番は起動できない。
    ///
    /// 「Overlapping method route」で落ちた場合は、同じパスを2箇所で
    /// 登録している。`src/lib.rs` の `build_app()` を見て、
    /// `.merge(...)` や `.route(...)` が重複していないか確認すること。
    /// マージのあとに起きやすい。
    #[test]
    fn ルーターを組み立てられる() {
        let _ = build_app(minimal_state());
    }

    /// 同じものを2回組み立てても壊れないこと。
    /// （静的な登録漏れだけでなく、組み立てに副作用が無いことも見る）
    #[test]
    fn ルーターを二度組み立てても壊れない() {
        let _ = build_app(minimal_state());
        let _ = build_app(minimal_state());
    }
}
