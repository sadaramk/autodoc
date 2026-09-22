//! Outbound HTTP calls (fetch / axios, requests / httpx, net/http, reqwest) and their match
//! against the operations units publish.

use super::text::*;
use super::*;

enum Part {
    Lit(String),
    Dyn(String),
}

/// Normalises a URL built from literal and dynamic parts to a path with `{param}` segments.
fn normalise(parts: Vec<Part>) -> Option<String> {
    let mut parts = parts.into_iter().skip_while(|p| matches!(p, Part::Dyn(_))).peekable();
    let mut s = String::new();
    for p in parts.by_ref() {
        match p {
            Part::Lit(l) => s.push_str(&l),
            Part::Dyn(d) => {
                let name: String = d
                    .rsplit(['.', ' ', '('])
                    .next()
                    .unwrap_or("")
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                s.push_str(&format!("{{{}}}", if name.is_empty() { "param".into() } else { name }));
            }
        }
    }
    for scheme in ["http://", "https://"] {
        if let Some(rest) = s.strip_prefix(scheme) {
            s = rest.find('/').map(|i| rest[i..].to_string()).unwrap_or_else(|| "/".into());
        }
    }
    let s = s.split(['?', '#']).next().unwrap_or("").to_string();
    if !s.starts_with('/') {
        return None;
    }
    Some(join_path("", &s))
}

fn template_parts(t: &str) -> Vec<Part> {
    let mut out = Vec::new();
    let mut rest = t;
    while let Some(o) = rest.find("${") {
        if o > 0 {
            out.push(Part::Lit(rest[..o].to_string()));
        }
        let Some(c) = rest[o..].find('}') else { break };
        out.push(Part::Dyn(rest[o + 2..o + c].to_string()));
        rest = &rest[o + c + 1..];
    }
    if !rest.is_empty() {
        out.push(Part::Lit(rest.to_string()));
    }
    out
}

fn brace_parts(t: &str, positional: &[&str]) -> Vec<Part> {
    // Python f-strings / Rust format!: {name} or {} placeholders.
    let mut out = Vec::new();
    let mut rest = t;
    let mut i = 0;
    while let Some(o) = rest.find('{') {
        if o > 0 {
            out.push(Part::Lit(rest[..o].to_string()));
        }
        let Some(c) = rest[o..].find('}') else { break };
        let inner = rest[o + 1..o + c].split(':').next().unwrap_or("");
        let name = if inner.is_empty() {
            let n = positional.get(i).copied().unwrap_or("param");
            i += 1;
            n.to_string()
        } else {
            inner.to_string()
        };
        out.push(Part::Dyn(name));
        rest = &rest[o + c + 1..];
    }
    if !rest.is_empty() {
        out.push(Part::Lit(rest.to_string()));
    }
    out
}

fn concat_parts(expr: &str) -> Vec<Part> {
    split_top(expr, &['+'])
        .into_iter()
        .map(|p| {
            let p = p.trim();
            match string_lit(p) {
                Some(l) if !p.starts_with('`') => Part::Lit(l),
                _ if p.starts_with('`') => Part::Lit(String::new()),
                _ => Part::Dyn(p.to_string()),
            }
        })
        .collect()
}

/// The host a URL expression names, when it names one literally.
///
/// `url_path` throws this away — it wants the path. Keeping it is what lets
/// the book say *this service calls `notifications`, which is not in this
/// repository* instead of dropping the call on the floor, which is what it
/// used to do with every outbound call it could not attribute.
///
/// Only a literal host is taken. `${SERVICE_URL}` with no default names
/// nothing knowable, and guessing from a variable name would be inference.
/// The host a call names, following one level of indirection.
///
/// `const NOTIFY_URL = process.env.NOTIFY_URL ?? "http://notifications:9000"`
/// and then `` fetch(`${NOTIFY_URL}/v1/notifications`) `` is the ordinary
/// shape, and the host is in neither place alone. `resolve_ts_var` only
/// follows a bare identifier, so a template needs its leading `${…}` resolved
/// before the URL can be read.
pub(crate) fn url_host_near(src: &Src, expr: &str) -> Option<String> {
    if let Some(h) = url_host_of(expr) {
        return Some(h);
    }
    let start = expr.find("${")? + 2;
    let end = expr[start..].find('}')? + start;
    let ident = expr[start..end].trim();
    if ident.is_empty() || !ident.bytes().all(is_ident) {
        return None;
    }
    url_host_of(&resolve_ts_var(src, ident))
}

pub(crate) fn url_host_of(expr: &str) -> Option<String> {
    for scheme in ["http://", "https://"] {
        if let Some(at) = expr.find(scheme) {
            let rest = &expr[at + scheme.len()..];
            let host: String =
                rest.chars().take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '.' | '_')).collect();
            if !host.is_empty() && !host.starts_with("localhost") {
                return Some(host);
            }
        }
    }
    None
}

fn url_path(lang: Language, expr: &str) -> Option<String> {
    let e = expr.trim().trim_start_matches('&');
    match lang {
        Language::TypeScript | Language::JavaScript => {
            if e.starts_with('`') && e.ends_with('`') {
                return normalise(template_parts(&e[1..e.len() - 1]));
            }
            if let Some(l) = string_lit(e) {
                return normalise(vec![Part::Lit(l)]);
            }
            if e.contains('+') {
                return normalise(concat_parts(e));
            }
            None
        }
        Language::Python => {
            if let Some(inner) = e.strip_prefix('f').and_then(string_lit) {
                return normalise(brace_parts(&inner, &[]));
            }
            if let Some(l) = string_lit(e) {
                return normalise(vec![Part::Lit(l)]);
            }
            if e.contains('+') {
                return normalise(concat_parts(e));
            }
            None
        }
        Language::Go => {
            if let Some(args) = e.strip_prefix("fmt.Sprintf(").and_then(|r| r.strip_suffix(')')) {
                let parts = split_top(args, &[',']);
                let fmt = string_lit(parts.first()?)?;
                let mut out = Vec::new();
                let mut rest = fmt.as_str();
                let mut i = 1;
                while let Some(p) = rest.find('%') {
                    if p > 0 {
                        out.push(Part::Lit(rest[..p].to_string()));
                    }
                    out.push(Part::Dyn(parts.get(i).copied().unwrap_or("param").to_string()));
                    i += 1;
                    rest = rest.get(p + 2..).unwrap_or("");
                }
                if !rest.is_empty() {
                    out.push(Part::Lit(rest.to_string()));
                }
                return normalise(out);
            }
            if let Some(l) = string_lit(e) {
                return normalise(vec![Part::Lit(l)]);
            }
            normalise(concat_parts(e))
        }
        Language::Rust => {
            if let Some(args) = e.strip_prefix("format!(").and_then(|r| r.strip_suffix(')')) {
                let parts = split_top(args, &[',']);
                let fmt = string_lit(parts.first()?)?;
                return normalise(brace_parts(&fmt, &parts[1..]));
            }
            string_lit(e).and_then(|l| normalise(vec![Part::Lit(l)]))
        }
        _ => None,
    }
}

fn clientish(recv: &str) -> bool {
    let last = recv.rsplit(['.', ':']).next().unwrap_or(recv).to_lowercase();
    matches!(
        last.as_str(),
        "axios" | "requests" | "httpx" | "ky" | "got" | "superagent" | "reqwest" | "http" | "$http" | "session" | "api"
    ) || last.contains("client")
        || last.ends_with("api") && last.len() > 3
}

/// One outbound call found in a file: offset, method, path, the type the caller
/// parses the response into, and field names it reads off the body directly.
/// A call site: offset, method, path, the host it names when it names one,
/// the type the caller reads the body into, and the fields it reads directly.
type Found = (usize, String, String, Option<String>, Option<String>, Vec<String>);

pub(crate) fn extract(files: &[Loaded], h: &mut Harvest) {
    for f in files {
        let src = &f.src;
        let code = &src.code;
        let mut found: Vec<Found> = Vec::new();
        match f.lang {
            Language::TypeScript | Language::JavaScript => {
                for at in find_word(code, "fetch") {
                    if at > 0 && matches!(code.as_bytes()[at - 1], b'.' | b'$') && !code[..at].ends_with("window.") {
                        continue;
                    }
                    let open = skip_ws(code, at + 5);
                    if code.as_bytes().get(open) != Some(&b'(') || code[..at].trim_end().ends_with("function") {
                        continue;
                    }
                    let Some(close) = matching(code, open) else { continue };
                    let args = split_args(code, open + 1, close);
                    let Some(&(us, ue)) = args.first() else { continue };
                    let __expr = resolve_ts_var(src, src.slice(us, ue));
                    let Some(path) = url_path(f.lang, &__expr) else { continue };
                    let host = url_host_near(src, &__expr);
                    let method = args
                        .get(1)
                        .filter(|&&(s, _)| code.as_bytes()[s] == b'{')
                        .and_then(|&(s, _)| object_entries(src, s).into_iter().find(|(k, ..)| k == "method"))
                        .and_then(|(_, _, vs, ve)| string_lit(src.slice(vs, ve)))
                        .unwrap_or_else(|| "GET".into());
                    found.push((at, method.to_uppercase(), path, host, ts_expects(f, close, None), Vec::new()));
                }
                for (rs, ms, open) in method_calls(code, &["get", "post", "put", "patch", "delete"]) {
                    let recv = src.code_slice(rs, ms - 1);
                    if !clientish(recv) || declared_router(src, recv) {
                        continue;
                    }
                    let Some(close) = matching(code, open) else { continue };
                    let Some(&(us, ue)) = split_args(code, open + 1, close).first() else { continue };
                    let __expr = resolve_ts_var(src, src.slice(us, ue));
                    let Some(path) = url_path(f.lang, &__expr) else { continue };
                    let host = url_host_near(src, &__expr);
                    let name_end = ms + code[ms..].bytes().take_while(|&b| is_ident(b)).count();
                    let generic = (code.as_bytes().get(name_end) == Some(&b'<'))
                        .then(|| matching(code, name_end).map(|c| src.slice(name_end + 1, c).to_string()))
                        .flatten();
                    found.push((
                        rs,
                        code[ms..name_end].to_uppercase(),
                        path,
                        host,
                        ts_expects(f, close, generic),
                        Vec::new(),
                    ));
                }
            }
            Language::Python => {
                for (rs, ms, open) in method_calls(code, &["get", "post", "put", "patch", "delete"]) {
                    let recv = src.code_slice(rs, ms - 1);
                    if !clientish(recv) || src.slice(src.line_start(src.line(rs)), rs).trim_start().starts_with('@') {
                        continue;
                    }
                    let Some(close) = matching(code, open) else { continue };
                    let Some(&(us, ue)) = split_args(code, open + 1, close).first() else { continue };
                    let __expr = src.slice(us, ue);
                    let Some(path) = url_path(f.lang, __expr) else { continue };
                    let host = url_host_near(src, __expr);
                    let name_end = ms + code[ms..].bytes().take_while(|&b| is_ident(b)).count();
                    let (expects, keys) = py_expects(f, rs, close + 1);
                    found.push((rs, code[ms..name_end].to_uppercase(), path, host, expects, keys));
                }
            }
            Language::Go => {
                for (rs, ms, open) in method_calls(code, &["NewRequest", "NewRequestWithContext", "Get", "Post"]) {
                    let recv = src.code_slice(rs, ms - 1);
                    let name_end = ms + code[ms..].bytes().take_while(|&b| is_ident(b)).count();
                    let name = &code[ms..name_end];
                    if !(recv == "http" || clientish(recv)) {
                        continue;
                    }
                    let Some(close) = matching(code, open) else { continue };
                    let args = split_args(code, open + 1, close);
                    let (method, url) = match name {
                        "NewRequest" => (args.first().map(|&(s, e)| go_method(src.slice(s, e))), args.get(1)),
                        "NewRequestWithContext" => (args.get(1).map(|&(s, e)| go_method(src.slice(s, e))), args.get(2)),
                        m => (Some(m.to_uppercase()), args.first()),
                    };
                    let (Some(method), Some(&(us, ue))) = (method, url) else { continue };
                    let __expr = src.slice(us, ue);
                    let Some(path) = url_path(f.lang, __expr) else { continue };
                    let host = url_host_near(src, __expr);
                    found.push((rs, method, path, host, go_expects(f, rs), Vec::new()));
                }
            }
            Language::Rust => {
                for (rs, ms, open) in method_calls(code, &["get", "post", "put", "patch", "delete"]) {
                    let recv = src.code_slice(rs, ms - 1);
                    if !clientish(recv) {
                        continue;
                    }
                    let Some(close) = matching(code, open) else { continue };
                    let Some(&(us, ue)) = split_args(code, open + 1, close).first() else { continue };
                    let __expr = src.slice(us, ue);
                    let Some(path) = url_path(f.lang, __expr) else { continue };
                    let host = url_host_near(src, __expr);
                    let name_end = ms + code[ms..].bytes().take_while(|&b| is_ident(b)).count();
                    found.push((rs, code[ms..name_end].to_uppercase(), path, host, None, Vec::new()));
                }
            }
            _ => {}
        }
        for (at, method, path, host, expects, expects_fields) in found {
            let line = src.line(at);
            let caller = f.enclosing(line).map(|s| s.name.clone());
            let mut evidence = src.ev(at);
            evidence.symbol_name = None;
            h.clients.push(ClientDraft {
                call: ClientCall {
                    unit: f.unit.to_string(),
                    method,
                    path,
                    target_host: host,
                    target_unit: None,
                    operation: None,
                    caller,
                    drift: vec![],
                    evidence,
                },
                expects,
                expects_fields,
            });
        }
    }
}

fn go_method(expr: &str) -> String {
    let e = expr.trim();
    string_lit(e)
        .unwrap_or_else(|| e.rsplit('.').next().unwrap_or(e).trim_start_matches("Method").to_string())
        .to_uppercase()
}

fn declared_router(src: &Src, recv: &str) -> bool {
    ["Router(", "express(", "Hono(", "Fastify("]
        .iter()
        .any(|c| src.text.contains(&format!("{recv} = {c}")) || src.text.contains(&format!("{recv} = new {c}")))
}

/// `const url = \`${BASE}/x\`` used as `fetch(url)`.
fn resolve_ts_var(src: &Src, expr: &str) -> String {
    let e = expr.trim();
    if e.bytes().all(is_ident) && !e.is_empty() {
        for kw in ["const ", "let "] {
            if let Some(p) = src.text.find(&format!("{kw}{e} = ")) {
                let rest = &src.text[p + kw.len() + e.len() + 3..];
                return rest.split([';', '\n']).next().unwrap_or("").trim().to_string();
            }
        }
    }
    e.to_string()
}

/// The response type the caller expects: `(await res.json()) as T`, `get<T>()` or the function's `Promise<T>`.
fn ts_expects(f: &Loaded, call_end: usize, generic: Option<String>) -> Option<String> {
    if let Some(g) = generic {
        return Some(g);
    }
    let line = f.src.line(call_end);
    let sym = f.enclosing(line)?;
    let (s, e) = f.span(sym);
    let after = f.src.slice(call_end, e);
    if let Some(p) = after.find(".json()") {
        let rest = &after[p + 7..];
        let rest = rest.trim_start().trim_start_matches(')').trim_start();
        if let Some(t) = rest.strip_prefix("as ") {
            let t: String =
                t.chars().take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '[' | ']' | '<' | '>')).collect();
            return Some(t);
        }
    }
    if let Some(p) = after.find(".json<") {
        return Some(after[p + 6..].split('>').next().unwrap_or("").to_string());
    }
    let header = f.src.slice(s, f.src.code[s..e].find('{').map(|x| s + x).unwrap_or(e));
    header.rfind("Promise<").map(|p| {
        let inner = &header[p + 8..];
        inner[..inner.rfind('>').unwrap_or(inner.len())].to_string()
    })
}

/// Best operation for a call: same method (or ANY), matching segments; exact paths beat suffix matches.
pub(crate) fn match_operation<'a>(c: &ClientCall, ops: &'a [Operation]) -> Option<&'a Operation> {
    let segs = |p: &str| p.split('/').filter(|s| !s.is_empty()).map(str::to_string).collect::<Vec<_>>();
    let cs = segs(&c.path);
    let seg_eq = |a: &str, b: &str| a == b || a.starts_with('{') || b.starts_with('{');
    let mut best: Vec<(u32, &Operation)> = Vec::new();
    for op in ops {
        if op.unit == c.unit || !(op.method == c.method || op.method == "ANY") {
            continue;
        }
        let os = segs(&op.path);
        // A literal or a parameter on both sides beats a parameter standing in for a literal.
        let same_kind = |a: &String, b: &String| a == b || a.starts_with('{') && b.starts_with('{');
        let score = if os.len() == cs.len() && os.iter().zip(&cs).all(|(a, b)| seg_eq(a, b)) {
            1000 + os.iter().zip(&cs).filter(|(a, b)| same_kind(a, b)).count() as u32
        } else if cs.len() > os.len()
            && !os.is_empty()
            && op.path_partial
            && cs[cs.len() - os.len()..].iter().zip(&os).all(|(a, b)| seg_eq(a, b))
        {
            // The operation's prefix is unresolved: the caller's base URL holds it.
            500 + os.len() as u32
        } else if os.len() > cs.len()
            && !cs.is_empty()
            && os[os.len() - cs.len()..].iter().zip(&cs).all(|(a, b)| seg_eq(a, b))
        {
            // The caller's base URL includes part of the path.
            100 + cs.len() as u32
        } else {
            continue;
        };
        best.push((score, op));
    }
    best.sort_by(|a, b| b.0.cmp(&a.0));
    match best.as_slice() {
        [] => None,
        [(_, op)] => Some(op),
        [(s1, op), (s2, _), ..] if s1 > s2 => Some(op),
        _ => None,
    }
}

/// The name a statement binds, and any type annotation on it: `resp = client.get(…)`
/// → `("resp", None)`, `account: Account = client.get(…)` → `("account", Some("Account"))`.
fn bound_name(head: &str) -> (Option<String>, Option<String>) {
    let lhs = match head.trim_end().strip_suffix('=') {
        Some(l) if !l.ends_with(['=', '!', '<', '>', '+', '-', '*', '/']) => l.trim(),
        _ => return (None, None),
    };
    let (name, ann) = match lhs.split_once(':') {
        Some((n, t)) => (n.trim(), Some(t.trim().to_string())),
        None => (lhs, None),
    };
    if name.is_empty() || !name.bytes().all(is_ident) {
        return (None, None);
    }
    (Some(name.to_string()), ann)
}

/// A model the body is parsed into, written immediately before it:
/// `Account(**…)`, `Account.model_validate(…)`, `Account.parse_obj(…)`.
fn wrapping_model(before: &str) -> Option<String> {
    let b = before.trim_end();
    let b = b.strip_suffix("**").unwrap_or(b).trim_end();
    let b = b.strip_suffix('(')?;
    let b = ["model_validate", "model_validate_json", "parse_obj", "parse_raw", "validate_python", "from_dict"]
        .iter()
        .find_map(|m| b.strip_suffix(m).and_then(|x| x.strip_suffix('.')))
        .unwrap_or(b);
    let name: String = b.bytes().rev().take_while(|&c| is_ident(c)).map(|c| c as char).collect();
    let name: String = name.chars().rev().collect();
    name.chars().next().is_some_and(|c| c.is_uppercase()).then_some(name)
}

/// A key read straight off a body expression: `["total"]` or `.get("total")`.
fn read_key(after: &str, keys: &mut Vec<String>) {
    let a = after.trim_start();
    let inner = match (a.strip_prefix('['), a.strip_prefix(".get(")) {
        (Some(r), _) => r.split(']').next().unwrap_or(""),
        (_, Some(r)) => r.split(&[',', ')'][..]).next().unwrap_or(""),
        _ => return,
    };
    if let Some(k) = string_lit(inner.trim()) {
        if !k.is_empty() && !keys.contains(&k) {
            keys.push(k);
        }
    }
}

/// What a Python caller expects back: the model it validates the body into, and the
/// keys it reads out of `response.json()` — both only within the calling function.
fn py_expects(f: &Loaded, at: usize, call_end: usize) -> (Option<String>, Vec<String>) {
    let src = &f.src;
    let line = src.line(at);
    let Some(sym) = f.enclosing(line) else { return (None, Vec::new()) };
    let (_, end) = f.span(sym);
    if end <= call_end || at < src.line_start(line) {
        return (None, Vec::new());
    }
    let after = src.slice(call_end, end);
    let head = src.slice(src.line_start(line), at);
    let (name, ann) = bound_name(head);
    let mut model = None;
    let mut keys = Vec::new();
    // `bodies` are expressions that already hold the parsed body.
    let mut bodies: Vec<String> = Vec::new();
    match after.trim_start().strip_prefix(".json()") {
        // `data = client.get(…).json()` — the call is parsed in place.
        Some(rest) => {
            read_key(rest, &mut keys);
            model = ann.or_else(|| wrapping_model(head));
            bodies.extend(name);
        }
        // `resp = client.get(…)` — look for `resp.json()` further down the function.
        None => {
            let Some(var) = name else { return (None, keys) };
            let pat = format!("{var}.json()");
            let mut from = 0;
            while let Some(p) = after[from..].find(&pat) {
                let abs = from + p;
                read_key(&after[abs + pat.len()..], &mut keys);
                let stmt = after[..abs].rsplit('\n').next().unwrap_or("");
                if model.is_none() {
                    let (bound, bound_ann) = bound_name(stmt);
                    model = bound_ann.or_else(|| wrapping_model(stmt));
                    bodies.extend(bound.filter(|_| model.is_none()));
                }
                from = abs + pat.len();
            }
        }
    }
    for b in bodies {
        let mut from = 0;
        while let Some(p) = after[from..].find(&b) {
            let abs = from + p;
            let before_ok = abs == 0 || !is_ident(after.as_bytes()[abs - 1]);
            if before_ok {
                read_key(&after[abs + b.len()..], &mut keys);
            }
            from = abs + b.len();
        }
    }
    (model.map(|m| m.trim().to_string()).filter(|m| !m.is_empty()), keys)
}

/// What a Go caller expects back: the named struct it decodes the body into
/// (`json.Unmarshal(body, &out)`, `json.NewDecoder(resp.Body).Decode(&out)`).
fn go_expects(f: &Loaded, at: usize) -> Option<String> {
    let src = &f.src;
    let sym = f.enclosing(src.line(at))?;
    let (s, e) = f.span(sym);
    if e <= at || s > at {
        return None;
    }
    let body = src.slice(s, e);
    let after = src.slice(at, e);
    let var = ["json.Unmarshal(", ".Decode("]
        .iter()
        .find_map(|n| {
            let p = after.find(n)?;
            let args = after[p + n.len()..].split(')').next()?;
            let last = args.rsplit(',').next()?.trim();
            last.strip_prefix('&').map(|v| v.trim().to_string())
        })
        .filter(|v| v.bytes().all(is_ident) && !v.is_empty())?;
    // Its declaration: `var out Foo`, `out := Foo{}`, `out := []Foo{}`.
    let decl = [format!("var {var} "), format!("{var} := ")].iter().find_map(|pat| {
        let p = body.find(pat.as_str())?;
        let before_ok = p == 0 || !is_ident(body.as_bytes()[p - 1]);
        before_ok.then(|| body[p + pat.len()..].lines().next().unwrap_or("").trim().to_string())
    })?;
    let ty = decl.split(['{', '(', ' ', '\t', ';']).next().unwrap_or("").trim();
    let ty = ty.trim_start_matches(&['[', ']', '*', '&'][..]);
    let ty = ty.rsplit('.').next().unwrap_or(ty);
    (!ty.is_empty() && ty.chars().next().is_some_and(|c| c.is_uppercase())).then(|| ty.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_normalise_to_paths() {
        assert_eq!(
            url_path(Language::TypeScript, "`${API_URL}/orders/${order.id}/items?expand=1`").as_deref(),
            Some("/orders/{id}/items")
        );
        assert_eq!(url_path(Language::TypeScript, "\"http://payments:8080/charges\"").as_deref(), Some("/charges"));
        assert_eq!(url_path(Language::TypeScript, "BASE + \"/users/\" + id").as_deref(), Some("/users/{id}"));
        assert_eq!(
            url_path(Language::Python, "f\"{settings.BASE}/api/v1/items/{item_id}\"").as_deref(),
            Some("/api/v1/items/{item_id}")
        );
        assert_eq!(
            url_path(Language::Go, "fmt.Sprintf(\"%s/charges/%s\", c.baseURL, id)").as_deref(),
            Some("/charges/{id}")
        );
        assert_eq!(url_path(Language::Go, "c.baseURL + \"/charges\"").as_deref(), Some("/charges"));
        assert_eq!(
            url_path(Language::Rust, "&format!(\"{}/reports/{day}\", self.base)").as_deref(),
            Some("/reports/{day}")
        );
        assert_eq!(url_path(Language::TypeScript, "`${a}${b}`"), None);
    }

    fn op(unit: &str, method: &str, path: &str, partial: bool) -> Operation {
        let ev = EvidenceRef { file_path: "x".into(), start_line: 1, end_line: 1, symbol_name: None, note: None };
        let mut o = new_op(unit, "t", method, path.into(), SymbolRef { name: "h".into(), evidence: ev.clone() }, ev);
        o.path_partial = partial;
        o
    }

    #[test]
    fn calls_match_exact_then_suffix() {
        let ops = vec![
            op("api", "GET", "/api/v1/items/{id}", false),
            op("api", "GET", "/api/v1/items/me", false),
            op("svc", "POST", "/charges", true),
        ];
        let call = |m: &str, p: &str| ClientCall {
            unit: "web".into(),
            method: m.into(),
            path: p.into(),
            target_host: None,
            target_unit: None,
            operation: None,
            caller: None,
            drift: vec![],
            evidence: ops[0].evidence.clone(),
        };
        assert_eq!(
            match_operation(&call("GET", "/api/v1/items/me"), &ops).map(|o| o.path.as_str()),
            Some("/api/v1/items/me")
        );
        assert_eq!(
            match_operation(&call("GET", "/api/v1/items/{x}"), &ops).map(|o| o.path.as_str()),
            Some("/api/v1/items/{id}"),
            "a templated segment matches the templated route, not the literal `me`"
        );
        assert!(match_operation(&call("GET", "/items/{id}"), &ops).is_none(), "two suffix candidates tie");
        assert_eq!(match_operation(&call("POST", "/payments/charges"), &ops).map(|o| o.unit.as_str()), Some("svc"));
        assert!(match_operation(&call("DELETE", "/charges"), &ops).is_none());
    }
}
