//! Read-only benchmark of the same adapter used by /api/crm/metadata. No server/main/DB initialization.
use rust_dashboard::handlers::crm_metadata::MetadataService;
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
    let service = match MetadataService::new(token) {
        Ok(service) => service,
        Err(error) => {
            eprintln!("{}", error.code);
            std::process::exit(1);
        }
    };
    for (name, refresh) in [("cold", false), ("warm", false), ("refresh", true)] {
        match service.metadata(refresh).await {
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
                eprintln!("{}", error.code);
                std::process::exit(1);
            }
        }
    }
}
