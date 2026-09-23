//! A command-line tool, declared the way clap's derive style does it.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "ledger", version, about = "Reconcile a ledger, and report what does not balance")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Json,
}

#[derive(Subcommand)]
enum Command {
    /// Import entries from a statement file.
    Import {
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Fail instead of skipping a row that does not parse.
        #[arg(long)]
        strict: bool,
        /// Where to write the imported ledger.
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
    },
    /// Report the balance, and every entry that does not reconcile.
    Report {
        account: String,
        #[arg(long, value_enum)]
        format: Option<Format>,
    },
    /// Print the schema and exit.
    Schema,
}

fn main() {
    let _cli = Cli::parse();
}
