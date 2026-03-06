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
#[derive(Debug, Serialize)]
struct CompactFinding {
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

        // Solodit doesn't have a dedicated tags endpoint — fetch from a broad search
        // and collect unique tags, or use a known list
        self.rate_limiter.acquire().await;
        let url = format!("{}/tags", API_BASE);
        let response = self.http.get(&url).send().await;

        let result = match response {
            Ok(resp) if resp.status().is_success() => {
                resp.json::<serde_json::Value>()
                    .await
                    .context("Failed to parse tags response")?
            }
            _ => {
                // Fallback: return known major tags relevant to security auditing
                warn!("Tags endpoint not available, returning known tags");
                serde_json::json!(known_tags())
            }
        };

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
        let response = self.http.get(&url).send().await;

        let result = match response {
            Ok(resp) if resp.status().is_success() => {
                resp.json::<serde_json::Value>()
                    .await
                    .context("Failed to parse categories response")?
            }
            _ => {
                warn!("Categories endpoint not available, returning known categories");
                serde_json::json!(known_protocol_categories())
            }
        };

        self.cache.set(cache_key, result.clone(), METADATA_TTL).await;
        Ok(result)
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
