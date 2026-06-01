use super::types::{ChunkId, DocId};

#[derive(Clone, Debug)]
pub struct LinkEdge {
    pub source_doc: DocId,
    pub source_chunk: ChunkId,
    pub target_doc: DocId,
    pub target_chunk: Option<ChunkId>,
    pub page_id: usize,
    pub display_id: usize,
    pub context_id: usize,
}

#[derive(Clone, Debug, Default)]
pub struct LinkGraph {
    pub(crate) forward: Vec<Vec<LinkEdge>>,
    pub(crate) backward: Vec<Vec<LinkEdge>>,
}

impl LinkGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn links_from(&self, doc: DocId) -> &[LinkEdge] {
        self.forward
            .get(doc.0 as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn links_to(&self, doc: DocId) -> &[LinkEdge] {
        self.backward
            .get(doc.0 as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

/// Builder that accumulates edges and produces a LinkGraph.
/// Sorts edges by doc ID during build.
#[derive(Default)]
pub struct LinkGraphBuilder {
    edges: Vec<LinkEdge>,
}

impl LinkGraphBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_edge(&mut self, edge: LinkEdge) {
        self.edges.push(edge);
    }

    pub fn build(self, n_docs: usize) -> LinkGraph {
        let mut forward: Vec<Vec<LinkEdge>> = (0..n_docs).map(|_| Vec::new()).collect();
        let mut backward: Vec<Vec<LinkEdge>> = (0..n_docs).map(|_| Vec::new()).collect();

        for e in &self.edges {
            forward[e.source_doc.0 as usize].push(e.clone());
            backward[e.target_doc.0 as usize].push(e.clone());
        }

        LinkGraph { forward, backward }
    }
}

#[cfg(test)]
impl LinkGraph {
    pub fn add_edge(&mut self, edge: LinkEdge) {
        self.ensure_doc_capacity(edge.source_doc);
        self.ensure_doc_capacity(edge.target_doc);
        self.forward[edge.source_doc.0 as usize].push(edge.clone());
        self.backward[edge.target_doc.0 as usize].push(edge.clone());
    }

    fn ensure_doc_capacity(&mut self, doc: DocId) {
        let idx = doc.0 as usize;
        if idx >= self.forward.len() {
            self.forward.resize_with(idx + 1, Vec::new);
            self.backward.resize_with(idx + 1, Vec::new);
        }
    }

    pub fn all_edges(&self) -> Vec<&LinkEdge> {
        self.forward.iter().flat_map(|v| v.iter()).collect()
    }

    pub fn sources_for(&self, doc: DocId) -> impl Iterator<Item = DocId> + '_ {
        self.links_to(doc).iter().map(|e| e.source_doc)
    }

    pub fn targets_for(&self, doc: DocId) -> impl Iterator<Item = DocId> + '_ {
        self.links_from(doc).iter().map(|e| e.target_doc)
    }

    pub fn doc_has_incoming(&self, doc: DocId) -> bool {
        self.backward
            .get(doc.0 as usize)
            .map_or_else(|| false, |v| !v.is_empty())
    }

    pub fn doc_has_outgoing(&self, doc: DocId) -> bool {
        self.forward
            .get(doc.0 as usize)
            .map_or_else(|| false, |v| !v.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eg(d: u32) -> DocId {
        DocId(d)
    }
    fn ec(d: u32) -> ChunkId {
        ChunkId(d)
    }

    fn make_edge(src: u32, tgt: u32, pid: usize) -> LinkEdge {
        LinkEdge {
            source_doc: eg(src),
            source_chunk: ec(0),
            target_doc: eg(tgt),
            target_chunk: Some(ec(0)),
            page_id: pid,
            display_id: 0,
            context_id: 0,
        }
    }

    fn edge_count(lg: &LinkGraph) -> usize {
        lg.forward.iter().map(|v| v.len()).sum()
    }

    #[test]
    fn empty_graph() {
        let lg = LinkGraph::new();
        assert_eq!(edge_count(&lg), 0);
        assert!(!lg.doc_has_incoming(eg(0)));
        assert!(!lg.doc_has_outgoing(eg(0)));
    }

    #[test]
    fn forward_and_backward() {
        let mut lg = LinkGraph::new();
        lg.add_edge(make_edge(0, 1, 10));
        lg.add_edge(make_edge(0, 2, 11));
        lg.add_edge(make_edge(1, 0, 12));

        assert_eq!(lg.links_from(eg(0)).len(), 2);
        assert_eq!(lg.links_to(eg(0)).len(), 1);
        assert_eq!(lg.links_to(eg(1)).len(), 1);
        assert_eq!(lg.links_from(eg(2)).len(), 0);
    }

    #[test]
    fn sources_and_targets() {
        let mut lg = LinkGraph::new();
        lg.add_edge(make_edge(0, 2, 10));
        lg.add_edge(make_edge(1, 2, 11));

        let sources: Vec<DocId> = lg.sources_for(eg(2)).collect();
        assert_eq!(sources.len(), 2);
        assert!(sources.contains(&eg(0)));
        assert!(sources.contains(&eg(1)));

        let targets: Vec<DocId> = lg.targets_for(eg(0)).collect();
        assert_eq!(targets, vec![eg(2)]);
    }

    #[test]
    fn document_has_links_checks() {
        let mut lg = LinkGraph::new();
        lg.add_edge(make_edge(0, 1, 10));

        assert!(lg.doc_has_outgoing(eg(0)));
        assert!(!lg.doc_has_outgoing(eg(1)));
        assert!(lg.doc_has_incoming(eg(1)));
        assert!(!lg.doc_has_incoming(eg(0)));
    }

    #[test]
    fn all_edges_contains_all() {
        let mut lg = LinkGraph::new();
        lg.add_edge(make_edge(0, 1, 10));
        lg.add_edge(make_edge(1, 2, 11));
        assert_eq!(edge_count(&lg), 2);
    }

    #[test]
    fn self_link() {
        let mut lg = LinkGraph::new();
        lg.add_edge(make_edge(0, 0, 10));
        assert_eq!(lg.links_from(eg(0)).len(), 1);
        assert_eq!(lg.links_to(eg(0)).len(), 1);
    }

    #[test]
    fn multiple_edges_between_same_docs() {
        let mut lg = LinkGraph::new();
        lg.add_edge(make_edge(0, 1, 10));
        lg.add_edge(make_edge(0, 1, 11));
        assert_eq!(lg.links_from(eg(0)).len(), 2);
        assert_eq!(lg.links_to(eg(1)).len(), 2);
    }
}
