use crate::bs::cache::BuildCache;
use crate::bs::config::Manifest;
use crate::markup::render;
use std::fs;
use walkdir::WalkDir;

pub struct Builder {
    manifest: Manifest,
    cache: BuildCache,
}

impl Builder {
    pub fn new(manifest: Manifest) -> anyhow::Result<Self> {
        let cache = BuildCache::load(manifest.project.cache_dir_path())?;
        Ok(Self { manifest, cache })
    }

    pub fn build(&mut self) -> anyhow::Result<()> {
        let project = &self.manifest.project;

        fs::create_dir_all(project.out_dir_path())?;
        fs::create_dir_all(project.cache_dir_path())?;

        for entry in WalkDir::new(project.src_dir_path())
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| Option::map_or(e.path().extension(), false, |ext| ext == "md"))
        {
            let src_path = entry.path();
            let rel_path = src_path
                .strip_prefix(project.src_dir_path())
                .expect("Failed to strip prefix");

            if !self.cache.is_fresh(&project.src_dir_path(), src_path) {
                let content = fs::read_to_string(src_path)?;
                let html = render(&content);

                let mut out_path = project.out_dir_path().join(rel_path);
                out_path.set_extension("html");

                if let Some(parent) = out_path.parent() {
                    fs::create_dir_all(parent)?;
                }

                fs::write(&out_path, html)?;
                self.cache.update(&project.src_dir_path(), src_path);
            }
        }

        self.cache.save(project.cache_dir_path())?;
        Ok(())
    }
}
