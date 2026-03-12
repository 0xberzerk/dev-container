use crate::client::SoloditClient;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
    ServerHandler,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone)]
pub struct SoloditServer {
    tool_router: ToolRouter<Self>,
    client: Arc<SoloditClient>,
}

// -- Tool parameter schemas --

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchFindingsParams {
    /// Free-text search query (e.g. "reentrancy", "flash loan", "oracle manipulation")
    #[serde(default)]
    pub keywords: String,

    /// Severity filter. Only HIGH and MEDIUM are allowed — others are silently stripped.
    /// Defaults to both if omitted or empty after filtering.
    #[serde(default)]
    pub impact: Option<Vec<String>>,

    /// Vulnerability classification tags (e.g. "Reentrancy", "ERC4626", "Oracle")
    #[serde(default)]
    pub tags: Option<Vec<String>>,

    /// Protocol categories (e.g. "Lending", "Dexes", "Bridge")
    #[serde(default)]
    pub protocol_categories: Option<Vec<String>>,

    /// Minimum quality score (1-5). Filters out low-quality findings.
    #[serde(default)]
    pub min_quality: Option<u8>,

    /// Sort field. Default: "Quality"
    #[serde(default = "default_sort_field")]
    pub sort_field: String,

    /// Sort direction. Default: "Desc"
    #[serde(default = "default_sort_direction")]
    pub sort_direction: String,

    /// Page number (1-indexed). Default: 1
    #[serde(default = "default_page")]
    pub page: u32,

    /// Results per page (max 100). Default: 20
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

fn default_sort_field() -> String {
    "Quality".to_string()
}
fn default_sort_direction() -> String {
    "Desc".to_string()
}
fn default_page() -> u32 {
    1
}
fn default_page_size() -> u32 {
    20
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetFindingParams {
    /// Unique slug identifier of the finding (from search results)
    pub slug: String,
}

// -- Tool implementations --

#[tool_router(router = tool_router)]
impl SoloditServer {
    pub fn new(client: SoloditClient) -> Self {
        Self {
            tool_router: Self::tool_router(),
            client: Arc::new(client),
        }
    }

    #[tool(description = "Search the Solodit vulnerability database for known smart contract security findings. Filter by severity, vulnerability tags, and protocol category. Only HIGH and MEDIUM severity findings are returned.")]
    async fn search_findings(
        &self,
        Parameters(params): Parameters<SearchFindingsParams>,
    ) -> String {
        match self
            .client
            .search_findings(
                &params.keywords,
                params.impact,
                params.tags,
                params.protocol_categories,
                params.min_quality,
                &params.sort_field,
                &params.sort_direction,
                params.page,
                params.page_size,
            )
            .await
        {
            Ok(result) => {
                serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
            }
            Err(e) => format!("{{\"error\": \"search_failed\", \"message\": \"{}\"}}", e),
        }
    }

    #[tool(description = "Get the full content of a specific Solodit finding by its slug. Returns the complete finding including description, impact analysis, proof of concept (if available), and recommended mitigation.")]
    async fn get_finding(&self, Parameters(params): Parameters<GetFindingParams>) -> String {
        match self.client.get_finding(&params.slug).await {
            Ok(result) => {
                serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
            }
            Err(e) => format!("{{\"error\": \"get_finding_failed\", \"message\": \"{}\"}}", e),
        }
    }

    #[tool(description = "List available vulnerability tags for filtering search results. Tags classify vulnerability types (e.g. Reentrancy, Oracle, ERC4626, Flash Loan).")]
    async fn list_tags(&self) -> String {
        match self.client.list_tags().await {
            Ok(result) => {
                serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
            }
            Err(e) => format!("{{\"error\": \"list_tags_failed\", \"message\": \"{}\"}}", e),
        }
    }

    #[tool(description = "List available protocol categories for filtering search results. Categories classify the type of protocol audited (e.g. Lending, Dexes, Bridge, Yield).")]
    async fn list_protocol_categories(&self) -> String {
        match self.client.list_protocol_categories().await {
            Ok(result) => {
                serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
            }
            Err(e) => format!(
                "{{\"error\": \"list_categories_failed\", \"message\": \"{}\"}}",
                e
            ),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for SoloditServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default()
            .with_instructions(
                "Solodit vulnerability database for smart contract security auditing. \
                 Search 50,000+ findings from top audit firms. Only HIGH \
                 and MEDIUM severity findings are surfaced.",
            )
    }
}
