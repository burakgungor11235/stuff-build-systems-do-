use sbd::bs::builder::Builder;
use sbd::bs::config::Manifest;
use sbd::markup::assembler::{render_to_html, RenderContext};
use sbd::markup::ast::{Block, Chunk};
use sbd::markup::parser::parse;
use sbd::markup::semantic::{ChunkGraph, LinkGraph, NameTable, RenderState};

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

// I mean like rust. You can interpret the most complex of types when they are
// in a function. but this. nah you need it to be explicitly stated out and
// you will throw an error saying EXACTLY what this is.
//
// Lies man lies.
static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

static TEST_ROOT: std::sync::LazyLock<PathBuf> = std::sync::LazyLock::new(|| {
    let dir = std::env::temp_dir().join("sbd_integration_tests");
    // Remove leftovers from a previous aborted run.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
});

fn next_id() -> u64 {
    COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

struct TestProject {
    root: PathBuf,
}

impl TestProject {
    fn new() -> Self {
        let id = next_id();
        let root = std::env::temp_dir().join(format!(
            "sbd_integration_test_{}_{}",
            std::process::id(),
            id
        ));

        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("out")).unwrap();
        fs::create_dir_all(root.join("cache")).unwrap();

        Self { root }
    }

    fn write_src(&self, name: &str, content: &str) {
        let path = self.root.join("src").join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn read_out(&self, name: &str) -> String {
        let path = self.root.join("out").join(name);
        fs::read_to_string(path).unwrap()
    }

    fn out_exists(&self, name: &str) -> bool {
        self.root.join("out").join(name).exists()
    }

    fn manifest(&self) -> Manifest {
        let manifest_path = self.root.join("stuff.toml");
        let toml_content = r#"[project]
name = "test_project"
src_dir = "src"
out_dir = "out"
cache_dir = "cache"
"#
        .to_string();
        fs::write(&manifest_path, toml_content).unwrap();
        Manifest::load(&manifest_path).unwrap()
    }

    fn build(&self) -> Builder {
        let manifest = self.manifest();
        Builder::new(manifest).unwrap()
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct TestProjectBuilder {
    project: TestProject,
    files: Vec<(String, String)>,
}

fn test_project() -> TestProjectBuilder {
    TestProjectBuilder {
        project: TestProject::new(),
        files: Vec::new(),
    }
}

impl TestProjectBuilder {
    fn file(mut self, name: &str, content: &str) -> Self {
        self.files.push((name.to_string(), content.to_string()));
        self
    }

    fn build(self) -> TestProject {
        for (name, content) in &self.files {
            self.project.write_src(name, content);
        }
        self.project
    }

    // ok, this allows us to do some weird shit like being able to define a graph and then generate
    // it and basically test it.
    //
    // pretty important stuff
    //
    // Thank you
    //      Stuffman
    //              + Stuffman?
    // Becase, um...
    //     you were doing stuff.
    //              + Oh yeah..
    //
    fn add_graph_files<F>(mut self, n: usize, edges: F) -> Self
    where
        F: Fn(usize) -> Vec<usize>,
    {
        for i in 0..n {
            let mut content = format!("#1 f{}\n\n", i);
            for j in edges(i) {
                content.push_str(&format!("!&f{}.1\n", j));
            }
            if edges(i).is_empty() {
                content.push_str("Leaf content.\n");
            }
            self = self.file(&format!("f{}.stuff", i), &content);
        }
        self
    }
    // N x N
    fn add_complete_graph(self, n: usize) -> Self {
        self.add_graph_files(n, |_| (0..n).collect())
    }

    // a -> ... (n - 2) -> b
    fn add_chain(self, n: usize) -> Self {
        self.add_graph_files(n, |i| if i == 0 { vec![] } else { vec![i - 1] })
    }
    // ??
    fn add_star(self, n: usize) -> Self {
        self.add_graph_files(n, |i| if i == 0 { vec![] } else { vec![0] })
    }
    // O where r * \pi = n
    fn add_ring(self, n: usize) -> Self {
        self.add_graph_files(n, |i| vec![(i + 1) % n])
    }
    // \give @p 264
    fn add_grid_diamond(self, side: usize) -> Self {
        let n = side * side;
        self.add_graph_files(n, |idx| {
            let i = idx / side;
            let j = idx % side;
            let mut deps = Vec::with_capacity(2);
            if i > 0 {
                deps.push((i - 1) * side + j);
            }
            if j > 0 {
                deps.push(i * side + (j - 1));
            }
            deps
        })
    }

    // wiki link helpers
    // generate [[page]] links instead of transclusions
    fn add_wiki_files<F>(mut self, n: usize, links: F) -> Self
    where
        F: Fn(usize) -> Vec<usize>,
    {
        for i in 0..n {
            let targets = links(i);
            let mut content = format!("#1 f{i}\n\n");
            if targets.is_empty() {
                content.push_str("Leaf content.\n");
            } else {
                for (j, t) in targets.iter().enumerate() {
                    if j > 0 {
                        content.push_str(" and ");
                    }
                    content.push_str(&format!("[[f{t}]]"));
                }
                content.push('\n');
            }
            self = self.file(&format!("f{}.stuff", i), &content);
        }
        self
    }

    // f0 isolated; f1 -> [[f0]] -> ... -> f{n-1}
    fn add_wiki_chain(self, n: usize) -> Self {
        self.add_wiki_files(n, |i| if i == 0 { vec![] } else { vec![i - 1] })
    }

    // f0 isolated (center); all others link to [[f0]]
    fn add_wiki_star(self, n: usize) -> Self {
        self.add_wiki_files(n, |i| if i == 0 { vec![] } else { vec![0] })
    }

    // every page links to every other page (N-1 outgoing per node)
    fn add_wiki_all_pairs(self, n: usize) -> Self {
        self.add_wiki_files(n, |i| (0..n).filter(|&j| j != i).collect())
    }

    // N isolated files, no links at all
    fn add_wiki_orphans(self, n: usize) -> Self {
        self.add_wiki_files(n, |_| vec![])
    }

    // N files all linking to a non-existent page
    fn add_wiki_broken(self, n: usize) -> Self {
        self.add_wiki_files(n, |_| vec![999999])
    }
}

fn run_with_timeout<F>(timeout: Duration, f: F)
where
    F: FnOnce() + Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        f();
        // Only reached if f() completed without panicking.
        let _ = tx.send(());
    });

    match rx.recv_timeout(timeout) {
        Ok(()) => {
            // Thread signaled completion. Join to propagate any panic that
            // might have happened after the send
            if let Err(e) = handle.join() {
                std::panic::resume_unwind(e);
            }
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            eprintln!("Test timed out, skill issue ");
            std::process::abort();
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            // The spawned thread panicked and dropped the sender.
            match handle.join() {
                Err(e) => std::panic::resume_unwind(e),
                Ok(()) => unreachable!("Thread finished without sending but channel disconnected?"),
            }
        }
    }
}
struct BuiltProject {
    project: TestProject,
    rebuilt_count: usize,
}

impl BuiltProject {
    #[must_use]
    fn read_out(&self, name: &str) -> String {
        self.project.read_out(name)
    }

    #[must_use]
    fn out_exists(&self, name: &str) -> bool {
        self.project.out_exists(name)
    }

    #[must_use]
    fn rebuilt_count(&self) -> usize {
        self.rebuilt_count
    }
}

impl TestProject {
    fn build_and_run(self) -> BuiltProject {
        let mut builder = self.build();
        builder.build().unwrap();
        BuiltProject {
            rebuilt_count: builder.rebuilt_count(),
            project: self,
        }
    }
}

fn assert_render(source: &str, expected: &str) {
    let built = test_project()
        .file("test.stuff", source)
        .build()
        .build_and_run();
    let html = built.read_out("test.html");
    assert!(
        html.contains(expected),
        "Expected '{}' in HTML:\n{}",
        expected,
        html
    );
}

fn assert_rebuilt(built: &BuiltProject, expected: usize) {
    assert_eq!(
        built.rebuilt_count(),
        expected,
        "Expected {} rebuilt files",
        expected
    );
}

fn assert_output_contains(built: &BuiltProject, name: &str, expected: &str) {
    let html = built.read_out(name);
    assert!(
        html.contains(expected),
        "Expected '{}' in {}:\n{}",
        expected,
        name,
        html
    );
}

fn assert_output_not_contains(built: &BuiltProject, name: &str, unexpected: &str) {
    let html = built.read_out(name);
    assert!(
        !html.contains(unexpected),
        "Unexpected '{}' in {}:\n{}",
        unexpected,
        name,
        html
    );
}

#[test]
fn full_build_renders_all_files() {
    let built = test_project()
        .file("index.stuff", "#1 Hello\n\nWorld\n")
        .file("about.stuff", "#1 About\n\nThis is about.\n")
        .build()
        .build_and_run();

    assert!(built.out_exists("index.html"));
    assert!(built.out_exists("about.html"));
    assert_rebuilt(&built, 2);

    let index_html = built.read_out("index.html");
    assert!(index_html.contains("<h1> Hello</h1>"));
    assert!(index_html.contains("<p>World</p>"));
}

#[test]
fn cached_build_rebuilds_zero() {
    let built = test_project()
        .file("file.stuff", "#1 Title\n\nContent here.\n")
        .build()
        .build_and_run();
    assert_rebuilt(&built, 1);

    let built2 = built.project.build_and_run();
    assert_rebuilt(&built2, 0);
}

#[test]
fn content_change_triggers_rebuild() {
    let built = test_project()
        .file("file.stuff", "#1 Original\n\nOriginal content.\n")
        .build()
        .build_and_run();
    assert_rebuilt(&built, 1);

    built
        .project
        .write_src("file.stuff", "#1 Updated\n\nUpdated content.\n");
    let built2 = built.project.build_and_run();
    assert_rebuilt(&built2, 1);

    let html = built2.read_out("file.html");
    assert!(html.contains("<h1> Updated</h1>"));
}

#[test]
fn unchanged_file_not_rebuilt() {
    let built = test_project()
        .file("a.stuff", "#1 A\n\nContent A.\n")
        .file("b.stuff", "#1 B\n\nContent B.\n")
        .build()
        .build_and_run();
    assert_rebuilt(&built, 2);

    built
        .project
        .write_src("a.stuff", "#1 A Modified\n\nModified A.\n");
    let built2 = built.project.build_and_run();
    assert_rebuilt(&built2, 1);
}

#[test]
fn transclusion_triggers_transitive_rebuild() {
    let built = test_project()
        .file("base.stuff", "#1 Base\n\nBase content.\n")
        .file("derived.stuff", "#1 Derived\n\n!&base.1\n")
        .build()
        .build_and_run();
    assert_rebuilt(&built, 2);
    assert_output_contains(&built, "derived.html", "Base content");

    built
        .project
        .write_src("base.stuff", "#1 Base\n\nModified base content.\n");
    let built2 = built.project.build_and_run();
    assert_rebuilt(&built2, 2);
    assert_output_contains(&built2, "derived.html", "Modified base content");
}

#[test]
fn nested_transclusion_propagates() {
    let built = test_project()
        .file("a.stuff", "#1 A\n\nContent A.\n")
        .file("b.stuff", "#1 B\n\n!&a.1\n")
        .build()
        .build_and_run();

    assert_rebuilt(&built, 2);
    assert_output_contains(&built, "b.html", "Content A");

    built.project.write_src("a.stuff", "#1 A\n\nModified A.\n");
    let built2 = built.project.build_and_run();
    assert_rebuilt(&built2, 2);
    assert_output_contains(&built2, "b.html", "Modified A");
}

#[test]
fn multiple_chunks_partial_cache_hit() {
    let built = test_project()
        .file(
            "multi.stuff",
            "#1 First\n\nFirst content.\n\n#2 Second\n\nSecond content.\n",
        )
        .build()
        .build_and_run();
    assert_rebuilt(&built, 1);

    built.project.write_src(
        "multi.stuff",
        "#1 First\n\nFirst content CHANGED.\n\n#2 Second\n\nSecond content.\n",
    );
    let built2 = built.project.build_and_run();
    assert_rebuilt(&built2, 1);

    let html = built2.read_out("multi.html");
    assert!(html.contains("First content CHANGED"));
    assert!(html.contains("Second content"));
}

#[test]
fn empty_source_directory_builds_successfully() {
    let built = test_project().build().build_and_run();
    assert_rebuilt(&built, 0);
}

#[test]
fn subdirectory_source_files() {
    let built = test_project()
        .file("sub/file.stuff", "#1 Sub\n\nSub content.\n")
        .build()
        .build_and_run();
    assert_rebuilt(&built, 1);
    assert!(built.out_exists("sub/file.html"));
}

#[test]
fn directive_renders_as_comment() {
    assert_render("#1 Title\n\n@toc\n", "<!-- @toc -->");
}

#[test]
fn reference_renders_as_link() {
    assert_render(
        "#1 Title\n\nSee &+1 for more.\n\n#2 Details\n\nDetails here.\n",
        "<a href=",
    );
}

#[test]
fn list_renders_correctly() {
    let built = test_project()
        .file("list.stuff", "- item one\n- item two\n- item three\n")
        .build()
        .build_and_run();
    let html = built.read_out("list.html");
    assert!(html.contains("<ul>"));
    assert!(html.contains("<li>item one</li>"));
    assert!(html.contains("<li>item two</li>"));
    assert!(html.contains("<li>item three</li>"));
}

#[test]
fn ordered_list_renders_correctly() {
    let built = test_project()
        .file("olist.stuff", "1. first\n2. second\n3. third\n")
        .build()
        .build_and_run();
    let html = built.read_out("olist.html");
    assert!(html.contains("<ol>"));
    assert!(html.contains("<li>first</li>"));
}

#[test]
fn blockquote_renders() {
    assert_render(">+ This is a quote\n", "<blockquote>");
}

#[test]
fn horizontal_rule_renders() {
    assert_render("---\n", "<hr>");
}

#[test]
fn image_renders() {
    let built = test_project()
        .file("img.stuff", "![alt text|image.png]\n")
        .build()
        .build_and_run();
    let html = built.read_out("img.html");
    assert!(html.contains("src=\"image.png\""));
    assert!(html.contains("alt=\"alt text\""));
}

#[test]
fn bold_italic_strikethrough() {
    let built = test_project()
        .file("fmt.stuff", "*bold* _italic_ ~strike~\n")
        .build()
        .build_and_run();
    let html = built.read_out("fmt.html");
    assert!(html.contains("<strong>bold</strong>"));
    assert!(html.contains("<em>italic</em>"));
    assert!(html.contains("<del>strike</del>"));
}

#[test]
fn html_escaping() {
    assert_render(
        "normal text no special chars\n",
        "<p>normal text no special chars</p>",
    );
}

#[test]
fn explicit_chunk_renders() {
    assert_render(":<\n\nContent inside.\n\n>:(box)\n", "Content inside");
}

#[test]
fn parser_produces_valid_ast_for_all_block_types() {
    let inputs = vec![
        "#1 Heading\n",
        "Plain paragraph.\n",
        "*bold* and _italic_\n",
        "- item\n- item2\n",
        "+ ordered\n+ items\n",
        ">+ quoted\n",
        "---\n",
        "![alt](url)\n",
        "@directive(body)\n",
        "@simple\n",
    ];

    for input in inputs {
        let doc = parse(input);
        assert!(!doc.chunks.is_empty(), "Failed to parse: {}", input);

        let graph = ChunkGraph::default();
        let render_state = RenderState::default();
        let names = NameTable::default();
        let link_graph = LinkGraph::default();
        let ctx = RenderContext::new("test.stuff", 0, &graph, &render_state, &names, &link_graph);
        let html = render_to_html(&doc, &ctx);
        assert!(!html.is_empty() || doc.chunks.iter().all(|c| matches!(c, Chunk::Implicit { block, .. } if matches!(block, Block::HorizontalRule))),
            "Failed to render: {}", input);
    }
}

#[test]
fn cas_index_persists_across_builds() {
    let built = test_project()
        .file("persist.stuff", "#1 Persist\n\nPersist content.\n")
        .build()
        .build_and_run();

    let index_path = built.project.root.join("cache").join("cas_index.json");
    assert!(index_path.exists());
    let index_content = fs::read_to_string(&index_path).unwrap();
    assert!(index_content.contains("rendered_chunk"));
}

#[test]
fn cas_artifacts_stored_in_sharded_dirs() {
    let built = test_project()
        .file("shard.stuff", "#1 Shard\n\nShard content.\n")
        .build()
        .build_and_run();

    let artifacts_dir = built.project.root.join("cache").join("artifacts");
    assert!(artifacts_dir.exists());

    let mut found_shard = false;

    // Did I tell you that I really appreciate the style of fors where it looks like a function
    // signature but is actually just traversing a random data structure and checking if it's valid
    // at the same time. Honestly good architecture by the rust team.

    for entry in walkdir::WalkDir::new(&artifacts_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.path().extension().is_some_and(|ext| ext == "bin") {
            found_shard = true;
            let parent = entry.path().parent().unwrap();
            assert_eq!(
                parent.file_name().unwrap().len(),
                2,
                "Shard prefix should be 2 chars"
            );
        }
    }
    assert!(
        found_shard,
        "No artifact files found in sharded directories :C"
    );
}

#[test]
fn build_idempotent() {
    // sanity check
    let built1 = test_project()
        .file("good.stuff", "#1 Idemp\n\nIdempotent content.\n")
        .build()
        .build_and_run();
    let html1 = built1.read_out("good.html");

    let built2 = built1.project.build_and_run();
    let html2 = built2.read_out("good.html");

    assert_eq!(html1, html2);
}

#[test]
fn fully_connected_dependency_graph_smol() {
    run_with_timeout(Duration::from_secs(2), || {
        let built = test_project().add_complete_graph(4).build().build_and_run();

        assert_rebuilt(&built, 4);
        let f0_html = built.read_out("f0.html");
        assert!(f0_html.contains("<h1> f0</h1>"));
        assert!(!f0_html.contains("unresolved transclusion"));

        built.project.write_src("f1.stuff", "#1 f1\n\nMODIFIED\n");
        let built2 = built.project.build_and_run();
        assert_rebuilt(&built2, 4);
        assert_rebuilt(&built2.project.build_and_run(), 0);
    });
}

#[test]
fn chain_stress_test() {
    run_with_timeout(Duration::from_secs(10), || {
        const N: usize = 10_000;
        let built = test_project().add_chain(N).build().build_and_run();

        assert_rebuilt(&built, N);
        assert_output_contains(&built, "f9999.html", "Leaf content");

        built
            .project
            .write_src("f0.stuff", "#1 f0\n\nCHANGED ROOT\n");
        let built2 = built.project.build_and_run();
        assert_rebuilt(&built2, N);
        assert_output_contains(&built2, "f9999.html", "CHANGED ROOT");

        assert_rebuilt(&built2.project.build_and_run(), 0);
    });
}

#[test]
fn star_stress_test() {
    run_with_timeout(Duration::from_secs(5), || {
        const N: usize = 1_000;
        let built = test_project().add_star(N).build().build_and_run();

        assert_rebuilt(&built, N);
        assert_output_contains(&built, "f1.html", "Leaf content");

        built
            .project
            .write_src("f0.stuff", "#1 f0\n\nCENTER MODIFIED\n");
        let built2 = built.project.build_and_run();
        assert_rebuilt(&built2, N);
        assert_output_contains(&built2, "f1.html", "CENTER MODIFIED");

        assert_rebuilt(&built2.project.build_and_run(), 0);
    });
}

#[test]
fn grid_diamond_medium() {
    run_with_timeout(Duration::from_secs(5), || {
        let built = test_project()
            .add_grid_diamond(20) // 400 files
            .build()
            .build_and_run();

        assert_rebuilt(&built, 400);
        assert_output_contains(&built, "f399.html", "Leaf content");

        built
            .project
            .write_src("f0.stuff", "#1 f0\n\nGRID ROOT MODIFIED\n");
        let built2 = built.project.build_and_run();
        assert_rebuilt(&built2, 400);
        assert_output_contains(&built2, "f399.html", "GRID ROOT MODIFIED");

        assert_rebuilt(&built2.project.build_and_run(), 0);
    });
}

#[test]
fn ring_cycle_handling() {
    run_with_timeout(Duration::from_secs(2), || {
        let built = test_project().add_ring(10).build().build_and_run();

        let f0_html = built.read_out("f0.html");
        assert!(f0_html.contains("<h1> f0</h1>"));
        assert!(!f0_html.contains("unresolved transclusion"));
    });
}
#[test]
fn unresolved_transclusion_renders_comment() {
    assert_render("#1 Title\n\n!&missing.0\n", "<!-- unresolved transclusion");
}

#[test]
fn unresolved_reference_renders_comment() {
    assert_render("#1 Title\n\nSee &missing.\n", "<!-- unresolved ref");
}

#[test]
fn multiple_files_no_transclusion_independent() {
    let built = test_project()
        .file("a.stuff", "#1 A\n\nA content.\n")
        .file("b.stuff", "#1 B\n\nB content.\n")
        .file("c.stuff", "#1 C\n\nC content.\n")
        .build()
        .build_and_run();
    assert_rebuilt(&built, 3);

    built.project.write_src("b.stuff", "#1 B\n\nB modified.\n");
    let built2 = built.project.build_and_run();
    assert_rebuilt(&built2, 1);
}

#[test]
fn link_renders_with_target() {
    assert_render(
        "[[https://example.com|click here]]\n",
        "<a href=\"https://example.com\">click here</a>",
    );
}

#[test]
fn nested_formatting_renders() {
    assert_render("*bold and _italic inside_*\n", "<strong>");
}

#[test]
fn wiki_link_resolves_to_html_path() {
    let built = test_project()
        .file("target-page.stuff", "#1 Target\n\nHello.\n")
        .file("source.stuff", "see [[target-page]] here\n")
        .build()
        .build_and_run();
    let html = built.read_out("source.html");
    assert!(html.contains("href=\"target-page.html\""));
    assert!(html.contains(">target-page<"));
}

#[test]
fn wiki_link_with_custom_display() {
    let built = test_project()
        .file("target-page.stuff", "#1 Target\n\nHello.\n")
        .file("source.stuff", "see [[target-page|click me]]\n")
        .build()
        .build_and_run();
    let html = built.read_out("source.html");
    assert!(html.contains("href=\"target-page.html\""));
    assert!(html.contains(">click me<"));
}

#[test]
fn wiki_link_normalizes_page_name() {
    let built = test_project()
        .file("my-page.stuff", "#1 My Page\n\nHello.\n")
        .file("source.stuff", "see [[My Page]]\n")
        .build()
        .build_and_run();
    let html = built.read_out("source.html");
    assert!(html.contains("<a href=\"my-page.html\" title=\"My Page\">My Page</a>"));
}

#[test]
fn url_link_still_works() {
    assert_render(
        "[[https://example.com|click here]]\n",
        "<a href=\"https://example.com\">click here</a>",
    );
}

#[test]
fn wiki_link_missing_page_shows_class() {
    let built = test_project()
        .file("source.stuff", "see [[missing-page]]\n")
        .build()
        .build_and_run();
    let html = built.read_out("source.html");
    assert!(html.contains("class=\"wiki-missing\""));
}

#[test]
fn wiki_link_backlinks_section_appears() {
    let built = test_project()
        .file("a.stuff", "#1 A\n\nLinks to [[b]].\n")
        .file("b.stuff", "#1 B\n\nContent.\n")
        .build()
        .build_and_run();
    let html = built.read_out("b.html");
    assert!(html.contains("meta name=\"sbd:backlinks\""));
}

#[test]
fn wiki_link_no_backlinks_when_none() {
    let built = test_project()
        .file("orphan.stuff", "#1 Orphan\n\nAlone.\n")
        .build()
        .build_and_run();
    let html = built.read_out("orphan.html");
    assert!(!html.contains("meta name=\"sbd:backlinks\""));
}

#[test]
fn wiki_link_outlinks_section_appears() {
    let built = test_project()
        .file("a.stuff", "links to [[b]] and [[c]]\n")
        .file("b.stuff", "#1 B\n\nContent.\n")
        .file("c.stuff", "#1 C\n\nContent.\n")
        .build()
        .build_and_run();
    let html = built.read_out("a.html");
    assert!(html.contains("meta name=\"sbd:outlinks\""));
}

#[test]
fn wiki_link_incremental_rebuild() {
    let built = test_project()
        .file("target.stuff", "#1 Target\n\nOriginal.\n")
        .file("source.stuff", "see [[target]]\n")
        .build()
        .build_and_run();
    assert_rebuilt(&built, 2);

    let html = built.read_out("source.html");
    assert!(html.contains("<a href=\"target.html\" title=\"Target\">target</a>"));

    built
        .project
        .write_src("target.stuff", "#1 Target\n\nModified.\n");
    let built2 = built.project.build_and_run();
    assert!(built2.rebuilt_count() > 0);
}

#[test]
fn wiki_link_subdirectory_resolution() {
    let built = test_project()
        .file("sub/deep-page.stuff", "#1 Deep\n\nHidden.\n")
        .file("source.stuff", "see [[sub/deep-page]]\n")
        .build()
        .build_and_run();
    let html = built.read_out("source.html");
    assert!(html.contains("<a href=\"sub/deep-page.html\" title=\"Deep\">sub/deep-page</a>"));
}

#[test]
fn wiki_chain_stress() {
    run_with_timeout(Duration::from_secs(15), || {
        const N: usize = 10_000;
        let built = test_project().add_wiki_chain(N).build().build_and_run();
        assert_rebuilt(&built, N);
        assert_output_contains(
            &built,
            "f9999.html",
            "<a href=\"f9998.html\" title=\"f9998\">f9998</a>",
        );
        assert_output_contains(&built, "f0.html", "meta name=\"sbd:backlinks\"");
        assert_output_contains(&built, "f9999.html", "<!-- orphan: true -->");
        assert_output_not_contains(&built, "f0.html", "<!-- orphan: true -->");
        assert_output_not_contains(&built, "f9999.html", "meta name=\"sbd:backlinks\"");

        let built2 = built.project.build_and_run();
        assert_rebuilt(&built2, 0);
    });
}

#[test]
fn wiki_star_stress() {
    run_with_timeout(Duration::from_secs(10), || {
        const N: usize = 1_000;
        let built = test_project().add_wiki_star(N).build().build_and_run();
        assert_rebuilt(&built, N);
        assert_output_contains(&built, "f0.html", "meta name=\"sbd:backlinks\"");
        assert_output_contains(&built, "f1.html", "<a href=\"f0.html\" title=\"f0\">f0</a>");
        assert_output_contains(&built, "f1.html", "<!-- orphan: true -->");
        assert_output_not_contains(&built, "f0.html", "<!-- orphan: true -->");

        let built2 = built.project.build_and_run();
        assert_rebuilt(&built2, 0);
    });
}

#[test]
fn wiki_all_pairs_medium() {
    run_with_timeout(Duration::from_secs(15), || {
        const N: usize = 200;
        let built = test_project().add_wiki_all_pairs(N).build().build_and_run();
        assert_rebuilt(&built, N);
        assert_output_contains(&built, "f0.html", "meta name=\"sbd:backlinks\"");
        assert_output_contains(&built, "f0.html", "meta name=\"sbd:outlinks\"");

        let built2 = built.project.build_and_run();
        assert_rebuilt(&built2, 0);
    });
}

#[test]
fn wiki_orphan_flood() {
    // I don't know if I shold insert a technoblade joke here. (RIP) 

    run_with_timeout(Duration::from_secs(10), || {
        const N: usize = 1_000;
        let built = test_project().add_wiki_orphans(N).build().build_and_run();
        assert_rebuilt(&built, N);
        assert_output_contains(&built, "f0.html", "<!-- orphan: true -->");
        assert_output_contains(&built, "f500.html", "<!-- orphan: true -->");
        assert_output_contains(&built, "f999.html", "<!-- orphan: true -->");

        let built2 = built.project.build_and_run();
        assert_rebuilt(&built2, 0);
    });
}

#[test]
fn wiki_broken_flood() {
    run_with_timeout(Duration::from_secs(10), || {
        const N: usize = 1_000;
        let built = test_project().add_wiki_broken(N).build().build_and_run();
        assert_rebuilt(&built, N);
        assert_output_contains(&built, "f0.html", "class=\"wiki-missing\"");
        assert_output_contains(&built, "f999.html", "class=\"wiki-missing\"");

        let built2 = built.project.build_and_run();
        assert_rebuilt(&built2, 0);
    });
}
