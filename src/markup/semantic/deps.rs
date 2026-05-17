use crate::markup::semantic::{ChunkGraph, ChunkId, DocId};
use rustc_hash::FxHashMap;
use std::collections::HashSet;

pub struct DependencyTracker;

impl DependencyTracker {
    // hair plucking simulator 4000
    pub fn transitive_deps_with_reverse(
        graph: &ChunkGraph,
        reverse_deps: &FxHashMap<DocId, Vec<ChunkId>>,
        changed_files: &HashSet<DocId>,
    ) -> HashSet<DocId> {
        let mut to_rebuild: HashSet<DocId> = changed_files.clone();
        let mut queue: Vec<DocId> = changed_files.iter().copied().collect();

        while let Some(doc_id) = queue.pop() {
            if let Some(chunk_ids) = reverse_deps.get(&doc_id) {
                for &chunk_id in chunk_ids {
                    if let Some(chunk) = graph.chunk(chunk_id) {
                        let owner = chunk.doc;
                        if to_rebuild.insert(owner) {
                            queue.push(owner);
                        }
                    }
                }
            }
        }

        to_rebuild
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markup::semantic::ChunkGraph;

    fn make_graph() -> ChunkGraph {
        // file1: chunks 0,1  file2: chunks 2,3  file3: chunk 4
        ChunkGraph::new_for_test(vec![
            ("file1.stuff".into(), vec![ChunkId(0), ChunkId(1)]),
            ("file2.stuff".into(), vec![ChunkId(2), ChunkId(3)]),
            ("file3.stuff".into(), vec![ChunkId(4)]),
        ])
    }

    #[test]
    fn no_changed_files_returns_empty() {
        let graph = make_graph();
        let reverse_deps = FxHashMap::default();
        let changed = HashSet::new();
        let result = DependencyTracker::transitive_deps_with_reverse(&graph, &reverse_deps, &changed);
        assert!(result.is_empty());
    }

    #[test]
    fn changed_file_with_no_reverse_deps_returns_only_itself() {
        let graph = make_graph();
        let reverse_deps = FxHashMap::default();
        let mut changed = HashSet::new();
        changed.insert(DocId(0));
        let result = DependencyTracker::transitive_deps_with_reverse(&graph, &reverse_deps, &changed);
        assert_eq!(result.len(), 1);
        assert!(result.contains(&DocId(0)));
    }

    #[test]
    fn transitive_dependency_one_hop() {
        let graph = make_graph();
        // file2's chunk 2 transcludes file1 -> reverse_deps[file1] = [chunk2]
        let mut reverse_deps = FxHashMap::default();
        reverse_deps.insert(DocId(0), vec![ChunkId(2)]);

        let mut changed = HashSet::new();
        changed.insert(DocId(0));
        let result = DependencyTracker::transitive_deps_with_reverse(&graph, &reverse_deps, &changed);

        assert_eq!(result.len(), 2);
        assert!(result.contains(&DocId(0)));
        assert!(result.contains(&DocId(1)));
    }

    #[test]
    fn transitive_dependency_chain() {
        let graph = make_graph();
        // file2's chunk 2 transcludes file1 -> reverse_deps[file1] = [chunk2]
        // file3's chunk 4 transcludes file2 -> reverse_deps[file2] = [chunk4]
        let mut reverse_deps = FxHashMap::default();
        reverse_deps.insert(DocId(0), vec![ChunkId(2)]);
        reverse_deps.insert(DocId(1), vec![ChunkId(4)]);

        let mut changed = HashSet::new();
        changed.insert(DocId(0));
        let result = DependencyTracker::transitive_deps_with_reverse(&graph, &reverse_deps, &changed);

        assert_eq!(result.len(), 3);
        assert!(result.contains(&DocId(0)));
        assert!(result.contains(&DocId(1)));
        assert!(result.contains(&DocId(2)));
    }

    #[test]
    fn diamond_dependency() {
        let graph = make_graph();
        // file2's chunk 2 transcludes file1
        // file3's chunk 4 also transcludes file1
        let mut reverse_deps = FxHashMap::default();
        reverse_deps.insert(DocId(0), vec![ChunkId(2), ChunkId(4)]);

        let mut changed = HashSet::new();
        changed.insert(DocId(0));
        let result = DependencyTracker::transitive_deps_with_reverse(&graph, &reverse_deps, &changed);

        assert_eq!(result.len(), 3);
        assert!(result.contains(&DocId(0)));
        assert!(result.contains(&DocId(1)));
        assert!(result.contains(&DocId(2)));
    }

    #[test]
    fn multiple_changed_docs() {
        let graph = make_graph();
        // file2's chunk 2 transcludes file1
        // file3's chunk 4 transcludes file2
        let mut reverse_deps = FxHashMap::default();
        reverse_deps.insert(DocId(0), vec![ChunkId(2)]); // file2 references file1
        reverse_deps.insert(DocId(1), vec![ChunkId(4)]); // file3 references file2

        // Both file1 and file2 changed -> file3 gets pulled via file2
        let mut changed = HashSet::new();
        changed.insert(DocId(0));
        changed.insert(DocId(1));
        let result = DependencyTracker::transitive_deps_with_reverse(&graph, &reverse_deps, &changed);

        assert_eq!(result.len(), 3);
        assert!(result.contains(&DocId(0)));
        assert!(result.contains(&DocId(1)));
        assert!(result.contains(&DocId(2)));
    }
}
