//! `autodoc.toml`: per-repository defaults. CLI flags always win.

use std::path::Path;

use anyhow::{Context, Result};
use autodoc_analyzer::Depth;
use autodoc_ir::Theme;
use autodoc_mcp::engine::OutputFormat;
use serde::Deserialize;

pub const FILE_NAME: &str = "autodoc.toml";

pub const TEMPLATE: &str = r#"# autodoc-engine configuration. CLI flags override these values.

[output]
dir = "docs/architecture"   # where `autodoc generate` writes diagrams
format = "html"             # default for `autodoc render` without -o (generate writes both)

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
"#;

#[derive(Debug, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub output: OutputConfig,
    pub style: StyleConfig,
    pub analysis: AnalysisConfig,
    pub validation: ValidationConfig,
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
        ValidationConfig { max_density: autodoc_ir::MAX_VISUAL_DENSITY, strict: false }
    }
}

impl Config {
    /// Nearest `autodoc.toml` walking up from `start`; defaults when absent.
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
