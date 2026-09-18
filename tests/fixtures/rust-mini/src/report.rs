//! Formats word counts for the terminal.

use std::collections::HashMap;

use crate::tokenizer::count_words;

/// Renders the `top` most frequent words, ties broken alphabetically.
pub fn render_top(counts: &HashMap<String, usize>, top: usize) -> String {
    let mut rows: Vec<_> = counts.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    rows.into_iter()
        .take(top)
        .map(|(word, n)| format!("{n:>6}  {word}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_by_frequency() {
        let counts = count_words("b a b", false);
        assert_eq!(render_top(&counts, 1).trim(), "2  b");
    }
}
