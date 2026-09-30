// A small Rust sample to show off rvim's highlighting.
/*
 * This is a multi-line block comment. rvim tracks block-comment state
 * across line boundaries, so every line here stays comment-colored —
 * including `keywords`, "strings", and 1234 numbers that would otherwise
 * be highlighted.
 */
use std::collections::HashMap;

/// Count word frequencies in a blob of text.
fn word_counts(text: &str) -> HashMap<String, u32> {
    let mut counts: HashMap<String, u32> = HashMap::new();
    for word in text.split_whitespace() {
        *counts.entry(word.to_lowercase()).or_insert(0) += 1;
    }
    counts
}

fn main() {
    let sample = "the quick brown fox the lazy dog the";
    let counts = word_counts(sample);
    let mut pairs: Vec<(&String, &u32)> = counts.iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(a.1));
    for (word, n) in pairs {
        println!("{word:>8}: {n}");
    }
}
