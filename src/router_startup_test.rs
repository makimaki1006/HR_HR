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
    use std::sync::Arc;

    use crate::config::AppConfig;
    use crate::db::cache::AppCache;
    use crate::{build_app, AppState};

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
