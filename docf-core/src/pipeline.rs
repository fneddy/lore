use crate::error::DocfError;
use crate::extract::extractor_for;
use crate::match_::{Match, SourceKind};
use crate::match_set::MatchSet;
use crate::params::SearchParams;
use aho_corasick::AhoCorasick;
use std::path::Path;

/// Extension allowlist/denylist + excluded-path-prefix check. Cheap —
/// no extraction, just metadata.
pub(crate) fn passes_path_filters(path: &Path, params: &SearchParams) -> bool {
    if params
        .excluded_paths
        .iter()
        .any(|ex| path.starts_with(ex))
    {
        return false;
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if !params.extensions.is_empty()
        && !params
            .extensions
            .iter()
            .any(|e| e.trim_start_matches('.').eq_ignore_ascii_case(&ext))
    {
        return false;
    }

    if params
        .excluded_extensions
        .iter()
        .any(|e| e.trim_start_matches('.').eq_ignore_ascii_case(&ext))
    {
        return false;
    }

    true
}

/// Concatenates path + extracted content when `include_path_in_search`
/// is set, so a single pattern can hit either half without being
/// checked separately.
pub(crate) fn build_searchable_text(path: &Path, extracted: &str, params: &SearchParams) -> String {
    if params.include_path_in_search {
        format!("{}\n{}", path.to_string_lossy(), extracted)
    } else {
        extracted.to_string()
    }
}

/// Single-pass evaluation of the whole `MatchSet` against `text`.
/// Every group is checked, no short-circuiting on the first satisfying
/// group — offsets are collected from every fully-satisfied group.
/// Returns `None` if excluded or if no group is fully satisfied.
pub(crate) fn document_matches(
    text: &str,
    match_set: &Option<MatchSet>,
    exclude_patterns: &[String],
) -> Option<Vec<usize>> {
    if !exclude_patterns.is_empty() {
        if let Ok(ac) = AhoCorasick::new(exclude_patterns) {
            if ac.is_match(text) {
                return None;
            }
        }
    }

    let Some(set) = match_set else {
        return Some(vec![]);
    };

    if set.is_empty() {
        return Some(vec![]);
    }

    // Flatten every term across every group into one automaton, single
    // pass over the text, then re-associate each hit back to its
    // owning group to check per-group AND satisfaction.
    let mut patterns: Vec<&str> = vec![];
    let mut owner_group: Vec<usize> = vec![];
    for (gi, group) in set.groups.iter().enumerate() {
        for term in group {
            patterns.push(term.as_str());
            owner_group.push(gi);
        }
    }

    if patterns.is_empty() {
        return Some(vec![]);
    }

    let ac = match AhoCorasick::new(&patterns) {
        Ok(ac) => ac,
        Err(_) => return None,
    };

    let mut hits_per_group: Vec<Vec<Option<usize>>> = set
        .groups
        .iter()
        .map(|g| vec![None; g.len()])
        .collect();
    // index within its own group for each flattened pattern, to know
    // which slot in hits_per_group[gi] to fill
    let mut term_index_in_group: Vec<usize> = vec![];
    {
        let mut counters = vec![0usize; set.groups.len()];
        for &gi in &owner_group {
            term_index_in_group.push(counters[gi]);
            counters[gi] += 1;
        }
    }

    for m in ac.find_overlapping_iter(text) {
        let pattern_idx = m.pattern().as_usize();
        let gi = owner_group[pattern_idx];
        let slot = term_index_in_group[pattern_idx];
        if hits_per_group[gi][slot].is_none() {
            hits_per_group[gi][slot] = Some(m.start());
        }
    }

    let mut offsets = vec![];
    let mut any_group_matched = false;

    for (gi, group) in set.groups.iter().enumerate() {
        if group.is_empty() {
            continue;
        }
        let hits = &hits_per_group[gi];
        if hits.iter().all(|h| h.is_some()) {
            any_group_matched = true;
            offsets.extend(hits.iter().flatten().copied());
        }
    }

    any_group_matched.then_some(offsets)
}

/// The core per-file decision: does this file pass path filters, and
/// (if extraction is needed) does its searchable text satisfy the
/// match set? `source` is supplied by the walker (known exactly, from
/// which root the file was discovered under — not guessed after the
/// fact). Returns `None` for files that simply don't match or aren't
/// extractable; returns `Some(Match)` with `extracted: Err(..)` for
/// files that matched path filters but failed extraction, so failures
/// are surfaced rather than silently dropped.
pub(crate) fn file_matches(path: &Path, source: SourceKind, params: &SearchParams) -> Option<Match> {
    if !passes_path_filters(path, params) {
        return None;
    }

    let needs_extraction = params.include_path_in_search
        || !params.exclude_patterns.is_empty()
        || params.match_set.as_ref().map_or(false, |s| !s.is_empty());

    if !needs_extraction {
        // No content filter at all — every path-filtered file matches.
        return Some(Match {
            path: path.to_owned(),
            source,
            extracted: Ok(()),
            offsets: vec![],
        });
    }

    let Some(extractor) = extractor_for(path) else {
        return None;
    };

    match extractor.extract(path) {
        Ok(extracted) => {
            let text = build_searchable_text(path, &extracted, params);
            document_matches(&text, &params.match_set, &params.exclude_patterns).map(|offsets| Match {
                path: path.to_owned(),
                source,
                extracted: Ok(()),
                offsets,
            })
        }
        Err(e) => Some(Match {
            path: path.to_owned(),
            source,
            extracted: Err(DocfError::from(e)),
            offsets: vec![],
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn or_of_and_groups() {
        let set = MatchSet::new().add("hashmap").add("serde").or().add("btreemap");
        let text = "this file mentions btreemap only";
        let result = document_matches(text, &Some(set), &[]);
        assert!(result.is_some());
    }

    #[test]
    fn and_group_requires_all_terms() {
        let set = MatchSet::new().add("hashmap").add("serde");
        let text = "this file mentions hashmap only";
        assert!(document_matches(text, &Some(set), &[]).is_none());
    }

    #[test]
    fn exclude_wins_over_match() {
        let set = MatchSet::new().add("hashmap");
        let text = "hashmap but also deprecated";
        let result = document_matches(text, &Some(set), &["deprecated".to_string()]);
        assert!(result.is_none());
    }

    #[test]
    fn every_group_scanned_offsets_from_all_satisfied() {
        let set = MatchSet::new().add("foo").or().add("bar");
        let text = "foo appears here and bar appears there too";
        let offsets = document_matches(text, &Some(set), &[]).unwrap();
        assert_eq!(offsets.len(), 2, "both groups satisfied, offsets from both expected");
    }

    #[test]
    fn no_match_set_matches_everything() {
        assert_eq!(document_matches("anything", &None, &[]), Some(vec![]));
    }
}
