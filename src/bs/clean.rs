// Gotta clean what you made you know 

use crate::bs::config::Manifest;
use std::fs;
use tracing::info;

pub struct Cleaner {
    manifest: Manifest,
}

impl Cleaner {
    pub fn new(manifest: Manifest) -> Self {
        Self { manifest }
    }

    pub fn clean(&self) -> anyhow::Result<()> {
        let project = &self.manifest.project;
        
        if project.out_dir_path().exists() {
            info!(dir = %project.out_dir_path().display(), "Cleaning output directory");
            fs::remove_dir_all(project.out_dir_path())?;
        }
        
        if project.cache_dir_path().exists() {
            info!(dir = %project.cache_dir_path().display(), "Cleaning cache directory");
            fs::remove_dir_all(project.cache_dir_path())?;
        }
        
        Ok(())
    }
}
