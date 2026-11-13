//! 関連を最後までたどり、読み取った原本から案を作る。HubSpotへの書き込みはしない。
use super::*;
use crate::job_gen::rikurozi::{self as draft, Comparison, Source};
use axum::extract::Path as AxumPath;
const PROPERTIES: &[&str] = &[
    "hs_name",
    "id_hrhakkaa",
    "id_shop_hrhakkaa",
    "hrh_kyuujinhyou_honbun",
    "hrh_kyuujinhyou_gazou",
    "id_airwork",
    "shigotonaiyou",
    "baitai_genjoukyou_airwork",
    "kyuujin_status",
    "todoufuken",
    "shikuchouson",
];
fn publication(value: Option<&str>, normalized: Option<&str>) -> Option<&'static str> {
    match crate::job_gen::publication::label(normalized, value, "airwork") {
        "掲載中" => Some("掲載中"),
        "掲載前" | "掲載終了" => None,
        _ => Some("掲載中か不明"),
    }
}
impl JobReadService {
    async fn rikurozi_source(&self, id: &str) -> Result<Source, ReadError> {
        valid_id(id)?;
        let record = self
            .batch("0-420", &[id.to_owned()], PROPERTIES)
            .await?
            .remove(0);
        let job_id = record.value("id_hrhakkaa").ok_or(ReadError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "hrh_source_required",
        ))?;
        let shop = record.value("id_shop_hrhakkaa").ok_or(ReadError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "hrh_shop_missing",
        ))?;
        let body = record.value("hrh_kyuujinhyou_honbun").ok_or(ReadError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "hrh_body_missing",
        ))?;
        let parsed = super::hrh_copy::fields84(body);
        if parsed.is_empty() {
            return Err(ReadError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "hrh_fields_missing",
            ));
        }
        let mut row: draft::Row = crate::job_gen::hrhacker::HRHACKER_COLUMNS
            .iter()
            .map(|c| (c.to_string(), parsed.get(*c).cloned().unwrap_or_default()))
            .collect();
        row.insert("求人id".into(), job_id.into());
        row.insert("店舗id".into(), shop.into());
        // The image property retains slots; never shift image3 into an empty image2.
        let images = super::hrh_copy::fields(record.value("hrh_kyuujinhyou_gazou").unwrap_or(""));
        for column in ["画像1", "画像2", "画像3"] {
            row.insert(
                column.into(),
                images
                    .get(column)
                    .filter(|url| url.starts_with("https://"))
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        let deals = self.all_associations("0-420", id, "deals").await?;
        let mut companies = BTreeSet::new();
        for targets in self
            .association_map("deals", &deals, "companies")
            .await?
            .into_values()
        {
            companies.extend(targets);
        }
        if companies.len() != 1 {
            return Err(ReadError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "customer_relation_unknown",
            ));
        }
        let company = companies.first().expect("one company");
        let deals = self.all_associations("companies", company, "deals").await?;
        let mut ids = BTreeSet::new();
        for targets in self
            .association_map("deals", &deals, "0-420")
            .await?
            .into_values()
        {
            ids.extend(targets);
        }
        if !ids.contains(id) {
            return Err(ReadError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "customer_relation_unknown",
            ));
        }
        // Bound the complete scope; never silently use a partial comparison set.
        if ids.len() > 2000 {
            return Err(ReadError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "comparison_scope_too_large",
            ));
        }
        let rows = self
            .batch("0-420", &ids.into_iter().collect::<Vec<_>>(), PROPERTIES)
            .await?;
        let comparisons = rows
            .iter()
            .filter(|r| r.value("id_airwork").is_some())
            .filter_map(|r| {
                Some(Comparison {
                    title: r.value("hs_name").unwrap_or("求人名未取得").into(),
                    publication: publication(
                        r.value("baitai_genjoukyou_airwork"),
                        r.value("id_hrhakkaa")
                            .is_none()
                            .then(|| r.value("kyuujin_status"))
                            .flatten(),
                    )?
                    .into(),
                    body: r.value("shigotonaiyou").unwrap_or("").into(),
                })
            })
            .collect();
        let location = [record.value("todoufuken"), record.value("shikuchouson")]
            .into_iter()
            .flatten()
            .collect::<String>();
        Ok(Source {
            row,
            body: body.into(),
            comparisons,
            location: (!location.is_empty()).then_some(location),
        })
    }
}
/// 裏で作っている案の受付。単一インスタンス前提でプロセス内にだけ持つ（再起動で消える）。
/// HubSpot には何も書かないので、消えても作り直せば足りる。
#[derive(Default)]
struct DraftJobs {
    jobs: std::collections::HashMap<String, DraftJob>,
}
struct DraftJob {
    owner: String,
    listing: String,
    started: Instant,
    outcome: Option<Result<Value, ReadError>>,
}
/// 終わった案を覚えておく時間。画面は数秒おきに確かめるので短くてよい。
const JOB_KEEP: Duration = Duration::from_secs(60 * 60);
/// 同時に裏で作る案の上限（MiniMax に同時に頼む量を抑える。1件で26項目を同時に頼む）。
const MAX_RUNNING: usize = 2;
/// 1件の作成全体の上限（HubSpot の読み取り + 案の作成）。画面は 1,020 秒まで待つ。
const BUILD_LIMIT: Duration = Duration::from_secs(960);
impl DraftJobs {
    fn prune(&mut self, now: Instant) {
        // 終わったものは1時間、作っている途中のものは作成の上限を少し過ぎたら捨てる。
        self.jobs.retain(|_, job| {
            let limit = if job.outcome.is_some() {
                JOB_KEEP
            } else {
                BUILD_LIMIT + Duration::from_secs(60)
            };
            now.saturating_duration_since(job.started) < limit
        });
    }
    /// 受け付ける。同じ人・同じ求人で作っている途中ならその受付を返す（二重に作らない）。
    fn start(
        &mut self,
        owner: &str,
        listing: &str,
        now: Instant,
    ) -> Result<(String, bool), ReadError> {
        self.prune(now);
        if let Some((id, _)) = self
            .jobs
            .iter()
            .find(|(_, j)| j.owner == owner && j.listing == listing && j.outcome.is_none())
        {
            return Ok((id.clone(), false));
        }
        if self.jobs.values().filter(|j| j.outcome.is_none()).count() >= MAX_RUNNING {
            return Err(ReadError(StatusCode::SERVICE_UNAVAILABLE, "draft_busy"));
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        self.jobs.insert(
            id.clone(),
            DraftJob {
                owner: owner.to_owned(),
                listing: listing.to_owned(),
                started: now,
                outcome: None,
            },
        );
        Ok((id, true))
    }
    fn finish(&mut self, id: &str, outcome: Result<Value, ReadError>) {
        if let Some(job) = self.jobs.get_mut(id) {
            job.outcome = Some(outcome);
        }
    }
    /// 本人の受付だけを返す。無い・他人のものは同じ「見つからない」。
    fn get(
        &mut self,
        owner: &str,
        id: &str,
        now: Instant,
    ) -> Option<Result<Option<Value>, ReadError>> {
        self.prune(now);
        let job = self.jobs.get(id).filter(|j| j.owner == owner)?;
        Some(match &job.outcome {
            None => Ok(None),
            Some(Ok(value)) => Ok(Some(value.clone())),
            Some(Err(error)) => Err(ReadError(error.0, error.1)),
        })
    }
}
static JOBS: std::sync::LazyLock<std::sync::Mutex<DraftJobs>> =
    std::sync::LazyLock::new(Default::default);
fn jobs() -> std::sync::MutexGuard<'static, DraftJobs> {
    JOBS.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 案作りを受け付けて 202 と受付番号を返す。作るのは裏で行い、画面は `draft_job` で確かめる。
pub(super) async fn generate(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    AxumPath(id): AxumPath<String>,
) -> Result<impl IntoResponse, ReadError> {
    let owner = authorized_user(&state, &access, &session).await?;
    valid_id(&id)?;
    let threshold = draft::threshold(std::env::var("RIKUROZI_OVERLAP_THRESHOLD").ok().as_deref())
        .map_err(|_| {
        ReadError(
            StatusCode::SERVICE_UNAVAILABLE,
            "draft_configuration_invalid",
        )
    })?;
    let min_chars =
        draft::min_sentence_chars(std::env::var("RIKUROZI_MIN_SENTENCE_CHARS").ok().as_deref())
            .map_err(|_| {
                ReadError(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "draft_configuration_invalid",
                )
            })?;
    let service = access.service.clone().ok_or(ReadError(
        StatusCode::SERVICE_UNAVAILABLE,
        "hubspot_not_configured",
    ))?;
    let base_url = std::env::var("MINIMAX_BASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| crate::crm::talk_review::config::DEFAULT_BASE_URL.into());
    let model = std::env::var("RIKUROZI_MODEL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| crate::crm::talk_review::config::DEFAULT_MODEL.into());
    let minimax = crate::crm::talk_review::judge::MiniMaxJudge::new(base_url.trim(), model.trim())
        .ok_or(ReadError(
            StatusCode::SERVICE_UNAVAILABLE,
            "draft_configuration_invalid",
        ))?;
    let (job_id, created) = jobs().start(&owner, &id, Instant::now())?;
    if created {
        let job = job_id.clone();
        tokio::spawn(async move {
            // 作る処理が途中で落ちても（panic）、受付を必ず失敗で閉じる。閉じないと
            // 同じ求人を作り直せず、同時に作る枠も塞いだままになる。
            let work = tokio::spawn(async move {
                tokio::time::timeout(
                    BUILD_LIMIT,
                    build(&service, &id, threshold, min_chars, &minimax),
                )
                .await
                .unwrap_or_else(|_| Err(fail("draft_generation_failed")))
            });
            let outcome = work.await.unwrap_or_else(|_| {
                tracing::warn!("rikurozi draft task ended unexpectedly");
                Err(fail("draft_generation_failed"))
            });
            jobs().finish(&job, outcome);
        });
    }
    Ok((
        StatusCode::ACCEPTED,
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({ "job_id": job_id })),
    ))
}

async fn build(
    service: &JobReadService,
    id: &str,
    threshold: f64,
    min_chars: usize,
    minimax: &crate::crm::talk_review::judge::MiniMaxJudge,
) -> Result<Value, ReadError> {
    let source = service.rikurozi_source(id).await?;
    let result = draft::run(
        &source,
        threshold,
        min_chars,
        |prompt, _schema, _temperature| async move {
            // The item prompt specifies the single text field. The shared client
            // reuses its connection and never logs the prompt or response body.
            minimax
                .complete_json(&prompt)
                .await
                .map_err(|_| anyhow::anyhow!("MiniMaxで項目を作成できませんでした"))
        },
    )
    .await
    .map_err(|error| {
        if error.downcast_ref::<draft::CopyMismatch>().is_some() {
            ReadError(StatusCode::UNPROCESSABLE_ENTITY, "draft_copy_mismatch")
        } else {
            // 理由はこちらで決めた文だけ（本文・応答は含まない）。画面には出さずログに残す。
            tracing::warn!(reason = %error, "rikurozi draft generation failed");
            fail("draft_generation_failed")
        }
    })?;
    serde_json::to_value(result).map_err(|_| fail("draft_generation_failed"))
}

/// 受付の状態。作っている間は 200 `running`、できたら 200 `done` と案、失敗は元の状態コードとコード。
pub(super) async fn draft_job(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    AxumPath(job_id): AxumPath<String>,
) -> Result<impl IntoResponse, ReadError> {
    let owner = authorized_user(&state, &access, &session).await?;
    let state = jobs()
        .get(&owner, &job_id, Instant::now())
        .ok_or(ReadError(StatusCode::NOT_FOUND, "draft_job_not_found"))??;
    let body = match state {
        None => json!({ "status": "running" }),
        Some(draft) => json!({ "status": "done", "draft": draft }),
    };
    Ok(([(header::CACHE_CONTROL, "private, no-store")], Json(body)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn draft_jobs_dedupe_limit_owner_and_expiry() {
        let mut jobs = DraftJobs::default();
        let now = Instant::now();
        let (a, created) = jobs.start("a@x", "1", now).unwrap();
        assert!(created);
        // 同じ人・同じ求人の作成中は同じ受付を返す。
        assert_eq!(jobs.start("a@x", "1", now).unwrap(), (a.clone(), false));
        // 他人には見えない。
        assert!(jobs.get("b@x", &a, now).is_none());
        assert!(matches!(jobs.get("a@x", &a, now), Some(Ok(None))));
        jobs.start("a@x", "2", now).unwrap();
        let busy = jobs.start("b@x", "5", now).unwrap_err();
        assert_eq!(
            (busy.0, busy.1),
            (StatusCode::SERVICE_UNAVAILABLE, "draft_busy")
        );
        jobs.finish(&a, Ok(json!({"csv":"x"})));
        assert_eq!(
            jobs.get("a@x", &a, now).unwrap().unwrap(),
            Some(json!({"csv":"x"}))
        );
        // 終わった分は数えないので次を受け付ける。終わったあとは同じ求人でも新しく作る。
        let (b, created) = jobs.start("a@x", "1", now).unwrap();
        assert!(created && b != a);
        jobs.finish(
            &b,
            Err(ReadError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "draft_copy_mismatch",
            )),
        );
        let err = jobs.get("a@x", &b, now).unwrap().unwrap_err();
        assert_eq!(
            (err.0, err.1),
            (StatusCode::UNPROCESSABLE_ENTITY, "draft_copy_mismatch")
        );
        // 1時間を過ぎたら消える。
        assert!(jobs
            .get("a@x", &a, now + Duration::from_secs(3601))
            .is_none());
    }
    #[test]
    fn publication_uses_only_media_status_and_includes_unknown() {
        let cases: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/rikurozi_publication.json"
        ))
        .unwrap();
        for case in cases.as_array().unwrap() {
            let normalized = case["air_only"]
                .as_bool()
                .unwrap()
                .then(|| case["normalized"].as_str())
                .flatten();
            assert_eq!(
                publication(case["raw"].as_str(), normalized),
                case["expected"].as_str()
            );
        }
        for value in [Some("公開"), Some("掲載中")] {
            assert_eq!(publication(value, None), Some("掲載中"));
        }
        for value in [
            Some("非公開"),
            Some("公開終了"),
            Some("掲載終了"),
            Some("公開開始前"),
        ] {
            assert_eq!(publication(value, None), None);
        }
        for value in [None, Some(""), Some("不明"), Some("active"), Some("募集中")] {
            assert_eq!(publication(value, None), Some("掲載中か不明"));
        }
    }
    #[tokio::test]
    async fn collects_all_company_deals_deduplicates_and_paginates_without_other_customer() {
        use axum::{
            body::{to_bytes, Body},
            http::Request,
            routing::any,
        };
        let app = Router::new().fallback(any(|req: Request<Body>| async move {
            let path = req.uri().path().to_owned(); let query = req.uri().query().unwrap_or("").to_owned();
            let body = to_bytes(req.into_body(), 100000).await.unwrap();
            let body: Value = if body.is_empty() { Value::Null } else { serde_json::from_slice(&body).unwrap() };
            let result = match path.as_str() {
                "/crm/v4/objects/0-420/1/associations/deals" => json!({"results":[{"toObjectId":10}]}),
                "/crm/v4/associations/deals/companies/batch/read" => json!({"results":[{"from":{"id":"10"},"to":[{"toObjectId":100}]}]}),
                "/crm/v4/objects/companies/100/associations/deals" if !query.contains("after=") => json!({"results":[{"toObjectId":10}],"paging":{"next":{"after":"10"}}}),
                "/crm/v4/objects/companies/100/associations/deals" => json!({"results":[{"toObjectId":20}]}),
                "/crm/v4/associations/deals/0-420/batch/read" => {
                    assert_eq!(body["inputs"],json!([{"id":"10"},{"id":"20"}]));
                    json!({"results":[{"from":{"id":"10"},"to":[{"toObjectId":1},{"toObjectId":2}]},{"from":{"id":"20"},"to":[{"toObjectId":2},{"toObjectId":3},{"toObjectId":4},{"toObjectId":5}]}]})
                },
                "/crm/v3/objects/0-420/batch/read" => {
                    assert!(body["properties"].as_array().unwrap().iter().any(|p| p == "kyuujin_status"));
                    let ids: Vec<_> = body["inputs"].as_array().unwrap().iter().map(|v| v["id"].as_str().unwrap()).collect();
                    assert!(!ids.contains(&"99")); // Other company is never requested.
                    let rows: Vec<_> = ids.iter().map(|id| {
                        let properties = match *id {
                            "1" => json!({"id_hrhakkaa":"1234567","id_shop_hrhakkaa":"4081","hrh_kyuujinhyou_honbun":"案件名：基準の求人\n仕事内容：品物を集めます\n雇用形態：パート\n給与形態：時給\n基本給与 最小：1200\n勤務時間：9:00〜18:00\n自由項目1の内容：土日休み", "hrh_kyuujinhyou_gazou":"画像1：https://example.invalid/one.jpg\n画像2：なし\n画像3：https://example.invalid/three.png"}),
                            "2" => json!({"hs_name":"公開の求人","id_airwork":"77","shigotonaiyou":"棚卸しします","baitai_genjoukyou_airwork":"01","kyuujin_status":"公開中"}),
                            "3" => json!({"hs_name":"公開状況不明の求人","id_airwork":"77","shigotonaiyou":"箱を運びます"}),
                            "4" => json!({"hs_name":"掲載終了の求人","id_airwork":"78","baitai_genjoukyou_airwork":"02","kyuujin_status":"公開終了"}),
                            "5" => json!({"hs_name":"別のHRハッカー","id_hrhakkaa":"98765"}),
                            _ => panic!("unexpected id"),
                        }; json!({"id":id,"properties":properties})
                    }).collect(); json!({"results":rows})
                },
                _ => panic!("unexpected path {path}"),
            };
            Json(result)
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let service =
            JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let source = service.rikurozi_source("1").await.unwrap();
        assert_eq!(source.row["求人id"], "1234567");
        assert_eq!(source.row["店舗id"], "4081");
        assert_eq!(source.row["画像1"], "https://example.invalid/one.jpg");
        assert_eq!(source.row["画像2"], "");
        assert_eq!(source.row["画像3"], "https://example.invalid/three.png");
        assert_eq!(source.comparisons.len(), 2);
        assert_eq!(source.comparisons[0].title, "公開の求人");
        assert_eq!(source.comparisons[1].publication, "掲載中か不明");
        assert_eq!(source.comparisons[1].body, "箱を運びます");
        task.abort();
    }
}
