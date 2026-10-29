//! 背景の周回 (ADR-037)。`APPLICANT_CALENDAR_ENABLED=1` のときだけ動く。
//!
//! 数分おきに HubSpot Search で応募 (0-421) を読み、載せる項目ごとに予定を作る・移す・消す。
//! - 起動直後の初回: 載せる項目のどれかの値が今日 − 30 日以降の応募 (項目ごとの filterGroups の OR)
//! - 2 回目以降: `hs_lastmodifieddate` が前回の読み始め − 5 分以降の応募
//! - 項目の型 (日時 / 日付) が変わったら、その項目は初回と同じ読み方で読み直して予定を作り直す
//! - 6 時間に 1 回、カレンダーにある自分の予定のうち応募が消えた (読めない) ものを消す
//!
//! HubSpot は背景の優先度 (関所、§18)。Calendar は先に自分の予定の一覧を 1 回読み、
//! 内容が同じなら書かない。周回の状態はプロセス内だけ (Turso に書かない)。
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde_json::{json, Value};

use super::google::{CalError, CalendarApi, Counts, Inserted, Problem};
use super::{
    applicant_link, description, event_id, insert_body, jst, patch_body, same, summary, when_of,
    CalendarConfig, Desired, When, APPLICANT_KEY, PROP_KEY,
};
use crate::crm::applicants::{ranked_deal, report_deal, REPORT_DEAL_PROPERTIES};
use crate::hubspot::{HubSpotClient, HubSpotRecord};
use crate::AppState;

const OBJECT: &str = "0-421";
/// 前回の読み始めからさかのぼる余裕 (HubSpot の Search の反映の遅れ)
const OVERLAP_MS: i64 = 5 * 60 * 1000;
const DEFINITIONS_TTL: Duration = Duration::from_secs(6 * 3600);
const OWNERS_TTL: Duration = Duration::from_secs(10 * 60);
const SEARCH_PAGE: usize = 200;
/// HubSpot の Search は 1 つの条件で 10,000 件まで
const SEARCH_MAX: usize = 10_000;
const NAME_PROPS: [&str; 3] = [
    "ouboshashimei",
    "ouboshashimei_kana",
    "oubosaki_torihiki_name",
];

// ---------------------------------------------------------------------------
// HubSpot (テストで偽物に差し替える)
// ---------------------------------------------------------------------------

pub trait CalendarHubSpot: Send + Sync {
    /// 応募のプロパティ定義 (`/crm/v3/properties/0-421` の `results`)
    fn applicant_properties(&self) -> impl Future<Output = Result<Vec<Value>, String>> + Send;
    fn search_applicants(&self, body: Value) -> impl Future<Output = Result<Value, String>> + Send;
    /// from の ID → 関連の ID (HubSpot の返した順)
    fn associations(
        &self,
        from: &str,
        to: &str,
        ids: &[String],
    ) -> impl Future<Output = Result<BTreeMap<String, Vec<String>>, String>> + Send;
    fn read(
        &self,
        object: &str,
        ids: &[String],
        props: &[&str],
    ) -> impl Future<Output = Result<Vec<HubSpotRecord>, String>> + Send;
    /// 担当者 ID → メール (有効な担当者だけ、小文字)
    fn owner_emails(&self) -> impl Future<Output = Result<HashMap<String, String>, String>> + Send;
}

impl CalendarHubSpot for HubSpotClient {
    async fn applicant_properties(&self) -> Result<Vec<Value>, String> {
        let r = self
            .read_raw(false, "/crm/v3/properties/0-421", &[], None)
            .await
            .map_err(|e| format!("properties: {e:?}"))?;
        if !(200..300).contains(&r.status) {
            return Err(format!("properties: status {}", r.status));
        }
        r.body
            .and_then(|b| b["results"].as_array().cloned())
            .ok_or_else(|| "properties: no results".to_string())
    }

    async fn search_applicants(&self, body: Value) -> Result<Value, String> {
        self.search(OBJECT, body)
            .await
            .map_err(|e| format!("search: {e:?}"))
    }

    async fn associations(
        &self,
        from: &str,
        to: &str,
        ids: &[String],
    ) -> Result<BTreeMap<String, Vec<String>>, String> {
        if ids.is_empty() {
            return Ok(BTreeMap::new());
        }
        Ok(self
            .batch_associations(from, to, ids)
            .await
            .map_err(|e| format!("associations: {e:?}"))?
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().map(|r| r.id).collect()))
            .collect())
    }

    async fn read(
        &self,
        object: &str,
        ids: &[String],
        props: &[&str],
    ) -> Result<Vec<HubSpotRecord>, String> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.batch_read(object, ids, props)
            .await
            .map_err(|e| format!("batch read: {e:?}"))
    }

    async fn owner_emails(&self) -> Result<HashMap<String, String>, String> {
        let all = crate::crm::owners::fetch_all(self)
            .await
            .map_err(|e| format!("owners: {e:?}"))?;
        Ok(all
            .owners
            .into_iter()
            .filter(|o| !o.archived)
            .filter_map(|o| Some((o.id, o.email?.trim().to_ascii_lowercase())))
            .filter(|(_, e)| !e.is_empty())
            .collect())
    }
}

// ---------------------------------------------------------------------------
// 周回
// ---------------------------------------------------------------------------

/// 周回の結果
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    pub counts: Counts,
    pub problem: Option<Problem>,
    /// Calendar が 401 を返した (access token を捨てる)
    pub auth_failed: bool,
}

#[derive(Debug, Clone)]
pub struct Params {
    /// Calendar への呼び出しの間に空ける時間 (Google の 1 分あたりの上限に当たらないように)
    pub pacing: Duration,
    /// 応募が消えた予定を探す間隔
    pub reap_every: Duration,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            pacing: Duration::from_millis(150),
            reap_every: Duration::from_secs(6 * 3600),
        }
    }
}

#[derive(Default)]
pub struct Cycle {
    /// 前回の読み始め (ミリ秒)。None なら初回の読み方
    since_ms: Option<i64>,
    /// 項目 → 前回見た型
    kinds: HashMap<String, String>,
    /// 型が変わった項目 (初回と同じ読み方で読み直す)
    resync: BTreeSet<String>,
    defs: Option<(Instant, Vec<Value>)>,
    owners: Option<(Instant, HashMap<String, String>)>,
    last_reap: Option<Instant>,
}

struct Applicant {
    id: String,
    props: BTreeMap<String, String>,
}

impl Applicant {
    fn get(&self, name: &str) -> &str {
        self.props.get(name).map(|s| s.trim()).unwrap_or("")
    }
}

/// 今日 (日本時間) − `days` 日の 0 時
fn window_start(now: DateTime<Utc>, days: i64) -> DateTime<Utc> {
    let day = now.with_timezone(&jst()).date_naive() - ChronoDuration::days(days);
    day.and_hms_opt(0, 0, 0)
        .and_then(|t| t.and_local_timezone(jst()).single())
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or(now)
}

fn text(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or("").trim().to_string()
}

fn record_of(v: &Value) -> Option<Applicant> {
    let id = match &v["id"] {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    let props = v["properties"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
        .collect();
    Some(Applicant { id, props })
}

/// 項目ごとの filterGroups の OR (1 回の Search は 5 グループまで)
pub fn date_search_bodies(props: &[String], from_ms: i64, properties: &[String]) -> Vec<Value> {
    props
        .chunks(5)
        .map(|chunk| {
            json!({
                "filterGroups": chunk.iter().map(|p| json!({"filters": [
                    {"propertyName": p, "operator": "GTE", "value": from_ms.to_string()}
                ]})).collect::<Vec<_>>(),
                "sorts": [{"propertyName": "hs_object_id", "direction": "ASCENDING"}],
                "properties": properties,
                "limit": SEARCH_PAGE,
            })
        })
        .collect()
}

pub fn modified_search_body(since_ms: i64, properties: &[String]) -> Value {
    json!({
        "filterGroups": [{"filters": [
            {"propertyName": "hs_lastmodifieddate", "operator": "GTE", "value": since_ms.to_string()}
        ]}],
        "sorts": [{"propertyName": "hs_lastmodifieddate", "direction": "ASCENDING"}],
        "properties": properties,
        "limit": SEARCH_PAGE,
    })
}

/// Search を最後のページまで (10,000 件まで)。上限に当たったら true
async fn search_all<H: CalendarHubSpot>(
    hs: &H,
    body: Value,
    out: &mut BTreeMap<String, Applicant>,
) -> Result<bool, String> {
    let mut after: Option<String> = None;
    let mut seen = 0usize;
    loop {
        let mut b = body.clone();
        if let Some(a) = &after {
            b["after"] = json!(a);
        }
        let v = hs.search_applicants(b).await?;
        for r in v["results"].as_array().into_iter().flatten() {
            seen += 1;
            if let Some(a) = record_of(r) {
                out.insert(a.id.clone(), a);
            }
        }
        let next = match &v["paging"]["next"]["after"] {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        };
        match next {
            Some(n) if seen + SEARCH_PAGE <= SEARCH_MAX && Some(&n) != after.as_ref() => {
                after = Some(n)
            }
            Some(_) => return Ok(true),
            None => return Ok(false),
        }
    }
}

/// 応募ごとのゲストと会社名
#[derive(Default)]
struct Enriched {
    guest: Option<String>,
    company: Option<String>,
}

fn unique_push(v: &mut Vec<String>, id: &str) {
    if !v.iter().any(|x| x == id) {
        v.push(id.to_string());
    }
}

impl Cycle {
    /// テスト用: 定義を次の周回で読み直させる (本番は 6 時間で読み直す)
    #[cfg(test)]
    pub fn forget_definitions(&mut self) {
        self.defs = None;
    }

    async fn definitions<H: CalendarHubSpot>(&mut self, hs: &H) -> Result<Vec<Value>, String> {
        if let Some((at, defs)) = &self.defs {
            if at.elapsed() < DEFINITIONS_TTL {
                return Ok(defs.clone());
            }
        }
        let defs = hs.applicant_properties().await?;
        self.defs = Some((Instant::now(), defs.clone()));
        Ok(defs)
    }

    async fn owners<H: CalendarHubSpot>(
        &mut self,
        hs: &H,
    ) -> Result<HashMap<String, String>, String> {
        if let Some((at, o)) = &self.owners {
            if at.elapsed() < OWNERS_TTL {
                return Ok(o.clone());
            }
        }
        let o = hs.owner_emails().await?;
        self.owners = Some((Instant::now(), o.clone()));
        Ok(o)
    }

    /// 応募 → 求人票 → 取引 (報告シートと同じ選び方) → 担当者のメール、と取引の会社名
    async fn enrich<H: CalendarHubSpot>(
        &mut self,
        hs: &H,
        ids: &[String],
    ) -> Result<HashMap<String, Enriched>, String> {
        let mut out: HashMap<String, Enriched> = HashMap::new();
        if ids.is_empty() {
            return Ok(out);
        }
        let listings_of = hs.associations(OBJECT, "0-420", ids).await?;
        let mut listings = Vec::new();
        for l in listings_of.values().flatten() {
            unique_push(&mut listings, l);
        }
        let deals_of = hs.associations("0-420", "deals", &listings).await?;
        let mut all_deals = Vec::new();
        for d in deals_of.values().flatten() {
            unique_push(&mut all_deals, d);
        }
        let deals = hs.read("deals", &all_deals, REPORT_DEAL_PROPERTIES).await?;
        let owners = if deals.is_empty() {
            HashMap::new()
        } else {
            self.owners(hs).await?
        };
        // 応募ごとに、求人票の関連の順で取引 ID を並べる (報告シートと同じ)
        let mut company_deal_of: HashMap<String, String> = HashMap::new();
        let mut company_deals = Vec::new();
        for id in ids {
            let mut deal_ids = Vec::new();
            for l in listings_of.get(id).into_iter().flatten() {
                for d in deals_of.get(l).into_iter().flatten() {
                    unique_push(&mut deal_ids, d);
                }
            }
            // 報告シートの URL を持つ取引があれば報告シートと同じ選び方、無ければ同じ順位で全取引から
            let selected =
                report_deal(&deal_ids, &deals).or_else(|| ranked_deal(&deal_ids, &deals));
            let guest = selected
                .and_then(|r| r.properties.get("hubspot_owner_id").cloned().flatten())
                .and_then(|oid| owners.get(oid.trim()).cloned());
            // 会社名は選んだ取引から (取引が読めなければ無し)
            let for_company = selected.map(|r| r.id.clone());
            if let Some(d) = for_company {
                unique_push(&mut company_deals, &d);
                company_deal_of.insert(id.clone(), d);
            }
            out.insert(
                id.clone(),
                Enriched {
                    guest,
                    company: None,
                },
            );
        }
        // 取引 → 会社 (会社が 1 つに決まるときだけ名前を使う)
        let companies_of = hs
            .associations("deals", "companies", &company_deals)
            .await?;
        let mut company_ids = Vec::new();
        for list in companies_of.values() {
            if list.len() == 1 {
                unique_push(&mut company_ids, &list[0]);
            }
        }
        let companies = hs.read("companies", &company_ids, &["name"]).await?;
        for (id, deal) in &company_deal_of {
            let name = companies_of
                .get(deal)
                .filter(|l| l.len() == 1)
                .and_then(|l| companies.iter().find(|c| c.id == l[0]))
                .and_then(|c| c.properties.get("name").cloned().flatten())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            if let Some(e) = out.get_mut(id) {
                e.company = name;
            }
        }
        Ok(out)
    }

    /// 1 周。HubSpot を読んで予定を合わせる
    pub async fn run_once<H: CalendarHubSpot, C: CalendarApi>(
        &mut self,
        hs: &H,
        cal: &C,
        cfg: &CalendarConfig,
        params: &Params,
        now: DateTime<Utc>,
        portal_id: &str,
    ) -> Outcome {
        let mut outcome = Outcome::default();
        let hubspot_failed = |e: String| {
            tracing::warn!(error = %e, "[calendar] HubSpot の読み取りに失敗");
            Outcome {
                problem: Some(Problem::HubSpot),
                counts: Counts {
                    errors: 1,
                    ..Default::default()
                },
                auth_failed: false,
            }
        };
        let defs = match self.definitions(hs).await {
            Ok(d) => d,
            Err(e) => return hubspot_failed(e),
        };
        // 項目の型 (日時 / 日付)。それ以外の型・定義に無い項目は載せない
        let mut kinds: HashMap<String, String> = HashMap::new();
        for (prop, _) in &cfg.props {
            let kind = defs
                .iter()
                .find(|d| d["name"] == prop.as_str())
                .map(|d| text(d, "type"))
                .filter(|t| t == "datetime" || t == "date");
            match kind {
                Some(k) => {
                    if self.since_ms.is_some() && self.kinds.get(prop) != Some(&k) {
                        tracing::info!(prop = %prop, kind = %k, "[calendar] 項目の型が変わったので予定を作り直す");
                        self.resync.insert(prop.clone());
                    }
                    kinds.insert(prop.clone(), k);
                }
                None => {
                    tracing::warn!(prop = %prop, "[calendar] 項目が日時・日付の型でないので載せない")
                }
            }
        }
        self.kinds = kinds.clone();
        let mut active: Vec<String> = cfg
            .props
            .iter()
            .map(|(p, _)| p.clone())
            .filter(|p| kinds.contains_key(p))
            .collect();
        active.sort();
        let job_props: Vec<String> = defs
            .iter()
            .filter(|d| {
                let name = text(d, "name");
                let label = text(d, "label");
                name.starts_with("oubosaki_")
                    && (label.contains("タイトル") || label.contains("求人名"))
            })
            .map(|d| text(d, "name"))
            .collect();
        let mut properties: Vec<String> = active.clone();
        properties.extend(NAME_PROPS.iter().map(|s| s.to_string()));
        properties.extend(job_props.iter().cloned());
        properties.push("hs_lastmodifieddate".into());

        // ---- 読む ----
        let start_ms = now.timestamp_millis();
        let from_ms = window_start(now, cfg.lookback_days).timestamp_millis();
        let mut applicants: BTreeMap<String, Applicant> = BTreeMap::new();
        let mut next_since = start_ms;
        let read = async {
            match self.since_ms {
                None => {
                    for body in date_search_bodies(&active, from_ms, &properties) {
                        if search_all(hs, body, &mut applicants).await? {
                            tracing::warn!("[calendar] 初回の対象が多すぎて一部しか読めなかった");
                        }
                    }
                }
                Some(since) => {
                    if search_all(
                        hs,
                        modified_search_body(since - OVERLAP_MS, &properties),
                        &mut applicants,
                    )
                    .await?
                    {
                        // 続きは次の周回で (最後に読んだ更新時刻から)
                        if let Some(last) = applicants
                            .values()
                            .filter_map(|a| when_of("datetime", a.get("hs_lastmodifieddate")))
                            .filter_map(|w| match w {
                                When::Timed(t) => Some(t.timestamp_millis()),
                                When::AllDay(_) => None,
                            })
                            .max()
                        {
                            next_since = last + OVERLAP_MS;
                        }
                    }
                    let resync: Vec<String> = self
                        .resync
                        .iter()
                        .filter(|p| kinds.contains_key(*p))
                        .cloned()
                        .collect();
                    for body in date_search_bodies(&resync, from_ms, &properties) {
                        search_all(hs, body, &mut applicants).await?;
                    }
                }
            }
            Ok::<_, String>(())
        };
        if let Err(e) = read.await {
            return hubspot_failed(e);
        }

        // ---- 載せたい予定 ----
        let with_dates: Vec<String> = applicants
            .values()
            .filter(|a| {
                active
                    .iter()
                    .any(|p| when_of(&kinds[p], a.get(p)).is_some())
            })
            .map(|a| a.id.clone())
            .collect();
        let enriched = match self.enrich(hs, &with_dates).await {
            Ok(e) => e,
            Err(e) => return hubspot_failed(e),
        };
        let time_min = from_ms;
        let time_min_dt = DateTime::<Utc>::from_timestamp_millis(time_min).unwrap_or(now);
        let listed = match cal.list_ours(time_min_dt).await {
            Ok(l) => l,
            Err(e) => {
                outcome.problem = Some(Problem::Google);
                outcome.auth_failed = e == CalError::Auth;
                outcome.counts.errors = 1;
                return outcome;
            }
        };
        let mut existing: HashMap<String, Value> = listed
            .iter()
            .filter_map(|e| Some((e["id"].as_str()?.to_string(), e.clone())))
            .collect();

        let minutes = cfg.event_minutes;
        let mut stopped = false;
        'outer: for a in applicants.values() {
            let extra = enriched.get(&a.id);
            let name = Some(a.get("ouboshashimei"))
                .filter(|s| !s.is_empty())
                .unwrap_or(a.get("ouboshashimei_kana"))
                .to_string();
            let company = extra
                .and_then(|e| e.company.clone())
                .unwrap_or_else(|| a.get("oubosaki_torihiki_name").to_string());
            let job = job_props
                .iter()
                .map(|p| a.get(p))
                .find(|s| !s.is_empty())
                .unwrap_or("")
                .to_string();
            for prop in &active {
                let id = event_id(&a.id, prop);
                let desired = when_of(&kinds[prop], a.get(prop)).map(|when| Desired {
                    applicant_id: a.id.clone(),
                    prop: prop.clone(),
                    summary: summary(cfg.label_of(prop).unwrap_or(prop), &name, &company),
                    description: description(
                        &name,
                        &company,
                        &job,
                        &applicant_link(portal_id, &a.id),
                    ),
                    when,
                    attendees: extra.and_then(|e| e.guest.clone()).into_iter().collect(),
                });
                let result = self
                    .apply(
                        cal,
                        &id,
                        desired.as_ref(),
                        existing.remove(&id),
                        time_min,
                        minutes,
                        &mut outcome.counts,
                    )
                    .await;
                if let Err(e) = result {
                    outcome.counts.errors += 1;
                    outcome.problem = Some(Problem::Google);
                    if matches!(e, CalError::Auth | CalError::RateLimited) {
                        outcome.auth_failed = e == CalError::Auth;
                        stopped = true;
                        break 'outer;
                    }
                }
                if !params.pacing.is_zero() {
                    tokio::time::sleep(params.pacing).await;
                }
            }
        }

        // ---- 応募が消えた予定 (6 時間に 1 回) ----
        let reap_due = self
            .last_reap
            .is_none_or(|t| t.elapsed() >= params.reap_every);
        if !stopped && reap_due {
            match self
                .reap(
                    hs,
                    cal,
                    &listed,
                    &applicants,
                    cfg,
                    &mut outcome.counts,
                    params,
                )
                .await
            {
                Ok(()) => self.last_reap = Some(Instant::now()),
                Err(Some(e)) => {
                    outcome.counts.errors += 1;
                    outcome.problem = Some(Problem::Google);
                    outcome.auth_failed = e == CalError::Auth;
                }
                Err(None) => {
                    outcome.counts.errors += 1;
                    outcome.problem.get_or_insert(Problem::HubSpot);
                }
            }
        }

        if outcome.counts.errors == 0 {
            self.since_ms = Some(next_since);
            self.resync.clear();
        }
        outcome
    }

    /// 1 件の予定を合わせる
    #[allow(clippy::too_many_arguments)]
    async fn apply<C: CalendarApi>(
        &self,
        cal: &C,
        id: &str,
        desired: Option<&Desired>,
        listed: Option<Value>,
        time_min_ms: i64,
        minutes: i64,
        counts: &mut Counts,
    ) -> Result<(), CalError> {
        let Some(d) = desired else {
            // 値が空になった: 自分が作った予定なら消す (一覧に無い = 無い / 消し済み / 範囲より前)
            if listed.is_some() {
                cal.delete(id).await?;
                counts.deleted += 1;
            }
            return Ok(());
        };
        let end_ms = match d.when {
            When::Timed(t) => (t + ChronoDuration::minutes(minutes)).timestamp_millis(),
            When::AllDay(day) => day
                .succ_opt()
                .and_then(|n| n.and_hms_opt(0, 0, 0))
                .and_then(|t| t.and_local_timezone(jst()).single())
                .map(|t| t.timestamp_millis())
                .unwrap_or(i64::MAX),
        };
        // 一覧の範囲より前に終わる予定は一覧に出ないので、書く前に読む
        let current = match listed {
            Some(e) => Some(e),
            None if end_ms <= time_min_ms => cal.get(id).await?,
            None => None,
        };
        match current {
            Some(e) if same(&e, d, minutes) => {
                counts.unchanged += 1;
            }
            Some(_) => {
                cal.patch(id, &patch_body(d, minutes)).await?;
                counts.updated += 1;
            }
            None => match cal.insert(&insert_body(d, minutes)).await? {
                Inserted::Created => counts.created += 1,
                // すでにある (削除済みを含む): 上書きして表示に戻す
                Inserted::Exists => {
                    cal.patch(id, &patch_body(d, minutes)).await?;
                    counts.updated += 1;
                }
            },
        }
        Ok(())
    }

    /// 応募が消えた (読めない)・載せる項目から外した予定を消す。
    /// Err(None) は HubSpot を読めなかった (何も消さない)
    #[allow(clippy::too_many_arguments)]
    async fn reap<H: CalendarHubSpot, C: CalendarApi>(
        &self,
        hs: &H,
        cal: &C,
        listed: &[Value],
        fetched: &BTreeMap<String, Applicant>,
        cfg: &CalendarConfig,
        counts: &mut Counts,
        params: &Params,
    ) -> Result<(), Option<CalError>> {
        let configured: HashSet<&str> = cfg.props.iter().map(|(p, _)| p.as_str()).collect();
        let mut by_applicant: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        for e in listed {
            let private = &e["extendedProperties"]["private"];
            let (Some(id), Some(aid), Some(prop)) = (
                e["id"].as_str(),
                private[APPLICANT_KEY].as_str(),
                private[PROP_KEY].as_str(),
            ) else {
                continue;
            };
            // 予定の ID が応募と項目から作ったものと一致するものだけ (手で印を付けた予定は触らない)
            if event_id(aid, prop) != id {
                continue;
            }
            by_applicant
                .entry(aid.to_string())
                .or_default()
                .push((id.to_string(), prop.to_string()));
        }
        let unknown: Vec<String> = by_applicant
            .keys()
            .filter(|a| !fetched.contains_key(*a) && a.bytes().all(|b| b.is_ascii_digit()))
            .cloned()
            .collect();
        let alive: HashSet<String> = hs
            .read(OBJECT, &unknown, &["hs_object_id"])
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "[calendar] 応募が残っているか確かめられなかった");
                None
            })?
            .into_iter()
            .map(|r| r.id)
            .collect();
        for (aid, events) in &by_applicant {
            let gone = unknown.contains(aid) && !alive.contains(aid);
            for (id, prop) in events {
                if gone || !configured.contains(prop.as_str()) {
                    cal.delete(id).await.map_err(Some)?;
                    counts.deleted += 1;
                    if !params.pacing.is_zero() {
                        tokio::time::sleep(params.pacing).await;
                    }
                }
            }
        }
        Ok(())
    }
}

/// 起動。`APPLICANT_CALENDAR_ENABLED=1`・許可の設定・HubSpot がそろっていなければ何もしない
pub fn spawn_worker(state: Arc<AppState>) {
    let cfg = CalendarConfig::from_env();
    if !cfg.enabled {
        return;
    }
    let Some(svc) = super::ApplicantCalendar::global(state.audit.clone()) else {
        return;
    };
    let Some(client) = state.crm_hubspot() else {
        tracing::warn!("[calendar] HubSpot が未接続のため起動しません");
        return;
    };
    tracing::info!(
        props = cfg.props.len(),
        "[calendar] 応募の日付をカレンダーに載せる周回を始める"
    );
    let bg = client.background();
    let portal_id = crate::hubspot::deep_link::hubspot_portal_id();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        let mut cycle = Cycle::default();
        let params = Params::default();
        loop {
            match svc.access().await {
                Err(p) => svc.record(Counts::default(), Some(p), false),
                Ok(token) => {
                    let cal = svc.calendar(token);
                    let o = cycle
                        .run_once(&bg, &cal, &svc.cfg, &params, Utc::now(), &portal_id)
                        .await;
                    if o.auth_failed {
                        svc.forget_token().await;
                    }
                    let c = &o.counts;
                    if c.created + c.updated + c.deleted + c.errors > 0 {
                        tracing::info!(
                            created = c.created,
                            updated = c.updated,
                            deleted = c.deleted,
                            errors = c.errors,
                            "[calendar] cycle"
                        );
                    }
                    let finished = o.problem.is_none();
                    svc.record(o.counts, o.problem, finished);
                }
            }
            tokio::time::sleep(svc.cfg.interval).await;
        }
    });
}
