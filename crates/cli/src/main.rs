//! `autodoc` — scan repositories, validate DiagramIR, render editorial
//! diagrams, and serve the same engine over MCP.

mod config;
mod report;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use autodoc_analyzer::Depth;
use autodoc_ir::Theme;
use autodoc_mcp::engine::{self, CompileOutcome, CompileRequest, IrInput, OutputFormat};
use autodoc_renderer::Accent;
use autodoc_validator::{validate_json, ValidateOptions};
use clap::{Parser, Subcommand, ValueEnum};

use config::Config;

#[derive(Parser)]
#[command(name = "autodoc", version, about = "Verifiable, editorial architecture diagrams from source code")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum DepthArg {
    System,
    Container,
    Component,
}

impl From<DepthArg> for Depth {
    fn from(d: DepthArg) -> Depth {
        match d {
            DepthArg::System => Depth::System,
            DepthArg::Container => Depth::Container,
            DepthArg::Component => Depth::Component,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum FormatArg {
    Html,
    Svg,
}

#[derive(Clone, Copy, ValueEnum)]
enum ThemeArg {
    Light,
    Dark,
}

impl From<ThemeArg> for Theme {
    fn from(t: ThemeArg) -> Theme {
        match t {
            ThemeArg::Light => Theme::EditorialLight,
            ThemeArg::Dark => Theme::EditorialDark,
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Write an autodoc.toml with editorial defaults into a repository.
    Init {
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Overwrite an existing autodoc.toml.
        #[arg(long)]
        force: bool,
    },
    /// Scan a repository: C4 containers, relationships, evidence, draft IR.
    Analyze {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, value_enum)]
        depth: Option<DepthArg>,
        /// Container id to decompose at component depth.
        #[arg(long)]
        focus: Option<String>,
        /// Write the full scan report as JSON (`-` for stdout).
        #[arg(long, value_name = "FILE")]
        json: Option<PathBuf>,
        /// Write the draft DiagramIR.
        #[arg(long, value_name = "FILE")]
        emit_ir: Option<PathBuf>,
        #[arg(long, value_enum)]
        theme: Option<ThemeArg>,
        /// Also scan test, fixture and example directories.
        #[arg(long)]
        include_tests: bool,
    },
    /// Validate a DiagramIR file and print diagnostics (exit 1 on errors).
    Validate {
        ir: PathBuf,
        /// Repository used to verify evidence (defaults to metadata.targetRepo).
        #[arg(long)]
        repo: Option<PathBuf>,
        #[arg(long)]
        strict: bool,
        /// Print the machine-readable report.
        #[arg(long)]
        json: bool,
    },
    /// Compile a DiagramIR into standalone HTML or SVG.
    Render {
        ir: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long, value_enum)]
        format: Option<FormatArg>,
        #[arg(long)]
        repo: Option<PathBuf>,
        /// indigo | coral | #RRGGBB
        #[arg(long)]
        accent: Option<String>,
        #[arg(long)]
        strict: bool,
        /// Skip evidence verification and snippets.
        #[arg(long)]
        no_verify: bool,
        #[arg(long)]
        json: bool,
    },
    /// Generate the architecture book: index.html, Markdown mirror, llms.txt, diagrams.
    Generate {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short, long)]
        out: Option<PathBuf>,
        #[arg(long)]
        accent: Option<String>,
        #[arg(long, value_enum)]
        theme: Option<ThemeArg>,
        #[arg(long)]
        include_tests: bool,
        #[arg(long)]
        json: bool,
    },
    /// What changed architecturally between two revisions. Markdown for a PR comment.
    Diff {
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Revision to compare against, such as `main` or `origin/main`.
        #[arg(long, default_value = "HEAD~1")]
        base: String,
        /// Revision to compare. Defaults to the working tree's HEAD.
        #[arg(long, default_value = "HEAD")]
        head: String,
        #[arg(long)]
        include_tests: bool,
        #[arg(long)]
        json: bool,
        /// Exit 1 when anything changed, so a pipeline can require a review.
        #[arg(long)]
        exit_code: bool,
    },
    /// Fail (exit 1) when the book is out of date or cites stale code. For CI.
    Check {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short, long)]
        out: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Verify evidence references like `src/main.rs:10-42`.
    Verify {
        #[arg(required = true, value_name = "FILE:LINE[-END]")]
        refs: Vec<String>,
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        /// Pinned commit to check drift against (defaults to HEAD).
        #[arg(long)]
        commit: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Print the DiagramIR JSON Schema.
    Schema,
    /// Run the MCP server on stdio.
    Serve,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::Init { path, force } => init(&path, force),
        Command::Analyze { path, depth, focus, json, emit_ir, theme, include_tests } => {
            let cfg = Config::discover(&path)?;
            let depth = depth.map(Depth::from).unwrap_or(cfg.analysis.depth);
            let focus = focus.or(cfg.analysis.focus.clone());
            let theme = theme.map(Theme::from).unwrap_or(cfg.style.theme);
            let include_tests = include_tests || cfg.analysis.include_tests;
            let res = engine::scan_repository(&path, depth, focus, include_tests, theme)?;
            if let Some(dest) = &json {
                write_or_stdout(dest, &serde_json::to_string_pretty(&res)?)?;
            }
            if let Some(dest) = &emit_ir {
                write_or_stdout(dest, &(res.draft_ir.to_json_pretty() + "\n"))?;
            }
            if json.as_deref() != Some(Path::new("-")) {
                print!("{}", report::scan_summary(&res));
                if let Some(dest) = &emit_ir {
                    println!("\nDraft IR written to {}", dest.display());
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Validate { ir, repo, strict, json } => {
            let text = std::fs::read_to_string(&ir).with_context(|| format!("reading {}", ir.display()))?;
            let cfg = Config::discover(ir.parent().unwrap_or(Path::new(".")))?;
            let repo = repo.or_else(|| target_repo(&text));
            let (_, report) = validate_json(
                &text,
                &ValidateOptions {
                    repo_root: repo,
                    verify_evidence: true,
                    max_density: cfg.validation.max_density,
                    strict: strict || cfg.validation.strict,
                    evidence_cache: None,
                },
            );
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", report::validation(&report));
            }
            Ok(if report.valid { ExitCode::SUCCESS } else { ExitCode::from(1) })
        }
        Command::Render { ir, output, format, repo, accent, strict, no_verify, json } => {
            let text = std::fs::read_to_string(&ir).with_context(|| format!("reading {}", ir.display()))?;
            let cfg = Config::discover(ir.parent().unwrap_or(Path::new(".")))?;
            let format = match format {
                Some(FormatArg::Svg) => OutputFormat::Svg,
                Some(FormatArg::Html) => OutputFormat::Html,
                None => match output.as_ref().and_then(|o| o.extension()).and_then(|e| e.to_str()) {
                    Some("svg") => OutputFormat::Svg,
                    _ => cfg.output.format,
                },
            };
            let output = output.unwrap_or_else(|| {
                let stem = ir.file_stem().and_then(|s| s.to_str()).unwrap_or("diagram").trim_end_matches(".ir");
                ir.with_file_name(format!("{stem}.{}", format.extension()))
            });
            let accent = parse_accent(accent.as_deref(), &cfg)?;
            let outcome = engine::compile_diagram(&CompileRequest {
                ir: IrInput::Json(text),
                output_path: output,
                format,
                repo_path: repo,
                accent,
                strict: strict || cfg.validation.strict,
                verify_evidence: !no_verify,
                max_density: cfg.validation.max_density,
            })?;
            if json {
                println!("{}", serde_json::to_string_pretty(&outcome)?);
            } else {
                print!("{}", report::compile(&outcome));
            }
            Ok(match outcome {
                CompileOutcome::Compiled(_) => ExitCode::SUCCESS,
                CompileOutcome::Rejected { .. } => ExitCode::from(1),
            })
        }
        Command::Generate { path, out, accent, theme, include_tests, json } => {
            generate(&path, out, accent, theme, include_tests, json)
        }
        Command::Diff { path, base, head, include_tests, json, exit_code } => {
            diff(&path, &base, &head, include_tests, json, exit_code)
        }
        Command::Check { path, out, json } => check(&path, out, json),
        Command::Verify { refs, repo, commit, json } => {
            let items = refs
                .iter()
                .map(|r| engine::parse_evidence_arg(r))
                .collect::<Result<Vec<_>, _>>()
                .map_err(anyhow::Error::msg)?;
            let res = engine::verify_evidence(&repo, &items, commit.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&res)?);
            } else {
                print!("{}", report::verify(&res));
            }
            Ok(if res.all_verified { ExitCode::SUCCESS } else { ExitCode::from(1) })
        }
        Command::Schema => {
            println!("{}", serde_json::to_string_pretty(&autodoc_ir::json_schema())?);
            Ok(ExitCode::SUCCESS)
        }
        Command::Serve => {
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            autodoc_mcp::Server::default().serve(stdin.lock(), stdout.lock())?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn target_repo(ir_json: &str) -> Option<PathBuf> {
    let v: serde_json::Value = serde_json::from_str(ir_json).ok()?;
    let p = PathBuf::from(v.get("metadata")?.get("targetRepo")?.as_str()?);
    p.is_dir().then_some(p)
}

fn parse_accent(flag: Option<&str>, cfg: &Config) -> Result<Accent> {
    Accent::parse(flag.unwrap_or(&cfg.style.accent)).map_err(anyhow::Error::msg)
}

fn write_or_stdout(dest: &Path, content: &str) -> Result<()> {
    if dest == Path::new("-") {
        std::io::stdout().write_all(content.as_bytes())?;
        return Ok(());
    }
    if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(dest, content).with_context(|| format!("writing {}", dest.display()))
}

fn init(path: &Path, force: bool) -> Result<ExitCode> {
    if !path.is_dir() {
        bail!("{} is not a directory", path.display());
    }
    let file = path.join(config::FILE_NAME);
    if file.exists() && !force {
        bail!("{} already exists (use --force to overwrite)", file.display());
    }
    std::fs::write(&file, config::TEMPLATE)?;
    println!("Created {}", file.display());
    println!();
    println!("Next steps:");
    println!("  autodoc generate {}            # system, container and component diagrams", path.display());
    println!("  autodoc analyze {} --emit-ir architecture.ir.json", path.display());
    println!("  autodoc render architecture.ir.json");
    println!();
    println!("Use it from an agent (MCP over stdio):");
    println!("  claude mcp add autodoc -- autodoc serve");
    Ok(ExitCode::SUCCESS)
}

fn book_options(
    cfg: &Config,
    accent: Option<&str>,
    theme: Option<ThemeArg>,
    include_tests: bool,
) -> Result<autodoc_book::BookOptions> {
    Ok(autodoc_book::BookOptions {
        theme: theme.map(Theme::from).unwrap_or(cfg.style.theme),
        accent: parse_accent(accent, cfg)?,
        include_tests: include_tests || cfg.analysis.include_tests,
        max_density: cfg.validation.max_density,
    })
}

fn generate(
    path: &Path,
    out: Option<PathBuf>,
    accent: Option<String>,
    theme: Option<ThemeArg>,
    include_tests: bool,
    json: bool,
) -> Result<ExitCode> {
    let cfg = Config::discover(path)?;
    let out = match out {
        Some(explicit) => explicit,
        None => {
            crate::config::confined(&cfg.output.dir)?;
            path.join(&cfg.output.dir)
        }
    };
    let opts = book_options(&cfg, accent.as_deref(), theme, include_tests)?;
    let report = autodoc_book::generate(path, &out, &opts)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report::book(&report));
    }
    Ok(ExitCode::SUCCESS)
}

/// Scan two revisions in throwaway worktrees and report what the architecture
/// model disagrees about.
///
/// Worktrees rather than the directory in place: comparing revisions must not
/// touch the caller's working tree, and in CI the checkout is the thing being
/// tested.
fn diff(path: &Path, base: &str, head: &str, include_tests: bool, json: bool, exit_code: bool) -> Result<ExitCode> {
    let ctx = autodoc_git::repo_context(path);
    let git_root =
        ctx.git_root.clone().with_context(|| format!("{} is not inside a git repository", path.display()))?;
    let prefix = ctx.prefix.clone();
    let tmp = tempfile::tempdir().context("cannot create a temporary directory for the comparison")?;

    let opts = autodoc_analyzer::ScanOptions { behavior: true, include_tests, ..Default::default() };
    let scan_at = |rev: &str| -> Result<autodoc_analyzer::ScanReport> {
        let wt = autodoc_git::worktree_at(&git_root, rev, tmp.path())
            .with_context(|| format!("cannot read revision `{rev}`"))?;
        let root = if prefix.is_empty() { wt.path().to_path_buf() } else { wt.path().join(&prefix) };
        let report = autodoc_analyzer::scan(&root, &opts).with_context(|| format!("cannot scan `{rev}`"))?;
        Ok(report)
    };
    let before = scan_at(base)?;
    let after = scan_at(head)?;

    let mut d = autodoc_analyzer::diff::diff(&before, &after);
    d.base = base.to_string();
    d.head = head.to_string();
    if json {
        println!("{}", serde_json::to_string_pretty(&d)?);
    } else {
        print!("{}", autodoc_analyzer::diff::markdown(&d));
    }
    Ok(if exit_code && !d.is_empty() { ExitCode::from(1) } else { ExitCode::SUCCESS })
}

fn check(path: &Path, out: Option<PathBuf>, json: bool) -> Result<ExitCode> {
    let cfg = Config::discover(path)?;
    let out = match out {
        Some(explicit) => explicit,
        None => {
            crate::config::confined(&cfg.output.dir)?;
            path.join(&cfg.output.dir)
        }
    };
    let opts = book_options(&cfg, None, None, false)?;
    let report = autodoc_book::check(path, &out, &opts)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report::check(&report));
    }
    Ok(if report.ok { ExitCode::SUCCESS } else { ExitCode::from(1) })
}
