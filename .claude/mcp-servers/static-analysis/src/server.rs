use crate::normalize::{build_report, merge_and_dedup, parse_aderyn, parse_slither};
use crate::runner::{read_report, resolve_project_path, run_aderyn, run_slither, write_report};
use crate::types::{
    AnalysisTool, ReportMetadata, Severity, StaticAnalysisReport, ToolRunStatus, UnifiedFinding,
};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
    ServerHandler,
};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct StaticAnalysisServer {
    tool_router: ToolRouter<Self>,
}

// ─── Parameter types ──────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SaRunParams {
    /// Absolute path to the project root. Defaults to PROJECT_ROOT env var or cwd.
    #[serde(default)]
    pub project_path: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SaGetResultsParams {
    /// Absolute path to the project root. Defaults to PROJECT_ROOT env var or cwd.
    #[serde(default)]
    pub project_path: Option<String>,
    /// Filter by severity: "high" or "medium".
    #[serde(default)]
    pub severity: Option<String>,
    /// Filter by file path (substring match).
    #[serde(default)]
    pub file: Option<String>,
    /// Filter by detector name (substring match).
    #[serde(default)]
    pub detector: Option<String>,
    /// Maximum number of findings to return (default 100, max 500).
    #[serde(default)]
    pub limit: Option<usize>,
}

// ─── Tool implementations ─────────────────────────────────────────

#[tool_router(router = tool_router)]
impl StaticAnalysisServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Run Slither + Aderyn static analysis on the project. Normalizes output into analysis/static-analysis.json. Only HIGH and MEDIUM severity findings are kept. Returns a summary.")]
    async fn sa_run(&self, Parameters(params): Parameters<SaRunParams>) -> String {
        let project_path = resolve_project_path(params.project_path.as_deref());

        if !project_path.exists() {
            return format!(
                "Error: project path does not exist: {}",
                project_path.display()
            );
        }

        // Run both tools (sequentially — they both need the compiler).
        let slither_result = run_slither(&project_path).await;
        let aderyn_result = run_aderyn(&project_path).await;

        // Parse results.
        let (slither_findings, slither_status) = match slither_result {
            Ok(json) => match parse_slither(&json) {
                Ok(r) => r,
                Err(e) => (
                    vec![],
                    ToolRunStatus {
                        ran: true,
                        success: false,
                        error: Some(e),
                        ..Default::default()
                    },
                ),
            },
            Err(status) => (vec![], status),
        };

        let (aderyn_findings, aderyn_status) = match aderyn_result {
            Ok(json) => match parse_aderyn(&json) {
                Ok(r) => r,
                Err(e) => (
                    vec![],
                    ToolRunStatus {
                        ran: true,
                        success: false,
                        error: Some(e),
                        ..Default::default()
                    },
                ),
            },
            Err(status) => (vec![], status),
        };

        // Merge and dedup.
        let findings = merge_and_dedup(slither_findings, aderyn_findings);

        // Build report.
        let report = build_report(
            findings,
            project_path.to_str().unwrap_or("."),
            slither_status,
            aderyn_status,
        );

        // Write to disk.
        let report_json = match serde_json::to_string_pretty(&report) {
            Ok(j) => j,
            Err(e) => return format!("Error serializing report: {e}"),
        };

        if let Err(e) = write_report(&project_path, &report_json).await {
            return format!("Error writing report: {e}");
        }

        // Return summary.
        format_summary(&report)
    }

    #[tool(description = "Read normalized static analysis results from analysis/static-analysis.json. Supports filtering by severity, file, detector, and limit. Returns findings as JSON.")]
    async fn sa_get_results(
        &self,
        Parameters(params): Parameters<SaGetResultsParams>,
    ) -> String {
        let project_path = resolve_project_path(params.project_path.as_deref());

        let json = match read_report(&project_path).await {
            Ok(j) => j,
            Err(e) => {
                return format!(
                    "Error: could not read analysis/static-analysis.json — run sa_run first. ({e})"
                );
            }
        };

        let report: StaticAnalysisReport = match serde_json::from_str(&json) {
            Ok(r) => r,
            Err(e) => return format!("Error parsing report: {e}"),
        };

        let mut findings = report.findings;

        // Filter by severity.
        if let Some(ref sev) = params.severity {
            let sev_lower = sev.to_lowercase();
            findings.retain(|f| {
                let f_sev = serde_json::to_string(&f.severity).unwrap_or_default();
                f_sev.trim_matches('"') == sev_lower
            });
        }

        // Filter by file path (substring match).
        if let Some(ref file) = params.file {
            findings.retain(|f| f.locations.iter().any(|l| l.file.contains(file.as_str())));
        }

        // Filter by detector name (substring match).
        if let Some(ref det) = params.detector {
            let det_lower = det.to_lowercase();
            findings.retain(|f| f.detector.to_lowercase().contains(&det_lower));
        }

        // Apply limit.
        let limit = params.limit.unwrap_or(100).min(500);
        findings.truncate(limit);

        // Return as JSON.
        let result = SaQueryResult {
            total_matched: findings.len(),
            metadata: report.metadata,
            findings,
        };

        match serde_json::to_string_pretty(&result) {
            Ok(j) => j,
            Err(e) => format!("Error serializing results: {e}"),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for StaticAnalysisServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default().with_instructions(
            "Static analysis runner and normalizer for smart contract security auditing. \
             Runs Slither and Aderyn, merges and deduplicates findings into a unified format. \
             Only HIGH and MEDIUM severity findings are surfaced.",
        )
    }
}

// ─── Internal types ───────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct SaQueryResult {
    total_matched: usize,
    metadata: ReportMetadata,
    findings: Vec<UnifiedFinding>,
}

// ─── Formatting ───────────────────────────────────────────────────

fn format_summary(report: &StaticAnalysisReport) -> String {
    let m = &report.metadata;
    let mut lines = Vec::new();

    lines.push("=== Static Analysis Complete ===".to_string());
    lines.push(format!("Project: {}", m.project_path));
    lines.push(format!(
        "Slither: {} (raw: {}, kept: {}{})",
        if m.slither.success { "OK" } else { "FAILED" },
        m.slither.raw_findings,
        m.slither.kept_findings,
        m.slither
            .error
            .as_ref()
            .map(|e| format!(", error: {e}"))
            .unwrap_or_default()
    ));
    lines.push(format!(
        "Aderyn:  {} (raw: {}, kept: {}{})",
        if m.aderyn.success { "OK" } else { "FAILED" },
        m.aderyn.raw_findings,
        m.aderyn.kept_findings,
        m.aderyn
            .error
            .as_ref()
            .map(|e| format!(", error: {e}"))
            .unwrap_or_default()
    ));
    lines.push(format!(
        "Total findings: {} (high: {}, medium: {})",
        m.total_findings, m.by_severity.high, m.by_severity.medium
    ));

    // List high-severity findings briefly.
    let highs: Vec<&UnifiedFinding> = report
        .findings
        .iter()
        .filter(|f| f.severity == Severity::High)
        .collect();
    if !highs.is_empty() {
        lines.push(String::new());
        lines.push("HIGH severity:".to_string());
        for f in &highs {
            let loc = f
                .locations
                .first()
                .map(|l| format!("{}:{}", l.file, l.start_line))
                .unwrap_or_else(|| "unknown".to_string());
            let tools: Vec<&str> = f
                .tools
                .iter()
                .map(|t| match t.tool {
                    AnalysisTool::Slither => "slither",
                    AnalysisTool::Aderyn => "aderyn",
                })
                .collect();
            lines.push(format!("  - [{}] {} ({})", tools.join("+"), f.title, loc));
        }
    }

    lines.push(String::new());
    lines.push("Output: analysis/static-analysis.json".to_string());

    lines.join("\n")
}
