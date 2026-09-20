//! Deterministic validation for `DiagramIR`.
//!
//! The compiler refuses to render a cluttered or dishonest diagram. Instead it
//! returns diagnostics an agent can act on: a stable `code`, the JSON path of
//! the offending element, ranked suggestions and — whenever the fix is
//! mechanical — an RFC 6902 JSON Patch that applies it.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use nunki_git::{EvidenceQuery, EvidenceReport, EvidenceState, RepoContext};
use nunki_ir::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub mod codes {
    pub const SCHEMA: &str = "ERR_SCHEMA";
    pub const EMPTY_DIAGRAM: &str = "ERR_EMPTY_DIAGRAM";
    pub const EMPTY_FIELD: &str = "ERR_EMPTY_FIELD";
    pub const INVALID_ID: &str = "ERR_INVALID_ID";
    pub const DUPLICATE_ID: &str = "ERR_DUPLICATE_ID";
    pub const UNKNOWN_CONTAINER: &str = "ERR_UNKNOWN_CONTAINER";
    pub const MISSING_ENDPOINT: &str = "ERR_MISSING_ENDPOINT";
    pub const SELF_LOOP: &str = "ERR_SELF_LOOP";
    pub const ORPHAN_NODE: &str = "ERR_ORPHAN_NODE";
    pub const UNLABELED_CYCLE: &str = "ERR_UNLABELED_CYCLE";
    pub const HIGH_DENSITY: &str = "ERR_HIGH_DENSITY";
    pub const ACCENT_OVERUSE: &str = "ERR_ACCENT_OVERUSE";
    pub const EVIDENCE_RANGE: &str = "ERR_EVIDENCE_RANGE";
    pub const EVIDENCE_FILE_MISSING: &str = "ERR_EVIDENCE_FILE_MISSING";
    pub const EVIDENCE_OUT_OF_RANGE: &str = "ERR_EVIDENCE_OUT_OF_RANGE";
    pub const EVIDENCE_SYMBOL_MISMATCH: &str = "ERR_EVIDENCE_SYMBOL_MISMATCH";
    pub const EVIDENCE_OUTSIDE_REPO: &str = "ERR_EVIDENCE_OUTSIDE_REPO";
    pub const DUPLICATE_EDGE: &str = "WARN_DUPLICATE_EDGE";
    pub const EMPTY_CONTAINER: &str = "WARN_EMPTY_CONTAINER";
    pub const NO_FOCAL_POINT: &str = "WARN_NO_FOCAL_POINT";
    pub const PRIMARY_PATH_OVERUSE: &str = "WARN_PRIMARY_PATH_OVERUSE";
    pub const PRIMARY_PATH_BROKEN: &str = "WARN_PRIMARY_PATH_BROKEN";
    pub const DENSITY_MISMATCH: &str = "WARN_DENSITY_MISMATCH";
    pub const LABEL_TOO_LONG: &str = "WARN_LABEL_TOO_LONG";
    pub const EVIDENCE_STALE: &str = "WARN_EVIDENCE_STALE";
    pub const EVIDENCE_UNTRACKED: &str = "WARN_EVIDENCE_UNTRACKED";
    pub const EVIDENCE_UNVERIFIED: &str = "WARN_EVIDENCE_UNVERIFIED";
    pub const FOCAL_WITHOUT_EVIDENCE: &str = "WARN_FOCAL_WITHOUT_EVIDENCE";
    pub const COMMIT_MISMATCH: &str = "WARN_COMMIT_MISMATCH";
    pub const SEQUENCE_MISSING: &str = "ERR_SEQUENCE_MISSING";
    pub const SEQUENCE_DUPLICATE: &str = "ERR_SEQUENCE_DUPLICATE";
    pub const TOO_MANY_PARTICIPANTS: &str = "ERR_TOO_MANY_PARTICIPANTS";
    pub const TOO_MANY_MESSAGES: &str = "ERR_TOO_MANY_MESSAGES";
    pub const TERMINAL_HAS_TRANSITIONS: &str = "ERR_TERMINAL_HAS_TRANSITIONS";
    pub const NO_INITIAL_STATE: &str = "WARN_NO_INITIAL_STATE";
    pub const UNREACHABLE_STATE: &str = "WARN_UNREACHABLE_STATE";
    pub const UNLABELED_TRANSITION: &str = "WARN_UNLABELED_TRANSITION";
    pub const MISSING_CARDINALITY: &str = "WARN_MISSING_CARDINALITY";
    pub const ENTITY_WITHOUT_ATTRIBUTES: &str = "WARN_ENTITY_WITHOUT_ATTRIBUTES";
    pub const TOO_MANY_ATTRIBUTES: &str = "WARN_TOO_MANY_ATTRIBUTES";
    pub const FIELD_IGNORED: &str = "WARN_FIELD_IGNORED";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    /// JSON path of the offending element, e.g. `$.nodes[3].isKeyFocalPoint`.
    pub path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub element_ids: Vec<String>,
    /// Ranked, concrete remedies in plain language.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
    /// RFC 6902 operations that apply the first suggestion, when mechanical.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patch: Vec<Value>,
}

impl Diagnostic {
    fn new(code: &str, severity: Severity, path: impl Into<String>, message: impl Into<String>) -> Self {
        Diagnostic {
            code: code.into(),
            severity,
            message: message.into(),
            path: path.into(),
            element_ids: vec![],
            suggestions: vec![],
            patch: vec![],
        }
    }
    fn ids<I: IntoIterator<Item = S>, S: Into<String>>(mut self, ids: I) -> Self {
        self.element_ids = ids.into_iter().map(Into::into).collect();
        self
    }
    fn suggest(mut self, s: impl Into<String>) -> Self {
        self.suggestions.push(s.into());
        self
    }
    fn patch(mut self, ops: Vec<Value>) -> Self {
        self.patch = ops;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DensityReport {
    pub score: f64,
    pub max: f64,
    pub nodes: usize,
    pub edges: usize,
    pub grid_units: u32,
    /// Largest nodes + edges allowed at `max`.
    pub budget: usize,
    /// Elements to remove to get under budget (0 when compliant).
    pub excess: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    pub valid: bool,
    pub error_count: usize,
    pub warning_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density: Option<DensityReport>,
    pub diagnostics: Vec<Diagnostic>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<EvidenceReport>,
    /// One-paragraph instruction for an agent on how to self-correct.
    pub agent_hint: String,
}

#[derive(Debug, Clone)]
pub struct ValidateOptions {
    /// Repository to verify evidence against. `None` skips file checks.
    pub repo_root: Option<PathBuf>,
    pub verify_evidence: bool,
    /// Density ceiling; values above the editorial 0.40 are clamped down.
    pub max_density: f64,
    /// Treat warnings as errors.
    pub strict: bool,
    /// Git snapshots shared across validations in one operation (many figures
    /// of one book); `None` reads git fresh for this validation.
    pub evidence_cache: Option<std::sync::Arc<nunki_git::EvidenceCache>>,
}

impl Default for ValidateOptions {
    fn default() -> Self {
        ValidateOptions {
            repo_root: None,
            verify_evidence: true,
            max_density: MAX_VISUAL_DENSITY,
            strict: false,
            evidence_cache: None,
        }
    }
}

pub const MAX_FOCAL_POINTS: usize = 2;
pub const MAX_PRIMARY_EDGES: usize = 6;
const NODE_LABEL_MAX: usize = 32;
const EDGE_LABEL_MAX: usize = 32;
const SUBTITLE_MAX: usize = 64;
const TECH_MAX: usize = 40;
const TITLE_MAX: usize = 80;

/// Parses then validates. Returns the IR when it parsed, even if invalid.
pub fn validate_json(json: &str, opts: &ValidateOptions) -> (Option<DiagramIR>, ValidationReport) {
    match parse_ir(json) {
        Ok(ir) => {
            let report = validate(&ir, opts);
            (Some(ir), report)
        }
        Err(e) => {
            let mut d = Diagnostic::new(codes::SCHEMA, Severity::Error, e.path.clone(), e.message.clone());
            if let (Some(l), Some(c)) = (e.line, e.column) {
                d.message = format!("{} (line {l}, column {c})", e.message);
            }
            d.suggestions.push(schema_hint(&e.message));
            (None, finish(vec![d], None, vec![], opts.strict))
        }
    }
}

fn schema_hint(message: &str) -> String {
    if message.contains("unknown field") {
        "Remove or rename the field; DiagramIR rejects unknown keys (check camelCase spelling, e.g. `isKeyFocalPoint`)."
            .into()
    } else if message.contains("unknown variant") {
        "Use one of the enum values listed in the message.".into()
    } else if message.contains("missing field") {
        "Add the required field; see `nunki schema` for the full JSON Schema.".into()
    } else {
        "Fix the value at `path` to match the DiagramIR JSON Schema (`nunki schema`).".into()
    }
}

pub fn validate(ir: &DiagramIR, opts: &ValidateOptions) -> ValidationReport {
    let mut diags = Vec::new();
    let node_index: HashMap<&str, usize> = ir.nodes.iter().enumerate().map(|(i, n)| (n.id.as_str(), i)).collect();

    check_fields_and_ids(ir, &mut diags);
    check_containers(ir, &mut diags);
    let pending_endpoints = check_edges(ir, &node_index, &mut diags);
    check_orphans(ir, &pending_endpoints, &mut diags);
    if !matches!(ir.diagram_type, DiagramType::Sequence | DiagramType::EntityRelationship) {
        // Calls and replies loop by design; foreign keys may reference each other.
        check_cycles(ir, &node_index, &mut diags);
    }
    check_diagram_type(ir, &node_index, &mut diags);
    check_color_budget(ir, &mut diags);
    let density = check_density(ir, opts, &mut diags);
    check_legibility(ir, &mut diags);
    let evidence = check_evidence(ir, opts, &mut diags);

    finish(diags, Some(density), evidence, opts.strict)
}

fn finish(
    mut diags: Vec<Diagnostic>,
    density: Option<DensityReport>,
    evidence: Vec<EvidenceReport>,
    strict: bool,
) -> ValidationReport {
    if strict {
        for d in &mut diags {
            d.severity = Severity::Error;
        }
    }
    diags.sort_by(|a, b| {
        a.severity.cmp(&b.severity).then_with(|| a.path.cmp(&b.path)).then_with(|| a.code.cmp(&b.code))
    });
    let error_count = diags.iter().filter(|d| d.severity == Severity::Error).count();
    let warning_count = diags.len() - error_count;
    let agent_hint = if error_count == 0 {
        if warning_count == 0 {
            "IR is valid. Compile it.".to_string()
        } else {
            format!("IR is renderable. {warning_count} warning(s) flag editorial or evidence issues worth fixing before publishing.")
        }
    } else {
        let codes: BTreeSet<&str> =
            diags.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.as_str()).collect();
        format!(
            "Rendering refused: {error_count} error(s) [{}]. Fix only the elements named in `path`/`elementIds`; apply `patch` ops (RFC 6902) where present — indices refer to the IR you submitted, so apply removals highest index first — otherwise follow the first suggestion. Resubmit the full IR.",
            codes.into_iter().collect::<Vec<_>>().join(", ")
        )
    };
    ValidationReport {
        valid: error_count == 0,
        error_count,
        warning_count,
        density,
        diagnostics: diags,
        evidence,
        agent_hint,
    }
}

fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
        && id.len() <= 80
}

fn sanitize_id(id: &str) -> String {
    let mut s: String = id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':') { c } else { '-' })
        .collect();
    s = s.trim_matches(|c: char| !c.is_ascii_alphanumeric()).to_string();
    s.truncate(80);
    if s.is_empty() {
        "id".into()
    } else {
        s
    }
}

fn check_fields_and_ids(ir: &DiagramIR, diags: &mut Vec<Diagnostic>) {
    if ir.title.trim().is_empty() {
        diags.push(
            Diagnostic::new(codes::EMPTY_FIELD, Severity::Error, "$.title", "title is empty")
                .suggest("Give the diagram a short editorial title (≤ 80 chars)."),
        );
    }
    if ir.nodes.is_empty() {
        diags.push(
            Diagnostic::new(codes::EMPTY_DIAGRAM, Severity::Error, "$.nodes", "diagram has no nodes")
                .suggest("Run nunki_scan_repository and add nodes for the containers it reports."),
        );
    }
    let mut seen: HashMap<String, String> = HashMap::new();
    let mut check = |kind: &str, i: usize, id: &str, label: Option<&str>, diags: &mut Vec<Diagnostic>| {
        let path = format!("$.{kind}[{i}]");
        if !valid_id(id) {
            let fixed = sanitize_id(id);
            diags.push(
                Diagnostic::new(
                    codes::INVALID_ID,
                    Severity::Error,
                    format!("{path}.id"),
                    format!("id `{id}` must match [A-Za-z0-9][A-Za-z0-9._:-]* (≤ 80 chars)"),
                )
                .ids([id])
                .suggest(format!("Rename to `{fixed}` and update every reference to it.")),
            );
        }
        if let Some(prev) = seen.get(id) {
            diags.push(
                Diagnostic::new(
                    codes::DUPLICATE_ID,
                    Severity::Error,
                    format!("{path}.id"),
                    format!("id `{id}` is already used by {prev}"),
                )
                .ids([id])
                .suggest("Ids must be unique across containers, nodes and edges; rename one of them."),
            );
        } else {
            seen.insert(id.to_string(), path.clone());
        }
        if let Some(l) = label {
            if l.trim().is_empty() {
                diags.push(
                    Diagnostic::new(
                        codes::EMPTY_FIELD,
                        Severity::Error,
                        format!("{path}.label"),
                        format!("`{id}` has an empty label"),
                    )
                    .ids([id]),
                );
            }
        }
    };
    for (i, c) in ir.containers.iter().enumerate() {
        check("containers", i, &c.id, Some(&c.label), diags);
    }
    for (i, n) in ir.nodes.iter().enumerate() {
        check("nodes", i, &n.id, Some(&n.label), diags);
    }
    for (i, e) in ir.edges.iter().enumerate() {
        check("edges", i, &e.id, None, diags);
    }
}

fn check_containers(ir: &DiagramIR, diags: &mut Vec<Diagnostic>) {
    let ids: BTreeSet<&str> = ir.containers.iter().map(|c| c.id.as_str()).collect();
    for (i, n) in ir.nodes.iter().enumerate() {
        if let Some(c) = &n.container_id {
            if !ids.contains(c.as_str()) {
                let mut d = Diagnostic::new(
                    codes::UNKNOWN_CONTAINER,
                    Severity::Error,
                    format!("$.nodes[{i}].containerId"),
                    format!("node `{}` references container `{c}`, which is not declared", n.id),
                )
                .ids([n.id.as_str()]);
                match closest(c, ids.iter().copied()) {
                    Some(best) => {
                        d = d.suggest(format!("Did you mean `{best}`?")).patch(vec![
                            json!({"op": "replace", "path": format!("/nodes/{i}/containerId"), "value": best}),
                        ]);
                    }
                    None => {
                        d = d
                            .suggest(format!("Declare container `{c}` in `containers`, or remove `containerId`."))
                            .patch(vec![json!({"op": "remove", "path": format!("/nodes/{i}/containerId")})]);
                    }
                }
                diags.push(d);
            }
        }
    }
    for (i, c) in ir.containers.iter().enumerate() {
        if !ir.nodes.iter().any(|n| n.container_id.as_deref() == Some(c.id.as_str())) {
            diags.push(
                Diagnostic::new(
                    codes::EMPTY_CONTAINER,
                    Severity::Warning,
                    format!("$.containers[{i}]"),
                    format!("container `{}` holds no nodes", c.id),
                )
                .ids([c.id.as_str()])
                .suggest("Remove the empty boundary; it adds ink without information.")
                .patch(vec![json!({"op": "remove", "path": format!("/containers/{i}")})]),
            );
        }
    }
}

/// Returns node ids proposed as fixes for broken endpoints, so they aren't
/// also reported (and patched away) as orphans.
fn check_edges(ir: &DiagramIR, nodes: &HashMap<&str, usize>, diags: &mut Vec<Diagnostic>) -> BTreeSet<String> {
    let mut proposed = BTreeSet::new();
    let mut pairs: HashMap<(&str, &str, EdgeType), usize> = HashMap::new();
    for (i, e) in ir.edges.iter().enumerate() {
        for (field, end) in [("source", &e.source), ("target", &e.target)] {
            if !nodes.contains_key(end.as_str()) {
                let mut d = Diagnostic::new(
                    codes::MISSING_ENDPOINT,
                    Severity::Error,
                    format!("$.edges[{i}].{field}"),
                    format!("edge `{}` {field} `{end}` is not a node", e.id),
                )
                .ids([e.id.as_str()]);
                if let Some(best) = closest(end, nodes.keys().copied()) {
                    proposed.insert(best.clone());
                    d = d
                        .suggest(format!("Did you mean `{best}`?"))
                        .patch(vec![json!({"op": "replace", "path": format!("/edges/{i}/{field}"), "value": best})]);
                } else {
                    d = d
                        .suggest(format!("Add node `{end}` or delete the edge."))
                        .patch(vec![json!({"op": "remove", "path": format!("/edges/{i}")})]);
                }
                diags.push(d);
            }
        }
        // A self-call in a sequence or a re-entered state is meaningful.
        if e.source == e.target && !matches!(ir.diagram_type, DiagramType::Sequence | DiagramType::Lifecycle) {
            diags.push(
                Diagnostic::new(
                    codes::SELF_LOOP,
                    Severity::Error,
                    format!("$.edges[{i}]"),
                    format!("edge `{}` connects `{}` to itself", e.id, e.source),
                )
                .ids([e.id.as_str()])
                .suggest("Self-calls are implementation detail; describe them in the node subtitle instead.")
                .patch(vec![json!({"op": "remove", "path": format!("/edges/{i}")})]),
            );
        }
        let key = (e.source.as_str(), e.target.as_str(), e.edge_type);
        if let Some(&first) = pairs.get(&key) {
            diags.push(
                Diagnostic::new(
                    codes::DUPLICATE_EDGE,
                    Severity::Warning,
                    format!("$.edges[{i}]"),
                    format!(
                        "edge `{}` duplicates `{}` ({} → {}, {:?})",
                        e.id, ir.edges[first].id, e.source, e.target, e.edge_type
                    ),
                )
                .ids([e.id.as_str(), ir.edges[first].id.as_str()])
                .suggest("Merge parallel edges into one and combine their labels.")
                .patch(vec![json!({"op": "remove", "path": format!("/edges/{i}")})]),
            );
        } else {
            pairs.insert(key, i);
        }
    }
    proposed
}

fn check_orphans(ir: &DiagramIR, pending: &BTreeSet<String>, diags: &mut Vec<Diagnostic>) {
    // A table without foreign keys is still part of the data model; an unreachable
    // state is reported by the lifecycle rules as a warning.
    if ir.nodes.len() <= 1 || matches!(ir.diagram_type, DiagramType::EntityRelationship | DiagramType::Lifecycle) {
        return;
    }
    let connected: BTreeSet<&str> = ir.edges.iter().flat_map(|e| [e.source.as_str(), e.target.as_str()]).collect();
    for (i, n) in ir.nodes.iter().enumerate() {
        if !connected.contains(n.id.as_str()) && !pending.contains(&n.id) {
            diags.push(
                Diagnostic::new(
                    codes::ORPHAN_NODE,
                    Severity::Error,
                    format!("$.nodes[{i}]"),
                    format!("node `{}` has no edges", n.id),
                )
                .ids([n.id.as_str()])
                .suggest(format!("Connect `{}` to the element it interacts with, or remove it.", n.id))
                .patch(vec![json!({"op": "remove", "path": format!("/nodes/{i}")})]),
            );
        }
    }
}

/// Strongly connected components (Tarjan, iterative) whose edges carry no label
/// at all: a loop the reader can't interpret.
pub const MAX_PARTICIPANTS: usize = 8;
pub const MAX_MESSAGES: usize = 30;
pub const MAX_ATTRIBUTES: usize = 16;

/// Rules that only make sense for one kind of diagram.
fn check_diagram_type(ir: &DiagramIR, nodes: &HashMap<&str, usize>, diags: &mut Vec<Diagnostic>) {
    match ir.diagram_type {
        DiagramType::Sequence => {
            if ir.nodes.len() > MAX_PARTICIPANTS {
                diags.push(
                    Diagnostic::new(
                        codes::TOO_MANY_PARTICIPANTS,
                        Severity::Error,
                        "$.nodes",
                        format!("{} participants; a readable sequence has at most {MAX_PARTICIPANTS}", ir.nodes.len()),
                    )
                    .suggest("Collapse internal modules of one service into that service, or split the flow at a queue or an async boundary."),
                );
            }
            if ir.edges.len() > MAX_MESSAGES {
                diags.push(
                    Diagnostic::new(
                        codes::TOO_MANY_MESSAGES,
                        Severity::Error,
                        "$.edges",
                        format!("{} messages; keep one flow to at most {MAX_MESSAGES}", ir.edges.len()),
                    )
                    .suggest("Drop replies that carry nothing, or split the flow into its synchronous and asynchronous halves."),
                );
            }
            let mut seen: HashMap<u32, &str> = HashMap::new();
            for (i, e) in ir.edges.iter().enumerate() {
                match e.sequence {
                    None => diags.push(
                        Diagnostic::new(
                            codes::SEQUENCE_MISSING,
                            Severity::Error,
                            format!("$.edges[{i}].sequence"),
                            format!("message `{}` has no order", e.id),
                        )
                        .ids([e.id.as_str()])
                        .suggest("Number every message 1, 2, 3… in the order it happens.")
                        .patch(vec![
                            json!({"op": "add", "path": format!("/edges/{i}/sequence"), "value": i as u32 + 1}),
                        ]),
                    ),
                    Some(n) => {
                        if let Some(prev) = seen.insert(n, e.id.as_str()) {
                            diags.push(
                                Diagnostic::new(
                                    codes::SEQUENCE_DUPLICATE,
                                    Severity::Error,
                                    format!("$.edges[{i}].sequence"),
                                    format!("messages `{prev}` and `{}` both have order {n}", e.id),
                                )
                                .ids([prev, e.id.as_str()])
                                .suggest("Renumber so each message has a unique position."),
                            );
                        }
                    }
                }
            }
        }
        DiagramType::EntityRelationship => {
            for (i, n) in ir.nodes.iter().enumerate() {
                match n.attributes.as_ref().map(Vec::len).unwrap_or(0) {
                    0 => diags.push(
                        Diagnostic::new(codes::ENTITY_WITHOUT_ATTRIBUTES, Severity::Warning, format!("$.nodes[{i}]"), format!("entity `{}` lists no attributes", n.id))
                            .ids([n.id.as_str()])
                            .suggest("Add at least the key columns so relations can be read."),
                    ),
                    k if k > MAX_ATTRIBUTES => diags.push(
                        Diagnostic::new(
                            codes::TOO_MANY_ATTRIBUTES,
                            Severity::Warning,
                            format!("$.nodes[{i}].attributes"),
                            format!("entity `{}` has {k} attributes; {MAX_ATTRIBUTES} are drawn and the rest summarised", n.id),
                        )
                        .ids([n.id.as_str()])
                        .suggest("Keep keys, foreign keys and business-relevant columns; audit timestamps rarely earn a row."),
                    ),
                    _ => {}
                }
            }
            for (i, e) in ir.edges.iter().enumerate().filter(|(_, e)| e.cardinality.is_none()) {
                diags.push(
                    Diagnostic::new(
                        codes::MISSING_CARDINALITY,
                        Severity::Warning,
                        format!("$.edges[{i}]"),
                        format!("relation `{}` has no cardinality", e.id),
                    )
                    .ids([e.id.as_str()])
                    .suggest("State 1:1, 1:n, n:1 or n:m; a foreign key column is the n side."),
                );
            }
        }
        DiagramType::Lifecycle => {
            let initial: Vec<usize> = ir
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.state_kind == Some(StateKind::Initial))
                .map(|(i, _)| i)
                .collect();
            if initial.is_empty() && !ir.nodes.is_empty() {
                diags.push(
                    Diagnostic::new(
                        codes::NO_INITIAL_STATE,
                        Severity::Warning,
                        "$.nodes",
                        "no state is marked initial",
                    )
                    .suggest("Mark the state new records start in with stateKind: initial."),
                );
            }
            for (i, n) in ir.nodes.iter().enumerate().filter(|(_, n)| n.state_kind == Some(StateKind::Terminal)) {
                let outgoing: Vec<&str> =
                    ir.edges.iter().filter(|e| e.source == n.id && e.target != n.id).map(|e| e.id.as_str()).collect();
                if !outgoing.is_empty() {
                    diags.push(
                        Diagnostic::new(
                            codes::TERMINAL_HAS_TRANSITIONS,
                            Severity::Error,
                            format!("$.nodes[{i}].stateKind"),
                            format!("terminal state `{}` has outgoing transitions [{}]", n.id, outgoing.join(", ")),
                        )
                        .ids([n.id.as_str()])
                        .suggest("Either the state isn't terminal, or those transitions are wrong.")
                        .patch(vec![
                            json!({"op": "replace", "path": format!("/nodes/{i}/stateKind"), "value": "normal"}),
                        ]),
                    );
                }
            }
            if !initial.is_empty() {
                let mut reached: BTreeSet<&str> = initial.iter().map(|&i| ir.nodes[i].id.as_str()).collect();
                let mut stack: Vec<&str> = reached.iter().copied().collect();
                while let Some(cur) = stack.pop() {
                    for e in ir.edges.iter().filter(|e| e.source == cur) {
                        if nodes.contains_key(e.target.as_str()) && reached.insert(e.target.as_str()) {
                            stack.push(e.target.as_str());
                        }
                    }
                }
                for (i, n) in ir.nodes.iter().enumerate().filter(|(_, n)| !reached.contains(n.id.as_str())) {
                    diags.push(
                        Diagnostic::new(
                            codes::UNREACHABLE_STATE,
                            Severity::Warning,
                            format!("$.nodes[{i}]"),
                            format!("state `{}` can't be reached from an initial state", n.id),
                        )
                        .ids([n.id.as_str()])
                        .suggest("A transition into it wasn't found in code; say so in the node subtitle or add it."),
                    );
                }
            }
            for (i, e) in
                ir.edges.iter().enumerate().filter(|(_, e)| e.label.as_deref().is_none_or(|l| l.trim().is_empty()))
            {
                diags.push(
                    Diagnostic::new(
                        codes::UNLABELED_TRANSITION,
                        Severity::Warning,
                        format!("$.edges[{i}].label"),
                        format!("transition `{}` doesn't say what triggers it", e.id),
                    )
                    .ids([e.id.as_str()])
                    .suggest("Label it with the event or function that performs the change."),
                );
            }
        }
        _ => {}
    }
    // Type-specific fields on other diagrams are ignored by the renderer: say so.
    let misplaced = |field: &str, path: String, allowed: &[DiagramType]| {
        (!allowed.contains(&ir.diagram_type)).then(|| {
            Diagnostic::new(
                codes::FIELD_IGNORED,
                Severity::Warning,
                path,
                format!("`{field}` is only drawn on {allowed:?} diagrams"),
            )
        })
    };
    for (i, n) in ir.nodes.iter().enumerate() {
        if n.attributes.is_some() {
            diags.extend(misplaced(
                "attributes",
                format!("$.nodes[{i}].attributes"),
                &[DiagramType::EntityRelationship],
            ));
        }
        if n.state_kind.is_some() {
            diags.extend(misplaced("stateKind", format!("$.nodes[{i}].stateKind"), &[DiagramType::Lifecycle]));
        }
    }
    for (i, e) in ir.edges.iter().enumerate() {
        if e.sequence.is_some() || e.reply.is_some() || e.payload.is_some() {
            diags.extend(misplaced("sequence/reply/payload", format!("$.edges[{i}]"), &[DiagramType::Sequence]));
        }
        if e.cardinality.is_some() {
            diags.extend(misplaced(
                "cardinality",
                format!("$.edges[{i}].cardinality"),
                &[DiagramType::EntityRelationship],
            ));
        }
    }
}

fn check_cycles(ir: &DiagramIR, nodes: &HashMap<&str, usize>, diags: &mut Vec<Diagnostic>) {
    let n = ir.nodes.len();
    let mut adj: Vec<Vec<usize>> = vec![vec![]; n];
    for e in &ir.edges {
        if let (Some(&s), Some(&t)) = (nodes.get(e.source.as_str()), nodes.get(e.target.as_str())) {
            if s != t {
                adj[s].push(t);
            }
        }
    }
    for comp in tarjan(&adj) {
        if comp.len() < 2 {
            continue;
        }
        let members: BTreeSet<&str> = comp.iter().map(|&i| ir.nodes[i].id.as_str()).collect();
        let inner: Vec<(usize, &Edge)> = ir
            .edges
            .iter()
            .enumerate()
            .filter(|(_, e)| members.contains(e.source.as_str()) && members.contains(e.target.as_str()))
            .collect();
        if inner.iter().all(|(_, e)| e.label.as_deref().is_none_or(|l| l.trim().is_empty())) {
            let (first_i, first) = inner[0];
            diags.push(
                Diagnostic::new(
                    codes::UNLABELED_CYCLE,
                    Severity::Error,
                    format!("$.edges[{first_i}]"),
                    format!(
                        "nodes [{}] form a cycle with no labelled edge",
                        members.iter().copied().collect::<Vec<_>>().join(", ")
                    ),
                )
                .ids(inner.iter().map(|(_, e)| e.id.as_str()))
                .suggest(format!(
                    "Label at least one edge in the loop (e.g. `{}` → `{}`) to say why it feeds back.",
                    first.source, first.target
                ))
                .suggest("If the return leg is a callback or event, set its edgeType to `async`/`event` and label it.")
                .patch(vec![
                    json!({"op": "add", "path": format!("/edges/{first_i}/label"), "value": "returns result"}),
                ]),
            );
        }
    }
}

fn tarjan(adj: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let n = adj.len();
    let mut index = vec![usize::MAX; n];
    let mut low = vec![0; n];
    let mut on_stack = vec![false; n];
    let mut stack = Vec::new();
    let mut out = Vec::new();
    let mut counter = 0;
    for root in 0..n {
        if index[root] != usize::MAX {
            continue;
        }
        let mut work: Vec<(usize, usize)> = vec![(root, 0)];
        while let Some(&mut (v, ref mut next)) = work.last_mut() {
            if *next == 0 && index[v] == usize::MAX {
                index[v] = counter;
                low[v] = counter;
                counter += 1;
                stack.push(v);
                on_stack[v] = true;
            }
            if let Some(&w) = adj[v].get(*next) {
                *next += 1;
                if index[w] == usize::MAX {
                    work.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
            } else {
                work.pop();
                if let Some(&(parent, _)) = work.last() {
                    low[parent] = low[parent].min(low[v]);
                }
                if low[v] == index[v] {
                    let mut comp = Vec::new();
                    while let Some(w) = stack.pop() {
                        on_stack[w] = false;
                        comp.push(w);
                        if w == v {
                            break;
                        }
                    }
                    out.push(comp);
                }
            }
        }
    }
    out
}

fn degrees(ir: &DiagramIR) -> HashMap<&str, usize> {
    let mut d: HashMap<&str, usize> = HashMap::new();
    for e in &ir.edges {
        *d.entry(e.source.as_str()).or_default() += 1;
        *d.entry(e.target.as_str()).or_default() += 1;
    }
    d
}

fn check_color_budget(ir: &DiagramIR, diags: &mut Vec<Diagnostic>) {
    let degree = degrees(ir);
    let focal: Vec<(usize, &Node)> = ir.nodes.iter().enumerate().filter(|(_, n)| n.is_key_focal_point).collect();
    if focal.len() > MAX_FOCAL_POINTS {
        // Keep the most connected; demote the rest.
        let mut ranked = focal.clone();
        ranked.sort_by_key(|(_, n)| (std::cmp::Reverse(degree.get(n.id.as_str()).copied().unwrap_or(0)), n.id.clone()));
        let keep: Vec<&str> = ranked.iter().take(1).map(|(_, n)| n.id.as_str()).collect();
        let demote: Vec<(usize, &Node)> = ranked.iter().skip(1).copied().collect();
        diags.push(
            Diagnostic::new(
                codes::ACCENT_OVERUSE,
                Severity::Error,
                "$.nodes",
                format!(
                    "{} nodes set isKeyFocalPoint; the accent budget allows at most {MAX_FOCAL_POINTS} (ideally 1)",
                    focal.len()
                ),
            )
            .ids(focal.iter().map(|(_, n)| n.id.as_str()))
            .suggest(format!(
                "Keep `{}` (most connected) as the single focal point and set isKeyFocalPoint=false on [{}].",
                keep[0],
                demote.iter().map(|(_, n)| n.id.as_str()).collect::<Vec<_>>().join(", ")
            ))
            .suggest("Express importance of the others through the primary path (`isPrimaryPath`) instead of more accent.")
            .patch(demote.iter().map(|(i, _)| json!({"op": "replace", "path": format!("/nodes/{i}/isKeyFocalPoint"), "value": false})).collect()),
        );
    } else if focal.is_empty() && ir.nodes.len() > 1 {
        let best =
            ir.nodes.iter().enumerate().max_by_key(|(_, n)| {
                (degree.get(n.id.as_str()).copied().unwrap_or(0), std::cmp::Reverse(n.id.clone()))
            });
        if let Some((i, n)) = best {
            diags.push(
                Diagnostic::new(
                    codes::NO_FOCAL_POINT,
                    Severity::Warning,
                    "$.nodes",
                    "no node is marked as the key focal point",
                )
                .suggest(format!("Mark the node the story is about; `{}` is the most connected.", n.id))
                .patch(vec![json!({"op": "replace", "path": format!("/nodes/{i}/isKeyFocalPoint"), "value": true})]),
            );
        }
    }

    let primary: Vec<(usize, &Edge)> = ir.edges.iter().enumerate().filter(|(_, e)| e.primary()).collect();
    let limit = MAX_PRIMARY_EDGES.min(((ir.edges.len() as f64) * 0.5).ceil() as usize).max(1);
    if primary.len() > limit {
        diags.push(
            Diagnostic::new(
                codes::PRIMARY_PATH_OVERUSE,
                Severity::Warning,
                "$.edges",
                format!(
                    "{} edges are on the primary path; keep it to ≤ {limit} so the accent still reads as one story",
                    primary.len()
                ),
            )
            .ids(primary.iter().map(|(_, e)| e.id.as_str()))
            .suggest("Highlight only the critical transaction path (client → focal node → system of record)."),
        );
    }
    if primary.len() >= 2 {
        let components = weak_components(primary.iter().map(|(_, e)| (e.source.as_str(), e.target.as_str())));
        if components > 1 {
            diags.push(
                Diagnostic::new(codes::PRIMARY_PATH_BROKEN, Severity::Warning, "$.edges", format!("primary-path edges form {components} disconnected segments"))
                    .ids(primary.iter().map(|(_, e)| e.id.as_str()))
                    .suggest("The primary path should read as one continuous route; connect the segments or un-highlight the stray edges."),
            );
        }
    }
}

fn weak_components<'a>(edges: impl Iterator<Item = (&'a str, &'a str)>) -> usize {
    let mut parent: HashMap<&str, &str> = HashMap::new();
    fn find<'a>(p: &mut HashMap<&'a str, &'a str>, x: &'a str) -> &'a str {
        let mut r = x;
        while let Some(&up) = p.get(r) {
            if up == r {
                break;
            }
            r = up;
        }
        p.insert(x, r);
        r
    }
    for (a, b) in edges {
        parent.entry(a).or_insert(a);
        parent.entry(b).or_insert(b);
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb {
            parent.insert(ra, rb);
        }
    }
    let keys: Vec<&str> = parent.keys().copied().collect();
    keys.into_iter().map(|k| find(&mut parent, k)).collect::<BTreeSet<_>>().len()
}

pub fn density_report(ir: &DiagramIR, max: f64) -> DensityReport {
    let max = max.min(MAX_VISUAL_DENSITY);
    let budget = (max * AVAILABLE_GRID_UNITS as f64 + 1e-9).floor() as usize;
    let total = ir.nodes.len() + ir.edges.len();
    DensityReport {
        score: visual_density(ir.nodes.len(), ir.edges.len()),
        max,
        nodes: ir.nodes.len(),
        edges: ir.edges.len(),
        grid_units: AVAILABLE_GRID_UNITS,
        budget,
        excess: total.saturating_sub(budget),
    }
}

fn check_density(ir: &DiagramIR, opts: &ValidateOptions, diags: &mut Vec<Diagnostic>) -> DensityReport {
    let report = density_report(ir, opts.max_density);
    if ir.diagram_type == DiagramType::Sequence {
        // Sequences are bounded by participants and messages instead of the grid.
        return report;
    }
    if let Some(claimed) = ir.metadata.visual_density_score {
        if (claimed - report.score).abs() > 0.005 {
            diags.push(
                Diagnostic::new(
                    codes::DENSITY_MISMATCH,
                    Severity::Warning,
                    "$.metadata.visualDensityScore",
                    format!(
                        "declared density {claimed} but (nodes + edges) / {AVAILABLE_GRID_UNITS} = {}",
                        report.score
                    ),
                )
                .patch(vec![json!({"op": "replace", "path": "/metadata/visualDensityScore", "value": report.score})]),
            );
        }
    }
    if report.excess > 0 {
        let mut d = Diagnostic::new(
            codes::HIGH_DENSITY,
            Severity::Error,
            "$",
            format!(
                "visual density {:.3} exceeds {:.2}: {} nodes + {} edges over {} grid units; remove or merge at least {} element(s)",
                report.score, report.max, report.nodes, report.edges, report.grid_units, report.excess
            ),
        );
        for s in density_suggestions(ir, report.excess) {
            d.element_ids.extend(s.ids.iter().cloned());
            d.suggestions.push(s.text);
        }
        d.element_ids.sort();
        d.element_ids.dedup();
        diags.push(d);
    }
    report
}

struct Remedy {
    text: String,
    ids: Vec<String>,
    saves: usize,
}

/// Ranked remedies, largest savings first, enough to cover `excess`.
fn density_suggestions(ir: &DiagramIR, excess: usize) -> Vec<Remedy> {
    let degree = degrees(ir);
    let mut out: Vec<Remedy> = Vec::new();

    // Collapsing a populated container into one node.
    for c in &ir.containers {
        let members: Vec<&Node> =
            ir.nodes.iter().filter(|n| n.container_id.as_deref() == Some(c.id.as_str())).collect();
        if members.len() < 3 || members.iter().any(|n| n.is_key_focal_point) {
            continue;
        }
        let ids: BTreeSet<&str> = members.iter().map(|n| n.id.as_str()).collect();
        let saves = members.len() - 1 + collapsed_edge_savings(ir, &ids);
        out.push(Remedy {
            text: format!(
                "Group nodes [{}] (container `{}`) into a single node labelled \"{}\" — saves {saves} element(s).",
                ids.iter().copied().collect::<Vec<_>>().join(", "),
                c.id,
                c.label
            ),
            ids: ids.iter().map(|s| s.to_string()).collect(),
            saves,
        });
    }

    // Ungrouped nodes with identical neighbourhoods are one concept drawn twice.
    let mut by_neighbours: BTreeMap<Vec<(String, bool)>, Vec<&str>> = BTreeMap::new();
    for n in ir.nodes.iter().filter(|n| !n.is_key_focal_point) {
        let mut nb: Vec<(String, bool)> = ir
            .edges
            .iter()
            .filter_map(|e| {
                if e.source == n.id {
                    Some((e.target.clone(), true))
                } else if e.target == n.id {
                    Some((e.source.clone(), false))
                } else {
                    None
                }
            })
            .collect();
        nb.sort();
        nb.dedup();
        if !nb.is_empty() {
            by_neighbours.entry(nb).or_default().push(n.id.as_str());
        }
    }
    for (nb, group) in by_neighbours.into_iter().filter(|(_, g)| g.len() >= 2) {
        let ids: BTreeSet<&str> = group.iter().copied().collect();
        let saves = group.len() - 1 + nb.len() * (group.len() - 1);
        out.push(Remedy {
            text: format!(
                "Group nodes [{}] into a container and represent them as one node — they share the same {} neighbour(s); saves {saves} element(s).",
                group.join(", "),
                nb.len()
            ),
            ids: ids.iter().map(|s| s.to_string()).collect(),
            saves,
        });
    }

    // Peripheral leaves that aren't part of the story.
    let on_primary: BTreeSet<&str> =
        ir.edges.iter().filter(|e| e.primary()).flat_map(|e| [e.source.as_str(), e.target.as_str()]).collect();
    let mut leaves: Vec<&str> = ir
        .nodes
        .iter()
        .filter(|n| {
            !n.is_key_focal_point
                && !on_primary.contains(n.id.as_str())
                && degree.get(n.id.as_str()).copied().unwrap_or(0) <= 1
        })
        .map(|n| n.id.as_str())
        .collect();
    leaves.sort();
    if !leaves.is_empty() {
        let take: Vec<&str> = leaves.iter().copied().take(excess.div_ceil(2).max(1)).collect();
        out.push(Remedy {
            text: format!(
                "Remove peripheral leaf nodes [{}] (degree ≤ 1, off the primary path) — saves {} element(s).",
                take.join(", "),
                take.len() * 2
            ),
            ids: take.iter().map(|s| s.to_string()).collect(),
            saves: take.len() * 2,
        });
    }

    out.sort_by(|a, b| b.saves.cmp(&a.saves).then_with(|| a.text.cmp(&b.text)));
    let mut covered = 0;
    let mut chosen = Vec::new();
    for r in out {
        if covered >= excess && !chosen.is_empty() {
            break;
        }
        covered += r.saves;
        chosen.push(r);
    }
    if chosen.is_empty() {
        chosen.push(Remedy {
            text: format!("Split into two diagrams (e.g. a container view and a component view of the busiest node) — you need to drop {excess} element(s)."),
            ids: vec![],
            saves: excess,
        });
    }
    chosen
}

/// Edges removed when `members` collapse: internal edges vanish and parallel
/// edges to the same outside neighbour merge.
fn collapsed_edge_savings(ir: &DiagramIR, members: &BTreeSet<&str>) -> usize {
    let mut internal = 0;
    let mut external: BTreeMap<(String, bool), usize> = BTreeMap::new();
    for e in &ir.edges {
        let (s, t) = (members.contains(e.source.as_str()), members.contains(e.target.as_str()));
        match (s, t) {
            (true, true) => internal += 1,
            (true, false) => *external.entry((e.target.clone(), true)).or_default() += 1,
            (false, true) => *external.entry((e.source.clone(), false)).or_default() += 1,
            _ => {}
        }
    }
    internal + external.values().map(|c| c - 1).sum::<usize>()
}

fn check_legibility(ir: &DiagramIR, diags: &mut Vec<Diagnostic>) {
    let mut long = |path: String, id: &str, what: &str, value: &str, max: usize| {
        let n = value.chars().count();
        if n > max {
            diags.push(
                Diagnostic::new(
                    codes::LABEL_TOO_LONG,
                    Severity::Warning,
                    path,
                    format!("{what} of `{id}` is {n} chars (max {max}); it will be truncated"),
                )
                .ids([id])
                .suggest(format!("Shorten to ≤ {max} chars; move detail into `metadata.docstring`.")),
            );
        }
    };
    long("$.title".into(), "diagram", "title", &ir.title, TITLE_MAX);
    for (i, n) in ir.nodes.iter().enumerate() {
        long(format!("$.nodes[{i}].label"), &n.id, "label", &n.label, NODE_LABEL_MAX);
        if let Some(s) = &n.subtitle {
            long(format!("$.nodes[{i}].subtitle"), &n.id, "subtitle", s, SUBTITLE_MAX);
        }
        if let Some(s) = &n.tech_stack {
            long(format!("$.nodes[{i}].techStack"), &n.id, "techStack", s, TECH_MAX);
        }
    }
    for (i, e) in ir.edges.iter().enumerate() {
        if let Some(l) = &e.label {
            long(format!("$.edges[{i}].label"), &e.id, "label", l, EDGE_LABEL_MAX);
        }
    }
}

fn check_evidence(ir: &DiagramIR, opts: &ValidateOptions, diags: &mut Vec<Diagnostic>) -> Vec<EvidenceReport> {
    let mut reports = Vec::new();
    for (i, n) in ir.nodes.iter().enumerate() {
        let Some(ev) = &n.evidence else {
            if n.is_key_focal_point {
                diags.push(
                    Diagnostic::new(
                        codes::FOCAL_WITHOUT_EVIDENCE,
                        Severity::Warning,
                        format!("$.nodes[{i}]"),
                        format!("focal node `{}` has no source evidence", n.id),
                    )
                    .ids([n.id.as_str()])
                    .suggest("Pin the focal point to its entry point from nunki_scan_repository's evidenceMap."),
                );
            }
            continue;
        };
        if ev.start_line == 0 || ev.end_line < ev.start_line {
            diags.push(
                Diagnostic::new(
                    codes::EVIDENCE_RANGE,
                    Severity::Error,
                    format!("$.nodes[{i}].evidence"),
                    format!("evidence for `{}` has invalid range {}-{} (1-based, start ≤ end)", n.id, ev.start_line, ev.end_line),
                )
                .ids([n.id.as_str()])
                .patch(vec![
                    json!({"op": "replace", "path": format!("/nodes/{i}/evidence/startLine"), "value": ev.start_line.max(1)}),
                    json!({"op": "replace", "path": format!("/nodes/{i}/evidence/endLine"), "value": ev.end_line.max(ev.start_line.max(1))}),
                ]),
            );
        }
    }
    for (i, e) in ir.edges.iter().enumerate() {
        let Some(ev) = &e.evidence else { continue };
        if ev.start_line == 0 || ev.end_line < ev.start_line {
            diags.push(
                Diagnostic::new(
                    codes::EVIDENCE_RANGE,
                    Severity::Error,
                    format!("$.edges[{i}].evidence"),
                    format!(
                        "evidence for `{}` has invalid range {}-{} (1-based, start ≤ end)",
                        e.id, ev.start_line, ev.end_line
                    ),
                )
                .ids([e.id.as_str()])
                .patch(vec![json!({"op": "remove", "path": format!("/edges/{i}/evidence")})]),
            );
        }
    }
    if !opts.verify_evidence {
        return reports;
    }
    let Some(root) = &opts.repo_root else {
        if ir.nodes.iter().any(|n| n.evidence.is_some()) || ir.edges.iter().any(|e| e.evidence.is_some()) {
            diags.push(
                Diagnostic::new(
                    codes::EVIDENCE_UNVERIFIED,
                    Severity::Warning,
                    "$.metadata.targetRepo",
                    "evidence was not verified: no readable repository root",
                )
                .suggest("Pass `repoPath` (MCP) or `--repo` (CLI) pointing at the scanned repository."),
            );
        }
        return reports;
    };
    let ctx: RepoContext = match &opts.evidence_cache {
        Some(cache) => cache.context(root),
        None => nunki_git::repo_context(root),
    };
    let mut verifier =
        nunki_git::Verifier::cached(&ctx, ir.metadata.commit_hash.as_deref(), opts.evidence_cache.as_deref());
    let pinned = ir.metadata.commit_hash.as_deref();
    if let (Some(pin), Some(head)) = (pinned, ctx.head_commit.as_deref()) {
        if !head.starts_with(pin) && !pin.starts_with(head) {
            diags.push(
                Diagnostic::new(
                    codes::COMMIT_MISMATCH,
                    Severity::Warning,
                    "$.metadata.commitHash",
                    format!(
                        "diagram is pinned to {} but HEAD is {}; evidence was checked for drift",
                        nunki_git::short(pin),
                        nunki_git::short(head)
                    ),
                )
                .suggest("Re-scan and update commitHash once stale evidence is fixed.")
                .patch(vec![json!({"op": "replace", "path": "/metadata/commitHash", "value": head})]),
            );
        }
    }
    let pinned_items: Vec<(&str, usize, &str, &Evidence)> = ir
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| n.evidence.as_ref().map(|e| ("nodes", i, n.id.as_str(), e)))
        .chain(
            ir.edges
                .iter()
                .enumerate()
                .filter_map(|(i, e)| e.evidence.as_ref().map(|ev| ("edges", i, e.id.as_str(), ev))),
        )
        .collect();
    for (kind, i, id, ev) in pinned_items {
        if ev.start_line == 0 || ev.end_line < ev.start_line {
            continue;
        }
        let q = EvidenceQuery {
            file_path: ev.file_path.clone(),
            line: ev.start_line,
            end_line: Some(ev.end_line),
            symbol_name: ev.symbol_name.clone(),
        };
        let r = verifier.verify(&q);
        let path = format!("$.{kind}[{i}].evidence");
        match r.state {
            EvidenceState::Verified => {}
            EvidenceState::FileMissing => diags.push(
                Diagnostic::new(
                    codes::EVIDENCE_FILE_MISSING,
                    Severity::Error,
                    format!("{path}.filePath"),
                    format!("`{id}`: {}", r.detail),
                )
                .ids([id])
                .suggest("Use a repository-relative path exactly as reported by nunki_scan_repository.")
                .patch(vec![json!({"op": "remove", "path": format!("/{kind}/{i}/evidence")})]),
            ),
            EvidenceState::OutsideRepo => diags.push(
                Diagnostic::new(
                    codes::EVIDENCE_OUTSIDE_REPO,
                    Severity::Error,
                    format!("{path}.filePath"),
                    format!("`{id}`: {}", r.detail),
                )
                .ids([id])
                .patch(vec![json!({"op": "remove", "path": format!("/{kind}/{i}/evidence")})]),
            ),
            EvidenceState::LineOutOfRange => {
                let count = r.line_count.unwrap_or(1).max(1);
                diags.push(
                    Diagnostic::new(codes::EVIDENCE_OUT_OF_RANGE, Severity::Error, format!("{path}.endLine"), format!("`{id}`: {}", r.detail))
                        .ids([id])
                        .suggest(format!("Clamp the range into 1-{count} or re-read the symbol's range from the scan."))
                        .patch(vec![
                            json!({"op": "replace", "path": format!("/{kind}/{i}/evidence/startLine"), "value": ev.start_line.min(count)}),
                            json!({"op": "replace", "path": format!("/{kind}/{i}/evidence/endLine"), "value": ev.end_line.min(count)}),
                        ]),
                );
            }
            EvidenceState::SymbolMismatch => diags.push(
                Diagnostic::new(
                    codes::EVIDENCE_SYMBOL_MISMATCH,
                    Severity::Error,
                    format!("{path}.symbolName"),
                    format!("`{id}`: {}", r.detail),
                )
                .ids([id])
                .suggest("Point startLine/endLine at the symbol's definition, or correct symbolName."),
            ),
            EvidenceState::Stale => diags.push(
                Diagnostic::new(
                    codes::EVIDENCE_STALE,
                    Severity::Warning,
                    path.clone(),
                    format!("`{id}`: {}", r.detail),
                )
                .ids([id])
                .suggest("Re-scan and refresh this evidence range, then update metadata.commitHash."),
            ),
            EvidenceState::Untracked => diags.push(
                Diagnostic::new(
                    codes::EVIDENCE_UNTRACKED,
                    Severity::Warning,
                    path.clone(),
                    format!("`{id}`: {}", r.detail),
                )
                .ids([id])
                .suggest("Commit the file so the evidence has a permanent reference."),
            ),
        }
        reports.push(r);
    }
    reports
}

/// Closest candidate by edit distance, when it's plausibly a typo.
fn closest<'a>(needle: &str, candidates: impl Iterator<Item = &'a str>) -> Option<String> {
    candidates
        .map(|c| (levenshtein(&needle.to_lowercase(), &c.to_lowercase()), c))
        .filter(|(d, c)| *d <= (c.len().max(needle.len()) / 3).max(1))
        .min_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)))
        .map(|(_, c)| c.to_string())
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push((prev[j] + usize::from(ca != *cb)).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Validates and removes evidence that doesn't verify (up to three rounds),
/// the way an agent applies `ERR_EVIDENCE_*` fixes. Returns the final report
/// and a note per removed pin.
pub fn heal_evidence(ir: &mut DiagramIR, opts: &ValidateOptions) -> (ValidationReport, Vec<String>) {
    let mut notes = Vec::new();
    let mut report = validate(ir, opts);
    for _ in 0..3 {
        let index = |prefix: &str, d: &Diagnostic| -> Option<usize> {
            d.path.strip_prefix(prefix)?.split(']').next()?.parse().ok()
        };
        let evidence_errors: Vec<&Diagnostic> = report
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error && d.code.starts_with("ERR_EVIDENCE"))
            .collect();
        let nodes: Vec<usize> = evidence_errors.iter().filter_map(|d| index("$.nodes[", d)).collect();
        let edges: Vec<usize> = evidence_errors.iter().filter_map(|d| index("$.edges[", d)).collect();
        if nodes.is_empty() && edges.is_empty() {
            break;
        }
        for i in nodes {
            if let Some(n) = ir.nodes.get_mut(i) {
                if let Some(ev) = n.evidence.take() {
                    notes.push(format!(
                        "removed unverifiable evidence {}:{} from `{}`",
                        ev.file_path, ev.start_line, n.id
                    ));
                }
            }
        }
        for i in edges {
            if let Some(e) = ir.edges.get_mut(i) {
                if let Some(ev) = e.evidence.take() {
                    notes.push(format!(
                        "removed unverifiable evidence {}:{} from `{}`",
                        ev.file_path, ev.start_line, e.id
                    ));
                }
            }
        }
        report = validate(ir, opts);
    }
    (report, notes)
}

/// Applies RFC 6902 `add`/`remove`/`replace` ops. Used by tests and by agents'
/// harnesses to demonstrate that diagnostics are self-healing.
pub fn apply_patch(doc: &mut Value, ops: &[Value]) -> Result<(), String> {
    for op in ops {
        let kind = op.get("op").and_then(Value::as_str).ok_or("op missing")?;
        let path = op.get("path").and_then(Value::as_str).ok_or("path missing")?;
        let mut parts: Vec<String> = path.split('/').skip(1).map(|p| p.replace("~1", "/").replace("~0", "~")).collect();
        let last = parts.pop().ok_or("empty path")?;
        let mut target = &mut *doc;
        for p in &parts {
            target = match target {
                Value::Array(a) => a.get_mut(p.parse::<usize>().map_err(|_| format!("bad index {p}"))?),
                Value::Object(o) => o.get_mut(p),
                _ => None,
            }
            .ok_or_else(|| format!("path {path} not found"))?;
        }
        let value = op.get("value").cloned();
        match (target, kind) {
            (Value::Object(o), "add" | "replace") => {
                o.insert(last, value.ok_or("value missing")?);
            }
            (Value::Object(o), "remove") => {
                o.remove(&last).ok_or_else(|| format!("{path} not found"))?;
            }
            (Value::Array(a), "remove") => {
                let i: usize = last.parse().map_err(|_| "bad index")?;
                if i >= a.len() {
                    return Err(format!("{path} out of bounds"));
                }
                a.remove(i);
            }
            (Value::Array(a), "replace") => {
                let i: usize = last.parse().map_err(|_| "bad index")?;
                *a.get_mut(i).ok_or("out of bounds")? = value.ok_or("value missing")?;
            }
            (Value::Array(a), "add") => {
                let v = value.ok_or("value missing")?;
                if last == "-" {
                    a.push(v)
                } else {
                    a.insert(last.parse().map_err(|_| "bad index")?, v)
                }
            }
            _ => return Err(format!("unsupported op {kind} at {path}")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
