//! 納品管理全体の担当・予定。求人のない取引も含め、読み取りだけを背景で更新する。
use super::*;
use chrono::{NaiveDate, Utc};
use std::time::Instant;
#[path = "todo_refresh.rs"]
mod facts_refresh;

pub(super) const DEAL_PROPERTIES: &[&str] = &[
    "amount",
    "oubo",
    "mensetu",
    "syoudaku",
    "saiyomokuhyou",
    "contract_period",
    "nps",
    "nps2",
    "nps3",
    "nps4",
    "nps5",
    "nps6",
    "nps7",
    "nps8",
    "nps9",
    "nps10",
    "nps11",
    "dealname",
    "pipeline",
    "dealstage",
    "createdate",
    "consultant",
    "hubspot_owner_id",
    "actionyotei",
    "actionnaiyou",
    "contract_expiration_date",
    "contract_start_date",
    "contract_type",
    "notes_last_contacted",
];
/// HubSpot の closed / probability は業務上の終了と異なるため参照しない。
pub(super) const ENDED_STAGES: [&str; 6] = [
    "1016664339",
    "90598807",
    "52016159",
    "52016158",
    "66848546",
    "1082007429",
];
const TTL: Duration = Duration::from_secs(5 * 60);
const FACTS_TTL: Duration = Duration::from_secs(60 * 60);
const OWNER_TTL: Duration = Duration::from_secs(10 * 60);
const RETRY: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub(super) struct OwnerQuery {
    pub(super) owner_field: Option<String>,
    pub(super) owner: Option<String>,
}
impl OwnerQuery {
    pub(super) fn validate(&self) -> Result<(), ReadError> {
        if self
            .owner_field
            .as_deref()
            .is_some_and(|v| !["consultant", "hubspot_owner_id"].contains(&v))
        {
            return Err(ReadError(StatusCode::BAD_REQUEST, "listing_filter_invalid"));
        }
        if let Some(id) = self
            .owner
            .as_deref()
            .filter(|id| !["all", "me"].contains(id))
        {
            valid_id(id)?;
        }
        Ok(())
    }
    pub(super) fn field(&self) -> &str {
        self.owner_field.as_deref().unwrap_or("consultant")
    }
}
#[derive(Clone, Debug, Serialize)]
pub(super) struct Owner {
    pub(super) id: String,
    pub(super) name: String,
    #[serde(skip)]
    email: String,
    archived: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Assignment {
    #[serde(default)]
    pub(super) acquired: bool,
    pub(super) consultant: Option<String>,
    pub(super) hubspot_owner_id: Option<String>,
    pub(super) actionyotei: Option<String>,
    pub(super) actionnaiyou: Option<String>,
    pub(super) contract_expiration_date: Option<String>,
}
impl Assignment {
    pub(super) fn from_record(record: &Record) -> Self {
        let get = |key| {
            record
                .value(key)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };
        Self {
            acquired: true,
            consultant: get("consultant"),
            hubspot_owner_id: get("hubspot_owner_id"),
            actionyotei: get("actionyotei"),
            actionnaiyou: get("actionnaiyou"),
            contract_expiration_date: get("contract_expiration_date"),
        }
    }
    fn assigned(&self, field: &str) -> Option<&str> {
        if field == "hubspot_owner_id" {
            self.hubspot_owner_id.as_deref()
        } else {
            self.consultant.as_deref()
        }
    }
}
#[derive(Clone, Debug)]
pub(super) struct WorkDeal {
    pub(super) record: Record,
    pub(super) assignment: Assignment,
}
#[derive(Clone, Default)]
pub(super) struct Snapshot {
    pub(super) complete: bool,
    pub(super) deals: BTreeMap<String, WorkDeal>,
    pub(super) owners: Vec<Owner>,
    labels: BTreeMap<String, String>,
    pub(super) companies: BTreeMap<String, (String, Option<String>)>,
    pub(super) stages: BTreeMap<String, String>,
    pub(super) facts: BTreeMap<String, super::todo_signals::Facts>,
}
#[derive(Default)]
pub(super) struct Cache {
    state: tokio::sync::Mutex<CacheState>,
    owners: tokio::sync::Mutex<Option<(Instant, Vec<Owner>)>>,
    labels: tokio::sync::Mutex<Option<(Instant, BTreeMap<String, String>)>>,
}
#[derive(Default)]
struct CacheState {
    revision: u64,
    snapshot: Option<(Instant, Arc<Snapshot>)>,
    refreshing: bool,
    failed_at: Option<Instant>,
    facts_snapshot: Option<(Instant, BTreeMap<String, super::todo_signals::Facts>)>,
    facts_built_at: Option<chrono::DateTime<Utc>>,
    facts_refreshing: bool,
    facts_pending: Option<facts_refresh::Progress>,
    facts_failed_at: Option<Instant>,
}
pub(super) async fn restore_facts(service: &JobReadService) {
    facts_refresh::restore(service).await;
}

impl CacheState {
    fn sync_revision(&mut self, revision: u64) {
        if self.revision != revision {
            self.revision = revision;
            self.snapshot = None;
            self.failed_at = None;
        }
    }
}
pub(super) struct View {
    pub(super) snapshot: Option<Arc<Snapshot>>,
    pub(super) failed: bool,
    pub(super) partial: bool,
    pub(super) refreshing: bool,
    pub(super) facts_built_at: Option<chrono::DateTime<Utc>>,
    pub(super) facts_stale: bool,
}
/// 初回も待たない。取得中は UI が集計中と表示し、次回の要求で結果を読む。
pub(super) async fn view(service: &JobReadService) -> View {
    view_inner(service, false).await
}
/// 重い応募の更新は、ホーム／次にやることを開く要求だけが開始する。
pub(super) async fn work_view(service: &JobReadService) -> View {
    view_inner(service, true).await
}
async fn view_inner(service: &JobReadService, include_facts: bool) -> View {
    report_snapshot::ensure_restored(service).await;
    let mut state = service.owner_todo.state.lock().await;
    let revision = service.hs.deal_write_revision();
    state.sync_revision(revision);
    let fresh = state
        .snapshot
        .as_ref()
        .is_some_and(|(at, _)| at.elapsed() < TTL);
    if !fresh && !state.refreshing && !state.failed_at.is_some_and(|at| at.elapsed() < RETRY) {
        state.refreshing = true;
        let mut service = service.clone();
        service.hs = service.hs.background();
        tokio::spawn(async move {
            let result = tokio::time::timeout(Duration::from_secs(120), fetch(&service))
                .await
                .unwrap_or(Err(ReadError(
                    StatusCode::GATEWAY_TIMEOUT,
                    "todo_owner_timeout",
                )));
            let mut state = service.owner_todo.state.lock().await;
            // 保存前に始めた読み取りは、保存後のキャッシュへ戻さない。
            if service.hs.deal_write_revision() != revision {
                state.refreshing = false;
                return;
            }
            match result {
                Ok(mut snapshot) => {
                    if let Some((_, facts)) = &state.facts_snapshot {
                        snapshot.facts = facts.clone();
                    }
                    state.snapshot = Some((Instant::now(), Arc::new(snapshot)));
                    state.failed_at = None;
                }
                Err(_) => {
                    state.failed_at = Some(Instant::now());
                    tracing::warn!("job-copy owner/todo refresh failed");
                }
            }
            state.refreshing = false;
        });
    }
    // 期限切れの確認は画面からの要求時だけ。取引の更新や保存で60分の期限を変えない。
    if include_facts {
        refresh_facts_on_request(service, &mut state);
    }
    let refreshing = state.refreshing || (include_facts && state.facts_refreshing);
    let failed = state.failed_at.is_some() || (include_facts && state.facts_failed_at.is_some());
    let partial = include_facts && state.facts_snapshot.is_none();
    let snapshot = state.snapshot.as_ref().map(|(_, s)| s.clone());
    let facts_built_at = state.facts_built_at;
    let facts_stale = state
        .facts_snapshot
        .as_ref()
        .is_some_and(|(at, _)| at.elapsed() >= FACTS_TTL);
    drop(state);
    if snapshot.is_some() {
        return View {
            snapshot,
            failed,
            partial,
            refreshing,
            facts_built_at,
            facts_stale,
        };
    }
    // owners が届いたら、従来の索引でホームのカードを先に絞る。
    // 全件 Search の完了までは、やることの件数を 0 として表示しない。
    let owners = service
        .owner_todo
        .owners
        .try_lock()
        .ok()
        .and_then(|cache| cache.as_ref().map(|(_, owners)| owners.clone()));
    let snapshot = if let Some(owners) = owners {
        listings::ownership_from_index(service, owners).await
    } else {
        None
    };
    View {
        snapshot,
        failed,
        partial: true,
        refreshing,
        facts_built_at,
        facts_stale,
    }
}
/// 求人・応募は別の単一実行枠で更新する。取引の5分更新を待たせない。
fn refresh_facts_on_request(service: &JobReadService, state: &mut CacheState) {
    if state.refreshing
        || state.facts_refreshing
        || state
            .facts_snapshot
            .as_ref()
            .is_some_and(|(at, _)| at.elapsed() < FACTS_TTL)
        || state.facts_failed_at.is_some_and(|at| at.elapsed() < RETRY)
    {
        return;
    }
    let Some((_, snapshot)) = &state.snapshot else {
        return;
    };
    let deals = snapshot
        .deals
        .iter()
        .filter(|(_, deal)| super::todo_signals::eligible(deal))
        .map(|(id, deal)| (id.clone(), deal.clone()))
        .collect();
    state.facts_refreshing = true;
    let mut service = service.clone();
    service.hs = service.hs.background();
    tokio::spawn(async move {
        facts_refresh::run(&service, &deals).await;
    });
}

fn parse_owners(data: &Value, archived: bool) -> Result<Vec<Owner>, ReadError> {
    data["results"]
        .as_array()
        .ok_or_else(|| fail("hubspot_invalid_response"))?
        .iter()
        .map(|row| {
            let id = row["id"]
                .as_str()
                .ok_or_else(|| fail("hubspot_invalid_response"))?
                .to_owned();
            valid_id(&id)?;
            let name = [
                row["lastName"].as_str().unwrap_or(""),
                row["firstName"].as_str().unwrap_or(""),
            ]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
            Ok(Owner {
                name: if name.is_empty() {
                    "氏名未登録".into()
                } else {
                    name
                },
                id,
                email: row["email"].as_str().unwrap_or("").trim().to_lowercase(),
                archived,
            })
        })
        .collect()
}
async fn owners(service: &JobReadService) -> Result<Vec<Owner>, ReadError> {
    let mut cache = service.owner_todo.owners.lock().await;
    if let Some((at, owners)) = cache.as_ref() {
        if at.elapsed() < OWNER_TTL {
            return Ok(owners.clone());
        }
    }
    let mut result = vec![];
    for archived in [false, true] {
        let mut after = None;
        let mut seen = BTreeSet::new();
        loop {
            let data = service
                .hs
                .owners_page(archived, after.as_deref())
                .await
                .map_err(client_error)?;
            result.extend(parse_owners(&data, archived)?);
            after = next(&data)?;
            match &after {
                None => break,
                Some(id) if seen.insert(id.clone()) => {}
                _ => return Err(fail("hubspot_invalid_response")),
            }
        }
    }
    result.sort_by(|a, b| {
        a.archived
            .cmp(&b.archived)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    });
    *cache = Some((Instant::now(), result.clone()));
    Ok(result)
}
fn next(data: &Value) -> Result<Option<String>, ReadError> {
    let Some(paging) = data.get("paging") else {
        return Ok(None);
    };
    let value = &paging["next"]["after"];
    let after = value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_u64().map(|v| v.to_string()))
        .ok_or_else(|| fail("hubspot_invalid_response"))?;
    // owners の継続キーは数値IDではなく不透明な文字列。query として渡し、URLとして使わない。
    if after.is_empty() || after.len() > 1024 || after.bytes().any(|b| b.is_ascii_control()) {
        return Err(fail("hubspot_invalid_response"));
    }
    Ok(Some(after))
}
async fn fetch(service: &JobReadService) -> Result<Snapshot, ReadError> {
    let started = Instant::now();
    let owners = owners(service).await?;
    let mut deals = BTreeMap::new();
    let mut after = None;
    let mut seen = BTreeSet::new();
    let mut calls = 0;
    loop {
        let mut body = json!({"filterGroups": listings::delivery_pipelines().iter().map(|(id, _)| json!({"filters":[{"propertyName":"pipeline","operator":"EQ","value":id}]})).collect::<Vec<_>>(), "properties": DEAL_PROPERTIES, "limit":200});
        if let Some(after) = &after {
            body["after"] = json!(after);
        }
        let data = service
            .hs
            .search("deals", body)
            .await
            .map_err(client_error)?;
        calls += 1;
        let page: Page =
            serde_json::from_value(data.clone()).map_err(|_| fail("hubspot_invalid_response"))?;
        for record in page.results {
            if !listings::is_delivery(record.value("pipeline")) || deals.contains_key(&record.id) {
                return Err(fail("hubspot_invalid_response"));
            }
            deals.insert(
                record.id.clone(),
                WorkDeal {
                    assignment: Assignment::from_record(&record),
                    record,
                },
            );
        }
        after = next(&data)?;
        match &after {
            None => break,
            Some(id) if seen.insert(id.clone()) && deals.len() < 10_000 => {}
            _ => return Err(fail("hubspot_invalid_response")),
        }
    }
    let mut label_cache = service.owner_todo.labels.lock().await;
    let labels = if let Some((at, labels)) = label_cache
        .as_ref()
        .filter(|(at, _)| at.elapsed() < Duration::from_secs(6 * 60 * 60))
    {
        let _ = at;
        labels.clone()
    } else {
        let defs = service
            .request("/crm/v3/properties/deals/actionnaiyou", &[], None)
            .await?;
        let labels: BTreeMap<String, String> = defs["options"]
            .as_array()
            .ok_or_else(|| fail("hubspot_invalid_response"))?
            .iter()
            .filter_map(|v| Some((v["value"].as_str()?.into(), v["label"].as_str()?.into())))
            .collect();
        *label_cache = Some((Instant::now(), labels.clone()));
        labels
    };
    drop(label_cache);
    let defs = listings::todo_stages(service).await;
    // 一覧で終了した案件も開けるよう、求人や要対応がない案件の会社名も補う。
    let ids: Vec<_> = deals.keys().cloned().collect();
    let companies = listings::todo_companies(service, &ids).await?;
    tracing::info!(
        count = deals.len(),
        search_calls = calls,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "job-copy owner/todo full delivery read"
    );
    Ok(Snapshot {
        complete: true,
        deals,
        owners,
        labels,
        companies,
        stages: defs,
        facts: BTreeMap::new(),
    })
}
impl Snapshot {
    pub(super) fn for_index(deals: BTreeMap<String, WorkDeal>, owners: Vec<Owner>) -> Self {
        Self {
            deals,
            owners,
            ..Self::default()
        }
    }
    pub(super) fn selection(
        &self,
        query: &OwnerQuery,
        email: &str,
    ) -> (Option<String>, Option<&'static str>) {
        match query.owner.as_deref().unwrap_or("me") {
            "all" => (None, None),
            "me" => {
                let ids: BTreeSet<_> = self
                    .owners
                    .iter()
                    .filter(|o| !o.email.is_empty() && o.email.eq_ignore_ascii_case(email.trim()))
                    .map(|o| o.id.clone())
                    .collect();
                if ids.len() == 1 {
                    (ids.into_iter().next(), None)
                } else {
                    (
                        None,
                        Some(
                            "ログインした方の担当者情報が見つからないため、全員を表示しています。",
                        ),
                    )
                }
            }
            id => (Some(id.to_owned()), None),
        }
    }
    pub(super) fn matches(&self, deal: &str, field: &str, owner: Option<&str>) -> bool {
        owner.is_none_or(|id| {
            self.deals
                .get(deal)
                .is_some_and(|d| d.assignment.assigned(field) == Some(id))
        })
    }
    pub(super) fn scope(&self, query: &OwnerQuery, email: &str) -> Value {
        let (owner, notice) = self.selection(query, email);
        let field = query.field();
        let assigned: BTreeSet<_> = self
            .deals
            .values()
            .filter_map(|d| d.assignment.assigned(field))
            .collect();
        let options: Vec<_> = self.owners.iter().filter(|o| assigned.contains(o.id.as_str())).map(|o| json!({"id":o.id,"name":format!("{}{}",o.name,if o.archived {"（退職）"} else {""}),"archived":o.archived})).collect();
        json!({"status":"ready", "owner":owner.as_deref().unwrap_or("all"), "owner_field":field, "notice":notice, "owners":options})
    }
    pub(super) fn action_label(&self, value: Option<&str>) -> Option<&str> {
        value.and_then(|v| self.labels.get(v)).map(String::as_str)
    }
    #[cfg(test)]
    pub(super) fn report(
        &self,
        query: &OwnerQuery,
        email: &str,
        today: NaiveDate,
        failed: bool,
    ) -> Value {
        self.report_with_metrics(
            query,
            email,
            today,
            failed,
            &super::deal_board::MeetingData::default(),
        )
    }
    pub(super) fn report_with_metrics(
        &self,
        query: &OwnerQuery,
        email: &str,
        today: NaiveDate,
        failed: bool,
        meetings: &super::deal_board::MeetingData,
    ) -> Value {
        let metrics = super::deal_board::Metrics::new(self, meetings, today);
        let scope = self.scope(query, email);
        let (owner, _) = self.selection(query, email);
        let field = query.field();
        let mut actions = BTreeMap::<&str, Vec<Value>>::from([
            ("overdue", vec![]),
            ("today", vec![]),
            ("week", vec![]),
            ("later", vec![]),
        ]);
        let mut expirations = vec![];
        let mut signals = vec![];
        for (id, deal) in &self.deals {
            if !self.matches(id, field, owner.as_deref())
                || !listings::is_delivery(deal.record.value("pipeline"))
                || ENDED_STAGES.contains(&deal.record.value("dealstage").unwrap_or(""))
                // 紹介料の取引はコンサルの契約ではないので、やること・期限・兆候に出さない
                || crate::handlers::cs_dashboard::is_referral_deal(
                    deal.record.value("dealname").unwrap_or(""),
                    deal.record.value("dealstage").unwrap_or(""),
                )
            {
                continue;
            }
            let a = &deal.assignment;
            let reasons = metrics.todo_reasons(self, id, today);
            let row = |due: NaiveDate, content: Option<&str>| {
                let company = self.companies.get(id);
                let consultant = self
                    .owners
                    .iter()
                    .find(|o| Some(o.id.as_str()) == a.consultant.as_deref());
                json!({"id":id,"name":deal.record.value("dealname"),"company_id":company.map(|c| &c.0),"company":company.and_then(|c| c.1.as_ref()),"stage":deal.record.value("dealstage").and_then(|s| self.stages.get(s)),"content":content,"due":due.to_string(),"reasons":reasons,"actionyotei":a.actionyotei,"actionnaiyou":a.actionnaiyou,"consultant":consultant.map(|o| format!("{}{}",o.name,if o.archived {"（退職）"} else {""}))})
            };
            if !reasons.is_empty() {
                let mut signal = row(today, None);
                signal["reasons"] = json!(reasons);
                signal["expiration"] = json!(a
                    .contract_expiration_date
                    .as_deref()
                    .and_then(date)
                    .map(|d| d.to_string()));
                signal["content"] =
                    json!(a.actionnaiyou.as_deref().and_then(|v| self.labels.get(v)));
                signals.push(signal);
            }
            if let Some(due) = a.actionyotei.as_deref().and_then(date) {
                let key = bucket(today, due);
                actions.get_mut(key).unwrap().push(row(
                    due,
                    a.actionnaiyou
                        .as_deref()
                        .and_then(|v| self.labels.get(v))
                        .map(String::as_str),
                ));
            }
            if let Some(due) = a
                .contract_expiration_date
                .as_deref()
                .and_then(date)
                .filter(|d| {
                    (0..=super::todo_signals::EXPIRATION_DAYS).contains(&(*d - today).num_days())
                })
            {
                expirations.push(row(due, Some("契約満了")));
            }
        }
        let sort = |rows: &mut Vec<Value>| {
            rows.sort_by(|a, b| {
                a["due"]
                    .as_str()
                    .cmp(&b["due"].as_str())
                    .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
            })
        };
        actions.values_mut().for_each(sort);
        sort(&mut expirations);
        signals.sort_by(|a, b| {
            b["reasons"]
                .as_array()
                .map(Vec::len)
                .cmp(&a["reasons"].as_array().map(Vec::len))
                .then_with(|| {
                    a["expiration"]
                        .as_str()
                        .unwrap_or("9999")
                        .cmp(b["expiration"].as_str().unwrap_or("9999"))
                })
                .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
        });
        json!({"status":"ready","owner":owner.as_deref().unwrap_or("all"),"owner_field":field,"notice":scope["notice"],"owners":scope["owners"],"actions":actions,"expirations":expirations,"signals":signals,"hubspot_portal_id":crate::hubspot::deep_link::hubspot_portal_id(),"sheet_as_of":meetings.as_of,"refresh_failed":failed})
    }
}
pub(super) fn today(now: chrono::DateTime<Utc>) -> NaiveDate {
    (now + chrono::Duration::hours(9)).date_naive()
}
pub(super) fn date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .or_else(|| {
            chrono::DateTime::parse_from_rfc3339(value)
                .ok()
                .map(|d| today(d.with_timezone(&Utc)))
        })
        .or_else(|| {
            value
                .parse::<i64>()
                .ok()
                .and_then(chrono::DateTime::from_timestamp_millis)
                .map(today)
        })
}
fn bucket(today: NaiveDate, due: NaiveDate) -> &'static str {
    match (due - today).num_days() {
        n if n < 0 => "overdue",
        0 => "today",
        1..=7 => "week",
        _ => "later",
    }
}

#[cfg(test)]
pub(super) async fn seed_empty(service: &JobReadService) {
    let mut state = service.owner_todo.state.lock().await;
    state.snapshot = Some((Instant::now(), Arc::new(Snapshot::default())));
    state.facts_snapshot = Some((Instant::now(), BTreeMap::new()));
}
#[cfg(test)]
#[path = "owner_todo_tests.rs"]
mod tests;

/// 案件の画面もホームと同じ判定・同じ背景キャッシュを使う。
pub(super) async fn read_deal(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    valid_id(&id)?;
    let view = work_view(listings::service(&access)?).await;
    let body = if let Some(snapshot) = view.snapshot.filter(|_| !view.partial) {
        let meetings = super::deal_board::MeetingData::cached().await;
        let day = today(Utc::now());
        let reasons = super::deal_board::Metrics::new(&snapshot, &meetings, day)
            .todo_reasons(&snapshot, &id, day);
        json!({"status":"ready","reasons":reasons,"refresh_failed":view.failed,"refreshing":view.refreshing})
    } else {
        json!({"status":if view.failed {"failed"} else {"preparing"}})
    };
    Ok(([(header::CACHE_CONTROL, "private, no-store")], Json(body)))
}

/// 保存済み取引だけを最新化し、応募の背景集計を毎回やり直さない。
pub(super) async fn apply_fresh_deal(service: &JobReadService, record: Record) {
    let revision = service.hs.deal_write_revision();
    let needs_company = service
        .owner_todo
        .state
        .lock()
        .await
        .snapshot
        .as_ref()
        .is_some_and(|(_, s)| !s.companies.contains_key(&record.id));
    let companies = if needs_company {
        listings::todo_companies(service, std::slice::from_ref(&record.id))
            .await
            .unwrap_or_default()
    } else {
        BTreeMap::new()
    };
    let mut state = service.owner_todo.state.lock().await;
    if revision != service.hs.deal_write_revision() {
        return;
    }
    state.revision = revision;
    if let Some((_, snapshot)) = state.snapshot.as_mut() {
        let snapshot = Arc::make_mut(snapshot);
        snapshot.companies.extend(companies);
        snapshot.deals.insert(
            record.id.clone(),
            WorkDeal {
                assignment: Assignment::from_record(&record),
                record,
            },
        );
    }
}
