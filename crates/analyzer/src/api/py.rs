//! Python: FastAPI (APIRouter prefixes, include_router, dependencies, response_model) and
//! Flask blueprints; Pydantic / SQLModel models with Field constraints.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::text::*;
use super::*;

const METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "options", "head", "api_route", "route"];
const SKIP_TYPES: &[&str] = &[
    "Session",
    "AsyncSession",
    "SessionDep",
    "Request",
    "Response",
    "BackgroundTasks",
    "WebSocket",
    "HTTPConnection",
    "OAuth2PasswordRequestForm",
    "SecurityScopes",
    "Connection",
];

type Key = (usize, String);

struct Decl {
    root: bool,
    framework: &'static str,
    prefix: Option<String>,
    auth: Vec<Requirement>,
}

struct Mount {
    parent: Key,
    child: Key,
    prefix: Option<String>,
    auth: Vec<Requirement>,
}

enum Imported {
    Module(usize),
    Name(usize, String),
}

struct Unit<'f, 'a> {
    unit: &'f str,
    files: Vec<&'f Loaded<'a>>,
    consts: HashMap<String, String>,
    /// `CurrentUser = Annotated[User, Depends(get_current_user)]`
    aliases: HashMap<String, String>,
    imports: Vec<HashMap<String, Imported>>,
    decls: BTreeMap<Key, Decl>,
    models: HashSet<String>,
    enums: HashMap<String, Vec<String>>,
}

pub(crate) fn extract(files: &[Loaded], h: &mut Harvest) {
    let mut by_unit: BTreeMap<&str, Vec<&Loaded>> = BTreeMap::new();
    for f in files.iter().filter(|f| f.lang == Language::Python) {
        by_unit.entry(f.unit).or_default().push(f);
    }
    for (unit, fs) in by_unit {
        extract_unit(unit, fs, h);
    }
}

/// Logical lines of the code view: (start offset, end offset, indent), joined across open brackets.
fn logical_lines(src: &Src) -> Vec<(usize, usize, usize)> {
    let code = &src.code;
    let b = code.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        let mut depth = 0i32;
        let mut j = i;
        while j < b.len() {
            match b[j] {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                b'\n' if depth <= 0 => break,
                _ => {}
            }
            j += 1;
        }
        let line = &code[start..j];
        let indent = line.len() - line.trim_start().len();
        if !line.trim().is_empty() {
            out.push((start + indent, j, indent));
        }
        i = j + 1;
    }
    out
}

/// End offset of the indented block that follows logical line `li`.
fn block_end(lines: &[(usize, usize, usize)], li: usize) -> usize {
    let indent = lines[li].2;
    let mut end = lines[li].1;
    for l in &lines[li + 1..] {
        if l.2 <= indent {
            break;
        }
        end = l.1;
    }
    end
}

fn extract_unit(unit: &str, files: Vec<&Loaded>, h: &mut Harvest) {
    let mut u = Unit {
        unit,
        imports: Vec::new(),
        consts: HashMap::new(),
        aliases: HashMap::new(),
        decls: BTreeMap::new(),
        models: HashSet::new(),
        enums: HashMap::new(),
        files,
    };
    let lines: Vec<Vec<(usize, usize, usize)>> = u.files.iter().map(|f| logical_lines(&f.src)).collect();
    for (fi, ls) in lines.iter().enumerate() {
        let imports = parse_imports(&u, fi, ls);
        u.imports.push(imports);
        collect_consts(&mut u, fi, ls);
    }
    let classes = collect_classes(&mut u, &lines);
    for m in build_models(&u, &classes) {
        h.model(m);
    }
    for (fi, ls) in lines.iter().enumerate() {
        collect_decls(&mut u, fi, ls);
    }
    let mut mounts = Vec::new();
    for (fi, ls) in lines.iter().enumerate() {
        collect_mounts(&u, fi, ls, &mut mounts);
    }
    for (fi, ls) in lines.iter().enumerate() {
        collect_routes(&u, fi, ls, &mounts, h);
    }
}

// ---------------------------------------------------------------- module resolution

fn module_file(u: &Unit, module: &str) -> Option<usize> {
    let rel = module.replace('.', "/");
    for cand in [format!("{rel}.py"), format!("{rel}/__init__.py")] {
        if let Some(i) = u.files.iter().position(|f| f.path() == cand || f.path().ends_with(&format!("/{cand}"))) {
            return Some(i);
        }
    }
    None
}

fn parse_imports(u: &Unit, fi: usize, lines: &[(usize, usize, usize)]) -> HashMap<String, Imported> {
    let src = &u.files[fi].src;
    let mut out = HashMap::new();
    let pkg: Vec<&str> = src.path.rsplit_once('/').map(|(d, _)| d.split('/').collect()).unwrap_or_default();
    for &(s, e, _) in lines {
        let line = src.slice(s, e);
        if let Some(rest) = line.strip_prefix("from ") {
            let Some((module, names)) = rest.split_once(" import ") else { continue };
            let module = module.trim();
            let module = if module.starts_with('.') {
                let dots = module.bytes().take_while(|&b| b == b'.').count();
                let mut base: Vec<&str> = pkg.clone();
                for _ in 1..dots {
                    base.pop();
                }
                let tail = &module[dots..];
                let mut m = base.join(".");
                if !tail.is_empty() {
                    m = if m.is_empty() { tail.to_string() } else { format!("{m}.{tail}") };
                }
                m
            } else {
                module.to_string()
            };
            for item in names.trim().trim_matches(['(', ')']).split(',') {
                let item = item.trim();
                if item.is_empty() || item == "*" {
                    continue;
                }
                let (orig, local) = item.split_once(" as ").map(|(a, b)| (a.trim(), b.trim())).unwrap_or((item, item));
                if let Some(f) = module_file(u, &format!("{module}.{orig}")) {
                    out.insert(local.to_string(), Imported::Module(f));
                } else if let Some(f) = module_file(u, &module) {
                    out.insert(local.to_string(), Imported::Name(f, orig.to_string()));
                }
            }
        } else if let Some(rest) = line.strip_prefix("import ") {
            for item in rest.split(',') {
                let (module, local) =
                    item.split_once(" as ").map(|(a, b)| (a.trim(), b.trim())).unwrap_or((item.trim(), item.trim()));
                if let Some(f) = module_file(u, module) {
                    out.insert(local.to_string(), Imported::Module(f));
                }
            }
        }
    }
    out
}

/// Resolves `name` or `module.name` used in file `fi` to its defining file and name.
fn resolve(u: &Unit, fi: usize, expr: &str) -> Key {
    let expr = expr.trim();
    if let Some((head, member)) = expr.split_once('.') {
        if let Some(Imported::Module(f)) = u.imports[fi].get(head) {
            return (*f, member.to_string());
        }
    }
    match u.imports[fi].get(expr) {
        Some(Imported::Name(f, n)) => (*f, n.clone()),
        _ => (fi, expr.to_string()),
    }
}

fn collect_consts(u: &mut Unit, fi: usize, lines: &[(usize, usize, usize)]) {
    let src = &u.files[fi].src;
    for &(s, e, _) in lines {
        let code = src.code_slice(s, e);
        let Some(eq) = code.find('=') else { continue };
        if code[eq + 1..].starts_with('=') || eq == 0 {
            continue;
        }
        let lhs = code[..eq].trim();
        let name = lhs.split(':').next().unwrap_or("").trim();
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            continue;
        }
        let rhs = src.slice(s + eq + 1, e).trim();
        if let Some(v) = string_lit(rhs) {
            u.consts.entry(name.to_string()).or_insert(v);
        } else if rhs.starts_with("Annotated[") {
            u.aliases.insert(name.to_string(), rhs.to_string());
        }
    }
}

/// Static value of a prefix expression: literal, constant (`settings.API_V1_STR`), f-string or concatenation.
fn eval_str(u: &Unit, expr: &str) -> Option<String> {
    let e = expr.trim();
    if let Some(inner) = e.strip_prefix('f').and_then(string_lit) {
        let mut out = String::new();
        let mut rest = inner.as_str();
        while let Some(o) = rest.find('{') {
            out.push_str(&rest[..o]);
            let c = rest[o..].find('}')? + o;
            out.push_str(&eval_str(u, &rest[o + 1..c])?);
            rest = &rest[c + 1..];
        }
        out.push_str(rest);
        return Some(out);
    }
    if let Some(v) = string_lit(e) {
        return Some(v);
    }
    if e.contains('+') {
        return e.split('+').map(|p| eval_str(u, p)).collect::<Option<Vec<_>>>().map(|v| v.concat());
    }
    let last = e.rsplit('.').next().unwrap_or(e);
    u.consts.get(last).cloned()
}

// ---------------------------------------------------------------- routers

fn call_args(src: &Src, open: usize) -> Option<(usize, Vec<(usize, usize)>)> {
    let close = matching(&src.code, open)?;
    Some((close, split_args(&src.code, open + 1, close)))
}

fn kw<'s>(src: &'s Src, args: &[(usize, usize)], name: &str) -> Option<&'s str> {
    args.iter().find_map(|&(s, e)| {
        let piece = src.code_slice(s, e);
        let rest = piece.strip_prefix(name)?.trim_start();
        let v = rest.strip_prefix('=')?;
        if v.starts_with('=') {
            return None;
        }
        Some(src.slice(e - v.len(), e).trim())
    })
}

fn positional<'s>(src: &'s Src, args: &[(usize, usize)], i: usize) -> Option<&'s str> {
    args.iter()
        .filter(|&&(s, e)| {
            let p = src.code_slice(s, e);
            let eq = p.find('=');
            !(eq.is_some_and(|x| {
                p[..x].trim().bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') && !p[x + 1..].starts_with('=')
            }))
        })
        .nth(i)
        .map(|&(s, e)| src.slice(s, e).trim())
}

fn dependencies_auth(src: &Src, deps: Option<&str>, ev: &EvidenceRef) -> Vec<Requirement> {
    let Some(d) = deps else { return vec![] };
    d.trim_matches(['[', ']'])
        .split("Depends(")
        .chain(d.split("Security("))
        .skip(1)
        .filter_map(|p| {
            let target = p.split([')', ',']).next().unwrap_or("").trim();
            auth_requirement(target, ev.clone()).map(|a| Requirement { detail: format!("Depends({target})"), ..a })
        })
        .fold(Vec::new(), |mut acc, r| {
            if !acc.iter().any(|x: &Requirement| x.detail == r.detail) {
                acc.push(r);
            }
            let _ = src;
            acc
        })
}

fn collect_decls(u: &mut Unit, fi: usize, lines: &[(usize, usize, usize)]) {
    let src = &u.files[fi].src;
    let mut found = Vec::new();
    for &(s, e, _) in lines {
        let code = src.code_slice(s, e);
        let Some(eq) = code.find('=') else { continue };
        let name = code[..eq].split(':').next().unwrap_or("").trim();
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            continue;
        }
        let rhs_at = skip_ws(&src.code, s + eq + 1);
        let rhs = &src.code[rhs_at..e];
        let ctor = rhs.split('(').next().unwrap_or("").trim();
        let ctor = ctor.rsplit('.').next().unwrap_or(ctor);
        let (root, framework, prefix_kw) = match ctor {
            "FastAPI" => (true, "fastapi", "root_path"),
            "APIRouter" => (false, "fastapi", "prefix"),
            "Flask" => (true, "flask", "static_url_path"),
            "Blueprint" => (false, "flask", "url_prefix"),
            _ => continue,
        };
        let open = rhs_at + rhs.find('(').unwrap_or(0);
        let Some((_, args)) = call_args(src, open) else { continue };
        let ev = src.ev(s);
        let prefix = match kw(src, &args, prefix_kw) {
            Some(p) if prefix_kw != "static_url_path" => eval_str(u, p),
            _ => Some(String::new()),
        };
        let auth = dependencies_auth(src, kw(src, &args, "dependencies"), &ev);
        found.push((name.to_string(), Decl { root, framework, prefix, auth }));
    }
    for (n, d) in found {
        u.decls.insert((fi, n), d);
    }
}

fn collect_mounts(u: &Unit, fi: usize, lines: &[(usize, usize, usize)], out: &mut Vec<Mount>) {
    let src = &u.files[fi].src;
    for &(s, e, _) in lines {
        let code = src.code_slice(s, e);
        for (m, prefix_kw) in [(".include_router(", "prefix"), (".register_blueprint(", "url_prefix")] {
            let Some(p) = code.find(m) else { continue };
            let recv = code[..p].trim();
            let recv = recv.rsplit([' ', ':']).next().unwrap_or(recv);
            let parent = resolve(u, fi, recv);
            let open = s + p + m.len() - 1;
            let Some((_, args)) = call_args(src, open) else { continue };
            let Some(child_expr) = positional(src, &args, 0) else { continue };
            let child = resolve(u, fi, child_expr);
            let prefix = match kw(src, &args, prefix_kw) {
                Some(x) => eval_str(u, x),
                None => Some(String::new()),
            };
            let ev = src.ev(s);
            let auth = dependencies_auth(src, kw(src, &args, "dependencies"), &ev);
            out.push(Mount { parent, child, prefix, auth });
        }
    }
}

fn prefixes(u: &Unit, key: &Key, mounts: &[Mount], depth: usize) -> Vec<(String, bool, Vec<Requirement>)> {
    let decl = u.decls.get(key);
    let own = decl.and_then(|d| d.prefix.clone());
    let own_partial = decl.is_some_and(|d| d.prefix.is_none());
    let own_auth = decl.map(|d| d.auth.clone()).unwrap_or_default();
    let own = own.unwrap_or_default();
    let parents: Vec<&Mount> = mounts.iter().filter(|m| m.child == *key).collect();
    if parents.is_empty() || depth > 8 {
        let root = decl.is_some_and(|d| d.root);
        return vec![(own, !root || own_partial, own_auth)];
    }
    let mut out: Vec<(String, bool, Vec<Requirement>)> = Vec::new();
    for m in parents {
        for (pp, partial, mut auth) in prefixes(u, &m.parent, mounts, depth + 1) {
            // FastAPI: include_router prefix goes before the router's own prefix.
            let path = join_path(&join_path(&pp, m.prefix.as_deref().unwrap_or("")), &own);
            auth.extend(m.auth.iter().cloned());
            auth.extend(own_auth.iter().cloned());
            if !out.iter().any(|o| o.0 == path) {
                out.push((path, partial || m.prefix.is_none() || own_partial, auth));
            }
        }
    }
    out
}

// ---------------------------------------------------------------- routes

fn collect_routes(u: &Unit, fi: usize, lines: &[(usize, usize, usize)], mounts: &[Mount], h: &mut Harvest) {
    let f = u.files[fi];
    let src = &f.src;
    let mut li = 0;
    while li < lines.len() {
        let (s, _, indent) = lines[li];
        if !src.code[s..].starts_with('@') {
            li += 1;
            continue;
        }
        // Decorator stack, then the def.
        let first = li;
        while li < lines.len() && src.code[lines[li].0..].starts_with('@') {
            li += 1;
        }
        if li >= lines.len() {
            break;
        }
        let (ds, de, _) = lines[li];
        let def_code = src.code_slice(ds, de);
        let def_rest = def_code.trim_start_matches("async ").trim_start();
        if !def_rest.starts_with("def ") {
            continue;
        }
        let def_at = ds + (def_code.len() - def_rest.len());
        let Some((ns, ne)) = ident_at(&src.code, def_at + 3) else { continue };
        let fname = src.slice(ns, ne).to_string();
        let Some(po) = src.code[ne..de].find('(').map(|x| ne + x) else { continue };
        let Some(pc) = matching(&src.code, po) else { continue };
        let ret = src.code[pc + 1..de]
            .trim()
            .strip_prefix("->")
            .map(|r| src.slice(de - r.len(), de).trim().trim_end_matches(':').trim().to_string());
        let body_end = block_end(lines, li);
        let body = (de, body_end);
        let decorators: Vec<(usize, usize)> = lines[first..li].iter().map(|&(a, b, _)| (a, b)).collect();
        let summary = docstring(src, lines, li);
        let decorator_auth: Vec<Requirement> = decorators
            .iter()
            .filter_map(|&(a, b)| {
                let d = src.slice(a + 1, b);
                (!d.contains(".get(") && !d.contains(".post(") && !d.contains(".route("))
                    .then(|| auth_requirement(d, src.ev(a)))
                    .flatten()
            })
            .collect();
        let _ = indent;

        for &(a, b) in &decorators {
            let d = src.code_slice(a + 1, b);
            let Some(dot) = d.find('.') else { continue };
            let recv = &d[..dot];
            let Some(open_rel) = d[dot..].find('(') else { continue };
            let method = &d[dot + 1..dot + open_rel];
            if !METHODS.contains(&method) || !recv.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
                continue;
            }
            let key = resolve(u, fi, recv);
            let Some(decl) = u.decls.get(&key) else { continue };
            let open = a + 1 + dot + open_rel;
            let Some((close, args)) = call_args(src, open) else { continue };
            let Some(path) = positional(src, &args, 0).and_then(|p| eval_str(u, p)) else { continue };
            let methods: Vec<String> = if matches!(method, "route" | "api_route") {
                kw(src, &args, "methods")
                    .map(|m| {
                        m.trim_matches(['[', ']', '(', ')'])
                            .split(',')
                            .filter_map(string_lit)
                            .map(|x| x.to_uppercase())
                            .collect()
                    })
                    .filter(|v: &Vec<String>| !v.is_empty())
                    .unwrap_or_else(|| vec!["GET".into()])
            } else {
                vec![method.to_uppercase()]
            };
            let ev = src.ev_range(a, close, None);
            let route_auth = dependencies_auth(src, kw(src, &args, "dependencies"), &ev);
            for (prefix, partial, inherited_auth) in prefixes(u, &key, mounts, 0) {
                for m in &methods {
                    let full = join_path(&prefix, &path);
                    let mut op = new_op(
                        u.unit,
                        decl.framework,
                        m,
                        full,
                        SymbolRef { name: fname.clone(), evidence: src.ev_range(ds, body_end, Some(&fname)) },
                        ev.clone(),
                    );
                    op.evidence.symbol_name = None;
                    op.path_partial = partial;
                    op.summary = summary.clone();
                    for r in inherited_auth.iter().chain(&route_auth).chain(&decorator_auth) {
                        add_auth(&mut op, r.clone());
                    }
                    // Flask converters: <int:post_id>.
                    for seg in path.split('/').filter(|s| s.starts_with('<') && s.contains(':')) {
                        let (conv, name) = seg.trim_matches(['<', '>']).split_once(':').unwrap_or(("", ""));
                        let type_name = match conv {
                            "int" => "int",
                            "float" => "float",
                            "uuid" => "UUID",
                            _ => "str",
                        };
                        add_param(
                            &mut op,
                            Param {
                                name: name.into(),
                                code_name: None,
                                location: "path".into(),
                                type_name: type_name.into(),
                                required: true,
                                rules: vec![],
                                evidence: ev.clone(),
                            },
                        );
                    }
                    let mut d = Draft { op, request_declared: false, response_declared: false };
                    if decl.framework == "fastapi" {
                        fastapi_signature(u, fi, (po, pc), ret.as_deref(), &args, &mut d);
                    }
                    analyze_body(u, fi, body, &mut d);
                    h.ops.push(d);
                }
            }
        }
    }
}

fn docstring(src: &Src, lines: &[(usize, usize, usize)], def_li: usize) -> Option<String> {
    let next = lines.get(def_li + 1)?;
    if next.2 <= lines[def_li].2 {
        return None;
    }
    let text = src.slice(next.0, src.text.len());
    let q = if text.starts_with("\"\"\"") {
        "\"\"\""
    } else if text.starts_with("'''") {
        "'''"
    } else {
        return None;
    };
    let end = text[3..].find(q)? + 3;
    let doc = text[3..end].split("\n\n").next().unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ");
    first_sentence(&doc)
}

/// Splits `name: annotation = default` at top level.
fn split_param(p: &str) -> (String, Option<String>, Option<String>) {
    let mut depth = 0;
    let mut colon = None;
    let mut eq = None;
    for (i, c) in p.char_indices() {
        match c {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth -= 1,
            ':' if depth == 0 && colon.is_none() => colon = Some(i),
            '=' if depth == 0 && eq.is_none() => eq = Some(i),
            _ => {}
        }
    }
    let name_end = colon.or(eq).unwrap_or(p.len());
    let name = p[..name_end].trim().to_string();
    let ann = colon.map(|c| p[c + 1..eq.unwrap_or(p.len())].trim().to_string());
    let default = eq.map(|e| p[e + 1..].trim().to_string());
    (name, ann, default)
}

fn call_parts(expr: &str) -> Option<(&str, Vec<&str>)> {
    let e = expr.trim();
    let open = e.find('(')?;
    let close = e.rfind(')')?;
    Some((e[..open].trim(), split_top(&e[open + 1..close], &[','])))
}

fn kwargs_of<'a>(parts: &[&'a str]) -> (Vec<&'a str>, HashMap<&'a str, &'a str>) {
    let mut pos = Vec::new();
    let mut kws = HashMap::new();
    for p in parts {
        match p.split_once('=') {
            Some((k, v)) if k.trim().bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') && !v.starts_with('=') => {
                kws.insert(k.trim(), v.trim());
            }
            _ => pos.push(*p),
        }
    }
    (pos, kws)
}

fn constraint_rules(kws: &HashMap<&str, &str>, ann: &str, ev: &EvidenceRef) -> Vec<Rule> {
    let mut out = Vec::new();
    let is_list = unwrap_type(ann).1;
    for (k, v) in [
        ("min_length", "min_length"),
        ("max_length", "max_length"),
        ("ge", "ge"),
        ("gt", "gt"),
        ("le", "le"),
        ("lt", "lt"),
        ("pattern", "pattern"),
        ("regex", "regex"),
        ("min_items", "min_items"),
        ("max_items", "max_items"),
        ("multiple_of", "multiple_of"),
    ] {
        let Some(val) = kws.get(k) else { continue };
        let val = string_lit(val.trim_start_matches('r')).unwrap_or_else(|| val.to_string());
        let (statement, kind) = match v {
            "min_length" if is_list => (min_items(&val), "length"),
            "max_length" if is_list => (max_items(&val), "length"),
            "min_length" => (min_len(&val), "length"),
            "max_length" => (max_len(&val), "length"),
            "min_items" => (min_items(&val), "length"),
            "max_items" => (max_items(&val), "length"),
            "ge" => (format!("must be ≥ {val}"), "range"),
            "gt" => (format!("must be > {val}"), "range"),
            "le" => (format!("must be ≤ {val}"), "range"),
            "lt" => (format!("must be < {val}"), "range"),
            "multiple_of" => (format!("must be a multiple of {val}"), "range"),
            _ => (format!("must match {val}"), "pattern"),
        };
        out.push(rule(statement, kind, ev));
    }
    out
}

fn type_rules(u: &Unit, ann: &str, ev: &EvidenceRef) -> Vec<Rule> {
    let mut out = Vec::new();
    let (inner, _) = unwrap_type(ann);
    match inner.as_str() {
        "EmailStr" => out.push(rule("must be a valid email", "format", ev)),
        "HttpUrl" | "AnyUrl" | "AnyHttpUrl" => out.push(rule("must be a valid URL", "format", ev)),
        "UUID" | "UUID4" => out.push(rule("must be a UUID", "format", ev)),
        "PositiveInt" => out.push(rule("must be greater than 0", "range", ev)),
        "datetime" => out.push(rule("must be an ISO-8601 date-time", "format", ev)),
        "date" => out.push(rule("must be an ISO-8601 date", "format", ev)),
        _ => {}
    }
    if let Some(vals) = u.enums.get(&inner) {
        out.push(rule(format!("one of: {}", vals.join(", ")), "enum", ev));
    }
    if let Some(p) = ann.find("Literal[") {
        let inner = &ann[p + 8..];
        let inner = &inner[..inner.find(']').unwrap_or(inner.len())];
        let vals: Vec<String> =
            inner.split(',').map(|v| string_lit(v.trim()).unwrap_or_else(|| v.trim().to_string())).collect();
        out.push(rule(format!("one of: {}", vals.join(", ")), "enum", ev));
    }
    for ctor in ["constr(", "conint(", "confloat(", "conlist(", "StringConstraints(", "Field("] {
        if let Some(p) = ann.find(ctor) {
            if let Some((_, parts)) = call_parts(&ann[p..]) {
                let (_, kws) = kwargs_of(&parts);
                out.extend(constraint_rules(&kws, ann, ev));
            }
        }
    }
    out
}

fn fastapi_signature(
    u: &Unit,
    fi: usize,
    (po, pc): (usize, usize),
    ret: Option<&str>,
    deco_args: &[(usize, usize)],
    d: &mut Draft,
) {
    let src = &u.files[fi].src;
    let path_names = path_params(&d.op.path);
    for (s, e) in split_args(&src.code, po + 1, pc) {
        let raw = src.slice(s, e).trim();
        if matches!(raw, "*" | "/" | "self" | "cls") || raw.starts_with("**") || raw.starts_with('*') {
            continue;
        }
        let ev = src.ev(s);
        let (name, ann, default) = split_param(raw);
        let mut ann = ann.unwrap_or_default();
        let mut marker: Option<String> = default.clone().filter(|x| x.contains('('));
        if let Some(alias) = u.aliases.get(ann.trim()) {
            ann = alias.clone();
        }
        if let Some(inner) = ann.strip_prefix("Annotated[").and_then(|x| x.strip_suffix(']')).map(str::to_string) {
            let parts = split_top(&inner, &[',']);
            ann = parts.first().map(|x| x.to_string()).unwrap_or_default();
            marker = parts.get(1).map(|x| x.to_string()).or(marker);
        }
        let (base, _) = unwrap_type(&ann);
        if let Some(m) = &marker {
            let (ctor, parts) = call_parts(m).unwrap_or((m.as_str(), vec![]));
            let ctor = ctor.rsplit('.').next().unwrap_or(ctor);
            if matches!(ctor, "Depends" | "Security") {
                if parts.is_empty() && base.ends_with("Form") {
                    // Class dependency parsed from the form body (OAuth2PasswordRequestForm).
                    d.op.request_body = Some(type_ref(&ann));
                    d.request_declared = true;
                    continue;
                }
                let target = parts.first().copied().unwrap_or(base.as_str());
                if let Some(a) = auth_requirement(target, ev.clone()).or_else(|| auth_requirement(&name, ev.clone())) {
                    add_auth(&mut d.op, Requirement { detail: format!("{ctor}({target})"), ..a });
                }
                continue;
            }
        }
        if SKIP_TYPES.contains(&base.as_str()) {
            continue;
        }
        let (loc, kws, pos): (&str, HashMap<&str, &str>, Vec<&str>) = match marker.as_deref().and_then(call_parts) {
            Some((ctor, parts)) => {
                let ctor = ctor.rsplit('.').next().unwrap_or(ctor);
                let (pos, kws) = kwargs_of(&parts);
                let loc = match ctor {
                    "Path" => "path",
                    "Query" => "query",
                    "Header" => "header",
                    "Cookie" => "cookie",
                    "Body" | "Form" | "File" => "body",
                    _ => "query",
                };
                (loc, kws, pos)
            }
            None => {
                let loc = if path_names.contains(&name) {
                    "path"
                } else if u.models.contains(&base) {
                    "body"
                } else {
                    "query"
                };
                (loc, HashMap::new(), vec![])
            }
        };
        if loc == "body" {
            d.op.request_body = Some(type_ref(&ann));
            d.request_declared = true;
            continue;
        }
        let has_default = match marker.as_deref().and_then(call_parts) {
            Some(_) => kws.contains_key("default") || pos.first().is_some_and(|p| *p != "..."),
            None => default.is_some(),
        } || default.as_deref().is_some_and(|x| !x.contains('('));
        let wire = kws.get("alias").and_then(|a| string_lit(a)).unwrap_or_else(|| {
            if loc == "header" {
                name.replace('_', "-")
            } else {
                name.clone()
            }
        });
        let mut rules = constraint_rules(&kws, &ann, &ev);
        rules.extend(type_rules(u, &ann, &ev));
        let type_name = if ann.is_empty() { "string".to_string() } else { ann.clone() };
        let code_name = (wire != name).then(|| name.clone());
        add_param(
            &mut d.op,
            Param {
                name: wire,
                code_name,
                location: loc.into(),
                type_name,
                required: loc == "path" || !has_default,
                rules,
                evidence: ev,
            },
        );
    }
    let response_model = kw(src, deco_args, "response_model").filter(|x| *x != "None");
    if let Some(rm) = response_model {
        d.op.response = Some(type_ref(rm));
        d.response_declared = true;
    } else if let Some(r) = ret {
        let (inner, _) = unwrap_type(r);
        if !matches!(
            inner.as_str(),
            "Any"
                | "None"
                | "dict"
                | "list"
                | "Response"
                | "JSONResponse"
                | "HTMLResponse"
                | "RedirectResponse"
                | "StreamingResponse"
                | "FileResponse"
                | ""
        ) {
            d.op.response = Some(type_ref(r));
            d.response_declared = true;
        }
    }
    if let Some(st) = kw(src, deco_args, "status_code").and_then(status_code) {
        d.op.success_status = Some(st);
    }
    if let Some(summary) = kw(src, deco_args, "summary").and_then(string_lit) {
        d.op.summary = Some(summary);
    }
}

fn analyze_body(u: &Unit, fi: usize, (bs, be): (usize, usize), d: &mut Draft) {
    let src = &u.files[fi].src;
    let code = &src.code;
    for ctor in ["HTTPException", "abort"] {
        for at in find_word(&code[bs..be], ctor) {
            let abs = bs + at;
            let open = skip_ws(code, abs + ctor.len());
            if code.as_bytes().get(open) != Some(&b'(') {
                continue;
            }
            let Some((_, args)) = call_args(src, open) else { continue };
            let status = kw(src, &args, "status_code")
                .or_else(|| kw(src, &args, "code"))
                .or_else(|| positional(src, &args, 0))
                .and_then(status_code);
            let msg = kw(src, &args, "detail")
                .or_else(|| kw(src, &args, "description"))
                .or_else(|| positional(src, &args, 1))
                .and_then(string_lit);
            if status.is_some_and(|s| s >= 400) {
                add_error(&mut d.op, status, msg, src.ev(abs));
            }
        }
    }
    if d.op.framework == "flask" {
        let body = &code[bs..be];
        for at in find_word(body, "return") {
            let s = skip_ws(code, bs + at + 6);
            let e = code[s..be].find('\n').map(|x| s + x).unwrap_or(be);
            let stmt = src.slice(s, e);
            let parts = split_top(stmt, &[',']);
            if let Some(st) = parts.get(1).and_then(|p| status_code(p)) {
                if st < 400 {
                    d.op.success_status.get_or_insert(st);
                } else {
                    add_error(&mut d.op, Some(st), None, src.ev(s));
                }
            }
        }
        for (pat, loc) in [("request.args.get(", "query"), ("request.headers.get(", "header")] {
            for at in find_all_plain(body, pat) {
                let open = bs + at + pat.len() - 1;
                if let Some((_, args)) = call_args(src, open) {
                    if let Some(n) = positional(src, &args, 0).and_then(string_lit) {
                        let required = args.len() < 2;
                        add_param(
                            &mut d.op,
                            Param {
                                name: n,
                                code_name: None,
                                location: loc.into(),
                                type_name: "string".into(),
                                required: required && loc == "path",
                                rules: vec![],
                                evidence: src.ev(open),
                            },
                        );
                    }
                }
            }
        }
    }
}

fn find_all_plain(hay: &str, needle: &str) -> Vec<usize> {
    hay.match_indices(needle).map(|(i, _)| i).collect()
}

// ---------------------------------------------------------------- models

struct Class {
    file: usize,
    name: String,
    bases: Vec<String>,
    line: usize,
    body: Vec<(usize, usize, usize)>,
    decorators: String,
    header: (usize, usize),
}

fn collect_classes(u: &mut Unit, lines: &[Vec<(usize, usize, usize)>]) -> Vec<Class> {
    let mut out = Vec::new();
    for (fi, ls) in lines.iter().enumerate() {
        let src = &u.files[fi].src;
        for (li, &(s, e, indent)) in ls.iter().enumerate() {
            let code = src.code_slice(s, e);
            let Some(rest) = code.strip_prefix("class ") else { continue };
            let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            let name_end = s + 6 + name.len();
            let bases: Vec<String> = match src.code.as_bytes().get(name_end) {
                Some(b'(') => matching(&src.code, name_end)
                    .map(|close| {
                        split_top(src.slice(name_end + 1, close), &[','])
                            .into_iter()
                            .filter(|b| !b.contains('='))
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
                _ => vec![],
            };
            let body: Vec<(usize, usize, usize)> = ls[li + 1..].iter().take_while(|l| l.2 > indent).copied().collect();
            let body_indent = body.first().map(|l| l.2).unwrap_or(0);
            let body: Vec<(usize, usize, usize)> = body.into_iter().filter(|l| l.2 == body_indent).collect();
            let mut decorators = String::new();
            let mut k = li;
            while k > 0 && src.code[ls[k - 1].0..].starts_with('@') {
                k -= 1;
                decorators.push_str(src.slice(ls[k].0, ls[k].1));
            }
            let is_enum = bases.iter().any(|b| b.ends_with("Enum") || b == "enum.Enum");
            if is_enum {
                let vals: Vec<String> = body
                    .iter()
                    .filter_map(|&(bs, be, _)| {
                        src.slice(bs, be).split_once('=').and_then(|(_, v)| {
                            string_lit(v.trim()).or_else(|| v.trim().parse::<i64>().ok().map(|n| n.to_string()))
                        })
                    })
                    .collect();
                u.enums.insert(name.clone(), vals);
            }
            out.push(Class { file: fi, name, bases, line: s, body, decorators, header: (s, e) });
        }
    }
    // Model classes: Pydantic / SQLModel / dataclasses / TypedDict, transitively.
    let mut models: HashSet<String> = HashSet::new();
    loop {
        let before = models.len();
        for c in &out {
            let base_names: Vec<String> = c
                .bases
                .iter()
                .map(|b| b.split('[').next().unwrap_or(b).rsplit('.').next().unwrap_or(b).to_string())
                .collect();
            if base_names.iter().any(|b| {
                matches!(b.as_str(), "BaseModel" | "SQLModel" | "Schema" | "TypedDict" | "RootModel" | "BaseSchema")
                    || models.contains(b)
            }) || c.decorators.contains("dataclass")
            {
                models.insert(c.name.clone());
            }
        }
        if models.len() == before {
            break;
        }
    }
    u.models = models;
    out
}

fn build_models(u: &Unit, classes: &[Class]) -> Vec<Model> {
    let by_name: HashMap<&str, &Class> =
        classes.iter().filter(|c| u.models.contains(&c.name)).map(|c| (c.name.as_str(), c)).collect();
    let mut out = Vec::new();
    for c in classes.iter().filter(|c| u.models.contains(&c.name)) {
        let mut chain = vec![c];
        let mut cur = c;
        for _ in 0..8 {
            let Some(parent) = cur.bases.iter().find_map(|b| by_name.get(b.rsplit('.').next().unwrap_or(b))) else {
                break;
            };
            chain.push(parent);
            cur = parent;
        }
        let camel_aliases = chain.iter().any(|k| class_config(u, k).contains("to_camel"));
        let mut fields: Vec<Field> = Vec::new();
        for k in chain.iter().rev() {
            for f in class_fields(u, k, camel_aliases) {
                match fields
                    .iter_mut()
                    .find(|x| x.code_name.as_ref().unwrap_or(&x.name) == f.code_name.as_ref().unwrap_or(&f.name))
                {
                    Some(existing) => *existing = f,
                    None => fields.push(f),
                }
            }
        }
        let src = &u.files[c.file].src;
        let end = c.body.last().map(|l| l.1).unwrap_or(c.header.1);
        out.push(Model {
            id: format!("{}:{}", u.unit, c.name),
            unit: u.unit.into(),
            name: c.name.clone(),
            fields,
            doc: class_doc(src, c),
            evidence: src.ev_range(c.line, end, None),
        });
    }
    out
}

fn class_config(u: &Unit, c: &Class) -> String {
    let src = &u.files[c.file].src;
    c.body
        .iter()
        .map(|&(s, e, _)| src.slice(s, e))
        .filter(|l| l.starts_with("model_config") || l.contains("alias_generator"))
        .collect()
}

fn class_doc(src: &Src, c: &Class) -> Option<String> {
    let first = c.body.first()?;
    let text = src.slice(first.0, src.text.len());
    let q = ["\"\"\"", "'''"].into_iter().find(|q| text.starts_with(q))?;
    let end = text[3..].find(q)? + 3;
    first_sentence(&text[3..end].split_whitespace().collect::<Vec<_>>().join(" "))
}

fn class_fields(u: &Unit, c: &Class, camel_aliases: bool) -> Vec<Field> {
    let src = &u.files[c.file].src;
    let mut out = Vec::new();
    for &(s, e, _) in &c.body {
        let code = src.code_slice(s, e);
        let first: String = code.chars().take_while(|ch| ch.is_alphanumeric() || *ch == '_').collect();
        if first.is_empty()
            || matches!(
                first.as_str(),
                "def" | "class" | "async" | "return" | "pass" | "if" | "for" | "model_config" | "Config"
            )
            || first.starts_with('_')
        {
            continue;
        }
        let after = code[first.len()..].trim_start();
        if !after.starts_with(':') {
            continue;
        }
        let (name, ann, default) = split_param(src.slice(s, e));
        let ann = ann.unwrap_or_default();
        if ann.starts_with("ClassVar")
            || ann.contains("Relationship")
            || default.as_deref().is_some_and(|d| d.starts_with("Relationship("))
        {
            continue;
        }
        let ev = src.ev(s);
        let mut rules = type_rules(u, &ann, &ev);
        let mut required = default.is_none();
        let mut wire = if camel_aliases { camel(&name) } else { name.clone() };
        let mut doc = None;
        let field_call = default.as_deref().filter(|d| d.starts_with("Field(")).map(str::to_string).or_else(|| {
            ann.find("Field(").map(|p| {
                let rest = &ann[p..];
                rest[..rest.rfind(')').map(|x| x + 1).unwrap_or(rest.len())].to_string()
            })
        });
        if let Some(fc) = field_call {
            if let Some((_, parts)) = call_parts(&fc) {
                let (pos, kws) = kwargs_of(&parts);
                if default.as_deref().is_some_and(|d| d.starts_with("Field(")) {
                    required = !(kws.contains_key("default")
                        || kws.contains_key("default_factory")
                        || pos.first().is_some_and(|p| *p != "..."));
                }
                rules.extend(constraint_rules(&kws, &ann, &ev));
                if let Some(a) = kws.get("alias").or_else(|| kws.get("serialization_alias")).and_then(|a| string_lit(a))
                {
                    wire = a;
                }
                doc = kws.get("description").and_then(|d| string_lit(d));
            }
        }
        let mut seen = Vec::new();
        rules.retain(|r| {
            let k = r.statement.clone();
            let fresh = !seen.contains(&k);
            seen.push(k);
            fresh
        });
        let type_name = ann
            .strip_prefix("Annotated[")
            .and_then(|x| split_top(x, &[',']).first().map(|t| t.to_string()))
            .unwrap_or(ann.clone());
        out.push(Field {
            code_name: (wire != name).then(|| name.clone()),
            name: wire,
            type_name,
            required,
            rules,
            doc,
            model: None,
            evidence: ev,
        });
    }
    out
}
