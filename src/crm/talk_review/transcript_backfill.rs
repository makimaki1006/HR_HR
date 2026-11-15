//! #442 より前に判定した電話の文字起こしを、Zoom の録音が消える前に HubSpot の通話 (`hrhr_transcript`) へ
//! 一度だけ移す (移行)。本番のログで「移し終えた」を確かめたら、この処理は別の PR で外す。
//!
//! `hs_store::Stored` (#442) は、判定・ヒアリング・画面で文字起こしが要った通話だけを残す。#442 が本番に入った後に
//! 判定した通話は判定のときに残しているので、対象は `CUTOFF` より前の判定済みの通話 (増えない有限の集合) に限る。
//!
//! - 起動後に 1 回だけ、監査 Turso の判定済みの行 (`SINCE <= call_at < CUTOFF`) の通話 ID と録音 ID を 500 行ずつ読む。
//! - 古い電話から (録音が先に消える側から) HubSpot の有無を batch read で 50 件ずつ確かめ、空の通話だけ Zoom から取って PATCH する。
//!   Zoom の録音が消えていた (404)・文字起こしできない (12000/12001) 通話には印の文を書く (`hs_store::GONE_MARK` ほか)。
//!   印のある通話は次の起動でも「既にある」になり、Zoom を呼ばない (`hs_store::fetch_stored` も印を読んで Zoom を呼ばない)。
//! - 全部片付いたら (書けた・印を書いた・既にあった・諦めた)、その起動の間は Turso も HubSpot も読まない。
//! - Turso には書かない (表・列・UPDATE を足さない)。進み具合はプロセス内だけ。
//! - 一時的な失敗 (Zoom の 429 など・HubSpot の失敗・締め切り) は回数に数えず、その周回を止めて次の周回でやり直す。
//!   文字起こしがまだ・空のときだけ `MAX_NOT_READY` 回まで試して諦める (印は書かないので次の起動でまた試す)。

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use tokio::time::{timeout_at, Instant};

use super::hs_store::{save_mark, save_transcript, transcript_is_empty, TRANSCRIPT_PROP};
use super::store::JudgedRef;
use super::types::TalkTurn;
use super::zoom::{TranscriptSource, ZoomError, REC_PREFIX};
use crate::hubspot::HubSpotClient;

/// 対象の上の端 (この時刻より前の電話だけ)。#442 (文字起こしを HubSpot に残す) のマージは 2026-11-11T05:29Z。
/// その後のデプロイまでの間に判定した電話も入るよう、約 18 時間の余裕を取った。これより後の電話は
/// #442 の入った worker が判定したので、判定のときに残している
pub const CUTOFF: &str = "2026-11-12T00:00:00Z";
/// 対象の下の端。トークの振り返りの判定は 2026-10 から (これより前の行は無い)
pub const SINCE: &str = "2026-01-01T00:00:00Z";
/// 1 周に Zoom から取る本数の上限
pub const PER_CYCLE: usize = 80;
/// 1 周に HubSpot の有無を確かめる本数の上限 (batch read 40 回まで)。
/// 1 周 80 本が `CYCLE_BUDGET` に収まらなければ、締め切りで残りを次の周回に回す
pub const CHECK_PER_CYCLE: usize = 2_000;
/// HubSpot の有無を 1 回で確かめる本数。文字起こしの本文 (最大 65,000 字) ごと返るので、100 件より小さくする
pub const CHECK_CHUNK: usize = 50;
/// 文字起こしがまだ・空の通話を試す回数 (この起動の中で)
pub const MAX_NOT_READY: u32 = 3;
/// 1 周にかけてよい時間。過ぎたら残りは次の周回 (取り込み・判定を待たせない)
pub const CYCLE_BUDGET: Duration = Duration::from_secs(180);
/// 1 日 (UTC) にこの処理が Zoom を呼ぶ回数の上限。録音・文字起こしの API の rate limit の種類は
/// 公式の資料で確かめられなかったので、Zoom Phone でいちばん厳しい Heavy とみなす。会社の Zoom は Business+
/// (2026-11-15 ユーザー確認) で、Heavy は 10 回/秒・Heavy と Resource-intensive で共有の 1 日 30,000 回
/// (<https://developers.zoom.us/docs/api/rate-limits/>)。判定・接触の判定の分を残す。
/// 日の区切りは UTC 0:00 (日本時間 9:00) にする。同じページは日次の上限の区切りを明記していないが、
/// 書いてある唯一の区切り (会議の作成・更新の上限) が「00:00 UTC にリセット」なので、それに合わせる
/// (日本時間の日で数えると、日本時間 0〜9 時に使い切った分が Zoom の同じ日に二重に乗る)
pub const ZOOM_DAILY_BUDGET: u32 = 8_000;
/// 1 時間 (UTC の時) にこの処理が Zoom を呼ぶ回数の上限 (8,000 ÷ 24 ≈ 333 を切り上げ)。数えはプロセス内だけで
/// 再起動すると 0 に戻るが、この上限で、再起動しても前倒しで使えるのは 1 時間分までになる
pub const ZOOM_HOURLY_BUDGET: u32 = 340;
/// batch read がこの回数続けて失敗したら warn を出す (同じ 50 件で止まっていると分かるように)
pub const CHECK_FAIL_WARN: u32 = 3;

/// 1 周の結果 (ログとテスト用)
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StepReport {
    /// HubSpot に有無を確かめた本数
    pub checked: usize,
    /// もう HubSpot にあった (または通話が無い) 本数
    pub present: usize,
    /// Zoom から取って HubSpot に書いた本数
    pub saved: usize,
    /// 録音が無い・文字起こしできないので印の文を書いた本数
    pub marked: usize,
    /// 文字起こしがまだ・空のまま諦めた本数
    pub given_up: usize,
    /// 文字起こしがまだ・空で後に回した本数
    pub retry_later: usize,
    /// 一時的な失敗・締め切り・1 日の上限で周回を途中で止めたか
    pub stopped: bool,
    /// この周回で全部片付いたか
    pub finished: bool,
}

/// 起動してからの合計 (「移し終えた」のログ用)
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Totals {
    pub present: usize,
    pub saved: usize,
    pub marked: usize,
    pub given_up: usize,
    /// 録音 ID が無いので対象にしなかった行
    pub no_recording_id: usize,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Phase {
    #[default]
    NotLoaded,
    Running,
    Done,
}

#[derive(Debug)]
struct Ref {
    id: String,
    zid: String,
}

/// 移す通話の待ち行列 (プロセス内だけ)
#[derive(Debug, Default)]
pub struct TranscriptBackfill {
    phase: Phase,
    /// 有無をまだ確かめていない通話 (古い順)
    unchecked: VecDeque<Ref>,
    /// HubSpot に無いと分かった通話 (古い順)。Zoom から取る
    missing: VecDeque<Ref>,
    /// 文字起こしがまだ・空だった回数
    not_ready: HashMap<String, u32>,
    /// batch read が続けて失敗した回数
    check_failures: u32,
    /// Zoom を呼んだ回数 (UTC の日ごと)
    zoom_day: Option<(NaiveDate, u32)>,
    /// Zoom を呼んだ回数 (UTC の時ごと。UNIX 時刻 ÷ 3600)
    zoom_hour: Option<(i64, u32)>,
    totals: Totals,
}

/// Zoom を呼ぶ回数。通話ログ ID なら録音 ID を引く分と文字起こしの 2 回、録音 ID (`rec:`) なら 1 回
fn zoom_cost(zid: &str) -> u32 {
    if zid.starts_with(REC_PREFIX) {
        1
    } else {
        2
    }
}

enum Write {
    Turns(Vec<TalkTurn>),
    /// 印の文 (true = 文字起こしできない、false = 録音が無い)
    Mark(bool),
}

impl TranscriptBackfill {
    /// 判定済みの行を読む必要があるか (起動後の最初だけ)
    pub fn needs_load(&self) -> bool {
        self.phase == Phase::NotLoaded
    }

    /// 全部片付いたか (この起動の間は何も読まない)
    pub fn is_done(&self) -> bool {
        self.phase == Phase::Done
    }

    pub fn totals(&self) -> &Totals {
        &self.totals
    }

    /// 起動後に読んだ判定済みの行を入れる (古い順に並べたもの)。録音 ID の無い行は取れないので入れない
    pub fn load(&mut self, refs: VecDeque<JudgedRef>) {
        for r in refs {
            match r.zoom_call_log_id {
                Some(zid) if !r.hubspot_call_id.is_empty() => self.unchecked.push_back(Ref {
                    id: r.hubspot_call_id,
                    zid,
                }),
                _ => self.totals.no_recording_id += 1,
            }
        }
        self.phase = if self.remaining() == 0 {
            Phase::Done
        } else {
            Phase::Running
        };
    }

    /// まだ残っている本数
    pub fn remaining(&self) -> usize {
        self.unchecked.len() + self.missing.len()
    }

    /// Zoom を `cost` 回呼べるか (UTC の日・時の上限)。呼べるなら数える
    fn take_zoom(&mut self, now: DateTime<Utc>, cost: u32) -> bool {
        let day = now.date_naive();
        let hour = now.timestamp().div_euclid(3600);
        let used_day = match self.zoom_day {
            Some((d, n)) if d == day => n,
            _ => 0,
        };
        let used_hour = match self.zoom_hour {
            Some((h, n)) if h == hour => n,
            _ => 0,
        };
        if used_day + cost > ZOOM_DAILY_BUDGET || used_hour + cost > ZOOM_HOURLY_BUDGET {
            return false;
        }
        self.zoom_day = Some((day, used_day + cost));
        self.zoom_hour = Some((hour, used_hour + cost));
        true
    }

    /// 1 周。HubSpot は `client` の優先度のまま呼ぶ (worker は `Background` を渡す)。
    /// `deadline` を過ぎたら残りは次の周回。`now` は Zoom の日・時の上限に使う
    pub async fn step<Z: TranscriptSource + ?Sized>(
        &mut self,
        client: &HubSpotClient,
        zoom: &Z,
        deadline: Instant,
        now: DateTime<Utc>,
    ) -> StepReport {
        let mut rep = StepReport::default();
        if self.phase != Phase::Running {
            return rep;
        }
        self.check(client, deadline, &mut rep).await;
        if !rep.stopped {
            self.fetch_missing(client, zoom, deadline, now, &mut rep)
                .await;
        }
        if self.remaining() == 0 {
            self.phase = Phase::Done;
            self.not_ready.clear();
            rep.finished = true;
        }
        rep
    }

    /// 書く本数に足りるまで、HubSpot で空の通話を探す
    async fn check(&mut self, client: &HubSpotClient, deadline: Instant, rep: &mut StepReport) {
        while self.missing.len() < PER_CYCLE
            && rep.checked < CHECK_PER_CYCLE
            && !self.unchecked.is_empty()
        {
            if Instant::now() >= deadline {
                rep.stopped = true;
                return;
            }
            let n = CHECK_CHUNK.min(self.unchecked.len());
            let ids: Vec<String> = self
                .unchecked
                .iter()
                .take(n)
                .map(|r| r.id.clone())
                .collect();
            let read = timeout_at(
                deadline,
                client.batch_read("calls", &ids, &[TRANSCRIPT_PROP]),
            )
            .await;
            let recs = match read {
                Ok(Ok(r)) => r,
                Ok(Err(e)) => {
                    self.check_failures += 1;
                    if self.check_failures >= CHECK_FAIL_WARN {
                        tracing::warn!(
                            error = %e.error_kind(),
                            failures = self.check_failures,
                            "talk review: transcript backfill check keeps failing on the same calls"
                        );
                    } else {
                        tracing::info!(error = %e.error_kind(), "talk review: transcript backfill check failed");
                    }
                    // 確かめられなかった分は待ち行列の先頭に元の順のまま残し、次の周回でやり直す
                    rep.stopped = true;
                    return;
                }
                Err(_) => {
                    rep.stopped = true;
                    return;
                }
            };
            self.check_failures = 0;
            let empty: HashSet<&str> = recs
                .iter()
                .filter(|r| {
                    r.properties
                        .get(TRANSCRIPT_PROP)
                        .cloned()
                        .flatten()
                        .is_none_or(|s| s.trim().is_empty())
                })
                .map(|r| r.id.as_str())
                .collect();
            rep.checked += n;
            for r in self.unchecked.drain(..n) {
                // batch read に出てこない通話 (消された) は書けないので、あったものと同じく外す
                if empty.contains(r.id.as_str()) {
                    self.missing.push_back(r);
                } else {
                    rep.present += 1;
                    self.totals.present += 1;
                }
            }
        }
    }

    /// 空の通話を Zoom から取って書く (古い順)
    async fn fetch_missing<Z: TranscriptSource + ?Sized>(
        &mut self,
        client: &HubSpotClient,
        zoom: &Z,
        deadline: Instant,
        now: DateTime<Utc>,
        rep: &mut StepReport,
    ) {
        let mut later = Vec::new();
        let mut done = 0;
        while done < PER_CYCLE {
            let Some(r) = self.missing.pop_front() else {
                break;
            };
            if Instant::now() >= deadline || !self.take_zoom(now, zoom_cost(&r.zid)) {
                self.missing.push_front(r);
                rep.stopped = true;
                break;
            }
            done += 1;
            let fetched = match timeout_at(deadline, zoom.fetch(&r.zid)).await {
                Ok(f) => f,
                Err(_) => Err(ZoomError::Transient("deadline".into())),
            };
            let write = match fetched {
                Ok(t) if !t.is_empty() => Write::Turns(t),
                Err(ZoomError::Gone) => Write::Mark(false),
                Err(ZoomError::Untranscribable) => Write::Mark(true),
                // 文字起こしがまだ・空
                Ok(_) | Err(ZoomError::NotReady) => {
                    let n = self.not_ready.entry(r.id.clone()).or_insert(0);
                    *n += 1;
                    if *n >= MAX_NOT_READY {
                        self.not_ready.remove(&r.id);
                        rep.given_up += 1;
                        self.totals.given_up += 1;
                    } else {
                        rep.retry_later += 1;
                        later.push(r);
                    }
                    continue;
                }
                // 混雑 (429)・通信・設定なし・締め切り: 回数に数えず、続けて呼ぶと悪くなるので周回を止める
                Err(e) => {
                    if let ZoomError::Transient(reason) = &e {
                        tracing::info!(%reason, "talk review: transcript backfill zoom failed");
                    }
                    self.missing.push_front(r);
                    rep.stopped = true;
                    break;
                }
            };
            // 印を書く直前に読み直す。「空」と確かめてから今までの間に、画面やヒアリング (`fetch_stored`) が
            // 本物の文字起こしを残していれば、印で上書きしない (上書きすると戻せない)
            if matches!(write, Write::Mark(_)) {
                match timeout_at(deadline, transcript_is_empty(client, &r.id)).await {
                    Ok(Ok(true)) => {}
                    Ok(Ok(false)) => {
                        self.not_ready.remove(&r.id);
                        rep.present += 1;
                        self.totals.present += 1;
                        continue;
                    }
                    failed => {
                        if let Ok(Err(e)) = failed {
                            tracing::info!(error = %e.error_kind(), "talk review: transcript backfill re-read failed");
                        }
                        self.missing.push_front(r);
                        rep.stopped = true;
                        break;
                    }
                }
            }
            let saved = match &write {
                Write::Turns(t) => timeout_at(deadline, save_transcript(client, &r.id, t)).await,
                Write::Mark(u) => timeout_at(deadline, save_mark(client, &r.id, *u)).await,
            };
            match saved {
                Ok(Ok(())) => {
                    self.not_ready.remove(&r.id);
                    if matches!(write, Write::Turns(_)) {
                        rep.saved += 1;
                        self.totals.saved += 1;
                    } else {
                        rep.marked += 1;
                        self.totals.marked += 1;
                    }
                }
                failed => {
                    if let Ok(Err(e)) = failed {
                        tracing::info!(error = %e.error_kind(), "talk review: transcript backfill save failed");
                    }
                    // 回数に数えず、次の周回で同じ通話からやり直す
                    self.missing.push_front(r);
                    rep.stopped = true;
                    break;
                }
            }
        }
        // 後に回した分は、待ち行列の最後に入れる
        self.missing.extend(later);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crm::talk_review::hs_store::{fetch_stored, GONE_MARK, UNTRANSCRIBABLE_MARK};
    use crate::hubspot::ClientOptions;
    use axum::extract::{Path, State};
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use serde_json::{json, Value};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    /// 偽 HubSpot の中身: 通話 ID → 文字起こし (空文字は「項目が空」)。無い ID は「通話が無い」
    #[derive(Default)]
    struct Hs {
        calls: Mutex<HashMap<String, String>>,
        batch_reads: Mutex<Vec<Vec<String>>>,
        patches: Mutex<Vec<String>>,
        fail_reads: AtomicBool,
        fail_patches: AtomicBool,
    }

    async fn batch_read(State(s): State<Arc<Hs>>, Json(body): Json<Value>) -> Response {
        let ids: Vec<String> = body["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap().to_string())
            .collect();
        s.batch_reads.lock().unwrap().push(ids.clone());
        if s.fail_reads.load(Ordering::SeqCst) {
            return (StatusCode::FORBIDDEN, Json(json!({"message": "no"}))).into_response();
        }
        let calls = s.calls.lock().unwrap();
        let results: Vec<Value> = ids
            .iter()
            .filter_map(|id| {
                let t = calls.get(id)?;
                let v = if t.is_empty() { Value::Null } else { json!(t) };
                Some(json!({"id": id, "properties": {TRANSCRIPT_PROP: v}}))
            })
            .collect();
        Json(json!({"status": "COMPLETE", "results": results})).into_response()
    }

    async fn get_call(State(s): State<Arc<Hs>>, Path(id): Path<String>) -> Json<Value> {
        let v = s
            .calls
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .unwrap_or_default();
        Json(json!({"id": id, "properties": {TRANSCRIPT_PROP: v}}))
    }

    async fn patch_call(
        State(s): State<Arc<Hs>>,
        Path(id): Path<String>,
        Json(body): Json<Value>,
    ) -> Response {
        if s.fail_patches.load(Ordering::SeqCst) {
            return (StatusCode::BAD_GATEWAY, "x").into_response();
        }
        let text = body["properties"][TRANSCRIPT_PROP]
            .as_str()
            .unwrap()
            .to_string();
        s.calls.lock().unwrap().insert(id.clone(), text.clone());
        s.patches.lock().unwrap().push(id.clone());
        Json(json!({"id": id, "properties": {TRANSCRIPT_PROP: text}})).into_response()
    }

    async fn start(hs: Arc<Hs>) -> HubSpotClient {
        let app = Router::new()
            .route("/crm/v3/objects/calls/batch/read", post(batch_read))
            .route(
                "/crm/v3/objects/calls/{id}",
                get(get_call).patch(patch_call),
            )
            .with_state(hs);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let opts = ClientOptions {
            max_retries: 0,
            ..ClientOptions::default()
        };
        HubSpotClient::new("fake-test-key".into(), &url, opts).unwrap()
    }

    type ZoomResult = Result<Vec<TalkTurn>, ZoomError>;

    /// 偽 Zoom: 録音 ID → 結果。呼ばれた録音 ID を数える。`delay` だけ待ってから答える
    #[derive(Default)]
    struct Zoom {
        results: Mutex<HashMap<String, ZoomResult>>,
        calls: Mutex<Vec<String>>,
        delay: Duration,
        /// 答える前に偽 HubSpot の通話へ文字起こしを書く (画面・ヒアリングが同時に残した場合を再現)
        on_fetch: Option<(Arc<Hs>, String, String)>,
    }
    impl Zoom {
        fn with(results: impl IntoIterator<Item = (String, ZoomResult)>) -> Self {
            Zoom {
                results: Mutex::new(results.into_iter().collect()),
                ..Default::default()
            }
        }
        fn ok_for(ids: &[String]) -> Self {
            Self::with(ids.iter().map(|i| (format!("z{i}"), Ok(turns()))))
        }
        fn called(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }
    impl TranscriptSource for Zoom {
        async fn fetch(&self, id: &str) -> Result<Vec<TalkTurn>, ZoomError> {
            self.calls.lock().unwrap().push(id.to_string());
            if let Some((hs, call, text)) = &self.on_fetch {
                hs.calls.lock().unwrap().insert(call.clone(), text.clone());
            }
            if !self.delay.is_zero() {
                tokio::time::sleep(self.delay).await;
            }
            self.results
                .lock()
                .unwrap()
                .get(id)
                .cloned()
                .unwrap_or(Err(ZoomError::Gone))
        }
        async fn find_recording(&self, _: &str, _: i64) -> Result<Option<String>, ZoomError> {
            Ok(None)
        }
    }

    fn z(id: &str, r: ZoomResult) -> (String, ZoomResult) {
        (id.to_string(), r)
    }

    fn turns() -> Vec<TalkTurn> {
        vec![
            TalkTurn {
                n: 1,
                speaker: "rep".into(),
                text: "お世話になっております".into(),
            },
            TalkTurn {
                n: 2,
                speaker: "customer".into(),
                text: "はい".into(),
            },
        ]
    }

    fn r(id: &str) -> JudgedRef {
        JudgedRef {
            hubspot_call_id: id.into(),
            zoom_call_log_id: Some(format!("z{id}")),
        }
    }

    fn refs<S: AsRef<str>>(ids: &[S]) -> VecDeque<JudgedRef> {
        ids.iter().map(|i| r(i.as_ref())).collect()
    }

    fn numbered(n: usize) -> Vec<String> {
        (1..=n).map(|i| i.to_string()).collect()
    }

    fn empty_calls<S: AsRef<str>>(hs: &Hs, ids: &[S]) {
        let mut c = hs.calls.lock().unwrap();
        for id in ids {
            c.insert(id.as_ref().to_string(), String::new());
        }
    }

    fn later() -> Instant {
        Instant::now() + Duration::from_secs(60)
    }

    fn day() -> DateTime<Utc> {
        at("2026-11-16T03:00:00Z")
    }

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn only_empty_calls_are_fetched_and_written_then_it_finishes() {
        let hs = Arc::new(Hs::default());
        empty_calls(&hs, &["1", "3"]);
        {
            let mut c = hs.calls.lock().unwrap();
            c.insert("2".into(), "営業: もう入っている".into());
            c.insert("4".into(), "  ".into());
            // 5 は HubSpot に通話が無い
        }
        let client = start(hs.clone()).await;
        let zoom = Zoom::ok_for(&numbered(4));
        let mut b = TranscriptBackfill::default();
        let mut all = refs(&["1", "2", "3", "4", "5"]);
        all.push_back(JudgedRef {
            hubspot_call_id: "6".into(),
            zoom_call_log_id: None,
        });
        b.load(all);
        let rep = b.step(&client, &zoom, later(), day()).await;
        assert_eq!(
            rep,
            StepReport {
                checked: 5,
                present: 2,
                saved: 3,
                finished: true,
                ..Default::default()
            }
        );
        // Zoom は空の 3 本だけ、入れた順 (古い順) に呼ぶ
        assert_eq!(zoom.called(), ["z1", "z3", "z4"]);
        assert_eq!(*hs.patches.lock().unwrap(), ["1", "3", "4"]);
        assert_eq!(
            hs.calls.lock().unwrap()["1"],
            "営業: お世話になっております\n先方: はい"
        );
        assert_eq!(hs.calls.lock().unwrap()["2"], "営業: もう入っている");
        assert_eq!(hs.batch_reads.lock().unwrap().len(), 1);
        assert!(b.is_done());
        assert_eq!(
            *b.totals(),
            Totals {
                present: 2,
                saved: 3,
                no_recording_id: 1,
                ..Default::default()
            }
        );
        // 片付いた後は HubSpot も Zoom も呼ばない
        let rep = b.step(&client, &zoom, later(), day()).await;
        assert_eq!(rep, StepReport::default());
        assert_eq!(zoom.called().len(), 3);
        assert_eq!(hs.batch_reads.lock().unwrap().len(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_cycle_stops_at_the_limit_and_continues_next_time() {
        let hs = Arc::new(Hs::default());
        let ids = numbered(PER_CYCLE + 10);
        empty_calls(&hs, &ids);
        let client = start(hs.clone()).await;
        let zoom = Zoom::ok_for(&ids);
        let mut b = TranscriptBackfill::default();
        b.load(refs(&ids));
        let rep = b.step(&client, &zoom, later(), day()).await;
        assert_eq!((rep.saved, rep.finished), (PER_CYCLE, false));
        // 80 本見つけるまで 50 件ずつ確かめる (50 + 40。残りの 10 本も同じ読み取りで確かめ済み)
        let sizes: Vec<usize> = hs
            .batch_reads
            .lock()
            .unwrap()
            .iter()
            .map(Vec::len)
            .collect();
        assert_eq!(sizes, [50, 40]);
        assert_eq!(b.remaining(), 10);
        assert_eq!(zoom.called()[0], "z1");
        assert_eq!(zoom.called()[PER_CYCLE - 1], format!("z{PER_CYCLE}"));
        let rep = b.step(&client, &zoom, later(), day()).await;
        assert_eq!((rep.saved, rep.checked, rep.finished), (10, 0, true));
        assert_eq!(hs.patches.lock().unwrap().len(), ids.len());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn gone_and_untranscribable_get_a_mark_and_zoom_is_not_called_after_a_restart() {
        let hs = Arc::new(Hs::default());
        empty_calls(&hs, &["1", "2", "3"]);
        let client = start(hs.clone()).await;
        let zoom = Zoom::with([
            z("z1", Err(ZoomError::Gone)),
            z("z2", Err(ZoomError::Untranscribable)),
            z("z3", Ok(turns())),
        ]);
        let mut b = TranscriptBackfill::default();
        b.load(refs(&["1", "2", "3"]));
        let rep = b.step(&client, &zoom, later(), day()).await;
        assert_eq!((rep.saved, rep.marked, rep.finished), (1, 2, true));
        assert_eq!(hs.calls.lock().unwrap()["1"], GONE_MARK);
        assert_eq!(hs.calls.lock().unwrap()["2"], UNTRANSCRIBABLE_MARK);
        // 再起動: 同じ行を読み直しても、全部「既にある」で Zoom を呼ばない
        let mut again = TranscriptBackfill::default();
        again.load(refs(&["1", "2", "3"]));
        let rep = again.step(&client, &zoom, later(), day()).await;
        assert_eq!((rep.present, rep.saved, rep.marked), (3, 0, 0));
        assert_eq!(zoom.called(), ["z1", "z2", "z3"]);
        // 判定・画面の読み方 (`fetch_stored`) も印を読んで Zoom を呼ばず、Zoom の 404・12000 と同じ誤りを返す
        // (画面の文言は「録音が残っていません」「短すぎる通話か、設定により文字起こしできません」のまま)
        assert_eq!(
            fetch_stored(&client, &zoom, "1", Some("z1")).await,
            Err(ZoomError::Gone)
        );
        assert_eq!(
            fetch_stored(&client, &zoom, "2", Some("z2")).await,
            Err(ZoomError::Untranscribable)
        );
        assert_eq!(zoom.called().len(), 3);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn not_ready_or_empty_is_tried_three_times_then_given_up_without_a_mark() {
        let hs = Arc::new(Hs::default());
        empty_calls(&hs, &["1", "2", "3"]);
        let client = start(hs.clone()).await;
        let zoom = Zoom::with([
            z("z1", Err(ZoomError::NotReady)),
            z("z2", Ok(vec![])),
            z("z3", Ok(turns())),
        ]);
        let mut b = TranscriptBackfill::default();
        b.load(refs(&["1", "2", "3"]));
        let rep = b.step(&client, &zoom, later(), day()).await;
        assert_eq!((rep.saved, rep.retry_later), (1, 2));
        b.step(&client, &zoom, later(), day()).await;
        let rep = b.step(&client, &zoom, later(), day()).await;
        assert_eq!((rep.given_up, rep.finished), (2, true));
        let count = |id: &str| zoom.called().iter().filter(|c| *c == id).count();
        assert_eq!((count("z1"), count("z2")), (3, 3));
        // 印は書かない (次の起動でまた試す)
        assert_eq!(hs.calls.lock().unwrap()["1"], "");
        assert_eq!(b.totals().given_up, 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_busy_zoom_stops_the_cycle_and_is_not_counted_as_an_attempt() {
        let hs = Arc::new(Hs::default());
        empty_calls(&hs, &["1", "2"]);
        let client = start(hs.clone()).await;
        let zoom = Zoom::with([
            z("z1", Err(ZoomError::Transient("http 429".into()))),
            z("z2", Ok(turns())),
        ]);
        let mut b = TranscriptBackfill::default();
        b.load(refs(&["1", "2"]));
        // 4 周続けて 429 でも諦めない (3 回の数に入れない)
        for _ in 0..4 {
            let rep = b.step(&client, &zoom, later(), day()).await;
            assert!(rep.stopped);
            assert_eq!((rep.saved, rep.given_up, rep.retry_later), (0, 0, 0));
        }
        assert_eq!(zoom.called(), ["z1"; 4]);
        assert_eq!(b.remaining(), 2);
        zoom.results
            .lock()
            .unwrap()
            .insert("z1".into(), Ok(turns()));
        let rep = b.step(&client, &zoom, later(), day()).await;
        assert_eq!((rep.saved, rep.finished), (2, true));
        assert_eq!(*hs.patches.lock().unwrap(), ["1", "2"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_failed_save_is_retried_next_cycle_without_counting() {
        let hs = Arc::new(Hs::default());
        empty_calls(&hs, &["1", "2"]);
        hs.fail_patches.store(true, Ordering::SeqCst);
        let client = start(hs.clone()).await;
        let zoom = Zoom::with([z("z1", Ok(turns())), z("z2", Err(ZoomError::Gone))]);
        let mut b = TranscriptBackfill::default();
        b.load(refs(&["1", "2"]));
        for _ in 0..4 {
            let rep = b.step(&client, &zoom, later(), day()).await;
            assert!(rep.stopped);
            assert_eq!(rep.saved + rep.given_up, 0);
        }
        assert_eq!(b.remaining(), 2);
        hs.fail_patches.store(false, Ordering::SeqCst);
        let rep = b.step(&client, &zoom, later(), day()).await;
        assert_eq!((rep.saved, rep.marked, rep.finished), (1, 1, true));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_failed_check_keeps_the_calls_in_the_original_order() {
        let hs = Arc::new(Hs::default());
        let ids = numbered(60);
        empty_calls(&hs, &ids);
        hs.fail_reads.store(true, Ordering::SeqCst);
        let client = start(hs.clone()).await;
        let zoom = Zoom::default();
        let mut b = TranscriptBackfill::default();
        b.load(refs(&ids));
        for _ in 0..CHECK_FAIL_WARN + 1 {
            let rep = b.step(&client, &zoom, later(), day()).await;
            assert!(rep.stopped);
            assert_eq!(rep.checked, 0);
        }
        assert_eq!(b.check_failures, CHECK_FAIL_WARN + 1);
        assert_eq!(b.remaining(), 60);
        // 毎回、同じ先頭の 50 件 (古い順) を読もうとしている
        let reads = hs.batch_reads.lock().unwrap().clone();
        assert!(reads.iter().all(|r| *r == ids[..50]));
        let order: Vec<&str> = b.unchecked.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(order, ids);
        assert!(zoom.called().is_empty());
        // 直れば続きから進み、失敗の数は戻る
        hs.fail_reads.store(false, Ordering::SeqCst);
        let zoom = Zoom::ok_for(&ids);
        b.step(&client, &zoom, later(), day()).await;
        assert_eq!(b.check_failures, 0);
        assert_eq!(zoom.called()[0], "z1");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_deadline_stops_a_cycle_and_the_next_one_continues() {
        let hs = Arc::new(Hs::default());
        let ids = numbered(10);
        empty_calls(&hs, &ids);
        let client = start(hs.clone()).await;
        let mut zoom = Zoom::ok_for(&ids);
        zoom.delay = Duration::from_millis(100);
        let mut b = TranscriptBackfill::default();
        b.load(refs(&ids));
        // 締め切りがもう過ぎていれば何も呼ばない
        let rep = b.step(&client, &zoom, Instant::now(), day()).await;
        assert!(rep.stopped);
        assert!(zoom.called().is_empty() && hs.batch_reads.lock().unwrap().is_empty());
        // 350ms の締め切り: 100ms ずつの Zoom を何本か呼んだところで止まる
        let rep = b
            .step(
                &client,
                &zoom,
                Instant::now() + Duration::from_millis(350),
                day(),
            )
            .await;
        assert!(rep.stopped);
        assert!(rep.saved >= 1 && rep.saved < 10, "{rep:?}");
        let first = rep.saved;
        // 次の周回で残りを続ける (書いた分は二度書かない)
        let rep = b.step(&client, &zoom, later(), day()).await;
        assert_eq!(first + rep.saved, 10);
        assert!(rep.finished);
        let mut patched = hs.patches.lock().unwrap().clone();
        patched.sort_by_key(|s| s.parse::<u32>().unwrap());
        assert_eq!(patched, ids);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_daily_zoom_budget_stops_until_the_next_day() {
        let hs = Arc::new(Hs::default());
        empty_calls(&hs, &["1", "2"]);
        let client = start(hs.clone()).await;
        let zoom = Zoom::ok_for(&numbered(2));
        let mut b = TranscriptBackfill::default();
        b.load(refs(&["1", "2"]));
        // 今日はあと 3 回だけ (通話ログ ID は 1 本 2 回)
        b.zoom_day = Some((day().date_naive(), ZOOM_DAILY_BUDGET - 3));
        let rep = b.step(&client, &zoom, later(), day()).await;
        assert_eq!((rep.saved, rep.stopped), (1, true));
        let rep = b.step(&client, &zoom, later(), day()).await;
        assert_eq!((rep.saved, rep.stopped), (0, true));
        let next = day() + chrono::Duration::days(1);
        let rep = b.step(&client, &zoom, later(), next).await;
        assert_eq!((rep.saved, rep.finished), (1, true));
        assert_eq!(zoom_cost("rec:abc"), 1);
        assert_eq!(zoom_cost("12345"), 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_hourly_zoom_budget_stops_until_the_next_hour() {
        let hs = Arc::new(Hs::default());
        empty_calls(&hs, &["1", "2"]);
        let client = start(hs.clone()).await;
        let zoom = Zoom::ok_for(&numbered(2));
        let mut b = TranscriptBackfill::default();
        b.load(refs(&["1", "2"]));
        // この時はあと 3 回だけ。日の上限には余裕がある
        b.zoom_hour = Some((day().timestamp().div_euclid(3600), ZOOM_HOURLY_BUDGET - 3));
        let rep = b.step(&client, &zoom, later(), day()).await;
        assert_eq!((rep.saved, rep.stopped), (1, true));
        let rep = b
            .step(
                &client,
                &zoom,
                later(),
                day() + chrono::Duration::minutes(59),
            )
            .await;
        assert_eq!((rep.saved, rep.stopped), (0, true));
        let rep = b
            .step(&client, &zoom, later(), day() + chrono::Duration::hours(1))
            .await;
        assert_eq!((rep.saved, rep.finished), (1, true));
    }

    #[test]
    fn the_daily_budget_is_counted_by_the_utc_day() {
        // 日本時間 11/16 0:30 (= UTC 11/15 15:30) に 1 日分を使い切った
        let mut b = TranscriptBackfill {
            zoom_day: Some((at("2026-11-15T15:30:00Z").date_naive(), ZOOM_DAILY_BUDGET)),
            ..Default::default()
        };
        // 日本時間 11/16 8:30 は UTC ではまだ同じ日なので呼べない (日本時間の日で数えると呼べてしまう)
        assert!(!b.take_zoom(at("2026-11-15T23:30:00Z"), 1));
        // UTC 0:00 (日本時間 9:00) を過ぎたら呼べる
        assert!(b.take_zoom(at("2026-11-16T00:10:00Z"), 2));
        assert_eq!(
            b.zoom_day,
            Some((at("2026-11-16T00:10:00Z").date_naive(), 2))
        );
        // 1 時間の上限: 同じ時にはそれ以上呼べない
        let mut b = TranscriptBackfill::default();
        let t = at("2026-11-16T05:00:00Z");
        for _ in 0..ZOOM_HOURLY_BUDGET / 2 {
            assert!(b.take_zoom(t, 2));
        }
        assert!(!b.take_zoom(t + chrono::Duration::minutes(59), 2));
        assert!(b.take_zoom(t + chrono::Duration::hours(1), 2));
        // 再起動 (数えが 0 に戻る) しても、前倒しで使えるのは 1 時間分まで
        let mut restarted = TranscriptBackfill::default();
        let used = (0..10_000)
            .take_while(|_| restarted.take_zoom(t, 1))
            .count();
        assert_eq!(used, ZOOM_HOURLY_BUDGET as usize);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_mark_does_not_overwrite_a_transcript_saved_in_the_meantime() {
        let hs = Arc::new(Hs::default());
        empty_calls(&hs, &["1", "2"]);
        let client = start(hs.clone()).await;
        let real = "営業: お世話になっております\n先方: はい".to_string();
        let mut zoom = Zoom::with([z("z1", Err(ZoomError::Gone)), z("z2", Err(ZoomError::Gone))]);
        // z1 を取りに行く間に、画面が通話 1 の本物の文字起こしを残した
        zoom.on_fetch = Some((hs.clone(), "1".into(), real.clone()));
        let mut b = TranscriptBackfill::default();
        b.load(refs(&["1", "2"]));
        let rep = b.step(&client, &zoom, later(), day()).await;
        assert_eq!(
            (rep.present, rep.marked, rep.finished),
            (1, 1, true),
            "{rep:?}"
        );
        assert_eq!(hs.calls.lock().unwrap()["1"], real);
        assert_eq!(hs.calls.lock().unwrap()["2"], GONE_MARK);
        assert_eq!(*hs.patches.lock().unwrap(), ["2"]);
    }

    #[test]
    fn nothing_to_move_is_done_right_away() {
        let mut b = TranscriptBackfill::default();
        assert!(b.needs_load());
        b.load(VecDeque::new());
        assert!(b.is_done() && !b.needs_load());
    }
}
