mod arena;
mod resolve;
mod types;

pub mod deps;
pub mod images;
pub mod plugin;
pub mod validate;

pub use arena::StringArena;
pub use resolve::{resolve_ref, resolve_transclusion};
use tracing::warn;
pub use types::{Chunk, ChunkId, DocId, Document, StringId};

use crate::markup::ast;
use rustc_hash::FxHashMap;

const SOURCE_EXT: &str = ".stuff";

#[inline]
fn normalize_path(p: &str) -> &str {
    p.strip_suffix(SOURCE_EXT).unwrap_or(p)
}

pub struct ChunkGraph {
    chunks: Vec<Chunk>,
    docs: Vec<Document>,
    strings: StringArena,
    path_to_docid: FxHashMap<String, DocId>,
}

impl Default for ChunkGraph {
    fn default() -> Self {
        Self {
            chunks: Vec::new(),
            docs: Vec::new(),
            strings: StringArena::with_capacity(64),
            path_to_docid: FxHashMap::default(),
        }
    }
}

#[derive(Default)]
pub struct RenderState {
    chunk_html: FxHashMap<ChunkId, String>,
    chunk_inlines: FxHashMap<ChunkId, Vec<ast::Inline>>,
}

impl RenderState {
    pub fn set(&mut self, chunk_id: ChunkId, html: String) {
        self.chunk_html.insert(chunk_id, html);
    }

    pub fn get(&self, chunk_id: ChunkId) -> Option<&str> {
        self.chunk_html.get(&chunk_id).map(|s| s.as_str())
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
        }
    }

    pub fn add_document(&mut self, doc: &ast::Document, rel_path: String) -> DocId {
        let id = DocId(self.docs.len() as u32);
        let mut chunk_ids = Vec::new();

        for (chunk_idx, chunk) in doc.chunks.iter().enumerate() {
            let cid = ChunkId(self.chunks.len() as u32);
            chunk_ids.push(cid);

            let chunk_name = match chunk {
                ast::Chunk::Implicit { name, .. } => name.as_deref().map(|s| self.strings.intern(s)),
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
        self.path_to_docid.get(normalized).map(|&id| self.chunks_in(id))
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

    #[must_use]
    pub fn normalize_idx(&self, idx: i32, len: i32) -> Option<i32> {
        if idx >= 0 && idx < len {
            Some(idx)
        } else {
            warn!("something has gone verry wrong");
            None
        }
    }
}
