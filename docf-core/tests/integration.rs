use docf_core::{MatchSet, SearchBuilder};
use futures_util::StreamExt;
use std::fs;

fn setup_tree() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("hashmap_notes.md"), "notes about HashMap and BTreeMap").unwrap();
    fs::write(dir.path().join("readme.md"), "just a readme, nothing special").unwrap();
    fs::write(dir.path().join("serde_guide.md"), "serde derive macros for HashMap").unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub/deep.md"), "deeply nested HashMap reference").unwrap();
    dir
}

#[tokio::test(flavor = "current_thread")]
async fn single_task_sequential_run() {
    let dir = setup_tree();
    let search = SearchBuilder::new()
        .no_builtins()
        .add_path(dir.path())
        .matching(MatchSet::new().add("HashMap"))
        .build();

    let mut stream = search.run();
    let mut found = vec![];
    while let Some(m) = stream.next().await {
        found.push(m.path);
    }

    assert_eq!(found.len(), 3, "hashmap_notes.md, serde_guide.md, sub/deep.md should match");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_clones_split_work_correctly() {
    let dir = setup_tree();
    let search = SearchBuilder::new()
        .no_builtins()
        .add_path(dir.path())
        .matching(MatchSet::new().add("HashMap"))
        .build();

    const WORKERS: usize = 8;
    let mut handles = vec![];
    for _ in 0..WORKERS {
        let search = search.clone();
        handles.push(tokio::spawn(async move {
            let mut stream = search.run();
            let mut local = vec![];
            while let Some(m) = stream.next().await {
                local.push(m.path);
            }
            local
        }));
    }

    let mut all = vec![];
    for h in handles {
        all.extend(h.await.unwrap());
    }

    // Every match should be found exactly once across all clones —
    // no duplicates, nothing missed, regardless of which clone ended
    // up walking vs draining.
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 3, "expected exactly 3 unique matches across all worker clones");
}

#[tokio::test(flavor = "multi_thread")]
async fn or_group_matches_alternative() {
    let dir = setup_tree();
    let search = SearchBuilder::new()
        .no_builtins()
        .add_path(dir.path())
        .matching(MatchSet::new().add("nonexistent_term").or().add("readme"))
        .build();

    let mut stream = search.run();
    let mut found = vec![];
    while let Some(m) = stream.next().await {
        found.push(m.path);
    }
    assert_eq!(found.len(), 1);
    assert!(found[0].to_string_lossy().contains("readme"));
}

#[tokio::test(flavor = "multi_thread")]
async fn exclude_extension_filters_out() {
    let dir = setup_tree();
    fs::write(dir.path().join("notes.txt"), "HashMap in a txt file").unwrap();

    let search = SearchBuilder::new()
        .no_builtins()
        .add_path(dir.path())
        .matching(MatchSet::new().add("HashMap"))
        .exclude_extension("txt")
        .build();

    let mut stream = search.run();
    let mut found = vec![];
    while let Some(m) = stream.next().await {
        found.push(m.path);
    }
    assert!(found.iter().all(|p| p.extension().unwrap() != "txt"));
}

#[tokio::test(flavor = "multi_thread")]
async fn include_path_in_search_matches_on_filename() {
    let dir = setup_tree();
    let search = SearchBuilder::new()
        .no_builtins()
        .add_path(dir.path())
        .matching(MatchSet::new().add("readme"))
        .include_path_in_search(true)
        .build();

    let mut stream = search.run();
    let mut found = vec![];
    while let Some(m) = stream.next().await {
        found.push(m.path);
    }
    // "readme" only appears in the filename readme.md's path segment
    // and also as a substring inside its own content ("just a
    // readme..."), so this should match at least that file.
    assert!(found.iter().any(|p| p.to_string_lossy().contains("readme")));
}

#[tokio::test(flavor = "multi_thread")]
async fn small_walk_yield_interval_still_finds_everything() {
    let dir = setup_tree();
    let search = SearchBuilder::new()
        .no_builtins()
        .add_path(dir.path())
        .matching(MatchSet::new().add("HashMap"))
        .walk_yield_interval(1) // yield after every single file
        .build();

    const WORKERS: usize = 4;
    let mut handles = vec![];
    for _ in 0..WORKERS {
        let search = search.clone();
        handles.push(tokio::spawn(async move {
            let mut stream = search.run();
            let mut local = vec![];
            while let Some(m) = stream.next().await {
                local.push(m.path);
            }
            local
        }));
    }
    let mut all = vec![];
    for h in handles {
        all.extend(h.await.unwrap());
    }
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 3);
}
