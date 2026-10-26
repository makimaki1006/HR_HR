//! 背景の worker (ADR-035)。`APO_JUDGE_ENABLED=1` のときだけ動く。
//!
//! 5 分おき (既定) に、アポ日確定のステージに直近 `APO_JUDGE_LOOKBACK_HOURS` 時間以内に入った取引で
//! `apo_ai_judged_at` がまだ空のものを Search で探し、1 件ずつ
//! 取引の担当者 (Contact) → ステージに入る前の直近の発信 (Call) → Zoom の文字起こし → MiniMax の判定 → 取引への書き込み、と進める。
//! 文字起こしがまだ無ければ次の周回に回し、ステージに入ってから `APO_JUDGE_WAIT_HOURS` 時間たっても取れなければ
//! 「文字起こしなし」を書いて終わる。HubSpot の呼び出しはすべて背景の優先度 (関所、§18)。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use super::judge::{judge_apo, ApoJudgement};
use super::{ApoJudgeConfig, PROP_CALL_ID, PROP_GRADE, PROP_JUDGED_AT, PROP_REASON, PROP_SCORE};
use crate::crm::caller_metrics::flt;
use crate::crm::talk_review::judge::{Completer, JudgeError, MiniMaxJudge};
use crate::crm::talk_review::transcript::{extract_call_log_id, has_both_sides};
use crate::crm::talk_review::zoom::{TranscriptSource, ZoomClient, ZoomError, REC_PREFIX};
use crate::hubspot::HubSpotClient;
use crate::AppState;

/// 録音 URL が Call に無いとき、通話履歴で探すまで待つ時間 (talk_review の `URL_GRACE` と同じ)
const URL_GRACE: chrono::Duration = chrono::Duration::hours(2);
/// ステージに入った時刻より少し後の電話まで候補にする (HubSpot の時刻のずれ)
const SLACK_MS: i64 = 120_000;
/// 電話の候補を見る件数 (新しい順)
const MAX_CANDIDATES: usize = 3;
/// 判定が続けて失敗したら諦める回数
const MAX_JUDGE_FAILURES: u32 = 3;
/// 取引に付ける Contact の上限 (Search のフィルタグループは 5 つまで)
const MAX_CONTACTS: usize = 5;

/// 文字起こしがないと書くときの区分の内部値
pub const GRADE_NO_TRANSCRIPT: &str = "no_transcript";
/// 判定が続けて失敗したときの区分の内部値
pub const GRADE_JUDGE_FAILED: &str = "judge_failed";

// ---------------------------------------------------------------------------
// HubSpot (テストで偽物に差し替える)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Deal {
    pub id: String,
    /// アポ日確定に入った時刻 (ミリ秒)。対象ステージのうち最も新しいもの
    pub entered_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CallRef {
    pub id: String,
    pub ts_ms: i64,
    pub duration_ms: i64,
    pub body: String,
    pub to_number: Option<String>,
}

pub trait ApoHubSpot: Send + Sync {
    fn find_deals(
        &self,
        stages: &[String],
        since_ms: i64,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Deal>, String>> + Send;

    fn deal_contact_ids(
        &self,
        deal_id: &str,
    ) -> impl Future<Output = Result<Vec<String>, String>> + Send;

    /// Contact に関連する発信の Call を、`until_ms` 以前で新しい順に
    fn find_calls(
        &self,
        contact_ids: &[String],
        until_ms: i64,
    ) -> impl Future<Output = Result<Vec<CallRef>, String>> + Send;

    fn write(
        &self,
        deal_id: &str,
        props: BTreeMap<String, Option<String>>,
    ) -> impl Future<Output = Result<(), String>> + Send;
}

fn time_ms(v: &Value) -> Option<i64> {
    match v {
        Value::String(s) => s.parse().ok().or_else(|| {
            DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|t| t.timestamp_millis())
        }),
        Value::Number(n) => n.as_i64(),
        _ => None,
    }
}

fn id_of(v: &Value) -> Option<String> {
    match v.get("id")? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

pub fn entered_prop(stage: &str) -> String {
    format!("hs_v2_date_entered_{stage}")
}

/// 対象ステージの検索。ステージごとに「入った時刻 ≧ since かつ judged_at が空」を OR でつなぐ
pub fn deal_search_body(stages: &[String], since_ms: i64, limit: usize) -> Value {
    let groups: Vec<Value> = stages
        .iter()
        .map(|s| {
            json!({"filters": [
                flt(&entered_prop(s), "GTE", json!(since_ms.to_string())),
                {"propertyName": PROP_JUDGED_AT, "operator": "NOT_HAS_PROPERTY"},
            ]})
        })
        .collect();
    let mut props: Vec<String> = stages.iter().map(|s| entered_prop(s)).collect();
    props.push(PROP_JUDGED_AT.to_string());
    json!({
        "filterGroups": groups,
        "properties": props,
        "sorts": [{"propertyName": "hs_object_id", "direction": "ASCENDING"}],
        "limit": limit,
    })
}

pub fn parse_deals(v: &Value, stages: &[String]) -> Vec<Deal> {
    v.get("results")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|r| {
                    let id = id_of(r)?;
                    let entered_ms = stages
                        .iter()
                        .filter_map(|s| r.pointer("/properties")?.get(entered_prop(s)))
                        .filter_map(time_ms)
                        .max()?;
                    Some(Deal { id, entered_ms })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Contact ごとに 1 つのフィルタグループ (OR)。発信だけ、`until_ms` 以前、新しい順
pub fn call_search_body(contact_ids: &[String], until_ms: i64) -> Value {
    let groups: Vec<Value> = contact_ids
        .iter()
        .take(MAX_CONTACTS)
        .map(|c| {
            json!({"filters": [
                flt("associations.contact", "EQ", json!(c)),
                flt("hs_call_direction", "EQ", json!("OUTBOUND")),
                flt("hs_timestamp", "LTE", json!(until_ms.to_string())),
            ]})
        })
        .collect();
    json!({
        "filterGroups": groups,
        "properties": ["hs_timestamp", "hs_call_duration", "hs_call_body", "hs_call_to_number"],
        "sorts": [{"propertyName": "hs_timestamp", "direction": "DESCENDING"}],
        "limit": 10,
    })
}

pub fn parse_calls(v: &Value) -> Vec<CallRef> {
    v.get("results")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|r| {
                    let p = r.get("properties")?;
                    Some(CallRef {
                        id: id_of(r)?,
                        ts_ms: time_ms(p.get("hs_timestamp")?)?,
                        duration_ms: p.get("hs_call_duration").and_then(time_ms).unwrap_or(0),
                        body: p
                            .get("hs_call_body")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        to_number: p
                            .get("hs_call_to_number")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

impl ApoHubSpot for HubSpotClient {
    async fn find_deals(
        &self,
        stages: &[String],
        since_ms: i64,
        limit: usize,
    ) -> Result<Vec<Deal>, String> {
        let v = self
            .search("deals", deal_search_body(stages, since_ms, limit))
            .await
            .map_err(|e| format!("deal search: {e:?}"))?;
        Ok(parse_deals(&v, stages))
    }

    async fn deal_contact_ids(&self, deal_id: &str) -> Result<Vec<String>, String> {
        let (refs, _) = self
            .list_associations("deals", deal_id, "contacts")
            .await
            .map_err(|e| format!("associations: {e:?}"))?;
        Ok(refs.into_iter().map(|r| r.id).collect())
    }

    async fn find_calls(
        &self,
        contact_ids: &[String],
        until_ms: i64,
    ) -> Result<Vec<CallRef>, String> {
        if contact_ids.is_empty() {
            return Ok(Vec::new());
        }
        let v = self
            .search("calls", call_search_body(contact_ids, until_ms))
            .await
            .map_err(|e| format!("call search: {e:?}"))?;
        Ok(parse_calls(&v))
    }

    async fn write(
        &self,
        deal_id: &str,
        props: BTreeMap<String, Option<String>>,
    ) -> Result<(), String> {
        self.patch_object("deals", deal_id, &props)
            .await
            .map(|_| ())
            .map_err(|e| format!("patch: {e:?}"))
    }
}

// ---------------------------------------------------------------------------
// 1 件の処理
// ---------------------------------------------------------------------------

/// 取引に書く結果
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Scored { j: ApoJudgement, call_id: String },
    NoTranscript(&'static str),
    JudgeFailed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Verdict(Verdict),
    /// 次の周回でやり直す (理由の短い記号)
    Wait(&'static str),
    /// 判定が失敗した (回数は呼び出し側が数える)
    JudgeError,
}

enum Fetched {
    Turns(Vec<crate::crm::talk_review::types::TalkTurn>),
    /// まだ取れない (後で)
    NotYet,
    /// 取れない (録音が無い・消えた・文字起こしできない)
    Gone,
}

async fn fetch_turns<Z: TranscriptSource>(zoom: &Z, call: &CallRef, now: DateTime<Utc>) -> Fetched {
    let call_age = now - DateTime::<Utc>::from_timestamp_millis(call.ts_ms).unwrap_or(now);
    let id = match extract_call_log_id(&call.body) {
        Some(id) => id,
        // Call Queue 経由の発信は URL が入らない。通話履歴から相手番号と時刻で探す
        None => {
            let Some(number) = call.to_number.as_deref() else {
                return Fetched::Gone;
            };
            match zoom.find_recording(number, call.ts_ms).await {
                Ok(Some(rec)) => format!("{REC_PREFIX}{rec}"),
                Ok(None) if call_age < URL_GRACE => return Fetched::NotYet,
                Ok(None) => return Fetched::Gone,
                Err(ZoomError::Gone | ZoomError::Untranscribable) => return Fetched::Gone,
                Err(_) => return Fetched::NotYet,
            }
        }
    };
    match zoom.fetch(&id).await {
        Ok(turns) if has_both_sides(&turns) => Fetched::Turns(turns),
        // 営業しか話していない (留守電など)
        Ok(_) => Fetched::Gone,
        Err(ZoomError::Gone | ZoomError::Untranscribable) => Fetched::Gone,
        Err(_) => Fetched::NotYet,
    }
}

pub async fn process_deal<H: ApoHubSpot, Z: TranscriptSource, C: Completer>(
    hs: &H,
    zoom: &Z,
    judge: &C,
    cfg: &ApoJudgeConfig,
    deal: &Deal,
    now: DateTime<Utc>,
) -> Outcome {
    let entered = DateTime::<Utc>::from_timestamp_millis(deal.entered_ms).unwrap_or(now);
    let past_deadline = now - entered >= cfg.wait;
    // 取れない理由。期限前なら待ち、期限を過ぎていれば「文字起こしなし」で終わる
    let not_yet = |why: &'static str| {
        if past_deadline {
            Outcome::Verdict(Verdict::NoTranscript(why))
        } else {
            Outcome::Wait(why)
        }
    };
    let contacts = match hs.deal_contact_ids(&deal.id).await {
        Ok(c) => c,
        Err(e) => {
            tracing::info!(deal = %deal.id, error = %e, "apo judge: contacts failed");
            return not_yet("hubspot_error");
        }
    };
    if contacts.is_empty() {
        return not_yet("no_contact");
    }
    let calls = match hs.find_calls(&contacts, deal.entered_ms + SLACK_MS).await {
        Ok(c) => c,
        Err(e) => {
            tracing::info!(deal = %deal.id, error = %e, "apo judge: calls failed");
            return not_yet("hubspot_error");
        }
    };
    let candidates: Vec<&CallRef> = calls
        .iter()
        .filter(|c| c.duration_ms >= cfg.min_talk_sec * 1000)
        .take(MAX_CANDIDATES)
        .collect();
    if candidates.is_empty() {
        return not_yet("no_call");
    }
    let mut found: Option<(&CallRef, Vec<_>)> = None;
    for call in candidates {
        match fetch_turns(zoom, call, now).await {
            Fetched::Turns(t) => {
                found = Some((call, t));
                break;
            }
            // 新しい電話がまだ処理中なら、古い電話に切り替えず待つ (期限後は古い電話も見る)
            Fetched::NotYet if !past_deadline => return Outcome::Wait("transcript_not_ready"),
            Fetched::NotYet | Fetched::Gone => continue,
        }
    }
    let Some((call, turns)) = found else {
        return Outcome::Verdict(Verdict::NoTranscript("transcript_unavailable"));
    };
    match judge_apo(judge, &turns).await {
        Ok(j) => Outcome::Verdict(Verdict::Scored {
            j,
            call_id: call.id.clone(),
        }),
        Err(JudgeError::Transient(_) | JudgeError::NotConfigured) => Outcome::JudgeError,
    }
}

fn no_transcript_text(why: &str) -> &'static str {
    match why {
        "no_contact" => "取引に関連する担当者が見つかりませんでした",
        "no_call" => "アポ日確定より前の発信の記録が見つかりませんでした",
        "transcript_unavailable" => "発信の録音・文字起こしを取得できませんでした",
        _ => "文字起こしを取得できませんでした",
    }
}

/// 書き込むプロパティ。`judged_at` は書き込みの時刻 (これが判定済みの印)
pub fn props_for(v: &Verdict, now: DateTime<Utc>) -> BTreeMap<String, Option<String>> {
    let mut m: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut set = |k: &str, val: String| {
        m.insert(k.to_string(), Some(val));
    };
    match v {
        Verdict::Scored { j, call_id } => {
            set(PROP_SCORE, j.score.to_string());
            set(PROP_GRADE, j.grade.value().to_string());
            set(PROP_REASON, j.reason_text());
            set(PROP_CALL_ID, call_id.clone());
        }
        Verdict::NoTranscript(why) => {
            set(PROP_GRADE, GRADE_NO_TRANSCRIPT.to_string());
            set(PROP_REASON, no_transcript_text(why).to_string());
        }
        Verdict::JudgeFailed => {
            set(PROP_GRADE, GRADE_JUDGE_FAILED.to_string());
            set(
                PROP_REASON,
                "判定を作れませんでした。電話の内容は取引のメモから確認してください".to_string(),
            );
        }
    }
    set(PROP_JUDGED_AT, now.timestamp_millis().to_string());
    m
}

// ---------------------------------------------------------------------------
// 周回
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct Cycle {
    /// 書き込み済み (または dry-run で判定済み) の取引。Search の反映遅れで二度判定しない
    done: HashSet<String>,
    /// 判定は済んだが書き込みに失敗したもの。次の周回で書き込みだけやり直す (再判定しない)
    unwritten: HashMap<String, Verdict>,
    judge_failures: HashMap<String, u32>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CycleStats {
    pub found: usize,
    pub written: usize,
    pub waiting: usize,
}

impl Cycle {
    pub async fn run_once<H: ApoHubSpot, Z: TranscriptSource, C: Completer>(
        &mut self,
        hs: &H,
        zoom: &Z,
        judge: &C,
        cfg: &ApoJudgeConfig,
        now: DateTime<Utc>,
    ) -> Result<CycleStats, String> {
        let since = (now - cfg.lookback).timestamp_millis();
        let deals = hs
            .find_deals(&cfg.stages, since, cfg.max_per_cycle.max(1) * 3)
            .await?;
        let mut stats = CycleStats {
            found: deals.len(),
            ..Default::default()
        };
        let mut handled = 0;
        for deal in deals {
            if self.done.contains(&deal.id) {
                continue;
            }
            if handled >= cfg.max_per_cycle {
                break;
            }
            handled += 1;
            let verdict = if let Some(v) = self.unwritten.remove(&deal.id) {
                v
            } else {
                match process_deal(hs, zoom, judge, cfg, &deal, now).await {
                    Outcome::Verdict(v) => v,
                    Outcome::Wait(why) => {
                        tracing::debug!(deal = %deal.id, why, "apo judge: waiting");
                        stats.waiting += 1;
                        continue;
                    }
                    Outcome::JudgeError => {
                        let n = self.judge_failures.entry(deal.id.clone()).or_default();
                        *n += 1;
                        if *n < MAX_JUDGE_FAILURES {
                            stats.waiting += 1;
                            continue;
                        }
                        Verdict::JudgeFailed
                    }
                }
            };
            if cfg.dry_run {
                // 件数と区分だけログに出す (本文・理由は出さない)
                let (grade, score) = match &verdict {
                    Verdict::Scored { j, .. } => (j.grade.value(), Some(j.score)),
                    Verdict::NoTranscript(_) => (GRADE_NO_TRANSCRIPT, None),
                    Verdict::JudgeFailed => (GRADE_JUDGE_FAILED, None),
                };
                tracing::info!(deal = %deal.id, grade, ?score, "apo judge (dry run): not written");
                self.done.insert(deal.id);
                stats.written += 1;
                continue;
            }
            match hs.write(&deal.id, props_for(&verdict, now)).await {
                Ok(()) => {
                    self.judge_failures.remove(&deal.id);
                    self.done.insert(deal.id);
                    stats.written += 1;
                }
                Err(e) => {
                    tracing::warn!(deal = %deal.id, error = %e, "apo judge: write failed; will retry");
                    self.unwritten.insert(deal.id, verdict);
                }
            }
        }
        Ok(stats)
    }
}

/// 起動。`APO_JUDGE_ENABLED=1`・HubSpot・Zoom・MiniMax の設定がそろっていなければ何もしない
pub fn spawn_worker(state: Arc<AppState>) {
    let cfg = ApoJudgeConfig::from_env();
    if !cfg.enabled {
        return;
    }
    let Some(client) = state.crm_hubspot() else {
        tracing::warn!("apo judge: HubSpot が未接続のため起動しません");
        return;
    };
    let Some(zoom) = ZoomClient::from_env() else {
        tracing::warn!("apo judge: ZOOM_ACCOUNT_ID / ZOOM_CLIENT_ID / ZOOM_CLIENT_SECRET が未設定のため起動しません");
        return;
    };
    let Some(judge) = MiniMaxJudge::new(&cfg.minimax_base_url, &cfg.model) else {
        tracing::warn!("apo judge: MINIMAX_API_KEY が未設定のため起動しません");
        return;
    };
    tracing::info!(model = %cfg.model, stages = ?cfg.stages, dry_run = cfg.dry_run, "apo judge worker started");
    let bg = client.background();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(90)).await;
        let mut cycle = Cycle::default();
        loop {
            match cycle.run_once(&bg, &*zoom, &judge, &cfg, Utc::now()).await {
                Ok(s) if s.found > 0 => {
                    tracing::info!(
                        found = s.found,
                        written = s.written,
                        waiting = s.waiting,
                        "apo judge cycle"
                    )
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "apo judge cycle failed"),
            }
            tokio::time::sleep(cfg.interval).await;
        }
    });
}

// ---------------------------------------------------------------------------
// テスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crm::talk_review::types::TalkTurn;
    use std::sync::Mutex;

    const NOW_MS: i64 = 1_790_000_000_000;

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp_millis(NOW_MS).unwrap()
    }

    fn cfg() -> ApoJudgeConfig {
        ApoJudgeConfig::from_lookup(|_| None)
    }

    fn good_json() -> Value {
        json!({
            "conditions": {
                "budget": {"present": true, "quote": "月30万円"},
                "timing": {"present": true, "quote": "来月から"},
                "decision_maker": {"present": true, "quote": "社長です"},
                "vacancy_reason": {"present": true, "quote": "退職"},
                "competing": {"present": false, "quote": null}
            },
            "excluded_layers": [],
            "reason": "予算と時期が具体的です。"
        })
    }

    type Written = (String, BTreeMap<String, Option<String>>);

    #[derive(Default)]
    struct FakeHs {
        deals: Vec<Deal>,
        contacts: Vec<String>,
        calls: Vec<CallRef>,
        written: Mutex<Vec<Written>>,
        fail_writes: Mutex<u32>,
        search_args: Mutex<Vec<(Vec<String>, i64, usize)>>,
        call_until: Mutex<Vec<i64>>,
    }

    impl ApoHubSpot for FakeHs {
        async fn find_deals(
            &self,
            stages: &[String],
            since_ms: i64,
            limit: usize,
        ) -> Result<Vec<Deal>, String> {
            self.search_args
                .lock()
                .unwrap()
                .push((stages.to_vec(), since_ms, limit));
            Ok(self.deals.clone())
        }
        async fn deal_contact_ids(&self, _: &str) -> Result<Vec<String>, String> {
            Ok(self.contacts.clone())
        }
        async fn find_calls(&self, _: &[String], until_ms: i64) -> Result<Vec<CallRef>, String> {
            self.call_until.lock().unwrap().push(until_ms);
            Ok(self.calls.clone())
        }
        async fn write(
            &self,
            id: &str,
            props: BTreeMap<String, Option<String>>,
        ) -> Result<(), String> {
            let mut f = self.fail_writes.lock().unwrap();
            if *f > 0 {
                *f -= 1;
                return Err("boom".into());
            }
            self.written.lock().unwrap().push((id.to_string(), props));
            Ok(())
        }
    }

    struct FakeZoom {
        /// 録音 URL の無い Call に対する通話履歴の答え
        recording: Result<Option<String>, ZoomError>,
        fetch: Result<Vec<TalkTurn>, ZoomError>,
        fetched: Mutex<Vec<String>>,
    }

    fn turns() -> Vec<TalkTurn> {
        vec![
            TalkTurn {
                n: 1,
                speaker: "rep".into(),
                text: "お世話になります".into(),
            },
            TalkTurn {
                n: 2,
                speaker: "customer".into(),
                text: "月30万円で来月から。社長です。退職の補充".into(),
            },
        ]
    }

    impl FakeZoom {
        fn ok() -> Self {
            Self {
                recording: Ok(Some("REC1".into())),
                fetch: Ok(turns()),
                fetched: Mutex::new(vec![]),
            }
        }
    }

    impl TranscriptSource for FakeZoom {
        async fn fetch(&self, id: &str) -> Result<Vec<TalkTurn>, ZoomError> {
            self.fetched.lock().unwrap().push(id.to_string());
            self.fetch.clone()
        }
        async fn find_recording(&self, _: &str, _: i64) -> Result<Option<String>, ZoomError> {
            self.recording.clone()
        }
    }

    struct FakeJudge(Mutex<Vec<Result<Value, JudgeError>>>);
    impl Completer for FakeJudge {
        async fn complete(&self, _: &str) -> Result<Value, JudgeError> {
            let mut q = self.0.lock().unwrap();
            if q.is_empty() {
                Err(JudgeError::Transient("empty".into()))
            } else {
                q.remove(0)
            }
        }
    }

    fn call(id: &str, age_min: i64, secs: i64, body: &str) -> CallRef {
        CallRef {
            id: id.into(),
            ts_ms: NOW_MS - age_min * 60_000,
            duration_ms: secs * 1000,
            body: body.into(),
            to_number: Some("03-1234-5678".into()),
        }
    }

    fn url_body() -> String {
        "https://x/audio/play/1234/20261026-aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".to_string()
    }

    fn deal(age_min: i64) -> Deal {
        Deal {
            id: "D1".into(),
            entered_ms: NOW_MS - age_min * 60_000,
        }
    }

    #[test]
    fn deal_search_asks_for_entered_in_window_and_no_judged_at() {
        let stages = vec!["111".to_string(), "222".to_string()];
        let b = deal_search_body(&stages, 1000, 60);
        let groups = b["filterGroups"].as_array().unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups[0]["filters"][0],
            json!({"propertyName": "hs_v2_date_entered_111", "operator": "GTE", "value": "1000"})
        );
        assert_eq!(
            groups[1]["filters"][1],
            json!({"propertyName": "apo_ai_judged_at", "operator": "NOT_HAS_PROPERTY"})
        );
        assert_eq!(b["limit"], 60);
    }

    #[test]
    fn deals_take_the_latest_entered_time_and_skip_rows_without_one() {
        let stages = vec!["111".to_string(), "222".to_string()];
        let v = json!({"results": [
            {"id": "1", "properties": {"hs_v2_date_entered_111": "2026-10-26T01:00:00Z", "hs_v2_date_entered_222": "2026-10-26T03:00:00.000Z"}},
            {"id": "2", "properties": {"hs_v2_date_entered_111": "1790000000000"}},
            {"id": "3", "properties": {"hs_v2_date_entered_111": null}},
        ]});
        let d = parse_deals(&v, &stages);
        assert_eq!(d.len(), 2);
        assert_eq!(
            d[0].entered_ms,
            DateTime::parse_from_rfc3339("2026-10-26T03:00:00Z")
                .unwrap()
                .timestamp_millis()
        );
        assert_eq!(
            (d[1].id.as_str(), d[1].entered_ms),
            ("2", 1_790_000_000_000)
        );
    }

    #[test]
    fn call_search_is_one_group_per_contact_outbound_before_entered() {
        let ids: Vec<String> = (1..=7).map(|i| i.to_string()).collect();
        let b = call_search_body(&ids, 5000);
        let g = b["filterGroups"].as_array().unwrap();
        // フィルタグループは 5 つまで
        assert_eq!(g.len(), 5);
        assert_eq!(g[0]["filters"][0]["propertyName"], "associations.contact");
        assert_eq!(g[0]["filters"][1]["value"], "OUTBOUND");
        assert_eq!(g[0]["filters"][2]["value"], "5000");
        assert_eq!(b["sorts"][0]["direction"], "DESCENDING");
    }

    #[test]
    fn calls_parse_with_numbers_and_recording_url() {
        let v = json!({"results": [{"id": 9, "properties": {"hs_timestamp": "2026-10-26T01:00:00Z", "hs_call_duration": "95000", "hs_call_body": "b", "hs_call_to_number": " 03 "}}]});
        let c = parse_calls(&v);
        assert_eq!((c[0].id.as_str(), c[0].duration_ms), ("9", 95_000));
        assert_eq!(c[0].to_number.as_deref(), Some("03"));
    }

    async fn run(
        hs: &FakeHs,
        zoom: &FakeZoom,
        replies: Vec<Result<Value, JudgeError>>,
        cfg: &ApoJudgeConfig,
        cycle: &mut Cycle,
    ) -> CycleStats {
        cycle
            .run_once(hs, zoom, &FakeJudge(Mutex::new(replies)), cfg, now())
            .await
            .unwrap()
    }

    fn hs_with(deals: Vec<Deal>, calls: Vec<CallRef>) -> FakeHs {
        FakeHs {
            deals,
            contacts: vec!["C1".into()],
            calls,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn judges_and_writes_the_properties_with_concrete_values() {
        let hs = hs_with(vec![deal(30)], vec![call("CALL9", 40, 120, &url_body())]);
        let zoom = FakeZoom::ok();
        let mut cycle = Cycle::default();
        let s = run(&hs, &zoom, vec![Ok(good_json())], &cfg(), &mut cycle).await;
        assert_eq!((s.found, s.written, s.waiting), (1, 1, 0));
        // Search は直近 72 時間・既定の 2 ステージ
        let (stages, since, _) = hs.search_args.lock().unwrap()[0].clone();
        assert_eq!(stages, vec!["52035886", "1095457875"]);
        assert_eq!(since, NOW_MS - 72 * 3_600_000);
        // 電話は「ステージに入った時刻 + 2 分」以前
        assert_eq!(
            hs.call_until.lock().unwrap()[0],
            NOW_MS - 30 * 60_000 + 120_000
        );
        // URL から取った ID で文字起こしを取っている
        assert_eq!(
            zoom.fetched.lock().unwrap()[0],
            "20261026-aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"
        );
        let w = hs.written.lock().unwrap();
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].0, "D1");
        let p = &w[0].1;
        assert_eq!(p["apo_ai_score"].as_deref(), Some("4"));
        assert_eq!(p["apo_ai_grade"].as_deref(), Some("attend"));
        assert_eq!(p["apo_ai_call_id"].as_deref(), Some("CALL9"));
        assert_eq!(
            p["apo_ai_judged_at"].as_deref(),
            Some(NOW_MS.to_string().as_str())
        );
        assert!(p["apo_ai_reason"]
            .as_deref()
            .unwrap()
            .starts_with("区分: 同席推奨 / 点数: 4/5\n○ 予算の金額の目安「月30万円」"));
        assert_eq!(p.len(), 5);
    }

    #[tokio::test]
    async fn an_already_written_deal_is_skipped_even_if_search_still_returns_it() {
        let hs = hs_with(vec![deal(30)], vec![call("C", 40, 120, &url_body())]);
        let zoom = FakeZoom::ok();
        let mut cycle = Cycle::default();
        run(&hs, &zoom, vec![Ok(good_json())], &cfg(), &mut cycle).await;
        // 二周目は判定も書き込みもしない (MiniMax の返事が空でも失敗しない)
        let s = run(&hs, &zoom, vec![], &cfg(), &mut cycle).await;
        assert_eq!((s.found, s.written), (1, 0));
        assert_eq!(hs.written.lock().unwrap().len(), 1);
        assert_eq!(zoom.fetched.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn transcript_not_ready_waits_then_gives_up_after_the_deadline() {
        let mut zoom = FakeZoom::ok();
        zoom.fetch = Err(ZoomError::NotReady);
        // ステージに入って 1 時間: 待つ
        let hs = hs_with(vec![deal(60)], vec![call("C", 70, 120, &url_body())]);
        let mut cycle = Cycle::default();
        let s = run(&hs, &zoom, vec![], &cfg(), &mut cycle).await;
        assert_eq!((s.waiting, s.written), (1, 0));
        assert!(hs.written.lock().unwrap().is_empty());
        // 7 時間: 「文字起こしなし」を書く (点数は書かない)
        let hs = hs_with(
            vec![deal(7 * 60)],
            vec![call("C", 8 * 60, 120, &url_body())],
        );
        let mut cycle = Cycle::default();
        let s = run(&hs, &zoom, vec![], &cfg(), &mut cycle).await;
        assert_eq!(s.written, 1);
        let w = hs.written.lock().unwrap();
        let p = &w[0].1;
        assert_eq!(p["apo_ai_grade"].as_deref(), Some("no_transcript"));
        assert_eq!(
            p["apo_ai_reason"].as_deref(),
            Some("発信の録音・文字起こしを取得できませんでした")
        );
        assert!(!p.contains_key("apo_ai_score"));
        assert!(p.contains_key("apo_ai_judged_at"));
    }

    #[tokio::test]
    async fn untranscribable_call_is_final_immediately() {
        let mut zoom = FakeZoom::ok();
        zoom.fetch = Err(ZoomError::Untranscribable);
        let hs = hs_with(vec![deal(10)], vec![call("C", 20, 120, &url_body())]);
        let mut cycle = Cycle::default();
        let s = run(&hs, &zoom, vec![], &cfg(), &mut cycle).await;
        assert_eq!(s.written, 1);
        assert_eq!(
            hs.written.lock().unwrap()[0].1["apo_ai_grade"].as_deref(),
            Some("no_transcript")
        );
    }

    #[tokio::test]
    async fn no_contact_or_no_call_waits_before_the_deadline() {
        let mut hs = hs_with(vec![deal(10)], vec![]);
        hs.contacts = vec![];
        let mut cycle = Cycle::default();
        let s = run(&hs, &FakeZoom::ok(), vec![], &cfg(), &mut cycle).await;
        assert_eq!((s.waiting, s.written), (1, 0));
        // 電話が無い (短すぎる電話だけ) 場合も同じ
        let hs = hs_with(vec![deal(10)], vec![call("C", 20, 30, &url_body())]);
        let mut cycle = Cycle::default();
        let s = run(&hs, &FakeZoom::ok(), vec![], &cfg(), &mut cycle).await;
        assert_eq!((s.waiting, s.written), (1, 0));
    }

    #[tokio::test]
    async fn call_without_url_uses_the_zoom_call_history_recording() {
        let hs = hs_with(vec![deal(300)], vec![call("C", 310, 120, "")]);
        let zoom = FakeZoom::ok();
        let mut cycle = Cycle::default();
        run(&hs, &zoom, vec![Ok(good_json())], &cfg(), &mut cycle).await;
        assert_eq!(zoom.fetched.lock().unwrap()[0], "rec:REC1");
        // 履歴にも無く、電話が 2 時間より新しければ待つ
        let mut zoom = FakeZoom::ok();
        zoom.recording = Ok(None);
        let hs = hs_with(vec![deal(30)], vec![call("C", 40, 120, "")]);
        let mut cycle = Cycle::default();
        let s = run(&hs, &zoom, vec![], &cfg(), &mut cycle).await;
        assert_eq!((s.waiting, s.written), (1, 0));
    }

    #[tokio::test]
    async fn only_rep_voice_is_not_judged() {
        let mut zoom = FakeZoom::ok();
        zoom.fetch = Ok(vec![turns()[0].clone()]);
        let hs = hs_with(vec![deal(10)], vec![call("C", 20, 120, &url_body())]);
        let mut cycle = Cycle::default();
        run(&hs, &zoom, vec![], &cfg(), &mut cycle).await;
        assert_eq!(
            hs.written.lock().unwrap()[0].1["apo_ai_grade"].as_deref(),
            Some("no_transcript")
        );
    }

    #[tokio::test]
    async fn judge_failures_are_retried_then_written_as_failed() {
        let hs = hs_with(vec![deal(10)], vec![call("C", 20, 120, &url_body())]);
        let zoom = FakeZoom::ok();
        let mut cycle = Cycle::default();
        for _ in 0..2 {
            let s = run(&hs, &zoom, vec![], &cfg(), &mut cycle).await;
            assert_eq!((s.waiting, s.written), (1, 0));
        }
        let s = run(&hs, &zoom, vec![], &cfg(), &mut cycle).await;
        assert_eq!(s.written, 1);
        assert_eq!(
            hs.written.lock().unwrap()[0].1["apo_ai_grade"].as_deref(),
            Some("judge_failed")
        );
    }

    #[tokio::test]
    async fn failed_write_is_retried_without_judging_again() {
        let hs = hs_with(vec![deal(10)], vec![call("C", 20, 120, &url_body())]);
        *hs.fail_writes.lock().unwrap() = 1;
        let zoom = FakeZoom::ok();
        let mut cycle = Cycle::default();
        let s = run(&hs, &zoom, vec![Ok(good_json())], &cfg(), &mut cycle).await;
        assert_eq!(s.written, 0);
        // 二周目: MiniMax の返事は空だが、保持した結果を書く
        let s = run(&hs, &zoom, vec![], &cfg(), &mut cycle).await;
        assert_eq!(s.written, 1);
        assert_eq!(
            hs.written.lock().unwrap()[0].1["apo_ai_score"].as_deref(),
            Some("4")
        );
        assert_eq!(zoom.fetched.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn dry_run_judges_but_never_writes() {
        let hs = hs_with(vec![deal(10)], vec![call("C", 20, 120, &url_body())]);
        let mut c = cfg();
        c.dry_run = true;
        let mut cycle = Cycle::default();
        let s = run(&hs, &FakeZoom::ok(), vec![Ok(good_json())], &c, &mut cycle).await;
        assert_eq!(s.written, 1);
        assert!(hs.written.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn newest_call_not_ready_blocks_falling_back_to_an_older_one() {
        struct Z;
        impl TranscriptSource for Z {
            async fn fetch(&self, id: &str) -> Result<Vec<TalkTurn>, ZoomError> {
                if id.starts_with("20261026-bbbbbbbb") {
                    Err(ZoomError::NotReady)
                } else {
                    Ok(turns())
                }
            }
            async fn find_recording(&self, _: &str, _: i64) -> Result<Option<String>, ZoomError> {
                Ok(None)
            }
        }
        let newer = "https://x/audio/play/1/20261026-bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
        let hs = hs_with(
            vec![deal(30)],
            vec![
                call("NEW", 40, 120, newer),
                call("OLD", 50, 120, &url_body()),
            ],
        );
        let mut cycle = Cycle::default();
        let s = cycle
            .run_once(&hs, &Z, &FakeJudge(Mutex::new(vec![])), &cfg(), now())
            .await
            .unwrap();
        assert_eq!((s.waiting, s.written), (1, 0));
    }
}
