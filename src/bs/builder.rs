use crate::bs::cache::{BuildCache, CacheStatus};
use std::fs;
use tracing::{debug, info, warn};
use walkdir::WalkDir;

use crate::bs::config::Manifest;
use crate::markup::assembler::render_to_html;
use crate::markup::parser::parse;

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
        info!(project = %project.name, "Starting build");

        fs::create_dir_all(project.out_dir_path())?;
        fs::create_dir_all(project.cache_dir_path())?;

        let mut processed = 0;
        let mut rebuilt = 0;
        fn fun_name(e: &walkdir::DirEntry) -> bool {
            e.path()
                .extension()
                .map_or_else(|| false, |ext| ext == "stuff")
        }

        let tmp = WalkDir::new(project.src_dir_path())
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(fun_name);
        for entry in tmp {
            let src_path = entry.path();
            let rel_path = src_path.strip_prefix(project.src_dir_path())?;

            processed += 1;
            debug!(file = %rel_path.display(), "Checking file");

            if self.cache.is_fresh(&project.src_dir_path(), src_path) != CacheStatus::UpToDate {
                info!(file = %rel_path.display(), "Rebuilding file");
                rebuilt += 1;

                let content = fs::read_to_string(src_path)?;
                let doc = parse(&content);
                let html = render_to_html(&doc);

                let mut out_path = project.out_dir_path().join(rel_path);
                out_path.set_extension("html");

                if let Some(parent) = out_path.parent() {
                    fs::create_dir_all(parent)?;
                }

                fs::write(&out_path, html)?;
                self.cache.update(&project.src_dir_path(), src_path);
            } else {
                debug!(file = %rel_path.display(), "File up to date");
            }
        }

        self.cache.save(project.cache_dir_path())?;
        info!(processed, rebuilt, "Build complete");
        Ok(())
    }
}

