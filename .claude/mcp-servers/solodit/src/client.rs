use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, warn};

const API_BASE: &str = "https://solodit.cyfrin.io/api/v1/solodit";
const ALLOWED_IMPACTS: &[&str] = &["HIGH", "MEDIUM"];

// Cache TTLs
const SEARCH_TTL: Duration = Duration::from_secs(300); // 5 minutes
const FINDING_TTL: Duration = Duration::from_secs(3600); // 1 hour
const METADATA_TTL: Duration = Duration::from_secs(3600); // 1 hour

// Rate limit
const RATE_LIMIT_MAX: usize = 20;
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

// -- TTL Cache --

struct CacheEntry {
    value: serde_json::Value,
    expires_at: Instant,
}

pub struct TtlCache {
    store: RwLock<HashMap<String, CacheEntry>>,
}

impl TtlCache {
    fn new() -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
        }
    }

    async fn get(&self, key: &str) -> Option<serde_json::Value> {
        let store = self.store.read().await;
        if let Some(entry) = store.get(key) {
            if Instant::now() < entry.expires_at {
                return Some(entry.value.clone());
            }
        }
        None
    }

    async fn set(&self, key: String, value: serde_json::Value, ttl: Duration) {
        let mut store = self.store.write().await;
        // Evict expired entries opportunistically
        store.retain(|_, entry| Instant::now() < entry.expires_at);
        store.insert(
            key,
            CacheEntry {
                value,
                expires_at: Instant::now() + ttl,
            },
        );
    }
}

// -- Rate Limiter --

struct RateLimiter {
    timestamps: RwLock<Vec<Instant>>,
}

impl RateLimiter {
    fn new() -> Self {
        Self {
            timestamps: RwLock::new(Vec::new()),
        }
    }

    async fn acquire(&self) {
        loop {
            let now = Instant::now();
            let mut timestamps = self.timestamps.write().await;
            timestamps.retain(|t| now.duration_since(*t) < RATE_LIMIT_WINDOW);

            if timestamps.len() < RATE_LIMIT_MAX {
                timestamps.push(now);
                return;
            }

            let oldest = timestamps[0];
            let sleep_time = RATE_LIMIT_WINDOW - now.duration_since(oldest);
            drop(timestamps);
            debug!("Rate limit reached, sleeping for {:?}", sleep_time);
            tokio::time::sleep(sleep_time).await;
        }
    }
}

// -- API Types --

#[derive(Debug, Serialize)]
pub struct SearchRequest {
    pub page: u32,
    #[serde(rename = "pageSize")]
    pub page_size: u32,
    pub filters: SearchFilters,
}

#[derive(Debug, Serialize)]
pub struct SearchFilters {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub keywords: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub impact: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<TagFilter>,
    #[serde(rename = "protocolCategories", skip_serializing_if = "Vec::is_empty")]
    pub protocol_categories: Vec<String>,
    pub languages: Vec<LanguageFilter>,
    #[serde(rename = "sortField")]
    pub sort_field: String,
    #[serde(rename = "sortDirection")]
    pub sort_direction: String,
}

#[derive(Debug, Serialize)]
pub struct ValueFilter {
    pub value: String,
}

// Type aliases for clarity
pub type LanguageFilter = ValueFilter;
pub type TagFilter = ValueFilter;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Finding {
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub impact: String,
    #[serde(default)]
    pub quality_score: f64,
    #[serde(default)]
    pub firm_name: String,
    #[serde(default)]
    pub protocol_name: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub issues_issuetagscore: Vec<serde_json::Value>,
    #[serde(default)]
    pub protocols_protocol: Option<serde_json::Value>,
}

/// Compact finding format for search results (strips verbose nested data)
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CompactFinding {
    slug: String,
    title: String,
    impact: String,
    quality_score: f64,
    firm: String,
    protocol: String,
    tags: Vec<String>,
    category: String,
    summary: Option<String>,
}

impl From<&Finding> for CompactFinding {
    fn from(f: &Finding) -> Self {
        let tags: Vec<String> = f
            .issues_issuetagscore
            .iter()
            .filter_map(|t| {
                t.get("tags_tag")
                    .and_then(|tt| tt.get("title"))
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
            .collect();

        let category = f
            .protocols_protocol
            .as_ref()
            .and_then(|p| p.get("protocols_protocolcategoryscore"))
            .and_then(|cats| cats.as_array())
            .and_then(|arr| arr.first())
            .and_then(|c| c.get("protocols_protocolcategory"))
            .and_then(|pc| pc.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        CompactFinding {
            slug: f.slug.clone(),
            title: f.title.clone(),
            impact: f.impact.clone(),
            quality_score: f.quality_score,
            firm: f.firm_name.clone(),
            protocol: f.protocol_name.clone(),
            tags,
            category,
            summary: f.summary.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SearchResponse {
    #[serde(default)]
    pub findings: Vec<Finding>,
}

// -- Client --

pub struct SoloditClient {
    http: reqwest::Client,
    cache: Arc<TtlCache>,
    rate_limiter: Arc<RateLimiter>,
}

impl SoloditClient {
    pub fn new(api_key: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Cyfrin-API-Key",
            HeaderValue::from_str(api_key).context("Invalid API key format")?,
        );

        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(15))
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            http,
            cache: Arc::new(TtlCache::new()),
            rate_limiter: Arc::new(RateLimiter::new()),
        })
    }

    pub async fn search_findings(
        &self,
        keywords: &str,
        impact: Option<Vec<String>>,
        tags: Option<Vec<String>>,
        protocol_categories: Option<Vec<String>>,
        min_quality: Option<u8>,
        sort_field: &str,
        sort_direction: &str,
        page: u32,
        page_size: u32,
    ) -> Result<serde_json::Value> {
        // Enforce severity guardrail
        let impact = sanitize_impact(impact);

        let request = SearchRequest {
            page,
            page_size: page_size.min(100),
            filters: SearchFilters {
                keywords: keywords.to_string(),
                impact: impact.clone(),
                tags: tags
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|v| TagFilter { value: v })
                    .collect(),
                protocol_categories: protocol_categories.clone().unwrap_or_default(),
                languages: vec![LanguageFilter {
                    value: "Solidity".to_string(),
                }],
                sort_field: sort_field.to_string(),
                sort_direction: sort_direction.to_string(),
            },
        };

        let cache_key = make_cache_key("search", &request)?;

        // Check cache
        if let Some(cached) = self.cache.get(&cache_key).await {
            debug!("Cache hit for search: {}", cache_key);
            return Ok(cached);
        }

        // Rate limit + API call
        self.rate_limiter.acquire().await;
        let response = self.post_with_retry(&format!("{}/findings", API_BASE), &request).await?;

        let mut search_response: SearchResponse = response
            .json()
            .await
            .context("Failed to parse search response")?;

        // Client-side quality filter
        if let Some(min_q) = min_quality {
            search_response.findings.retain(|f| f.quality_score >= min_q as f64);
        }

        let compact: Vec<CompactFinding> = search_response
            .findings
            .iter()
            .map(CompactFinding::from)
            .collect();

        let result = serde_json::json!({
            "findings": compact,
            "count": compact.len(),
            "page": page,
            "page_size": page_size,
        });

        self.cache.set(cache_key, result.clone(), SEARCH_TTL).await;
        Ok(result)
    }

    pub async fn get_finding(&self, slug: &str) -> Result<serde_json::Value> {
        let cache_key = format!("finding:{}", slug);

        if let Some(cached) = self.cache.get(&cache_key).await {
            debug!("Cache hit for finding: {}", slug);
            return Ok(cached);
        }

        // Search by slug as keyword and match exact slug in results
        self.rate_limiter.acquire().await;

        // Extract a meaningful keyword from the slug (first segment before the firm name)
        let keyword = slug
            .split('-')
            .take(5)
            .collect::<Vec<_>>()
            .join(" ");

        let request = SearchRequest {
            page: 1,
            page_size: 10,
            filters: SearchFilters {
                keywords: keyword,
                impact: vec![],
                tags: vec![],
                protocol_categories: vec![],
                languages: vec![LanguageFilter {
                    value: "Solidity".to_string(),
                }],
                sort_field: "Quality".to_string(),
                sort_direction: "Desc".to_string(),
            },
        };

        let response = self
            .post_with_retry(&format!("{}/findings", API_BASE), &request)
            .await?;

        let search_response: SearchResponse = response
            .json()
            .await
            .context("Failed to parse search response")?;

        let result = match search_response.findings.iter().find(|f| f.slug == slug) {
            Some(finding) => serde_json::to_value(finding)
                .context("Failed to serialize finding")?,
            None => serde_json::json!({
                "error": "not_found",
                "message": format!("Finding '{}' not found", slug),
            }),
        };

        self.cache.set(cache_key, result.clone(), FINDING_TTL).await;
        Ok(result)
    }

    pub async fn list_tags(&self) -> Result<serde_json::Value> {
        let cache_key = "metadata:tags".to_string();

        if let Some(cached) = self.cache.get(&cache_key).await {
            debug!("Cache hit for tags");
            return Ok(cached);
        }

        self.rate_limiter.acquire().await;
        let url = format!("{}/tags", API_BASE);
        let result = self.get_with_fallback(&url, serde_json::json!(known_tags())).await?;

        self.cache.set(cache_key, result.clone(), METADATA_TTL).await;
        Ok(result)
    }

    pub async fn list_protocol_categories(&self) -> Result<serde_json::Value> {
        let cache_key = "metadata:categories".to_string();

        if let Some(cached) = self.cache.get(&cache_key).await {
            debug!("Cache hit for categories");
            return Ok(cached);
        }

        self.rate_limiter.acquire().await;
        let url = format!("{}/protocol-categories", API_BASE);
        let result = self
            .get_with_fallback(&url, serde_json::json!(known_protocol_categories()))
            .await?;

        self.cache.set(cache_key, result.clone(), METADATA_TTL).await;
        Ok(result)
    }

    /// GET with fallback for non-auth errors only. Auth failures (401/403) propagate as errors.
    async fn get_with_fallback(
        &self,
        url: &str,
        fallback: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let response = self.http.get(url).send().await;

        match response {
            Ok(resp) => {
                let status = resp.status();
                if status == reqwest::StatusCode::UNAUTHORIZED
                    || status == reqwest::StatusCode::FORBIDDEN
                {
                    anyhow::bail!("Authentication failed — check CYFRIN_API_KEY");
                }
                if status.is_success() {
                    return resp
                        .json::<serde_json::Value>()
                        .await
                        .context("Failed to parse response");
                }
                warn!("GET {} returned {}, using fallback", url, status);
                Ok(fallback)
            }
            Err(e) => {
                warn!("GET {} failed: {}, using fallback", url, e);
                Ok(fallback)
            }
        }
    }

    async fn post_with_retry(
        &self,
        url: &str,
        body: &impl Serialize,
    ) -> Result<reqwest::Response> {
        let response = self.http.post(url).json(body).send().await;

        match response {
            Ok(resp) => {
                let status = resp.status();
                if status == reqwest::StatusCode::UNAUTHORIZED
                    || status == reqwest::StatusCode::FORBIDDEN
                {
                    anyhow::bail!("Authentication failed — check CYFRIN_API_KEY");
                }
                if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    warn!("Rate limited by API, retrying after 5s");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    return self
                        .http
                        .post(url)
                        .json(body)
                        .send()
                        .await
                        .context("Retry after rate limit failed");
                }
                if status.is_server_error() {
                    warn!("Server error {}, retrying after 2s", status);
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    return self
                        .http
                        .post(url)
                        .json(body)
                        .send()
                        .await
                        .context("Retry after server error failed");
                }
                Ok(resp)
            }
            Err(e) => {
                warn!("Request failed: {}, retrying after 2s", e);
                tokio::time::sleep(Duration::from_secs(2)).await;
                self.http
                    .post(url)
                    .json(body)
                    .send()
                    .await
                    .context("Retry after network error failed")
            }
        }
    }
}

fn sanitize_impact(impact: Option<Vec<String>>) -> Vec<String> {
    let filtered: Vec<String> = impact
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.to_uppercase())
        .filter(|s| ALLOWED_IMPACTS.contains(&s.as_str()))
        .collect();

    if filtered.is_empty() {
        ALLOWED_IMPACTS.iter().map(|s| s.to_string()).collect()
    } else {
        filtered
    }
}

fn make_cache_key(prefix: &str, request: &impl Serialize) -> Result<String> {
    let json = serde_json::to_string(request).context("Failed to serialize for cache key")?;
    let hash = Sha256::digest(json.as_bytes())
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    Ok(format!("{}:{}", prefix, &hash[..16]))
}

fn known_tags() -> Vec<&'static str> {
    vec![
        "Access Control", "Reentrancy", "Read-only Reentrancy",
        "Oracle", "Chainlink", "Flash Loan", "Front-Running",
        "Sandwich Attack", "Grief Attack", "Denial-Of-Service",
        "ERC20", "ERC721", "ERC4626", "Fee-on-Transfer", "Rebasing",
        "Overflow/Underflow", "Precision Loss", "Rounding", "Truncation",
        "Storage Collision", "Uninitialized", "Delegatecall",
        "Block.timestamp", "Deadline", "Cross-Chain",
        "LayerZero", "CCIP", "Uniswap", "Aave", "Compound",
        "Proxy", "Upgradeable", "Initializer",
        "Signature", "Replay Attack", "EIP-712",
        "Liquidation", "Collateral", "Interest Rate",
        "AMM", "Slippage", "MEV",
    ]
}

fn known_protocol_categories() -> Vec<&'static str> {
    vec![
        "Lending", "Dexes", "Bridge", "CDP", "Yield", "Derivatives",
        "Liquid Staking", "Cross Chain", "Synthetics", "RWA",
        "NFT Marketplace", "Options", "Prediction Market", "Gaming",
        "Oracle", "Insurance", "Privacy", "Payments", "Staking",
        "Yield Aggregator", "Governance", "Launchpad",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- sanitize_impact --

    #[test]
    fn sanitize_impact_defaults_to_high_medium_when_none() {
        let result = sanitize_impact(None);
        assert_eq!(result, vec!["HIGH", "MEDIUM"]);
    }

    #[test]
    fn sanitize_impact_defaults_to_high_medium_when_empty() {
        let result = sanitize_impact(Some(vec![]));
        assert_eq!(result, vec!["HIGH", "MEDIUM"]);
    }

    #[test]
    fn sanitize_impact_strips_critical() {
        let result = sanitize_impact(Some(vec!["CRITICAL".into(), "HIGH".into()]));
        assert_eq!(result, vec!["HIGH"]);
    }

    #[test]
    fn sanitize_impact_strips_low_and_gas() {
        let result = sanitize_impact(Some(vec!["LOW".into(), "GAS".into(), "MEDIUM".into()]));
        assert_eq!(result, vec!["MEDIUM"]);
    }

    #[test]
    fn sanitize_impact_normalizes_case() {
        let result = sanitize_impact(Some(vec!["high".into(), "Medium".into()]));
        assert_eq!(result, vec!["HIGH", "MEDIUM"]);
    }

    #[test]
    fn sanitize_impact_defaults_when_all_stripped() {
        let result = sanitize_impact(Some(vec!["LOW".into(), "CRITICAL".into(), "GAS".into()]));
        assert_eq!(result, vec!["HIGH", "MEDIUM"]);
    }

    // -- SearchRequest serialization (API contract) --

    #[test]
    fn search_request_serializes_tags_as_value_objects() {
        let req = SearchRequest {
            page: 1,
            page_size: 10,
            filters: SearchFilters {
                keywords: String::new(),
                impact: vec![],
                tags: vec![TagFilter { value: "ERC4626".into() }],
                protocol_categories: vec![],
                languages: vec![LanguageFilter { value: "Solidity".into() }],
                sort_field: "Quality".into(),
                sort_direction: "Desc".into(),
            },
        };
        let json: serde_json::Value = serde_json::to_value(&req).unwrap();

        // tags must be [{value: "ERC4626"}]
        let tags = json["filters"]["tags"].as_array().unwrap();
        assert_eq!(tags[0]["value"], "ERC4626");

        // languages must be [{value: "Solidity"}]
        let langs = json["filters"]["languages"].as_array().unwrap();
        assert_eq!(langs[0]["value"], "Solidity");
    }

    #[test]
    fn search_request_uses_camel_case_field_names() {
        let req = SearchRequest {
            page: 1,
            page_size: 20,
            filters: SearchFilters {
                keywords: "test".into(),
                impact: vec!["HIGH".into()],
                tags: vec![],
                protocol_categories: vec!["Lending".into()],
                languages: vec![LanguageFilter { value: "Solidity".into() }],
                sort_field: "Quality".into(),
                sort_direction: "Desc".into(),
            },
        };
        let json: serde_json::Value = serde_json::to_value(&req).unwrap();

        assert!(json.get("pageSize").is_some(), "pageSize must be camelCase");
        assert!(json["filters"].get("sortField").is_some(), "sortField must be camelCase");
        assert!(json["filters"].get("sortDirection").is_some(), "sortDirection must be camelCase");
        assert!(json["filters"].get("protocolCategories").is_some(), "protocolCategories must be camelCase");
    }

    #[test]
    fn search_request_skips_empty_optional_filters() {
        let req = SearchRequest {
            page: 1,
            page_size: 10,
            filters: SearchFilters {
                keywords: String::new(),
                impact: vec![],
                tags: vec![],
                protocol_categories: vec![],
                languages: vec![LanguageFilter { value: "Solidity".into() }],
                sort_field: "Quality".into(),
                sort_direction: "Desc".into(),
            },
        };
        let json: serde_json::Value = serde_json::to_value(&req).unwrap();

        assert!(json["filters"].get("keywords").is_none(), "empty keywords should be skipped");
        assert!(json["filters"].get("impact").is_none(), "empty impact should be skipped");
        assert!(json["filters"].get("tags").is_none(), "empty tags should be skipped");
        assert!(json["filters"].get("protocolCategories").is_none(), "empty categories should be skipped");
    }

    // -- Finding deserialization (API response) --

    #[test]
    fn finding_deserializes_from_api_response() {
        let api_json = serde_json::json!({
            "slug": "test-finding-cyfrin-protocol-markdown",
            "title": "Test Finding",
            "impact": "HIGH",
            "quality_score": 4.5,
            "firm_name": "Cyfrin",
            "protocol_name": "Test Protocol",
            "summary": "A test finding summary",
            "content": "Full content here",
            "issues_issuetagscore": [
                {"tags_tag": {"title": "Reentrancy"}},
                {"tags_tag": {"title": "ERC4626"}}
            ],
            "protocols_protocol": {
                "name": "Test Protocol",
                "protocols_protocolcategoryscore": [
                    {"protocols_protocolcategory": {"title": "Lending"}, "score": 1}
                ]
            }
        });
        let finding: Finding = serde_json::from_value(api_json).unwrap();

        assert_eq!(finding.slug, "test-finding-cyfrin-protocol-markdown");
        assert_eq!(finding.impact, "HIGH");
        assert_eq!(finding.firm_name, "Cyfrin");
        assert_eq!(finding.issues_issuetagscore.len(), 2);
    }

    #[test]
    fn finding_handles_missing_optional_fields() {
        let minimal = serde_json::json!({
            "slug": "minimal",
            "title": "Minimal",
            "impact": "MEDIUM"
        });
        let finding: Finding = serde_json::from_value(minimal).unwrap();

        assert_eq!(finding.slug, "minimal");
        assert!(finding.content.is_none());
        assert!(finding.protocols_protocol.is_none());
        assert!(finding.issues_issuetagscore.is_empty());
    }

    // -- CompactFinding::from --

    #[test]
    fn compact_finding_extracts_tags_from_nested_structure() {
        let finding = Finding {
            slug: "test".into(),
            title: "Test".into(),
            impact: "HIGH".into(),
            quality_score: 5.0,
            firm_name: "Cyfrin".into(),
            protocol_name: "Proto".into(),
            summary: None,
            content: None,
            issues_issuetagscore: vec![
                serde_json::json!({"tags_tag": {"title": "Reentrancy"}}),
                serde_json::json!({"tags_tag": {"title": "Flash Loan"}}),
            ],
            protocols_protocol: None,
        };
        let compact = CompactFinding::from(&finding);

        assert_eq!(compact.tags, vec!["Reentrancy", "Flash Loan"]);
    }

    #[test]
    fn compact_finding_extracts_category_from_nested_protocol() {
        let finding = Finding {
            slug: "test".into(),
            title: "Test".into(),
            impact: "HIGH".into(),
            quality_score: 5.0,
            firm_name: "Cyfrin".into(),
            protocol_name: "Proto".into(),
            summary: None,
            content: None,
            issues_issuetagscore: vec![],
            protocols_protocol: Some(serde_json::json!({
                "protocols_protocolcategoryscore": [
                    {"protocols_protocolcategory": {"title": "Dexes"}, "score": 1}
                ]
            })),
        };
        let compact = CompactFinding::from(&finding);

        assert_eq!(compact.category, "Dexes");
    }

    #[test]
    fn compact_finding_defaults_empty_when_no_tags_or_category() {
        let finding = Finding {
            slug: "test".into(),
            title: "Test".into(),
            impact: "MEDIUM".into(),
            quality_score: 3.0,
            firm_name: "".into(),
            protocol_name: "".into(),
            summary: None,
            content: None,
            issues_issuetagscore: vec![],
            protocols_protocol: None,
        };
        let compact = CompactFinding::from(&finding);

        assert!(compact.tags.is_empty());
        assert_eq!(compact.category, "");
    }

    // -- make_cache_key --

    #[test]
    fn cache_key_is_deterministic() {
        let req = SearchRequest {
            page: 1,
            page_size: 10,
            filters: SearchFilters {
                keywords: "test".into(),
                impact: vec!["HIGH".into()],
                tags: vec![],
                protocol_categories: vec![],
                languages: vec![LanguageFilter { value: "Solidity".into() }],
                sort_field: "Quality".into(),
                sort_direction: "Desc".into(),
            },
        };
        let key1 = make_cache_key("search", &req).unwrap();
        let key2 = make_cache_key("search", &req).unwrap();
        assert_eq!(key1, key2);
        assert!(key1.starts_with("search:"));
    }

    #[test]
    fn cache_key_differs_for_different_inputs() {
        let req1 = SearchRequest {
            page: 1,
            page_size: 10,
            filters: SearchFilters {
                keywords: "reentrancy".into(),
                impact: vec![],
                tags: vec![],
                protocol_categories: vec![],
                languages: vec![LanguageFilter { value: "Solidity".into() }],
                sort_field: "Quality".into(),
                sort_direction: "Desc".into(),
            },
        };
        let req2 = SearchRequest {
            page: 1,
            page_size: 10,
            filters: SearchFilters {
                keywords: "flash loan".into(),
                impact: vec![],
                tags: vec![],
                protocol_categories: vec![],
                languages: vec![LanguageFilter { value: "Solidity".into() }],
                sort_field: "Quality".into(),
                sort_direction: "Desc".into(),
            },
        };
        let key1 = make_cache_key("search", &req1).unwrap();
        let key2 = make_cache_key("search", &req2).unwrap();
        assert_ne!(key1, key2);
    }

    // -- TtlCache --

    #[tokio::test]
    async fn cache_returns_none_for_missing_key() {
        let cache = TtlCache::new();
        assert!(cache.get("missing").await.is_none());
    }

    #[tokio::test]
    async fn cache_stores_and_retrieves_value() {
        let cache = TtlCache::new();
        cache
            .set("key".into(), serde_json::json!("value"), Duration::from_secs(60))
            .await;
        let result = cache.get("key").await;
        assert_eq!(result, Some(serde_json::json!("value")));
    }

    #[tokio::test]
    async fn cache_returns_none_after_expiry() {
        let cache = TtlCache::new();
        cache
            .set("key".into(), serde_json::json!("value"), Duration::from_millis(1))
            .await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(cache.get("key").await.is_none());
    }

    // -- Integration tests (require CYFRIN_API_KEY, hit live API) --

    #[tokio::test]
    #[ignore]
    async fn live_search_returns_results() {
        let api_key = std::env::var("CYFRIN_API_KEY").expect("CYFRIN_API_KEY required");
        let client = SoloditClient::new(&api_key).unwrap();
        let result = client
            .search_findings("reentrancy", None, None, None, None, "Quality", "Desc", 1, 5)
            .await
            .unwrap();

        let count = result["count"].as_u64().unwrap_or(0);
        assert!(count > 0, "Expected results for 'reentrancy' search");
    }

    #[tokio::test]
    #[ignore]
    async fn live_search_with_tag_filter() {
        let api_key = std::env::var("CYFRIN_API_KEY").expect("CYFRIN_API_KEY required");
        let client = SoloditClient::new(&api_key).unwrap();
        let result = client
            .search_findings(
                "",
                Some(vec!["HIGH".into()]),
                Some(vec!["ERC4626".into()]),
                None,
                None,
                "Quality",
                "Desc",
                1,
                5,
            )
            .await
            .unwrap();

        let findings = result["findings"].as_array().unwrap();
        assert!(!findings.is_empty(), "Expected ERC4626+HIGH results");
    }

    // NOTE: /tags and /protocol-categories endpoints don't exist in the Solodit API (404).
    // list_tags and list_protocol_categories always use the hardcoded fallback.
    // No live tests for those — the fallback path is covered by unit tests below.

    #[test]
    fn fallback_tags_returns_known_tags() {
        // Ensures the fallback list stays non-empty and covers key audit categories
        let tags = known_tags();
        assert!(tags.len() > 30);
        assert!(tags.contains(&"Reentrancy"));
        assert!(tags.contains(&"ERC4626"));
        assert!(tags.contains(&"Flash Loan"));
        assert!(tags.contains(&"Oracle"));
    }

    #[test]
    fn fallback_categories_returns_known_categories() {
        let cats = known_protocol_categories();
        assert!(cats.len() > 10);
        assert!(cats.contains(&"Lending"));
        assert!(cats.contains(&"Dexes"));
        assert!(cats.contains(&"Bridge"));
    }

    #[tokio::test]
    #[ignore]
    async fn live_search_fails_with_bad_key() {
        let client = SoloditClient::new("invalid_key").unwrap();
        let result = client
            .search_findings("reentrancy", None, None, None, None, "Quality", "Desc", 1, 1)
            .await;
        assert!(result.is_err(), "Bad API key should fail on search");
    }
}
