//! Human-readable terminal output.

use std::fmt::Write;

use nunki_mcp::engine::{CompileOutcome, ScanResponse, VerifyResponse};
use nunki_validator::{Severity, ValidationReport};

pub fn scan_summary(res: &ScanResponse) -> String {
    let r = &res.report;
    let mut s = String::new();
    let git = match (&r.repo.branch, &r.repo.commit_hash) {
        (Some(b), Some(c)) => format!("{b} @ {}", &c[..c.len().min(8)]),
        (None, Some(c)) => c[..c.len().min(8)].to_string(),
        _ => "not a git repository".into(),
    };
    let dirty =
        if r.repo.dirty_files.is_empty() { String::new() } else { format!(", {} dirty", r.repo.dirty_files.len()) };
    let _ = writeln!(s, "{}  ({git}{dirty})", r.system.name);
    let langs: Vec<String> = r.stats.languages.iter().map(|(l, st)| format!("{l} {}", st.files)).collect();
    let _ = writeln!(
        s,
        "{} files · {} lines · {} symbols · {} · {} ms",
        r.stats.files,
        r.stats.lines,
        r.stats.symbols,
        langs.join(", "),
        r.stats.duration_ms
    );

    let _ = writeln!(s, "\nContainers");
    for u in &r.containers {
        let ev = r
            .evidence_map
            .get(&u.id)
            .map(|e| format!("{}:{}-{}", e.file_path, e.start_line, e.end_line))
            .unwrap_or_default();
        let _ = writeln!(s, "  {:<18} {:<18} {:<24} {}", u.id, u.kind.role(), u.tech_stack, ev);
    }
    if !r.infrastructure.is_empty() {
        let _ = writeln!(s, "\nInfrastructure");
        for i in &r.infrastructure {
            let _ = writeln!(s, "  {:<18} {:<18} used by {}", i.id, i.role, i.used_by.join(", "));
        }
    }
    if !r.relationships.is_empty() {
        let _ = writeln!(s, "\nRelationships");
        for rel in &r.relationships {
            let sources: Vec<String> = rel.sources.iter().map(|x| format!("{x:?}").to_lowercase()).collect();
            let _ = writeln!(
                s,
                "  {:<36} {:<6} {:<26} [{}]",
                format!("{} → {}", rel.source, rel.target),
                format!("{:?}", rel.edge_type).to_lowercase(),
                rel.label,
                sources.join(", ")
            );
        }
    }
    if let Some(c) = &r.components {
        let _ = writeln!(s, "\nComponents of {}", c.unit);
        for m in &c.modules {
            let _ = writeln!(s, "  {:<32} {} file(s){}", m.id, m.files.len(), if m.is_entry { " · entry" } else { "" });
        }
    }
    let ir = &res.draft_ir;
    let v = &res.draft_validation;
    let _ = writeln!(
        s,
        "\nDraft IR: {} nodes · {} edges · density {:.2} · {}",
        ir.nodes.len(),
        ir.edges.len(),
        ir.metadata.visual_density_score.unwrap_or_default(),
        if v.valid { "valid".to_string() } else { format!("{} error(s)", v.error_count) }
    );
    for n in res.draft_notes.iter().chain(r.notes.iter()) {
        let _ = writeln!(s, "  note: {n}");
    }
    s
}

pub fn validation(report: &ValidationReport) -> String {
    let mut s = String::new();
    if let Some(d) = &report.density {
        let _ = writeln!(
            s,
            "density {:.3} (max {:.2}, {} nodes + {} edges / {} grid units)",
            d.score, d.max, d.nodes, d.edges, d.grid_units
        );
    }
    for d in &report.diagnostics {
        let tag = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let _ = writeln!(s, "{tag}[{}] {}: {}", d.code, d.path, d.message);
        for (i, sug) in d.suggestions.iter().enumerate() {
            let _ = writeln!(s, "    {} {sug}", if i == 0 { "→" } else { "·" });
        }
        if !d.patch.is_empty() {
            let _ = writeln!(s, "    patch: {}", serde_json::to_string(&d.patch).unwrap_or_default());
        }
    }
    let verified = report.evidence.iter().filter(|e| e.is_ok()).count();
    if !report.evidence.is_empty() {
        let _ = writeln!(s, "evidence: {verified}/{} verified", report.evidence.len());
    }
    let _ = writeln!(
        s,
        "{} — {} error(s), {} warning(s)",
        if report.valid { "valid" } else { "invalid" },
        report.error_count,
        report.warning_count
    );
    s
}

pub fn compile(outcome: &CompileOutcome) -> String {
    match outcome {
        CompileOutcome::Compiled(c) => {
            let mut s = String::new();
            for w in &c.warnings {
                let _ = writeln!(s, "warning[{}] {}: {}", w.code, w.path, w.message);
            }
            let _ = writeln!(
                s,
                "compiled {} · {} nodes · {} edges · density {:.2} · evidence {}/{} verified{}",
                c.output_path,
                c.nodes,
                c.edges,
                c.density,
                c.evidence.verified,
                c.evidence.pinned,
                if c.evidence.stale > 0 { format!(", {} stale", c.evidence.stale) } else { String::new() }
            );
            s
        }
        CompileOutcome::Rejected { validation: v } => format!("rejected — nothing was written\n{}", validation(v)),
    }
}

pub fn verify(res: &VerifyResponse) -> String {
    let mut s = String::new();
    for r in &res.results {
        let state = serde_json::to_value(r.state).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
        let _ = writeln!(s, "{:<18} {}:{}-{}  {}", state, r.file_path, r.start_line, r.end_line, r.detail);
    }
    let _ = writeln!(
        s,
        "{} ({} at {})",
        if res.all_verified { "all evidence verified" } else { "evidence needs attention" },
        res.repo_root,
        res.head_commit.as_deref().map(|c| &c[..c.len().min(8)]).unwrap_or("working tree")
    );
    s
}

pub fn book(r: &nunki_book::WriteReport) -> String {
    let mut s = String::new();
    let e = &r.evidence;
    let _ = writeln!(
        s,
        "book: {} pages · {} figures · {}/{} citations verified{}",
        r.pages,
        r.figures,
        e.verified,
        e.total,
        if e.stale + e.broken > 0 { format!(" ({} stale, {} broken)", e.stale, e.broken) } else { String::new() }
    );
    let _ = writeln!(s, "  {} written, {} unchanged, {} removed", r.written.len(), r.unchanged, r.removed.len());
    for c in &r.curated {
        let _ = writeln!(s, "  kept hand-edited diagrams/{c}.ir.json");
    }
    for w in &r.warnings {
        let _ = writeln!(s, "  warning: {w}");
    }
    let _ = writeln!(s, "open {}", r.index);
    s
}

pub fn check(r: &nunki_book::CheckReport) -> String {
    let mut s = String::new();
    for f in &r.missing {
        let _ = writeln!(s, "missing   {f}");
    }
    for f in &r.changed {
        let _ = writeln!(s, "outdated  {f}");
    }
    for c in &r.stale_citations {
        let _ = writeln!(s, "citation  {c}");
    }
    for id in &r.orphaned_authored {
        let _ = writeln!(s, "authored  `{id}` in authored.json names no operation — renamed, or removed");
    }
    if !r.unpinned_authored.is_empty() {
        let _ = writeln!(
            s,
            "note: {} authored intent(s) have no evidence pin, so nothing checks them: {}",
            r.unpinned_authored.len(),
            r.unpinned_authored.join(", ")
        );
    }
    let _ = writeln!(
        s,
        "{} — {}/{} citations verified",
        if r.ok {
            "book is up to date"
        } else if !r.orphaned_authored.is_empty() {
            "authored prose describes operations that are gone"
        } else if r.up_to_date {
            "book is current but cites code that changed"
        } else {
            "book is out of date: run `nunki generate`"
        },
        r.evidence.verified,
        r.evidence.total
    );
    s
}

/// What the code does and does not answer in a specification.
///
/// Ordered by what a reader should act on: what is declared and absent, then
/// what disagrees, then what exists unasked, then what could not be decided at
/// all. The last group is printed rather than hidden, because a tool that
/// silently drops what it could not judge is the kind that gets turned off.
pub fn conform(r: &nunki_book::conform::Report) -> String {
    use nunki_book::conform::Verdict;
    let mut s = String::new();
    let _ = writeln!(s, "read {}", r.sources.join(", "));

    let group = |v: Verdict| r.findings.iter().filter(move |f| f.verdict == v);
    let mut lines = |label: &str, v: Verdict| {
        for f in group(v) {
            let where_ = f
                .declared
                .as_ref()
                .map(|d| format!("{}:{}", d.source, d.line))
                .or_else(|| f.evidence.as_ref().map(|e| format!("{}:{}", e.file_path, e.start_line)))
                .unwrap_or_default();
            let subject = f
                .declared
                .as_ref()
                .map(|d| d.requirement.clone())
                .or_else(|| f.requirement.clone())
                .unwrap_or_default();
            let subject = if subject.is_empty() { String::new() } else { format!("{subject} — ") };
            let _ = writeln!(s, "{label:<12} {subject}{}  {where_}", f.detail);
        }
    };
    lines("missing", Verdict::Missing);
    lines("disagrees", Verdict::Partial);
    lines("unrequested", Verdict::Unrequested);
    lines("undecidable", Verdict::NotCheckable);

    let n = |k: &str| r.counts.get(k).copied().unwrap_or(0);
    let _ = writeln!(
        s,
        "\n{} matched · {} missing · {} disagreeing · {} unrequested · {} not checkable from code",
        n("matched"),
        n("missing"),
        n("partial"),
        n("unrequested"),
        n("not-checkable")
    );
    s
}
