pub mod curate;
pub mod fingerprint;
pub mod query;
pub mod raw;
pub mod store;
pub mod types;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::curate::Curator;
use crate::query::QueryEngine;
use crate::raw::RawCache;
use crate::store::Store;

// Re-exports for convenience — consumers can `use knowledge_base::KbEntry` etc.
pub use crate::raw::IngestFinding;
pub use crate::types::{
    CurationContext, CurationStats, CurationStatus, EntrySource, FeedbackItem, Impact, KbEntry,
    KbQuery, KbQueryResult, QueryParams,
};

/// Knowledge Base — local curated vulnerability index.
///
/// Sits between the Solodit MCP server and the agent pipeline.
/// Three layers: raw cache → curated index → agent consumption.
///
/// # Usage
///
/// ```ignore
/// let kb = KnowledgeBase::new("KnowledgeBase".into())?;
///
/// // Ingest from Solodit search results
/// kb.ingest(&params, findings)?;
///
/// // Run curation pass
/// kb.curate(&context)?;
///
/// // Query for agent consumption
/// let result = kb.query(&query)?;
///
/// // Apply auditor feedback
/// kb.apply_feedback(&feedback)?;
/// ```
pub struct KnowledgeBase {
    store: Store,
}

impl KnowledgeBase {
    /// Initialize KB with a directory path. Creates subdirectories if needed.
    pub fn new(base_dir: PathBuf) -> Result<Self> {
        let store = Store::new(base_dir).context("initializing KB store")?;
        Ok(Self { store })
    }

    // -- Layer 1: Raw Cache --

    /// Ingest findings from a Solodit search into the raw cache.
    ///
    /// Computes a fingerprint from query params, skips if cache is fresh
    /// (within TTL), filters out non-HIGH/MEDIUM findings, and writes to disk.
    /// Returns the number of entries ingested (0 if cache was fresh).
    pub fn ingest(
        &self,
        params: &QueryParams,
        findings: Vec<IngestFinding>,
    ) -> Result<usize> {
        RawCache::new(&self.store).ingest(params, findings)
    }

    /// Check if a query is cached and still fresh.
    pub fn is_cached(&self, params: &QueryParams) -> Result<bool> {
        RawCache::new(&self.store).is_cached(params)
    }

    /// Invalidate cached results for a query (force refresh on next ingest).
    pub fn invalidate(&self, params: &QueryParams) -> Result<()> {
        RawCache::new(&self.store).invalidate(params)
    }

    /// Evict all expired raw cache files from disk. Returns count evicted.
    pub fn evict_expired(&self) -> Result<usize> {
        RawCache::new(&self.store).evict_expired()
    }

    // -- Layer 2: Curation --

    /// Run a curation pass: collect all raw + seed entries, deduplicate,
    /// score, and write curated severity files.
    ///
    /// Preserves existing curation status for previously curated entries.
    /// `context` is accepted for future scoring (Task 7) but currently unused.
    pub fn curate(&self, context: &CurationContext) -> Result<CurationStats> {
        let raw = RawCache::new(&self.store);
        let curator = Curator::new(&self.store);

        // Collect all entries: raw cache + seeds
        let mut all_entries = raw.all_entries()?;
        let seed_entries = self.collect_seed_entries()?;
        all_entries.extend(seed_entries);

        curator.curate(all_entries, context)
    }

    /// Update curation status for a single entry by ID.
    pub fn set_curation(
        &self,
        entry_id: &str,
        status: CurationStatus,
        notes: Option<String>,
    ) -> Result<bool> {
        Curator::new(&self.store).set_curation(entry_id, status, notes)
    }

    /// Apply bulk feedback from auditor review. Returns count of entries updated.
    pub fn apply_feedback(&self, feedback: &[FeedbackItem]) -> Result<usize> {
        Curator::new(&self.store).apply_feedback(feedback)
    }

    // -- Layer 3: Agent Consumption --

    /// Query the curated index for agent consumption.
    ///
    /// Reads HIGH → MEDIUM, filters by tags/categories/keywords,
    /// excludes noise, and truncates to context budget.
    pub fn query(&self, q: &KbQuery) -> Result<KbQueryResult> {
        QueryEngine::new(&self.store).query(q)
    }

    // -- Seeds --

    /// Import a seed file from a given path into `KnowledgeBase/seeds/`.
    /// Returns the number of entries imported.
    pub fn import_seed_file(&self, path: &Path) -> Result<usize> {
        let data = std::fs::read_to_string(path)
            .with_context(|| format!("reading seed file {}", path.display()))?;
        let seed: crate::types::SeedFile = serde_json::from_str(&data)
            .with_context(|| format!("parsing seed file {}", path.display()))?;
        let count = seed.entries.len();
        self.store.write_seed(&seed)?;
        Ok(count)
    }

    /// List all seed file paths.
    pub fn list_seeds(&self) -> Result<Vec<PathBuf>> {
        self.store.list_seed_paths()
    }

    // -- Internal --

    /// Collect all entries from seed files, ensuring correct source and ID.
    fn collect_seed_entries(&self) -> Result<Vec<KbEntry>> {
        let seeds = self.store.read_seeds()?;
        let entries = seeds
            .into_iter()
            .flat_map(|sf| sf.entries)
            .map(|mut e| {
                // Ensure seed entries have correct source
                if e.source != EntrySource::Seed {
                    e.source = EntrySource::Seed;
                }
                // Ensure ID has seed prefix
                if !e.id.starts_with("seed:") {
                    e.id = format!("seed:{}", e.slug);
                }
                e
            })
            .collect();
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;

    fn temp_kb() -> (TempDir, KnowledgeBase) {
        let tmp = TempDir::new().unwrap();
        let kb = KnowledgeBase::new(tmp.path().to_path_buf()).unwrap();
        (tmp, kb)
    }

    fn sample_params(keywords: &str) -> QueryParams {
        QueryParams {
            keywords: keywords.to_string(),
            impact: vec!["HIGH".to_string(), "MEDIUM".to_string()],
            tags: vec![],
            protocol_categories: vec![],
            min_quality: None,
        }
    }

    fn sample_finding(slug: &str, impact: &str, tags: &[&str], category: &str) -> IngestFinding {
        IngestFinding {
            slug: slug.to_string(),
            title: format!("Finding: {}", slug),
            impact: impact.to_string(),
            quality_score: 4.0,
            firm: "Firm".to_string(),
            protocol: "Proto".to_string(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            category: category.to_string(),
            summary: Some(format!("Summary for {}", slug)),
            content: None,
        }
    }

    fn sample_seed_json(domain: &str, entries: &[(&str, &str)]) -> String {
        let entry_objs: Vec<String> = entries
            .iter()
            .map(|(slug, impact)| {
                format!(
                    r#"{{
                        "id": "seed:{slug}",
                        "slug": "{slug}",
                        "title": "Seed: {slug}",
                        "impact": "{impact}",
                        "quality_score": 5.0,
                        "firm": "",
                        "protocol": "",
                        "tags": [],
                        "category": "{domain}",
                        "summary": "Seed summary",
                        "content": null,
                        "source": "seed",
                        "curation": "critical",
                        "relevance_score": 1.0,
                        "confidence": 1.0,
                        "ingested_at": "{now}",
                        "last_curated_at": null,
                        "auditor_notes": null
                    }}"#,
                    slug = slug,
                    impact = impact,
                    domain = domain,
                    now = Utc::now().to_rfc3339(),
                )
            })
            .collect();

        format!(
            r#"{{"domain": "{}", "description": "test seeds", "entries": [{}]}}"#,
            domain,
            entry_objs.join(",")
        )
    }

    // -- End-to-end tests --

    #[test]
    fn e2e_ingest_curate_query() {
        let (_tmp, kb) = temp_kb();

        // Ingest two queries
        let params1 = sample_params("reentrancy");
        let findings1 = vec![
            sample_finding("reent-1", "HIGH", &["Reentrancy"], "Lending"),
            sample_finding("reent-2", "MEDIUM", &["Reentrancy"], "Lending"),
        ];
        assert_eq!(kb.ingest(&params1, findings1).unwrap(), 2);

        let params2 = sample_params("oracle");
        let findings2 = vec![
            sample_finding("oracle-1", "HIGH", &["Oracle", "Chainlink"], "Dexes"),
        ];
        assert_eq!(kb.ingest(&params2, findings2).unwrap(), 1);

        // Curate
        let stats = kb.curate(&CurationContext::default()).unwrap();
        assert_eq!(stats.total_processed, 3);
        assert_eq!(stats.high_count, 2);
        assert_eq!(stats.medium_count, 1);

        // Query all
        let result = kb
            .query(&KbQuery {
                max_entries: 100,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(result.entries.len(), 3);

        // Query filtered by tag
        let result = kb
            .query(&KbQuery {
                tags: vec!["Reentrancy".to_string()],
                max_entries: 100,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(result.entries.len(), 2);

        // Query filtered by category
        let result = kb
            .query(&KbQuery {
                categories: vec!["Dexes".to_string()],
                max_entries: 100,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].slug, "oracle-1");
    }

    #[test]
    fn e2e_feedback_loop() {
        let (_tmp, kb) = temp_kb();

        // Ingest + curate
        let params = sample_params("test");
        let findings = vec![
            sample_finding("confirmed-bug", "HIGH", &[], ""),
            sample_finding("false-alarm", "HIGH", &[], ""),
            sample_finding("needs-escalation", "MEDIUM", &[], ""),
        ];
        kb.ingest(&params, findings).unwrap();
        kb.curate(&CurationContext::default()).unwrap();

        // Apply feedback
        let feedback = vec![
            FeedbackItem {
                entry_id: "solodit:confirmed-bug".to_string(),
                new_status: CurationStatus::Useful,
                reason: Some("valid — matches our vault pattern".to_string()),
            },
            FeedbackItem {
                entry_id: "solodit:false-alarm".to_string(),
                new_status: CurationStatus::Noise,
                reason: Some("admin has a fee cap".to_string()),
            },
            FeedbackItem {
                entry_id: "solodit:needs-escalation".to_string(),
                new_status: CurationStatus::Critical,
                reason: Some("attacker controls both params".to_string()),
            },
        ];
        let updated = kb.apply_feedback(&feedback).unwrap();
        assert_eq!(updated, 3);

        // Query — noise should be excluded
        let result = kb
            .query(&KbQuery {
                max_entries: 100,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(result.entries.len(), 2);

        // Verify curation persists
        let confirmed = result.entries.iter().find(|e| e.slug == "confirmed-bug").unwrap();
        assert_eq!(confirmed.curation, CurationStatus::Useful);
        assert_eq!(confirmed.confidence, 0.80);

        let escalated = result.entries.iter().find(|e| e.slug == "needs-escalation").unwrap();
        assert_eq!(escalated.curation, CurationStatus::Critical);
        assert_eq!(escalated.confidence, 0.95);
    }

    #[test]
    fn e2e_curation_survives_reingestion() {
        let (_tmp, kb) = temp_kb();
        let params = sample_params("test");

        // First pass: ingest + curate + mark as useful
        kb.ingest(&params, vec![sample_finding("bug-1", "HIGH", &[], "")]).unwrap();
        kb.curate(&CurationContext::default()).unwrap();
        kb.set_curation("solodit:bug-1", CurationStatus::Useful, Some("good".to_string())).unwrap();

        // Invalidate cache and re-ingest (simulates refresh)
        kb.invalidate(&params).unwrap();
        kb.ingest(&params, vec![sample_finding("bug-1", "HIGH", &[], "")]).unwrap();

        // Re-curate — should preserve Useful status
        kb.curate(&CurationContext::default()).unwrap();

        let result = kb
            .query(&KbQuery { max_entries: 100, ..Default::default() })
            .unwrap();
        assert_eq!(result.entries[0].curation, CurationStatus::Useful);
        assert_eq!(result.entries[0].auditor_notes.as_deref(), Some("good"));
    }

    #[test]
    fn e2e_seeds_merge_with_solodit() {
        let (tmp, kb) = temp_kb();

        // Write a seed file
        let seed_json = sample_seed_json("Yield", &[("inflation-attack", "HIGH")]);
        let seed_path = tmp.path().join("external-seed.json");
        std::fs::write(&seed_path, &seed_json).unwrap();

        // Import it
        let imported = kb.import_seed_file(&seed_path).unwrap();
        assert_eq!(imported, 1);

        // Ingest Solodit findings
        let params = sample_params("vault");
        kb.ingest(
            &params,
            vec![sample_finding("vault-bug", "HIGH", &["ERC4626"], "Yield")],
        )
        .unwrap();

        // Curate — should include both seed and Solodit entries
        let stats = kb.curate(&CurationContext::default()).unwrap();
        assert_eq!(stats.high_count, 2);

        // Query
        let result = kb
            .query(&KbQuery { max_entries: 100, ..Default::default() })
            .unwrap();
        assert_eq!(result.entries.len(), 2);

        // Seed entry should rank first (Critical curation)
        assert_eq!(result.entries[0].slug, "inflation-attack");
        assert_eq!(result.entries[0].curation, CurationStatus::Critical);
        assert_eq!(result.entries[0].source, EntrySource::Seed);
    }

    #[test]
    fn e2e_budget_truncation_preserves_priority() {
        let (_tmp, kb) = temp_kb();

        let params = sample_params("mixed");
        let findings = vec![
            sample_finding("critical-a", "HIGH", &["Reentrancy"], "Lending"),
            sample_finding("normal-b", "HIGH", &["Oracle"], "Lending"),
            sample_finding("normal-c", "MEDIUM", &["ERC4626"], "Yield"),
            sample_finding("normal-d", "MEDIUM", &["Rounding"], "Yield"),
        ];
        kb.ingest(&params, findings).unwrap();
        kb.curate(&CurationContext::default()).unwrap();

        // Mark one as critical
        kb.set_curation("solodit:critical-a", CurationStatus::Critical, None).unwrap();

        // Query with budget of 2
        let result = kb
            .query(&KbQuery { max_entries: 2, ..Default::default() })
            .unwrap();

        assert_eq!(result.entries.len(), 2);
        assert!(result.truncated);
        assert_eq!(result.total_available, 4);
        // Critical entry should be first, both should be HIGH
        assert_eq!(result.entries[0].slug, "critical-a");
        assert_eq!(result.entries[0].impact, Impact::High);
        assert_eq!(result.entries[1].impact, Impact::High);
    }

    #[test]
    fn e2e_evict_expired() {
        let (_tmp, kb) = temp_kb();

        // Ingest with immediate expiry (using raw cache directly)
        let raw = RawCache::new(&kb.store);
        let params = sample_params("stale");
        raw.ingest_with_ttl(&params, vec![sample_finding("old", "HIGH", &[], "")], 0).unwrap();

        // Also ingest a fresh one through the public API
        let fresh_params = sample_params("fresh");
        kb.ingest(&fresh_params, vec![sample_finding("new", "HIGH", &[], "")]).unwrap();

        let evicted = kb.evict_expired().unwrap();
        assert_eq!(evicted, 1);

        // Fresh one should still be there
        assert!(kb.is_cached(&fresh_params).unwrap());
        assert!(!kb.is_cached(&params).unwrap());
    }

    #[test]
    fn list_seeds_after_import() {
        let (tmp, kb) = temp_kb();

        let seed_json = sample_seed_json("Lending", &[("bug-1", "HIGH")]);
        let seed_path = tmp.path().join("lending-seed.json");
        std::fs::write(&seed_path, &seed_json).unwrap();

        kb.import_seed_file(&seed_path).unwrap();

        let paths = kb.list_seeds().unwrap();
        assert_eq!(paths.len(), 1);
    }
}
