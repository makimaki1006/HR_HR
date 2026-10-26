//! アポ数の内訳 (トークの振り返りでアポ数を押すと開く)。設計書 §22 の「アポ数の内訳」。
//!
//! 1 件の取引は、上から最初に当てはまる区分 1 つに入れる:
//! 1. 過去客 (会社で判定): 取引の会社に、この取引とは別に受注した取引がある。受注 = 納品管理・計上パイプラインにある取引
//!    (そのパイプラインのどれかのステージに入った最初の時刻 `hs_v2_date_entered_<ステージ>`、無ければ作成時刻が今回より前)、または受注管理_アポ前の「契約先企業」「過去契約先」に今回より前に入ったことがある取引
//! 2. 再商談: この取引が、今回より前に商談後のステージに入ったことがある
//! 3. 再アポ: この取引が、今回より前にアポ日確定 (`ST_APO` / `ST_APO_BPO`) に入ったことがある
//! 4. 純粋なアポ: どれにも当たらない
//!
//! 「今回」は期間内にアポ日確定に入った時刻 (`hs_v2_date_entered_<ステージ>`)。今回より後の履歴は数えない。
//!
//! HubSpot の呼び出し (全部読み取り、`Background` 優先度。N = アポの取引数、C = その会社の数、D = 会社の取引の数):
//! 取引の履歴 ⌈N/50⌉ + 取引→会社 ⌈N/100⌉ + 会社名 ⌈C/100⌉ + 会社→取引 ⌈C/100⌉ + 会社の取引 ⌈D/100⌉。
//! 結果はプロセス内に持つ (今日を含む期間は 5 分、過去だけの期間は 6 時間)。

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;
use tokio::sync::watch;

use super::counts::{entered_ms, ApoDeal};
use super::scope::{parse_stage_history, StageEntry};
use super::types::{TalkApoCounts, TalkApoDeal};
use crate::handlers::sales_kpi::{ST_APO, ST_APO_BPO};
use crate::hubspot::deep_link::record_url;
use crate::hubspot::types::RecordType;
use crate::hubspot::{HubSpotClient, HubSpotError};

/// 受注した取引が置かれるパイプライン (納品管理・計上。pipelines API で 2026-10-27 に確認)
pub const WON_PIPELINES: &[&str] = &[
    "21596025",  // リクロジ_納品管理
    "743961896", // 請負契約_納品管理
    "22417753",  // 商談管理_計上
    "741140824", // リクロジバイト＿計上
    "743705507", // 請負契約_計上
];

/// 受注管理_アポ前 (`default`) で、受注したことを表すステージ
pub const WON_STAGES: &[&str] = &[
    "1404977820", // 契約先企業
    "1251636516", // 過去契約先（2025/4月以前満了）
];

/// 商談をした後のステージ (入ったことがあれば、その取引は商談をしたことがある)
pub const POST_MEETING_STAGES: &[&str] = &[
    // リクロジ受注管理_商談 (21724969): 進捗確認〜成約 (営業KPI の JISSHI と同じ)
    "52035887",
    "52035888",
    "52035889",
    "52035890",
    "52035891",
    "52017683",
    // リクロジ_商談済リード (62583420)
    "155012220",
    "1422059151",
    "155012221",
    "155012222",
    "155012223",
    "155012224",
    // 新　商談実施済リード (913508269)
    "1388087372",
    "1387986832",
    "1388087375",
    "1388087376",
    "1388087377",
    "1388087378",
    // bpo_リクロジ (753186575): 商談実施処理
    "1325086466",
];

/// 今回の入った時刻と同じ記録とみなす幅 (履歴の時刻と `hs_v2_date_entered_*` は数 ms ずれることがある)
pub const SAME_ENTRY_MS: i64 = 60_000;

/// 区分。表示の順 (上が優先)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApoKind {
    PastCustomer,
    ReMeeting,
    ReApo,
    Fresh,
}

impl ApoKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ApoKind::PastCustomer => "past_customer",
            ApoKind::ReMeeting => "re_meeting",
            ApoKind::ReApo => "re_apo",
            ApoKind::Fresh => "fresh",
        }
    }
}

/// 同じ会社の別の取引 (過去客の判定に使う分だけ)
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OtherDeal {
    pub id: String,
    pub pipeline: Option<String>,
    /// 作られた時刻 (ms)。分からなければ None
    pub created_ms: Option<i64>,
    /// 受注パイプライン (納品管理・計上) のどれかのステージに入った時刻 (ms) のうち最も早いもの。
    /// 商談パイプラインで作られ、後で納品管理・計上へ移った取引の「受注した時刻」になる
    pub won_pipeline_ms: Option<i64>,
    /// 契約先企業・過去契約先に入った時刻 (ms) のうち早いほう
    pub won_stage_ms: Option<i64>,
}

/// 受注した取引か (今回より前に)。受注パイプラインにある取引は、受注パイプラインに入った時刻 (無ければ作成時刻) が
/// 今回より前のときだけ受注。どちらも分からないものは受注とみなす
pub fn won_before(d: &OtherDeal, at_ms: i64) -> bool {
    let in_won_pipeline = d
        .pipeline
        .as_deref()
        .is_some_and(|p| WON_PIPELINES.contains(&p))
        && d.won_pipeline_ms.or(d.created_ms).is_none_or(|c| c < at_ms);
    in_won_pipeline || d.won_stage_ms.is_some_and(|w| w < at_ms)
}

/// pipelines API の応答から、受注パイプラインのステージ ID
pub fn won_pipeline_stage_ids(pipelines: &Value) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for p in pipelines
        .get("results")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let Some(pid) = p.get("id").and_then(Value::as_str) else {
            continue;
        };
        if !WON_PIPELINES.contains(&pid) {
            continue;
        }
        for st in p
            .get("stages")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            if let Some(id) = st.get("id").and_then(Value::as_str) {
                out.insert(id.to_string());
            }
        }
    }
    out.into_iter().collect()
}

/// HubSpot が返すステージの履歴の上限 (新しい順に 20 件まで)
pub const HISTORY_LIMIT: usize = 20;

/// 履歴が上限いっぱいで、見えている一番古い記録が今回の時刻以降なら、今回より前の履歴が見えていない
pub fn history_short(record: &Value, entered_ms: i64) -> bool {
    let Some(a) = record
        .pointer("/propertiesWithHistory/dealstage")
        .and_then(Value::as_array)
    else {
        return false;
    };
    if a.len() < HISTORY_LIMIT {
        return false;
    }
    let oldest = parse_stage_history(record).iter().map(|e| e.at_ms).min();
    oldest.is_some_and(|o| o >= entered_ms - SAME_ENTRY_MS)
}

/// 同じ取引が別のアポ日確定 (BPO と新規営業) にも入っているとき、今回の時刻は早い方にそろえる
/// (もう片方を前のアポとして数えない)。返すのは `deals` と同じ順の「今回の時刻」
pub fn effective_times(deals: &[ApoDeal]) -> Vec<i64> {
    deals
        .iter()
        .map(|d| {
            deals
                .iter()
                .filter(|o| o.deal_id == d.deal_id && o.stage != d.stage)
                .map(|o| o.entered_ms)
                .fold(d.entered_ms, i64::min)
        })
        .collect()
}

/// 1 件の取引の区分 (純粋な関数)。
/// `entered_ms` = 今回アポ日確定に入った時刻、`history` = この取引のステージの履歴、
/// `company_deals` = 取引の会社の取引 (この取引自身が入っていても数えない)
pub fn classify(
    deal_id: &str,
    entered_ms: i64,
    history: &[StageEntry],
    company_deals: &[OtherDeal],
) -> ApoKind {
    if company_deals
        .iter()
        .any(|d| d.id != deal_id && won_before(d, entered_ms))
    {
        return ApoKind::PastCustomer;
    }
    let before = |e: &&StageEntry| e.at_ms < entered_ms - SAME_ENTRY_MS;
    if history
        .iter()
        .filter(before)
        .any(|e| POST_MEETING_STAGES.contains(&e.stage.as_str()))
    {
        return ApoKind::ReMeeting;
    }
    if history
        .iter()
        .filter(before)
        .any(|e| e.stage == ST_APO || e.stage == ST_APO_BPO)
    {
        return ApoKind::ReApo;
    }
    ApoKind::Fresh
}

/// 区分ごとの件数
pub fn tally(kinds: &[ApoKind]) -> TalkApoCounts {
    let mut c = TalkApoCounts::default();
    for k in kinds {
        match k {
            ApoKind::PastCustomer => c.past_customer += 1,
            ApoKind::ReMeeting => c.re_meeting += 1,
            ApoKind::ReApo => c.re_apo += 1,
            ApoKind::Fresh => c.fresh += 1,
        }
        c.total += 1;
    }
    c
}

/// 取引 1 件のステージの記録。`propertiesWithHistory.dealstage` (HubSpot は新しい順に 20 件までしか返さない) に、
/// 商談後のステージに最後に入った時刻 (`hs_v2_date_entered_<ステージ>`) を足す。20 件より前の商談も拾うため
pub fn stage_entries(record: &Value) -> Vec<StageEntry> {
    let mut out = parse_stage_history(record);
    for st in POST_MEETING_STAGES {
        let key = format!("/properties/hs_v2_date_entered_{st}");
        if let Some(at_ms) = record
            .pointer(&key)
            .filter(|v| !v.is_null() && v.as_str() != Some(""))
            .and_then(entered_ms)
        {
            out.push(StageEntry {
                at_ms,
                stage: (*st).to_string(),
            });
        }
    }
    out
}

fn ms_text(ms: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(ms)
        .map(|t| t.to_rfc3339_opts(SecondsFormat::Secs, true))
        .unwrap_or_default()
}

fn prop_ms(p: &BTreeMap<String, Option<String>>, key: &str) -> Option<i64> {
    p.get(key)
        .and_then(Option::as_deref)
        .filter(|s| !s.is_empty())
        .and_then(|s| entered_ms(&Value::String(s.to_string())))
}

/// 会社の関連のうち主の会社 (ラベル Primary)、無ければ最初
fn primary_company(refs: &[crate::hubspot::types::AssociationRef]) -> Option<String> {
    refs.iter()
        .find(|r| r.labels.iter().any(|l| l.eq_ignore_ascii_case("primary")))
        .or_else(|| refs.first())
        .map(|r| r.id.clone())
}

/// アポの取引を区分けして明細にする (HubSpot の読み取りだけ)
pub async fn breakdown(
    bg: &HubSpotClient,
    portal_id: &str,
    deals: &[ApoDeal],
) -> Result<Vec<TalkApoDeal>, HubSpotError> {
    if deals.is_empty() {
        return Ok(vec![]);
    }
    let ids: Vec<String> = deals
        .iter()
        .map(|d| d.deal_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    // 1. 取引のステージの履歴と名前
    let entered_props: Vec<String> = POST_MEETING_STAGES
        .iter()
        .map(|s| format!("hs_v2_date_entered_{s}"))
        .collect();
    let mut props: Vec<&str> = vec!["dealname"];
    props.extend(entered_props.iter().map(String::as_str));
    let records = bg
        .batch_read_with_history("deals", &ids, &props, &["dealstage"])
        .await?;
    let mut history: HashMap<String, Vec<StageEntry>> = HashMap::new();
    let mut raw: HashMap<String, Value> = HashMap::new();
    let mut names: HashMap<String, String> = HashMap::new();
    for r in &records {
        let Some(id) = r.get("id").and_then(Value::as_str) else {
            continue;
        };
        history.insert(id.to_string(), stage_entries(r));
        raw.insert(id.to_string(), r.clone());
        if let Some(n) = r
            .pointer("/properties/dealname")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
        {
            names.insert(id.to_string(), n.to_string());
        }
    }
    // 2. 取引 → 会社
    let deal_companies = bg.batch_associations("deals", "companies", &ids).await?;
    let company_ids: Vec<String> = deal_companies
        .values()
        .flatten()
        .map(|r| r.id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    // 3. 会社名、4. 会社 → 取引、5. 会社の取引のパイプライン・作成時刻・契約先に入った時刻
    let mut company_names: HashMap<String, String> = HashMap::new();
    let mut company_deals: HashMap<String, Vec<OtherDeal>> = HashMap::new();
    if !company_ids.is_empty() {
        for c in bg.batch_read("companies", &company_ids, &["name"]).await? {
            if let Some(n) = c
                .properties
                .get("name")
                .and_then(Option::as_deref)
                .filter(|s| !s.trim().is_empty())
            {
                company_names.insert(c.id.clone(), n.to_string());
            }
        }
        let c2d = bg
            .batch_associations("companies", "deals", &company_ids)
            .await?;
        let other_ids: Vec<String> = c2d
            .values()
            .flatten()
            .map(|r| r.id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let won_props: Vec<String> = WON_STAGES
            .iter()
            .map(|s| format!("hs_v2_date_entered_{s}"))
            .collect();
        // 受注パイプラインの各ステージに入った時刻 (パイプライン定義は 1 回読む)
        let won_pipe_props: Vec<String> = won_pipeline_stage_ids(&bg.deal_pipelines().await?)
            .iter()
            .map(|s| format!("hs_v2_date_entered_{s}"))
            .collect();
        let mut props: Vec<&str> = vec!["pipeline", "createdate"];
        props.extend(won_props.iter().map(String::as_str));
        props.extend(won_pipe_props.iter().map(String::as_str));
        let mut others: HashMap<String, OtherDeal> = HashMap::new();
        if !other_ids.is_empty() {
            for d in bg.batch_read("deals", &other_ids, &props).await? {
                let won_stage_ms = won_props
                    .iter()
                    .filter_map(|k| prop_ms(&d.properties, k))
                    .min();
                let won_pipeline_ms = won_pipe_props
                    .iter()
                    .filter_map(|k| prop_ms(&d.properties, k))
                    .min();
                let created_ms = prop_ms(&d.properties, "createdate").or_else(|| {
                    d.created_at
                        .as_deref()
                        .and_then(|s| entered_ms(&Value::String(s.to_string())))
                });
                others.insert(
                    d.id.clone(),
                    OtherDeal {
                        id: d.id.clone(),
                        pipeline: d.properties.get("pipeline").cloned().flatten(),
                        created_ms,
                        won_pipeline_ms,
                        won_stage_ms,
                    },
                );
            }
        }
        for (cid, refs) in &c2d {
            company_deals.insert(
                cid.clone(),
                refs.iter()
                    .filter_map(|r| others.get(&r.id).cloned())
                    .collect(),
            );
        }
    }
    let empty: Vec<OtherDeal> = Vec::new();
    let times = effective_times(deals);
    let mut out: Vec<TalkApoDeal> = deals
        .iter()
        .zip(times)
        .map(|(d, at_ms)| {
            let companies = deal_companies
                .get(&d.deal_id)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let all_other: Vec<OtherDeal> = companies
                .iter()
                .flat_map(|c| company_deals.get(&c.id).unwrap_or(&empty).iter().cloned())
                .collect();
            let h = history
                .get(&d.deal_id)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let kind = classify(&d.deal_id, at_ms, h, &all_other);
            // 区分は変えず、履歴が足りず前のアポ・商談を見落としたかもしれないものに印を付ける
            let short = matches!(kind, ApoKind::Fresh | ApoKind::ReApo)
                && raw.get(&d.deal_id).is_some_and(|r| history_short(r, at_ms));
            TalkApoDeal {
                deal_id: d.deal_id.clone(),
                deal_name: names.get(&d.deal_id).cloned(),
                company_name: primary_company(companies)
                    .and_then(|c| company_names.get(&c).cloned()),
                kind: kind.as_str().to_string(),
                via: if d.stage == ST_APO_BPO {
                    "bpo"
                } else {
                    "sales"
                }
                .to_string(),
                entered_at: ms_text(d.entered_ms),
                history_short: short,
                hubspot_url: record_url(portal_id, RecordType::Deal, &d.deal_id),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.entered_at
            .cmp(&a.entered_at)
            .then_with(|| a.deal_id.cmp(&b.deal_id))
    });
    Ok(out)
}

/// 区分の文字列 → 件数 (明細から数え直す)
pub fn counts_of(deals: &[TalkApoDeal]) -> TalkApoCounts {
    let kinds: Vec<ApoKind> = deals
        .iter()
        .map(|d| match d.kind.as_str() {
            "past_customer" => ApoKind::PastCustomer,
            "re_meeting" => ApoKind::ReMeeting,
            "re_apo" => ApoKind::ReApo,
            _ => ApoKind::Fresh,
        })
        .collect();
    let mut c = tally(&kinds);
    c.history_short = deals.iter().filter(|d| d.history_short).count() as u32;
    c
}

// ---------------------------------------------------------------------------
// キャッシュ (取引 ID の組 → 明細)
// ---------------------------------------------------------------------------

/// 今日を含む期間の持ち時間
pub const RECENT_TTL: Duration = Duration::from_secs(300);
/// 過去だけの期間の持ち時間
pub const SETTLED_TTL: Duration = Duration::from_secs(6 * 3600);
const CACHE_MAX: usize = 200;

type Key = Vec<(String, i64)>;
/// 入れた時刻・持ち時間・明細
type Entry = (Instant, Duration, Vec<TalkApoDeal>);

type Outcome = Result<Vec<TalkApoDeal>, Arc<HubSpotError>>;

#[derive(Default)]
pub struct BreakdownCache {
    inner: Mutex<HashMap<Key, Entry>>,
    /// 読み込み中の組 (同じ組は 1 本だけ読み、他は結果を待つ)
    inflight: Mutex<HashMap<Key, watch::Receiver<Option<Outcome>>>>,
}

/// 待ちの結果
pub enum LoadError {
    /// 待ちの上限に達した (裏の読み込みは続き、終われば結果はキャッシュに入る)
    Timeout,
    /// HubSpot の読み取りが失敗した
    Hub(Arc<HubSpotError>),
    /// 読み込みが途中で止まった
    Aborted,
}

/// 読み込み中の印を、読み込みの終わり (パニックも含む) で必ず外す
struct InflightGuard {
    cache: Arc<BreakdownCache>,
    key: Key,
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        if let Ok(mut g) = self.cache.inflight.lock() {
            g.remove(&self.key);
        }
    }
}

pub fn cache_key(deals: &[ApoDeal]) -> Key {
    let mut k: Key = deals
        .iter()
        .map(|d| (d.deal_id.clone(), d.entered_ms))
        .collect();
    k.sort();
    k
}

impl BreakdownCache {
    /// キャッシュにあればそれを返す。無ければ同じ組の読み込みが走っていればそれに相乗りし、無ければ読み込みを 1 本始める。
    /// 待ちは `deadline` まで。超えても読み込みは裏で続け、結果はキャッシュに入れる。
    pub async fn get_or_load<F>(
        self: &Arc<Self>,
        key: Key,
        ttl: Duration,
        deadline: Duration,
        load: F,
    ) -> Result<Vec<TalkApoDeal>, LoadError>
    where
        F: std::future::Future<Output = Result<Vec<TalkApoDeal>, HubSpotError>> + Send + 'static,
    {
        let mut rx = {
            let mut g = self.inflight.lock().unwrap();
            if let Some(rx) = g.get(&key) {
                rx.clone()
            } else if let Some(v) = self.get(&key) {
                return Ok(v);
            } else {
                let (tx, rx) = watch::channel(None);
                g.insert(key.clone(), rx.clone());
                let guard = InflightGuard {
                    cache: self.clone(),
                    key: key.clone(),
                };
                tokio::spawn(async move {
                    let out: Outcome = match load.await {
                        Ok(v) => {
                            guard.cache.put(guard.key.clone(), ttl, v.clone());
                            Ok(v)
                        }
                        Err(e) => Err(Arc::new(e)),
                    };
                    drop(guard);
                    let _ = tx.send(Some(out));
                });
                rx
            }
        };
        let wait = async {
            loop {
                if let Some(o) = rx.borrow().clone() {
                    return o.map_err(LoadError::Hub);
                }
                if rx.changed().await.is_err() {
                    return Err(LoadError::Aborted);
                }
            }
        };
        match tokio::time::timeout(deadline, wait).await {
            Err(_) => Err(LoadError::Timeout),
            Ok(r) => r,
        }
    }

    pub fn get(&self, key: &Key) -> Option<Vec<TalkApoDeal>> {
        let g = self.inner.lock().unwrap();
        g.get(key)
            .filter(|(at, ttl, _)| at.elapsed() < *ttl)
            .map(|(_, _, v)| v.clone())
    }

    pub fn put(&self, key: Key, ttl: Duration, v: Vec<TalkApoDeal>) {
        let mut g = self.inner.lock().unwrap();
        g.retain(|_, (at, t, _)| at.elapsed() < *t);
        if g.len() >= CACHE_MAX {
            if let Some(oldest) = g
                .iter()
                .min_by_key(|(_, (at, _, _))| *at)
                .map(|(k, _)| k.clone())
            {
                g.remove(&oldest);
            }
        }
        g.insert(key, (Instant::now(), ttl, v));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(s: &str) -> i64 {
        DateTime::parse_from_rfc3339(s).unwrap().timestamp_millis()
    }
    fn entry(at: &str, stage: &str) -> StageEntry {
        StageEntry {
            at_ms: ms(at),
            stage: stage.into(),
        }
    }
    const NOW: &str = "2026-10-20T03:00:00Z";
    fn now() -> i64 {
        ms(NOW)
    }
    fn won(id: &str, pipeline: &str, created: &str) -> OtherDeal {
        OtherDeal {
            id: id.into(),
            pipeline: Some(pipeline.into()),
            created_ms: Some(ms(created)),
            ..Default::default()
        }
    }

    #[test]
    fn fresh_when_nothing_matches() {
        let h = vec![
            entry("2026-10-01T00:00:00Z", "1095387442"),
            entry(NOW, ST_APO_BPO),
        ];
        assert_eq!(classify("D1", now(), &h, &[]), ApoKind::Fresh);
    }

    #[test]
    fn re_apo_when_entered_appointment_stage_before() {
        let h = vec![
            entry("2026-09-01T00:00:00Z", ST_APO),
            entry("2026-09-05T00:00:00Z", "1422048803"), // 先方都合キャンセル
            entry(NOW, ST_APO),
        ];
        assert_eq!(classify("D1", now(), &h, &[]), ApoKind::ReApo);
        // BPO のアポ日確定も同じ
        let h = vec![
            entry("2026-09-01T00:00:00Z", ST_APO_BPO),
            entry(NOW, ST_APO_BPO),
        ];
        assert_eq!(classify("D1", now(), &h, &[]), ApoKind::ReApo);
    }

    #[test]
    fn current_entry_itself_is_not_a_previous_appointment() {
        // 今回の記録 (数秒ずれ) だけなら純粋
        let h = vec![entry("2026-10-20T03:00:02Z", ST_APO)];
        assert_eq!(classify("D1", now(), &h, &[]), ApoKind::Fresh);
    }

    #[test]
    fn re_meeting_beats_re_apo() {
        let h = vec![
            entry("2026-08-01T00:00:00Z", ST_APO),
            entry("2026-08-10T00:00:00Z", "52035888"), // Dヨミ
            entry("2026-09-01T00:00:00Z", "155012221"), // 商談済リード
            entry(NOW, ST_APO),
        ];
        assert_eq!(classify("D1", now(), &h, &[]), ApoKind::ReMeeting);
        // BPO の商談実施処理 → 再商談
        let h = vec![
            entry("2026-09-01T00:00:00Z", "1325086466"),
            entry(NOW, ST_APO_BPO),
        ];
        assert_eq!(classify("D1", now(), &h, &[]), ApoKind::ReMeeting);
    }

    #[test]
    fn history_after_this_time_is_not_counted() {
        // 今回より後に商談・アポ日確定に入っても数えない
        let h = vec![
            entry(NOW, ST_APO),
            entry("2026-10-25T00:00:00Z", "52035887"),
            entry("2026-10-26T00:00:00Z", ST_APO),
        ];
        assert_eq!(classify("D1", now(), &h, &[]), ApoKind::Fresh);
        // 会社の取引も、今回より後に作られた受注は数えない
        let later = won("D9", "21596025", "2026-10-21T00:00:00Z");
        assert_eq!(classify("D1", now(), &h, &[later]), ApoKind::Fresh);
        // 契約先企業に入ったのが今回より後も数えない
        let o = OtherDeal {
            id: "D9".into(),
            pipeline: Some("default".into()),
            created_ms: Some(ms("2025-01-01T00:00:00Z")),
            won_stage_ms: Some(ms("2026-10-22T00:00:00Z")),
            ..Default::default()
        };
        assert_eq!(classify("D1", now(), &h, &[o]), ApoKind::Fresh);
    }

    #[test]
    fn past_customer_beats_re_meeting() {
        let h = vec![
            entry("2026-08-01T00:00:00Z", ST_APO),
            entry("2026-08-10T00:00:00Z", "52035888"),
            entry(NOW, ST_APO),
        ];
        let o = won("D9", "21596025", "2025-04-01T00:00:00Z"); // 納品管理
        assert_eq!(classify("D1", now(), &h, &[o]), ApoKind::PastCustomer);
        // 計上パイプライン・BPO のアポでも同じ
        let h = vec![entry(NOW, ST_APO_BPO)];
        let o = won("D8", "22417753", "2025-04-01T00:00:00Z");
        assert_eq!(classify("D1", now(), &h, &[o]), ApoKind::PastCustomer);
    }

    #[test]
    fn past_customer_by_contract_stage() {
        // 受注管理_アポ前の契約先企業に入ったことがある別の取引
        let o = OtherDeal {
            id: "D9".into(),
            pipeline: Some("default".into()),
            created_ms: Some(ms("2024-01-01T00:00:00Z")),
            won_stage_ms: Some(ms("2025-03-01T00:00:00Z")),
            ..Default::default()
        };
        assert_eq!(classify("D1", now(), &[], &[o]), ApoKind::PastCustomer);
    }

    #[test]
    fn own_deal_and_unwon_deals_do_not_make_past_customer() {
        // この取引自身が納品管理にあっても、別の取引ではないので過去客にしない
        let me = won("D1", "21596025", "2025-01-01T00:00:00Z");
        // 商談パイプラインの別の取引は受注ではない
        let other = won("D2", "21724969", "2025-01-01T00:00:00Z");
        assert_eq!(classify("D1", now(), &[], &[me, other]), ApoKind::Fresh);
    }

    #[test]
    fn stage_entries_add_the_last_entry_of_post_meeting_stages() {
        // 履歴 (20 件まで) に無い古い商談も、ステージに入った時刻の項目から拾う
        let r = serde_json::json!({
            "id": "1",
            "properties": {
                "hs_v2_date_entered_52035888": "2025-03-01T00:00:00Z",
                "hs_v2_date_entered_155012221": "",
                "hs_v2_date_entered_1325086466": null
            },
            "propertiesWithHistory": {"dealstage": [
                {"value": ST_APO, "timestamp": NOW}
            ]}
        });
        let e = stage_entries(&r);
        assert_eq!(e.len(), 2);
        assert!(e.contains(&entry("2025-03-01T00:00:00Z", "52035888")));
        assert_eq!(classify("1", now(), &e, &[]), ApoKind::ReMeeting);
    }

    #[test]
    fn tally_counts_each_kind_once() {
        let c = tally(&[
            ApoKind::PastCustomer,
            ApoKind::Fresh,
            ApoKind::Fresh,
            ApoKind::ReApo,
            ApoKind::ReMeeting,
        ]);
        assert_eq!(
            (c.past_customer, c.re_meeting, c.re_apo, c.fresh, c.total),
            (1, 1, 1, 2, 5)
        );
    }

    #[test]
    fn cache_key_is_order_independent() {
        let a = |id: &str, t: i64| ApoDeal {
            deal_id: id.into(),
            stage: ST_APO.into(),
            entered_ms: t,
            who: vec![],
        };
        assert_eq!(
            cache_key(&[a("2", 5), a("1", 3)]),
            cache_key(&[a("1", 3), a("2", 5)])
        );
    }

    fn apo(id: &str, stage: &str, at: &str) -> ApoDeal {
        ApoDeal {
            deal_id: id.into(),
            stage: stage.into(),
            entered_ms: ms(at),
            who: vec![],
        }
    }

    #[test]
    fn won_time_is_the_earliest_entry_into_a_won_pipeline_stage() {
        // 商談パイプラインで 2025 年に作られ、今回より後に納品管理へ移った取引は、今回の時点では受注していない
        let moved_later = OtherDeal {
            id: "D9".into(),
            pipeline: Some("21596025".into()),
            created_ms: Some(ms("2025-01-01T00:00:00Z")),
            won_pipeline_ms: Some(ms("2026-10-25T00:00:00Z")),
            won_stage_ms: None,
        };
        assert!(!won_before(&moved_later, now()));
        assert_eq!(classify("D1", now(), &[], &[moved_later]), ApoKind::Fresh);
        // 今回より前に移った取引は受注済み (作成が今回より後でも、入った時刻で決める)
        let moved_before = OtherDeal {
            id: "D8".into(),
            pipeline: Some("21596025".into()),
            created_ms: Some(ms("2025-01-01T00:00:00Z")),
            won_pipeline_ms: Some(ms("2026-09-01T00:00:00Z")),
            won_stage_ms: None,
        };
        assert_eq!(
            classify("D1", now(), &[], &[moved_before]),
            ApoKind::PastCustomer
        );
        // 入った時刻が無ければ作成時刻 (従来どおり)
        let created_only = won("D7", "22417753", "2026-10-21T00:00:00Z");
        assert!(!won_before(&created_only, now()));
    }

    #[test]
    fn won_pipeline_stage_ids_pick_only_won_pipelines() {
        let p = serde_json::json!({"results": [
            {"id": "21596025", "stages": [{"id": "a1"}, {"id": "a2"}]},
            {"id": "21724969", "stages": [{"id": "b1"}]},
            {"id": "22417753", "stages": [{"id": "c1"}]}
        ]});
        assert_eq!(won_pipeline_stage_ids(&p), vec!["a1", "a2", "c1"]);
    }

    fn full_history(oldest: &str) -> Value {
        // 20 件 (新しい順)。一番古いのが oldest
        let base = ms(oldest);
        let h: Vec<Value> = (0..20)
            .rev()
            .map(|i| {
                let t = DateTime::<Utc>::from_timestamp_millis(base + i * 86_400_000).unwrap();
                serde_json::json!({"value": "1095387442", "timestamp": t.to_rfc3339_opts(SecondsFormat::Secs, true)})
            })
            .collect();
        serde_json::json!({"id": "1", "propertiesWithHistory": {"dealstage": h}})
    }

    #[test]
    fn history_short_when_full_and_oldest_is_not_before_this_time() {
        // 20 件で、一番古い記録が今回 (10/20) より後 → 前の履歴が見えていない
        assert!(history_short(&full_history("2026-10-21T00:00:00Z"), now()));
        // 今回の記録そのものが一番古い (数秒のずれ) ときも同じ
        assert!(history_short(&full_history("2026-10-20T03:00:02Z"), now()));
        // 一番古い記録が今回より前なら足りている
        assert!(!history_short(&full_history("2026-09-01T00:00:00Z"), now()));
        // 19 件なら上限に達していないので足りている
        let mut r = full_history("2026-10-21T00:00:00Z");
        r["propertiesWithHistory"]["dealstage"]
            .as_array_mut()
            .unwrap()
            .pop();
        assert!(!history_short(&r, now()));
    }

    #[test]
    fn counts_show_how_many_have_short_history() {
        let d = |id: &str, short: bool| TalkApoDeal {
            deal_id: id.into(),
            deal_name: None,
            company_name: None,
            kind: "fresh".into(),
            via: "bpo".into(),
            entered_at: String::new(),
            history_short: short,
            hubspot_url: String::new(),
        };
        let c = counts_of(&[d("1", true), d("2", false), d("3", true)]);
        assert_eq!((c.fresh, c.total, c.history_short), (3, 3, 2));
    }

    #[test]
    fn same_deal_in_both_appointment_stages_uses_the_earlier_time() {
        let rows = [
            apo("D1", ST_APO_BPO, "2026-10-20T03:00:00Z"),
            apo("D1", ST_APO, "2026-10-20T05:00:00Z"),
            apo("D2", ST_APO, "2026-10-21T00:00:00Z"),
        ];
        let t = effective_times(&rows);
        assert_eq!(
            t,
            vec![
                ms("2026-10-20T03:00:00Z"),
                ms("2026-10-20T03:00:00Z"),
                ms("2026-10-21T00:00:00Z")
            ]
        );
        // 両方の記録が履歴にあっても、どちらの行も純粋なアポ
        let h = vec![
            entry("2026-10-20T03:00:00Z", ST_APO_BPO),
            entry("2026-10-20T05:00:00Z", ST_APO),
        ];
        assert_eq!(classify("D1", t[0], &h, &[]), ApoKind::Fresh);
        assert_eq!(classify("D1", t[1], &h, &[]), ApoKind::Fresh);
        // そろえなければ遅い方は再アポになってしまう (従来の誤り)
        assert_eq!(classify("D1", rows[1].entered_ms, &h, &[]), ApoKind::ReApo);
    }

    #[test]
    fn same_deal_same_stage_on_another_day_stays_separate() {
        let rows = [
            apo("D1", ST_APO, "2026-10-10T03:00:00Z"),
            apo("D1", ST_APO, "2026-10-20T03:00:00Z"),
        ];
        let t = effective_times(&rows);
        assert_eq!(t, vec![rows[0].entered_ms, rows[1].entered_ms]);
    }

    fn row(id: &str) -> TalkApoDeal {
        TalkApoDeal {
            deal_id: id.into(),
            deal_name: None,
            company_name: None,
            kind: "fresh".into(),
            via: "bpo".into(),
            entered_at: String::new(),
            history_short: false,
            hubspot_url: String::new(),
        }
    }

    #[tokio::test]
    async fn concurrent_requests_share_one_load() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let cache = Arc::new(BreakdownCache::default());
        let reads = Arc::new(AtomicUsize::new(0));
        let key: Key = vec![("D1".into(), 1)];
        let mk = |reads: Arc<AtomicUsize>| async move {
            reads.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(80)).await;
            Ok(vec![row("D1")])
        };
        let dl = Duration::from_secs(5);
        let (a, b, c) = tokio::join!(
            cache.get_or_load(key.clone(), RECENT_TTL, dl, mk(reads.clone())),
            cache.get_or_load(key.clone(), RECENT_TTL, dl, mk(reads.clone())),
            cache.get_or_load(key.clone(), RECENT_TTL, dl, mk(reads.clone())),
        );
        for r in [a, b, c] {
            assert_eq!(r.ok().unwrap().len(), 1);
        }
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        // 済んだあとはキャッシュから (読まない)
        let r = cache
            .get_or_load(key, RECENT_TTL, dl, mk(reads.clone()))
            .await;
        assert!(r.is_ok());
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn timeout_keeps_loading_and_fills_the_cache() {
        let cache = Arc::new(BreakdownCache::default());
        let key: Key = vec![("D2".into(), 1)];
        let slow = async {
            tokio::time::sleep(Duration::from_millis(150)).await;
            Ok(vec![row("D2")])
        };
        let r = cache
            .get_or_load(key.clone(), RECENT_TTL, Duration::from_millis(20), slow)
            .await;
        assert!(matches!(r, Err(LoadError::Timeout)));
        assert!(cache.get(&key).is_none());
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(cache.get(&key).unwrap()[0].deal_id, "D2");
    }
}
