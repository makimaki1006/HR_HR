//! 案件管理で扱う取引のパイプライン（納品管理だけ）と、顧客の案件一覧。
//! 計上・商談・債権管理などのパイプラインの取引は、一覧にも件数にも入れない（正本はこのファイルの定数）。
use super::*;
use std::time::Instant as StdInstant;

/// 案件管理に出すパイプライン（HubSpot のパイプライン ID と、定義が読めないときの呼び名）。
pub(super) const DELIVERY_PIPELINES: [(&str, &str); 2] = [
    ("21596025", "リクロジ_納品管理"),
    ("743961896", "請負契約_納品管理"),
];
const DEFS_TTL: Duration = Duration::from_secs(6 * 60 * 60);

pub(super) fn is_delivery(pipeline: Option<&str>) -> bool {
    pipeline.is_some_and(|id| DELIVERY_PIPELINES.iter().any(|(own, _)| *own == id))
}
fn fallback_label(id: &str) -> Option<&'static str> {
    DELIVERY_PIPELINES
        .iter()
        .find(|(own, _)| *own == id)
        .map(|(_, label)| *label)
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct StageDef {
    pub(super) label: String,
    /// 完了（受注・失注など）のステージ。進行中の案件を先に並べるのに使う。
    pub(super) closed: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Defs {
    /// パイプライン ID → (名前, ステージ ID → ステージ)
    pub(super) pipelines: BTreeMap<String, (String, BTreeMap<String, StageDef>)>,
}
pub(super) struct CachedDefs {
    at: StdInstant,
    defs: Arc<Defs>,
}

pub(super) fn parse_defs(data: &Value) -> Defs {
    let mut defs = Defs::default();
    for pipeline in data["results"].as_array().into_iter().flatten() {
        let Some(id) = pipeline["id"].as_str() else {
            continue;
        };
        let stages = pipeline["stages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|stage| {
                Some((
                    stage["id"].as_str()?.to_owned(),
                    StageDef {
                        label: stage["label"].as_str()?.to_owned(),
                        closed: stage.pointer("/metadata/isClosed").and_then(Value::as_str)
                            == Some("true"),
                    },
                ))
            })
            .collect();
        defs.pipelines.insert(
            id.to_owned(),
            (pipeline["label"].as_str().unwrap_or(id).to_owned(), stages),
        );
    }
    defs
}

/// 定義は一度読んで使い回す。読めなかったときは空（名前は呼び名とステージ未取得で出す）。
async fn load_defs(service: &JobReadService) -> Arc<Defs> {
    let mut slot = service.listing_index.pipeline_defs.lock().await;
    if let Some(cached) = slot.as_ref() {
        if cached.at.elapsed() < DEFS_TTL {
            return cached.defs.clone();
        }
    }
    match service.request("/crm/v3/pipelines/deals", &[], None).await {
        Ok(data) => {
            let defs = Arc::new(parse_defs(&data));
            *slot = Some(CachedDefs {
                at: StdInstant::now(),
                defs: defs.clone(),
            });
            defs
        }
        Err(error) => {
            tracing::warn!(code = error.1, "job-copy pipeline definitions read failed");
            slot.as_ref()
                .map(|cached| cached.defs.clone())
                .unwrap_or_default()
        }
    }
}

#[derive(Serialize, Debug, PartialEq)]
pub(super) struct DealRow {
    pub(super) id: String,
    pub(super) name: Option<String>,
    pub(super) pipeline_id: String,
    pub(super) pipeline_label: String,
    pub(super) stage_label: Option<String>,
    /// 完了していない（進行中の）ステージか。ステージが分からないときは進行中として扱う。
    pub(super) active: bool,
    pub(super) created_at: Option<String>,
    pub(super) contract_start: Option<String>,
    pub(super) contract_end: Option<String>,
    /// 結び付いた求人。索引が未準備なら null。
    pub(super) listings_total: Option<usize>,
    pub(super) listings_live: Option<usize>,
    /// 掲載中の求人の、直近 4 週の応募と課金。週ごとの実績のある求人が無ければ null。
    pub(super) recent_apps: Option<i64>,
    pub(super) recent_cost: Option<i64>,
}

/// 進行中を先に、同じなら作成日の新しい順。
pub(super) fn sort_deals(deals: &mut [DealRow]) {
    deals.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then_with(|| b.created_at.cmp(&a.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });
}

type Counts = (Option<usize>, Option<usize>, Option<i64>, Option<i64>);
fn counts(records: Option<&[Listing]>, deal: &str) -> Counts {
    let Some(records) = records else {
        return (None, None, None, None);
    };
    let linked: Vec<&Listing> = records
        .iter()
        .filter(|row| row.deal_ids.iter().any(|id| id == deal))
        .collect();
    let live: Vec<&Listing> = linked
        .iter()
        .copied()
        .filter(|row| row.publication_status.as_deref() == Some("掲載中"))
        .collect();
    let with_perf: Vec<&home::PerfDigest> =
        live.iter().filter_map(|row| row.perf.as_ref()).collect();
    let sum = |f: fn(&home::PerfDigest) -> i64| {
        (!with_perf.is_empty()).then(|| with_perf.iter().map(|p| f(p)).sum())
    };
    (
        Some(linked.len()),
        Some(live.len()),
        sum(|p| p.recent_apps),
        sum(|p| p.recent_cost),
    )
}

fn date_part(value: Option<&str>) -> Option<String> {
    value
        .map(|v| v.chars().take(10).collect::<String>())
        .filter(|v| !v.is_empty())
}

pub(super) fn build_rows(
    records: &[Record],
    defs: &Defs,
    listings: Option<&[Listing]>,
) -> Vec<DealRow> {
    let mut rows: Vec<DealRow> = records
        .iter()
        .filter(|record| is_delivery(record.value("pipeline")))
        .filter_map(|record| {
            let pipeline = record.value("pipeline")?;
            let known = defs.pipelines.get(pipeline);
            let stage = record
                .value("dealstage")
                .and_then(|id| known.and_then(|(_, stages)| stages.get(id)));
            let (total, live, apps, cost) = counts(listings, &record.id);
            Some(DealRow {
                id: record.id.clone(),
                name: record.value("dealname").map(str::to_owned),
                pipeline_id: pipeline.to_owned(),
                pipeline_label: known
                    .map(|(label, _)| label.clone())
                    .or_else(|| fallback_label(pipeline).map(str::to_owned))
                    .unwrap_or_else(|| pipeline.to_owned()),
                stage_label: stage.map(|s| s.label.clone()),
                active: stage.is_none_or(|s| !s.closed),
                created_at: date_part(record.value("createdate")),
                contract_start: date_part(record.value("contract_start_date")),
                contract_end: date_part(record.value("contract_expiration_date")),
                listings_total: total,
                listings_live: live,
                recent_apps: apps,
                recent_cost: cost,
            })
        })
        .collect();
    sort_deals(&mut rows);
    rows
}

#[derive(Deserialize)]
pub(in crate::handlers::job_copy_live) struct CompanyDealsQuery {
    company: String,
}

/// 顧客（事業所）の案件。納品管理のパイプラインの取引だけを、結び付いた求人の数と一緒に返す。
pub(in crate::handlers::job_copy_live) async fn company_deals(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<CompanyDealsQuery>,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    let service = service(&access)?;
    valid_id(&query.company)?;
    let ids = service
        .all_associations("companies", &query.company, "deals")
        .await?;
    let records = service
        .batch(
            "deals",
            &ids,
            &[
                "dealname",
                "pipeline",
                "dealstage",
                "createdate",
                "contract_start_date",
                "contract_expiration_date",
            ],
        )
        .await?;
    let defs = load_defs(service).await;
    let snapshot = service.listing_index.state.read().await.snapshot.clone();
    let deals = build_rows(
        &records,
        &defs,
        snapshot.as_ref().map(|i| i.records.as_slice()),
    );
    let name = service
        .batch("companies", std::slice::from_ref(&query.company), &["name"])
        .await?
        .first()
        .and_then(|record| record.value("name").map(str::to_owned));
    let pipelines: Vec<Value> = DELIVERY_PIPELINES
        .iter()
        .map(|(id, label)| json!({"id": id, "label": label}))
        .collect();
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({
            "company": {"id": query.company, "name": name},
            "deals": deals,
            "delivery_pipelines": pipelines,
            "index_ready": snapshot.is_some(),
        })),
    ))
}

#[cfg(test)]
#[path = "listing_pipelines_tests.rs"]
mod tests;
