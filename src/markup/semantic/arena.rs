use rustc_hash::FxHashMap;
use std::collections::hash_map::Entry;

use super::types::StringId;

#[derive(Default)]
pub struct StringArena {
    strings: Vec<String>,
    index: FxHashMap<String, StringId>,
}

impl StringArena {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            strings: Vec::with_capacity(capacity),
            index: FxHashMap::with_capacity_and_hasher(capacity, Default::default()),
        }
    }

    pub fn intern(&mut self, s: &str) -> StringId {
        match self.index.entry(s.to_string()) {
            Entry::Occupied(e) => *e.get(),
            Entry::Vacant(e) => {
                let id = StringId(self.strings.len() as u32);
                self.strings.push(s.to_string());
                e.insert(id);
                id
            }
        }
    }

    pub fn get(&self, id: StringId) -> &str {
        &self.strings[id.0 as usize]
    }

    #[allow(unused)]
    #[must_use] // I love clippy
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    #[allow(unused)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_deduplicates_strings() {
        let mut arena = StringArena::default();
        let id1 = arena.intern("hello");
        let id2 = arena.intern("hello");
        assert_eq!(id1, id2);
        assert_eq!(arena.len(), 1);
    }

    #[test]
    fn intern_and_get_round_trip() {
        let mut arena = StringArena::default();
        let id = arena.intern("world");
        assert_eq!(arena.get(id), "world");
    }
}
