//! Bounded, resumable facts reads; checkpoints use the existing report snapshot only.
use super::super::todo_signals::Facts;
use super::*;

const TIME_LIMIT: Duration = Duration::from_secs(10 * 60);
const DEALS_PER_CHUNK: usize = 50;

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Progress {
    started_at: chrono::DateTime<Utc>,
    completed: BTreeSet<String>,
    facts: BTreeMap<String, Facts>,
}
#[derive(Serialize, Deserialize)]
struct Complete {
    built_at: chrono::DateTime<Utc>,
    facts: BTreeMap<String, Facts>,
}
#[derive(Serialize, Deserialize)]
struct Checkpoint {
    format: u8,
    complete: Option<Complete>,
    pending: Option<Progress>,
}
// Existing completed snapshots remain readable after deploying this change.
#[derive(Deserialize)]
#[serde(untagged)]
enum Stored {
    Checkpoint(Checkpoint),
    Legacy(BTreeMap<String, Facts>),
}
pub(super) async fn restore(service: &JobReadService) {
    let Some(saved) =
        report_snapshot::restore::<Stored>(service.audit.clone(), report_snapshot::Kind::Facts)
            .await
    else {
        return;
    };
    let checkpoint = match saved.data {
        Stored::Checkpoint(c) => c,
        Stored::Legacy(facts) => Checkpoint {
            format: 1,
            complete: Some(Complete {
                built_at: saved.built_at,
                facts,
            }),
            pending: None,
        },
    };
    if checkpoint.format != 1 {
        return;
    }
    let mut state = service.owner_todo.state.lock().await;
    if state.facts_snapshot.is_none() {
        if let Some(complete) = checkpoint.complete {
            if let Some((_, snapshot)) = state.snapshot.as_mut() {
                Arc::make_mut(snapshot).facts = complete.facts.clone();
            }
            state.facts_snapshot = Some((
                report_snapshot::instant(complete.built_at, FACTS_TTL),
                complete.facts,
            ));
            state.facts_built_at = Some(complete.built_at);
        }
        state.facts_pending = checkpoint.pending;
    }
}
async fn persist(service: &JobReadService) {
    let checkpoint = {
        let state = service.owner_todo.state.lock().await;
        Checkpoint {
            format: 1,
            complete: state.facts_snapshot.as_ref().zip(state.facts_built_at).map(
                |((_, facts), built_at)| Complete {
                    facts: facts.clone(),
                    built_at,
                },
            ),
            pending: state.facts_pending.clone(),
        }
    };
    report_snapshot::persist(
        service.audit.clone(),
        report_snapshot::Kind::Facts,
        Arc::new(checkpoint),
        Utc::now(),
    )
    .await;
}
pub(super) async fn run(service: &JobReadService, deals: &BTreeMap<String, WorkDeal>) {
    refresh_with_limit(service, deals, TIME_LIMIT).await;
}
async fn refresh_with_limit(
    service: &JobReadService,
    deals: &BTreeMap<String, WorkDeal>,
    limit: Duration,
) {
    let started = Instant::now();
    let result = run_with_limit(service, deals, limit).await;
    let mut state = service.owner_todo.state.lock().await;
    if let Err(error) = result {
        state.facts_failed_at = Some(Instant::now());
        tracing::warn!(
            reason = error.1,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "job-copy todo applications refresh stopped"
        );
    }
    state.facts_refreshing = false;
}
// The deadline includes gateway waits and retries. Persistence is awaited outside the
// cancellable read, so a detached blocking save cannot overwrite a later generation.
async fn run_with_limit(
    service: &JobReadService,
    deals: &BTreeMap<String, WorkDeal>,
    limit: Duration,
) -> Result<(), ReadError> {
    let deadline = tokio::time::Instant::now() + limit;
    let started = Instant::now();
    let mut progress = service
        .owner_todo
        .state
        .lock()
        .await
        .facts_pending
        .clone()
        .filter(|p| {
            Utc::now()
                .signed_duration_since(p.started_at)
                .to_std()
                .is_ok_and(|age| age < FACTS_TTL)
        })
        .unwrap_or_else(|| Progress {
            started_at: Utc::now(),
            completed: BTreeSet::new(),
            facts: BTreeMap::new(),
        });
    let ids: Vec<_> = deals
        .keys()
        .filter(|id| !progress.completed.contains(*id))
        .cloned()
        .collect();
    tracing::info!(
        count = deals.len(),
        completed = progress.completed.len(),
        remaining = ids.len(),
        "job-copy todo facts refresh started"
    );
    for chunk in ids.chunks(DEALS_PER_CHUNK) {
        let batch = chunk
            .iter()
            .map(|id| (id.clone(), deals[id].clone()))
            .collect();
        let facts =
            tokio::time::timeout_at(deadline, super::super::todo_signals::fetch(service, &batch))
                .await
                .map_err(|_| ReadError(StatusCode::GATEWAY_TIMEOUT, "todo_facts_timeout"))??;
        progress.facts.extend(facts);
        progress.completed.extend(chunk.iter().cloned());
        service.owner_todo.state.lock().await.facts_pending = Some(progress.clone());
        persist(service).await;
        tracing::info!(
            completed = progress.completed.len(),
            count = deals.len(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "job-copy todo facts checkpoint"
        );
    }
    progress.facts.retain(|id, _| deals.contains_key(id));
    {
        let mut state = service.owner_todo.state.lock().await;
        if let Some((_, snapshot)) = state.snapshot.as_mut() {
            Arc::make_mut(snapshot).facts = progress.facts.clone();
        }
        state.facts_snapshot = Some((Instant::now(), progress.facts));
        state.facts_built_at = Some(Utc::now());
        state.facts_pending = None;
        state.facts_failed_at = None;
    }
    persist(service).await;
    tracing::info!(
        count = deals.len(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "job-copy todo facts refresh completed"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    fn deals(count: usize) -> BTreeMap<String, WorkDeal> {
        (1..=count)
            .map(|id| {
                let record = Record {
                    id: id.to_string(),
                    properties: BTreeMap::from([
                        ("contract_start_date".into(), Some("2026-01-01".into())),
                        ("pipeline".into(), Some("21596025".into())),
                    ]),
                };
                (
                    record.id.clone(),
                    WorkDeal {
                        assignment: Assignment::from_record(&record),
                        record,
                    },
                )
            })
            .collect()
    }
    async fn seed(service: &JobReadService, deals: BTreeMap<String, WorkDeal>) {
        service.owner_todo.state.lock().await.snapshot = Some((
            Instant::now(),
            Arc::new(Snapshot {
                complete: true,
                deals,
                ..Snapshot::default()
            }),
        ));
    }
    #[tokio::test(flavor = "multi_thread")]
    async fn deadline_stops_preparing_and_restart_resumes_saved_chunks() {
        let (audit, _) = crate::audit::fake_turso::start_sqlite_audit().await;
        let blocked = Arc::new(AtomicBool::new(true));
        let completed_reads = Arc::new(AtomicUsize::new(0));
        let (block, reads) = (blocked.clone(), completed_reads.clone());
        let app = Router::new().fallback(axum::routing::post(move |Json(body): Json<Value>| {
            let (block, reads) = (block.clone(), reads.clone());
            async move {
                let inputs = body["inputs"].as_array().unwrap();
                if inputs.len() == 1 && block.load(Ordering::SeqCst) {
                    std::future::pending::<()>().await;
                }
                if inputs.len() == DEALS_PER_CHUNK { reads.fetch_add(1, Ordering::SeqCst); }
                Json(json!({"results": inputs.iter().map(|v| json!({"from":{"id":v["id"]},"to":[]})).collect::<Vec<_>>()}))
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut service = JobReadService::for_test(base.clone());
        service.audit = Some(audit.clone());
        // BTreeMap order is lexical; 51 deals still form one 50-deal chunk and one remainder.
        let deals = deals(51);
        seed(&service, deals.clone()).await;
        service.owner_todo.state.lock().await.facts_refreshing = true;
        tokio::time::timeout(
            Duration::from_secs(2),
            refresh_with_limit(&service, &deals, Duration::from_millis(250)),
        )
        .await
        .unwrap();
        let view = work_view(&service).await;
        assert!(view.failed);
        assert!(view.partial);
        assert!(!view.refreshing);
        assert_eq!(completed_reads.load(Ordering::SeqCst), 1);
        assert_eq!(
            service
                .owner_todo
                .state
                .lock()
                .await
                .facts_pending
                .as_ref()
                .unwrap()
                .completed
                .len(),
            50
        );

        let mut restarted = JobReadService::for_test(base);
        restarted.audit = Some(audit);
        seed(&restarted, deals.clone()).await;
        restore(&restarted).await;
        assert_eq!(
            restarted
                .owner_todo
                .state
                .lock()
                .await
                .facts_pending
                .as_ref()
                .unwrap()
                .completed
                .len(),
            50
        );
        blocked.store(false, Ordering::SeqCst);
        refresh_with_limit(&restarted, &deals, Duration::from_secs(2)).await;
        assert_eq!(
            completed_reads.load(Ordering::SeqCst),
            1,
            "completed chunk must not be read again"
        );
        let view = work_view(&restarted).await;
        assert!(!view.partial);
        assert!(!view.failed);
        assert_eq!(view.snapshot.unwrap().facts.len(), 51);
        server.abort();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn saved_complete_is_returned_while_refresh_is_blocked_or_times_out() {
        let (audit, _) = crate::audit::fake_turso::start_sqlite_audit().await;
        let facts = BTreeMap::from([(
            "1".into(),
            Facts {
                live: 7,
                ..Facts::default()
            },
        )]);
        let checkpoint = Checkpoint {
            format: 1,
            complete: Some(Complete {
                built_at: Utc::now() - chrono::Duration::hours(2),
                facts: facts.clone(),
            }),
            pending: None,
        };
        report_snapshot::persist(
            Some(audit.clone()),
            report_snapshot::Kind::Facts,
            Arc::new(checkpoint),
            Utc::now(),
        )
        .await;
        let mut service = JobReadService::for_test("http://127.0.0.1:1".into());
        service.audit = Some(audit);
        seed(&service, deals(1)).await;
        // Prevent an unrelated new worker: a refresh is already in progress.
        service.owner_todo.state.lock().await.facts_refreshing = true;
        let view = tokio::time::timeout(Duration::from_millis(500), work_view(&service))
            .await
            .unwrap();
        assert!(!view.partial);
        assert!(view.refreshing);
        assert!(view.facts_stale);
        assert_eq!(view.snapshot.unwrap().facts, facts);
        refresh_with_limit(&service, &deals(1), Duration::ZERO).await;
        let view = work_view(&service).await;
        assert!(!view.partial);
        assert!(view.failed);
        assert!(!view.refreshing);
        assert_eq!(view.snapshot.unwrap().facts, facts);
    }
}
