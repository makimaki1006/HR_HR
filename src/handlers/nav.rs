//! ナビ定義 (旧シェル `templates/dashboard_inline.html` と React シェルの共通の元、2026-09-30)
//!
//! - `NAV_DEFS` が唯一の定義。旧シェルは `render_legacy_nav()` でこの定義から
//!   `<button hx-get>` / `<a href>` を組み立てて `{{NAV_TOP_ITEMS}}` / `{{NAV_EXPLORE_ITEMS}}` に
//!   差し込み、React シェルは `GET /api/nav` (`NavResponse`) を読む。
//! - 非表示の画面は削除せず `hidden` で隠す (ユーザー決定 2026-09-29、計画書 §6-3)。
//!   `hidden` の項目は `/api/nav` の `items` には入る (React は hidden=false だけ描く) が、
//!   旧シェルの HTML には出ない。ルートは残っているので URL 直アクセスは従来どおり動く。
//!   **`hidden: None` に戻せば旧新両方のナビに再表示される** (`tests::hidden切替は旧新両方に効く`)。
//! - React 画面に置き換わった画面は `kind: NavKind::App` + `target: "/app/xxx"` に変えるだけで、
//!   旧シェルは `<a href="/app/xxx">`、React は `href: "/app/xxx"` になる。
//! - `KEYWORDS_TAB` / `JOBGEN_TAB` の環境変数による出し分けは `requires` で表す。
//!   条件を満たさない項目は `items` から除外する (hidden とは別物)。
//! - 応答型は ts-rs で `frontend/src/generated/` に書き出す (`app_api::tests::export_ts_bindings`)。
//!   React 側は先にこの形で実装済みなので **フィールドの変更・削除はしない** (追加のみ可)。

use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use tower_sessions::Session;
use ts_rs::TS;

use super::helpers::escape_html;
use crate::auth::SESSION_USER_KEY;
use crate::config::AppConfig;
use crate::AppState;

/// ナビ項目の種類。`legacy_tab` は旧シェルの HTMX タブ (`/tab/*` partial)、
/// `page` は独立ページ (`/sales-kpi` 等)、`app` は React 画面 (`/app/*`)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum NavKind {
    LegacyTab,
    Page,
    App,
}

/// `/api/nav` の 1 項目。React 側 `frontend/src/shell/types.ts` の `NavItem` と同じ形。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct NavItem {
    pub id: String,
    pub label: String,
    pub title: Option<String>,
    pub kind: NavKind,
    /// `legacy_tab` は旧シェルの `?tab=` 復元形 (`/?tab=/tab/xxx`)、`page` / `app` はそのままの URL。
    pub href: String,
    /// サブナビのグループ id (`groups[].id`)。トップ行の項目は `None`。
    pub group: Option<String>,
    /// 非表示 (ナビに出さない。URL 直アクセスは可)。
    pub hidden: bool,
    /// 隠した理由 (hidden=true のときだけ)。
    pub hidden_reason: Option<String>,
    /// 隠した日付 `YYYY-MM-DD` (hidden=true のときだけ)。
    pub hidden_since: Option<String>,
}

/// サブナビのグループ (「調べる」)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct NavGroup {
    pub id: String,
    pub label: String,
}

/// `GET /api/nav` の応答。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct NavResponse {
    pub user_email: String,
    pub is_admin: bool,
    /// ヘッダー右側: ガイド (?) / 管理 (admin のみ) / 設定 / ログアウト。
    pub header_links: Vec<NavItem>,
    /// 旧ナビの表示順。hidden の項目も含む (React は hidden=false だけ描く)。
    pub items: Vec<NavItem>,
    pub groups: Vec<NavGroup>,
}

/// 環境変数で出し分ける機能。条件を満たさない項目は `items` に入らない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    /// Google Ads 資格情報がそろっている (`media_engine::handlers::media_engine_enabled`)。
    KeywordTools,
    /// `GEMINI_API_KEY` がある (`media_engine::config::gemini_api_key`)。
    JobgenTools,
    /// CRM (`/app/crm`)。今は管理者だけ (`crm_visible`)。
    Crm,
}

/// 環境変数の判定結果。ハンドラは `from_env()`、テストは任意の値を渡す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NavFeatures {
    pub keyword_tools: bool,
    pub jobgen_tools: bool,
    /// CRM を出すか。環境変数ではなく利用者の役割で決まる (`crm_visible`)。`from_env()` では false。
    pub crm: bool,
}

impl NavFeatures {
    /// 旧 `dashboard_page` の `{{KEYWORDS_TAB}}` / `{{JOBGEN_TAB}}` と同じ判定。
    pub fn from_env() -> Self {
        Self {
            keyword_tools: crate::media_engine::handlers::media_engine_enabled(),
            jobgen_tools: !crate::media_engine::config::gemini_api_key().is_empty(),
            crm: false,
        }
    }

    fn has(&self, f: Feature) -> bool {
        match f {
            Feature::KeywordTools => self.keyword_tools,
            Feature::JobgenTools => self.jobgen_tools,
            Feature::Crm => self.crm,
        }
    }
}

/// CRM をナビに出す条件 (1 箇所)。**役割が決まったら差し替える**。
/// 今は管理者 (`is_admin`) だけに出す。さらに `/app/crm` が `KNOWN_SCREENS` に登録されるまでは
/// 出さない (未登録のうちは 404 のリンクになるため。crm-team の画面 PR とマージ順を問わない)。
pub fn crm_visible(is_admin: bool) -> bool {
    is_admin && crm_screen_registered()
}

/// `/app/crm` が React 画面として公開済みか (`spa_shell::KNOWN_SCREENS`)。
pub fn crm_screen_registered() -> bool {
    super::spa_shell::KNOWN_SCREENS.contains(&"crm")
}

/// 隠した理由と日付。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hidden {
    pub reason: &'static str,
    /// 隠した日付。記録が無いものは推測で埋めず `None`。
    pub since: Option<&'static str>,
}

/// ナビ定義の 1 行。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavDef {
    pub id: &'static str,
    pub label: &'static str,
    pub title: Option<&'static str>,
    pub kind: NavKind,
    /// `LegacyTab` は partial のパス (`/tab/xxx`、旧シェルの `hx-get`)。`Page` / `App` は URL そのもの。
    pub target: &'static str,
    pub group: Option<&'static str>,
    pub requires: Option<Feature>,
    pub hidden: Option<Hidden>,
}

/// 「調べる」グループ。旧シェルの `#explore-group-btn` / `#explore-subnav` と対応する。
pub const EXPLORE_GROUP: &str = "explore";

/// サブナビのグループ一覧。
pub const NAV_GROUPS: &[(&str, &str)] = &[(EXPLORE_GROUP, "調べる")];

/// 旧シェルが最初に開くタブ (`dashboard_inline.html` の JS `DEFAULT_TAB` と同じ値)。
/// 旧シェルの HTML ではこの項目に `active` / `aria-selected="true"` を付けて出す。
pub const DEFAULT_LEGACY_TAB: &str = "/tab/survey";

const HIDDEN_2026_05_15: Hidden = Hidden {
    reason: "2026-05-15 の整理で未使用タブとして UI から非表示 (handler/route は温存、URL 直アクセスは可)",
    since: Some("2026-05-15"),
};

const HIDDEN_DEAD_ROUTE: Hidden = Hidden {
    reason: "旧 templates/dashboard.html (V1 遺物) 専用のタブで、現行シェルにはリンクが無い (CLAUDE.md §3.2 dead route)",
    since: None,
};

/// ナビ定義 (表示順)。hidden の項目は表示される項目の後ろに、隠した順で並べる。
pub const NAV_DEFS: &[NavDef] = &[
    // ---- トップ行 ----
    NavDef {
        id: "survey",
        label: "媒体分析",
        title: None,
        kind: NavKind::LegacyTab,
        target: "/tab/survey",
        group: None,
        requires: None,
        hidden: None,
    },
    NavDef {
        id: "competitor",
        label: "競合調査",
        title: Some("Excel競合調査・検索需要・Indeed採用市場・人口統計"),
        kind: NavKind::Page,
        target: "/competitor",
        group: None,
        requires: None,
        hidden: None,
    },
    NavDef {
        id: "keyword-tools",
        label: "キーワード需要",
        title: Some("検索キーワードの需要をアプリ内で確認"),
        kind: NavKind::LegacyTab,
        target: "/tab/keyword_tools",
        group: None,
        requires: Some(Feature::KeywordTools),
        hidden: None,
    },
    NavDef {
        id: "jobgen-tools",
        label: "求人票作成",
        title: Some("求人票生成・競合比較・応募者ジャーニー診断"),
        kind: NavKind::LegacyTab,
        target: "/tab/jobgen_tools",
        group: None,
        requires: Some(Feature::JobgenTools),
        hidden: None,
    },
    // ---- 「調べる」サブナビ (2026-08-10 に 1 グループへ集約) ----
    NavDef {
        id: "jobmap",
        label: "地図",
        title: None,
        kind: NavKind::LegacyTab,
        target: "/tab/jobmap",
        group: Some(EXPLORE_GROUP),
        requires: None,
        hidden: None,
    },
    NavDef {
        id: "regional-analysis",
        label: "地域分析",
        title: None,
        kind: NavKind::LegacyTab,
        target: "/tab/regional_analysis",
        group: Some(EXPLORE_GROUP),
        requires: None,
        hidden: None,
    },
    NavDef {
        id: "company",
        label: "企業検索",
        title: None,
        kind: NavKind::LegacyTab,
        target: "/tab/company",
        group: Some(EXPLORE_GROUP),
        requires: None,
        hidden: None,
    },
    NavDef {
        id: "driver",
        label: "職種辞典",
        title: None,
        kind: NavKind::LegacyTab,
        target: "/tab/driver",
        group: Some(EXPLORE_GROUP),
        requires: None,
        hidden: None,
    },
    NavDef {
        id: "license",
        label: "資格辞書",
        title: None,
        kind: NavKind::LegacyTab,
        target: "/tab/license",
        group: Some(EXPLORE_GROUP),
        requires: None,
        hidden: None,
    },
    NavDef {
        id: "indeed",
        label: "採用市場",
        title: Some("求人媒体の掲載データから見た、職種別・地域別の採用市場の動き"),
        kind: NavKind::LegacyTab,
        target: "/tab/indeed",
        group: Some(EXPLORE_GROUP),
        requires: None,
        hidden: None,
    },
    // ---- 独立ページ (中身がフル HTML なので HTMX で swap せず <a> で移動する) ----
    NavDef {
        id: "sales-kpi",
        label: "営業KPI",
        title: Some("今月の商談と架電。営業の現場が毎朝見る画面"),
        kind: NavKind::Page,
        target: "/sales-kpi",
        group: None,
        requires: None,
        hidden: None,
    },
    NavDef {
        id: "consulting",
        label: "コンサルKPI",
        title: Some("契約の継続と成果、いま手を打つべき顧客。コンサルが見る画面"),
        kind: NavKind::Page,
        target: "/consulting",
        group: None,
        requires: None,
        hidden: None,
    },
    // CRM (React 画面 /app/crm)。`crm_visible` を満たすときだけ items に入る (今は管理者のみ)。
    NavDef {
        id: "crm",
        label: "CRM",
        title: None,
        kind: NavKind::App,
        target: "/app/crm",
        group: None,
        requires: Some(Feature::Crm),
        hidden: None,
    },
    // Reading private data still requires Google OIDC and JOB_COPY_ALLOWED_EMAILS.
    NavDef {
        id: "job-copy",
        label: "求人文面（MOC）",
        title: Some("求人本文・画像の履歴、差分、応募構成を確認"),
        kind: NavKind::App,
        target: "/app/job-copy",
        group: None,
        requires: None,
        hidden: None,
    },
    // ---- 非表示 (削除しない。hidden を None にすれば旧新両方のナビに戻る) ----
    // 2026-05-15 に UI から外した 8 タブ (旧 dashboard_inline.html:122-127 のコメント)
    NavDef {
        id: "market",
        label: "市場概況",
        title: None,
        kind: NavKind::LegacyTab,
        target: "/tab/market",
        group: None,
        requires: None,
        hidden: Some(HIDDEN_2026_05_15),
    },
    NavDef {
        id: "region-karte",
        label: "地域カルテ",
        title: None,
        kind: NavKind::LegacyTab,
        target: "/tab/region_karte",
        group: None,
        requires: None,
        hidden: Some(HIDDEN_2026_05_15),
    },
    NavDef {
        id: "analysis",
        label: "詳細分析",
        title: None,
        kind: NavKind::LegacyTab,
        target: "/tab/analysis",
        group: None,
        requires: None,
        hidden: Some(HIDDEN_2026_05_15),
    },
    NavDef {
        id: "insight",
        label: "総合診断",
        title: None,
        kind: NavKind::LegacyTab,
        target: "/tab/insight",
        group: None,
        requires: None,
        hidden: Some(HIDDEN_2026_05_15),
    },
    NavDef {
        id: "trend",
        label: "トレンド",
        title: None,
        kind: NavKind::LegacyTab,
        target: "/tab/trend",
        group: None,
        requires: None,
        hidden: Some(HIDDEN_2026_05_15),
    },
    NavDef {
        id: "comparison",
        label: "都道府県比較",
        title: None,
        kind: NavKind::LegacyTab,
        target: "/tab/comparison",
        group: None,
        requires: None,
        hidden: Some(HIDDEN_2026_05_15),
    },
    NavDef {
        id: "diagnostic",
        label: "条件診断",
        title: None,
        kind: NavKind::LegacyTab,
        target: "/tab/diagnostic",
        group: None,
        requires: None,
        hidden: Some(HIDDEN_2026_05_15),
    },
    NavDef {
        id: "recruitment-diag",
        label: "採用診断",
        title: None,
        kind: NavKind::App,
        target: "/app/recruitment-diag",
        group: None,
        requires: None,
        hidden: Some(HIDDEN_2026_05_15),
    },
    // 2026-07-28 のタブ再編で UI から外した求人検索
    NavDef {
        id: "competitive",
        label: "求人検索",
        title: None,
        kind: NavKind::LegacyTab,
        target: "/tab/competitive",
        group: None,
        requires: None,
        hidden: Some(Hidden {
            reason: "2026-07-28 のタブ再編 (ユーザー指定順) で UI 非表示 (handler/URL 直アクセスは温存)",
            since: Some("2026-07-28"),
        }),
    },
    // dead route 4 (CLAUDE.md §3.2)。ラベルは各ハンドラの見出し
    NavDef {
        id: "overview",
        label: "地域概況",
        title: None,
        kind: NavKind::LegacyTab,
        target: "/tab/overview",
        group: None,
        requires: None,
        hidden: Some(HIDDEN_DEAD_ROUTE),
    },
    NavDef {
        id: "demographics",
        label: "採用動向",
        title: None,
        kind: NavKind::LegacyTab,
        target: "/tab/demographics",
        group: None,
        requires: None,
        hidden: Some(HIDDEN_DEAD_ROUTE),
    },
    NavDef {
        id: "balance",
        label: "企業分析",
        title: None,
        kind: NavKind::LegacyTab,
        target: "/tab/balance",
        group: None,
        requires: None,
        hidden: Some(HIDDEN_DEAD_ROUTE),
    },
    NavDef {
        id: "workstyle",
        label: "求人条件",
        title: None,
        kind: NavKind::LegacyTab,
        target: "/tab/workstyle",
        group: None,
        requires: None,
        hidden: Some(HIDDEN_DEAD_ROUTE),
    },
    // 採用提案パッケージの試作モック (2026-07-25 追加、2026-08-11 のナビ集約で外れた)
    NavDef {
        id: "proposal-mock",
        label: "提案パッケージ試作",
        title: Some("採用提案パッケージの試作モック (全数値ダミー)"),
        kind: NavKind::Page,
        target: "/proposal-mock",
        group: None,
        requires: None,
        hidden: Some(Hidden {
            reason: "2026-08-11 のナビ集約 (4 タブ化) でリンクを外した試作モック。全数値ダミー",
            since: Some("2026-08-11"),
        }),
    },
    // 架電クオリティ (「まだ見えなくてよい」ユーザー判断 2026-09-07)
    NavDef {
        id: "call-quality",
        label: "架電クオリティ",
        title: Some("架電の記録と品質。GAS 版ダッシュボードの移植先"),
        kind: NavKind::Page,
        target: "/call-quality",
        group: None,
        requires: None,
        hidden: Some(Hidden {
            reason: "「まだ見えなくてよい」というユーザー判断 (2026-09-07)。ページは残してあり URL 直アクセスは可",
            since: Some("2026-09-07"),
        }),
    },
];

/// `legacy_tab` の href。旧シェルが `URLSearchParams.get('tab')` で復号する `?tab=` 復元形。
/// (`/tab/xxx` にクエリを含まない前提。含む場合は `dict_cards::tab_url` のように encode する)
pub fn legacy_tab_href(target: &str) -> String {
    format!("/?tab={target}")
}

fn def_to_item(d: &NavDef) -> NavItem {
    let href = match d.kind {
        NavKind::LegacyTab => legacy_tab_href(d.target),
        NavKind::Page | NavKind::App => d.target.to_string(),
    };
    NavItem {
        id: d.id.to_string(),
        label: d.label.to_string(),
        title: d.title.map(str::to_string),
        kind: d.kind,
        href,
        group: d.group.map(str::to_string),
        hidden: d.hidden.is_some(),
        hidden_reason: d.hidden.map(|h| h.reason.to_string()),
        hidden_since: d.hidden.and_then(|h| h.since.map(str::to_string)),
    }
}

/// 定義 → `/api/nav` の `items`。`requires` を満たさない項目は除外する。hidden は残す。
pub fn nav_items(defs: &[NavDef], features: &NavFeatures) -> Vec<NavItem> {
    defs.iter()
        .filter(|d| d.requires.is_none_or(|f| features.has(f)))
        .map(def_to_item)
        .collect()
}

/// `/api/nav` の `groups`。
pub fn nav_groups() -> Vec<NavGroup> {
    NAV_GROUPS
        .iter()
        .map(|(id, label)| NavGroup {
            id: (*id).to_string(),
            label: (*label).to_string(),
        })
        .collect()
}

/// 管理者判定 (旧 `dashboard_page` から移動)。config の admin_emails で判定する (DB 往復を避ける)。
/// `upsert_account` は「昇格のみ」なので config に載っている限り DB 側も admin になる。
/// 実際の入場ゲートは `require_admin_mw` (DB の role を見る) で、ここはリンクの出し分けだけに使う。
pub fn is_admin(config: &AppConfig, user_email: &str) -> bool {
    config
        .admin_emails
        .iter()
        .any(|a| a.eq_ignore_ascii_case(user_email))
}

/// ヘッダー右側のリンク。旧シェルの並び (? / 管理 / 設定 / ログアウト) と同じ。
pub fn header_links(is_admin: bool) -> Vec<NavItem> {
    let link = |id: &str, label: &str, title: Option<&str>, kind: NavKind, href: &str| NavItem {
        id: id.to_string(),
        label: label.to_string(),
        title: title.map(str::to_string),
        kind,
        href: href.to_string(),
        group: None,
        hidden: false,
        hidden_reason: None,
        hidden_since: None,
    };
    let mut v = vec![link(
        "guide",
        "?",
        Some("使い方ガイド"),
        NavKind::LegacyTab,
        &legacy_tab_href("/tab/guide"),
    )];
    if is_admin {
        v.push(link(
            "admin",
            "管理",
            Some("利用状況・ユーザー管理"),
            NavKind::Page,
            "/admin/usage",
        ));
    }
    v.push(link(
        "settings",
        "設定",
        Some("プロフィール編集"),
        NavKind::Page,
        "/my/profile",
    ));
    v.push(link("logout", "ログアウト", None, NavKind::Page, "/logout"));
    v
}

/// 旧シェルの `{{ADMIN_LINK}}`。`header_links` の admin 項目から作る (定義を二重に持たない)。
pub fn render_legacy_admin_link(is_admin: bool) -> String {
    header_links(is_admin)
        .iter()
        .find(|l| l.id == "admin")
        .map(|l| {
            format!(
                r#"<a href="{}" class="text-slate-400 hover:text-white text-sm transition" title="{}">{}</a>"#,
                escape_html(&l.href),
                escape_html(l.title.as_deref().unwrap_or("")),
                escape_html(&l.label)
            )
        })
        .unwrap_or_default()
}

/// `/api/nav` の応答を組み立てる (ハンドラとテストの共通部分)。
pub fn build_nav_response(
    user_email: String,
    is_admin: bool,
    features: &NavFeatures,
) -> NavResponse {
    let features = &NavFeatures {
        crm: crm_visible(is_admin),
        ..*features
    };
    NavResponse {
        user_email,
        is_admin,
        header_links: header_links(is_admin),
        items: nav_items(NAV_DEFS, features),
        groups: nav_groups(),
    }
}

/// 旧シェル (`dashboard_inline.html`) に差し込むナビ HTML。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LegacyNav {
    /// `<nav role="tablist">` の中身 (`{{NAV_TOP_ITEMS}}`)。
    pub top: String,
    /// `#explore-subnav` の中身 (`{{NAV_EXPLORE_ITEMS}}`)。
    pub explore: String,
}

fn title_attr(title: Option<&str>) -> String {
    match title {
        Some(t) => format!(r#" title="{}""#, escape_html(t)),
        None => String::new(),
    }
}

/// 1 項目分の旧シェル HTML。属性の並びは置き換え前のテンプレートと同じにしてある
/// (`tests::旧シェルのナビは置き換え前のテンプレートと同じ要素列になる` で逆証明)。
fn render_legacy_item(item: &NavItem, active: bool) -> String {
    match item.kind {
        NavKind::LegacyTab => {
            let target = item
                .href
                .strip_prefix("/?tab=")
                .unwrap_or(item.href.as_str());
            format!(
                "        <button class=\"tab-btn{}\" role=\"tab\" aria-selected=\"{}\" hx-get=\"{}\" hx-target=\"#content\" hx-swap=\"innerHTML\"\n            onclick=\"setActiveTab(this)\"{}>{}</button>\n",
                if active { " active" } else { "" },
                if active { "true" } else { "false" },
                escape_html(target),
                title_attr(item.title.as_deref()),
                escape_html(&item.label)
            )
        }
        NavKind::Page | NavKind::App => format!(
            "        <a class=\"tab-btn tab-link\" href=\"{}\"{}>{}</a>\n",
            escape_html(&item.href),
            title_attr(item.title.as_deref()),
            escape_html(&item.label)
        ),
    }
}

/// `items` (hidden 込み) から旧シェルのナビ HTML を組み立てる。
///
/// - hidden の項目は出さない。
/// - グループの項目は、最初の項目の位置にグループの開閉ボタンを 1 つ出し、中身は `explore` 側に出す。
///   旧シェルの JS (`toggleExploreGroup` / `syncExploreGroup`) は `#explore-group-btn` と
///   `#explore-subnav` だけを見るので、対応しているグループは `EXPLORE_GROUP` の 1 つ。
/// - 「調べる」グループ後の最初の `page` / `app` の前に区切りを出す。
///   グループ前の競合調査は媒体分析に隣接させる。
/// - `DEFAULT_LEGACY_TAB` の項目に `active` を付ける (JS の `DEFAULT_TAB` と同じ画面)。
pub fn render_legacy_nav(items: &[NavItem]) -> LegacyNav {
    let mut top = String::new();
    let mut explore = String::new();
    let mut group_emitted = false;
    let mut sep_emitted = false;
    for item in items.iter().filter(|i| !i.hidden) {
        match item.group.as_deref() {
            Some(g) if g == EXPLORE_GROUP => {
                if !group_emitted {
                    group_emitted = true;
                    let label = NAV_GROUPS
                        .iter()
                        .find(|(id, _)| *id == EXPLORE_GROUP)
                        .map(|(_, l)| *l)
                        .unwrap_or(EXPLORE_GROUP);
                    top.push_str(&format!(
                        "        <button type=\"button\" id=\"explore-group-btn\" class=\"tab-btn-group\" aria-expanded=\"false\"\n            aria-controls=\"explore-subnav\" onclick=\"toggleExploreGroup()\">{} ▾</button>\n",
                        escape_html(label)
                    ));
                }
                explore.push_str(&render_legacy_item(item, false));
            }
            Some(other) => {
                // 旧シェルが知らないグループ。項目を失わないようトップ行に出す
                tracing::warn!("旧シェルは未対応のナビグループ {other:?} をトップ行に出す");
                top.push_str(&render_legacy_item(item, false));
            }
            None => {
                if group_emitted
                    && matches!(item.kind, NavKind::Page | NavKind::App)
                    && !sep_emitted
                {
                    sep_emitted = true;
                    top.push_str("        <span class=\"tab-sep\" aria-hidden=\"true\"></span>\n");
                }
                let active = item.kind == NavKind::LegacyTab
                    && item.href == legacy_tab_href(DEFAULT_LEGACY_TAB);
                top.push_str(&render_legacy_item(item, active));
            }
        }
    }
    LegacyNav { top, explore }
}

/// `GET /api/nav`。要ログイン (`protected_routes` の `auth_middleware` 配下)。
pub async fn api_nav(State(state): State<Arc<AppState>>, session: Session) -> Json<NavResponse> {
    let user_email: String = session
        .get(SESSION_USER_KEY)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string());
    let admin = is_admin(&state.config, &user_email);
    Json(build_nav_response(
        user_email,
        admin,
        &NavFeatures::from_env(),
    ))
}

/// `/api/nav` のルータ。`protected_routes` に merge する。
pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/nav", get(api_nav))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn features(keyword_tools: bool, jobgen_tools: bool) -> NavFeatures {
        NavFeatures {
            keyword_tools,
            jobgen_tools,
            crm: false,
        }
    }

    fn ids(items: &[NavItem]) -> Vec<&str> {
        items.iter().map(|i| i.id.as_str()).collect()
    }

    // ---------------------------------------------------------------- 定義の中身

    #[test]
    fn itemsは旧ナビの表示順でhrefが復元形になる() {
        let items = nav_items(NAV_DEFS, &features(true, true));
        let visible: Vec<(&str, &str, Option<&str>)> = items
            .iter()
            .filter(|i| !i.hidden)
            .map(|i| (i.id.as_str(), i.href.as_str(), i.group.as_deref()))
            .collect();
        assert_eq!(
            visible,
            vec![
                ("survey", "/?tab=/tab/survey", None),
                ("competitor", "/competitor", None),
                ("keyword-tools", "/?tab=/tab/keyword_tools", None),
                ("jobgen-tools", "/?tab=/tab/jobgen_tools", None),
                ("jobmap", "/?tab=/tab/jobmap", Some("explore")),
                (
                    "regional-analysis",
                    "/?tab=/tab/regional_analysis",
                    Some("explore")
                ),
                ("company", "/?tab=/tab/company", Some("explore")),
                ("driver", "/?tab=/tab/driver", Some("explore")),
                ("license", "/?tab=/tab/license", Some("explore")),
                ("indeed", "/?tab=/tab/indeed", Some("explore")),
                ("sales-kpi", "/sales-kpi", None),
                ("consulting", "/consulting", None),
                ("job-copy", "/app/job-copy", None),
            ]
        );
        let survey = &items[0];
        assert_eq!(survey.kind, NavKind::LegacyTab);
        assert_eq!(survey.label, "媒体分析");
        assert_eq!(survey.title, None);
        assert!(!survey.hidden);
        assert_eq!(survey.hidden_reason, None);
        assert_eq!(survey.hidden_since, None);
        let consulting = items.iter().find(|i| i.id == "consulting").unwrap();
        assert_eq!(consulting.kind, NavKind::Page);
        assert_eq!(
            consulting.title.as_deref(),
            Some("契約の継続と成果、いま手を打つべき顧客。コンサルが見る画面")
        );
        assert_eq!(
            nav_groups(),
            vec![NavGroup {
                id: "explore".into(),
                label: "調べる".into()
            }]
        );
    }

    #[test]
    fn 隠し対象は全部hiddenで理由と日付を持つ() {
        let items = nav_items(NAV_DEFS, &features(true, true));
        let hidden: Vec<(&str, &str, Option<&str>)> = items
            .iter()
            .filter(|i| i.hidden)
            .map(|i| (i.id.as_str(), i.href.as_str(), i.hidden_since.as_deref()))
            .collect();
        assert_eq!(
            hidden,
            vec![
                ("market", "/?tab=/tab/market", Some("2026-05-15")),
                (
                    "region-karte",
                    "/?tab=/tab/region_karte",
                    Some("2026-05-15")
                ),
                ("analysis", "/?tab=/tab/analysis", Some("2026-05-15")),
                ("insight", "/?tab=/tab/insight", Some("2026-05-15")),
                ("trend", "/?tab=/tab/trend", Some("2026-05-15")),
                ("comparison", "/?tab=/tab/comparison", Some("2026-05-15")),
                ("diagnostic", "/?tab=/tab/diagnostic", Some("2026-05-15")),
                (
                    "recruitment-diag",
                    "/app/recruitment-diag",
                    Some("2026-05-15")
                ),
                ("competitive", "/?tab=/tab/competitive", Some("2026-07-28")),
                ("overview", "/?tab=/tab/overview", None),
                ("demographics", "/?tab=/tab/demographics", None),
                ("balance", "/?tab=/tab/balance", None),
                ("workstyle", "/?tab=/tab/workstyle", None),
                ("proposal-mock", "/proposal-mock", Some("2026-08-11")),
                ("call-quality", "/call-quality", Some("2026-09-07")),
            ]
        );
        for i in items.iter().filter(|i| i.hidden) {
            let reason = i.hidden_reason.as_deref().unwrap_or("");
            assert!(!reason.is_empty(), "{}: hidden_reason が空", i.id);
            // 日付の記録が無い dead route だけ None (推測の日付は入れない)
            let Some(since) = i.hidden_since.as_deref() else {
                continue;
            };
            assert!(
                chrono::NaiveDate::parse_from_str(since, "%Y-%m-%d").is_ok(),
                "{}: hidden_since {since:?} が YYYY-MM-DD でない",
                i.id
            );
        }
        for i in items.iter().filter(|i| !i.hidden) {
            assert_eq!(i.hidden_reason, None, "{}", i.id);
            assert_eq!(i.hidden_since, None, "{}", i.id);
        }
        let call_quality = items.iter().find(|i| i.id == "call-quality").unwrap();
        assert!(
            call_quality
                .hidden_reason
                .as_deref()
                .unwrap()
                .contains("2026-09-07"),
            "{call_quality:?}"
        );
    }

    #[test]
    fn idは重複しない() {
        let mut seen = std::collections::HashSet::new();
        for d in NAV_DEFS {
            assert!(seen.insert(d.id), "id {} が重複", d.id);
        }
        for d in NAV_DEFS {
            if let Some(g) = d.group {
                assert!(
                    NAV_GROUPS.iter().any(|(id, _)| *id == g),
                    "{}: 未定義のグループ {g}",
                    d.id
                );
            }
            if d.kind == NavKind::LegacyTab {
                assert!(
                    d.target.starts_with("/tab/"),
                    "{}: legacy_tab の target は /tab/ で始まる",
                    d.id
                );
            }
        }
    }

    #[test]
    fn env出し分けはitemsから除外でhiddenとは別物() {
        let none = nav_items(NAV_DEFS, &features(false, false));
        assert!(!ids(&none).contains(&"keyword-tools"));
        assert!(!ids(&none).contains(&"jobgen-tools"));
        let kw = nav_items(NAV_DEFS, &features(true, false));
        assert!(ids(&kw).contains(&"keyword-tools"));
        assert!(!ids(&kw).contains(&"jobgen-tools"));
        let both = nav_items(NAV_DEFS, &features(true, true));
        assert_eq!(
            ids(&both)[..4],
            ["survey", "competitor", "keyword-tools", "jobgen-tools"]
        );
        assert_eq!(ids(&none)[..3], ["survey", "competitor", "jobmap"]);
        // 除外は hidden とは別: 除外された項目は items に存在しない (hidden=true で残るのではない)
        assert_eq!(both.len(), none.len() + 2);
        assert_eq!(
            both.iter().filter(|i| i.hidden).count(),
            none.iter().filter(|i| i.hidden).count()
        );
    }

    #[test]
    fn header_linksはadminのときだけ管理が入る() {
        let admin = header_links(true);
        assert_eq!(
            admin
                .iter()
                .map(|l| (l.id.as_str(), l.label.as_str(), l.href.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("guide", "?", "/?tab=/tab/guide"),
                ("admin", "管理", "/admin/usage"),
                ("settings", "設定", "/my/profile"),
                ("logout", "ログアウト", "/logout"),
            ]
        );
        assert_eq!(admin[1].title.as_deref(), Some("利用状況・ユーザー管理"));
        let user = header_links(false);
        assert_eq!(ids(&user), ["guide", "settings", "logout"]);
        assert!(user.iter().all(|l| !l.hidden));
    }

    #[test]
    fn 旧シェルのadmin_linkは置き換え前と同じ文字列() {
        assert_eq!(
            render_legacy_admin_link(true),
            r#"<a href="/admin/usage" class="text-slate-400 hover:text-white text-sm transition" title="利用状況・ユーザー管理">管理</a>"#
        );
        assert_eq!(render_legacy_admin_link(false), "");
    }

    #[test]
    fn is_adminは大文字小文字を区別しない() {
        let mut config = crate::config::AppConfig::from_env();
        config.admin_emails = vec!["Boss@f-a-c.co.jp".to_string()];
        assert!(is_admin(&config, "boss@f-a-c.co.jp"));
        assert!(!is_admin(&config, "staff@f-a-c.co.jp"));
        config.admin_emails.clear();
        assert!(!is_admin(&config, "boss@f-a-c.co.jp"));
    }

    #[test]
    fn build_nav_responseの形() {
        let r = build_nav_response("a@f-a-c.co.jp".into(), true, &features(false, true));
        assert_eq!(r.user_email, "a@f-a-c.co.jp");
        assert!(r.is_admin);
        assert!(r.header_links.iter().any(|l| l.id == "admin"));
        assert!(ids(&r.items).contains(&"jobgen-tools"));
        assert!(!ids(&r.items).contains(&"keyword-tools"));
        let v = serde_json::to_value(&r).unwrap();
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            ["user_email", "is_admin", "header_links", "items", "groups"]
        );
        let item_keys: Vec<&str> = v["items"][0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            item_keys,
            [
                "id",
                "label",
                "title",
                "kind",
                "href",
                "group",
                "hidden",
                "hidden_reason",
                "hidden_since"
            ]
        );
        assert_eq!(v["items"][0]["kind"], "legacy_tab");
        assert_eq!(v["items"][0]["title"], serde_json::Value::Null);
        let market = v["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == "market")
            .unwrap();
        assert_eq!(market["hidden"], true);
        assert_eq!(market["hidden_since"], "2026-05-15");
        let consulting = v["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == "consulting")
            .unwrap();
        assert_eq!(consulting["kind"], "page");
    }

    #[test]
    fn crmは管理者のときだけitemsに入る() {
        // 項目の形と位置は、CRM を出す features で直接確かめる
        let with_crm = nav_items(
            NAV_DEFS,
            &NavFeatures {
                crm: true,
                ..features(false, false)
            },
        );
        let crm = with_crm
            .iter()
            .find(|i| i.id == "crm")
            .expect("crm=true の items に crm が無い");
        assert_eq!(crm.label, "CRM");
        assert_eq!(crm.kind, NavKind::App);
        assert_eq!(crm.href, "/app/crm");
        assert!(!crm.hidden);
        assert_eq!(crm.group, None);
        // 位置: コンサルKPI の直後 (hidden 群の前)
        let pos = |items: &[NavItem], id: &str| items.iter().position(|i| i.id == id).unwrap();
        assert_eq!(pos(&with_crm, "crm"), pos(&with_crm, "consulting") + 1);

        // 実際に出すかは「管理者」かつ「/app/crm が KNOWN_SCREENS に登録済み」
        let admin = build_nav_response("a@f-a-c.co.jp".into(), true, &features(false, false));
        assert_eq!(
            ids(&admin.items).contains(&"crm"),
            crm_screen_registered(),
            "admin の CRM 表示は /app/crm の登録有無と一致する"
        );
        let user = build_nav_response("u@f-a-c.co.jp".into(), false, &features(false, false));
        assert!(!ids(&user.items).contains(&"crm"));
        // from_env() では crm は false (役割は build_nav_response が決める)
        assert!(!NavFeatures::from_env().crm);
        assert_eq!(crm_visible(true), crm_screen_registered());
        assert!(!crm_visible(false));
    }

    #[test]
    fn ts型の宣言はreact側のtypes_tsと同じフィールド() {
        let cfg = ts_rs::Config::default();
        let kind = NavKind::decl(&cfg);
        assert_eq!(
            kind, r#"type NavKind = "legacy_tab" | "page" | "app";"#,
            "{kind}"
        );
        let item = NavItem::decl(&cfg);
        for field in [
            "id: string,",
            "label: string,",
            "title: string | null,",
            "kind: NavKind,",
            "href: string,",
            "group: string | null,",
            "hidden: boolean,",
            "hidden_reason: string | null,",
            "hidden_since: string | null,",
        ] {
            assert!(item.contains(field), "{field} が無い: {item}");
        }
        let resp = NavResponse::decl(&cfg);
        for field in [
            "user_email: string,",
            "is_admin: boolean,",
            "header_links: Array<NavItem>,",
            "items: Array<NavItem>,",
            "groups: Array<NavGroup>,",
        ] {
            assert!(resp.contains(field), "{field} が無い: {resp}");
        }
    }

    // ---------------------------------------------------------------- 旧シェルの HTML

    /// 置き換え前 (HEAD c2b566a) の `templates/dashboard_inline.html:137-155`。
    /// `{{KEYWORDS_TAB}}` / `{{JOBGEN_TAB}}` は旧 `dashboard_page` が差し込んでいた文字列。
    const OLD_TOP: &str = r##"        <button class="tab-btn active" role="tab" aria-selected="true" hx-get="/tab/survey" hx-target="#content" hx-swap="innerHTML"
            onclick="setActiveTab(this)">媒体分析</button>
        {{KEYWORDS_TAB}}
        {{JOBGEN_TAB}}
        <button type="button" id="explore-group-btn" class="tab-btn-group" aria-expanded="false"
            aria-controls="explore-subnav" onclick="toggleExploreGroup()">調べる ▾</button>
        <!-- ここから先は #content に差し込むのではなく、独立したページへ移動する。
             中身がフルHTML（自前の <style> と <script> を持つ）なので、HTMX で
             swap すると <html> が二重になって壊れる。だから <a> にしてある。
             営業が毎朝見るものなので「調べる」の中には入れず、表に出しておく。

             🔴 架電クオリティ(/call-quality) はここに出さない。
             「まだ見えなくてよい」というユーザー判断（2026-09-07）。
             ページ自体は残してあるので、URL を直接叩けば見られる。 -->
        <span class="tab-sep" aria-hidden="true"></span>
        <a class="tab-btn tab-link" href="/sales-kpi"
           title="今月の商談と架電。営業の現場が毎朝見る画面">営業KPI</a>
        <a class="tab-btn tab-link" href="/consulting"
           title="契約の継続と成果、いま手を打つべき顧客。コンサルが見る画面">コンサルKPI</a>
"##;
    const OLD_KEYWORDS_TAB: &str = r##"<button class="tab-btn" role="tab" aria-selected="false" hx-get="/tab/keyword_tools" hx-target="#content" hx-swap="innerHTML" onclick="setActiveTab(this)" title="検索キーワードの需要をアプリ内で確認">キーワード需要</button>"##;
    const OLD_JOBGEN_TAB: &str = r##"<button class="tab-btn" role="tab" aria-selected="false" hx-get="/tab/jobgen_tools" hx-target="#content" hx-swap="innerHTML" onclick="setActiveTab(this)" title="求人票生成・競合比較・応募者ジャーニー診断">求人票作成</button>"##;
    /// 置き換え前の `templates/dashboard_inline.html:176-187` (`#explore-subnav` の中身)。
    const OLD_EXPLORE: &str = r##"        <button class="tab-btn" role="tab" aria-selected="false" hx-get="/tab/jobmap" hx-target="#content" hx-swap="innerHTML"
            onclick="setActiveTab(this)">地図</button>
        <button class="tab-btn" role="tab" aria-selected="false" hx-get="/tab/regional_analysis" hx-target="#content" hx-swap="innerHTML"
            onclick="setActiveTab(this)">地域分析</button>
        <button class="tab-btn" role="tab" aria-selected="false" hx-get="/tab/company" hx-target="#content" hx-swap="innerHTML"
            onclick="setActiveTab(this)">企業検索</button>
        <button class="tab-btn" role="tab" aria-selected="false" hx-get="/tab/driver" hx-target="#content" hx-swap="innerHTML"
            onclick="setActiveTab(this)">職種辞典</button>
        <button class="tab-btn" role="tab" aria-selected="false" hx-get="/tab/license" hx-target="#content" hx-swap="innerHTML"
            onclick="setActiveTab(this)">資格辞書</button>
        <button class="tab-btn" role="tab" aria-selected="false" hx-get="/tab/indeed" hx-target="#content" hx-swap="innerHTML"
            onclick="setActiveTab(this)" title="求人媒体の掲載データから見た、職種別・地域別の採用市場の動き">採用市場</button>
"##;

    /// HTML を「要素 (タグ名, 属性の並び, テキスト)」の列にする。空白と HTML コメントは無視する。
    /// 属性の値・順序・onclick・title まで同じかを、字下げに左右されずに比べるため。
    type Element = (String, Vec<(String, String)>, String);

    fn elements(html: &str) -> Vec<Element> {
        let mut out = Vec::new();
        let mut rest = html;
        while let Some(lt) = rest.find('<') {
            rest = &rest[lt..];
            if let Some(after) = rest.strip_prefix("<!--") {
                let end = after.find("-->").expect("閉じていないコメント");
                rest = &after[end + 3..];
                continue;
            }
            if rest.starts_with("</") {
                let end = rest.find('>').unwrap();
                rest = &rest[end + 1..];
                continue;
            }
            let end = rest.find('>').unwrap();
            let tag_body = &rest[1..end];
            rest = &rest[end + 1..];
            let mut parts = tag_body.splitn(2, char::is_whitespace);
            let tag = parts.next().unwrap().to_string();
            let mut attrs = Vec::new();
            let mut a = parts.next().unwrap_or("").trim();
            while !a.is_empty() {
                let eq = a.find('=').expect("値の無い属性は使っていない");
                let name = a[..eq].trim().to_string();
                let after_eq = &a[eq + 1..];
                assert!(after_eq.starts_with('"'), "{after_eq}");
                let close = after_eq[1..].find('"').unwrap() + 1;
                attrs.push((name, after_eq[1..close].to_string()));
                a = after_eq[close + 1..].trim();
            }
            let text_end = rest.find('<').unwrap_or(rest.len());
            let text = rest[..text_end].trim().to_string();
            rest = &rest[text_end..];
            out.push((tag, attrs, text));
        }
        out
    }

    #[test]
    fn job_copy_navigation_contract_is_shared_without_granting_data_access() {
        for admin in [false, true] {
            let response =
                build_nav_response("viewer@example.test".into(), admin, &features(false, false));
            let matching: Vec<_> = response
                .items
                .iter()
                .filter(|item| item.id == "job-copy")
                .collect();
            assert_eq!(matching.len(), 1);
            let item = matching[0];
            assert_eq!(item.label, "求人文面（MOC）");
            assert_eq!(item.kind, NavKind::App);
            assert_eq!(item.href, "/app/job-copy");
            assert!(!item.hidden);
            assert_eq!(item.group, None);
            let json = serde_json::to_value(item).unwrap();
            assert_eq!(json["kind"], "app");
            assert_eq!(json["href"], "/app/job-copy");
            let nav = render_legacy_nav(&response.items);
            assert_eq!(nav.top.matches("href=\"/app/job-copy\"").count(), 1);
            assert!(!nav.explore.contains("/app/job-copy"));
        }
    }

    fn old_top(keywords: bool, jobgen: bool) -> String {
        OLD_TOP
            .replace(
                "{{KEYWORDS_TAB}}",
                if keywords { OLD_KEYWORDS_TAB } else { "" },
            )
            .replace("{{JOBGEN_TAB}}", if jobgen { OLD_JOBGEN_TAB } else { "" })
    }

    #[test]
    fn 旧シェルのナビは置き換え前のテンプレートと同じ要素列になる() {
        for (kw, jg) in [(false, false), (true, false), (false, true), (true, true)] {
            let items = nav_items(NAV_DEFS, &features(kw, jg));
            let nav = render_legacy_nav(&items);
            let top = elements(&nav.top);
            let competitor: Vec<_> = top.iter().filter(|e| e.2 == "競合調査").collect();
            assert_eq!(competitor.len(), 1);
            assert_eq!(competitor[0].0, "a");
            assert!(competitor[0]
                .1
                .contains(&("href".into(), "/competitor".into())));
            let job_copy_items: Vec<_> =
                items.iter().filter(|item| item.id == "job-copy").collect();
            assert_eq!(job_copy_items.len(), 1);
            assert_eq!(job_copy_items[0].kind, NavKind::App);
            assert_eq!(job_copy_items[0].href, "/app/job-copy");
            let job_copy_links: Vec<_> = top
                .iter()
                .filter(|element| element.2 == "求人文面（MOC）")
                .collect();
            assert_eq!(job_copy_links.len(), 1);
            assert_eq!(job_copy_links[0].0, "a");
            assert!(job_copy_links[0]
                .1
                .contains(&("href".into(), "/app/job-copy".into())));
            assert_eq!(
                top.iter()
                    .filter(|element| element.1.contains(&("href".into(), "/app/job-copy".into())))
                    .count(),
                1
            );
            assert_eq!(
                top.into_iter()
                    .filter(|e| e.2 != "競合調査")
                    // Exclude only the separately validated new link from the old golden.
                    .filter(|e| !e.1.contains(&("href".into(), "/app/job-copy".into())))
                    .collect::<Vec<_>>(),
                elements(&old_top(kw, jg)),
                "top (keywords={kw}, jobgen={jg})\n--- 生成 ---\n{}",
                nav.top
            );
            assert_eq!(
                elements(&nav.explore),
                elements(OLD_EXPLORE),
                "explore\n--- 生成 ---\n{}",
                nav.explore
            );
        }
        // 比較関数そのものが動いていること (属性値・順序・テキストの違いを見分ける)
        let a = elements(r#"<a class="x" href="/a">A</a>"#);
        assert_ne!(a, elements(r#"<a class="x" href="/b">A</a>"#));
        assert_ne!(a, elements(r#"<a href="/a" class="x">A</a>"#));
        assert_ne!(a, elements(r#"<a class="x" href="/a">B</a>"#));
        assert_eq!(
            a,
            elements("  <a class=\"x\"\n   href=\"/a\">A</a>\n<!-- c -->")
        );
        assert_eq!(
            elements(&old_top(true, true)).len(),
            7,
            "survey, kw, jobgen, group, sep, sales, consulting"
        );
        assert_eq!(elements(OLD_EXPLORE).len(), 6);
    }

    #[test]
    fn 旧シェルのナビは要素ごとに旧jsが頼る属性を持つ() {
        let items = nav_items(NAV_DEFS, &features(true, true));
        let nav = render_legacy_nav(&items);
        // setActiveTab / reloadActiveTab / applyActiveTab は .tab-btn の hx-get と onclick を見る
        for (tag, attrs, _) in elements(&nav.top)
            .iter()
            .chain(elements(&nav.explore).iter())
        {
            let get = |n: &str| attrs.iter().find(|(k, _)| k == n).map(|(_, v)| v.as_str());
            match tag.as_str() {
                "button" if get("hx-get").is_some() => {
                    assert!(get("class").unwrap().starts_with("tab-btn"));
                    assert_eq!(get("onclick"), Some("setActiveTab(this)"));
                    assert_eq!(get("hx-target"), Some("#content"));
                    assert_eq!(get("hx-swap"), Some("innerHTML"));
                    assert_eq!(get("role"), Some("tab"));
                }
                "button" => {
                    assert_eq!(get("id"), Some("explore-group-btn"));
                    assert_eq!(get("class"), Some("tab-btn-group"));
                    assert_eq!(get("onclick"), Some("toggleExploreGroup()"));
                    assert_eq!(get("aria-controls"), Some("explore-subnav"));
                }
                "a" => assert_eq!(get("class"), Some("tab-btn tab-link")),
                "span" => assert_eq!(get("class"), Some("tab-sep")),
                other => panic!("想定外のタグ {other}"),
            }
        }
        // 既定タブだけが active
        let active: Vec<_> = elements(&nav.top)
            .into_iter()
            .filter(|(_, attrs, _)| {
                attrs
                    .iter()
                    .any(|(k, v)| k == "class" && v.contains("active"))
            })
            .collect();
        assert_eq!(active.len(), 1);
        assert!(active[0]
            .1
            .contains(&("hx-get".to_string(), DEFAULT_LEGACY_TAB.to_string())));
        assert!(active[0]
            .1
            .contains(&("aria-selected".to_string(), "true".to_string())));
        // JS 側の既定タブと同じ値
        let tpl = include_str!("../../templates/dashboard_inline.html");
        assert!(
            tpl.contains(&format!("var DEFAULT_TAB = '{DEFAULT_LEGACY_TAB}';")),
            "dashboard_inline.html の DEFAULT_TAB が {DEFAULT_LEGACY_TAB} でない"
        );
        assert!(tpl.contains("{{NAV_TOP_ITEMS}}") && tpl.contains("{{NAV_EXPLORE_ITEMS}}"));
        assert!(tpl.contains(r#"id="explore-subnav""#));
    }

    #[test]
    fn hidden切替は旧新両方に効く() {
        let f = features(false, false);
        // 現状: market は hidden → JSON には hidden=true で在り、旧シェル HTML には無い
        let items = nav_items(NAV_DEFS, &f);
        let market = items.iter().find(|i| i.id == "market").unwrap();
        assert!(market.hidden);
        let nav = render_legacy_nav(&items);
        assert!(!nav.top.contains(r#"hx-get="/tab/market""#), "{}", nav.top);
        assert!(!nav.explore.contains("/tab/market"));
        let json = serde_json::to_value(&items).unwrap();
        let j = json
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == "market")
            .unwrap();
        assert_eq!(j["hidden"], true);

        // 定義の hidden を 1 か所外す → 旧シェル HTML と JSON の両方に出る
        let mut defs: Vec<NavDef> = NAV_DEFS.to_vec();
        let d = defs.iter_mut().find(|d| d.id == "market").unwrap();
        d.hidden = None;
        let items = nav_items(&defs, &f);
        let nav = render_legacy_nav(&items);
        assert!(
            nav.top.contains(
                r##"<button class="tab-btn" role="tab" aria-selected="false" hx-get="/tab/market" hx-target="#content" hx-swap="innerHTML""##
            ),
            "{}",
            nav.top
        );
        assert!(nav
            .top
            .contains(r#"onclick="setActiveTab(this)">市場概況</button>"#));
        let json = serde_json::to_value(&items).unwrap();
        let j = json
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == "market")
            .unwrap();
        assert_eq!(j["hidden"], false);
        assert_eq!(j["hidden_reason"], serde_json::Value::Null);
        assert_eq!(j["hidden_since"], serde_json::Value::Null);
        assert_eq!(j["href"], "/?tab=/tab/market");

        // 逆: 表示中の項目を hidden にすると両方から消える
        let mut defs: Vec<NavDef> = NAV_DEFS.to_vec();
        let d = defs.iter_mut().find(|d| d.id == "jobmap").unwrap();
        d.hidden = Some(Hidden {
            reason: "テスト",
            since: Some("2026-09-30"),
        });
        let items = nav_items(&defs, &f);
        let nav = render_legacy_nav(&items);
        assert!(!nav.explore.contains("/tab/jobmap"), "{}", nav.explore);
        assert!(nav.explore.contains("/tab/regional_analysis"));
        let json = serde_json::to_value(&items).unwrap();
        let j = json
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"] == "jobmap")
            .unwrap();
        assert_eq!(j["hidden"], true);
        assert_eq!(j["hidden_reason"], "テスト");
        assert_eq!(j["hidden_since"], "2026-09-30");
        // hidden の page も HTML に出ない (call_quality / proposal_mock)
        let items = nav_items(NAV_DEFS, &f);
        let nav = render_legacy_nav(&items);
        assert!(!nav.top.contains("/call-quality"));
        assert!(!nav.top.contains("/proposal-mock"));
    }

    #[test]
    fn 採用診断はreact画面を指し非表示のまま() {
        let items = nav_items(NAV_DEFS, &features(false, false));
        let it = items.iter().find(|i| i.id == "recruitment-diag").unwrap();
        assert_eq!(it.kind, NavKind::App);
        assert_eq!(it.href, "/app/recruitment-diag");
        assert!(it.hidden);
    }

    #[test]
    fn kindとhrefを変えれば旧シェルはaタグに切り替わる() {
        // 将来 React 画面に置き換わったとき: kind と target を 1 か所変えるだけ
        let mut defs: Vec<NavDef> = NAV_DEFS.to_vec();
        let d = defs
            .iter_mut()
            .find(|d| d.id == "recruitment-diag")
            .unwrap();
        d.kind = NavKind::App;
        d.target = "/app/recruitment-diag";
        d.hidden = None;
        let items = nav_items(&defs, &features(false, false));
        let it = items.iter().find(|i| i.id == "recruitment-diag").unwrap();
        assert_eq!(it.href, "/app/recruitment-diag");
        assert_eq!(it.kind, NavKind::App);
        let nav = render_legacy_nav(&items);
        assert!(
            nav.top.contains(
                r#"<a class="tab-btn tab-link" href="/app/recruitment-diag">採用診断</a>"#
            ),
            "{}",
            nav.top
        );
        // 区切りは最初の page/app の前に 1 つだけ
        assert_eq!(nav.top.matches("tab-sep").count(), 1);
        let sep = nav.top.find("tab-sep").unwrap();
        let sales = nav.top.find("/sales-kpi").unwrap();
        let app = nav.top.find("/app/recruitment-diag").unwrap();
        assert!(sep < sales && sales < app);
    }

    #[test]
    fn ラベルとtitleはエスケープされる() {
        let item = NavItem {
            id: "x".into(),
            label: "<b>&".into(),
            title: Some("a\"b".into()),
            kind: NavKind::Page,
            href: "/x?a=1&b=2".into(),
            group: None,
            hidden: false,
            hidden_reason: None,
            hidden_since: None,
        };
        let html = render_legacy_item(&item, false);
        assert_eq!(
            html.trim(),
            r#"<a class="tab-btn tab-link" href="/x?a=1&amp;b=2" title="a&quot;b">&lt;b&gt;&amp;</a>"#
        );
    }
}
