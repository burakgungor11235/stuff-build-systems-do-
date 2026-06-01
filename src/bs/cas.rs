use anyhow::{Context, Result};
use blake3;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const INDEX_FILE: &str = "cas_index.json";
const TMP_DIR: &str = ".tmp";
const ARTIFACTS_DIR: &str = "artifacts";
const ARTIFACT_EXT: &str = "bin";
const ARTIFACT_TYPE: &str = "rendered_chunk";
const HASH_TRUNCATION: usize = 20;
const HASH_PREFIX_LEN: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hash(String);

impl Hash {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn prefix(&self) -> &str {
        &self.0[..HASH_PREFIX_LEN]
    }

    #[must_use]
    #[allow(unused)]
    pub fn from_hex(s: &str) -> Self {
        Hash(s.to_string())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArtifactMeta {
    pub artifact_type: String,
    pub size: usize,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CasIndex {
    pub entries: std::collections::HashMap<String, ArtifactMeta>,
}

pub struct Cas {
    cache_dir: PathBuf,
    index: CasIndex,
    dirty: bool,
}

impl Default for Cas {
    fn default() -> Self {
        Self {
            cache_dir: PathBuf::new(),
            index: CasIndex::default(),
            dirty: false,
        }
    }
}

impl Cas {
    pub fn load(cache_dir: &Path) -> Result<Self> {
        let index_file = cache_dir.join(INDEX_FILE);
        let index = if index_file.exists() {
            let content = fs::read_to_string(&index_file)
                .with_context(|| format!("Failed to read CAS index: {:?}", index_file))?;
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            CasIndex::default()
        };

        Ok(Self {
            cache_dir: cache_dir.to_path_buf(),
            index,
            dirty: false,
        })
    }

    pub fn save(&mut self) -> Result<()> {
        if !self.dirty && self.index.entries.is_empty() {
            return Ok(());
        }

        fs::create_dir_all(&self.cache_dir)?;
        let index_file = self.cache_dir.join(INDEX_FILE);
        let content = serde_json::to_string_pretty(&self.index)?;
        fs::write(&index_file, content)?;
        self.dirty = false;
        Ok(())
    }

    #[must_use]
    pub fn get(&self, key: &Hash) -> Option<Vec<u8>> {
        if !self.index.entries.contains_key(key.as_str()) {
            return None;
        }

        let artifact_path = self.artifact_path(key);
        fs::read(&artifact_path).ok()
    }

    pub fn put(&mut self, key: &Hash, data: &[u8]) -> Result<()> {
        let artifact_path = self.artifact_path(key);
        let tmp_path = self.cache_dir.join(TMP_DIR).join(key.as_str());

        fs::create_dir_all(tmp_path.parent().unwrap())?;
        fs::create_dir_all(artifact_path.parent().unwrap())?;

        fs::write(&tmp_path, data)?;
        fs::rename(&tmp_path, &artifact_path)?;

        self.index.entries.insert(
            key.as_str().to_string(),
            ArtifactMeta {
                artifact_type: ARTIFACT_TYPE.to_string(),
                size: data.len(),
            },
        );
        self.dirty = true;
        Ok(())
    }

    #[must_use]
    pub fn contains(&self, key: &Hash) -> bool {
        self.index.entries.contains_key(key.as_str())
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn entry_count(&self) -> usize {
        self.index.entries.len()
    }

    pub fn iter_entries(&self) -> impl Iterator<Item = (&str, &ArtifactMeta)> {
        self.index.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    #[must_use]
    pub fn compute_chunk_key(rel_path: &str, chunk_index: usize, content_hash: &[u8]) -> Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(ARTIFACT_TYPE.as_bytes());
        hasher.update(rel_path.as_bytes());
        hasher.update(&chunk_index.to_le_bytes());
        hasher.update(content_hash);
        let hex = hasher.finalize().to_hex();
        Hash(hex.as_str()[..HASH_TRUNCATION].to_string())
    }

    #[must_use]
    pub fn compute_content_hash(content: &[u8]) -> Vec<u8> {
        blake3::hash(content).as_bytes().to_vec()
    }

    pub fn read_and_hash(path: &std::path::Path) -> std::io::Result<(String, Vec<u8>)> {
        let content = std::fs::read_to_string(path)?;
        let hash = Self::compute_content_hash(content.as_bytes());
        Ok((content, hash))
    }

    #[must_use]
    #[allow(dead_code)]
    // look ma, I made a gc language!
    pub fn gc(&mut self, live_keys: &HashSet<Hash>) -> usize {
        let mut removed = 0;
        let dead_keys: Vec<String> = self
            .index
            .entries
            .keys()
            .filter(|k| !live_keys.contains(&Hash(k.to_string())))
            .cloned()
            .collect();

        for key in &dead_keys {
            let artifact_path = self.artifact_path(&Hash(key.clone()));
            let _ = fs::remove_file(&artifact_path);
            self.index.entries.remove(key);
            removed += 1;
        }

        if removed > 0 {
            self.dirty = true;
        }
        removed
    }

    fn artifact_path(&self, key: &Hash) -> PathBuf {
        self.cache_dir
            .join(ARTIFACTS_DIR)
            .join(key.prefix())
            .join(format!("{}.{}", key.as_str(), ARTIFACT_EXT))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("sbd_cas_test_{}_{}", std::process::id(), n));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn default_cas_is_empty() {
        let dir = temp_dir();
        let cas = Cas::load(&dir).unwrap();
        assert!(!cas.contains(&Hash("abcdef".to_string())));
        cleanup(&dir);
    }

    #[test]
    fn put_and_get_round_trip() {
        let dir = temp_dir();
        let mut cas = Cas::load(&dir).unwrap();
        let key = Hash("ab1234567890".to_string());
        let data = b"<p>hello</p>";

        cas.put(&key, data).unwrap();
        let result = cas.get(&key).unwrap();
        assert_eq!(result, data);
        cleanup(&dir);
    }

    #[test]
    fn get_returns_none_for_missing_key() {
        let dir = temp_dir();
        let cas = Cas::load(&dir).unwrap();
        assert!(cas.get(&Hash("nonexistent".to_string())).is_none());
        cleanup(&dir);
    }

    #[test]
    fn contains_reflects_put() {
        let dir = temp_dir();
        let mut cas = Cas::load(&dir).unwrap();
        let key = Hash("ab1234567890".to_string());

        assert!(!cas.contains(&key));
        cas.put(&key, b"data").unwrap();
        assert!(cas.contains(&key));
        cleanup(&dir);
    }

    #[test]
    fn save_and_load_persists_index() {
        let dir = temp_dir();
        {
            let mut cas = Cas::load(&dir).unwrap();
            let key = Hash("ab1234567890".to_string());
            cas.put(&key, b"<p>persisted</p>").unwrap();
            cas.save().unwrap();
        }

        let cas = Cas::load(&dir).unwrap();
        assert!(cas.contains(&Hash("ab1234567890".to_string())));
        let result = cas.get(&Hash("ab1234567890".to_string())).unwrap();
        assert_eq!(result, b"<p>persisted</p>");
        cleanup(&dir);
    }

    #[test]
    fn put_overwrites_existing() {
        let dir = temp_dir();
        let mut cas = Cas::load(&dir).unwrap();
        let key = Hash("ab1234567890".to_string());

        cas.put(&key, b"old").unwrap();
        cas.put(&key, b"new").unwrap();

        let result = cas.get(&key).unwrap();
        assert_eq!(result, b"new");
        cleanup(&dir);
    }

    #[test]
    fn compute_chunk_key_deterministic() {
        let k1 = Cas::compute_chunk_key("file.stuff", 0, &[1, 2, 3]);
        let k2 = Cas::compute_chunk_key("file.stuff", 0, &[1, 2, 3]);
        assert_eq!(k1, k2);
    }

    #[test]
    fn compute_chunk_key_different_path() {
        let k1 = Cas::compute_chunk_key("file1.stuff", 0, &[1, 2, 3]);
        let k2 = Cas::compute_chunk_key("file2.stuff", 0, &[1, 2, 3]);
        assert_ne!(k1, k2);
    }

    #[test]
    fn compute_chunk_key_different_index() {
        let k1 = Cas::compute_chunk_key("file.stuff", 0, &[1, 2, 3]);
        let k2 = Cas::compute_chunk_key("file.stuff", 1, &[1, 2, 3]);
        assert_ne!(k1, k2);
    }

    #[test]
    fn compute_chunk_key_different_content() {
        let k1 = Cas::compute_chunk_key("file.stuff", 0, &[1, 2, 3]);
        let k2 = Cas::compute_chunk_key("file.stuff", 0, &[4, 5, 6]);
        assert_ne!(k1, k2);
    }

    #[test]
    fn compute_chunk_key_is_20_chars() {
        let key = Cas::compute_chunk_key("file.stuff", 0, &[1, 2, 3]);
        assert_eq!(key.as_str().len(), 20);
    }

    #[test]
    fn compute_chunk_key_is_hex() {
        let key = Cas::compute_chunk_key("file.stuff", 0, &[1, 2, 3]);
        assert!(key.as_str().chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn compute_content_hash_deterministic() {
        let h1 = Cas::compute_content_hash(b"hello");
        let h2 = Cas::compute_content_hash(b"hello");
        assert_eq!(h1, h2);
    }

    #[test]
    fn compute_content_hash_different_input() {
        let h1 = Cas::compute_content_hash(b"hello");
        let h2 = Cas::compute_content_hash(b"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn gc_removes_dead_entries() {
        let dir = temp_dir();
        let mut cas = Cas::load(&dir).unwrap();

        let k1 = Hash("ab1111111111".to_string());
        let k2 = Hash("cd2222222222".to_string());
        cas.put(&k1, b"data1").unwrap();
        cas.put(&k2, b"data2").unwrap();

        let mut live = HashSet::new();
        live.insert(k1.clone());

        let removed = cas.gc(&live);
        assert_eq!(removed, 1);
        assert!(cas.contains(&k1));
        assert!(!cas.contains(&k2));
        cleanup(&dir);
    }

    #[test]
    fn gc_removes_nothing_when_all_live() {
        let dir = temp_dir();
        let mut cas = Cas::load(&dir).unwrap();

        let k1 = Hash("ab1111111111".to_string());
        let k2 = Hash("cd2222222222".to_string());
        cas.put(&k1, b"data1").unwrap();
        cas.put(&k2, b"data2").unwrap();

        let mut live = HashSet::new();
        live.insert(k1.clone());
        live.insert(k2.clone());

        let removed = cas.gc(&live);
        assert_eq!(removed, 0);
        assert!(cas.contains(&k1));
        assert!(cas.contains(&k2));
        cleanup(&dir);
    }

    #[test]
    fn artifacts_stored_in_sharded_subdirectories() {
        let dir = temp_dir();
        let mut cas = Cas::load(&dir).unwrap();

        let k1 = Hash("ab1111111111".to_string());
        let k2 = Hash("cd2222222222".to_string());
        cas.put(&k1, b"a").unwrap();
        cas.put(&k2, b"b").unwrap();

        assert!(dir.join(ARTIFACTS_DIR).join("ab").exists());
        assert!(dir.join(ARTIFACTS_DIR).join("cd").exists());
        cleanup(&dir);
    }

    #[test]
    fn save_without_changes_is_noop() {
        let dir = temp_dir();
        let mut cas = Cas::load(&dir).unwrap();
        cas.save().unwrap();

        let index_file = dir.join(INDEX_FILE);
        assert!(!index_file.exists());
        cleanup(&dir);
    }
}
