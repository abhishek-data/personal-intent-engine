//! Learned/synced vocabulary store, persisted separately from the user's
//! manual `pronunciation.json` so it can be reset or inspected on its own.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Where a learned entry came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LearnedSource {
    /// Background learner (Phase 2).
    Auto,
    /// Initial vocabulary sync (Phase 3).
    Sync,
}

/// One learned heard->canonical mapping with reinforcement metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LearnedEntry {
    pub heard: String,
    pub canonical: String,
    pub source: LearnedSource,
    pub confidence: f32,
    pub seen_count: u32,
    pub first_seen: u64,
    pub last_seen: u64,
}

/// Persistent collection of learned entries.
pub struct LearnedStore {
    entries: Vec<LearnedEntry>,
    path: Option<PathBuf>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl LearnedStore {
    /// Load from `path`; a missing or unparseable file yields an empty store
    /// (the path is retained so later writes land in the right place).
    pub fn load(path: PathBuf) -> Self {
        let entries = std::fs::read_to_string(&path)
            .ok()
            .and_then(|j| serde_json::from_str::<Vec<LearnedEntry>>(&j).ok())
            .unwrap_or_default();
        Self {
            entries,
            path: Some(path),
        }
    }

    /// Persist the current entries as pretty JSON. No-op without a path.
    pub fn save(&self) -> anyhow::Result<()> {
        if let Some(path) = &self.path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, serde_json::to_string_pretty(&self.entries)?)?;
        }
        Ok(())
    }

    /// Insert a new entry or reinforce an existing one (case-insensitive on
    /// `heard`), then persist.
    pub fn add_or_reinforce(
        &mut self,
        heard: &str,
        canonical: &str,
        source: LearnedSource,
    ) -> anyhow::Result<()> {
        let key = heard.trim().to_lowercase();
        let now = now_unix();
        if let Some(e) = self.entries.iter_mut().find(|e| e.heard == key) {
            e.seen_count = e.seen_count.saturating_add(1);
            e.last_seen = now;
            e.confidence += (1.0 - e.confidence) * 0.34;
        } else {
            self.entries.push(LearnedEntry {
                heard: key,
                canonical: canonical.trim().to_string(),
                source,
                confidence: 0.5,
                seen_count: 1,
                first_seen: now,
                last_seen: now,
            });
        }
        self.save()
    }

    /// Reinforce an existing entry if present; returns whether one was found.
    /// Does not write when nothing matches.
    pub fn reinforce(&mut self, heard: &str) -> anyhow::Result<bool> {
        let key = heard.trim().to_lowercase();
        let now = now_unix();
        if let Some(e) = self.entries.iter_mut().find(|e| e.heard == key) {
            e.seen_count = e.seen_count.saturating_add(1);
            e.last_seen = now;
            e.confidence += (1.0 - e.confidence) * 0.34;
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Check if an entry with the given heard (case-insensitive) exists.
    pub fn has_entry(&self, heard: &str) -> bool {
        let key = heard.trim().to_lowercase();
        self.entries.iter().any(|e| e.heard == key)
    }

    /// Get a slice of all learned entries.
    pub fn entries(&self) -> &[LearnedEntry] {
        &self.entries
    }

    /// Clear all learned entries and persist the empty set.
    pub fn reset(&mut self) -> anyhow::Result<()> {
        self.entries.clear();
        self.save()
    }

    /// Get the number of learned entries.
    pub fn count(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static N: AtomicU64 = AtomicU64::new(0);
    fn temp_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "pie-learned-{}-{}.json",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn add_then_reinforce_bumps_seen_count_and_persists() {
        let path = temp_path();
        let mut s = LearnedStore::load(path.clone());
        s.add_or_reinforce("next jazz", "Next.js", LearnedSource::Auto)
            .unwrap();
        assert_eq!(s.count(), 1);
        assert_eq!(s.entries()[0].seen_count, 1);
        s.add_or_reinforce("Next Jazz", "Next.js", LearnedSource::Auto)
            .unwrap(); // case-insensitive same key
        assert_eq!(s.count(), 1, "same heard must not duplicate");
        assert_eq!(s.entries()[0].seen_count, 2);

        let reloaded = LearnedStore::load(path.clone());
        assert_eq!(reloaded.count(), 1);
        assert_eq!(reloaded.entries()[0].seen_count, 2);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reinforce_missing_returns_false_no_write() {
        let path = temp_path();
        let mut s = LearnedStore::load(path.clone());
        assert!(!s.reinforce("nope").unwrap());
        assert!(
            !path.exists(),
            "reinforce of a missing key must not create the file"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reset_clears_and_persists_empty() {
        let path = temp_path();
        let mut s = LearnedStore::load(path.clone());
        s.add_or_reinforce("engine x", "Nginx", LearnedSource::Sync)
            .unwrap();
        s.reset().unwrap();
        assert_eq!(s.count(), 0);
        assert_eq!(LearnedStore::load(path.clone()).count(), 0);
        let _ = std::fs::remove_file(path);
    }
}
