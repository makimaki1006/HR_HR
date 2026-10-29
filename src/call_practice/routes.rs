//! `/api/call-practice/*`。管理者だけ (Google ログイン + 会社ドメイン + 役割 admin)。
//! 認可で落ちる限り、LLM は呼ばない。

use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::{
    body::{Body, Bytes},
    extract::{Extension, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use http_body::Frame;
use serde_json::json;
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio::sync::Semaphore;
use tower_sessions::Session;

use super::persona;
use super::roleplay::{self, AnthropicClient, Customer, LlmError};
use super::score::{self, ScoreInput};
use super::session::{Begin, FinishErr, Store, MAX_MINUTES, MAX_TEXT_CHARS, MAX_TURNS};
use super::types::*;
use crate::crm::rbac::{self, CrmAccess};
use crate::crm::talk_review::config::TalkReviewConfig;
use crate::crm::talk_review::judge::{Completer, MiniMaxJudge};
use crate::AppState;

/// 同時に呼ぶ LLM の数 (本番の判定の枠を守る)
const MAX_LLM: usize = 2;
const SCORE_TIMEOUT: Duration = Duration::from_secs(90);
const DEFAULT_SCORE_MODEL: &str = "MiniMax-M3";

pub struct PracticeCtx {
    pub access: CrmAccess,
    pub store: Store,
    pub customer: Option<Arc<Customer>>,
    pub llm: Arc<Semaphore>,
}

impl PracticeCtx {
    pub fn new(access: CrmAccess, customer: Option<Customer>) -> Self {
        Self {
            access,
            store: Store::default(),
            customer: customer.map(Arc::new),
            llm: Arc::new(Semaphore::new(MAX_LLM)),
        }
    }
}

pub fn router(access: CrmAccess) -> Router<Arc<AppState>> {
    let customer = AnthropicClient::from_env().map(Customer::Anthropic);
    if customer.is_none() {
        tracing::warn!("ANTHROPIC_API_KEY が無いので、架電の練習のお客さん役は使えない");
    }
    let ctx = Arc::new(PracticeCtx::new(access, customer));
    Router::new()
        .route("/api/call-practice/config", get(get_config))
        .route(
            "/api/call-practice/sessions",
            post(post_start).route_layer(axum::middleware::from_fn(csrf_guard)),
        )
        .route("/api/call-practice/sessions/{id}", get(get_session))
        .route(
            "/api/call-practice/sessions/{id}/turns",
            post(post_turn).route_layer(axum::middleware::from_fn(csrf_guard)),
        )
        .route(
            "/api/call-practice/sessions/{id}/finish",
            post(post_finish).route_layer(axum::middleware::from_fn(csrf_guard)),
        )
        .layer(Extension(ctx))
}

async fn csrf_guard(request: axum::extract::Request, next: axum::middleware::Next) -> Response {
    if let Err(msg) = crate::check_csrf(&request) {
        return (StatusCode::FORBIDDEN, format!("Forbidden: {msg}")).into_response();
    }
    next.run(request).await
}

// ---------------------------------------------------------------------------
// 共通
// ---------------------------------------------------------------------------

type ApiErr = (StatusCode, &'static str);

/// 画面にそのまま出せるやさしい日本語
fn message_of(kind: &str) -> &'static str {
    match kind {
        "practice_unavailable" => "いまはお客さん役を呼び出せません。管理者に連絡してください。",
        "llm_busy" => "お客さん役が返事できませんでした。もう一度送ってください。",
        "refusal" => "お客さん役がこの場面を続けられませんでした。もう一度送ってください。",
        "session_not_found" => "練習が途切れました。もう一度始めてください。",
        "session_finished" => "この練習は終わっています。",
        "session_closed" => "お客さん役が会話を閉じました。記録を見直して終えてください。",
        "turn_in_progress" => "お客さん役が返事をしています。少し待ってから送ってください。",
        "sessions_full" => "いま練習が混み合っています。少し待ってからもう一度始めてください。",
        "invalid_input" => "入力を確かめてください。",
        "forbidden" => "この画面は管理者だけが使えます。",
        _ => "うまく動きませんでした。もう一度試してください。",
    }
}

fn error_json(status: StatusCode, kind: &str) -> Response {
    (
        status,
        Json(json!({"error_kind": kind, "message": message_of(kind)})),
    )
        .into_response()
}

fn err_response((status, kind): ApiErr) -> Response {
    error_json(status, kind)
}

#[allow(clippy::result_large_err)]
async fn admin_gate(
    session: &Session,
    state: &Arc<AppState>,
    ctx: &PracticeCtx,
) -> Result<String, Response> {
    let p = rbac::authorize(session, state, &ctx.access, None)
        .await
        .map_err(IntoResponse::into_response)?;
    if !rbac::resolve_role(&p).is_admin() {
        return Err(error_json(StatusCode::FORBIDDEN, "forbidden"));
    }
    Ok(p.email.unwrap_or_default())
}

/// チャンネルから流すだけの本文 (SSE)
struct ChanBody(UnboundedReceiver<Bytes>);

impl http_body::Body for ChanBody {
    type Data = Bytes;
    type Error = Infallible;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        self.0.poll_recv(cx).map(|o| o.map(|b| Ok(Frame::data(b))))
    }
}

fn sse(event: &str, data: serde_json::Value) -> Bytes {
    Bytes::from(format!("event: {event}\ndata: {data}\n\n"))
}

// ---------------------------------------------------------------------------
// 設定・開始・取り直し
// ---------------------------------------------------------------------------

pub fn config_of(ctx: &PracticeCtx) -> PracticeConfig {
    let enabled = ctx.customer.is_some();
    PracticeConfig {
        enabled,
        unavailable_reason: (!enabled).then(|| message_of("practice_unavailable").to_string()),
        max_turns: MAX_TURNS,
        max_minutes: MAX_MINUTES,
    }
}

async fn get_config(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<PracticeCtx>>,
    session: Session,
) -> Response {
    if let Err(r) = admin_gate(&session, &state, &ctx).await {
        return r;
    }
    Json(config_of(&ctx)).into_response()
}

pub fn start_session(
    ctx: &PracticeCtx,
    owner: &str,
    req: &PracticeStartRequest,
) -> Result<PracticeStartResponse, ApiErr> {
    if ctx.customer.is_none() {
        return Err((StatusCode::SERVICE_UNAVAILABLE, "practice_unavailable"));
    }
    let p = match req.persona_id.as_deref() {
        Some(id) => persona::find(id).ok_or((StatusCode::BAD_REQUEST, "invalid_input"))?,
        None => persona::pick_random(),
    };
    let (id, opening) = ctx
        .store
        .create(owner, p, req.script_seen)
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "sessions_full"))?;
    Ok(PracticeStartResponse {
        session_id: id,
        opening,
        max_turns: MAX_TURNS,
    })
}

async fn post_start(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<PracticeCtx>>,
    session: Session,
    body: Option<Json<PracticeStartRequest>>,
) -> Response {
    let owner = match admin_gate(&session, &state, &ctx).await {
        Ok(o) => o,
        Err(r) => return r,
    };
    let req = body.map(|Json(b)| b).unwrap_or_default();
    match start_session(&ctx, &owner, &req) {
        Ok(r) => Json(r).into_response(),
        Err(e) => err_response(e),
    }
}

async fn get_session(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<PracticeCtx>>,
    session: Session,
    Path(id): Path<String>,
) -> Response {
    let owner = match admin_gate(&session, &state, &ctx).await {
        Ok(o) => o,
        Err(r) => return r,
    };
    match ctx.store.view(&id, &owner) {
        Some(v) => Json(v).into_response(),
        None => error_json(StatusCode::NOT_FOUND, "session_not_found"),
    }
}

// ---------------------------------------------------------------------------
// 発言
// ---------------------------------------------------------------------------

/// 発言を受け付け、お客さん役の返事を SSE (`delta` `done` `error`) で流す入れ物を返す
pub async fn start_turn(
    ctx: &Arc<PracticeCtx>,
    id: &str,
    owner: &str,
    req: PracticeTurnRequest,
) -> Result<UnboundedReceiver<Bytes>, ApiErr> {
    let text = req.text.trim().to_string();
    let cid = req.client_turn_id.trim().to_string();
    if text.is_empty() || text.chars().count() > MAX_TEXT_CHARS || cid.is_empty() || cid.len() > 64
    {
        return Err((StatusCode::BAD_REQUEST, "invalid_input"));
    }
    let Some(customer) = ctx.customer.clone() else {
        return Err((StatusCode::SERVICE_UNAVAILABLE, "practice_unavailable"));
    };
    let plan = match ctx.store.begin_turn(id, owner, &cid, &text) {
        Begin::NotFound => return Err((StatusCode::NOT_FOUND, "session_not_found")),
        Begin::Finished => return Err((StatusCode::CONFLICT, "session_finished")),
        Begin::Closed => return Err((StatusCode::CONFLICT, "session_closed")),
        Begin::InProgress => return Err((StatusCode::CONFLICT, "turn_in_progress")),
        Begin::Replay(resp) => {
            let (tx, rx) = mpsc::unbounded_channel();
            let _ = tx.send(sse("done", json!(resp)));
            return Ok(rx);
        }
        Begin::Ready(p) => p,
    };
    // 待たせず断る (本番の判定の枠を守る)
    let Ok(permit) = ctx.llm.clone().try_acquire_owned() else {
        ctx.store.abort_turn(id);
        return Err((StatusCode::SERVICE_UNAVAILABLE, "llm_busy"));
    };
    let (out_tx, out_rx) = mpsc::unbounded_channel::<Bytes>();
    let ctx2 = ctx.clone();
    let id = id.to_string();
    tokio::spawn(async move {
        let _permit = permit;
        let (dtx, mut drx) = mpsc::unbounded_channel::<String>();
        let fwd_tx = out_tx.clone();
        let fwd = tokio::spawn(async move {
            while let Some(t) = drx.recv().await {
                let _ = fwd_tx.send(sse("delta", json!({"text": t})));
            }
        });
        let res = customer
            .stream_reply(&plan.system, &plan.messages, &dtx)
            .await;
        drop(dtx);
        let _ = fwd.await;
        let fail = |kind: &str| {
            ctx2.store.abort_turn(&id);
            let _ = out_tx.send(sse(
                "error",
                json!({"error_kind": kind, "message": message_of(kind)}),
            ));
        };
        match res {
            Ok(full) => {
                let reply = roleplay::parse_reply(&full);
                if reply.text.is_empty() {
                    return fail("llm_busy");
                }
                match ctx2.store.complete_turn(&id, &plan, reply) {
                    Some(resp) => {
                        let _ = out_tx.send(sse("done", json!(resp)));
                    }
                    None => fail("session_not_found"),
                }
            }
            Err(LlmError::Refusal) => fail("refusal"),
            Err(LlmError::Failed(e)) => {
                tracing::warn!("架電の練習: お客さん役の呼び出しに失敗: {e}");
                fail("llm_busy")
            }
        }
    });
    Ok(out_rx)
}

async fn post_turn(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<PracticeCtx>>,
    session: Session,
    Path(id): Path<String>,
    Json(req): Json<PracticeTurnRequest>,
) -> Response {
    let owner = match admin_gate(&session, &state, &ctx).await {
        Ok(o) => o,
        Err(r) => return r,
    };
    match start_turn(&ctx, &id, &owner, req).await {
        Ok(rx) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .header("X-Accel-Buffering", "no")
            .body(Body::new(ChanBody(rx)))
            .unwrap_or_else(|_| error_json(StatusCode::INTERNAL_SERVER_ERROR, "internal")),
        Err(e) => err_response(e),
    }
}

// ---------------------------------------------------------------------------
// 終える・採点
// ---------------------------------------------------------------------------

fn record_valid(r: &HearingRecord) -> bool {
    [
        &r.hiring_3m,
        &r.hiring_later,
        &r.issue,
        &r.urgency,
        &r.consider,
        &r.budget,
        &r.decider,
        &r.attend,
    ]
    .iter()
    .all(|f| {
        matches!(f.mode.as_str(), "value" | "not_heard" | "empty") && f.value.chars().count() <= 200
    })
}

/// 採点して結果を保存する。`completer` が無い・使えないときは規則の部分だけで返し、理由を添える
pub async fn finish_session<C: Completer>(
    ctx: &PracticeCtx,
    id: &str,
    owner: &str,
    req: PracticeFinishRequest,
    completer: Option<&C>,
) -> Result<PracticeResult, ApiErr> {
    if !record_valid(&req.record) || !["hot", "mixed", "cold"].contains(&req.heat_guess.as_str()) {
        return Err((StatusCode::BAD_REQUEST, "invalid_input"));
    }
    let snap = match ctx.store.snapshot_for_finish(id, owner) {
        Ok(s) => s,
        Err(FinishErr::NotFound) => return Err((StatusCode::NOT_FOUND, "session_not_found")),
        Err(FinishErr::InProgress) => return Err((StatusCode::CONFLICT, "turn_in_progress")),
        Err(FinishErr::Done(r)) => return Ok(*r),
    };
    let inp = ScoreInput {
        persona: snap.persona,
        transcript: &snap.transcript,
        marks: &snap.marks,
        irritated_turns: &snap.irritated_turns,
        question_count: snap.question_count,
        fee_told: snap.fee_told,
        budget_at_ask: snap.budget_at_ask.clone(),
        script_seen: snap.script_seen,
    };
    let mut commentary = None;
    let mut issue_match = None;
    let note = match (completer, ctx.llm.clone().try_acquire_owned()) {
        (None, _) => Some("コメントはいま付けられません。".to_string()),
        (Some(_), Err(_)) => {
            Some("いま混み合っているので、コメントは付けられませんでした。".to_string())
        }
        (Some(c), Ok(_permit)) => {
            match tokio::time::timeout(SCORE_TIMEOUT, score::commentary_via(c, &inp, &req)).await {
                Ok(Ok((com, im))) => {
                    commentary = Some(com);
                    issue_match = im;
                    None
                }
                _ => Some("コメントを作れませんでした。数字の部分は上のとおりです。".to_string()),
            }
        }
    };
    let result = score::build_result(&inp, &req, commentary, note, issue_match);
    ctx.store.store_result(id, result.clone());
    Ok(result)
}

async fn post_finish(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<PracticeCtx>>,
    session: Session,
    Path(id): Path<String>,
    Json(req): Json<PracticeFinishRequest>,
) -> Response {
    let owner = match admin_gate(&session, &state, &ctx).await {
        Ok(o) => o,
        Err(r) => return r,
    };
    let model = std::env::var("CALL_PRACTICE_SCORE_MODEL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_SCORE_MODEL.to_string());
    let base = TalkReviewConfig::from_env().minimax_base_url;
    let judge = MiniMaxJudge::new(&base, &model);
    match finish_session(&ctx, &id, &owner, req, judge.as_ref()).await {
        Ok(r) => Json(r).into_response(),
        Err(e) => err_response(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call_practice::roleplay::FakeCustomer;
    use crate::crm::talk_review::judge::JudgeError;
    use serde_json::Value;

    fn ctx_with(replies: Vec<Result<String, LlmError>>) -> Arc<PracticeCtx> {
        Arc::new(PracticeCtx::new(
            CrmAccess::from_list(""),
            Some(Customer::Fake(FakeCustomer::new(replies))),
        ))
    }

    async fn drain(mut rx: UnboundedReceiver<Bytes>) -> Vec<(String, Value)> {
        let mut raw = String::new();
        while let Some(b) = rx.recv().await {
            raw.push_str(std::str::from_utf8(&b).unwrap());
        }
        raw.split("\n\n")
            .filter(|b| !b.trim().is_empty())
            .map(|b| {
                let ev = b
                    .lines()
                    .next()
                    .unwrap()
                    .trim_start_matches("event: ")
                    .to_string();
                let data = b.lines().nth(1).unwrap().trim_start_matches("data: ");
                (ev, serde_json::from_str(data).unwrap())
            })
            .collect()
    }

    fn treq(cid: &str, text: &str) -> PracticeTurnRequest {
        PracticeTurnRequest {
            client_turn_id: cid.into(),
            text: text.into(),
            source: "voice".into(),
        }
    }

    #[tokio::test]
    async fn 返事は文字を流し最後に番号つきの完了を返す_印の行は流さない() {
        let ctx = ctx_with(vec![Ok(
            "ドライバーを2名ですね\n###{\"asked_slots\":[\"hiring_3m\"],\"is_question\":true,\"mood\":\"normal\",\"end\":false}".into(),
        )]);
        let r = start_session(
            &ctx,
            "a@f-a-c.co.jp",
            &PracticeStartRequest {
                persona_id: Some("hot_01".into()),
                script_seen: false,
            },
        )
        .unwrap();
        assert_eq!(r.opening.text, "はい、ではその日でお願いします");
        assert_eq!(r.max_turns, 30);
        let rx = start_turn(
            &ctx,
            &r.session_id,
            "a@f-a-c.co.jp",
            treq("t1", "直近3か月以内で、採用が必要な職種はありますか？"),
        )
        .await
        .unwrap();
        let ev = drain(rx).await;
        let streamed: String = ev
            .iter()
            .filter(|(e, _)| e == "delta")
            .map(|(_, d)| d["text"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(streamed.trim_end(), "ドライバーを2名ですね");
        let (name, done) = ev.last().unwrap();
        assert_eq!(name, "done");
        assert_eq!(done["rep_turn"]["n"], 2);
        assert_eq!(done["customer_turn"]["n"], 3);
        assert_eq!(done["customer_turn"]["text"], "ドライバーを2名ですね");
        assert_eq!(done["ended"], false);
        // 隠れた答えは練習の取り直しの応答に含まれない
        let v = serde_json::to_string(&ctx.store.view(&r.session_id, "a@f-a-c.co.jp").unwrap())
            .unwrap();
        assert!(!v.contains("hot_01") && !v.contains("熱い型") && !v.contains("社長"));
        // 同じ発言番号の再送は LLM を呼ばず前の完了を返す (偽の返事は 1 つしか無い)
        let rx = start_turn(
            &ctx,
            &r.session_id,
            "a@f-a-c.co.jp",
            treq("t1", "直近3か月以内で、採用が必要な職種はありますか？"),
        )
        .await
        .unwrap();
        let ev = drain(rx).await;
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].1["customer_turn"]["n"], 3);
    }

    #[tokio::test]
    async fn 失敗したら発言を取り消してやさしい文言を返す() {
        let ctx = ctx_with(vec![
            Err(LlmError::Failed("http 529".into())),
            Err(LlmError::Refusal),
        ]);
        let r = start_session(&ctx, "a", &PracticeStartRequest::default()).unwrap();
        let ev = drain(
            start_turn(&ctx, &r.session_id, "a", treq("1", "こんにちは"))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(ev[0].0, "error");
        assert_eq!(ev[0].1["error_kind"], "llm_busy");
        assert_eq!(
            ev[0].1["message"],
            "お客さん役が返事できませんでした。もう一度送ってください。"
        );
        // 発言は残っていない (やり取りは最初の一言だけ)
        assert_eq!(
            ctx.store.view(&r.session_id, "a").unwrap().transcript.len(),
            1
        );
        let ev = drain(
            start_turn(&ctx, &r.session_id, "a", treq("2", "こんにちは"))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(ev[0].1["error_kind"], "refusal");
    }

    #[tokio::test]
    async fn 鍵が無いときは使えない旨を返し不正な入力と他人の練習を断る() {
        let none = Arc::new(PracticeCtx::new(CrmAccess::from_list(""), None));
        let c = config_of(&none);
        assert!(!c.enabled);
        assert_eq!(
            c.unavailable_reason.as_deref(),
            Some("いまはお客さん役を呼び出せません。管理者に連絡してください。")
        );
        assert_eq!(
            start_session(&none, "a", &PracticeStartRequest::default())
                .unwrap_err()
                .1,
            "practice_unavailable"
        );
        let ctx = ctx_with(vec![]);
        let r = start_session(&ctx, "a", &PracticeStartRequest::default()).unwrap();
        assert_eq!(
            start_turn(&ctx, &r.session_id, "a", treq("1", "  "))
                .await
                .unwrap_err()
                .1,
            "invalid_input"
        );
        assert_eq!(
            start_turn(&ctx, &r.session_id, "b", treq("1", "はい"))
                .await
                .unwrap_err()
                .1,
            "session_not_found"
        );
        assert_eq!(
            start_session(
                &ctx,
                "a",
                &PracticeStartRequest {
                    persona_id: Some("zzz".into()),
                    script_seen: false
                }
            )
            .unwrap_err()
            .1,
            "invalid_input"
        );
    }

    #[tokio::test]
    async fn 同時に呼ぶ_llm_は2本まで() {
        let ctx = ctx_with(vec![]);
        let a = ctx.llm.clone().try_acquire_owned().unwrap();
        let b = ctx.llm.clone().try_acquire_owned().unwrap();
        let r = start_session(&ctx, "a", &PracticeStartRequest::default()).unwrap();
        let e = start_turn(&ctx, &r.session_id, "a", treq("1", "こんにちは"))
            .await
            .unwrap_err();
        assert_eq!(e, (StatusCode::SERVICE_UNAVAILABLE, "llm_busy"));
        drop((a, b));
        // 断った発言は残らず、あとで受けられる
        assert!(ctx
            .store
            .begin_turn(&r.session_id, "a", "2", "こんにちは")
            .is_ready());
    }

    struct FakeC;
    impl Completer for FakeC {
        async fn complete(&self, _p: &str) -> Result<Value, JudgeError> {
            Ok(
                json!({"summary":"金額を直接聞かず、月額定額を先に伝えるとよいです","good":[],"bad":[{"turn":2,"comment":"金額を直接聞いています","better":"弊社は月額定額のサービスです"}],"issue_match":null}),
            )
        }
    }
    struct DownC;
    impl Completer for DownC {
        async fn complete(&self, _p: &str) -> Result<Value, JudgeError> {
            Err(JudgeError::Transient("http 429".into()))
        }
    }

    fn fld(mode: &str, v: &str) -> RecordField {
        RecordField {
            mode: mode.into(),
            value: v.into(),
        }
    }
    fn finish_req(guess: &str) -> PracticeFinishRequest {
        PracticeFinishRequest {
            record: HearingRecord {
                hiring_3m: fld("value", "ドライバー 2名"),
                hiring_later: fld("not_heard", ""),
                issue: fld("not_heard", ""),
                urgency: fld("not_heard", ""),
                consider: fld("not_heard", ""),
                budget: fld("not_heard", ""),
                decider: fld("not_heard", ""),
                attend: fld("not_heard", ""),
            },
            heat_guess: guess.into(),
        }
    }

    #[tokio::test]
    async fn 終えると採点が返り終えた練習は同じ結果を返す() {
        let ctx = ctx_with(vec![Ok(
            "ドライバーを2名ですね\n###{\"asked_slots\":[\"hiring_3m\"],\"is_question\":true,\"mood\":\"normal\",\"end\":false}".into(),
        )]);
        let r = start_session(
            &ctx,
            "a",
            &PracticeStartRequest {
                persona_id: Some("hot_01".into()),
                script_seen: true,
            },
        )
        .unwrap();
        drain(
            start_turn(&ctx, &r.session_id, "a", treq("1", "職種はありますか"))
                .await
                .unwrap(),
        )
        .await;
        let res = finish_session(&ctx, &r.session_id, "a", finish_req("hot"), Some(&FakeC))
            .await
            .unwrap();
        let st: Vec<&str> = res.items.iter().map(|i| i.status.as_str()).collect();
        assert_eq!(st, ["got", "not_heard", "not_heard", "not_heard"]);
        assert_eq!(res.not_heard_left, 3);
        assert_eq!(res.question_count, 1);
        assert_eq!(res.persona.label, "熱い型");
        assert!(res.heat.guess_matches_truth);
        assert_eq!(res.heat.hot_marks_heard, 1);
        assert!(res.script_seen);
        assert_eq!(res.commentary.as_ref().unwrap().bad[0].turn, 2);
        assert!(res.commentary_note.is_none());
        let again = finish_session(&ctx, &r.session_id, "a", finish_req("cold"), None::<&FakeC>)
            .await
            .unwrap();
        assert_eq!(again, res);
        assert_eq!(
            start_turn(&ctx, &r.session_id, "a", treq("9", "もう一つ"))
                .await
                .unwrap_err()
                .1,
            "session_finished"
        );
    }

    #[tokio::test]
    async fn 採点のコメントが作れなくても数字の部分は返す() {
        let ctx = ctx_with(vec![]);
        let r = start_session(
            &ctx,
            "a",
            &PracticeStartRequest {
                persona_id: Some("cold_01".into()),
                script_seen: false,
            },
        )
        .unwrap();
        let res = finish_session(&ctx, &r.session_id, "a", finish_req("cold"), Some(&DownC))
            .await
            .unwrap();
        assert!(res.commentary.is_none());
        assert_eq!(
            res.commentary_note.as_deref(),
            Some("コメントを作れませんでした。数字の部分は上のとおりです。")
        );
        assert_eq!(res.persona.id, "cold_01");
        assert_eq!(res.items[0].status, "recorded_without_answer");
        let r2 = start_session(&ctx, "a", &PracticeStartRequest::default()).unwrap();
        let bad = PracticeFinishRequest {
            heat_guess: "boiling".into(),
            ..finish_req("hot")
        };
        assert_eq!(
            finish_session(&ctx, &r2.session_id, "a", bad, None::<&FakeC>)
                .await
                .unwrap_err()
                .1,
            "invalid_input"
        );
    }
}
