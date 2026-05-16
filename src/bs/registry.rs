use crate::markup::ast::*;

#[derive(Clone, Debug)]
pub struct ChunkInfo {
    /// 0-based index within the file
    pub index: usize,
    /// Optional name from explicit chunk (:>(name))
    pub name: Option<String>,
    /// Heading text if this chunk is preceded by a heading
    pub heading: Option<String>,
    /// First inline text for generating link titles
    pub first_inline_text: Option<String>,
    /// The chunk's rendered HTML (set during pass 2)
    pub html: String,
    /// The anchor ID for linking
    pub anchor_id: String,
}

/// A registry of all chunks across all files. Enables cross-referencing
/// between files and resolving `&` reference expressions.
#[derive(Clone, Default)]
pub struct ChunkRegistry {
    /// file relative path -> ordered list of chunks
    pub files: std::collections::HashMap<String, Vec<ChunkInfo>>,
}

impl ChunkRegistry {
    /// Extract chunk metadata from a parsed document and register it.
    pub fn collect_from(&mut self, doc: &Document, rel_path: String) {
        let mut infos = Vec::new();

        for (chunk_idx, chunk) in doc.chunks.iter().enumerate() {
            match chunk {
                Chunk::Implicit { name, block } => {
                    let info = self.chunk_info_from_block(block, chunk_idx, name.clone());
                    infos.push(info);
                }
                Chunk::Explicit { name, blocks } => {
                    for (block_idx, block) in blocks.iter().enumerate() {
                        let chunk_name: Option<String> = if block_idx == 0 {
                            Some(name.clone())
                        } else {
                            None
                        };
                        let info = self.chunk_info_from_block(block, chunk_idx, chunk_name);
                        infos.push(info);
                    }
                }
            }
        }
        self.files.insert(rel_path, infos);
    }

    fn chunk_info_from_block(
        &self,
        block: &Block,
        index: usize,
        name: Option<String>,
    ) -> ChunkInfo {
        let (heading, first_inline_text) = match block {
            Block::Heading { level: _, content } => {
                let text = inlines_to_plain_text(content);
                (Some(text.clone()), Some(text))
            }
            Block::Paragraph(inlines) => {
                let text = inlines_to_plain_text(inlines);
                (None, if text.is_empty() { None } else { Some(text) })
            }
            Block::List { items, .. } => {
                let text = if let Some(first) = items.first() {
                    inlines_to_plain_text(first)
                } else {
                    String::new()
                };
                (None, if text.is_empty() { None } else { Some(text) })
            }
            _ => (None, None),
        };

        ChunkInfo {
            index,
            name,
            heading,
            first_inline_text,
            html: String::new(),
            anchor_id: format!("chunk-{}", index),
        }
    }

    // Resolve a single RefExpr to a chunk's info.
    pub fn resolve(
        &self,
        expr: &RefExpr,
        current_file: &str,
        current_idx: usize,
    ) -> Option<&ChunkInfo> {
        match expr {
            RefExpr::Named(name) => self
                .files
                .get(current_file)
                .and_then(|chunks| chunks.iter().find(|c| c.name.as_deref() == Some(name))),

            RefExpr::Relative(offset) => {
                let chunks = self.files.get(current_file)?;
                let target = current_idx as i32 + offset;
                if target >= 0 && (target as usize) < chunks.len() {
                    Some(&chunks[target as usize])
                } else {
                    None
                }
            }

            RefExpr::Absolute(idx) => self
                .files
                .get(current_file)
                .and_then(|chunks| chunks.get(*idx)),

            RefExpr::Range(start, end) => {
                let chunks = self.files.get(current_file)?;
                let len = chunks.len() as i32;
                let s = self.normalize_idx(current_idx as i32 + start, len)?;
                let e = self.normalize_idx(current_idx as i32 + end, len)?;
                let lo = s.min(e);
                chunks.get(lo as usize)
            }

            RefExpr::List(exprs) => {
                for e in exprs {
                    if let Some(info) = self.resolve(e, current_file, current_idx) {
                        return Some(info);
                    }
                }
                None
            }

            RefExpr::FileByIndex(file, idx) => self
                .files
                .get(file)
                .and_then(|chunks| chunks.get(*idx)),

            RefExpr::FileByName(file, name) => self
                .files
                .get(file)
                .and_then(|chunks| chunks.iter().find(|c| c.name.as_deref() == Some(name))),

            RefExpr::FileByHeading(file, heading) => {
                let chunks = self.files.get(file)?;
                self.find_by_heading(chunks, heading)
            }

            RefExpr::FileByHeadingIndex(file, heading, idx) => {
                let chunks = self.files.get(file)?;
                let under = self.chunks_under_heading(chunks, heading);
                under.into_iter().nth(*idx)
            }

            RefExpr::FileByHeadingName(file, heading, name) => {
                let chunks = self.files.get(file)?;
                let under = self.chunks_under_heading(chunks, heading);
                under
                    .into_iter()
                    .find(|c| c.name.as_deref() == Some(name))
            }

            RefExpr::HeadingRange(heading) => {
                let chunks = self.files.get(current_file)?;
                let under = self.chunks_under_heading(chunks, heading);
                under.into_iter().next()
            }
        }
    }

    pub fn resolve_transclusion(
        &self,
        expr: &RefExpr,
        current_file: &str,
        current_idx: usize,
    ) -> Vec<&ChunkInfo> {
        match expr {
            RefExpr::HeadingRange(heading) => {
                let chunks = match self.files.get(current_file) {
                    Some(c) => c,
                    None => return Vec::new(),
                };
                self.chunks_under_heading(chunks, heading)
                    .into_iter()
                    .collect()
            }

            RefExpr::Range(start, end) => {
                let chunks = match self.files.get(current_file) {
                    Some(c) => c,
                    None => return Vec::new(),
                };
                let len = chunks.len() as i32;
                let s = match self.normalize_idx(current_idx as i32 + start, len) {
                    Some(s) => s,
                    None => return Vec::new(),
                };
                let e = match self.normalize_idx(current_idx as i32 + end, len) {
                    Some(e) => e,
                    None => return Vec::new(),
                };
                let (lo, hi) = if s <= e { (s, e) } else { (e, s) };
                chunks[lo as usize..=hi as usize].iter().collect()
            }

            RefExpr::List(exprs) => {
                let mut result = Vec::new();
                for e in exprs {
                    if let Some(info) = self.resolve(e, current_file, current_idx) {
                        result.push(info);
                    }
                }
                result
            }

            _ => {
                if let Some(info) = self.resolve(expr, current_file, current_idx) {
                    vec![info]
                } else {
                    Vec::new()
                }
            }
        }
    }

    /// Return all chunks under a given heading (exclusive of the heading itself).
    fn chunks_under_heading<'a>(
        &'a self,
        chunks: &'a [ChunkInfo],
        heading: &str,
    ) -> Vec<&'a ChunkInfo> {
        let start_idx = chunks.iter().position(|c| {
            c.heading
                .as_deref()
                .map(|h| h.to_lowercase() == heading.to_lowercase())
                .unwrap_or(false)
        });

        if let Some(start) = start_idx {
            let mut end = chunks.len();
            for (i, c) in chunks.iter().enumerate().skip(start + 1) {
                if c.heading.is_some() {
                    end = i;
                    break;
                }
            }
            chunks[start + 1..end].iter().collect()
        } else {
            Vec::new()
        }
    }

    fn find_by_heading<'a>(
        &'a self,
        chunks: &'a [ChunkInfo],
        heading: &str,
    ) -> Option<&'a ChunkInfo> {
        chunks.iter().find(|c| {
            c.heading
                .as_deref()
                .map(|h| h.to_lowercase() == heading.to_lowercase())
                .unwrap_or(false)
        })
    }

    fn normalize_idx(&self, idx: i32, len: i32) -> Option<i32> {
        if idx >= 0 && idx < len {
            Some(idx)
        } else {
            None
        }
    }
}

fn inlines_to_plain_text(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => s.push_str(t),
            Inline::Bold(inner)
            | Inline::Italic(inner)
            | Inline::Strikethrough(inner) => {
                s.push_str(&inlines_to_plain_text(inner));
            }
            Inline::Reference(_) => {}
            Inline::Link { .. } => {}
            Inline::Transclusion(_) => {}
        }
    }
    s
}
