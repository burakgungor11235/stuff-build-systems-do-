use crate::bs::config::Manifest;
use crate::markup::semantic::deps::DependencyTracker;
use crate::markup::semantic::{ChunkGraph, ChunkId, DocId, RenderState};
use crate::markup::assembler::{render_chunk, render_to_html, RenderContext};
use crate::markup::ast::{Document, extract_transclusion_refs};
use crate::markup::parser::parse;

use rustc_hash::FxHashMap;
use std::collections::HashSet;
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

        let mut graph = ChunkGraph::default();
        let mut render_state = RenderState::default();
        let mut docs: Vec<(String, Document, u64)> = Vec::new();

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
            let abs_path = src_dir.join(&rel_path);
            let mtime = std::fs::metadata(&abs_path)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            graph.add_document(&doc, rel_path.clone());
            docs.push((rel_path, doc, mtime));
        }

        let processed = docs.len();
        let mut rebuilt = 0;

        let mut reverse_deps: FxHashMap<DocId, Vec<ChunkId>> = FxHashMap::default();
        let mut stale_docs: HashSet<DocId> = HashSet::new();

        for (rel_path, doc, mtime) in &docs {
            let abs_path = src_dir.join(rel_path);
            let doc_id = graph.doc_by_path(rel_path).unwrap();
            let chunk_ids = graph.chunks_in(doc_id);

            if self.cache.is_fresh(&src_dir, &abs_path) != crate::bs::cache::CacheStatus::UpToDate {
                stale_docs.insert(doc_id);
            }

            for (chunk_idx, chunk) in doc.chunks.iter().enumerate() {
                let ctx = RenderContext::new(rel_path, chunk_idx, &graph, &render_state);
                let chunk_html = render_chunk(chunk, &ctx);
                if let Some(&chunk_id) = chunk_ids.get(chunk_idx) {
                    render_state.set(chunk_id, chunk_html);

                    for expr in extract_transclusion_refs(chunk) {
                        let targets = graph.resolve_transclusion(expr, rel_path, chunk_idx);
                        for target in targets {
                            if target.doc != doc_id {
                                reverse_deps
                                    .entry(target.doc)
                                    .or_default()
                                    .push(chunk_id);
                            }
                        }
                    }
                }
            }
        }

        let transitive_dirty = DependencyTracker::transitive_deps_with_reverse(
            &graph,
            &reverse_deps,
            &stale_docs,
        );

        for (rel_path, doc, _mtime) in &docs {
            let abs_path = src_dir.join(rel_path);
            let doc_id = graph.doc_by_path(rel_path).unwrap();

            if stale_docs.contains(&doc_id) || transitive_dirty.contains(&doc_id) {
                if !stale_docs.contains(&doc_id) {
                    info!(file = %rel_path, "Rebuilding (transitive dependency changed)");
                } else {
                    info!(file = %rel_path, "Rebuilding file");
                }
                rebuilt += 1;

                let ctx = RenderContext::new(rel_path, 0, &graph, &render_state);
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
