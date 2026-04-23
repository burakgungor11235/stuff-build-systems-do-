use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

/// Tolerance for timestamp comparisons (milliseconds)
/// Handles clock precision issues and touch-rebuild scenarios
/// Before this, it wasn't an incremental build system half of the time 60% of the time
const TIMESTAMP_TOLERANCE_MS: u64 = 2;

#[derive(Debug, PartialEq, Eq)]
pub enum CacheStatus {
    /// cached and fresh
    UpToDate,
    /// (cached but outdated)
    Stale,
    NotCached,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct BuildCache {
    /// Maps relative file paths to their last modified timestamp (ms)
    pub entries: HashMap<String, u64>,
}

#[allow(dead_code)]
impl BuildCache {
    pub fn load<P: AsRef<Path>>(cache_dir: P) -> Result<Self> {
        let cache_file = cache_dir.as_ref().join("cache.json");
        if !cache_file.exists() {
            return Ok(Self::default());
        }
        
        let content = fs::read_to_string(&cache_file)
            .with_context(|| format!("Failed to read cache file: {:?}", cache_file))?;
        
        let cache: BuildCache = serde_json::from_str(&content)?;
        Ok(cache)
    }

    pub fn save<P: AsRef<Path>>(&self, cache_dir: P) -> Result<()> {
        let cache_dir = cache_dir.as_ref();
        fs::create_dir_all(cache_dir)?;
        let cache_file = cache_dir.join("cache.json");
        
        let content = serde_json::to_string_pretty(self)?;
        fs::write(cache_file, content)?;
        Ok(())
    }

    fn get_mtime(path: &Path) -> Option<u64> {
        let metadata = fs::metadata(path).ok()?;
        let modified = metadata.modified().ok()?;
        Some(
            modified
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        )
    }

    /// Get the relative path of source_path from src_dir, if it is a descendant.
    pub fn get_rel_path(src_dir: &Path, source_path: &Path) -> Option<String> {
        source_path
            .strip_prefix(src_dir)
            .ok()
            .map(|p| p.to_string_lossy().to_string())
    }

    pub fn is_fresh(&self, src_dir: &Path, source_path: &Path) -> CacheStatus {
        let rel_path = match Self::get_rel_path(src_dir, source_path) {
            Some(p) => p,
            None => return CacheStatus::NotCached,
        };

        if let Some(&cached_time) = self.entries.get(&rel_path) {
            if let Some(current_time) = Self::get_mtime(source_path) {
                // Consider fresh if current time is not newer than cached time + tolerance
                // This handles clock precision issues and touch-rebuild scenarios
                if current_time <= cached_time.saturating_add(TIMESTAMP_TOLERANCE_MS) {
                    return CacheStatus::UpToDate;
                }
            }
            // Either no current time or current time > cached_time + tolerance
            return CacheStatus::Stale;
        }
        CacheStatus::NotCached
    }

    pub fn update(&mut self, src_dir: &Path, source_path: &Path) {
        if let Some(rel_path) = Self::get_rel_path(src_dir, source_path) {
            if let Some(current_time) = Self::get_mtime(source_path) {
                self.entries.insert(rel_path, current_time);
            }
        }
    }

    /// Remove a cache entry for the given source path
    pub fn remove<P: AsRef<Path>>(&mut self, src_dir: P, source_path: P) -> Option<u64> {
        if let Some(rel_path) = Self::get_rel_path(src_dir.as_ref(), source_path.as_ref()) {
            self.entries.remove(&rel_path)
        } else {
            None
        }
    }

    /// Remove cache entries for files that no longer exist under src_dir
    /// Returns number of entries removed
    pub fn gc<P: AsRef<Path>>(&mut self, src_dir: P) -> usize {
        let src_dir = src_dir.as_ref();
        let mut removed = 0;
        self.entries.retain(|rel_path, _| {
            let abs_path = src_dir.join(rel_path);
            let keep = abs_path.exists();
            if !keep { removed += 1; }
            keep
        });
        removed
    }
}
