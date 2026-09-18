//! Command-line configuration.

/// Options controlling a wordcount run.
#[derive(Debug)]
pub struct Config {
    pub path: String,
    pub top: usize,
    pub case_sensitive: bool,
}

impl Config {
    /// Builds a config from `<path> [--top N] [--case-sensitive]`.
    pub fn from_args(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let path = args.next().ok_or("usage: wordcount <path> [--top N] [--case-sensitive]")?;
        let mut config = Config { path, top: 10, case_sensitive: false };
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--top" => {
                    let n = args.next().ok_or("--top needs a value")?;
                    config.top = n.parse().map_err(|_| format!("invalid --top: {n}"))?;
                }
                "--case-sensitive" => config.case_sensitive = true,
                other => return Err(format!("unknown argument: {other}")),
            }
        }
        Ok(config)
    }
}
