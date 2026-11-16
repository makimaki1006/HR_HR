//! 案件一覧。取引は既存の5分キャッシュ、MTGはKPIの先読み済みシートだけを使う。
use super::owner_todo::{OwnerQuery, Snapshot, WorkDeal, ENDED_STAGES};
use super::*;
use crate::handlers::cs_dashboard::{
    self as cs,
    routes::{contract_month, cpa_band_medians, cpa_band_of},
};
use chrono::NaiveDate;
use std::collections::HashMap;

type LastMeetings = HashMap<String, (Option<NaiveDate>, Option<NaiveDate>)>;

#[derive(Default)]
pub(super) struct MeetingData {
    pub(super) last: Option<Arc<LastMeetings>>,
    pub(super) as_of: Option<String>,
    /// 取引ごとの最後の接触日（MTG または60秒超の通話。メールは数えない）。
    /// 危ない案件・コンサルKPIと同じ `contacts_by_deal` で数える。シートを読めていなければ `None`。
    pub(super) contacts: Option<Arc<HashMap<String, NaiveDate>>>,
    /// 取引ごとの最後のメールの日（`CS_メール履歴`。接触には数えない）。
    pub(super) emails: Option<Arc<HashMap<String, NaiveDate>>>,
}
/// シートの取得時刻（MTG・Zoom・通話・メール）。どれかが取り直されたら数え直す。
type SheetsKey = [Option<std::time::Instant>; 4];
type Counted = (
    Option<Arc<LastMeetings>>,
    Option<Arc<HashMap<String, NaiveDate>>>,
    Option<Arc<HashMap<String, NaiveDate>>>,
);
/// 数えた結果を 1 組だけ覚える（取り直したシートの分で置き換えるので増えない）。
/// 通話明細（約 2 万行）を画面の要求のたびに数え直さないため。
static COUNTED: std::sync::Mutex<Option<(SheetsKey, Counted)>> = std::sync::Mutex::new(None);
impl MeetingData {
    pub(super) async fn cached() -> Self {
        let Ok(state) = crate::handlers::call_quality::routes::cq_state() else {
            return Self::default();
        };
        // 既存の定期先読みを借りる。画面要求からシートの読み込みを待たない (以前どおり手元の値だけ)。
        // 先読みは見ている人がいる間だけ続くので、この要求も「見ている」に数え、古ければ裏で取り直しを始める (2026-11-16)。
        cs::note_view(None).await;
        Self::from_store(&state.store).await
    }
    /// 手元のシートで数える。期限 (TTL) を過ぎていても前回の値があればそれで答え、`as_of` はその値の
    /// データの日付になる (夜のあいだ取り直さなくても、朝いちばんの画面が「不明」にならないように。2026-11-16)。
    /// 手元に一度も値が無いとき (起動直後など) だけ、接触は「不明」・日付は空。
    async fn from_store(store: &crate::handlers::call_quality::sheets::SheetStore) -> Self {
        let mtg = store.latest(cs::SHEET_MTG).await;
        let meta = store.latest(cs::SHEET_META).await;
        let zoom = store.latest(cs::SHEET_ZOOM_MTG).await;
        let call = store.latest(cs::SHEET_CALL).await;
        let mail = store.latest(cs::SHEET_MAIL_HISTORY).await;
        let (last, contacts, emails) = counted(
            mtg.as_deref(),
            zoom.as_deref(),
            call.as_deref(),
            mail.as_deref(),
        );
        Self {
            last,
            as_of: meta.as_ref().and_then(|m| cs::data_as_of(m)),
            contacts,
            emails,
        }
    }
    /// 案件一覧・要対応で使う接触。MTG の集計対象（リクロジ_納品管理）だけ記録から決める。
    fn contact(&self, work: &WorkDeal) -> super::todo_signals::Contact {
        use super::todo_signals::Contact;
        if work.record.value("pipeline") != Some(MTG_PIPELINE) {
            return Contact::Unknown;
        }
        match &self.contacts {
            Some(by) => Contact::Last(by.get(&work.record.id).copied()),
            None => Contact::Unknown,
        }
    }
}
/// MTG・接触・メールの日付を数える。同じ取得時刻のシートなら前に数えた結果を返す。
fn counted(
    mtg: Option<&SheetData>,
    zoom: Option<&SheetData>,
    call: Option<&SheetData>,
    mail: Option<&SheetData>,
) -> Counted {
    let key: SheetsKey = [mtg, zoom, call, mail].map(|s| s.map(|s| s.fetched_at));
    if let Some((saved, result)) = COUNTED.lock().ok().as_deref().and_then(Option::as_ref) {
        if *saved == key {
            return result.clone();
        }
    }
    let empty = cs::empty_history_sheet();
    let factual = mtg.map(|m| cs::factual_mtg(m, zoom.unwrap_or(&empty)));
    let result: Counted = (
        factual
            .as_ref()
            .map(|m| Arc::new(cs::last_recorded_mtg_by_deal(m))),
        factual
            .as_ref()
            .zip(call)
            .map(|(m, c)| Arc::new(last_contacts(c, m))),
        mail.map(|m| Arc::new(last_emails(m))),
    );
    if let Ok(mut slot) = COUNTED.lock() {
        *slot = Some((key, result.clone()));
    }
    result
}
/// MTG と接触の記録を集めているパイプライン（コンサルKPIの対象。リクロジ_納品管理）。
const MTG_PIPELINE: &str = "21596025";
type SheetData = crate::handlers::call_quality::sheets::SheetData;
fn last_contacts(call: &SheetData, mtg: &SheetData) -> HashMap<String, NaiveDate> {
    cs::contacts_by_deal(call, mtg)
        .0
        .into_iter()
        .filter_map(|(id, days)| days.last().copied().map(|d| (id, d)))
        .collect()
}
fn last_emails(mail: &SheetData) -> HashMap<String, NaiveDate> {
    let mut by: HashMap<String, NaiveDate> = HashMap::new();
    for row in &mail.rows {
        let deal = mail.get(row, "deal_id").trim();
        let Some(day) = cs::date10(mail.get(row, "送信日時（JST）")) else {
            continue;
        };
        if deal.is_empty() {
            continue;
        }
        let e = by.entry(deal.to_string()).or_insert(day);
        if day > *e {
            *e = day;
        }
    }
    by
}
#[derive(Serialize, Deserialize, ts_rs::TS)]
struct DealBoardReason {
    #[ts(
        type = "'expiration' | 'expired' | 'contact' | 'stalled' | 'first_contract' | 'nps' | 'target' | 'cpa' | 'mtg' | 'mtg_gap'"
    )]
    kind: String,
    text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    strong: Option<bool>,
}
#[derive(Serialize, Deserialize, ts_rs::TS)]
struct DealBoardRow {
    #[serde(default)]
    #[ts(optional = nullable)]
    application_reasons: Option<super::deal_application_reasons::DealApplicationReasonSummary>,
    id: String,
    name: Option<String>,
    company: Option<String>,
    pipeline: String,
    active: bool,
    stage: Option<String>,
    consultant: Option<String>,
    reasons: Vec<DealBoardReason>,
    #[ts(type = "number | null")]
    months: Option<i64>,
    expiration: Option<String>,
    #[ts(type = "number | null")]
    days_left: Option<i64>,
    /// 最後の接触（MTG または60秒超の通話。メールは数えない）。
    last_contact: Option<String>,
    #[serde(default)]
    #[ts(type = "'ready' | 'outside' | 'unavailable' | 'none'")]
    contact_status: String,
    /// 最後のメールの日（接触には数えない。添えて見せるだけ）。
    #[serde(default)]
    #[ts(optional = nullable)]
    last_email: Option<String>,
    last_mtg: Option<String>,
    #[ts(type = "'ready' | 'outside' | 'unavailable' | 'none'")]
    mtg_status: String,
    nps: Option<f64>,
    applications: Option<f64>,
    #[serde(default)]
    #[ts(type = "number | null")]
    recent_applications: Option<i64>,
    #[ts(type = "number | null")]
    media_recent_applications: Option<i64>,
    /// 満了から8週を過ぎた終了済みの取引は応募を数えない。
    #[serde(default)]
    applications_out_of_scope: bool,
    recent_application_ids: Option<Vec<String>>,
    previous_application_ids: Option<Vec<String>>,
    unknown_application_ids: Option<Vec<String>>,
    #[ts(type = "number | null")]
    previous_applications: Option<i64>,
    #[ts(type = "number | null")]
    applications_change: Option<i64>,
    interviews: Option<f64>,
    hires: Option<f64>,
    target: Option<f64>,
    achievement: Option<f64>,
    cpa: Option<f64>,
    amount: Option<f64>,
    first_contract: bool,
    launch: super::todo_signals::Launch,
    #[serde(default)]
    #[ts(type = "number | null")]
    listing_count: Option<usize>,
    #[serde(default)]
    zero_applicants: bool,
    /// 契約の開始日から満了日までの応募数（応募日で数える）。求人票が無い・応募日が不明な応募がある・日付が無いときは null。
    #[serde(default)]
    #[ts(type = "number | null", optional)]
    contract_applications: Option<i64>,
    /// 指定した期間（period_from〜period_to）の応募数。期間を指定しなかった・数えられないときは null。
    #[serde(default)]
    #[ts(type = "number | null", optional)]
    period_applications: Option<i64>,
}

/// 期間を指定して応募ゼロを調べるための指定（日本時間の日付、両端を含む）。
#[derive(Clone, Debug, Default, Deserialize)]
pub(super) struct PeriodQuery {
    period_from: Option<String>,
    period_to: Option<String>,
}
impl PeriodQuery {
    fn range(&self) -> Result<Option<(NaiveDate, NaiveDate)>, ReadError> {
        let bad = || ReadError(StatusCode::BAD_REQUEST, "listing_filter_invalid");
        let parse = |v: &str| NaiveDate::parse_from_str(v, "%Y-%m-%d").map_err(|_| bad());
        match (self.period_from.as_deref(), self.period_to.as_deref()) {
            (None, None) => Ok(None),
            (Some(from), Some(to)) => {
                let (from, to) = (parse(from)?, parse(to)?);
                if from > to {
                    return Err(bad());
                }
                Ok(Some((from, to)))
            }
            _ => Err(bad()),
        }
    }
}

/// 求人票が付いていて応募日がすべて分かる取引だけ、期間内の応募数を返す。
/// 求人票が無い、または応募日が分からない応募がある取引は数えられないので None。
fn applications_between(
    facts: Option<&super::todo_signals::Facts>,
    from: NaiveDate,
    to: NaiveDate,
) -> Option<i64> {
    let facts = facts?;
    if !matches!(facts.listings, Some(n) if n >= 1) || facts.unknown_dates > 0 {
        return None;
    }
    Some(
        facts
            .days
            .iter()
            .filter(|d| **d >= from && **d <= to)
            .count() as i64,
    )
}
struct ApplicationWindows {
    recent: Vec<String>,
    previous: Vec<String>,
    unknown: Vec<String>,
}

/// 今日を含む28日、その前の28日。日付不明は両期間から除く。
fn application_windows(
    facts: Option<&super::todo_signals::Facts>,
    today: NaiveDate,
) -> Option<ApplicationWindows> {
    let facts = facts?;
    let (mut recent, mut previous, mut unknown) = (vec![], vec![], vec![]);
    for (id, day) in &facts.application_days {
        match day.map(|d| (today - d).num_days()) {
            Some(0..=27) => recent.push(id.clone()),
            Some(28..=55) => previous.push(id.clone()),
            None => unknown.push(id.clone()),
            _ => {}
        }
    }
    Some(ApplicationWindows {
        recent,
        previous,
        unknown,
    })
}
fn current(record: &Record) -> cs::Deal {
    let text = |key| record.value(key).unwrap_or("").to_owned();
    let num = |key| {
        record
            .value(key)
            .and_then(cs::opt_num)
            .filter(|v| v.is_finite())
    };
    cs::Deal {
        id: record.id.clone(),
        name: text("dealname"),
        stage: text("dealstage"),
        contract_kind: cs::contract_kind_from_name(record.value("dealname").unwrap_or(""))
            .to_owned(),
        contract_start_date: text("contract_start_date"),
        contract_expiration_date: text("contract_expiration_date"),
        is_active: !ENDED_STAGES.contains(&record.value("dealstage").unwrap_or("")),
        amount: num("amount").filter(|v| *v > 0.0),
        oubo: num("oubo"),
        mensetu: num("mensetu"),
        syoudaku: num("syoudaku"),
        saiyomokuhyou: num("saiyomokuhyou"),
        contract_period: num("contract_period"),
        ..Default::default()
    }
}
fn nps(record: &Record) -> Option<f64> {
    cs::NPS_PROPS.iter().rev().find_map(|key| {
        record
            .value(key)
            .and_then(cs::opt_num)
            .filter(|v| v.is_finite())
    })
}
pub(super) struct Metrics<'a> {
    deals: BTreeMap<String, cs::Deal>,
    medians: [(Option<f64>, usize); 3],
    meetings: &'a MeetingData,
    // 応募動機の選択肢の定義（全取引で共通。取引ごとには持たない）。
    selection_options: Option<BTreeMap<String, Vec<Value>>>,
}
impl<'a> Metrics<'a> {
    pub(super) fn new(snapshot: &Snapshot, meetings: &'a MeetingData, today: NaiveDate) -> Self {
        let deals: BTreeMap<_, _> = snapshot
            .deals
            .iter()
            .map(|(id, d)| (id.clone(), current(&d.record)))
            .collect();
        let population: Vec<_> = deals
            .values()
            .filter(|d| d.is_active && !d.is_excluded())
            .cloned()
            .collect();
        Self {
            medians: cpa_band_medians(&population, today),
            deals,
            meetings,
            selection_options: None,
        }
    }
    pub(super) fn with_selection_options(
        mut self,
        options: Option<BTreeMap<String, Vec<Value>>>,
    ) -> Self {
        self.selection_options = options;
        self
    }
    pub(super) fn reasons(&self, snapshot: &Snapshot, id: &str, today: NaiveDate) -> Vec<Value> {
        let Some(work) = snapshot.deals.get(id) else {
            return vec![];
        };
        let d = &self.deals[id];
        let mut reasons = super::todo_signals::reasons(
            work,
            snapshot.facts.get(id),
            self.meetings.contact(work),
            today,
        );
        if !d.is_active || !listings::is_delivery(work.record.value("pipeline")) {
            return reasons;
        }
        let mut add = |kind: &str, text: &str, strong: bool| {
            reasons.push(json!({"kind":kind,"text":text,"strong":strong}))
        };
        if cs::low_nps(nps(&work.record)) {
            add("nps", "定期の満足度が4以下", false);
        }
        let established =
            cs::date10(&d.contract_start_date).is_some_and(|st| (today - st).num_days() > 30);
        if established && cs::below_half_target(d) {
            add("target", "採用目標の半分に届いていない", false);
        }
        let median = cpa_band_of(d, today).and_then(|b| self.medians[b].0);
        if cs::high_cpa(cs::cpa(d), median) {
            add("cpa", "採用単価が同じ進捗帯の1.5倍以上", false);
        }
        // 60日以上は「MTGが途絶えている」（今日の要対応と同じ基準）、30〜59日は別の印「MTGが空いている」。
        if let Some(days) = self.mtg_days(work, today) {
            match cs::mtg_band_for_days(days) {
                cs::MtgBand::Critical => add(
                    "mtg",
                    &format!("MTGが途絶えている（最後のMTGから {days} 日）"),
                    true,
                ),
                cs::MtgBand::Yellow => add(
                    "mtg_gap",
                    &format!("MTGが空いている（最後のMTGから {days} 日）"),
                    false,
                ),
                _ => {}
            }
        }
        reasons
    }
    /// 最後の MTG からの日数（MTG の集計対象の契約で、記録があるときだけ）。
    fn mtg_days(&self, work: &WorkDeal, today: NaiveDate) -> Option<i64> {
        if work.record.value("pipeline") != Some("21596025") {
            return None;
        }
        let last = self.meetings.last.as_ref()?;
        let d = self.deals.get(&work.record.id)?;
        cs::mtg_gap_of(d, last, today).days
    }
    /// ホームの要対応と取引ごとの「次にやること」で使う理由。
    /// 「MTGが途絶えている」（60 日以上）だけを出し、30〜59 日の「MTGが空いている」は案件一覧の印だけにする。
    pub(super) fn todo_reasons(
        &self,
        snapshot: &Snapshot,
        id: &str,
        today: NaiveDate,
    ) -> Vec<Value> {
        let mut reasons = self.reasons(snapshot, id, today);
        reasons.retain(|r| r["kind"] != "mtg_gap");
        reasons
    }
    fn rows(
        &self,
        snapshot: &Snapshot,
        query: &OwnerQuery,
        email: &str,
        today: NaiveDate,
    ) -> Vec<DealBoardRow> {
        self.rows_in(snapshot, query, email, today, None)
    }
    fn rows_in(
        &self,
        snapshot: &Snapshot,
        query: &OwnerQuery,
        email: &str,
        today: NaiveDate,
        period: Option<(NaiveDate, NaiveDate)>,
    ) -> Vec<DealBoardRow> {
        let (owner, _) = snapshot.selection(query, email);
        snapshot.deals.iter().filter(|(id, work)| snapshot.matches(id, query.field(), owner.as_deref()) && listings::is_delivery(work.record.value("pipeline")) && !self.deals[*id].is_referral()).map(|(id, work)| {
            let windows = application_windows(snapshot.facts.get(id), today);
            let d = &self.deals[id];
            let company = snapshot.companies.get(id);
            let consultant = snapshot.owners.iter().find(|o| Some(o.id.as_str()) == work.assignment.consultant.as_deref());
            let supported = work.record.value("pipeline") == Some(MTG_PIPELINE);
            let contact = match self.meetings.contact(work) {
                super::todo_signals::Contact::Last(day) => day,
                super::todo_signals::Contact::Unknown => None,
            };
            let contact_status = if !supported { "outside" } else if self.meetings.contacts.is_none() { "unavailable" } else if contact.is_none() { "none" } else { "ready" };
            let last_email = self.meetings.emails.as_ref().and_then(|m| m.get(id)).map(|d| d.to_string());
            let last = self.meetings.last.as_ref().and_then(|m| m.get(id)).and_then(|(d, _)| *d);
            let mtg_status = if !supported { "outside" } else if self.meetings.last.is_none() { "unavailable" } else if last.is_none() { "none" } else { "ready" };
            serde_json::from_value(json!({"id":id,"name":work.record.value("dealname"),"company":company.and_then(|c| c.1.as_ref()),
                "application_reasons":super::deal_application_reasons::summarize(snapshot.facts.get(id),self.selection_options.as_ref(),today),
                "pipeline":work.record.value("pipeline"),"active":d.is_active,"stage":snapshot.stages.get(&d.stage),
                "consultant":consultant.map(|o| &o.name),"reasons":self.reasons(snapshot,id,today),
                "months":contract_month(&d.contract_start_date,today),"expiration":cs::date10(&d.contract_expiration_date).map(|d| d.to_string()),
                "days_left":cs::date10(&d.contract_expiration_date).map(|d| (d-today).num_days()),
                "last_contact":contact.map(|d| d.to_string()),"contact_status":contact_status,"last_email":last_email,
                "last_mtg":if supported {last.map(|d| d.to_string())} else {None},"mtg_status":mtg_status,
                "nps":nps(&work.record),"applications":d.oubo,"interviews":d.mensetu,"hires":d.syoudaku,
                "recent_applications":windows.as_ref().map(|w| w.recent.len()),
                "previous_applications":windows.as_ref().map(|w| w.previous.len()),
                "applications_change":windows.as_ref().map(|w| w.recent.len() as i64-w.previous.len() as i64),
                "recent_application_ids":windows.as_ref().map(|w| &w.recent),
                "previous_application_ids":windows.as_ref().map(|w| &w.previous),
                "unknown_application_ids":windows.as_ref().map(|w| &w.unknown),
                "media_recent_applications":null,
                "applications_out_of_scope":!super::todo_signals::eligible(work, today),
                "first_contract": super::todo_signals::is_first_contract(&work.record),
                "launch": super::todo_signals::launch(&work.record,snapshot.facts.get(id),today),
                "listing_count":snapshot.facts.get(id).and_then(|f| f.listings),
                "zero_applicants":d.is_active && super::todo_signals::zero_applicants(snapshot.facts.get(id)),
                "contract_applications":cs::date10(&d.contract_start_date).zip(cs::date10(&d.contract_expiration_date)).and_then(|(from,to)| applications_between(snapshot.facts.get(id),from,to)),
                "period_applications":period.and_then(|(from,to)| applications_between(snapshot.facts.get(id),from,to)),
                "target":d.saiyomokuhyou,"achievement":d.rate_tassei(),"cpa":cs::cpa(d),"amount":d.amount})).expect("deal board row contract")
        }).collect()
    }
}
pub(super) async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<OwnerQuery>,
    Query(period): Query<PeriodQuery>,
) -> Result<impl IntoResponse, ReadError> {
    let email = authorized_user(&state, &access, &session).await?;
    query.validate()?;
    let period = period.range()?;
    let view = owner_todo::work_view(listings::service(&access)?).await;
    let apps = listings::consultant_applications(listings::service(&access)?).await;
    let body = if let Some(snapshot) = view.snapshot.filter(|s| s.complete) {
        let today = owner_todo::today(chrono::Utc::now());
        let meetings = MeetingData::cached().await;
        let options = super::apply_reasons::read_options(listings::service(&access)?).await;
        let metrics = Metrics::new(&snapshot, &meetings, today)
            .with_selection_options(options.map(|o| o.definitions));
        let mut rows = metrics.rows_in(&snapshot, &query, &email, today, period);
        for row in &mut rows {
            row.media_recent_applications = apps
                .as_ref()
                .and_then(|(counts, _, _)| counts.get(&row.id))
                .map(|(_, recent)| *recent);
        }
        json!({"status":"ready","rows":rows,"hubspot_portal_id":crate::hubspot::deep_link::hubspot_portal_id(),"sheet_as_of":meetings.as_of,
            "refreshing":view.refreshing,"refresh_failed":view.failed,
            "facts_built_at":view.facts_built_at,"facts_stale":view.facts_stale,
            "applications_status": if view.partial { if view.failed { "unavailable" } else { "preparing" } } else { "ready" },
            "work":snapshot.report_with_metrics(&query,&email,today,view.failed,&meetings)})
    } else {
        json!({"status":if view.failed {"failed"} else {"preparing"}})
    };
    Ok(([(header::CACHE_CONTROL, "private, no-store")], Json(body)))
}

#[cfg(test)]
mod tests {
    use super::super::owner_todo::{Assignment, WorkDeal};
    use super::*;
    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 16).unwrap()
    }
    fn snapshot(age: i64, score: &str, pipeline: &str) -> Snapshot {
        let record = Record {
            id: "1".into(),
            properties: [
                ("dealname", "架空の採用支援".to_owned()),
                ("pipeline", pipeline.to_owned()),
                ("dealstage", "open".to_owned()),
                (
                    "contract_start_date",
                    (day() - chrono::Duration::days(age)).to_string(),
                ),
                ("contract_expiration_date", "2026-11-01".into()),
                ("notes_last_contacted", day().to_string()),
                ("nps", "1".into()),
                ("nps2", score.into()),
                ("amount", "300000".into()),
                ("syoudaku", "2".into()),
                ("saiyomokuhyou", "10".into()),
                ("contract_period", "12".into()),
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), Some(v)))
            .collect(),
        };
        let mut result = Snapshot::for_index(
            BTreeMap::from([(
                "1".into(),
                WorkDeal {
                    assignment: Assignment::from_record(&record),
                    record,
                },
            )]),
            vec![],
        );
        result.complete = true;
        result
    }
    fn meetings(days: i64) -> MeetingData {
        MeetingData {
            last: Some(Arc::new(HashMap::from([(
                "1".into(),
                (Some(day() - chrono::Duration::days(days)), None),
            )]))),
            as_of: Some("2026-10-15".into()),
            contacts: Some(Arc::new(HashMap::from([("1".into(), day())]))),
            emails: Some(Arc::new(HashMap::new())),
        }
    }
    fn reasons(s: &Snapshot, m: &MeetingData) -> Vec<Value> {
        Metrics::new(s, m, day()).reasons(s, "1", day())
    }
    #[test]
    fn application_windows_include_jst_today_and_exact_28_day_boundaries() {
        let today = owner_todo::today(
            chrono::DateTime::parse_from_rfc3339("2026-11-08T15:00:00Z")
                .unwrap()
                .to_utc(),
        );
        assert_eq!(today.to_string(), "2026-11-09");
        let values = [
            ("old", Some("2026-09-14T14:59:59Z")),
            ("previous_start", Some("2026-09-14T15:00:00Z")),
            ("previous_end", Some("2026-10-12T14:59:59Z")),
            ("recent_start", Some("2026-10-12T15:00:00Z")),
            ("today", Some("2026-11-09T14:59:59Z")),
            ("future", Some("2026-11-09T15:00:00Z")),
            ("unknown", None),
        ];
        let facts = super::super::todo_signals::Facts {
            application_days: values
                .into_iter()
                .map(|(id, value)| {
                    (
                        id.into(),
                        value
                            .and_then(super::super::deal_summary::jst_day)
                            .and_then(|d| owner_todo::date(&d)),
                    )
                })
                .collect(),
            ..Default::default()
        };
        let ApplicationWindows {
            recent,
            previous,
            unknown,
        } = application_windows(Some(&facts), today).unwrap();
        assert_eq!(recent, ["recent_start", "today"]);
        assert_eq!(previous, ["previous_end", "previous_start"]);
        assert_eq!(unknown, ["unknown"]);
        assert!(application_windows(None, today).is_none());
        let mut s = snapshot(0, "5", "21596025");
        s.facts.insert("1".into(), facts);
        let rows = Metrics::new(&s, &MeetingData::default(), today).rows(
            &s,
            &OwnerQuery {
                owner: Some("all".into()),
                ..Default::default()
            },
            "",
            today,
        );
        assert_eq!(rows[0].recent_applications, Some(2));
        assert_eq!(rows[0].previous_applications, Some(2));
        assert_eq!(rows[0].applications_change, Some(0));
        assert!(!rows[0].applications_out_of_scope);
        assert_eq!(
            rows[0].unknown_application_ids.as_ref().unwrap(),
            &["unknown"]
        );
    }
    #[test]
    fn reason_summary_is_in_rows_and_old_facts_are_pending() {
        let mut s = snapshot(0, "5", "21596025");
        let old: super::super::todo_signals::Facts = serde_json::from_value(json!({
            "live":1,"unknown_dates":0,"days":["2026-09-14"],
            "application_days":{"a":"2026-09-14"},"listings":1,
            // 前の版で取引ごとに持っていた定義が残る保存データも読める。
            "selection_options":{"oubodouki_1":[]}
        }))
        .unwrap();
        s.facts.insert("1".into(), old);
        let query = OwnerQuery {
            owner: Some("all".into()),
            ..Default::default()
        };
        let meetings = MeetingData::default();
        let rows = Metrics::new(&s, &meetings, day()).rows(&s, &query, "", day());
        assert!(rows[0].application_reasons.is_none());
        let facts = s.facts.get_mut("1").unwrap();
        facts.selections = Some(BTreeMap::from([(
            "a".into(),
            BTreeMap::from([("oubodouki_1".into(), vec!["休日".into()])]),
        )]));
        let options = Some(
            super::super::apply_reasons::SELECT_PROPERTIES
                .into_iter()
                .map(|p| {
                    (
                        p.into(),
                        vec![json!({"value":"休日","label":"休日","display_order":0})],
                    )
                })
                .collect(),
        );
        let rows = Metrics::new(&s, &meetings, day())
            .with_selection_options(options)
            .rows(&s, &query, "", day());
        let value = serde_json::to_value(&rows[0]).unwrap();
        assert_eq!(value["application_reasons"]["total"], 1);
        assert_eq!(
            value["application_reasons"]["answered"],
            json!([1, 0, 0, 0])
        );
        assert_eq!(value["application_reasons"]["top"][0]["label"], "休日");
        assert_eq!(value["application_reasons"]["top"][0]["counts"], json!([1]));
    }
    #[test]
    fn export_board_types() {
        use ts_rs::TS;
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("frontend/src/generated");
        let cfg = ts_rs::Config::default().with_out_dir(dir);
        DealBoardRow::export_all(&cfg).unwrap();
    }
    #[test]
    fn option_peer_population_uses_kind_prefix_not_contract_type_or_words_in_name() {
        for (name, option) in [
            ("求人追加_架空の契約", true),
            ("AirWork広告費用＿架空の契約", true),
            ("(新規)_架空の求人追加支援", false),
        ] {
            let mut s = snapshot(120, "5", "21596025");
            s.deals
                .get_mut("1")
                .unwrap()
                .record
                .properties
                .insert("dealname".into(), Some(name.into()));
            assert_eq!(current(&s.deals["1"].record).is_option(), option);
        }
    }
    #[test]
    fn nps_current_round_and_target_grace_boundaries() {
        for (score, has) in [("4", true), ("5", false)] {
            let s = snapshot(31, score, "21596025");
            assert_eq!(nps(&s.deals["1"].record), score.parse().ok());
            assert_eq!(
                reasons(&s, &MeetingData::default())
                    .iter()
                    .any(|r| r["kind"] == "nps"),
                has
            );
        }
        for (age, has) in [(29, false), (30, false), (31, true)] {
            assert_eq!(
                reasons(&snapshot(age, "5", "21596025"), &MeetingData::default())
                    .iter()
                    .any(|r| r["kind"] == "target"),
                has
            );
        }
        let mut s = snapshot(31, "5", "21596025");
        s.deals
            .get_mut("1")
            .unwrap()
            .record
            .properties
            .insert("syoudaku".into(), Some("5".into()));
        assert!(!reasons(&s, &MeetingData::default())
            .iter()
            .any(|r| r["kind"] == "target"));
    }
    #[test]
    fn mtg_sixty_days_is_stopped_and_thirty_to_fifty_nine_is_a_separate_mark() {
        let s = snapshot(120, "5", "21596025");
        for (days, kind, text, strong) in [
            (29, None, "", false),
            (
                30,
                Some("mtg_gap"),
                "MTGが空いている（最後のMTGから 30 日）",
                false,
            ),
            (
                59,
                Some("mtg_gap"),
                "MTGが空いている（最後のMTGから 59 日）",
                false,
            ),
            (
                60,
                Some("mtg"),
                "MTGが途絶えている（最後のMTGから 60 日）",
                true,
            ),
            (
                90,
                Some("mtg"),
                "MTGが途絶えている（最後のMTGから 90 日）",
                true,
            ),
        ] {
            let r = reasons(&s, &meetings(days));
            let marks: Vec<_> = r
                .iter()
                .filter(|r| r["kind"] == "mtg" || r["kind"] == "mtg_gap")
                .collect();
            assert_eq!(marks.len(), usize::from(kind.is_some()), "days={days}");
            if let Some(kind) = kind {
                assert_eq!(marks[0]["kind"], kind, "days={days}");
                assert_eq!(marks[0]["text"], text);
                assert_eq!(marks[0]["strong"], strong);
            }
        }
    }
    #[test]
    fn home_todo_and_deal_board_count_the_same_stopped_mtg_deals() {
        // 本番の食い違い（案件一覧 7 件・今日 2 件）の形: 30〜59 日の案件は「MTGが途絶えている」に入らない。
        let s = snapshot(120, "5", "21596025");
        for (days, stopped, gap) in [
            (29, false, false),
            (30, false, true),
            (59, false, true),
            (60, true, false),
            (90, true, false),
        ] {
            let m = meetings(days);
            let metrics = Metrics::new(&s, &m, day());
            let has = |v: &[Value], k: &str| v.iter().any(|r| r["kind"] == k);
            let todo = metrics.todo_reasons(&s, "1", day());
            let board = metrics.reasons(&s, "1", day());
            assert_eq!(has(&todo, "mtg"), stopped, "days={days}");
            assert_eq!(has(&board, "mtg"), stopped, "days={days}");
            assert_eq!(has(&board, "mtg_gap"), gap, "days={days}");
            assert!(!has(&todo, "mtg_gap"), "days={days}");
        }
    }
    fn sheet(header: &[&str], rows: &[&[&str]]) -> SheetData {
        SheetData {
            header: header.iter().map(|h| (*h).to_owned()).collect(),
            rows: rows
                .iter()
                .map(|r| r.iter().map(|v| std::sync::Arc::from(*v)).collect())
                .collect(),
            fetched_at: std::time::Instant::now(),
        }
    }
    #[test]
    fn contact_is_mtg_or_call_over_sixty_seconds_and_never_email() {
        let call = sheet(
            &["deal_id", "duration_sec", "ts"],
            &[
                &["1", "61", "2026-09-05T01:00:00Z"],
                // 60秒ちょうどは接触にしない。
                &["1", "60", "2026-10-10T01:00:00Z"],
                // 日本時間では 10/01。
                &["2", "300", "2026-09-30T15:30:00Z"],
            ],
        );
        let mtg = sheet(
            &["deal_id", "開催日"],
            &[&["1", "2026-09-10"], &["3", "2026-08-01"]],
        );
        let mail = sheet(
            &["deal_id", "送信日時（JST）"],
            &[&["1", "2026-10-15 10:00:00"], &["1", "2026-10-01 09:00:00"]],
        );
        let last = last_contacts(&call, &mtg);
        assert_eq!(last["1"].to_string(), "2026-09-10");
        assert_eq!(last["2"].to_string(), "2026-10-01");
        assert_eq!(last["3"].to_string(), "2026-08-01");
        assert_eq!(last_emails(&mail)["1"].to_string(), "2026-10-15");

        // 取引の notes_last_contacted（メールを含む今日）が新しくても、記録からの接触で印を立てる。
        let s = snapshot(120, "5", "21596025");
        let m = MeetingData {
            contacts: Some(Arc::new(last)),
            emails: Some(Arc::new(last_emails(&mail))),
            ..meetings(10)
        };
        let metrics = Metrics::new(&s, &m, day());
        let contact: Vec<_> = metrics
            .reasons(&s, "1", day())
            .into_iter()
            .filter(|r| r["kind"] == "contact")
            .collect();
        assert_eq!(contact.len(), 1);
        assert_eq!(contact[0]["text"], "最後の接触から 36 日");
        let q = OwnerQuery {
            owner: Some("all".into()),
            ..Default::default()
        };
        let row = serde_json::to_value(&metrics.rows(&s, &q, "", day())[0]).unwrap();
        assert_eq!(row["last_contact"], "2026-09-10");
        assert_eq!(row["contact_status"], "ready");
        assert_eq!(row["last_email"], "2026-10-15");
    }
    #[tokio::test]
    async fn expired_sheets_still_answer_with_the_previous_values_and_date() {
        // 夜のあいだ取り直さず、期限 (60 分) を 2 時間過ぎた前回の値だけが手元にある
        let store = crate::handlers::call_quality::sheets::SheetStore::new();
        let old = std::time::Instant::now() - std::time::Duration::from_secs(2 * 3600);
        let put = |name: &str, mut data: SheetData| {
            data.fetched_at = old;
            (name.to_owned(), data)
        };
        let sheets = [
            put(
                cs::SHEET_CALL,
                sheet(
                    &["deal_id", "duration_sec", "ts"],
                    &[&["7", "120", "2026-11-14T01:00:00Z"]],
                ),
            ),
            put(
                cs::SHEET_MTG,
                sheet(&["deal_id", "開催日"], &[&["7", "2026-11-10"]]),
            ),
            put(
                cs::SHEET_MAIL_HISTORY,
                sheet(
                    &["deal_id", "送信日時（JST）"],
                    &[&["7", "2026-11-13 10:00"]],
                ),
            ),
            put(
                cs::SHEET_META,
                sheet(
                    &["key", "value"],
                    &[&["データ取得時刻(JST)", "2026-11-15 21:30:00"]],
                ),
            ),
        ];
        for (name, data) in sheets {
            store.put_for_test(&name, data).await;
        }
        assert!(
            store.fresh(cs::SHEET_CALL).await.is_none(),
            "期限は切れている"
        );
        let m = MeetingData::from_store(&store).await;
        // 古い日付のまま答える (画面の「○時点」も前回のデータの日付)
        assert_eq!(m.as_of.as_deref(), Some("2026-11-15 21:30:00"));
        assert_eq!(m.contacts.as_ref().unwrap()["7"].to_string(), "2026-11-14");
        assert_eq!(m.last.as_ref().map(|l| l.len()), Some(1));
        assert_eq!(m.emails.as_ref().unwrap()["7"].to_string(), "2026-11-13");
        // 手元に一度も値が無いときだけ「不明」(接触は読めていない・日付は空)
        let empty = crate::handlers::call_quality::sheets::SheetStore::new();
        let none = MeetingData::from_store(&empty).await;
        assert!(none.contacts.is_none() && none.as_of.is_none() && none.last.is_none());
    }
    #[test]
    fn counted_dates_are_reused_until_a_sheet_is_fetched_again() {
        let call = sheet(
            &["deal_id", "duration_sec", "ts"],
            &[&["1", "120", "2026-09-05T01:00:00Z"]],
        );
        let mtg = sheet(&["deal_id", "開催日"], &[&["1", "2026-09-10"]]);
        let first = counted(Some(&mtg), None, Some(&call), None);
        let again = counted(Some(&mtg), None, Some(&call), None);
        // 同じ取得時刻のシートなら数え直さず、同じ結果（同じ入れ物）を返す。
        assert!(Arc::ptr_eq(
            first.1.as_ref().unwrap(),
            again.1.as_ref().unwrap()
        ));
        assert_eq!(again.1.as_ref().unwrap()["1"].to_string(), "2026-09-10");
        // 通話明細を取り直したら数え直す（覚えるのは 1 組だけ）。
        let newer = sheet(
            &["deal_id", "duration_sec", "ts"],
            &[&["1", "120", "2026-09-20T01:00:00Z"]],
        );
        let recounted = counted(Some(&mtg), None, Some(&newer), None);
        assert!(!Arc::ptr_eq(
            first.1.as_ref().unwrap(),
            recounted.1.as_ref().unwrap()
        ));
        assert_eq!(recounted.1.as_ref().unwrap()["1"].to_string(), "2026-09-20");
        // 通話明細が無ければ接触は「読めていない」。
        assert!(counted(Some(&mtg), None, None, None).1.is_none());
    }
    #[test]
    fn contact_is_not_judged_outside_the_sheet_or_before_it_is_read() {
        for (pipeline, m, status) in [
            ("21596025", MeetingData::default(), "unavailable"),
            ("743961896", meetings(10), "outside"),
        ] {
            let s = snapshot(120, "5", pipeline);
            let metrics = Metrics::new(&s, &m, day());
            assert!(!metrics
                .reasons(&s, "1", day())
                .iter()
                .any(|r| r["kind"] == "contact"));
            let q = OwnerQuery {
                owner: Some("all".into()),
                ..Default::default()
            };
            let row = serde_json::to_value(&metrics.rows(&s, &q, "", day())[0]).unwrap();
            assert_eq!(row["contact_status"], status);
            assert_eq!(row["last_contact"], Value::Null);
        }
        // 記録を読めていて接触が 1 件も無い取引（開始から 30 日以上）は「接触の記録が無い」。
        let s = snapshot(120, "5", "21596025");
        let m = MeetingData {
            contacts: Some(Arc::new(HashMap::new())),
            ..meetings(10)
        };
        let r = Metrics::new(&s, &m, day()).reasons(&s, "1", day());
        assert!(r
            .iter()
            .any(|r| r["kind"] == "contact" && r["text"] == "接触の記録が無い"));
    }
    #[test]
    fn sheet_failure_outside_pipeline_no_mtg_flags_or_dates_and_contract_values() {
        for (pipeline, m, status) in [
            ("21596025", MeetingData::default(), "unavailable"),
            ("743961896", meetings(90), "outside"),
        ] {
            let s = snapshot(120, "4", pipeline);
            let metrics = Metrics::new(&s, &m, day());
            assert!(!metrics
                .reasons(&s, "1", day())
                .iter()
                .any(|r| r["kind"] == "mtg"));
            let q = OwnerQuery {
                owner: Some("all".into()),
                ..Default::default()
            };
            let row = &serde_json::to_value(&metrics.rows(&s, &q, "", day())[0]).unwrap();
            assert_eq!(row["mtg_status"], status);
            assert_eq!(row["last_mtg"], Value::Null);
            assert_eq!(row["amount"], 300000.0);
            assert_eq!(row["achievement"], 0.2);
            assert_eq!(row["cpa"], 150000.0);
            assert_eq!(row["nps"], 4.0);
            assert_eq!(row["days_left"], 16);
        }
    }
    #[test]
    fn referral_deals_are_left_out_of_rows() {
        let mut s = snapshot(120, "5", "21596025");
        let base = s.deals["1"].clone();
        // 2: マーケ関連ステージ、3: 別のステージで名前に紹介料を含む
        for (id, name, stage) in [
            ("2", "架空広告_施策", "1278456227"),
            ("3", "9月分紹介料＿架空花子", "open"),
        ] {
            let mut d = base.clone();
            d.record.id = id.into();
            d.record
                .properties
                .insert("dealname".into(), Some(name.into()));
            d.record
                .properties
                .insert("dealstage".into(), Some(stage.into()));
            s.deals.insert(id.into(), d);
        }
        let m = MeetingData::default();
        let metrics = Metrics::new(&s, &m, day());
        let q = OwnerQuery {
            owner: Some("all".into()),
            ..Default::default()
        };
        let ids: Vec<String> = metrics
            .rows(&s, &q, "", day())
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, vec!["1".to_string()]);
    }
    #[test]
    fn cpa_peer_median_uses_full_population_and_threshold_inclusive() {
        let mut s = snapshot(120, "5", "21596025");
        let base = s.deals["1"].clone();
        for id in 2..=30 {
            let mut d = base.clone();
            d.record.id = id.to_string();
            d.record
                .properties
                .insert("amount".into(), Some("200000".into()));
            s.deals.insert(id.to_string(), d);
        }
        let m = MeetingData::default();
        assert!(Metrics::new(&s, &m, day())
            .reasons(&s, "1", day())
            .iter()
            .any(|r| r["kind"] == "cpa"));
        s.deals
            .get_mut("1")
            .unwrap()
            .record
            .properties
            .insert("amount".into(), Some("299999".into()));
        assert!(!Metrics::new(&s, &m, day())
            .reasons(&s, "1", day())
            .iter()
            .any(|r| r["kind"] == "cpa"));
        s.deals.remove("30");
        s.deals
            .get_mut("1")
            .unwrap()
            .record
            .properties
            .insert("amount".into(), Some("300000".into()));
        assert!(!Metrics::new(&s, &m, day())
            .reasons(&s, "1", day())
            .iter()
            .any(|r| r["kind"] == "cpa"));
    }
    #[test]
    fn zero_applicant_rows_need_listings_no_applications_at_all_and_an_active_deal() {
        use super::super::todo_signals::Facts;
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let mut s = snapshot(120, "5", "21596025");
        let base = s.deals["1"].clone();
        // (id, 終了ステージか, 事実)
        let cases: Vec<(&str, bool, Facts)> = vec![
            // 1: 求人票 2 件・応募 0 件 → 出る
            (
                "1",
                false,
                Facts {
                    listings: Some(2),
                    ..Facts::default()
                },
            ),
            // 2: 応募が契約期間より前にだけある → 全期間で見るので出ない
            (
                "2",
                false,
                Facts {
                    listings: Some(1),
                    days: vec![d("2020-01-05")],
                    ..Facts::default()
                },
            ),
            // 3: 求人票なし → 出ない
            (
                "3",
                false,
                Facts {
                    listings: Some(0),
                    ..Facts::default()
                },
            ),
            // 4: 応募あり → 出ない
            (
                "4",
                false,
                Facts {
                    listings: Some(1),
                    days: vec![d("2026-10-10")],
                    ..Facts::default()
                },
            ),
            // 5: 稼働中でない → 出ない
            (
                "5",
                true,
                Facts {
                    listings: Some(1),
                    ..Facts::default()
                },
            ),
            // 6: 応募日が不明な応募だけ → 応募はあるので出ない
            (
                "6",
                false,
                Facts {
                    listings: Some(1),
                    unknown_dates: 1,
                    ..Facts::default()
                },
            ),
            // 7: 古い保存データ（求人票の数が不明）→ 出ない
            (
                "7",
                false,
                Facts {
                    listings: None,
                    ..Facts::default()
                },
            ),
        ];
        for (id, ended, facts) in cases {
            let mut deal = base.clone();
            deal.record.id = id.into();
            if ended {
                deal.record
                    .properties
                    .insert("dealstage".into(), Some("1016664339".into()));
            }
            s.deals.insert(id.into(), deal);
            s.facts.insert(id.into(), facts);
        }
        let m = MeetingData::default();
        let metrics = Metrics::new(&s, &m, day());
        let q = OwnerQuery {
            owner: Some("all".into()),
            ..Default::default()
        };
        let rows = metrics.rows(&s, &q, "", day());
        let zero: Vec<&str> = rows
            .iter()
            .filter(|r| r.zero_applicants)
            .map(|r| r.id.as_str())
            .collect();
        assert_eq!(zero, vec!["1"]);
        let listings: Vec<_> = rows
            .iter()
            .map(|r| (r.id.as_str(), r.listing_count))
            .collect();
        assert!(listings.contains(&("1", Some(2))) && listings.contains(&("7", None)));
    }
    #[test]
    fn period_and_contract_application_counts_use_application_days_and_skip_uncountable_deals() {
        use super::super::todo_signals::Facts;
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let mut s = snapshot(30, "5", "21596025");
        let base = s.deals["1"].clone();
        // 契約は 2026-09-16 から 2026-11-01。応募日は契約の前・中・満了後に散らす。
        let days = vec![
            d("2026-09-10"),
            d("2026-09-20"),
            d("2026-10-12"),
            d("2026-11-05"),
        ];
        let cases: Vec<(&str, Option<&str>, Facts)> = vec![
            (
                "1",
                Some("0"),
                Facts {
                    listings: Some(1),
                    days: days.clone(),
                    ..Facts::default()
                },
            ),
            (
                "2",
                None,
                Facts {
                    listings: Some(1),
                    days: days.clone(),
                    unknown_dates: 1,
                    ..Facts::default()
                },
            ),
            (
                "3",
                Some("4"),
                Facts {
                    listings: Some(0),
                    ..Facts::default()
                },
            ),
            (
                "4",
                None,
                Facts {
                    listings: None,
                    days: days.clone(),
                    ..Facts::default()
                },
            ),
        ];
        for (id, interviews, facts) in cases {
            let mut deal = base.clone();
            deal.record.id = id.into();
            deal.record
                .properties
                .insert("mensetu".into(), interviews.map(Into::into));
            s.deals.insert(id.into(), deal);
            s.facts.insert(id.into(), facts);
        }
        let m = MeetingData::default();
        let metrics = Metrics::new(&s, &m, day());
        let q = OwnerQuery {
            owner: Some("all".into()),
            ..Default::default()
        };
        let by = |rows: &[DealBoardRow], id: &str| {
            let r = rows.iter().find(|r| r.id == id).unwrap();
            (r.contract_applications, r.period_applications, r.interviews)
        };
        let rows = metrics.rows_in(&s, &q, "", day(), Some((d("2026-10-10"), d("2026-10-16"))));
        assert_eq!(by(&rows, "1"), (Some(2), Some(1), Some(0.0)));
        // 応募日が分からない応募がある・求人票が無い・求人票の数が不明な取引は数えない。面接の数字が空なら null。
        assert_eq!(by(&rows, "2"), (None, None, None));
        assert_eq!(by(&rows, "3"), (None, None, Some(4.0)));
        assert_eq!(by(&rows, "4"), (None, None, None));
        let rows = metrics.rows_in(&s, &q, "", day(), Some((d("2026-10-13"), d("2026-10-16"))));
        assert_eq!(by(&rows, "1").1, Some(0));
        let rows = metrics.rows(&s, &q, "", day());
        assert_eq!(by(&rows, "1").1, None);
    }
    #[test]
    fn period_query_needs_both_dates_in_order() {
        let q = |from: Option<&str>, to: Option<&str>| PeriodQuery {
            period_from: from.map(Into::into),
            period_to: to.map(Into::into),
        };
        assert!(q(None, None).range().unwrap().is_none());
        assert!(q(Some("2026-10-01"), Some("2026-10-16"))
            .range()
            .unwrap()
            .is_some());
        assert!(q(Some("2026-10-16"), Some("2026-10-01")).range().is_err());
        assert!(q(Some("2026-10-01"), None).range().is_err());
        assert!(q(Some("2026-13-01"), Some("2026-13-02")).range().is_err());
    }
}
