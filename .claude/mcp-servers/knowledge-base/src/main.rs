mod server;

use std::path::PathBuf;

use anyhow::{Context, Result};
use rmcp::{transport::stdio, ServiceExt};
use tracing::info;

/// Default KB directory relative to the project root.
const DEFAULT_KB_DIR: &str = "KnowledgeBase";

#[tokio::main]
async fn main() -> Result<()> {
    // Logging to stderr — stdout is reserved for MCP protocol
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_target(false)
        .init();

    // KB directory: from env or default
    let kb_dir = std::env::var("KB_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_KB_DIR));

    let kb = knowledge_base::KnowledgeBase::new(kb_dir)
        .context("Failed to initialize Knowledge Base")?;

    let server = server::KbServer::new(kb);

    info!("knowledge-base MCP server starting on stdio");

    let service = server
        .serve(stdio())
        .await
        .context("Failed to start MCP service")?;
    service.waiting().await?;

    Ok(())
}
