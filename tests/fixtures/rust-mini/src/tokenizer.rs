//! Splits text into words and tallies them.

use std::collections::HashMap;

/// Counts alphanumeric words, optionally folding case.
pub fn count_words(text: &str, case_sensitive: bool) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for word in text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
        let key = if case_sensitive { word.to_string() } else { word.to_lowercase() };
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}
