//! Good signals share the home periods and KPI CPA calculation. Read only.
use super::owner_todo::OwnerQuery;
use super::*;
use crate::handlers::cs_dashboard as cs;
use chrono::NaiveDate;

type Applications = BTreeMap<String, (i64, i64)>;
fn positives(sheets: &cs::Sheets, today: NaiveDate) -> BTreeMap<String, String> {
    let mut latest = BTreeMap::<String, (NaiveDate, String, String)>::new();
    for row in &sheets.mtg.rows {
        let get = |key| sheets.mtg.get(row, key).trim();
        let Some(day) = cs::date10(get("開催日")) else {
            continue;
        };
        if !(0..=90).contains(&(today - day).num_days()) {
            continue;
        }
        let id = get("deal_id");
        if id.is_empty() {
            continue;
        }
        let time = get("開催日時(JST)").to_string();
        let entry = latest
            .entry(id.into())
            .or_insert((day, time.clone(), String::new()));
        if (day, &time) >= (entry.0, &entry.1) {
            *entry = (day, time, get("前向きシグナル").to_string());
        }
    }
    latest
        .into_iter()
        .filter_map(|(id, (_, _, text))| {
            (!matches!(text.as_str(), "" | "なし" | "特になし" | "—")).then_some((id, text))
        })
        .collect()
}
fn signals(
    row: &Value,
    applications: Option<&(i64, i64)>,
    positive: Option<&String>,
) -> Vec<Value> {
    let mut flags = Vec::new();
    let mut add = |key: &str, label: String| {
        flags.push(json!({"key":key,"label":label,"new":false,"since":null}))
    };
    if let Some(&(prev, recent)) = applications {
        if listings::application_trend(prev, recent) == "more" {
            add("applications", format!("応募 {prev} → {recent}"));
        }
    }
    // These values already use cpa_band_medians and cpa_band_of in deal_rows.
    if let (Some(value), Some(median)) = (row["cpa"].as_f64(), row["cpa_band_median"].as_f64()) {
        if value < median {
            add(
                "cpa",
                format!(
                    "採用単価 {:.1}万円（区分の中央値 {:.1}万円）",
                    value / 10000.0,
                    median / 10000.0
                ),
            );
        }
    }
    if let Some(text) = positive {
        add(
            "positive",
            format!("MTG：{}", text.chars().take(30).collect::<String>()),
        );
    }
    flags
}
fn good_rows(
    mut rows: Vec<Value>,
    apps: &Applications,
    positive: &BTreeMap<String, String>,
) -> Vec<Value> {
    for row in &mut rows {
        let id = row["deal_id"].as_str().unwrap_or_default();
        let flags = signals(row, apps.get(id), positive.get(id));
        row["good_flags"] = json!(flags);
        row["risk_flags"] = json!([]);
    }
    rows.retain(|r| r["good_flags"].as_array().is_some_and(|f| !f.is_empty()));
    rows.sort_by(|a, b| {
        b["good_flags"]
            .as_array()
            .map(Vec::len)
            .cmp(&a["good_flags"].as_array().map(Vec::len))
            .then_with(|| {
                a["days_left"]
                    .as_i64()
                    .unwrap_or(i64::MAX)
                    .cmp(&b["days_left"].as_i64().unwrap_or(i64::MAX))
            })
            .then_with(|| a["deal_id"].as_str().cmp(&b["deal_id"].as_str()))
    });
    rows
}
pub(super) async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<OwnerQuery>,
) -> Result<impl IntoResponse, ReadError> {
    let email = authorized_user(&state, &access, &session).await?;
    query.validate()?;
    let service = listings::service(&access)?;
    let view = owner_todo::work_view(service).await;
    let apps = listings::consultant_applications(service).await;
    let body = if let (Some(snapshot), Some((apps, refreshing, failed))) =
        (view.snapshot.filter(|s| s.complete), apps)
    {
        let cq = crate::handlers::call_quality::routes::cq_state()
            .map_err(|_| ReadError(StatusCode::SERVICE_UNAVAILABLE, "consulting_unavailable"))?;
        let sheets = cs::load(&cq.client, &cq.store)
            .await
            .map_err(|_| ReadError(StatusCode::SERVICE_UNAVAILABLE, "consulting_unavailable"))?;
        let day = cs::data_as_of(&sheets.meta).and_then(|d| cs::date10(&d));
        let today = day.unwrap_or_else(|| owner_todo::today(chrono::Utc::now()));
        let copy = snapshot.clone();
        let mut rows = tokio::task::spawn_blocking(move || {
            good_rows(
                consultant_risk::rows(&sheets, today, &copy),
                &apps,
                &positives(&sheets, today),
            )
        })
        .await
        .map_err(|_| ReadError(StatusCode::INTERNAL_SERVER_ERROR, "consulting_unavailable"))?;
        let (owner, _) = snapshot.selection(&query, &email);
        rows.retain(|r| {
            snapshot.matches(
                r["deal_id"].as_str().unwrap_or_default(),
                query.field(),
                owner.as_deref(),
            )
        });
        for row in &mut rows {
            let id = row["deal_id"].as_str().unwrap_or_default().to_string();
            row["company"] = json!(snapshot.companies.get(&id).and_then(|c| c.1.as_deref()));
            if let Some(deal) = snapshot.deals.get(&id) {
                row["actionyotei"] = json!(deal.assignment.actionyotei);
                row["actionnaiyou"] =
                    json!(snapshot.action_label(deal.assignment.actionnaiyou.as_deref()));
            }
        }
        json!({"status":"ready","data_day":day.map(|d| d.to_string()),"rows":rows,"refreshing":view.refreshing || refreshing,"refresh_failed":view.failed || failed})
    } else {
        json!({"status":if view.failed {"failed"} else {"preparing"}})
    };
    Ok(([(header::CACHE_CONTROL, "private, no-store")], Json(body)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::call_quality::sheets::SheetData;
    fn sheet(header: &[&str], rows: &[Vec<String>]) -> Arc<SheetData> {
        Arc::new(SheetData {
            header: header.iter().map(|s| s.to_string()).collect(),
            rows: rows
                .iter()
                .map(|r| r.iter().map(|s| Arc::from(s.as_str())).collect())
                .collect(),
            fetched_at: std::time::Instant::now(),
        })
    }
    fn sheets() -> cs::Sheets {
        let empty = sheet(&[], &[]);
        cs::Sheets {
            pending_recordings: None,
            deal: empty.clone(),
            owner_hist: empty.clone(),
            call: empty.clone(),
            mtg: empty.clone(),
            history: empty.clone(),
            customer: empty.clone(),
            mail_mtg: empty.clone(),
            handover: empty.clone(),
            meta: empty,
            all_cached: true,
        }
    }
    #[test]
    fn application_and_cpa_boundaries_and_all_labels() {
        let row = json!({"cpa":50000,"cpa_band_median":100000});
        let text =
            "継続に向けて採用人数を増やすことを検討しており次回の打合せで相談する".to_string();
        let flags = signals(&row, Some(&(3, 7)), Some(&text));
        assert_eq!(
            flags
                .iter()
                .map(|f| f["label"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec![
                "応募 3 → 7",
                "採用単価 5.0万円（区分の中央値 10.0万円）",
                &format!("MTG：{}", text.chars().take(30).collect::<String>())
            ]
        );
        for (prev, recent, yes) in [
            (0, 0, false),
            (0, 1, true),
            (7, 7, false),
            (3, 7, true),
            (100, 101, false),
            (100, 102, true),
        ] {
            assert_eq!(
                !signals(&json!({}), Some(&(prev, recent)), None).is_empty(),
                yes
            );
        }
        assert!(signals(&json!({"cpa":null,"cpa_band_median":100000}), None, None).is_empty());
        assert!(signals(&json!({"cpa":100000,"cpa_band_median":100000}), None, None).is_empty());
    }
    #[test]
    fn cpa_values_and_medians_come_from_kpi_including_zero_hires() {
        let mut sheets = sheets();
        let initial = sheet(
            &[
                "deal_id",
                "dealname",
                "is_active",
                "contract_kind",
                "contract_start_date",
                "contract_expiration_date",
                "amount",
                "syoudaku",
            ],
            &[
                vec![
                    "101",
                    "架空の支援A",
                    "true",
                    "本体契約",
                    "2026-08-01",
                    "2026-12-01",
                    "100000",
                    "2",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                vec![
                    "102",
                    "架空の支援B",
                    "true",
                    "本体契約",
                    "2026-08-01",
                    "2026-12-01",
                    "200000",
                    "2",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                vec![
                    "103",
                    "架空の支援C",
                    "true",
                    "本体契約",
                    "2026-08-01",
                    "2026-12-01",
                    "300000",
                    "2",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                vec![
                    "104",
                    "架空の支援D",
                    "true",
                    "本体契約",
                    "2026-08-01",
                    "2026-12-01",
                    "300000",
                    "0",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
            ],
        );
        let mut data: Vec<Vec<String>> = initial
            .rows
            .iter()
            .map(|r| r.iter().map(|v| v.to_string()).collect())
            .collect();
        for n in 0..27 {
            data.push(vec![
                (200 + n).to_string(),
                "架空の比較案件".into(),
                "true".into(),
                "本体契約".into(),
                "2026-08-01".into(),
                "2026-12-01".into(),
                "200000".into(),
                "2".into(),
            ]);
        }
        for row in &mut data {
            row.push("4".into());
        }
        let mut header: Vec<_> = initial.header.iter().map(String::as_str).collect();
        header.push("contract_period");
        sheets.deal = sheet(&header, &data);
        let day = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let rows = cs::routes::deal_rows(&sheets, day).0;
        let row = |id: &str| rows.iter().find(|r| r["deal_id"] == id).unwrap();
        assert_eq!(row("101")["cpa"], 50000.0);
        assert_eq!(row("101")["cpa_band_median"], 100000.0);
        assert_eq!(signals(row("101"), None, None)[0]["key"], "cpa");
        assert!(signals(row("102"), None, None).is_empty());
        assert!(row("104")["cpa"].is_null());
        assert!(signals(row("104"), None, None).is_empty());
    }
    #[test]
    fn latest_mtg_not_latest_positive_and_ninety_day_window() {
        let mut sheets = sheets();
        let today = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let mut rows = vec![];
        for (id, age, text) in [
            ("101", 0, "特になし"),
            ("102", 91, "継続する"),
            ("103", 90, "継続する"),
            ("104", 0, "なし"),
            ("105", 0, "—"),
            ("106", 0, ""),
        ] {
            rows.push(vec![
                id.into(),
                (today - chrono::Duration::days(age)).to_string(),
                String::new(),
                text.into(),
            ]);
        }
        rows.push(vec![
            "101".into(),
            "2026-10-18".into(),
            String::new(),
            "前は前向き".into(),
        ]);
        rows.push(vec![
            "107".into(),
            "2026-10-19".into(),
            "2026-10-19 10:00".into(),
            "継続".into(),
        ]);
        rows.push(vec![
            "107".into(),
            "2026-10-19".into(),
            "2026-10-19 11:00".into(),
            "なし".into(),
        ]);
        sheets.mtg = sheet(
            &["deal_id", "開催日", "開催日時(JST)", "前向きシグナル"],
            &rows,
        );
        assert_eq!(
            positives(&sheets, today),
            [("103".into(), "継続する".into())].into_iter().collect()
        );
    }
    #[test]
    fn sort_by_signal_count_then_expiration_keep_risk_flags() {
        let rows = vec![
            json!({"deal_id":"101","days_left":10,"flags":["満了まで60日以内"]}),
            json!({"deal_id":"102","days_left":5}),
            json!({"deal_id":"103","days_left":90,"cpa":5,"cpa_band_median":10}),
        ];
        let apps = ["101", "102", "103"]
            .map(|id| (id.into(), (0, 1)))
            .into_iter()
            .collect();
        let output = good_rows(rows, &apps, &BTreeMap::new());
        assert_eq!(
            output
                .iter()
                .map(|r| r["deal_id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["103", "102", "101"]
        );
        assert_eq!(output[2]["flags"], json!(["満了まで60日以内"]));
    }
}
