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

pub(super) async fn read(
    client: &HubSpotClient,
    path: &str,
    query: &[(&str, String)],
    body: Option<&Value>,
    delays: &[Duration],
) -> Result<Value, ReadError> {
    for attempt in 0..=delays.len() {
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
