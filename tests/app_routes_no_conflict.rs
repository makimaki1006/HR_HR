//! `build_app()` を実際に呼んで、ルートの重複で起動できなくなることを防ぐ
//!
//! 2026-09-21 追加。
//!
//! ------------------------------------------------------------------
//! なぜ要るか
//! ------------------------------------------------------------------
//! **`cargo test` が全部通っても、サーバが起動しないことがある。**
//! axum は同じパスを2度登録すると `Router::merge` / `Router::route` の時点で
//! panic するが、それは `build_app()` を通らないと起きない。
//! 各ハンドラのユニットテストは自分の `router()` しか見ないので、
//! **別のモジュールと同じパスを取り合っていても気づけない**。
//!
//! 実害の記録: マージで同じルートが2行残り、本番が 21時間出なかった。
//!
//! ここでは本物の `build_app()` をそのまま呼ぶ。panic すればテストが落ちる。
//!
//! ------------------------------------------------------------------
//! AppState について
//! ------------------------------------------------------------------
//! DB も Turso も監査も `Option` なので、**全部 `None` で組める**。
//! ルートの重複はデータに関係なく `Router` の組み立てだけで決まるので、
//! 重い依存を用意する必要はない。

use std::sync::Arc;

use rust_dashboard::auth::session::RateLimiter;
use rust_dashboard::config::AppConfig;
use rust_dashboard::db::cache::AppCache;
use rust_dashboard::{build_app, AppState};

fn bare_state() -> Arc<AppState> {
    let config = AppConfig::from_env();
    let cache = AppCache::new(config.cache_ttl_secs, config.cache_max_entries);
    let rate_limiter = RateLimiter::new(
        config.rate_limit_max_attempts,
        config.rate_limit_lockout_secs,
    );
    Arc::new(AppState {
        config,
        hw_db: None,
        indeed_db: None,
        turso_db: None,
        salesnow_db: None,
        scout_db: None,
        cache,
        rate_limiter,
        company_geo_cache: None,
        audit: None,
    })
}

/// 同じパスを2つのルータが登録していたら、ここで panic して落ちる。
#[test]
fn build_appがルートの重複で落ちない() {
    let _router = build_app(bare_state());
}

/// コンサルダッシュボードのパスが、実際に `build_app()` の中に入っていること。
///
/// 重複していないことと、**そもそも配線されていること**は別。
/// `.merge()` を書き忘れても上のテストは通ってしまう。
///
/// 🔴 `protected_routes` は最後に `route_layer(auth_middleware)` を当てているので、
/// 未ログインだと 303（/login へのリダイレクト）になる。
/// **404 でなければ配線されている**、という見方をする。
#[tokio::test]
async fn コンサルダッシュボードのパスが配線されている() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let app = build_app(bare_state());
    for path in [
        "/consulting",
        "/api/consulting/focus",
        "/api/consulting/renewal",
        "/api/consulting/customer",
        "/api/consulting/headquarters",
        "/api/consulting/mtg-quality",
        "/api/consulting/phone",
        "/api/consulting/rampup",
        "/api/consulting/outcome",
        "/api/consulting/data-quality",
    ] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .body(Body::empty())
                    .expect("リクエストを組めない"),
            )
            .await
            .expect("ルータが応答しない");
        assert_ne!(
            res.status(),
            StatusCode::NOT_FOUND,
            "{path} が 404。build_app() に配線されていない\
（src/lib.rs の protected_routes に .merge() を足したか確認）"
        );
    }
}

/// 架電クオリティと営業KPI のパスも生きていること。
///
/// コンサルを足したせいで既存の画面が消えていないか（同じパスを取り合って
/// 片方が負けていないか）を見る。
#[tokio::test]
async fn 既存の画面のパスが消えていない() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let app = build_app(bare_state());
    for path in [
        "/call-quality",
        "/api/call-quality/tabs",
        "/api/call-quality/overview",
        "/sales-kpi",
        "/api/sales-kpi/data",
    ] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .body(Body::empty())
                    .expect("リクエストを組めない"),
            )
            .await
            .expect("ルータが応答しない");
        assert_ne!(
            res.status(),
            StatusCode::NOT_FOUND,
            "{path} が 404。コンサルを足したときに既存の画面を消していないか確認"
        );
    }
}

/// 🔴 コンサルダッシュボードが**認証の内側**に入っていること。
///
/// `protected_routes` に `.merge()` するのを間違えて外側に置くと、
/// **顧客ごとの契約金額が誰でも見られる**ことになる。
/// 未ログインで 303（`/login` へのリダイレクト）になることで確かめる。
/// 200 が返ったら、認証の外に出ている。
#[tokio::test]
async fn コンサルダッシュボードは認証の内側にある() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    let app = build_app(bare_state());
    for path in ["/consulting", "/api/consulting/focus", "/api/consulting/customer"] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .body(Body::empty())
                    .expect("リクエストを組めない"),
            )
            .await
            .expect("ルータが応答しない");
        assert_eq!(
            res.status(),
            StatusCode::SEE_OTHER,
            "{path} が {} を返した。未ログインなら 303 で /login へ飛ぶはず。200 なら認証の外に出ている（protected_routes の .route_layer より前に merge しているか確認）",
            res.status()
        );
    }
}

// ================================================================ 画面から辿れるか

/// 🔴 **ルートが生きていることと、画面から辿れることは別。**
///
/// 2026-09-21 の実害: `/consulting` は 303 を返していた（＝配線済み）のに、
/// **既存レイアウトにリンクが無かった**ので、画面を見ている人には
/// 存在しないのと同じだった。URL を直接叩けば出る、では使われない。
///
/// ここではテンプレートの中身を見る。レンダリングを通さないのは、
/// このナビが `{{JOBGEN_TAB}}` のような差し込みを含んでいて、
/// 組み立てに `AppState` の中身が要るため。**リンクが書かれているか**だけを見る。
#[test]
fn 既存の画面からコンサルへ行ける() {
    let nav = include_str!("../templates/dashboard_inline.html");
    assert!(
        nav.contains(r#"href="/consulting""#),
        "templates/dashboard_inline.html に /consulting へのリンクが無い。\
ルートが生きていても、画面から辿れなければ存在しないのと同じ"
    );
    // 先例が消えていないことも一緒に見る（並びごと消える事故を捕まえる）
    assert!(
        nav.contains(r#"href="/sales-kpi""#),
        "templates/dashboard_inline.html から /sales-kpi のリンクが消えている"
    );

    // 🔴 ラベルも固定する（2026-09-21 ユーザー指定「営業KPIの隣にコンサルKPI」）。
    //    リンクだけ見ていると、文言が英語や内部IDに変わっても気づけない。
    assert!(
        nav.contains("コンサルKPI"),
        "ナビのラベルが「コンサルKPI」でない。営業KPI の隣に並べる文言として指定されている"
    );
    assert!(
        nav.contains("営業KPI"),
        "ナビから「営業KPI」のラベルが消えている"
    );
}

/// 行きっぱなしで戻れないと使えない。
#[test]
fn コンサルから既存の画面へ戻れる() {
    let page = include_str!("../templates/tabs/cs_dashboard.html");
    assert!(
        page.contains(r#"href="/""#),
        "templates/tabs/cs_dashboard.html に戻り導線が無い。\
/sales-kpi と同じく「← ダッシュボードへ戻る」を置くこと"
    );
}

/// 🔴 画面の JS が壊れていると、**どのテストにも引っかからずに無言で死ぬ**。
///
/// 2026-09-21 の実害: テンプレートの JS に構文エラーが1つあり、`<script>` 全体が
/// 実行されなかった。API は1本も呼ばれず画面は空。それでも
///   - `cargo test` は 3,393 件すべて通る（Rust 側は無関係）
///   - `curl` で `/api/consulting/*` は 200（サーバは正常）
///   - `/consulting` も 200（HTML は出ている）
/// だったので、どの確認にも掛からなかった。
///
/// 原因は JS の文字列に `\\"` と書いたこと。JS では `\` が
/// バックスラッシュ1つになるので、次の `"` で文字列が終わってしまう。
///
/// ここでは**その1パターンだけ**を見る。構文そのものの検査は Rust からは
/// できないので、`tests/consulting_page_js.js`（Node）が担当する。
/// こちらは cargo test で毎回走る安い見張り。
#[test]
fn 画面のjsに文字列を壊すエスケープが無い() {
    for (name, src) in [
        ("templates/tabs/cs_dashboard.html", include_str!("../templates/tabs/cs_dashboard.html")),
    ] {
        let bad: Vec<(usize, &str)> = src
            .lines()
            .enumerate()
            .filter(|(_, l)| l.contains("\\\\\""))
            .map(|(i, l)| (i + 1, l.trim()))
            .collect();
        assert!(
            bad.is_empty(),
            "{name} の JS に `\\\\\"` がある。JS では文字列がそこで終わってしまい、\
<script> 全体が動かなくなる。HTML 属性を書きたいなら外側をシングルクォートにすること。\n{:?}",
            bad
        );
    }
}
