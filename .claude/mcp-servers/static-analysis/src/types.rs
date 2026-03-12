#![allow(dead_code)] // Deserialization structs have fields read by serde, not by code.

use serde::{Deserialize, Serialize};

// ─── Unified Schema ───────────────────────────────────────────────

/// Top-level normalized output written to `analysis/static-analysis.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticAnalysisReport {
    pub metadata: ReportMetadata,
    pub findings: Vec<UnifiedFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportMetadata {
    pub timestamp: String,
    pub project_path: String,
    pub slither: ToolRunStatus,
    pub aderyn: ToolRunStatus,
    pub total_findings: usize,
    pub by_severity: SeverityCounts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRunStatus {
    pub ran: bool,
    pub success: bool,
    /// Raw finding count before severity filter.
    pub raw_findings: usize,
    /// Finding count after severity filter.
    pub kept_findings: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Default for ToolRunStatus {
    fn default() -> Self {
        Self {
            ran: false,
            success: false,
            raw_findings: 0,
            kept_findings: 0,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SeverityCounts {
    pub high: usize,
    pub medium: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedFinding {
    /// Deterministic hash for dedup.
    pub id: String,
    /// Which tool(s) produced this finding.
    pub tools: Vec<ToolSource>,
    pub detector: String,
    pub severity: Severity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
    pub title: String,
    pub description: String,
    pub locations: Vec<Location>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSource {
    pub tool: AnalysisTool,
    pub detector: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnalysisTool {
    Slither,
    Aderyn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    High,
    Medium,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub file: String,
    pub start_line: usize,
    pub end_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<usize>,
}

// ─── Slither Raw Types ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SlitherOutput {
    pub success: bool,
    pub error: Option<String>,
    pub results: Option<SlitherResults>,
}

#[derive(Debug, Deserialize)]
pub struct SlitherResults {
    pub detectors: Option<Vec<SlitherDetector>>,
}

#[derive(Debug, Deserialize)]
pub struct SlitherDetector {
    pub check: String,
    pub impact: String,
    pub confidence: String,
    pub description: String,
    #[serde(default)]
    pub markdown: Option<String>,
    pub id: String,
    #[serde(default)]
    pub elements: Vec<SlitherElement>,
}

#[derive(Debug, Deserialize)]
pub struct SlitherElement {
    #[serde(rename = "type")]
    pub element_type: Option<String>,
    pub name: Option<String>,
    pub source_mapping: Option<SlitherSourceMapping>,
}

#[derive(Debug, Deserialize)]
pub struct SlitherSourceMapping {
    pub filename_relative: Option<String>,
    #[serde(default)]
    pub lines: Vec<usize>,
    pub starting_column: Option<usize>,
    pub ending_column: Option<usize>,
    #[serde(default)]
    pub is_dependency: bool,
}

// ─── Aderyn Raw Types ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AderynOutput {
    pub files_summary: Option<AderynFilesSummary>,
    pub issue_count: Option<AderynIssueCount>,
    pub high_issues: Option<AderynIssueGroup>,
    pub low_issues: Option<AderynIssueGroup>,
    #[serde(default)]
    pub detectors_used: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct AderynFilesSummary {
    pub total_source_units: Option<usize>,
    pub total_sloc: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct AderynIssueCount {
    pub high: Option<usize>,
    pub low: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct AderynIssueGroup {
    pub issues: Vec<AderynIssue>,
}

#[derive(Debug, Deserialize)]
pub struct AderynIssue {
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub detector_name: Option<String>,
    #[serde(default)]
    pub instances: Vec<AderynInstance>,
}

#[derive(Debug, Deserialize)]
pub struct AderynInstance {
    pub contract_path: Option<String>,
    pub line_no: Option<usize>,
    pub src: Option<String>,
    pub src_char: Option<String>,
    pub hint: Option<String>,
}
