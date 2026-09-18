//! Rust: axum routers (`route`, `nest`, `merge`, layers) and actix-web (`#[get]` macros,
//! `web::scope`, `web::resource`, `configure`); serde structs with validator attributes.

use std::collections::{BTreeMap, HashMap};

use super::text::*;
use super::*;

const AXUM_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options", "any"];

struct Unit<'f, 'a> {
    unit: &'f str,
    files: Vec<&'f Loaded<'a>>,
    structs: HashMap<String, Vec<Field>>,
}

/// Where a router chain's routes end up: the variable it's bound to or the function returning it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Owner {
    Fn(String),
    Var(String, String),
}

struct Chain {
    file: usize,
    owner: Owner,
    routes: Vec<(String, String, String, usize, usize)>, // method, path, handler, start, end
    nests: Vec<(String, Owner)>,
    auth: Vec<Requirement>,
}

pub(crate) fn extract(files: &[Loaded], h: &mut Harvest) {
    let mut by_unit: BTreeMap<&str, Vec<&Loaded>> = BTreeMap::new();
    for f in files.iter().filter(|f| f.lang == Language::Rust) {
        by_unit.entry(f.unit).or_default().push(f);
    }
    for (unit, fs) in by_unit {
        let mut u = Unit { unit, files: fs, structs: HashMap::new() };
        let first = h.models.len();
        for fi in 0..u.files.len() {
            collect_structs(&u, fi, h);
        }
        u.structs = h.models[first..].iter().map(|m| (m.name.clone(), m.fields.clone())).collect();
        axum(&u, h);
        actix(&u, h);
    }
}

// ---------------------------------------------------------------- axum

fn axum(u: &Unit, h: &mut Harvest) {
    let mut chains: Vec<Chain> = Vec::new();
    for (fi, f) in u.files.iter().enumerate() {
        if !f.src.text.contains("axum") && !f.src.text.contains("Router") {
            continue;
        }
        let code = &f.src.code;
        for at in find_all_str(code, "Router::new()") {
            let src = &f.src;
            let line = src.line(at);
            let fn_name = f.enclosing(line).map(|s| s.name.clone()).unwrap_or_default();
            // `let api = Router::new()…;` binds a variable; otherwise the function returns it.
            let stmt_start = code[..at].rfind([';', '{', '}']).map(|x| x + 1).unwrap_or(0);
            let head = src.code_slice(stmt_start, at).trim();
            let owner = match head
                .strip_prefix("let ")
                .map(|r| r.trim_start_matches("mut ").split(['=', ':']).next().unwrap_or("").trim().to_string())
            {
                Some(var) if !var.is_empty() => Owner::Var(fn_name.clone(), var),
                _ => Owner::Fn(fn_name.clone()),
            };
            let mut chain = Chain { file: fi, owner, routes: vec![], nests: vec![], auth: vec![] };
            for (name, open, close) in super::ts::call_chain(code, at + "Router::new()".len()) {
                let args = split_args(code, open + 1, close);
                match name.as_str() {
                    "route" if args.len() == 2 => {
                        let Some(path) = string_lit(src.slice(args[0].0, args[0].1)) else { continue };
                        // get(h).post(h2) / axum::routing::get(h) / get(h).layer(...)
                        let (ms, me) = args[1];
                        let mut i = ms;
                        while i < me {
                            let Some((ns, ne)) = ident_at(code, i) else {
                                i += 1;
                                continue;
                            };
                            let word = &code[ns..ne];
                            let o = ne;
                            if code.as_bytes().get(o) == Some(&b'(') && AXUM_METHODS.contains(&word) {
                                let c = matching(code, o).unwrap_or(me);
                                let handler = src.slice(o + 1, c).trim();
                                let handler = handler.rsplit("::").next().unwrap_or(handler).to_string();
                                let method = if word == "any" { "ANY".to_string() } else { word.to_uppercase() };
                                chain.routes.push((method, path.clone(), handler, at.min(open), close));
                                i = c + 1;
                            } else {
                                i = ne.max(i + 1);
                            }
                        }
                    }
                    "nest" | "nest_service" if args.len() == 2 => {
                        let Some(prefix) = string_lit(src.slice(args[0].0, args[0].1)) else { continue };
                        let target = src.code_slice(args[1].0, args[1].1).trim();
                        chain.nests.push((prefix, target_owner(target, &fn_name)));
                    }
                    "merge" if args.len() == 1 => {
                        let target = src.code_slice(args[0].0, args[0].1).trim();
                        chain.nests.push((String::new(), target_owner(target, &fn_name)));
                    }
                    "layer" | "route_layer" => {
                        let text = src.slice(open + 1, close);
                        for part in text.split(['(', ',', ')']) {
                            if let Some(a) = auth_requirement(part.trim(), src.ev(open)) {
                                if !part.trim().is_empty() && !chain.auth.iter().any(|x| x.detail == a.detail) {
                                    chain.auth.push(a);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            chains.push(chain);
        }
    }

    // Prefixes for an owner: every chain nesting it, recursively.
    fn prefixes(chains: &[Chain], owner: &Owner, depth: usize) -> Vec<(String, Vec<Requirement>)> {
        let parents: Vec<(&Chain, &String)> =
            chains.iter().flat_map(|c| c.nests.iter().filter(|(_, o)| o == owner).map(move |(p, _)| (c, p))).collect();
        if parents.is_empty() || depth > 8 {
            return vec![(String::new(), vec![])];
        }
        let mut out: Vec<(String, Vec<Requirement>)> = Vec::new();
        for (c, p) in parents {
            for (pp, mut auth) in prefixes(chains, &c.owner, depth + 1) {
                auth.extend(c.auth.iter().cloned());
                let path = join_path(&pp, p);
                if !out.iter().any(|o| o.0 == path) {
                    out.push((path, auth));
                }
            }
        }
        out
    }

    for c in &chains {
        let src = &u.files[c.file].src;
        for (prefix, inherited) in prefixes(&chains, &c.owner, 0) {
            for (method, path, handler, start, end) in &c.routes {
                let full = join_path(&prefix, path);
                let mut op = new_op(
                    u.unit,
                    "axum",
                    method,
                    full,
                    SymbolRef { name: handler.clone(), evidence: src.ev(*start) },
                    src.ev_range(*start, *end, None),
                );
                let route_line = find_route_line(src, *start, *end, path);
                op.evidence = route_line;
                for a in inherited.iter().chain(&c.auth) {
                    add_auth(&mut op, a.clone());
                }
                let mut d = Draft { op, request_declared: false, response_declared: false };
                analyze_fn(u, c.file, handler, &mut d);
                h.ops.push(d);
            }
        }
    }
}

fn find_all_str(hay: &str, needle: &str) -> Vec<usize> {
    hay.match_indices(needle).map(|(i, _)| i).collect()
}

fn find_route_line(src: &Src, start: usize, end: usize, path: &str) -> EvidenceRef {
    let quoted = format!("\"{path}\"");
    let base = super::text::floor_boundary(&src.text, start);
    match src.slice(base, end).find(&quoted) {
        Some(p) => src.ev(base + p),
        None => src.ev(start),
    }
}

fn target_owner(target: &str, fn_name: &str) -> Owner {
    match target.find('(') {
        Some(p) => Owner::Fn(target[..p].rsplit("::").next().unwrap_or("").trim().to_string()),
        None => Owner::Var(fn_name.to_string(), target.to_string()),
    }
}

// ---------------------------------------------------------------- actix

/// handler, prefix, explicit (method, path), registration (file, offset)
type Reg = (String, String, Option<(String, String)>, (usize, usize));

fn actix(u: &Unit, h: &mut Harvest) {
    if !u.files.iter().any(|f| f.src.text.contains("actix_web")) {
        return;
    }
    // Macro handlers: #[get("/path")] async fn name(...)
    let mut macro_routes: HashMap<String, Vec<(usize, String, String, usize)>> = HashMap::new();
    for (fi, f) in u.files.iter().enumerate() {
        let code = &f.src.code;
        for m in ["get", "post", "put", "patch", "delete", "head", "options"] {
            for (at, _) in code.match_indices(&format!("#[{m}(")) {
                let open = at + m.len() + 2;
                let Some(close) = matching(code, open) else { continue };
                let Some(path) =
                    split_args(code, open + 1, close).first().and_then(|&(s, e)| string_lit(f.src.slice(s, e)))
                else {
                    continue;
                };
                let Some(fn_at) = code[close..].find("fn ").map(|x| close + x) else { continue };
                let Some((ns, ne)) = ident_at(code, fn_at + 3) else { continue };
                macro_routes.entry(code[ns..ne].to_string()).or_default().push((fi, m.to_uppercase(), path, at));
            }
        }
    }
    // Scopes: web::scope("/p").service(h).route("/x", web::get().to(h2)).configure(f)
    let mut registered: Vec<Reg> = Vec::new();
    let mut configured: HashMap<String, Vec<String>> = HashMap::new();
    for (fi, f) in u.files.iter().enumerate() {
        let src = &f.src;
        let code = &src.code;
        for at in find_word(code, "scope") {
            let open = at + 5;
            if code.as_bytes().get(open) != Some(&b'(') || at > 0 && code.as_bytes()[at - 1] == b'.' {
                continue;
            }
            let Some(close) = matching(code, open) else { continue };
            let Some(prefix) = string_lit(src.slice(open + 1, close)) else { continue };
            // Only top-level scopes here; nested ones are handled recursively.
            if enclosing_scope(code, at).is_some() {
                continue;
            }
            let outer = scope_prefix_from_fn(u, fi, at, &configured);
            walk_scope(u, fi, close + 1, &join_path(&outer, &prefix), &mut registered, &mut configured);
        }
        for (at, _) in code
            .match_indices(".service(")
            .chain(code.match_indices(".route("))
            .chain(code.match_indices(".configure("))
        {
            if enclosing_scope(code, at).is_some() {
                continue;
            }
            let recv_start = receiver_start(code, at);
            let recv = &code[recv_start..at];
            if recv.contains("scope(") || recv.contains("resource(") {
                continue;
            }
            // App::new()… or cfg.service(…) at the root.
            let outer = scope_prefix_from_fn(u, fi, at, &configured);
            walk_calls(u, fi, &[(at, true)], &outer, &mut registered, &mut configured);
        }
    }
    // Apply configure() prefixes to functions registered through them (one level).
    let mut emitted = std::collections::HashSet::new();
    for (handler, prefix, explicit, (reg_fi, reg_at)) in registered {
        let targets: Vec<(usize, String, String, usize)> = match &explicit {
            // Evidence is the registration; the handler may live in another crate.
            Some((m, p)) => vec![(reg_fi, m.clone(), p.clone(), reg_at)],
            None => macro_routes.get(&handler).cloned().unwrap_or_default(),
        };
        for (fi, method, path, at) in targets {
            let full = if explicit.is_some() { path.clone() } else { join_path(&prefix, &path) };
            if !emitted.insert((method.clone(), full.clone())) {
                continue;
            }
            push_actix(u, h, fi, &handler, &method, full, at, false);
        }
    }
    for (handler, routes) in &macro_routes {
        for (fi, method, path, at) in routes {
            let full = join_path("", path);
            if emitted.iter().any(|(m, p): &(String, String)| m == method && p.ends_with(&full)) {
                continue;
            }
            emitted.insert((method.clone(), full.clone()));
            push_actix(u, h, *fi, handler, method, full, *at, true);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_actix(
    u: &Unit,
    h: &mut Harvest,
    fi: usize,
    handler: &str,
    method: &str,
    path: String,
    at: usize,
    partial: bool,
) {
    let src = &u.files[fi].src;
    let mut op =
        new_op(u.unit, "actix-web", method, path, SymbolRef { name: handler.into(), evidence: src.ev(at) }, src.ev(at));
    op.path_partial = partial;
    let mut d = Draft { op, request_declared: false, response_declared: false };
    analyze_fn(u, fi, handler, &mut d);
    h.ops.push(d);
}

fn enclosing_scope(code: &str, at: usize) -> Option<usize> {
    // Is `at` inside the argument list of a web::scope(...)…service(...) chain?
    let mut depth = 0i32;
    let b = code.as_bytes();
    let mut i = at;
    while i > 0 {
        i -= 1;
        match b[i] {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    let before = &code[..i];
                    if before.ends_with(".service") || before.ends_with(".configure") {
                        let rs = receiver_start(code, i - if before.ends_with(".service") { 8 } else { 10 });
                        if code[rs..i].contains("scope(")
                            || code[rs..i].contains("App::new")
                            || code[rs..i].starts_with("cfg")
                        {
                            return Some(i);
                        }
                    }
                    if before.ends_with("fn") || before.trim_end().ends_with('{') {
                        return None;
                    }
                } else {
                    depth -= 1;
                }
            }
            b'{' | b'}' | b';' if depth == 0 => return None,
            _ => {}
        }
    }
    None
}

fn scope_prefix_from_fn(u: &Unit, fi: usize, at: usize, configured: &HashMap<String, Vec<String>>) -> String {
    let f = u.files[fi];
    f.enclosing(f.src.line(at))
        .and_then(|s| configured.get(&s.name))
        .and_then(|v| v.first().cloned())
        .unwrap_or_default()
}

fn walk_scope(
    u: &Unit,
    fi: usize,
    from: usize,
    prefix: &str,
    registered: &mut Vec<Reg>,
    configured: &mut HashMap<String, Vec<String>>,
) {
    let code = &u.files[fi].src.code;
    let chain: Vec<(usize, bool)> =
        super::ts::call_chain(code, from).into_iter().map(|(n, o, _)| (o - n.len() - 1, true)).collect();
    walk_calls(u, fi, &chain, prefix, registered, configured);
}

fn walk_calls(
    u: &Unit,
    fi: usize,
    calls: &[(usize, bool)],
    prefix: &str,
    registered: &mut Vec<Reg>,
    configured: &mut HashMap<String, Vec<String>>,
) {
    let src = &u.files[fi].src;
    let code = &src.code;
    for &(dot, _) in calls {
        let Some((ns, ne)) = ident_at(code, dot + 1) else { continue };
        let name = &code[ns..ne];
        let open = ne;
        let Some(close) = matching(code, open) else { continue };
        let args = split_args(code, open + 1, close);
        match name {
            "service" => {
                let Some(&(s, e)) = args.first() else { continue };
                let expr = src.code_slice(s, e).trim();
                let web = if expr.starts_with("web::") { 5 } else { 0 };
                if expr[web..].starts_with("scope(") {
                    let open = s + web + 5;
                    let p_close = matching(code, open).unwrap_or(open);
                    let inner_prefix = string_lit(src.slice(open + 1, p_close)).unwrap_or_default();
                    walk_scope(u, fi, p_close + 1, &join_path(prefix, &inner_prefix), registered, configured);
                } else if expr[web..].starts_with("resource(") {
                    let open = s + web + 8;
                    let p_close = matching(code, open).unwrap_or(open);
                    let path = string_lit(src.slice(open + 1, p_close)).unwrap_or_default();
                    for (n, o, c) in super::ts::call_chain(code, p_close + 1) {
                        if n == "route" {
                            if let Some(&(rs, re)) = split_args(code, o + 1, c).first() {
                                route_to(src.code_slice(rs, re), &join_path(prefix, &path), registered, (fi, rs));
                            }
                        }
                    }
                } else if expr.bytes().all(|b| is_ident(b) || b == b':') {
                    let handler = expr.rsplit("::").next().unwrap_or(expr).to_string();
                    registered.push((handler, prefix.to_string(), None, (fi, s)));
                }
            }
            "route" if args.len() == 2 => {
                let Some(path) = string_lit(src.slice(args[0].0, args[0].1)) else { continue };
                route_to(src.code_slice(args[1].0, args[1].1), &join_path(prefix, &path), registered, (fi, args[0].0));
            }
            "configure" => {
                if let Some(&(s, e)) = args.first() {
                    let f = src.code_slice(s, e).trim();
                    let fname = f.rsplit("::").next().unwrap_or(f).to_string();
                    configured.entry(fname).or_default().push(prefix.to_string());
                }
            }
            _ => {}
        }
    }
}

fn route_to(expr: &str, path: &str, registered: &mut Vec<Reg>, at: (usize, usize)) {
    // web::get().to(handler)
    let expr = expr.trim().trim_start_matches("web::");
    let Some(m) = expr
        .split('(')
        .next()
        .filter(|m| matches!(*m, "get" | "post" | "put" | "patch" | "delete" | "head" | "method"))
    else {
        return;
    };
    let Some(p) = expr.find(".to(") else { return };
    let handler = expr[p + 4..].trim_end_matches(')').trim();
    let handler = handler.rsplit("::").next().unwrap_or(handler).to_string();
    registered.push((handler, String::new(), Some((m.to_uppercase(), path.to_string())), at));
}

// ---------------------------------------------------------------- handlers

fn analyze_fn(u: &Unit, fi: usize, name: &str, d: &mut Draft) {
    let found = std::iter::once(fi).chain(0..u.files.len()).find_map(|i| u.files[i].function(name).map(|s| (i, s)));
    let Some((hfi, sym)) = found else { return };
    let f = u.files[hfi];
    let src = &f.src;
    let code = &src.code;
    let (s, e) = f.span(sym);
    d.op.handler.evidence = src.ev_range(s, e, Some(name));
    d.op.summary = sym.doc.as_deref().and_then(first_sentence);
    let Some(name_at) = code[s..e].find(&format!("fn {name}")).map(|x| s + x + 3 + name.len()) else { return };
    let po = if code.as_bytes().get(name_at) == Some(&b'<') {
        matching(code, name_at).map(|c| c + 1).unwrap_or(name_at)
    } else {
        name_at
    };
    let Some(pc) = matching(code, po) else { return };
    let Some(bo) = code[pc..e].find('{').map(|x| pc + x) else { return };
    let ret = src.slice(pc + 1, bo).trim().trim_start_matches("->").trim().to_string();
    let path_names = path_params(&d.op.path);
    let mut path_i = 0;
    for (ps, pe) in split_args(code, po + 1, pc) {
        let p = src.slice(ps, pe);
        let Some((pat, ty)) = p.split_once(':') else { continue };
        let ty = ty.trim();
        let ty_head = ty.split('<').next().unwrap_or(ty).trim().rsplit("::").next().unwrap_or("");
        let inner =
            ty.find('<').map(|o| ty[o + 1..ty.rfind('>').unwrap_or(ty.len())].trim().to_string()).unwrap_or_default();
        let ev = src.ev(ps);
        match ty_head {
            "Json" | "Form" => {
                d.op.request_body = Some(type_ref(&inner));
                d.request_declared = true;
            }
            "Path" => {
                let types: Vec<&str> = if inner.starts_with('(') {
                    split_top(inner.trim_matches(['(', ')']), &[','])
                } else {
                    vec![inner.as_str()]
                };
                if types.len() == 1 && !is_scalar(types[0]) {
                    continue;
                }
                let names: Vec<String> = pat
                    .trim()
                    .trim_start_matches("Path")
                    .trim_matches(['(', ')'])
                    .split(',')
                    .map(|n| n.trim().to_string())
                    .collect();
                for t in types {
                    let Some(wire) = path_names.get(path_i).cloned() else { break };
                    let code_name = names.get(path_i).filter(|n| **n != wire && !n.is_empty()).cloned();
                    add_param(
                        &mut d.op,
                        Param {
                            name: wire,
                            code_name,
                            location: "path".into(),
                            type_name: t.to_string(),
                            required: true,
                            rules: vec![],
                            evidence: ev.clone(),
                        },
                    );
                    path_i += 1;
                }
            }
            "Query" => {
                for f in u.structs.get(&unwrap_type(&inner).0).into_iter().flatten() {
                    add_param(
                        &mut d.op,
                        Param {
                            name: f.name.clone(),
                            code_name: f.code_name.clone(),
                            location: "query".into(),
                            type_name: f.type_name.clone(),
                            required: f.required,
                            rules: f.rules.clone(),
                            evidence: f.evidence.clone(),
                        },
                    );
                }
            }
            "State" | "Extension" | "Data" | "HttpRequest" | "HeaderMap" | "Request" | "ConnectInfo" | "Payload" => {}
            other => {
                if let Some(a) = auth_requirement(other, ev.clone()).or_else(|| {
                    (other.contains("User") || other.contains("Claims") || other.contains("Session"))
                        .then(|| Requirement { kind: "authenticated".into(), detail: other.to_string(), evidence: ev })
                }) {
                    add_auth(&mut d.op, Requirement { detail: format!("extractor {other}"), ..a });
                }
            }
        }
    }
    // Return type: Json<T>, Result<Json<T>, E>, (StatusCode, Json<T>), web::Json<T>.
    if let Some(p) = ret.find("Json<") {
        let inner = &ret[p + 5..];
        let mut depth = 1;
        let mut end = inner.len();
        for (i, ch) in inner.char_indices() {
            match ch {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i;
                        break;
                    }
                }
                _ => {}
            }
        }
        d.op.response = Some(type_ref(&inner[..end]));
        d.response_declared = true;
    }
    let body = src.slice(bo, e);
    for (at, _) in body.match_indices("StatusCode::") {
        let name: String =
            body[at + 12..].chars().take_while(|c| c.is_ascii_uppercase() || *c == '_' || c.is_ascii_digit()).collect();
        let Some(st) = status_code(&name) else { continue };
        if st >= 400 {
            add_error(&mut d.op, Some(st), None, src.ev(bo + at));
        } else if st != 200 {
            d.op.success_status.get_or_insert(st);
        }
    }
    for (at, _) in body.match_indices("HttpResponse::") {
        let name: String = body[at + 14..].chars().take_while(|c| c.is_alphanumeric()).collect();
        let Some(st) = status_code(&name) else { continue };
        if st >= 400 {
            add_error(&mut d.op, Some(st), None, src.ev(bo + at));
        } else {
            d.op.success_status.get_or_insert(st);
            // HttpResponse::Created().json(Item { … })
            let rest = &body[at + 14 + name.len()..];
            if d.op.response.is_none() {
                if let Some(arg) =
                    rest.trim_start().strip_prefix("()").and_then(|r| r.trim_start().strip_prefix(".json("))
                {
                    let ty: String =
                        arg.trim_start().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                    if ty.chars().next().is_some_and(|c| c.is_uppercase()) {
                        d.op.response = Some(type_ref(&ty));
                        d.response_declared = true;
                    }
                }
            }
        }
    }
    if ret.contains("StatusCode") && !ret.contains("Json") {
        for (at, _) in ret.match_indices("StatusCode") {
            let _ = at;
        }
    }
    if let Some(p) = ret.find("Result<") {
        let parts = split_top(&ret[p + 7..ret.rfind('>').unwrap_or(ret.len())], &[',']);
        if parts.get(1).is_some_and(|e| e.trim() == "StatusCode") && d.op.errors.is_empty() {
            add_error(&mut d.op, None, Some("status code chosen at runtime".into()), src.ev(pc));
        }
    }
}

fn is_scalar(t: &str) -> bool {
    matches!(
        t.trim(),
        "String" | "&str" | "i32" | "i64" | "u32" | "u64" | "usize" | "Uuid" | "uuid::Uuid" | "i16" | "u16" | "bool"
    )
}

// ---------------------------------------------------------------- serde structs

fn collect_structs(u: &Unit, fi: usize, h: &mut Harvest) {
    let src = &u.files[fi].src;
    let code = &src.code;
    for at in find_word(code, "struct") {
        let Some((ns, ne)) = ident_at(code, at + 6) else { continue };
        let open = code[ne..].find(['{', ';', '(']).map(|x| ne + x);
        let Some(open) = open.filter(|o| code.as_bytes()[*o] == b'{') else { continue };
        let Some(close) = matching(code, open) else { continue };
        // Attributes above the struct.
        let attr_start = struct_attrs_start(src, at);
        let attrs = src.slice(attr_start, at);
        if !(attrs.contains("Serialize") || attrs.contains("Deserialize") || attrs.contains("Validate")) {
            continue;
        }
        let rename_all = attr_value(attrs, "rename_all");
        let name = src.slice(ns, ne).to_string();
        let mut fields = Vec::new();
        let mut pending_attrs = String::new();
        let mut pending_start = None;
        for (s, e) in split_args(code, open + 1, close) {
            // Leading attributes of the field live in the same piece.
            let piece = src.slice(s, e);
            let mut rest = piece;
            let mut offset = s;
            loop {
                let t = rest.trim_start();
                offset += rest.len() - t.len();
                rest = t;
                if rest.starts_with("#[") {
                    let Some(c) = matching(code, offset + 1) else { break };
                    pending_attrs.push_str(src.slice(offset, c + 1));
                    pending_start.get_or_insert(offset);
                    let consumed = c + 2 - offset;
                    rest = &rest[consumed.min(rest.len())..];
                    offset = c + 2;
                } else if rest.starts_with("///") || rest.starts_with("//") {
                    let nl = rest.find('\n').unwrap_or(rest.len());
                    rest = &rest[nl..];
                    offset += nl;
                } else {
                    break;
                }
            }
            let decl = rest.trim().trim_start_matches("pub(crate) ").trim_start_matches("pub ");
            let Some((fname, ty)) = decl.split_once(':') else {
                pending_attrs.clear();
                continue;
            };
            let fname = fname.trim().to_string();
            let ty = ty.trim().to_string();
            let fattrs = std::mem::take(&mut pending_attrs);
            pending_start = None;
            if fattrs.contains("skip") && !fattrs.contains("skip_serializing_if") {
                continue;
            }
            let ev = src.ev(offset);
            let wire = attr_value(&fattrs, "rename").unwrap_or_else(|| apply_rename_all(&fname, rename_all.as_deref()));
            let required = !(ty.starts_with("Option<") || fattrs.contains("default"));
            let rules = validator_rules(&fattrs, &ty, &ev);
            let doc = doc_above(src, ev.start_line, Style::Rust);
            fields.push(Field {
                code_name: (wire != fname).then(|| fname.clone()),
                name: wire,
                type_name: ty,
                required,
                rules,
                doc,
                model: None,
                evidence: ev,
            });
        }
        let _ = pending_start;
        h.model(Model {
            id: format!("{}:{name}", u.unit),
            unit: u.unit.into(),
            name,
            fields,
            doc: doc_above(src, src.line(at), Style::Rust),
            evidence: src.ev_range(at, close, None),
        });
    }
}

fn struct_attrs_start(src: &Src, at: usize) -> usize {
    let mut line = src.line(at);
    while line > 1 {
        let prev = src.slice(src.line_start(line - 1), src.line_end(line - 1)).trim();
        if prev.starts_with("#[")
            || prev.starts_with("///")
            || prev.ends_with(")]")
            || prev.starts_with("pub") && prev.ends_with("struct")
        {
            line -= 1;
        } else {
            break;
        }
    }
    src.line_start(line)
}

fn attr_value(attrs: &str, key: &str) -> Option<String> {
    let mut from = 0;
    while let Some(p) = attrs[from..].find(key) {
        let at = from + p;
        let rest = attrs[at + key.len()..].trim_start();
        let before_ok =
            at == 0 || !attrs.as_bytes()[at - 1].is_ascii_alphanumeric() && attrs.as_bytes()[at - 1] != b'_';
        if before_ok {
            if let Some(v) = rest.strip_prefix('=') {
                let v = v.trim_start();
                if let Some(inner) = v.strip_prefix('"') {
                    return inner.find('"').map(|e| inner[..e].to_string());
                }
            }
        }
        from = at + key.len();
    }
    None
}

fn apply_rename_all(name: &str, style: Option<&str>) -> String {
    match style {
        Some("camelCase") => camel(name),
        Some("PascalCase") => pascal(name),
        Some("kebab-case") => name.replace('_', "-"),
        Some("SCREAMING_SNAKE_CASE") => name.to_uppercase(),
        Some("lowercase") => name.to_lowercase(),
        _ => name.to_string(),
    }
}

fn validator_rules(attrs: &str, ty: &str, ev: &EvidenceRef) -> Vec<Rule> {
    let mut out = Vec::new();
    let Some(p) = attrs.find("validate(") else { return out };
    let inner = &attrs[p + 9..];
    let inner = &inner[..inner.rfind(')').unwrap_or(inner.len())];
    let is_vec = ty.contains("Vec<");
    for part in split_top(inner, &[',']) {
        let (name, args) =
            part.split_once('(').map(|(a, b)| (a.trim(), b.trim_end_matches(')'))).unwrap_or((part.trim(), ""));
        let kv: HashMap<&str, &str> = args
            .split(',')
            .filter_map(|x| x.split_once('='))
            .map(|(k, v)| (k.trim(), v.trim().trim_matches('"')))
            .collect();
        match name {
            "length" => {
                if let Some(v) = kv.get("min") {
                    out.push(rule(if is_vec { min_items(v) } else { min_len(v) }, "length", ev));
                }
                if let Some(v) = kv.get("max") {
                    out.push(rule(if is_vec { max_items(v) } else { max_len(v) }, "length", ev));
                }
                if let Some(v) = kv.get("equal") {
                    out.push(rule(format!("exactly {v} characters"), "length", ev));
                }
            }
            "range" => {
                if let Some(v) = kv.get("min") {
                    out.push(rule(format!("must be ≥ {v}"), "range", ev));
                }
                if let Some(v) = kv.get("max") {
                    out.push(rule(format!("must be ≤ {v}"), "range", ev));
                }
            }
            "email" => out.push(rule("must be a valid email", "format", ev)),
            "url" => out.push(rule("must be a valid URL", "format", ev)),
            "regex" => out.push(rule(format!("must match {}", kv.get("path").copied().unwrap_or(args)), "pattern", ev)),
            "custom" => {
                out.push(rule(format!("custom: {}", kv.get("function").copied().unwrap_or(args)), "custom", ev))
            }
            "required" => out.push(rule("must be present", "required", ev)),
            _ => {}
        }
    }
    out
}
