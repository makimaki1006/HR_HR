//! React 画面の HTML シェル配信口 (Phase 0-3, 2026-09-29)
//!
//! `GET /app/{screen}` が `<div id="app-root">` と、Vite の manifest から解決した
//! ハッシュ付き JS (`/static/app/assets/<name>-<hash>.js`) を読む最小の HTML を返す。
//! JS/CSS 本体は既存の `/static` 配信 (immutable キャッシュ) がそのまま返す。
//!
//! - manifest (`static/app/.vite/manifest.json`) は **起動時 (`build_app()`) に実行時読み込み**する。
//!   `include_str!` にすると `cargo test` が Node のビルド成果物に依存するため使わない。
//! - manifest が無い / 壊れている / 画面のエントリが無い場合は、200 で
//!   「フロントエンド未ビルド」の注記ページを返す (graceful degradation)。
//! - 画面名は英小文字とハイフンだけを受け付け、さらに `KNOWN_SCREENS` に載っているものだけを通す。
//!   それ以外は 404。manifest の参照キーは `src/entries/{screen}.tsx` で、
//!   画面名がファイルパスとして使われることはない (パス操作をさせない)。
//! - 認証は `protected_routes` 側の `route_layer(auth_middleware)` に任せる (未ログインは 303 /login)。

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use axum::extract::Path as UrlPath;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use super::helpers::escape_html;
use crate::AppState;

/// Vite `build.manifest: true` の出力先 (frontend/vite.config.ts の outDir = static/app)。
pub const MANIFEST_PATH: &str = "static/app/.vite/manifest.json";

/// Vite の `base` と同じ。manifest の `file` はこの下からの相対パス。
const ASSET_BASE: &str = "/static/app/";

/// React 化した画面の一覧。ここに無い名前は manifest の有無にかかわらず 404。
/// 追加するときは `frontend/src/entries/{screen}.tsx` と vite.config.ts の input も足す。
pub const KNOWN_SCREENS: &[&str] = &["dummy", "jobgen"];

/// 注記ページの見出し。テストと E2E が文言で判定する。
pub const NOT_BUILT_HEADING: &str = "フロントエンド未ビルド";

#[derive(Debug, Clone, Deserialize)]
struct ManifestEntry {
    file: String,
    #[serde(default)]
    css: Vec<String>,
    #[serde(default, rename = "isEntry")]
    is_entry: bool,
}

/// 起動時に読み込んだ Vite manifest。
#[derive(Debug, Clone)]
pub struct AppManifest {
    entries: HashMap<String, ManifestEntry>,
}

impl AppManifest {
    /// manifest JSON 文字列を解釈する。形式が違えば `Err`。
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        let entries: HashMap<String, ManifestEntry> = serde_json::from_str(json)?;
        Ok(Self { entries })
    }

    /// 画面名 → エントリ。manifest のキーは `src/entries/{screen}.tsx`。
    fn entry_for(&self, screen: &str) -> Option<&ManifestEntry> {
        self.entries
            .get(&format!("src/entries/{screen}.tsx"))
            .filter(|e| e.is_entry)
    }
}

/// manifest をファイルから読む。無い・読めない・壊れている場合は `None` (起動は止めない)。
pub fn load_manifest(path: &Path) -> Option<AppManifest> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(
                "React manifest を読めない ({}): {e}。/app/* は「{NOT_BUILT_HEADING}」の注記を返す",
                path.display()
            );
            return None;
        }
    };
    match AppManifest::parse(&text) {
        Ok(m) => {
            tracing::info!(
                "React manifest 読み込み: {} ({} エントリ)",
                path.display(),
                m.entries.len()
            );
            Some(m)
        }
        Err(e) => {
            tracing::warn!("React manifest の形式が不正 ({}): {e}", path.display());
            None
        }
    }
}

/// 英小文字で始まり、英小文字とハイフンだけから成る 1〜64 文字。
fn is_valid_screen_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.as_bytes()[0].is_ascii_lowercase()
        && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
}

/// `/app/{screen}` のルータ。`manifest` は起動時に読み込んだもの (テストでは任意に渡す)。
pub fn router(manifest: Option<AppManifest>) -> Router<Arc<AppState>> {
    let manifest = Arc::new(manifest);
    Router::new().route(
        "/app/{screen}",
        get(move |UrlPath(screen): UrlPath<String>| {
            let manifest = Arc::clone(&manifest);
            async move { render_screen(manifest.as_ref().as_ref(), &screen) }
        }),
    )
}

fn render_screen(manifest: Option<&AppManifest>, screen: &str) -> Response {
    if !is_valid_screen_name(screen) || !KNOWN_SCREENS.contains(&screen) {
        return (StatusCode::NOT_FOUND, "Not Found").into_response();
    }
    match manifest.and_then(|m| m.entry_for(screen)) {
        Some(entry) => Html(shell_html(entry)).into_response(),
        None => Html(not_built_html(screen)).into_response(),
    }
}

fn shell_html(entry: &ManifestEntry) -> String {
    let css_links: String = entry
        .css
        .iter()
        .map(|c| {
            format!(
                "<link rel=\"stylesheet\" href=\"{ASSET_BASE}{}\">\n",
                escape_html(c)
            )
        })
        .collect();
    format!(
        "<!DOCTYPE html>\n<html lang=\"ja\">\n<head>\n<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<title>HR_HR</title>\n{css_links}\
<script type=\"module\" src=\"{ASSET_BASE}{}\"></script>\n\
</head>\n<body>\n<div id=\"app-root\"></div>\n</body>\n</html>\n",
        escape_html(&entry.file)
    )
}

fn not_built_html(screen: &str) -> String {
    format!(
        "<!DOCTYPE html>\n<html lang=\"ja\">\n<head>\n<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<title>HR_HR</title>\n</head>\n<body>\n<main>\n<h1>{NOT_BUILT_HEADING}</h1>\n\
<p>画面「{}」のフロントエンド (React) がビルドされていないため表示できません。</p>\n\
<p>開発環境では <code>cd frontend &amp;&amp; npm run build</code> を実行してからサーバを再起動してください。</p>\n\
<p><a href=\"/\">ダッシュボードへ戻る</a></p>\n</main>\n</body>\n</html>\n",
        escape_html(screen)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tower::ServiceExt;

    const SAMPLE_MANIFEST: &str = r#"{
  "src/entries/dummy.tsx": {
    "file": "assets/dummy-AbC123xy.js",
    "name": "dummy",
    "src": "src/entries/dummy.tsx",
    "isEntry": true
  }
}"#;

    /// 認証層を外した `/app/*` ルータ。`with_state` には AppState が要るが、
    /// このハンドラは State を読まないので `router()` の型だけ合わせて Router<()> にする。
    fn app(manifest: Option<AppManifest>) -> Router {
        Router::new().merge(router(manifest).with_state(test_state()))
    }

    fn test_state() -> Arc<AppState> {
        let config = crate::config::AppConfig::from_env();
        let cache = crate::db::cache::AppCache::new(60, 10);
        let rate_limiter = crate::auth::session::RateLimiter::new(5, 60);
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
            google_oidc: None,
        })
    }

    async fn get_path(app: Router, uri: &str) -> (StatusCode, String) {
        let res = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let body = to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
        (status, String::from_utf8(body.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn manifestありならハッシュ付きjsが1本だけ入る() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        std::fs::write(&path, SAMPLE_MANIFEST).unwrap();
        let manifest = load_manifest(&path).expect("一時 manifest を読めない");

        let (status, body) = get_path(app(Some(manifest)), "/app/dummy").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains(r#"<div id="app-root"></div>"#), "{body}");
        assert!(
            body.contains(
                r#"<script type="module" src="/static/app/assets/dummy-AbC123xy.js"></script>"#
            ),
            "{body}"
        );
        assert_eq!(body.matches("/static/app/assets/").count(), 1, "{body}");
        assert_eq!(body.matches("<script").count(), 1, "{body}");
        assert!(!body.contains(NOT_BUILT_HEADING));
    }

    #[tokio::test]
    async fn manifestのcssはlinkで入る() {
        let json = r#"{"src/entries/dummy.tsx":{"file":"assets/dummy-A1.js","isEntry":true,"css":["assets/dummy-B2.css"]}}"#;
        let (status, body) =
            get_path(app(Some(AppManifest::parse(json).unwrap())), "/app/dummy").await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            body.contains(r#"<link rel="stylesheet" href="/static/app/assets/dummy-B2.css">"#),
            "{body}"
        );
    }

    #[tokio::test]
    async fn manifestなしなら200で未ビルドの注記() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("no_such_manifest.json");
        assert!(load_manifest(&missing).is_none());

        let (status, body) = get_path(app(None), "/app/dummy").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("<h1>フロントエンド未ビルド</h1>"), "{body}");
        assert!(!body.contains("app-root"));
        assert!(!body.contains("<script"));
    }

    #[tokio::test]
    async fn 壊れたmanifestやエントリ欠落も注記になる() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert!(load_manifest(&path).is_none());

        // manifest はあるが dummy のエントリが無い (古いビルド等)
        let other = AppManifest::parse(
            r#"{"src/entries/other.tsx":{"file":"assets/other-X.js","isEntry":true}}"#,
        )
        .unwrap();
        let (status, body) = get_path(app(Some(other)), "/app/dummy").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains(NOT_BUILT_HEADING), "{body}");
    }

    #[tokio::test]
    async fn 未知や不正な画面名は404() {
        let manifest = AppManifest::parse(SAMPLE_MANIFEST).unwrap();
        for uri in [
            "/app/unknown",   // 形式は正しいが KNOWN_SCREENS に無い
            "/app/Dummy",     // 大文字
            "/app/..%2Fx",    // デコード後 "../x"
            "/app/../x",      // セグメント数が合わずルートに当たらない
            "/app/dummy.tsx", // ドット
            "/app/-dummy",    // 先頭ハイフン
            "/app/",          // 空
            "/app",           // 画面名なし
        ] {
            let (status, body) = get_path(app(Some(manifest.clone())), uri).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{uri} -> {status} {body}");
            assert!(!body.contains("/static/app/assets/"), "{uri}");
        }
        // manifest が無くても、未知の画面名は注記ではなく 404
        let (status, _) = get_path(app(None), "/app/unknown").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[test]
    fn 画面名の検査() {
        assert!(is_valid_screen_name("dummy"));
        assert!(is_valid_screen_name("recruitment-diag"));
        for bad in ["", "Dummy", "../x", "a/b", "a.b", "-a", "a_b", "a1", "ａ"] {
            assert!(!is_valid_screen_name(bad), "{bad:?}");
        }
        assert!(!is_valid_screen_name(&"a".repeat(65)));
    }

    #[test]
    fn manifestのファイル名はエスケープされる() {
        let entry = ManifestEntry {
            file: "assets/x\"><script>alert(1)</script>.js".into(),
            css: vec![],
            is_entry: true,
        };
        let html = shell_html(&entry);
        assert_eq!(html.matches("<script").count(), 1, "{html}");
        assert!(html.contains("&quot;&gt;&lt;script&gt;"), "{html}");
    }
}
