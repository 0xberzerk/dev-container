use anyhow::Result;

use crate::store::Store;
use crate::types::{CurationStatus, Impact, KbEntry, KbQuery, KbQueryResult};

/// Layer 3 — Agent consumption: filtered, ranked, budget-truncated queries.
///
/// Reads curated files in severity order (HIGH → MEDIUM), applies filters,
/// and truncates to the caller's context budget.
pub struct QueryEngine<'a> {
    store: &'a Store,
}

impl<'a> QueryEngine<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self { store }
    }

    /// Query the curated index for agent consumption.
    ///
    /// - Reads HIGH entries first, then MEDIUM (severity order)
    /// - Filters by tags, categories, and keywords
    /// - Excludes noise (unless exclude_noise is false)
    /// - Within each severity: Critical curation > Useful > Unreviewed
    /// - Truncates to max_entries (context budget)
    pub fn query(&self, q: &KbQuery) -> Result<KbQueryResult> {
        let mut all_entries = Vec::new();

        // Read in severity order: HIGH first, then MEDIUM
        for impact in &[Impact::High, Impact::Medium] {
            if let Some(file) = self.store.read_curated(impact)? {
                // Curated files are already sorted by curation rank + score
                all_entries.extend(file.entries);
            }
        }

        // Apply filters
        let filtered: Vec<KbEntry> = all_entries
            .into_iter()
            .filter(|e| {
                if q.exclude_noise && e.curation == CurationStatus::Noise {
                    return false;
                }
                if !q.tags.is_empty() && !has_overlap(&e.tags, &q.tags) {
                    return false;
                }
                if !q.categories.is_empty() && !matches_category(&e.category, &q.categories) {
                    return false;
                }
                if !q.keywords.is_empty() && !matches_keywords(e, &q.keywords) {
                    return false;
                }
                true
            })
            .collect();

        let total_available = filtered.len();
        let truncated = total_available > q.max_entries;

        let entries = filtered.into_iter().take(q.max_entries).collect();

        Ok(KbQueryResult {
            entries,
            total_available,
            truncated,
        })
    }
}

/// Check if any entry tag matches any query tag (case-insensitive).
fn has_overlap(entry_tags: &[String], query_tags: &[String]) -> bool {
    entry_tags.iter().any(|et| {
        query_tags
            .iter()
            .any(|qt| et.eq_ignore_ascii_case(qt))
    })
}

/// Check if entry category matches any query category (case-insensitive).
fn matches_category(entry_cat: &str, query_cats: &[String]) -> bool {
    query_cats
        .iter()
        .any(|qc| entry_cat.eq_ignore_ascii_case(qc))
}

/// Check if any keyword appears in the entry's title or summary (case-insensitive).
fn matches_keywords(entry: &KbEntry, keywords: &[String]) -> bool {
    let title_lower = entry.title.to_lowercase();
    let summary_lower = entry
        .summary
        .as_deref()
        .unwrap_or("")
        .to_lowercase();

    keywords.iter().any(|kw| {
        let kw_lower = kw.to_lowercase();
        title_lower.contains(&kw_lower) || summary_lower.contains(&kw_lower)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use chrono::Utc;
    use tempfile::TempDir;

    fn temp_store() -> (TempDir, Store) {
        let tmp = TempDir::new().unwrap();
        let store = Store::new(tmp.path().to_path_buf()).unwrap();
        (tmp, store)
    }

    fn make_entry(
        id: &str,
        impact: Impact,
        quality: f64,
        curation: CurationStatus,
        tags: &[&str],
        category: &str,
    ) -> KbEntry {
        KbEntry {
            id: id.to_string(),
            slug: id.to_string(),
            title: format!("Finding: {}", id),
            impact,
            quality_score: quality,
            firm: "Firm".to_string(),
            protocol: "Proto".to_string(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            category: category.to_string(),
            summary: Some(format!("Summary for {}", id)),
            content: None,
            source: EntrySource::Solodit,
            curation,
            relevance_score: 0.5,
            confidence: 0.5,
            ingested_at: Utc::now(),
            last_curated_at: None,
            auditor_notes: None,
        }
    }

    fn seed_curated(store: &Store, high: Vec<KbEntry>, medium: Vec<KbEntry>) {
        let now = Utc::now();
        store
            .write_curated(&CuratedFile {
                impact: Impact::High,
                last_curated_at: now,
                entries: high,
            })
            .unwrap();
        store
            .write_curated(&CuratedFile {
                impact: Impact::Medium,
                last_curated_at: now,
                entries: medium,
            })
            .unwrap();
    }

    #[test]
    fn query_returns_all_when_no_filters() {
        let (_tmp, store) = temp_store();
        seed_curated(
            &store,
            vec![make_entry("h1", Impact::High, 4.0, CurationStatus::Unreviewed, &["Reentrancy"], "Lending")],
            vec![make_entry("m1", Impact::Medium, 3.0, CurationStatus::Unreviewed, &["ERC4626"], "Yield")],
        );

        let engine = QueryEngine::new(&store);
        let result = engine
            .query(&KbQuery {
                max_entries: 100,
                ..Default::default()
            })
            .unwrap();

        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.total_available, 2);
        assert!(!result.truncated);
    }

    #[test]
    fn query_severity_order_high_before_medium() {
        let (_tmp, store) = temp_store();
        seed_curated(
            &store,
            vec![make_entry("h1", Impact::High, 4.0, CurationStatus::Unreviewed, &[], "")],
            vec![make_entry("m1", Impact::Medium, 5.0, CurationStatus::Critical, &[], "")],
        );

        let engine = QueryEngine::new(&store);
        let result = engine
            .query(&KbQuery {
                max_entries: 100,
                ..Default::default()
            })
            .unwrap();

        // HIGH comes first regardless of curation/score
        assert_eq!(result.entries[0].id, "h1");
        assert_eq!(result.entries[1].id, "m1");
    }

    #[test]
    fn query_filter_by_tags() {
        let (_tmp, store) = temp_store();
        seed_curated(
            &store,
            vec![
                make_entry("reent", Impact::High, 4.0, CurationStatus::Unreviewed, &["Reentrancy"], "Lending"),
                make_entry("oracle", Impact::High, 4.0, CurationStatus::Unreviewed, &["Oracle", "Chainlink"], "Lending"),
            ],
            vec![],
        );

        let engine = QueryEngine::new(&store);
        let result = engine
            .query(&KbQuery {
                tags: vec!["Reentrancy".to_string()],
                max_entries: 100,
                ..Default::default()
            })
            .unwrap();

        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].id, "reent");
    }

    #[test]
    fn query_filter_by_tags_case_insensitive() {
        let (_tmp, store) = temp_store();
        seed_curated(
            &store,
            vec![make_entry("r1", Impact::High, 4.0, CurationStatus::Unreviewed, &["Reentrancy"], "")],
            vec![],
        );

        let engine = QueryEngine::new(&store);
        let result = engine
            .query(&KbQuery {
                tags: vec!["reentrancy".to_string()],
                max_entries: 100,
                ..Default::default()
            })
            .unwrap();

        assert_eq!(result.entries.len(), 1);
    }

    #[test]
    fn query_filter_by_category() {
        let (_tmp, store) = temp_store();
        seed_curated(
            &store,
            vec![
                make_entry("lend", Impact::High, 4.0, CurationStatus::Unreviewed, &[], "Lending"),
                make_entry("dex", Impact::High, 4.0, CurationStatus::Unreviewed, &[], "Dexes"),
            ],
            vec![],
        );

        let engine = QueryEngine::new(&store);
        let result = engine
            .query(&KbQuery {
                categories: vec!["lending".to_string()],
                max_entries: 100,
                ..Default::default()
            })
            .unwrap();

        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].id, "lend");
    }

    #[test]
    fn query_filter_by_keywords() {
        let (_tmp, store) = temp_store();
        seed_curated(
            &store,
            vec![
                make_entry("withdraw-bug", Impact::High, 4.0, CurationStatus::Unreviewed, &[], ""),
                make_entry("deposit-bug", Impact::High, 4.0, CurationStatus::Unreviewed, &[], ""),
            ],
            vec![],
        );

        let engine = QueryEngine::new(&store);
        let result = engine
            .query(&KbQuery {
                keywords: vec!["withdraw".to_string()],
                max_entries: 100,
                ..Default::default()
            })
            .unwrap();

        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].id, "withdraw-bug");
    }

    #[test]
    fn query_excludes_noise_by_default() {
        let (_tmp, store) = temp_store();
        seed_curated(
            &store,
            vec![
                make_entry("good", Impact::High, 4.0, CurationStatus::Useful, &[], ""),
                make_entry("bad", Impact::High, 4.0, CurationStatus::Noise, &[], ""),
            ],
            vec![],
        );

        let engine = QueryEngine::new(&store);
        let result = engine
            .query(&KbQuery {
                max_entries: 100,
                ..Default::default()
            })
            .unwrap();

        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].id, "good");
    }

    #[test]
    fn query_includes_noise_when_requested() {
        let (_tmp, store) = temp_store();
        seed_curated(
            &store,
            vec![
                make_entry("good", Impact::High, 4.0, CurationStatus::Useful, &[], ""),
                make_entry("bad", Impact::High, 4.0, CurationStatus::Noise, &[], ""),
            ],
            vec![],
        );

        let engine = QueryEngine::new(&store);
        let result = engine
            .query(&KbQuery {
                exclude_noise: false,
                max_entries: 100,
                ..Default::default()
            })
            .unwrap();

        assert_eq!(result.entries.len(), 2);
    }

    #[test]
    fn query_truncates_to_budget() {
        let (_tmp, store) = temp_store();
        seed_curated(
            &store,
            vec![
                make_entry("h1", Impact::High, 5.0, CurationStatus::Critical, &[], ""),
                make_entry("h2", Impact::High, 4.0, CurationStatus::Useful, &[], ""),
                make_entry("h3", Impact::High, 3.0, CurationStatus::Unreviewed, &[], ""),
            ],
            vec![
                make_entry("m1", Impact::Medium, 3.0, CurationStatus::Unreviewed, &[], ""),
            ],
        );

        let engine = QueryEngine::new(&store);
        let result = engine
            .query(&KbQuery {
                max_entries: 2,
                ..Default::default()
            })
            .unwrap();

        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.total_available, 4);
        assert!(result.truncated);
        // Should keep highest severity + highest curation rank
        assert_eq!(result.entries[0].id, "h1");
        assert_eq!(result.entries[1].id, "h2");
    }

    #[test]
    fn query_combined_filters() {
        let (_tmp, store) = temp_store();
        seed_curated(
            &store,
            vec![
                make_entry("match", Impact::High, 4.0, CurationStatus::Useful, &["Reentrancy"], "Lending"),
                make_entry("wrong-tag", Impact::High, 4.0, CurationStatus::Useful, &["Oracle"], "Lending"),
                make_entry("wrong-cat", Impact::High, 4.0, CurationStatus::Useful, &["Reentrancy"], "Dexes"),
            ],
            vec![],
        );

        let engine = QueryEngine::new(&store);
        let result = engine
            .query(&KbQuery {
                tags: vec!["Reentrancy".to_string()],
                categories: vec!["Lending".to_string()],
                max_entries: 100,
                ..Default::default()
            })
            .unwrap();

        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].id, "match");
    }

    #[test]
    fn query_empty_curated_returns_empty() {
        let (_tmp, store) = temp_store();
        let engine = QueryEngine::new(&store);
        let result = engine
            .query(&KbQuery::default())
            .unwrap();

        assert_eq!(result.entries.len(), 0);
        assert_eq!(result.total_available, 0);
        assert!(!result.truncated);
    }
}
