//! `GET /api/crm/workspace/deals/{id}` (架電ワークスペースの詳細。HubSpot からの読み取りだけ)。
//!
//! 架電キューの 1 行を選んだときに、案件 (Deal)・主担当者 (Contact)・会社 (Company)・直近の活動履歴
//! (通話・メモ・メール・ミーティング) を 1 回の要求でまとめて返す。書き込みはしない。
//!
//! ## 認可 (順番に意味がある)
//! 1. `rbac::authorize` (Google OIDC + 許可リスト + 無効アカウント)。通らなければ HubSpot を **1 回も呼ばない**
//! 2. id の形式 (数字 1〜20 桁)。不正なら HubSpot を呼ばずに 400
//! 3. HubSpot 未設定なら 503
//! 4. CRM の利用者は全員 (管理者かどうかを問わず)、担当者に関係なくどの案件も読める (決定 2026-10-07)。
//!    本人の owner は引かない。役割が決まっていない (最小権限 `user`) ときだけ、従来のレコード単位の関門
//!    ([`deal_in_queue`]、外れたら 403 `forbidden_record`) を通す (安全側。通常の経路では来ない)
//!
//! 担当者・会社は読んだ案件の関連から引くので、その案件に紐づくものだけが返る。
//!
//! 5. サーバの短いキャッシュ ([`super::workspace_cache`]、60 秒、利用者をまたいで共有)。1〜4 は**キャッシュがあっても
//!    毎回**通し、関門のある人には キャッシュの案件の本体で同じ判定をしてから返す。`?fresh=1` はその案件の全キーを
//!    捨てて読み直す (画面の「最新にする」と、通話が終わった後の自動の読み直し)。欠け (`partial`) のある応答は入れない。
//!    応答の `fetched_at` は HubSpot から読んだ時刻、`cached` はキャッシュから返したか
//!
//! ## HubSpot 呼び出し回数 (1 回の要求あたり。鍵は営業自動化バッチと共有で 100 req/10 秒の枠を食うため上限を固定)
//! 順番に待つ段は 3 つ (以前は 4 つ)。同じ段の呼び出しは並列。キャッシュから返すときは 0 回。
//! | 段 | 呼び出し | 回数 |
//! |---|---|---|
//! | 1 | 案件の本体 + 通話/メモ/メール/ミーティング/担当者/会社の関連 ID (`GET deals/{id}?associations=..`) | 1 |
//! | 1 | 関連ラベルの定義 ([`super::assoc_labels`]、6 時間キャッシュ)・ステージ名 (5 分キャッシュ) | 冷えているとき 2 + 1 |
//! | 2 | 担当者の読み取り・会社の読み取り・担当者 → 通話 の関連・メモ/メール/ミーティングの `batch_read` | 最大 6 |
//! | 3 | 通話の `batch_read` (案件直付き + 担当者経由をまとめて 1 回。担当者がいなければ段 2 で読む) | 最大 1 |
//!
//! 定義が温まっていれば最大 8 回 (以前は 10 回)。冷えていれば最大 11 回 (以前と同じ)。
//! 例外: メールの読み取りスコープが共有鍵に無く 401/403 になったときだけ、メール抜きでもう 1 回 (段 1) 読み直す
//! (+1、`partial` に `emails` を出す)。関連の型名を定義で直せない・定義を読めないときだけ、従来の
//! v4 の関連 (ラベル付き) を読み直す (段 1 と 2 の間に +1 段、最大 +2 回)。
//! 活動の型ごとの上限は [`MAX_ENGAGEMENTS_PER_TYPE`] 件、表示は新しい順に [`MAX_ACTIVITIES`] 件まで。
//!
//! ## 活動の範囲 (`activity_scope` に文言で出す)
//! - 案件に直接つながる 通話・メモ・メール・ミーティング
//! - 案件の担当者 (先頭 [`MAX_CONTACTS_FOR_CALLS`] 人) に直接つながる **通話** (`deal → contacts → calls`)。
//!   同じ担当者の別の案件の通話も混ざりうる (`via` = `contact`)
//! - 担当者に直接つながるメモ・メール・ミーティングは含まない (呼び出し回数を抑えるため)
//!
//! 一部の取得 (担当者・会社・活動・ステージ名) が失敗しても案件は 200 で返し、失敗した部分を `partial` に出す。
//! 案件の取得の失敗だけがエラー応答になる。応答には `Cache-Control: no-store` が付く (router の層)。
//!
//! ## 選んだ項目 (「プロパティ」パネル)
//! `?deal_props=a,b&contact_props=..&company_props=..` (各 [`super::property_catalog::MAX_SELECTED_PER_OBJECT`] 件まで) で、
//! 利用者が選んだ項目の値も返す (`selected`)。案件は段 1 の本体の読み取り、担当者・会社は段 2 の batch_read に
//! 項目を足すだけなので、**HubSpot の呼び出し回数は増えない**。
//! 名前は形 (英数字と `_`) を確かめ、不正なら HubSpot を呼ばずに 400 `invalid_properties`。
//! さらに項目の一覧 ([`super::property_catalog`]、6 時間キャッシュ) に無い名前があれば 400 `invalid_properties`。
//! 一覧が冷えているときだけ一覧の取得 (定義 6 回) が先に走る。一覧を取れなかったときは選んだ項目を読まずに
//! 案件を返し、`partial` に `selected_properties` を出す。
//! 画面の既定は HubSpot の取引レコードの左サイドバーのカード「リスト情報」「BPOアポ情報」の 63 項目
//! (`frontend/src/screens/crm/hubspotCards.json`)。それを足しても案件の本体は GET 1 回のままで、URL は約 1,450 文字
//! (テストで 2,048 文字未満を確かめる。項目を最大の 100 件まで足すと長くなるため、そのときは batch read (POST) へ移す)。

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::SecondsFormat;
use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use ts_rs::TS;

use super::assoc_labels::resolve_labels;
use super::call_queue::{contact_name, deal_in_queue, jst_today_ms, nz, pick, DEAL_PROPERTIES};
use super::property_catalog::parse_selected;
use super::rbac;
use super::routes::{
    error_json, hubspot_error_response, is_valid_id, sort_ids_newest_first, timeout_response,
    timestamp_millis, CrmCtx, CRM_REQUEST_DEADLINE,
};
use super::workspace_cache::{CachedWorkspace, WorkspaceCacheKey};
use crate::hubspot::deep_link::{hubspot_portal_id, record_url};
use crate::hubspot::gateway::{cache_hit, cache_miss};
use crate::hubspot::{
    AssociationRef, EngagementType, HubSpotClient, HubSpotError, HubSpotRecord, RecordType,
};
use crate::AppState;

/// 活動の型ごとに `batch_read` で読む ID の上限 (超えたら ID の大きい = 新しい方を残し `activities_truncated`)
pub const MAX_ENGAGEMENTS_PER_TYPE: usize = 100;
/// 画面に返す活動の最大件数 (新しい順)
pub const MAX_ACTIVITIES: usize = 50;
/// 読む担当者の最大人数 (主 → 残り)
pub const MAX_CONTACTS: usize = 10;
/// 読む会社の最大数
pub const MAX_COMPANIES: usize = 3;
/// `担当者 → 通話` を辿る担当者の最大人数
pub const MAX_CONTACTS_FOR_CALLS: usize = 20;
/// 本文・件名の最大文字数 (超えたら末尾に `…`)
pub const MAX_TEXT_CHARS: usize = 600;
/// 活動の平文の全文の上限
pub const MAX_FULL_TEXT_CHARS: usize = 20_000;
/// 活動の HTML 本文の上限 (超えた分は切る)
pub const MAX_HTML_CHARS: usize = 60_000;

const DATA_SCOPE: &str = "HubSpot の読み取り結果。書き込みはしない";
const ACTIVITY_SCOPE: &str =
    "案件に直接つながる通話・メモ・メール・ミーティングと、案件の担当者に直接つながる通話。\
担当者に直接つながるメモ・メール・ミーティングは含みません。";

const CONTACT_PROPS: &[&str] = &[
    "firstname",
    "lastname",
    "jobtitle",
    "phone",
    "mobilephone",
    "email",
];
const COMPANY_PROPS: &[&str] = &[
    "name", "phone", "address", "city", "state", "zip", "industry", "domain", "website",
];
/// 詳細だけで読む案件の項目 (キューの Search では読まない)。中央の列でリンクとして開く URL の項目を含む。
/// 案件本体の読み取り (#1) に足すだけなので、HubSpot の呼び出し回数は増えない
const DETAIL_ONLY_DEAL_PROPS: &[&str] = &[
    "amount",
    "closedate",
    "bpo_32",
    "website_url",
    "recruit_media_observed_urls",
    "risuto_jigyousyokibo",
];
const CALL_PROPS: &[&str] = &[
    "hs_timestamp",
    "hs_call_title",
    "hs_call_body",
    "hs_call_recording_url",
    "hs_call_direction",
    "hs_call_status",
    "hs_call_duration",
    "hs_call_source",
    "hubspot_owner_id",
];
const NOTE_PROPS: &[&str] = &[
    "hs_timestamp",
    "hs_note_body",
    "hs_attachment_ids",
    "hubspot_owner_id",
];
const EMAIL_PROPS: &[&str] = &[
    "hs_timestamp",
    "hs_email_subject",
    "hs_email_text",
    "hs_email_direction",
    "hs_email_status",
    "hs_email_html",
    "hs_email_from_email",
    "hs_email_from_firstname",
    "hs_email_from_lastname",
    "hs_email_to_email",
    "hs_email_cc_email",
    "hs_email_thread_id",
    "hs_attachment_ids",
    "hubspot_owner_id",
];
const MEETING_PROPS: &[&str] = &[
    "hs_timestamp",
    "hs_meeting_title",
    "hs_meeting_body",
    "hs_meeting_outcome",
    "hs_meeting_start_time",
    "hs_meeting_end_time",
    "hs_meeting_location",
    "hubspot_owner_id",
];

/// 読む活動の型 (タスクは対象外)
const ENGAGEMENTS: [EngagementType; 4] = [
    EngagementType::Call,
    EngagementType::Note,
    EngagementType::Email,
    EngagementType::Meeting,
];

fn engagement_props(et: EngagementType) -> &'static [&'static str] {
    match et {
        EngagementType::Call => CALL_PROPS,
        EngagementType::Note => NOTE_PROPS,
        EngagementType::Email => EMAIL_PROPS,
        EngagementType::Meeting => MEETING_PROPS,
        EngagementType::Task => &[],
    }
}

// ---------------------------------------------------------------------------
// 応答の型 (ts-rs で frontend/src/generated/ に書き出す)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkspaceStop {
    /// 架電禁止理由 `bpo_3`
    pub prohibited_reason: Option<String>,
    /// ブロック理由 `bpo_4`
    pub block_reason: Option<String>,
    /// 不通時チェック `bpo_10`
    pub unreachable_check: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkspaceDeal {
    pub id: String,
    pub name: Option<String>,
    pub stage_id: Option<String>,
    /// パイプライン定義から引いた表示名。引けなければ null (ID を表示名にしない)
    pub stage_label: Option<String>,
    pub pipeline_id: Option<String>,
    pub owner_id: Option<String>,
    /// HubSpot の値のまま (文字列)
    pub amount: Option<String>,
    pub close_date: Option<String>,
    /// 次回架電日 `bpo_13`
    pub next_call_date: Option<String>,
    /// 次回架電の時間 `bpo_14`
    pub next_call_time: Option<String>,
    /// 最終架電日 `bpo_20`
    pub last_call_date: Option<String>,
    pub stop: WorkspaceStop,
    /// 案件に入っている担当者の電話番号 `bpo_29`
    pub bpo_phone: Option<String>,
    /// URL_求人検索 `bpo_32` (Google 検索の URL。HubSpot の値のまま。画面で http(s) かを確かめて開く)
    pub job_search_url: Option<String>,
    /// ホームページ `website_url` (HubSpot の値のまま)
    pub homepage_url: Option<String>,
    /// 外部求人媒体_求人URL `recruit_media_observed_urls` (複数の URL が入ることがある。HubSpot の値のまま)
    pub media_job_urls: Option<String>,
    /// 求人票URL `risuto_jigyousyokibo` (HubSpot の値のまま)
    pub job_posting_url: Option<String>,
    pub deep_link: String,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkspaceContact {
    pub id: String,
    pub name: Option<String>,
    pub job_title: Option<String>,
    pub phone: Option<String>,
    pub mobile: Option<String>,
    pub email: Option<String>,
    /// 案件との関連ラベル (無ければ空)
    pub labels: Vec<String>,
    /// 架電の第一候補 (関連ラベル「主」、なければ最初の 1 人)
    pub is_primary: bool,
    pub deep_link: String,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkspaceCompany {
    pub id: String,
    pub name: Option<String>,
    pub phone: Option<String>,
    /// 郵便番号・都道府県・市区町村・番地を空白でつないだもの
    pub address: Option<String>,
    pub industry: Option<String>,
    pub domain: Option<String>,
    /// Website URL `website` (HubSpot の値のまま)
    pub website: Option<String>,
    pub labels: Vec<String>,
    pub is_primary: bool,
    pub deep_link: String,
}

/// 架けるべき番号 (キューと同じ優先順位: 案件 `bpo_29` → 主担当者 phone → 主担当者 mobilephone → 主会社 phone)
#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkspaceDial {
    pub number: String,
    /// `deal` / `contact` / `mobile` / `company`
    pub source: String,
}

/// 全文表示用の追加情報。既存の項目 (`body` は 600 文字までの平文) とは別に、
/// 同じ HubSpot の読み取り結果から取り出す (追加の呼び出しはしない)
#[derive(Debug, Clone, Default, Serialize, TS)]
pub struct ActivityRich {
    /// HTML 本文 (HubSpot の値のまま、最大 [`MAX_HTML_CHARS`] 文字)。画面側で無害化してから描く
    pub body_html: Option<String>,
    /// 平文の全文 (改行を保つ、最大 [`MAX_FULL_TEXT_CHARS`] 文字)
    pub body_full: Option<String>,
    /// メールの差出人の名前
    pub from_name: Option<String>,
    /// メールの差出人のアドレス
    pub from_email: Option<String>,
    /// メールの宛先のアドレス
    pub to: Vec<String>,
    /// メールの CC のアドレス
    pub cc: Vec<String>,
    /// メールのスレッド (同じやりとりのまとまり) の識別子
    pub thread_id: Option<String>,
    /// 添付ファイルの数 (数えられるとき)
    pub attachments_count: Option<u32>,
    /// ミーティングの開始・終了時刻 (HubSpot の値のまま)
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    /// ミーティングの場所
    pub location: Option<String>,
    /// 通話の録音の URL (https のものだけ)
    pub recording_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkspaceActivity {
    pub id: String,
    /// `call` / `note` / `email` / `meeting`
    pub kind: String,
    /// 並べ替えに使った時刻 (`hs_timestamp`。HubSpot の文字列のまま)
    pub timestamp: Option<String>,
    /// 件名・タイトル (通話 / メール / ミーティング)
    pub title: Option<String>,
    /// 本文。HTML のタグは除いた平文で、最大 [`MAX_TEXT_CHARS`] 文字
    pub body: Option<String>,
    /// `INBOUND` / `OUTBOUND` 等 (HubSpot の値のまま)
    pub direction: Option<String>,
    /// 通話・メールの状態 / ミーティングの結果 (HubSpot の値のまま)
    pub status: Option<String>,
    /// 通話時間 (ミリ秒。HubSpot の `hs_call_duration`)
    pub duration_ms: Option<u32>,
    pub owner_id: Option<String>,
    /// 通話の登録元 `hs_call_source` (Zoom 連携か手入力かの見分け用)
    pub source: Option<String>,
    /// 見つけた経路: `deal` (案件に直接) / `contact` (担当者経由の通話)
    pub via: String,
    pub via_id: String,
    /// 全文表示用の追加情報 (無ければ省く)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rich: Option<ActivityRich>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkspacePartial {
    /// `contacts` / `companies` / `associations` / `stage_labels` / `calls_via_contacts` / `calls` / `notes` / `emails` / `meetings`
    pub part: String,
    /// `HubSpotError::error_kind()` の値
    pub error_kind: String,
}

/// 利用者が選んだ項目の値 (内部名 → HubSpot の値のまま。空なら null)。要求した名前はすべてキーに入る
#[derive(Debug, Clone, Default, Serialize, TS)]
pub struct WorkspaceSelected {
    pub deal: BTreeMap<String, Option<String>>,
    /// 主担当者 (`contacts` の先頭) の値。担当者が読めなければ空
    pub contact: BTreeMap<String, Option<String>>,
    /// 主会社 (`companies` の先頭) の値。会社が読めなければ空
    pub company: BTreeMap<String, Option<String>>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkspaceResponse {
    pub deal: WorkspaceDeal,
    pub dial: Option<WorkspaceDial>,
    /// 主 → 残りの順。最大 [`MAX_CONTACTS`] 人
    pub contacts: Vec<WorkspaceContact>,
    /// 案件に紐づく担当者の総数 (読んだ人数より多いことがある)
    pub contacts_total: u32,
    pub companies: Vec<WorkspaceCompany>,
    pub companies_total: u32,
    /// 新しい順。最大 [`MAX_ACTIVITIES`] 件
    pub activities: Vec<WorkspaceActivity>,
    /// 件数の上限や関連の打ち切りで、活動が完全でない可能性がある
    pub activities_truncated: bool,
    /// 活動の範囲の説明 (画面にそのまま出す)
    pub activity_scope: String,
    pub partial: Vec<WorkspacePartial>,
    /// `?deal_props=` 等で選んだ項目の値
    pub selected: WorkspaceSelected,
    pub hubspot_portal_id: String,
    pub data_scope: String,
    pub generated_at: String,
    /// この内容を HubSpot から読んだ時刻 (RFC 3339)。キャッシュから返したときは、そのとき読んだ時刻
    pub fetched_at: String,
    /// サーバの短いキャッシュ (60 秒) から返したか。`?fresh=1` なら常に false
    pub cached: bool,
}

// ---------------------------------------------------------------------------
// 文字列の整形
// ---------------------------------------------------------------------------

/// HubSpot の本文 (HTML) を平文にする。タグを除き、段落・改行は改行にし、主要な文字参照を戻し、
/// 空行を除き、`max` 文字で切る。画面は React が文字列として描くので HTML は解釈されないが、
/// 画面に `<p>` が出ないよう整える。
pub fn plain_text(raw: &str, max: usize) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '<' {
            out.push(c);
            continue;
        }
        let mut tag = String::new();
        let mut closed = false;
        for t in chars.by_ref() {
            if t == '>' {
                closed = true;
                break;
            }
            tag.push(t);
        }
        if !closed {
            // 閉じない '<' は文字として残す
            out.push('<');
            out.push_str(&tag);
            break;
        }
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        if matches!(
            name.as_str(),
            "br" | "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4"
        ) {
            out.push('\n');
        }
    }
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    // 空行は捨てる (段落は 1 つの改行で区切る)
    let lines: Vec<&str> = decoded
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let text = lines.join("\n");
    if text.is_empty() {
        return None;
    }
    if text.chars().count() > max {
        let cut: String = text.chars().take(max).collect();
        Some(format!("{cut}…"))
    } else {
        Some(text)
    }
}

fn text_prop(rec: &HubSpotRecord, key: &str) -> Option<String> {
    nz(rec, key).and_then(|v| plain_text(&v, MAX_TEXT_CHARS))
}

fn join_address(c: &HubSpotRecord) -> Option<String> {
    let parts: Vec<String> = ["zip", "state", "city", "address"]
        .iter()
        .filter_map(|k| nz(c, k))
        .collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

const PRIMARY_CONTACT_LABELS: &[&str] = &["主"];
const PRIMARY_COMPANY_LABELS: &[&str] = &["主", "Primary"];

/// 関連を「主 → 残り (HubSpot の返した順)」に並べ、先頭 `cap` 件に絞る。
fn ordered_refs(refs: &[AssociationRef], labels: &[&str], cap: usize) -> Vec<AssociationRef> {
    let mut out: Vec<AssociationRef> = Vec::new();
    if let Some(p) = pick(refs, labels) {
        out.push(p.clone());
    }
    for r in refs {
        if !out.iter().any(|o| o.id == r.id) {
            out.push(r.clone());
        }
    }
    out.truncate(cap);
    out
}

fn partial(part: &str, e: &HubSpotError) -> WorkspacePartial {
    tracing::warn!(
        error_kind = e.error_kind(),
        part,
        "crm workspace: partial failure"
    );
    WorkspacePartial {
        part: part.to_string(),
        error_kind: e.error_kind().to_string(),
    }
}

// ---------------------------------------------------------------------------
// ハンドラ
// ---------------------------------------------------------------------------

/// 選んだ項目 (カンマ区切りの内部名) と `fresh` (キャッシュを使わずに読み直す)
#[derive(Debug, Default, Deserialize)]
pub(super) struct WorkspaceQuery {
    deal_props: Option<String>,
    contact_props: Option<String>,
    company_props: Option<String>,
    fresh: Option<String>,
}

impl WorkspaceQuery {
    /// `fresh=1` / `fresh=true` だけを真とみなす
    fn fresh(&self) -> bool {
        matches!(
            self.fresh.as_deref().map(str::trim),
            Some("1") | Some("true")
        )
    }
}

/// 選んだ項目の名前 (形は確かめ済み)
#[derive(Debug, Default)]
struct SelectedProps {
    deal: Vec<String>,
    contact: Vec<String>,
    company: Vec<String>,
}

impl SelectedProps {
    fn parse(q: &WorkspaceQuery) -> Option<Self> {
        Some(Self {
            deal: parse_selected(q.deal_props.as_deref())?,
            contact: parse_selected(q.contact_props.as_deref())?,
            company: parse_selected(q.company_props.as_deref())?,
        })
    }
    fn is_empty(&self) -> bool {
        self.deal.is_empty() && self.contact.is_empty() && self.company.is_empty()
    }
}

/// `base` に `extra` を足した読み取り項目 (重複なし)
fn with_extra<'a>(base: &[&'a str], extra: &'a [String]) -> Vec<&'a str> {
    let mut out: Vec<&str> = base.to_vec();
    for n in extra {
        if !out.contains(&n.as_str()) {
            out.push(n.as_str());
        }
    }
    out
}

/// 選んだ項目の値 (要求した名前はすべてキーに入れる。空白だけ・空は null)
fn selected_values(
    rec: Option<&HubSpotRecord>,
    names: &[String],
) -> BTreeMap<String, Option<String>> {
    let Some(rec) = rec else {
        return BTreeMap::new();
    };
    names
        .iter()
        .map(|n| {
            let v = rec
                .properties
                .get(n)
                .and_then(|v| v.as_deref())
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string);
            (n.clone(), v)
        })
        .collect()
}

pub(super) async fn get_workspace_deal(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    Path(id): Path<String>,
    Query(query): Query<WorkspaceQuery>,
) -> Response {
    // 1) 認可 (設定有無より先)。通らなければ HubSpot を呼ばない。キャッシュがあっても毎回ここを通す
    let principal =
        match rbac::authorize(&session, &state, &ctx.access, Some(RecordType::Deal)).await {
            Ok(p) => p,
            Err(denied) => return denied.into_response(),
        };
    let role = rbac::resolve_role(&principal);
    // 2) id
    if !is_valid_id(&id) {
        return error_json(StatusCode::BAD_REQUEST, "invalid_id");
    }
    // 選んだ項目の形 (不正なら HubSpot を呼ばない)
    let Some(selected) = SelectedProps::parse(&query) else {
        return error_json(StatusCode::BAD_REQUEST, "invalid_properties");
    };
    // 3) HubSpot 設定
    let Some(client) = state.hubspot.clone() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    // 4) キャッシュ。`fresh` ならその案件の全キーを捨てて読み直す (画面の「最新にする」・通話が終わった後)。
    //    レコード単位の関門が要る人 (役割が決まっていない最小権限) には、関門を通してからでないと返さない
    let key = WorkspaceCacheKey::new(&id, &selected.deal, &selected.contact, &selected.company);
    let gated = !role.reads_all_records();
    let cached = if query.fresh() {
        ctx.workspace_cache.invalidate_deal(&id);
        None
    } else {
        ctx.workspace_cache.get(&key)
    };
    if cached.is_some() {
        cache_hit("workspace");
    } else {
        cache_miss("workspace");
    }
    super::routes::refresh_ahead(&ctx, &client);
    if let Some(hit) = &cached {
        if !gated {
            return cached_response(hit);
        }
    }
    // 5) 同時実行の枠待ちも含めて全体に締め切りを付ける
    let started = std::time::Instant::now();
    let _slot = match super::routes::acquire_read_slot(&ctx, &client).await {
        Ok(permit) => permit,
        Err(resp) => return resp,
    };
    let email = principal.email.clone().unwrap_or_default();
    let remaining = CRM_REQUEST_DEADLINE.saturating_sub(started.elapsed());
    let work = async {
        // 役割が決まっていない人 (最小権限) だけレコード関門を通す (安全側)。
        // 関門には自分の owner が要る。引けなければ何も読ませない (全員分に倒さない)
        let owner = if gated {
            match ctx.queue.owner_for(&client, &email).await {
                Ok(Some(o)) => Some(o),
                Ok(None) => return Err(error_json(StatusCode::FORBIDDEN, "owner_not_found")),
                Err(e) => return Err(hubspot_error_response(&e)),
            }
        } else {
            None
        };
        if let (Some(hit), Some(owner)) = (&cached, &owner) {
            // キャッシュも関門を通す (新しく読んだときと同じ判定。外れたら 403、本文は返さない)
            if !deal_in_queue(&hit.deal, owner, jst_today_ms(ctx.queue.now())) {
                return Err(forbidden_record());
            }
            return Ok(cached_response(hit));
        }
        let read_epoch = ctx.workspace_cache.epoch();
        let (body, deal) = build(&client, &ctx, owner.as_deref(), &id, selected).await?;
        // 欠けの無い成功した応答だけを入れる (読んでいる間に捨てる操作があれば入れない)
        if body.partial.is_empty() {
            ctx.workspace_cache
                .insert(key, body.clone(), deal, read_epoch);
        }
        Ok(Json(body).into_response())
    };
    match tokio::time::timeout(remaining, work).await {
        Err(_elapsed) => {
            tracing::warn!(error_kind = "crm_timeout", "crm workspace timed out");
            timeout_response()
        }
        Ok(Ok(resp)) => resp,
        Ok(Err(resp)) => resp,
    }
}

fn cached_response(hit: &CachedWorkspace) -> Response {
    let mut body = hit.body.clone();
    body.cached = true;
    Json(body).into_response()
}

fn forbidden_record() -> Response {
    error_json(StatusCode::FORBIDDEN, "forbidden_record")
}

async fn read_engagements(
    client: &HubSpotClient,
    et: EngagementType,
    ids: &[String],
) -> Result<Vec<HubSpotRecord>, HubSpotError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    client
        .batch_read(et.api_name(), ids, engagement_props(et))
        .await
}

async fn read_records(
    client: &HubSpotClient,
    object: &str,
    ids: &[String],
    props: &[&str],
) -> Result<Vec<HubSpotRecord>, HubSpotError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    client.batch_read(object, ids, props).await
}

/// 案件の関連 (担当者・会社) のラベルを付ける。v3 の型名を関連ラベルの定義で直す。
/// 定義を読めない・直せない型名があるときだけ、従来の v4 の関連 (ラベル付き) を読み直す (+1 回)。
/// v4 も失敗したら、ラベル無しのまま `associations` を `partial` に出す (ID は v3 のまま使う)
async fn label_refs(
    client: &HubSpotClient,
    deal_id: &str,
    to: &str,
    refs: &mut [AssociationRef],
    defs: Option<&[crate::hubspot::AssociationLabelDef]>,
) -> Result<(), HubSpotError> {
    if refs.is_empty() {
        return Ok(());
    }
    if let Some(defs) = defs {
        let resolved: Option<Vec<Vec<String>>> = refs
            .iter()
            .map(|r| resolve_labels(&r.type_names, defs))
            .collect();
        if let Some(all) = resolved {
            for (r, labels) in refs.iter_mut().zip(all) {
                r.labels = labels;
            }
            return Ok(());
        }
        tracing::warn!(
            to,
            "crm workspace: unrecognized association type; reading labelled associations"
        );
    }
    let mut m = client
        .batch_associations("deals", to, &[deal_id.to_string()])
        .await?;
    let v4 = m.remove(deal_id).unwrap_or_default();
    for r in refs.iter_mut() {
        if let Some(x) = v4.iter().find(|x| x.id == r.id) {
            r.labels = x.labels.clone();
        }
    }
    Ok(())
}

/// 1 画面分を HubSpot から読む。`gate_owner` があれば (役割が決まっていない人)、案件を読んだ直後に
/// レコード単位の関門を通し、外れたらそれ以降の HubSpot 呼び出しをしない。
/// 戻り値の 2 つ目は案件の本体 (キャッシュに入れて、キャッシュを返すときの関門に使う)。
async fn build(
    client: &HubSpotClient,
    ctx: &CrmCtx,
    gate_owner: Option<&str>,
    id: &str,
    mut selected: SelectedProps,
) -> Result<(WorkspaceResponse, HubSpotRecord), Response> {
    let is_gated = gate_owner.is_some();
    let mut partials: Vec<WorkspacePartial> = Vec::new();
    // 選んだ項目は一覧 (許可リスト) で確かめる。一覧に無い名前は 400。一覧を取れなければ選んだ項目は読まない
    if !selected.is_empty() {
        match ctx.catalog.get(client).await {
            Ok((catalog, _)) => {
                let unknown = [
                    ("deals", &selected.deal),
                    ("contacts", &selected.contact),
                    ("companies", &selected.company),
                ]
                .iter()
                .any(|(o, names)| !catalog.unknown(o, names).is_empty());
                if unknown {
                    return Err(error_json(StatusCode::BAD_REQUEST, "invalid_properties"));
                }
            }
            Err(e) => {
                partials.push(partial("selected_properties", &e));
                selected = SelectedProps::default();
            }
        }
    }
    let mut base_deal: Vec<&str> = DEAL_PROPERTIES.to_vec();
    base_deal.extend(DETAIL_ONLY_DEAL_PROPS);
    let deal_props = with_extra(&base_deal, &selected.deal);
    let contact_props = with_extra(CONTACT_PROPS, &selected.contact);
    let company_props = with_extra(COMPANY_PROPS, &selected.company);

    // 1) 案件の本体 + 活動・担当者・会社の関連 ID (1 回)。関連ラベルの定義とステージ名 (どちらもキャッシュ) と並列。
    //    メールのスコープが無く 401/403 になったときだけ、メール抜きで読み直す
    let mut all_types: Vec<&str> = ENGAGEMENTS.iter().map(|e| e.api_name()).collect();
    all_types.extend(["contacts", "companies"]);
    let read_deal = async {
        match client
            .get_object_with_associations("deals", id, &deal_props, &all_types)
            .await
        {
            Err(e @ HubSpotError::Auth { .. }) => {
                let no_email: Vec<&str> = all_types
                    .iter()
                    .copied()
                    .filter(|t| *t != "emails")
                    .collect();
                client
                    .get_object_with_associations("deals", id, &deal_props, &no_email)
                    .await
                    .map(|v| (v, Some(e)))
            }
            other => other.map(|v| (v, None)),
        }
    };
    let (first, label_defs, labels) = tokio::join!(
        read_deal,
        ctx.assoc_labels.get(client),
        ctx.queue.stage_labels(client)
    );
    let ((deal, mut assocs), email_err) = match first {
        Ok(v) => v,
        Err(e) => return Err(deal_error(&e, is_gated)),
    };
    if let Some(e) = &email_err {
        partials.push(partial("emails", e));
    }
    if deal.archived {
        return Err(deal_error(&HubSpotError::NotFound, is_gated));
    }
    // 関門: 外れたら本文は返さず、これ以降の HubSpot 呼び出しもしない
    if let Some(owner) = gate_owner {
        if !deal_in_queue(&deal, owner, jst_today_ms(ctx.queue.now())) {
            return Err(forbidden_record());
        }
    }
    let portal = hubspot_portal_id();
    let stage_labels = match labels {
        Ok(m) => m,
        Err(e) => {
            partials.push(partial("stage_labels", &e));
            HashMap::new()
        }
    };

    // 担当者・会社の関連 (#1 で読んだ ID)。ラベルは定義から付ける (直せなければ v4 を読み直す)
    let (mut contact_refs, contacts_more) = assocs.remove("contacts").unwrap_or_default();
    let (mut company_refs, _) = assocs.remove("companies").unwrap_or_default();
    let (c_defs, co_defs) = match &label_defs {
        Ok(d) => (Some(d.contacts.as_slice()), Some(d.companies.as_slice())),
        Err(e) => {
            tracing::warn!(
                error_kind = e.error_kind(),
                "crm workspace: association label definitions unavailable"
            );
            (None, None)
        }
    };
    let (c_lab, co_lab) = tokio::join!(
        label_refs(client, &deal.id, "contacts", &mut contact_refs, c_defs),
        label_refs(client, &deal.id, "companies", &mut company_refs, co_defs)
    );
    if let Some(e) = c_lab.err().or(co_lab.err()) {
        partials.push(partial("associations", &e));
    }
    let contacts_total = contact_refs.len() as u32;
    let companies_total = company_refs.len() as u32;
    let contact_order = ordered_refs(&contact_refs, PRIMARY_CONTACT_LABELS, MAX_CONTACTS);
    let company_order = ordered_refs(&company_refs, PRIMARY_COMPANY_LABELS, MAX_COMPANIES);
    let contact_ids: Vec<String> = contact_order.iter().map(|r| r.id.clone()).collect();
    let company_ids: Vec<String> = company_order.iter().map(|r| r.id.clone()).collect();
    // 通話を辿る担当者 (主を先頭に、関連の全員から MAX_CONTACTS_FOR_CALLS 人)
    let call_contacts: Vec<String> = ordered_refs(
        &contact_refs,
        PRIMARY_CONTACT_LABELS,
        MAX_CONTACTS_FOR_CALLS,
    )
    .into_iter()
    .map(|r| r.id)
    .collect();
    let mut activities_truncated = contacts_more || contact_refs.len() > MAX_CONTACTS_FOR_CALLS;

    // 活動の ID を型ごとに集める (案件直付き。重複除去)。通話は担当者経由の分を後で足す
    struct Pick {
        et: EngagementType,
        ids: Vec<String>,
        via: HashMap<String, (&'static str, String)>,
    }
    let mut picks: Vec<Pick> = Vec::new();
    for et in ENGAGEMENTS {
        let (refs, more) = assocs.remove(et.api_name()).unwrap_or_default();
        activities_truncated |= more;
        let mut ids: Vec<String> = Vec::new();
        let mut via: HashMap<String, (&'static str, String)> = HashMap::new();
        for r in refs {
            if !via.contains_key(&r.id) {
                via.insert(r.id.clone(), ("deal", deal.id.clone()));
                ids.push(r.id);
            }
        }
        picks.push(Pick { et, ids, via });
    }
    let cap = |ids: &mut Vec<String>, truncated: &mut bool| {
        if ids.len() > MAX_ENGAGEMENTS_PER_TYPE {
            *truncated = true;
            sort_ids_newest_first(ids);
            ids.truncate(MAX_ENGAGEMENTS_PER_TYPE);
        }
    };
    // 通話は担当者経由の関連が分かってから 1 回でまとめて読む (担当者がいなければ #2 で読む)
    let calls_wait_for_contacts = !call_contacts.is_empty();
    for p in picks.iter_mut() {
        if p.et != EngagementType::Call || !calls_wait_for_contacts {
            cap(&mut p.ids, &mut activities_truncated);
        }
    }
    let empty: Vec<String> = Vec::new();
    let ids_now = |et: EngagementType| -> &Vec<String> {
        if et == EngagementType::Call && calls_wait_for_contacts {
            &empty
        } else {
            &picks.iter().find(|p| p.et == et).expect("pick").ids
        }
    };

    // 2) 担当者・会社の読み取り、担当者 → 通話、メモ・メール・ミーティング (と担当者がいなければ通話) を並列
    let (c_read, co_read, c_calls, r_calls, r_notes, r_emails, r_meetings) = tokio::join!(
        read_records(client, "contacts", &contact_ids, &contact_props),
        read_records(client, "companies", &company_ids, &company_props),
        async {
            if call_contacts.is_empty() {
                Ok(Default::default())
            } else {
                client
                    .batch_associations("contacts", "calls", &call_contacts)
                    .await
            }
        },
        read_engagements(client, EngagementType::Call, ids_now(EngagementType::Call)),
        read_engagements(client, EngagementType::Note, ids_now(EngagementType::Note)),
        read_engagements(
            client,
            EngagementType::Email,
            ids_now(EngagementType::Email)
        ),
        read_engagements(
            client,
            EngagementType::Meeting,
            ids_now(EngagementType::Meeting)
        ),
    );
    let contact_recs: HashMap<String, HubSpotRecord> = match c_read {
        Ok(v) => v
            .into_iter()
            .filter(|r| !r.archived)
            .map(|r| (r.id.clone(), r))
            .collect(),
        Err(e) => {
            partials.push(partial("contacts", &e));
            HashMap::new()
        }
    };
    let company_recs: HashMap<String, HubSpotRecord> = match co_read {
        Ok(v) => v
            .into_iter()
            .filter(|r| !r.archived)
            .map(|r| (r.id.clone(), r))
            .collect(),
        Err(e) => {
            partials.push(partial("companies", &e));
            HashMap::new()
        }
    };

    // 3) 通話 (案件直付き + 担当者経由)。担当者がいるときだけ、ここで 1 回まとめて読む
    let r_calls = if calls_wait_for_contacts {
        let contact_calls: Vec<(String, String)> = match c_calls {
            Ok(map) => {
                let mut v = Vec::new();
                for cid in &call_contacts {
                    for r in map.get(cid).into_iter().flatten() {
                        v.push((r.id.clone(), cid.clone()));
                    }
                }
                v
            }
            Err(e) => {
                partials.push(partial("calls_via_contacts", &e));
                Vec::new()
            }
        };
        let p = picks
            .iter_mut()
            .find(|p| p.et == EngagementType::Call)
            .expect("calls pick");
        for (call_id, cid) in contact_calls {
            if !p.via.contains_key(&call_id) {
                p.via.insert(call_id.clone(), ("contact", cid));
                p.ids.push(call_id);
            }
        }
        cap(&mut p.ids, &mut activities_truncated);
        read_engagements(client, EngagementType::Call, &p.ids).await
    } else {
        r_calls
    };

    let results = [
        (EngagementType::Call, r_calls),
        (EngagementType::Note, r_notes),
        (EngagementType::Email, r_emails),
        (EngagementType::Meeting, r_meetings),
    ];
    let mut activities: Vec<WorkspaceActivity> = Vec::new();
    for (et, res) in results {
        let pick_ = picks.iter().find(|p| p.et == et).expect("pick");
        match res {
            Ok(recs) => {
                for rec in recs.into_iter().filter(|r| !r.archived) {
                    let (via, via_id) = pick_
                        .via
                        .get(&rec.id)
                        .cloned()
                        .unwrap_or(("deal", deal.id.clone()));
                    activities.push(to_activity(et, &rec, via, via_id));
                }
            }
            Err(e) => partials.push(partial(et.api_name(), &e)),
        }
    }
    activities.sort_by(|a, b| {
        let ta = timestamp_millis(a.timestamp.as_deref());
        let tb = timestamp_millis(b.timestamp.as_deref());
        tb.cmp(&ta).then_with(|| a.id.cmp(&b.id))
    });
    if activities.len() > MAX_ACTIVITIES {
        activities_truncated = true;
        activities.truncate(MAX_ACTIVITIES);
    }

    // 応答の組み立て
    let contacts: Vec<WorkspaceContact> = contact_order
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let c = contact_recs.get(&r.id)?;
            Some(WorkspaceContact {
                id: c.id.clone(),
                name: contact_name(c),
                job_title: nz(c, "jobtitle"),
                phone: nz(c, "phone"),
                mobile: nz(c, "mobilephone"),
                email: nz(c, "email"),
                labels: r.labels.clone(),
                is_primary: i == 0,
                deep_link: record_url(&portal, RecordType::Contact, &c.id),
            })
        })
        .collect();
    let companies: Vec<WorkspaceCompany> = company_order
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let c = company_recs.get(&r.id)?;
            Some(WorkspaceCompany {
                id: c.id.clone(),
                name: nz(c, "name"),
                phone: nz(c, "phone"),
                address: join_address(c),
                industry: nz(c, "industry"),
                domain: nz(c, "domain"),
                website: nz(c, "website"),
                labels: r.labels.clone(),
                is_primary: i == 0,
                deep_link: record_url(&portal, RecordType::Company, &c.id),
            })
        })
        .collect();
    // 主 (先頭) が読めていないとき、2 人目を主として電話番号を選ばない (キューと同じ扱い)
    let primary_contact = contact_order.first().and_then(|r| contact_recs.get(&r.id));
    let primary_company = company_order.first().and_then(|r| company_recs.get(&r.id));
    let dial = [
        ("deal", nz(&deal, "bpo_29")),
        ("contact", primary_contact.and_then(|c| nz(c, "phone"))),
        ("mobile", primary_contact.and_then(|c| nz(c, "mobilephone"))),
        ("company", primary_company.and_then(|c| nz(c, "phone"))),
    ]
    .into_iter()
    .find_map(|(src, v)| {
        v.map(|number| WorkspaceDial {
            number,
            source: src.to_string(),
        })
    });

    let selected_out = WorkspaceSelected {
        deal: selected_values(Some(&deal), &selected.deal),
        contact: selected_values(primary_contact, &selected.contact),
        company: selected_values(primary_company, &selected.company),
    };
    let stage_id = nz(&deal, "dealstage");
    let wdeal = WorkspaceDeal {
        id: deal.id.clone(),
        name: nz(&deal, "dealname"),
        stage_label: stage_id.as_ref().and_then(|s| stage_labels.get(s).cloned()),
        stage_id,
        pipeline_id: nz(&deal, "pipeline"),
        owner_id: nz(&deal, "hubspot_owner_id"),
        amount: nz(&deal, "amount"),
        close_date: nz(&deal, "closedate"),
        next_call_date: nz(&deal, "bpo_13"),
        next_call_time: nz(&deal, "bpo_14"),
        last_call_date: nz(&deal, "bpo_20"),
        stop: WorkspaceStop {
            prohibited_reason: nz(&deal, "bpo_3"),
            block_reason: nz(&deal, "bpo_4"),
            unreachable_check: nz(&deal, "bpo_10"),
        },
        bpo_phone: nz(&deal, "bpo_29"),
        job_search_url: nz(&deal, "bpo_32"),
        homepage_url: nz(&deal, "website_url"),
        media_job_urls: nz(&deal, "recruit_media_observed_urls"),
        job_posting_url: nz(&deal, "risuto_jigyousyokibo"),
        deep_link: record_url(&portal, RecordType::Deal, &deal.id),
    };
    let fetched_at = ctx
        .workspace_cache
        .now()
        .to_rfc3339_opts(SecondsFormat::Secs, true);
    let body = WorkspaceResponse {
        deal: wdeal,
        dial,
        contacts,
        contacts_total,
        companies,
        companies_total,
        activities,
        activities_truncated,
        activity_scope: ACTIVITY_SCOPE.to_string(),
        partial: partials,
        selected: selected_out,
        hubspot_portal_id: portal,
        data_scope: DATA_SCOPE.to_string(),
        generated_at: ctx.queue.now().to_rfc3339_opts(SecondsFormat::Secs, true),
        fetched_at,
        cached: false,
    };
    Ok((body, deal))
}

/// 案件の取得の失敗を応答にする。関門のある人には「存在しない」も「担当外」も同じ 403 (id の存在を探らせない)
fn deal_error(e: &HubSpotError, is_gated: bool) -> Response {
    if is_gated && matches!(e, HubSpotError::NotFound) {
        return forbidden_record();
    }
    tracing::warn!(
        error_kind = e.error_kind(),
        "crm workspace: deal read failed"
    );
    hubspot_error_response(e)
}

fn to_activity(
    et: EngagementType,
    rec: &HubSpotRecord,
    via: &str,
    via_id: String,
) -> WorkspaceActivity {
    let ts = nz(rec, "hs_timestamp");
    let owner_id = nz(rec, "hubspot_owner_id");
    let mut a = WorkspaceActivity {
        id: rec.id.clone(),
        kind: et.as_str().to_string(),
        timestamp: ts,
        title: None,
        body: None,
        direction: None,
        status: None,
        duration_ms: None,
        owner_id,
        source: None,
        via: via.to_string(),
        via_id,
        rich: None,
    };
    let mut rich = ActivityRich::default();
    match et {
        EngagementType::Call => {
            a.title = text_prop(rec, "hs_call_title");
            a.body = text_prop(rec, "hs_call_body");
            a.direction = nz(rec, "hs_call_direction");
            a.status = nz(rec, "hs_call_status");
            a.duration_ms = nz(rec, "hs_call_duration")
                .and_then(|v| v.parse::<u64>().ok())
                .map(|v| v.min(u32::MAX as u64) as u32);
            a.source = nz(rec, "hs_call_source");
            rich.body_full =
                nz(rec, "hs_call_body").and_then(|v| plain_text(&v, MAX_FULL_TEXT_CHARS));
            rich.recording_url =
                nz(rec, "hs_call_recording_url").filter(|u| u.starts_with("https://"));
        }
        EngagementType::Note => {
            a.body = text_prop(rec, "hs_note_body");
            rich.body_html = html_prop(rec, "hs_note_body");
            rich.body_full =
                nz(rec, "hs_note_body").and_then(|v| plain_text(&v, MAX_FULL_TEXT_CHARS));
            rich.attachments_count = attachment_count(rec);
        }
        EngagementType::Email => {
            a.title = text_prop(rec, "hs_email_subject");
            a.body = text_prop(rec, "hs_email_text");
            a.direction = nz(rec, "hs_email_direction");
            a.status = nz(rec, "hs_email_status");
            rich.body_html = html_prop(rec, "hs_email_html");
            rich.body_full =
                nz(rec, "hs_email_text").and_then(|v| plain_text(&v, MAX_FULL_TEXT_CHARS));
            rich.from_email = nz(rec, "hs_email_from_email");
            let name = [
                nz(rec, "hs_email_from_lastname"),
                nz(rec, "hs_email_from_firstname"),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
            rich.from_name = (!name.is_empty()).then_some(name);
            rich.to = split_addresses(nz(rec, "hs_email_to_email"));
            rich.cc = split_addresses(nz(rec, "hs_email_cc_email"));
            rich.thread_id = nz(rec, "hs_email_thread_id");
            rich.attachments_count = attachment_count(rec);
        }
        EngagementType::Meeting => {
            a.title = text_prop(rec, "hs_meeting_title");
            a.body = text_prop(rec, "hs_meeting_body");
            a.status = nz(rec, "hs_meeting_outcome");
            rich.body_html = html_prop(rec, "hs_meeting_body");
            rich.body_full =
                nz(rec, "hs_meeting_body").and_then(|v| plain_text(&v, MAX_FULL_TEXT_CHARS));
            rich.start_time = nz(rec, "hs_meeting_start_time");
            rich.end_time = nz(rec, "hs_meeting_end_time");
            rich.location = text_prop(rec, "hs_meeting_location");
        }
        EngagementType::Task => {}
    }
    a.rich = Some(rich);
    a
}

/// HTML 本文を、上限で切って返す (描く前に画面側で無害化する)。タグを含まない値は返さない
fn html_prop(rec: &HubSpotRecord, key: &str) -> Option<String> {
    let v = nz(rec, key)?;
    if !v.contains('<') {
        return None;
    }
    Some(v.chars().take(MAX_HTML_CHARS).collect())
}

/// `a@x;b@y` や `a@x, b@y` を分けて空を除く
fn split_addresses(raw: Option<String>) -> Vec<String> {
    raw.map(|v| {
        v.split([';', ','])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

/// 添付ファイルの数 (`hs_attachment_ids` は `;` 区切りの ID)
fn attachment_count(rec: &HubSpotRecord) -> Option<u32> {
    let n = split_addresses(nz(rec, "hs_attachment_ids")).len();
    (n > 0).then_some(n as u32)
}

#[cfg(test)]
impl WorkspaceResponse {
    /// 中身を問わない最小の応答 (キャッシュの単体テスト用)
    pub(super) fn empty_for_test(deal_id: &str) -> Self {
        WorkspaceResponse {
            deal: WorkspaceDeal {
                id: deal_id.to_string(),
                name: None,
                stage_id: None,
                stage_label: None,
                pipeline_id: None,
                owner_id: None,
                amount: None,
                close_date: None,
                next_call_date: None,
                next_call_time: None,
                last_call_date: None,
                stop: WorkspaceStop {
                    prohibited_reason: None,
                    block_reason: None,
                    unreachable_check: None,
                },
                bpo_phone: None,
                job_search_url: None,
                homepage_url: None,
                media_job_urls: None,
                job_posting_url: None,
                deep_link: String::new(),
            },
            dial: None,
            contacts: Vec::new(),
            contacts_total: 0,
            companies: Vec::new(),
            companies_total: 0,
            activities: Vec::new(),
            activities_truncated: false,
            activity_scope: String::new(),
            partial: Vec::new(),
            selected: WorkspaceSelected::default(),
            hubspot_portal_id: String::new(),
            data_scope: String::new(),
            generated_at: String::new(),
            fetched_at: String::new(),
            cached: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 本文の_html_は平文にする() {
        let t = plain_text(
            "<p>こんにちは&nbsp;<b>太郎</b></p><p>A &amp; B &lt;x&gt;</p><br>末尾",
            600,
        )
        .unwrap();
        assert_eq!(t, "こんにちは 太郎\nA & B <x>\n末尾");
        assert_eq!(plain_text("   ", 10), None);
        assert_eq!(plain_text("<p></p>", 10), None);
    }

    #[test]
    fn 活動の読み取りにメールの本文と宛先の項目が入る() {
        // 追加の呼び出しはせず、既存の型ごとの一括読み取りの項目に足しているだけ
        let p = engagement_props(EngagementType::Email);
        for k in [
            "hs_email_html",
            "hs_email_from_email",
            "hs_email_to_email",
            "hs_email_cc_email",
            "hs_email_thread_id",
            "hs_attachment_ids",
        ] {
            assert!(p.contains(&k), "{k}");
        }
        assert!(engagement_props(EngagementType::Note).contains(&"hs_note_body"));
        assert!(engagement_props(EngagementType::Meeting).contains(&"hs_meeting_start_time"));
        assert!(engagement_props(EngagementType::Call).contains(&"hs_call_recording_url"));
        assert_eq!(ENGAGEMENTS.len(), 4);
    }

    #[test]
    fn メール活動は差出人と宛先を分けて持つ() {
        let mut props = std::collections::BTreeMap::new();
        for (k, v) in [
            ("hs_email_subject", "ご挨拶(架空)"),
            ("hs_email_html", "<p>本文</p><blockquote>引用</blockquote>"),
            ("hs_email_text", "本文\n引用"),
            ("hs_email_from_email", "taro@example.invalid"),
            ("hs_email_from_lastname", "山田"),
            ("hs_email_from_firstname", "太郎"),
            ("hs_email_to_email", "a@example.invalid; b@example.invalid"),
            ("hs_attachment_ids", "1;2"),
        ] {
            props.insert(k.to_string(), Some(v.to_string()));
        }
        let rec = HubSpotRecord {
            id: "1".into(),
            properties: props,
            created_at: None,
            updated_at: None,
            archived: false,
        };
        let a = to_activity(EngagementType::Email, &rec, "deal", "9".into());
        let r = a.rich.unwrap();
        assert_eq!(r.from_name.as_deref(), Some("山田 太郎"));
        assert_eq!(r.to, ["a@example.invalid", "b@example.invalid"]);
        assert_eq!(r.attachments_count, Some(2));
        assert!(r.body_html.unwrap().contains("blockquote"));
        assert_eq!(r.body_full.as_deref(), Some("本文\n引用"));
    }

    #[test]
    fn 長い本文は切って省略記号() {
        let t = plain_text(&"あ".repeat(700), 600).unwrap();
        assert_eq!(t.chars().count(), 601);
        assert!(t.ends_with('…'));
    }

    #[test]
    fn 閉じない角括弧は文字として残る() {
        assert_eq!(plain_text("1 < 2", 100).unwrap(), "1 < 2");
    }

    #[test]
    fn 主を先頭に並べて上限で切る() {
        let r = |id: &str, l: &[&str]| AssociationRef {
            id: id.to_string(),
            labels: l.iter().map(|s| s.to_string()).collect(),
            type_names: Vec::new(),
        };
        let refs = vec![r("1", &[]), r("2", &["主"]), r("3", &[])];
        let o = ordered_refs(&refs, PRIMARY_CONTACT_LABELS, 2);
        assert_eq!(
            o.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(),
            ["2", "1"]
        );
        // 主が無ければ最初の 1 人が先頭
        let refs = vec![r("5", &[]), r("6", &[])];
        assert_eq!(ordered_refs(&refs, PRIMARY_CONTACT_LABELS, 5)[0].id, "5");
        assert!(ordered_refs(&[], PRIMARY_CONTACT_LABELS, 5).is_empty());
    }
}
