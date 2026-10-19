//! 既存の偽 HubSpot・SQLite 偽 Turso を通す。実サービスには接続しない。
use super::*;
fn fake_applicant() -> Fake {
    let mut f = base_fake();
    f.objects.insert(
        ("0-421".into(), DEAL.into()),
        BTreeMap::from([
            ("hs_pipeline".into(), "app-pipeline".into()),
            ("hs_pipeline_stage".into(), "1343099551".into()),
            ("renrakukakuninmokuteki".into(), "10/18 旧メモ".into()),
        ]),
    );
    f
}
fn body(op: &str) -> Value {
    json!({"operation_id":op,"base":{"hs_pipeline":"app-pipeline","hs_pipeline_stage":"1343099551","renrakukakuninmokuteki":"10/18 旧メモ"},"set":{"renrakukakuninmokuteki":"10/18 旧メモ\n10/19 希望を確認"},"stage":{"pipeline_id":"app-pipeline","stage_id":"1431574804"}})
}
async fn patch(e: &Env, body: Value) -> (StatusCode, Value) {
    call(
        &e.app,
        Method::PATCH,
        &format!("/api/crm/applicants/{DEAL}"),
        Some(&e.op),
        Some(body),
        true,
    )
    .await
}
#[tokio::test]
async fn applicant_patch_atomic_body_replay_audit_and_cache_invalidation() {
    let e = env_with(fake_applicant(), open(), true).await;
    let detail_url = format!("/api/crm/applicants/{DEAL}");
    let (status, before) = call(&e.app, Method::GET, &detail_url, Some(&e.op), None, false).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(before["values"]["hs_pipeline_stage"], "1343099551");
    assert_eq!(before["writes_enabled"], true);
    assert!(!before["editable"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["name"] == "meeruadoresu" || p["name"] == "shuugyoujoukyou"));
    let url = "/api/crm/applicants/queue";
    assert_eq!(
        call(&e.app, Method::GET, url, Some(&e.op), None, false)
            .await
            .0,
        StatusCode::OK
    );
    let (status, saved) = patch(&e, body("app-save-0001")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["values"]["hs_pipeline_stage"], "1431574804");
    assert_eq!(
        e.hs.lock().unwrap().patches(),
        vec![
            json!({"properties":{"hs_pipeline_stage":"1431574804","renrakukakuninmokuteki":"10/18 旧メモ\n10/19 希望を確認"}})
        ]
    );
    assert_eq!(patch(&e, body("app-save-0001")).await.0, StatusCode::OK);
    assert_eq!(e.calls("PATCH /crm/v3/objects/0-421/"), 1);
    assert_eq!(e.calls("GET /crm/v3/objects/0-421/"), 1);
    assert_eq!(e.ledger_count(), 1);
    assert!(e
        .audit_metas()
        .iter()
        .any(|m| m.to_string().contains("app-save-0001")));
    let (_, after) = call(&e.app, Method::GET, &detail_url, Some(&e.op), None, false).await;
    assert_eq!(after["values"]["hs_pipeline_stage"], "1431574804");
    call(&e.app, Method::GET, url, Some(&e.op), None, false).await;
    assert_eq!(e.calls("POST /crm/v3/objects/0-421/search"), 4);
    assert_eq!(e.calls("POST /crm/v3/objects/0-421/batch/read"), 2);
    assert_eq!(e.client.applicant_write_revision(), 1);
}
#[tokio::test]
async fn applicant_conflict_does_not_overwrite_and_cross_pipeline_is_rejected() {
    let e = env_with(fake_applicant(), open(), true).await;
    let mut req = body("app-conflict-0001");
    req["base"]["renrakukakuninmokuteki"] = json!("古い内容");
    let (status, v) = patch(&e, req).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(v["current"]["renrakukakuninmokuteki"], "10/18 旧メモ");
    assert_eq!(v["changed_by_hubspot"], json!(["renrakukakuninmokuteki"]));
    e.hs.lock()
        .unwrap()
        .objects
        .get_mut(&("0-421".into(), DEAL.into()))
        .unwrap()
        .insert("hs_pipeline".into(), "other-pipeline".into());
    let (status, v) = patch(&e, body("app-conflict-0002")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(v["current"]["hs_pipeline"], "other-pipeline");
    assert_eq!(e.calls("PATCH "), 0);
}
#[tokio::test]
async fn applicant_transient_queues_and_worker_sends_only_to_applicant() {
    let mut fake = fake_applicant();
    fake.patch_fail = vec![503];
    let e = env_with(fake, open(), true).await;
    let (status, v) = patch(&e, body("app-retry-0001")).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(v["operation_id"], "app-retry-0001");
    let row = super::super::pending::blocking(&e.audit, |t| {
        super::super::pending::get_op(t, "app-retry-0001")
    })
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    assert_eq!(row.status, "pending");
    assert_eq!(row.payload.steps[0].object, "0-421");
    assert_eq!(row.payload.steps.len(), 1);
    assert!(write::process_op(&e.audit, &e.client, &e.write, &row).await);
    assert_eq!(e.ledger("app-retry-0001").0, "saved");
    assert_eq!(e.calls("PATCH /crm/v3/objects/0-421/"), 2);
    assert_eq!(e.calls("PATCH /crm/v3/objects/deals/"), 0);
    assert_eq!(e.calls("PATCH /crm/v3/objects/contacts/"), 0);
    let (_, state) = call(
        &e.app,
        Method::GET,
        "/api/crm/operations/app-retry-0001",
        Some(&e.op),
        None,
        false,
    )
    .await;
    assert_eq!(state["status"], "saved");
    assert_eq!(e.client.applicant_write_revision(), 2);
}
#[tokio::test]
async fn applicant_rejects_readonly_legacy_unknown_values_and_outside_stages() {
    let e = env_with(fake_applicant(), open(), true).await;
    for name in ["meeruadoresu", "shuugyoujoukyou", "unknown"] {
        let mut req = body(&format!("app-invalid-{name}"));
        req["set"][name] = json!("入力");
        req["base"][name] = Value::Null;
        assert_eq!(patch(&e, req).await.0, StatusCode::BAD_REQUEST, "{name}");
    }
    for (key, value) in [("stage_id", "outside"), ("pipeline_id", "outside")] {
        let mut req = body(&format!("app-invalid-{key}"));
        req["stage"][key] = json!(value);
        assert_eq!(patch(&e, req).await.0, StatusCode::BAD_REQUEST);
    }
    let mut req = body("app-invalid-choice");
    req["set"]["taioushubetsu"] = json!("outside");
    req["base"]["taioushubetsu"] = Value::Null;
    assert_eq!(patch(&e, req).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(e.calls("PATCH "), 0);
    assert_eq!(e.ledger_count(), 0);
}
#[tokio::test]
async fn applicant_scope_off_csrf_and_operation_target_boundary() {
    let e = env_with(fake_applicant(), WriteConfig::default(), true).await;
    assert_eq!(
        patch(&e, body("app-disabled-0001")).await,
        (
            StatusCode::FORBIDDEN,
            json!({"error":"writes_disabled","error_kind":"writes_disabled"})
        )
    );
    assert_eq!(e.hs.lock().unwrap().log.len(), 0);
    assert_eq!(e.ledger_count(), 0);
    let mut f = fake_applicant();
    f.patch_fail = vec![403];
    let e = env_with(f, open(), true).await;
    let (_, v) = patch(&e, body("app-forbidden-0001")).await;
    assert_eq!(
        v["message"],
        "HubSpot の鍵に応募を書き込む権限がありません。何も保存されていません。"
    );
    assert_eq!(e.ledger("app-forbidden-0001").0, "failed");
    assert_eq!(
        call(
            &e.app,
            Method::PATCH,
            &format!("/api/crm/applicants/{DEAL}"),
            Some(&e.op),
            Some(body("app-no-csrf-0001")),
            false
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(patch(&e, body("app-target-0001")).await.0, StatusCode::OK);
    assert_eq!(e.patch(json!({"operation_id":"app-target-0001","base":{"bpo_50":"旧メモ"},"set":{"bpo_50":"次"}})).await.0,StatusCode::UNPROCESSABLE_ENTITY);
}
