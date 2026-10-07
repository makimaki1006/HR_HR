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
//! ## HubSpot 呼び出し回数 (1 回の要求あたり。鍵は営業自動化バッチと共有で 100 req/10 秒の枠を食うため上限を固定)
//! | # | 呼び出し | 回数 |
//! |---|---|---|
//! | 1 | 案件の本体 + 通話/メモ/ミーティング/メールの関連 ID (`GET deals/{id}?associations=..`) | 1 |
//! | 2 | 案件 → 担当者・案件 → 会社 の関連 (ラベル付き。`batch_associations` を並列) | 2 |
//! | 3 | 担当者の読み取り・会社の読み取り・担当者 → 通話 の関連・ステージ名 (並列) | 3 (+1: ステージ名が冷えているとき) |
//! | 4 | 活動の型ごとの `batch_read` (通話・メモ・メール・ミーティング。ID がある型だけ) | 最大 4 |
//!
//! 合計は最大 11 回。通常は 10 回以下。メールの読み取りスコープが
//! 共有鍵に無く 401/403 になったときだけ、メール抜きでもう 1 回 (#1) 読み直す (+1、`partial` に `emails` を出す)。
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

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::SecondsFormat;
use serde::Serialize;
use tower_sessions::Session;
use ts_rs::TS;

use super::call_queue::{contact_name, deal_in_queue, jst_today_ms, nz, pick, DEAL_PROPERTIES};
use super::rbac::{self, CrmRole};
use super::routes::{
    error_json, hubspot_error_response, is_valid_id, sort_ids_newest_first, timeout_response,
    timestamp_millis, CrmCtx, CRM_REQUEST_DEADLINE,
};
use crate::hubspot::deep_link::{hubspot_portal_id, record_url};
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
    "name", "phone", "address", "city", "state", "zip", "industry", "domain",
];
const CALL_PROPS: &[&str] = &[
    "hs_timestamp",
    "hs_call_title",
    "hs_call_body",
    "hs_call_direction",
    "hs_call_status",
    "hs_call_duration",
    "hs_call_source",
    "hubspot_owner_id",
];
const NOTE_PROPS: &[&str] = &["hs_timestamp", "hs_note_body", "hubspot_owner_id"];
const EMAIL_PROPS: &[&str] = &[
    "hs_timestamp",
    "hs_email_subject",
    "hs_email_text",
    "hs_email_direction",
    "hs_email_status",
    "hubspot_owner_id",
];
const MEETING_PROPS: &[&str] = &[
    "hs_timestamp",
    "hs_meeting_title",
    "hs_meeting_body",
    "hs_meeting_outcome",
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
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WorkspacePartial {
    /// `contacts` / `companies` / `associations` / `stage_labels` / `calls_via_contacts` / `calls` / `notes` / `emails` / `meetings`
    pub part: String,
    /// `HubSpotError::error_kind()` の値
    pub error_kind: String,
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
    pub hubspot_portal_id: String,
    pub data_scope: String,
    pub generated_at: String,
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

pub(super) async fn get_workspace_deal(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    Path(id): Path<String>,
) -> Response {
    // 1) 認可 (設定有無より先)。通らなければ HubSpot を呼ばない
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
    // 3) HubSpot 設定
    let Some(client) = state.hubspot.clone() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    // 4) 同時実行の枠待ちも含めて全体に締め切りを付ける
    let started = std::time::Instant::now();
    let _slot = match tokio::time::timeout(CRM_REQUEST_DEADLINE, ctx.read_slots.acquire()).await {
        Ok(Ok(permit)) => permit,
        _ => {
            tracing::warn!(
                error_kind = "crm_timeout",
                "crm workspace waited too long for a slot"
            );
            return timeout_response();
        }
    };
    let email = principal.email.clone().unwrap_or_default();
    let remaining = CRM_REQUEST_DEADLINE.saturating_sub(started.elapsed());
    match tokio::time::timeout(remaining, build(&client, &ctx, role, &email, &id)).await {
        Err(_elapsed) => {
            tracing::warn!(error_kind = "crm_timeout", "crm workspace timed out");
            timeout_response()
        }
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(resp)) => resp,
    }
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

async fn build(
    client: &HubSpotClient,
    ctx: &CrmCtx,
    role: CrmRole,
    email: &str,
    id: &str,
) -> Result<WorkspaceResponse, Response> {
    // 管理者以外の全員 はレコード関門を通す (安全側)
    let is_bpo = !role.reads_all_records();
    // BPO: 自分の owner。引けなければ何も読ませない (全員分に倒さない)
    let bpo_owner = if is_bpo {
        match ctx.queue.owner_for(client, email).await {
            Ok(Some(o)) => Some(o),
            Ok(None) => return Err(error_json(StatusCode::FORBIDDEN, "owner_not_found")),
            Err(e) => return Err(hubspot_error_response(&e)),
        }
    } else {
        None
    };

    let mut partials: Vec<WorkspacePartial> = Vec::new();
    let mut deal_props: Vec<&str> = DEAL_PROPERTIES.to_vec();
    deal_props.extend(["amount", "closedate"]);

    // 1) 案件の本体 + 活動の関連 ID。メールのスコープが無く 401/403 になったときだけ、メール抜きで読み直す
    let all_types: Vec<&str> = ENGAGEMENTS.iter().map(|e| e.api_name()).collect();
    let first = client
        .get_object_with_associations("deals", id, &deal_props, &all_types)
        .await;
    let (deal, mut assocs) = match first {
        Ok(v) => v,
        Err(e @ HubSpotError::Auth { .. }) => {
            let no_email: Vec<&str> = all_types
                .iter()
                .copied()
                .filter(|t| *t != "emails")
                .collect();
            match client
                .get_object_with_associations("deals", id, &deal_props, &no_email)
                .await
            {
                Ok(v) => {
                    partials.push(partial("emails", &e));
                    v
                }
                Err(e2) => return Err(deal_error(&e2, is_bpo)),
            }
        }
        Err(e) => return Err(deal_error(&e, is_bpo)),
    };
    if deal.archived {
        return Err(deal_error(&HubSpotError::NotFound, is_bpo));
    }
    // BPO の関門: 外れたら本文は返さず、これ以降の HubSpot 呼び出しもしない
    if let Some(owner) = &bpo_owner {
        if !deal_in_queue(&deal, owner, jst_today_ms(ctx.queue.now())) {
            return Err(forbidden_record());
        }
    }
    let portal = hubspot_portal_id();

    // 2) 担当者・会社の関連 (ラベル付き)
    let deal_ids = vec![deal.id.clone()];
    let (c_assoc, co_assoc) = tokio::join!(
        client.batch_associations("deals", "contacts", &deal_ids),
        client.batch_associations("deals", "companies", &deal_ids)
    );
    let contact_refs: Vec<AssociationRef> = match c_assoc {
        Ok(mut m) => m.remove(&deal.id).unwrap_or_default(),
        Err(e) => {
            partials.push(partial("associations", &e));
            Vec::new()
        }
    };
    let company_refs: Vec<AssociationRef> = match co_assoc {
        Ok(mut m) => m.remove(&deal.id).unwrap_or_default(),
        Err(e) => {
            if !partials.iter().any(|p| p.part == "associations") {
                partials.push(partial("associations", &e));
            }
            Vec::new()
        }
    };
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
    let mut activities_truncated = contact_refs.len() > MAX_CONTACTS_FOR_CALLS;

    // 3) 担当者・会社の読み取り、担当者 → 通話、ステージ名 (互いに独立なので並列)
    let (c_read, co_read, c_calls, labels) = tokio::join!(
        async {
            if contact_ids.is_empty() {
                Ok(Vec::new())
            } else {
                client
                    .batch_read("contacts", &contact_ids, CONTACT_PROPS)
                    .await
            }
        },
        async {
            if company_ids.is_empty() {
                Ok(Vec::new())
            } else {
                client
                    .batch_read("companies", &company_ids, COMPANY_PROPS)
                    .await
            }
        },
        async {
            if call_contacts.is_empty() {
                Ok(Default::default())
            } else {
                client
                    .batch_associations("contacts", "calls", &call_contacts)
                    .await
            }
        },
        ctx.queue.stage_labels(client)
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
    let stage_labels = match labels {
        Ok(m) => m,
        Err(e) => {
            partials.push(partial("stage_labels", &e));
            HashMap::new()
        }
    };
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

    // 4) 活動。型ごとに ID を集め (案件直付き優先で重複除去)、上限を超えたら新しい方を残して batch_read
    struct Pick {
        et: EngagementType,
        ids: Vec<String>,
        via: HashMap<String, (&'static str, String)>,
    }
    let mut picks: Vec<Pick> = Vec::new();
    for et in ENGAGEMENTS {
        let (refs, more) = assocs.remove(et.api_name()).unwrap_or_default();
        activities_truncated |= more;
        let mut seen: HashSet<String> = HashSet::new();
        let mut ids: Vec<String> = Vec::new();
        let mut via: HashMap<String, (&'static str, String)> = HashMap::new();
        for r in refs {
            if seen.insert(r.id.clone()) {
                via.insert(r.id.clone(), ("deal", deal.id.clone()));
                ids.push(r.id);
            }
        }
        if et == EngagementType::Call {
            for (call_id, cid) in &contact_calls {
                if seen.insert(call_id.clone()) {
                    via.insert(call_id.clone(), ("contact", cid.clone()));
                    ids.push(call_id.clone());
                }
            }
        }
        if ids.len() > MAX_ENGAGEMENTS_PER_TYPE {
            activities_truncated = true;
            sort_ids_newest_first(&mut ids);
            ids.truncate(MAX_ENGAGEMENTS_PER_TYPE);
        }
        picks.push(Pick { et, ids, via });
    }
    let results = tokio::join!(
        read_engagements(client, picks[0].et, &picks[0].ids),
        read_engagements(client, picks[1].et, &picks[1].ids),
        read_engagements(client, picks[2].et, &picks[2].ids),
        read_engagements(client, picks[3].et, &picks[3].ids),
    );
    let results = [results.0, results.1, results.2, results.3];
    let mut activities: Vec<WorkspaceActivity> = Vec::new();
    for (pick_, res) in picks.iter().zip(results) {
        match res {
            Ok(recs) => {
                for rec in recs.into_iter().filter(|r| !r.archived) {
                    let (via, via_id) = pick_
                        .via
                        .get(&rec.id)
                        .cloned()
                        .unwrap_or(("deal", deal.id.clone()));
                    activities.push(to_activity(pick_.et, &rec, via, via_id));
                }
            }
            Err(e) => partials.push(partial(pick_.et.api_name(), &e)),
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
        deep_link: record_url(&portal, RecordType::Deal, &deal.id),
    };
    Ok(WorkspaceResponse {
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
        hubspot_portal_id: portal,
        data_scope: DATA_SCOPE.to_string(),
        generated_at: ctx.queue.now().to_rfc3339_opts(SecondsFormat::Secs, true),
    })
}

/// 案件の取得の失敗を応答にする。BPO は「存在しない」も「担当外」も同じ 403 (id の存在を探らせない)
fn deal_error(e: &HubSpotError, is_bpo: bool) -> Response {
    if is_bpo && matches!(e, HubSpotError::NotFound) {
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
    };
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
        }
        EngagementType::Note => {
            a.body = text_prop(rec, "hs_note_body");
        }
        EngagementType::Email => {
            a.title = text_prop(rec, "hs_email_subject");
            a.body = text_prop(rec, "hs_email_text");
            a.direction = nz(rec, "hs_email_direction");
            a.status = nz(rec, "hs_email_status");
        }
        EngagementType::Meeting => {
            a.title = text_prop(rec, "hs_meeting_title");
            a.body = text_prop(rec, "hs_meeting_body");
            a.status = nz(rec, "hs_meeting_outcome");
        }
        EngagementType::Task => {}
    }
    a
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
