//! Bounded retries for background reads only; every attempt still goes through the gateway.
use super::*;
use crate::hubspot::{HubSpotClient, HubSpotError};

pub(super) const DELAYS: [Duration; 3] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(15),
];
fn transient(error: &HubSpotError) -> bool {
    matches!(
        error,
        HubSpotError::Timeout
            | HubSpotError::Transport(_)
            | HubSpotError::Busy
            | HubSpotError::RateLimited
            | HubSpotError::Upstream { status: 500..=599 }
    )
}

/// One build shares its send spacing across pages, associations, names and retries.
/// The gateway remains the authority for shared traffic/priority/429 pauses. A small
/// 130ms spacing avoids compressed bursts at the receiver around a 1s window edge.
#[derive(Clone)]
pub(super) struct IndexReader {
    client: HubSpotClient,
    next_start: Arc<tokio::sync::Mutex<tokio::time::Instant>>,
}
impl IndexReader {
    pub(super) fn new(client: &HubSpotClient) -> Self {
        Self {
            client: client.clone(),
            next_start: Arc::new(tokio::sync::Mutex::new(tokio::time::Instant::now())),
        }
    }
    pub(super) async fn read(
        &self,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
        delays: &[Duration],
    ) -> Result<Value, ReadError> {
        read_with_spacing(
            &self.client,
            path,
            query,
            body,
            delays,
            Some(&self.next_start),
        )
        .await
    }
}

pub(super) async fn read(
    client: &HubSpotClient,
    path: &str,
    query: &[(&str, String)],
    body: Option<&Value>,
    delays: &[Duration],
) -> Result<Value, ReadError> {
    read_with_spacing(client, path, query, body, delays, None).await
}
async fn read_with_spacing(
    client: &HubSpotClient,
    path: &str,
    query: &[(&str, String)],
    body: Option<&Value>,
    delays: &[Duration],
    spacing: Option<&tokio::sync::Mutex<tokio::time::Instant>>,
) -> Result<Value, ReadError> {
    for attempt in 0..=delays.len() {
        if let Some(spacing) = spacing {
            let mut next = spacing.lock().await;
            tokio::time::sleep_until(*next).await;
            *next = tokio::time::Instant::now() + Duration::from_millis(130);
        }
        let (error, retry, retry_after) =
            match client.read_raw(body.is_some(), path, query, body).await {
                Ok(reply) if (200..300).contains(&reply.status) => {
                    return reply.body.ok_or_else(|| fail("hubspot_invalid_response"))
                }
                Ok(reply) => {
                    let error = match reply.status {
                        403 => fail("hubspot_scope_denied"),
                        401 => fail("hubspot_auth_failed"),
                        429 => ReadError(StatusCode::TOO_MANY_REQUESTS, "hubspot_rate_limited"),
                        _ => fail("hubspot_read_failed"),
                    };
                    (
                        error,
                        reply.status == 429 || (500..600).contains(&reply.status),
                        Duration::from_secs(reply.retry_after_secs.unwrap_or(0)),
                    )
                }
                Err(error) => {
                    let retry = transient(&error);
                    (client_error(error), retry, Duration::ZERO)
                }
            };
        if !retry || attempt == delays.len() {
            return Err(error);
        }
        let wait = delays[attempt].max(retry_after);
        tracing::info!(
            count = attempt + 1,
            elapsed_ms = wait.as_millis() as u64,
            "job-copy background read retry wait"
        );
        tokio::time::sleep(wait).await;
    }
    Err(fail("hubspot_read_failed"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_transient_failures_are_retried() {
        for error in [
            HubSpotError::Timeout,
            HubSpotError::Transport(String::new()),
            HubSpotError::Busy,
            HubSpotError::RateLimited,
            HubSpotError::Upstream { status: 503 },
        ] {
            assert!(transient(&error));
        }
        for error in [
            HubSpotError::NotConfigured,
            HubSpotError::Auth { status: 401 },
            HubSpotError::Auth { status: 403 },
            HubSpotError::NotFound,
            HubSpotError::Upstream { status: 400 },
            HubSpotError::Decode(String::new()),
        ] {
            assert!(!transient(&error));
        }
    }
}
