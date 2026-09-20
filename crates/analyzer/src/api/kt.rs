//! Kotlin HTTP contracts: Spring MVC / WebFlux annotated controllers,
//! Micronaut controllers, JAX-RS resources and Ktor's routing DSL.
//!
//! Routes, parameters and validation are read from the annotation facts the
//! extractor already collected; types come from the signature text, because
//! Kotlin writes them as `name: Type = default` with nullability in the type.

use std::collections::BTreeSet;

use super::text::{self, matching, skip_ws, split_args, split_top, Src};
use super::{
    add_auth, add_error, add_param, fill_path_params, new_op, rule, Draft, Excluded, Harvest, Loaded, Model, Operation,
    Param, Requirement, SymbolRef,
};
use crate::extract::{Annotation, Symbol, SymbolKind};
use crate::lang::Language;
use crate::scan::EvidenceRef;

/// Spring status constants are SCREAMING_SNAKE, Ktor's are CamelCase.
fn status_of(name: &str) -> Option<u16> {
    let n = name.rsplit('.').next().unwrap_or(name).trim();
    let norm: String = n.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_uppercase();
    Some(match norm.as_str() {
        "OK" => 200,
        "CREATED" => 201,
        "ACCEPTED" => 202,
        "NOCONTENT" => 204,
        "BADREQUEST" => 400,
        "UNAUTHORIZED" => 401,
        "PAYMENTREQUIRED" => 402,
        "FORBIDDEN" => 403,
        "NOTFOUND" => 404,
        "METHODNOTALLOWED" => 405,
        "CONFLICT" => 409,
        "GONE" => 410,
        "UNPROCESSABLEENTITY" => 422,
        "TOOMANYREQUESTS" => 429,
        "INTERNALSERVERERROR" => 500,
        "NOTIMPLEMENTED" => 501,
        "BADGATEWAY" => 502,
        "SERVICEUNAVAILABLE" => 503,
        _ => return None,
    })
}

fn known_exception(name: &str) -> Option<u16> {
    Some(match name {
        "NotFoundException" | "EntityNotFoundException" | "NoSuchElementException" => 404,
        "BadRequestException" | "IllegalArgumentException" | "ValidationException" => 400,
        "UnauthorizedException" | "AuthenticationException" => 401,
        "ForbiddenException" | "AccessDeniedException" => 403,
        "ConflictException" | "DataIntegrityViolationException" => 409,
        _ => return None,
    })
}

const SPRING_VERBS: &[(&str, &str)] = &[
    ("GetMapping", "GET"),
    ("PostMapping", "POST"),
    ("PutMapping", "PUT"),
    ("DeleteMapping", "DELETE"),
    ("PatchMapping", "PATCH"),
];

const MICRONAUT_VERBS: &[(&str, &str)] = &[
    ("Get", "GET"),
    ("Post", "POST"),
    ("Put", "PUT"),
    ("Delete", "DELETE"),
    ("Patch", "PATCH"),
    ("Head", "HEAD"),
    ("Options", "OPTIONS"),
];

const JAXRS_VERBS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];

const KTOR_VERBS: &[&str] = &["get", "post", "put", "delete", "patch", "head", "options"];

/// Types a handler receives from the framework rather than from the request.
fn injected(ty: &str) -> bool {
    let t = super::unwrap_type(ty).0;
    matches!(
        t.as_str(),
        "Principal"
            | "Authentication"
            | "Pageable"
            | "Sort"
            | "Model"
            | "ModelMap"
            | "HttpServletRequest"
            | "HttpServletResponse"
            | "ServerHttpRequest"
            | "ServerHttpResponse"
            | "ServerWebExchange"
            | "UriComponentsBuilder"
            | "BindingResult"
            | "Locale"
            | "UserDetails"
            | "Continuation"
    )
}

#[derive(Clone, Copy, PartialEq)]
enum Flavor {
    Spring,
    Micronaut,
    JaxRs,
}

/// One `name: Type = default` parameter of a Kotlin function.
struct KParam {
    name: String,
    type_name: String,
    has_default: bool,
}

/// A parsed `fun` signature: parameters and the declared return type.
struct KSig {
    params: Vec<KParam>,
    return_type: Option<String>,
    /// `fun f() = expr`: the return type is inferred, not `Unit`.
    expression_body: bool,
}

/// Strips annotations, `vararg`/`crossinline` and reads `name: Type = default`.
fn parse_param(piece: &str) -> Option<KParam> {
    let mut rest = piece.trim();
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix('@') {
            let name_len = after.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':' || c == '.'))?;
            let mut i = name_len + 1;
            let b = rest.as_bytes();
            let j = skip_ws(rest, i);
            if b.get(j) == Some(&b'(') {
                i = matching(rest, j)? + 1;
            } else {
                i = j;
            }
            rest = &rest[i..];
            continue;
        }
        let mut changed = false;
        for m in ["vararg ", "crossinline ", "noinline ", "val ", "var "] {
            if let Some(r) = rest.strip_prefix(m) {
                rest = r;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let (name, after) = rest.split_once(':')?;
    let name = name.trim();
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '`') {
        return None;
    }
    // A default value may itself contain `=` inside a call, so split at top level.
    let parts = split_top(after, &['=']);
    let type_name = parts.first().map(|t| t.trim().to_string()).unwrap_or_default();
    Some(KParam { name: name.trim_matches('`').to_string(), type_name, has_default: parts.len() > 1 })
}

/// `fun name(params): Return` inside `span`.
fn parse_signature(src: &Src, span: (usize, usize), name: &str) -> Option<KSig> {
    let code = &src.code;
    let region = src.code_slice(span.0, span.1);
    let at = text::find_word(region, "fun").into_iter().find(|i| region[*i..].contains(name))?;
    let open = span.0 + at + region[at..].find('(')?;
    let close = matching(code, open)?;
    let params =
        split_args(code, open + 1, close).into_iter().filter_map(|(s, e)| parse_param(src.code_slice(s, e))).collect();
    let after = skip_ws(code, close + 1);
    let return_type = (code.as_bytes().get(after) == Some(&b':')).then(|| {
        let start = skip_ws(code, after + 1);
        let mut depth = 0i32;
        let mut end = start;
        let b = code.as_bytes();
        while end < span.1 {
            match b[end] {
                b'<' | b'(' => depth += 1,
                b'>' | b')' => depth -= 1,
                b'{' | b'=' if depth <= 0 => break,
                b'\n' if depth <= 0 => break,
                _ => {}
            }
            end += 1;
        }
        src.slice(start, end).trim().to_string()
    });
    Some(KSig {
        params,
        return_type: return_type.filter(|t| !t.is_empty() && t != "Unit"),
        expression_body: code.as_bytes().get(after) == Some(&b'='),
    })
}

/// Unwraps Kotlin and Spring wrappers; returns the inner type and whether it is a collection.
fn unwrap_kotlin(t: &str) -> (String, bool) {
    let mut ty = t.trim().trim_end_matches('?').trim().to_string();
    let mut collection = false;
    while let Some((head, args)) =
        ty.split_once('<').map(|(h, rest)| (h.trim().to_string(), rest.trim_end().trim_end_matches('>').to_string()))
    {
        let simple = head.rsplit('.').next().unwrap_or(&head).to_string();
        match simple.as_str() {
            "ResponseEntity"
            | "HttpResponse"
            | "Mono"
            | "Optional"
            | "Single"
            | "Maybe"
            | "Deferred"
            | "MutableHttpResponse"
            | "RestResponse" => ty = args,
            "Flux" | "Flow" | "List" | "MutableList" | "Set" | "MutableSet" | "Collection" | "Iterable" | "Array"
            | "Page" | "Multi" | "Observable" | "Publisher" => {
                collection = true;
                ty = args;
            }
            "Map" | "MutableMap" | "Pair" => break,
            _ => break,
        }
        ty = ty.trim().trim_end_matches('?').trim().to_string();
    }
    (ty.split(',').next().unwrap_or(&ty).trim().trim_end_matches('?').to_string(), collection)
}

/// Top-level pieces of an annotation argument list.
fn ann_args(a: &Annotation) -> Vec<&str> {
    if a.arguments.trim().is_empty() {
        return vec![];
    }
    split_top(&a.arguments, &[',']).into_iter().map(str::trim).filter(|p| !p.is_empty()).collect()
}

/// Argument value by key; `""` matches the first positional argument.
fn ann_value<'a>(a: &'a Annotation, keys: &[&str]) -> Option<&'a str> {
    for piece in ann_args(a) {
        let (key, value) = match piece.split_once('=') {
            Some((k, v)) if k.trim().chars().all(|c| c.is_alphanumeric() || c == '_') && !k.trim().is_empty() => {
                (k.trim(), v.trim())
            }
            _ => ("", piece),
        };
        if keys.contains(&key) || (key.is_empty() && keys.contains(&"value")) || (key == "value" && keys.contains(&""))
        {
            return Some(value);
        }
    }
    None
}

/// First string literal of an argument (`"/x"` or `["/x", "/y"]`).
fn ann_string(a: &Annotation, keys: &[&str]) -> Option<String> {
    let v = ann_value(a, keys)?;
    let inner = v.trim().trim_start_matches('[').trim_end_matches(']');
    text::string_lit(split_top(inner, &[',']).first()?.trim()).map(|s| unescape_dollar(&s))
}

/// Every string in an annotation argument: `@RequestMapping(["/a", "/b"])`
/// registers two routes, and taking only the first silently loses one.
fn ann_strings(a: &Annotation, keys: &[&str]) -> Vec<String> {
    let Some(v) = ann_value(a, keys) else { return vec![] };
    split_top(v.trim().trim_start_matches('[').trim_end_matches(']'), &[','])
        .into_iter()
        .filter_map(|p| text::string_lit(p.trim()).map(|s| unescape_dollar(&s)))
        .collect()
}

/// `"\${api.prefix}"` is how Kotlin writes a literal `${…}`, since a bare `$`
/// starts a template. The backslash is syntax, not part of the path.
fn unescape_dollar(s: &str) -> String {
    s.replace("\\$", "$")
}

fn ann_bool(a: &Annotation, key: &str) -> Option<bool> {
    ann_value(a, &[key]).and_then(|v| v.trim().parse().ok())
}

/// Trailing identifiers of an argument (`[RequestMethod.POST]` → `POST`).
fn ann_idents(a: &Annotation, keys: &[&str]) -> Vec<String> {
    let Some(v) = ann_value(a, keys) else { return vec![] };
    split_top(v.trim().trim_start_matches('[').trim_end_matches(']'), &[','])
        .into_iter()
        .map(|p| p.trim().rsplit('.').next().unwrap_or("").trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

struct Index<'a> {
    files: Vec<&'a Loaded<'a>>,
    config: Vec<crate::jvm::ConfigDoc>,
    units: &'a [crate::scan::UnitSummary],
}

impl<'a> Index<'a> {
    /// Annotations of one declaration in a file.
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

    /// A configuration value that applies to `unit` (its own `application`/`bootstrap` files).
    /// `${key}` / `${key:default}` resolved against configuration, and whether
    /// anything was left unresolved — an unresolved prefix makes the path
    /// partial rather than exact.
    fn placeholders(&self, unit: &str, s: &str) -> (String, bool) {
        text::resolve_placeholders(s, |key| self.config_value(unit, key))
    }

    fn config_value(&self, unit: &str, key: &str) -> Option<String> {
        let root = self.units.iter().find(|u| u.id == unit).map(|u| u.root.clone()).unwrap_or_default();
        let prefix = if root.is_empty() { String::new() } else { format!("{root}/") };
        self.config
            .iter()
            .filter(|d| d.path.starts_with(&prefix) && matches!(d.stem.as_str(), "application" | "bootstrap"))
            .find_map(|d| d.get(key).map(|(v, _)| v.to_string()))
            .filter(|v| !v.contains("${"))
    }
}

pub(crate) fn extract(index: &crate::source::SourceIndex, files: &[Loaded], h: &mut Harvest) {
    let kotlin: Vec<&Loaded> = files.iter().filter(|f| f.lang == Language::Kotlin).collect();
    if kotlin.is_empty() {
        return;
    }
    let idx = Index { files: kotlin, config: crate::jvm::config_documents(&index.root), units: index.units };
    let mut models = Models::default();
    let mut ops: Vec<Draft> = vec![];
    for fi in 0..idx.files.len() {
        controllers(&idx, fi, &mut models, &mut ops, &mut h.excluded);
        co_router_routes(&idx, fi, &mut models, &mut ops);
        ktor_routes(&idx, fi, &mut models, &mut ops);
        feign_clients(&idx, fi, &mut models, h);
        http_client_calls(&idx, fi, &mut models, h);
    }
    for d in ops {
        h.ops.push(d);
    }
    models.emit(&idx, h);
}

/// The unit a service id, application name or host refers to.
fn unit_named(units: &[crate::scan::UnitSummary], name: &str) -> Option<String> {
    let n = name.trim().to_lowercase();
    if n.is_empty() || n.contains("${") {
        return None;
    }
    let variants = |s: &str| {
        let s = s.to_lowercase();
        let mut v = vec![s.clone()];
        for suffix in ["-service", "-svc", "-api", "-server", "-app"] {
            if let Some(x) = s.strip_suffix(suffix) {
                v.push(x.to_string());
            }
        }
        v
    };
    let wanted = variants(&n);
    units
        .iter()
        .find(|u| {
            u.aliases.iter().any(|a| a.to_lowercase() == n)
                || wanted.contains(&u.id.to_lowercase())
                || u.compose_service.as_deref().is_some_and(|c| wanted.contains(&c.to_lowercase()))
                || variants(&u.id).contains(&n)
        })
        .map(|u| u.id.clone())
}

/// `@FeignClient(name = "statistics-service")` interfaces: each method is a call to
/// that service, and its return type is what the caller expects back.
fn feign_clients(idx: &Index, fi: usize, models: &mut Models, h: &mut Harvest) {
    let f = idx.files[fi];
    for class in class_symbols(f) {
        let anns = idx.anns(fi, &["class", "interface", "object"], &class.name, None);
        let Some(client) = idx.find(&anns, "FeignClient").or_else(|| idx.find(&anns, "RegisterRestClient")) else {
            continue;
        };
        let target = ann_string(client, &["name", "value", ""])
            .or_else(|| ann_string(client, &["configKey"]))
            .or_else(|| ann_string(client, &["url", "baseUri"]).and_then(|u| crate::jvm::url_host(&u)));
        let Some(target_unit) = target.as_deref().and_then(|t| unit_named(idx.units, t)).filter(|u| u != f.unit) else {
            continue;
        };
        let base =
            idx.find(&anns, "RequestMapping").and_then(|a| ann_string(a, &["value", "path", ""])).unwrap_or_default();
        let (cs, ce) = f.span(class);
        for m in f.facts.symbols.iter().filter(|s| s.kind == SymbolKind::Method) {
            let (ms, me) = f.span(m);
            if ms < cs || me > ce {
                continue;
            }
            let manns = idx.anns(fi, &["method"], &m.name, Some(&class.name));
            let Some((verb, path)) = mapping(idx, &manns, Flavor::Spring).and_then(|m| m.into_iter().next()) else {
                continue;
            };
            let expects = parse_signature(&f.src, (ms, me), &m.name)
                .and_then(|sig| sig.return_type)
                .map(|t| unwrap_kotlin(&t).0)
                .filter(|t| !t.is_empty() && t != "Unit");
            // The caller's own view of the response is compared with what the
            // operation declares, so it has to be resolved as a model too.
            if let Some(t) = &expects {
                models.want(f.unit, t);
            }
            h.clients.push(super::ClientDraft {
                call: super::ClientCall {
                    unit: f.unit.to_string(),
                    method: verb,
                    path: text::join_path(&base, &path),
                    target_unit: Some(target_unit.clone()),
                    operation: None,
                    caller: Some(format!("{}.{}", class.name, m.name)),
                    drift: vec![],
                    evidence: idx.ev(fi, manns.first().map(|a| a.line).unwrap_or(m.start_line)),
                },
                expects,
                expects_fields: Vec::new(),
            });
        }
    }
}

const CLIENT_VERBS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options", "request", "submitForm"];

/// Ktor `client.get("http://svc/x")` / Spring `webClient.get().uri("/x")` calls
/// made from Kotlin, with the type the caller reads the body into.
fn http_client_calls(idx: &Index, fi: usize, models: &mut Models, h: &mut Harvest) {
    let f = idx.files[fi];
    let code = &f.src.code;
    if !code.contains("client") && !code.contains("Client") {
        return;
    }
    for verb in CLIENT_VERBS {
        for at in text::find_word(code, verb) {
            // A client call has a receiver that names a client.
            let recv_start = text::receiver_start(code, at);
            let receiver = code[recv_start..at].trim_end_matches('.').trim();
            if !receiver.to_lowercase().ends_with("client") {
                continue;
            }
            let Some(url) = call_string(&f.src, at, verb) else { continue };
            let host = crate::jvm::url_host(&url);
            let path = match url.split_once("://") {
                Some((_, rest)) => rest.find('/').map(|i| rest[i..].to_string()).unwrap_or_else(|| "/".into()),
                None => url.clone(),
            };
            if !path.starts_with('/') {
                continue;
            }
            let target_unit = host.as_deref().and_then(|hst| unit_named(idx.units, hst)).filter(|u| u != f.unit);
            let Some(target_unit) = target_unit else { continue };
            // `.body<Account>()` / `awaitBody<Account>()` right after the call.
            let tail = &code[at..(at + 400).min(code.len())];
            let expects = ["body<", "awaitBody<", "bodyToMono<", "receive<"].iter().find_map(|m| {
                let i = tail.find(m)? + m.len();
                let end = tail[i..].find('>')? + i;
                Some(unwrap_kotlin(&tail[i..end]).0)
            });
            if let Some(t) = &expects {
                models.want(f.unit, t);
            }
            let line = f.src.line(at);
            h.clients.push(super::ClientDraft {
                call: super::ClientCall {
                    unit: f.unit.to_string(),
                    method: verb.to_uppercase(),
                    path: text::normalize_segment(&path),
                    target_unit: Some(target_unit),
                    operation: None,
                    caller: Some(enclosing_name(idx, fi, at)),
                    drift: vec![],
                    evidence: idx.ev(fi, line),
                },
                expects,
                expects_fields: Vec::new(),
            });
        }
    }
}

/// Types referenced by operations, resolved to models afterwards.
#[derive(Default)]
struct Models {
    wanted: BTreeSet<(String, String)>,
}

impl Models {
    fn want(&mut self, unit: &str, type_name: &str) {
        let (inner, _) = unwrap_kotlin(type_name);
        if inner.chars().next().is_some_and(|c| c.is_uppercase()) {
            self.wanted.insert((unit.to_string(), inner));
        }
    }

    /// Emits a model for every wanted class declared in Kotlin, following nested types.
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
                let Some(sym) = f.facts.symbols.iter().find(|s| {
                    s.name == name && matches!(s.kind, SymbolKind::Class | SymbolKind::Enum | SymbolKind::Interface)
                }) else {
                    continue;
                };
                if sym.kind == SymbolKind::Enum {
                    continue;
                }
                let fields = model_fields(idx, fi, sym, &mut self.wanted, &unit);
                if fields.is_empty() {
                    continue;
                }
                h.model(Model {
                    id: format!("{unit}:{name}"),
                    unit: unit.clone(),
                    name: name.clone(),
                    fields,
                    doc: sym.doc.clone(),
                    evidence: idx.files[fi].src.ev_range(
                        idx.files[fi].src.line_start(sym.start_line),
                        idx.files[fi].src.line_end(sym.end_line),
                        Some(&name),
                    ),
                });
                break;
            }
        }
    }
}

/// Enum members of a Kotlin `enum class`, for `one of: …` rules.
fn enum_values(idx: &Index, unit: &str, name: &str) -> Option<Vec<String>> {
    for f in idx.files.iter().filter(|f| f.unit == unit) {
        let Some(sym) = f.facts.symbols.iter().find(|s| s.name == name && s.kind == SymbolKind::Enum) else {
            continue;
        };
        let (s, e) = f.span(sym);
        let body = f.src.code_slice(s, e);
        let open = body.find('{')?;
        let close = matching(body, open)?;
        let values: Vec<String> = split_top(&body[open + 1..close], &[','])
            .into_iter()
            .map(|v| v.trim().split(['(', ' ', ';']).next().unwrap_or("").trim().to_string())
            .filter(|v| !v.is_empty() && v.chars().next().is_some_and(|c| c.is_alphabetic()))
            .collect();
        return (!values.is_empty()).then_some(values);
    }
    None
}

/// Bean Validation on a property (`@field:NotBlank`, `@Size(max = 40)`).
fn field_rules(idx: &Index, fi: usize, anns: &[&Annotation]) -> (Vec<super::Rule>, bool) {
    let mut rules = vec![];
    let mut required = false;
    for a in anns {
        let ev = idx.ev(fi, a.line);
        match a.name.as_str() {
            "NotNull" | "NotBlank" | "NotEmpty" => {
                required = true;
                if a.name == "NotBlank" {
                    rules.push(rule("must not be blank", "required", &ev));
                } else if a.name == "NotEmpty" {
                    rules.push(rule("must not be empty", "required", &ev));
                }
            }
            "Size" | "Length" => {
                if let Some(min) = ann_value(a, &["min"]) {
                    rules.push(rule(text::min_len(min.trim()), "length", &ev));
                }
                if let Some(max) = ann_value(a, &["max"]) {
                    rules.push(rule(text::max_len(max.trim()), "length", &ev));
                }
            }
            "Min" | "DecimalMin" => {
                if let Some(v) = ann_value(a, &["value", ""]) {
                    rules.push(rule(format!("at least {}", v.trim()), "range", &ev));
                }
            }
            "Max" | "DecimalMax" => {
                if let Some(v) = ann_value(a, &["value", ""]) {
                    rules.push(rule(format!("at most {}", v.trim()), "range", &ev));
                }
            }
            "Positive" => rules.push(rule("must be positive", "range", &ev)),
            "PositiveOrZero" => rules.push(rule("must be zero or greater", "range", &ev)),
            "Negative" => rules.push(rule("must be negative", "range", &ev)),
            "Email" => rules.push(rule("must be a valid email", "format", &ev)),
            "Pattern" => {
                if let Some(p) = ann_string(a, &["regexp", "value", ""]) {
                    rules.push(rule(format!("must match {p}"), "pattern", &ev));
                }
            }
            "Past" | "PastOrPresent" => rules.push(rule("must be in the past", "range", &ev)),
            "Future" | "FutureOrPresent" => rules.push(rule("must be in the future", "range", &ev)),
            _ => {}
        }
    }
    (rules, required)
}

/// Properties of a Kotlin class: primary-constructor `val`/`var` and body properties.
fn model_fields(
    idx: &Index,
    fi: usize,
    sym: &Symbol,
    wanted: &mut BTreeSet<(String, String)>,
    unit: &str,
) -> Vec<super::Field> {
    let f = idx.files[fi];
    let (s, e) = f.span(sym);
    let region = f.src.code_slice(s, e);
    let mut out: Vec<super::Field> = vec![];
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut push = |name: String, type_name: String, has_default: bool, line: u32, out: &mut Vec<super::Field>| {
        if !seen.insert(name.clone()) {
            return;
        }
        let anns = idx.anns(fi, &["field"], &name, Some(&sym.name));
        let (mut rules, required_ann) = field_rules(idx, fi, &anns);
        let wire = anns
            .iter()
            .find(|a| a.name == "JsonProperty" || a.name == "SerialName" || a.name == "Field")
            .and_then(|a| ann_string(a, &["value", ""]));
        if anns.iter().any(|a| a.name == "JsonIgnore" || a.name == "Transient") {
            return;
        }
        let nullable = type_name.trim_end().ends_with('?');
        let (inner, collection) = unwrap_kotlin(&type_name);
        if let Some(values) = enum_values(idx, unit, &inner) {
            let ev = crate::source::line_ref(f.path(), line);
            rules.push(rule(format!("one of: {}", values.join(", ")), "enum", &ev));
        } else {
            wanted.insert((unit.to_string(), inner.clone()));
        }
        let shown = if collection { format!("{inner}[]") } else { type_name.trim().to_string() };
        out.push(super::Field {
            name: wire.clone().unwrap_or_else(|| name.clone()),
            code_name: wire.map(|_| name.clone()),
            type_name: shown,
            required: required_ann || (!nullable && !has_default),
            rules,
            doc: None,
            model: None,
            evidence: crate::source::line_ref(f.path(), line),
        });
    };
    // Primary constructor: `class X(val a: T = d, …)`.
    if let Some(open) = constructor_parens(region) {
        if let Some(close) = matching(region, open) {
            for (ps, pe) in split_args(region, open + 1, close) {
                let piece = &region[ps..pe];
                if !piece.contains("val ") && !piece.contains("var ") {
                    continue;
                }
                if let Some(p) = parse_param(piece) {
                    let line = f.src.line(s + ps);
                    push(p.name, p.type_name, p.has_default, line, &mut out);
                }
            }
        }
    }
    // Body properties: `val a: T = …`.
    for (i, l) in region.lines().enumerate() {
        let t = l.trim_start();
        let decl = t.strip_prefix("val ").or_else(|| t.strip_prefix("var ")).or_else(|| {
            // `@field:NotBlank val name: String`
            t.split_once("val ").map(|(pre, rest)| if pre.trim().starts_with('@') { rest } else { t })
        });
        let Some(decl) = decl else { continue };
        let Some((name, rest)) = decl.split_once(':') else { continue };
        let name = name.trim();
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let type_name = split_top(rest, &['=']).first().map(|t| t.trim().to_string()).unwrap_or_default();
        if type_name.is_empty() {
            continue;
        }
        let has_default = rest.contains('=');
        push(name.to_string(), type_name, has_default, sym.start_line + i as u32, &mut out);
    }
    out
}

/// Offset of the primary constructor's `(` in a class declaration region.
fn constructor_parens(region: &str) -> Option<usize> {
    let name_at = text::find_word(region, "class").first().copied()?;
    let rest = &region[name_at..];
    let stop = rest.find(['{', '\n']).unwrap_or(rest.len());
    let open = rest[..stop].find('(')?;
    Some(name_at + open)
}

fn class_symbols<'s>(f: &'s Loaded<'s>) -> Vec<&'s Symbol> {
    f.facts.symbols.iter().filter(|s| matches!(s.kind, SymbolKind::Class | SymbolKind::Interface)).collect()
}

/// Annotated controllers: Spring MVC / WebFlux, Micronaut, JAX-RS.
fn controllers(idx: &Index, fi: usize, models: &mut Models, ops: &mut Vec<Draft>, excluded: &mut Vec<Excluded>) {
    let f = idx.files[fi];
    for class in class_symbols(f) {
        let anns = idx.anns(fi, &["class", "interface", "object"], &class.name, None);
        let flavor = if anns.iter().any(|a| a.name == "RestController" || a.name == "Controller")
            && anns.iter().any(|a| a.name == "RestController")
        {
            Flavor::Spring
        } else if anns.iter().any(|a| a.name == "Controller") {
            // Micronaut's `@Controller("/path")` takes the base path as its argument.
            if idx.find(&anns, "Controller").and_then(|a| ann_string(a, &["value", ""])).is_some() {
                Flavor::Micronaut
            } else {
                Flavor::Spring
            }
        } else if anns.iter().any(|a| a.name == "Path") {
            Flavor::JaxRs
        } else {
            continue;
        };
        let base = match flavor {
            Flavor::Spring => idx.find(&anns, "RequestMapping").and_then(|a| ann_string(a, &["value", "path", ""])),
            Flavor::Micronaut => idx.find(&anns, "Controller").and_then(|a| ann_string(a, &["value", ""])),
            Flavor::JaxRs => idx.find(&anns, "Path").and_then(|a| ann_string(a, &["value", ""])),
        }
        .unwrap_or_default();
        let context = context_path(idx, f.unit, flavor);
        let base = text::join_path(&context, &base);
        let rest_by_default =
            anns.iter().any(|a| a.name == "RestController" || a.name == "ResponseBody") || flavor != Flavor::Spring;
        let class_auth = auth_requirements(idx, fi, &anns);
        let (cs, ce) = f.span(class);
        for m in f.facts.symbols.iter().filter(|s| s.kind == SymbolKind::Method) {
            let (ms, me) = f.span(m);
            if ms < cs || me > ce {
                continue;
            }
            let manns = idx.anns(fi, &["method"], &m.name, Some(&class.name));
            let Some(mappings) = mapping(idx, &manns, flavor) else { continue };
            // Spring MVC's plain `@Controller` returns view names, not payloads;
            // only `@RestController`, or `@ResponseBody` on the class or the
            // method, makes a handler part of an HTTP API. Correct to leave out,
            // wrong to leave unsaid — the route is recorded as excluded.
            if flavor == Flavor::Spring && !rest_by_default && !manns.iter().any(|a| a.name == "ResponseBody") {
                if let Some((verb, path)) = mappings.first() {
                    let full = text::join_path("", &text::join_path(&base, path));
                    excluded.push(Excluded {
                        operation: format!("{}:{} {full}", f.unit, verb.to_uppercase()),
                        reason: "server-rendered view, not an API operation".into(),
                        evidence: f.src.ev_range(ms, me, Some(&m.name)),
                    });
                }
                continue;
            }
            let framework = match flavor {
                Flavor::Spring => "spring",
                Flavor::Micronaut => "micronaut",
                Flavor::JaxRs => "jax-rs",
            };
            for (verb, path) in mappings {
                let raw = text::join_path(&base, &path);
                let (full, partial) = idx.placeholders(f.unit, &raw);
                // A placeholder that resolves to nothing leaves an empty segment
                // behind: `/api/${api.prefix}/reports` must not become `/api//reports`.
                let full = text::join_path("", &full);
                let handler = SymbolRef { name: m.name.clone(), evidence: f.src.ev_range(ms, me, Some(&m.name)) };
                let evidence = idx.ev(fi, manns.first().map(|a| a.line).unwrap_or(m.start_line));
                let mut op = new_op(f.unit, framework, &verb, full, handler, evidence);
                op.path_partial = partial || raw.contains("${");
                op.summary = m.doc.as_deref().and_then(text::first_sentence);
                describe(idx, fi, m, &manns, flavor, models, &mut op);
                for r in &class_auth {
                    add_auth(&mut op, r.clone());
                }
                for r in auth_requirements(idx, fi, &manns) {
                    add_auth(&mut op, r);
                }
                fill_path_params(&mut op);
                let request_declared = op.request_body.is_some();
                let response_declared = op.response.is_some();
                ops.push(Draft { op, request_declared, response_declared });
            }
        }
    }
}

/// Server prefix configured outside the code (`server.servlet.context-path`).
fn context_path(idx: &Index, unit: &str, flavor: Flavor) -> String {
    let keys: &[&str] = match flavor {
        Flavor::Spring => &[
            "server.servlet.context-path",
            "spring.mvc.servlet.path",
            "spring.webflux.base-path",
            "server.context-path",
        ],
        Flavor::Micronaut => &["micronaut.server.context-path"],
        Flavor::JaxRs => &["quarkus.http.root-path", "quarkus.rest.path"],
    };
    keys.iter().filter_map(|k| idx.config_value(unit, k)).fold(String::new(), |acc, v| text::join_path(&acc, &v))
}

/// Every verb and path a method's mapping annotation registers.
///
/// `@RequestMapping` with no `method` answers every verb, and both `method` and
/// the path may be lists — Spring registers the product of the two.
fn mapping(idx: &Index, anns: &[&Annotation], flavor: Flavor) -> Option<Vec<(String, String)>> {
    let spread = |verbs: Vec<String>, paths: Vec<String>| {
        let paths = if paths.is_empty() { vec![String::new()] } else { paths };
        verbs.iter().flat_map(|v| paths.iter().map(move |p| (v.to_uppercase(), p.clone()))).collect::<Vec<_>>()
    };
    match flavor {
        Flavor::Spring => {
            for (ann, verb) in SPRING_VERBS {
                if let Some(a) = idx.find(anns, ann) {
                    return Some(spread(vec![verb.to_string()], ann_strings(a, &["value", "path", ""])));
                }
            }
            let a = idx.find(anns, "RequestMapping")?;
            let mut verbs = ann_idents(a, &["method"]);
            if verbs.is_empty() {
                verbs.push("ANY".into());
            }
            Some(spread(verbs, ann_strings(a, &["value", "path", ""])))
        }
        Flavor::Micronaut => {
            for (ann, verb) in MICRONAUT_VERBS {
                if let Some(a) = idx.find(anns, ann) {
                    return Some(spread(vec![verb.to_string()], ann_strings(a, &["value", "uri", ""])));
                }
            }
            None
        }
        Flavor::JaxRs => {
            let verb = JAXRS_VERBS.iter().find(|v| idx.find(anns, v).is_some())?;
            let paths = idx.find(anns, "Path").map(|a| ann_strings(a, &["value", ""])).unwrap_or_default();
            Some(spread(vec![verb.to_string()], paths))
        }
    }
}

/// Parameters, request body, response type, status and errors of one handler.
fn describe(
    idx: &Index,
    fi: usize,
    m: &Symbol,
    manns: &[&Annotation],
    flavor: Flavor,
    models: &mut Models,
    op: &mut Operation,
) {
    let f = idx.files[fi];
    let (ms, me) = f.span(m);
    let sig = parse_signature(&f.src, (ms, me), &m.name);
    if let Some(sig) = &sig {
        for p in &sig.params {
            let panns = idx.anns(fi, &["parameter"], &p.name, Some(&m.name));
            let (rules, required_ann) = field_rules(idx, fi, &panns);
            let nullable = p.type_name.trim_end().ends_with('?');
            let required = required_ann || (!nullable && !p.has_default);
            let ev = idx.ev(fi, panns.first().map(|a| a.line).unwrap_or(m.start_line));
            let body_ann = panns.iter().find(|a| matches!(a.name.as_str(), "RequestBody" | "Body"));
            if let Some(_b) = body_ann {
                models.want(f.unit, &p.type_name);
                op.request_body = Some(typed(&p.type_name));
                continue;
            }
            let location = panns.iter().find_map(|a| match a.name.as_str() {
                "PathVariable" | "PathParam" => Some("path"),
                "RequestParam" | "QueryValue" | "QueryParam" | "FormParam" => Some("query"),
                "RequestHeader" | "HeaderParam" | "Header" => Some("header"),
                "CookieValue" | "CookieParam" => Some("cookie"),
                _ => None,
            });
            let Some(location) = location else {
                // An un-annotated entity parameter is the JAX-RS request body.
                if flavor == Flavor::JaxRs && !injected(&p.type_name) && op.request_body.is_none() {
                    models.want(f.unit, &p.type_name);
                    op.request_body = Some(typed(&p.type_name));
                }
                continue;
            };
            let named = panns
                .iter()
                .find(|a| a.target == p.name)
                .and_then(|a| ann_string(a, &["value", "name", ""]))
                .filter(|s| !s.is_empty());
            let default = panns.iter().find_map(|a| ann_string(a, &["defaultValue"]));
            let req = panns
                .iter()
                .find_map(|a| ann_bool(a, "required"))
                // Spring / Micronaut query parameters are required unless the
                // signature says otherwise: a `?` type, a Kotlin default, or
                // `defaultValue`.
                .unwrap_or(required && default.is_none());
            let wire = named.clone().unwrap_or_else(|| p.name.clone());
            add_param(
                op,
                Param {
                    name: wire.clone(),
                    code_name: (wire != p.name).then(|| p.name.clone()),
                    location: location.to_string(),
                    type_name: unwrap_kotlin(&p.type_name).0,
                    required: if location == "path" { true } else { req },
                    rules,
                    evidence: ev,
                },
            );
            let (inner, _) = unwrap_kotlin(&p.type_name);
            if let Some(values) = enum_values(idx, f.unit, &inner) {
                if let Some(param) = op.params.iter_mut().find(|x| x.name == wire) {
                    let ev = idx.ev(fi, m.start_line);
                    param.rules.push(rule(format!("one of: {}", values.join(", ")), "enum", &ev));
                }
            }
        }
        if let Some(ret) = &sig.return_type {
            let (inner, collection) = unwrap_kotlin(ret);
            if !inner.is_empty() && inner != "Unit" && inner != "Any" {
                models.want(f.unit, &inner);
                op.response = Some(super::TypeRef {
                    type_name: if collection { format!("{inner}[]") } else { inner },
                    model: None,
                    collection,
                });
            }
        }
    }
    // Status: `@ResponseStatus(HttpStatus.CREATED)` / Micronaut `@Status`.
    if let Some(s) = manns
        .iter()
        .find(|a| matches!(a.name.as_str(), "ResponseStatus" | "Status"))
        .and_then(|a| ann_value(a, &["value", "code", ""]))
        .and_then(status_of)
    {
        op.success_status = Some(s);
    }
    let body = f.src.code_slice(ms, me);
    if op.success_status.is_none() {
        if body.contains("ResponseEntity.created") || body.contains("HttpResponse.created") {
            op.success_status = Some(201);
        } else if body.contains("ResponseEntity.noContent") || body.contains("HttpResponse.noContent") {
            op.success_status = Some(204);
        } else if let Some(at) = body.find("ResponseEntity.status(").or_else(|| body.find("HttpResponse.status(")) {
            let rest = &body[at..];
            let inner = rest.find('(').and_then(|o| matching(rest, o).map(|c| &rest[o + 1..c]));
            op.success_status = inner.and_then(status_of);
        }
    }
    // No declared return type and a block body really is `Unit`; an expression
    // body only means Kotlin infers the type, so the response stays undeclared.
    if op.success_status.is_none() && sig.as_ref().is_some_and(|s| s.return_type.is_none() && !s.expression_body) {
        op.success_status = Some(if op.method == "POST" { 201 } else { 204 });
    }
    errors(idx, fi, ms, body, op);
}

fn typed(type_name: &str) -> super::TypeRef {
    let (inner, collection) = unwrap_kotlin(type_name);
    super::TypeRef { type_name: if collection { format!("{inner}[]") } else { inner }, model: None, collection }
}

/// `throw ResponseStatusException(HttpStatus.NOT_FOUND, "msg")` and known exception types.
fn errors(idx: &Index, fi: usize, base: usize, body: &str, op: &mut Operation) {
    let f = idx.files[fi];
    for at in text::find_word(body, "throw") {
        let rest = &body[at..];
        let Some(open) = rest.find('(') else { continue };
        let name = rest[5..open].trim().rsplit('.').next().unwrap_or("").trim().to_string();
        let line = f.src.line(base + at);
        let ev = idx.ev(fi, line);
        let Some(close) = matching(rest, open) else { continue };
        let args = &rest[open + 1..close];
        let pieces = split_top(args, &[',']);
        let status = pieces.first().and_then(|p| status_of(p.trim())).or_else(|| known_exception(&name));
        // The code view blanks literals; read the message from the source text.
        let message = split_args(&f.src.code, base + at + open + 1, base + at + close)
            .into_iter()
            .find_map(|(s, e)| text::string_lit(f.src.slice(s, e).trim()));
        if status.is_some() {
            add_error(op, status, message, ev);
        }
    }
}

/// `@PreAuthorize`, `@Secured`, `@RolesAllowed`, `@Authenticated`.
fn auth_requirements(idx: &Index, fi: usize, anns: &[&Annotation]) -> Vec<Requirement> {
    let mut out = vec![];
    for a in anns {
        let ev = idx.ev(fi, a.line);
        match a.name.as_str() {
            "PreAuthorize" | "PostAuthorize" => {
                let Some(expr) = ann_string(a, &["value", ""]) else { continue };
                let kind = if expr.contains("hasRole") || expr.contains("hasAuthority") {
                    "role"
                } else if expr.contains("hasScope") || expr.contains("hasPermission") {
                    "scope"
                } else if expr.contains("isAuthenticated") {
                    "authenticated"
                } else {
                    "custom"
                };
                out.push(Requirement { kind: kind.into(), detail: expr, evidence: ev });
            }
            "Secured" | "RolesAllowed" => {
                let roles = ann_args(a)
                    .iter()
                    .filter_map(|p| text::string_lit(p.trim().trim_start_matches('[').trim_end_matches(']')))
                    .collect::<Vec<_>>()
                    .join(", ");
                if !roles.is_empty() {
                    out.push(Requirement { kind: "role".into(), detail: roles, evidence: ev });
                }
            }
            "Authenticated" => {
                out.push(Requirement { kind: "authenticated".into(), detail: "authenticated".into(), evidence: ev })
            }
            _ => {}
        }
    }
    out
}

// ──────────────────── Spring WebFlux functional DSL ────────────────────

const CO_ROUTER_VERBS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];

/// Spring WebFlux's Kotlin DSL: `coRouter { "/books".nest { GET("/{id}", handler::byId) } }`.
/// Paths come from the nested blocks; everything else from the handler function.
fn co_router_routes(idx: &Index, fi: usize, models: &mut Models, ops: &mut Vec<Draft>) {
    let f = idx.files[fi];
    let code = &f.src.code;
    if !code.contains("coRouter") && !code.contains("router") {
        return;
    }
    let base = context_path(idx, f.unit, Flavor::Spring);
    for word in ["coRouter", "router"] {
        for at in text::find_word(code, word) {
            let Some(open) = brace_after(code, at + word.len()) else { continue };
            let Some(close) = matching(code, open) else { continue };
            walk_co_router(idx, fi, (open + 1, close), base.clone(), models, ops);
        }
    }
}

/// The path a `.nest { }` receiver contributes: `"/books".nest` → `/books`.
/// `accept(APPLICATION_JSON).nest { }` contributes nothing.
fn nest_segment(src: &Src, at: usize) -> String {
    let b = src.code.as_bytes();
    let mut i = at;
    while i > 0 && b[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    if i == 0 || b[i - 1] != b'.' {
        return String::new();
    }
    i -= 1;
    while i > 0 && b[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    // The code view keeps quotes and blanks contents, so the previous quote opens it.
    if i < 2 || b[i - 1] != b'"' {
        return String::new();
    }
    let mut open = i - 1;
    while open > 0 && b[open - 1] != b'"' {
        open -= 1;
    }
    text::string_lit(src.slice(open - 1, i).trim()).unwrap_or_default()
}

fn walk_co_router(
    idx: &Index,
    fi: usize,
    span: (usize, usize),
    prefix: String,
    models: &mut Models,
    ops: &mut Vec<Draft>,
) {
    let f = idx.files[fi];
    let code = &f.src.code;
    let mut i = span.0;
    while i < span.1 {
        let Some((s, e)) = text::ident_at(code, i).or_else(|| {
            let next = code[i..span.1].find(|c: char| c.is_alphabetic()).map(|d| i + d)?;
            text::ident_at(code, next)
        }) else {
            break;
        };
        if s >= span.1 {
            break;
        }
        let word = &code[s..e];
        let mut next = e;
        if word == "nest" {
            if let Some(open) = brace_after(code, e) {
                if let Some(close) = matching(code, open) {
                    let seg = nest_segment(&f.src, s);
                    walk_co_router(idx, fi, (open + 1, close), text::join_path(&prefix, &seg), models, ops);
                    next = close + 1;
                }
            }
        } else if CO_ROUTER_VERBS.contains(&word) {
            let after = skip_ws(code, e);
            if code.as_bytes().get(after) == Some(&b'(') {
                if let Some(close) = matching(code, after) {
                    let args = text::split_args(code, after + 1, close);
                    let path = args
                        .first()
                        .and_then(|(s2, e2)| text::string_lit(f.src.slice(*s2, *e2).trim()))
                        .unwrap_or_default();
                    // `GET("/{id}", handler::byId)` or `GET("/{id}") { req -> … }`
                    let reference = args
                        .get(1)
                        .or_else(|| args.first().filter(|_| args.len() == 1 && path.is_empty()))
                        .map(|(s2, e2)| code[*s2..*e2].trim().to_string())
                        .filter(|r| r.contains("::"));
                    let lambda = brace_after(code, close + 1).and_then(|o| matching(code, o).map(|c| (o + 1, c)));
                    let handler = reference
                        .as_deref()
                        .and_then(|r| r.rsplit("::").next())
                        .filter(|n| !n.is_empty())
                        .and_then(|n| find_function(idx, fi, n).map(|(hfi, sp)| (n.to_string(), hfi, sp)))
                        .or_else(|| lambda.map(|sp| (enclosing_name(idx, fi, sp.0), fi, sp)));
                    co_router_op(idx, fi, s, word, text::join_path(&prefix, &path), handler, models, ops);
                    next = lambda.map(|(_, c)| c + 1).unwrap_or(close + 1);
                }
            }
        }
        i = next.max(e);
    }
}

fn enclosing_name(idx: &Index, fi: usize, at: usize) -> String {
    let f = idx.files[fi];
    f.enclosing(f.src.line(at)).map(|s| s.name.clone()).unwrap_or_else(|| "route".into())
}

/// The handler function `name` refers to, preferring this file, then this unit.
fn find_function(idx: &Index, fi: usize, name: &str) -> Option<(usize, (usize, usize))> {
    let unit = idx.files[fi].unit;
    let order = std::iter::once(fi).chain((0..idx.files.len()).filter(|&i| i != fi && idx.files[i].unit == unit));
    for i in order {
        let f = idx.files[i];
        if let Some(sym) = f.function(name) {
            return Some((i, f.span(sym)));
        }
    }
    None
}

/// String arguments of every `name("…")` call in the span, with their lines.
fn string_args_of(src: &Src, span: (usize, usize), name: &str) -> Vec<(String, u32)> {
    let mut out = vec![];
    let code = &src.code;
    for at in text::find_word(&code[span.0..span.1], name) {
        let at = span.0 + at;
        if let Some(v) = call_string(src, at, name) {
            out.push((v, src.line(at)));
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn co_router_op(
    idx: &Index,
    fi: usize,
    route_at: usize,
    verb: &str,
    path: String,
    handler: Option<(String, usize, (usize, usize))>,
    models: &mut Models,
    ops: &mut Vec<Draft>,
) {
    let f = idx.files[fi];
    let Some((name, hfi, (bs, be))) = handler else { return };
    let hf = idx.files[hfi];
    // The route is declared here; the handler lives wherever the reference points.
    let evidence = idx.ev(fi, f.src.line(route_at));
    let handler_ref = SymbolRef { name: name.clone(), evidence: hf.src.ev_range(bs, be, Some(&name)) };
    let mut op = new_op(f.unit, "spring-webflux", verb, path, handler_ref, evidence);
    let body = hf.src.code_slice(bs, be);
    // `request.awaitBody<PlaceOrder>()` / `bodyToMono<T>()` / `awaitBodyOrNull<T>()`
    for pat in ["awaitBodyOrNull<", "awaitBody<", "bodyToMono<", "bodyToFlux<"] {
        if let Some(at) = body.find(pat) {
            let rest = &body[at + pat.len()..];
            if let Some(end) = rest.find('>') {
                let ty = rest[..end].trim().to_string();
                if !ty.is_empty() {
                    models.want(hf.unit, &ty);
                    op.request_body = Some(typed(&ty));
                }
                break;
            }
        }
    }
    // `ServerResponse.status(CREATED)` / `created(…)` / `noContent()` / `ok()`
    op.success_status = if body.contains("noContent(") {
        Some(204)
    } else if body.contains("created(") {
        Some(201)
    } else if let Some(at) = body.find("status(") {
        call_string(&hf.src, bs + at, "status")
            .and_then(|s| status_of(&s))
            .or_else(|| {
                let rest = &body[at + "status(".len()..];
                rest.find(')').and_then(|e| status_of(rest[..e].trim()))
            })
            .or(Some(200))
    } else if body.contains("ok(") {
        Some(200)
    } else {
        None
    };
    // `request.pathVariable("id")` / `queryParamOrNull("q")`
    for (call, location) in [("pathVariable", "path"), ("queryParam", "query"), ("queryParamOrNull", "query")] {
        for (name, line) in string_args_of(&hf.src, (bs, be), call) {
            let is_path = location == "path" || op.path.contains(&format!("{{{name}}}"));
            add_param(
                &mut op,
                Param {
                    name: name.clone(),
                    code_name: None,
                    location: if is_path { "path" } else { "query" }.to_string(),
                    type_name: "string".into(),
                    required: is_path || call == "queryParam",
                    rules: vec![],
                    evidence: crate::source::line_ref(hf.path(), line),
                },
            );
        }
    }
    errors(idx, hfi, bs, body, &mut op);
    fill_path_params(&mut op);
    let request_declared = op.request_body.is_some();
    ops.push(Draft { op, request_declared, response_declared: false });
}

// ─────────────────────────────── Ktor ───────────────────────────────

/// Ktor's routing DSL: `routing { route("/x") { get { … } } }`, with the path
/// prefix and `authenticate(…)` taken from the enclosing blocks.
fn ktor_routes(idx: &Index, fi: usize, models: &mut Models, ops: &mut Vec<Draft>) {
    let f = idx.files[fi];
    let code = &f.src.code;
    if !code.contains("routing") && !code.contains("embeddedServer") {
        return;
    }
    for at in text::find_word(code, "routing") {
        let Some(open) = brace_after(code, at + "routing".len()) else { continue };
        let Some(close) = matching(code, open) else { continue };
        walk_ktor(idx, fi, (open + 1, close), String::new(), &[], models, ops);
    }
}

/// Offset of the `{` that opens a trailing lambda, skipping `(...)` arguments.
fn brace_after(code: &str, mut i: usize) -> Option<usize> {
    i = skip_ws(code, i);
    if code.as_bytes().get(i) == Some(&b'(') {
        i = skip_ws(code, matching(code, i)? + 1);
    }
    (code.as_bytes().get(i) == Some(&b'{')).then_some(i)
}

/// First string argument of the call starting at `at`. Offsets come from the
/// code view; the literal itself is read from the original text, which the
/// code view blanks.
fn call_string(src: &Src, at: usize, word: &str) -> Option<String> {
    let code = &src.code;
    let after = skip_ws(code, at + word.len());
    if code.as_bytes().get(after) != Some(&b'(') {
        return None;
    }
    let close = matching(code, after)?;
    let (s, e) = split_args(code, after + 1, close).into_iter().next()?;
    text::string_lit(src.slice(s, e).trim())
}

fn walk_ktor(
    idx: &Index,
    fi: usize,
    span: (usize, usize),
    prefix: String,
    auth: &[String],
    models: &mut Models,
    ops: &mut Vec<Draft>,
) {
    let f = idx.files[fi];
    let code = &f.src.code;
    let mut i = span.0;
    while i < span.1 {
        let Some((s, e)) = text::ident_at(code, i).or_else(|| {
            let next = code[i..span.1].find(|c: char| c.is_alphabetic()).map(|d| i + d)?;
            text::ident_at(code, next)
        }) else {
            break;
        };
        if s >= span.1 {
            break;
        }
        let word = &code[s..e];
        let mut next = e;
        match word {
            "route" => {
                if let (Some(path), Some(open)) = (call_string(&f.src, s, word), brace_after(code, e)) {
                    if let Some(close) = matching(code, open) {
                        walk_ktor(idx, fi, (open + 1, close), text::join_path(&prefix, &path), auth, models, ops);
                        next = close + 1;
                    }
                }
            }
            "authenticate" => {
                if let Some(open) = brace_after(code, e) {
                    if let Some(close) = matching(code, open) {
                        let name = call_string(&f.src, s, word).unwrap_or_else(|| "authenticated".into());
                        let mut inner = auth.to_vec();
                        inner.push(name);
                        walk_ktor(idx, fi, (open + 1, close), prefix.clone(), &inner, models, ops);
                        next = close + 1;
                    }
                }
            }
            w if KTOR_VERBS.contains(&w) => {
                if let Some(open) = brace_after(code, e) {
                    if let Some(close) = matching(code, open) {
                        let path = call_string(&f.src, s, word).unwrap_or_default();
                        ktor_op(idx, fi, w, text::join_path(&prefix, &path), (open + 1, close), auth, models, ops);
                        next = close + 1;
                    }
                }
            }
            _ => {}
        }
        i = next.max(e);
    }
}

#[allow(clippy::too_many_arguments)]
fn ktor_op(
    idx: &Index,
    fi: usize,
    verb: &str,
    path: String,
    body: (usize, usize),
    auth: &[String],
    models: &mut Models,
    ops: &mut Vec<Draft>,
) {
    let f = idx.files[fi];
    let code = &f.src.code;
    let line = f.src.line(body.0);
    let handler_name = f.enclosing(line).map(|s| s.name.clone()).unwrap_or_else(|| format!("{verb} {path}"));
    let evidence = idx.ev(fi, line);
    let handler = SymbolRef { name: handler_name, evidence: f.src.ev_range(body.0, body.1, None) };
    let mut op = new_op(f.unit, "ktor", verb, path, handler, evidence);
    let text_body = &code[body.0..body.1];
    // `call.receive<CreateOrder>()`
    if let Some(at) = text_body.find("receive<") {
        let rest = &text_body[at + "receive<".len()..];
        if let Some(end) = rest.find('>') {
            let ty = rest[..end].trim().to_string();
            models.want(f.unit, &ty);
            op.request_body = Some(typed(&ty));
        }
    }
    // `call.respond(HttpStatusCode.Created, order)`
    if let Some(at) = text_body.find("respond") {
        let rest = &text_body[at..];
        if let Some(open) = rest.find('(') {
            if let Some(close) = matching(rest, open) {
                let args = split_top(&rest[open + 1..close], &[',']);
                if let Some(s) = args.first().and_then(|a| status_of(a.trim())) {
                    op.success_status = Some(s);
                }
            }
        }
    }
    // `call.parameters["id"]` / `call.request.queryParameters["q"]`
    for (needle, location) in [("parameters[", "path"), ("queryParameters[", "query")] {
        let mut from = 0;
        while let Some(at) = text_body[from..].find(needle) {
            let start = from + at + needle.len();
            from = start;
            let Some(end) = text_body[start..].find(']') else { break };
            let Some(name) = text::string_lit(f.src.slice(body.0 + start, body.0 + start + end).trim()) else {
                continue;
            };
            let is_path = op.path.contains(&format!("{{{name}}}"));
            add_param(
                &mut op,
                Param {
                    name: name.clone(),
                    code_name: None,
                    location: if is_path { "path" } else { location }.to_string(),
                    type_name: "string".into(),
                    required: is_path,
                    rules: vec![],
                    evidence: idx.ev(fi, f.src.line(body.0 + start)),
                },
            );
        }
    }
    for name in auth {
        add_auth(
            &mut op,
            Requirement { kind: "authenticated".into(), detail: name.clone(), evidence: idx.ev(fi, line) },
        );
    }
    fill_path_params(&mut op);
    let request_declared = op.request_body.is_some();
    ops.push(Draft { op, request_declared, response_declared: false });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kotlin_parameters() {
        let p = parse_param("@RequestParam(required = false) q: String? = null").unwrap();
        assert_eq!((p.name.as_str(), p.type_name.as_str(), p.has_default), ("q", "String?", true));
        let p = parse_param("@PathVariable id: Long").unwrap();
        assert_eq!((p.name.as_str(), p.type_name.as_str(), p.has_default), ("id", "Long", false));
        let p = parse_param("val name: String").unwrap();
        assert_eq!(p.name, "name");
    }

    #[test]
    fn unwraps_kotlin_response_types() {
        assert_eq!(unwrap_kotlin("ResponseEntity<Order>"), ("Order".into(), false));
        assert_eq!(unwrap_kotlin("Flow<Order>"), ("Order".into(), true));
        assert_eq!(unwrap_kotlin("ResponseEntity<List<Order>>"), ("Order".into(), true));
        assert_eq!(unwrap_kotlin("Order?"), ("Order".into(), false));
    }

    #[test]
    fn status_names_of_both_conventions() {
        assert_eq!(status_of("HttpStatus.CREATED"), Some(201));
        assert_eq!(status_of("HttpStatusCode.NoContent"), Some(204));
        assert_eq!(status_of("nonsense"), None);
    }
}
