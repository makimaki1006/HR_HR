//! Consultant flags reuse the KPI sheet calculation without a second definition.
use super::consultant_flags::{self, Comparison};
use super::owner_todo::{OwnerQuery, Snapshot};
use super::*;
use crate::handlers::cs_dashboard as cs;

pub(super) async fn scope(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<OwnerQuery>,
) -> Result<impl IntoResponse, ReadError> {
    let email = authorized_user(&state, &access, &session).await?;
    query.validate()?;
    let view = owner_todo::work_view(listings::service(&access)?).await;
    let body = view
        .snapshot
        .filter(|s| s.complete)
        .map(|s| s.scope(&query, &email))
        .unwrap_or_else(|| json!({"status":if view.failed {"failed"} else {"preparing"}}));
    Ok(([(header::CACHE_CONTROL, "private, no-store")], Json(body)))
}
pub(super) fn rows(
    sheets: &cs::Sheets,
    today: chrono::NaiveDate,
    snapshot: &Snapshot,
) -> Vec<Value> {
    let (rows, _) = cs::routes::deal_rows(sheets, today);
    rows.into_iter()
        .filter(|r| {
            snapshot
                .deals
                .get(r["deal_id"].as_str().unwrap_or_default())
                .is_some_and(|d| listings::is_delivery(d.record.value("pipeline")))
        })
        .collect()
}
pub(super) async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<OwnerQuery>,
) -> Result<impl IntoResponse, ReadError> {
    let email = authorized_user(&state, &access, &session).await?;
    query.validate()?;
    let view = owner_todo::work_view(listings::service(&access)?).await;
    let Some(snapshot) = view.snapshot.filter(|s| s.complete) else {
        return Ok((
            [(header::CACHE_CONTROL, "private, no-store")],
            Json(json!({"status":if view.failed {"failed"} else {"preparing"}})),
        ));
    };
    let cq = crate::handlers::call_quality::routes::cq_state()
        .map_err(|_| ReadError(StatusCode::SERVICE_UNAVAILABLE, "consulting_unavailable"))?;
    let sheets = cs::load(&cq.client, &cq.store)
        .await
        .map_err(|_| ReadError(StatusCode::SERVICE_UNAVAILABLE, "consulting_unavailable"))?;
    let day = cs::data_as_of(&sheets.meta).and_then(|d| cs::date10(&d));
    let today = day.unwrap_or_else(|| owner_todo::today(chrono::Utc::now()));
    let snapshot_copy = snapshot.clone();
    let rows = tokio::task::spawn_blocking(move || rows(&sheets, today, &snapshot_copy))
        .await
        .map_err(|_| ReadError(StatusCode::INTERNAL_SERVER_ERROR, "consulting_unavailable"))?;
    let all_flags = rows
        .iter()
        .flat_map(|r| {
            r["flags"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|f| {
                    (
                        r["deal_id"].as_str().unwrap_or_default().to_string(),
                        f.to_string(),
                    )
                })
        })
        .collect();
    let day = day.map(|d| d.to_string());
    let comparison =
        consultant_flags::comparison(state.audit.clone(), day.clone(), all_flags).await;
    let previous_day = comparison.as_ref().and_then(|c| c.previous_day.clone());
    let (owner, _) = snapshot.selection(&query, &email);
    let empty = Comparison::default();
    let output: Vec<_> = rows
        .into_iter()
        .filter(|r| {
            snapshot.matches(
                r["deal_id"].as_str().unwrap_or_default(),
                query.field(),
                owner.as_deref(),
            ) && r["n_flags"].as_u64().unwrap_or(0) > 0
        })
        .map(|mut row| {
            let id = row["deal_id"].as_str().unwrap_or_default().to_string();
            row["risk_flags"] = json!(comparison.as_ref().unwrap_or(&empty).flags(
                &id,
                row["flags"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                if comparison.is_some() {
                    day.as_deref()
                } else {
                    None
                }
            ));
            row["company"] = json!(snapshot.companies.get(&id).and_then(|c| c.1.as_deref()));
            if let Some(d) = snapshot.deals.get(&id) {
                row["actionyotei"] = json!(d.assignment.actionyotei);
                row["actionnaiyou"] =
                    json!(snapshot.action_label(d.assignment.actionnaiyou.as_deref()));
            }
            row
        })
        .collect();
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(
            json!({"status":"ready","data_day":day,"previous_day":previous_day,"comparison":if previous_day.is_some(){"ready"}else if comparison.is_some(){"first"}else{"unavailable"},"rows":output,"refreshing":view.refreshing,"refresh_failed":view.failed}),
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::call_quality::sheets::SheetData;
    fn sheet(header: &[&str], rows: &[&[&str]]) -> Arc<SheetData> {
        Arc::new(SheetData {
            header: header.iter().map(|s| s.to_string()).collect(),
            rows: rows
                .iter()
                .map(|r| r.iter().map(|s| Arc::from(*s)).collect())
                .collect(),
            fetched_at: std::time::Instant::now(),
        })
    }
    #[test]
    fn consultant_flags_match_kpi_on_same_fictional_deal() {
        let empty = sheet(&[], &[]);
        let mut sheets = cs::Sheets {
            pending_recordings: None,
            mail_history: crate::handlers::cs_dashboard::empty_history_sheet(),
            zoom_mtg: crate::handlers::cs_dashboard::empty_history_sheet(),
            deal: sheet(
                &[
                    "deal_id",
                    "dealname",
                    "is_active",
                    "contract_start_date",
                    "contract_expiration_date",
                ],
                &[&["101", "架空の採用支援", "true", "2026-08-01", "2026-11-01"]],
            ),
            owner_hist: sheet(
                &["deal_id", "owner", "date", "retired"],
                &[&["101", "架空の担当", "2026-10-01", "true"]],
            ),
            call: empty.clone(),
            mtg: empty.clone(),
            history: empty.clone(),
            customer: empty.clone(),
            mail_mtg: empty.clone(),
            handover: empty.clone(),
            meta: empty,
            all_cached: true,
        };
        let record = Record {
            id: "101".into(),
            properties: [("pipeline".into(), Some("21596025".into()))]
                .into_iter()
                .collect(),
        };
        let mut snapshot = Snapshot::default();
        snapshot.deals.insert(
            "101".into(),
            super::super::owner_todo::WorkDeal {
                assignment: super::super::owner_todo::Assignment::from_record(&record),
                record,
            },
        );
        let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let risk = rows(&sheets, day, &snapshot);
        let kpi = cs::routes::deal_rows(&sheets, day).0;
        assert_eq!(risk[0]["flags"], kpi[0]["flags"]);
        assert_eq!(
            risk[0]["flags"],
            json!(["満了まで60日以内", "接触の記録が無い", "担当が退職者のまま"])
        );
        assert_eq!(risk[0]["days_left"], 13);
        // A 120-second call on Oct 18 must agree with the detail fixture:
        // one day since contact on Oct 19, with no missing-contact flag.
        sheets.call = sheet(
            &["deal_id", "ts", "duration_sec"],
            &[&["101", "2026-10-18T03:00:00Z", "120"]],
        );
        let contacted = rows(&sheets, day, &snapshot);
        assert_eq!(contacted.len(), 1);
        assert_eq!(contacted[0]["deal_id"], "101");
        assert_eq!(contacted[0]["days_since_contact"], 1);
        assert_eq!(
            contacted[0]["flags"],
            json!(["満了まで60日以内", "担当が退職者のまま"])
        );
    }
}
