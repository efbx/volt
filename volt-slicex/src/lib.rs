//! Stable in-place deduplication for vectors.
//!
//! [`DedupStable`] removes repeated items from a `Vec` while keeping the first
//! occurrence of each in its original position, scanning short vectors and
//! hashing long ones.

#![warn(missing_docs)]

use std::collections::HashSet;
use std::hash::Hash;

/// The length up to which comparing against the kept prefix beats building a
/// hash set.
const SCAN_LIMIT: usize = 64;

/// Deduplication that keeps the first occurrence of each item where it was.
pub trait DedupStable {
    /// Removes every repeated item, keeping the first occurrence and the order of
    /// what remains.
    fn unique_stable(&mut self);
    /// Appends `tail`, then removes the repeats across the combined vector.
    #[must_use]
    fn dedup_stable(self, tail: Self) -> Self;
}

impl<T: Eq + Hash> DedupStable for Vec<T> {
    fn unique_stable(&mut self) {
        match self.len() {
            len if len <= SCAN_LIMIT => dedup_by_scan(self),
            _ => dedup_by_hash(self),
        }
    }

    fn dedup_stable(mut self, tail: Self) -> Self {
        if self.is_empty() {
            self = tail;
        } else {
            self.extend(tail);
        }
        self.unique_stable();
        self
    }
}

/// Keeps the first of each item by comparing against the kept prefix, for short
/// vectors.
fn dedup_by_scan<T: PartialEq>(all: &mut Vec<T>) {
    let mut kept = 0;
    for index in 0..all.len() {
        if !all[..kept].contains(&all[index]) {
            all.swap(kept, index);
            kept += 1;
        }
    }
    all.truncate(kept);
}

/// Keeps the first of each item by recording what has been seen in a hash set,
/// for long vectors.
fn dedup_by_hash<T: Eq + Hash>(all: &mut Vec<T>) {
    let mut seen = HashSet::with_capacity(all.len());
    let keep: Vec<bool> = all.iter().map(|item| seen.insert(item)).collect();
    let mut keep = keep.into_iter();
    all.retain(|_| keep.next().unwrap_or(false));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_stable_merges_first_occurrence_wins() {
        for (head, tail, expected, ctx) in [
            (
                vec!["a", "b"],
                vec!["c", "d"],
                vec!["a", "b", "c", "d"],
                "disjoint keeps order",
            ),
            (
                vec!["new", "hi"],
                vec!["hi", "old"],
                vec!["new", "hi", "old"],
                "cross duplicates keep first position",
            ),
            (
                vec!["x", "x", "y"],
                vec!["y", "x"],
                vec!["x", "y"],
                "duplicates within head collapse too",
            ),
            (vec![], vec!["a"], vec!["a"], "empty head"),
            (vec!["a"], vec![], vec!["a"], "empty tail"),
            (vec![], vec![], vec![], "both empty"),
        ] {
            let head: Vec<String> = head.into_iter().map(String::from).collect();
            let tail: Vec<String> = tail.into_iter().map(String::from).collect();
            let expected: Vec<String> = expected.into_iter().map(String::from).collect();
            assert_eq!(head.dedup_stable(tail), expected, "{ctx}");
        }
    }

    #[test]
    fn unique_stable_keeps_the_first_of_each() {
        let mut list: Vec<String> = ["Alt", "Png2", "Alt", "", "", "Last", "Png2"]
            .into_iter()
            .map(String::from)
            .collect();
        list.unique_stable();
        assert_eq!(list, vec!["Alt", "Png2", "", "Last"]);

        let mut empty = Vec::<String>::new();
        empty.unique_stable();
        assert_eq!(empty, Vec::<String>::new());

        let limit = u32::try_from(SCAN_LIMIT).unwrap();
        let mut large: Vec<u32> = (0..limit).chain([3, 500, 7, 500]).collect();
        large.unique_stable();
        let expected: Vec<u32> = (0..limit).chain([500]).collect();
        assert_eq!(large, expected, "the hash path keeps the same order");
    }

    #[test]
    fn dedup_stable_above_scan_limit_matches_small_path_semantics() {
        let limit = u32::try_from(SCAN_LIMIT).unwrap();
        let head: Vec<u32> = (0..limit).chain([3, 7]).collect();
        let tail: Vec<u32> = [7, 500, 3, 501, 500].into();
        let mut expected: Vec<u32> = (0..limit).collect();
        expected.extend([500, 501]);
        assert!(head.len() + tail.len() > SCAN_LIMIT);
        assert_eq!(head.dedup_stable(tail), expected);
    }
}
