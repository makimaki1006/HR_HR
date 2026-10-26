//! アポ取り以外の電話を外す絞り込み (2026-10-26、ADR-033 の追記、設計書 §22)。
//!
//! 新規営業の人は、アポ取りのほかに商談の前日・当日の確認、日程の再調整、商談後の回答の催促、既存客の件でも電話をかける。
//! それらは接触数・日程切り・9 項目を歪めるので、**かけた時点の取引のステージ**で見分けて、接触の判定の手前で外す。
//! 今のステージは使えない (かけて 2 時間以内に 37% の取引でステージが変わる)。取引の `dealstage` の履歴で
//! 「発信時刻以前の最新の値」を出す。
//!
//! 流れ: Zoom の通話ログの発信 (被呼番号の下 9 桁・時刻) → HubSpot の Call (番号 + 時刻 ±3 分) →
//! 取引 (Call → Deal、無ければ Call → Contact → Deal) → 取引のステージ履歴 → 対象か。
//! HubSpot の呼び出しはすべて `Background` 優先度で、まとめて (Search の窓・関連・取引の履歴は batch) 読む。
//!
//! 判定がまだできない (HubSpot に Call がまだ無い等) ときは [`Scope::Unknown`] を返し、呼び出し側が待って再試行する。
//! 上限 ([`SCOPE_WAIT`]) まで待っても分からなければ対象として扱う (取りこぼしより混入を許す。今の挙動と同じ側)。
//! このファイルは HubSpot の読み取りだけで、Turso には何も書かない。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration as ChronoDuration};
use serde_json::{json, Value};

use super::zoom::CALLER_WINDOW_MS;
use crate::crm::caller_metrics::{flt, CALL_SOURCE};
use crate::hubspot::HubSpotClient;

/// 判定できないまま待つ上限 (通話の時刻から)。接触の印を待つ上限 (`target::NO_MARK_WAIT` = 6 時間) より短くする
pub const SCOPE_WAIT: ChronoDuration = ChronoDuration::hours(3);
/// 通話の時刻からこれだけ経つまで判定しない (HubSpot に Call ができるのを待つ)
const SETTLE_DELAY_MS: i64 = 5 * 60_000;
/// Call が見つかっても取引・担当者の関連がまだ無いとき、取引なしと決めるまでの猶予
const NO_DEAL_SETTLE_MS: i64 = 15 * 60_000;
/// Call を探す窓を分ける間隔 (これより離れた通話は別の窓にする)
const WINDOW_GAP_MS: i64 = 30 * 60_000;
/// 1 つの窓の長さの上限
const WINDOW_MAX_MS: i64 = 6 * 3_600_000;
/// 1 つの窓で読む Search のページ数の上限 (暴走の安全装置)
const MAX_PAGES: usize = 40;
/// 取引の履歴のキャッシュの件数の上限 (超えたら全部捨てる)
const CACHE_MAX: usize = 5_000;
/// 通話からこれだけ経っても分からない発信は、Search を毎周回でなくこの間隔で読み直す
/// (Call が作られない発信が 3 時間残るあいだ、全社の Call を 10 分ごとに読み直さないため)
const SLOW_RETRY_AFTER_MS: i64 = 30 * 60_000;
const SLOW_RETRY_EVERY_MS: i64 = 30 * 60_000;

/// 既定の対象のステージ (アポ取り)。(パイプライン ID, パイプライン名, ステージ ID, ステージ名)。
/// 2026-10-26 に HubSpot の pipelines API (読み取り) で名前から特定した。`CALL_REVIEW_SCOPE_STAGES`
/// (ステージ ID のカンマ区切り) を設定すると、この表の代わりにそのステージだけが対象になる。
/// ここに無いステージ (受注管理_商談・納品管理・計上・債権管理・bpo_リクロジ・請負契約など) は対象外。
pub const DEFAULT_SCOPE_STAGES: &[(&str, &str, &str, &str)] = &[
    // 受注管理_アポ前 (契約先企業・過去契約先は契約まわり、日程確保済み・TEL_アポ日確定はアポ確定後の確認電話なので除く。2026-10-27)
    ("default", "受注管理_アポ前", "appointmentscheduled", "未済"),
    (
        "default",
        "受注管理_アポ前",
        "presentationscheduled",
        "受付ブロック",
    ),
    (
        "default",
        "受注管理_アポ前",
        "decisionmakerboughtin",
        "担当者不在",
    ),
    (
        "default",
        "受注管理_アポ前",
        "closedwon",
        "担当者接触/担当者ブロック",
    ),
    (
        "default",
        "受注管理_アポ前",
        "122445644",
        "担当者_有料サービス利用経験ナシ",
    ),
    (
        "default",
        "受注管理_アポ前",
        "122445645",
        "担当者_有料サービス利用経験アリ",
    ),
    (
        "default",
        "受注管理_アポ前",
        "1435377571",
        "決定者_有料サービス利用経験ナシ",
    ),
    (
        "default",
        "受注管理_アポ前",
        "1435377572",
        "決定者_有料サービス利用経験アリ",
    ),
    (
        "default",
        "受注管理_アポ前",
        "1435440973",
        "決裁者_有料サービス利用経験ナシ",
    ),
    (
        "default",
        "受注管理_アポ前",
        "1435376679",
        "決裁者_有料サービス利用経験アリ",
    ),
    ("default", "受注管理_アポ前", "1366400580", "アポヨミ"),
    ("default", "受注管理_アポ前", "89363529", "架電禁止先"),
    ("default", "受注管理_アポ前", "qualifiedtobuy", "TEL_不通"),
    ("default", "受注管理_アポ前", "1387263026", "成果報酬のみ"),
    ("default", "受注管理_アポ前", "999032072", "大分BPOリスト"),
    ("default", "受注管理_アポ前", "973220404", "本社一括管理"),
    (
        "default",
        "受注管理_アポ前",
        "1404735259",
        "ターゲット外（業種、職種、費用0など）",
    ),
    ("default", "受注管理_アポ前", "1430705153", "パート募集のみ"),
    // リクロジ_商談済リード (期間を空けた再アプローチ。アポ率に入れる、とユーザー決定。全ステージ)
    (
        "62583420",
        "リクロジ_商談済リード",
        "155012220",
        "T_ターゲット外案件",
    ),
    (
        "62583420",
        "リクロジ_商談済リード",
        "1422059151",
        "非価値合意案件",
    ),
    (
        "62583420",
        "リクロジ_商談済リード",
        "155012221",
        "D_担当者価値合意案件",
    ),
    (
        "62583420",
        "リクロジ_商談済リード",
        "155012222",
        "C_決定者価値合意案件",
    ),
    (
        "62583420",
        "リクロジ_商談済リード",
        "155012223",
        "B_決裁者価値合意案件",
    ),
    (
        "62583420",
        "リクロジ_商談済リード",
        "155012224",
        "A_決裁者価値合意案件_成約見込み",
    ),
    // 新　商談実施済リード (商談済リードの新しい版。同じ扱い。全ステージ)
    (
        "913508269",
        "新　商談実施済リード",
        "1388087372",
        "再アプローチの必要なし",
    ),
    (
        "913508269",
        "新　商談実施済リード",
        "1387986832",
        "担当者到達 提案未実施",
    ),
    (
        "913508269",
        "新　商談実施済リード",
        "1388087375",
        "担当者到達 提案済",
    ),
    (
        "913508269",
        "新　商談実施済リード",
        "1388087376",
        "担当者価値合意",
    ),
    (
        "913508269",
        "新　商談実施済リード",
        "1388087377",
        "決裁者到達",
    ),
    (
        "913508269",
        "新　商談実施済リード",
        "1388087378",
        "決裁者の価値合意",
    ),
    // リクロジエージェント (未済などアポ前に当たるステージだけ。導入先・ヨミ・失注・解約・再調整・アポ確定などは対象外)
    (
        "681393283",
        "リクロジエージェント",
        "1016664325",
        "TEL_未済",
    ),
    (
        "681393283",
        "リクロジエージェント",
        "1025335864",
        "TEL_未済(30名未満)",
    ),
    (
        "681393283",
        "リクロジエージェント",
        "1025314205",
        "TEL_不通",
    ),
    (
        "681393283",
        "リクロジエージェント",
        "1025346954",
        "TEL_担当者不在",
    ),
    (
        "681393283",
        "リクロジエージェント",
        "1025346955",
        "TEL_受付ブロック",
    ),
    (
        "681393283",
        "リクロジエージェント",
        "1025346956",
        "TEL_担当者ブロック",
    ),
    (
        "681393283",
        "リクロジエージェント",
        "1031941507",
        "TEL_担当者ブロック/可能性あり",
    ),
    (
        "681393283",
        "リクロジエージェント",
        "1031941508",
        "ニーズなし/無料のみ",
    ),
    (
        "681393283",
        "リクロジエージェント",
        "1031941509",
        "ニーズなし/有料あり",
    ),
    (
        "681393283",
        "リクロジエージェント",
        "1031941510",
        "ニーズあり/無料のみ",
    ),
    (
        "681393283",
        "リクロジエージェント",
        "1031941511",
        "ニーズあり/有料あり",
    ),
];

/// 対象のステージ ID の集合。`override_ids` (環境変数) が空でなければそれだけ、無ければ既定の表
pub fn scope_stage_set(override_ids: &[String]) -> HashSet<String> {
    if override_ids.is_empty() {
        DEFAULT_SCOPE_STAGES
            .iter()
            .map(|(_, _, id, _)| (*id).to_string())
            .collect()
    } else {
        override_ids.iter().cloned().collect()
    }
}

// ---------------------------------------------------------------------------
// 純粋な部分
// ---------------------------------------------------------------------------

/// 取引のステージの履歴の 1 件 (この時刻にこのステージに入った)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageEntry {
    pub at_ms: i64,
    pub stage: String,
}

/// `propertiesWithHistory.dealstage[]` (`value` / `timestamp`) を読む。読めない行は飛ばす
pub fn parse_stage_history(record: &Value) -> Vec<StageEntry> {
    record
        .pointer("/propertiesWithHistory/dealstage")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    let stage = e.get("value")?.as_str()?.to_string();
                    if stage.is_empty() {
                        return None;
                    }
                    let at_ms = DateTime::parse_from_rfc3339(e.get("timestamp")?.as_str()?)
                        .ok()?
                        .timestamp_millis();
                    Some(StageEntry { at_ms, stage })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 発信時刻以前の最新のステージ。履歴に発信より前のものが無い (取引が発信より後にできた) ときは `None`
pub fn stage_at(history: &[StageEntry], at_ms: i64) -> Option<&str> {
    history
        .iter()
        .filter(|e| e.at_ms <= at_ms)
        .max_by_key(|e| e.at_ms)
        .map(|e| e.stage.as_str())
}

/// 取引ごとの履歴から、発信時点のステージで対象か。
/// - 発信時点のステージが分かる取引が 1 つも無い → 対象 (取引なしと同じ扱い)
/// - どれか 1 つでも対象のステージ → 対象
/// - 分かる取引が全部対象外 → 対象外
pub fn in_scope_by_stages(stages_at_call: &[&str], scope: &HashSet<String>) -> bool {
    stages_at_call.is_empty() || stages_at_call.iter().any(|s| scope.contains(*s))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// アポ取りとして数える (理由の短い記号)
    In(&'static str),
    /// アポ取り以外 (接触の判定をしない)
    Out,
    /// まだ分からない (待って再試行。理由の短い記号)
    Unknown(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeQuery {
    /// Zoom の通話ログ ID
    pub id: String,
    /// 被呼番号の下 9 桁
    pub callee9: Option<String>,
    pub at_ms: i64,
}

/// HubSpot の Call (番号と時刻だけ)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HsCall {
    pub id: String,
    pub to9: Option<String>,
    pub ts_ms: i64,
}

/// 近い時刻の通話をまとめて、Call を探す窓 (開始, 終了) にする。長さは [`WINDOW_MAX_MS`] まで
fn windows(sorted_at: &[i64]) -> Vec<(i64, i64)> {
    let mut out: Vec<(i64, i64)> = Vec::new();
    for &t in sorted_at {
        match out.last_mut() {
            Some((from, to)) if t - *to <= WINDOW_GAP_MS && t - *from <= WINDOW_MAX_MS => *to = t,
            _ => out.push((t, t)),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// HubSpot の読み取り (差し替え可能)
// ---------------------------------------------------------------------------

pub trait ScopeSource: Send + Sync {
    /// `[from_ms, to_ms)` の Zoom 純正連携の発信 Call
    fn search_calls(
        &self,
        from_ms: i64,
        to_ms: i64,
    ) -> impl Future<Output = Result<Vec<HsCall>, String>> + Send;
    /// 関連 (from の ID → to の ID)。`from` / `to` は `calls` / `contacts` / `deals`
    fn associated(
        &self,
        from: &'static str,
        to: &'static str,
        ids: &[String],
    ) -> impl Future<Output = Result<BTreeMap<String, Vec<String>>, String>> + Send;
    /// 取引の `dealstage` の履歴。見つからない取引は結果に入らない
    fn stage_histories(
        &self,
        deal_ids: &[String],
    ) -> impl Future<Output = Result<HashMap<String, Vec<StageEntry>>, String>> + Send;
}

/// 本番の読み取り。`client` は `Background` 優先度にして渡す
pub struct HubspotScope {
    client: HubSpotClient,
}

impl HubspotScope {
    pub fn new(client: &HubSpotClient) -> Self {
        Self {
            client: client.background(),
        }
    }
}

fn call_search_body(from: i64, to: i64, after: Option<&str>) -> Value {
    let mut body = json!({
        "filterGroups": [{"filters": [
            flt("hs_call_source", "EQ", json!(CALL_SOURCE)),
            flt("hs_call_direction", "EQ", json!("OUTBOUND")),
            flt("hs_timestamp", "GTE", json!(from.to_string())),
            flt("hs_timestamp", "LT", json!(to.to_string())),
        ]}],
        "properties": ["hs_timestamp", "hs_call_to_number"],
        "sorts": [{"propertyName": "hs_object_id", "direction": "ASCENDING"}],
        "limit": 100,
    });
    if let Some(a) = after {
        body["after"] = json!(a);
    }
    body
}

fn parse_hs_call(v: &Value) -> Option<HsCall> {
    let id = match v.get("id")? {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    let p = v.get("properties")?;
    let ts = p.get("hs_timestamp")?;
    let ts_ms = match ts {
        Value::String(s) => s.parse().ok().or_else(|| {
            DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|t| t.timestamp_millis())
        })?,
        Value::Number(n) => n.as_i64()?,
        _ => return None,
    };
    Some(HsCall {
        id,
        to9: p
            .get("hs_call_to_number")
            .and_then(Value::as_str)
            .and_then(super::zoom::normalize_number),
        ts_ms,
    })
}

impl ScopeSource for HubspotScope {
    async fn search_calls(&self, from_ms: i64, to_ms: i64) -> Result<Vec<HsCall>, String> {
        let mut out = Vec::new();
        let mut after: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let v = self
                .client
                .search("calls", call_search_body(from_ms, to_ms, after.as_deref()))
                .await
                .map_err(|e| e.error_kind().to_string())?;
            let results = v
                .get("results")
                .and_then(Value::as_array)
                .ok_or_else(|| "calls without results".to_string())?;
            out.extend(results.iter().filter_map(parse_hs_call));
            match v
                .pointer("/paging/next/after")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                Some(n) => after = Some(n.to_string()),
                None => return Ok(out),
            }
        }
        Err("too many pages".into())
    }

    async fn associated(
        &self,
        from: &'static str,
        to: &'static str,
        ids: &[String],
    ) -> Result<BTreeMap<String, Vec<String>>, String> {
        let m = self
            .client
            .batch_associations(from, to, ids)
            .await
            .map_err(|e| e.error_kind().to_string())?;
        Ok(m.into_iter()
            .map(|(k, v)| (k, v.into_iter().map(|a| a.id).collect()))
            .collect())
    }

    async fn stage_histories(
        &self,
        deal_ids: &[String],
    ) -> Result<HashMap<String, Vec<StageEntry>>, String> {
        let recs = self
            .client
            .batch_read_with_history("deals", deal_ids, &["dealstage"], &["dealstage"])
            .await
            .map_err(|e| e.error_kind().to_string())?;
        Ok(recs
            .iter()
            .filter_map(|r| {
                let id = match r.get("id")? {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    _ => return None,
                };
                Some((id, parse_stage_history(r)))
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// 判定
// ---------------------------------------------------------------------------

pub struct ScopeResolver<S> {
    source: S,
    stages: HashSet<String>,
    /// 取引 ID → (読んだ時刻, 履歴)。読んだ時刻が発信より後のものだけ、その発信の判定に使える
    cache: Mutex<HashMap<String, (i64, Vec<StageEntry>)>>,
    /// 通話 ID → 最後に Search で探した時刻 (間隔をあけて読み直すため)
    tried: Mutex<HashMap<String, i64>>,
}

impl<S: ScopeSource> ScopeResolver<S> {
    pub fn new(source: S, stages: HashSet<String>) -> Self {
        Self {
            source,
            stages,
            cache: Mutex::new(HashMap::new()),
            tried: Mutex::new(HashMap::new()),
        }
    }

    /// 通話ごとに対象かを決める。`now_ms` は現在時刻。返すマップは `qs` の全部の ID を持つ
    pub async fn resolve(&self, qs: &[ScopeQuery], now_ms: i64) -> HashMap<String, Scope> {
        let mut out: HashMap<String, Scope> = HashMap::new();
        let mut todo: Vec<(&ScopeQuery, &str)> = Vec::new();
        {
            let mut tried = self.tried.lock().unwrap();
            let live: HashSet<&str> = qs.iter().map(|q| q.id.as_str()).collect();
            tried.retain(|id, _| live.contains(id.as_str()));
            for q in qs {
                match q.callee9.as_deref() {
                    None => {
                        out.insert(q.id.clone(), Scope::In("no_number"));
                    }
                    Some(_) if now_ms - q.at_ms < SETTLE_DELAY_MS => {
                        out.insert(q.id.clone(), Scope::Unknown("settling"));
                    }
                    Some(_)
                        if now_ms - q.at_ms >= SLOW_RETRY_AFTER_MS
                            && tried
                                .get(&q.id)
                                .is_some_and(|t| now_ms - t < SLOW_RETRY_EVERY_MS) =>
                    {
                        out.insert(q.id.clone(), Scope::Unknown("backoff"));
                    }
                    Some(n) => {
                        tried.insert(q.id.clone(), now_ms);
                        todo.push((q, n));
                    }
                }
            }
        }
        if todo.is_empty() {
            return out;
        }
        todo.sort_by_key(|(q, _)| q.at_ms);
        let fail = |out: &mut HashMap<String, Scope>, list: &[(&ScopeQuery, &str)], why| {
            for (q, _) in list {
                out.insert(q.id.clone(), Scope::Unknown(why));
            }
        };

        // 1. HubSpot の Call を、時刻の近い通話ごとの窓でまとめて読む
        let at: Vec<i64> = todo.iter().map(|(q, _)| q.at_ms).collect();
        let mut calls: Vec<HsCall> = Vec::new();
        for (from, to) in windows(&at) {
            match self
                .source
                .search_calls(from - CALLER_WINDOW_MS, to + CALLER_WINDOW_MS + 1)
                .await
            {
                Ok(v) => calls.extend(v),
                Err(e) => {
                    // 読めなかった窓の通話だけを待ちにして、ほかの窓は続ける
                    tracing::info!(reason = %e, "talk scope: call search failed");
                    let (bad, rest): (Vec<_>, Vec<_>) = todo
                        .into_iter()
                        .partition(|(q, _)| q.at_ms >= from && q.at_ms <= to);
                    fail(&mut out, &bad, "hubspot_error");
                    todo = rest;
                }
            }
        }
        if todo.is_empty() {
            return out;
        }

        // 2. 番号の下 9 桁と時刻 (±3 分) で結び付ける。同じ Call は 1 本の通話にだけ
        let mut used: HashSet<&str> = HashSet::new();
        let mut matched: Vec<(&ScopeQuery, &str)> = Vec::new(); // (通話, Call ID)
        for (q, n) in &todo {
            let best = calls
                .iter()
                .filter(|c| {
                    c.to9.as_deref() == Some(*n)
                        && !used.contains(c.id.as_str())
                        && (c.ts_ms - q.at_ms).abs() <= CALLER_WINDOW_MS
                })
                .min_by_key(|c| (c.ts_ms - q.at_ms).abs());
            match best {
                Some(c) => {
                    used.insert(c.id.as_str());
                    matched.push((q, c.id.as_str()));
                }
                None => {
                    out.insert(q.id.clone(), Scope::Unknown("call_not_found"));
                }
            }
        }
        if matched.is_empty() {
            return out;
        }

        // 3. Call → Deal。無ければ Call → Contact → Deal
        let call_ids: Vec<String> = matched.iter().map(|(_, c)| (*c).to_string()).collect();
        let direct = match self.source.associated("calls", "deals", &call_ids).await {
            Ok(m) => m,
            Err(e) => {
                tracing::info!(reason = %e, "talk scope: call->deal failed");
                let list: Vec<_> = todo
                    .iter()
                    .filter(|(q, _)| matched.iter().any(|(m, _)| m.id == q.id))
                    .copied()
                    .collect();
                fail(&mut out, &list, "hubspot_error");
                return out;
            }
        };
        let no_deal: Vec<String> = call_ids
            .iter()
            .filter(|c| direct.get(*c).is_none_or(Vec::is_empty))
            .cloned()
            .collect();
        let mut via_contact: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut contact_err = false;
        if !no_deal.is_empty() {
            match self.source.associated("calls", "contacts", &no_deal).await {
                Ok(call_contacts) => {
                    let mut cids: Vec<String> = call_contacts.values().flatten().cloned().collect();
                    cids.sort();
                    cids.dedup();
                    let contact_deals = if cids.is_empty() {
                        Ok(BTreeMap::new())
                    } else {
                        self.source.associated("contacts", "deals", &cids).await
                    };
                    match contact_deals {
                        Ok(cd) => {
                            for (call, contacts) in &call_contacts {
                                let mut deals: Vec<String> = contacts
                                    .iter()
                                    .flat_map(|c| cd.get(c).cloned().unwrap_or_default())
                                    .collect();
                                deals.sort();
                                deals.dedup();
                                via_contact.insert(call.clone(), deals);
                            }
                        }
                        Err(e) => {
                            tracing::info!(reason = %e, "talk scope: contact->deal failed");
                            contact_err = true;
                        }
                    }
                }
                Err(e) => {
                    tracing::info!(reason = %e, "talk scope: call->contact failed");
                    contact_err = true;
                }
            }
        }
        // 通話ごとの取引
        let mut deals_of: Vec<(&ScopeQuery, Vec<String>)> = Vec::new();
        for (q, call) in &matched {
            let direct_deals = direct.get(*call).filter(|d| !d.is_empty());
            let deals = match direct_deals {
                Some(d) => d.clone(),
                None if contact_err => {
                    out.insert(q.id.clone(), Scope::Unknown("hubspot_error"));
                    continue;
                }
                None => via_contact.get(*call).cloned().unwrap_or_default(),
            };
            if deals.is_empty() {
                // 関連がまだ付いていないだけかもしれないので、しばらくは待つ
                out.insert(
                    q.id.clone(),
                    if now_ms - q.at_ms < NO_DEAL_SETTLE_MS {
                        Scope::Unknown("associations_pending")
                    } else {
                        Scope::In("no_deal")
                    },
                );
            } else {
                deals_of.push((q, deals));
            }
        }
        if deals_of.is_empty() {
            return out;
        }

        // 4. 取引のステージ履歴 (読んだ時刻が発信より後のキャッシュは使い回す)
        let mut need: HashMap<String, i64> = HashMap::new(); // 取引 ID → 必要な最も遅い発信時刻
        for (q, deals) in &deals_of {
            for d in deals {
                let e = need.entry(d.clone()).or_insert(q.at_ms);
                *e = (*e).max(q.at_ms);
            }
        }
        let missing: Vec<String> = {
            let cache = self.cache.lock().unwrap();
            let mut m: Vec<String> = need
                .iter()
                .filter(|(d, at)| cache.get(*d).is_none_or(|(fetched, _)| fetched <= *at))
                .map(|(d, _)| d.clone())
                .collect();
            m.sort();
            m
        };
        let mut fetched_err = false;
        if !missing.is_empty() {
            match self.source.stage_histories(&missing).await {
                Ok(h) => {
                    let mut cache = self.cache.lock().unwrap();
                    if cache.len() + missing.len() > CACHE_MAX {
                        cache.clear();
                    }
                    for d in &missing {
                        // 見つからない取引 (統合・削除) は空の履歴にして、再読み込みを繰り返さない
                        cache.insert(d.clone(), (now_ms, h.get(d).cloned().unwrap_or_default()));
                    }
                }
                Err(e) => {
                    tracing::info!(reason = %e, "talk scope: deal history failed");
                    fetched_err = true;
                }
            }
        }
        let cache = self.cache.lock().unwrap();
        for (q, deals) in &deals_of {
            // 読み直しに失敗したときは、発信より前に読んだ古い履歴でも決めない
            if fetched_err
                && deals
                    .iter()
                    .any(|d| cache.get(d).is_none_or(|(fetched, _)| *fetched <= q.at_ms))
            {
                out.insert(q.id.clone(), Scope::Unknown("hubspot_error"));
                continue;
            }
            let stages: Vec<&str> = deals
                .iter()
                .filter_map(|d| cache.get(d).and_then(|(_, h)| stage_at(h, q.at_ms)))
                .collect();
            out.insert(
                q.id.clone(),
                if stages.is_empty() {
                    Scope::In("no_stage")
                } else if in_scope_by_stages(&stages, &self.stages) {
                    Scope::In("stage")
                } else {
                    Scope::Out
                },
            );
        }
        out
    }
}

/// 待っている通話を判定した結果の振り分け
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Partition {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    /// 分からなかったが、待つ上限を超えたので対象として扱うもの
    pub timed_out: Vec<String>,
    pub waiting: Vec<String>,
}

/// 判定の結果と、通話からの経過時間で、対象にする / 外す / 待つ / 上限を超えたので対象にする、に分ける
pub fn partition(
    verdicts: &HashMap<String, Scope>,
    ages: &HashMap<String, ChronoDuration>,
) -> Partition {
    let mut p = Partition::default();
    let mut ids: Vec<&String> = verdicts.keys().collect();
    ids.sort();
    for id in ids {
        match &verdicts[id] {
            Scope::In(_) => p.include.push(id.clone()),
            Scope::Out => p.exclude.push(id.clone()),
            Scope::Unknown(_) => {
                if ages.get(id).is_some_and(|a| *a >= SCOPE_WAIT) {
                    p.timed_out.push(id.clone());
                } else {
                    p.waiting.push(id.clone());
                }
            }
        }
    }
    p
}

/// 設定から作る。`client` は `background()` にして使う
pub fn production_resolver(
    client: &Arc<HubSpotClient>,
    override_ids: &[String],
) -> ScopeResolver<HubspotScope> {
    ScopeResolver::new(HubspotScope::new(client), scope_stage_set(override_ids))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn ms(s: &str) -> i64 {
        DateTime::parse_from_rfc3339(s).unwrap().timestamp_millis()
    }
    fn entry(at: &str, stage: &str) -> StageEntry {
        StageEntry {
            at_ms: ms(at),
            stage: stage.into(),
        }
    }

    #[test]
    fn default_table_has_the_expected_stages() {
        let s = scope_stage_set(&[]);
        // アポ前: 未済は対象。契約先企業・過去契約先は対象外
        assert!(s.contains("appointmentscheduled"));
        assert!(!s.contains("1404977820"), "契約先企業");
        assert!(!s.contains("1251636516"), "過去契約先");
        // アポ確定後の確認電話の段階は対象外 (2026-10-27)。アポヨミは対象
        assert!(!s.contains("1332175104"), "日程確保済み");
        assert!(!s.contains("51997752"), "TEL_アポ日確定");
        assert!(s.contains("1366400580"), "アポヨミ");
        // 商談 (アポ日確定 52035886 ほか)・納品管理・bpo は対象外
        for id in ["52035886", "52035887", "52016153", "66848546", "1095387442"] {
            assert!(!s.contains(id), "{id}");
        }
        // 商談済リードは全ステージ、エージェントは未済系だけ
        assert!(s.contains("155012224") && s.contains("1388087378"));
        assert!(s.contains("1016664325"));
        assert!(!s.contains("998359425"), "Bヨミ");
        assert!(!s.contains("998359422"), "導入先");
        assert!(!s.contains("1016858793"), "再調整");
        assert_eq!(s.len(), DEFAULT_SCOPE_STAGES.len());
        // 環境変数で上書き
        let o = scope_stage_set(&["x".into(), "y".into()]);
        assert_eq!(o.len(), 2);
        assert!(!o.contains("appointmentscheduled"));
    }

    #[test]
    fn stage_is_the_latest_entry_at_or_before_the_call() {
        // HubSpot の履歴は新しい順
        let h = vec![
            entry("2026-10-20T03:00:00Z", "52035886"),
            entry("2026-10-20T01:00:00Z", "closedwon"),
            entry("2026-10-10T00:00:00Z", "appointmentscheduled"),
        ];
        assert_eq!(
            stage_at(&h, ms("2026-10-20T00:59:59Z")),
            Some("appointmentscheduled")
        );
        assert_eq!(
            stage_at(&h, ms("2026-10-20T01:00:00Z")),
            Some("closedwon"),
            "ちょうどの時刻は含む"
        );
        assert_eq!(
            stage_at(&h, ms("2026-10-20T02:00:00Z")),
            Some("closedwon"),
            "2 時間後に変わっても発信時点は前のまま"
        );
        assert_eq!(stage_at(&h, ms("2026-10-21T00:00:00Z")), Some("52035886"));
        assert_eq!(
            stage_at(&h, ms("2026-10-01T00:00:00Z")),
            None,
            "取引ができる前の発信"
        );
    }

    #[test]
    fn history_parses_value_and_timestamp() {
        let r = json!({"id": "1", "propertiesWithHistory": {"dealstage": [
            {"value": "closedwon", "timestamp": "2026-10-20T01:00:00.000Z"},
            {"value": "", "timestamp": "2026-10-19T01:00:00.000Z"},
            {"value": "x", "timestamp": "bad"},
            {"value": "appointmentscheduled", "timestamp": "2026-10-10T00:00:00.000Z"},
        ]}});
        let h = parse_stage_history(&r);
        assert_eq!(h.len(), 2);
        assert_eq!(h[0], entry("2026-10-20T01:00:00Z", "closedwon"));
        assert!(parse_stage_history(&json!({"id": "2"})).is_empty());
    }

    #[test]
    fn several_deals_any_in_scope_counts_and_none_known_counts() {
        let s = scope_stage_set(&[]);
        assert!(
            in_scope_by_stages(&["52035886", "appointmentscheduled"], &s),
            "どれか 1 つが対象なら対象"
        );
        assert!(
            !in_scope_by_stages(&["52035886", "66848546"], &s),
            "全部対象外なら対象外"
        );
        assert!(
            in_scope_by_stages(&[], &s),
            "ステージが分かる取引が無ければ対象"
        );
    }

    #[test]
    fn windows_split_on_gaps_and_cap_length() {
        let m = 60_000;
        assert_eq!(windows(&[0, 10 * m, 20 * m]), vec![(0, 20 * m)]);
        assert_eq!(
            windows(&[0, 10 * m, 100 * m]),
            vec![(0, 10 * m), (100 * m, 100 * m)]
        );
        // 29 分ずつ続くと 6 時間で区切る
        let at: Vec<i64> = (0..20).map(|i| i * 29 * m).collect();
        let w = windows(&at);
        assert!(w.len() >= 2 && w.iter().all(|(a, b)| b - a <= WINDOW_MAX_MS));
    }

    // ---- 偽の HubSpot ----
    #[derive(Default)]
    struct Fake {
        calls: Vec<HsCall>,
        call_deals: BTreeMap<String, Vec<String>>,
        call_contacts: BTreeMap<String, Vec<String>>,
        contact_deals: BTreeMap<String, Vec<String>>,
        histories: HashMap<String, Vec<StageEntry>>,
        fail_search: bool,
        fail_history: std::sync::atomic::AtomicBool,
        searches: AtomicUsize,
        reads: AtomicUsize,
    }
    impl ScopeSource for Fake {
        async fn search_calls(&self, _f: i64, _t: i64) -> Result<Vec<HsCall>, String> {
            self.searches.fetch_add(1, Ordering::SeqCst);
            if self.fail_search {
                return Err("hubspot_busy".into());
            }
            Ok(self.calls.clone())
        }
        async fn associated(
            &self,
            from: &'static str,
            to: &'static str,
            ids: &[String],
        ) -> Result<BTreeMap<String, Vec<String>>, String> {
            let src = match (from, to) {
                ("calls", "deals") => &self.call_deals,
                ("calls", "contacts") => &self.call_contacts,
                ("contacts", "deals") => &self.contact_deals,
                _ => return Err("bad".into()),
            };
            Ok(ids
                .iter()
                .filter_map(|i| src.get(i).map(|v| (i.clone(), v.clone())))
                .collect())
        }
        async fn stage_histories(
            &self,
            ids: &[String],
        ) -> Result<HashMap<String, Vec<StageEntry>>, String> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            if self.fail_history.load(Ordering::SeqCst) {
                return Err("hubspot_busy".into());
            }
            Ok(ids
                .iter()
                .filter_map(|i| self.histories.get(i).map(|v| (i.clone(), v.clone())))
                .collect())
        }
    }

    const NOW: &str = "2026-10-20T06:00:00Z";
    fn q(id: &str, n: &str, at: &str) -> ScopeQuery {
        ScopeQuery {
            id: id.into(),
            callee9: Some(n.into()),
            at_ms: ms(at),
        }
    }
    fn hc(id: &str, n: &str, at: &str) -> HsCall {
        HsCall {
            id: id.into(),
            to9: Some(n.into()),
            ts_ms: ms(at),
        }
    }
    fn v(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[tokio::test]
    async fn resolves_by_stage_at_call_time_with_direct_contact_and_no_deals() {
        let mut f = Fake::default();
        f.calls = vec![
            hc("c1", "311111111", "2026-10-20T01:00:20Z"), // アポ前 (直接)
            hc("c2", "322222222", "2026-10-20T01:10:00Z"), // 商談 (Contact 経由)
            hc("c3", "333333333", "2026-10-20T01:20:00Z"), // 取引なし
            hc("c4", "344444444", "2026-10-20T01:30:00Z"), // 複数 (商談 + アポ前)
            hc("c5", "355555555", "2026-10-20T01:40:00Z"), // 発信 2 時間後にステージが変わる (発信時点はアポ前)
        ];
        f.call_deals = [
            ("c1", v(&["d1"])),
            ("c4", v(&["d4a", "d4b"])),
            ("c5", v(&["d5"])),
        ]
        .into_iter()
        .map(|(k, x)| (k.to_string(), x))
        .collect();
        f.call_contacts = [("c2", v(&["k2"])), ("c3", v(&[]))]
            .into_iter()
            .map(|(k, x)| (k.to_string(), x))
            .collect();
        f.contact_deals = [("k2", v(&["d2"]))]
            .into_iter()
            .map(|(k, x)| (k.to_string(), x))
            .collect();
        f.histories = [
            (
                "d1",
                vec![entry("2026-10-01T00:00:00Z", "appointmentscheduled")],
            ),
            ("d2", vec![entry("2026-10-01T00:00:00Z", "52035886")]),
            ("d4a", vec![entry("2026-10-01T00:00:00Z", "52035887")]),
            ("d4b", vec![entry("2026-10-01T00:00:00Z", "155012221")]),
            (
                "d5",
                vec![
                    entry("2026-10-20T03:40:00Z", "52035886"),
                    entry("2026-10-05T00:00:00Z", "closedwon"),
                ],
            ),
        ]
        .into_iter()
        .map(|(k, x)| (k.to_string(), x))
        .collect();
        let r = ScopeResolver::new(f, scope_stage_set(&[]));
        let qs = vec![
            q("z1", "311111111", "2026-10-20T01:00:00Z"),
            q("z2", "322222222", "2026-10-20T01:10:30Z"),
            q("z3", "333333333", "2026-10-20T01:20:00Z"),
            q("z4", "344444444", "2026-10-20T01:30:00Z"),
            q("z5", "355555555", "2026-10-20T01:40:00Z"),
        ];
        let out = r.resolve(&qs, ms(NOW)).await;
        assert_eq!(out["z1"], Scope::In("stage"));
        assert_eq!(
            out["z2"],
            Scope::Out,
            "Contact 経由で見つけた商談の取引は対象外"
        );
        assert_eq!(
            out["z3"],
            Scope::In("no_deal"),
            "取引が無い電話は対象のまま"
        );
        assert_eq!(
            out["z4"],
            Scope::In("stage"),
            "複数の取引のどれか 1 つが対象なら対象"
        );
        assert_eq!(
            out["z5"],
            Scope::In("stage"),
            "今は商談でも、発信時点のステージで決める"
        );
        // 近い時刻の通話は 1 つの窓にまとまり、Search は 1 回
        assert_eq!(r.source.searches.load(Ordering::SeqCst), 1);
        // 取引の履歴は 1 回の batch (d1, d2, d4a, d4b, d5)
        assert_eq!(r.source.reads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn waits_when_call_missing_settling_or_hubspot_fails() {
        let mut f = Fake::default();
        f.calls = vec![hc("c1", "311111111", "2026-10-20T05:50:00Z")];
        f.call_deals = [("c1".to_string(), v(&[]))].into_iter().collect();
        let r = ScopeResolver::new(f, scope_stage_set(&[]));
        let qs = vec![
            q("old_no_call", "399999999", "2026-10-20T01:00:00Z"),
            q("fresh", "388888888", "2026-10-20T05:58:00Z"),
            // Call はあるが関連がまだ無い (10 分前)
            q("assoc", "311111111", "2026-10-20T05:50:00Z"),
        ];
        let out = r.resolve(&qs, ms(NOW)).await;
        assert_eq!(out["old_no_call"], Scope::Unknown("call_not_found"));
        assert_eq!(
            out["fresh"],
            Scope::Unknown("settling"),
            "5 分経つまで探さない"
        );
        assert_eq!(out["assoc"], Scope::Unknown("associations_pending"));
        // 15 分以上経てば取引なし
        let out = r.resolve(&qs, ms("2026-10-20T06:10:00Z")).await;
        assert_eq!(out["assoc"], Scope::In("no_deal"));

        let mut f = Fake::default();
        f.fail_search = true;
        let r = ScopeResolver::new(f, scope_stage_set(&[]));
        let out = r
            .resolve(&[q("a", "311111111", "2026-10-20T01:00:00Z")], ms(NOW))
            .await;
        assert_eq!(out["a"], Scope::Unknown("hubspot_error"));
        // 番号が無い通話は判定の材料が無いので対象
        let out = r
            .resolve(
                &[ScopeQuery {
                    id: "n".into(),
                    callee9: None,
                    at_ms: ms("2026-10-20T01:00:00Z"),
                }],
                ms(NOW),
            )
            .await;
        assert_eq!(out["n"], Scope::In("no_number"));
    }

    #[tokio::test]
    async fn old_unmatched_calls_are_searched_every_30_minutes_not_every_round() {
        // Call が作られない発信 (通話から 1 時間)。最初は探し、30 分たつまでは探さない
        let r = ScopeResolver::new(Fake::default(), scope_stage_set(&[]));
        let qs = vec![q("x", "311111111", "2026-10-20T04:00:00Z")];
        let out = r.resolve(&qs, ms("2026-10-20T05:00:00Z")).await;
        assert_eq!(out["x"], Scope::Unknown("call_not_found"));
        let out = r.resolve(&qs, ms("2026-10-20T05:10:00Z")).await;
        assert_eq!(out["x"], Scope::Unknown("backoff"));
        let out = r.resolve(&qs, ms("2026-10-20T05:20:00Z")).await;
        assert_eq!(out["x"], Scope::Unknown("backoff"));
        assert_eq!(r.source.searches.load(Ordering::SeqCst), 1);
        let out = r.resolve(&qs, ms("2026-10-20T05:30:00Z")).await;
        assert_eq!(out["x"], Scope::Unknown("call_not_found"));
        assert_eq!(r.source.searches.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn stale_history_is_not_used_when_rereading_fails() {
        // 10:00 に読んだ履歴は商談 (対象外)。11:00 の発信のために読み直すが失敗 → 古い履歴で決めない
        let mut f = Fake::default();
        f.calls = vec![
            hc("c0", "311111111", "2026-10-20T00:30:00Z"),
            hc("c1", "322222222", "2026-10-20T02:00:00Z"),
        ];
        f.call_deals = [
            ("c0".to_string(), v(&["d1"])),
            ("c1".to_string(), v(&["d1"])),
        ]
        .into_iter()
        .collect();
        f.histories = [(
            "d1".to_string(),
            vec![entry("2026-10-01T00:00:00Z", "52035887")],
        )]
        .into_iter()
        .collect();
        let r = ScopeResolver::new(f, scope_stage_set(&[]));
        let out = r
            .resolve(
                &[q("a", "311111111", "2026-10-20T00:30:00Z")],
                ms("2026-10-20T01:00:00Z"),
            )
            .await;
        assert_eq!(out["a"], Scope::Out);
        r.source.fail_history.store(true, Ordering::SeqCst);
        let out = r
            .resolve(
                &[q("b", "322222222", "2026-10-20T02:00:00Z")],
                ms("2026-10-20T03:00:00Z"),
            )
            .await;
        assert_eq!(out["b"], Scope::Unknown("hubspot_error"));
    }

    #[tokio::test]
    async fn one_hubspot_call_matches_only_one_zoom_call_and_history_is_cached() {
        let mut f = Fake::default();
        f.calls = vec![hc("c1", "311111111", "2026-10-20T01:00:00Z")];
        f.call_deals = [("c1".to_string(), v(&["d1"]))].into_iter().collect();
        f.histories = [(
            "d1".to_string(),
            vec![entry("2026-10-01T00:00:00Z", "52035886")],
        )]
        .into_iter()
        .collect();
        let r = ScopeResolver::new(f, scope_stage_set(&[]));
        // 同じ番号に 1 分差で 2 回かけたが、HubSpot の Call は 1 本だけ。近い方だけが結び付く
        let qs = vec![
            q("z1", "311111111", "2026-10-20T01:00:30Z"),
            q("z2", "311111111", "2026-10-20T01:01:00Z"),
        ];
        let out = r.resolve(&qs, ms(NOW)).await;
        assert_eq!(out["z1"], Scope::Out);
        assert_eq!(out["z2"], Scope::Unknown("call_not_found"));
        assert_eq!(r.source.reads.load(Ordering::SeqCst), 1);
        // 2 回目は履歴を読み直さない (読んだ時刻が発信より後)
        let _ = r.resolve(&qs[..1], ms(NOW)).await;
        assert_eq!(r.source.reads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn partition_waits_then_gives_up_as_in_scope() {
        let mut vd = HashMap::new();
        vd.insert("in".to_string(), Scope::In("stage"));
        vd.insert("out".to_string(), Scope::Out);
        vd.insert("young".to_string(), Scope::Unknown("call_not_found"));
        vd.insert("old".to_string(), Scope::Unknown("call_not_found"));
        let mut ages = HashMap::new();
        ages.insert("young".to_string(), ChronoDuration::minutes(179));
        ages.insert("old".to_string(), ChronoDuration::hours(3));
        let p = partition(&vd, &ages);
        assert_eq!(p.include, vec!["in"]);
        assert_eq!(p.exclude, vec!["out"]);
        assert_eq!(p.waiting, vec!["young"]);
        assert_eq!(p.timed_out, vec!["old"]);
    }

    #[test]
    fn call_search_body_has_no_owner_filter_and_a_time_window() {
        let b = call_search_body(1000, 2000, Some("x"));
        let f = b["filterGroups"][0]["filters"].as_array().unwrap();
        assert_eq!(f.len(), 4);
        assert!(f.iter().all(|x| x["propertyName"] != "hubspot_owner_id"));
        assert_eq!(b["after"], "x");
    }
}
