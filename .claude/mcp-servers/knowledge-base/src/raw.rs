use anyhow::Result;
use chrono::Utc;

use crate::fingerprint::fingerprint;
use crate::store::Store;
use crate::types::{
    CurationStatus, EntrySource, Impact, KbEntry, QueryParams, RawCacheEnvelope,
};

/// Default TTL for raw cache entries (5 minutes, matches Solodit MCP).
const DEFAULT_TTL_SECS: u64 = 300;

/// Ingestion input — a single finding as received from the Solodit MCP.
/// Mirror struct with zero coupling to the Solodit crate.
#[derive(Debug, Clone)]
pub struct IngestFinding {
    pub slug: String,
    pub title: String,
    pub impact: String,
    pub quality_score: f64,
    pub firm: String,
    pub protocol: String,
    pub tags: Vec<String>,
    pub category: String,
    pub summary: Option<String>,
    pub content: Option<String>,
}

/// Layer 1 — Raw cache operations.
///
/// Handles ingestion from Solodit search results, TTL-based deduplication,
/// and forced refresh. All data is persisted to `KnowledgeBase/raw/`.
pub struct RawCache<'a> {
    store: &'a Store,
}

impl<'a> RawCache<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self { store }
    }

    /// Ingest findings from a Solodit search into the raw cache.
    ///
    /// - Computes fingerprint from query params
    /// - Skips if cache is fresh (within TTL)
    /// - Converts findings to KbEntry and writes to disk
    ///
    /// Returns the number of entries ingested (0 if cache was fresh).
    pub fn ingest(
        &self,
        params: &QueryParams,
        findings: Vec<IngestFinding>,
    ) -> Result<usize> {
        self.ingest_with_ttl(params, findings, DEFAULT_TTL_SECS)
    }

    /// Ingest with a custom TTL (useful for testing).
    pub fn ingest_with_ttl(
        &self,
        params: &QueryParams,
        findings: Vec<IngestFinding>,
        ttl_secs: u64,
    ) -> Result<usize> {
        let fp = fingerprint(params);

        // Check if cache is fresh
        if let Some(existing) = self.store.read_raw(&fp)? {
            if !existing.is_expired() {
                tracing::debug!("raw cache hit for fingerprint {}, skipping ingestion", fp);
                return Ok(0);
            }
        }

        let entries: Vec<KbEntry> = findings
            .into_iter()
            .filter_map(|f| to_kb_entry(f))
            .collect();

        let count = entries.len();

        let envelope = RawCacheEnvelope {
            fingerprint: fp,
            query_params: params.clone(),
            fetched_at: Utc::now(),
            ttl_secs,
            entries,
        };

        self.store.write_raw(&envelope)?;
        Ok(count)
    }

    /// Check if a query is cached and still fresh.
    pub fn is_cached(&self, params: &QueryParams) -> Result<bool> {
        let fp = fingerprint(params);
        match self.store.read_raw(&fp)? {
            Some(env) => Ok(!env.is_expired()),
            None => Ok(false),
        }
    }

    /// Force refresh: delete the cached entry for this fingerprint so the
    /// next ingestion writes fresh data regardless of TTL.
    pub fn invalidate(&self, params: &QueryParams) -> Result<()> {
        let fp = fingerprint(params);
        self.store.delete_raw(&fp)
    }

    /// Collect all entries across all raw cache files (expired or not).
    /// Used by the curation layer to build the curated index.
    pub fn all_entries(&self) -> Result<Vec<KbEntry>> {
        let envelopes = self.store.list_raw()?;
        let entries = envelopes
            .into_iter()
            .flat_map(|env| env.entries)
            .collect();
        Ok(entries)
    }

    /// Collect entries only from non-expired cache files.
    pub fn fresh_entries(&self) -> Result<Vec<KbEntry>> {
        let envelopes = self.store.list_raw()?;
        let entries = envelopes
            .into_iter()
            .filter(|env| !env.is_expired())
            .flat_map(|env| env.entries)
            .collect();
        Ok(entries)
    }

    /// Evict all expired raw cache files from disk.
    pub fn evict_expired(&self) -> Result<usize> {
        let envelopes = self.store.list_raw()?;
        let mut evicted = 0;
        for env in envelopes {
            if env.is_expired() {
                self.store.delete_raw(&env.fingerprint)?;
                evicted += 1;
            }
        }
        Ok(evicted)
    }
}

/// Convert an ingestion finding to a KbEntry.
/// Returns None if the impact is not HIGH or MEDIUM (severity guardrail).
fn to_kb_entry(f: IngestFinding) -> Option<KbEntry> {
    let impact = match f.impact.to_uppercase().as_str() {
        "HIGH" => Impact::High,
        "MEDIUM" => Impact::Medium,
        _ => {
            tracing::warn!(
                "dropping finding '{}' with unsupported impact '{}'",
                f.slug,
                f.impact
            );
            return None;
        }
    };

    let id = format!("solodit:{}", f.slug);

    Some(KbEntry {
        id,
        slug: f.slug,
        title: f.title,
        impact,
        quality_score: f.quality_score,
        firm: f.firm,
        protocol: f.protocol,
        tags: f.tags,
        category: f.category,
        summary: f.summary,
        content: f.content,
        source: EntrySource::Solodit,
        curation: CurationStatus::Unreviewed,
        relevance_score: 0.0,
        confidence: 0.0,
        ingested_at: Utc::now(),
        last_curated_at: None,
        auditor_notes: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_store() -> (TempDir, Store) {
        let tmp = TempDir::new().unwrap();
        let store = Store::new(tmp.path().to_path_buf()).unwrap();
        (tmp, store)
    }

    fn sample_params() -> QueryParams {
        QueryParams {
            keywords: "reentrancy".to_string(),
            impact: vec!["HIGH".to_string(), "MEDIUM".to_string()],
            tags: vec!["Reentrancy".to_string()],
            protocol_categories: vec![],
            min_quality: None,
        }
    }

    fn sample_finding(slug: &str, impact: &str) -> IngestFinding {
        IngestFinding {
            slug: slug.to_string(),
            title: format!("Finding: {}", slug),
            impact: impact.to_string(),
            quality_score: 4.0,
            firm: "TestFirm".to_string(),
            protocol: "TestProtocol".to_string(),
            tags: vec!["Reentrancy".to_string()],
            category: "Lending".to_string(),
            summary: Some("Test summary".to_string()),
            content: None,
        }
    }

    #[test]
    fn ingest_writes_to_raw_cache() {
        let (_tmp, store) = temp_store();
        let raw = RawCache::new(&store);
        let params = sample_params();
        let findings = vec![sample_finding("test-1", "HIGH")];

        let count = raw.ingest(&params, findings).unwrap();
        assert_eq!(count, 1);

        // Verify it's cached
        assert!(raw.is_cached(&params).unwrap());
    }

    #[test]
    fn ingest_skips_when_cache_fresh() {
        let (_tmp, store) = temp_store();
        let raw = RawCache::new(&store);
        let params = sample_params();

        // First ingestion
        let count1 = raw.ingest(&params, vec![sample_finding("a", "HIGH")]).unwrap();
        assert_eq!(count1, 1);

        // Second ingestion — should skip (cache fresh)
        let count2 = raw.ingest(&params, vec![sample_finding("b", "HIGH")]).unwrap();
        assert_eq!(count2, 0);
    }

    #[test]
    fn ingest_overwrites_expired_cache() {
        let (_tmp, store) = temp_store();
        let raw = RawCache::new(&store);
        let params = sample_params();

        // Ingest with 0s TTL (immediately expired)
        let count1 = raw.ingest_with_ttl(&params, vec![sample_finding("old", "HIGH")], 0).unwrap();
        assert_eq!(count1, 1);

        // Should not be cached (expired)
        assert!(!raw.is_cached(&params).unwrap());

        // Re-ingest — should write new data
        let count2 = raw.ingest(&params, vec![sample_finding("new", "HIGH")]).unwrap();
        assert_eq!(count2, 1);
    }

    #[test]
    fn ingest_filters_invalid_impact() {
        let (_tmp, store) = temp_store();
        let raw = RawCache::new(&store);
        let params = sample_params();
        let findings = vec![
            sample_finding("high-one", "HIGH"),
            sample_finding("low-one", "LOW"),
            sample_finding("gas-one", "GAS"),
            sample_finding("med-one", "MEDIUM"),
        ];

        let count = raw.ingest(&params, findings).unwrap();
        assert_eq!(count, 2); // only HIGH and MEDIUM survive

        let entries = raw.all_entries().unwrap();
        let slugs: Vec<&str> = entries.iter().map(|e| e.slug.as_str()).collect();
        assert!(slugs.contains(&"high-one"));
        assert!(slugs.contains(&"med-one"));
        assert!(!slugs.contains(&"low-one"));
        assert!(!slugs.contains(&"gas-one"));
    }

    #[test]
    fn ingest_sets_correct_defaults() {
        let (_tmp, store) = temp_store();
        let raw = RawCache::new(&store);
        let params = sample_params();

        raw.ingest(&params, vec![sample_finding("test", "HIGH")]).unwrap();
        let entries = raw.all_entries().unwrap();

        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.id, "solodit:test");
        assert_eq!(entry.source, EntrySource::Solodit);
        assert_eq!(entry.curation, CurationStatus::Unreviewed);
        assert_eq!(entry.relevance_score, 0.0);
        assert_eq!(entry.confidence, 0.0);
        assert!(entry.last_curated_at.is_none());
    }

    #[test]
    fn invalidate_removes_cache() {
        let (_tmp, store) = temp_store();
        let raw = RawCache::new(&store);
        let params = sample_params();

        raw.ingest(&params, vec![sample_finding("x", "HIGH")]).unwrap();
        assert!(raw.is_cached(&params).unwrap());

        raw.invalidate(&params).unwrap();
        assert!(!raw.is_cached(&params).unwrap());
    }

    #[test]
    fn all_entries_across_multiple_queries() {
        let (_tmp, store) = temp_store();
        let raw = RawCache::new(&store);

        let params1 = QueryParams {
            keywords: "reentrancy".to_string(),
            impact: vec!["HIGH".to_string()],
            tags: vec![],
            protocol_categories: vec![],
            min_quality: None,
        };
        let params2 = QueryParams {
            keywords: "overflow".to_string(),
            impact: vec!["HIGH".to_string()],
            tags: vec![],
            protocol_categories: vec![],
            min_quality: None,
        };

        raw.ingest(&params1, vec![sample_finding("f1", "HIGH")]).unwrap();
        raw.ingest(&params2, vec![sample_finding("f2", "MEDIUM")]).unwrap();

        let all = raw.all_entries().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn fresh_entries_excludes_expired() {
        let (_tmp, store) = temp_store();
        let raw = RawCache::new(&store);

        let params1 = QueryParams {
            keywords: "fresh".to_string(),
            impact: vec!["HIGH".to_string()],
            tags: vec![],
            protocol_categories: vec![],
            min_quality: None,
        };
        let params2 = QueryParams {
            keywords: "stale".to_string(),
            impact: vec!["HIGH".to_string()],
            tags: vec![],
            protocol_categories: vec![],
            min_quality: None,
        };

        // Fresh entry (default TTL)
        raw.ingest(&params1, vec![sample_finding("fresh-1", "HIGH")]).unwrap();
        // Expired entry (0s TTL)
        raw.ingest_with_ttl(&params2, vec![sample_finding("stale-1", "HIGH")], 0).unwrap();

        let fresh = raw.fresh_entries().unwrap();
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].slug, "fresh-1");
    }

    #[test]
    fn evict_expired_removes_stale_files() {
        let (_tmp, store) = temp_store();
        let raw = RawCache::new(&store);

        let fresh_params = QueryParams {
            keywords: "keep".to_string(),
            impact: vec![],
            tags: vec![],
            protocol_categories: vec![],
            min_quality: None,
        };
        let stale_params = QueryParams {
            keywords: "remove".to_string(),
            impact: vec![],
            tags: vec![],
            protocol_categories: vec![],
            min_quality: None,
        };

        raw.ingest(&fresh_params, vec![sample_finding("keep-1", "HIGH")]).unwrap();
        raw.ingest_with_ttl(&stale_params, vec![sample_finding("rm-1", "HIGH")], 0).unwrap();

        let evicted = raw.evict_expired().unwrap();
        assert_eq!(evicted, 1);

        // Only the fresh one remains
        let all = raw.all_entries().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].slug, "keep-1");
    }

    #[test]
    fn is_cached_false_when_missing() {
        let (_tmp, store) = temp_store();
        let raw = RawCache::new(&store);
        assert!(!raw.is_cached(&sample_params()).unwrap());
    }
}
