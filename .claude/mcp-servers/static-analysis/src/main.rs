mod normalize;
mod runner;
mod server;
mod types;

use anyhow::{Context, Result};
use rmcp::{transport::stdio, ServiceExt};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Log to stderr (stdout is reserved for MCP protocol).
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_target(false)
        .init();

    let server = server::StaticAnalysisServer::new();

    info!("static-analysis MCP server starting on stdio");

    let service = server
        .serve(stdio())
        .await
        .context("Failed to start MCP service")?;
    service.waiting().await?;

    Ok(())
}
