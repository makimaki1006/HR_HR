//! 納品管理全体の担当・予定。求人のない取引も含め、読み取りだけを背景で更新する。
use super::*;
use chrono::{NaiveDate, Utc};
use std::time::Instant;

pub(super) const DEAL_PROPERTIES: &[&str] = &[
    "dealname",
    "pipeline",
    "dealstage",
    "createdate",
    "consultant",
    "hubspot_owner_id",
    "actionyotei",
    "actionnaiyou",
    "contract_expiration_date",
];
/// HubSpot の closed / probability は業務上の終了と異なるため参照しない。
const ENDED_STAGES: [&str; 6] = [
    "1016664339",
    "90598807",
    "52016159",
    "52016158",
    "66848546",
    "1082007429",
];
const TTL: Duration = Duration::from_secs(5 * 60);
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
    id: String,
    name: String,
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
#[derive(Default)]
pub(super) struct Snapshot {
    pub(super) deals: BTreeMap<String, WorkDeal>,
    pub(super) owners: Vec<Owner>,
    labels: BTreeMap<String, String>,
    pub(super) companies: BTreeMap<String, (String, Option<String>)>,
    stages: BTreeMap<String, String>,
}
#[derive(Default)]
pub(super) struct Cache {
    state: tokio::sync::Mutex<CacheState>,
    owners: tokio::sync::Mutex<Option<(Instant, Vec<Owner>)>>,
    labels: tokio::sync::Mutex<Option<(Instant, BTreeMap<String, String>)>>,
}
#[derive(Default)]
struct CacheState {
    snapshot: Option<(Instant, Arc<Snapshot>)>,
    refreshing: bool,
    failed_at: Option<Instant>,
}
pub(super) struct View {
    pub(super) snapshot: Option<Arc<Snapshot>>,
    pub(super) failed: bool,
    pub(super) partial: bool,
}
/// 初回も待たない。取得中は UI が集計中と表示し、次回の要求で結果を読む。
pub(super) async fn view(service: &JobReadService) -> View {
    let mut state = service.owner_todo.state.lock().await;
    let fresh = state
        .snapshot
        .as_ref()
        .is_some_and(|(at, _)| at.elapsed() < TTL);
    if !fresh && !state.refreshing && !state.failed_at.is_some_and(|at| at.elapsed() < RETRY) {
        state.refreshing = true;
        let mut service = service.clone();
        service.hs = service.hs.background();
        tokio::spawn(async move {
            let result = fetch(&service).await;
            let mut state = service.owner_todo.state.lock().await;
            match result {
                Ok(snapshot) => {
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
    let failed = state.failed_at.is_some();
    let snapshot = state.snapshot.as_ref().map(|(_, s)| s.clone());
    drop(state);
    if snapshot.is_some() {
        return View {
            snapshot,
            failed,
            partial: false,
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
    }
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
    // 会社名が必要なのは予定一覧に出る取引だけ。担当者候補のために読んだ終了済み等の取引まで補完しない。
    // 日付が変わる直前のキャッシュも使えるよう、満了日は翌日の30日後まで先読みする。
    let current_day = today(Utc::now());
    let ids: Vec<_> = deals
        .iter()
        .filter(|(_, deal)| {
            !ENDED_STAGES.contains(&deal.record.value("dealstage").unwrap_or(""))
                && (deal
                    .assignment
                    .actionyotei
                    .as_deref()
                    .and_then(date)
                    .is_some()
                    || deal
                        .assignment
                        .contract_expiration_date
                        .as_deref()
                        .and_then(date)
                        .is_some_and(|due| (0..=31).contains(&(due - current_day).num_days())))
        })
        .map(|(id, _)| id.clone())
        .collect();
    let companies = listings::todo_companies(service, &ids).await?;
    tracing::info!(
        count = deals.len(),
        search_calls = calls,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "job-copy owner/todo full delivery read"
    );
    Ok(Snapshot {
        deals,
        owners,
        labels,
        companies,
        stages: defs,
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
    pub(super) fn report(
        &self,
        query: &OwnerQuery,
        email: &str,
        today: NaiveDate,
        failed: bool,
    ) -> Value {
        let (owner, notice) = self.selection(query, email);
        let field = query.field();
        let assigned: BTreeSet<_> = self
            .deals
            .values()
            .filter_map(|d| d.assignment.assigned(field))
            .collect();
        let options: Vec<_> = self.owners.iter().filter(|o| assigned.contains(o.id.as_str())).map(|o| json!({"id":o.id,"name":format!("{}{}",o.name,if o.archived {"（退職）"} else {""}),"archived":o.archived})).collect();
        let mut actions = BTreeMap::<&str, Vec<Value>>::from([
            ("overdue", vec![]),
            ("today", vec![]),
            ("week", vec![]),
            ("later", vec![]),
        ]);
        let mut expirations = vec![];
        for (id, deal) in &self.deals {
            if !self.matches(id, field, owner.as_deref())
                || ENDED_STAGES.contains(&deal.record.value("dealstage").unwrap_or(""))
            {
                continue;
            }
            let a = &deal.assignment;
            let row = |due: NaiveDate, content: Option<&str>| {
                let company = self.companies.get(id);
                let consultant = self
                    .owners
                    .iter()
                    .find(|o| Some(o.id.as_str()) == a.consultant.as_deref());
                json!({"id":id,"name":deal.record.value("dealname"),"company_id":company.map(|c| &c.0),"company":company.and_then(|c| c.1.as_ref()),"stage":deal.record.value("dealstage").and_then(|s| self.stages.get(s)),"content":content,"due":due.to_string(),"consultant":consultant.map(|o| format!("{}{}",o.name,if o.archived {"（退職）"} else {""}))})
            };
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
                .filter(|d| (0..=30).contains(&(*d - today).num_days()))
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
        json!({"status":"ready","owner":owner.as_deref().unwrap_or("all"),"owner_field":field,"notice":notice,"owners":options,"actions":actions,"expirations":expirations,"refresh_failed":failed})
    }
}
pub(super) fn today(now: chrono::DateTime<Utc>) -> NaiveDate {
    (now + chrono::Duration::hours(9)).date_naive()
}
fn date(value: &str) -> Option<NaiveDate> {
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
    service.owner_todo.state.lock().await.snapshot =
        Some((Instant::now(), Arc::new(Snapshot::default())));
}
#[cfg(test)]
#[path = "owner_todo_tests.rs"]
mod tests;
