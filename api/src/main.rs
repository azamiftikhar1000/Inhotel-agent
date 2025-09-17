use anyhow::Result;
use api::{domain::config::ConnectionsConfig, server::Server};
use dotenvy::dotenv;
use envconfig::Envconfig;
use osentities::telemetry::{get_subscriber, init_subscriber, OtelGuard};
use tracing::info;

fn main() -> Result<()> {
    dotenv().ok();
    
    // Load the full config using Envconfig
    let mut config = ConnectionsConfig::init_from_env()?;
    
    // Set buildable_secret from environment
    config.buildable_secret = std::env::var("BUILDABLE_SECRET")
        .unwrap_or_else(|_| "".to_string());

    // Only create OtelGuard if we have a valid OTLP endpoint
    let _guard = config.otlp_endpoint.clone().filter(|url| !url.is_empty()).map(|url| OtelGuard {
        otlp_url: Some(url),
    });

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(config.worker_threads.unwrap_or(num_cpus::get()))
        .enable_all()
        .build()?;

    runtime.block_on(async move {
        let subscriber = get_subscriber(
            "connections-api".into(),
            "info".into(),
            std::io::stdout,
            // Only pass OTLP endpoint if it's not empty
            config.otlp_endpoint.clone().filter(|url| !url.is_empty()),
        );

        init_subscriber(subscriber);

        info!("Starting API with config:\n{config}");

        let server: Server = Server::init(config).await?;

        server.run().await
    })?;

    Ok(())
}
