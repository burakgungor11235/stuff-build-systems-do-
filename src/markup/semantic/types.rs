#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ChunkId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct DocId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct StringId(pub u32);

#[derive(Clone)]
pub struct Chunk {
    pub id: ChunkId,
    pub doc: DocId,
    pub index: usize,
    pub name: Option<StringId>,
    pub heading: Option<StringId>,
    pub first_inline_text: Option<StringId>,
}

impl Chunk {
    pub fn anchor_id(&self) -> String {
        format!("chunk-{}", self.index)
    }
}

#[derive(Clone)]
pub struct Document {
    pub id: DocId,
    pub rel_path: String,
    pub chunk_ids: Vec<ChunkId>,
}
