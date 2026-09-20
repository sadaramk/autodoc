//! Go: net/http (1.22 method patterns), gorilla/mux, chi (Route / Group / Mount / With),
//! gin, echo and fiber groups; structs with json and validate / binding tags.

use std::collections::{BTreeMap, HashMap};

use super::text::*;
use super::*;

const ROUTE_METHODS: &[&str] = &[
    "HandleFunc",
    "Handle",
    "Get",
    "Post",
    "Put",
    "Patch",
    "Delete",
    "Head",
    "Options",
    "GET",
    "POST",
    "PUT",
    "PATCH",
    "DELETE",
    "HEAD",
    "OPTIONS",
    "Any",
    "Method",
    "MethodFunc",
];

struct Unit<'f, 'a> {
    unit: &'f str,
    files: Vec<&'f Loaded<'a>>,
    consts: HashMap<String, String>,
    /// Prefixes a function's routes receive from `Mount("/x", fn())` / `StripPrefix`.
    /// Prefixes a function's routes receive, and whether each was fully resolved.
    fn_prefixes: HashMap<String, Vec<(String, bool)>>,
    structs: HashMap<String, Vec<Field>>,
    fn_returns: HashMap<String, String>,
}

pub(crate) fn extract(files: &[Loaded], h: &mut Harvest) {
    let mut by_unit: BTreeMap<&str, Vec<&Loaded>> = BTreeMap::new();
    for f in files.iter().filter(|f| f.lang == Language::Go && !f.path().ends_with("_test.go")) {
        by_unit.entry(f.unit).or_default().push(f);
    }
    for (unit, fs) in by_unit {
        let mut u = Unit {
            unit,
            files: fs,
            consts: HashMap::new(),
            fn_prefixes: HashMap::new(),
            structs: HashMap::new(),
            fn_returns: HashMap::new(),
        };
        let mut models = Vec::new();
        for fi in 0..u.files.len() {
            collect_consts(&mut u, fi);
            collect_structs(&u, fi, &mut models);
            collect_fn_returns(&mut u, fi);
        }
        // Embedded structs contribute their fields.
        let raw: HashMap<String, (Vec<Field>, Vec<String>)> = models
            .iter()
            .map(|(m, e): &(Model, Vec<String>)| (m.name.clone(), (m.fields.clone(), e.clone())))
            .collect();
        for (m, embedded) in &mut models {
            let mut fields = Vec::new();
            for e in embedded.iter() {
                if let Some((fs, _)) = raw.get(e) {
                    fields.extend(fs.iter().cloned());
                }
            }
            fields.append(&mut m.fields);
            m.fields = fields;
            u.structs.insert(m.name.clone(), m.fields.clone());
        }
        for (m, _) in models {
            h.model(m);
        }
        for fi in 0..u.files.len() {
            collect_mounts(&mut u, fi);
        }
        for fi in 0..u.files.len() {
            collect_routes(&u, fi, h);
            collect_builder_routes(&u, fi, h);
        }
    }
}

fn collect_consts(u: &mut Unit, fi: usize) {
    let src = &u.files[fi].src;
    for line in src.text.lines() {
        let l = line.trim().trim_start_matches("const ").trim_start_matches("var ");
        let Some((lhs, rhs)) = l.split_once('=') else { continue };
        let name = lhs.trim().trim_end_matches(':').split_whitespace().next().unwrap_or("");
        if !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            if let Some(v) = string_lit(rhs.trim()) {
                u.consts.entry(name.to_string()).or_insert(v);
            }
        }
    }
}

/// Static value of a path expression; `None` for the parts that can't be resolved.
fn eval_path(u: &Unit, src: &Src, s: usize, e: usize) -> (String, bool) {
    let mut out = String::new();
    let mut partial = false;
    for part in split_top(src.slice(s, e), &['+']) {
        match string_lit(part) {
            Some(v) => out.push_str(&v),
            None => match u.consts.get(part.rsplit('.').next().unwrap_or(part)) {
                Some(v) => out.push_str(v),
                None => partial = true,
            },
        }
    }
    (out, partial)
}

fn collect_mounts(u: &mut Unit, fi: usize) {
    let src = &u.files[fi].src;
    let ranges = chi_ranges(u, src);
    let mut found: Vec<(String, String, bool)> = Vec::new();
    for (rs, ms, open) in method_calls(&src.code, &["Mount", "Handle"]) {
        let Some(close) = matching(&src.code, open) else { continue };
        let args = split_args(&src.code, open + 1, close);
        if args.len() < 2 {
            continue;
        }
        let (prefix, prefix_partial) = eval_path(u, src, args[0].0, args[0].1);
        let (outer, outer_partial) = range_prefix(&ranges, rs);
        let prefix = join_path(&outer, &prefix);
        let prefix_partial = prefix_partial || outer_partial;
        let mut target = src.slice(args[1].0, args[1].1).trim().to_string();
        let is_mount = src.code[ms..].starts_with("Mount");
        if let Some(inner) = target.strip_prefix("http.StripPrefix(") {
            let parts = split_top(inner.trim_end_matches(')'), &[',']);
            target = parts.get(1).map(|x| x.to_string()).unwrap_or_default();
        } else if !is_mount {
            continue;
        }
        let Some(p) = target.find('(') else { continue };
        let name = target[..p].rsplit('.').next().unwrap_or("").to_string();
        if !name.is_empty() {
            found.push((name, if prefix == "/" { String::new() } else { prefix }, prefix_partial));
        }
    }
    for (n, p, partial) in found {
        u.fn_prefixes.entry(n).or_default().push((p, partial));
    }
}

fn is_client_receiver(recv: &str) -> bool {
    let last = recv.rsplit('.').next().unwrap_or(recv);
    let lower = last.to_lowercase();
    recv == "http"
        || lower.contains("client")
        || last == "Header"
        || recv.ends_with("Query()")
        || last == "URL"
        || last == "Values"
        || lower == "q"
        || lower.ends_with("cache")
        || lower.ends_with("store")
        || lower == "ctx"
        || lower == "tx"
        || lower == "os"
}

/// chi `Route("/x", func(r chi.Router) { … })` and `Group(func …)` bodies with their prefixes, outermost first.
fn chi_ranges(u: &Unit, src: &Src) -> Vec<(usize, usize, String, bool)> {
    let code = &src.code;
    let mut ranges = Vec::new();
    for (_, ms, open) in method_calls(code, &["Route", "Group"]) {
        let Some(close) = matching(code, open) else { continue };
        let args = split_args(code, open + 1, close);
        let is_route = code[ms..].starts_with("Route");
        let ((prefix, partial), func_arg) = match (is_route, args.as_slice()) {
            (true, [p, f, ..]) => (eval_path(u, src, p.0, p.1), *f),
            (false, [f]) => ((String::new(), false), *f),
            _ => continue,
        };
        if !src.code_slice(func_arg.0, func_arg.1).starts_with("func") {
            continue;
        }
        ranges.push((func_arg.0, func_arg.1, prefix, partial));
    }
    ranges
}

/// The enclosing chi `Route`/`Group` prefixes, and whether any could not be
/// resolved — a route under an unresolved prefix is a suffix of the truth.
fn range_prefix(ranges: &[(usize, usize, String, bool)], at: usize) -> (String, bool) {
    ranges.iter().filter(|(s, e, _, _)| *s < at && at < *e).fold((String::new(), false), |(acc, part), (_, _, p, q)| {
        (join_path(&acc, p).trim_end_matches('/').to_string(), part || *q)
    })
}

fn collect_routes(u: &Unit, fi: usize, h: &mut Harvest) {
    let f = u.files[fi];
    let src = &f.src;
    let code = &src.code;
    let ranges = chi_ranges(u, src);
    // gin / echo / fiber group variables: v1 := r.Group("/v1", mw).
    let mut groups: HashMap<String, (String, String, Vec<Requirement>, bool)> = HashMap::new();
    for (rs, ms, open) in method_calls(code, &["Group"]) {
        let Some(close) = matching(code, open) else { continue };
        let args = split_args(code, open + 1, close);
        let Some(&(ps, pe)) = args.first() else { continue };
        if src.code_slice(ps, pe).starts_with("func") {
            continue;
        }
        let before = code[..rs].trim_end();
        let Some(lhs) = before.strip_suffix(":=").or_else(|| before.strip_suffix('=')) else { continue };
        let Some((ns, ne)) = ident_before(code, lhs.len()) else { continue };
        let (prefix, prefix_partial) = eval_path(u, src, ps, pe);
        let auth = args[1..].iter().filter_map(|&(s, e)| auth_requirement(src.slice(s, e), src.ev(s))).collect();
        groups.insert(
            src.slice(ns, ne).to_string(),
            (src.code_slice(rs, ms - 1).to_string(), prefix, auth, prefix_partial),
        );
    }
    // Use(mw) per receiver name.
    let mut uses: Vec<(String, usize, Requirement)> = Vec::new();
    for (rs, ms, open) in method_calls(code, &["Use"]) {
        let Some(close) = matching(code, open) else { continue };
        for (s, e) in split_args(code, open + 1, close) {
            if let Some(a) = auth_requirement(src.slice(s, e), src.ev(s)) {
                uses.push((src.code_slice(rs, ms - 1).to_string(), rs, a));
            }
        }
    }

    for (rs, ms, open) in method_calls(code, ROUTE_METHODS) {
        let Some(close) = matching(code, open) else { continue };
        let name_len = code[ms..].bytes().take_while(|&b| is_ident(b)).count();
        let mname = &code[ms..ms + name_len];
        let recv = src.code_slice(rs, ms - 1).to_string();
        if is_client_receiver(&recv) {
            continue;
        }
        let mut args = split_args(code, open + 1, close);
        let mut method = match mname {
            "HandleFunc" | "Handle" => "ANY".to_string(),
            "Method" | "MethodFunc" => {
                let Some(m) = args
                    .first()
                    .and_then(|&(s, e)| string_lit(src.slice(s, e)).or_else(|| status_method(src.slice(s, e))))
                else {
                    continue;
                };
                args.remove(0);
                m.to_uppercase()
            }
            "Any" => "ANY".to_string(),
            m => m.to_uppercase(),
        };
        if args.len() < 2 {
            continue;
        }
        let (mut path, mut partial) = eval_path(u, src, args[0].0, args[0].1);
        if let Some((m, p)) = path.split_once(' ') {
            if m.chars().all(|c| c.is_ascii_uppercase()) {
                method = m.to_string();
                path = p.trim().to_string();
            }
        }
        let literal_empty = !partial && path.is_empty() && string_lit(src.slice(args[0].0, args[0].1)).is_some();
        if !path.starts_with('/') && !literal_empty {
            continue;
        }
        if mname == "Handle" && src.slice(args[1].0, args[1].1).contains("StripPrefix") {
            continue;
        }
        // gorilla: .Methods("POST")
        for (n, o, c) in super::ts::call_chain(code, close + 1) {
            if n == "Methods" {
                if let Some(m) = split_args(code, o + 1, c)
                    .first()
                    .and_then(|&(s, e)| string_lit(src.slice(s, e)).or_else(|| status_method(src.slice(s, e))))
                {
                    method = m.to_uppercase();
                }
            }
        }
        // Prefix: group variable chain, enclosing chi ranges, enclosing function mounts.
        let mut prefix = String::new();
        let mut auth: Vec<Requirement> = Vec::new();
        let base_recv = recv.split(".With(").next().unwrap_or(&recv).to_string();
        if recv.contains(".With(") {
            if let Some(p) = recv.find(".With(") {
                let inner = &recv[p + 6..];
                for part in split_top(inner.split(')').next().unwrap_or(""), &[',']) {
                    if let Some(a) = auth_requirement(part, src.ev(rs)) {
                        auth.push(a);
                    }
                }
            }
        }
        // A prefix that could not be resolved makes every route under it a
        // suffix of the real path, however exact the leaf looks.
        let mut prefix_partial = false;
        let mut cur = base_recv.clone();
        for _ in 0..6 {
            let Some((parent, p, a, q)) = groups.get(&cur) else { break };
            prefix = join_path(p, &prefix);
            auth.extend(a.iter().cloned());
            prefix_partial |= *q;
            cur = parent.clone();
        }
        let (outer, outer_partial) = range_prefix(&ranges, rs);
        prefix = join_path(&outer, &prefix);
        prefix_partial |= outer_partial;
        let line = src.line(rs);
        let enclosing = f.enclosing(line).map(|s| s.name.clone());
        let fn_prefixes = enclosing
            .as_ref()
            .and_then(|n| u.fn_prefixes.get(n))
            .cloned()
            .unwrap_or_else(|| vec![(String::new(), false)]);
        for (recv_name, at, a) in &uses {
            let same_scope = enclosing.as_ref() == f.enclosing(src.line(*at)).map(|s| &s.name);
            let in_range = ranges.iter().any(|(s, e, _, _)| s < at && at < e && *s < rs && rs < *e)
                || !ranges.iter().any(|(s, e, _, _)| s < at && at < e);
            if (*recv_name == base_recv || groups.get(&base_recv).is_some_and(|g| g.0 == *recv_name))
                && same_scope
                && in_range
            {
                auth.push(a.clone());
            }
        }
        for &(s, e) in &args[1..args.len() - 1] {
            if let Some(a) = auth_requirement(src.slice(s, e), src.ev(s)) {
                auth.push(a);
            }
        }
        if path.is_empty() {
            path = "/".into();
        }
        let framework = framework_of(f);
        let (hs, he) = *args.last().unwrap();
        for (fp, fp_partial) in &fn_prefixes {
            let full = join_path(&join_path(fp, &prefix), &path);
            let mut op = new_op(
                u.unit,
                framework,
                &method,
                full,
                SymbolRef { name: String::new(), evidence: src.ev(hs) },
                src.ev_range(rs, close, None),
            );
            op.path_partial = partial || prefix_partial || *fp_partial;
            for a in &auth {
                add_auth(&mut op, a.clone());
            }
            let mut d = Draft { op, request_declared: false, response_declared: false };
            let mut synth = Vec::new();
            handler(u, fi, (hs, he), &mut d, &mut synth);
            h.ops.push(d);
            for m in synth {
                h.model(m);
            }
        }
        partial = false;
        let _ = partial;
    }
}

/// Drop whitespace, so a chain broken across lines reads as one expression.
fn squeeze(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// A `*mux.Router` the function receives, not rebound by a `range` loop.
fn is_root_router(code: &str, name: &str) -> bool {
    let flat = squeeze(code);
    let bound_by_loop = flat.contains(&format!(",{name}:=range")) || flat.contains(&format!("for{name}:=range"));
    !bound_by_loop && (flat.contains(&format!("{name}*mux.Router")) || flat.contains(&format!("{name}mux.Router")))
}

/// Look through a middleware wrapper to the handler it decorates:
/// `s3APIMiddleware(api.HeadObjectHandler, flag)` serves HeadObject, and naming
/// the wrapper would give every route in the router the same handler and the
/// same documentation.
fn unwrap_middleware(code: &str, span: (usize, usize)) -> (usize, usize) {
    let mut span = span;
    for _ in 0..4 {
        let Some(rel) = code[span.0..span.1].find('(') else { break };
        let open = span.0 + rel;
        let Some(close) = matching(code, open) else { break };
        if close >= span.1 {
            break;
        }
        let inner = split_args(code, open + 1, close)
            .into_iter()
            .find(|&(a, b)| code[a..b].trim_end_matches(')').trim().ends_with("Handler"));
        match inner {
            Some(next) if next != span => span = next,
            _ => break,
        }
    }
    span
}

/// gorilla/mux, or one of the forks that keep its API: MinIO serves its whole
/// S3 surface through `github.com/minio/mux`, so matching the canonical import
/// path alone would document none of it.
fn imports_mux(text: &str) -> bool {
    text.contains("/mux\"") || text.contains("/mux/v")
}

/// gorilla/mux routers reached through `Subrouter()`, and the prefix each one
/// carries: `api := r.PathPrefix("/v1").Subrouter()` → `api` serves `/v1`.
///
/// `None` marks a router whose prefix could not be resolved — it was built in a
/// loop, or from a slice — so routes on it are reported with a partial path
/// rather than as though they sat at the root.
fn mux_subrouters(u: &Unit, src: &Src) -> HashMap<String, Option<String>> {
    let code = &src.code;
    let mut out: HashMap<String, Option<String>> = HashMap::new();
    // Several passes: a subrouter may be defined from one declared after it.
    for _ in 0..4 {
        for (rs, ms, open) in method_calls(code, &["NewRouter", "Subrouter"]) {
            let Some(close) = matching(code, open) else { continue };
            let before = code[..rs].trim_end();
            let Some(lhs) = before.strip_suffix(":=").or_else(|| before.strip_suffix('=')) else { continue };
            let Some((ns, ne)) = ident_before(code, lhs.len()) else { continue };
            let name = src.slice(ns, ne).to_string();
            // The code view blanks string contents, so a prefix literal has to
            // be read from the text; a chain may also wrap across lines.
            let recv = squeeze(src.slice(rs, ms - 1));
            let resolved = if code[ms..].starts_with("NewRouter") {
                Some(String::new())
            } else {
                mux_prefix_of(u, code, &recv, &out)
            };
            // An unresolved entry may become resolvable on a later pass, but a
            // resolved one never degrades.
            match out.get(&name) {
                Some(Some(_)) => {}
                _ => {
                    out.insert(name, resolved);
                }
            }
            let _ = close;
        }
    }
    out
}

/// The prefix a receiver expression serves, or `None` when it cannot be known.
fn mux_prefix_of(u: &Unit, code: &str, recv: &str, known: &HashMap<String, Option<String>>) -> Option<String> {
    let recv = recv.trim();
    if let Some(found) = known.get(recv) {
        return found.clone();
    }
    // A router the function is handed serves everything, unless the name is
    // rebound by a loop — MinIO registers the same routes against a slice of
    // subrouters that way, and each carries a prefix this cannot know.
    if !recv.contains('.') && is_root_router(code, recv) {
        return Some(String::new());
    }
    // `<base>.PathPrefix("/x")` — resolvable only when the base is.
    let (base, rest) = recv.split_once(".PathPrefix(")?;
    let arg = rest.strip_suffix(')')?;
    let base_prefix = mux_prefix_of(u, code, base, known)?;
    let literal = string_lit(arg).or_else(|| u.consts.get(arg.trim()).cloned())?;
    Some(join_path(&base_prefix, &literal))
}

/// gorilla/mux builder chains: `r.Methods("PUT").Path("/x").HandlerFunc(h).Queries("k", "")`.
///
/// The path is not an argument of the route call here — it is a separate link in
/// the chain — so the registration reads as no route at all and a whole API can
/// go undocumented. MinIO registers its entire S3 surface this way.
fn collect_builder_routes(u: &Unit, fi: usize, h: &mut Harvest) {
    let f = u.files[fi];
    let src = &f.src;
    let code = &src.code;
    if !imports_mux(&src.text) {
        return;
    }
    let subrouters = mux_subrouters(u, src);

    for (rs, ms, open) in method_calls(code, &["Methods", "Path", "PathPrefix"]) {
        let Some(close) = matching(code, open) else { continue };
        let recv = squeeze(src.slice(rs, ms - 1));
        // Only the head of a chain; the later links are reached from it.
        if recv.ends_with(')') || is_client_receiver(&recv) {
            continue;
        }
        let name_len = code[ms..].bytes().take_while(|&b| is_ident(b)).count();
        let head = (code[ms..ms + name_len].to_string(), open, close);
        let mut chain = vec![head];
        chain.extend(super::ts::call_chain(code, close + 1));

        let mut methods: Vec<String> = Vec::new();
        let mut path: Option<(String, bool)> = None;
        let mut queries: Vec<String> = Vec::new();
        let mut handler_at: Option<(usize, usize)> = None;
        for (name, o, c) in &chain {
            let args = split_args(code, o + 1, *c);
            match name.as_str() {
                "Methods" => {
                    for &(s, e) in &args {
                        if let Some(m) = string_lit(src.slice(s, e)).or_else(|| status_method(src.slice(s, e))) {
                            methods.push(m.to_uppercase());
                        }
                    }
                }
                "Path" | "PathPrefix" => {
                    if let Some(&(s, e)) = args.first() {
                        let (p, partial) = eval_path(u, src, s, e);
                        if !p.is_empty() {
                            path = Some((p, partial || name == "PathPrefix"));
                        }
                    }
                }
                // `Queries("uploads", "")` is what tells two routes on the same
                // method and path apart, exactly as Spring's `params` does.
                "Queries" => {
                    for &(s, e) in args.iter().step_by(2) {
                        if let Some(k) = string_lit(src.slice(s, e)) {
                            queries.push(k);
                        }
                    }
                }
                "HandlerFunc" | "Handler" => handler_at = args.first().copied(),
                _ => {}
            }
        }
        let (Some((path, mut partial)), Some(raw_handler)) = (path, handler_at) else { continue };
        let (hs, he) = unwrap_middleware(code, raw_handler);
        if !path.starts_with('/') {
            continue;
        }
        let prefix = match mux_prefix_of(u, code, &recv, &subrouters) {
            Some(p) => p,
            None => {
                partial = true;
                String::new()
            }
        };
        if methods.is_empty() {
            methods.push("ANY".into());
        }
        let framework = framework_of(f);
        let selector = (!queries.is_empty()).then(|| queries.join(", "));
        for method in &methods {
            let mut op = new_op(
                u.unit,
                framework,
                method,
                join_path(&prefix, &path),
                SymbolRef { name: String::new(), evidence: src.ev(hs) },
                src.ev_range(rs, *chain.last().map(|(_, _, c)| c).unwrap_or(&close), None),
            );
            op.path_partial = partial;
            if let Some(sel) = &selector {
                op.id = format!("{}?{sel}", op.id);
                op.selector = Some(sel.clone());
            }
            let mut d = Draft { op, request_declared: false, response_declared: false };
            let mut synth = Vec::new();
            handler(u, fi, (hs, he), &mut d, &mut synth);
            h.ops.push(d);
            for m in synth {
                h.model(m);
            }
        }
    }
}

fn status_method(expr: &str) -> Option<String> {
    let last = expr.trim().rsplit('.').next()?;
    last.strip_prefix("Method").map(|m| m.to_uppercase())
}

fn framework_of(f: &Loaded) -> &'static str {
    let t = &f.src.text;
    if t.contains("github.com/gin-gonic/gin") {
        "gin"
    } else if t.contains("github.com/labstack/echo") {
        "echo"
    } else if t.contains("github.com/gofiber/fiber") {
        "fiber"
    } else if t.contains("github.com/go-chi/chi") {
        "chi"
    } else if imports_mux(t) {
        "gorilla"
    } else {
        "net/http"
    }
}

fn handler(u: &Unit, fi: usize, (hs, he): (usize, usize), d: &mut Draft, synth: &mut Vec<Model>) {
    let src = &u.files[fi].src;
    let expr = src.slice(hs, he).trim().to_string();
    let (hfi, body, name) = if expr.starts_with("func") {
        let Some(open) = src.code[hs..he].find('{').map(|x| hs + x) else { return };
        (fi, (open, matching(&src.code, open).unwrap_or(he)), format!("{} {}", d.op.method, d.op.path))
    } else {
        let callee = expr.split('(').next().unwrap_or(&expr);
        let name = callee.rsplit('.').next().unwrap_or(callee).to_string();
        // Prefer hand-written implementations over generated wrappers (oapi-codegen `*.gen.go`).
        let generated = |i: &usize| u.files[*i].path().ends_with(".gen.go");
        let order: Vec<usize> = std::iter::once(fi)
            .chain(0..u.files.len())
            .filter(|i| !generated(i))
            .chain((0..u.files.len()).filter(generated))
            .collect();
        let found = order.into_iter().find_map(|i| u.files[i].function(&name).map(|s| (i, s)));
        let Some((i, sym)) = found else {
            d.op.handler.name = name;
            return;
        };
        let f = u.files[i];
        let (s, e) = f.span(sym);
        d.op.handler.evidence = f.src.ev_range(s, e, Some(&name));
        d.op.summary = sym.doc.as_deref().and_then(first_sentence);
        let Some(open) =
            f.src.code[s..e].find(") {").map(|x| s + x + 2).or_else(|| f.src.code[s..e].find('{').map(|x| s + x))
        else {
            return;
        };
        (i, (open, e), name)
    };
    d.op.handler.name = name;
    let src = &u.files[hfi].src;
    let code = &src.code;
    let (bs, be) = body;
    let body_code = &code[bs..be];

    // Request body.
    for m in
        ["Decode", "ShouldBindJSON", "BindJSON", "ShouldBind", "Bind", "BodyParser", "DecodeJSON", "ShouldBindWith"]
    {
        for at in find_word(body_code, m) {
            let open = skip_ws(code, bs + at + m.len());
            if code.as_bytes().get(open) != Some(&b'(') {
                continue;
            }
            let Some(close) = matching(code, open) else { continue };
            let args = split_args(code, open + 1, close);
            let Some(target) = args.iter().map(|&(s, e)| src.slice(s, e)).find(|a| a.starts_with('&')) else {
                continue;
            };
            if let Some(t) = var_type(u, src, body, target.trim_start_matches('&')) {
                d.op.request_body = Some(type_ref(&t));
                d.request_declared = true;
            }
        }
    }

    // Params.
    for (pat, loc) in [
        ("URL.Query().Get(", "query"),
        ("c.Query(", "query"),
        ("c.DefaultQuery(", "query"),
        ("c.QueryParam(", "query"),
        ("Header.Get(", "header"),
        ("c.GetHeader(", "header"),
    ] {
        for (at, _) in body_code.match_indices(pat) {
            let open = bs + at + pat.len() - 1;
            let Some(close) = matching(code, open) else { continue };
            let Some(n) = split_args(code, open + 1, close).first().and_then(|&(s, e)| string_lit(src.slice(s, e)))
            else {
                continue;
            };
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

    // Responses and statuses.
    let mut success: Option<u16> = None;
    for (rs, ms, open) in method_calls(
        body_code,
        &[
            "WriteHeader",
            "Error",
            "JSON",
            "IndentedJSON",
            "AbortWithStatus",
            "AbortWithStatusJSON",
            "AbortWithError",
            "String",
            "NoContent",
            "Status",
            "SendStatus",
            "NewHTTPError",
            "NewError",
            "Encode",
            "Redirect",
        ],
    ) {
        let (rs, ms, open) = (bs + rs, bs + ms, bs + open);
        let Some(close) = matching(code, open) else { continue };
        let name: String = code[ms..].bytes().take_while(|&b| is_ident(b)).map(|b| b as char).collect();
        let recv = src.code_slice(rs, ms - 1);
        let args = split_args(code, open + 1, close);
        let arg = |i: usize| args.get(i).map(|&(s, e)| src.slice(s, e).trim().to_string());
        let ev = src.ev(rs);
        match name.as_str() {
            "Error" if recv == "http" => {
                let st = arg(2).and_then(|a| status_code(&a));
                add_error(&mut d.op, st, arg(1).and_then(|a| string_lit(&a)), ev);
            }
            "WriteHeader" | "AbortWithStatus" | "NoContent" | "SendStatus" | "Status" => {
                if let Some(st) = arg(0).and_then(|a| status_code(&a)) {
                    if st >= 400 {
                        add_error(&mut d.op, Some(st), None, ev);
                    } else {
                        success = success.or(Some(st));
                    }
                }
            }
            "NewHTTPError" | "NewError" | "AbortWithError" => {
                if let Some(st) = arg(0).and_then(|a| status_code(&a)) {
                    add_error(&mut d.op, Some(st), arg(1).and_then(|a| string_lit(&a)), ev);
                }
            }
            "JSON" | "IndentedJSON" | "AbortWithStatusJSON" | "String" => {
                let st = arg(0).and_then(|a| status_code(&a));
                let payload = args.get(1).copied();
                match st {
                    Some(s) if s >= 400 => {
                        let msg = payload.and_then(|(ps, pe)| literal_message(src, ps, pe));
                        add_error(&mut d.op, Some(s), msg, ev);
                    }
                    _ => {
                        success = success.or(st);
                        if let Some((ps, pe)) = payload {
                            response_payload(u, src, body, (ps, pe), d, synth);
                        }
                    }
                }
            }
            "Encode" => {
                if let Some(&(ps, pe)) = args.first() {
                    response_payload(u, src, body, (ps, pe), d, synth);
                }
            }
            _ => {}
        }
    }
    // Helper writers: writeJSON(w, http.StatusCreated, resp) / respond(w, r, status, payload).
    for (at, _) in body_code.match_indices('(') {
        let open = bs + at;
        let Some((ns, ne)) = ident_before(code, open) else { continue };
        let fname = &code[ns..ne];
        let lower = fname.to_lowercase();
        if !(lower.contains("json") || lower.starts_with("respond") || lower.starts_with("write"))
            || matches!(fname, "JSON" | "IndentedJSON" | "AbortWithStatusJSON" | "WriteHeader" | "Write")
        {
            continue;
        }
        if ns > 0
            && code.as_bytes()[ns - 1] == b'.'
            && !code[..ns - 1].ends_with("render")
            && !code[..ns - 1].trim_end().ends_with('h')
        {
            continue;
        }
        let Some(close) = matching(code, open) else { continue };
        let args = split_args(code, open + 1, close);
        let st = args.iter().find_map(|&(s, e)| {
            let a = src.slice(s, e);
            (a.contains("Status") || a.parse::<u16>().is_ok()).then(|| status_code(a)).flatten()
        });
        let Some(&(ps, pe)) = args.last() else { continue };
        match st {
            Some(s) if s >= 400 => add_error(&mut d.op, Some(s), literal_message(src, ps, pe), src.ev(ns)),
            _ => {
                success = success.or(st);
                if args.len() >= 2 {
                    response_payload(u, src, body, (ps, pe), d, synth);
                }
            }
        }
    }
    d.op.success_status = success.or(d.op.success_status);
}

fn literal_message(src: &Src, s: usize, e: usize) -> Option<String> {
    let text = src.slice(s, e).trim();
    if let Some(v) = string_lit(text) {
        return Some(v);
    }
    let open = src.code[s..e].find('{').map(|x| s + x)?;
    object_entries(src, open)
        .into_iter()
        .filter(|(k, ..)| matches!(k.as_str(), "error" | "message" | "msg"))
        .find_map(|(_, _, vs, ve)| string_lit(src.slice(vs, ve)))
}

fn response_payload(
    u: &Unit,
    src: &Src,
    body: (usize, usize),
    (ps, pe): (usize, usize),
    d: &mut Draft,
    synth: &mut Vec<Model>,
) {
    if d.response_declared {
        return;
    }
    let expr = src.slice(ps, pe).trim().to_string();
    let head = expr.split('{').next().unwrap_or("").trim().to_string();
    if expr.contains('{') && (head.starts_with("map[") || head.ends_with(".H") || head.ends_with(".Map") || head == "H")
    {
        let open = ps + src.code[ps..pe].find('{').unwrap_or(0);
        let name = format!("{}{}Response", noun(&d.op.path), pascal(&d.op.method.to_lowercase()));
        let fields: Vec<Field> = object_entries(src, open)
            .into_iter()
            .map(|(k, ks, vs, ve)| Field {
                name: k,
                code_name: None,
                type_name: go_literal_type(src.slice(vs, ve), &head),
                required: true,
                rules: vec![],
                doc: None,
                model: None,
                evidence: src.ev(ks),
            })
            .collect();
        if d.op.response.is_none() && !fields.is_empty() {
            d.op.response = Some(TypeRef {
                type_name: name.clone(),
                model: Some(format!("{}:{name}", d.op.unit)),
                collection: false,
            });
            synth.push(Model {
                id: format!("{}:{name}", d.op.unit),
                unit: d.op.unit.clone(),
                name,
                fields,
                doc: None,
                evidence: src.ev(ps),
            });
        }
        return;
    }
    let t = if expr.contains('{') {
        Some(head.trim_start_matches('&').to_string())
    } else {
        var_type(u, src, body, expr.trim_start_matches('&'))
    };
    if let Some(t) = t.filter(|t| !t.is_empty() && !t.starts_with("map[")) {
        d.op.response = Some(type_ref(&t));
        d.response_declared = true;
    }
}

fn noun(path: &str) -> String {
    path.split('/')
        .filter(|s| !s.is_empty() && !s.starts_with('{'))
        .next_back()
        .map(|s| pascal(&singular(s)))
        .unwrap_or_else(|| "Root".into())
}

fn go_literal_type(v: &str, map_head: &str) -> String {
    let v = v.trim();
    if v.starts_with('"') || v.starts_with('`') {
        return "string".into();
    }
    if v.parse::<f64>().is_ok() {
        return "number".into();
    }
    if v == "true" || v == "false" {
        return "bool".into();
    }
    if let Some(t) = map_head.strip_prefix("map[string]") {
        if t != "any" && t != "interface{}" {
            return t.to_string();
        }
    }
    "unknown".into()
}

/// Declared type of a local variable in a function body.
fn var_type(u: &Unit, src: &Src, (bs, be): (usize, usize), var: &str) -> Option<String> {
    let body = src.slice(bs, be);
    let var = var.trim();
    if !var.bytes().all(is_ident) || var.is_empty() {
        return None;
    }
    for line in body.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix(&format!("var {var} ")) {
            let t = rest.split('=').next().unwrap_or("").trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
        let lhs_rhs = l.split_once(":=").or_else(|| l.split_once(" = "));
        if let Some((lhs, rhs)) = lhs_rhs {
            let names: Vec<&str> = lhs.split(',').map(str::trim).collect();
            if names.first() != Some(&var) {
                continue;
            }
            let rhs = rhs.trim().trim_start_matches('&');
            if let Some(inner) = rhs.strip_prefix("new(") {
                return Some(inner.trim_end_matches(')').to_string());
            }
            if let Some(p) = rhs.find('{') {
                let t = rhs[..p].trim();
                if !t.is_empty() && !t.contains('(') {
                    return Some(t.to_string());
                }
            }
            // x, err := h.svc.Create(...): the callee's first return type.
            if let Some(p) = rhs.find('(') {
                let callee = rhs[..p].rsplit('.').next().unwrap_or("");
                if let Some(t) = u.fn_returns.get(callee) {
                    return Some(t.clone());
                }
            }
        }
    }
    None
}

fn collect_fn_returns(u: &mut Unit, fi: usize) {
    let f = u.files[fi];
    for sym in &f.facts.symbols {
        if !matches!(sym.kind, SymbolKind::Function | SymbolKind::Method) {
            continue;
        }
        let (s, e) = f.span(sym);
        let header_end = f.src.code[s..e].find('{').map(|x| s + x).unwrap_or(e);
        let header = f.src.slice(s, header_end);
        let Some(name_at) = header.find(&format!("{}(", sym.name)) else { continue };
        let after_name = s + name_at + sym.name.len();
        let Some(close) = matching(&f.src.code, after_name) else { continue };
        let ret = f.src.slice(close + 1, header_end).trim();
        let first = ret.trim_start_matches('(').split(',').next().unwrap_or("").trim().trim_end_matches(')');
        if !first.is_empty() && first != "error" {
            u.fn_returns.entry(sym.name.clone()).or_insert(first.to_string());
        }
    }
}

fn collect_structs(u: &Unit, fi: usize, out: &mut Vec<(Model, Vec<String>)>) {
    let src = &u.files[fi].src;
    let code = &src.code;
    for at in find_word(code, "type") {
        let Some((ns, ne)) = ident_at(code, at + 4) else { continue };
        let kw = skip_ws(code, ne);
        if !code[kw..].starts_with("struct") {
            continue;
        }
        let Some(open) = code[kw..].find('{').map(|x| kw + x) else { continue };
        let Some(close) = matching(code, open) else { continue };
        let name = src.slice(ns, ne).to_string();
        let mut fields = Vec::new();
        let mut embedded = Vec::new();
        let body = src.slice(open + 1, close);
        let mut offset = open + 1;
        for line in body.split('\n') {
            let line_start = offset;
            offset += line.len() + 1;
            let l = line.trim();
            if l.is_empty() || l.starts_with("//") {
                continue;
            }
            let (decl, tag) = match l.find('`') {
                Some(p) => (&l[..p], l[p..].trim_matches('`')),
                None => (l.split("//").next().unwrap_or(l), ""),
            };
            let parts: Vec<&str> = decl.split_whitespace().collect();
            if parts.len() == 1 {
                embedded.push(parts[0].trim_start_matches('*').rsplit('.').next().unwrap_or("").to_string());
                continue;
            }
            if parts.len() < 2 {
                continue;
            }
            let ty = parts[parts.len() - 1].to_string();
            let joined = parts[..parts.len() - 1].join(" ");
            let names: Vec<&str> = joined.split(',').map(str::trim).filter(|n| !n.is_empty()).collect();
            let ev = src.ev(line_start + (line.len() - line.trim_start().len()));
            let json = tag_value(tag, "json");
            let validate = tag_value(tag, "validate").or_else(|| tag_value(tag, "binding"));
            for code_name in names {
                if !code_name.chars().next().is_some_and(|c| c.is_uppercase()) {
                    continue;
                }
                let json_parts: Vec<&str> = json.as_deref().unwrap_or("").split(',').collect();
                if json_parts.first() == Some(&"-") {
                    continue;
                }
                let wire = json_parts
                    .first()
                    .filter(|n| !n.is_empty())
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| code_name.to_string());
                let omitempty = json_parts.contains(&"omitempty");
                let rules_src: Vec<&str> =
                    validate.as_deref().unwrap_or("").split(',').filter(|r| !r.is_empty()).collect();
                let required = if rules_src.contains(&"required") {
                    true
                } else if rules_src.contains(&"omitempty") || omitempty || ty.starts_with('*') {
                    false
                } else {
                    validate.is_none()
                };
                let rules = go_rules(&rules_src, &ty, &ev);
                let doc = doc_above(src, ev.start_line, Style::CLike).or_else(|| {
                    line.split("//").nth(1).map(|c| c.trim().to_string()).filter(|c| !c.is_empty() && !tag.contains(c))
                });
                fields.push(Field {
                    name: wire.clone(),
                    code_name: (wire != code_name).then(|| code_name.to_string()),
                    type_name: ty.clone(),
                    required,
                    rules,
                    doc,
                    model: None,
                    evidence: ev.clone(),
                });
            }
        }
        out.push((
            Model {
                id: format!("{}:{name}", u.unit),
                unit: u.unit.into(),
                name,
                fields,
                doc: doc_above(src, src.line(at), Style::CLike),
                evidence: src.ev_range(at, close, None),
            },
            embedded,
        ));
    }
}

fn tag_value(tag: &str, key: &str) -> Option<String> {
    let p = tag.find(&format!("{key}:\""))?;
    let rest = &tag[p + key.len() + 2..];
    Some(rest[..rest.find('"')?].to_string())
}

fn go_rules(rules: &[&str], ty: &str, ev: &EvidenceRef) -> Vec<Rule> {
    let is_str = ty.trim_start_matches('*') == "string";
    let is_slice = ty.starts_with("[]");
    let mut out = Vec::new();
    for r in rules {
        let (k, v) = r.split_once('=').unwrap_or((r, ""));
        let (statement, kind) = match k {
            "min" if is_str => (min_len(v), "length"),
            "max" if is_str => (max_len(v), "length"),
            "len" if is_str => (format!("exactly {v} characters"), "length"),
            "min" if is_slice => (min_items(v), "length"),
            "max" if is_slice => (max_items(v), "length"),
            "min" | "gte" => (format!("must be ≥ {v}"), "range"),
            "max" | "lte" => (format!("must be ≤ {v}"), "range"),
            "gt" => (format!("must be > {v}"), "range"),
            "lt" => (format!("must be < {v}"), "range"),
            "email" => ("must be a valid email".into(), "format"),
            "url" | "http_url" => ("must be a valid URL".into(), "format"),
            "uuid" | "uuid4" => ("must be a UUID".into(), "format"),
            "e164" => ("must be an E.164 phone number".into(), "format"),
            "alphanum" => ("letters and digits only".into(), "pattern"),
            "numeric" => ("digits only".into(), "pattern"),
            "iso3166_1_alpha2" => ("ISO 3166 country code".into(), "format"),
            "datetime" => (format!("must be a date-time ({v})"), "format"),
            "oneof" => (format!("one of: {}", v.split_whitespace().collect::<Vec<_>>().join(", ")), "enum"),
            "dive" | "required" | "omitempty" => continue,
            other => (other.to_string(), "custom"),
        };
        out.push(rule(statement, kind, ev));
    }
    out
}
