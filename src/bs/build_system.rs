use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Default)]
pub struct ProjectConfig {
    pub name: String,
    pub src_dir: String,
    pub out_dir: String,
    pub cache_dir: String,
    #[serde(default)]
    #[doc(hidden)]
    pub manifest_dir: PathBuf,
}

impl ProjectConfig {
    pub fn cache_dir_path(&self) -> PathBuf {
        self.manifest_dir.join(&self.cache_dir)
    }
    
    pub fn src_dir_path(&self) -> PathBuf {
        self.manifest_dir.join(&self.src_dir)
    }
    
    pub fn out_dir_path(&self) -> PathBuf {
        self.manifest_dir.join(&self.out_dir)
    }
}
