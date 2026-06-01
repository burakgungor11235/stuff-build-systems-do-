#![allow(dead_code)]

use crate::markup::semantic::ChunkGraph;

pub struct Diagnostic {
    pub message: String,
    pub severity: DiagnosticSeverity,
}

pub enum DiagnosticSeverity {
    Warning,
    Error,
}

pub struct Validator;

impl Validator {
    pub fn validate_all_refs_resolve(_graph: &ChunkGraph) -> Vec<Diagnostic> {
        Vec::new()
    }

    pub fn validate_no_cyclic_transclusions(_graph: &ChunkGraph) -> Vec<Diagnostic> {
        Vec::new()
    }

    pub fn validate_images_exist(_graph: &ChunkGraph) -> Vec<Diagnostic> {
        Vec::new()
    }
}
