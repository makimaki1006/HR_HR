//! 応募者からの折り返しの電話を Slack のチャンネルに知らせる (2026-11-20 の会議で決定)。
//!
//! 応募の画面のボタンから 1 件ずつ送る。ステージは動かさない。HubSpot にも DB にも書かない (記録は Render のログ)。
//!
//! - `APPLICANT_CALLBACK_SLACK_CHANNEL_ID`: 送り先のチャンネル ID。無ければ画面のボタンは押せない
//! - `TALK_REVIEW_SLACK_BOT_TOKEN`: 新規営業のアポ通知と同じ Slack Bot のトークン (`chat:write`)。Bot を送り先のチャンネルに入れておく
//! - `TALK_REVIEW_APP_URL`: 文に入れる HR_HR のリンクの頭 (既定 `https://hr-hw.onrender.com`)
//!
//! 送るのは 1 回だけで、自動では送り直さない (二重に投稿しないことを優先する)。
//! 同じ操作 ID の送り直しは 1 回にまとめる (プロセス内のメモリ。単一インスタンス前提)。
use crate::crm::talk_review::{slack::app_url_from, slack_bot};
use crate::crm::{rbac, routes::CrmCtx};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant},
};
use tower_sessions::Session;

const CHANNEL_ENV: &str = "APPLICANT_CALLBACK_SLACK_CHANNEL_ID";
const TOKEN_ENV: &str = "TALK_REVIEW_SLACK_BOT_TOKEN";
const MEMO_MAX: usize = 1000;
const LABEL_MAX: usize = 200;
/// 同じ操作 ID を覚えておく時間
const SEEN_TTL: Duration = Duration::from_secs(600);
const SEEN_MAX: usize = 1000;

fn env_text(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// (トークン, チャンネル)。どちらかが無い・形が違えば None
fn destination() -> Option<(String, String)> {
    let token = env_text(TOKEN_ENV).filter(|t| t.starts_with("xoxb-"))?;
    let channel = env_text(CHANNEL_ENV)?;
    Some((token, channel))
}

/// 折り返しを知らせられるか (画面がボタンを押せるかを決める)
pub(super) fn configured() -> bool {
    destination().is_some()
}

#[derive(Deserialize)]
pub(in crate::crm) struct CallbackNotice {
    /// 画面が入力欄を開くたびに作る ID。同じ ID の送り直しは 1 回にまとめる
    operation_id: String,
    #[serde(default)]
    memo: String,
    /// 画面に出ている応募者の名前・取引 (会社) の名前・求人の名前
    #[serde(default)]
    name: String,
    #[serde(default)]
    deal: String,
    #[serde(default)]
    job: String,
}

/// 操作 ID → (受け付けた時刻, 送り終えたか)
static SEEN: LazyLock<Mutex<HashMap<String, (Instant, bool)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

enum Claim {
    New,
    Sending,
    Sent,
}

fn claim(op: &str, now: Instant) -> Claim {
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    seen.retain(|_, (at, _)| now.duration_since(*at) < SEEN_TTL);
    match seen.get(op) {
        Some((_, true)) => Claim::Sent,
        Some((_, false)) => Claim::Sending,
        None => {
            if seen.len() < SEEN_MAX {
                seen.insert(op.to_string(), (now, false));
            }
            Claim::New
        }
    }
}

fn finish(op: &str, sent: bool) {
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if sent {
        if let Some(e) = seen.get_mut(op) {
            e.1 = true;
        }
    } else {
        seen.remove(op);
    }
}

/// Slack の文で特別な意味を持つ文字を置き換える
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn clip(s: &str, max: usize) -> String {
    s.trim().chars().take(max).collect()
}

fn message(id: &str, sender: &str, n: &CallbackNotice, app_url: &str) -> String {
    let or = |s: &str, empty: &str| {
        let s = clip(s, LABEL_MAX);
        if s.is_empty() {
            empty.to_string()
        } else {
            esc(&s)
        }
    };
    let mut lines = vec![
        "*応募者から折り返しの電話がありました*".to_string(),
        format!("応募者: {}", or(&n.name, "氏名未登録")),
        format!("会社: {}", or(&n.deal, "不明")),
        format!("求人: {}", or(&n.job, "不明")),
    ];
    let memo = clip(&n.memo, MEMO_MAX);
    if !memo.is_empty() {
        lines.push(format!("メモ:\n{}", esc(&memo)));
    }
    lines.push(format!("知らせた人: {}", esc(sender)));
    lines.push(format!(
        "<{app_url}/app/crm?applicant={id}|HR_HR で開く> ・ <{}|HubSpot で開く>",
        super::link(super::OBJECT, id)
    ));
    lines.join("\n")
}

fn reply(status: StatusCode, result: &str) -> Response {
    (status, Json(json!({ "result": result }))).into_response()
}

/// `POST /api/crm/applicants/{id}/callback`: 折り返しの電話を Slack に知らせる。
///
/// 応答の `result`: `sent` (送った) / `sending` (同じ操作を送っている途中) / `not_configured` (送り先が未設定) /
/// `failed` (送れなかった。何も送られていない) / `unknown` (送ったが、届いたか確かめられなかった)
pub(in crate::crm) async fn notify(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    session: Session,
    Path(id): Path<String>,
    Json(notice): Json<CallbackNotice>,
) -> Response {
    let principal = match rbac::authorize(&session, &state, &ctx.access, None).await {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let op = notice.operation_id.trim();
    if !crate::crm::routes::is_valid_id(&id) || op.is_empty() || op.len() > 64 {
        return reply(StatusCode::BAD_REQUEST, "invalid");
    }
    let Some((token, channel)) = destination() else {
        return reply(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    match claim(op, Instant::now()) {
        Claim::Sent => return reply(StatusCode::OK, "sent"),
        Claim::Sending => return reply(StatusCode::CONFLICT, "sending"),
        Claim::New => {}
    }
    let email = principal.email.unwrap_or_default().to_lowercase();
    let sender = email.split('@').next().unwrap_or(&email).to_string();
    let app_url = app_url_from(&|k: &str| std::env::var(k).ok());
    let body = json!({
        "channel": channel,
        "text": message(&id, &sender, &notice, &app_url),
        "unfurl_links": false,
    });
    let http = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(h) => h,
        Err(_) => {
            finish(op, false);
            return reply(StatusCode::BAD_GATEWAY, "failed");
        }
    };
    match slack_bot::chat_post(&http, slack_bot::SLACK_API, &token, &body).await {
        Ok(_) => {
            finish(op, true);
            tracing::info!(applicant = %id, sender = %sender, "applicant callback: slack に知らせた");
            reply(StatusCode::OK, "sent")
        }
        Err(e) => {
            finish(op, false);
            tracing::warn!(applicant = %id, sender = %sender, error = %e.reason, maybe_sent = e.maybe_sent, "applicant callback: slack に送れなかった");
            reply(
                StatusCode::BAD_GATEWAY,
                if e.maybe_sent { "unknown" } else { "failed" },
            )
        }
    }
}
