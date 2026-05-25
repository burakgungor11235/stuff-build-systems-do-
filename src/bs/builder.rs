use crate::bs::cas::Cas;
use crate::bs::config::Manifest;
use crate::markup::assembler::{render_chunk, RenderContext};
use crate::markup::ast::{extract_transclusion_refs, Document, RefExpr};
use crate::markup::parser::parse;
use crate::markup::semantic::deps::DependencyTracker;
use crate::markup::semantic::{ChunkGraph, ChunkId, DocId, RenderState};

use rustc_hash::FxHashMap;
use std::collections::HashSet;
use std::path::Path;
use tracing::{info, warn};

const SOURCE_EXT: &str = ".stuff";

fn normalize_path(p: &str) -> &str {
    p.strip_suffix(SOURCE_EXT).unwrap_or(p)
}

struct ParsedDoc {
    rel_path: String,
    doc: Document,
    content_hash: Vec<u8>,
}

pub struct Builder {
    manifest: Manifest,
    cas: Cas,
    rebuilt_count: usize,
}

impl Builder {
    pub fn new(manifest: Manifest) -> anyhow::Result<Self> {
        let cas = Cas::load(&manifest.project.cache_dir_path())?;
        Ok(Self {
            manifest,
            cas,
            rebuilt_count: 0,
        })
    }

    #[allow(unused)] // used in tests n stuff
    #[must_use]
    pub fn rebuilt_count(&self) -> usize {
        self.rebuilt_count
    }

    pub fn build(&mut self) -> anyhow::Result<()> {
        let project = &self.manifest.project;
        info!(project = %project.name, "Starting build");

        std::fs::create_dir_all(project.out_dir_path())?;
        std::fs::create_dir_all(project.cache_dir_path())?;

        let src_dir = project.src_dir_path();
        let parsed = self.parse_source_files(&src_dir)?;
        let mut graph = ChunkGraph::default();
        let mut path_to_docid: FxHashMap<String, DocId> = FxHashMap::default();
        let mut id_to_parsed_idx: FxHashMap<DocId, usize> = FxHashMap::default();

        for (i, p) in parsed.iter().enumerate() {
            let doc_id = graph.add_document(&p.doc, p.rel_path.clone());
            path_to_docid.insert(normalize_path(&p.rel_path).to_string(), doc_id);
            id_to_parsed_idx.insert(doc_id, i);
        }

        let (chunk_order, doc_order) = Self::compute_render_order(&parsed, &graph, &path_to_docid);
        let (stale_docs, mut render_state, reverse_deps) =
            self.render_chunks_pass1(&parsed, &graph, &path_to_docid, &chunk_order)?;
        let transitive_dirty = self.compute_transitive_dirty(&graph, &stale_docs, &reverse_deps)?;
        self.rerender_chunks(
            &parsed,
            &graph,
            &path_to_docid,
            &mut render_state,
            &stale_docs,
            &transitive_dirty,
            &chunk_order,
        )?;
        self.write_documents_pass2(
            &parsed,
            &graph,
            &id_to_parsed_idx,
            &render_state,
            &stale_docs,
            &transitive_dirty,
            &doc_order,
        )?;

        self.cas.save()?;
        info!(
            processed = parsed.len(),
            rebuilt = self.rebuilt_count,
            "Build complete"
        );
        Ok(())
    }

    fn parse_source_files(&self, src_dir: &Path) -> anyhow::Result<Vec<ParsedDoc>> {
        let mut docs = Vec::new();
        for entry in self.manifest.project.walk_source_files() {
            let src_path = entry.path();
            let rel_path = match src_path.strip_prefix(src_dir) {
                Ok(p) => p.to_string_lossy().to_string(),
                Err(_) => continue,
            };

            match Cas::read_and_hash(src_path) {
                Ok((content, content_hash)) => {
                    let doc = parse(&content);
                    docs.push(ParsedDoc {
                        rel_path,
                        doc,
                        content_hash,
                    });
                }
                Err(e) => Self::warn_log(&rel_path, &e),
            }
        }
        Ok(docs)
    }

    #[cold]
    fn warn_log(rel_path: &str, e: &std::io::Error) {
        warn!(file = %rel_path, error = %e, "Failed to read source file");
    }

    fn render_chunks_pass1(
        &mut self,
        parsed: &[ParsedDoc],
        graph: &ChunkGraph,
        path_to_docid: &FxHashMap<String, DocId>,
        order: &[(usize, usize)],
    ) -> anyhow::Result<(HashSet<DocId>, RenderState, FxHashMap<DocId, Vec<ChunkId>>)> {
        let mut reverse_deps: FxHashMap<DocId, Vec<ChunkId>> = FxHashMap::default();
        let mut stale_docs: HashSet<DocId> = HashSet::new();
        let mut render_state = RenderState::default();

        for &(doc_idx, chunk_idx) in order {
            let p = &parsed[doc_idx];
            let doc_id = path_to_docid[normalize_path(&p.rel_path)];
            let chunk_ids = graph.chunks_in(doc_id);
            let chunk_id = chunk_ids[chunk_idx];
            let key = Cas::compute_chunk_key(&p.rel_path, chunk_idx, &p.content_hash);
            let chunk = &p.doc.chunks[chunk_idx];

            let (chunk_html, was_cached) = self.render_or_cache_chunk(
                &key,
                chunk,
                &p.rel_path,
                chunk_idx,
                graph,
                &render_state,
            )?;

            if !was_cached {
                stale_docs.insert(doc_id);
            }

            render_state.set(chunk_id, chunk_html);
            render_state.set_inlines(chunk_id, chunk.inline_content());
            self.track_transclusion_deps(
                chunk,
                &p.rel_path,
                chunk_idx,
                graph,
                doc_id,
                chunk_id,
                &mut reverse_deps,
            );
        }

        Ok((stale_docs, render_state, reverse_deps))
    }

    fn render_or_cache_chunk(
        &mut self,
        key: &crate::bs::cas::Hash,
        chunk: &crate::markup::ast::Chunk,
        rel_path: &str,
        chunk_idx: usize,
        graph: &ChunkGraph,
        render_state: &RenderState,
    ) -> anyhow::Result<(String, bool)> {
        if let Some(cached) = self.cas.get(key) {
            return Ok((String::from_utf8(cached).unwrap_or_default(), true));
        }

        let ctx = RenderContext::for_chunk(rel_path, chunk_idx, graph, render_state);
        let html = render_chunk(chunk, &ctx);
        self.cas.put(key, html.as_bytes())?;
        Ok((html, false))
    }

    #[inline]
    fn track_transclusion_deps(
        &self,
        chunk: &crate::markup::ast::Chunk,
        rel_path: &str,
        chunk_idx: usize,
        graph: &ChunkGraph,
        doc_id: DocId,
        chunk_id: ChunkId,
        reverse_deps: &mut FxHashMap<DocId, Vec<ChunkId>>,
    ) {
        for expr in extract_transclusion_refs(chunk) {
            let targets = graph.resolve_transclusion(expr, rel_path, chunk_idx);
            for target in targets {
                if target.doc != doc_id {
                    reverse_deps.entry(target.doc).or_default().push(chunk_id);
                }
            }
        }
    }

    fn compute_transitive_dirty(
        &self,
        graph: &ChunkGraph,
        stale_docs: &HashSet<DocId>,
        reverse_deps: &FxHashMap<DocId, Vec<ChunkId>>,
    ) -> anyhow::Result<HashSet<DocId>> {
        Ok(DependencyTracker::transitive_deps_with_reverse(
            graph,
            reverse_deps,
            stale_docs,
        ))
    }

    /// Transclusion resolution using a DocId map,
    /// avoiding O(N) linear search inside ChunkGraph::doc_by_path.
    fn resolve_dep_ids(
        expr: &RefExpr,
        current_doc_id: DocId,
        chunk_idx: usize,
        path_to_docid: &FxHashMap<String, DocId>,
        graph: &ChunkGraph,
    ) -> Vec<ChunkId> {
        match expr {
            RefExpr::Named(name) => {
                let ids = graph.chunks_in(current_doc_id);
                ids.iter()
                    .find(|&&cid| graph.has_name(cid, name))
                    .copied()
                    .map(|c| vec![c])
                    .unwrap_or_default()
            }

            RefExpr::Relative(offset) => {
                let ids = graph.chunks_in(current_doc_id);
                let target = chunk_idx as i32 + offset;
                ids.get(target as usize)
                    .copied()
                    .map(|c| vec![c])
                    .unwrap_or_default()
            }

            RefExpr::Absolute(idx) => {
                let ids = graph.chunks_in(current_doc_id);
                ids.get(*idx).copied().map(|c| vec![c]).unwrap_or_default()
            }

            RefExpr::Range(start, end) => {
                let ids = graph.chunks_in(current_doc_id);
                let len = ids.len() as i32;
                let s = range_idx(chunk_idx as i32 + start, len);
                let e = range_idx(chunk_idx as i32 + end, len);
                let (lo, hi) = if s <= e { (s, e) } else { (e, s) };
                ids.get(lo as usize..=hi as usize)
                    .map(|sl| sl.to_vec())
                    .unwrap_or_default()
            }

            RefExpr::List(exprs) => {
                let mut result = Vec::new();
                for e in exprs {
                    result.extend(Self::resolve_dep_ids(
                        e,
                        current_doc_id,
                        chunk_idx,
                        path_to_docid,
                        graph,
                    ));
                }
                result
            }

            RefExpr::FileByIndex(file, idx) => {
                if let Some(&doc_id) = path_to_docid.get(normalize_path(file)) {
                    let ids = graph.chunks_in(doc_id);
                    ids.get(*idx).copied().map(|c| vec![c]).unwrap_or_default()
                } else {
                    vec![]
                }
            }

            RefExpr::FileByName(file, name) => {
                if let Some(&doc_id) = path_to_docid.get(normalize_path(file)) {
                    let ids = graph.chunks_in(doc_id);
                    ids.iter()
                        .find(|&&cid| graph.has_name(cid, name))
                        .copied()
                        .map(|c| vec![c])
                        .unwrap_or_default()
                } else {
                    vec![]
                }
            }

            RefExpr::FileByHeading(file, heading) => {
                if let Some(&doc_id) = path_to_docid.get(normalize_path(file)) {
                    let ids = graph.chunks_in(doc_id);
                    ids.iter()
                        .find(|&&cid| graph.heading_matches(cid, heading))
                        .copied()
                        .map(|c| vec![c])
                        .unwrap_or_default()
                } else {
                    vec![]
                }
            }

            RefExpr::FileByHeadingIndex(file, heading, idx) => {
                if let Some(&doc_id) = path_to_docid.get(normalize_path(file)) {
                    let ids = graph.chunks_in(doc_id);
                    graph
                        .chunks_under_heading(ids, heading)
                        .and_then(|under| under.into_iter().nth(*idx))
                        .map(|c| vec![c])
                        .unwrap_or_default()
                } else {
                    vec![]
                }
            }

            RefExpr::FileByHeadingName(file, heading, name) => {
                if let Some(&doc_id) = path_to_docid.get(normalize_path(file)) {
                    let ids = graph.chunks_in(doc_id);
                    graph
                        .chunks_under_heading(ids, heading)
                        .and_then(|under| under.into_iter().find(|&cid| graph.has_name(cid, name)))
                        .map(|c| vec![c])
                        .unwrap_or_default()
                } else {
                    vec![]
                }
            }

            RefExpr::HeadingRange(heading) => {
                let ids = graph.chunks_in(current_doc_id);
                graph
                    .chunks_under_heading(ids, heading)
                    .and_then(|under| under.into_iter().next())
                    .map(|c| vec![c])
                    .unwrap_or_default()
            }
        }
    }

    /// Topological sort of chunks by transclusion dependencies.
    fn compute_render_order(
        parsed: &[ParsedDoc],
        graph: &ChunkGraph,
        path_to_docid: &FxHashMap<String, DocId>,
    ) -> (Vec<(usize, usize)>, Vec<DocId>) {
        let mut deps: FxHashMap<ChunkId, Vec<ChunkId>> = FxHashMap::default();

        parsed.iter().for_each(|p| {
            let doc_id = path_to_docid[normalize_path(&p.rel_path)];

            // I could have written this with for_each but it is harder to understand.

            for (chunk_idx, &chunk_id) in graph.chunks_in(doc_id).iter().enumerate() {
                let chunk = &p.doc.chunks[chunk_idx];
                let mut chunk_deps = Vec::new();

                for expr in extract_transclusion_refs(chunk) {
                    let targets =
                        Self::resolve_dep_ids(expr, doc_id, chunk_idx, path_to_docid, graph);

                    for t in targets.into_iter() {
                        if let Some(c) = graph.chunk(t) {
                            if c.doc != doc_id || c.id != chunk_id {
                                chunk_deps.push(t);
                            }
                        }
                    }
                }
                deps.insert(chunk_id, chunk_deps);
            }
        });

        let mut in_deg: FxHashMap<ChunkId, usize> = FxHashMap::default();
        let mut forward: FxHashMap<ChunkId, Vec<ChunkId>> = FxHashMap::default();

        for (&cid, cdeps) in &deps {
            *in_deg.entry(cid).or_insert(0) += cdeps.len();
            cdeps.iter().for_each(|&dep| {
                forward.entry(dep).or_default().push(cid);
            });
        }

        let mut q: Vec<ChunkId> = in_deg
            .iter()
            .filter(|(_, &d)| d == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut order_ids = Vec::with_capacity(deps.len());

        while let Some(cid) = q.pop() {
            order_ids.push(cid);

            if let Some(dependents) = forward.remove(&cid) {
                for &dep in &dependents {
                    if let Some(d) = in_deg.get_mut(&dep) {
                        *d -= 1;
                        if *d == 0 {
                            q.push(dep);
                        }
                    }
                }
            }
        }

        for (&cid, &d) in &in_deg {
            if d > 0 {
                order_ids.push(cid);
            }
        }

        let mut cid_to_pos: FxHashMap<ChunkId, (usize, usize)> = FxHashMap::default();

        for (doc_idx, p) in parsed.iter().enumerate() {
            let doc_id = path_to_docid[normalize_path(&p.rel_path)];
            for (chunk_idx, &cid) in graph.chunks_in(doc_id).iter().enumerate() {
                cid_to_pos.insert(cid, (doc_idx, chunk_idx));
            }
        }

        let mut chunk_order = Vec::with_capacity(order_ids.len());
        let mut seen_docs: HashSet<DocId> = HashSet::new();
        let mut doc_order = Vec::new();

        for &cid in &order_ids {
            if let Some(&pos) = cid_to_pos.get(&cid) {
                chunk_order.push(pos);
                if let Some(c) = graph.chunk(cid) {
                    if seen_docs.insert(c.doc) {
                        doc_order.push(c.doc);
                    }
                }
            }
        }
        (chunk_order, doc_order)
    }

    fn rerender_chunks(
        &mut self,
        parsed: &[ParsedDoc],
        graph: &ChunkGraph,
        path_to_docid: &FxHashMap<String, DocId>,
        render_state: &mut RenderState,
        stale_docs: &HashSet<DocId>,
        transitive_dirty: &HashSet<DocId>,
        chunk_order: &[(usize, usize)],
    ) -> anyhow::Result<()> {
        let to_rerender: Vec<DocId> = transitive_dirty.difference(stale_docs).copied().collect();
        if to_rerender.is_empty() {
            return Ok(());
        }

        for &(doc_idx, chunk_idx) in chunk_order {
            let doc_id = path_to_docid[normalize_path(&parsed[doc_idx].rel_path)];
            if !transitive_dirty.contains(&doc_id) || stale_docs.contains(&doc_id) {
                continue;
            }
            let chunk_ids = graph.chunks_in(doc_id);
            let chunk_id = chunk_ids[chunk_idx];
            let chunk = &parsed[doc_idx].doc.chunks[chunk_idx];
            let chunk_html = {
                let ctx = RenderContext::for_chunk(
                    &parsed[doc_idx].rel_path,
                    chunk_idx,
                    graph,
                    &*render_state,
                );
                render_chunk(chunk, &ctx)
            };
            render_state.set(chunk_id, chunk_html);
            render_state.set_inlines(chunk_id, chunk.inline_content());
        }
        Ok(())
    }

    fn write_documents_pass2(
        &mut self,
        parsed: &[ParsedDoc],
        graph: &ChunkGraph,
        id_to_parsed_idx: &FxHashMap<DocId, usize>,
        render_state: &RenderState,
        stale_docs: &HashSet<DocId>,
        transitive_dirty: &HashSet<DocId>,
        doc_order: &[DocId],
    ) -> anyhow::Result<()> {
        let project = &self.manifest.project;
        let dirty: HashSet<DocId> = stale_docs.union(transitive_dirty).copied().collect();
        let mut written: HashSet<DocId> = HashSet::new();

        for &doc_id in doc_order {
            if !dirty.contains(&doc_id) {
                continue;
            }
            if !written.insert(doc_id) {
                continue;
            }

            let &parsed_idx = id_to_parsed_idx
                .get(&doc_id)
                .expect("doc in graph must be in parsed");
            let p = &parsed[parsed_idx];

            if stale_docs.contains(&doc_id) {
                info!(file = %p.rel_path, "Rebuilding file");
            } else {
                info!(file = %p.rel_path, "Rebuilding (transitive dependency changed)");
            }
            self.rebuilt_count += 1;

            let html: String = graph
                .chunks_in(doc_id)
                .iter()
                .filter_map(|&cid| render_state.get(cid))
                .collect();
            let out_path = project.output_path_for_source(&p.rel_path);

            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&out_path, html)?;
        }

        Ok(())
    }
}

fn range_idx(idx: i32, len: i32) -> i32 {
    if idx < 0 {
        0
    } else if idx >= len {
        len - 1
    } else {
        idx
    }
}
