//! `nunki.toml`: per-repository defaults. CLI flags always win.

use std::path::Path;

use anyhow::{Context, Result};
use nunki_analyzer::Depth;
use nunki_ir::Theme;
use nunki_mcp::engine::OutputFormat;
use serde::Deserialize;

pub const FILE_NAME: &str = "nunki.toml";

pub const TEMPLATE: &str = r#"# nunki configuration. CLI flags override these values.

[output]
dir = "docs/architecture"   # where `nunki generate` writes diagrams
format = "html"             # default for `nunki render` without -o (generate writes both)

[style]
theme = "editorial-light"   # editorial-light | editorial-dark
accent = "indigo"           # indigo | coral | #RRGGBB — one accent hue, used sparingly

[analysis]
depth = "container"         # system | container | component
# focus = "api-gateway"     # container to decompose at component depth
include_tests = false       # scan test, fixture and example directories too

[validation]
max_density = 0.40          # may be lowered, never raised above 0.40
strict = false              # treat warnings as errors

# [workspace]
# Other repositories of the same system, relative to this one. A call that
# leaves this repository is resolved against the service that answers it, and
# the citation says which repository it was read from. Each has to be checked
# out, and has to be a git repository — nunki reads and cites its lines.
# members = ["../billing-service", "../identity-service"]
"#;

#[derive(Debug, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub output: OutputConfig,
    pub style: StyleConfig,
    pub analysis: AnalysisConfig,
    pub validation: ValidationConfig,
    pub workspace: WorkspaceConfig,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct WorkspaceConfig {
    /// Sibling repositories of the same system.
    pub members: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OutputConfig {
    pub dir: String,
    pub format: OutputFormat,
}

impl Default for OutputConfig {
    fn default() -> Self {
        OutputConfig { dir: "docs/architecture".into(), format: OutputFormat::Html }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StyleConfig {
    pub theme: Theme,
    pub accent: String,
}

impl Default for StyleConfig {
    fn default() -> Self {
        StyleConfig { theme: Theme::EditorialLight, accent: "indigo".into() }
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct AnalysisConfig {
    pub depth: Depth,
    pub focus: Option<String>,
    pub include_tests: bool,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ValidationConfig {
    pub max_density: f64,
    pub strict: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        ValidationConfig { max_density: nunki_ir::MAX_VISUAL_DENSITY, strict: false }
    }
}

/// A configured output directory must stay inside the repository it describes:
/// an absolute path replaces the base outright when joined, and `..` climbs out
/// of it, either of which lets a scanned repository choose where nunki writes.
pub fn confined(dir: &str) -> Result<()> {
    let p = Path::new(dir);
    let escapes = p.is_absolute()
        || p.components().any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::RootDir))
        || dir.trim().is_empty();
    if escapes {
        anyhow::bail!(
            "output.dir `{dir}` must be a relative path inside the repository; pass --out to write somewhere else"
        );
    }
    Ok(())
}

/// Resolves configured member repositories against the repository being
/// documented.
///
/// Members are *meant* to escape the repository — a sibling checkout is the
/// normal shape — so `confined` does not apply. What applies instead is that a
/// member must be a git repository: nunki reads its lines and cites them at a
/// commit, so a directory with no commit cannot be cited, and requiring one
/// keeps a scanned `nunki.toml` from pointing nunki at an arbitrary directory
/// and quoting it into the book.
///
/// A member that is missing or is not a repository is reported and dropped
/// rather than failing the build: the common cause is a checkout that has not
/// happened yet, and the honest result is a book that says the call is
/// unresolved.
pub fn members_of(repo: &Path, declared: &[String]) -> (Vec<std::path::PathBuf>, Vec<String>) {
    let mut paths = Vec::new();
    let mut warnings = Vec::new();
    for m in declared {
        let p = Path::new(m);
        let joined = if p.is_absolute() { p.to_path_buf() } else { repo.join(p) };
        let resolved = joined.canonicalize().unwrap_or(joined);
        if !resolved.is_dir() {
            warnings.push(format!("workspace member `{m}` is not checked out; calls to it stay unresolved"));
        } else if !resolved.join(".git").exists() {
            warnings.push(format!(
                "workspace member `{m}` is not a git repository; its lines could not be cited at a commit"
            ));
        } else if resolved == repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf()) {
            warnings.push(format!("workspace member `{m}` is this repository; ignored"));
        } else if let Some(clash) = paths.iter().find(|p: &&std::path::PathBuf| {
            nunki_analyzer::scan::member_name(p) == nunki_analyzer::scan::member_name(&resolved)
        }) {
            // One name per repository is what makes a citation's `repo` an
            // address. Two members answering to the same name would attribute
            // each other's lines, so the second is refused rather than renamed
            // behind the reader's back.
            warnings.push(format!(
                "workspace member `{m}` is named the same as `{}`; ignored — rename one checkout",
                clash.display()
            ));
        } else {
            paths.push(resolved);
        }
    }
    (paths, warnings)
}

impl Config {
    /// Nearest `nunki.toml` walking up from `start`; defaults when absent.
    ///
    /// The file is part of the repository being documented, which is not
    /// necessarily code the user wrote, so its output directory is confined to
    /// the repository. `--out` is the user's own instruction and is not.
    pub fn discover(start: &Path) -> Result<Config> {
        let start = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
        let mut dir = Some(start.as_path());
        while let Some(d) = dir {
            let candidate = d.join(FILE_NAME);
            if candidate.is_file() {
                let text = std::fs::read_to_string(&candidate)?;
                return toml::from_str(&text).with_context(|| format!("invalid {}", candidate.display()));
            }
            dir = d.parent();
        }
        Ok(Config::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_parses_to_defaults() {
        let c: Config = toml::from_str(TEMPLATE).unwrap();
        assert_eq!(c.output.dir, "docs/architecture");
        assert_eq!(c.analysis.depth, Depth::Container);
        assert_eq!(c.validation.max_density, 0.40);
        assert!(toml::from_str::<Config>("[style]\naccnt = \"coral\"").is_err());
    }
}
