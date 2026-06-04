use serde::Deserialize;
use std::ffi::OsStr;
use std::path::PathBuf;
use walkdir::WalkDir;

pub const SOURCE_EXTENSION: &str = "stuff";
pub const OUTPUT_EXTENSION: &str = "html";

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

    pub fn output_path_for_source(&self, rel_path: &str) -> PathBuf {
        let mut p = self.out_dir_path().join(rel_path);
        p.set_extension(OUTPUT_EXTENSION);
        p
    }

    pub fn walk_source_files(&self) -> impl Iterator<Item = walkdir::DirEntry> {
        WalkDir::new(self.src_dir_path())
            .into_iter()
            .filter_map(|e| e.ok())
            // no magic numbers my arse.
            .filter(|e| e.path().extension() == Some(OsStr::new(SOURCE_EXTENSION)))
    }
}
