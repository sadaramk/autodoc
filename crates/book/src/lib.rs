//! Architecture book: one deterministic, evidence-verified documentation set
//! per repository.
//!
//! ```text
//! docs/architecture/
//!   index.html            the book (self-contained, offline)
//!   manifest.json         commit, pages, figures, evidence health, file hashes
//!   README.md, pages/*.md Markdown mirror for GitHub / Obsidian
//!   llms.txt, llms-full.txt
//!   diagrams/<id>.ir.json + .svg
//! ```

pub mod authored;
pub mod behaviour;
pub mod build;
pub mod html;
pub mod markdown;
pub mod model;
pub mod openspec;

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

pub use build::{BookOptions, Built};
pub use model::Book;

#[derive(Debug, thiserror::Error)]
pub enum BookError {
    // `transparent` rather than `{0}`: with `#[from]` the inner error is also the
    // source, so anyhow's `{e:#}` printed the same sentence twice.
    #[error(transparent)]
    Scan(#[from] nunki_analyzer::ScanError),
    #[error("cannot write {path}: {source}")]
    Io { path: String, source: std::io::Error },
    #[error("refusing to write `{path}`: it would leave the output directory")]
    UnsafePath { path: String },
}

pub const MANIFEST: &str = "manifest.json";

/// Resolve a path recorded in a manifest against the book directory, or `None`
/// when it would reach outside it.
///
/// The manifest is read back from the output directory, and a book may be
/// generated for a repository the user does not control, so its file list is
/// untrusted input. `Path::join` follows `..` and an absolute path replaces the
/// base outright, which on the pruning pass would delete a file anywhere the
/// process can write. Accept only plain, relative components, then confirm the
/// resolved parent really is inside the book — a symlinked directory within it
/// could otherwise redirect an innocent-looking path.
fn book_path(out_dir: &Path, rel: &str) -> Option<PathBuf> {
    let path = relative_in(out_dir, rel)?;
    let root = out_dir.canonicalize().ok()?;
    // A symlinked directory inside the book could redirect an otherwise plain
    // path, so resolve before reading or deleting anything it names.
    let parent = path.parent()?.canonicalize().ok()?;
    parent.starts_with(&root).then_some(path)
}

/// `out_dir` joined with `rel`, accepting only plain relative components.
///
/// Used where the file need not exist yet, so the parent cannot be resolved.
fn relative_in(out_dir: &Path, rel: &str) -> Option<PathBuf> {
    let mut safe = PathBuf::new();
    for c in Path::new(rel).components() {
        match c {
            Component::Normal(part) => safe.push(part),
            _ => return None,
        }
    }
    (!safe.as_os_str().is_empty()).then(|| out_dir.join(safe))
}

/// Files of a book, keyed by path relative to the book directory.
pub struct Planned {
    pub built: Built,
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteReport {
    pub out_dir: String,
    pub index: String,
    pub pages: usize,
    pub figures: usize,
    pub written: Vec<String>,
    pub unchanged: usize,
    pub removed: Vec<String>,
    pub curated: Vec<String>,
    pub evidence: model::EvidenceHealth,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckReport {
    pub up_to_date: bool,
    /// Files whose regenerated content differs from what's on disk.
    pub changed: Vec<String>,
    pub missing: Vec<String>,
    pub evidence: model::EvidenceHealth,
    pub stale_citations: Vec<String>,
    /// Authored intents naming an operation that no longer exists. Failing,
    /// because the prose has silently stopped being published: the lookup
    /// misses and the book simply omits it.
    pub orphaned_authored: Vec<String>,
    /// Authored intents with prose and no pin. Reported, never failing — the
    /// file belongs to the human, and a book written before pinning existed
    /// must not start failing CI on upgrade.
    pub unpinned_authored: Vec<String>,
    pub ok: bool,
}

/// FNV-1a: stable across platforms and runs, no dependency.
pub fn content_hash(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn previous_manifest(out_dir: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(out_dir.join(MANIFEST)).ok()?).ok()
}

/// Diagram IR files edited by hand since the last run (hash differs from the
/// one the previous run recorded).
pub fn curated_diagrams(out_dir: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(manifest) = previous_manifest(out_dir) else { return out };
    let Some(files) = manifest.get("files").and_then(Value::as_object) else { return out };
    for (path, recorded) in files {
        let Some(id) = path.strip_prefix("diagrams/").and_then(|p| p.strip_suffix(".ir.json")) else { continue };
        let Some(full) = book_path(out_dir, path) else { continue };
        let Ok(text) = std::fs::read_to_string(full) else { continue };
        if Some(content_hash(&text).as_str()) != recorded.as_str() {
            out.insert(id.to_string(), text);
        }
    }
    out
}

pub fn plan(repo: &Path, out_dir: &Path, opts: &BookOptions) -> Result<Planned, BookError> {
    let curated = curated_diagrams(out_dir);
    let built = build::build(repo, out_dir, opts, &curated)?;
    let mut files = BTreeMap::new();
    for d in &built.diagrams {
        let ir_text = if d.curated {
            curated.get(&d.id).cloned().unwrap_or_else(|| d.ir.to_json_pretty() + "\n")
        } else {
            d.ir.to_json_pretty() + "\n"
        };
        files.insert(format!("diagrams/{}.ir.json", d.id), ir_text);
        files.insert(format!("diagrams/{}.svg", d.id), d.standalone_svg.clone());
    }
    let repo_rel = relative_repo_root(out_dir, Path::new(&built.report.repo.root));
    for (path, text) in markdown::render(&built.book, repo_rel.as_deref()) {
        files.insert(path, text);
    }
    files.insert("index.html".into(), html::render(&built.book, &opts.accent));
    // A generated file like any other: `check` compares it, so the model
    // cannot drift from the prose built beside it.
    files.insert("behaviour.json".into(), built.behaviour.to_json_pretty());
    Ok(Planned { built, files })
}

/// Generates the book and writes only what changed.
pub fn generate(repo: &Path, out_dir: &Path, opts: &BookOptions) -> Result<WriteReport, BookError> {
    let previous = previous_manifest(out_dir);
    let planned = plan(repo, out_dir, opts)?;
    let io = |path: &Path, source| BookError::Io { path: path.display().to_string(), source };
    let mut written = Vec::new();
    let mut unchanged = 0;
    for (rel, content) in &planned.files {
        let path = match relative_in(out_dir, rel) {
            Some(p) => p,
            None => return Err(BookError::UnsafePath { path: rel.clone() }),
        };
        if std::fs::read_to_string(&path).is_ok_and(|old| &old == content) {
            unchanged += 1;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
        }
        std::fs::write(&path, content).map_err(|e| io(&path, e))?;
        written.push(rel.clone());
    }
    // Files the previous run generated that this run doesn't (a removed service).
    let mut removed = Vec::new();
    if let Some(prev) = previous.as_ref().and_then(|m| m.get("files")).and_then(Value::as_object) {
        for old in prev.keys() {
            if planned.files.contains_key(old) || old == MANIFEST {
                continue;
            }
            let Some(path) = book_path(out_dir, old) else { continue };
            if path.is_file() && std::fs::remove_file(&path).is_ok() {
                removed.push(old.clone());
            }
        }
    }
    // Human-authored intent: created once, never regenerated or pruned.
    let authored = out_dir.join(authored::AUTHORED);
    if !authored.exists() {
        std::fs::create_dir_all(out_dir).map_err(|e| io(out_dir, e))?;
        std::fs::write(&authored, &planned.built.authored_template).map_err(|e| io(&authored, e))?;
        written.push(authored::AUTHORED.to_string());
    }
    let manifest = manifest(&planned);
    let manifest_path = out_dir.join(MANIFEST);
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).unwrap() + "\n")
        .map_err(|e| io(&manifest_path, e))?;
    let b = &planned.built;
    Ok(WriteReport {
        out_dir: out_dir.canonicalize().unwrap_or_else(|_| out_dir.to_path_buf()).display().to_string(),
        index: out_dir
            .join("index.html")
            .canonicalize()
            .unwrap_or_else(|_| out_dir.join("index.html"))
            .display()
            .to_string(),
        pages: b.book.pages.len(),
        figures: b.book.diagrams.len(),
        written,
        unchanged,
        removed,
        curated: b.diagrams.iter().filter(|d| d.curated).map(|d| d.id.clone()).collect(),
        evidence: b.book.meta.evidence.clone(),
        warnings: b.warnings.clone(),
    })
}

/// Regenerates in memory and compares with disk: for CI.
pub fn check(repo: &Path, out_dir: &Path, opts: &BookOptions) -> Result<CheckReport, BookError> {
    let planned = plan(repo, out_dir, opts)?;
    let mut changed = Vec::new();
    let mut missing = Vec::new();
    for (rel, content) in &planned.files {
        match std::fs::read_to_string(out_dir.join(rel)) {
            Ok(old) if &old == content => {}
            Ok(_) => changed.push(rel.clone()),
            Err(_) => missing.push(rel.clone()),
        }
    }
    let book = &planned.built.book;
    let stale_citations: Vec<String> = book
        .cites
        .values()
        .filter(|c| c.state != "verified")
        .map(|c| format!("{}:{}-{} ({}: {})", c.file, c.start, c.end, c.state, c.detail.clone().unwrap_or_default()))
        .collect();
    let up_to_date = changed.is_empty() && missing.is_empty();
    let orphaned_authored = planned.built.authored_orphans.clone();
    let unpinned_authored = planned.built.authored_unpinned.clone();
    Ok(CheckReport {
        up_to_date,
        ok: up_to_date && stale_citations.is_empty() && orphaned_authored.is_empty(),
        changed,
        missing,
        evidence: book.meta.evidence.clone(),
        stale_citations,
        orphaned_authored,
        unpinned_authored,
    })
}

fn manifest(planned: &Planned) -> Value {
    let b = &planned.built;
    json!({
        "generator": b.book.meta.generator,
        "generatedAt": nunki_ir::now_rfc3339(),
        "repository": b.book.meta.repo,
        "commit": b.book.meta.commit,
        "commitDate": b.book.meta.commit_date,
        "branch": b.book.meta.branch,
        "evidence": b.book.meta.evidence,
        "pages": b.book.pages.iter().map(|p| json!({"id": p.id, "title": p.title, "section": p.section, "markdown": p.md_path})).collect::<Vec<_>>(),
        "diagrams": b.diagrams.iter().map(|d| json!({
            "id": d.id,
            "title": d.ir.title,
            "ir": format!("diagrams/{}.ir.json", d.id),
            "svg": format!("diagrams/{}.svg", d.id),
            "nodes": d.ir.nodes.len(),
            "edges": d.ir.edges.len(),
            "density": nunki_ir::visual_density(d.ir.nodes.len(), d.ir.edges.len()),
            "curated": d.curated,
            "notes": d.notes,
        })).collect::<Vec<_>>(),
        "warnings": b.warnings,
        "files": planned.files.iter().map(|(k, v)| (k.clone(), Value::String(content_hash(v)))).collect::<serde_json::Map<_, _>>(),
    })
}

/// `../../` from the book directory back to the repository root, when the
/// book lives inside the repository (so Markdown can link to source files).
fn relative_repo_root(out_dir: &Path, repo_root: &Path) -> Option<String> {
    let out = absolute(out_dir);
    let root = repo_root.canonicalize().ok()?;
    let rel = out.strip_prefix(&root).ok()?;
    let depth = rel.components().count();
    Some("../".repeat(depth))
}

fn absolute(p: &Path) -> PathBuf {
    if let Ok(c) = p.canonicalize() {
        return c;
    }
    let base = std::env::current_dir().unwrap_or_default().join(p);
    // Parent may exist even when the book directory doesn't yet.
    match (base.parent().and_then(|x| x.canonicalize().ok()), base.file_name()) {
        (Some(parent), Some(name)) => parent.join(name),
        _ => base,
    }
}
