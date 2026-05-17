use crate::markup::ast;
use super::ChunkGraph;
use super::types::{Chunk, ChunkId};

pub fn resolve_ref<'a>(
    graph: &'a ChunkGraph,
    expr: &ast::RefExpr,
    current_file: &str,
    current_idx: usize,
) -> Option<&'a Chunk> {
    resolve_to_ids(graph, expr, current_file, current_idx)
        .unwrap_or_default()
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
        .unwrap_or_default()
        .into_iter()
        .filter_map(|cid| graph.chunk(cid))
        .collect()
}

// Cyclometric complexity is my biggest opp. 
// I don't regret the decisions that lead me to this.
// Wasted hours ticker : 2
fn resolve_to_ids(
    graph: &ChunkGraph,
    expr: &ast::RefExpr,
    current_file: &str,
    current_idx: usize,
) -> Option<Vec<ChunkId>> {
    match expr {
        ast::RefExpr::Named(name) => find_by_name(graph, current_file, name).map(|c| vec![c]),

        ast::RefExpr::Relative(offset) => {
            let ids = graph.get_chunks(current_file)?;
            let target = current_idx as i32 + offset;
            ids.get(target as usize).copied().map(|c| vec![c])
        }

        ast::RefExpr::Absolute(idx) => {
            let ids = graph.get_chunks(current_file)?;
            ids.get(*idx).copied().map(|c| vec![c])
        }

        ast::RefExpr::Range(start, end) => {
            let ids = graph.get_chunks(current_file)?;
            let len = ids.len() as i32;
            let s = graph.normalize_idx(current_idx as i32 + start, len)?;
            let e = graph.normalize_idx(current_idx as i32 + end, len)?;
            let (lo, hi) = if s <= e { (s, e) } else { (e, s) };
            Some(ids[lo as usize..=hi as usize].to_vec())
        }

        ast::RefExpr::List(exprs) => {
            let mut result = Vec::new();
            for e in exprs {
                if let Some(ids) = resolve_to_ids(graph, e, current_file, current_idx) {
                    result.extend(ids);
                }
            }
            Some(result)
        }

        ast::RefExpr::FileByIndex(file, idx) => {
            let ids = graph.get_chunks(file)?;
            ids.get(*idx).copied().map(|c| vec![c])
        }

        ast::RefExpr::FileByName(file, name) => find_by_name(graph, file, name).map(|c| vec![c]),

        ast::RefExpr::FileByHeading(file, heading) => {
            let ids = graph.get_chunks(file)?;
            ids.iter()
                .find(|&&cid| graph.heading_matches(cid, heading))
                .copied()
                .map(|c| vec![c])
        }

        ast::RefExpr::FileByHeadingIndex(file, heading, idx) => {
            let ids = graph.get_chunks(file)?;
            graph.chunks_under_heading(ids, heading)?
                .into_iter()
                .nth(*idx)
                .map(|c| vec![c])
        }

        ast::RefExpr::FileByHeadingName(file, heading, name) => {
            let ids = graph.get_chunks(file)?;
            graph.chunks_under_heading(ids, heading)?
                .into_iter()
                .find(|&cid| graph.has_name(cid, name))
                .map(|c| vec![c])
        }

        ast::RefExpr::HeadingRange(heading) => {
            let ids = graph.get_chunks(current_file)?;
            graph.chunks_under_heading(ids, heading)?
                .into_iter()
                .next()
                .map(|c| vec![c])
        }
    }
}

fn find_by_name(graph: &ChunkGraph, file: &str, name: &str) -> Option<ChunkId> {
    graph.get_chunks(file)?
        .iter()
        .find(|&&cid| graph.has_name(cid, name))
        .copied()
}
