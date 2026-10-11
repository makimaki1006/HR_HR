use super::{
    grants, tracking, EmailError, EmailOperation, EmailResult, GmailGrantStatus, SendEmailRequest,
};
use crate::crm::{
    pending, rbac, record_lock,
    routes::{error_json, hubspot_error_response, CrmCtx},
};
use crate::{hubspot::RecordType, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use std::{collections::HashSet, sync::Arc};
use tower_sessions::Session;

pub(in crate::crm) async fn grant_status(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
) -> Response {
    let p = match rbac::authorize(&session, &state, &ctx.access, None).await {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    if ctx.write.email.is_none() {
        return Json(GmailGrantStatus {
            enabled: false,
            granted: false,
        })
        .into_response();
    }
    let Some(audit) = &state.audit else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "queue_unavailable");
    };
    let who = p.email.unwrap_or_default();
    match pending::blocking(audit, move |t| grants::get(t, &who)).await {
        Ok(Ok(t)) => Json(GmailGrantStatus {
            enabled: state.google_oidc.is_some(),
            granted: t.is_some(),
        })
        .into_response(),
        _ => error_json(StatusCode::SERVICE_UNAVAILABLE, "queue_unavailable"),
    }
}
fn error(e: EmailError) -> Response {
    error_json(
        StatusCode::from_u16(e.status()).unwrap_or(StatusCode::SERVICE_UNAVAILABLE),
        e.code(),
    )
}
fn result(row: &pending::OpRow) -> Response {
    let r = EmailResult::from_row(row);
    let status = match r.status.as_str() {
        "saved" => StatusCode::OK,
        "pending" => StatusCode::ACCEPTED,
        _ => {
            StatusCode::from_u16(row.http_status.max(400) as u16).unwrap_or(StatusCode::BAD_GATEWAY)
        }
    };
    (status, Json(r)).into_response()
}

pub(in crate::crm) async fn get_email(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    Path(id): Path<String>,
) -> Response {
    let p = match rbac::authorize(&session, &state, &ctx.access, None).await {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let Some(audit) = &state.audit else {
        return error(EmailError::Storage);
    };
    match pending::blocking(audit, move |t| pending::get_op(t, &id)).await {
        Ok(Ok(Some(r)))
            if r.payload.email.is_some() && r.operator_email == p.email.unwrap_or_default() =>
        {
            Json(EmailResult::from_row(&r)).into_response()
        }
        Ok(Ok(_)) => error_json(StatusCode::NOT_FOUND, "not_found"),
        _ => error(EmailError::Storage),
    }
}
pub(in crate::crm) async fn send_email(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    Json(req): Json<SendEmailRequest>,
) -> Response {
    let p = match rbac::authorize(&session, &state, &ctx.access, Some(RecordType::Deal)).await {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let Some(service) = ctx.write.email.clone() else {
        return error_json(StatusCode::FORBIDDEN, "email_disabled");
    };
    let Some(audit) = state.audit.clone() else {
        return error(EmailError::Storage);
    };
    let Some(client) = state.hubspot.clone() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    let operator = p.email.unwrap_or_default();
    if let Err(e) = super::validate(&req) {
        return error(e);
    }
    let Ok(guard) = service
        .locks
        .acquire(
            [record_lock::key("emails", &req.operation_id)],
            ctx.write.lock_wait,
        )
        .await
    else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "record_busy");
    };
    // Primary key plus this lock protects parallel requests; persisted tombstones protect restarts.
    let id = req.operation_id.clone();
    match pending::blocking(&audit, move |t| pending::get_op(t, &id)).await {
        Ok(Ok(Some(row))) => {
            return if row.operator_email == operator
                && row.deal_id == req.hubspot_deal_id
                && row.payload.email.is_some()
            {
                result(&row)
            } else {
                error_json(StatusCode::CONFLICT, "operation_id_in_use")
            }
        }
        Ok(Ok(None)) => {}
        _ => return error(EmailError::Storage),
    }
    if !ctx.write_rate.allow(&operator) {
        return error_json(StatusCode::TOO_MANY_REQUESTS, "rate_limited");
    }
    let max = ctx.write.pending_max;
    match pending::blocking(&audit, pending::count_active).await {
        Ok(Ok(n)) if n < max => {}
        Ok(Ok(_)) => return error_json(StatusCode::SERVICE_UNAVAILABLE, "queue_full"),
        _ => return error(EmailError::Storage),
    }
    // Check consent before any HubSpot read, then resolve a usable access token before reserving a send.
    let refresh = match service.refresh_token(&audit, &operator).await {
        Ok(t) => t,
        Err(e) => return error(e),
    };
    let access = match service.access_token(&refresh).await {
        Ok(t) => t,
        Err(e) => return error(e),
    };
    let (deal, mut assoc) = match client
        .get_object_with_associations("deals", &req.hubspot_deal_id, &[], &["contacts"])
        .await
    {
        Ok(d) => d,
        Err(e) => return hubspot_error_response(&e),
    };
    if deal.archived {
        return error(EmailError::Recipient);
    }
    let refs = assoc.remove("contacts").map(|(r, _)| r).unwrap_or_default();
    let ids: Vec<String> = refs.into_iter().map(|r| r.id).take(100).collect();
    if !ids.contains(&req.contact_id) {
        return error(EmailError::Recipient);
    }
    let contacts = match client.batch_read("contacts", &ids, &["email"]).await {
        Ok(c) => c,
        Err(e) => return hubspot_error_response(&e),
    };
    let to = req.to.to_lowercase();
    if !contacts.iter().any(|c| {
        !c.archived
            && c.id == req.contact_id
            && c.properties
                .get("email")
                .and_then(|v| v.as_deref())
                .is_some_and(|v| v.eq_ignore_ascii_case(&to))
    }) {
        return error(EmailError::Recipient);
    }
    let cc: Vec<String> = req
        .cc
        .iter()
        .map(|e| e.to_lowercase())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let mut contact_ids = vec![req.contact_id.clone()];
    for addr in &cc {
        let Some(contact) = contacts.iter().find(|c| {
            !c.archived
                && c.properties
                    .get("email")
                    .and_then(|v| v.as_deref())
                    .is_some_and(|v| v.eq_ignore_ascii_case(addr))
        }) else {
            return error(EmailError::Recipient);
        };
        contact_ids.push(contact.id.clone());
    }
    contact_ids.sort();
    contact_ids.dedup();
    let owner = match ctx.queue.owner_for(&client, &operator).await {
        Ok(Some(o)) => o,
        Ok(None) => return error_json(StatusCode::FORBIDDEN, "owner_not_found"),
        Err(e) => return hubspot_error_response(&e),
    };
    let tracked = match tracking::instrument_html(
        &audit,
        &service.cfg.base_url,
        &req.html,
        tracking::TrackingMeta {
            operator_email: operator.clone(),
            hubspot_deal_id: req.hubspot_deal_id.clone(),
            hubspot_contact_id: req.contact_id.clone(),
            subject: req.subject.clone(),
        },
    )
    .await
    {
        Ok(t) => t,
        Err(_) => return error(EmailError::Tracking),
    };
    let email = EmailOperation {
        contact_id: req.contact_id.clone(),
        contact_ids,
        to,
        cc,
        subject: req.subject,
        html: tracked.html,
        owner_id: owner,
        tracking_id: tracked.tracking_id,
        requested_message_id: format!(
            "<{}.{}@{}>",
            req.operation_id,
            uuid::Uuid::new_v4(),
            service.cfg.google.hosted_domain
        ),
        rfc_message_id: None,
        gmail: None,
        hubspot_email_id: None,
        sent_at: pending::now_iso(),
        send_state: "sending".into(),
        create_uncertain: false,
    };
    let payload = pending::Payload {
        steps: vec![],
        email: Some(email.clone()),
    };
    let (op, who, did, pl) = (
        req.operation_id.clone(),
        operator.clone(),
        req.hubspot_deal_id.clone(),
        payload.clone(),
    );
    if !matches!(
        pending::blocking(&audit, move |t| pending::insert_op(
            t, &op, &who, &did, &pl, "emails"
        ))
        .await,
        Ok(Ok(()))
    ) {
        return error(EmailError::Storage);
    }
    let mut row = pending::OpRow {
        operation_id: req.operation_id,
        operator_email: operator,
        deal_id: req.hubspot_deal_id,
        payload,
        status: "in_progress".into(),
        attempts: 0,
        last_error_code: String::new(),
        http_status: 202,
        result_json: String::new(),
        next_retry_at: pending::iso(chrono::Utc::now() + chrono::Duration::seconds(90)),
        created_at: pending::now_iso(),
        updated_at: pending::now_iso(),
    };
    let cache_ctx = ctx.clone();
    // Detached after reservation: a browser disconnect cannot cancel recording a successful send.
    let task = tokio::spawn(async move {
        let _held = guard;
        match service
            .send_gmail(&access, &row.operator_email, &email)
            .await
        {
            Ok(msg) => {
                let e = row.payload.email.as_mut().unwrap();
                e.gmail = Some(msg);
                e.send_state = "sent".into();
                if super::persist(&audit, &row).await.is_err() {
                    super::write_wake();
                    return error(EmailError::Storage);
                }
                if super::record(&audit, &client, &service, &mut row)
                    .await
                    .is_err()
                {
                    super::write_wake();
                    return error(EmailError::Storage);
                }
            }
            Err(e) => {
                if e == EmailError::GmailRejected {
                    row.payload.email.as_mut().unwrap().send_state = "rejected".into();
                    super::finish(&mut row, "failed", e.code());
                } else {
                    super::finish(&mut row, "pending", e.code());
                }
                if super::persist(&audit, &row).await.is_err() {
                    return error(EmailError::Storage);
                }
            }
        }
        cache_ctx.workspace_cache.invalidate_deal(&row.deal_id);
        super::write_wake();
        result(&row)
    });
    task.await.unwrap_or_else(|_| error(EmailError::Storage))
}

#[derive(serde::Deserialize)]
pub(in crate::crm) struct ListQuery {
    deal_id: String,
}
pub(in crate::crm) async fn list_emails(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    axum::extract::Query(q): axum::extract::Query<ListQuery>,
) -> Response {
    let p = match rbac::authorize(&session, &state, &ctx.access, None).await {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    if ctx.write.email.is_none() {
        return Json(Vec::<super::PendingEmailActivity>::new()).into_response();
    }
    if !crate::crm::routes::is_valid_id(&q.deal_id) {
        return error(EmailError::Invalid);
    }
    let Some(audit) = &state.audit else {
        return error(EmailError::Storage);
    };
    let who = p.email.unwrap_or_default();
    match pending::blocking(audit, move |t| pending::list_email_ops(t, &who, &q.deal_id)).await {
        Ok(Ok(rows)) => Json(
            rows.into_iter()
                .filter_map(|row| {
                    let e = row.payload.email.as_ref()?;
                    Some(super::PendingEmailActivity {
                        result: EmailResult::from_row(&row),
                        subject: e.subject.clone(),
                        to: e.to.clone(),
                        cc: e.cc.clone(),
                        sent_at: e.sent_at.clone(),
                    })
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        _ => error(EmailError::Storage),
    }
}
