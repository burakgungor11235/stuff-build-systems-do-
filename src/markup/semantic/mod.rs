pub mod intern;
pub mod link_graph;
pub mod resolve;
mod types;
mod viz;

pub mod deps;

pub use intern::{NameTable, StringArena};
pub use link_graph::LinkGraph;
pub use resolve::{resolve_ref, resolve_transclusion};
use std::cmp::min;
pub use types::{Chunk, ChunkId, DocId, Document, StringId};

use crate::markup::ast;
use rustc_hash::FxHashMap;

const SOURCE_EXT: &str = ".stuff";

#[inline]
pub(crate) fn normalize_path(p: &str) -> &str {
    p.strip_suffix(SOURCE_EXT).unwrap_or(p)
}

pub struct ChunkGraph {
    chunks: Vec<Chunk>,
    docs: Vec<Document>,
    strings: StringArena,
    path_to_docid: FxHashMap<String, DocId>,
    page_id_to_docid: Vec<Option<DocId>>,
}

impl Default for ChunkGraph {
    fn default() -> Self {
        Self {
            chunks: Vec::new(),
            docs: Vec::new(),
            strings: StringArena::with_capacity(64),
            path_to_docid: FxHashMap::default(),
            page_id_to_docid: Vec::new(),
        }
    }
}

pub fn normalize_page_name(name: &str) -> String {
    let stripped = name.strip_suffix(SOURCE_EXT).unwrap_or(name);
    stripped.to_lowercase().replace(' ', "-")
}

pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let la = a.len();
    let lb = b.len();
    if la < lb {
        return levenshtein_distance(b, a);
    }
    if lb == 0 {
        return la;
    }
    let mut prev: Vec<usize> = (0..=lb).collect();
    let mut curr = vec![0; lb + 1];
    for (i, ca) in a.chars().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.chars().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = min(min(curr[j] + 1, prev[j + 1] + 1), prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[lb]
}

#[derive(Default)]
pub struct RenderState {
    chunk_html: Vec<String>,
    chunk_ready: Vec<bool>,
    chunk_inlines: FxHashMap<ChunkId, Vec<ast::Inline>>,
}

impl RenderState {
    pub fn with_capacity(n: usize) -> Self {
        let mut chunk_html = Vec::with_capacity(n);
        let mut chunk_ready = Vec::with_capacity(n);
        for _ in 0..n {
            chunk_html.push(String::new());
            chunk_ready.push(false);
        }
        Self {
            chunk_html,
            chunk_ready,
            chunk_inlines: FxHashMap::default(),
        }
    }

    pub fn set(&mut self, chunk_id: ChunkId, html: String) {
        let idx = chunk_id.0 as usize;
        self.chunk_html[idx] = html;
        self.chunk_ready[idx] = true;
    }

    pub fn get(&self, chunk_id: ChunkId) -> Option<&str> {
        let idx = chunk_id.0 as usize;
        if idx < self.chunk_ready.len() && self.chunk_ready[idx] {
            Some(self.chunk_html[idx].as_str())
        } else {
            None
        }
    }

    pub fn store(&mut self, chunk_id: ChunkId, html: String, inlines: Vec<ast::Inline>) {
        let idx = chunk_id.0 as usize;
        self.chunk_html[idx] = html;
        self.chunk_ready[idx] = true;
        self.chunk_inlines.insert(chunk_id, inlines);
    }

    pub fn set_inlines(&mut self, chunk_id: ChunkId, inlines: Vec<ast::Inline>) {
        self.chunk_inlines.insert(chunk_id, inlines);
    }

    pub fn get_inlines(&self, chunk_id: ChunkId) -> Option<&[ast::Inline]> {
        self.chunk_inlines.get(&chunk_id).map(|v| v.as_slice())
    }
}

impl ChunkGraph {
    #[cfg(test)]
    pub fn new_for_test(docs: Vec<(String, Vec<ChunkId>)>) -> Self {
        let mut chunks = Vec::new();
        let mut documents = Vec::new();
        let strings = StringArena::with_capacity(16);
        let mut path_to_docid = FxHashMap::default();

        for (doc_idx, (rel_path, chunk_ids)) in docs.into_iter().enumerate() {
            let doc_id = DocId(doc_idx as u32);
            let mut mapped_ids = Vec::new();
            for (c_idx, _cid) in chunk_ids.into_iter().enumerate() {
                let new_cid = ChunkId(chunks.len() as u32);
                mapped_ids.push(new_cid);
                chunks.push(Chunk {
                    id: new_cid,
                    doc: doc_id,
                    index: c_idx,
                    name: None,
                    heading: None,
                    first_inline_text: None,
                });
            }
            let normalized = normalize_path(&rel_path).to_string();
            path_to_docid.insert(normalized.clone(), doc_id);
            documents.push(Document {
                id: doc_id,
                rel_path: normalized,
                chunk_ids: mapped_ids,
            });
        }

        Self {
            chunks,
            docs: documents,
            strings,
            path_to_docid,
            page_id_to_docid: Vec::new(),
        }
    }

    pub fn add_document(
        &mut self,
        doc: &ast::Document,
        rel_path: String,
        names: &mut NameTable,
    ) -> DocId {
        let id = DocId(self.docs.len() as u32);
        let mut chunk_ids = Vec::new();

        for (chunk_idx, chunk) in doc.chunks.iter().enumerate() {
            let cid = ChunkId(self.chunks.len() as u32);
            chunk_ids.push(cid);

            let chunk_name = match chunk {
                ast::Chunk::Implicit { name, .. } => {
                    name.as_deref().map(|s| self.strings.intern(s))
                }
                ast::Chunk::Explicit { name, .. } => Some(self.strings.intern(name)),
            };

            let first_block = chunk.blocks().next();
            let (heading, first_inline_text) = match first_block {
                Some(block) if block.is_heading() => {
                    let id = block.heading_and_text().map(|t| self.strings.intern(&t));
                    (id, id)
                }
                Some(block) => {
                    let id = block.heading_and_text().map(|t| self.strings.intern(&t));
                    (None, id)
                }
                None => (None, None),
            };

            self.chunks.push(Chunk {
                id: cid,
                doc: id,
                index: chunk_idx,
                name: chunk_name,
                heading,
                first_inline_text,
            });
        }

        let normalized = normalize_path(&rel_path).to_string();
        self.path_to_docid.insert(normalized.clone(), id);

        let page_key = normalize_page_name(&normalized);
        let page_id = names.intern(&page_key);
        if page_id >= self.page_id_to_docid.len() {
            self.page_id_to_docid.resize(page_id + 1, None);
        }
        self.page_id_to_docid[page_id] = Some(id);

        self.docs.push(Document {
            id,
            rel_path: normalized,
            chunk_ids,
        });
        id
    }

    #[must_use]
    pub fn chunk(&self, id: ChunkId) -> Option<&Chunk> {
        self.chunks.get(id.0 as usize)
    }

    #[must_use]
    pub fn doc(&self, doc_id: DocId) -> Option<&Document> {
        self.docs.get(doc_id.0 as usize)
    }

    #[must_use]
    pub fn chunks_in(&self, doc_id: DocId) -> &[ChunkId] {
        self.docs
            .get(doc_id.0 as usize)
            .map(|d| d.chunk_ids.as_slice())
            .unwrap_or(&[])
    }

    #[must_use]
    pub fn doc_by_path(&self, rel_path: &str) -> Option<DocId> {
        let normalized = normalize_path(rel_path);
        self.path_to_docid.get(normalized).copied()
    }

    #[must_use]
    pub fn resolve_wiki_page(&self, page_id: usize) -> Option<DocId> {
        self.page_id_to_docid.get(page_id).copied().flatten()
    }


    pub fn all_doc_ids(&self) -> Vec<DocId> {
        self.docs.iter().map(|d| d.id).collect()
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    #[must_use]
    pub fn resolve_ref(
        &self,
        expr: &ast::RefExpr,
        current_file: &str,
        current_idx: usize,
    ) -> Option<&Chunk> {
        resolve_ref(self, expr, current_file, current_idx)
    }

    #[must_use]
    pub fn resolve_transclusion(
        &self,
        expr: &ast::RefExpr,
        current_file: &str,
        current_idx: usize,
    ) -> Vec<&Chunk> {
        resolve_transclusion(self, expr, current_file, current_idx)
    }

    #[must_use]
    pub fn string(&self, id: StringId) -> &str {
        self.strings.get(id)
    }

    #[must_use]
    pub fn get_chunks(&self, file: &str) -> Option<&[ChunkId]> {
        let normalized = normalize_path(file);
        self.path_to_docid
            .get(normalized)
            .map(|&id| self.chunks_in(id))
    }

    #[must_use]
    pub fn has_name(&self, cid: ChunkId, name: &str) -> bool {
        self.chunk(cid)
            .and_then(|c| c.name.map(|id| self.strings.get(id) == name))
            .unwrap_or(false)
    }

    #[must_use]
    pub fn heading_matches(&self, cid: ChunkId, heading: &str) -> bool {
        self.chunk(cid)
            .and_then(|c| {
                c.heading.map(|id| {
                    let stored = self.strings.get(id);
                    stored.len() == heading.len()
                        && stored
                            .as_bytes()
                            .iter()
                            .zip(heading.as_bytes())
                            .all(|(a, b)| a.eq_ignore_ascii_case(b))
                })
            })
            .unwrap_or(false)
    }

    pub fn chunks_under_heading(
        &self,
        chunk_ids: &[ChunkId],
        heading: &str,
    ) -> Option<Vec<ChunkId>> {
        let start = chunk_ids
            .iter()
            .position(|&cid| self.heading_matches(cid, heading))?;

        let end = chunk_ids
            .iter()
            .enumerate()
            .skip(start + 1)
            .find(|(_, &cid)| {
                self.chunk(cid)
                    .map(|c| c.heading.is_some())
                    .unwrap_or(false)
            })
            .map(|(i, _)| i)
            .unwrap_or(chunk_ids.len());
        Some(chunk_ids[start + 1..end].to_vec())

        // writing loops are my passion.
    }
}
