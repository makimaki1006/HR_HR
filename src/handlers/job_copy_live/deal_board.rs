//! 案件一覧。取引は既存の5分キャッシュ、MTGはKPIの先読み済みシートだけを使う。
use super::owner_todo::{OwnerQuery, Snapshot, ENDED_STAGES};
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
    pub(super) last: Option<LastMeetings>,
    pub(super) as_of: Option<String>,
}
impl MeetingData {
    pub(super) async fn cached() -> Self {
        let Ok(state) = crate::handlers::call_quality::routes::cq_state() else {
            return Self::default();
        };
        // 既存の定期先読みを借りる。画面要求からSheets APIを呼ばない。
        let mtg = state.store.fresh(cs::SHEET_MTG).await;
        let meta = state.store.fresh(cs::SHEET_META).await;
        let zoom = state.store.fresh(cs::SHEET_ZOOM_MTG).await;
        Self {
            last: mtg.as_ref().map(|m| {
                cs::last_recorded_mtg_by_deal(&cs::factual_mtg(
                    m,
                    zoom.as_deref().unwrap_or(&cs::empty_history_sheet()),
                ))
            }),
            as_of: meta.as_ref().and_then(|m| cs::data_as_of(m)),
        }
    }
}
#[derive(Serialize, Deserialize, ts_rs::TS)]
struct DealBoardReason {
    #[ts(
        type = "'expiration' | 'contact' | 'stalled' | 'first_contract' | 'nps' | 'target' | 'cpa' | 'mtg'"
    )]
    kind: String,
    text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    strong: Option<bool>,
}
#[derive(Serialize, Deserialize, ts_rs::TS)]
struct DealBoardRow {
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
    last_contact: Option<String>,
    last_mtg: Option<String>,
    #[ts(type = "'ready' | 'outside' | 'unavailable' | 'none'")]
    mtg_status: String,
    nps: Option<f64>,
    applications: Option<f64>,
    #[serde(default)]
    #[ts(type = "number | null")]
    recent_applications: Option<i64>,
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
        }
    }
    pub(super) fn reasons(&self, snapshot: &Snapshot, id: &str, today: NaiveDate) -> Vec<Value> {
        let Some(work) = snapshot.deals.get(id) else {
            return vec![];
        };
        let d = &self.deals[id];
        let mut reasons = super::todo_signals::reasons(work, snapshot.facts.get(id), today);
        if !d.is_active || !listings::is_delivery(work.record.value("pipeline")) {
            return reasons;
        }
        let mut add = |kind: &str, text: &str, strong: bool| {
            reasons.push(json!({"kind":kind,"text":text,"strong":strong}))
        };
        if cs::low_nps(nps(&work.record)) {
            add("nps", "定期NPSが4以下", false);
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
        if work.record.value("pipeline") == Some("21596025") {
            if let Some(last) = &self.meetings.last {
                let gap = cs::mtg_gap_of(d, last, today);
                if let Some(days) = gap.days {
                    let band = cs::mtg_band_for_days(days);
                    if band.is_alert() {
                        add("mtg", band.label(), days >= cs::MTG_GAP_CRITICAL_DAYS);
                    }
                }
            }
        }
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
            let d = &self.deals[id];
            let company = snapshot.companies.get(id);
            let consultant = snapshot.owners.iter().find(|o| Some(o.id.as_str()) == work.assignment.consultant.as_deref());
            let supported = work.record.value("pipeline") == Some("21596025");
            let last = self.meetings.last.as_ref().and_then(|m| m.get(id)).and_then(|(d, _)| *d);
            let mtg_status = if !supported { "outside" } else if self.meetings.last.is_none() { "unavailable" } else if last.is_none() { "none" } else { "ready" };
            serde_json::from_value(json!({"id":id,"name":work.record.value("dealname"),"company":company.and_then(|c| c.1.as_ref()),
                "pipeline":work.record.value("pipeline"),"active":d.is_active,"stage":snapshot.stages.get(&d.stage),
                "consultant":consultant.map(|o| &o.name),"reasons":self.reasons(snapshot,id,today),
                "months":contract_month(&d.contract_start_date,today),"expiration":cs::date10(&d.contract_expiration_date).map(|d| d.to_string()),
                "days_left":cs::date10(&d.contract_expiration_date).map(|d| (d-today).num_days()),
                "last_contact":work.record.value("notes_last_contacted").and_then(super::owner_todo::date).map(|d| d.to_string()),
                "last_mtg":if supported {last.map(|d| d.to_string())} else {None},"mtg_status":mtg_status,
                "nps":nps(&work.record),"applications":d.oubo,"interviews":d.mensetu,"hires":d.syoudaku,
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
        let metrics = Metrics::new(&snapshot, &meetings, today);
        let mut rows = metrics.rows_in(&snapshot, &query, &email, today, period);
        for row in &mut rows {
            row.recent_applications = apps
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
            last: Some(HashMap::from([(
                "1".into(),
                (Some(day() - chrono::Duration::days(days)), None),
            )])),
            as_of: Some("2026-10-15".into()),
        }
    }
    fn reasons(s: &Snapshot, m: &MeetingData) -> Vec<Value> {
        Metrics::new(s, m, day()).reasons(s, "1", day())
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
    fn mtg_exact_days_only_ninety_is_strong_even_near_expiry() {
        let s = snapshot(120, "5", "21596025");
        for (days, label, strong) in [
            (29, None, false),
            (30, Some("MTGが30〜59日空いている"), false),
            (59, Some("MTGが30〜59日空いている"), false),
            (60, Some("MTGが60〜89日空いている"), false),
            (89, Some("MTGが60〜89日空いている"), false),
            (90, Some("MTGが90日以上空いている"), true),
        ] {
            let r = reasons(&s, &meetings(days));
            let mtg = r.iter().find(|r| r["kind"] == "mtg");
            assert_eq!(mtg.is_some(), label.is_some(), "days={days}");
            if let Some(mtg) = mtg {
                assert_eq!(mtg["strong"], strong);
                assert_eq!(mtg["text"], label.unwrap());
            }
        }
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
