//! wordcount: prints the most frequent words in a file.

mod config;
mod report;
mod tokenizer;

use crate::config::Config;
use crate::report::render_top;
use crate::tokenizer::count_words;

/// Parses arguments, counts words, prints a report.
fn main() {
    let config = match Config::from_args(std::env::args().skip(1)) {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };
    let text = std::fs::read_to_string(&config.path).expect("readable input file");
    let counts = count_words(&text, config.case_sensitive);
    println!("{}", render_top(&counts, config.top));
}
