use lore_core::{extractor_for, SearchBuilder};
use std::fs;
use std::io::Write;

fn collect_matches(search: &lore_core::Search) -> Vec<lore_core::Match> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(async {
        use futures_util::StreamExt;
        search.run().collect::<Vec<_>>().await
    })
}

#[test]
fn search_matches_any_term() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("has-hashmap.md"), "hashmap only").unwrap();
    fs::write(dir.path().join("has-serde.md"), "serde only").unwrap();

    let search = SearchBuilder::new()
        .no_builtins()
        .add_path(dir.path())
        .add_pattern("hashmap")
        .add_pattern("serde")
        .build();

    let matches = collect_matches(&search);

    assert_eq!(matches.len(), 2);
}

#[test]
fn search_exclude_match_overrides_positive_match() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("candidate.md"), "hashmap but also deprecated").unwrap();

    let search = SearchBuilder::new()
        .no_builtins()
        .add_path(dir.path())
        .add_pattern("hashmap")
        .exclude_match("deprecated")
        .build();

    let matches = collect_matches(&search);

    assert!(matches.is_empty());
}

#[test]
fn search_collects_offsets_from_all_matches() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("multi.md"), "foo appears here and bar appears there too").unwrap();

    let search = SearchBuilder::new()
        .no_builtins()
        .add_path(dir.path())
        .add_pattern("foo")
        .add_pattern("bar")
        .build();

    let matches = collect_matches(&search);

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].offsets.len(), 2);
}

#[test]
fn search_without_match_set_returns_path_filtered_files() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("keep.md"), "anything").unwrap();
    fs::write(dir.path().join("skip.txt"), "anything").unwrap();

    let search = SearchBuilder::new()
        .no_builtins()
        .add_path(dir.path())
        .exclude_extension("txt")
        .build();

    let matches = collect_matches(&search);

    assert_eq!(matches.len(), 1);
    assert!(matches[0].path.ends_with("keep.md"));
}

#[test]
fn extractor_supports_compressed_text_files() {
    use flate2::write::GzEncoder;
    use flate2::Compression;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.txt.gz");

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(b"Compressed text content").unwrap();
    let compressed = encoder.finish().unwrap();
    fs::write(&path, compressed).unwrap();

    let extractor = extractor_for(&path).unwrap();
    let bytes = extractor.extract(&path).unwrap();

    assert_eq!(bytes, b"Compressed text content");
}

#[test]
fn extractor_returns_none_for_files_without_extension() {
    assert!(extractor_for(std::path::Path::new("Makefile")).is_none());
}

#[test]
fn extractor_detects_format_from_content_over_extension() {
    // A .txt file whose content carries a PDF magic header must be
    // dispatched to the PDF reader, not passed through as plain text.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mislabeled.txt");
    let bytes: &[u8] = b"%PDF-1.4\nnot really a valid pdf";
    fs::write(&path, bytes).unwrap();

    let extractor = extractor_for(&path).unwrap();
    let result = extractor.extract(&path);

    // Plain-text passthrough would return the bytes unchanged; the PDF
    // reader either errors on the malformed document or returns
    // extracted text — never the raw bytes.
    assert_ne!(result.ok().as_deref(), Some(bytes));
}
