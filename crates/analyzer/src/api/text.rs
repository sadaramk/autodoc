//! Offset-preserving source scanning shared by the framework extractors:
//! comments and string contents are blanked in a `code` view so structural
//! searches never match inside them, while literals are read from the original.

use crate::lang::Language;
use crate::scan::EvidenceRef;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Style {
    CLike,
    Rust,
    Python,
}

pub fn style_for(lang: Language) -> Style {
    match lang {
        Language::Python => Style::Python,
        Language::Rust => Style::Rust,
        _ => Style::CLike,
    }
}

pub struct Src {
    pub path: String,
    pub text: String,
    /// Comments and string contents replaced by spaces (quotes kept), and every
    /// other non-ASCII byte by `_`: pure ASCII, so any byte offset is a valid
    /// slice boundary and offsets line up with `text` byte for byte.
    pub code: String,
    line_starts: Vec<usize>,
}

impl Src {
    pub fn new(path: &str, text: String, style: Style) -> Src {
        let code = blank(&text, style);
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Src { path: path.to_string(), text, code, line_starts }
    }

    /// 1-based line of a byte offset.
    pub fn line(&self, offset: usize) -> u32 {
        match self.line_starts.binary_search(&offset) {
            Ok(i) => i as u32 + 1,
            Err(i) => i as u32,
        }
    }

    pub fn line_start(&self, line: u32) -> usize {
        self.line_starts.get(line.saturating_sub(1) as usize).copied().unwrap_or(self.text.len())
    }

    pub fn line_end(&self, line: u32) -> usize {
        self.line_starts.get(line as usize).map(|s| s.saturating_sub(1)).unwrap_or(self.text.len())
    }

    pub fn ev(&self, offset: usize) -> EvidenceRef {
        let l = self.line(offset);
        EvidenceRef { file_path: self.path.clone(), start_line: l, end_line: l, symbol_name: None, note: None }
    }

    pub fn ev_range(&self, start: usize, end: usize, symbol: Option<&str>) -> EvidenceRef {
        EvidenceRef {
            file_path: self.path.clone(),
            start_line: self.line(start),
            end_line: self.line(end.max(start)),
            symbol_name: symbol.map(str::to_string),
            note: None,
        }
    }

    /// Original text of a range.
    /// Offsets are widened to the nearest character boundaries.
    pub fn slice(&self, start: usize, end: usize) -> &str {
        let end = ceil_boundary(&self.text, end.min(self.text.len()));
        let start = floor_boundary(&self.text, start.min(end));
        &self.text[start..end]
    }

    /// Code view of a range.
    pub fn code_slice(&self, start: usize, end: usize) -> &str {
        let end = end.min(self.code.len());
        &self.code[start.min(end)..end]
    }
}

fn blank(text: &str, style: Style) -> String {
    let b = text.as_bytes();
    let mut out: Vec<u8> = b.to_vec();
    let n = b.len();
    let mut i = 0;
    let fill = |out: &mut Vec<u8>, from: usize, to: usize| {
        for c in out.iter_mut().take(to.min(n)).skip(from) {
            if *c != b'\n' {
                *c = b' ';
            }
        }
    };
    while i < n {
        let c = b[i];
        match style {
            Style::Python if c == b'#' => {
                let end = text[i..].find('\n').map(|x| i + x).unwrap_or(n);
                fill(&mut out, i, end);
                i = end;
                continue;
            }
            Style::CLike | Style::Rust if c == b'/' && i + 1 < n && b[i + 1] == b'/' => {
                let end = text[i..].find('\n').map(|x| i + x).unwrap_or(n);
                fill(&mut out, i, end);
                i = end;
                continue;
            }
            Style::CLike | Style::Rust if c == b'/' && i + 1 < n && b[i + 1] == b'*' => {
                let end = text[i + 2..].find("*/").map(|x| i + 2 + x + 2).unwrap_or(n);
                fill(&mut out, i, end);
                i = end;
                continue;
            }
            _ => {}
        }
        // Strings.
        if style == Style::Python && (c == b'"' || c == b'\'') {
            let triple = i + 2 < n && b[i + 1] == c && b[i + 2] == c;
            if triple {
                let q = &text[i..i + 3];
                let end = text[i + 3..].find(q).map(|x| i + 3 + x).unwrap_or(n);
                fill(&mut out, i + 3, end);
                i = (end + 3).min(n);
            } else {
                let end = string_end(b, i, c);
                fill(&mut out, i + 1, end);
                i = (end + 1).min(n);
            }
            continue;
        }
        if style == Style::Rust && c == b'r' && i + 1 < n && (b[i + 1] == b'"' || b[i + 1] == b'#') {
            let mut j = i + 1;
            let mut hashes = 0;
            while j < n && b[j] == b'#' {
                hashes += 1;
                j += 1;
            }
            if j < n && b[j] == b'"' && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_')) {
                let closing = format!("\"{}", "#".repeat(hashes));
                let end = text[j + 1..].find(&closing).map(|x| j + 1 + x).unwrap_or(n);
                fill(&mut out, j + 1, end);
                i = (end + closing.len()).min(n);
                continue;
            }
        }
        if style == Style::Rust && c == b'\'' {
            // Char literal ('x', '\n') vs lifetime ('a).
            let is_char = (i + 2 < n && b[i + 2] == b'\'') || (i + 1 < n && b[i + 1] == b'\\');
            if is_char {
                let end = string_end(b, i, b'\'');
                fill(&mut out, i + 1, end);
                i = (end + 1).min(n);
            } else {
                i += 1;
            }
            continue;
        }
        // Java text block: `"""` then a line break, closed by the next `"""`.
        if style == Style::CLike && c == b'"' && text[i..].starts_with("\"\"\"") {
            let rest = &text[i + 3..];
            let line_end = rest.find('\n').unwrap_or(rest.len());
            if rest[..line_end].trim().is_empty() && line_end < rest.len() {
                let end = rest.find("\"\"\"").map(|x| i + 3 + x).unwrap_or(n);
                fill(&mut out, i + 3, end);
                i = (end + 3).min(n);
                continue;
            }
        }
        if c == b'"' || (style == Style::CLike && (c == b'\'' || c == b'`')) {
            let end = string_end(b, i, c);
            fill(&mut out, i + 1, end);
            i = (end + 1).min(n);
            continue;
        }
        i += 1;
    }
    for c in out.iter_mut() {
        if *c >= 0x80 {
            *c = b'_';
        }
    }
    String::from_utf8(out).expect("ASCII")
}

fn string_end(b: &[u8], start: usize, q: u8) -> usize {
    let mut j = start + 1;
    while j < b.len() {
        if b[j] == b'\\' {
            j += 2;
            continue;
        }
        if b[j] == q {
            return j;
        }
        if b[j] == b'\n' && q != b'`' {
            return j;
        }
        j += 1;
    }
    b.len()
}

/// Index of the bracket closing the one at `open` (in the code view).
pub fn matching(code: &str, open: usize) -> Option<usize> {
    let b = code.as_bytes();
    let (o, c) = match b.get(open)? {
        b'(' => (b'(', b')'),
        b'[' => (b'[', b']'),
        b'{' => (b'{', b'}'),
        b'<' => (b'<', b'>'),
        _ => return None,
    };
    let mut depth = 0i32;
    for (i, &ch) in b.iter().enumerate().skip(open) {
        if ch == o {
            depth += 1;
        } else if ch == c {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// Top-level comma-separated pieces of `code[start..end]` as absolute (start, end) ranges, trimmed.
pub fn split_args(code: &str, start: usize, end: usize) -> Vec<(usize, usize)> {
    let b = code.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut piece = start;
    for (i, &ch) in b.iter().enumerate().take(end).skip(start) {
        match ch {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                out.push(trim(code, piece, i));
                piece = i + 1;
            }
            _ => {}
        }
    }
    let last = trim(code, piece, end);
    if last.1 > last.0 {
        out.push(last);
    }
    out.into_iter().filter(|(s, e)| e > s).collect()
}

pub fn trim(code: &str, mut s: usize, mut e: usize) -> (usize, usize) {
    let b = code.as_bytes();
    while s < e && b[s].is_ascii_whitespace() {
        s += 1;
    }
    while e > s && b[e - 1].is_ascii_whitespace() {
        e -= 1;
    }
    (s, e)
}

/// Offsets where `word` occurs as a whole identifier in the code view.
pub fn find_word(code: &str, word: &str) -> Vec<usize> {
    let b = code.as_bytes();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = code[from..].find(word) {
        let at = from + i;
        let before_ok = at == 0 || !is_ident(b[at - 1]);
        let after = at + word.len();
        let after_ok = after >= b.len() || !is_ident(b[after]);
        if before_ok && after_ok {
            out.push(at);
        }
        from = at + word.len().max(1);
    }
    out
}

pub fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
}

/// `.method(` call sites: returns (receiver chain start, method start, open paren) for each.
pub fn method_calls(code: &str, methods: &[&str]) -> Vec<(usize, usize, usize)> {
    let b = code.as_bytes();
    let mut out = Vec::new();
    for m in methods {
        for at in find_word(code, m) {
            if at == 0 || b[at - 1] != b'.' {
                continue;
            }
            let mut p = at + m.len();
            // Generic arguments between name and '(' (TS `get<T>(`, Rust turbofish).
            while p < b.len() && b[p].is_ascii_whitespace() {
                p += 1;
            }
            if p < b.len() && b[p] == b'<' {
                match matching(code, p) {
                    Some(close) => p = close + 1,
                    None => continue,
                }
            }
            if p >= b.len() || b[p] != b'(' {
                continue;
            }
            let recv_end = at - 1;
            let recv_start = receiver_start(code, recv_end);
            out.push((recv_start, at, p));
        }
    }
    out.sort();
    out
}

/// Start of the receiver expression ending at `end` (exclusive), e.g. `this.app` or `r`.
pub fn receiver_start(code: &str, end: usize) -> usize {
    let b = code.as_bytes();
    let mut i = end;
    while i > 0 {
        let c = b[i - 1];
        if is_ident(c) || c == b'.' || c == b':' {
            i -= 1;
        } else if c == b')' {
            // Chained call: skip back over the balanced argument list.
            let mut depth = 0;
            let mut j = i;
            while j > 0 {
                j -= 1;
                match b[j] {
                    b')' => depth += 1,
                    b'(' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            i = j;
        } else {
            break;
        }
    }
    i
}

/// Literal value of a string argument (`"x"`, `'x'`, `` `x` `` without
/// interpolation, Python `r"x"`), from the original text.
pub fn string_lit(text: &str) -> Option<String> {
    let t = text.trim();
    let t = t.trim_start_matches(['r', 'b', 'u']);
    for q in ["\"", "'", "`"] {
        if t.len() >= 2 && t.starts_with(q) && t.ends_with(q) {
            let inner = &t[1..t.len() - 1];
            if q == "`" && inner.contains("${") {
                return None;
            }
            return Some(inner.to_string());
        }
    }
    None
}

/// Normalises route syntax (`:id`, `<int:id>`, `{id:[0-9]+}`, `*`) to `{id}` and joins prefixes.
pub fn join_path(prefix: &str, path: &str) -> String {
    let mut out = String::new();
    for part in [prefix, path] {
        for seg in part.split('/').filter(|s| !s.is_empty()) {
            out.push('/');
            out.push_str(&normalize_segment(seg));
        }
    }
    if out.is_empty() {
        "/".into()
    } else {
        out
    }
}

pub fn normalize_segment(seg: &str) -> String {
    if let Some(name) = seg.strip_prefix(':') {
        return format!("{{{}}}", name.trim_end_matches('?'));
    }
    if seg.starts_with('<') && seg.ends_with('>') {
        let inner = &seg[1..seg.len() - 1];
        let name = inner.rsplit(':').next().unwrap_or(inner);
        return format!("{{{name}}}");
    }
    if seg.starts_with('{') && seg.ends_with('}') {
        let inner = &seg[1..seg.len() - 1];
        let name = inner.split(':').next().unwrap_or(inner).trim_start_matches('*');
        return format!("{{{name}}}");
    }
    if let Some(rest) = seg.strip_prefix('*') {
        return format!("{{{}}}", if rest.is_empty() { "path" } else { rest });
    }
    seg.to_string()
}

pub fn path_params(path: &str) -> Vec<String> {
    path.split('/').filter_map(|s| s.strip_prefix('{').and_then(|x| x.strip_suffix('}'))).map(str::to_string).collect()
}

/// First sentence of a doc comment block ending right above `line`.
pub fn doc_above(src: &Src, line: u32, style: Style) -> Option<String> {
    if style == Style::Python || line <= 1 {
        return None;
    }
    let text_of = |l: u32| src.slice(src.line_start(l), src.line_end(l)).trim();
    let mut i = line - 1;
    // Skip decorators / attributes stacked above.
    while i >= 1 && (text_of(i).starts_with('@') || text_of(i).starts_with("#[")) {
        i -= 1;
    }
    if i == 0 {
        return None;
    }
    let mut collected: Vec<String> = Vec::new();
    if text_of(i).ends_with("*/") {
        let mut j = i;
        while j > 1 && !text_of(j).contains("/*") {
            j -= 1;
        }
        for l in j..=i {
            let t = text_of(l)
                .trim_start_matches("/**")
                .trim_start_matches("/*")
                .trim_end_matches("*/")
                .trim_start_matches('*')
                .trim();
            if !t.is_empty() {
                collected.push(t.to_string());
            }
        }
    } else {
        let mut j = i;
        while j >= 1 && text_of(j).starts_with("//") {
            j -= 1;
        }
        for l in j + 1..=i {
            let t = text_of(l).trim_start_matches('/').trim_start_matches('!').trim();
            if !t.is_empty() {
                collected.push(t.to_string());
            }
        }
    }
    first_sentence(&collected.join(" "))
}

pub fn first_sentence(s: &str) -> Option<String> {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let s = s.as_str();
    if s.is_empty() {
        return None;
    }
    let end = s.find(". ").map(|i| i + 1).unwrap_or(s.len());
    Some(s[..end].trim().to_string())
}

pub fn pascal(s: &str) -> String {
    s.split(['_', '-', ' '])
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect()
}

pub fn singular(s: &str) -> String {
    if let Some(x) = s.strip_suffix("ies") {
        format!("{x}y")
    } else if s.ends_with("ss") {
        s.to_string()
    } else if let Some(x) = s.strip_suffix('s') {
        x.to_string()
    } else {
        s.to_string()
    }
}

/// HTTP status for a named constant (`http.StatusNotFound`, `status.HTTP_404_NOT_FOUND`,
/// `StatusCode::NOT_FOUND`, `HttpStatus.NOT_FOUND`) or a literal number.
pub fn status_code(expr: &str) -> Option<u16> {
    let e = expr.trim();
    if let Ok(n) = e.parse::<u16>() {
        return (100..600).contains(&n).then_some(n);
    }
    let digits: String = e.chars().filter(|c| c.is_ascii_digit()).collect();
    if e.contains("HTTP_") && digits.len() == 3 {
        return digits.parse().ok();
    }
    let name = e.rsplit(['.', ':']).next().unwrap_or(e).trim_start_matches("Status").to_lowercase().replace('_', "");
    Some(match name.as_str() {
        "ok" => 200,
        "created" => 201,
        "accepted" => 202,
        "nocontent" => 204,
        "movedpermanently" => 301,
        "found" => 302,
        "notmodified" => 304,
        "badrequest" => 400,
        "unauthorized" => 401,
        "paymentrequired" => 402,
        "forbidden" => 403,
        "notfound" => 404,
        "methodnotallowed" => 405,
        "conflict" => 409,
        "gone" => 410,
        "unprocessableentity" => 422,
        "toomanyrequests" => 429,
        "internalservererror" => 500,
        "notimplemented" => 501,
        "badgateway" => 502,
        "serviceunavailable" => 503,
        "gatewaytimeout" => 504,
        _ => return None,
    })
}

/// Entries of an object/dict/struct literal whose opening brace is at `open`:
/// `(key, key_offset, value_start, value_end)`; shorthand entries have an empty value range.
pub fn object_entries(src: &Src, open: usize) -> Vec<(String, usize, usize, usize)> {
    let Some(close) = matching(&src.code, open) else { return vec![] };
    let mut out = Vec::new();
    for (s, e) in split_args(&src.code, open + 1, close) {
        let piece = src.code_slice(s, e);
        if piece.starts_with("...") {
            continue;
        }
        // Key: identifier or quoted string, followed by ':' (or '=>' / '=' in other languages).
        let (key, key_end) = if piece.starts_with(['"', '\'', '`']) {
            let q = piece.as_bytes()[0] as char;
            let Some(end) = piece[1..].find(q) else { continue };
            (src.slice(s + 1, s + 1 + end).to_string(), s + end + 2)
        } else {
            let len = piece.bytes().take_while(|&b| is_ident(b)).count();
            if len == 0 {
                continue;
            }
            (piece[..len].to_string(), s + len)
        };
        let rest = src.code_slice(key_end, e);
        let trimmed = rest.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('?') && trimmed[1..].trim().is_empty() {
            out.push((key, s, e, e));
            continue;
        }
        let Some(after) = trimmed.strip_prefix(':') else { continue };
        let vs = e - after.len();
        let (vs, ve) = trim(&src.code, vs, e);
        out.push((key, s, vs, ve));
    }
    out
}

/// Skips whitespace forward from `i` in the code view.
pub fn skip_ws(code: &str, mut i: usize) -> usize {
    let b = code.as_bytes();
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// Identifier starting at `i` (after whitespace).
pub fn ident_at(code: &str, i: usize) -> Option<(usize, usize)> {
    let s = skip_ws(code, i);
    let b = code.as_bytes();
    let mut e = s;
    while e < b.len() && is_ident(b[e]) {
        e += 1;
    }
    (e > s).then_some((s, e))
}

/// Identifier ending right before `i` (skipping whitespace backwards).
pub fn ident_before(code: &str, i: usize) -> Option<(usize, usize)> {
    let b = code.as_bytes();
    let mut e = i;
    while e > 0 && b[e - 1].is_ascii_whitespace() {
        e -= 1;
    }
    let mut s = e;
    while s > 0 && is_ident(b[s - 1]) {
        s -= 1;
    }
    (e > s).then_some((s, e))
}

pub fn camel(s: &str) -> String {
    let p = pascal(s);
    let mut c = p.chars();
    c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// Human-readable rule statements shared by the validators of every language.
pub fn min_len(n: &str) -> String {
    format!("at least {n} character{}", if n == "1" { "" } else { "s" })
}
pub fn max_len(n: &str) -> String {
    format!("at most {n} character{}", if n == "1" { "" } else { "s" })
}
pub fn min_items(n: &str) -> String {
    format!("at least {n} item{}", if n == "1" { "" } else { "s" })
}
pub fn max_items(n: &str) -> String {
    format!("at most {n} item{}", if n == "1" { "" } else { "s" })
}

/// Top-level split of a plain string on any of `seps` (brackets of all kinds nest).
pub fn split_top<'a>(s: &'a str, seps: &[char]) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    let mut prev = ' ';
    for (i, c) in s.char_indices() {
        match c {
            '<' | '{' | '[' | '(' => depth += 1,
            '>' if prev != '=' => depth -= 1,
            '}' | ']' | ')' => depth -= 1,
            c if depth == 0 && seps.contains(&c) => {
                out.push(s[start..i].trim());
                start = i + c.len_utf8();
            }
            _ => {}
        }
        prev = c;
    }
    out.push(s[start..].trim());
    out.into_iter().filter(|p| !p.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blanking_keeps_offsets_and_quotes() {
        let src = Src::new("a.ts", "app.get(\"/x\", h); // app.post(\"/y\")\nconst s = `a${b}c`;".into(), Style::CLike);
        assert_eq!(src.text.len(), src.code.len());
        assert!(src.code.starts_with("app.get(\"  \", h);"));
        assert!(!src.code.contains("post"));
        let py = Src::new("a.py", "x = \"\"\"doc # not comment\"\"\"  # comment\ny = 'q'".into(), Style::Python);
        assert!(!py.code.contains("comment"));
        let rs = Src::new("a.rs", "fn f<'a>(x: &'a str) -> char { 'x' } // c".into(), Style::Rust);
        assert!(rs.code.contains("<'a>"));
        assert!(!rs.code.contains("'x'"));
    }

    #[test]
    fn java_text_blocks_are_blanked() {
        let src = Src::new(
            "A.java",
            "@Op(notes = \"\"\"\n  a \"quoted\" (paren\n  \"\"\" + X)\nvoid f() {}".into(),
            Style::CLike,
        );
        assert!(!src.code.contains("paren"));
        assert!(src.code.contains("+ X)\nvoid f() {}"));
    }

    #[test]
    fn paths_join_and_normalise() {
        assert_eq!(join_path("/api/v1", "/items/{id}"), "/api/v1/items/{id}");
        assert_eq!(join_path("/users/", ":id"), "/users/{id}");
        assert_eq!(join_path("", "/files/<int:file_id>"), "/files/{file_id}");
        assert_eq!(join_path("/", "/"), "/");
        assert_eq!(join_path("/r", "{id:[0-9]+}"), "/r/{id}");
    }

    #[test]
    fn statuses() {
        assert_eq!(status_code("http.StatusPaymentRequired"), Some(402));
        assert_eq!(status_code("status.HTTP_404_NOT_FOUND"), Some(404));
        assert_eq!(status_code("StatusCode::NOT_FOUND"), Some(404));
        assert_eq!(status_code("201"), Some(201));
    }
}

pub fn floor_boundary(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

pub fn ceil_boundary(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

#[cfg(test)]
mod boundary_tests {
    use super::*;

    #[test]
    fn non_ascii_source_never_splits_characters() {
        let text = "import { μ } from './units';\n// ångström 🚀\nconst s = \"日本\";\n".to_string();
        let src = Src::new("a.ts", text.clone(), Style::CLike);
        assert!(src.code.is_ascii());
        assert_eq!(src.code.len(), text.len());
        for i in 0..=text.len() {
            for j in i..=text.len() {
                let _ = src.slice(i, j);
                let _ = src.code_slice(i, j);
            }
        }
    }
}
