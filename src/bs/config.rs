use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::bs::build_system::ProjectConfig;

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub project: ProjectConfig,
}

impl Manifest {
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let manifest_dir = path.parent().unwrap_or(Path::new("."));
        
        let content = fs::read_to_string(path)?;
        let mut manifest: Manifest = toml::from_str(&content)?;
        manifest.project.manifest_dir = manifest_dir.to_path_buf();
        Ok(manifest)
    }
}
