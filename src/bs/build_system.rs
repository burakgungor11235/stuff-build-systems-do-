use anyhow::{bail, Error, Result};
use serde::Deserialize;
use std::ffi::OsStr;
use std::fs::exists;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tracing::warn;
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

    entry_point: Option<String>,
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

    pub fn entry_point_rel_out(&self) -> Result<PathBuf> {
        let entry = self.get_entry_point()?;
        let src_dir = self.src_dir_path();
        let rel = entry.strip_prefix(&src_dir).unwrap_or(&entry);
        let mut rel_path = rel.to_path_buf();
        rel_path.set_extension(OUTPUT_EXTENSION);
        Ok(rel_path)
    }

    pub fn walk_source_files(&self) -> impl Iterator<Item = walkdir::DirEntry> {
        WalkDir::new(self.src_dir_path())
            .into_iter()
            .filter_map(|e| e.ok())
            // no magic numbers my arse.
            .filter(|e| e.path().extension() == Some(OsStr::new(SOURCE_EXTENSION)))
    }

    pub fn get_entry_point(&self) -> Result<PathBuf> {
        if let Some(s) = &self.entry_point {
            let p = self.manifest_dir.join(s);
            if p.is_file() {
                return Ok(p);
            }
            warn!(
                configured = %s,
                resolved = %p.display(),
                "configured entry_point not found, falling back to default",
            );
        }
        self.default_entry_point()
    }

    // this isn't idiomatic rust, this is idiotic rust.
    fn default_entry_point(&self) -> Result<PathBuf> {
        let dir = self.src_dir_path();

        let mut files: Vec<PathBuf> = self.walk_source_files().map(|e| e.into_path()).collect();
        files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

        match files.into_iter().next() {
            Some(p) => Ok(p),
            None => bail!(
                "no .stuff files found in {}; \
             create at least one source file, or set \
             `entry_point` in stuff.toml\n on a serious note. do you really expect me to compile you a document out of nothin? ",
                dir.display(),
            ),
        }
    }
}
