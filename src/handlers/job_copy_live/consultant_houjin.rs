//! 法人の集計はコンサルKPIを共用。会社名の類似判定はせず、共通の案件IDで結ぶ。
use super::owner_todo::{OwnerQuery, Snapshot};
use super::*;
use crate::handlers::cs_dashboard as cs;
use chrono::NaiveDate;

#[derive(Deserialize)]
pub(super) struct HoujinQuery {
    #[serde(flatten)]
    owner: OwnerQuery,
    houjin: Option<String>,
}

pub(super) fn customer(sheets: &cs::Sheets, houjin: Option<&str>, today: NaiveDate) -> Value {
    cs::routes::freshen(
        cs::routes::build_customer(sheets, houjin, today),
        sheets,
        today,
    )
}

fn report(
    sheets: &cs::Sheets,
    snapshot: &Snapshot,
    query: &HoujinQuery,
    email: &str,
    today: NaiveDate,
) -> Value {
    let (owner, _) = snapshot.selection(&query.owner, email);
    let deals = cs::deals_all_of(&sheets.deal);
    let eligible: BTreeSet<_> = deals
        .iter()
        .filter(|d| snapshot.matches(&d.id, query.owner.field(), owner.as_deref()))
        .map(|d| d.houjin_resolved.as_str())
        .collect();
    let mut index = customer(sheets, None, today);
    let rows = index["index"].as_array_mut().expect("customer index");
    if owner.is_some() {
        rows.retain(|r| r["houjin"].as_str().is_some_and(|h| eligible.contains(h)));
    }
    let links: BTreeMap<String, BTreeMap<String, Option<String>>> = rows
        .iter()
        .map(|r| {
            let h = r["houjin"].as_str().unwrap_or_default();
            let companies = deals
                .iter()
                .filter(|d| d.houjin_resolved == h)
                .filter_map(|d| snapshot.companies.get(&d.id))
                .cloned()
                .collect();
            (h.to_string(), companies)
        })
        .collect();
    let unmatched = links.values().filter(|v| v.is_empty()).count();
    let default = rows
        .iter()
        .max_by(|a, b| {
            a["deals"]
                .as_f64()
                .unwrap_or_default()
                .total_cmp(&b["deals"].as_f64().unwrap_or_default())
        })
        .and_then(|r| r["houjin"].as_str())
        .map(str::to_string);
    let selected = query
        .houjin
        .as_deref()
        .filter(|h| rows.iter().any(|r| r["houjin"] == *h))
        .or(default.as_deref());
    json!({"status":"ready", "index":index["index"], "default_houjin":default,
        "selected_houjin":selected, "customer":selected.map(|h| customer(sheets,Some(h),today)),
        "companies":selected.and_then(|h| links.get(h)).map(|m| m.iter().map(|(id,name)| json!({"id":id,"name":name})).collect::<Vec<_>>()).unwrap_or_default(),
        "linkage":{"total":links.len(),"unmatched":unmatched}, "scope":snapshot.scope(&query.owner,email)})
}

pub(super) async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<HoujinQuery>,
) -> Result<impl IntoResponse, ReadError> {
    let email = authorized_user(&state, &access, &session).await?;
    query.owner.validate()?;
    let view = owner_todo::work_view(listings::service(&access)?).await;
    let body = if let Some(snapshot) = view.snapshot.filter(|s| s.complete) {
        let cq = crate::handlers::call_quality::routes::cq_state()
            .map_err(|_| ReadError(StatusCode::SERVICE_UNAVAILABLE, "consulting_unavailable"))?;
        let sheets = cs::load(&cq.client, &cq.store)
            .await
            .map_err(|_| ReadError(StatusCode::SERVICE_UNAVAILABLE, "consulting_unavailable"))?;
        tokio::task::spawn_blocking(move || {
            report(
                &sheets,
                &snapshot,
                &query,
                &email,
                owner_todo::today(chrono::Utc::now()),
            )
        })
        .await
        .map_err(|_| ReadError(StatusCode::INTERNAL_SERVER_ERROR, "consulting_unavailable"))?
    } else {
        json!({"status":if view.failed {"failed"} else {"preparing"}})
    };
    Ok(([(header::CACHE_CONTROL, "private, no-store")], Json(body)))
}

#[cfg(test)]
mod tests {
    use super::super::owner_todo::{Assignment, WorkDeal};
    use super::*;
    use crate::handlers::call_quality::sheets::SheetData;
    fn sheet(header: &[&str], rows: &[&[&str]]) -> Arc<SheetData> {
        Arc::new(SheetData {
            header: header.iter().map(|s| s.to_string()).collect(),
            rows: rows
                .iter()
                .map(|r| r.iter().map(|v| Arc::from(*v)).collect())
                .collect(),
            fetched_at: Instant::now(),
        })
    }
    fn fixture() -> cs::Sheets {
        let empty = sheet(&[], &[]);
        cs::Sheets {
            pending_recordings: None,
            all_cached: true,
            deal: sheet(
                &[
                    "deal_id",
                    "dealname",
                    "houjin_resolved",
                    "is_active",
                    "contract_kind",
                    "amount",
                    "syoudaku",
                    "kyoten_key",
                    "contract_start_date",
                ],
                &[
                    &[
                        "101",
                        "架空の案件A",
                        "法人A",
                        "true",
                        "本体契約",
                        "600000",
                        "3",
                        "拠点A",
                        "2026-08-01",
                    ],
                    &[
                        "102",
                        "架空の案件B",
                        "法人B",
                        "true",
                        "本体契約",
                        "200000",
                        "1",
                        "拠点B",
                        "2026-09-01",
                    ],
                    &[
                        "103",
                        "架空の案件C",
                        "法人C",
                        "true",
                        "本体契約",
                        "100000",
                        "0",
                        "拠点C",
                        "2026-09-01",
                    ],
                ],
            ),
            customer: sheet(
                &[
                    "houjin",
                    "name",
                    "is_display_target",
                    "deal_count",
                    "ltv",
                    "kyoten_unique",
                ],
                &[
                    &["法人A", "架空の法人A", "true", "1", "600000", "1"],
                    &["法人B", "架空の法人B", "true", "1", "200000", "1"],
                    &["法人C", "架空の法人C", "true", "1", "100000", "1"],
                ],
            ),
            call: empty.clone(),
            mtg: empty.clone(),
            history: empty.clone(),
            mail_mtg: empty.clone(),
            handover: empty.clone(),
            owner_hist: empty.clone(),
            meta: empty,
        }
    }
    fn snapshot() -> Snapshot {
        let mut snapshot = Snapshot::for_index(
            [("101", "11"), ("102", "22"), ("103", "11")]
                .into_iter()
                .map(|(id, owner)| {
                    (
                        id.to_string(),
                        WorkDeal {
                            record: Record {
                                id: id.into(),
                                properties: BTreeMap::new(),
                            },
                            assignment: Assignment {
                                consultant: Some(owner.into()),
                                ..Default::default()
                            },
                        },
                    )
                })
                .collect(),
            vec![],
        );
        // 会社名はシートと異なっていても案件IDの関連で結び付く。
        snapshot
            .companies
            .insert("101".into(), ("501".into(), Some("架空の別表記A".into())));
        snapshot
            .companies
            .insert("102".into(), ("502".into(), Some("架空の別表記B".into())));
        snapshot
    }
    #[test]
    fn same_customer_payload_as_consulting_and_owner_scoped_index() {
        let sheets = fixture();
        let today = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let snapshot = snapshot();
        for h in [None, Some("法人A"), Some("法人B")] {
            assert_eq!(
                customer(&sheets, h, today),
                cs::routes::freshen(
                    cs::routes::build_customer(&sheets, h, today),
                    &sheets,
                    today
                )
            );
        }
        let q = |owner: &str, houjin: Option<&str>| HoujinQuery {
            owner: OwnerQuery {
                owner: Some(owner.into()),
                owner_field: None,
            },
            houjin: houjin.map(str::to_string),
        };
        let all = report(&sheets, &snapshot, &q("all", Some("法人A")), "", today);
        assert_eq!(all["index"].as_array().unwrap().len(), 3);
        assert_eq!(all["linkage"], json!({"total":3,"unmatched":1}));
        assert_eq!(all["customer"], customer(&sheets, Some("法人A"), today));
        assert_eq!(all["customer"]["deals"][0]["amount"], 600000.0);
        assert_eq!(all["companies"][0]["id"], "501");
        let scoped = report(&sheets, &snapshot, &q("22", Some("法人A")), "", today);
        assert_eq!(scoped["index"].as_array().unwrap().len(), 1);
        assert_eq!(scoped["selected_houjin"], "法人B");
        assert_eq!(scoped["customer"]["meta"]["houjin"], "法人B");
        let none = report(&sheets, &snapshot, &q("33", None), "", today);
        assert!(none["customer"].is_null());
    }
    #[test]
    fn option_only_customer_still_matches_owner_and_company() {
        let mut sheets = fixture();
        sheets.deal = sheet(
            &["deal_id", "dealname", "houjin_resolved", "contract_kind"],
            &[&["101", "求人追加_架空の案件", "法人A", "求人追加"]],
        );
        let query = HoujinQuery {
            owner: OwnerQuery {
                owner: Some("11".into()),
                owner_field: None,
            },
            houjin: Some("法人A".into()),
        };
        let result = report(
            &sheets,
            &snapshot(),
            &query,
            "",
            NaiveDate::from_ymd_opt(2026, 10, 19).unwrap(),
        );
        assert_eq!(result["index"].as_array().unwrap().len(), 1);
        assert_eq!(result["companies"][0]["id"], "501");
        assert!(result["customer"]["deals"].as_array().unwrap().is_empty());
    }
}
