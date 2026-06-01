use super::types::DocId;
use super::{ChunkGraph, LinkGraph};

#[allow(dead_code)]
pub fn dump_link_graph(
    link_graph: &LinkGraph,
    all_doc_ids: &[DocId],
    graph: &ChunkGraph,
) -> String {
    let mut out = String::new();
    for &did in all_doc_ids {
        let name = graph
            .doc(did)
            .map(|d| d.rel_path.as_str())
            .unwrap_or("?");
        let outgoing = link_graph.links_from(did);
        let incoming = link_graph.links_to(did);
        out.push_str(&format!(
            "{}: {} out, {} in\n",
            name,
            outgoing.len(),
            incoming.len()
        ));
        for e in outgoing {
            let tgt = graph
                .doc(e.target_doc)
                .map(|d| d.rel_path.as_str())
                .unwrap_or("?");
            out.push_str(&format!("  -> {}\n", tgt));
        }
        for e in incoming {
            let src = graph
                .doc(e.source_doc)
                .map(|d| d.rel_path.as_str())
                .unwrap_or("?");
            out.push_str(&format!("  <- {}\n", src));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markup::semantic::link_graph::LinkEdge;
    use crate::markup::semantic::{ChunkId, LinkGraph, NameTable};

    fn make_edge(src: u32, tgt: u32) -> LinkEdge {
        LinkEdge {
            source_doc: DocId(src),
            source_chunk: ChunkId(0),
            target_doc: DocId(tgt),
            target_chunk: Some(ChunkId(0)),
            page_id: 0,
            display_id: 0,
            context_id: 0,
        }
    }

    fn make_graph(doc_count: u32) -> (ChunkGraph, Vec<DocId>) {
        let mut graph = ChunkGraph::default();
        let mut names = NameTable::with_capacity(64);
        let mut ids = Vec::new();
        for i in 0..doc_count {
            let doc = crate::markup::ast::Document {
                chunks: vec![crate::markup::ast::Chunk::Implicit {
                    name: None,
                    block: crate::markup::ast::Block::Paragraph(vec![
                        crate::markup::ast::Inline::Text(format!("Page {}", i)),
                    ]),
                }],
            };
            let rel_path = format!("page{}.stuff", i);
            let id = graph.add_document(&doc, rel_path, &mut names);
            ids.push(id);
        }
        (graph, ids)
    }

    #[test]
    fn empty_graph() {
        let lg = LinkGraph::new();
        let (graph, ids) = make_graph(0);
        let result = dump_link_graph(&lg, &ids, &graph);
        assert_eq!(result, "");
    }

    #[test]
    fn shows_edges() {
        let mut lg = LinkGraph::new();
        lg.add_edge(make_edge(0, 1));
        lg.add_edge(make_edge(1, 2));
        let (graph, ids) = make_graph(3);
        let result = dump_link_graph(&lg, &ids, &graph);
        assert!(result.contains("page0: 1 out, 0 in"));
        assert!(result.contains("page1: 1 out, 1 in"));
        assert!(result.contains("page2: 0 out, 1 in"));
        assert!(result.contains("-> page1"));
        assert!(result.contains("-> page2"));
        assert!(result.contains("<- page0"));
    }

    #[test]
    fn orphan_shows_no_edges() {
        let lg = LinkGraph::new();
        let (graph, ids) = make_graph(2);
        let result = dump_link_graph(&lg, &ids, &graph);
        assert!(result.contains("page0: 0 out, 0 in"));
        assert!(result.contains("page1: 0 out, 0 in"));
    }
}
