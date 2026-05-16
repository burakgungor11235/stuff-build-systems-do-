use crate::bs::config::Manifest;
use crate::bs::registry::ChunkRegistry;
use crate::markup::assembler::{render_chunk, render_to_html};
use crate::markup::ast::Document;
use crate::markup::parser::parse;

use tracing::{debug, info};
use walkdir::WalkDir;

pub struct Builder {
    manifest: Manifest,
    cache: crate::bs::cache::BuildCache,
}

impl Builder {
    pub fn new(manifest: Manifest) -> anyhow::Result<Self> {
        let cache = crate::bs::cache::BuildCache::load(manifest.project.cache_dir_path())?;
        Ok(Self { manifest, cache })
    }

    pub fn build(&mut self) -> anyhow::Result<()> {
        let project = &self.manifest.project;
        info!(project = %project.name, "Starting build");

        std::fs::create_dir_all(project.out_dir_path())?;
        std::fs::create_dir_all(project.cache_dir_path())?;

        let src_dir = project.src_dir_path();

        let mut registry = ChunkRegistry::default();
        let mut docs: Vec<(String, Document)> = Vec::new();

        for entry in WalkDir::new(&src_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map_or_else(|| false, |ext| ext == "stuff")
            })
        {
            let src_path = entry.path();
            let rel_path = match src_path.strip_prefix(&src_dir) {
                Ok(p) => p.to_string_lossy().to_string(),
                Err(_) => continue,
            };

            let content = match std::fs::read_to_string(src_path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(file = %rel_path, error = %e, "Failed to read source file");
                    continue;
                }
            };

            let doc = parse(&content);
            registry.collect_from(&doc, rel_path.clone());
            docs.push((rel_path, doc));
        }

        //  Render each file using the registry for cross-references.
        let mut processed = 0;
        let mut rebuilt = 0;

        for (rel_path, doc) in &docs {
            processed += 1;
            let abs_path = src_dir.join(rel_path);

            if self.cache.is_fresh(&src_dir, &abs_path) != crate::bs::cache::CacheStatus::UpToDate {
                rebuilt += 1;
                info!(file = %rel_path, "Rebuilding file");

                // Render each chunk individually and store HTML in registry
                // so transclusions can reference already-rendered chunks.
                populate_chunk_html(&mut registry, rel_path, doc);

                let ctx = crate::markup::assembler::RenderContext::new(rel_path, 0, &registry);
                let html = render_to_html(doc, &ctx);

                let mut out_path = project.out_dir_path().join(rel_path.clone());
                out_path.set_extension("html");

                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                std::fs::write(&out_path, html)?;
                self.cache.update(&src_dir, &abs_path);
            } else {
                debug!(file = %rel_path, "File up to date");
            }
        }

        self.cache.save(project.cache_dir_path())?;
        info!(processed, rebuilt, "Build complete");
        Ok(())
    }
}

/// Render each chunk individually and store its HTML in the registry.
/// This enables transclusions to reference rendered content from other chunks.
fn populate_chunk_html(registry: &mut ChunkRegistry, rel_path: &str, doc: &Document) {
    let chunk_count = doc.chunks.len();
    for i in 0..chunk_count {
        let chunk = &doc.chunks[i];
        let ctx = crate::markup::assembler::RenderContext::new(rel_path, i, registry);
        let chunk_html = render_chunk(chunk, &ctx);
        if let Some(chunk_infos) = registry.files.get_mut(rel_path) {
            if let Some(info) = chunk_infos.get_mut(i) {
                info.html = chunk_html;
            }
        }
    }
}
