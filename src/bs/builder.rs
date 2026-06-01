use crate::bs::cas::Cas;
use crate::bs::config::Manifest;
use crate::markup::assembler::{render_chunk, HtmlPage, RenderContext};
use crate::markup::ast::{extract_transclusion_refs, extract_wiki_links, Document};
use crate::markup::parser::parse_with_names;
use crate::markup::semantic::deps::DependencyTracker;
use crate::markup::semantic::link_graph::{LinkEdge, LinkGraphBuilder};
use crate::markup::semantic::resolve;
use crate::markup::semantic::{
    normalize_path, ChunkGraph, ChunkId, DocId, LinkGraph, NameTable, RenderState,
};

use rustc_hash::FxHashMap;
use std::collections::HashSet;
use std::path::Path;
use tracing::{info, warn};

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
        let mut names = NameTable::default();
        let parsed = self.parse_source_files(&src_dir, &mut names)?;
        let mut graph = ChunkGraph::default();
        let mut path_to_docid: FxHashMap<String, DocId> = FxHashMap::default();
        let mut id_to_parsed_idx: FxHashMap<DocId, usize> = FxHashMap::default();

        for (i, p) in parsed.iter().enumerate() {
            let doc_id = graph.add_document(&p.doc, p.rel_path.clone(), &mut names);
            path_to_docid.insert(normalize_path(&p.rel_path).to_string(), doc_id);
            id_to_parsed_idx.insert(doc_id, i);
        }

        // basically.
        //
        //
        //  let there be documents
        //      then let them be connected              (forward)
        //          and then connected                  (reverse)
        //              then sorted
        //                  then rendered               ( fresh )
        //                      then rendered           ( dirty )
        //                          then saved.

        let n_docs = graph.all_doc_ids().len();
        let link_graph = {
            let mut lb = LinkGraphBuilder::new();
            for p in &parsed {
                let doc_id = path_to_docid[normalize_path(&p.rel_path)];
                for (chunk_idx, &chunk_id) in graph.chunks_in(doc_id).iter().enumerate() {
                    let chunk = &p.doc.chunks[chunk_idx];
                    for entry in extract_wiki_links(chunk) {
                        if let Some(target_doc) = graph.resolve_wiki_page(entry.page_id) {
                            let display_id = names.intern(&entry.display_plain);
                            let context_id = names.intern(&entry.context_plain);
                            lb.add_edge(LinkEdge {
                                source_doc: doc_id,
                                source_chunk: chunk_id,
                                target_doc,
                                target_chunk: None,
                                page_id: entry.page_id,
                                display_id,
                                context_id,
                            });
                        }
                    }
                }
            }
            lb.build(n_docs)
        };

        let (chunk_order, doc_order) = Self::compute_render_order(&parsed, &graph, &path_to_docid);

        let (stale_docs, mut render_state, reverse_deps) = self.render_chunks_pass1(
            &parsed,
            &graph,
            &path_to_docid,
            &chunk_order,
            &names,
            &link_graph,
        )?;

        let transitive_dirty = self.compute_transitive_dirty(&graph, &stale_docs, &reverse_deps)?;

        self.rerender_chunks(
            &parsed,
            &graph,
            &path_to_docid,
            &mut render_state,
            &stale_docs,
            &transitive_dirty,
            &chunk_order,
            &names,
            &link_graph,
        )?;

        self.write_documents_pass2(
            &parsed,
            &graph,
            &id_to_parsed_idx,
            &render_state,
            &stale_docs,
            &transitive_dirty,
            &doc_order,
            &names,
            &link_graph,
        )?;

        self.cas.save()?;
        info!(
            processed = parsed.len(),
            rebuilt = self.rebuilt_count,
            "Build complete"
        );
        Ok(())
    }

    fn parse_source_files(
        &self,
        src_dir: &Path,
        names: &mut NameTable,
    ) -> anyhow::Result<Vec<ParsedDoc>> {
        let mut docs = Vec::new();
        for entry in self.manifest.project.walk_source_files() {
            let src_path = entry.path();
            let rel_path = match src_path.strip_prefix(src_dir) {
                Ok(p) => p.to_string_lossy().to_string(),
                Err(_) => continue,
            };

            match Cas::read_and_hash(src_path) {
                Ok((content, content_hash)) => {
                    let doc = parse_with_names(&content, names);
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
        names: &NameTable,
        link_graph: &LinkGraph,
    ) -> anyhow::Result<(HashSet<DocId>, RenderState, FxHashMap<DocId, Vec<ChunkId>>)> {
        let mut reverse_deps: FxHashMap<DocId, Vec<ChunkId>> = FxHashMap::default();
        let mut stale_docs: HashSet<DocId> = HashSet::new();
        let mut render_state = RenderState::with_capacity(graph.chunk_count());

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
                names,
                link_graph,
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
        names: &NameTable,
        link_graph: &LinkGraph,
    ) -> anyhow::Result<(String, bool)> {
        if let Some(cached) = self.cas.get(key) {
            return Ok((String::from_utf8(cached).unwrap_or_default(), true));
        }

        let ctx =
            RenderContext::for_chunk(rel_path, chunk_idx, graph, render_state, names, link_graph);
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
                    let targets = resolve::resolve_to_ids(graph, expr, &p.rel_path, chunk_idx);

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
        names: &NameTable,
        link_graph: &LinkGraph,
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
                    names,
                    link_graph,
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
        names: &NameTable,
        link_graph: &LinkGraph,
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

            let body: String = graph
                .chunks_in(doc_id)
                .iter()
                .filter_map(|&cid| render_state.get(cid))
                .collect();

            let mut page = HtmlPage::new(body);
            let orphan = link_graph.links_to(doc_id).is_empty();
            page.set_orphan(orphan);

            for edge in link_graph.links_from(doc_id) {
                let target_doc = graph.doc(edge.target_doc).expect("target doc in graph");
                let href = format!("{}.html", target_doc.rel_path);
                let text = names.get(edge.display_id);
                page.add_outlink(&href, text);
            }
            for edge in link_graph.links_to(doc_id) {
                let source_doc = graph.doc(edge.source_doc).expect("source doc in graph");
                let href = format!("{}.html", source_doc.rel_path);
                let text = names.get(edge.display_id);
                page.add_backlink(&href, text);
            }

            let html = page.render();
            let out_path = project.output_path_for_source(&p.rel_path);

            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&out_path, html)?;
        }

        Ok(())
    }
}
