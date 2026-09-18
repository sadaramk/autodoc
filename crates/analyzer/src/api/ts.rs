//! TypeScript / JavaScript: Express, Fastify, Hono, Koa routers and NestJS controllers;
//! interfaces, type literals, DTO classes (class-validator) and zod schemas.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::text::*;
use super::*;

const METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "options", "head", "all"];
const APPISH: &[&str] = &["app", "router", "server", "fastify", "instance"];

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Key {
    file: usize,
    name: String,
}

struct Decl {
    root: bool,
    framework: &'static str,
    own_prefix: String,
}

struct Mount {
    parent: Key,
    child: Key,
    prefix: Option<String>,
    auth: Vec<Requirement>,
}

struct Route {
    key: Key,
    file: usize,
    method: String,
    path: String,
    call: (usize, usize),
    args: Vec<(usize, usize)>,
    generic: Option<(usize, usize)>,
}

struct Unit<'f, 'a> {
    files: Vec<&'f Loaded<'a>>,
    imports: Vec<HashMap<String, (usize, String)>>,
    default_exports: Vec<Option<String>>,
    decls: BTreeMap<Key, Decl>,
    zod: HashMap<String, String>,
    fn_returns: HashMap<String, String>,
    /// `Enum.Member` and `CONST` string values (controller prefixes).
    consts: HashMap<String, String>,
}

pub(crate) fn extract(files: &[Loaded], h: &mut Harvest) {
    let mut by_unit: BTreeMap<&str, Vec<&Loaded>> = BTreeMap::new();
    for f in files.iter().filter(|f| matches!(f.lang, Language::TypeScript | Language::JavaScript)) {
        by_unit.entry(f.unit).or_default().push(f);
    }
    for (unit, fs) in by_unit {
        extract_unit(unit, fs, h);
    }
}

fn extract_unit(unit: &str, files: Vec<&Loaded>, h: &mut Harvest) {
    let paths: Vec<&str> = files.iter().map(|f| f.path()).collect();
    let imports = files.iter().map(|f| parse_imports(f, &paths)).collect();
    let default_exports = files.iter().map(|f| default_export(&f.src)).collect();
    let mut u = Unit {
        files,
        imports,
        default_exports,
        decls: BTreeMap::new(),
        zod: HashMap::new(),
        fn_returns: HashMap::new(),
        consts: HashMap::new(),
    };

    // Models first: handlers resolve zod schema names and function return types against them.
    let mut models: Vec<(Model, Vec<String>)> = Vec::new();
    for fi in 0..u.files.len() {
        collect_models(&mut u, unit, fi, &mut models);
        collect_fn_returns(&mut u, fi);
    }
    let names: BTreeSet<String> = models.iter().map(|(m, _)| m.name.clone()).collect();
    let snapshot: HashMap<String, Vec<Field>> =
        models.iter().map(|(m, _)| (m.name.clone(), m.fields.clone())).collect();
    for (m, parents) in &mut models {
        let mut inherited: Vec<Field> = Vec::new();
        if let Some(marker) = parents.get(1).filter(|p| {
            p.starts_with(['P', 'O', 'R'])
                && p.chars().next().is_some_and(|c| c.is_uppercase())
                && !snapshot.contains_key(*p)
        }) {
            let (util, keys) = marker.split_once(':').unwrap_or((marker, ""));
            let keys: Vec<&str> = keys.split(',').filter(|k| !k.is_empty()).collect();
            if let Some(fs) = snapshot.get(&parents[0]) {
                for f in fs {
                    let keep = match util {
                        "Pick" => keys.contains(&f.name.as_str()),
                        "Omit" => !keys.contains(&f.name.as_str()),
                        _ => true,
                    };
                    if keep {
                        let mut f = f.clone();
                        match util {
                            "Partial" => f.required = false,
                            "Required" => f.required = true,
                            _ => {}
                        }
                        inherited.push(f);
                    }
                }
            }
            parents.clear();
        }
        for p in parents.iter() {
            if let Some(fs) = snapshot.get(p) {
                inherited.extend(fs.iter().filter(|f| !m.fields.iter().any(|x| x.name == f.name)).cloned());
            }
        }
        inherited.append(&mut m.fields);
        m.fields = inherited;
    }
    let model_fields: HashMap<String, Vec<Field>> =
        models.iter().map(|(m, _)| (m.name.clone(), m.fields.clone())).collect();
    for (m, _) in models {
        h.model(m);
    }

    for fi in 0..u.files.len() {
        collect_decls(&mut u, fi);
    }
    let mut routes = Vec::new();
    let mut mounts = Vec::new();
    let mut router_auth: HashMap<Key, Vec<Requirement>> = HashMap::new();
    for fi in 0..u.files.len() {
        collect_routes(&u, fi, &mut routes);
    }
    let fn_keys: BTreeSet<Key> = routes.iter().map(|r| r.key.clone()).filter(|k| k.name.starts_with("fn:")).collect();
    for fi in 0..u.files.len() {
        collect_mounts(&u, fi, &fn_keys, &mut mounts, &mut router_auth);
    }

    let framework_default = unit_framework(&u);
    let mut synthesized: Vec<Model> = Vec::new();
    for r in &routes {
        let decl = u.decls.get(&r.key);
        let framework = decl.map(|d| d.framework).unwrap_or(framework_default);
        for (prefix, partial, inherited_auth) in prefixes(&u, &r.key, &mounts, &router_auth, 0) {
            let path = join_path(&prefix, &r.path);
            let f = u.files[r.file];
            let mut op = new_op(
                unit,
                framework,
                &r.method,
                path,
                SymbolRef { name: String::new(), evidence: f.src.ev(r.call.0) },
                f.src.ev_range(r.call.0, r.call.1, None),
            );
            op.path_partial = partial;
            for a in inherited_auth {
                add_auth(&mut op, a);
            }
            let mut draft = Draft { op, request_declared: false, response_declared: false };
            analyze_route(&u, r, &mut draft, &model_fields, &names, &mut synthesized);
            h.ops.push(draft);
        }
    }
    nest(&u, unit, &model_fields, h);
    for m in synthesized {
        h.model(m);
    }
}

// ---------------------------------------------------------------- imports

fn parse_imports(f: &Loaded, paths: &[&str]) -> HashMap<String, (usize, String)> {
    let mut out = HashMap::new();
    let src = &f.src;
    for at in find_word(&src.code, "import") {
        // The clause ends at `from "<spec>"`.
        let window_end = (at + 600).min(src.code.len());
        let Some(from) = src.code[at..window_end].find(" from ").map(|x| at + x) else { continue };
        let clause = src.slice(at + 6, from).trim().trim_start_matches("type ").trim();
        let spec_start = skip_ws(&src.code, from + 6);
        let spec_end = src.code[spec_start + 1..].find(['"', '\'']).map(|x| spec_start + 1 + x).unwrap_or(spec_start);
        let spec = src.slice(spec_start + 1, spec_end);
        let Some(target) = resolve_spec(src.path.as_str(), spec, paths) else { continue };
        let (default, named) = match clause.find('{') {
            Some(b) => (
                clause[..b].trim().trim_end_matches(',').trim(),
                Some(&clause[b + 1..clause.rfind('}').unwrap_or(clause.len())]),
            ),
            None => (clause, None),
        };
        if let Some(ns) = default.strip_prefix("* as ") {
            out.insert(ns.trim().to_string(), (target, "*".to_string()));
        } else if !default.is_empty() && default.bytes().all(is_ident) {
            out.insert(default.to_string(), (target, "default".to_string()));
        }
        for item in named.unwrap_or("").split(',') {
            let item = item.trim().trim_start_matches("type ").trim();
            if item.is_empty() {
                continue;
            }
            let (orig, local) = item.split_once(" as ").map(|(a, b)| (a.trim(), b.trim())).unwrap_or((item, item));
            out.insert(local.to_string(), (target, orig.to_string()));
        }
    }
    out
}

fn resolve_spec(from: &str, spec: &str, paths: &[&str]) -> Option<usize> {
    if !spec.starts_with('.') {
        return None;
    }
    let dir = from.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let mut parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    for seg in spec.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    let base = parts.join("/");
    let stem = base.trim_end_matches(".js").trim_end_matches(".jsx").trim_end_matches(".mjs");
    for cand in [
        base.clone(),
        format!("{stem}.ts"),
        format!("{stem}.tsx"),
        format!("{stem}.js"),
        format!("{stem}.jsx"),
        format!("{stem}.mjs"),
        format!("{stem}/index.ts"),
        format!("{stem}/index.js"),
    ] {
        if let Some(i) = paths.iter().position(|p| *p == cand) {
            return Some(i);
        }
    }
    None
}

fn default_export(src: &Src) -> Option<String> {
    let at = src.code.find("export default ")?;
    let rest = &src.code[at + 15..];
    let rest =
        rest.trim_start().trim_start_matches("async ").trim_start_matches("function ").trim_start_matches("class ");
    let name: String = rest.bytes().take_while(|&b| is_ident(b)).map(|b| b as char).collect();
    (!name.is_empty()).then_some(name)
}

/// Resolves an identifier used in file `fi` to the file and name that define it.
fn resolve_ident(u: &Unit, fi: usize, ident: &str) -> (usize, String) {
    let (head, member) = ident.split_once('.').map(|(a, b)| (a, Some(b))).unwrap_or((ident, None));
    if let Some((target, orig)) = u.imports[fi].get(head) {
        return match (orig.as_str(), member) {
            ("*", Some(m)) => (*target, m.to_string()),
            ("default", _) => (*target, u.default_exports[*target].clone().unwrap_or_else(|| "default".into())),
            _ => (*target, orig.clone()),
        };
    }
    (fi, ident.to_string())
}

// ---------------------------------------------------------------- routers

fn collect_decls(u: &mut Unit, fi: usize) {
    let src = &u.files[fi].src;
    let mut found = Vec::new();
    for kw in ["const", "let", "var"] {
        for at in find_word(&src.code, kw) {
            let Some((ns, ne)) = ident_at(&src.code, at + kw.len()) else { continue };
            let mut i = skip_ws(&src.code, ne);
            if src.code.as_bytes().get(i) == Some(&b':') {
                // Type annotation: skip to '='.
                match src.code[i..].find('=') {
                    Some(x) => i += x,
                    None => continue,
                }
            }
            if src.code.as_bytes().get(i) != Some(&b'=') {
                continue;
            }
            let init_start = skip_ws(&src.code, i + 1);
            let init = src.code_slice(init_start, (init_start + 200).min(src.code.len()));
            let init = init.trim_start_matches("await ").trim_start_matches("new ");
            let (root, framework) = if init.starts_with("express()") {
                (true, "express")
            } else if init.starts_with("express.Router(") || init.starts_with("Router(") {
                (false, "express")
            } else if init.starts_with("Fastify(") || init.starts_with("fastify(") {
                (true, "fastify")
            } else if init.starts_with("Hono(") || init.starts_with("Hono<") || init.starts_with("OpenAPIHono(") {
                (true, "hono")
            } else if init.starts_with("KoaRouter(") || init.starts_with("Router({") && src.text.contains("koa") {
                (false, "koa")
            } else if init.starts_with("Elysia(") {
                (true, "elysia")
            } else {
                continue;
            };
            let mut own_prefix = String::new();
            let stmt_end = src.code[init_start..].find([';', '\n']).map(|x| init_start + x).unwrap_or(src.code.len());
            let stmt = src.slice(init_start, stmt_end);
            if let Some(p) = stmt.find(".basePath(").or_else(|| stmt.find("prefix:")) {
                let rest = &stmt[p..];
                if let Some(q) = rest.find(['"', '\'']) {
                    let quote = rest.as_bytes()[q] as char;
                    if let Some(end) = rest[q + 1..].find(quote) {
                        own_prefix = rest[q + 1..q + 1 + end].to_string();
                    }
                }
            }
            found.push((src.slice(ns, ne).to_string(), Decl { root, framework, own_prefix }));
        }
    }
    for (name, d) in found {
        u.decls.insert(Key { file: fi, name }, d);
    }
}

fn unit_framework(u: &Unit) -> &'static str {
    let all: String = u.files.iter().flat_map(|f| u_imports_text(f)).collect();
    if all.contains("fastify") {
        "fastify"
    } else if all.contains("hono") {
        "hono"
    } else if all.contains("koa") {
        "koa"
    } else {
        "express"
    }
}

fn u_imports_text(f: &Loaded) -> Vec<String> {
    f.src
        .text
        .lines()
        .filter(|l| l.trim_start().starts_with("import") || l.contains("require("))
        .map(str::to_string)
        .collect()
}

/// Router key for a receiver expression used in file `fi` at `offset`, if it is a router.
fn receiver_key(u: &Unit, fi: usize, recv: &str, offset: usize) -> Option<Key> {
    let f = u.files[fi];
    let head: String = recv.bytes().take_while(|&b| is_ident(b)).map(|b| b as char).collect();
    if head.is_empty() {
        return None;
    }
    let local = Key { file: fi, name: head.clone() };
    if u.decls.contains_key(&local) {
        return Some(local);
    }
    let (tf, tn) = resolve_ident(u, fi, &head);
    let imported = Key { file: tf, name: tn };
    if tf != fi && u.decls.contains_key(&imported) {
        return Some(imported);
    }
    // A function parameter that receives the app (`function routes(app: FastifyInstance)`).
    let declared_elsewhere = ["const ", "let ", "var "].iter().any(|kw| f.src.code.contains(&format!("{kw}{head} =")));
    let typed = f.src.text.contains(&format!("{head}: FastifyInstance"))
        || f.src.text.contains(&format!("{head}: Router"))
        || f.src.text.contains(&format!("{head}: Express"))
        || f.src.text.contains(&format!("{head}: Application"))
        || f.src.text.contains(&format!("{head}: Hono"));
    if declared_elsewhere || !(typed || APPISH.contains(&head.as_str())) {
        return None;
    }
    // Inline plugin registered with a prefix: `app.register(async (i) => { i.get(...) }, { prefix })`.
    let line = f.src.line(offset);
    let name = match f.enclosing(line) {
        Some(sym) if sym.start_line < line => sym.name.clone(),
        _ => u.default_exports[fi].clone().unwrap_or_else(|| "module".into()),
    };
    Some(Key { file: fi, name: format!("fn:{name}") })
}

fn collect_routes(u: &Unit, fi: usize, out: &mut Vec<Route>) {
    let src = &u.files[fi].src;
    for (rs, ms, open) in method_calls(&src.code, METHODS) {
        let Some(close) = matching(&src.code, open) else { continue };
        let args = split_args(&src.code, open + 1, close);
        let Some(&(ps, pe)) = args.first() else { continue };
        let recv = src.code_slice(rs, ms - 1).to_string();
        let (recv_base, path) = if let Some(rp) = recv.find(".route(") {
            let inner_open = rs + rp + 6;
            let Some(inner_close) = matching(&src.code, inner_open) else { continue };
            let Some(p) = string_lit(src.slice(inner_open + 1, inner_close)) else { continue };
            (recv[..rp].to_string(), p)
        } else {
            match string_lit(src.slice(ps, pe)) {
                Some(p) => (recv.clone(), p),
                None => continue,
            }
        };
        if !(path.starts_with('/') || path == "*" || path.is_empty()) {
            continue;
        }
        let Some(key) = receiver_key(u, fi, &recv_base, rs) else { continue };
        let method = src.slice(ms, ms + src.code[ms..].bytes().take_while(|&b| is_ident(b)).count()).to_string();
        let method = if method == "all" { "ANY".to_string() } else { method.to_uppercase() };
        let name_end = ms + method.len();
        let generic = (src.code.as_bytes().get(skip_ws(&src.code, name_end)) == Some(&b'<'))
            .then(|| {
                let g = skip_ws(&src.code, name_end);
                matching(&src.code, g).map(|e| (g + 1, e))
            })
            .flatten();
        let route_args = if recv.contains(".route(") { args } else { args[1..].to_vec() };
        out.push(Route { key, file: fi, method, path, call: (rs, close), args: route_args, generic });
    }
}

fn collect_mounts(
    u: &Unit,
    fi: usize,
    fn_keys: &BTreeSet<Key>,
    mounts: &mut Vec<Mount>,
    router_auth: &mut HashMap<Key, Vec<Requirement>>,
) {
    let src = &u.files[fi].src;
    let router_of = |expr: &str| -> Option<Key> {
        let e = expr.trim();
        let ident: String = e.bytes().take_while(|&b| is_ident(b) || b == b'.').map(|b| b as char).collect();
        if ident.is_empty() {
            return None;
        }
        let (tf, tn) = resolve_ident(u, fi, &ident);
        let direct = Key { file: tf, name: tn.clone() };
        if u.decls.contains_key(&direct) {
            return Some(direct);
        }
        let fk = Key { file: tf, name: format!("fn:{tn}") };
        if fn_keys.contains(&fk) {
            return Some(fk);
        }
        // Factory: `app.use("/orders", ordersRouter())` where the function returns a Router.
        if e[ident.len()..].trim_start().starts_with('(') {
            let f = u.files[tf];
            if let Some(sym) = f.function(&tn) {
                let (s, end) = f.span(sym);
                for at in find_word(f.src.code_slice(s, end), "return") {
                    if let Some((a, b)) = ident_at(&f.src.code, s + at + 6) {
                        let k = Key { file: tf, name: f.src.slice(a, b).to_string() };
                        if u.decls.contains_key(&k) {
                            return Some(k);
                        }
                    }
                }
            }
        }
        None
    };

    for (rs, ms, open) in method_calls(&src.code, &["use", "route", "register"]) {
        let method = &src.code[ms..open];
        let Some(close) = matching(&src.code, open) else { continue };
        let recv = src.code_slice(rs, ms - 1).to_string();
        let Some(parent) = receiver_key(u, fi, &recv, rs) else { continue };
        let args = split_args(&src.code, open + 1, close);
        if args.is_empty() {
            continue;
        }
        let ev = src.ev_range(rs, close, None);
        match method.trim() {
            "use" | "route" => {
                let first = src.slice(args[0].0, args[0].1);
                let (prefix, rest) = match string_lit(first) {
                    Some(p) => (Some(p), &args[1..]),
                    None if first.trim_start().starts_with(['"', '\'', '`']) => (None, &args[1..]),
                    None => (Some(String::new()), &args[..]),
                };
                if method.trim() == "route" && args.len() != 2 {
                    continue;
                }
                let mut children = Vec::new();
                let mut auth = Vec::new();
                for &(s, e) in rest {
                    let expr = src.slice(s, e);
                    if let Some(k) = router_of(expr) {
                        children.push(k);
                    } else if let Some(a) = auth_requirement(expr, ev.clone()) {
                        auth.push(a);
                    }
                }
                if children.is_empty() {
                    router_auth.entry(parent.clone()).or_default().extend(auth);
                    continue;
                }
                for child in children {
                    if child != parent {
                        mounts.push(Mount {
                            parent: parent.clone(),
                            child,
                            prefix: prefix.clone(),
                            auth: auth.clone(),
                        });
                    }
                }
            }
            _ => {
                // Fastify: register(plugin, { prefix: "/x" }).
                let plugin = src.slice(args[0].0, args[0].1);
                let prefix = args.get(1).and_then(|&(s, _)| {
                    (src.code.as_bytes()[s] == b'{')
                        .then(|| object_entries(src, s).into_iter().find(|(k, ..)| k == "prefix"))
                        .flatten()
                        .map(|(_, _, vs, ve)| string_lit(src.slice(vs, ve)))
                });
                let prefix = match prefix {
                    None => Some(String::new()),
                    Some(p) => p,
                };
                let child = if plugin.trim_start().starts_with("async")
                    || plugin.trim_start().starts_with('(')
                    || plugin.contains("=>")
                {
                    // Inline plugin: routes inside are keyed by the function enclosing the register call.
                    continue;
                } else {
                    router_of(plugin)
                };
                if let Some(child) = child {
                    mounts.push(Mount { parent, child, prefix, auth: vec![] });
                }
            }
        }
    }

    // Direct calls: `registerNoteRoutes(app, store)`.
    for key in fn_keys {
        let fname = &key.name[3..];
        let locals: Vec<String> = if key.file == fi {
            vec![fname.to_string()]
        } else {
            u.imports[fi].iter().filter(|(_, (t, o))| *t == key.file && o == fname).map(|(l, _)| l.clone()).collect()
        };
        for local in locals {
            for at in find_word(&src.code, &local) {
                let b = src.code.as_bytes();
                if at > 0 && b[at - 1] == b'.' {
                    continue;
                }
                let before = src.code[..at].trim_end();
                if before.ends_with("function") || before.ends_with("import {") || before.ends_with("export") {
                    continue;
                }
                let open = skip_ws(&src.code, at + local.len());
                if b.get(open) != Some(&b'(') {
                    continue;
                }
                let Some(close) = matching(&src.code, open) else { continue };
                let args = split_args(&src.code, open + 1, close);
                let Some(&(s, e)) = args.first() else { continue };
                if let Some(parent) = receiver_key(u, fi, src.slice(s, e), s) {
                    if parent != *key {
                        mounts.push(Mount { parent, child: key.clone(), prefix: Some(String::new()), auth: vec![] });
                    }
                }
            }
        }
    }
}

type Prefixed = (String, bool, Vec<Requirement>);

fn prefixes(
    u: &Unit,
    key: &Key,
    mounts: &[Mount],
    router_auth: &HashMap<Key, Vec<Requirement>>,
    depth: usize,
) -> Vec<Prefixed> {
    let own = u.decls.get(key).map(|d| d.own_prefix.clone()).unwrap_or_default();
    let own_auth = router_auth.get(key).cloned().unwrap_or_default();
    let parents: Vec<&Mount> = mounts.iter().filter(|m| m.child == *key).collect();
    if parents.is_empty() || depth > 8 {
        let root = u.decls.get(key).map(|d| d.root).unwrap_or(false);
        return vec![(own, !root, own_auth)];
    }
    let mut out: Vec<Prefixed> = Vec::new();
    for m in parents {
        for (pp, partial, mut auth) in prefixes(u, &m.parent, mounts, router_auth, depth + 1) {
            let joined = join_path(&join_path(&pp, m.prefix.as_deref().unwrap_or("")), &own);
            auth.extend(m.auth.iter().cloned());
            auth.extend(own_auth.iter().cloned());
            let entry = (joined, partial || m.prefix.is_none(), auth);
            if !out.iter().any(|o| o.0 == entry.0) {
                out.push(entry);
            }
        }
    }
    out
}

// ---------------------------------------------------------------- handlers

struct Handler {
    file: usize,
    body: (usize, usize),
    params: Vec<String>,
    /// Parameter list text (for `Request<P, ResBody, ReqBody, Query>` annotations).
    param_text: String,
}

fn find_handler(u: &Unit, r: &Route) -> (String, Option<Handler>) {
    let src = &u.files[r.file].src;
    let Some(&(hs, he)) = r.args.last() else { return (String::new(), None) };
    let mut expr = src.slice(hs, he).trim().to_string();
    let mut start = hs;
    // Unwrap `asyncHandler(fn)` / `catchAsync(async (req, res) => …)`.
    for _ in 0..2 {
        if let Some(p) = expr.find('(') {
            let head = &expr[..p];
            if head.bytes().all(|b| is_ident(b) || b == b'.')
                && !head.is_empty()
                && !matches!(head, "async" | "function")
            {
                let open = start + p;
                if let Some(close) = matching(&src.code, open) {
                    if let Some(&(s, e)) = split_args(&src.code, open + 1, close).last() {
                        start = s;
                        expr = src.slice(s, e).to_string();
                        continue;
                    }
                }
            }
        }
        break;
    }
    let inline =
        expr.starts_with("async") || expr.starts_with('(') || expr.starts_with("function") || expr.contains("=>");
    if inline {
        return (String::new(), inline_handler(&src.code, r.file, start, he, src));
    }
    let ident = expr.trim_end_matches(".bind(").split(".bind(").next().unwrap_or(&expr).to_string();
    let (tf, tn) = resolve_ident(u, r.file, &ident);
    let member = tn.rsplit('.').next().unwrap_or(&tn).to_string();
    let candidates = std::iter::once(tf).chain(0..u.files.len());
    for fi in candidates {
        let f = u.files[fi];
        if let Some(sym) = f.function(&member) {
            let (s, e) = f.span(sym);
            if let Some(hd) = named_handler(f, fi, s, e) {
                return (member, Some(hd));
            }
        }
    }
    (member, None)
}

fn inline_handler(code: &str, file: usize, start: usize, end: usize, src: &Src) -> Option<Handler> {
    let slice = &code[start..end];
    let arrow = slice.find("=>");
    let paren = slice.find('(');
    let (params, param_text) = match (paren, arrow) {
        (Some(p), Some(a)) if p < a => {
            let close = matching(code, start + p)?;
            let text = src.slice(start + p + 1, close).to_string();
            (param_names(&text), text)
        }
        (_, Some(a)) => {
            let name = slice[..a].trim().trim_start_matches("async").trim().to_string();
            (vec![name.clone()], name)
        }
        (Some(p), None) => {
            let close = matching(code, start + p)?;
            let text = src.slice(start + p + 1, close).to_string();
            (param_names(&text), text)
        }
        _ => return None,
    };
    let body_open = code[start..end].find(['{']).map(|x| start + x);
    let body = match (body_open, arrow) {
        (Some(b), Some(a)) if start + a < b => (b, matching(code, b).unwrap_or(end)),
        (Some(b), None) => (b, matching(code, b).unwrap_or(end)),
        (_, Some(a)) => (start + a + 2, end),
        _ => (start, end),
    };
    Some(Handler { file, body, params, param_text })
}

fn named_handler(f: &Loaded, fi: usize, s: usize, e: usize) -> Option<Handler> {
    let code = &f.src.code;
    let open = code[s..e].find('(').map(|x| s + x)?;
    let close = matching(code, open)?;
    let text = f.src.slice(open + 1, close).to_string();
    let body_open = code[close..e].find('{').map(|x| close + x).unwrap_or(close);
    Some(Handler { file: fi, body: (body_open, e), params: param_names(&text), param_text: text })
}

fn param_names(text: &str) -> Vec<String> {
    let mut depth = 0;
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        match c {
            '<' | '{' | '[' | '(' => depth += 1,
            '>' | '}' | ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    out.push(cur);
    out.iter()
        .map(|p| p.split([':', '=']).next().unwrap_or("").trim().trim_end_matches('?').to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn analyze_route(
    u: &Unit,
    r: &Route,
    d: &mut Draft,
    model_fields: &HashMap<String, Vec<Field>>,
    names: &BTreeSet<String>,
    synthesized: &mut Vec<Model>,
) {
    let rsrc = &u.files[r.file].src;
    // Middleware between path and handler.
    for &(s, e) in r.args.iter().rev().skip(1) {
        let expr = rsrc.slice(s, e);
        if let Some(a) = (!expr.starts_with('{')).then(|| auth_requirement(expr, rsrc.ev(s))).flatten() {
            add_auth(&mut d.op, a);
        }
        // validate(schema) / zValidator("json", schema) / celebrate.
        let lower = expr.to_lowercase();
        if lower.contains("valid") {
            if let Some(schema) =
                expr.split(['(', ',', ')']).map(str::trim).filter(|x| !x.is_empty()).find(|x| u.zod.contains_key(*x))
            {
                let loc = if expr.contains("\"query\"") || expr.contains("'query'") { "query" } else { "body" };
                if loc == "body" {
                    d.op.request_body = Some(type_ref(&u.zod[schema]));
                    d.request_declared = true;
                } else if let Some(fields) = model_fields.get(&u.zod[schema]) {
                    params_from_fields(&mut d.op, fields, "query");
                }
            }
        }
        // Fastify route options: { preHandler: [authenticate], schema: { body: X } }.
        if rsrc.code.as_bytes()[s] == b'{' {
            for (k, _, vs, ve) in object_entries(rsrc, s) {
                if matches!(k.as_str(), "preHandler" | "onRequest" | "preValidation") {
                    for part in rsrc.slice(vs, ve).trim_matches(['[', ']']).split(',') {
                        if let Some(a) = auth_requirement(part, rsrc.ev(vs)) {
                            add_auth(&mut d.op, a);
                        }
                    }
                }
            }
        }
    }
    // Fastify generics: post<{ Body: T; Reply: R; Params: P; Querystring: Q }>.
    if let Some((gs, ge)) = r.generic {
        let g = rsrc.slice(gs, ge);
        for (k, v) in generic_entries(g) {
            match k.as_str() {
                "Body" => {
                    d.op.request_body = Some(type_ref(&v));
                    d.request_declared = true;
                }
                "Reply" => {
                    d.op.response = Some(type_ref(&v));
                    d.response_declared = true;
                }
                "Querystring" => {
                    if let Some(fields) = model_fields.get(&unwrap_type(&v).0) {
                        params_from_fields(&mut d.op, fields, "query");
                    }
                }
                _ => {}
            }
        }
    }

    let (name, handler) = find_handler(u, r);
    let file = u.files[r.file];
    let route_line = rsrc.line(r.call.0);
    d.op.summary = doc_above(rsrc, route_line, Style::CLike);
    let Some(hd) = handler else {
        d.op.handler.name = if name.is_empty() { format!("{} {}", d.op.method, d.op.path) } else { name };
        return;
    };
    let hf = u.files[hd.file];
    let src = &hf.src;
    d.op.handler = SymbolRef {
        name: if name.is_empty() { anon_name(file, r) } else { name.clone() },
        evidence: src.ev_range(hd.body.0, hd.body.1, (!name.is_empty()).then_some(name.as_str())),
    };
    if d.op.summary.is_none() && !name.is_empty() {
        d.op.summary = hf.function(&name).and_then(|s| s.doc.as_deref()).and_then(first_sentence);
    }
    if !name.is_empty() {
        if let Some(sym) = hf.function(&name) {
            let (s, _) = hf.span(sym);
            d.op.handler.evidence = src.ev_range(s, hd.body.1, Some(&name));
        }
    }
    let req = hd.params.first().cloned().unwrap_or_else(|| "req".into());
    let res = hd.params.get(1).cloned().unwrap_or_else(|| "res".into());
    let hono = d.op.framework == "hono";

    // Request<Params, ResBody, ReqBody, Query> annotations.
    if let Some(p) = hd.param_text.find("Request<") {
        let text = &hd.param_text[p + 7..];
        if let Some(end) = generic_close(text) {
            let parts = split_top(&text[1..end], &[',']);
            if let Some(b) = parts.get(2).filter(|t| !is_empty_type(t)) {
                d.op.request_body = Some(type_ref(b));
                d.request_declared = true;
            }
            if let Some(b) = parts.get(1).filter(|t| !is_empty_type(t)) {
                d.op.response = Some(type_ref(b));
                d.response_declared = true;
            }
            if let Some(q) = parts.get(3).and_then(|t| model_fields.get(&unwrap_type(t).0)) {
                params_from_fields(&mut d.op, q, "query");
            }
        }
    }
    if let Some(p) = hd.param_text.find("Response<") {
        let text = &hd.param_text[p + 8..];
        if let Some(end) = generic_close(text) {
            let t = text[1..end].trim();
            if !is_empty_type(t) {
                d.op.response = Some(type_ref(t));
                d.response_declared = true;
            }
        }
    }

    let (bs, be) = hd.body;
    let code = &src.code;
    let body_code = &code[bs..be];

    // ---- request body
    let body_expr = if hono { None } else { Some(format!("{req}.body")) };
    if let Some(bexpr) = &body_expr {
        for at in find_all(body_code, bexpr) {
            let abs = bs + at;
            let after = &code[abs + bexpr.len()..];
            if after.starts_with(|c: char| is_ident(c as u8)) {
                continue;
            }
            // X.parse(req.body)
            let before = code[..abs].trim_end();
            if let Some(b) = before.strip_suffix('(') {
                let call = b.trim_end();
                for m in [".parse", ".safeParse", ".parseAsync", ".safeParseAsync", ".validate", ".validateSync"] {
                    if let Some(recv) = call.strip_suffix(m) {
                        if let Some((s, e)) = ident_before(code, recv.len()) {
                            let schema = src.slice(s, e);
                            let t = u.zod.get(schema).cloned().unwrap_or_else(|| schema.to_string());
                            d.op.request_body = Some(type_ref(&t));
                            d.request_declared = true;
                        }
                    }
                }
            }
            // req.body as T
            if let Some(t) = after.trim_start().strip_prefix("as ") {
                let t: String =
                    t.chars().take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '[' | ']' | '<' | '>')).collect();
                d.op.request_body = Some(type_ref(&t));
                d.request_declared = true;
            }
            // const x: T = req.body
            if let Some(b) = before.strip_suffix('=') {
                let lhs = b.trim_end();
                let line_start = lhs.rfind(['\n', ';', '{']).map(|x| x + 1).unwrap_or(0);
                let decl = src.slice(line_start, lhs.len());
                if let Some((_, t)) = decl.split_once(':') {
                    if !decl.contains('{') {
                        d.op.request_body = Some(type_ref(t.trim()));
                        d.request_declared = true;
                    }
                }
                // const { a, b } = req.body
                if !d.request_declared && lhs.ends_with('}') {
                    if let Some(open) = lhs.rfind('{') {
                        let names = destructured(src.slice(open + 1, lhs.len() - 1));
                        infer_body(&mut d.op, &names, src, abs, file, r, synthesized);
                    }
                }
            }
            // req.body.field
            if let Some(rest) = after.strip_prefix('.') {
                let field: String = rest.bytes().take_while(|&b| is_ident(b)).map(|b| b as char).collect();
                if !field.is_empty() && !d.request_declared {
                    infer_body(&mut d.op, &[field], src, abs, file, r, synthesized);
                }
            }
        }
    }
    // Hono: c.req.json<T>() / c.req.valid("json")
    if hono {
        if let Some(p) = body_code.find(".req.json<") {
            let t: String = body_code[p + 10..].chars().take_while(|&c| c != '>').collect();
            d.op.request_body = Some(type_ref(t.trim()));
            d.request_declared = true;
        }
    }

    // ---- params
    let q_prefix = if hono { format!("{req}.req") } else { req.clone() };
    for (accessor, loc) in [("query", "query"), ("params", "path"), ("headers", "header")] {
        let base = format!("{req}.{accessor}");
        for at in find_all(body_code, &base) {
            let abs = bs + at + base.len();
            let rest = &code[abs..];
            if rest.starts_with(|c: char| is_ident(c as u8)) {
                continue;
            }
            let mut names = Vec::new();
            if let Some(m) = rest.strip_prefix('.') {
                names.push(m.bytes().take_while(|&b| is_ident(b)).map(|b| b as char).collect::<String>());
            } else if rest.starts_with('[') {
                if let Some(close) = rest.find(']') {
                    if let Some(n) = string_lit(src.slice(abs + 1, abs + close)) {
                        names.push(n);
                    }
                }
            } else {
                let before = code[..bs + at].trim_end();
                if let Some(lhs) = before.strip_suffix('=').map(str::trim_end) {
                    if lhs.ends_with('}') {
                        if let Some(open) = lhs.rfind('{') {
                            names.extend(destructured(src.slice(open + 1, lhs.len() - 1)));
                        }
                    }
                }
            }
            for n in names.into_iter().filter(|n| !n.is_empty()) {
                if loc == "path" {
                    continue;
                }
                let wire = if loc == "header" { n.replace('_', "-") } else { n };
                add_param(
                    &mut d.op,
                    Param {
                        name: wire,
                        code_name: None,
                        location: loc.into(),
                        type_name: "string".into(),
                        required: false,
                        rules: vec![],
                        evidence: src.ev(abs),
                    },
                );
            }
        }
    }
    for (m, loc) in [("header(", "header"), ("get(", "header"), ("query(", "query"), ("param(", "path")] {
        let call = format!("{q_prefix}.{m}");
        for at in find_all(body_code, &call) {
            let open = bs + at + call.len() - 1;
            let Some(close) = matching(code, open) else { continue };
            let Some(n) = string_lit(src.slice(open + 1, close)) else { continue };
            if loc == "path" || (m == "get(" && hono) {
                continue;
            }
            add_param(
                &mut d.op,
                Param {
                    name: n,
                    code_name: None,
                    location: loc.into(),
                    type_name: "string".into(),
                    required: false,
                    rules: vec![],
                    evidence: src.ev(open),
                },
            );
        }
    }

    // ---- responses
    let mut success: Vec<(u16, usize)> = Vec::new();
    let mut payloads: Vec<(usize, usize)> = Vec::new();
    let responder = if hono { req.clone() } else { res.clone() };
    for at in find_word(body_code, &responder) {
        let abs = bs + at;
        if abs > 0 && code.as_bytes()[abs - 1] == b'.' {
            continue;
        }
        let chain = call_chain(code, abs + responder.len());
        if chain.is_empty() {
            continue;
        }
        let mut status: Option<u16> = None;
        let mut payload: Option<(usize, usize)> = None;
        let mut message: Option<String> = None;
        let mut responds = false;
        for (name, open, close) in &chain {
            let args = split_args(code, open + 1, *close);
            let arg = |i: usize| args.get(i).map(|&(s, e)| src.slice(s, e).to_string());
            match name.as_str() {
                "status" | "code" | "sendStatus" => {
                    status = arg(0).and_then(|a| status_code(&a));
                    responds |= name == "sendStatus";
                }
                "json" | "send" | "jsonp" | "end" => {
                    responds = true;
                    if let Some(&(s, e)) = args.first() {
                        if hono {
                            if let Some(sv) = arg(1).and_then(|a| status_code(&a)) {
                                status = Some(sv);
                            }
                        }
                        message = message.or_else(|| string_lit(src.slice(s, e)));
                        payload = Some((s, e));
                    }
                }
                "text" | "body" if hono => {
                    responds = true;
                    status = arg(1).and_then(|a| status_code(&a)).or(status);
                    message = arg(0).and_then(|a| string_lit(&a));
                }
                "notFound" if hono => {
                    responds = true;
                    status = Some(404);
                }
                "redirect" => {
                    responds = true;
                    status = status.or(Some(302));
                }
                _ => {}
            }
        }
        if !responds {
            // `reply.code(201); return note;`
            if let (Some(s), true) = (status, d.op.framework == "fastify") {
                success.push((s, abs));
            }
            continue;
        }
        let st = status.unwrap_or(200);
        if st >= 400 {
            let msg = message.or_else(|| payload.and_then(|(s, _)| payload_message(src, s)));
            add_error(&mut d.op, Some(st), msg, src.ev(abs));
        } else {
            success.push((st, abs));
            if let Some(p) = payload {
                payloads.push(p);
            }
        }
    }
    // Fastify handlers return the payload: `async () => store.list()` or `return note`.
    if d.op.framework == "fastify" && code.as_bytes().get(bs) != Some(&b'{') {
        let (s, e) = trim(code, bs, be);
        if e > s {
            payloads.push((s, e));
        }
    }
    if d.op.framework == "fastify" {
        for at in find_word(body_code, "return") {
            let abs = bs + at + 6;
            let s = skip_ws(code, abs);
            let e = code[s..be].find([';', '\n']).map(|x| s + x).unwrap_or(be);
            if e > s && !code[s..e].starts_with(&res) {
                payloads.push((s, e));
            }
        }
    }
    // Thrown HTTP errors.
    for at in find_word(body_code, "throw") {
        let abs = bs + at + 5;
        let s = skip_ws(code, abs);
        let stmt = src.slice(s, code[s..be].find(';').map(|x| s + x).unwrap_or(be).min(s + 200));
        if let Some((status, msg)) = thrown_status(stmt) {
            add_error(&mut d.op, Some(status), msg, src.ev(s));
        }
    }
    if let Some(&(st, _)) = success.iter().min_by_key(|(s, at)| (*s == 200, *at)) {
        d.op.success_status = Some(st);
    }
    if d.op.response.is_none() {
        let mut literals = Vec::new();
        for &(s, e) in &payloads {
            let expr = src.slice(s, e).trim().to_string();
            if code.as_bytes()[s] == b'{' {
                literals.push(s);
            } else if let Some(t) = expr.split(" as ").nth(1) {
                d.op.response = Some(type_ref(t.trim()));
                d.response_declared = true;
            } else if let Some(t) = typed_ident(u, hf, src, (bs, be), &expr) {
                d.op.response = Some(type_ref(&t));
                d.response_declared = names.contains(&unwrap_type(&t).0) || d.response_declared;
                d.response_declared = true;
            }
        }
        if d.op.response.is_none() && !literals.is_empty() {
            let name = format!("{}Response", base_name(file, r, &d.op));
            let mut model = Model {
                id: format!("{}:{name}", d.op.unit),
                unit: d.op.unit.clone(),
                name: name.clone(),
                fields: vec![],
                doc: None,
                evidence: src.ev(literals[0]),
            };
            for (i, &open) in literals.iter().enumerate() {
                let entries = object_entries(src, open);
                for (k, ks, vs, ve) in &entries {
                    let t = literal_type(src.slice(*vs, *ve), k);
                    match model.fields.iter_mut().find(|f| f.name == *k) {
                        Some(f) => {
                            if f.type_name != t && !f.type_name.split(" | ").any(|x| x == t) {
                                f.type_name = format!("{} | {t}", f.type_name);
                            }
                        }
                        None => model.fields.push(Field {
                            name: k.clone(),
                            code_name: None,
                            type_name: t,
                            required: i == 0,
                            rules: vec![],
                            doc: None,
                            model: None,
                            evidence: src.ev(*ks),
                        }),
                    }
                }
                for f in &mut model.fields {
                    if !entries.iter().any(|(k, ..)| *k == f.name) {
                        f.required = false;
                    }
                }
            }
            d.op.response = Some(TypeRef { type_name: name.clone(), model: Some(model.id.clone()), collection: false });
            if !synthesized.iter().any(|m| m.id == model.id) {
                synthesized.push(model);
            }
        }
    }
}

/// Index of the `>` closing the `<` at the start of `text`.
fn generic_close(text: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in text.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn is_empty_type(t: &str) -> bool {
    matches!(
        t.trim(),
        "" | "{}" | "()" | "any" | "unknown" | "Record<string, never>" | "ParamsDictionary" | "void" | "never"
    )
}

fn generic_entries(g: &str) -> Vec<(String, String)> {
    let inner = g.trim().trim_start_matches('{').trim_end_matches('}');
    split_top(inner, &[';', ','])
        .into_iter()
        .filter_map(|p| p.split_once(':').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
        .collect()
}

fn find_all(hay: &str, needle: &str) -> Vec<usize> {
    let b = hay.as_bytes();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = hay[from..].find(needle) {
        let at = from + i;
        if at == 0 || !is_ident(b[at - 1]) && b[at - 1] != b'.' {
            out.push(at);
        }
        from = at + needle.len();
    }
    out
}

/// `.a(...).b(...)` chain starting at `i`: (name, open, close).
pub(super) fn call_chain(code: &str, mut i: usize) -> Vec<(String, usize, usize)> {
    let b = code.as_bytes();
    let mut out = Vec::new();
    loop {
        let j = skip_ws(code, i);
        if b.get(j) != Some(&b'.') {
            break;
        }
        let Some((s, e)) = ident_at(code, j + 1) else { break };
        let mut open = skip_ws(code, e);
        if b.get(open) == Some(&b'<') {
            match matching(code, open) {
                Some(c) => open = skip_ws(code, c + 1),
                None => break,
            }
        }
        if b.get(open) != Some(&b'(') {
            break;
        }
        let Some(close) = matching(code, open) else { break };
        out.push((code[s..e].to_string(), open, close));
        i = close + 1;
    }
    out
}

fn destructured(inner: &str) -> Vec<String> {
    inner
        .split(',')
        .map(|p| p.split([':', '=']).next().unwrap_or("").trim().trim_start_matches("...").to_string())
        .filter(|p| !p.is_empty() && p.bytes().all(is_ident))
        .collect()
}

fn anon_name(f: &Loaded, r: &Route) -> String {
    let base = r.key.name.trim_start_matches("fn:");
    let _ = f;
    format!("{base}.{}", r.method.to_lowercase())
}

fn base_name(_f: &Loaded, r: &Route, op: &Operation) -> String {
    let segs: Vec<&str> = op.path.split('/').filter(|s| !s.is_empty() && !s.starts_with('{')).collect();
    let noun = segs
        .last()
        .map(|s| pascal(s))
        .unwrap_or_else(|| pascal(r.key.name.trim_start_matches("fn:").trim_end_matches("Router")));
    format!("{noun}{}", pascal(&op.method.to_lowercase()))
}

fn infer_body(
    op: &mut Operation,
    names: &[String],
    src: &Src,
    at: usize,
    f: &Loaded,
    r: &Route,
    synthesized: &mut Vec<Model>,
) {
    let name = format!("{}Request", base_name(f, r, op));
    let id = format!("{}:{name}", op.unit);
    let model = match synthesized.iter_mut().position(|m| m.id == id) {
        Some(i) => &mut synthesized[i],
        None => {
            synthesized.push(Model {
                id: id.clone(),
                unit: op.unit.clone(),
                name: name.clone(),
                fields: vec![],
                doc: None,
                evidence: src.ev(at),
            });
            synthesized.last_mut().unwrap()
        }
    };
    for n in names {
        if !model.fields.iter().any(|f| f.name == *n) {
            model.fields.push(Field {
                name: n.clone(),
                code_name: None,
                type_name: "unknown".into(),
                required: false,
                rules: vec![],
                doc: None,
                model: None,
                evidence: src.ev(at),
            });
        }
    }
    op.request_body = Some(TypeRef { type_name: name, model: Some(id), collection: false });
}

fn literal_type(v: &str, key: &str) -> String {
    let v = v.trim();
    if v.is_empty() {
        return "unknown".into();
    }
    if v.starts_with('"') || v.starts_with('\'') {
        return match string_lit(v) {
            Some(s) => format!("\"{s}\""),
            None => "string".into(),
        };
    }
    if v.starts_with('`') {
        return "string".into();
    }
    if v.parse::<f64>().is_ok() {
        return "number".into();
    }
    if v == "true" || v == "false" {
        return "boolean".into();
    }
    if v.starts_with('[') {
        return "array".into();
    }
    if v.starts_with('{') {
        return "object".into();
    }
    if v.starts_with("new Date") || v.ends_with("toISOString()") {
        return "string".into();
    }
    let _ = key;
    "unknown".into()
}

fn payload_message(src: &Src, open: usize) -> Option<String> {
    if src.code.as_bytes().get(open) != Some(&b'{') {
        return None;
    }
    object_entries(src, open)
        .into_iter()
        .filter(|(k, ..)| matches!(k.as_str(), "error" | "message" | "detail" | "msg"))
        .find_map(|(_, _, vs, ve)| string_lit(src.slice(vs, ve)))
}

fn thrown_status(stmt: &str) -> Option<(u16, Option<String>)> {
    let s = stmt.trim().trim_start_matches("new ").trim();
    let name: String = s.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.').collect();
    let args = s[name.len()..].trim_start().strip_prefix('(').unwrap_or("");
    let first_str = args.find(['"', '\'', '`']).and_then(|q| {
        let quote = args.as_bytes()[q] as char;
        args[q + 1..].find(quote).map(|e| args[q + 1..q + 1 + e].to_string())
    });
    let base = name.rsplit('.').next().unwrap_or(&name);
    if matches!(base, "HTTPException" | "HttpError" | "createError" | "HttpException" | "ApiError" | "AppError") {
        let num: String = args.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
        let status = status_code(&num).or_else(|| {
            args.split([',', ')', ' ']).find_map(|a| a.contains("HttpStatus.").then(|| status_code(a.trim())).flatten())
        })?;
        return Some((status, first_str));
    }
    let stripped = base.trim_end_matches("Exception").trim_end_matches("Error");
    let status = status_code(stripped).or_else(|| status_code(&base.replace("Http", "")))?;
    (status >= 400).then_some((status, first_str))
}

/// Type of an identifier returned by a handler: `const x: T`, `as T`, or a unit function's declared return type.
fn typed_ident(u: &Unit, _f: &Loaded, src: &Src, body: (usize, usize), expr: &str) -> Option<String> {
    let e = expr.trim().trim_start_matches("await ").trim();
    if e.bytes().all(is_ident) && !e.is_empty() {
        let body_text = src.slice(body.0, body.1);
        for kw in ["const ", "let "] {
            if let Some(p) = body_text.find(&format!("{kw}{e}")) {
                let rest = &body_text[p + kw.len() + e.len()..];
                let rest = rest.trim_start();
                if let Some(t) = rest.strip_prefix(':') {
                    let t = t.split('=').next().unwrap_or("").trim();
                    if !t.is_empty() {
                        return Some(t.to_string());
                    }
                }
                if let Some(init) = rest.strip_prefix('=') {
                    return typed_ident(u, _f, src, body, init.split([';', '\n']).next().unwrap_or(""));
                }
            }
        }
        return None;
    }
    // Direct call of a unit function with a declared return type.
    let callee: String = e.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.').collect();
    let last = callee.rsplit('.').next().unwrap_or(&callee);
    u.fn_returns.get(last).cloned()
}

fn collect_fn_returns(u: &mut Unit, fi: usize) {
    let f = u.files[fi];
    for sym in &f.facts.symbols {
        if !matches!(sym.kind, SymbolKind::Function | SymbolKind::Method) {
            continue;
        }
        let (s, e) = f.span(sym);
        let code = f.src.code_slice(s, e);
        let Some(p) = code.find('(') else { continue };
        let Some(close) = matching(&f.src.code, s + p) else { continue };
        let colon = skip_ws(&f.src.code, close + 1);
        if colon >= e || f.src.code.as_bytes()[colon] != b':' {
            continue;
        }
        let t_end = f.src.code[colon..e].find(['{', '=']).map(|x| colon + x).unwrap_or(e);
        let mut ty = f.src.slice(colon + 1, t_end).trim().to_string();
        if let Some(inner) = ty.strip_prefix("Promise<").and_then(|t| t.strip_suffix('>')) {
            ty = inner.trim().to_string();
        }
        let (inner, _) = unwrap_type(&ty);
        let generic_param = inner.len() <= 2 && inner.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
        if !generic_param
            && !matches!(inner.as_str(), "void" | "any" | "unknown" | "Response" | "boolean" | "number" | "string" | "")
        {
            u.fn_returns.entry(sym.name.clone()).or_insert(ty);
        }
    }
}

fn params_from_fields(op: &mut Operation, fields: &[Field], loc: &str) {
    for f in fields {
        add_param(
            op,
            Param {
                name: f.name.clone(),
                code_name: f.code_name.clone(),
                location: loc.into(),
                type_name: f.type_name.clone(),
                required: f.required,
                rules: f.rules.clone(),
                evidence: f.evidence.clone(),
            },
        );
    }
}

// ---------------------------------------------------------------- models

fn collect_models(u: &mut Unit, unit: &str, fi: usize, out: &mut Vec<(Model, Vec<String>)>) {
    let f = u.files[fi];
    let src = &f.src;
    let code = &src.code;
    // enum RouteKey { User = 'users' } and const X = "…"
    for at in find_word(code, "enum") {
        let Some((ns, ne)) = ident_at(code, at + 4) else { continue };
        let Some(open) = code[ne..].find('{').map(|x| ne + x).filter(|o| src.code_slice(ne, *o).trim().is_empty())
        else {
            continue;
        };
        let Some(close) = matching(code, open) else { continue };
        let name = src.slice(ns, ne).to_string();
        for (s, e) in split_args(code, open + 1, close) {
            if let Some((k, v)) = src.slice(s, e).split_once('=') {
                if let Some(v) = string_lit(v.trim()) {
                    u.consts.insert(format!("{name}.{}", k.trim()), v);
                }
            }
        }
    }
    for line in src.text.lines() {
        let l = line.trim().trim_start_matches("export ");
        if let Some(rest) = l.strip_prefix("const ") {
            if let Some((k, v)) = rest.split_once('=') {
                let k = k.split(':').next().unwrap_or("").trim();
                if let Some(v) = string_lit(v.trim().trim_end_matches(';').trim_end_matches(" as const")) {
                    u.consts.entry(k.to_string()).or_insert(v);
                }
            }
        }
    }
    // interface X extends A, B { … }
    for at in find_word(code, "interface") {
        let Some((ns, ne)) = ident_at(code, at + 9) else { continue };
        let Some(open) = code[ne..].find('{').map(|x| ne + x) else { continue };
        let between = src.slice(ne, open);
        let parents: Vec<String> = between
            .trim()
            .strip_prefix("extends")
            .map(|p| p.split(',').map(|x| unwrap_type(x.trim()).0).collect())
            .unwrap_or_default();
        if between.contains('=') || between.contains('(') {
            continue;
        }
        let name = src.slice(ns, ne).to_string();
        let fields = type_literal_fields(src, open);
        out.push((
            Model {
                id: format!("{unit}:{name}"),
                unit: unit.into(),
                name,
                fields,
                doc: doc_above(src, src.line(at), Style::CLike),
                evidence: src.ev_range(at, matching(code, open).unwrap_or(open), None),
            },
            parents,
        ));
    }
    // type X = { … } / type X = A & { … }
    for at in find_word(code, "type") {
        let Some((ns, ne)) = ident_at(code, at + 4) else { continue };
        let eq = skip_ws(code, ne);
        let eq = if code.as_bytes().get(eq) == Some(&b'<') {
            matching(code, eq).map(|c| skip_ws(code, c + 1)).unwrap_or(eq)
        } else {
            eq
        };
        if code.as_bytes().get(eq) != Some(&b'=') {
            continue;
        }
        let rhs_start = skip_ws(code, eq + 1);
        let stmt_end = code[rhs_start..].find(';').map(|x| rhs_start + x).unwrap_or(code.len());
        let name = src.slice(ns, ne).to_string();
        let rhs = src.slice(rhs_start, stmt_end);
        // Pick<A, "x" | "y"> / Omit<A, "x"> / Partial<A>: resolved after all models are known.
        for util in ["Pick<", "Omit<", "Partial<", "Required<"] {
            if let Some(inner) = rhs.trim().strip_prefix(util).and_then(|r| r.strip_suffix('>')) {
                let parts = split_top(inner, &[',']);
                let keys: Vec<String> = parts
                    .get(1)
                    .map(|k| k.split('|').map(|x| x.trim().trim_matches(['"', '\'']).to_string()).collect())
                    .unwrap_or_default();
                let marker = format!(
                    "{}{}",
                    &util[..util.len() - 1],
                    if keys.is_empty() { String::new() } else { format!(":{}", keys.join(",")) }
                );
                out.push((
                    Model {
                        id: format!("{unit}:{name}"),
                        unit: unit.into(),
                        name: name.clone(),
                        fields: vec![],
                        doc: doc_above(src, src.line(at), Style::CLike),
                        evidence: src.ev_range(at, stmt_end, None),
                    },
                    vec![parts.first().map(|p| unwrap_type(p).0).unwrap_or_default(), marker],
                ));
            }
        }
        // z.infer<typeof schema> aliases name the schema's model.
        if let Some(p) = rhs.find("typeof ") {
            if rhs.contains("infer") || rhs.contains("input") || rhs.contains("output") {
                let var: String = rhs[p + 7..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                u.zod.insert(var, name.clone());
            }
            continue;
        }
        let Some(open) = code[rhs_start..stmt_end.min(code.len())].find('{').map(|x| rhs_start + x) else { continue };
        let parents: Vec<String> = src
            .slice(rhs_start, open)
            .split('&')
            .map(|x| x.trim())
            .filter(|x| !x.is_empty())
            .map(|x| unwrap_type(x).0)
            .collect();
        let fields = type_literal_fields(src, open);
        out.push((
            Model {
                id: format!("{unit}:{name}"),
                unit: unit.into(),
                name,
                fields,
                doc: doc_above(src, src.line(at), Style::CLike),
                evidence: src.ev_range(at, stmt_end, None),
            },
            parents,
        ));
    }
    // class X (extends Y) { props } — DTOs.
    for at in find_word(code, "class") {
        let Some((ns, ne)) = ident_at(code, at + 5) else { continue };
        let Some(open) = code[ne..].find('{').map(|x| ne + x) else { continue };
        let header = src.slice(ne, open);
        let parents: Vec<String> = header
            .split_once("extends")
            .map(|(_, p)| {
                p.split("implements")
                    .next()
                    .unwrap_or("")
                    .split(',')
                    .map(|x| unwrap_type(x.trim()).0)
                    .filter(|x| !x.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let name = src.slice(ns, ne).to_string();
        if let Some(p) = header.find("createZodDto(") {
            let var: String = header[p + 13..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            u.zod.insert(var, name.clone());
            continue;
        }
        let fields = class_fields(src, open);
        if fields.is_empty() {
            continue;
        }
        out.push((
            Model {
                id: format!("{unit}:{name}"),
                unit: unit.into(),
                name,
                fields,
                doc: doc_above(src, src.line(at), Style::CLike),
                evidence: src.ev_range(at, matching(code, open).unwrap_or(open), None),
            },
            parents,
        ));
    }
    // const xSchema = z.object({ … })
    for kw in ["const", "let"] {
        for at in find_word(code, kw) {
            let Some((ns, ne)) = ident_at(code, at + kw.len()) else { continue };
            let eq = skip_ws(code, ne);
            if code.as_bytes().get(eq) != Some(&b'=') {
                continue;
            }
            let rhs = skip_ws(code, eq + 1);
            // const B = A.extend({ … })
            if let Some((ps, pe)) = ident_at(code, rhs) {
                let dot = skip_ws(code, pe);
                let ext = skip_ws(code, dot + 1);
                if code.as_bytes().get(dot) == Some(&b'.') && code[ext..].starts_with("extend(") && &code[ps..pe] != "z"
                {
                    let brace = skip_ws(code, ext + 7);
                    if code.as_bytes().get(brace) == Some(&b'{') {
                        let var = src.slice(ns, ne).to_string();
                        let name = u.zod.get(&var).cloned().unwrap_or_else(|| zod_model_name(&var));
                        u.zod.insert(var, name.clone());
                        let parent_var = src.slice(ps, pe);
                        let parent = u.zod.get(parent_var).cloned().unwrap_or_else(|| zod_model_name(parent_var));
                        let mut nested = Vec::new();
                        let fields = zod_fields(u, src, brace, &name, unit, &mut nested);
                        let end = matching(code, ext + 6).unwrap_or(brace);
                        out.push((
                            Model {
                                id: format!("{unit}:{name}"),
                                unit: unit.into(),
                                name,
                                fields,
                                doc: doc_above(src, src.line(at), Style::CLike),
                                evidence: src.ev_range(at, end, None),
                            },
                            vec![parent],
                        ));
                        out.extend(nested.into_iter().map(|m| (m, vec![])));
                        continue;
                    }
                }
            }
            if !code[rhs..].starts_with('z') {
                continue;
            }
            let dot = skip_ws(code, rhs + 1);
            let obj = skip_ws(code, dot + 1);
            if code.as_bytes().get(dot) != Some(&b'.') || !code[obj..].starts_with("object(") {
                continue;
            }
            let rhs = obj - 2;
            let var = src.slice(ns, ne).to_string();
            let name = u.zod.get(&var).cloned().unwrap_or_else(|| zod_model_name(&var));
            u.zod.insert(var.clone(), name.clone());
            let open = rhs + 8;
            let brace = skip_ws(code, open + 1);
            if code.as_bytes().get(brace) != Some(&b'{') {
                continue;
            }
            let mut nested = Vec::new();
            let fields = zod_fields(u, src, brace, &name, unit, &mut nested);
            let end = matching(code, open).unwrap_or(open);
            out.push((
                Model {
                    id: format!("{unit}:{name}"),
                    unit: unit.into(),
                    name,
                    fields,
                    doc: doc_above(src, src.line(at), Style::CLike),
                    evidence: src.ev_range(at, end, None),
                },
                vec![],
            ));
            out.extend(nested.into_iter().map(|m| (m, vec![])));
        }
    }
}

fn zod_model_name(var: &str) -> String {
    let base = var.trim_end_matches("Schema").trim_end_matches("schema").trim_end_matches("Validator");
    pascal(if base.is_empty() { var } else { base })
}

fn type_literal_fields(src: &Src, open: usize) -> Vec<Field> {
    let code = &src.code;
    let Some(close) = matching(code, open) else { return vec![] };
    let mut out = Vec::new();
    let mut depth = 0;
    let mut piece = open + 1;
    let b = code.as_bytes();
    let mut pieces = Vec::new();
    for i in open + 1..close {
        match b[i] {
            b'{' | b'(' | b'[' | b'<' => depth += 1,
            b'}' | b')' | b']' => depth -= 1,
            b'>' if i > 0 && b[i - 1] != b'=' => depth -= 1,
            b';' | b',' | b'\n' if depth == 0 => {
                pieces.push((piece, i));
                piece = i + 1;
            }
            _ => {}
        }
    }
    pieces.push((piece, close));
    for (s, e) in pieces {
        let (s, e) = trim(code, s, e);
        if s >= e {
            continue;
        }
        let piece = src.slice(s, e);
        let piece_code = src.code_slice(s, e);
        let Some(colon) = piece_code.find(':') else { continue };
        let mut key = piece[..colon].trim().trim_start_matches("readonly ").trim().to_string();
        if key.contains('(') || key.starts_with('[') || key.is_empty() {
            continue;
        }
        let optional = key.ends_with('?');
        key = key.trim_end_matches('?').trim_matches(['"', '\'']).to_string();
        let ty = piece[colon + 1..].trim().to_string();
        let doc = doc_above(src, src.line(s), Style::CLike);
        let required = !optional && !ty.split('|').any(|p| p.trim() == "undefined");
        out.push(Field {
            name: key,
            code_name: None,
            rules: literal_enum_rule(&ty, &src.ev(s)),
            type_name: ty,
            required,
            doc,
            model: None,
            evidence: src.ev(s),
        });
    }
    out
}

fn literal_enum_rule(ty: &str, ev: &EvidenceRef) -> Vec<Rule> {
    let parts: Vec<&str> = ty.split('|').map(str::trim).filter(|p| *p != "null" && *p != "undefined").collect();
    if parts.len() > 1 && parts.iter().all(|p| p.starts_with('"') || p.starts_with('\'')) {
        let vals: Vec<&str> = parts.iter().map(|p| p.trim_matches(['"', '\''])).collect();
        return vec![rule(format!("one of: {}", vals.join(", ")), "enum", ev)];
    }
    vec![]
}

fn class_fields(src: &Src, open: usize) -> Vec<Field> {
    let code = &src.code;
    let Some(close) = matching(code, open) else { return vec![] };
    let mut out = Vec::new();
    let mut i = open + 1;
    let mut decorators: Vec<(usize, usize)> = Vec::new();
    while i < close {
        i = skip_ws(code, i);
        if i >= close {
            break;
        }
        let b = code.as_bytes()[i];
        if b == b'@' {
            let Some((_, ne)) = ident_at(code, i + 1) else {
                i += 1;
                continue;
            };
            let mut end = ne;
            let mut j = ne;
            while code.as_bytes().get(j) == Some(&b'.') {
                if let Some((_, e2)) = ident_at(code, j + 1) {
                    j = e2;
                    end = e2;
                } else {
                    break;
                }
            }
            if code.as_bytes().get(end) == Some(&b'(') {
                end = matching(code, end).map(|c| c + 1).unwrap_or(end + 1);
            }
            decorators.push((i, end));
            i = end;
            continue;
        }
        // Statement / member up to ';' or newline at depth 0, skipping bodies.
        let mut j = i;
        let mut member_end = close;
        while j < close {
            match code.as_bytes()[j] {
                b'{' | b'(' => {
                    j = matching(code, j).map(|c| c + 1).unwrap_or(close);
                    // Method or constructor body ends the member.
                    if code.as_bytes().get(skip_ws(code, j)) == Some(&b'{')
                        || code[i..j].contains('(') && code.as_bytes()[j - 1] == b'}'
                    {
                        let k = skip_ws(code, j);
                        if code.as_bytes().get(k) == Some(&b'{') {
                            j = matching(code, k).map(|c| c + 1).unwrap_or(close);
                        }
                        member_end = j;
                        break;
                    }
                }
                b';' | b'\n' => {
                    member_end = j;
                    break;
                }
                _ => j += 1,
            }
        }
        let member = src.slice(i, member_end).trim().to_string();
        let member_code = src.code_slice(i, member_end).to_string();
        let decs = std::mem::take(&mut decorators);
        i = member_end + 1;
        let head = member_code.split([':', '=', '(']).next().unwrap_or("").trim();
        if member_code.contains('(') && member_code.find('(') < member_code.find(':')
            || head.starts_with("private")
            || head.starts_with("protected")
            || head.starts_with("static")
            || head.starts_with("constructor")
            || head.starts_with("get ")
            || head.starts_with("set ")
        {
            continue;
        }
        let Some(colon) = member_code.find(':') else { continue };
        let key = member[..colon].trim().trim_start_matches("public ").trim_start_matches("readonly ").trim();
        let optional_mark = key.ends_with('?');
        let key = key.trim_end_matches(['?', '!']).to_string();
        if key.is_empty() || !key.bytes().all(is_ident) {
            continue;
        }
        let ty = member[colon + 1..].split('=').next().unwrap_or("").trim().trim_end_matches(';').to_string();
        let has_default = member_code[colon..].contains('=');
        let ev = src.ev(i.min(member_end));
        let mut rules = literal_enum_rule(&ty, &ev);
        let mut required = !optional_mark && !has_default;
        for (ds, de) in decs {
            let dec = src.slice(ds + 1, de);
            let (dname, dargs) =
                dec.split_once('(').map(|(a, b)| (a.trim(), b.trim_end_matches(')'))).unwrap_or((dec.trim(), ""));
            let dev = src.ev(ds);
            match dname {
                "IsOptional" => required = false,
                "IsNotEmpty" | "IsDefined" => rules.push(rule("must not be empty", "required", &dev)),
                "IsEmail" => rules.push(rule("must be a valid email", "format", &dev)),
                "IsUUID" => rules.push(rule("must be a UUID", "format", &dev)),
                "IsUrl" | "IsURL" => rules.push(rule("must be a valid URL", "format", &dev)),
                "IsInt" => rules.push(rule("must be an integer", "format", &dev)),
                "IsPositive" => rules.push(rule("must be greater than 0", "range", &dev)),
                "IsDateString" | "IsISO8601" => rules.push(rule("must be an ISO-8601 date", "format", &dev)),
                "MinLength" => rules.push(rule(min_len(dargs.split(',').next().unwrap_or("").trim()), "length", &dev)),
                "MaxLength" => rules.push(rule(max_len(dargs.split(',').next().unwrap_or("").trim()), "length", &dev)),
                "Length" => {
                    let parts: Vec<&str> = dargs.split(',').map(str::trim).collect();
                    match parts.as_slice() {
                        [a, b, ..] if b.parse::<u64>().is_ok() => {
                            rules.push(rule(format!("between {a} and {b} characters"), "length", &dev))
                        }
                        [a, ..] => rules.push(rule(min_len(a), "length", &dev)),
                        _ => {}
                    }
                }
                "Min" => rules.push(rule(
                    format!("must be ≥ {}", dargs.split(',').next().unwrap_or("").trim()),
                    "range",
                    &dev,
                )),
                "Max" => rules.push(rule(
                    format!("must be ≤ {}", dargs.split(',').next().unwrap_or("").trim()),
                    "range",
                    &dev,
                )),
                "ArrayMinSize" => rules.push(rule(min_items(dargs.trim()), "length", &dev)),
                "ArrayMaxSize" => rules.push(rule(max_items(dargs.trim()), "length", &dev)),
                "IsIn" => {
                    let vals: Vec<String> = dargs
                        .trim_matches(['[', ']'])
                        .split(',')
                        .map(|v| v.trim().trim_matches(['"', '\'']).to_string())
                        .filter(|v| !v.is_empty())
                        .collect();
                    rules.push(rule(format!("one of: {}", vals.join(", ")), "enum", &dev));
                }
                "IsEnum" => rules.push(rule(format!("one of the {} values", dargs.trim()), "enum", &dev)),
                "Matches" => rules.push(rule(
                    format!("must match {}", dargs.split(',').next().unwrap_or("").trim()),
                    "pattern",
                    &dev,
                )),
                _ => {}
            }
        }
        out.push(Field {
            name: key,
            code_name: None,
            type_name: ty,
            required,
            rules,
            doc: doc_above(src, src.line(i.min(member_end).saturating_sub(1).max(open + 1)), Style::CLike)
                .filter(|_| false),
            model: None,
            evidence: ev,
        });
    }
    out
}

fn zod_fields(u: &Unit, src: &Src, brace: usize, parent: &str, unit: &str, nested: &mut Vec<Model>) -> Vec<Field> {
    let mut out = Vec::new();
    for (key, ks, vs, ve) in object_entries(src, brace) {
        let ev = src.ev(ks);
        let z = parse_zod(u, src, vs, ve, &format!("{parent}{}", pascal(&singular(&key))), unit, nested, &ev);
        out.push(Field {
            name: key,
            code_name: None,
            type_name: z.type_name,
            required: z.required,
            rules: z.rules,
            doc: z.doc.or_else(|| doc_above(src, src.line(ks), Style::CLike)),
            model: z.model,
            evidence: ev,
        });
    }
    out
}

struct Zod {
    type_name: String,
    required: bool,
    rules: Vec<Rule>,
    model: Option<String>,
    doc: Option<String>,
}

#[allow(clippy::too_many_arguments)]
fn parse_zod(
    u: &Unit,
    src: &Src,
    s: usize,
    e: usize,
    nested_name: &str,
    unit: &str,
    nested: &mut Vec<Model>,
    ev: &EvidenceRef,
) -> Zod {
    let code = &src.code;
    let mut z = Zod { type_name: "unknown".into(), required: true, rules: vec![], model: None, doc: None };
    let (s, e) = trim(code, s, e);
    let expr = src.code_slice(s, e);
    let z_dot = (expr.starts_with('z') && !expr.as_bytes().get(1).is_some_and(|&b| is_ident(b)))
        .then(|| skip_ws(code, s + 1))
        .filter(|&d| code.as_bytes().get(d) == Some(&b'.'));
    // Base: z.<kind>(args) or a schema variable.
    let (base_end, kind, args) = if let Some(dot) = z_dot {
        let rest = &code[skip_ws(code, dot + 1)..e];
        let rest_trim = rest.trim_start_matches("coerce.");
        let off = e - rest_trim.len();
        let Some((ks, ke)) = ident_at(code, off) else { return z };
        let open = ke;
        if code.as_bytes().get(open) != Some(&b'(') {
            return z;
        }
        let close = matching(code, open).unwrap_or(e);
        (close + 1, code[ks..ke].to_string(), (open + 1, close))
    } else {
        let len = expr.bytes().take_while(|&b| is_ident(b)).count();
        let var = &expr[..len];
        z.type_name = u.zod.get(var).cloned().unwrap_or_else(|| {
            if var.ends_with("Schema") {
                zod_model_name(var)
            } else {
                var.to_string()
            }
        });
        (s + len, String::new(), (s, s))
    };
    let ptr = |c: &str| -> String { c.to_string() };
    match kind.as_str() {
        "string" => z.type_name = ptr("string"),
        "email" | "url" | "uuid" | "uuidv4" | "uuidv7" | "cuid" | "cuid2" | "ulid" | "ipv4" | "ipv6" | "base64"
        | "iso" => {
            z.type_name = ptr("string");
            let statement = match kind.as_str() {
                "email" => "must be a valid email".to_string(),
                "url" => "must be a valid URL".to_string(),
                k if k.starts_with("uuid") => "must be a UUID".to_string(),
                k => format!("must be a valid {k}"),
            };
            z.rules.push(rule(statement, "format", ev));
        }
        "int" | "int32" | "int64" => {
            z.type_name = ptr("number");
            z.rules.push(rule("must be an integer", "format", ev));
        }
        "number" | "bigint" => z.type_name = ptr("number"),
        "boolean" => z.type_name = ptr("boolean"),
        "date" => z.type_name = ptr("Date"),
        "literal" => z.type_name = src.slice(args.0, args.1).trim().to_string(),
        "enum" => {
            let vals: Vec<String> = src
                .slice(args.0, args.1)
                .trim()
                .trim_matches(['[', ']'])
                .split(',')
                .map(|v| v.trim().trim_matches(['"', '\'']).to_string())
                .filter(|v| !v.is_empty())
                .collect();
            z.type_name = vals.iter().map(|v| format!("\"{v}\"")).collect::<Vec<_>>().join(" | ");
            z.rules.push(rule(format!("one of: {}", vals.join(", ")), "enum", ev));
        }
        "nativeEnum" => z.type_name = src.slice(args.0, args.1).trim().to_string(),
        "array" => {
            let inner = parse_zod(u, src, args.0, args.1, nested_name, unit, nested, ev);
            z.type_name = format!(
                "{}[]",
                if inner.type_name.contains('|') { format!("({})", inner.type_name) } else { inner.type_name }
            );
            z.model = inner.model;
            z.rules.extend(
                inner.rules.into_iter().map(|r| Rule { statement: format!("each item: {}", r.statement), ..r }),
            );
            z.doc = inner.doc;
        }
        "object" => {
            let brace = skip_ws(code, args.0);
            if code.as_bytes().get(brace) == Some(&b'{') {
                let fields = zod_fields(u, src, brace, nested_name, unit, nested);
                nested.push(Model {
                    id: format!("{unit}:{nested_name}"),
                    unit: unit.into(),
                    name: nested_name.into(),
                    fields,
                    doc: None,
                    evidence: ev.clone(),
                });
                z.type_name = nested_name.into();
                z.model = Some(format!("{unit}:{nested_name}"));
            }
        }
        _ => {}
    }
    let is_str = z.type_name == "string";
    let is_arr = z.type_name.ends_with("[]");
    for (name, open, close) in super::ts::call_chain(code, base_end) {
        if close > e {
            break;
        }
        let arg = src.slice(open + 1, close).split(',').next().unwrap_or("").trim().to_string();
        let r = |st: String, k: &str| rule(st, k, ev);
        match name.as_str() {
            "optional" | "nullish" | "default" => z.required = false,
            "describe" => z.doc = string_lit(&arg).and_then(|d| first_sentence(&d)),
            "nullable" => z.type_name = format!("{} | null", z.type_name),
            "min" | "nonempty" if is_arr => {
                z.rules.push(r(min_items(if arg.is_empty() { "1" } else { &arg }), "length"))
            }
            "max" if is_arr => z.rules.push(r(max_items(&arg), "length")),
            "min" if is_str => z.rules.push(r(min_len(&arg), "length")),
            "max" if is_str => z.rules.push(r(max_len(&arg), "length")),
            "length" => z.rules.push(r(format!("exactly {arg} characters"), "length")),
            "nonempty" => z.rules.push(r(min_len("1"), "length")),
            "min" | "gte" => z.rules.push(r(format!("must be ≥ {arg}"), "range")),
            "max" | "lte" => z.rules.push(r(format!("must be ≤ {arg}"), "range")),
            "gt" => z.rules.push(r(format!("must be > {arg}"), "range")),
            "lt" => z.rules.push(r(format!("must be < {arg}"), "range")),
            "positive" => z.rules.push(r("must be greater than 0".into(), "range")),
            "nonnegative" => z.rules.push(r("must be ≥ 0".into(), "range")),
            "int" => z.rules.push(r("must be an integer".into(), "format")),
            "email" => z.rules.push(r("must be a valid email".into(), "format")),
            "url" => z.rules.push(r("must be a valid URL".into(), "format")),
            "uuid" => z.rules.push(r("must be a UUID".into(), "format")),
            "datetime" => z.rules.push(r("must be an ISO-8601 date-time".into(), "format")),
            "regex" => z.rules.push(r(format!("must match {arg}"), "pattern")),
            "refine" | "superRefine" => {
                let msg = src.slice(open + 1, close).find(['"', '\'']).and_then(|q| {
                    let t = src.slice(open + 1, close);
                    let quote = t.as_bytes()[q] as char;
                    t[q + 1..].find(quote).map(|x| t[q + 1..q + 1 + x].to_string())
                });
                z.rules.push(r(msg.unwrap_or_else(|| "custom validation".into()), "custom"));
            }
            _ => {}
        }
    }
    z
}

// ---------------------------------------------------------------- NestJS

fn nest(u: &Unit, unit: &str, model_fields: &HashMap<String, Vec<Field>>, h: &mut Harvest) {
    let mut global = String::new();
    for f in &u.files {
        if let Some(p) = f.src.code.find(".setGlobalPrefix(") {
            if let Some(close) = matching(&f.src.code, p + 16) {
                if let Some(first) = split_args(&f.src.code, p + 17, close).first() {
                    global = string_lit(f.src.slice(first.0, first.1)).unwrap_or_default();
                }
            }
        }
    }
    for f in &u.files {
        let src = &f.src;
        let code = &src.code;
        for at in find_all(code, "@Controller") {
            let open = at + 11;
            let mut prefix = String::new();
            let mut partial = false;
            let mut after = open;
            if code.as_bytes().get(open) == Some(&b'(') {
                let close = matching(code, open).unwrap_or(open);
                let inner = src.slice(open + 1, close).trim();
                let expr = inner
                    .split("path:")
                    .nth(1)
                    .map(|p| p.split([',', '}']).next().unwrap_or("").trim())
                    .unwrap_or(inner);
                match string_lit(expr).or_else(|| u.consts.get(expr).cloned()) {
                    Some(p) => prefix = p,
                    None => partial = !expr.is_empty(),
                }
                after = close + 1;
            }
            let Some(class_at) = code[after..].find("class ").map(|x| after + x) else { continue };
            let Some(body_open) = code[class_at..].find('{').map(|x| class_at + x) else { continue };
            let Some(body_close) = matching(code, body_open) else { continue };
            let class_decorators = src.slice(at, class_at);
            let mut class_auth = Vec::new();
            decorator_auth(class_decorators, src.ev(at), &mut class_auth);
            let class_name = ident_at(code, class_at + 5).map(|(a, b)| src.slice(a, b).to_string()).unwrap_or_default();
            let mut i = body_open + 1;
            while i < body_close {
                let Some(dec) = code[i..body_close].find('@').map(|x| i + x) else { break };
                let Some((ns, ne)) = ident_at(code, dec + 1) else {
                    i = dec + 1;
                    continue;
                };
                let method = match &code[ns..ne] {
                    m @ ("Get" | "Post" | "Put" | "Patch" | "Delete" | "Options" | "Head" | "All") => m.to_uppercase(),
                    _ => {
                        i = ne;
                        continue;
                    }
                };
                let method = if method == "ALL" { "ANY".to_string() } else { method };
                let mut sub = String::new();
                let mut j = ne;
                if code.as_bytes().get(ne) == Some(&b'(') {
                    let c = matching(code, ne).unwrap_or(ne);
                    let arg = src.slice(ne + 1, c).trim();
                    sub = string_lit(arg).or_else(|| u.consts.get(arg).cloned()).unwrap_or_default();
                    j = c + 1;
                }
                // Further decorators, then `async name(params): Ret {`.
                let mut decorators_end = j;
                loop {
                    let k = skip_ws(code, decorators_end);
                    if code.as_bytes().get(k) != Some(&b'@') {
                        break;
                    }
                    let Some((_, e2)) = ident_at(code, k + 1) else { break };
                    decorators_end = if code.as_bytes().get(e2) == Some(&b'(') {
                        matching(code, e2).map(|c| c + 1).unwrap_or(e2)
                    } else {
                        e2
                    };
                }
                let method_decorators = src.slice(dec, decorators_end).to_string();
                let sig = skip_ws(code, decorators_end);
                let sig_rest = code[sig..].trim_start_matches("public ").trim_start_matches("async ");
                let name_at = sig + (code[sig..].len() - sig_rest.len());
                let Some((mns, mne)) = ident_at(code, name_at) else {
                    i = decorators_end;
                    continue;
                };
                let po = skip_ws(code, mne);
                if code.as_bytes().get(po) != Some(&b'(') {
                    i = decorators_end;
                    continue;
                }
                let pc = matching(code, po).unwrap_or(po);
                let Some(bo) = code[pc..body_close].find('{').map(|x| pc + x) else { break };
                let bc = matching(code, bo).unwrap_or(bo);
                let ret = src.slice(pc + 1, bo).trim().trim_start_matches(':').trim().to_string();
                let handler_name = src.slice(mns, mne).to_string();
                let path = join_path(&join_path(&global, &prefix), &sub);
                let mut op = new_op(
                    unit,
                    "nestjs",
                    &method,
                    path,
                    SymbolRef {
                        name: format!("{class_name}.{handler_name}"),
                        evidence: src.ev_range(dec, bc, Some(&handler_name)),
                    },
                    src.ev_range(dec, pc, Some(&handler_name)),
                );
                op.summary = method_decorators
                    .split("summary:")
                    .nth(1)
                    .and_then(|s| string_lit(s.split([',', '}']).next().unwrap_or("")))
                    .or_else(|| doc_above(src, src.line(dec), Style::CLike));
                op.path_partial = partial;
                let mut draft = Draft { op, request_declared: false, response_declared: false };
                for a in &class_auth {
                    add_auth(&mut draft.op, a.clone());
                }
                let mut method_auth = Vec::new();
                decorator_auth(&method_decorators, src.ev(dec), &mut method_auth);
                for a in method_auth {
                    add_auth(&mut draft.op, a);
                }
                if let Some(code_dec) = method_decorators.split("@HttpCode(").nth(1) {
                    draft.op.success_status = status_code(code_dec.split(')').next().unwrap_or(""));
                }
                if draft.op.success_status.is_none() {
                    draft.op.success_status = Some(if draft.op.method == "POST" { 201 } else { 200 });
                }
                // Parameters.
                for (ps, pe) in split_args(code, po + 1, pc) {
                    let p = src.slice(ps, pe);
                    let pev = src.ev(ps);
                    let (decor, rest) = match p.rfind(')') {
                        Some(x) if p.trim_start().starts_with('@') => (&p[..=x], &p[x + 1..]),
                        _ => ("", p),
                    };
                    let (pname, ptype) =
                        rest.split_once(':').map(|(a, b)| (a.trim(), b.trim())).unwrap_or((rest.trim(), "string"));
                    let optional = pname.ends_with('?');
                    let key =
                        decor.split('(').nth(1).and_then(|a| string_lit(a.split([',', ')']).next().unwrap_or("")));
                    if decor.starts_with("@Body") {
                        draft.op.request_body = Some(type_ref(ptype));
                        draft.request_declared = true;
                    } else if decor.starts_with("@Param") {
                        if let Some(k) = key {
                            add_param(
                                &mut draft.op,
                                Param {
                                    name: k,
                                    code_name: None,
                                    location: "path".into(),
                                    type_name: ptype.into(),
                                    required: true,
                                    rules: vec![],
                                    evidence: pev,
                                },
                            );
                        }
                    } else if decor.starts_with("@Query") || decor.starts_with("@Headers") {
                        let loc = if decor.starts_with("@Query") { "query" } else { "header" };
                        match key {
                            Some(k) => add_param(
                                &mut draft.op,
                                Param {
                                    name: k,
                                    code_name: None,
                                    location: loc.into(),
                                    type_name: ptype.into(),
                                    required: !optional,
                                    rules: vec![],
                                    evidence: pev,
                                },
                            ),
                            None => {
                                if let Some(fields) = model_fields.get(&unwrap_type(ptype).0) {
                                    params_from_fields(&mut draft.op, fields, loc);
                                }
                            }
                        }
                    } else if decor.contains("User") {
                        add_auth(
                            &mut draft.op,
                            Requirement {
                                kind: "authenticated".into(),
                                detail: decor.trim().to_string(),
                                evidence: pev,
                            },
                        );
                    }
                }
                let (inner, _) = unwrap_type(&ret);
                if !ret.is_empty() && !matches!(inner.as_str(), "void" | "any" | "unknown") {
                    draft.op.response = Some(type_ref(&ret));
                    draft.response_declared = true;
                }
                for t in find_word(&code[bo..bc], "throw") {
                    let s = skip_ws(code, bo + t + 5);
                    let stmt = src.slice(s, code[s..bc].find(';').map(|x| s + x).unwrap_or(bc));
                    if let Some((st, msg)) = thrown_status(stmt) {
                        add_error(&mut draft.op, Some(st), msg, src.ev(s));
                    }
                }
                h.ops.push(draft);
                i = bc + 1;
            }
        }
    }
}

fn decorator_auth(text: &str, ev: EvidenceRef, out: &mut Vec<Requirement>) {
    for part in text.split('@').skip(1) {
        let name: String = part.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        let full = format!("@{}", part.trim().trim_end_matches(|c: char| c.is_whitespace()));
        let full = full.lines().next().unwrap_or("").trim().to_string();
        match name.as_str() {
            "UseGuards" => {
                if let Some(a) =
                    auth_requirement(full.trim_start_matches("@UseGuards(").trim_end_matches(')'), ev.clone())
                {
                    out.push(Requirement { detail: full, ..a });
                } else {
                    out.push(Requirement { kind: "custom".into(), detail: full, evidence: ev.clone() });
                }
            }
            "Roles" => out.push(Requirement { kind: "role".into(), detail: full, evidence: ev.clone() }),
            "ApiBearerAuth" => {}
            n if n.contains("Auth") && !n.starts_with("Api") => {
                if let Some(a) = auth_requirement(&full, ev.clone()) {
                    out.push(Requirement { detail: full, ..a });
                }
            }
            "Public" => out.clear(),
            _ => {}
        }
    }
}
