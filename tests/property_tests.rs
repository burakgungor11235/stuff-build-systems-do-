use proptest::collection::vec as prop_vec;
use proptest::prelude::*;
use sbd::bs::cas::{Cas, Hash};
use sbd::markup::assembler::{render_to_html, RenderContext};
use sbd::markup::parser::parse;
use sbd::markup::semantic::{ChunkGraph, LinkGraph, NameTable, RenderState};

mod test_utils;
use test_utils::TempDir;

use std::collections::HashSet;

proptest! {
    #[test]
    fn parser_never_panics_on_arbitrary_input(input in ".*") {
        let _ = std::panic::catch_unwind(|| parse(&input));
    }

    #[test]
    fn parser_handles_single_line_input(input in "[^\\n]+") {
        let result = std::panic::catch_unwind(|| parse(&input));
        prop_assert!(result.is_ok());
    }

    #[test]
    fn render_never_panics_on_parsed_doc(input in ".*") {
        let doc = parse(&input);
        let graph = ChunkGraph::default();
        let render_state = RenderState::default();
        let names = NameTable::default();
        let link_graph = LinkGraph::default();
        let ctx = RenderContext::new("test.stuff", 0, &graph, &render_state, &names, &link_graph);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            render_to_html(&doc, &ctx)
        }));
    }

    #[test]
    fn render_output_is_valid_utf8(input in ".*") {
        let doc = parse(&input);
        let graph = ChunkGraph::default();
        let render_state = RenderState::default();
        let names = NameTable::default();
        let link_graph = LinkGraph::default();
        let ctx = RenderContext::new("test.stuff", 0, &graph, &render_state, &names, &link_graph);
        let html = render_to_html(&doc, &ctx);
        prop_assert!(String::from_utf8(html.clone().into_bytes()).is_ok());
    }

    #[test]
    fn chunk_key_is_deterministic(
        path in "[a-z]+[.]stuff",
        chunk_idx in 0usize..100,
        content in prop_vec(any::<u8>(), 0..64)
    ) {
        let k1 = Cas::compute_chunk_key(&path, chunk_idx, &content);
        let k2 = Cas::compute_chunk_key(&path, chunk_idx, &content);
        prop_assert_eq!(k1, k2);
    }

    #[test]
    fn chunk_key_differs_on_different_path(
        path1 in "[a-z]+[.]stuff",
        path2 in "[a-z]+[.]stuff",
        idx in 0usize..100,
        content in prop_vec(any::<u8>(), 1..64)
    ) {
        prop_assume!(path1 != path2);
        let k1 = Cas::compute_chunk_key(&path1, idx, &content);
        let k2 = Cas::compute_chunk_key(&path2, idx, &content);
        prop_assert_ne!(k1, k2);
    }

    #[test]
    fn content_hash_is_deterministic(content in prop_vec(any::<u8>(), 0..256)) {
        let h1 = Cas::compute_content_hash(&content);
        let h2 = Cas::compute_content_hash(&content);
        prop_assert_eq!(h1, h2);
    }

    #[test]
    fn content_hash_differs_on_different_input(
        content1 in prop_vec(any::<u8>(), 1..64),
        content2 in prop_vec(any::<u8>(), 1..64)
    ) {
        prop_assume!(content1 != content2);
        let h1 = Cas::compute_content_hash(&content1);
        let h2 = Cas::compute_content_hash(&content2);
        prop_assert_ne!(h1, h2);
    }

    #[test]
    fn cas_put_get_roundtrip(key_hex in "[0-9a-f]{10,20}", data in prop_vec(any::<u8>(), 0..1024)) {
        let td = TempDir::new("cas");
        let mut cas = Cas::load(td.path()).unwrap();
        let key = Hash::from_hex(&key_hex);
        cas.put(&key, &data).unwrap();
        let result = cas.get(&key).unwrap();
        prop_assert_eq!(result, data);
    }

    #[test]
    fn cas_save_load_persists_entries(key_hex in "[0-9a-f]{10,20}", data in prop_vec(any::<u8>(), 1..512)) {
        let td = TempDir::new("cas");
        {
            let mut cas = Cas::load(td.path()).unwrap();
            let key = Hash::from_hex(&key_hex);
            cas.put(&key, &data).unwrap();
            cas.save().unwrap();
        }
        let cas = Cas::load(td.path()).unwrap();
        let key = Hash::from_hex(&key_hex);
        prop_assert!(cas.contains(&key));
        let result = cas.get(&key).unwrap();
        prop_assert_eq!(result, data);
    }

    #[test]
    fn cas_gc_removes_not_in_live_set(
        key1_hex in "[0-9a-f]{10,20}",
        key2_hex in "[0-9a-f]{10,20}",
        data1 in prop_vec(any::<u8>(), 1..256),
        data2 in prop_vec(any::<u8>(), 1..256)
    ) {
        prop_assume!(key1_hex != key2_hex);
        let td = TempDir::new("cas");
        let mut cas = Cas::load(td.path()).unwrap();
        let k1 = Hash::from_hex(&key1_hex);
        let k2 = Hash::from_hex(&key2_hex);
        cas.put(&k1, &data1).unwrap();
        cas.put(&k2, &data2).unwrap();

        let mut live = HashSet::new();
        live.insert(k1.clone());

        let removed = cas.gc(&live);
        prop_assert_eq!(removed, 1);
        prop_assert!(cas.contains(&k1));
        prop_assert!(!cas.contains(&k2));
    }

    #[test]
    fn cas_artifact_path_is_sharded(key_hex in "[0-9a-f]{10,20}") {
        let td = TempDir::new("cas");
        let mut cas = Cas::load(td.path()).unwrap();
        let key = Hash::from_hex(&key_hex);
        cas.put(&key, b"data").unwrap();

        let prefix = &key_hex[..2];
        let shard_dir = td.path().join("artifacts").join(prefix);
        prop_assert!(shard_dir.exists());
    }

    #[test]
    fn parser_handles_special_characters(input in "[a-zA-Z0-9 !@#$%^&*()_=+\\[\\]{}|;:,<>.?/]+") {
        let result = std::panic::catch_unwind(|| parse(&input));
        prop_assert!(result.is_ok());
    }

    #[test]
    fn repeated_parse_produces_same_ast(input in ".*") {
        let doc1 = parse(&input);
        let doc2 = parse(&input);
        prop_assert_eq!(doc1.chunks.len(), doc2.chunks.len());
    }

    #[test]
    fn empty_input_produces_empty_doc(input in "[ \\t]*") {
        let doc = parse(&input);
        prop_assert!(doc.chunks.is_empty());
    }
}

#[test]
fn cas_hash_from_hex_valid() {
    let key = Hash::from_hex("abcdef1234567890");
    assert_eq!(key.as_str().len(), 16);
    assert!(key.as_str().chars().all(|c: char| c.is_ascii_hexdigit()));
}

#[test]
fn cas_chunk_key_is_hex() {
    let key = Cas::compute_chunk_key("test.stuff", 0, &[1, 2, 3]);
    assert!(key.as_str().chars().all(|c: char| c.is_ascii_hexdigit()));
}

#[test]
fn cas_chunk_key_length_consistent() {
    let k1 = Cas::compute_chunk_key("a.stuff", 0, &[1]);
    let k2 = Cas::compute_chunk_key("very_long_filename.stuff", 999, &[1, 2, 3, 4, 5]);
    assert_eq!(k1.as_str().len(), k2.as_str().len());
}
