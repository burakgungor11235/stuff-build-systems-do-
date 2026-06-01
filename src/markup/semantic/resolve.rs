use super::types::{Chunk, ChunkId};
use super::ChunkGraph;
use crate::markup::ast;

pub fn resolve_ref<'a>(
    graph: &'a ChunkGraph,
    expr: &ast::RefExpr,
    current_file: &str,
    current_idx: usize,
) -> Option<&'a Chunk> {
    resolve_to_ids(graph, expr, current_file, current_idx)
        .first()
        .and_then(|&cid| graph.chunk(cid))
}

pub fn resolve_transclusion<'a>(
    graph: &'a ChunkGraph,
    expr: &ast::RefExpr,
    current_file: &str,
    current_idx: usize,
) -> Vec<&'a Chunk> {
    resolve_to_ids(graph, expr, current_file, current_idx)
        .into_iter()
        .filter_map(|cid| graph.chunk(cid))
        .collect()
}

pub fn resolve_to_ids(
    graph: &ChunkGraph,
    expr: &ast::RefExpr,
    current_file: &str,
    current_idx: usize,
) -> Vec<ChunkId> {
    let clamp = |idx: i32, len: i32| -> Option<usize> {
        if idx < 0 || idx >= len {
            None
        } else {
            Some(idx as usize)
        }
    };

    match expr {
        ast::RefExpr::Named(name) => {
            let ids = graph.get_chunks(current_file).unwrap_or(&[]);
            ids.iter()
                .find(|&&cid| graph.has_name(cid, name))
                .copied()
                .map(|c| vec![c])
                .unwrap_or_default()
        }

        ast::RefExpr::Relative(offset) => {
            let ids = graph.get_chunks(current_file).unwrap_or(&[]);
            let target = current_idx as i32 + offset;
            ids.get(target as usize)
                .copied()
                .map(|c| vec![c])
                .unwrap_or_default()
        }

        ast::RefExpr::Absolute(idx) => {
            let ids = graph.get_chunks(current_file).unwrap_or(&[]);
            ids.get(*idx).copied().map(|c| vec![c]).unwrap_or_default()
        }

        ast::RefExpr::Range(start, end) => {
            let ids = graph.get_chunks(current_file).unwrap_or(&[]);
            let len = ids.len() as i32;
            let s = clamp(current_idx as i32 + start, len);
            let e = clamp(current_idx as i32 + end, len);
            match (s, e) {
                (Some(s), Some(e)) => {
                    let (lo, hi) = if s <= e { (s, e) } else { (e, s) };
                    ids[lo..=hi].to_vec()
                }
                _ => vec![],
            }
        }

        ast::RefExpr::List(exprs) => {
            let mut result = Vec::new();
            for e in exprs {
                result.extend(resolve_to_ids(graph, e, current_file, current_idx));
            }
            result
        }

        ast::RefExpr::FileByIndex(file, idx) => graph
            .get_chunks(file)
            .unwrap_or(&[])
            .get(*idx)
            .copied()
            .map(|c| vec![c])
            .unwrap_or_default(),

        ast::RefExpr::FileByName(file, name) => graph
            .get_chunks(file)
            .unwrap_or(&[])
            .iter()
            .find(|&&cid| graph.has_name(cid, name))
            .copied()
            .map(|c| vec![c])
            .unwrap_or_default(),

        ast::RefExpr::FileByHeading(file, heading) => graph
            .get_chunks(file)
            .unwrap_or(&[])
            .iter()
            .find(|&&cid| graph.heading_matches(cid, heading))
            .copied()
            .map(|c| vec![c])
            .unwrap_or_default(),

        ast::RefExpr::FileByHeadingIndex(file, heading, idx) => {
            let ids = graph.get_chunks(file).unwrap_or(&[]);
            match graph.chunks_under_heading(ids, heading) {
                Some(under) => under
                    .into_iter()
                    .nth(*idx)
                    .map(|c| vec![c])
                    .unwrap_or_default(),
                None => vec![],
            }
        }

        ast::RefExpr::FileByHeadingName(file, heading, name) => {
            let ids = graph.get_chunks(file).unwrap_or(&[]);
            match graph.chunks_under_heading(ids, heading) {
                Some(under) => under
                    .into_iter()
                    .find(|&cid| graph.has_name(cid, name))
                    .map(|c| vec![c])
                    .unwrap_or_default(),
                None => vec![],
            }
        }

        ast::RefExpr::HeadingRange(heading) => {
            let ids = graph.get_chunks(current_file).unwrap_or(&[]);
            match graph.chunks_under_heading(ids, heading) {
                Some(under) => under
                    .into_iter()
                    .next()
                    .map(|c| vec![c])
                    .unwrap_or_default(),
                None => vec![],
            }
        }
    }
}
