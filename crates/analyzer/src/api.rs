//! API contracts: operations a unit publishes (routes and their request /
//! response models, validation rules, errors, auth) and the calls other units
//! make to them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::scan::EvidenceRef;
use crate::source::SourceIndex;

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiModel {
    pub operations: Vec<Operation>,
    pub models: Vec<Model>,
    pub client_calls: Vec<ClientCall>,
    /// Operations deliberately not treated as functional (health probes, metrics, docs), with the reason.
    pub excluded: Vec<Excluded>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Confidence {
    /// Request and response types are declared.
    Typed,
    /// Some of the contract is declared.
    Partial,
    /// Nothing declared (e.g. an Express handler destructuring at runtime).
    Opaque,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    /// Stable id: `<unit>:<METHOD> <path>`.
    pub id: String,
    pub unit: String,
    /// `http`, `grpc`, `graphql`, `event` (consumer), `cli`.
    pub protocol: String,
    /// Framework that registers it (express, fastapi, gin, axum, …).
    pub framework: String,
    pub method: String,
    /// Full path with every router/app prefix resolved, e.g. `/api/v1/items/{id}`.
    pub path: String,
    /// True when a prefix could not be resolved statically.
    #[serde(default)]
    pub path_partial: bool,
    /// Request condition that picks this handler among others on the same
    /// route (Spring `@GetMapping(params = {"deviceName"})` → `deviceName`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    pub handler: SymbolRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<Param>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_body: Option<TypeRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<TypeRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success_status: Option<u16>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ErrorResponse>,
    /// Authentication / authorization the code enforces (middleware, dependencies, guards).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auth: Vec<Requirement>,
    /// First sentence of the handler's own documentation, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub confidence: Confidence,
    pub evidence: EvidenceRef,
    /// Capability group the operation belongs to (controller, router, tag), e.g. `Device`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SymbolRef {
    pub name: String,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Param {
    /// Name on the wire.
    pub name: String,
    /// Name in code when it differs from the wire name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_name: Option<String>,
    /// `path`, `query`, `header`, `cookie`.
    pub location: String,
    pub type_name: String,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TypeRef {
    pub type_name: String,
    /// Id of the resolved model in `ApiModel::models`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default)]
    pub collection: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    /// `<unit>:<Name>`.
    pub id: String,
    pub unit: String,
    pub name: String,
    pub fields: Vec<Field>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    /// Name on the wire (after alias / json tag / serde rename).
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_name: Option<String>,
    pub type_name: String,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    /// Resolved nested model id for object-typed fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    /// Human-readable: "at most 255 characters", "one of: pending, paid", "must be a valid email".
    pub statement: String,
    /// `length`, `range`, `pattern`, `format`, `enum`, `required`, `custom`.
    pub kind: String,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Requirement {
    /// `authenticated`, `role`, `scope`, `api-key`, `custom`.
    pub kind: String,
    pub detail: String,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClientCall {
    pub unit: String,
    pub method: String,
    /// Path as written, with dynamic parts normalised to `{param}`.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_unit: Option<String>,
    /// Matched operation id in the target unit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    /// Enclosing function making the call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller: Option<String>,
    /// Response fields the caller reads that the operation's response doesn't declare.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drift: Vec<String>,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Excluded {
    pub operation: String,
    pub reason: String,
    pub evidence: EvidenceRef,
}

mod client;
mod go;
mod java;
mod kt;
mod py;
mod rs;
pub(crate) mod text;
mod ts;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::extract::{FileFacts, Symbol, SymbolKind};
use crate::lang::Language;
use text::Src;

/// A parsed source file handed to the framework extractors.
pub(crate) struct Loaded<'a> {
    pub unit: &'a str,
    pub lang: Language,
    pub facts: &'a FileFacts,
    pub src: Src,
}

impl Loaded<'_> {
    pub fn path(&self) -> &str {
        &self.src.path
    }

    /// Innermost function/method symbol containing `line`.
    pub fn enclosing(&self, line: u32) -> Option<&Symbol> {
        self.facts
            .symbols
            .iter()
            .filter(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
            .filter(|s| s.start_line <= line && line <= s.end_line)
            .min_by_key(|s| s.end_line - s.start_line)
    }

    pub fn function(&self, name: &str) -> Option<&Symbol> {
        self.facts
            .symbols
            .iter()
            .find(|s| s.name == name && matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
    }

    /// Byte range covering a symbol's lines.
    pub fn span(&self, s: &Symbol) -> (usize, usize) {
        (self.src.line_start(s.start_line), self.src.line_end(s.end_line))
    }
}

/// An operation plus what the extractor could see declared (vs inferred from code).
pub(crate) struct Draft {
    pub op: Operation,
    pub request_declared: bool,
    pub response_declared: bool,
}

pub(crate) struct ClientDraft {
    pub call: ClientCall,
    /// Type the caller casts the response to (`as T`, `get<T>`, `Promise<T>`,
    /// a Feign return type, `X.class`, `ParameterizedTypeReference<X>`, a pydantic
    /// model, a Go struct decoded into).
    pub expects: Option<String>,
    /// Fields the caller reads straight off the body without naming a type
    /// (`response.json()["balance"]`).
    pub expects_fields: Vec<String>,
}

#[derive(Default)]
pub(crate) struct Harvest {
    pub ops: Vec<Draft>,
    pub models: Vec<Model>,
    pub clients: Vec<ClientDraft>,
    /// Handlers an extractor decided are not API operations, so the book can
    /// say what it left out instead of implying it documented everything.
    pub excluded: Vec<Excluded>,
}

impl Harvest {
    pub fn model(&mut self, m: Model) {
        self.models.push(m);
    }
}

pub(crate) fn new_op(
    unit: &str,
    framework: &str,
    method: &str,
    path: String,
    handler: SymbolRef,
    evidence: EvidenceRef,
) -> Operation {
    let method = method.to_uppercase();
    Operation {
        id: format!("{unit}:{method} {path}"),
        unit: unit.to_string(),
        protocol: "http".into(),
        framework: framework.into(),
        method,
        path,
        path_partial: false,
        selector: None,
        handler,
        params: vec![],
        request_body: None,
        response: None,
        success_status: None,
        errors: vec![],
        auth: vec![],
        summary: None,
        confidence: Confidence::Opaque,
        evidence,
        group: None,
    }
}

pub(crate) fn type_ref(type_name: &str) -> TypeRef {
    let (_, collection) = unwrap_type(type_name);
    TypeRef { type_name: type_name.trim().to_string(), model: None, collection }
}

pub(crate) fn rule(statement: impl Into<String>, kind: &str, evidence: &EvidenceRef) -> Rule {
    Rule { statement: statement.into(), kind: kind.into(), evidence: evidence.clone() }
}

/// Adds a path parameter for every `{name}` segment the extractor did not already describe.
pub(crate) fn fill_path_params(op: &mut Operation) {
    for name in text::path_params(&op.path) {
        if !op.params.iter().any(|p| p.location == "path" && p.name == name) {
            op.params.push(Param {
                name,
                code_name: None,
                location: "path".into(),
                type_name: "string".into(),
                required: true,
                rules: vec![],
                evidence: op.evidence.clone(),
            });
        }
    }
    // Path parameters in path order, then the rest in discovery order.
    let order = text::path_params(&op.path);
    op.params.sort_by_key(|p| {
        if p.location == "path" {
            order.iter().position(|n| *n == p.name).unwrap_or(99)
        } else {
            100
        }
    });
}

pub(crate) fn add_param(op: &mut Operation, p: Param) {
    if let Some(existing) = op.params.iter_mut().find(|x| x.name == p.name && x.location == p.location) {
        if existing.type_name == "string" && p.type_name != "string" {
            existing.type_name = p.type_name;
        }
        for r in p.rules {
            if !existing.rules.contains(&r) {
                existing.rules.push(r);
            }
        }
        return;
    }
    op.params.push(p);
}

pub(crate) fn add_error(op: &mut Operation, status: Option<u16>, message: Option<String>, evidence: EvidenceRef) {
    if op.errors.iter().any(|e| e.status == status && (e.message == message || message.is_none())) {
        return;
    }
    if let Some(e) = op.errors.iter_mut().find(|e| e.status == status && e.message.is_none()) {
        e.message = message;
        e.evidence = evidence;
        return;
    }
    op.errors.push(ErrorResponse { status, message, evidence });
}

pub(crate) fn add_auth(op: &mut Operation, r: Requirement) {
    if !op.auth.iter().any(|a| a.kind == r.kind && a.detail == r.detail) {
        op.auth.push(r);
    }
}

/// Classifies a middleware / dependency / guard name as an auth requirement.
pub(crate) fn auth_requirement(expr: &str, evidence: EvidenceRef) -> Option<Requirement> {
    let e = expr.trim();
    let name = e.split('(').next().unwrap_or(e);
    let last = name.rsplit(['.', ':']).next().unwrap_or(name).to_lowercase();
    let lower = e.to_lowercase();
    let authish = last.contains("auth")
        || last.starts_with("require")
        || last.contains("protect")
        || last.contains("guard")
        || last.contains("jwt")
        || last.contains("loggedin")
        || last.contains("login_required")
        || last.contains("current_user")
        || last.contains("currentuser")
        || last.contains("superuser")
        || last.contains("apikey")
        || last.contains("api_key")
        || last.contains("permission")
        || last.contains("role")
        || last.contains("admin");
    if !authish || last.contains("author") && !last.contains("authoriz") {
        return None;
    }
    let kind = if lower.contains("role") || lower.contains("admin") || lower.contains("superuser") {
        "role"
    } else if lower.contains("scope") || lower.contains("permission") {
        "scope"
    } else if lower.contains("apikey") || lower.contains("api_key") || lower.contains("api-key") {
        "api-key"
    } else {
        "authenticated"
    };
    Some(Requirement { kind: kind.into(), detail: e.split_whitespace().collect::<Vec<_>>().join(" "), evidence })
}

const PROBES: &[&str] = &[
    "health",
    "healthz",
    "healthcheck",
    "health-check",
    "health_check",
    "livez",
    "live",
    "liveness",
    "readyz",
    "ready",
    "readiness",
    "ping",
    "metrics",
    "docs",
    "redoc",
    "openapi.json",
    "openapi",
    "swagger",
    "swagger.json",
    "favicon.ico",
];

/// Segments that mark a route as operational wherever they appear. Kept to the
/// ones that are never a business resource: `/patients/{id}/health` is
/// functionality, `/v2/metrics/bucket` and `/debug/vars` are not, and matching
/// only the last segment missed both.
const OPERATIONAL_SEGMENTS: &[&str] = &["metrics", "pprof", "debug", "healthz", "livez", "readyz"];

fn excluded_reason(path: &str) -> Option<&'static str> {
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let last = segs.last()?.to_lowercase();
    // An operational route answers for the service as a whole, so it never hangs
    // off one resource: `/metrics` reports on the process, `/dashboards/{id}/metrics`
    // reports on a dashboard and is functionality.
    let about_one_resource = segs[..segs.len() - 1].iter().any(|s| s.contains('{') || s.starts_with(':'));
    if about_one_resource {
        return None;
    }
    if let Some(marker) = segs.iter().find(|s| OPERATIONAL_SEGMENTS.contains(&s.to_lowercase().as_str())) {
        return Some(match marker.to_lowercase().as_str() {
            "metrics" => "metrics endpoint",
            "pprof" | "debug" => "debug / profiling endpoint",
            _ => "health / liveness probe",
        });
    }
    if !PROBES.contains(&last.as_str()) && !segs.first().is_some_and(|f| matches!(*f, "docs" | "swagger")) {
        return None;
    }
    Some(match last.as_str() {
        "metrics" => "metrics endpoint",
        "docs" | "redoc" | "openapi.json" | "openapi" | "swagger" | "swagger.json" => "API documentation endpoint",
        "favicon.ico" => "static asset",
        _ if segs.first().is_some_and(|f| matches!(*f, "docs" | "swagger")) => "API documentation endpoint",
        _ => "health / liveness probe",
    })
}

/// Strips wrappers (`list[T]`, `Vec<T>`, `T[]`, `Option<T>`, `Json<T>`, `Promise<T>`, `*T`, `[]T`, …)
/// and returns the inner type name and whether a collection wrapper was seen.
pub(crate) fn unwrap_type(t: &str) -> (String, bool) {
    const COLLECTIONS: &[&str] = &[
        "Vec",
        "Array",
        "ReadonlyArray",
        "list",
        "List",
        "Sequence",
        "set",
        "Set",
        "HashSet",
        "BTreeSet",
        "Iterable",
        "tuple",
    ];
    const WRAPPERS: &[&str] = &[
        "Option",
        "Optional",
        "Json",
        "Promise",
        "Result",
        "Box",
        "Arc",
        "Rc",
        "Annotated",
        "Partial",
        "Readonly",
        "Required",
        "Awaited",
        "Query",
        "Path",
        "Form",
        "Data",
        "Reply",
        "Observable",
    ];
    let mut s = t.trim().to_string();
    let mut coll = false;
    for _ in 0..8 {
        s = s.trim().trim_start_matches('&').trim_start_matches("mut ").trim().to_string();
        // Unions with null / None / undefined.
        if s.contains('|') && !s.contains('<') && !s.contains('[') {
            let parts: Vec<&str> =
                s.split('|').map(str::trim).filter(|p| !matches!(*p, "None" | "null" | "undefined")).collect();
            if parts.len() == 1 {
                s = parts[0].to_string();
                continue;
            }
        }
        if let Some(x) = s.strip_suffix("[]") {
            coll = true;
            s = x.to_string();
            continue;
        }
        if let Some(x) = s.strip_prefix("[]") {
            coll = true;
            s = x.to_string();
            continue;
        }
        if let Some(x) = s.strip_prefix('*') {
            s = x.to_string();
            continue;
        }
        let open = s.find(['<', '[']);
        if let Some(o) = open {
            let close = if s.ends_with('>') || s.ends_with(']') { s.len() - 1 } else { break };
            let head = s[..o].trim();
            let head = head.rsplit(['.', ':']).next().unwrap_or(head);
            let inner = &s[o + 1..close];
            let first = first_top_level(inner);
            if COLLECTIONS.contains(&head) {
                coll = true;
                s = first;
                continue;
            }
            if WRAPPERS.contains(&head) {
                s = first;
                continue;
            }
            break;
        }
        break;
    }
    let base = s.rsplit(['.', ':']).next().unwrap_or(&s).trim().to_string();
    (base, coll)
}

fn first_top_level(s: &str) -> String {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '<' | '[' | '(' | '{' => depth += 1,
            '>' | ']' | ')' | '}' => depth -= 1,
            ',' if depth == 0 => return s[..i].trim().to_string(),
            _ => {}
        }
    }
    s.trim().to_string()
}

/// Extracts the API model: operations with every router prefix resolved, their
/// request/response models and validation rules, and the HTTP calls units make to each other.
pub fn extract(index: &SourceIndex) -> ApiModel {
    let mut files: Vec<Loaded> = index
        .files
        .iter()
        .filter(|f| {
            matches!(
                f.language,
                Language::Rust
                    | Language::TypeScript
                    | Language::JavaScript
                    | Language::Go
                    | Language::Python
                    | Language::Java
                    | Language::Kotlin
            )
        })
        .filter_map(|f| {
            let body = index.read(f.path)?;
            Some(Loaded {
                unit: f.unit,
                lang: f.language,
                facts: f.facts,
                src: Src::new(f.path, body, text::style_for(f.language)),
            })
        })
        .collect();
    files.sort_by(|a, b| a.src.path.cmp(&b.src.path));

    let mut h = Harvest::default();
    ts::extract(&files, &mut h);
    py::extract(&files, &mut h);
    go::extract(&files, &mut h);
    rs::extract(&files, &mut h);
    java::extract(index, &files, &mut h);
    kt::extract(index, &files, &mut h);
    client::extract(&files, &mut h);
    let mut api = finish(h);
    sanitize_symbols(&mut api, &files);
    crate::capability::assign_groups(&mut api, index);
    api
}

/// A citation names a symbol only when that identifier appears inside the cited lines.
fn sanitize_symbols(api: &mut ApiModel, files: &[Loaded]) {
    let by_path: HashMap<&str, &Src> = files.iter().map(|f| (f.src.path.as_str(), &f.src)).collect();
    let check = |e: &mut EvidenceRef| {
        let Some(sym) = &e.symbol_name else { return };
        let ok = by_path.get(e.file_path.as_str()).is_some_and(|src| {
            let start = src.line_start(e.start_line);
            let end = src.line_end(e.end_line);
            !text::find_word(src.slice(start, end), sym).is_empty()
        });
        if !ok {
            e.symbol_name = None;
        }
    };
    for op in &mut api.operations {
        check(&mut op.evidence);
        check(&mut op.handler.evidence);
        op.params.iter_mut().for_each(|p| check(&mut p.evidence));
        op.errors.iter_mut().for_each(|x| check(&mut x.evidence));
        op.auth.iter_mut().for_each(|x| check(&mut x.evidence));
    }
    for m in &mut api.models {
        check(&mut m.evidence);
        for f in &mut m.fields {
            check(&mut f.evidence);
            f.rules.iter_mut().for_each(|r| check(&mut r.evidence));
        }
    }
    api.client_calls.iter_mut().for_each(|c| check(&mut c.evidence));
    api.excluded.iter_mut().for_each(|x| check(&mut x.evidence));
}

fn finish(h: Harvest) -> ApiModel {
    // Model definitions per unit: first definition of a name wins (files are visited in path order).
    let mut defs: BTreeMap<(String, String), Model> = BTreeMap::new();
    for m in h.models {
        defs.entry((m.unit.clone(), m.name.clone())).or_insert(m);
    }
    let keys: BTreeSet<(String, String)> = defs.keys().cloned().collect();
    let resolve = |unit: &str, type_name: &str| -> (Option<String>, bool) {
        let (inner, coll) = unwrap_type(type_name);
        let id = keys.contains(&(unit.to_string(), inner.clone())).then(|| format!("{unit}:{inner}"));
        (id, coll)
    };
    for m in defs.values_mut() {
        let unit = m.unit.clone();
        let own = m.name.clone();
        for f in &mut m.fields {
            if f.model.is_none() {
                let (id, _) = resolve(&unit, &f.type_name);
                f.model = id.filter(|id| *id != format!("{unit}:{own}") || f.type_name.contains(&own));
            }
        }
    }

    let mut api = ApiModel::default();
    api.excluded.extend(h.excluded);
    let mut seen = HashMap::new();
    for d in h.ops {
        let mut op = d.op;
        if let Some(reason) = excluded_reason(&op.path) {
            if !api.excluded.iter().any(|e: &Excluded| e.operation == op.id) {
                api.excluded.push(Excluded {
                    operation: op.id.clone(),
                    reason: reason.into(),
                    evidence: op.evidence.clone(),
                });
            }
            continue;
        }
        for tr in [&mut op.request_body, &mut op.response].into_iter().flatten() {
            let (id, coll) = resolve(&op.unit, &tr.type_name);
            tr.model = tr.model.take().or(id);
            tr.collection |= coll;
        }
        fill_path_params(&mut op);
        if op.success_status.is_none() && op.response.is_some() {
            op.success_status = Some(200);
        }
        let response_declared = d.response_declared || op.success_status == Some(204) && op.response.is_none();
        let body_method = matches!(op.method.as_str(), "POST" | "PUT" | "PATCH");
        let declared_params =
            op.params.iter().any(|p| p.location != "path" && (!p.rules.is_empty() || p.type_name != "string"));
        op.confidence = match (body_method, d.request_declared, response_declared) {
            (true, true, true) => Confidence::Typed,
            (true, false, false) => Confidence::Opaque,
            (true, _, _) => Confidence::Partial,
            (false, _, true) => Confidence::Typed,
            (false, _, false) if declared_params || op.request_body.is_some() || op.response.is_some() => {
                Confidence::Partial
            }
            _ => Confidence::Opaque,
        };
        op.errors.sort_by_key(|e| (e.status, e.evidence.start_line));
        if seen.insert(op.id.clone(), ()).is_some() {
            continue;
        }
        api.operations.push(op);
    }

    // Keep only models the contracts reference, plus their nested models.
    let mut wanted: BTreeSet<String> = BTreeSet::new();
    let mut stack: Vec<String> = api
        .operations
        .iter()
        .flat_map(|o| [&o.request_body, &o.response])
        .flatten()
        .filter_map(|t| t.model.clone())
        .collect();
    while let Some(id) = stack.pop() {
        if !wanted.insert(id.clone()) {
            continue;
        }
        if let Some((unit, name)) = id.split_once(':') {
            if let Some(m) = defs.get(&(unit.to_string(), name.to_string())) {
                stack.extend(m.fields.iter().filter_map(|f| f.model.clone()));
            }
        }
    }

    // Client calls → operations.
    for c in h.clients {
        let mut call = c.call;
        // A declarative client that names its service is matched within that service only.
        let candidates: Vec<Operation> = match &call.target_unit {
            Some(t) => api.operations.iter().filter(|o| &o.unit == t).cloned().collect(),
            None => vec![],
        };
        let pool = if call.target_unit.is_some() { &candidates } else { &api.operations };
        if let Some(op) = client::match_operation(&call, pool) {
            call.target_unit = Some(op.unit.clone());
            call.operation = Some(op.id.clone());
            // What the caller reads out of the response, against what the operation
            // declares. The caller's expectation is either a type it parses into
            // (its own DTO, in any language) or the field names it reads directly.
            let declared = op
                .response
                .as_ref()
                .and_then(|r| r.model.as_ref())
                .and_then(|id| id.split_once(':'))
                .and_then(|(u, n)| defs.get(&(u.to_string(), n.to_string())))
                .filter(|m| !m.fields.is_empty());
            if let Some(declared) = declared {
                let caller_model = c
                    .expects
                    .as_deref()
                    .map(|e| unwrap_type(e).0)
                    .filter(|inner| !(*inner == declared.name && call.unit == declared.unit))
                    .and_then(|inner| defs.get(&(call.unit.clone(), inner)));
                let mut reads: Vec<&str> =
                    caller_model.iter().flat_map(|m| &m.fields).map(|f| f.name.as_str()).collect();
                reads.extend(c.expects_fields.iter().map(String::as_str));
                let have: BTreeSet<&str> = declared.fields.iter().map(|f| f.name.as_str()).collect();
                let missing: BTreeSet<&str> = reads.into_iter().filter(|n| !have.contains(n)).collect();
                call.drift = missing.into_iter().map(str::to_string).collect();
            }
        }
        if !api.client_calls.iter().any(|x: &ClientCall| {
            x.unit == call.unit
                && x.method == call.method
                && x.path == call.path
                && x.evidence.file_path == call.evidence.file_path
                && x.evidence.start_line == call.evidence.start_line
        }) {
            api.client_calls.push(call);
        }
    }

    api.models = defs.into_values().filter(|m| wanted.contains(&m.id)).collect();
    api.operations
        .sort_by(|a, b| (&a.unit, &a.path, method_rank(&a.method)).cmp(&(&b.unit, &b.path, method_rank(&b.method))));
    api.client_calls.sort_by(|a, b| {
        (&a.unit, &a.evidence.file_path, a.evidence.start_line).cmp(&(
            &b.unit,
            &b.evidence.file_path,
            b.evidence.start_line,
        ))
    });
    api.excluded.sort_by(|a, b| a.operation.cmp(&b.operation));
    api
}

fn method_rank(m: &str) -> u8 {
    match m {
        "GET" => 0,
        "POST" => 1,
        "PUT" => 2,
        "PATCH" => 3,
        "DELETE" => 4,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwraps_types() {
        assert_eq!(unwrap_type("list[ItemPublic]"), ("ItemPublic".into(), true));
        assert_eq!(unwrap_type("Json<Vec<DailyTotal>>"), ("DailyTotal".into(), true));
        assert_eq!(unwrap_type("Result<Json<Item>, AppError>"), ("Item".into(), false));
        assert_eq!(unwrap_type("CartItem[]"), ("CartItem".into(), true));
        assert_eq!(unwrap_type("[]*models.Order"), ("Order".into(), true));
        assert_eq!(unwrap_type("Optional[schemas.User]"), ("User".into(), false));
        assert_eq!(unwrap_type("Promise<CheckoutResult>"), ("CheckoutResult".into(), false));
        assert_eq!(unwrap_type("User | None"), ("User".into(), false));
    }

    #[test]
    fn probes_are_excluded() {
        assert!(excluded_reason("/healthz").is_some());
        // A marker anywhere in the path, not only at the end.
        assert_eq!(excluded_reason("/v2/metrics/bucket"), Some("metrics endpoint"));
        assert_eq!(excluded_reason("/debug/vars"), Some("debug / profiling endpoint"));
        assert_eq!(excluded_reason("/debug/pprof/heap"), Some("debug / profiling endpoint"));
        // …but a resource that merely reads like one is still functionality.
        // A probe answers for the service, not for one resource.
        assert_eq!(excluded_reason("/patients/{id}/health"), None);
        assert_eq!(excluded_reason("/api/v1/health"), Some("health / liveness probe"));
        // …and the same holds for the markers that match anywhere in the path.
        assert_eq!(excluded_reason("/api/v1/dashboards/{id}/metrics"), None);
        assert_eq!(excluded_reason("/devices/:id/debug"), None);
        assert!(excluded_reason("/api/v1/utils/health-check").is_some());
        assert!(excluded_reason("/api/v1/healthy-items").is_none());
        assert!(excluded_reason("/docs/oauth2-redirect").is_some());
        assert!(excluded_reason("/items").is_none());
    }
}
