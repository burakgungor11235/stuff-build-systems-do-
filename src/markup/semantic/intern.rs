use rustc_hash::FxHashMap;

use super::types::StringId;

/// Generic string intern pool: owns deduplicated strings, O(1) content lookup,
/// O(1) positional access by ID. 
pub struct InternPool<Id> {
    strings: Vec<Box<str>>,
    lookup: FxHashMap<String, Id>,
}

impl<Id: From<usize> + Into<usize> + Copy> InternPool<Id> {
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            strings: Vec::with_capacity(cap),
            lookup: FxHashMap::default(),
        }
    }

    pub fn intern(&mut self, s: &str) -> Id {
        if let Some(&id) = self.lookup.get(s) {
            return id;
        }
        let id = Id::from(self.strings.len());
        self.strings.push(s.into());
        self.lookup.insert(s.to_string(), id);
        id
    }

    pub fn get(&self, id: Id) -> &str {
        &self.strings[id.into()]
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

impl<Id: From<usize> + Into<usize> + Copy> Default for InternPool<Id> {
    fn default() -> Self {
        Self::with_capacity(64)
    }
}

/// Page-name intern pool (returns `usize` index used as `page_id`).
pub type NameTable = InternPool<usize>;

/// Text-content intern pool (returns `StringId`).
pub type StringArena = InternPool<StringId>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_table_intern_deduplicates() {
        let mut nt = NameTable::with_capacity(4);
        let a = nt.intern("hello");
        let b = nt.intern("hello");
        assert_eq!(a, b);
    }

    #[test]
    fn name_table_intern_different() {
        let mut nt = NameTable::with_capacity(4);
        let a = nt.intern("foo");
        let b = nt.intern("bar");
        assert_ne!(a, b);
    }

    #[test]
    fn name_table_get_returns_original() {
        let mut nt = NameTable::with_capacity(4);
        let id = nt.intern("my-cool-idea");
        assert_eq!(nt.get(id), "my-cool-idea");
    }

    #[test]
    fn name_table_intern_empty() {
        let mut nt = NameTable::with_capacity(4);
        let id = nt.intern("");
        assert_eq!(nt.get(id), "");
    }

    #[test]
    fn name_table_len_tracks_unique() {
        let mut nt = NameTable::with_capacity(4);
        assert_eq!(nt.len(), 0);
        nt.intern("a");
        nt.intern("b");
        nt.intern("a");
        assert_eq!(nt.len(), 2);
    }

    #[test]
    fn name_table_is_empty_initially() {
        let nt = NameTable::with_capacity(4);
        assert!(nt.is_empty());
    }

    #[test]
    fn string_arena_deduplicates() {
        let mut arena = StringArena::default();
        let id1 = arena.intern("hello");
        let id2 = arena.intern("hello");
        assert_eq!(id1, id2);
        assert_eq!(arena.len(), 1);
    }

    #[test]
    fn string_arena_round_trip() {
        let mut arena = StringArena::default();
        let id = arena.intern("world");
        assert_eq!(arena.get(id), "world");
    }
}
