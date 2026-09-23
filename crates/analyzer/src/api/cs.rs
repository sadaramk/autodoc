//! C# HTTP contracts: ASP.NET Core attribute-routed controllers and the
//! minimal-API `Map*` calls.
//!
//! Routes, parameters and validation come from the attribute facts the
//! extractor already collected; types come from the signature text, because C#
//! writes them as `Type name = default` with nullability as a `?` suffix.

use std::collections::BTreeSet;

use super::text::{self, matching, split_args, split_top, Src};
use super::{
    add_auth, add_error, add_param, fill_path_params, new_op, rule, Draft, Field, Harvest, Loaded, Model, Operation,
    Param, Requirement, SymbolRef,
};
use crate::extract::{Annotation, Symbol, SymbolKind};
use crate::lang::Language;
use crate::scan::EvidenceRef;

const VERBS: &[(&str, &str)] = &[
    ("HttpGet", "GET"),
    ("HttpPost", "POST"),
    ("HttpPut", "PUT"),
    ("HttpDelete", "DELETE"),
    ("HttpPatch", "PATCH"),
    ("HttpHead", "HEAD"),
    ("HttpOptions", "OPTIONS"),
];

/// `app.MapGet("/x", handler)` — minimal APIs, where the verb is in the method name.
const MAP_VERBS: &[(&str, &str)] =
    &[("MapGet", "GET"), ("MapPost", "POST"), ("MapPut", "PUT"), ("MapDelete", "DELETE"), ("MapPatch", "PATCH")];

/// `StatusCodes.Status201Created`, `HttpStatusCode.Created`, a bare `201`.
fn status_of(expr: &str) -> Option<u16> {
    let t = expr.trim().rsplit('.').next().unwrap_or(expr).trim();
    if let Some(rest) = t.strip_prefix("Status") {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(n) = digits.parse() {
            return Some(n);
        }
    }
    if let Ok(n) = t.parse::<u16>() {
        return (100..600).contains(&n).then_some(n);
    }
    let norm: String = t.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_uppercase();
    Some(match norm.as_str() {
        "OK" => 200,
        "CREATED" => 201,
        "ACCEPTED" => 202,
        "NOCONTENT" => 204,
        "BADREQUEST" => 400,
        "UNAUTHORIZED" => 401,
        "FORBIDDEN" => 403,
        "NOTFOUND" => 404,
        "CONFLICT" => 409,
        "UNPROCESSABLEENTITY" => 422,
        "TOOMANYREQUESTS" => 429,
        "INTERNALSERVERERROR" => 500,
        "SERVICEUNAVAILABLE" => 503,
        _ => return None,
    })
}

/// `ControllerBase` helpers that answer with a status. `Ok`/`Created` describe
/// success; the rest are errors the handler can return.
fn result_helper(name: &str) -> Option<(u16, bool)> {
    Some(match name {
        "Ok" => (200, true),
        "Created" | "CreatedAtAction" | "CreatedAtRoute" => (201, true),
        "Accepted" | "AcceptedAtAction" => (202, true),
        "NoContent" => (204, true),
        "BadRequest" | "ValidationProblem" => (400, false),
        "Unauthorized" | "Challenge" => (401, false),
        "Forbid" => (403, false),
        "NotFound" => (404, false),
        "Conflict" => (409, false),
        "UnprocessableEntity" => (422, false),
        _ => return None,
    })
}

/// Exceptions whose type names a status, the way `NotFoundException` does on the JVM.
fn known_exception(name: &str) -> Option<u16> {
    Some(match name {
        "KeyNotFoundException" | "NotFoundException" => 404,
        "ArgumentException" | "ArgumentNullException" | "ValidationException" | "BadRequestException" => 400,
        "UnauthorizedAccessException" => 403,
        "InvalidOperationException" | "DbUpdateConcurrencyException" => 409,
        "NotImplementedException" => 501,
        _ => return None,
    })
}

/// Types a handler receives from the framework rather than from the request.
fn injected(ty: &str) -> bool {
    let t = strip_nullable(&super::unwrap_type(ty).0);
    matches!(
        t.as_str(),
        "HttpContext"
            | "HttpRequest"
            | "HttpResponse"
            | "CancellationToken"
            | "ClaimsPrincipal"
            | "IFormFile"
            | "IUrlHelper"
            | "ILogger"
    ) || t.starts_with("ILogger<")
}

fn strip_nullable(t: &str) -> String {
    t.trim().trim_end_matches('?').trim().to_string()
}

/// `Task<ActionResult<Order>>` is a wrapper three deep around the payload. Only
/// the innermost type is what the client actually receives.
fn unwrap_response(ty: &str) -> Option<String> {
    let mut t = strip_nullable(ty);
    loop {
        let bare = t.split('<').next().unwrap_or(&t).rsplit('.').next().unwrap_or(&t).trim().to_string();
        match bare.as_str() {
            "void" | "Task" | "ValueTask" | "IActionResult" | "ActionResult" | "IResult" | "Results"
                if !t.contains('<') =>
            {
                return None;
            }
            "Task" | "ValueTask" | "ActionResult" | "ActionResult`1" | "Ok" | "JsonResult" => {
                let inner = generic_arg(&t)?;
                t = strip_nullable(&inner);
            }
            _ => return (!t.is_empty()).then_some(t),
        }
    }
}

/// The single type argument of `Wrapper<Inner>`, or `None` if there is no `<…>`.
fn generic_arg(t: &str) -> Option<String> {
    let open = t.find('<')?;
    let close = t.rfind('>')?;
    (close > open + 1).then(|| t[open + 1..close].trim().to_string())
}

/// One `[Attr] Type name = default` parameter of a C# method. Attributes keep
/// their arguments, because `[FromQuery(Name = "ref")]` renames the parameter.
struct CParam {
    attrs: Vec<(String, String)>,
    type_name: String,
    name: String,
    has_default: bool,
}

impl CParam {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(n, _)| n == name).map(|(_, a)| a.as_str())
    }

    /// The name the parameter is bound by on the wire, when a binding attribute
    /// overrides the C# identifier.
    fn wire_name(&self) -> Option<String> {
        self.attrs.iter().filter(|(n, _)| binding(n).is_some()).find_map(|(_, args)| named_arg(args, "Name"))
    }
}

/// `Name = "ref"` inside an attribute argument list.
fn named_arg(args: &str, key: &str) -> Option<String> {
    split_top(args, &[','])
        .into_iter()
        .filter_map(|p| p.split_once('='))
        .find(|(k, _)| k.trim() == key)
        .and_then(|(_, v)| text::string_lit(v.trim()))
}

/// Where `[FromQuery]` and friends bind from.
fn binding(attr: &str) -> Option<&'static str> {
    Some(match attr {
        "FromQuery" => "query",
        "FromRoute" => "path",
        "FromBody" => "body",
        "FromHeader" => "header",
        "FromForm" => "form",
        _ => return None,
    })
}

/// A type bound from the query string by default. Anything else that is not a
/// primitive is model-bound from the body.
fn scalar(ty: &str) -> bool {
    let t = strip_nullable(&super::unwrap_type(ty).0);
    matches!(
        t.as_str(),
        "string"
            | "int"
            | "uint"
            | "long"
            | "ulong"
            | "short"
            | "byte"
            | "bool"
            | "double"
            | "float"
            | "decimal"
            | "char"
            | "Guid"
            | "DateTime"
            | "DateTimeOffset"
            | "DateOnly"
            | "TimeSpan"
            | "String"
            | "Int32"
            | "Int64"
    ) || t.chars().next().is_some_and(|c| c.is_lowercase())
}

/// Where the method's own name is written in its declaration. A method whose
/// name also appears inside its return type — `List` in
/// `Task<ActionResult<List<Product>>>` — has two matches, and only the one
/// immediately followed by `(` opens the parameter list.
fn declaration_at(code: &str, span: (usize, usize), name: &str) -> Option<usize> {
    let (start, end) = span;
    text::find_word(&code[start..end], name)
        .into_iter()
        .map(|i| start + i)
        .find(|at| code[at + name.len()..end].trim_start().starts_with('('))
}

/// Parameters of the method declared inside `span`, read from the signature text.
fn parse_params(src: &Src, span: (usize, usize), name: &str) -> Vec<CParam> {
    let (start, end) = span;
    let code = &src.code;
    let Some(at) = declaration_at(code, (start, end), name) else { return vec![] };
    let Some(open) = code[at..end].find('(').map(|i| at + i) else { return vec![] };
    let Some(close) = matching(code, open) else { return vec![] };
    split_args(code, open + 1, close).into_iter().filter_map(|(s, e)| parse_param(&src.text[s..e])).collect()
}

fn parse_param(raw: &str) -> Option<CParam> {
    let mut rest = raw.trim();
    let mut attrs = Vec::new();
    // `[FromQuery(Name = "q")] [Required] int limit = 20`
    while let Some(inner_end) = rest.strip_prefix('[').and_then(|_| rest.find(']')) {
        let inner = &rest[1..inner_end];
        for a in split_top(inner, &[',']) {
            let a = a.trim();
            let n = a.split('(').next().unwrap_or(a).trim();
            let args =
                a.find('(').and_then(|o| a.rfind(')').map(|c| a[o + 1..c].trim().to_string())).unwrap_or_default();
            if !n.is_empty() {
                attrs.push((n.to_string(), args));
            }
        }
        rest = rest[inner_end + 1..].trim();
    }
    let (decl, has_default) = match split_top(rest, &['=']).first() {
        Some(d) => (d.trim().to_string(), rest.contains('=')),
        None => (rest.to_string(), false),
    };
    // Modifiers come before the type; the name is the last token.
    let mut tokens: Vec<&str> = decl.split_whitespace().collect();
    while tokens.first().is_some_and(|t| matches!(*t, "this" | "ref" | "out" | "in" | "params" | "readonly")) {
        tokens.remove(0);
    }
    let name = tokens.pop()?.trim().to_string();
    let type_name = tokens.join(" ").trim().to_string();
    (!name.is_empty() && !type_name.is_empty()).then_some(CParam { attrs, type_name, name, has_default })
}

/// Declared return type: everything between the modifiers and the method name.
fn return_type(src: &Src, span: (usize, usize), name: &str) -> Option<String> {
    let (start, end) = span;
    let code = &src.code;
    let at = declaration_at(code, (start, end), name)?;
    let head = src.text[start..at].trim_end();
    // `public async Task<ActionResult<T>> ` — the type is the last token, but a
    // generic argument may contain spaces, so scanning back over balanced
    // brackets is the only safe way to find where it starts.
    let bytes = head.as_bytes();
    let mut depth = 0i32;
    let mut i = bytes.len();
    while i > 0 {
        let c = bytes[i - 1];
        match c {
            b'>' => depth += 1,
            b'<' => depth -= 1,
            b' ' | b'\t' | b'\n' if depth == 0 => break,
            _ => {}
        }
        i -= 1;
    }
    let ty = head[i..].trim();
    (!ty.is_empty() && ty != "async").then(|| ty.to_string())
}

/// `[Route("api/[controller]")]` expands its tokens from the declaration it is on.
fn expand_tokens(path: &str, class: &str, action: &str) -> String {
    let controller = class.strip_suffix("Controller").unwrap_or(class);
    path.replace("[controller]", controller).replace("[action]", action).replace("[area]", "")
}

fn ann_arg(a: &Annotation, key: &str) -> Option<String> {
    for part in split_top(&a.arguments, &[',']) {
        let part = part.trim();
        match part.split_once('=') {
            Some((k, v)) if k.trim() == key => {
                return text::string_lit(v.trim()).or_else(|| Some(v.trim().to_string()))
            }
            Some(_) => {}
            // A positional argument answers to the empty key.
            None if key.is_empty() => return text::string_lit(part),
            None => {}
        }
    }
    None
}

/// The first positional string of an attribute: `[Route("api/x")]`, `[HttpGet("{id}")]`.
fn ann_path(a: &Annotation) -> Option<String> {
    ann_arg(a, "")
}

struct Index<'a> {
    files: Vec<&'a Loaded<'a>>,
    root: std::path::PathBuf,
}

impl<'a> Index<'a> {
    fn anns(&self, fi: usize, kinds: &[&str], target: &str, owner: Option<&str>) -> Vec<&'a Annotation> {
        self.files[fi]
            .facts
            .annotations
            .iter()
            .filter(|a| kinds.contains(&a.target_kind.as_str()) && a.target == target)
            .filter(|a| owner.is_none_or(|o| a.owner.as_deref() == Some(o)))
            .collect()
    }

    fn find<'b>(&self, anns: &[&'b Annotation], name: &str) -> Option<&'b Annotation> {
        anns.iter().copied().find(|a| a.name == name)
    }

    fn ev(&self, fi: usize, line: u32) -> EvidenceRef {
        crate::source::line_ref(self.files[fi].path(), line)
    }
}

pub(crate) fn extract(index: &crate::source::SourceIndex, files: &[Loaded], h: &mut Harvest) {
    let cs: Vec<&Loaded> = files.iter().filter(|f| f.lang == Language::CSharp).collect();
    if cs.is_empty() {
        return;
    }
    let idx = Index { files: cs, root: index.root.clone() };
    let mut models = Models::default();
    let mut ops: Vec<Draft> = vec![];
    for fi in 0..idx.files.len() {
        controllers(&idx, fi, &mut models, &mut ops);
        minimal_api(&idx, fi, &mut models, &mut ops);
    }
    h.ops.extend(ops);
    models.emit(&idx, h);
}

/// `app.UsePathBase("/api")` shifts every route under a prefix.
fn path_base(idx: &Index, unit: &str) -> String {
    for f in &idx.files {
        if f.unit != unit {
            continue;
        }
        for at in text::find_word(&f.src.code, "UsePathBase") {
            if let Some(open) = f.src.code[at..].find('(').map(|i| at + i) {
                if let Some(close) = matching(&f.src.code, open) {
                    if let Some(p) = text::string_lit(&f.src.text[open + 1..close]) {
                        return p;
                    }
                }
            }
        }
    }
    let _ = &idx.root;
    String::new()
}

fn class_symbols<'a>(f: &'a Loaded) -> impl Iterator<Item = &'a Symbol> {
    f.facts.symbols.iter().filter(|s| matches!(s.kind, SymbolKind::Class | SymbolKind::Struct))
}

fn controllers(idx: &Index, fi: usize, models: &mut Models, ops: &mut Vec<Draft>) {
    let f = idx.files[fi];
    for class in class_symbols(f) {
        let anns = idx.anns(fi, &["class", "struct", "record"], &class.name, None);
        let api_controller = anns.iter().any(|a| matches!(a.name.as_str(), "ApiController" | "Controller"));
        let class_route = idx.find(&anns, "Route").or_else(|| idx.find(&anns, "RoutePrefix"));
        // A `…Controller` base class with no attribute still routes by convention,
        // but only an attribute makes the path knowable.
        if !api_controller && class_route.is_none() {
            continue;
        }
        let base_raw = class_route.and_then(ann_path).unwrap_or_default();
        let base = text::join_path(&path_base(idx, f.unit), &expand_tokens(&base_raw, &class.name, ""));
        let class_auth = auth_requirements(idx, fi, &anns);
        let (cs, ce) = f.span(class);
        for m in f.facts.symbols.iter().filter(|s| s.kind == SymbolKind::Method) {
            let (ms, me) = f.span(m);
            if ms < cs || me > ce {
                continue;
            }
            let manns = idx.anns(fi, &["method"], &m.name, Some(&class.name));
            let verbs: Vec<(&str, String)> = VERBS
                .iter()
                .filter_map(|(attr, verb)| idx.find(&manns, attr).map(|a| (*verb, ann_path(a).unwrap_or_default())))
                .collect();
            if verbs.is_empty() {
                continue;
            }
            let params = parse_params(&f.src, (ms, me), &m.name);
            let body_text = f.src.slice(ms, me).to_string();
            for (verb, path_raw) in verbs {
                let expanded = expand_tokens(&path_raw, &class.name, &m.name);
                let full = text::join_path(&base, &expanded);
                let handler = SymbolRef { name: m.name.clone(), evidence: f.src.ev_range(ms, me, Some(&m.name)) };
                let evidence = idx.ev(fi, manns.first().map(|a| a.line).unwrap_or(m.start_line));
                let mut op = new_op(f.unit, "asp.net-core", verb, full, handler, evidence);
                op.summary = m.doc.as_deref().and_then(summary_of);
                describe(idx, fi, m, &manns, &params, &body_text, models, &mut op);
                for r in class_auth.iter().chain(auth_requirements(idx, fi, &manns).iter()) {
                    add_auth(&mut op, r.clone());
                }
                // `[AllowAnonymous]` on the action overrides the controller's `[Authorize]`.
                if manns.iter().any(|a| a.name == "AllowAnonymous") {
                    op.auth.clear();
                }
                fill_path_params(&mut op);
                let request_declared = op.request_body.is_some();
                let response_declared = op.response.is_some();
                ops.push(Draft { op, request_declared, response_declared });
            }
        }
    }
}

/// `/// <summary>Lists orders.</summary>` — the prose is inside the tag.
fn summary_of(doc: &str) -> Option<String> {
    let stripped =
        doc.replace("<summary>", " ").replace("</summary>", " ").replace("<remarks>", " ").replace("</remarks>", " ");
    let first = stripped.split('<').next().unwrap_or(&stripped).trim().to_string();
    text::first_sentence(if first.is_empty() { &stripped } else { &first })
}

#[allow(clippy::too_many_arguments)]
fn describe(
    idx: &Index,
    fi: usize,
    m: &Symbol,
    manns: &[&Annotation],
    params: &[CParam],
    body: &str,
    models: &mut Models,
    op: &mut Operation,
) {
    let f = idx.files[fi];
    let route_params: BTreeSet<String> = text::path_params(&op.path).into_iter().collect();
    for p in params {
        if p.attr("FromServices").is_some() || injected(&p.type_name) {
            continue;
        }
        let ev = f.src.ev_range(f.src.line_start(m.start_line), f.src.line_end(m.end_line), Some(&p.name));
        let declared = p.attrs.iter().find_map(|(a, _)| binding(a));
        let location = declared.unwrap_or(if route_params.contains(&p.name) {
            "path"
        } else if scalar(&p.type_name) {
            "query"
        } else {
            "body"
        });
        if location == "body" {
            models.want(f.unit, &p.type_name);
            op.request_body = Some(super::type_ref(&p.type_name));
            continue;
        }
        // `[FromQuery(Name = "ref")]` renames the wire parameter.
        let wire = p.wire_name().unwrap_or_else(|| p.name.clone());
        let nullable = p.type_name.trim_end().ends_with('?');
        let mut param = Param {
            name: wire.clone(),
            code_name: (wire != p.name).then(|| p.name.clone()),
            location: location.to_string(),
            type_name: strip_nullable(&p.type_name),
            required: location == "path" || p.attr("Required").is_some() || (!p.has_default && !nullable),
            rules: vec![],
            evidence: ev.clone(),
        };
        for r in validation_rules(&p.attrs, &ev) {
            param.rules.push(r);
        }
        add_param(op, param);
    }
    // `[ProducesResponseType(StatusCodes.Status201Created, Type = typeof(Order))]`
    for a in manns.iter().filter(|a| a.name == "ProducesResponseType") {
        let ev = idx.ev(fi, a.line);
        let status = ann_arg(a, "StatusCode").or_else(|| ann_arg(a, "")).and_then(|v| status_of(&v));
        match status {
            Some(s) if (200..300).contains(&s) => op.success_status = op.success_status.or(Some(s)),
            Some(s) => add_error(op, Some(s), None, ev),
            None => {}
        }
    }
    if let Some(s) = idx.find(manns, "ProducesDefaultResponseType").and(None::<u16>) {
        op.success_status = Some(s);
    }
    // Declared return type first; the helpers in the body only fill the gaps.
    let declared = return_type(&f.src, f.span(m), &m.name).as_deref().and_then(unwrap_response);
    if let Some(ty) = &declared {
        models.want(f.unit, ty);
        op.response = Some(super::type_ref(ty));
    }
    for (name, (status, success)) in
        result_helper_hits(body).into_iter().filter_map(|n| result_helper(&n).map(|r| (n, r)))
    {
        let ev = f.src.ev_range(f.src.line_start(m.start_line), f.src.line_end(m.end_line), Some(&name));
        if success {
            if op.success_status.is_none() && status != 200 {
                op.success_status = Some(status);
            }
        } else {
            add_error(op, Some(status), None, ev);
        }
    }
    for name in thrown(body) {
        if let Some(status) = known_exception(&name) {
            let ev = f.src.ev_range(f.src.line_start(m.start_line), f.src.line_end(m.end_line), Some(&name));
            add_error(op, Some(status), None, ev);
        }
    }
    if declared.is_none() && op.success_status.is_none() && body.contains("NoContent") {
        op.success_status = Some(204);
    }
}

/// Names called as `Name(` in a handler body — `return NotFound();`.
fn result_helper_hits(body: &str) -> Vec<String> {
    let mut out = BTreeSet::new();
    for (name, _) in [
        "Ok",
        "Created",
        "CreatedAtAction",
        "CreatedAtRoute",
        "Accepted",
        "AcceptedAtAction",
        "NoContent",
        "BadRequest",
        "ValidationProblem",
        "Unauthorized",
        "Challenge",
        "Forbid",
        "NotFound",
        "Conflict",
        "UnprocessableEntity",
    ]
    .iter()
    .map(|n| (*n, ()))
    {
        for at in text::find_word(body, name) {
            if body[at + name.len()..].trim_start().starts_with('(') {
                out.insert(name.to_string());
                break;
            }
        }
    }
    out.into_iter().collect()
}

/// `throw new KeyNotFoundException(…)` — the exception type.
fn thrown(body: &str) -> Vec<String> {
    let mut out = BTreeSet::new();
    for at in text::find_word(body, "throw") {
        let rest = body[at + 5..].trim_start();
        let rest = rest.strip_prefix("new").map(str::trim_start).unwrap_or(rest);
        let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if !name.is_empty() {
            out.insert(name);
        }
    }
    out.into_iter().collect()
}

/// `[Authorize]`, `[Authorize(Roles = "admin")]`, `[Authorize(Policy = "x")]`.
fn auth_requirements(idx: &Index, fi: usize, anns: &[&Annotation]) -> Vec<Requirement> {
    let mut out = Vec::new();
    for a in anns.iter().filter(|a| a.name == "Authorize") {
        let (kind, detail) = match (ann_arg(a, "Roles"), ann_arg(a, "Policy"), ann_arg(a, "AuthenticationSchemes")) {
            (Some(r), _, _) => ("role", r),
            (_, Some(p), _) => ("role", format!("policy {p}")),
            (_, _, Some(s)) => ("session", format!("scheme {s}")),
            _ => ("session", "authenticated".into()),
        };
        out.push(Requirement { kind: kind.into(), detail, evidence: idx.ev(fi, a.line) });
    }
    out
}

/// Data-annotation attributes as validation rules.
fn validation_rules(attrs: &[(String, String)], ev: &EvidenceRef) -> Vec<super::Rule> {
    attrs.iter().filter_map(|(a, args)| annotation_rule(a, args, ev)).collect()
}

/// `app.MapGet("/healthz", () => …)` and its siblings.
fn minimal_api(idx: &Index, fi: usize, models: &mut Models, ops: &mut Vec<Draft>) {
    let f = idx.files[fi];
    let code = &f.src.code;
    let base = path_base(idx, f.unit);
    for (attr, verb) in MAP_VERBS {
        for at in text::find_word(code, attr) {
            let Some(open) = code[at..].find('(').map(|i| at + i) else { continue };
            if open != at + attr.len() {
                continue;
            }
            let Some(close) = matching(code, open) else { continue };
            let args = split_args(code, open + 1, close);
            let Some(&(s, e)) = args.first() else { continue };
            let Some(path) = text::string_lit(&f.src.text[s..e]) else { continue };
            // ASP.NET Core takes a route relative to the app root, so
            // `MapGet("api/items", …)` is as ordinary as `MapGet("/api/items", …)`
            // — eShopOnWeb writes every one of its endpoints without the slash.
            // What must still be rejected is a string that is not a route at all:
            // a tag, a group name, a URL somewhere else.
            if path.contains("://") || path.trim().is_empty() {
                continue;
            }
            let full = text::join_path(&base, &path);
            let line = f.src.line(at);
            let ev = idx.ev(fi, line);
            // A lambda has no name; the route itself identifies the handler.
            let handler = SymbolRef { name: attr.to_string(), evidence: ev.clone() };
            let mut op = new_op(f.unit, "asp.net-core", verb, full, handler, ev);
            let tail = &f.src.text[close.min(f.src.text.len())..(close + 240).min(f.src.text.len())];
            if let Some(s) = ["Results.Created", "TypedResults.Created"].iter().find(|c| tail.contains(**c)) {
                let _ = s;
                op.success_status = Some(201);
            }
            // Minimal APIs declare their contract in two places, and reading
            // neither is why every operation in eShopOnWeb came out `Opaque`
            // (#48): `.Produces<T>()` names the response, and the lambda's first
            // non-injected parameter is the body.
            let stmt = &f.src.text[close.min(f.src.text.len())..statement_end(&f.src.code, close)];
            let mut response_declared = false;
            if let Some(ty) = produces_type(stmt) {
                models.want(f.unit, &ty);
                op.response = Some(super::type_ref(&ty));
                response_declared = true;
            }
            let mut request_declared = false;
            if matches!(verb, &"POST" | &"PUT" | &"PATCH") {
                if let Some(ty) = lambda_body_type(&f.src.text, args.get(1).copied()) {
                    models.want(f.unit, &ty);
                    op.request_body = Some(super::type_ref(&ty));
                    request_declared = true;
                }
            }
            fill_path_params(&mut op);
            ops.push(Draft { op, request_declared, response_declared });
        }
    }
}

/// End of the statement a route registration sits in.
///
/// `.Produces<T>()` is chained after the `Map…` call and before the `;`, and a
/// fixed lookahead would either miss a long chain or run into the next endpoint.
fn statement_end(code: &str, from: usize) -> usize {
    code[from..].find(';').map(|i| from + i).unwrap_or(code.len()).min(code.len())
}

/// The type in `.Produces<CreateCatalogItemResponse>()`, which is ASP.NET Core's
/// own machine-readable declaration of what an endpoint returns.
///
/// `Produces<T>(StatusCodes.Status201Created)` and bare `Produces(404)` both
/// occur; only the generic form names a type, and a non-2xx one describes an
/// error rather than the success shape.
fn produces_type(stmt: &str) -> Option<String> {
    for at in text::find_word(stmt, "Produces") {
        let rest = &stmt[at + "Produces".len()..];
        let Some(inner) = rest.strip_prefix('<') else { continue };
        let Some(end) = inner.find('>') else { continue };
        let ty = inner[..end].trim();
        // A status argument after the type: `.Produces<T>(StatusCodes.Status404NotFound)`
        // is about a failure, and taking it as the response would misreport it.
        let after = inner[end + 1..].trim_start();
        let failure = after.strip_prefix('(').map(|a| a.contains("Status4") || a.contains("Status5")).unwrap_or(false);
        if !ty.is_empty() && !failure && ty.chars().next().is_some_and(char::is_uppercase) {
            return Some(ty.to_string());
        }
    }
    None
}

/// The request body of a minimal-API lambda: its first parameter that is not
/// something the framework injects.
///
/// `[Authorize(…)] async (CreateCatalogItemRequest request, IRepository<CatalogItem> repo) => …`
/// is the ordinary shape — attributes and `async` before the list, services
/// after the body. `injected` already knows the framework's own types;
/// interfaces are dependencies by convention, which is what `I` followed by an
/// upper-case letter means in C#.
fn lambda_body_type(text: &str, arg: Option<(usize, usize)>) -> Option<String> {
    let (s, e) = arg?;
    let lambda = text.get(s..e)?;
    let arrow = lambda.find("=>")?;
    let head = &lambda[..arrow];
    let open = head.rfind('(')?;
    let close = head[open..].find(')')? + open;
    for part in head[open + 1..close].split(',') {
        let part = part.trim();
        // `[FromBody] Thing thing` and `Thing thing` alike: the type is the
        // second-to-last word, the name the last.
        let words: Vec<&str> = part.rsplitn(2, ' ').collect();
        if words.len() != 2 {
            continue;
        }
        let ty = words[1].trim().rsplit(']').next().unwrap_or(words[1]).trim();
        if ty.is_empty() || injected(ty) || scalar(ty) {
            continue;
        }
        let bare = super::unwrap_type(ty).0;
        let bare = bare.rsplit('.').next().unwrap_or(&bare);
        let mut cs = bare.chars();
        let dependency = cs.next() == Some('I') && cs.next().is_some_and(char::is_uppercase);
        if dependency || !bare.chars().next().is_some_and(char::is_uppercase) {
            continue;
        }
        return Some(ty.to_string());
    }
    None
}

#[derive(Default)]
struct Models {
    wanted: BTreeSet<(String, String)>,
}

impl Models {
    fn want(&mut self, unit: &str, type_name: &str) {
        let inner = strip_nullable(&super::unwrap_type(type_name).0);
        let inner = inner.rsplit('.').next().unwrap_or(&inner).to_string();
        if inner.chars().next().is_some_and(char::is_uppercase) && !injected(&inner) {
            self.wanted.insert((unit.to_string(), inner));
        }
    }

    fn emit(&mut self, idx: &Index, h: &mut Harvest) {
        let mut done: BTreeSet<(String, String)> = BTreeSet::new();
        while let Some(key) = self.wanted.iter().find(|k| !done.contains(*k)).cloned() {
            done.insert(key.clone());
            let (unit, name) = key;
            for fi in 0..idx.files.len() {
                let f = idx.files[fi];
                if f.unit != unit {
                    continue;
                }
                let Some(sym) = f
                    .facts
                    .symbols
                    .iter()
                    .find(|s| s.name == name && matches!(s.kind, SymbolKind::Class | SymbolKind::Struct))
                else {
                    continue;
                };
                let fields = model_fields(idx, fi, sym, &mut self.wanted, &unit);
                if fields.is_empty() {
                    continue;
                }
                let (ms, me) = f.span(sym);
                h.model(Model {
                    id: format!("{unit}:{name}"),
                    unit: unit.clone(),
                    name: name.clone(),
                    fields,
                    doc: sym.doc.as_deref().and_then(summary_of),
                    evidence: f.src.ev_range(ms, me, Some(&name)),
                });
                break;
            }
        }
    }
}

/// `public string Name { get; set; }` properties of a class, with their
/// data annotations as rules. Positional records are read from the parameter
/// list instead, since they declare their members there.
fn model_fields(
    idx: &Index,
    fi: usize,
    sym: &Symbol,
    wanted: &mut BTreeSet<(String, String)>,
    unit: &str,
) -> Vec<Field> {
    let f = idx.files[fi];
    let (cs, ce) = f.span(sym);
    let mut out = Vec::new();
    for a in f.facts.annotations.iter().filter(|a| a.target_kind == "field") {
        let _ = a;
    }
    for (i, raw) in f.src.text[cs..ce].lines().enumerate() {
        let line = f.src.line(cs) + i as u32;
        let t = raw.trim();
        // A property is `<modifiers> <Type> <Name> { get; … }`; the accessor list
        // is what tells it apart from a field or a method.
        let Some(brace) = t.find('{') else { continue };
        if !t[brace..].contains("get") {
            continue;
        }
        let decl = t[..brace].trim();
        let mut tokens: Vec<&str> = decl.split_whitespace().collect();
        while tokens.first().is_some_and(|x| {
            matches!(
                *x,
                "public"
                    | "private"
                    | "protected"
                    | "internal"
                    | "virtual"
                    | "override"
                    | "required"
                    | "static"
                    | "readonly"
                    | "new"
                    | "abstract"
                    | "sealed"
            )
        }) {
            tokens.remove(0);
        }
        let Some(name) = tokens.pop() else { continue };
        let type_name = tokens.join(" ");
        if type_name.is_empty() || !name.chars().next().is_some_and(char::is_alphabetic) {
            continue;
        }
        let attrs = property_attrs(idx, fi, sym, name);
        let ev = crate::source::line_ref(f.path(), line);
        let nullable = type_name.ends_with('?');
        let mut field = Field {
            name: name.to_string(),
            code_name: None,
            type_name: strip_nullable(&type_name),
            required: !nullable || attrs.iter().any(|(a, _)| a == "Required"),
            rules: vec![],
            doc: None,
            model: None,
            evidence: ev.clone(),
        };
        for (a, args) in &attrs {
            if let Some(r) = annotation_rule(a, args, &ev) {
                field.rules.push(r);
            }
            // `[JsonPropertyName("supplier_email")]` renames the wire field.
            if a == "JsonPropertyName" || a == "JsonProperty" {
                if let Some(wire) = text::string_lit(args.trim()) {
                    field.code_name = Some(name.to_string());
                    field.name = wire;
                }
            }
        }
        let inner = strip_nullable(&super::unwrap_type(&type_name).0);
        if inner.chars().next().is_some_and(char::is_uppercase) && !scalar(&inner) {
            wanted.insert((unit.to_string(), inner));
        }
        out.push(field);
    }
    out
}

/// Attributes recorded against a property of this class.
fn property_attrs(idx: &Index, fi: usize, class: &Symbol, prop: &str) -> Vec<(String, String)> {
    let f = idx.files[fi];
    let (cs, ce) = f.span(class);
    f.facts
        .annotations
        .iter()
        .filter(|a| a.target_kind == "field" && a.target == prop)
        .filter(|a| {
            let s = f.src.line_start(a.target_start);
            s >= cs && s <= ce
        })
        .map(|a| (a.name.clone(), a.arguments.clone()))
        .collect()
}

fn annotation_rule(name: &str, args: &str, ev: &EvidenceRef) -> Option<super::Rule> {
    let first = split_top(args, &[',']).first().map(|s| s.trim().to_string()).unwrap_or_default();
    let named = |key: &str| {
        split_top(args, &[','])
            .into_iter()
            .filter_map(|p| p.split_once('='))
            .find(|(k, _)| k.trim() == key)
            .map(|(_, v)| v.trim().to_string())
    };
    Some(match name {
        "Required" => rule("required", "required", ev),
        "EmailAddress" => rule("format: email", "format", ev),
        "Url" => rule("format: url", "format", ev),
        "Phone" => rule("format: phone", "format", ev),
        "CreditCard" => rule("format: credit-card", "format", ev),
        "MaxLength" => rule(format!("max length {first}"), "length", ev),
        "MinLength" => rule(format!("min length {first}"), "length", ev),
        "StringLength" => match named("MinimumLength") {
            Some(min) => rule(format!("length {min}–{first}"), "length", ev),
            None => rule(format!("max length {first}"), "length", ev),
        },
        "Range" => {
            let parts = split_top(args, &[',']);
            let (lo, hi) = (parts.first()?.trim(), parts.get(1)?.trim());
            rule(format!("range {lo}–{hi}"), "range", ev)
        }
        "RegularExpression" => rule(format!("pattern {first}"), "pattern", ev),
        "Key" => rule("primary key", "key", ev),
        _ => return None,
    })
}
