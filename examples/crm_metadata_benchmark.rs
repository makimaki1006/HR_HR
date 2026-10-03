//! Read-only benchmark of the same client and cache used by /api/crm/metadata. No server/main/DB initialization.
use rust_dashboard::handlers::crm_metadata::MetadataCache;
use rust_dashboard::hubspot::{ClientOptions, HubSpotClient, DEFAULT_BASE_URL};
#[tokio::main]
async fn main() {
    // Optional existing server env file. Load in this process only; never copy or print credentials.
    if let Some(path) = std::env::args().nth(1) {
        if dotenvy::from_path(path).is_err() {
            eprintln!("env_file_unavailable");
            std::process::exit(1);
        }
    }
    let token = std::env::var("HUBSPOT_ACCESS_TOKEN").unwrap_or_default();
    let client = match HubSpotClient::new(token, DEFAULT_BASE_URL, ClientOptions::default()) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{}", error.error_kind());
            std::process::exit(1);
        }
    };
    // refresh の下限なし (計測用)。本番ルートの 5 秒下限とは別
    let cache = MetadataCache::default();
    for (name, refresh) in [("cold", false), ("warm", false), ("refresh", true)] {
        match cache.get(&client, refresh).await {
            Ok(response) => println!(
                "{}",
                serde_json::json!({
                    "sample": name, "properties": response.properties.len(), "pipelines": response.pipelines.len(),
                    "stages": response.pipelines.iter().map(|p| p.stages.len()).sum::<usize>(),
                    "hubspot_ms": response.hubspot_ms, "total_ms": response.total_ms,
                    "cache_hit": response.cache_hit, "fetched_at": response.fetched_at,
                })
            ),
            Err(error) => {
                eprintln!("{}", error.error_kind());
                std::process::exit(1);
            }
        }
    }
}
