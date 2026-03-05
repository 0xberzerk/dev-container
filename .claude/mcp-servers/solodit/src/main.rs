mod client;
mod server;

use anyhow::{Context, Result};
use rmcp::{transport::stdio, ServiceExt};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Logging to stderr — stdout is reserved for MCP protocol
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_target(false)
        .init();

    let api_key = std::env::var("CYFRIN_API_KEY").context(
        "CYFRIN_API_KEY environment variable is required.\n\
         Get your key at: https://solodit.cyfrin.io → profile → API Keys",
    )?;

    let client =
        client::SoloditClient::new(&api_key).context("Failed to initialize Solodit client")?;
    let server = server::SoloditServer::new(client);

    info!("solodit-mcp server starting on stdio");

    let service = server.serve(stdio()).await.context("Failed to start MCP service")?;
    service.waiting().await?;

    Ok(())
}
