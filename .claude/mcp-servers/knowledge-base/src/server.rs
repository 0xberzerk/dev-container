use std::sync::Arc;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
    ServerHandler,
};
use serde::Deserialize;

use knowledge_base::types::{CurationContext, CurationStatus, FeedbackItem};
use knowledge_base::KnowledgeBase;

#[derive(Clone)]
pub struct KbServer {
    tool_router: ToolRouter<Self>,
    kb: Arc<KnowledgeBase>,
}

// -- Tool parameter schemas --

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IngestParams {
    /// Free-text search keywords used in the original Solodit query.
    pub keywords: String,

    /// Severity filter used in the query (e.g. ["HIGH", "MEDIUM"]).
    #[serde(default)]
    pub impact: Vec<String>,

    /// Vulnerability tags used in the query (e.g. ["Reentrancy", "ERC4626"]).
    #[serde(default)]
    pub tags: Vec<String>,

    /// Protocol categories used in the query (e.g. ["Lending"]).
    #[serde(default)]
    pub protocol_categories: Vec<String>,

    /// Minimum quality score filter (1-5).
    #[serde(default)]
    pub min_quality: Option<u8>,

    /// The findings to ingest (from a Solodit search result).
    pub findings: Vec<IngestFindingParam>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IngestFindingParam {
    pub slug: String,
    pub title: String,
    pub impact: String,
    #[serde(default)]
    pub quality_score: f64,
    #[serde(default)]
    pub firm: String,
    #[serde(default)]
    pub protocol: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CurateParams {
    /// Contract names, function names, identifiers from the source code.
    #[serde(default)]
    pub codebase_keywords: Vec<String>,

    /// Integration types from @audit-integration tags.
    #[serde(default)]
    pub integration_types: Vec<String>,

    /// Protocol categories from @audit tags or Architect detection.
    #[serde(default)]
    pub protocol_categories: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QueryParams {
    /// Filter by vulnerability tags. Empty = no filter.
    #[serde(default)]
    pub tags: Vec<String>,

    /// Filter by protocol categories. Empty = no filter.
    #[serde(default)]
    pub categories: Vec<String>,

    /// Keyword search against title/summary. Empty = no filter.
    #[serde(default)]
    pub keywords: Vec<String>,

    /// Max entries to return (context budget). Default: 50.
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,

    /// Exclude noise entries. Default: true.
    #[serde(default = "default_true")]
    pub exclude_noise: bool,
}

fn default_max_entries() -> usize {
    50
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetCurationParams {
    /// The entry ID to update (e.g. "solodit:some-finding-slug").
    pub entry_id: String,

    /// New curation status: "unreviewed", "useful", "noise", or "critical".
    pub status: String,

    /// Optional auditor notes.
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FeedbackParams {
    /// List of feedback items from auditor review.
    pub items: Vec<FeedbackItemParam>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FeedbackItemParam {
    /// The entry ID to update.
    pub entry_id: String,
    /// New curation status: "useful", "noise", or "critical".
    pub status: String,
    /// Optional reason for the status change.
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ImportSeedParams {
    /// Absolute path to the seed JSON file to import.
    pub path: String,
}

// -- Tool implementations --

#[tool_router(router = tool_router)]
impl KbServer {
    pub fn new(kb: KnowledgeBase) -> Self {
        Self {
            tool_router: Self::tool_router(),
            kb: Arc::new(kb),
        }
    }

    #[tool(description = "Ingest Solodit search results into the Knowledge Base raw cache. Computes a fingerprint from query params, skips if cache is fresh, and filters out non-HIGH/MEDIUM findings. Returns the number of entries ingested.")]
    async fn kb_ingest(&self, Parameters(params): Parameters<IngestParams>) -> String {
        let query_params = knowledge_base::types::QueryParams {
            keywords: params.keywords,
            impact: params.impact,
            tags: params.tags,
            protocol_categories: params.protocol_categories,
            min_quality: params.min_quality,
        };

        let findings: Vec<knowledge_base::raw::IngestFinding> = params
            .findings
            .into_iter()
            .map(|f| knowledge_base::raw::IngestFinding {
                slug: f.slug,
                title: f.title,
                impact: f.impact,
                quality_score: f.quality_score,
                firm: f.firm,
                protocol: f.protocol,
                tags: f.tags,
                category: f.category,
                summary: f.summary,
                content: f.content,
            })
            .collect();

        match self.kb.ingest(&query_params, findings) {
            Ok(count) => format!("{{\"ingested\": {}}}", count),
            Err(e) => format!("{{\"error\": \"ingest_failed\", \"message\": \"{}\"}}", e),
        }
    }

    #[tool(description = "Run a curation pass over all raw cache and seed entries. Deduplicates, scores by quality and impact, and writes curated severity files. Preserves existing curation status for previously curated entries.")]
    async fn kb_curate(&self, Parameters(params): Parameters<CurateParams>) -> String {
        let context = CurationContext {
            codebase_keywords: params.codebase_keywords,
            integration_types: params.integration_types,
            protocol_categories: params.protocol_categories,
        };

        match self.kb.curate(&context) {
            Ok(stats) => format!(
                "{{\"total_processed\": {}, \"high_count\": {}, \"medium_count\": {}, \"noise_skipped\": {}}}",
                stats.total_processed, stats.high_count, stats.medium_count, stats.noise_skipped
            ),
            Err(e) => format!("{{\"error\": \"curate_failed\", \"message\": \"{}\"}}", e),
        }
    }

    #[tool(description = "Query the curated Knowledge Base for agent consumption. Returns entries ordered by severity (HIGH before MEDIUM) then curation rank. Filter by tags, categories, and keywords. Noise is excluded by default.")]
    async fn kb_query(&self, Parameters(params): Parameters<QueryParams>) -> String {
        let query = knowledge_base::types::KbQuery {
            tags: params.tags,
            categories: params.categories,
            keywords: params.keywords,
            max_entries: params.max_entries,
            exclude_noise: params.exclude_noise,
        };

        match self.kb.query(&query) {
            Ok(result) => {
                serde_json::to_string_pretty(&result).unwrap_or_else(|e| {
                    format!("{{\"error\": \"serialize_failed\", \"message\": \"{}\"}}", e)
                })
            }
            Err(e) => format!("{{\"error\": \"query_failed\", \"message\": \"{}\"}}", e),
        }
    }

    #[tool(description = "Update curation status for a single Knowledge Base entry. Status can be: 'unreviewed', 'useful', 'noise', or 'critical'. Use after auditor reviews findings.")]
    async fn kb_set_curation(
        &self,
        Parameters(params): Parameters<SetCurationParams>,
    ) -> String {
        let status = match parse_curation_status(&params.status) {
            Some(s) => s,
            None => {
                return format!(
                    "{{\"error\": \"invalid_status\", \"message\": \"must be one of: unreviewed, useful, noise, critical\"}}"
                )
            }
        };

        match self.kb.set_curation(&params.entry_id, status, params.notes) {
            Ok(true) => "{\"updated\": true}".to_string(),
            Ok(false) => format!(
                "{{\"updated\": false, \"message\": \"entry '{}' not found\"}}",
                params.entry_id
            ),
            Err(e) => format!(
                "{{\"error\": \"set_curation_failed\", \"message\": \"{}\"}}",
                e
            ),
        }
    }

    #[tool(description = "Apply bulk feedback from auditor review. Maps audit actions to curation status: confirmed → useful, false-positive → noise, escalate → critical. Returns count of entries updated.")]
    async fn kb_apply_feedback(
        &self,
        Parameters(params): Parameters<FeedbackParams>,
    ) -> String {
        let mut items = Vec::new();
        for item in params.items {
            let status = match parse_curation_status(&item.status) {
                Some(s) => s,
                None => {
                    return format!(
                        "{{\"error\": \"invalid_status\", \"message\": \"'{}' for entry '{}' is not valid\"}}",
                        item.status, item.entry_id
                    )
                }
            };
            items.push(FeedbackItem {
                entry_id: item.entry_id,
                new_status: status,
                reason: item.reason,
            });
        }

        match self.kb.apply_feedback(&items) {
            Ok(count) => format!("{{\"updated\": {}}}", count),
            Err(e) => format!(
                "{{\"error\": \"feedback_failed\", \"message\": \"{}\"}}",
                e
            ),
        }
    }

    #[tool(description = "Import a seed file into the Knowledge Base. Seed files contain auditor-curated known bugs, war stories, and bookmarked findings. Provide the absolute path to the seed JSON file.")]
    async fn kb_import_seed(
        &self,
        Parameters(params): Parameters<ImportSeedParams>,
    ) -> String {
        let path = std::path::Path::new(&params.path);
        match self.kb.import_seed_file(path) {
            Ok(count) => format!("{{\"imported\": {}}}", count),
            Err(e) => format!(
                "{{\"error\": \"import_failed\", \"message\": \"{}\"}}",
                e
            ),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for KbServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default().with_instructions(
            "Knowledge Base for smart contract security auditing. \
             Curated vulnerability index between Solodit and the agent pipeline. \
             Ingest findings, run curation passes, and query for agent consumption.",
        )
    }
}

fn parse_curation_status(s: &str) -> Option<CurationStatus> {
    match s.to_lowercase().as_str() {
        "unreviewed" => Some(CurationStatus::Unreviewed),
        "useful" => Some(CurationStatus::Useful),
        "noise" => Some(CurationStatus::Noise),
        "critical" => Some(CurationStatus::Critical),
        _ => None,
    }
}
