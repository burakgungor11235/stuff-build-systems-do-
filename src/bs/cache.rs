use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct BuildCache {
    /// Maps relative file paths to their last modified timestamp (ms)
    /// Should be good enough for now.
    pub entries: HashMap<String, u64>,
}

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

    fn get_rel_path(src_dir: &Path, source_path: &Path) -> Option<String> {
        source_path
            .strip_prefix(src_dir)
            .ok()
            .map(|p| p.to_string_lossy().to_string())
    }

    pub fn is_fresh(&self, src_dir: &Path, source_path: &Path) -> bool {
        let rel_path = match Self::get_rel_path(src_dir, source_path) {
            Some(p) => p,
            None => return false,
        };

        if let Some(&cached_time) = self.entries.get(&rel_path) {
            if let Some(current_time) = Self::get_mtime(source_path) {
                // Use equality: if the file changed in any way, it's not fresh
                return current_time == cached_time;
            }
        }
        false
    }

    pub fn update(&mut self, src_dir: &Path, source_path: &Path) {
        if let Some(rel_path) = Self::get_rel_path(src_dir, source_path) {
            if let Some(current_time) = Self::get_mtime(source_path) {
                self.entries.insert(rel_path, current_time);
            }
        }
    }
}
