//! One facade over analyzer → validator → renderer, shared by the MCP server
//! and the CLI so both frontends behave identically.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use autodoc_analyzer::{Depth, DraftOptions, ScanOptions, ScanReport};
use autodoc_git::{EvidenceQuery, EvidenceReport, EvidenceState};
use autodoc_ir::{DiagramIR, Theme};
use autodoc_renderer::{Accent, EvidenceView, RenderOptions};
use autodoc_validator::{validate, validate_json, Diagnostic, Severity, ValidateOptions, ValidationReport};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    // `transparent` rather than `{0}`: with `#[from]` the inner error is also
    // the source, so `{e:#}` printed the same sentence twice.
    #[error(transparent)]
    Scan(#[from] autodoc_analyzer::ScanError),
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Book(#[from] autodoc_book::BookError),
    #[error("cannot write {path}: {source}")]
    Io { path: String, source: std::io::Error },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Html,
    Svg,
}

impl OutputFormat {
    pub fn extension(self) -> &'static str {
        match self {
            OutputFormat::Html => "html",
            OutputFormat::Svg => "svg",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResponse {
    pub report: ScanReport,
    /// Deterministic starting point; agents refine it rather than start blank.
    pub draft_ir: DiagramIR,
    pub draft_notes: Vec<String>,
    pub draft_validation: ValidationReport,
    pub next_step: String,
}

pub fn scan_repository(
    repo: &Path,
    depth: Depth,
    focus: Option<String>,
    include_tests: bool,
    theme: Theme,
) -> Result<ScanResponse, EngineError> {
    let report = autodoc_analyzer::scan(repo, &ScanOptions { depth, focus, include_tests, ..Default::default() })?;
    let mut draft = autodoc_analyzer::draft_ir(&report, &DraftOptions { theme, generated_at: None });
    let opts = ValidateOptions { repo_root: Some(PathBuf::from(&report.repo.root)), ..Default::default() };
    // The drafter heals its own IR the way an agent would: evidence that
    // doesn't verify is dropped rather than shipped.
    let (draft_validation, healed) = autodoc_validator::heal_evidence(&mut draft.ir, &opts);
    draft.notes.extend(healed);
    Ok(ScanResponse {
        report,
        draft_ir: draft.ir,
        draft_notes: draft.notes,
        draft_validation,
        next_step: "Refine `draftIr` (labels, grouping, focal point, primary path) using `evidenceMap` for file+line pins, then call autodoc_compile_diagram. Keep density ≤ 0.40 and at most 2 focal nodes.".into(),
    })
}

#[derive(Debug, Clone)]
pub struct CompileRequest {
    pub ir: IrInput,
    pub output_path: PathBuf,
    pub format: OutputFormat,
    /// Repository for evidence checks; defaults to `metadata.targetRepo` when it exists on disk.
    pub repo_path: Option<PathBuf>,
    pub accent: Accent,
    pub strict: bool,
    pub verify_evidence: bool,
    pub max_density: f64,
}

#[derive(Debug, Clone)]
pub enum IrInput {
    Json(String),
    Value(Value),
    Typed(Box<DiagramIR>),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum CompileOutcome {
    #[serde(rename = "compiled")]
    Compiled(Compiled),
    #[serde(rename = "rejected")]
    Rejected { validation: ValidationReport },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Compiled {
    pub output_path: String,
    pub file_url: String,
    pub format: OutputFormat,
    pub bytes: usize,
    pub width: f64,
    pub height: f64,
    pub density: f64,
    pub nodes: usize,
    pub edges: usize,
    pub warnings: Vec<Diagnostic>,
    pub evidence: EvidenceSummary,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceSummary {
    pub pinned: usize,
    pub verified: usize,
    pub stale: usize,
    pub unverified: usize,
    pub broken: usize,
}

fn resolve_repo(explicit: Option<&Path>, ir: &DiagramIR) -> Option<PathBuf> {
    explicit.map(Path::to_path_buf).or_else(|| Some(PathBuf::from(&ir.metadata.target_repo)).filter(|p| p.is_dir()))
}

pub fn compile_diagram(req: &CompileRequest) -> Result<CompileOutcome, EngineError> {
    let ext = req.output_path.extension().and_then(|e| e.to_str()).map(str::to_lowercase);
    let ext_ok = match req.format {
        OutputFormat::Html => matches!(ext.as_deref(), Some("html" | "htm")),
        OutputFormat::Svg => ext.as_deref() == Some("svg"),
    };
    if !ext_ok {
        return Err(EngineError::Invalid(format!(
            "outputPath `{}` must end in .{} for format `{}`",
            req.output_path.display(),
            req.format.extension(),
            req.format.extension()
        )));
    }
    let base_opts = |repo: Option<PathBuf>| ValidateOptions {
        repo_root: repo,
        verify_evidence: req.verify_evidence,
        max_density: req.max_density,
        strict: req.strict,
        evidence_cache: None,
    };
    let parsed = match &req.ir {
        IrInput::Typed(ir) => Ok((**ir).clone()),
        IrInput::Json(text) => autodoc_ir::parse_ir(text).map_err(|_| text.clone()),
        IrInput::Value(v) => autodoc_ir::parse_ir_value(v.clone()).map_err(|_| v.to_string()),
    };
    let ir = match parsed {
        Ok(ir) => ir,
        // Re-run through the validator so schema failures carry hints like any other diagnostic.
        Err(text) => return Ok(CompileOutcome::Rejected { validation: validate_json(&text, &base_opts(None)).1 }),
    };
    let repo = resolve_repo(req.repo_path.as_deref(), &ir);
    let validation = validate(&ir, &base_opts(repo));
    if !validation.valid {
        return Ok(CompileOutcome::Rejected { validation });
    }

    let repo = if req.verify_evidence { resolve_repo(req.repo_path.as_deref(), &ir) } else { None };
    let (evidence, summary) = evidence_views(&ir, repo.as_deref());
    let mut ir = ir;
    ir.metadata.visual_density_score = validation.density.as_ref().map(|d| d.score);
    let opts = RenderOptions { accent: req.accent.clone(), evidence, footer: None };
    let rendered = match req.format {
        OutputFormat::Html => autodoc_renderer::render_html(&ir, &opts),
        OutputFormat::Svg => autodoc_renderer::render_svg(&ir, &opts),
    };
    if let Some(parent) = req.output_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|source| EngineError::Io { path: parent.display().to_string(), source })?;
    }
    std::fs::write(&req.output_path, &rendered.content)
        .map_err(|source| EngineError::Io { path: req.output_path.display().to_string(), source })?;
    let abs = req.output_path.canonicalize().unwrap_or_else(|_| req.output_path.clone());
    Ok(CompileOutcome::Compiled(Compiled {
        output_path: abs.display().to_string(),
        file_url: format!("file://{}", abs.display()),
        format: req.format,
        bytes: rendered.content.len(),
        width: rendered.layout.width,
        height: rendered.layout.height,
        density: autodoc_ir::visual_density(ir.nodes.len(), ir.edges.len()),
        nodes: ir.nodes.len(),
        edges: ir.edges.len(),
        warnings: validation.diagnostics.into_iter().filter(|d| d.severity == Severity::Warning).collect(),
        evidence: summary,
    }))
}

const SNIPPET_MAX_LINES: u32 = 60;

fn state_name(s: EvidenceState) -> String {
    serde_json::to_value(s).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

pub fn evidence_views(ir: &DiagramIR, repo: Option<&Path>) -> (BTreeMap<String, EvidenceView>, EvidenceSummary) {
    let mut out = BTreeMap::new();
    let mut summary = EvidenceSummary::default();
    let ctx = repo.map(autodoc_git::repo_context);
    // Edge evidence (sequence messages, relationships) is keyed `edge:<id>`.
    let pinned = ir
        .nodes
        .iter()
        .filter_map(|n| n.evidence.as_ref().map(|ev| (n.id.clone(), ev)))
        .chain(ir.edges.iter().filter_map(|e| e.evidence.as_ref().map(|ev| (format!("edge:{}", e.id), ev))));
    for (key, ev) in pinned {
        summary.pinned += 1;
        let anchor = if ev.start_line == ev.end_line {
            format!("#L{}", ev.start_line)
        } else {
            format!("#L{}-L{}", ev.start_line, ev.end_line)
        };
        let mut view = EvidenceView {
            state: "unverified".into(),
            detail: "no repository available at compile time".into(),
            snippet: None,
            snippet_start: ev.start_line,
            permalink: None,
            git_ref: format!("{}{}", ev.file_path, anchor),
        };
        match &ctx {
            Some(ctx) => {
                let q = EvidenceQuery {
                    file_path: ev.file_path.clone(),
                    line: ev.start_line,
                    end_line: Some(ev.end_line),
                    symbol_name: ev.symbol_name.clone(),
                };
                let r = autodoc_git::verify_evidence(ctx, &q, ir.metadata.commit_hash.as_deref());
                match r.state {
                    EvidenceState::Verified => summary.verified += 1,
                    EvidenceState::Stale | EvidenceState::Untracked => summary.stale += 1,
                    _ => summary.broken += 1,
                }
                view.state = state_name(r.state);
                view.detail = r.detail.clone();
                if let Some(link) = &r.permalink {
                    view.git_ref = link.clone();
                    if link.starts_with("https://") {
                        view.permalink = Some(link.clone());
                    }
                }
                view.snippet =
                    autodoc_git::read_snippet(&ctx.root, &ev.file_path, ev.start_line, ev.end_line, SNIPPET_MAX_LINES);
            }
            None => summary.unverified += 1,
        }
        out.insert(key, view);
    }
    (out, summary)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResponse {
    pub repo_root: String,
    pub is_git: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub all_verified: bool,
    pub counts: BTreeMap<String, usize>,
    pub results: Vec<EvidenceReport>,
}

pub fn verify_evidence(
    repo: &Path,
    items: &[EvidenceQuery],
    pinned_commit: Option<&str>,
) -> Result<VerifyResponse, EngineError> {
    if !repo.is_dir() {
        return Err(EngineError::Invalid(format!("repoPath `{}` is not a directory", repo.display())));
    }
    let ctx = autodoc_git::repo_context(repo);
    let results: Vec<EvidenceReport> =
        items.iter().map(|q| autodoc_git::verify_evidence(&ctx, q, pinned_commit)).collect();
    let mut counts = BTreeMap::new();
    for r in &results {
        *counts.entry(state_name(r.state)).or_insert(0) += 1;
    }
    Ok(VerifyResponse {
        repo_root: ctx.root.display().to_string(),
        is_git: ctx.is_git(),
        head_commit: ctx.head_commit.clone(),
        branch: ctx.branch.clone(),
        all_verified: results.iter().all(EvidenceReport::is_ok),
        counts,
        results,
    })
}

/// Parses `path:12` or `path:12-40` (the CLI's evidence syntax).
pub fn parse_evidence_arg(arg: &str) -> Result<EvidenceQuery, String> {
    let (path, range) =
        arg.rsplit_once(':').ok_or_else(|| format!("`{arg}` must look like path:LINE or path:START-END"))?;
    let (start, end) = match range.split_once('-') {
        Some((a, b)) => (a.parse::<u32>(), b.parse::<u32>().map(Some)),
        None => (range.parse::<u32>(), Ok(None)),
    };
    match (start, end) {
        (Ok(line), Ok(end_line)) => {
            Ok(EvidenceQuery { file_path: path.to_string(), line, end_line, symbol_name: None })
        }
        _ => Err(format!("`{arg}`: line numbers must be positive integers")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_args() {
        let q = parse_evidence_arg("src/a.rs:10-20").unwrap();
        assert_eq!((q.line, q.end_line), (10, Some(20)));
        assert_eq!(parse_evidence_arg("C:/x/a.rs:3").unwrap().file_path, "C:/x/a.rs");
        assert!(parse_evidence_arg("src/a.rs").is_err());
        assert!(parse_evidence_arg("src/a.rs:x").is_err());
    }
}
