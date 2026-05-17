use std::path::{Path, PathBuf};

use crate::markup::semantic::ChunkGraph;

pub struct ResolvedImage {
    pub source_path: PathBuf,
    pub output_path: PathBuf,
    pub alt_text: String,
}

pub struct ImageResolver;

impl ImageResolver {
    pub fn resolve_image(
        _path: &str,
        _current_doc_path: &Path,
        _graph: &ChunkGraph,
    ) -> Option<ResolvedImage> {
        None
    }
}
