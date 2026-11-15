//! 判定済みの電話の文字起こしを、Zoom の録音が消える前に HubSpot の通話 (`hrhr_transcript`) へ移す。
//!
//! `hs_store::Stored` は、判定・ヒアリング・画面で文字起こしが要った通話だけを残す。触らなかった通話
//! (この仕組みより前に判定した電話) は、Zoom の録音が消えると失われるので、worker の手の空いた周回で先回りして移す。
//!
//! - 対象: 監査 Turso の判定済みの行 (`call_talk_reviews`、`status = 'judged'`)。起動後に 1 回だけ、
//!   通話 ID・時刻・録音 ID の 3 つの欄を 500 行ずつ読んでメモリに持つ (`store::for_each_judged_ref_page`)。
//!   この起動で判定した電話は判定のときに残しているので、読み直さない。
//! - 順: 新しい電話から。HubSpot の有無を batch read で 50 件ずつ確かめ、空の通話だけ Zoom から取って PATCH する。
//! - Turso には書かない (表・列・UPDATE を足さない)。進み具合はプロセス内だけ。再起動したら HubSpot の有無を見直すだけ。
//! - Zoom の 404 (録音なし)・12000/12001 (文字起こし不可) はこの起動では二度試さない。一時的な失敗は後の周回で
//!   (`MAX_ATTEMPTS` 回まで)。HubSpot・Zoom が混んでいる・設定が無いときは、その周回をそこで止める。

use std::collections::{HashMap, VecDeque};

use super::hs_store::{save_transcript, TRANSCRIPT_PROP};
use super::store::JudgedRef;
use super::zoom::{TranscriptSource, ZoomError};
use crate::hubspot::HubSpotClient;

/// 1 周に Zoom から取って HubSpot に書く本数の上限
pub const PER_CYCLE: usize = 60;
/// 1 周に HubSpot の有無を確かめる本数の上限 (batch read 40 回まで)
pub const CHECK_PER_CYCLE: usize = 2_000;
/// HubSpot の有無を 1 回で確かめる本数。文字起こしの本文 (最大 65,000 字) ごと返るので、100 件より小さくする
pub const CHECK_CHUNK: usize = 50;
/// 一時的な失敗を試し直す回数 (この起動の中で)
pub const MAX_ATTEMPTS: u32 = 3;
/// どこまで遡るか。監査 DB の行は 365 日で消える (`purge`) ので、表に残っている判定済みの行の全部
pub const SINCE_DAYS: i64 = 365;

/// 1 周の結果 (ログとテスト用)
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StepReport {
    /// HubSpot に有無を確かめた本数
    pub checked: usize,
    /// もう HubSpot にあった (または通話が無い) 本数
    pub present: usize,
    /// Zoom から取って HubSpot に書いた本数
    pub saved: usize,
    /// 録音が無い・文字起こしできない・録音 ID が無いので諦めた本数
    pub given_up: usize,
    /// 一時的な失敗で後に回した本数
    pub retry_later: usize,
    /// 混雑・設定なしで周回を途中で止めたか
    pub stopped: bool,
}

/// 移す通話の待ち行列 (プロセス内だけ)
#[derive(Debug, Default)]
pub struct TranscriptBackfill {
    /// 有無をまだ確かめていない通話 (新しい順)
    unchecked: VecDeque<JudgedRef>,
    /// HubSpot に無いと分かった通話 (新しい順)。Zoom から取る
    missing: VecDeque<JudgedRef>,
    /// 一時的な失敗の回数
    attempts: HashMap<String, u32>,
    /// 判定済みの行を読み終えたか
    loaded: bool,
}

impl TranscriptBackfill {
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// 起動後に読んだ判定済みの行を入れる (新しい順のまま)。録音 ID の無い行は取れないので入れない
    pub fn load(&mut self, refs: impl IntoIterator<Item = JudgedRef>) {
        self.unchecked.extend(
            refs.into_iter()
                .filter(|r| r.zoom_call_log_id.is_some() && !r.hubspot_call_id.is_empty()),
        );
        self.loaded = true;
    }

    /// まだ残っている本数
    pub fn remaining(&self) -> usize {
        self.unchecked.len() + self.missing.len()
    }

    /// 1 周。HubSpot は `client` の優先度のまま呼ぶ (worker は `Background` を渡す)
    pub async fn step<Z: TranscriptSource + ?Sized>(
        &mut self,
        client: &HubSpotClient,
        zoom: &Z,
    ) -> StepReport {
        let mut rep = StepReport::default();
        // 書く本数に足りるまで、空の通話を探す
        while self.missing.len() < PER_CYCLE
            && rep.checked < CHECK_PER_CYCLE
            && !self.unchecked.is_empty()
        {
            let n = CHECK_CHUNK.min(self.unchecked.len());
            let chunk: Vec<JudgedRef> = self.unchecked.drain(..n).collect();
            let ids: Vec<String> = chunk.iter().map(|r| r.hubspot_call_id.clone()).collect();
            let recs = match client.batch_read("calls", &ids, &[TRANSCRIPT_PROP]).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::info!(error = %e.error_kind(), "talk review: transcript backfill check failed");
                    // 確かめられなかった分は前に戻し、次の周回でやり直す
                    for r in chunk.into_iter().rev() {
                        self.unchecked.push_front(r);
                    }
                    rep.stopped = true;
                    return rep;
                }
            };
            rep.checked += chunk.len();
            let empty: std::collections::HashSet<&str> = recs
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
            for r in chunk {
                // batch read に出てこない通話 (消された) は書けないので、あったものと同じく外す
                if empty.contains(r.hubspot_call_id.as_str()) {
                    self.missing.push_back(r);
                } else {
                    rep.present += 1;
                }
            }
        }
        let mut later = Vec::new();
        let mut done = 0;
        while done < PER_CYCLE {
            let Some(r) = self.missing.pop_front() else {
                break;
            };
            done += 1;
            let Some(zid) = r.zoom_call_log_id.as_deref() else {
                rep.given_up += 1;
                continue;
            };
            let turns = match zoom.fetch(zid).await {
                Ok(t) if !t.is_empty() => t,
                Err(ZoomError::Gone | ZoomError::Untranscribable) => {
                    self.attempts.remove(&r.hubspot_call_id);
                    rep.given_up += 1;
                    continue;
                }
                Err(ZoomError::NotConfigured) => {
                    self.missing.push_front(r);
                    rep.stopped = true;
                    break;
                }
                Err(ZoomError::Transient(reason)) => {
                    // 混雑 (429) などは、続けて呼ぶと悪くなるので周回を止める
                    tracing::info!(%reason, "talk review: transcript backfill zoom failed");
                    self.retry(r, &mut later, &mut rep);
                    rep.stopped = true;
                    break;
                }
                // 文字起こしがまだ・空
                Ok(_) | Err(ZoomError::NotReady) => {
                    self.retry(r, &mut later, &mut rep);
                    continue;
                }
            };
            match save_transcript(client, &r.hubspot_call_id, &turns).await {
                Ok(()) => {
                    self.attempts.remove(&r.hubspot_call_id);
                    rep.saved += 1;
                }
                Err(e) => {
                    tracing::info!(error = %e.error_kind(), "talk review: transcript backfill save failed");
                    self.retry(r, &mut later, &mut rep);
                    rep.stopped = true;
                    break;
                }
            }
        }
        // 後に回した分は、待ち行列の最後 (古い電話の後) に入れる
        self.missing.extend(later);
        rep
    }

    fn retry(&mut self, r: JudgedRef, later: &mut Vec<JudgedRef>, rep: &mut StepReport) {
        let n = self.attempts.entry(r.hubspot_call_id.clone()).or_insert(0);
        *n += 1;
        if *n >= MAX_ATTEMPTS {
            self.attempts.remove(&r.hubspot_call_id);
            rep.given_up += 1;
        } else {
            rep.retry_later += 1;
            later.push(r);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crm::talk_review::types::TalkTurn;
    use crate::hubspot::ClientOptions;
    use axum::extract::{Path, State};
    use axum::routing::{patch, post};
    use axum::{Json, Router};
    use serde_json::{json, Value};
    use std::sync::{Arc, Mutex};

    /// 偽 HubSpot の中身: 通話 ID → 文字起こし (空文字は「項目が空」)。無い ID は「通話が無い」
    #[derive(Default)]
    struct Hs {
        calls: Mutex<HashMap<String, String>>,
        batch_reads: Mutex<Vec<usize>>,
        patches: Mutex<Vec<String>>,
    }

    async fn batch_read(State(s): State<Arc<Hs>>, Json(body): Json<Value>) -> Json<Value> {
        let ids: Vec<String> = body["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap().to_string())
            .collect();
        s.batch_reads.lock().unwrap().push(ids.len());
        let calls = s.calls.lock().unwrap();
        let results: Vec<Value> = ids
            .iter()
            .filter_map(|id| {
                let t = calls.get(id)?;
                let v = if t.is_empty() { Value::Null } else { json!(t) };
                Some(json!({"id": id, "properties": {TRANSCRIPT_PROP: v}}))
            })
            .collect();
        Json(json!({"status": "COMPLETE", "results": results}))
    }

    async fn patch_call(
        State(s): State<Arc<Hs>>,
        Path(id): Path<String>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        let text = body["properties"][TRANSCRIPT_PROP]
            .as_str()
            .unwrap()
            .to_string();
        s.calls.lock().unwrap().insert(id.clone(), text.clone());
        s.patches.lock().unwrap().push(id.clone());
        Json(json!({"id": id, "properties": {TRANSCRIPT_PROP: text}}))
    }

    async fn start(hs: Arc<Hs>) -> HubSpotClient {
        let app = Router::new()
            .route("/crm/v3/objects/calls/batch/read", post(batch_read))
            .route("/crm/v3/objects/calls/{id}", patch(patch_call))
            .with_state(hs);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        HubSpotClient::new("fake-test-key".into(), &url, ClientOptions::default()).unwrap()
    }

    /// 偽 Zoom: 録音 ID → 結果。呼ばれた録音 ID を数える
    #[derive(Default)]
    struct Zoom {
        results: HashMap<String, Result<Vec<TalkTurn>, ZoomError>>,
        calls: Mutex<Vec<String>>,
    }
    impl TranscriptSource for Zoom {
        async fn fetch(&self, id: &str) -> Result<Vec<TalkTurn>, ZoomError> {
            self.calls.lock().unwrap().push(id.to_string());
            self.results
                .get(id)
                .cloned()
                .unwrap_or(Err(ZoomError::Gone))
        }
        async fn find_recording(&self, _: &str, _: i64) -> Result<Option<String>, ZoomError> {
            Ok(None)
        }
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
            call_at: "2026-10-20T01:00:00Z".into(),
            zoom_call_log_id: Some(format!("z{id}")),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn only_empty_calls_are_fetched_and_written() {
        let hs = Arc::new(Hs::default());
        {
            let mut c = hs.calls.lock().unwrap();
            c.insert("1".into(), String::new());
            c.insert("2".into(), "営業: もう入っている".into());
            c.insert("3".into(), String::new());
            c.insert("4".into(), "  ".into());
            // 5 は HubSpot に通話が無い
        }
        let client = start(hs.clone()).await;
        let zoom = Zoom {
            results: ["z1", "z3", "z4"]
                .iter()
                .map(|z| (z.to_string(), Ok(turns())))
                .collect(),
            ..Default::default()
        };
        let mut b = TranscriptBackfill::default();
        b.load([r("1"), r("2"), r("3"), r("4"), r("5")]);
        let rep = b.step(&client, &zoom).await;
        assert_eq!(
            rep,
            StepReport {
                checked: 5,
                present: 2,
                saved: 3,
                ..Default::default()
            }
        );
        // Zoom は空の 3 本だけ、新しい順に呼ぶ
        assert_eq!(*zoom.calls.lock().unwrap(), ["z1", "z3", "z4"]);
        assert_eq!(*hs.patches.lock().unwrap(), ["1", "3", "4"]);
        assert_eq!(
            hs.calls.lock().unwrap()["1"],
            "営業: お世話になっております\n先方: はい"
        );
        assert_eq!(hs.calls.lock().unwrap()["2"], "営業: もう入っている");
        assert_eq!(*hs.batch_reads.lock().unwrap(), [5]);
        assert_eq!(b.remaining(), 0);
        // もう一度回しても何も呼ばない
        let rep = b.step(&client, &zoom).await;
        assert_eq!(rep, StepReport::default());
        assert_eq!(zoom.calls.lock().unwrap().len(), 3);
        assert_eq!(hs.batch_reads.lock().unwrap().len(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_cycle_stops_at_the_limit_and_continues_next_time() {
        let hs = Arc::new(Hs::default());
        let n = PER_CYCLE + 10;
        let ids: Vec<String> = (1..=n).map(|i| i.to_string()).collect();
        {
            let mut c = hs.calls.lock().unwrap();
            for id in &ids {
                c.insert(id.clone(), String::new());
            }
        }
        let client = start(hs.clone()).await;
        let zoom = Zoom {
            results: ids.iter().map(|i| (format!("z{i}"), Ok(turns()))).collect(),
            ..Default::default()
        };
        let mut b = TranscriptBackfill::default();
        b.load(ids.iter().map(|i| r(i)));
        let rep = b.step(&client, &zoom).await;
        assert_eq!(rep.saved, PER_CYCLE);
        assert_eq!(zoom.calls.lock().unwrap().len(), PER_CYCLE);
        // 60 本見つけるまで 50 件ずつ確かめる (50 + 20)
        assert_eq!(*hs.batch_reads.lock().unwrap(), [50, 20]);
        assert_eq!(b.remaining(), 10);
        // 最初の 60 本は新しい順
        assert_eq!(zoom.calls.lock().unwrap()[0], "z1");
        assert_eq!(
            zoom.calls.lock().unwrap()[PER_CYCLE - 1],
            format!("z{PER_CYCLE}")
        );
        let rep = b.step(&client, &zoom).await;
        assert_eq!(rep.saved, 10);
        assert_eq!(rep.checked, 0, "確かめ済みの分は読み直さない");
        assert_eq!(b.remaining(), 0);
        assert_eq!(hs.patches.lock().unwrap().len(), n);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn gone_and_untranscribable_are_given_up_and_not_tried_again() {
        let hs = Arc::new(Hs::default());
        {
            let mut c = hs.calls.lock().unwrap();
            for id in ["1", "2", "3"] {
                c.insert(id.into(), String::new());
            }
        }
        let client = start(hs.clone()).await;
        let zoom = Zoom {
            results: HashMap::from([
                ("z1".to_string(), Err(ZoomError::Gone)),
                ("z2".to_string(), Err(ZoomError::Untranscribable)),
                ("z3".to_string(), Ok(turns())),
            ]),
            ..Default::default()
        };
        let mut b = TranscriptBackfill::default();
        let mut no_rec = r("4");
        no_rec.zoom_call_log_id = None;
        b.load([r("1"), r("2"), r("3"), no_rec]);
        let rep = b.step(&client, &zoom).await;
        assert_eq!((rep.saved, rep.given_up, rep.checked), (1, 2, 3));
        assert_eq!(*hs.patches.lock().unwrap(), ["3"]);
        assert_eq!(b.remaining(), 0);
        b.step(&client, &zoom).await;
        assert_eq!(*zoom.calls.lock().unwrap(), ["z1", "z2", "z3"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn temporary_failures_are_retried_later_up_to_the_limit() {
        let hs = Arc::new(Hs::default());
        {
            let mut c = hs.calls.lock().unwrap();
            for id in ["1", "2"] {
                c.insert(id.into(), String::new());
            }
        }
        let client = start(hs.clone()).await;
        let zoom = Zoom {
            results: HashMap::from([
                ("z1".to_string(), Err(ZoomError::NotReady)),
                ("z2".to_string(), Ok(turns())),
            ]),
            ..Default::default()
        };
        let mut b = TranscriptBackfill::default();
        b.load([r("1"), r("2")]);
        let rep = b.step(&client, &zoom).await;
        assert_eq!((rep.saved, rep.retry_later), (1, 1));
        assert_eq!(b.remaining(), 1);
        let rep = b.step(&client, &zoom).await;
        assert_eq!(rep.retry_later, 1);
        let rep = b.step(&client, &zoom).await;
        assert_eq!(rep.given_up, 1);
        assert_eq!(b.remaining(), 0);
        let z1 = zoom
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|z| *z == "z1")
            .count();
        assert_eq!(z1, MAX_ATTEMPTS as usize);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_busy_zoom_stops_the_cycle_without_losing_the_rest() {
        let hs = Arc::new(Hs::default());
        {
            let mut c = hs.calls.lock().unwrap();
            for id in ["1", "2", "3"] {
                c.insert(id.into(), String::new());
            }
        }
        let client = start(hs.clone()).await;
        let zoom = Zoom {
            results: HashMap::from([
                (
                    "z1".to_string(),
                    Err(ZoomError::Transient("http 429".into())),
                ),
                ("z2".to_string(), Ok(turns())),
                ("z3".to_string(), Ok(turns())),
            ]),
            ..Default::default()
        };
        let mut b = TranscriptBackfill::default();
        b.load([r("1"), r("2"), r("3")]);
        let rep = b.step(&client, &zoom).await;
        assert!(rep.stopped);
        assert_eq!((rep.saved, rep.retry_later), (0, 1));
        assert_eq!(*zoom.calls.lock().unwrap(), ["z1"]);
        assert_eq!(b.remaining(), 3);
        // 次の周回で残りを書く (後に回した 1 は最後)
        let rep = b.step(&client, &zoom).await;
        assert_eq!(rep.saved, 2);
        assert_eq!(*hs.patches.lock().unwrap(), ["2", "3"]);
    }
}
