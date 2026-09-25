//! `nunki` — scan repositories, validate DiagramIR, render editorial
//! diagrams, and serve the same engine over MCP.

mod config;
mod report;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use nunki_analyzer::Depth;
use nunki_ir::Theme;
use nunki_mcp::engine::{self, CompileOutcome, CompileRequest, IrInput, OutputFormat};
use nunki_renderer::Accent;
use nunki_validator::{validate_json, ValidateOptions};

use config::Config;

#[derive(Parser)]
#[command(name = "nunki", version, about = "Verifiable, editorial architecture diagrams from source code")]
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
enum SpecFormat {
    /// `openspec/specs/<capability>/spec.md`, the current-state layout.
    Openspec,
}

#[derive(Clone, Copy, ValueEnum)]
enum ExportFormat {
    /// A draw.io file, carrying the book's layout: open it directly.
    Drawio,
    /// CSV for draw.io's Extras → Insert → Advanced → CSV, to merge into an
    /// existing drawing. draw.io routes the edges itself, so the result is
    /// laid out less well than the `drawio` file.
    DrawioCsv,
}

impl ExportFormat {
    fn extension(self) -> &'static str {
        match self {
            ExportFormat::Drawio => "drawio",
            ExportFormat::DrawioCsv => "drawio.csv",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            ExportFormat::Drawio => "open with draw.io",
            ExportFormat::DrawioCsv => "draw.io: Extras → Insert → Advanced → CSV",
        }
    }
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
    /// Write an nunki.toml with editorial defaults into a repository.
    Init {
        /// Repository to write the configuration into.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Overwrite an existing nunki.toml.
        #[arg(long)]
        force: bool,
    },
    /// Scan a repository: C4 containers, relationships, evidence, draft IR.
    Analyze {
        /// Repository to scan.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// How far to decompose the system: system, container or component.
        /// Defaults to what nunki.toml says.
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
        /// Colour scheme recorded in the draft IR: light or dark.
        #[arg(long, value_enum)]
        theme: Option<ThemeArg>,
        /// Also scan test, fixture and example directories.
        #[arg(long)]
        include_tests: bool,
    },
    /// Validate a DiagramIR file and print diagnostics (exit 1 on errors).
    Validate {
        /// DiagramIR file to validate.
        ir: PathBuf,
        /// Repository used to verify evidence (defaults to metadata.targetRepo).
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Treat every warning as an error.
        #[arg(long)]
        strict: bool,
        /// Print the machine-readable report.
        #[arg(long)]
        json: bool,
    },
    /// Compile a DiagramIR into standalone HTML or SVG.
    Render {
        /// DiagramIR file to compile.
        ir: PathBuf,
        /// File to write. Defaults to the IR's name with the format's extension.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// What to write: html or svg. Defaults to the output file's extension.
        #[arg(long, value_enum)]
        format: Option<FormatArg>,
        /// Repository used to verify evidence (defaults to metadata.targetRepo).
        #[arg(long)]
        repo: Option<PathBuf>,
        /// indigo | coral | #RRGGBB
        #[arg(long)]
        accent: Option<String>,
        /// Treat every warning as an error.
        #[arg(long)]
        strict: bool,
        /// Skip evidence verification and snippets.
        #[arg(long)]
        no_verify: bool,
        /// Print the machine-readable report.
        #[arg(long)]
        json: bool,
    },
    /// Generate the architecture book: index.html, Markdown mirror, llms.txt, diagrams.
    Generate {
        /// Repository to document.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Directory to write the book into. Defaults to what nunki.toml says.
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// indigo | coral | #RRGGBB
        #[arg(long)]
        accent: Option<String>,
        /// Colour scheme the book opens in: light or dark. A reader can switch.
        #[arg(long, value_enum)]
        theme: Option<ThemeArg>,
        /// Also read test, fixture and example directories.
        #[arg(long)]
        include_tests: bool,
        /// Write the book even when almost nothing in the repository could be read.
        #[arg(long)]
        allow_partial: bool,
        /// Print the machine-readable report.
        #[arg(long)]
        json: bool,
    },
    /// Export a diagram for another tool. One way: the source stays authoritative.
    Export {
        /// DiagramIR file to export.
        ir: PathBuf,
        /// What to write: a draw.io file, or CSV to merge into an existing drawing.
        #[arg(long, value_enum, default_value = "drawio")]
        format: ExportFormat,
        /// Output file. Defaults to the IR's name with the format's extension; `-` writes to stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Repository the evidence belongs to, for permalinks. Defaults to the
        /// directory holding the IR, which for a generated book is inside it.
        #[arg(long)]
        repo: Option<PathBuf>,
    },
    /// Write the specification in another toolchain's format, from the code.
    Spec {
        /// Repository to read the specification from.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Dialect to write. `openspec` writes one current-state spec per
        /// capability, which `openspec validate --strict` will check.
        #[arg(long, value_enum, default_value = "openspec")]
        format: SpecFormat,
        /// Directory to write into. Defaults to the repository root, so the
        /// files land where the toolchain expects them.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Check the code against a specification someone else wrote.
    Conform {
        /// Repository to check against the specification.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Specification files. Defaults to the layouts the toolkits use:
        /// `.kiro/specs/*/requirements.md`, `openspec/specs/*/spec.md`,
        /// `specs/*/spec.md`.
        #[arg(long)]
        spec: Vec<PathBuf>,
        /// Print the machine-readable report.
        #[arg(long)]
        json: bool,
        /// Exit 1 when something declared is missing or contradicted.
        #[arg(long)]
        exit_code: bool,
    },
    /// What changed architecturally between two revisions. Markdown for a PR comment.
    Diff {
        /// Repository whose revisions are compared.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Revision to compare against, such as `main` or `origin/main`.
        #[arg(long, default_value = "HEAD~1")]
        base: String,
        /// Revision to compare. Defaults to the working tree's HEAD.
        #[arg(long, default_value = "HEAD")]
        head: String,
        /// Also read test, fixture and example directories.
        #[arg(long)]
        include_tests: bool,
        /// Print the machine-readable report.
        #[arg(long)]
        json: bool,
        /// Exit 1 when anything changed, so a pipeline can require a review.
        #[arg(long)]
        exit_code: bool,
        /// Append this comparison to the book's architecture history, as the
        /// release named here. Run at release time: the entry is committed and
        /// never recomputed, so the history survives the code it describes.
        #[arg(long, value_name = "VERSION")]
        record: Option<String>,
        /// Book directory to record into. Defaults to whatever nunki.toml says.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Fail (exit 1) when the book is out of date or cites stale code. For CI.
    Check {
        /// Repository the book describes.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Book directory to check. Defaults to what nunki.toml says.
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Print the machine-readable report.
        #[arg(long)]
        json: bool,
    },
    /// Verify evidence references like `src/main.rs:10-42`.
    Verify {
        /// References to check, as `file:line` or `file:start-end`.
        #[arg(required = true, value_name = "FILE:LINE[-END]")]
        refs: Vec<String>,
        /// Repository the references are read from.
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        /// Pinned commit to check drift against (defaults to HEAD).
        #[arg(long)]
        commit: Option<String>,
        /// Print the machine-readable report.
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
                    // `validate` checks one file against one repository. Evidence
                    // from a member is reported as unverifiable here rather than
                    // checked against the wrong root; `check` is what reads a
                    // whole workspace.
                    member_roots: Default::default(),
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
        Command::Generate { path, out, accent, theme, include_tests, allow_partial, json } => {
            generate(&path, out, accent, theme, include_tests, allow_partial, json)
        }
        Command::Export { ir, format, output, repo } => export(&ir, format, output, repo),
        Command::Spec { path, format, out } => spec(&path, format, out),
        Command::Conform { path, spec, json, exit_code } => conform(&path, spec, json, exit_code),
        Command::Diff { path, base, head, include_tests, json, exit_code, record, out } => {
            diff(&path, &base, &head, include_tests, json, exit_code, record, out)
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
            println!("{}", serde_json::to_string_pretty(&nunki_ir::json_schema())?);
            Ok(ExitCode::SUCCESS)
        }
        Command::Serve => {
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            nunki_mcp::Server::default().serve(stdin.lock(), stdout.lock())?;
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
    println!("  nunki generate {}            # system, container and component diagrams", path.display());
    println!("  nunki analyze {} --emit-ir architecture.ir.json", path.display());
    println!("  nunki render architecture.ir.json");
    println!();
    println!("Use it from an agent (MCP over stdio):");
    println!("  claude mcp add nunki -- nunki serve");
    Ok(ExitCode::SUCCESS)
}

fn book_options(
    repo: &Path,
    cfg: &Config,
    accent: Option<&str>,
    theme: Option<ThemeArg>,
    include_tests: bool,
    allow_partial: bool,
) -> Result<nunki_book::BookOptions> {
    let (members, warnings) = config::members_of(repo, &cfg.workspace.members);
    for w in warnings {
        eprintln!("warning: {w}");
    }
    Ok(nunki_book::BookOptions {
        theme: theme.map(Theme::from).unwrap_or(cfg.style.theme),
        accent: parse_accent(accent, cfg)?,
        include_tests: include_tests || cfg.analysis.include_tests,
        max_density: cfg.validation.max_density,
        allow_partial,
        members,
    })
}

fn generate(
    path: &Path,
    out: Option<PathBuf>,
    accent: Option<String>,
    theme: Option<ThemeArg>,
    include_tests: bool,
    allow_partial: bool,
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
    let opts = book_options(path, &cfg, accent.as_deref(), theme, include_tests, allow_partial)?;
    let report = nunki_book::generate(path, &out, &opts)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report::book(&report));
    }
    Ok(ExitCode::SUCCESS)
}

/// Write a diagram in another tool's import format.
///
/// One way on purpose: reading a drawing back would mean deciding whether the
/// file or the source is right about the architecture, and the source is. What
/// travels is the evidence — every shape carries its `file:line` and, when the
/// repository has a forge remote, a link to it — so an exported diagram pasted
/// into a review can still be checked against the code.
/// Writes the specification in another toolchain's format.
///
/// The book is generated into a temporary directory rather than the
/// repository: this command answers "what does the code specify", and should
/// not leave a book behind as a side effect of asking.
/// Specification files in the layouts the surveyed toolkits use. Only these
/// three, and only where they actually sit: guessing that any `requirements.md`
/// anywhere is a specification would read a template or a sample and report
/// findings about neither the code nor the spec.
fn discover_specs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for (dir, file) in [(".kiro/specs", "requirements.md"), ("openspec/specs", "spec.md"), ("specs", "spec.md")] {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else { continue };
        let mut found: Vec<PathBuf> = entries.flatten().map(|e| e.path().join(file)).filter(|p| p.is_file()).collect();
        found.sort();
        out.extend(found);
    }
    out
}

/// Checks the code against a specification someone else wrote.
fn conform(path: &Path, spec_files: Vec<PathBuf>, json: bool, exit_code: bool) -> Result<ExitCode> {
    let files = if spec_files.is_empty() { discover_specs(path) } else { spec_files };
    if files.is_empty() {
        eprintln!(
            "no specification found under {}. Looked in .kiro/specs/*/requirements.md, \
             openspec/specs/*/spec.md and specs/*/spec.md; name one with --spec.",
            path.display()
        );
        return Ok(ExitCode::from(2));
    }

    let cfg = Config::discover(path)?;
    let scratch = tempfile::tempdir().context("cannot create a temporary directory for the build")?;
    let opts = book_options(path, &cfg, None, None, false, true)?;
    let planned = nunki_book::plan(path, scratch.path(), &opts)?;

    let mut declared = Vec::new();
    let mut undecidable = Vec::new();
    let mut sources = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).with_context(|| format!("reading {}", f.display()))?;
        let rel = f.strip_prefix(path).unwrap_or(f).to_string_lossy().replace('\\', "/");
        let found = nunki_book::conform::parse_spec(&rel, &text);
        undecidable.extend(nunki_book::conform::headings_without_endpoints(&text, &found));
        declared.extend(found);
        sources.push(rel);
    }

    let mut report = nunki_book::conform::compare(&declared, &undecidable, &planned.built.behaviour);
    report.sources = sources;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report::conform(&report));
    }
    Ok(if exit_code && report.has_gaps() { ExitCode::from(1) } else { ExitCode::SUCCESS })
}

fn spec(path: &Path, format: SpecFormat, out: Option<PathBuf>) -> Result<ExitCode> {
    let cfg = Config::discover(path)?;
    let scratch = tempfile::tempdir().context("cannot create a temporary directory for the build")?;
    let opts = book_options(path, &cfg, None, None, false, true)?;
    let planned = nunki_book::plan(path, scratch.path(), &opts)?;
    let built = &planned.built;

    let purpose = |unit: &str| -> Option<String> {
        built.report.containers.iter().find(|u| u.id == unit).and_then(|u| u.description.clone())
    };
    let files = match format {
        SpecFormat::Openspec => nunki_book::openspec::render(&built.behaviour, &purpose),
    };
    if files.is_empty() {
        eprintln!("nothing to write: no API operations were found in {}", path.display());
        return Ok(ExitCode::from(1));
    }

    let root = out.unwrap_or_else(|| path.to_path_buf());
    for (rel, text) in &files {
        let dest = root.join(rel);
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        std::fs::write(&dest, text).with_context(|| format!("writing {}", dest.display()))?;
        println!("{}", dest.display());
    }
    println!(
        "{} capabilit{} from {} requirement{}",
        files.len(),
        if files.len() == 1 { "y" } else { "ies" },
        built.behaviour.requirements.len(),
        if built.behaviour.requirements.len() == 1 { "" } else { "s" }
    );
    Ok(ExitCode::SUCCESS)
}

fn export(ir: &Path, format: ExportFormat, output: Option<PathBuf>, repo: Option<PathBuf>) -> Result<ExitCode> {
    let text = std::fs::read_to_string(ir).with_context(|| format!("reading {}", ir.display()))?;
    let diagram: nunki_ir::DiagramIR =
        serde_json::from_str(&text).with_context(|| format!("{} is not a DiagramIR", ir.display()))?;
    // The current directory is the wrong default: it is usually the repository
    // nunki is being run from, not the one the diagram describes, and a link
    // to the right path in the wrong repository is worse than no link.
    let start = repo.unwrap_or_else(|| ir.parent().unwrap_or(Path::new(".")).to_path_buf());
    let found = nunki_git::repo_context(&start);
    // From the repository root, not from wherever the IR sits: evidence paths are
    // relative to the scanned root, and a context rooted in `docs/architecture/
    // diagrams` would prepend that to every link.
    let root = found.git_root.clone().unwrap_or(start);
    let ctx = nunki_git::repo_context(&root);
    let commit = diagram.metadata.commit_hash.clone();
    // A commit the repository does not have means the evidence was pinned
    // somewhere else, so the line numbers would not be the ones being linked to.
    let known = commit.as_deref().is_some_and(|c| nunki_git::commit_exists(&ctx, c));
    let forge = ctx.remote_url.as_deref().and_then(nunki_git::web_base).is_some();
    if !(known && forge) {
        eprintln!(
            "note: no permalinks — {}. Shapes still carry file:line.",
            if !forge {
                format!("{} has no recognised forge remote", root.display())
            } else {
                format!(
                    "{} does not have commit {}",
                    root.display(),
                    commit.as_deref().map(nunki_git::short).unwrap_or("(none)")
                )
            }
        );
    }
    let link = |e: &nunki_ir::Evidence| -> Option<String> {
        (known && forge).then(|| ctx.permalink(&e.file_path, e.start_line, e.end_line, commit.as_deref()))
    };
    let body = match format {
        ExportFormat::Drawio => nunki_renderer::drawio::to_xml(&diagram, &link),
        ExportFormat::DrawioCsv => nunki_renderer::drawio::to_csv(&diagram, &link),
    };
    match output.as_deref().map(|p| p.to_string_lossy().into_owned()) {
        Some(ref o) if o == "-" => print!("{body}"),
        Some(_) | None => {
            let path = output.unwrap_or_else(|| {
                let stem = ir.file_stem().and_then(|s| s.to_str()).unwrap_or("diagram").trim_end_matches(".ir");
                ir.with_file_name(format!("{stem}.{}", format.extension()))
            });
            std::fs::write(&path, &body).with_context(|| format!("writing {}", path.display()))?;
            eprintln!("{} · {}", path.display(), format.hint());
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Scan two revisions in throwaway worktrees and report what the architecture
/// model disagrees about.
///
/// Worktrees rather than the directory in place: comparing revisions must not
/// touch the caller's working tree, and in CI the checkout is the thing being
/// tested.
#[allow(clippy::too_many_arguments)]
fn diff(
    path: &Path,
    base: &str,
    head: &str,
    include_tests: bool,
    json: bool,
    exit_code: bool,
    record: Option<String>,
    out: Option<PathBuf>,
) -> Result<ExitCode> {
    let ctx = nunki_git::repo_context(path);
    let git_root =
        ctx.git_root.clone().with_context(|| format!("{} is not inside a git repository", path.display()))?;
    let prefix = ctx.prefix.clone();
    let tmp = tempfile::tempdir().context("cannot create a temporary directory for the comparison")?;

    // No workspace members: `diff` compares two revisions of *this* repository.
    // A member has one working copy, at whatever commit it is on, so it would
    // contribute the same thing to both sides and could only add noise — or,
    // worse, report a change in a member as a change here.
    let opts = nunki_analyzer::ScanOptions { behavior: true, include_tests, ..Default::default() };
    let scan_at = |rev: &str| -> Result<nunki_analyzer::ScanReport> {
        let wt = nunki_git::worktree_at(&git_root, rev, tmp.path())
            .with_context(|| format!("cannot read revision `{rev}`"))?;
        let root = if prefix.is_empty() { wt.path().to_path_buf() } else { wt.path().join(&prefix) };
        let report = nunki_analyzer::scan(&root, &opts).with_context(|| format!("cannot scan `{rev}`"))?;
        Ok(report)
    };
    let before = scan_at(base)?;
    let after = scan_at(head)?;

    let mut d = nunki_analyzer::diff::diff(&before, &after);
    d.base = base.to_string();
    d.head = head.to_string();
    if json {
        println!("{}", serde_json::to_string_pretty(&d)?);
    } else {
        print!("{}", nunki_analyzer::diff::markdown(&d));
    }

    if let Some(release) = record {
        let cfg = Config::discover(path)?;
        let book = match out {
            Some(explicit) => explicit,
            None => {
                crate::config::confined(&cfg.output.dir)?;
                path.join(&cfg.output.dir)
            }
        };
        let (mut history, warning) = nunki_book::history::History::load(&book);
        if let Some(w) = warning {
            eprintln!("warning: {w}");
        }
        let replaced = history.record(nunki_book::history::Entry {
            release: release.clone(),
            date: nunki_git::commit_date(&ctx, head),
            base: base.to_string(),
            diff: d.clone(),
        });
        let dest = book.join(nunki_book::history::HISTORY);
        std::fs::create_dir_all(&book).with_context(|| format!("creating {}", book.display()))?;
        std::fs::write(&dest, history.to_json_pretty()).with_context(|| format!("writing {}", dest.display()))?;
        println!("{} {release} in {}", if replaced { "replaced" } else { "recorded" }, dest.display());
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
    // A book generated with --allow-partial must still be checkable.
    let opts = book_options(path, &cfg, None, None, false, true)?;
    let report = nunki_book::check(path, &out, &opts)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report::check(&report));
    }
    Ok(if report.ok { ExitCode::SUCCESS } else { ExitCode::from(1) })
}
