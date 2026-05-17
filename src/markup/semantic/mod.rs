mod arena;
mod resolve;
mod types;

pub mod cache;
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

pub struct ChunkGraph {
    chunks: Vec<Chunk>,
    docs: Vec<Document>,
    strings: StringArena,
}

impl Default for ChunkGraph {
    fn default() -> Self {
        Self {
            chunks: Vec::new(),
            docs: Vec::new(),
            strings: StringArena::with_capacity(64),
        }
    }
}

#[derive(Default)]
pub struct RenderState {
    chunk_html: FxHashMap<ChunkId, String>,
}

impl RenderState {
    pub fn set(&mut self, chunk_id: ChunkId, html: String) {
        self.chunk_html.insert(chunk_id, html);
    }

    pub fn get(&self, chunk_id: ChunkId) -> Option<&str> {
        self.chunk_html.get(&chunk_id).map(|s| s.as_str())
    }
}

impl ChunkGraph {
    #[cfg(test)]
    pub fn new_for_test(docs: Vec<(String, Vec<ChunkId>)>) -> Self {
        let mut chunks = Vec::new();
        let mut documents = Vec::new();
        let strings = StringArena::with_capacity(16);

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
            documents.push(Document {
                id: doc_id,
                rel_path,
                chunk_ids: mapped_ids,
            });
        }

        Self {
            chunks,
            docs: documents,
            strings,
        }
    }

    pub fn add_document(&mut self, doc: &ast::Document, rel_path: String) -> DocId {
        let id = DocId(self.docs.len() as u32);
        let mut chunk_ids = Vec::new();

        for (chunk_idx, chunk) in doc.chunks.iter().enumerate() {
            let cid = ChunkId(self.chunks.len() as u32);
            chunk_ids.push(cid);

            let (name, heading, first_inline_text) = match chunk {
                ast::Chunk::Implicit { name, block } => {
                    let (h, fit) = extract_heading_and_text(&mut self.strings, block);
                    (name.as_deref().map(|s| self.strings.intern(s)), h, fit)
                }
                ast::Chunk::Explicit { name, blocks } => {
                    let first_block = blocks.first();
                    let (h, fit) = first_block
                        .map(|b| extract_heading_and_text(&mut self.strings, b))
                        .unwrap_or((None, None));
                    (Some(self.strings.intern(name)), h, fit)
                }
            };

            self.chunks.push(Chunk {
                id: cid,
                doc: id,
                index: chunk_idx,
                name,
                heading,
                first_inline_text,
            });
        }

        self.docs.push(Document {
            id,
            rel_path,
            chunk_ids,
        });
        id
    }

    pub fn chunk(&self, id: ChunkId) -> Option<&Chunk> {
        self.chunks.get(id.0 as usize)
    }

    pub fn chunks_in(&self, doc_id: DocId) -> &[ChunkId] {
        self.docs
            .get(doc_id.0 as usize)
            .map(|d| d.chunk_ids.as_slice())
            .unwrap_or(&[])
    }

    pub fn doc_by_path(&self, rel_path: &str) -> Option<DocId> {
        self.docs
            .iter()
            .find(|d| {
                d.rel_path == rel_path
                    || d.rel_path == format!("{}.stuff", rel_path)
                    || d.rel_path
                        .strip_suffix(".stuff")
                        .map(|s| s == rel_path)
                        .unwrap_or(false)
            })
            .map(|d| d.id)
    }

    pub fn resolve_ref(
        &self,
        expr: &ast::RefExpr,
        current_file: &str,
        current_idx: usize,
    ) -> Option<&Chunk> {
        resolve_ref(self, expr, current_file, current_idx)
    }

    pub fn resolve_transclusion(
        &self,
        expr: &ast::RefExpr,
        current_file: &str,
        current_idx: usize,
    ) -> Vec<&Chunk> {
        resolve_transclusion(self, expr, current_file, current_idx)
    }

    pub fn string(&self, id: StringId) -> &str {
        self.strings.get(id)
    }

    pub fn get_chunks(&self, file: &str) -> Option<&[ChunkId]> {
        self.doc_by_path(file).map(|id| self.chunks_in(id))
    }

    pub fn has_name(&self, cid: ChunkId, name: &str) -> bool {
        self.chunk(cid)
            .and_then(|c| c.name.map(|id| self.strings.get(id) == name))
            .unwrap_or(false)
    }

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

    pub fn normalize_idx(&self, idx: i32, len: i32) -> Option<i32> {
        if idx >= 0 && idx < len {
            Some(idx)
        } else {
            warn!("something has gone verry wrong");
            None
        }
    }
}

fn extract_heading_and_text(
    strings: &mut StringArena,
    block: &ast::Block,
) -> (Option<StringId>, Option<StringId>) {
    match block {
        ast::Block::Heading { .. } => ast::block_heading_and_text(block)
            .map(|text| {
                let id = strings.intern(&text);
                (Some(id), Some(id))
            })
            .unwrap_or((None, None)),
        ast::Block::Paragraph(_) | ast::Block::List { .. } => ast::block_heading_and_text(block)
            .map(|text| (None, Some(strings.intern(&text))))
            .unwrap_or((None, None)),
        _ => (None, None),
    }
}
