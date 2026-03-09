use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::types::{CuratedFile, Impact, KbEntry, RawCacheEnvelope, SeedFile};

/// Filesystem operations for the KnowledgeBase directory.
///
/// All JSON I/O is isolated here so the core logic is testable with temp dirs.
pub struct Store {
    base_dir: PathBuf,
}

impl Store {
    /// Create a store rooted at the given directory.
    /// Creates `raw/`, `curated/`, and `seeds/` subdirectories if missing.
    pub fn new(base_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(base_dir.join("raw"))
            .with_context(|| format!("creating raw/ in {}", base_dir.display()))?;
        fs::create_dir_all(base_dir.join("curated"))
            .with_context(|| format!("creating curated/ in {}", base_dir.display()))?;
        fs::create_dir_all(base_dir.join("seeds"))
            .with_context(|| format!("creating seeds/ in {}", base_dir.display()))?;
        Ok(Self { base_dir })
    }

    // -- Raw cache --

    fn raw_path(&self, fingerprint: &str) -> PathBuf {
        self.base_dir.join("raw").join(format!("{}.json", fingerprint))
    }

    /// Read a raw cache envelope by fingerprint. Returns None if not found.
    pub fn read_raw(&self, fingerprint: &str) -> Result<Option<RawCacheEnvelope>> {
        let path = self.raw_path(fingerprint);
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let envelope: RawCacheEnvelope = serde_json::from_str(&data)
            .with_context(|| format!("parsing {}", path.display()))?;
        Ok(Some(envelope))
    }

    /// Write a raw cache envelope to disk.
    pub fn write_raw(&self, envelope: &RawCacheEnvelope) -> Result<()> {
        let path = self.raw_path(&envelope.fingerprint);
        let json = serde_json::to_string_pretty(envelope)
            .context("serializing raw cache envelope")?;
        fs::write(&path, json)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    /// Delete a raw cache file by fingerprint.
    pub fn delete_raw(&self, fingerprint: &str) -> Result<()> {
        let path = self.raw_path(fingerprint);
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("deleting {}", path.display()))?;
        }
        Ok(())
    }

    /// List all raw cache envelopes.
    pub fn list_raw(&self) -> Result<Vec<RawCacheEnvelope>> {
        let raw_dir = self.base_dir.join("raw");
        let mut envelopes = Vec::new();
        for entry in fs::read_dir(&raw_dir).with_context(|| format!("listing {}", raw_dir.display()))? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let data = fs::read_to_string(&path)
                    .with_context(|| format!("reading {}", path.display()))?;
                match serde_json::from_str::<RawCacheEnvelope>(&data) {
                    Ok(env) => envelopes.push(env),
                    Err(e) => {
                        tracing::warn!("skipping malformed raw cache file {}: {}", path.display(), e);
                    }
                }
            }
        }
        Ok(envelopes)
    }

    // -- Curated --

    fn curated_path(&self, impact: &Impact) -> PathBuf {
        let name = match impact {
            Impact::High => "high.json",
            Impact::Medium => "medium.json",
        };
        self.base_dir.join("curated").join(name)
    }

    /// Read a curated file for a given impact level. Returns None if not found.
    pub fn read_curated(&self, impact: &Impact) -> Result<Option<CuratedFile>> {
        let path = self.curated_path(impact);
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let file: CuratedFile = serde_json::from_str(&data)
            .with_context(|| format!("parsing {}", path.display()))?;
        Ok(Some(file))
    }

    /// Write a curated file for a given impact level.
    pub fn write_curated(&self, file: &CuratedFile) -> Result<()> {
        let path = self.curated_path(&file.impact);
        let json = serde_json::to_string_pretty(file)
            .context("serializing curated file")?;
        fs::write(&path, json)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    // -- Seeds --

    /// Read all seed files from `seeds/`.
    pub fn read_seeds(&self) -> Result<Vec<SeedFile>> {
        let seeds_dir = self.base_dir.join("seeds");
        let mut seed_files = Vec::new();
        for entry in fs::read_dir(&seeds_dir).with_context(|| format!("listing {}", seeds_dir.display()))? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let data = fs::read_to_string(&path)
                    .with_context(|| format!("reading {}", path.display()))?;
                match serde_json::from_str::<SeedFile>(&data) {
                    Ok(sf) => seed_files.push(sf),
                    Err(e) => {
                        tracing::warn!("skipping malformed seed file {}: {}", path.display(), e);
                    }
                }
            }
        }
        Ok(seed_files)
    }

    /// Write a seed file to `seeds/{domain}.json`.
    pub fn write_seed(&self, seed: &SeedFile) -> Result<()> {
        let filename = seed.domain.to_lowercase().replace(' ', "-");
        let path = self.base_dir.join("seeds").join(format!("{}.json", filename));
        let json = serde_json::to_string_pretty(seed)
            .context("serializing seed file")?;
        fs::write(&path, json)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    /// List seed file paths.
    pub fn list_seed_paths(&self) -> Result<Vec<PathBuf>> {
        let seeds_dir = self.base_dir.join("seeds");
        let mut paths = Vec::new();
        for entry in fs::read_dir(&seeds_dir).with_context(|| format!("listing {}", seeds_dir.display()))? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                paths.push(path);
            }
        }
        Ok(paths)
    }

    // -- Helpers --

    /// Find a KbEntry by ID across all curated files.
    pub fn find_entry_mut(&self, entry_id: &str) -> Result<Option<(Impact, KbEntry)>> {
        for impact in &[Impact::High, Impact::Medium] {
            if let Some(file) = self.read_curated(impact)? {
                if let Some(entry) = file.entries.iter().find(|e| e.id == entry_id) {
                    return Ok(Some((impact.clone(), entry.clone())));
                }
            }
        }
        Ok(None)
    }

    /// Update a single entry in the curated files by ID.
    /// Calls the provided closure to mutate the entry, then writes back.
    pub fn update_curated_entry<F>(&self, entry_id: &str, updater: F) -> Result<bool>
    where
        F: FnOnce(&mut KbEntry),
    {
        for impact in &[Impact::High, Impact::Medium] {
            if let Some(mut file) = self.read_curated(impact)? {
                if let Some(entry) = file.entries.iter_mut().find(|e| e.id == entry_id) {
                    updater(entry);
                    self.write_curated(&file)?;
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Base directory path.
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use chrono::Utc;
    use std::fs;
    use tempfile::TempDir;

    fn temp_store() -> (TempDir, Store) {
        let tmp = TempDir::new().unwrap();
        let store = Store::new(tmp.path().to_path_buf()).unwrap();
        (tmp, store)
    }

    fn sample_entry(id: &str, impact: Impact) -> KbEntry {
        KbEntry {
            id: id.to_string(),
            slug: id.to_string(),
            title: format!("Finding: {}", id),
            impact,
            quality_score: 4.0,
            firm: "TestFirm".to_string(),
            protocol: "TestProtocol".to_string(),
            tags: vec!["Reentrancy".to_string()],
            category: "Lending".to_string(),
            summary: Some("Test summary".to_string()),
            content: None,
            source: EntrySource::Solodit,
            curation: CurationStatus::Unreviewed,
            relevance_score: 0.0,
            confidence: 0.0,
            ingested_at: Utc::now(),
            last_curated_at: None,
            auditor_notes: None,
        }
    }

    fn sample_envelope(fingerprint: &str) -> RawCacheEnvelope {
        RawCacheEnvelope {
            fingerprint: fingerprint.to_string(),
            query_params: QueryParams {
                keywords: "reentrancy".to_string(),
                impact: vec!["HIGH".to_string()],
                tags: vec![],
                protocol_categories: vec![],
                min_quality: None,
            },
            fetched_at: Utc::now(),
            ttl_secs: 300,
            entries: vec![sample_entry("solodit:test-1", Impact::High)],
        }
    }

    // -- Raw cache tests --

    #[test]
    fn raw_roundtrip() {
        let (_tmp, store) = temp_store();
        let env = sample_envelope("abc123");

        store.write_raw(&env).unwrap();
        let loaded = store.read_raw("abc123").unwrap().unwrap();

        assert_eq!(loaded.fingerprint, "abc123");
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].id, "solodit:test-1");
    }

    #[test]
    fn raw_missing_returns_none() {
        let (_tmp, store) = temp_store();
        assert!(store.read_raw("nonexistent").unwrap().is_none());
    }

    #[test]
    fn raw_delete() {
        let (_tmp, store) = temp_store();
        let env = sample_envelope("to-delete");
        store.write_raw(&env).unwrap();
        assert!(store.read_raw("to-delete").unwrap().is_some());

        store.delete_raw("to-delete").unwrap();
        assert!(store.read_raw("to-delete").unwrap().is_none());
    }

    #[test]
    fn raw_list() {
        let (_tmp, store) = temp_store();
        store.write_raw(&sample_envelope("fp1")).unwrap();
        store.write_raw(&sample_envelope("fp2")).unwrap();

        let all = store.list_raw().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn raw_skips_malformed_files() {
        let (_tmp, store) = temp_store();
        // Write valid envelope
        store.write_raw(&sample_envelope("valid")).unwrap();
        // Write malformed file
        let bad_path = store.base_dir().join("raw").join("bad.json");
        fs::write(&bad_path, "not valid json").unwrap();

        let all = store.list_raw().unwrap();
        assert_eq!(all.len(), 1);
    }

    // -- Curated tests --

    #[test]
    fn curated_roundtrip() {
        let (_tmp, store) = temp_store();
        let file = CuratedFile {
            impact: Impact::High,
            last_curated_at: Utc::now(),
            entries: vec![sample_entry("solodit:high-1", Impact::High)],
        };

        store.write_curated(&file).unwrap();
        let loaded = store.read_curated(&Impact::High).unwrap().unwrap();

        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].id, "solodit:high-1");
    }

    #[test]
    fn curated_missing_returns_none() {
        let (_tmp, store) = temp_store();
        assert!(store.read_curated(&Impact::High).unwrap().is_none());
    }

    #[test]
    fn update_curated_entry_by_id() {
        let (_tmp, store) = temp_store();
        let file = CuratedFile {
            impact: Impact::High,
            last_curated_at: Utc::now(),
            entries: vec![sample_entry("solodit:target", Impact::High)],
        };
        store.write_curated(&file).unwrap();

        let updated = store
            .update_curated_entry("solodit:target", |e| {
                e.curation = CurationStatus::Useful;
                e.auditor_notes = Some("confirmed relevant".to_string());
            })
            .unwrap();
        assert!(updated);

        let loaded = store.read_curated(&Impact::High).unwrap().unwrap();
        assert_eq!(loaded.entries[0].curation, CurationStatus::Useful);
        assert_eq!(
            loaded.entries[0].auditor_notes.as_deref(),
            Some("confirmed relevant")
        );
    }

    #[test]
    fn update_nonexistent_entry_returns_false() {
        let (_tmp, store) = temp_store();
        let file = CuratedFile {
            impact: Impact::High,
            last_curated_at: Utc::now(),
            entries: vec![sample_entry("solodit:other", Impact::High)],
        };
        store.write_curated(&file).unwrap();

        let updated = store
            .update_curated_entry("solodit:missing", |_| {})
            .unwrap();
        assert!(!updated);
    }

    // -- Seed tests --

    #[test]
    fn seed_roundtrip() {
        let (_tmp, store) = temp_store();
        let seed = SeedFile {
            domain: "ERC4626".to_string(),
            description: "Known vault bugs".to_string(),
            entries: vec![sample_entry("seed:inflation-attack", Impact::High)],
        };

        store.write_seed(&seed).unwrap();
        let loaded = store.read_seeds().unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].domain, "ERC4626");
        assert_eq!(loaded[0].entries.len(), 1);
    }

    #[test]
    fn list_seed_paths() {
        let (_tmp, store) = temp_store();
        let seed = SeedFile {
            domain: "ERC4626".to_string(),
            description: "test".to_string(),
            entries: vec![],
        };
        store.write_seed(&seed).unwrap();

        let paths = store.list_seed_paths().unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].file_name().unwrap().to_str().unwrap().contains("erc4626"));
    }
}
