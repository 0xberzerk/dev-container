use crate::types::ToolRunStatus;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{info, warn};

/// Output paths relative to the project root.
const SLITHER_OUTPUT: &str = "analysis/slither.json";
const ADERYN_OUTPUT: &str = "analysis/aderyn.json";
pub const NORMALIZED_OUTPUT: &str = "analysis/static-analysis.json";

/// Run Slither on the project and write JSON output.
/// Returns the raw JSON string on success, or a ToolRunStatus on failure.
pub async fn run_slither(project_path: &Path) -> Result<String, ToolRunStatus> {
    let output_path = project_path.join(SLITHER_OUTPUT);
    ensure_analysis_dir(project_path).await;

    info!("Running Slither on {}", project_path.display());

    let result = Command::new("slither")
        .arg(".")
        .arg("--json")
        .arg(output_path.to_str().unwrap())
        .current_dir(project_path)
        .output()
        .await;

    match result {
        Ok(output) => {
            // Slither exits with non-zero for findings (that's normal).
            // Only treat as error if the JSON file wasn't produced.
            if output_path.exists() {
                match tokio::fs::read_to_string(&output_path).await {
                    Ok(json) => {
                        info!("Slither JSON output: {} bytes", json.len());
                        Ok(json)
                    }
                    Err(e) => Err(ToolRunStatus {
                        ran: true,
                        success: false,
                        error: Some(format!("Failed to read Slither output: {e}")),
                        ..Default::default()
                    }),
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("Slither did not produce output. stderr: {stderr}");
                Err(ToolRunStatus {
                    ran: true,
                    success: false,
                    error: Some(format!("Slither did not produce output. stderr: {}", truncate(&stderr, 500))),
                    ..Default::default()
                })
            }
        }
        Err(e) => {
            warn!("Failed to execute Slither: {e}");
            Err(ToolRunStatus {
                ran: true,
                success: false,
                error: Some(format!("Failed to execute slither: {e}")),
                ..Default::default()
            })
        }
    }
}

/// Run Aderyn on the project and write JSON output.
/// Returns the raw JSON string on success, or a ToolRunStatus on failure.
pub async fn run_aderyn(project_path: &Path) -> Result<String, ToolRunStatus> {
    let output_path = project_path.join(ADERYN_OUTPUT);
    ensure_analysis_dir(project_path).await;

    info!("Running Aderyn on {}", project_path.display());

    let result = Command::new("aderyn")
        .arg(".")
        .arg("-o")
        .arg(output_path.to_str().unwrap())
        .current_dir(project_path)
        .output()
        .await;

    match result {
        Ok(output) => {
            if output_path.exists() {
                match tokio::fs::read_to_string(&output_path).await {
                    Ok(json) => {
                        info!("Aderyn JSON output: {} bytes", json.len());
                        Ok(json)
                    }
                    Err(e) => Err(ToolRunStatus {
                        ran: true,
                        success: false,
                        error: Some(format!("Failed to read Aderyn output: {e}")),
                        ..Default::default()
                    }),
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("Aderyn did not produce output. stderr: {stderr}");
                Err(ToolRunStatus {
                    ran: true,
                    success: false,
                    error: Some(format!("Aderyn did not produce output. stderr: {}", truncate(&stderr, 500))),
                    ..Default::default()
                })
            }
        }
        Err(e) => {
            warn!("Failed to execute Aderyn: {e}");
            Err(ToolRunStatus {
                ran: true,
                success: false,
                error: Some(format!("Failed to execute aderyn: {e}")),
                ..Default::default()
            })
        }
    }
}

/// Write the normalized report to disk.
pub async fn write_report(project_path: &Path, json: &str) -> anyhow::Result<()> {
    let output_path = project_path.join(NORMALIZED_OUTPUT);
    ensure_analysis_dir(project_path).await;
    tokio::fs::write(&output_path, json).await?;
    info!("Normalized report written to {}", output_path.display());
    Ok(())
}

/// Read the normalized report from disk.
pub async fn read_report(project_path: &Path) -> anyhow::Result<String> {
    let output_path = project_path.join(NORMALIZED_OUTPUT);
    let json = tokio::fs::read_to_string(&output_path).await?;
    Ok(json)
}

/// Resolve the project path from an optional parameter or environment variable.
pub fn resolve_project_path(param: Option<&str>) -> PathBuf {
    if let Some(p) = param {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    if let Ok(p) = std::env::var("PROJECT_ROOT") {
        return PathBuf::from(p);
    }
    PathBuf::from(".")
}

async fn ensure_analysis_dir(project_path: &Path) {
    let dir = project_path.join("analysis");
    if !dir.exists() {
        let _ = tokio::fs::create_dir_all(&dir).await;
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}
