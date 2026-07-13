/// OR-of-AND match expression. Outer groups are ORed together; terms
/// within a group are ANDed. All matching is plain substring — no
/// fuzzy modes.
///
/// ```
/// use docf_core::MatchSet;
/// // "(hashmap AND serde) OR (btreemap AND collections)"
/// let set = MatchSet::new()
///     .add("hashmap")
///     .add("serde")
///     .or()
///     .add("btreemap")
///     .add("collections");
/// ```
#[derive(Debug, Clone, Default)]
pub struct MatchSet {
    pub(crate) groups: Vec<Vec<String>>,
}

impl MatchSet {
    pub fn new() -> Self {
        Self { groups: vec![vec![]] }
    }

    /// Adds a term to the current group (ANDed with terms already in it).
    pub fn add(mut self, pattern: impl Into<String>) -> Self {
        self.groups
            .last_mut()
            .expect("MatchSet always has at least one group")
            .push(pattern.into());
        self
    }

    /// Opens a new group. No-arg by design — it's a separator, not a
    /// term. Everything added after this ORs against everything before.
    pub fn or(mut self) -> Self {
        self.groups.push(vec![]);
        self
    }

    /// True if every group is empty (nothing was ever added).
    pub(crate) fn is_empty(&self) -> bool {
        self.groups.iter().all(|g| g.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_groups() {
        let set = MatchSet::new().add("a").add("b").or().add("c");
        assert_eq!(set.groups, vec![vec!["a".to_string(), "b".to_string()], vec!["c".to_string()]]);
    }

    #[test]
    fn empty_by_default() {
        assert!(MatchSet::new().is_empty());
        assert!(!MatchSet::new().add("x").is_empty());
    }
}
