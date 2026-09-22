//! Compare a specification somebody wrote against what the code is measured
//! to do.
//!
//! Spec-driven toolkits write a specification and build from it, and none of
//! them can tell you afterwards whether the code still matches, because none
//! of them keeps a link from a requirement to the code implementing it. Both
//! Spec Kit's `converge` and OpenSpec's `verify` do check — by searching the
//! codebase for each requirement, every run. This starts from a model where
//! every claim already carries a file, a line and the commit it was verified
//! against.
//!
//! The hard part is not parsing. It is refusing to guess. A declared
//! requirement that cannot be matched with confidence is reported as
//! unmatched, with what was looked for — never quietly paired with the nearest
//! operation, which would turn a documentation tool into a source of confident
//! wrong answers.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::behaviour::Behaviour;

/// What a specification says, as far as it can be read from prose.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Declared {
    /// File it came from, relative to the repository.
    pub source: String,
    pub line: u32,
    /// The requirement heading this sits under, which is how a person finds it.
    pub requirement: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Statuses the text says the system returns.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub statuses: Vec<u16>,
}

/// The vocabulary Spec Kit and OpenSpec both converged on, plus the one they
/// do not have and a real specification needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    /// Declared and implemented, and what the code returns agrees.
    Matched,
    /// Declared and implemented, but something the text states is not what the
    /// code was measured to do.
    Partial,
    /// Declared, and no implementation found.
    Missing,
    /// Implemented, and declared nowhere.
    Unrequested,
    /// A requirement naming no endpoint. Real specifications are full of them
    /// — protocols, deployment, performance — and calling them `missing`
    /// because nothing matched would be the loudest possible false positive.
    NotCheckable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub verdict: Verdict,
    /// Why, in a sentence a person can act on.
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declared: Option<Declared>,
    /// The requirement identifier on our side, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requirement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<nunki_analyzer::scan::EvidenceRef>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Specification files read.
    pub sources: Vec<String>,
    pub findings: Vec<Finding>,
    pub counts: BTreeMap<String, usize>,
}

impl Report {
    /// Anything the caller should act on. `not-checkable` and `unrequested`
    /// are reported and do not fail: the first is not a defect, and the second
    /// is a judgement about scope that belongs to a person.
    pub fn has_gaps(&self) -> bool {
        self.findings.iter().any(|f| matches!(f.verdict, Verdict::Missing | Verdict::Partial))
    }
}

const METHODS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];

/// `/v1/jobs/{id}` and `/v1/jobs/{job_id}` are the same route. A specification
/// names a parameter for the reader; the code names it for the compiler, and
/// comparing the two literally reports one correct endpoint as both missing
/// and unrequested.
fn normalise(path: &str) -> String {
    let mut out = String::new();
    for seg in path.trim_end_matches('/').split('/') {
        if seg.is_empty() {
            continue;
        }
        out.push('/');
        let is_param = (seg.starts_with('{') && seg.ends_with('}')) || seg.starts_with(':');
        out.push_str(if is_param { "{}" } else { seg });
    }
    if out.is_empty() {
        "/".into()
    } else {
        out
    }
}

/// Every `path`-looking token on a line, with the nearest HTTP method named
/// before it. Specifications write both `GET /x` and "a POST request to `/x`",
/// so the method is looked for anywhere earlier in the sentence rather than
/// immediately adjacent.
fn endpoints_in(line: &str) -> Vec<(Option<String>, String)> {
    let mut out = Vec::new();
    for (at, token) in path_tokens(line) {
        let before = &line[..at].to_uppercase();
        let method = METHODS
            .iter()
            .filter_map(|m| before.rfind(*m).map(|i| (i, *m)))
            // The nearest one before the path, so two endpoints on one line do
            // not both take the first method mentioned.
            .max_by_key(|(i, _)| *i)
            .map(|(_, m)| m.to_string());
        out.push((method, token));
    }
    out
}

/// Backticked paths first, since that is how specifications write them; then
/// bare ones, so a spec that does not use code spans still works.
fn path_tokens(line: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            if let Some(end) = line[i + 1..].find('`').map(|e| i + 1 + e) {
                let inner = line[i + 1..end].trim();
                // `POST /v1/jobs` — method and path inside one span.
                let (m, p) = match inner.split_once(' ') {
                    Some((a, b)) if METHODS.contains(&a.to_uppercase().as_str()) => (Some(a), b.trim()),
                    _ => (None, inner),
                };
                if p.starts_with('/') {
                    let at = if m.is_some() { i } else { i + 1 };
                    out.push((at, p.to_string()));
                }
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }
    if out.is_empty() {
        // No code spans: take bare tokens that look like routes.
        for (at, word) in line.match_indices(' ').map(|(i, _)| i + 1).chain(std::iter::once(0)).filter_map(|i| {
            let w = line[i..].split_whitespace().next()?;
            Some((i, w))
        }) {
            let w = word.trim_end_matches([',', '.', ';', ':', ')']);
            if w.starts_with('/') && w.len() > 1 && !w.contains("//") {
                out.push((at, w.to_string()));
            }
        }
    }
    out
}

fn statuses_in(line: &str) -> Vec<u16> {
    let mut out = Vec::new();
    let b = line.as_bytes();
    for (i, _) in line.match_indices(|c: char| c.is_ascii_digit()) {
        if i > 0 && (b[i - 1].is_ascii_digit() || b[i - 1] == b'.') {
            continue;
        }
        let digits: String = line[i..].chars().take_while(char::is_ascii_digit).collect();
        if digits.len() == 3 {
            if let Ok(code) = digits.parse::<u16>() {
                // Only in a context that reads as a status, so a port number
                // or a count does not become an assertion about behaviour.
                let before = line[..i].to_uppercase();
                if (100..600).contains(&code)
                    && (before.contains("HTTP")
                        || before.contains("STATUS")
                        || before.contains("RETURN")
                        || before.contains("RESPOND")
                        || before.contains("CODE"))
                {
                    out.push(code);
                }
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Requirements read out of one specification file. Handles the three layouts
/// the surveyed toolkits use, which differ only in how a requirement is
/// headed: Kiro `### Requirement N` with EARS criteria, OpenSpec
/// `### Requirement: <Name>` with scenarios, Spec Kit `- **FR-001**: …`.
pub fn parse_spec(source: &str, text: &str) -> Vec<Declared> {
    let mut out = Vec::new();
    let mut heading = String::from("(no requirement heading)");
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("### ") {
            heading = rest.trim().to_string();
            continue;
        }
        // Spec Kit states the requirement on the bullet itself.
        if let Some(rest) = line.strip_prefix("- **FR-").and_then(|r| r.split_once("**")) {
            heading = format!("FR-{}", rest.0);
        }
        let endpoints = endpoints_in(raw);
        if endpoints.is_empty() {
            continue;
        }
        let statuses = statuses_in(raw);
        for (method, path) in endpoints {
            out.push(Declared {
                source: source.to_string(),
                line: i as u32 + 1,
                requirement: heading.clone(),
                method,
                path: Some(path),
                statuses: statuses.clone(),
            });
        }
    }
    out
}

/// Requirement headings that named no endpoint anywhere — so a spec section
/// about a protocol or a deployment is reported as undecidable rather than
/// missing.
pub fn headings_without_endpoints(text: &str, declared: &[Declared]) -> Vec<String> {
    let named: BTreeSet<&str> = declared.iter().map(|d| d.requirement.as_str()).collect();
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("### ").map(str::trim))
        .filter(|h| !named.contains(h))
        .map(str::to_string)
        .collect()
}

/// Compares what was declared against what was measured.
pub fn compare(declared: &[Declared], undecidable: &[String], model: &Behaviour) -> Report {
    let mut findings: Vec<Finding> = Vec::new();
    // (method, normalised path) → our requirement.
    let implemented: BTreeMap<(String, String), &crate::behaviour::Requirement> =
        model.requirements.iter().map(|r| ((r.method.to_uppercase(), normalise(&r.path)), r)).collect();
    let mut hit: BTreeSet<(String, String)> = BTreeSet::new();

    for d in declared {
        let (Some(method), Some(path)) = (&d.method, &d.path) else {
            findings.push(Finding {
                verdict: Verdict::NotCheckable,
                detail: match &d.path {
                    Some(p) => format!("names `{p}` with no method, so it cannot be matched to an operation"),
                    None => "names neither a method nor a path".into(),
                },
                declared: Some(d.clone()),
                requirement: None,
                evidence: None,
            });
            continue;
        };
        let key = (method.to_uppercase(), normalise(path));
        match implemented.get(&key) {
            Some(r) => {
                hit.insert(key);
                // Only what the text actually states is checked. A spec that
                // mentions no status is not asserting the absence of one.
                let unmet: Vec<u16> = d
                    .statuses
                    .iter()
                    .copied()
                    .filter(|c| Some(*c) != r.success_status && !r.error_statuses.contains(c))
                    .collect();
                if unmet.is_empty() {
                    findings.push(Finding {
                        verdict: Verdict::Matched,
                        detail: format!("implemented as {} {}", r.method, r.path),
                        declared: Some(d.clone()),
                        requirement: Some(r.id.clone()),
                        evidence: Some(r.evidence.clone()),
                    });
                } else {
                    let observed: Vec<String> = r
                        .success_status
                        .into_iter()
                        .chain(r.error_statuses.iter().copied())
                        .map(|c| c.to_string())
                        .collect();
                    findings.push(Finding {
                        verdict: Verdict::Partial,
                        detail: format!(
                            "declares {}, and the handler was measured to return {}",
                            unmet.iter().map(u16::to_string).collect::<Vec<_>>().join(", "),
                            if observed.is_empty() { "no status".to_string() } else { observed.join(", ") }
                        ),
                        declared: Some(d.clone()),
                        requirement: Some(r.id.clone()),
                        evidence: Some(r.evidence.clone()),
                    });
                }
            }
            None => findings.push(Finding {
                verdict: Verdict::Missing,
                detail: format!("no operation answers {} {}", method, normalise(path)),
                declared: Some(d.clone()),
                requirement: None,
                evidence: None,
            }),
        }
    }

    for h in undecidable {
        findings.push(Finding {
            verdict: Verdict::NotCheckable,
            detail: format!("{h} names no endpoint, so the code cannot answer it either way"),
            declared: None,
            requirement: None,
            evidence: None,
        });
    }

    // Implemented and declared nowhere. This is the finding a spec-first tool
    // structurally cannot produce: it can only look within the scope its own
    // artifacts already describe.
    for ((method, path), r) in &implemented {
        if !hit.contains(&(method.clone(), path.clone())) {
            findings.push(Finding {
                verdict: Verdict::Unrequested,
                detail: format!("{} {} is implemented and declared nowhere", r.method, r.path),
                declared: None,
                requirement: Some(r.id.clone()),
                evidence: Some(r.evidence.clone()),
            });
        }
    }

    let mut counts = BTreeMap::new();
    for f in &findings {
        let k = serde_json::to_value(f.verdict).ok().and_then(|v| v.as_str().map(str::to_string));
        *counts.entry(k.unwrap_or_default()).or_insert(0) += 1;
    }
    Report { sources: Vec::new(), findings, counts }
}
