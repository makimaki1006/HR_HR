//! 架電クオリティ: axum ルーティング層
//!
//! 2026-08-16。GAS 版ダッシュボード16タブぶんのエンドポイントをここ1枚に集約する。
//! GAS 版とは**全く別のページ**として扱う（ユーザー決定）ため、既存 HR_HR の
//! タブ（`/tab/market` 等）と画面を混在させない。
//!
//! ------------------------------------------------------------------
//! パス規約（2026-08-16 チーム確定。勝手に変えない）
//! ------------------------------------------------------------------
//!   ページ : `/call-quality`
//!   API    : `/api/call-quality/<資源>`
//! 既存 HR_HR の実物（`/api/company/bulk-csv` `/api/driver/wage/{wage_code}`
//! `/api/insight/widget/overview` `/api/jobmap/heatmap`）と同じ
//! 「`/api/<領域>/<資源>` / 区切りはハイフン」に揃えている。
//! アンダースコア（`call_quality`）や内部ID（`/p8`）はパスに出さない。
//!
//! **`/api/call-quality/churn` と `/api/call-quality/timeline/*` はフロント担当
//! （port-p12-p13）にも同じものが伝わっている**。変えると繋がらなくなるので、
//! 変更が要るときは先にチームリードへ相談すること。
//!
//! ------------------------------------------------------------------
//! レスポンスの形（フロントは ECharts 5.5.1）
//! ------------------------------------------------------------------
//! 画面は ECharts 5.5.1（CDN・`templates/base.html`）+ `static/js/charts.js` の
//! `window.ChartHelpers` で描く。GAS 版の Chart.js ではない。
//!
//! **サーバは ECharts の option を組み立てない**。既存の動的 fetch パターン
//! （`static/js/survey_explore.js`）と同じく、返すのは
//! **集計済みの素の JSON**（`TabPayload<T>`）だけで、option の組み立ては
//! クライアント側に置く。ここで option を作ると、
//!   - 描画ライブラリを替えるたびにサーバのレスポンス型が壊れる
//!   - 同じ数字を表とグラフで二重に持つことになる
//! ため。タブ側の `handle()` も素の集計値を返す設計になっている。
//!
//! ------------------------------------------------------------------
//! 配線（`build_app()` 側でやること。このファイルからは触らない）
//! ------------------------------------------------------------------
//! 1. `src\handlers\call_quality\mod.rs` に `pub mod routes;` を足す
//! 2. `src\lib.rs` の `protected_routes` チェーンに
//!    `.merge(handlers::call_quality::routes::router())` を足す
//!    → **既存の `auth_middleware` / `activity_log_mw` がそのまま効く**。
//!      認証をこのファイルで作らないのはそのため（要件3）。
//!      `protected_routes` は最後に `.route_layer(auth_middleware)` を当てているので、
//!      merge するだけでログイン必須になる。
//! 3. （任意・推奨）`src\main.rs` の起動時に
//!    `handlers::call_quality::routes::init_from_env()` を呼ぶ。
//!    呼ばなくても初回リクエストで遅延初期化されるが、**起動時に呼んでおくと
//!    環境変数の設定漏れ（GOOGLE_SA_KEY_B64 / SPREADSHEET_ID）が
//!    デプロイ直後のログで分かる**。呼ばない場合は最初にアクセスした人が 503 を見る。
//!
//! ------------------------------------------------------------------
//! 設計判断
//! ------------------------------------------------------------------
//! **(A) なぜデータブラウザ(p7)だけ POST + JSON なのか**
//!   `BrowseQuery` / `ExportQuery` / `ChartQuery` は `RowFilter` を flatten で
//!   含み、その中に `filters: HashMap<String, Vec<String>>` がある。
//!   axum の `Query<T>`(serde_urlencoded) は **ネストした map/seq を復元できない**。
//!
//!   2026-08-16 に実測した挙動（想定より悪い）:
//!     `sheet=A&filters[owner_id][]=1&filters[owner_id][]=2`
//!       → **エラーにならない**。`Ok(BrowseQuery { filters: {} })` が返る。
//!     `sheet=A&filters.owner_id=1` → 同じく `filters: {}` で成功。
//!     `sheet=A&filters=owner_id`   → ここだけ "expected a map" でエラー。
//!   つまり `Query<T>` にしていると **400 で落ちてくれず、HTTP 200 のまま
//!   絞り込みが黙って消えた全件データが返る**。画面は絞り込んだつもりで
//!   絞り込まれていない数字を見ることになり、間違いに気づく手がかりが無い。
//!   コンパイルも通ってしまうので、この3本は POST + `Json<T>` に固定する。
//!
//!   逆に year_month / prefecture / owners のようなスカラだけのクエリは GET のままで良い。
//!   （将来タブを足すとき: **Query 型に HashMap / Vec が1つでもあれば POST**）
//!
//! **(B) なぜ SheetsClient / SheetStore を AppState に入れず OnceLock なのか**
//!   `AppState` は `src\lib.rs` にあり、このファイルの担当範囲外（並列作業中）。
//!   要件は「常駐で1インスタンス」なので、プロセス内シングルトン(`OnceLock`)で
//!   その要件は満たせる。リクエストごとに作らないので常駐キャッシュは効く。
//!   **AppState に移すときの手順**は `CallQualityState` の doc コメントに書いた。
//!   ハンドラは `State<Arc<AppState>>` を取っていないので、移設しても
//!   ハンドラ本体は書き換え不要（`cq()` の中身だけ差し替わる）。
//!
//! **(C) なぜエラーで 500 を投げっぱなしにしないのか**
//!   GAS 版は読取例外を握りつぶして空配列を「成功」として返しており、
//!   画面が「該当0件」と嘘をついた事故がある。ここでは
//!   `CqError` で **どのシートで失敗したかを JSON に載せて** 4xx/5xx を返す。
//!   加えて、タブ側が「1枚読めなくても他パネルを出す」設計（p2 の `get_or_empty`）
//!   の場合は成功応答になるので、`sources` に残った失敗を数えて
//!   `X-CallQuality-Degraded-Sheets` ヘッダと warn ログで可視化する。
//!
//! **(D) なぜ p0/p1 で「メンバーマスタ」を読むのか**
//!   `p0_overview::handle` / `p1_members::handle` の第4引数 `sales_owners` に
//!   `None` を渡すと **role による絞り込みが一切かからない**。
//!   これは tabs/mod.rs の約束5（メンバー未選択時に BPO/コンサルを混ぜない。
//!   GAS で 141名混在させてアポ率 0.93%→0.63% に希釈された事故）を再現する。
//!   よってルータ側で「メンバーマスタ」から role=sales を集めて渡す。
//!   **読めなかったときに None にフォールバックしない**（それが事故そのもの）。

use std::sync::{Arc, OnceLock};

use askama::Template;
use tower_sessions::Session;
use axum::extract::Query;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::db::sheets_client::SheetsClient;
use crate::SESSION_USER_KEY;
use crate::AppState;

use super::heatmap::{self, HeatmapCache, HeatmapQuery};
use super::sheets::SheetStore;
use super::tabs::{self, TabPayload};

// ================================================================ 常駐状態

/// 架電クオリティ専用の常駐リソース。
///
/// **リクエストごとに作ってはいけない**。作ると `SheetStore` の常駐キャッシュが
/// 毎回空になり、Sheets を叩き直すことになって移行の目的（実測 33.1MB → 6.7KB、
/// 2回目以降はネットワーク往復ゼロ）が消える。
///
/// AppState へ移す場合（`src\lib.rs` の担当者へ）:
///   1. `AppState` に `pub call_quality: Arc<CallQualityState>` を足す
///   2. `main.rs` で `CallQualityState::from_env()` を1回だけ呼んで詰める
///   3. このファイルの `cq()` を
///      `fn cq(state: &AppState) -> &CallQualityState { &state.call_quality }`
///      に差し替え、各ハンドラの引数に `State(state): State<Arc<AppState>>` を足す
///   ハンドラ本体（`finish(...)` の行）は変わらない。
pub struct CallQualityState {
    pub client: SheetsClient,
    /// 全シート共通の常駐キャッシュ（TTL 1h）
    pub store: SheetStore,
    /// 時間帯ヒートマップ専用の正規化済みキャッシュ（200,680行を型付きで保持）
    pub heatmap: HeatmapCache,
}

impl CallQualityState {
    /// 環境変数から初期化する。
    /// 必要な env: `GOOGLE_SA_KEY_B64`（Service Account JSON の base64）/ `SPREADSHEET_ID`
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            client: SheetsClient::from_env()?,
            store: SheetStore::new(),
            heatmap: HeatmapCache::new(),
        })
    }
}

/// プロセス内シングルトン。
///
/// `Result` ごとキャッシュしているのは、`from_env()` の失敗が
/// **環境変数の設定漏れ = プロセスを再起動しない限り直らない** 種類の失敗だから。
/// リクエストのたびに base64 デコードと JSON パースを繰り返しても直らないので、
/// 一度だけ試して結果を固定し、以後は同じ 503 を返す。
static CQ: OnceLock<Result<CallQualityState, String>> = OnceLock::new();

/// 起動時に明示初期化する（`main.rs` から呼ぶ想定）。
/// 呼ばなくても初回リクエストで遅延初期化されるが、設定漏れの発覚が
/// 「最初にアクセスした人が 503 を見たとき」まで遅れる。
pub fn init_from_env() -> anyhow::Result<()> {
    let built = CallQualityState::from_env();
    match CQ.get_or_init(|| built.map_err(|e| format!("{e:#}"))) {
        Ok(_) => {
            tracing::info!("架電クオリティ: Sheets クライアント初期化 OK");
            Ok(())
        }
        Err(msg) => anyhow::bail!("架電クオリティの初期化に失敗: {msg}"),
    }
}

fn cq() -> Result<&'static CallQualityState, CqError> {
    match CQ.get_or_init(|| CallQualityState::from_env().map_err(|e| format!("{e:#}"))) {
        Ok(s) => Ok(s),
        Err(msg) => Err(CqError::not_configured(msg)),
    }
}

// ================================================================ エラー

/// このタブ群が返すエラー。**必ず JSON で返す**。
///
/// GAS 版は取得失敗を空配列（＝成功）にすり替えて画面に「該当0件」と出していた。
/// 0件なのか失敗なのかを呼び出し側が区別できるよう、`error: true` を必ず立て、
/// 失敗したシート名を `sheet` に載せる。
#[derive(Debug, Serialize)]
pub struct CqError {
    /// HTTP ステータス（本文には出さない）
    #[serde(skip)]
    status: StatusCode,
    /// 常に true。データ本文と取り違えないための目印。
    error: bool,
    /// 機械判定用。`sheet_fetch_failed` / `sheet_not_allowed` / `not_configured` /
    /// `bad_request` / `sales_scope_empty` / `internal`
    ///
    /// 移植待ちのタブはルート自体をコメントアウトしてあるので **404** になる
    /// （このエラー型は通らない）。画面が「パスの打ち間違い」と
    /// 「まだ移植されていない」を区別したいときは `/api/call-quality/tabs` の
    /// `implemented` を見ること。
    code: &'static str,
    /// どのタブで起きたか（p0 / p8 / …）
    tab: Option<&'static str>,
    /// どのシートで失敗したか。取れないこともあるので Option。
    sheet: Option<String>,
    /// 人が読むメッセージ（先頭の1件）
    message: String,
    /// anyhow の context チェーン全部。`sheet` が取れなかったときの最後の手がかり。
    chain: Vec<String>,
}

impl CqError {
    fn not_configured(msg: &str) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            error: true,
            code: "not_configured",
            tab: None,
            sheet: None,
            message: format!(
                "架電クオリティが未設定です（GOOGLE_SA_KEY_B64 / SPREADSHEET_ID を確認）: {msg}"
            ),
            chain: vec![msg.to_string()],
        }
    }

    fn bad_request(message: String) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            error: true,
            code: "bad_request",
            tab: None,
            sheet: None,
            message,
            chain: Vec::new(),
        }
    }

    /// 営業スコープ（role=sales）が空だったとき。
    /// ここで「絞らない」にフォールバックすると BPO/コンサルが混ざって
    /// アポ率が希釈される（約束5の事故）。0件表示も嘘になるので、明確に失敗させる。
    fn sales_scope_empty(tab: &'static str) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            error: true,
            code: "sales_scope_empty",
            tab: Some(tab),
            sheet: Some(SHEET_MEMBERS.to_string()),
            message: format!(
                "シート「{SHEET_MEMBERS}」から role=sales の担当者が1人も取れませんでした。\
                 絞り込みなしで代用すると BPO/コンサルが混ざってアポ率が希釈されるため、\
                 集計を中止しました。Python バッチの出力を確認してください"
            ),
            chain: Vec::new(),
        }
    }

    /// anyhow のエラーをタブ名つきで分類する。
    ///
    /// 分類はメッセージ文字列を見ている。タブ側が独自のエラー型を持っていないため
    /// 現状これが唯一の手がかりで、**壊れやすい**。
    /// 誤分類しても `chain` に全文が載るので情報は失われない設計にしてある。
    /// タブが出揃ったら共有のエラー enum に寄せるのが本筋。
    fn from_anyhow(tab: &'static str, e: anyhow::Error) -> Self {
        let chain: Vec<String> = e.chain().map(|c| c.to_string()).collect();
        let joined = chain.join(" / ");

        // `SheetStore::get` が付ける context 「シート「◯◯」の取得に失敗」から拾う
        let sheet = chain.iter().find_map(|c| extract_sheet_name(c));

        let (status, code) = if joined.contains("許可されていないシート名") {
            // p7 のホワイトリスト外。呼び出し側の誤りなので 4xx
            (StatusCode::BAD_REQUEST, "sheet_not_allowed")
        } else if sheet.is_some() || joined.contains("取得に失敗") {
            // Sheets API 側の失敗。自分のバグではないので 502 で上流を指す
            (StatusCode::BAD_GATEWAY, "sheet_fetch_failed")
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, "internal")
        };

        Self {
            status,
            error: true,
            code,
            tab: Some(tab),
            sheet,
            message: chain.first().cloned().unwrap_or_else(|| e.to_string()),
            chain,
        }
    }
}

/// 「シート「◯◯」の取得に失敗」から ◯◯ を取り出す。
fn extract_sheet_name(msg: &str) -> Option<String> {
    let start = msg.find('\u{300c}')? + '\u{300c}'.len_utf8(); // 「
    let rest = &msg[start..];
    let end = rest.find('\u{300d}')?; // 」
    let name = rest[..end].trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

impl IntoResponse for CqError {
    fn into_response(self) -> Response {
        let status = self.status;
        if status.is_server_error() {
            tracing::error!("架電クオリティ {:?}: {}", self.tab, self.message);
        }
        (status, Json(self)).into_response()
    }
}

// ================================================================ 応答の共通処理

/// シート名（メンバーマスタ）。`KNOWN_SHEETS` には無いが実在するタブ外マスタ。
const SHEET_MEMBERS: &str = "メンバーマスタ";

/// タブの応答を組み立てる。
///
/// 「該当0件」と「取れなかった/空だった」を混同させないための後処理を挟む。
/// 数えるのは2種類:
///
/// 1. **取得失敗** — タブによっては「1枚読めなくても他パネルは出す」
///    （p2 の `get_or_empty`）ため、一部が欠けたまま 200 で返ることがある。
///
/// 2. **0行のシート** — 取得は成功したが中身が無い状態。Python バッチ未実行で
///    実際に起きる。この場合タブは全部 0 の集計を返し、**HTTP も JSON も
///    完全に正常に見える**。GAS 版で画面が「該当0件」と嘘をついた事故は
///    こちらの形。`sources[].total_rows` を見れば分かるが、
///    見なければ分からないので明示的に数える。
///
/// ヘッダに載せるのは**件数だけ**。HTTP ヘッダは ASCII しか安全に置けず、
/// 日本語のシート名を入れると送信時に落ちる。名前は JSON の `sources` と
/// warn ログの方に残す。
fn tab_json<T: Serialize>(tab: &'static str, payload: TabPayload<T>) -> Response {
    let degraded: Vec<&str> = payload
        .sources
        .iter()
        .filter(|s| s.sheet.contains("取得失敗"))
        .map(|s| s.sheet.as_str())
        .collect();
    let empty: Vec<&str> = payload
        .sources
        .iter()
        .filter(|s| !s.sheet.contains("取得失敗") && s.total_rows == 0)
        .map(|s| s.sheet.as_str())
        .collect();

    // 件数だけ先に取り出す（この後 payload を Json へ move するため、
    // sources を借りている degraded / empty はここで使い切る）
    let n_degraded = degraded.len();
    let n_empty = empty.len();
    if n_degraded > 0 {
        tracing::warn!("架電クオリティ{tab}: 読めなかったシートが{n_degraded}枚あります: {degraded:?}");
    }
    if n_empty > 0 {
        tracing::warn!(
            "架電クオリティ{tab}: 0行のシートが{n_empty}枚あります（Python バッチ未実行の可能性）: {empty:?}"
        );
    }

    let mut res = Json(payload).into_response();
    let h = res.headers_mut();
    if n_degraded > 0 {
        if let Ok(v) = HeaderValue::from_str(&n_degraded.to_string()) {
            h.insert("x-callquality-degraded-sheets", v);
        }
    }
    if n_empty > 0 {
        if let Ok(v) = HeaderValue::from_str(&n_empty.to_string()) {
            h.insert("x-callquality-empty-sheets", v);
        }
    }
    res
}

/// `handle()` の結果を HTTP 応答に変換する定型。
fn finish<T: Serialize>(
    tab: &'static str,
    r: anyhow::Result<TabPayload<T>>,
) -> Result<Response, CqError> {
    match r {
        Ok(p) => Ok(tab_json(tab, p)),
        Err(e) => Err(CqError::from_anyhow(tab, e)),
    }
}

/// メンバーが個別に選択されているか。
///
/// **タブ側(`p0_overview` / `p1_members`)と完全に同じ解析でなければならない**。
/// タブ側は `split(',') → trim → 空を捨てる` の結果が空リストなら「指定なし」と見なし、
/// `sales_owners` によるフォールバックへ進む。
/// ここで `trim().is_empty()` だけを見ると `owners=" , ,"` を「指定あり」と誤判定し、
/// タブへ `sales_owners=None` を渡してしまう。その組み合わせだけ
/// **role 絞り込みが両側とも外れて全員が対象になる**（141名混在の事故そのもの）。
fn has_member_selection(owners: Option<&str>) -> bool {
    owners
        .unwrap_or("")
        .split(',')
        .any(|t| !t.trim().is_empty())
}

/// 営業スコープ（role=sales の owner_id）を「メンバーマスタ」から集める。
///
/// - `selected_owners` にメンバー個別指定があるときは **読まない**。
///   タブ側でメンバー指定が優先されるので、無関係なシートの取得失敗で
///   リクエストごと落とすのは筋が悪い。
/// - 読めなかった場合・0名だった場合は **None にフォールバックしない**。
///   None は「絞り込みなし」であって「安全な既定値」ではない（約束5）。
async fn sales_scope(
    tab: &'static str,
    s: &CallQualityState,
    selected_owners: Option<&str>,
) -> Result<Option<Vec<String>>, CqError> {
    if has_member_selection(selected_owners) {
        return Ok(None);
    }

    let (d, _cached) = s
        .store
        .get(&s.client, SHEET_MEMBERS)
        .await
        .map_err(|e| CqError::from_anyhow(tab, e))?;

    let ids: Vec<String> = d
        .rows
        .iter()
        .filter(|r| d.get(r, "role").trim().eq_ignore_ascii_case("sales"))
        .map(|r| d.get(r, "owner_id").trim().to_string())
        .filter(|v| !v.is_empty())
        .collect();

    if ids.is_empty() {
        return Err(CqError::sales_scope_empty(tab));
    }
    Ok(Some(ids))
}

// ================================================================ タブ一覧

/// 画面に出すタブの台帳。
///
/// 表示名をフロント側にもう一度書かせないために、サーバが配る。
/// （GAS 版では表示名が index.html と JS の両方にあり、片方だけ直る事故があった）
#[derive(Debug, Serialize)]
pub struct TabInfo {
    /// GAS 版 `data-page` と同じ ID。パスの末尾もこれに揃えてある
    pub id: &'static str,
    /// 画面に出す日本語名
    pub name: &'static str,
    pub path: &'static str,
    pub method: &'static str,
    /// false のタブは 501 を返す（移植待ち）
    pub implemented: bool,
}

/// GAS 版 index.html の `data-page` の並び順そのまま。
///
/// `path` は 2026-08-16 にチームで確定した規約に従う（`API_PREFIX` の説明を参照）。
/// **`id` は GAS 版 `data-page` のまま残す**。パスから内部IDが消えたので、
/// 画面側が「どのタブか」を機械的に判別する手がかりがこれしかない。
const TABS: &[TabInfo] = &[
    TabInfo { id: "p0",   name: "全社サマリ",       path: "/api/call-quality/overview",        method: "GET",  implemented: true },
    TabInfo { id: "p1",   name: "メンバー比較",     path: "/api/call-quality/members",         method: "GET",  implemented: true },
    TabInfo { id: "pbpo", name: "BPOダッシュボード", path: "/api/call-quality/bpo",            method: "GET",  implemented: true },
    TabInfo { id: "p2",   name: "習慣の差",         path: "/api/call-quality/habits",          method: "GET",  implemented: true },
    TabInfo { id: "p3",   name: "時系列",           path: "/api/call-quality/timeseries",      method: "GET",  implemented: true },
    TabInfo { id: "ptf",  name: "ターゲット分析",   path: "/api/call-quality/target",          method: "GET",  implemented: true },
    // データブラウザだけ POST。理由はファイル冒頭 (A)
    TabInfo { id: "p7",   name: "データブラウザ",   path: "/api/call-quality/browse",          method: "POST", implemented: true },
    TabInfo { id: "prisk", name: "リスクボード",    path: "/api/call-quality/riskboard",       method: "GET",  implemented: true },
    TabInfo { id: "p8",   name: "コンサル接触",     path: "/api/call-quality/consulting",      method: "GET",  implemented: true },
    TabInfo { id: "p10",  name: "未来アクション",   path: "/api/call-quality/future-actions",  method: "GET",  implemented: true },
    TabInfo { id: "p11",  name: "行動量分析",       path: "/api/call-quality/activity",        method: "GET",  implemented: false },
    TabInfo { id: "p12",  name: "解約分析",         path: "/api/call-quality/churn",           method: "GET",  implemented: true },
    TabInfo { id: "p13",  name: "案件タイムライン", path: "/api/call-quality/timeline/deals",  method: "GET",  implemented: true },
    TabInfo { id: "p14",  name: "担当者360°",       path: "/api/call-quality/owner360",        method: "GET",  implemented: true },
    TabInfo { id: "p15",  name: "案件マネジメント", path: "/api/call-quality/pipeline-mgmt",   method: "GET",  implemented: false },
    TabInfo { id: "pja",  name: "求人・応募",       path: "/api/call-quality/job-application", method: "GET",  implemented: true },
];

// ================================================================ ルータ

/// 架電クオリティのルータ。
///
/// `Router<Arc<AppState>>` にしてあるのは `build_app()` の `protected_routes` へ
/// `.merge()` するため（既存 `handlers::driver::router()` 等と同じ形）。
/// ハンドラ自体は `AppState` を使わないが、merge するには state 型が一致している必要がある。
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        // ---- 運用系 ----
        // タブ台帳。画面が最初に1回だけ叩く。表示名の二重定義を防ぐために置く。
        .route("/api/call-quality/tabs", get(tabs_index))
        // 常駐キャッシュの中身（何シートが何行載っているか）。
        // 「本当に常駐しているか」を運用で確認するために置く。
        .route("/api/call-quality/cache", get(cache_stats))
        // GAS 版 `?refresh=1` 相当。**POST にする**理由:
        //   キャッシュ破棄はサーバ状態を変える操作で、GET にするとブラウザや
        //   プリフェッチャが勝手に叩いて全シート再取得（= Sheets API 消費）を招く。
        .route("/api/call-quality/refresh", post(refresh))
        // ---- サーバ側集計 PoC（時間帯ヒートマップ）----
        // クエリはスカラのみ（prefecture / industry / owners）なので GET で良い。
        .route("/api/call-quality/heatmap", get(heatmap_handler))
        // ---- 16タブ ----
        .route("/api/call-quality/overview", get(p0))
        .route("/api/call-quality/members", get(p1))
        .route("/api/call-quality/bpo", get(pbpo))
        .route("/api/call-quality/habits", get(p2))
        .route("/api/call-quality/timeseries", get(p3))
        .route("/api/call-quality/target", get(ptf))
        // データブラウザ: 選べるシート一覧だけは引数が無いので GET
        .route("/api/call-quality/browse/sheets", get(p7_sheets))
        // 以下3本は RowFilter(HashMap) を含むため POST + JSON 固定。理由は冒頭 (A)
        .route("/api/call-quality/browse", post(p7_browse))
        .route("/api/call-quality/browse/export", post(p7_export))
        // クイック可視化。確定パス一覧には無かったが、GAS 版データブラウザの
        // 機能で `handle_chart` も実装済みなので同じ browse 配下に置いた。
        .route("/api/call-quality/browse/chart", post(p7_chart))
        .route("/api/call-quality/riskboard", get(prisk))
        .route("/api/call-quality/consulting", get(p8))
        .route("/api/call-quality/future-actions", get(p10))
        .route("/api/call-quality/churn", get(p12))
        // 案件タイムラインは「一覧」と「1件の詳細」で分ける（GAS 版も2段構え）
        .route("/api/call-quality/timeline/deals", get(p13_index))
        .route("/api/call-quality/timeline/deal", get(p13_deal))
        .route("/api/call-quality/owner360", get(p14))
        .route("/api/call-quality/job-application", get(pja))
    // ---- 移植待ち（tabs/ にファイルが無い2本）----
    // 着地したらコメントを外す。ハンドラ側も同じ場所にコメントで置いてある。
    // 2026-08-16: 着地したので有効化
    .route("/api/call-quality/activity", get(p11))
    .route("/api/call-quality/pipeline-mgmt", get(p15))
    //
    // ---- 画面本体 ----
    // 2026-08-16: 画面本体。テンプレートが着地したので有効化。
    .route("/call-quality", get(call_quality_page))
}

// ================================================================ 運用系ハンドラ

async fn tabs_index() -> Json<&'static [TabInfo]> {
    Json(TABS)
}

#[derive(Debug, Serialize)]
struct CacheEntry {
    sheet: String,
    rows: usize,
    age_secs: u64,
}

async fn cache_stats() -> Result<Json<Vec<CacheEntry>>, CqError> {
    let s = cq()?;
    Ok(Json(
        s.store
            .stats()
            .await
            .into_iter()
            .map(|(sheet, rows, age_secs)| CacheEntry {
                sheet,
                rows,
                age_secs,
            })
            .collect(),
    ))
}

#[derive(Debug, Default, Deserialize)]
pub struct RefreshQuery {
    /// 破棄する1枚。**省略すると全シート破棄**（確定仕様）。
    pub sheet: Option<String>,
}

#[derive(Debug, Serialize)]
struct RefreshResult {
    /// 破棄したシート名。全破棄なら null
    cleared_sheet: Option<String>,
    /// 全シートを破棄したか
    cleared_all: bool,
}

/// 常駐キャッシュを破棄する（GAS 版 `?refresh=1` 相当）。
///
/// `sheet` 省略で全破棄。全破棄は次のアクセスで最大 69シートの再取得
/// （Sheets API と待ち時間）を招くので、**実行したことを必ず warn ログに残す**。
/// 誤操作の追跡がこれしかできないため。
/// GET ではなく POST なのは、プリフェッチャに勝手に叩かれないようにするため。
async fn refresh(Query(q): Query<RefreshQuery>) -> Result<Json<RefreshResult>, CqError> {
    let s = cq()?;
    match q.sheet.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        Some(sheet) => {
            s.store.invalidate(Some(sheet)).await;
            tracing::info!("架電クオリティ: キャッシュ破棄 シート「{sheet}」");
            Ok(Json(RefreshResult {
                cleared_sheet: Some(sheet.to_string()),
                cleared_all: false,
            }))
        }
        None => {
            s.store.invalidate(None).await;
            tracing::warn!(
                "架電クオリティ: 全シートのキャッシュを破棄しました。\
                 次のアクセスで全シートを Sheets から取り直します"
            );
            Ok(Json(RefreshResult {
                cleared_sheet: None,
                cleared_all: true,
            }))
        }
    }
}

async fn heatmap_handler(Query(q): Query<HeatmapQuery>) -> Result<Response, CqError> {
    let s = cq()?;
    match heatmap::handle(&s.client, &s.heatmap, q).await {
        Ok(r) => Ok(Json(r).into_response()),
        Err(e) => Err(CqError::from_anyhow("heatmap", e)),
    }
}

// ================================================================ 各タブ

// ---- p0 全社サマリ ----
async fn p0(Query(q): Query<tabs::p0_overview::OverviewQuery>) -> Result<Response, CqError> {
    let s = cq()?;
    // メンバー未選択なら role=sales に絞る（約束5。理由は冒頭 (D)）
    let sales = sales_scope("p0", s, q.owners.as_deref()).await?;
    finish("p0", tabs::p0_overview::handle(&s.client, &s.store, q, sales).await)
}

// ---- p1 メンバー比較 ----
async fn p1(Query(q): Query<tabs::p1_members::MembersQuery>) -> Result<Response, CqError> {
    let s = cq()?;
    let sales = sales_scope("p1", s, q.owners.as_deref()).await?;
    finish("p1", tabs::p1_members::handle(&s.client, &s.store, q, sales).await)
}

// ---- pbpo BPOダッシュボード ----
async fn pbpo(Query(q): Query<tabs::pbpo_dashboard::PbpoQuery>) -> Result<Response, CqError> {
    let s = cq()?;
    finish("pbpo", tabs::pbpo_dashboard::handle(&s.client, &s.store, q).await)
}

// ---- p2 習慣の差 ----
// p2 は「メンバーマスタ」を自前で読んで role=sales を既定にするので、
// ルータ側で sales_scope を渡す必要はない（p0/p1 と設計が違う）。
async fn p2(Query(q): Query<tabs::p2_habits::P2Query>) -> Result<Response, CqError> {
    let s = cq()?;
    finish("p2", tabs::p2_habits::handle(&s.client, &s.store, q).await)
}

// ---- p3 時系列 ----
// このタブだけ `HeatmapCache` も要る（時間帯×曜日ヒートマップを内包するため）。
// 常駐キャッシュを共有するので、専用のヒートマップ API と同じ実体を渡す。
async fn p3(Query(q): Query<tabs::p3_timeseries::TimeseriesQuery>) -> Result<Response, CqError> {
    let s = cq()?;
    let sales = sales_scope("p3", s, q.owners.as_deref()).await?;
    finish(
        "p3",
        tabs::p3_timeseries::handle(&s.client, &s.store, &s.heatmap, q, sales).await,
    )
}

// ---- ptf ターゲット分析 ----
async fn ptf(Query(q): Query<tabs::ptf_target::TargetQuery>) -> Result<Response, CqError> {
    let s = cq()?;
    let sales = sales_scope("ptf", s, q.owners.as_deref()).await?;
    finish(
        "ptf",
        tabs::ptf_target::handle(&s.client, &s.store, q, sales).await,
    )
}

// ---- p7 データブラウザ ----
async fn p7_sheets() -> Json<Vec<&'static str>> {
    Json(tabs::p7_data_browser::list_sheets())
}

async fn p7_browse(
    Json(q): Json<tabs::p7_data_browser::BrowseQuery>,
) -> Result<Response, CqError> {
    let s = cq()?;
    finish(
        "p7",
        tabs::p7_data_browser::handle_browse(&s.store, &s.client, q).await,
    )
}

/// CSV エクスポート。
///
/// `text/csv` の添付ファイルではなく **JSON で返す**。
/// `CsvExport` は `truncated` / `matched_rows` / `row_count` を持っており、
/// 生の CSV を流すとこれが落ちて「上限で切ったこと」が伝わらなくなる（約束3）。
/// ダウンロードは画面側で Blob 化してもらう。
async fn p7_export(
    Json(q): Json<tabs::p7_data_browser::ExportQuery>,
) -> Result<Json<tabs::p7_data_browser::CsvExport>, CqError> {
    let s = cq()?;
    tabs::p7_data_browser::handle_export(&s.store, &s.client, q)
        .await
        .map(Json)
        .map_err(|e| CqError::from_anyhow("p7", e))
}

async fn p7_chart(Json(q): Json<tabs::p7_data_browser::ChartQuery>) -> Result<Response, CqError> {
    let s = cq()?;
    finish(
        "p7",
        tabs::p7_data_browser::handle_chart(&s.store, &s.client, q).await,
    )
}

// ---- prisk リスクボード ----
async fn prisk(Query(q): Query<tabs::prisk_riskboard::PriskQuery>) -> Result<Response, CqError> {
    let s = cq()?;
    finish("prisk", tabs::prisk_riskboard::handle(&s.client, &s.store, q).await)
}

// ---- p10 未来アクション ----
async fn p10(
    Query(q): Query<tabs::p10_future_actions::P10Query>,
) -> Result<Response, CqError> {
    let s = cq()?;
    finish("p10", tabs::p10_future_actions::handle(&s.client, &s.store, q).await)
}

// ---- p14 担当者360° ----
async fn p14(Query(q): Query<tabs::p14_owner360::P14Query>) -> Result<Response, CqError> {
    let s = cq()?;
    finish("p14", tabs::p14_owner360::handle(&s.client, &s.store, q).await)
}

// ---- p8 コンサル接触 ----
async fn p8(
    Query(q): Query<tabs::p8_consulting_contact::P8Query>,
) -> Result<Response, CqError> {
    let s = cq()?;
    finish("p8", tabs::p8_consulting_contact::handle(&s.client, &s.store, q).await)
}

// ---- p12 解約分析 ----
// 絞り込みパラメータを持たない（Python 側で全期間集計済）ので引数なし。
async fn p12() -> Result<Response, CqError> {
    let s = cq()?;
    finish("p12", tabs::p12_churn::get_churn_analysis(&s.client, &s.store).await)
}

// ---- p13 案件タイムライン ----
// 一覧（案件インデックス）と 1件の詳細を別パスにする。
// 一覧は全案件ぶんで重く、詳細は deal_id ごとに毎回変わるため、
// 同じパスに混ぜるとキャッシュの寿命が揃わない。
async fn p13_index() -> Result<Response, CqError> {
    let s = cq()?;
    finish("p13", tabs::p13_timeline::get_deal_index(&s.client, &s.store).await)
}

async fn p13_deal(
    Query(q): Query<tabs::p13_timeline::DealDetailQuery>,
) -> Result<Response, CqError> {
    let s = cq()?;
    if q.deal_id.trim().is_empty() {
        return Err(CqError::bad_request("deal_id は必須です".to_string()));
    }
    finish(
        "p13",
        tabs::p13_timeline::get_deal_detail(&s.client, &s.store, &q).await,
    )
}

// ---- pja 求人・応募 ----
async fn pja(Query(q): Query<tabs::pja_job_application::PjaQuery>) -> Result<Response, CqError> {
    let s = cq()?;
    finish("pja", tabs::pja_job_application::handle(&s.store, &s.client, q).await)
}

// ================================================================ 移植待ちのタブ
//
// パスは先に確保してある。404 ではなく **501 + code:"not_implemented"** を返すので、
// 画面は「パスの打ち間違い」と「まだ移植されていない」を区別できる。
// 実装が入ったら各関数の中身を、コメントにある1行へ差し替える。

// チームリードの指示（2026-08-16）により、ハンドラごとコメントアウトしておく。
// ルータ側の `.route(...)` も同じ2本がコメントアウトしてある。
// 着地したら「ルータの1行」と「ここのハンドラ」と「TABS の implemented」の
// **3か所を同時に**有効化すること（片方だけだと台帳と実体がズレる。
// `未着地タブと台帳が一致している` テストがそれを検知する）。
//
// ---- p11 行動量分析 ----
// 引数なし（絞り込みはフロント側の表示切替で行う設計）。
async fn p11(Query(q): Query<tabs::p11_activity::P11Query>) -> Result<Response, CqError> {
    let s = cq()?;
    finish("p11", tabs::p11_activity::handle(&s.client, &s.store, q).await)
}

//
// ---- p15 案件マネジメント ----
async fn p15(Query(q): Query<tabs::p15_pipeline_mgmt::P15Query>) -> Result<Response, CqError> {
    let s = cq()?;
    finish("p15", tabs::p15_pipeline_mgmt::handle(&s.client, &s.store, q).await)
}


// ================================================================ テスト

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn タブ台帳は16件でidが重複しない() {
        // GAS 版 index.html の data-page が16個。増減したら気づけるようにする。
        assert_eq!(TABS.len(), 16);
        let mut ids: Vec<&str> = TABS.iter().map(|t| t.id).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n, "タブIDが重複している");
    }

    #[test]
    fn 未着地タブと台帳が一致している() {
        // 移植待ちのタブは router() の `.route(...)` をコメントアウトしてあるので
        // 404 になる。台帳の implemented がそれとズレると、画面は
        // 「実装済みのはずなのに 404」を踏んで原因を追えなくなる。
        // 2026-08-16 時点で未着地は p11 / p15 の2本（tabs/ にファイルが無い）。
        let pending: Vec<&str> = TABS
            .iter()
            .filter(|t| !t.implemented)
            .map(|t| t.id)
            .collect();
        assert_eq!(
            pending,
            vec!["p11", "p15"],
            "タブが着地したら「ルータの route」「ハンドラ」「台帳の implemented」の3か所を同時に有効化すること"
        );
        assert_eq!(TABS.iter().filter(|t| t.implemented).count(), 14);
    }

    #[test]
    fn データブラウザだけpostになっている() {
        // Query<T>(serde_urlencoded) は HashMap<String, Vec<String>> を復元できず、
        // コンパイルは通るのに実行時 400 で落ちる。p7 を GET に戻さないための番人。
        for t in TABS {
            let expect = if t.id == "p7" { "POST" } else { "GET" };
            assert_eq!(t.method, expect, "タブ {} のメソッド", t.id);
        }
    }

    #[test]
    fn パスがチーム確定の規約に従っている() {
        // 2026-08-16 確定: `/api/<領域>/<資源>`、区切りはハイフン。
        // アンダースコアや内部ID(p8 等)をパスに出さない。
        for t in TABS {
            assert!(
                t.path.starts_with("/api/call-quality/"),
                "{} のパスが /api/call-quality/ 配下にない: {}",
                t.id,
                t.path
            );
            assert!(
                !t.path.contains('_'),
                "{} のパスにアンダースコアが混ざっている: {}",
                t.id,
                t.path
            );
            let resource = t.path.trim_start_matches("/api/call-quality/");
            assert!(
                !resource.is_empty() && resource != t.id,
                "{} のパスが内部IDのままになっている: {}",
                t.id,
                t.path
            );
        }
    }

    #[test]
    fn フロント担当と共有済みのパスを固定する() {
        // port-p12-p13 担当に同じものを伝えてある。ここを変えると画面が繋がらない。
        let path = |id: &str| TABS.iter().find(|t| t.id == id).unwrap().path;
        assert_eq!(path("p12"), "/api/call-quality/churn");
        assert_eq!(path("p13"), "/api/call-quality/timeline/deals");
        // Deal詳細は台帳に無い(タブ1件に2エンドポイントあるため)。router() 側の
        // "/api/call-quality/timeline/deal" と対で変更すること。
    }

    fn src(sheet: &str, total_rows: usize) -> super::super::tabs::SourceInfo {
        super::super::tabs::SourceInfo {
            sheet: sheet.to_string(),
            total_rows,
            matched_rows: 0,
            from_cache: false,
            age_secs: 0,
        }
    }

    #[test]
    fn 取得失敗と0行シートを数えてヘッダに出す() {
        // 「該当0件」と「そもそも読めていない/空だった」を混同させない。
        // 0行シートは HTTP も JSON も正常に見えるので、明示的に数える必要がある。
        let payload = TabPayload {
            data: 0u32,
            sources: vec![
                src("月次明細", 1200),
                src("メンバーマスタ（取得失敗: timeout）", 0),
                src("解約_モデル指標", 0),
            ],
            elapsed_ms: 1,
        };
        let res = tab_json("p0", payload);
        assert_eq!(
            res.headers().get("x-callquality-degraded-sheets").unwrap(),
            "1"
        );
        assert_eq!(
            res.headers().get("x-callquality-empty-sheets").unwrap(),
            "1",
            "取得失敗ぶんを0行側で二重に数えない"
        );
    }

    #[test]
    fn 全シート正常ならヘッダは付かない() {
        let payload = TabPayload {
            data: 0u32,
            sources: vec![src("月次明細", 1200)],
            elapsed_ms: 1,
        };
        let res = tab_json("p0", payload);
        assert!(res.headers().get("x-callquality-degraded-sheets").is_none());
        assert!(res.headers().get("x-callquality-empty-sheets").is_none());
    }

    #[test]
    fn シート名をエラーメッセージから取り出せる() {
        // SheetStore::get が付ける context の形
        assert_eq!(
            extract_sheet_name("シート「コンサル別ベンチマーク」の取得に失敗"),
            Some("コンサル別ベンチマーク".to_string())
        );
        assert_eq!(extract_sheet_name("よく分からない失敗"), None);
        assert_eq!(extract_sheet_name("シート「」の取得に失敗"), None);
    }

    #[test]
    fn 取得失敗は502でシート名が付く() {
        let e = anyhow::anyhow!("connection reset")
            .context("シート「月次明細」の取得に失敗");
        let cq = CqError::from_anyhow("p0", e);
        assert_eq!(cq.status, StatusCode::BAD_GATEWAY, "上流の失敗は 502");
        assert_eq!(cq.code, "sheet_fetch_failed");
        assert_eq!(cq.sheet.as_deref(), Some("月次明細"));
        assert_eq!(cq.tab, Some("p0"));
        assert!(cq.error, "データ本文と取り違えないための目印");
        assert!(
            cq.chain.iter().any(|c| c.contains("connection reset")),
            "根本原因が chain から消えていない: {:?}",
            cq.chain
        );
    }

    #[test]
    fn 許可外シートは400になる() {
        // 呼び出し側の誤りを 500 にしない（上流障害と区別する）
        let e = anyhow::anyhow!("許可されていないシート名: 秘密のシート");
        let cq = CqError::from_anyhow("p7", e);
        assert_eq!(cq.status, StatusCode::BAD_REQUEST);
        assert_eq!(cq.code, "sheet_not_allowed");
    }

    #[test]
    fn 分類できない失敗も情報を落とさない() {
        let e = anyhow::anyhow!("想定外").context("集計中に失敗");
        let cq = CqError::from_anyhow("p12", e);
        assert_eq!(cq.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(cq.code, "internal");
        assert_eq!(cq.sheet, None);
        assert_eq!(cq.chain.len(), 2, "context チェーンを全部載せる");
    }

    #[test]
    fn メンバー選択の判定がタブ側と一致する() {
        // タブ側は split(',') → trim → 空を捨てる で判定する。
        // ここがズレると role 絞り込みが両側とも外れて全員が対象になる。
        assert!(has_member_selection(Some("123,456")));
        assert!(has_member_selection(Some(" 123 ")));
        // 「区切り文字だけ」はタブ側で空リストになる = 指定なし扱い。
        // trim().is_empty() だけで判定していると、ここを true と誤判定して事故る。
        assert!(!has_member_selection(Some(" , ,")));
        assert!(!has_member_selection(Some(",")));
        assert!(!has_member_selection(Some("   ")));
        assert!(!has_member_selection(Some("")));
        assert!(!has_member_selection(None));
    }

    #[test]
    fn 営業スコープが空なら黙って全員にしない() {
        // 約束5: None(絞らない) は「安全な既定値」ではない
        let e = CqError::sales_scope_empty("p0");
        assert_eq!(e.status, StatusCode::BAD_GATEWAY);
        assert_eq!(e.code, "sales_scope_empty");
        assert!(e.message.contains("希釈"));
    }
}

// ================================================================ 画面本体

/// 架電クオリティのページ。
///
/// 2026-08-16 追加。GAS 版とは**全く別のページ**として扱う（ユーザー決定）。
/// ログインは HR_HR 既存のものを共通で使う（`protected_routes` に merge されるため
/// `auth_middleware` が自動で効く。ここで認証を書かない）。
///
/// テンプレートは 16タブの空コンテナを1枚で返すだけ。データは各タブの JS が
/// `cq:tab-shown` を購読して `/api/call-quality/*` から取りに行く。
#[derive(Template)]
#[template(path = "tabs/call_quality/_layout.html")]
struct CallQualityPage {
    user_email: String,
}

async fn call_quality_page(session: Session) -> Response {
    // 既存 `dashboard_page` と同じ取り方に揃える。
    let user_email: String = session
        .get(SESSION_USER_KEY)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string());

    // 返し方も既存 `handlers::driver` に揃える（askama_axum の IntoResponse に
    // 頼らず、明示的に render して Html を返す）。
    let page = CallQualityPage { user_email };
    match page.render() {
        Ok(body) => Html(body).into_response(),
        Err(e) => {
            tracing::error!("架電クオリティ: Askama render 失敗: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "render failed").into_response()
        }
    }
}
