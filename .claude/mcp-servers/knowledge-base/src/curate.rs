use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;

use crate::store::Store;
use crate::types::{
    CuratedFile, CurationContext, CurationStats, CurationStatus, FeedbackItem, Impact, KbEntry,
};

/// Layer 2 — Curation: scoring, deduplication, severity bucketing.
///
/// Current scoring uses impact + quality_score + curation status only.
/// Context-aware scoring (tag overlap, keyword matching) will be added
/// when the Architect agent provides CurationContext (Task 7).
pub struct Curator<'a> {
    store: &'a Store,
}

impl<'a> Curator<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self { store }
    }

    /// Run a curation pass over all provided entries.
    ///
    /// - Deduplicates by slug (keeps highest quality_score on conflict)
    /// - Preserves existing curation status for known entries
    /// - Scores each entry for relevance and confidence
    /// - Buckets into curated/high.json and curated/medium.json
    /// - Sorts within each bucket: curation rank desc, then score desc
    /// - Excludes noise from output files
    ///
    /// `_context` is accepted for future use but currently ignored.
    pub fn curate(
        &self,
        entries: Vec<KbEntry>,
        _context: &CurationContext,
    ) -> Result<CurationStats> {
        // Load existing curation state so we don't lose human decisions
        let existing = self.load_existing_curation()?;

        // Deduplicate by slug, keeping highest quality_score
        let deduped = deduplicate(entries);

        let mut high_entries = Vec::new();
        let mut medium_entries = Vec::new();
        let mut noise_skipped = 0;

        for mut entry in deduped {
            // Restore existing curation status if this entry was already curated
            if let Some(prev) = existing.get(&entry.id) {
                entry.curation = prev.curation.clone();
                entry.auditor_notes = prev.auditor_notes.clone();
                // Keep the earlier last_curated_at if it exists
                if prev.last_curated_at.is_some() {
                    entry.last_curated_at = prev.last_curated_at;
                }
            }

            // Skip noise
            if entry.curation == CurationStatus::Noise {
                noise_skipped += 1;
                continue;
            }

            // Score
            score_entry(&mut entry);

            match entry.impact {
                Impact::High => high_entries.push(entry),
                Impact::Medium => medium_entries.push(entry),
            }
        }

        // Sort: curation rank desc, then relevance_score desc, then quality_score desc
        sort_entries(&mut high_entries);
        sort_entries(&mut medium_entries);

        let stats = CurationStats {
            total_processed: high_entries.len() + medium_entries.len() + noise_skipped,
            high_count: high_entries.len(),
            medium_count: medium_entries.len(),
            noise_skipped,
        };

        let now = Utc::now();

        self.store.write_curated(&CuratedFile {
            impact: Impact::High,
            last_curated_at: now,
            entries: high_entries,
        })?;

        self.store.write_curated(&CuratedFile {
            impact: Impact::Medium,
            last_curated_at: now,
            entries: medium_entries,
        })?;

        Ok(stats)
    }

    /// Update curation status for a single entry by ID.
    pub fn set_curation(
        &self,
        entry_id: &str,
        status: CurationStatus,
        notes: Option<String>,
    ) -> Result<bool> {
        let now = Utc::now();
        self.store.update_curated_entry(entry_id, |entry| {
            entry.curation = status;
            entry.last_curated_at = Some(now);
            if let Some(n) = notes {
                entry.auditor_notes = Some(n);
            }
            // Recalculate confidence after status change
            entry.confidence = confidence_from_curation(&entry.curation, entry.relevance_score);
        })
    }

    /// Apply bulk feedback from auditor review.
    ///
    /// Maps audit actions to curation status:
    /// - @audit-confirmed → Useful
    /// - @audit-false-positive → Noise
    /// - @audit-escalate → Critical
    pub fn apply_feedback(&self, feedback: &[FeedbackItem]) -> Result<usize> {
        let mut updated = 0;
        for item in feedback {
            let notes = item.reason.clone();
            if self.set_curation(&item.entry_id, item.new_status.clone(), notes)? {
                updated += 1;
            }
        }
        Ok(updated)
    }

    /// Load existing curation state from curated files.
    /// Returns a map of entry_id → (curation, notes, last_curated_at).
    fn load_existing_curation(&self) -> Result<HashMap<String, CurationSnapshot>> {
        let mut map = HashMap::new();
        for impact in &[Impact::High, Impact::Medium] {
            if let Some(file) = self.store.read_curated(impact)? {
                for entry in file.entries {
                    map.insert(
                        entry.id.clone(),
                        CurationSnapshot {
                            curation: entry.curation,
                            auditor_notes: entry.auditor_notes,
                            last_curated_at: entry.last_curated_at,
                        },
                    );
                }
            }
        }
        Ok(map)
    }
}

/// Snapshot of curation state for an entry (used to preserve human decisions).
struct CurationSnapshot {
    curation: CurationStatus,
    auditor_notes: Option<String>,
    last_curated_at: Option<chrono::DateTime<Utc>>,
}

/// Deduplicate entries by slug. On conflict, keep the one with highest quality_score.
fn deduplicate(entries: Vec<KbEntry>) -> Vec<KbEntry> {
    let mut by_slug: HashMap<String, KbEntry> = HashMap::new();
    for entry in entries {
        by_slug
            .entry(entry.slug.clone())
            .and_modify(|existing| {
                if entry.quality_score > existing.quality_score {
                    *existing = entry.clone();
                }
            })
            .or_insert(entry);
    }
    by_slug.into_values().collect()
}

/// Score an entry using currently available signals.
///
/// Current implementation (Task 5): impact rank + quality_score.
/// Future (Task 7): adds tag overlap, keyword overlap, category match, recency.
fn score_entry(entry: &mut KbEntry) {
    // Normalize quality_score to 0..1 (Solodit uses ~1..5 scale)
    let quality_norm = (entry.quality_score / 5.0).clamp(0.0, 1.0);

    // Impact contribution: HIGH=1.0, MEDIUM=0.5
    let impact_norm = entry.impact.rank() as f64 / 2.0;

    // Simple weighted average — only two signals for now
    // Quality: 0.6 weight (most reliable signal we have)
    // Impact: 0.4 weight
    entry.relevance_score = (quality_norm * 0.6 + impact_norm * 0.4).clamp(0.0, 1.0);

    // Confidence derived from curation status + relevance
    entry.confidence = confidence_from_curation(&entry.curation, entry.relevance_score);

    entry.last_curated_at = Some(Utc::now());
}

/// Derive confidence from curation status and relevance score.
fn confidence_from_curation(curation: &CurationStatus, relevance: f64) -> f64 {
    match curation {
        CurationStatus::Critical => 0.95,
        CurationStatus::Useful => 0.80,
        CurationStatus::Unreviewed => relevance * 0.7,
        CurationStatus::Noise => 0.0,
    }
}

/// Sort entries: curation rank desc → relevance_score desc → quality_score desc.
fn sort_entries(entries: &mut [KbEntry]) {
    entries.sort_by(|a, b| {
        b.curation
            .rank()
            .cmp(&a.curation.rank())
            .then(
                b.relevance_score
                    .partial_cmp(&a.relevance_score)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(
                b.quality_score
                    .partial_cmp(&a.quality_score)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
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

    fn make_entry(id: &str, impact: Impact, quality: f64, curation: CurationStatus) -> KbEntry {
        KbEntry {
            id: id.to_string(),
            slug: id.to_string(),
            title: format!("Finding: {}", id),
            impact,
            quality_score: quality,
            firm: "Firm".to_string(),
            protocol: "Proto".to_string(),
            tags: vec!["Reentrancy".to_string()],
            category: "Lending".to_string(),
            summary: Some("summary".to_string()),
            content: None,
            source: EntrySource::Solodit,
            curation,
            relevance_score: 0.0,
            confidence: 0.0,
            ingested_at: Utc::now(),
            last_curated_at: None,
            auditor_notes: None,
        }
    }

    fn empty_context() -> CurationContext {
        CurationContext::default()
    }

    #[test]
    fn curate_buckets_by_impact() {
        let (_tmp, store) = temp_store();
        let curator = Curator::new(&store);

        let entries = vec![
            make_entry("h1", Impact::High, 4.0, CurationStatus::Unreviewed),
            make_entry("m1", Impact::Medium, 3.0, CurationStatus::Unreviewed),
            make_entry("h2", Impact::High, 5.0, CurationStatus::Unreviewed),
        ];

        let stats = curator.curate(entries, &empty_context()).unwrap();
        assert_eq!(stats.high_count, 2);
        assert_eq!(stats.medium_count, 1);
        assert_eq!(stats.noise_skipped, 0);

        let high = store.read_curated(&Impact::High).unwrap().unwrap();
        assert_eq!(high.entries.len(), 2);
        let medium = store.read_curated(&Impact::Medium).unwrap().unwrap();
        assert_eq!(medium.entries.len(), 1);
    }

    #[test]
    fn curate_excludes_noise() {
        let (_tmp, store) = temp_store();
        let curator = Curator::new(&store);

        let entries = vec![
            make_entry("good", Impact::High, 4.0, CurationStatus::Unreviewed),
            make_entry("noisy", Impact::High, 3.0, CurationStatus::Noise),
        ];

        let stats = curator.curate(entries, &empty_context()).unwrap();
        assert_eq!(stats.high_count, 1);
        assert_eq!(stats.noise_skipped, 1);

        let high = store.read_curated(&Impact::High).unwrap().unwrap();
        assert_eq!(high.entries.len(), 1);
        assert_eq!(high.entries[0].id, "good");
    }

    #[test]
    fn curate_deduplicates_by_slug() {
        let (_tmp, store) = temp_store();
        let curator = Curator::new(&store);

        let entries = vec![
            make_entry("dup", Impact::High, 3.0, CurationStatus::Unreviewed),
            make_entry("dup", Impact::High, 5.0, CurationStatus::Unreviewed),
        ];

        let stats = curator.curate(entries, &empty_context()).unwrap();
        assert_eq!(stats.high_count, 1);

        let high = store.read_curated(&Impact::High).unwrap().unwrap();
        assert_eq!(high.entries[0].quality_score, 5.0); // kept higher quality
    }

    #[test]
    fn curate_sorts_by_curation_then_score() {
        let (_tmp, store) = temp_store();
        let curator = Curator::new(&store);

        let entries = vec![
            make_entry("low-q", Impact::High, 2.0, CurationStatus::Unreviewed),
            make_entry("critical", Impact::High, 3.0, CurationStatus::Critical),
            make_entry("high-q", Impact::High, 5.0, CurationStatus::Unreviewed),
            make_entry("useful", Impact::High, 3.0, CurationStatus::Useful),
        ];

        curator.curate(entries, &empty_context()).unwrap();
        let high = store.read_curated(&Impact::High).unwrap().unwrap();

        // Critical first, then Useful, then Unreviewed ordered by score
        assert_eq!(high.entries[0].id, "critical");
        assert_eq!(high.entries[1].id, "useful");
        assert_eq!(high.entries[2].id, "high-q");
        assert_eq!(high.entries[3].id, "low-q");
    }

    #[test]
    fn curate_preserves_existing_curation() {
        let (_tmp, store) = temp_store();
        let curator = Curator::new(&store);

        // First pass: entries arrive as unreviewed
        let entries = vec![
            make_entry("a", Impact::High, 4.0, CurationStatus::Unreviewed),
        ];
        curator.curate(entries, &empty_context()).unwrap();

        // Manually curate entry as Useful
        curator
            .set_curation("a", CurationStatus::Useful, Some("good one".to_string()))
            .unwrap();

        // Second pass: same entry arrives again as Unreviewed (fresh from API)
        let entries = vec![
            make_entry("a", Impact::High, 4.0, CurationStatus::Unreviewed),
        ];
        curator.curate(entries, &empty_context()).unwrap();

        // Curation should be preserved
        let high = store.read_curated(&Impact::High).unwrap().unwrap();
        assert_eq!(high.entries[0].curation, CurationStatus::Useful);
        assert_eq!(high.entries[0].auditor_notes.as_deref(), Some("good one"));
    }

    #[test]
    fn set_curation_updates_entry() {
        let (_tmp, store) = temp_store();
        let curator = Curator::new(&store);

        let entries = vec![
            make_entry("target", Impact::Medium, 3.0, CurationStatus::Unreviewed),
        ];
        curator.curate(entries, &empty_context()).unwrap();

        let updated = curator
            .set_curation("target", CurationStatus::Critical, Some("always surface".to_string()))
            .unwrap();
        assert!(updated);

        let medium = store.read_curated(&Impact::Medium).unwrap().unwrap();
        assert_eq!(medium.entries[0].curation, CurationStatus::Critical);
        assert_eq!(medium.entries[0].confidence, 0.95);
    }

    #[test]
    fn set_curation_nonexistent_returns_false() {
        let (_tmp, store) = temp_store();
        let curator = Curator::new(&store);

        // Write empty curated file
        let entries = vec![
            make_entry("other", Impact::High, 4.0, CurationStatus::Unreviewed),
        ];
        curator.curate(entries, &empty_context()).unwrap();

        let updated = curator
            .set_curation("missing", CurationStatus::Useful, None)
            .unwrap();
        assert!(!updated);
    }

    #[test]
    fn apply_feedback_bulk() {
        let (_tmp, store) = temp_store();
        let curator = Curator::new(&store);

        let entries = vec![
            make_entry("confirmed", Impact::High, 4.0, CurationStatus::Unreviewed),
            make_entry("rejected", Impact::High, 3.0, CurationStatus::Unreviewed),
            make_entry("escalated", Impact::Medium, 3.5, CurationStatus::Unreviewed),
        ];
        curator.curate(entries, &empty_context()).unwrap();

        let feedback = vec![
            FeedbackItem {
                entry_id: "confirmed".to_string(),
                new_status: CurationStatus::Useful,
                reason: Some("valid finding".to_string()),
            },
            FeedbackItem {
                entry_id: "rejected".to_string(),
                new_status: CurationStatus::Noise,
                reason: Some("fee is capped by admin".to_string()),
            },
            FeedbackItem {
                entry_id: "escalated".to_string(),
                new_status: CurationStatus::Critical,
                reason: Some("worse than medium".to_string()),
            },
        ];

        let updated = curator.apply_feedback(&feedback).unwrap();
        assert_eq!(updated, 3);

        let high = store.read_curated(&Impact::High).unwrap().unwrap();
        let confirmed = high.entries.iter().find(|e| e.id == "confirmed").unwrap();
        assert_eq!(confirmed.curation, CurationStatus::Useful);
        assert_eq!(confirmed.confidence, 0.80);

        let rejected = high.entries.iter().find(|e| e.id == "rejected").unwrap();
        assert_eq!(rejected.curation, CurationStatus::Noise);

        let medium = store.read_curated(&Impact::Medium).unwrap().unwrap();
        let escalated = medium.entries.iter().find(|e| e.id == "escalated").unwrap();
        assert_eq!(escalated.curation, CurationStatus::Critical);
        assert_eq!(escalated.confidence, 0.95);
    }

    #[test]
    fn score_entry_values() {
        // HIGH impact, quality 5.0 → relevance = (1.0 * 0.6) + (1.0 * 0.4) = 1.0
        let mut high_perfect = make_entry("hp", Impact::High, 5.0, CurationStatus::Unreviewed);
        score_entry(&mut high_perfect);
        assert!((high_perfect.relevance_score - 1.0).abs() < 0.001);

        // MEDIUM impact, quality 2.5 → relevance = (0.5 * 0.6) + (0.5 * 0.4) = 0.5
        let mut med_mid = make_entry("mm", Impact::Medium, 2.5, CurationStatus::Unreviewed);
        score_entry(&mut med_mid);
        assert!((med_mid.relevance_score - 0.5).abs() < 0.001);

        // Unreviewed confidence = relevance * 0.7
        assert!((high_perfect.confidence - 0.7).abs() < 0.001);
        assert!((med_mid.confidence - 0.35).abs() < 0.001);
    }

    #[test]
    fn confidence_from_curation_values() {
        assert_eq!(confidence_from_curation(&CurationStatus::Critical, 0.5), 0.95);
        assert_eq!(confidence_from_curation(&CurationStatus::Useful, 0.5), 0.80);
        assert!((confidence_from_curation(&CurationStatus::Unreviewed, 0.8) - 0.56).abs() < 0.001);
        assert_eq!(confidence_from_curation(&CurationStatus::Noise, 0.9), 0.0);
    }
}
