//! Java: Spring MVC / WebFlux (annotated controllers and functional routes), Jakarta REST
//! (JAX-RS — Quarkus, Dropwizard, Jersey, Helidon MP, Open Liberty) and Micronaut controllers;
//! Bean Validation / Jackson models; Feign, MicroProfile Rest Client and Micronaut declarative
//! clients, `RestTemplate`, `WebClient` and `RestClient` calls.
//!
//! Declarations are read from the parsed facts (classes, methods, annotations) and the
//! signatures and bodies from the source text, so a route's full path, parameters, body and
//! response types, status codes, thrown errors and security all carry their own line.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::text::*;
use super::*;
use crate::jvm::ConfigDoc;
use crate::scan::UnitSummary;
use crate::source::SourceIndex;

// ─────────────────────────────── source structure ───────────────────────────────

/// An annotation found in the text: name, argument pieces as absolute ranges, and position.
#[derive(Clone, Debug)]
struct Ann {
    name: String,
    at: usize,
    /// Top-level argument pieces (`value = "/x"`, `"/y"`) as (start, end) in the source.
    args: Vec<(usize, usize)>,
}

#[derive(Clone, Debug)]
struct JParam {
    anns: Vec<Ann>,
    type_name: String,
    name: String,
    at: usize,
}

#[derive(Clone, Debug)]
struct Sig {
    anns: Vec<Ann>,
    /// `None` for constructors.
    return_type: Option<String>,
    name: String,
    name_at: usize,
    params: Vec<JParam>,
    /// Body braces, when the method has one.
    body: Option<(usize, usize)>,
    start_line: u32,
    end_line: u32,
    doc: Option<String>,
}

#[derive(Clone, Debug)]
struct JClass {
    fi: usize,
    unit: String,
    package: String,
    name: String,
    /// `class`, `interface`, `enum`, `record`, `annotation`.
    kind: &'static str,
    start_line: u32,
    end_line: u32,
    doc: Option<String>,
}

/// Parsed on demand: class annotations, supertypes, body and members.
#[derive(Clone, Debug, Default)]
struct ClassDetail {
    anns: Vec<Ann>,
    extends: Option<String>,
    implements: Vec<String>,
    /// `record R(int a, String b)` components.
    components: Vec<JParam>,
    body: Option<(usize, usize)>,
    methods: Vec<Sig>,
    fields: Vec<JParam>,
    enum_constants: Vec<String>,
}

const MODIFIERS: &[&str] = &[
    "public",
    "private",
    "protected",
    "static",
    "final",
    "abstract",
    "synchronized",
    "native",
    "default",
    "strictfp",
    "transient",
    "volatile",
    "sealed",
    "non-sealed",
];

/// `@Name` or `@Name(args)` starting at `at` (the `@`).
fn parse_ann(src: &Src, at: usize) -> Option<(Ann, usize)> {
    let code = &src.code;
    let b = code.as_bytes();
    let mut e = at + 1;
    while e < b.len() && (is_ident(b[e]) || b[e] == b'.') {
        e += 1;
    }
    let full = &code[at + 1..e];
    if full.is_empty() || full == "interface" {
        return None;
    }
    let name = full.rsplit('.').next().unwrap_or(full).to_string();
    let p = skip_ws(code, e);
    if b.get(p) == Some(&b'(') {
        let close = matching(code, p)?;
        let args = split_args(code, p + 1, close);
        return Some((Ann { name, at, args }, close + 1));
    }
    Some((Ann { name, at, args: vec![] }, e))
}

/// Leading annotations and modifiers from `i`; returns them and the offset after.
fn prefix(src: &Src, mut i: usize, end: usize) -> (Vec<Ann>, usize) {
    let code = &src.code;
    let mut anns = Vec::new();
    loop {
        i = skip_ws(code, i);
        if i >= end {
            break;
        }
        if code.as_bytes()[i] == b'@' {
            match parse_ann(src, i) {
                Some((a, next)) => {
                    anns.push(a);
                    i = next;
                    continue;
                }
                None => break,
            }
        }
        match ident_at(code, i) {
            Some((s, e)) if MODIFIERS.contains(&&code[s..e]) => {
                i = e;
                // `non-sealed`
                if code[e..].starts_with("-sealed") {
                    i = e + 7;
                }
            }
            _ => break,
        }
    }
    (anns, i)
}

impl Ann {
    /// Argument value range by key (`""` = the unnamed `value`, which `value` also matches).
    fn arg(&self, src: &Src, keys: &[&str]) -> Option<(usize, usize)> {
        for &(s, e) in &self.args {
            let piece = src.code_slice(s, e);
            let (key, vs) = match assignment(piece) {
                Some(k) => (k.0, s + k.1),
                None => ("", s),
            };
            let key_ok = keys.contains(&key)
                || (key.is_empty() && keys.contains(&"value"))
                || (key == "value" && keys.contains(&""));
            if key_ok {
                let (vs, ve) = trim(&src.code, vs, e);
                return Some((vs, ve));
            }
        }
        None
    }

    fn has_arg(&self, src: &Src, key: &str) -> bool {
        self.args.iter().any(|&(s, e)| assignment(src.code_slice(s, e)).is_some_and(|(k, _)| k == key))
    }

    /// String values (single literal, `{"a", "b"}` array, `"a" + "b"`, or a constant) of an argument.
    fn strings(&self, src: &Src, keys: &[&str], consts: &HashMap<String, String>) -> Vec<String> {
        self.arg(src, keys).map(|(s, e)| string_values(src, s, e, consts)).unwrap_or_default()
    }

    /// Trailing identifiers of an argument (`RequestMethod.POST` → `POST`, arrays too).
    fn idents(&self, src: &Src, keys: &[&str]) -> Vec<String> {
        let Some((s, e)) = self.arg(src, keys) else { return vec![] };
        let v = src.code_slice(s, e).trim().trim_start_matches('{').trim_end_matches('}');
        split_top(v, &[','])
            .into_iter()
            .map(|p| p.rsplit('.').next().unwrap_or(p).trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    }

    fn raw(&self, src: &Src, keys: &[&str]) -> Option<String> {
        self.arg(src, keys).map(|(s, e)| src.slice(s, e).trim().to_string())
    }
}

/// `key = value` at top level of an annotation argument piece: (key, offset of value).
fn assignment(piece: &str) -> Option<(&str, usize)> {
    let b = piece.as_bytes();
    let mut i = 0;
    while i < b.len() && (is_ident(b[i]) || b[i].is_ascii_whitespace()) {
        i += 1;
    }
    if i < b.len() && b[i] == b'=' && b.get(i + 1) != Some(&b'=') {
        let key = piece[..i].trim();
        if !key.is_empty() && key.bytes().all(is_ident) {
            return Some((key, i + 1));
        }
    }
    None
}

fn string_values(src: &Src, s: usize, e: usize, consts: &HashMap<String, String>) -> Vec<String> {
    let code = &src.code;
    let (mut s, mut e) = trim(code, s, e);
    if code.as_bytes().get(s) == Some(&b'{') && e > s && code.as_bytes()[e - 1] == b'}' {
        s += 1;
        e -= 1;
    }
    split_args(code, s, e).into_iter().filter_map(|(ps, pe)| concat_value(src, ps, pe, consts)).collect()
}

/// `"a" + Consts.B + "c"` → the concatenated string, when every part is known.
fn concat_value(src: &Src, s: usize, e: usize, consts: &HashMap<String, String>) -> Option<String> {
    let mut out = String::new();
    for part in split_top(src.slice(s, e), &['+']) {
        let p = part.trim();
        if let Some(l) = string_lit(p) {
            out.push_str(&l);
        } else {
            let key = p.rsplit('.').next().unwrap_or(p);
            out.push_str(consts.get(key)?);
        }
    }
    Some(out)
}

/// Parses a method or constructor declaration starting at `start` (its first annotation / modifier).
fn parse_sig(src: &Src, start: usize, limit: usize) -> Option<Sig> {
    let code = &src.code;
    let (anns, mut i) = prefix(src, start, limit);
    i = skip_ws(code, i);
    // Type parameters `<T>`.
    if code.as_bytes().get(i) == Some(&b'<') {
        i = matching(code, i)? + 1;
    }
    let open = {
        let b = code.as_bytes();
        let mut j = i;
        let mut depth = 0i32;
        loop {
            if j >= limit {
                return None;
            }
            match b[j] {
                b'<' => depth += 1,
                b'>' => depth -= 1,
                b'(' if depth <= 0 => break j,
                b'{' | b';' | b'=' => return None,
                _ => {}
            }
            j += 1;
        }
    };
    let (ns, ne) = ident_before(code, open)?;
    let name = code[ns..ne].to_string();
    let ret = src.slice(i, ns).trim();
    let return_type = (!ret.is_empty()).then(|| ret.split_whitespace().collect::<Vec<_>>().join(" "));
    let close = matching(code, open)?;
    let params = split_args(code, open + 1, close).into_iter().filter_map(|(s, e)| parse_param(src, s, e)).collect();
    // `throws …` then body or `;`.
    let b = code.as_bytes();
    let mut j = close + 1;
    while j < limit && b[j] != b'{' && b[j] != b';' {
        j += 1;
    }
    let body = (j < limit && b[j] == b'{').then(|| matching(code, j).map(|c| (j, c))).flatten();
    let end = body.map(|(_, c)| c).unwrap_or(j.min(code.len().saturating_sub(1)));
    Some(Sig {
        anns,
        return_type,
        name,
        name_at: ns,
        params,
        body,
        start_line: src.line(start),
        end_line: src.line(end),
        doc: None,
    })
}

fn parse_param(src: &Src, s: usize, e: usize) -> Option<JParam> {
    let (anns, i) = prefix(src, s, e);
    let (ns, ne) = ident_before(&src.code, e)?;
    if ns < i {
        return None;
    }
    let type_name = src.slice(i, ns).split_whitespace().collect::<Vec<_>>().join(" ");
    if type_name.is_empty() {
        return None;
    }
    Some(JParam { anns, type_name, name: src.code_slice(ns, ne).to_string(), at: ns })
}

// ─────────────────────────────── index ───────────────────────────────

/// Method name → (file, body range) of the unit's methods that throw.
type ThrowingMethods = HashMap<String, Vec<(usize, (usize, usize))>>;

struct Index<'a> {
    files: &'a [Loaded<'a>],
    units: &'a [UnitSummary],
    classes: Vec<JClass>,
    by_name: HashMap<String, Vec<usize>>,
    details: std::cell::RefCell<HashMap<usize, std::rc::Rc<ClassDetail>>>,
    /// `static final String NAME = "…"` constants per unit.
    consts: HashMap<String, HashMap<String, String>>,
    config: Vec<ConfigDoc>,
    /// Application names per unit (`spring.application.name`, …).
    app_names: HashMap<String, Vec<String>>,
    /// (unit, exception simple name) → (status, evidence) from `@ExceptionHandler` advice and JAX-RS
    /// `ExceptionMapper`s (both apply within their application); `("", name)` for `@ResponseStatus`
    /// on the exception class itself, which travels with the class.
    exception_status: HashMap<(String, String), (u16, EvidenceRef)>,
    /// Per unit: method name → bodies of methods that contain `throw`.
    throwing: std::cell::RefCell<HashMap<String, std::rc::Rc<ThrowingMethods>>>,
}

const JAVA_TYPES: &[&str] = &[
    "String",
    "Integer",
    "Long",
    "Short",
    "Byte",
    "Double",
    "Float",
    "Boolean",
    "Character",
    "Object",
    "BigDecimal",
    "BigInteger",
    "UUID",
    "Instant",
    "LocalDate",
    "LocalDateTime",
    "LocalTime",
    "OffsetDateTime",
    "ZonedDateTime",
    "Duration",
    "Date",
    "Map",
    "HashMap",
    "List",
    "Set",
    "Collection",
    "Optional",
    "Void",
    "JsonNode",
    "ObjectNode",
    "byte",
    "int",
    "long",
    "short",
    "double",
    "float",
    "boolean",
    "char",
    "void",
    "URI",
    "URL",
    "Locale",
    "Currency",
];

impl<'a> Index<'a> {
    fn build(index: &SourceIndex<'a>, files: &'a [Loaded<'a>]) -> Index<'a> {
        let mut classes = Vec::new();
        let mut consts: HashMap<String, HashMap<String, String>> = HashMap::new();
        for (fi, f) in files.iter().enumerate().filter(|(_, f)| f.lang == Language::Java) {
            let package = f.facts.package.clone().unwrap_or_default();
            for s in &f.facts.symbols {
                let kind = match s.kind {
                    SymbolKind::Class if is_record(&f.src, s) => "record",
                    SymbolKind::Class => "class",
                    SymbolKind::Interface => "interface",
                    SymbolKind::Enum => "enum",
                    _ => continue,
                };
                classes.push(JClass {
                    fi,
                    unit: f.unit.to_string(),
                    package: package.clone(),
                    name: s.name.clone(),
                    kind,
                    start_line: s.start_line,
                    end_line: s.end_line,
                    doc: s.doc.as_deref().and_then(first_sentence),
                });
            }
            if f.src.text.contains("static final String") {
                let map = consts.entry(f.unit.to_string()).or_default();
                for at in find_word(&f.src.code, "String") {
                    let before = f.src.code_slice(at.saturating_sub(14), at);
                    if !before.contains("final") {
                        continue;
                    }
                    let Some((ns, ne)) = ident_at(&f.src.code, at + 6) else { continue };
                    let eq = skip_ws(&f.src.code, ne);
                    if f.src.code.as_bytes().get(eq) != Some(&b'=') {
                        continue;
                    }
                    let semi = f.src.code[eq..].find(';').map(|x| eq + x).unwrap_or(eq);
                    if let Some(v) = concat_value(&f.src, eq + 1, semi, map) {
                        map.entry(f.src.code_slice(ns, ne).to_string()).or_insert(v);
                    }
                }
            }
        }
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, c) in classes.iter().enumerate() {
            by_name.entry(c.name.clone()).or_default().push(i);
        }
        let config = if files.iter().any(|f| f.lang == Language::Java) {
            crate::jvm::config_documents(&index.root)
        } else {
            vec![]
        };
        // Application names: the scan's aliases, plus anything the unit's own config declares.
        let mut app_names: HashMap<String, Vec<String>> =
            index.units.iter().filter(|u| !u.aliases.is_empty()).map(|u| (u.id.clone(), u.aliases.clone())).collect();
        for u in index.units {
            let prefix = if u.root.is_empty() { String::new() } else { format!("{}/", u.root) };
            for d in config
                .iter()
                .filter(|d| d.path.starts_with(&prefix) && matches!(d.stem.as_str(), "application" | "bootstrap"))
            {
                for key in ["spring.application.name", "micronaut.application.name", "quarkus.application.name"] {
                    if let Some((v, _)) = d.get(key) {
                        let names = app_names.entry(u.id.clone()).or_default();
                        if !v.contains("${") && !names.iter().any(|n| n == v) {
                            names.push(v.to_string());
                        }
                    }
                }
            }
        }
        let mut idx = Index {
            files,
            units: index.units,
            classes,
            by_name,
            details: Default::default(),
            consts,
            config,
            app_names,
            exception_status: HashMap::new(),
            throwing: Default::default(),
        };
        idx.exception_status = idx.exception_statuses();
        idx
    }

    fn src(&self, ci: usize) -> &Src {
        &self.files[self.classes[ci].fi].src
    }

    fn consts(&self, unit: &str) -> &HashMap<String, String> {
        static EMPTY: std::sync::OnceLock<HashMap<String, String>> = std::sync::OnceLock::new();
        self.consts.get(unit).unwrap_or_else(|| EMPTY.get_or_init(HashMap::new))
    }

    /// Class detail parsed once.
    fn detail(&self, ci: usize) -> std::rc::Rc<ClassDetail> {
        if let Some(d) = self.details.borrow().get(&ci) {
            return d.clone();
        }
        let d = std::rc::Rc::new(self.parse_class(ci));
        self.details.borrow_mut().insert(ci, d.clone());
        d
    }

    fn parse_class(&self, ci: usize) -> ClassDetail {
        let c = &self.classes[ci];
        let f = &self.files[c.fi];
        let src = &f.src;
        let code = &src.code;
        let start = src.line_start(c.start_line);
        let limit = src.line_end(c.end_line).min(code.len());
        let mut d = ClassDetail::default();
        let (anns, i) = prefix(src, start, limit);
        d.anns = anns;
        let Some((ks, ke)) = ident_at(code, i) else { return d };
        if !matches!(&code[ks..ke], "class" | "interface" | "enum" | "record") {
            return d;
        }
        let Some((_, name_end)) = ident_at(code, ke) else { return d };
        let b = code.as_bytes();
        let mut j = skip_ws(code, name_end);
        if b.get(j) == Some(&b'<') {
            j = matching(code, j).map(|x| x + 1).unwrap_or(j);
        }
        j = skip_ws(code, j);
        if c.kind == "record" && b.get(j) == Some(&b'(') {
            if let Some(close) = matching(code, j) {
                d.components =
                    split_args(code, j + 1, close).into_iter().filter_map(|(s, e)| parse_param(src, s, e)).collect();
                j = close + 1;
            }
        }
        let Some(open) = code[j..limit].find('{').map(|x| j + x) else { return d };
        let header = src.slice(j, open);
        let supertypes = |kw: &str| -> Vec<String> {
            find_word(header, kw)
                .first()
                .map(|&p| {
                    let rest = &header[p + kw.len()..];
                    let rest = ["implements", "extends", "permits"]
                        .iter()
                        .filter(|k| **k != kw)
                        .filter_map(|k| find_word(rest, k).first().copied())
                        .min()
                        .map(|x| &rest[..x])
                        .unwrap_or(rest);
                    split_top(rest, &[','])
                        .into_iter()
                        .map(|t| t.trim().to_string())
                        .filter(|t| !t.is_empty())
                        .collect()
                })
                .unwrap_or_default()
        };
        let ext = supertypes("extends");
        if c.kind == "interface" {
            d.implements = ext;
        } else {
            d.extends = ext.into_iter().next();
            d.implements = supertypes("implements");
        }
        let Some(close) = matching(code, open) else { return d };
        d.body = Some((open, close));

        // Members at the top level of the body.
        let mut k = open + 1;
        if c.kind == "enum" {
            let end = code[k..close].find(';').map(|x| k + x).unwrap_or(close);
            for (s, e) in split_args(code, k, end) {
                let (_, p) = prefix(src, s, e);
                if let Some((cs, ce)) = ident_at(code, p) {
                    d.enum_constants.push(code[cs..ce].to_string());
                }
            }
            k = end + 1;
        }
        while k < close {
            k = skip_ws(code, k);
            if k >= close {
                break;
            }
            let member = k;
            // Scan to the member's end (`;` at depth 0, or a `{…}` block), past its annotations.
            let (_, past_anns) = prefix(src, member, close);
            let mut depth = 0i32;
            let mut j = past_anns.max(k);
            let mut paren_seen = false;
            let mut end = close;
            let mut is_block = false;
            while j < close {
                match b[j] {
                    b'(' => {
                        if depth == 0 {
                            paren_seen = true;
                        }
                        depth += 1
                    }
                    b')' => depth -= 1,
                    b'=' if depth == 0 && !paren_seen => {
                        // Field initialiser: runs to `;` at depth 0 (lambdas / arrays nest).
                        let mut d2 = 0i32;
                        let mut m = j;
                        while m < close {
                            match b[m] {
                                b'(' | b'{' | b'[' => d2 += 1,
                                b')' | b'}' | b']' => d2 -= 1,
                                b';' if d2 == 0 => break,
                                _ => {}
                            }
                            m += 1;
                        }
                        end = m;
                        break;
                    }
                    b';' if depth == 0 => {
                        end = j;
                        break;
                    }
                    b'{' if depth == 0 => {
                        end = matching(code, j).unwrap_or(close);
                        is_block = true;
                        break;
                    }
                    _ => {}
                }
                j += 1;
            }
            let (member_anns, after) = prefix(src, member, end);
            let head = code_word(code, after);
            if matches!(head, "class" | "interface" | "enum" | "record") || head.is_empty() && is_block {
                k = end + 1;
                continue;
            }
            if paren_seen {
                if let Some(mut sig) = parse_sig(src, member, (end + 1).min(code.len())) {
                    sig.doc = f
                        .facts
                        .symbols
                        .iter()
                        .find(|s| {
                            s.kind == SymbolKind::Method && s.name == sig.name && s.start_line == src.line(member)
                        })
                        .and_then(|s| s.doc.as_deref())
                        .and_then(first_sentence);
                    d.methods.push(sig);
                }
            } else if !is_block {
                let static_field = src.code_slice(member, after).split_whitespace().any(|w| w == "static");
                let decl_end = code[after..end].find('=').map(|x| after + x).unwrap_or(end);
                if !static_field {
                    if let Some(mut p) = parse_param(src, after, decl_end) {
                        p.anns = member_anns;
                        d.fields.push(p);
                    }
                }
            }
            k = end + 1;
        }
        d
    }

    /// Resolves a type as written in `fi` to a class: same package, explicit / wildcard
    /// imports, same unit, then a unique name anywhere.
    fn resolve(&self, fi: usize, type_name: &str) -> Option<usize> {
        let (inner, _) = unwrap_java(type_name);
        let simple = inner.rsplit('.').next().unwrap_or(&inner).trim().to_string();
        if JAVA_TYPES.contains(&simple.as_str()) {
            return None;
        }
        let cands = self.by_name.get(&simple)?;
        if cands.len() == 1 {
            return Some(cands[0]);
        }
        let f = &self.files[fi];
        let pkg = f.facts.package.clone().unwrap_or_default();
        if let Some(&c) = cands.iter().find(|&&c| self.classes[c].package == pkg && self.classes[c].unit == f.unit) {
            return Some(c);
        }
        for imp in &f.facts.imports {
            for &c in cands {
                let cl = &self.classes[c];
                if imp.specifier == format!("{}.{}", cl.package, cl.name)
                    || imp.specifier == format!("{}.*", cl.package)
                {
                    return Some(c);
                }
            }
        }
        let same_unit: Vec<usize> = cands.iter().copied().filter(|&c| self.classes[c].unit == f.unit).collect();
        (same_unit.len() == 1).then(|| same_unit[0])
    }

    fn class_ann<'d>(&self, d: &'d ClassDetail, names: &[&str]) -> Option<&'d Ann> {
        d.anns.iter().find(|a| names.contains(&a.name.as_str()))
    }

    /// Exception → status from `@ResponseStatus` on the exception class, `@ExceptionHandler`
    /// methods in `@ControllerAdvice` classes, and JAX-RS `ExceptionMapper<X>` providers.
    fn exception_statuses(&self) -> HashMap<(String, String), (u16, EvidenceRef)> {
        let mut out: HashMap<(String, String), (u16, EvidenceRef)> = HashMap::new();
        for ci in 0..self.classes.len() {
            let c = &self.classes[ci];
            let f = &self.files[c.fi];
            let t = &f.src.text;
            let interesting = (c.name.ends_with("Exception") || c.name.ends_with("Error"))
                && t.contains("ResponseStatus")
                || t.contains("ExceptionHandler")
                || t.contains("ExceptionMapper");
            if !interesting || c.kind == "enum" {
                continue;
            }
            let d = self.detail(ci);
            let src = &f.src;
            if let Some(a) = self.class_ann(&d, &["ResponseStatus"]) {
                if let Some(s) = a.raw(src, &["", "value", "code"]).as_deref().and_then(status_code) {
                    out.entry((String::new(), c.name.clone())).or_insert((s, src.ev(a.at)));
                }
            }
            let advice = self.class_ann(&d, &["ControllerAdvice", "RestControllerAdvice"]).is_some();
            if advice {
                for m in &d.methods {
                    let Some(h) = m.anns.iter().find(|a| a.name == "ExceptionHandler") else { continue };
                    let mut handled: Vec<String> = h
                        .arg(src, &["", "value"])
                        .map(|(s, e)| {
                            src.code_slice(s, e)
                                .trim_matches(['{', '}'])
                                .split(',')
                                .filter_map(|x| x.trim().strip_suffix(".class"))
                                .map(|x| x.rsplit('.').next().unwrap_or(x).to_string())
                                .collect()
                        })
                        .unwrap_or_default();
                    if handled.is_empty() {
                        handled =
                            m.params.iter().map(|p| p.type_name.clone()).filter(|t| t.ends_with("Exception")).collect();
                    }
                    let status = m
                        .anns
                        .iter()
                        .find(|a| a.name == "ResponseStatus")
                        .and_then(|a| a.raw(src, &["", "value", "code"]))
                        .and_then(|r| status_code(&r))
                        .or_else(|| {
                            m.body.and_then(|(s, e)| {
                                body_statuses(src, s, e).into_iter().map(|x| x.0).find(|s| *s >= 400)
                            })
                        });
                    if let Some(s) = status {
                        for ex in handled {
                            out.entry((c.unit.clone(), ex)).or_insert((s, src.ev(h.at)));
                        }
                    }
                }
            }
            if d.implements.iter().any(|i| i.starts_with("ExceptionMapper<")) {
                let ex = d
                    .implements
                    .iter()
                    .find_map(|i| i.strip_prefix("ExceptionMapper<").and_then(|x| x.strip_suffix('>')))
                    .map(|x| x.rsplit('.').next().unwrap_or(x).to_string());
                let status = d
                    .methods
                    .iter()
                    .find(|m| m.name == "toResponse")
                    .and_then(|m| m.body)
                    .and_then(|(s, e)| body_statuses(src, s, e).into_iter().find(|x| x.0 >= 400));
                if let (Some(ex), Some((s, at))) = (ex, status) {
                    out.entry((c.unit.clone(), ex)).or_insert((s, src.ev(at)));
                }
            }
        }
        out
    }

    fn throwing_methods(&self, unit: &str) -> std::rc::Rc<ThrowingMethods> {
        if let Some(m) = self.throwing.borrow().get(unit) {
            return m.clone();
        }
        let mut map: ThrowingMethods = HashMap::new();
        for (ci, c) in self.classes.iter().enumerate() {
            if c.unit != unit || c.kind != "class" || !self.files[c.fi].src.text.contains("throw new") {
                continue;
            }
            let src = &self.files[c.fi].src;
            for m in &self.detail(ci).methods {
                if let Some((bs, be)) = m.body {
                    if src.code_slice(bs, be).contains("throw") {
                        map.entry(m.name.clone()).or_default().push((c.fi, (bs, be)));
                    }
                }
            }
        }
        let rc = std::rc::Rc::new(map);
        self.throwing.borrow_mut().insert(unit.to_string(), rc.clone());
        rc
    }

    /// Status for an exception thrown in `fi`, for an operation of `unit`, following `extends` up to 4 levels.
    fn status_of(&self, unit: &str, fi: usize, name: &str) -> Option<(u16, EvidenceRef)> {
        let mut cur = name.to_string();
        let mut from = fi;
        for _ in 0..5 {
            if let Some(s) = known_exception(&cur) {
                return Some((s, self.files[fi].src.ev(0)));
            }
            if cur.is_empty() {
                return None;
            }
            for key in [(unit.to_string(), cur.clone()), (String::new(), cur.clone())] {
                if let Some(hit) = self.exception_status.get(&key) {
                    return Some(hit.clone());
                }
            }
            let ci = self.resolve(from, &cur)?;
            let d = self.detail(ci);
            cur = d.extends.clone()?;
            cur = cur.split('<').next().unwrap_or(&cur).trim().to_string();
            from = self.classes[ci].fi;
        }
        None
    }

    fn unit_config(&self, unit: &str) -> Vec<&ConfigDoc> {
        let root = self.units.iter().find(|u| u.id == unit).map(|u| u.root.clone()).unwrap_or_default();
        let prefix = if root.is_empty() { String::new() } else { format!("{root}/") };
        let names = self.app_names.get(unit).cloned().unwrap_or_default();
        self.config
            .iter()
            .filter(|d| {
                let own = d.path.starts_with(&prefix)
                    && matches!(d.stem.as_str(), "application" | "bootstrap")
                    && !d.dir.ends_with("/shared");
                let central = (d.dir.ends_with("/shared") || d.dir.contains("config-repo")) && names.contains(&d.stem);
                own || central
            })
            .collect()
    }

    fn config_value(&self, unit: &str, key: &str) -> Option<String> {
        self.unit_config(unit).into_iter().find_map(|d| d.get(key).map(|(v, _)| v.to_string()))
    }

    /// `${key:default}` placeholders resolved from the unit's configuration; `None` = unresolved.
    fn placeholders(&self, unit: &str, s: &str) -> (String, bool) {
        super::text::resolve_placeholders(s, |key| self.config_value(unit, key))
    }

    /// A unit addressed by service id, application name, compose service or host.
    fn unit_named(&self, name: &str) -> Option<String> {
        let n = name.trim().to_lowercase();
        if n.is_empty() {
            return None;
        }
        if let Some((u, _)) = self.app_names.iter().find(|(_, names)| names.iter().any(|x| x.to_lowercase() == n)) {
            return Some(u.clone());
        }
        let variants = |s: &str| {
            let s = s.to_lowercase();
            let mut v = vec![s.clone()];
            for suf in ["-service", "-svc", "-api", "-server", "-app"] {
                if let Some(x) = s.strip_suffix(suf) {
                    v.push(x.to_string());
                }
            }
            v
        };
        let wanted = variants(&n);
        self.units
            .iter()
            .find(|u| {
                wanted.contains(&u.id.to_lowercase())
                    || u.compose_service.as_deref().is_some_and(|c| wanted.contains(&c.to_lowercase()))
                    || variants(&u.id).contains(&n)
            })
            .map(|u| u.id.clone())
    }
}

fn is_record(src: &Src, s: &Symbol) -> bool {
    let start = src.line_start(s.start_line);
    let end = src.line_end(s.end_line).min(src.code.len());
    let (_, i) = prefix(src, start, end);
    code_word(&src.code, i) == "record"
}

fn code_word(code: &str, i: usize) -> &str {
    ident_at(code, i).map(|(s, e)| &code[s..e]).unwrap_or("")
}

/// Status of well-known framework exceptions (JAX-RS, Spring, Micronaut, JPA).
fn known_exception(name: &str) -> Option<u16> {
    Some(match name {
        "BadRequestException"
        | "MethodArgumentNotValidException"
        | "ConstraintViolationException"
        | "HttpMessageNotReadableException" => 400,
        "NotAuthorizedException" | "AuthenticationException" | "UnauthorizedException" => 401,
        "ForbiddenException" | "AccessDeniedException" => 403,
        "NotFoundException" | "EntityNotFoundException" => 404,
        "NotAllowedException" => 405,
        "NotAcceptableException" => 406,
        "NotSupportedException" => 415,
        "ServiceUnavailableException" => 503,
        "InternalServerErrorException" => 500,
        _ => return None,
    })
}

/// Java wrappers → (inner type, collection): `ResponseEntity<List<T>>`, `Mono<T>`, `Flux<T>`,
/// `Page<T>`, `Uni<T>`, `Multi<T>`, `Optional<T>`, `CompletableFuture<T>`, `T[]`.
fn unwrap_java(t: &str) -> (String, bool) {
    const WRAP: &[&str] = &[
        "ResponseEntity",
        "HttpEntity",
        "Mono",
        "Uni",
        "Optional",
        "CompletableFuture",
        "CompletionStage",
        "DeferredResult",
        "Callable",
        "HttpResponse",
        "RestResponse",
        "Single",
        "Maybe",
        "Future",
        "EntityModel",
        "Response",
        "MutableHttpResponse",
    ];
    const COLL: &[&str] = &[
        "List",
        "Set",
        "Collection",
        "Iterable",
        "Page",
        "Slice",
        "Flux",
        "Multi",
        "Stream",
        "PageData",
        "PagedModel",
        "CollectionModel",
        "Publisher",
        "Flowable",
        "Observable",
        "SortedSet",
        "LinkedList",
        "ArrayList",
        "HashSet",
    ];
    let mut s = t.trim().to_string();
    let mut coll = false;
    for _ in 0..6 {
        if let Some(x) = s.strip_suffix("[]") {
            coll = true;
            s = x.trim().to_string();
            continue;
        }
        let Some(o) = s.find('<') else { break };
        if !s.ends_with('>') {
            break;
        }
        let head = s[..o].trim();
        let head = head.rsplit('.').next().unwrap_or(head).to_string();
        let inner = s[o + 1..s.len() - 1].trim();
        let first = split_top(inner, &[',']).first().copied().unwrap_or("").to_string();
        if COLL.contains(&head.as_str()) {
            coll = true;
            s = first;
        } else if WRAP.contains(&head.as_str()) {
            s = first;
        } else {
            break;
        }
    }
    let s = s.trim_start_matches("? extends ").to_string();
    (s, coll)
}

/// Status codes named in a body: `ResponseEntity.status(HttpStatus.X)`, `.created(`, `.noContent()`,
/// `new ResponseEntity<>(…, HttpStatus.X)`, `Response.status(…)`, `HttpResponse.created(…)`.
fn body_statuses(src: &Src, s: usize, e: usize) -> Vec<(u16, usize)> {
    let code = &src.code;
    let mut out = Vec::new();
    let region = &code[s..e];
    for (word, status) in [
        ("created", 201),
        ("accepted", 202),
        ("noContent", 204),
        ("notFound", 404),
        ("badRequest", 400),
        ("unprocessableEntity", 422),
        ("unauthorized", 401),
        ("forbidden", 403),
        ("serverError", 500),
        ("internalServerError", 500),
        ("conflict", 409),
    ] {
        for at in find_word(region, word) {
            let abs = s + at;
            let recv = code[receiver_start(code, abs.saturating_sub(1))..abs].to_string();
            if abs > 0
                && code.as_bytes()[abs - 1] == b'.'
                && ["ResponseEntity.", "Response.", "HttpResponse.", "ServerResponse.", "RestResponse.", "status."]
                    .iter()
                    .any(|r| recv.ends_with(r))
            {
                out.push((status, abs));
            }
        }
    }
    for at in find_word(region, "status") {
        let abs = s + at;
        let p = skip_ws(code, abs + 6);
        if code.as_bytes().get(p) != Some(&b'(') || abs == 0 || code.as_bytes()[abs - 1] != b'.' {
            continue;
        }
        let Some(close) = matching(code, p) else { continue };
        if let Some(st) = split_args(code, p + 1, close).first().and_then(|&(a, b)| status_code(src.slice(a, b))) {
            out.push((st, abs));
        }
    }
    for at in find_word(region, "ResponseEntity") {
        let abs = s + at;
        if !code[..abs].trim_end().ends_with("new") {
            continue;
        }
        let Some(p) = code[abs..e].find('(').map(|x| abs + x) else { continue };
        let Some(close) = matching(code, p) else { continue };
        if let Some(st) = split_args(code, p + 1, close).last().and_then(|&(a, b)| status_code(src.slice(a, b))) {
            out.push((st, abs));
        }
    }
    out.sort_by_key(|x| x.1);
    out
}

// ─────────────────────────────── rules ───────────────────────────────

fn bean_rules(
    src: &Src,
    anns: &[Ann],
    type_name: &str,
    is_enum: Option<&[String]>,
    fallback: &EvidenceRef,
) -> (Vec<Rule>, bool) {
    let mut rules = Vec::new();
    let mut required = false;
    let (_, coll) = unwrap_java(type_name);
    let sized_items = coll || type_name.starts_with("Map") || type_name.ends_with("[]");
    let num =
        |a: &Ann, keys: &[&str]| a.raw(src, keys).map(|v| v.trim_matches('"').trim_end_matches(['L', 'l']).to_string());
    for a in anns {
        let ev = src.ev(a.at);
        match a.name.as_str() {
            "NotNull" | "NonNull" | "Nonnull" => required = true,
            "NotBlank" => {
                required = true;
                rules.push(rule("must not be blank", "required", &ev));
            }
            "NotEmpty" => {
                required = true;
                rules.push(rule("must not be empty", "required", &ev));
            }
            "Size" | "Length" => {
                let min = num(a, &["min"]);
                let max = num(a, &["max"]);
                if let Some(m) = min.filter(|m| m != "0") {
                    rules.push(rule(if sized_items { min_items(&m) } else { min_len(&m) }, "length", &ev));
                }
                if let Some(m) = max {
                    rules.push(rule(if sized_items { max_items(&m) } else { max_len(&m) }, "length", &ev));
                }
            }
            "Min" => {
                if let Some(v) = num(a, &["", "value"]) {
                    rules.push(rule(format!("must be ≥ {v}"), "range", &ev));
                }
            }
            "Max" => {
                if let Some(v) = num(a, &["", "value"]) {
                    rules.push(rule(format!("must be ≤ {v}"), "range", &ev));
                }
            }
            "DecimalMin" | "DecimalMax" => {
                if let Some(v) = num(a, &["", "value"]) {
                    let exclusive = a.raw(src, &["inclusive"]).is_some_and(|x| x == "false");
                    let op = match (a.name == "DecimalMin", exclusive) {
                        (true, false) => "≥",
                        (true, true) => ">",
                        (false, false) => "≤",
                        (false, true) => "<",
                    };
                    rules.push(rule(format!("must be {op} {v}"), "range", &ev));
                }
            }
            "Positive" => rules.push(rule("must be greater than 0", "range", &ev)),
            "PositiveOrZero" => rules.push(rule("must be ≥ 0", "range", &ev)),
            "Negative" => rules.push(rule("must be less than 0", "range", &ev)),
            "NegativeOrZero" => rules.push(rule("must be ≤ 0", "range", &ev)),
            "Range" => {
                let min = num(a, &["min"]).unwrap_or_else(|| "0".into());
                match num(a, &["max"]) {
                    Some(max) => rules.push(rule(format!("must be between {min} and {max}"), "range", &ev)),
                    None => rules.push(rule(format!("must be ≥ {min}"), "range", &ev)),
                }
            }
            "Pattern" => {
                if let Some((s, e)) = a.arg(src, &["regexp"]) {
                    if let Some(r) = string_lit(src.slice(s, e)) {
                        rules.push(rule(format!("must match {}", r.replace("\\\\", "\\")), "pattern", &ev));
                    }
                }
            }
            "Email" => rules.push(rule("must be a valid email", "format", &ev)),
            "URL" => rules.push(rule("must be a valid URL", "format", &ev)),
            "UUID" => rules.push(rule("must be a UUID", "format", &ev)),
            "Past" => rules.push(rule("must be in the past", "format", &ev)),
            "PastOrPresent" => rules.push(rule("must not be in the future", "format", &ev)),
            "Future" => rules.push(rule("must be in the future", "format", &ev)),
            "FutureOrPresent" => rules.push(rule("must not be in the past", "format", &ev)),
            "Digits" => {
                if let (Some(i), Some(f)) = (num(a, &["integer"]), num(a, &["fraction"])) {
                    rules.push(rule(format!("at most {i} integer and {f} fraction digits"), "range", &ev));
                }
            }
            "AssertTrue" => rules.push(rule("must be true", "custom", &ev)),
            "AssertFalse" => rules.push(rule("must be false", "custom", &ev)),
            _ => {}
        }
    }
    if let Some(vals) = is_enum.filter(|v| !v.is_empty()) {
        rules.push(rule(format!("one of: {}", vals.join(", ")), "enum", fallback));
    }
    (rules, required)
}

fn is_primitive(t: &str) -> bool {
    matches!(t, "int" | "long" | "short" | "byte" | "double" | "float" | "boolean" | "char")
}

// ─────────────────────────────── extraction ───────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Flavor {
    Spring,
    JaxRs,
    Micronaut,
}

struct JavaOp {
    draft: Draft,
    /// Path below the servlet / application context (what security matchers see).
    rel_path: String,
    auth_declared: bool,
}

struct Models<'i, 'a> {
    idx: &'i Index<'a>,
    built: BTreeSet<usize>,
    out: Vec<Model>,
}

impl Models<'_, '_> {
    /// Model id for a Java type used in `fi`, building its model (and nested models) once.
    fn model_for(&mut self, fi: usize, type_name: &str, depth: usize) -> Option<String> {
        let ci = self.idx.resolve(fi, type_name)?;
        let cl = self.idx.classes[ci].clone();
        if !matches!(cl.kind, "class" | "record") || depth > 6 {
            return None;
        }
        let id = format!("{}:{}", cl.unit, cl.name);
        if !self.built.insert(ci) {
            return Some(id);
        }
        let src = self.idx.src(ci);
        let fields = self.fields_of(ci, depth, 0);
        let evidence = EvidenceRef {
            file_path: src.path.clone(),
            start_line: cl.start_line,
            end_line: cl.end_line,
            symbol_name: Some(cl.name.clone()),
            note: None,
            repo: None,
        };
        self.out.push(Model {
            id: id.clone(),
            unit: cl.unit.clone(),
            name: cl.name.clone(),
            fields,
            doc: cl.doc.clone(),
            evidence,
        });
        Some(id)
    }

    fn fields_of(&mut self, ci: usize, depth: usize, inherit: usize) -> Vec<Field> {
        let idx = self.idx;
        let cl = idx.classes[ci].clone();
        let d = idx.detail(ci);
        let src = idx.src(ci);
        let mut fields: Vec<Field> = Vec::new();
        if let (Some(parent), true) = (d.extends.as_deref(), inherit < 4) {
            if let Some(pi) = idx.resolve(cl.fi, parent) {
                if matches!(idx.classes[pi].kind, "class") {
                    fields.extend(self.fields_of(pi, depth, inherit + 1));
                }
            }
        }
        let snake = d.anns.iter().any(|a| {
            a.name == "JsonNaming" && src.slice(a.at, a.args.last().map(|x| x.1).unwrap_or(a.at)).contains("Snake")
        });
        let mut members: Vec<JParam> = if cl.kind == "record" { d.components.clone() } else { d.fields.clone() };
        if members.is_empty() && cl.kind == "class" {
            // Getter-only POJOs.
            members = d
                .methods
                .iter()
                .filter(|m| m.params.is_empty() && m.return_type.as_deref().is_some_and(|r| r != "void"))
                .filter_map(|m| {
                    let prop = m
                        .name
                        .strip_prefix("get")
                        .or_else(|| m.name.strip_prefix("is"))
                        .filter(|p| p.chars().next().is_some_and(|c| c.is_ascii_uppercase()))?;
                    Some(JParam {
                        anns: m.anns.clone(),
                        type_name: m.return_type.clone()?,
                        name: camel(prop),
                        at: m.name_at,
                    })
                })
                .collect();
        }
        for m in members {
            if m.anns.iter().any(|a| a.name == "JsonIgnore") {
                continue;
            }
            let wire = m
                .anns
                .iter()
                .find(|a| matches!(a.name.as_str(), "JsonProperty" | "SerializedName" | "JsonbProperty"))
                .and_then(|a| a.strings(src, &["", "value"], idx.consts(&cl.unit)).into_iter().next())
                .filter(|w| !w.is_empty());
            let json_required = m
                .anns
                .iter()
                .any(|a| a.name == "JsonProperty" && a.raw(src, &["required"]).is_some_and(|r| r == "true"));
            let name = match (&wire, snake) {
                (Some(w), _) => w.clone(),
                (None, true) => to_snake(&m.name),
                (None, false) => m.name.clone(),
            };
            let ev = src.ev(m.at);
            let enum_vals = idx
                .resolve(cl.fi, &m.type_name)
                .filter(|&e| idx.classes[e].kind == "enum")
                .map(|e| idx.detail(e).enum_constants.clone());
            let (rules, required) = bean_rules(src, &m.anns, &m.type_name, enum_vals.as_deref(), &ev);
            let nested = if enum_vals.is_none() { self.model_for(cl.fi, &m.type_name, depth + 1) } else { None };
            let doc = doc_above(src, src.line(m.anns.first().map(|a| a.at).unwrap_or(m.at)), Style::CLike);
            fields.retain(|f| f.name != name);
            fields.push(Field {
                name: name.clone(),
                code_name: (name != m.name).then(|| m.name.clone()),
                type_name: m.type_name.clone(),
                required: required || json_required || is_primitive(&m.type_name),
                rules,
                doc,
                model: nested,
                evidence: ev,
            });
        }
        fields
    }
}

fn to_snake(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

const SPRING_VERBS: &[(&str, &str)] = &[
    ("GetMapping", "GET"),
    ("PostMapping", "POST"),
    ("PutMapping", "PUT"),
    ("DeleteMapping", "DELETE"),
    ("PatchMapping", "PATCH"),
];
const JAXRS_VERBS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];
const MICRONAUT_VERBS: &[(&str, &str)] = &[
    ("Get", "GET"),
    ("Post", "POST"),
    ("Put", "PUT"),
    ("Delete", "DELETE"),
    ("Patch", "PATCH"),
    ("Head", "HEAD"),
    ("Options", "OPTIONS"),
];

/// Parameter types the framework injects (never part of the contract).
fn injected(t: &str) -> bool {
    let base = t.split('<').next().unwrap_or(t).rsplit('.').next().unwrap_or(t);
    matches!(
        base,
        "HttpServletRequest"
            | "HttpServletResponse"
            | "ServletRequest"
            | "ServletResponse"
            | "HttpSession"
            | "Principal"
            | "Authentication"
            | "Model"
            | "ModelMap"
            | "BindingResult"
            | "Errors"
            | "ServerWebExchange"
            | "ServerHttpRequest"
            | "ServerHttpResponse"
            | "WebRequest"
            | "NativeWebRequest"
            | "Locale"
            | "TimeZone"
            | "ZoneId"
            | "UriComponentsBuilder"
            | "SessionStatus"
            | "RedirectAttributes"
            | "HttpHeaders"
            | "HttpEntity"
            | "RequestEntity"
            | "UriInfo"
            | "SecurityContext"
            | "AsyncResponse"
            | "Request"
            | "ContainerRequestContext"
            | "HttpRequest"
            | "RoutingContext"
            | "SseEventSink"
            | "Sse"
            | "Pageable"
            | "Sort"
            | "InputStream"
            | "OutputStream"
            | "Writer"
            | "Reader"
            | "Jwt"
            | "OAuth2Authentication"
            | "SecurityUser"
            | "DeferredResult"
    )
}

fn simple_type(t: &str) -> bool {
    let (inner, _) = unwrap_java(t);
    let base = inner.rsplit('.').next().unwrap_or(&inner);
    JAVA_TYPES.contains(&base) && !matches!(base, "Map" | "HashMap" | "JsonNode" | "ObjectNode" | "Object")
}

pub(crate) fn extract(index: &SourceIndex, files: &[Loaded], h: &mut Harvest) {
    if !files.iter().any(|f| f.lang == Language::Java) {
        return;
    }
    let idx = Index::build(index, files);
    let mut models = Models { idx: &idx, built: BTreeSet::new(), out: Vec::new() };
    let mut ops: Vec<JavaOp> = Vec::new();

    let mut by_unit: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (ci, c) in idx.classes.iter().enumerate() {
        by_unit.entry(c.unit.clone()).or_default().push(ci);
    }
    for (unit, classes) in &by_unit {
        let start = ops.len();
        for &ci in classes {
            let cl = &idx.classes[ci];
            let f = &files[cl.fi];
            let t = &f.src.text;
            if !(t.contains("Mapping")
                || t.contains("@Path")
                || t.contains("@Controller")
                || t.contains("@GET")
                || t.contains("@Get"))
            {
                continue;
            }
            controller_ops(&idx, &mut models, ci, &mut ops, &mut h.excluded);
            client_calls(&idx, &mut models, ci, h);
        }
        functional_routes(&idx, &mut models, unit, classes, &mut ops);
        apply_security(&idx, unit, classes, &mut ops[start..]);
    }
    for (fi, f) in files.iter().enumerate().filter(|(_, f)| f.lang == Language::Java) {
        template_calls(&idx, &mut models, fi, f, h);
    }
    for o in ops {
        h.ops.push(o.draft);
    }
    h.models.extend(models.out);
}

fn controller_ops(
    idx: &Index,
    models: &mut Models,
    ci: usize,
    ops: &mut Vec<JavaOp>,
    excluded: &mut Vec<super::Excluded>,
) {
    let cl = idx.classes[ci].clone();
    if cl.kind == "enum" || cl.kind == "record" {
        return;
    }
    let f = &idx.files[cl.fi];
    let src = &f.src;
    let d = idx.detail(ci);
    let micronaut_file = f.facts.imports.iter().any(|i| i.specifier.starts_with("io.micronaut"));
    let flavor = if d.anns.iter().any(|a| a.name == "Controller") && micronaut_file {
        Flavor::Micronaut
    } else if d.anns.iter().any(|a| matches!(a.name.as_str(), "RestController" | "Controller")) {
        Flavor::Spring
    } else if d.anns.iter().any(|a| a.name == "Path") && cl.kind != "interface" {
        Flavor::JaxRs
    } else {
        return;
    };
    if d.anns.iter().any(|a| matches!(a.name.as_str(), "FeignClient" | "RegisterRestClient" | "Client")) {
        return;
    }
    let consts = idx.consts(&cl.unit);
    let unit = cl.unit.as_str();

    // Mapped methods, with interface mappings inherited (API-first controllers).
    let mut methods: Vec<(Sig, usize)> = d.methods.iter().cloned().map(|m| (m, ci)).collect();
    let mut class_anns = d.anns.clone();
    let mut ann_src_ci = ci;
    for iface in &d.implements {
        let Some(ii) = idx.resolve(cl.fi, iface) else { continue };
        if idx.classes[ii].kind != "interface" {
            continue;
        }
        let id = idx.detail(ii);
        if class_anns.iter().all(|a| a.name != "RequestMapping" && a.name != "Path") {
            if let Some(a) = id.anns.iter().find(|a| matches!(a.name.as_str(), "RequestMapping" | "Path")) {
                class_anns.push(a.clone());
                ann_src_ci = ii;
            }
        }
        for m in &id.methods {
            let mapped = m.anns.iter().any(|a| is_mapping(a, flavor));
            if !mapped {
                continue;
            }
            match methods.iter_mut().find(|(x, _)| x.name == m.name && x.params.len() == m.params.len()) {
                Some((own, _)) if !own.anns.iter().any(|a| is_mapping(a, flavor)) => {
                    let mut merged = m.clone();
                    merged.body = own.body;
                    merged.start_line = own.start_line;
                    merged.end_line = own.end_line;
                    merged.name_at = own.name_at;
                    // Keep the interface's annotations for mapping/params; statuses come from the body.
                    *own = merged;
                    // Remember that annotations live in the interface file.
                    own.doc = own.doc.clone().or(m.doc.clone());
                }
                Some(_) => {}
                None => methods.push((m.clone(), ii)),
            }
        }
    }
    let spring_rest = flavor != Flavor::Spring
        || d.anns.iter().any(|a| matches!(a.name.as_str(), "RestController" | "ResponseBody"))
        || methods.iter().any(|(m, _)| m.anns.iter().any(|a| a.name == "ResponseBody"));
    if !methods.iter().any(|(m, _)| m.anns.iter().any(|a| is_mapping(a, flavor))) {
        return;
    }

    let ann_src = idx.src(ann_src_ci);
    let class_paths: Vec<String> = match flavor {
        Flavor::Spring => class_anns
            .iter()
            .find(|a| a.name == "RequestMapping")
            .map(|a| a.strings(ann_src, &["", "value", "path"], consts))
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| vec![String::new()]),
        Flavor::JaxRs => vec![class_anns
            .iter()
            .find(|a| a.name == "Path")
            .and_then(|a| a.strings(ann_src, &["", "value"], consts).into_iter().next())
            .unwrap_or_default()],
        Flavor::Micronaut => vec![class_anns
            .iter()
            .find(|a| a.name == "Controller")
            .and_then(|a| a.strings(ann_src, &["", "value"], consts).into_iter().next())
            .unwrap_or_default()],
    };
    let context = match flavor {
        Flavor::Spring => [
            "server.servlet.context-path",
            "spring.mvc.servlet.path",
            "spring.webflux.base-path",
            "server.context-path",
        ]
        .iter()
        .filter_map(|k| idx.config_value(unit, k))
        .fold(String::new(), |acc, v| join_path(&acc, &v)),
        Flavor::JaxRs => {
            let mut p = idx.config_value(unit, "quarkus.http.root-path").unwrap_or_default();
            if let Some(r) = ["quarkus.rest.path", "quarkus.resteasy.path", "quarkus.resteasy-reactive.path"]
                .iter()
                .find_map(|k| idx.config_value(unit, k))
            {
                p = join_path(&p, &r);
            }
            // `@ApplicationPath` on the unit's Application subclass.
            if let Some(app) = idx.classes.iter().enumerate().filter(|(_, c)| c.unit == cl.unit).find_map(|(i, c)| {
                idx.files[c.fi].src.text.contains("@ApplicationPath").then(|| idx.detail(i)).and_then(|d| {
                    d.anns
                        .iter()
                        .find(|a| a.name == "ApplicationPath")
                        .and_then(|a| a.strings(idx.src(i), &["", "value"], consts).into_iter().next())
                })
            }) {
                p = join_path(&p, &app);
            }
            p
        }
        Flavor::Micronaut => idx.config_value(unit, "micronaut.server.context-path").unwrap_or_default(),
    };
    let framework = match flavor {
        Flavor::Spring => {
            if f.facts.imports.iter().any(|i| i.specifier.contains("reactor.core"))
                || src.text.contains("Mono<")
                || src.text.contains("Flux<")
            {
                "spring-webflux"
            } else {
                "spring"
            }
        }
        Flavor::JaxRs => {
            if idx.config.iter().any(|c| c.entries.iter().any(|(k, _, _)| k.starts_with("quarkus.")))
                || f.facts
                    .imports
                    .iter()
                    .any(|i| i.specifier.starts_with("io.quarkus") || i.specifier.starts_with("org.jboss.resteasy"))
            {
                "quarkus"
            } else {
                "jax-rs"
            }
        }
        Flavor::Micronaut => "micronaut",
    };
    let class_auth = auth_from(ann_src, &class_anns, consts);

    for (m, owner) in &methods {
        let msrc = idx.src(*owner);
        let body_src = src; // bodies always live in the controller class
        let verbs: Vec<(String, Vec<String>, &Ann)> = m
            .anns
            .iter()
            .filter_map(|a| match flavor {
                Flavor::Spring => {
                    if let Some((_, v)) = SPRING_VERBS.iter().find(|(n, _)| *n == a.name) {
                        let paths = a.strings(msrc, &["", "value", "path"], consts);
                        return Some((v.to_string(), paths, a));
                    }
                    (a.name == "RequestMapping").then(|| {
                        let paths = a.strings(msrc, &["", "value", "path"], consts);
                        let ms = a.idents(msrc, &["method"]);
                        (ms.first().cloned().unwrap_or_else(|| "ANY".into()), paths, a)
                    })
                }
                Flavor::JaxRs => JAXRS_VERBS.contains(&a.name.as_str()).then(|| {
                    let p = m
                        .anns
                        .iter()
                        .find(|x| x.name == "Path")
                        .and_then(|x| x.strings(msrc, &["", "value"], consts).into_iter().next());
                    (a.name.clone(), p.into_iter().collect(), a)
                }),
                Flavor::Micronaut => MICRONAUT_VERBS.iter().find(|(n, _)| *n == a.name).map(|(_, v)| {
                    let p = a.strings(msrc, &["", "value", "uri"], consts);
                    (v.to_string(), p, a)
                }),
            })
            .collect();
        if verbs.is_empty() {
            continue;
        }
        if flavor == Flavor::Spring
            && !spring_rest
            && !m.anns.iter().any(|a| a.name == "ResponseBody")
            && !m.return_type.as_deref().is_some_and(|r| r.starts_with("ResponseEntity"))
        {
            // A `@Controller` returning a view name renders a page; it is not
            // part of an HTTP API. Correct to leave out, wrong to leave unsaid:
            // Spring PetClinic has seventeen mappings and two REST operations.
            let (verb, paths, _) = &verbs[0];
            let path = join_path(
                &join_path(&context, class_paths.first().map(String::as_str).unwrap_or_default()),
                paths.first().map(String::as_str).unwrap_or_default(),
            );
            excluded.push(super::Excluded {
                operation: format!("{unit}:{} {path}", verb.to_uppercase()),
                reason: "server-rendered view, not an API operation".into(),
                evidence: EvidenceRef {
                    file_path: body_src.path.clone(),
                    start_line: m.start_line,
                    end_line: m.end_line,
                    symbol_name: Some(m.name.clone()),
                    note: None,
                    repo: None,
                },
            });
            continue;
        }
        // Extra methods listed in `@RequestMapping(method = {GET, POST})`.
        let mut expanded: Vec<(String, String, &Ann)> = Vec::new();
        for (verb, paths, a) in &verbs {
            let all_methods: Vec<String> = if a.name == "RequestMapping" {
                let ms = a.idents(msrc, &["method"]);
                if ms.is_empty() {
                    vec![verb.clone()]
                } else {
                    ms
                }
            } else {
                vec![verb.clone()]
            };
            let paths = if paths.is_empty() { vec![String::new()] } else { paths.clone() };
            for mv in &all_methods {
                for p in &paths {
                    expanded.push((mv.to_uppercase(), p.clone(), a));
                }
            }
        }
        for (verb, sub, ann) in expanded {
            for cp in &class_paths {
                let raw_rel = join_path(cp, &sub);
                let (rel, partial) = idx.placeholders(unit, &raw_rel);
                let (ctx, ctx_partial) = idx.placeholders(unit, &context);
                let rel = join_path("", &rel);
                let path = join_path(&ctx, &rel);
                let handler_ev = EvidenceRef {
                    file_path: body_src.path.clone(),
                    start_line: m.start_line,
                    end_line: m.end_line,
                    symbol_name: Some(m.name.clone()),
                    note: None,
                    repo: None,
                };
                let mut op = new_op(
                    unit,
                    framework,
                    &verb,
                    path,
                    SymbolRef { name: m.name.clone(), evidence: handler_ev },
                    msrc.ev(ann.at),
                );
                op.path_partial = partial || ctx_partial || raw_rel.contains("${");
                // `params = {"deviceName"}`: a distinct operation on the same route.
                let selector = ann.strings(msrc, &["params"], consts);
                if !selector.is_empty() {
                    let selector = selector.join(", ");
                    op.id = format!("{}?{selector}", op.id);
                    op.selector = Some(selector);
                }
                op.summary = m
                    .anns
                    .iter()
                    .find(|a| matches!(a.name.as_str(), "Operation" | "ApiOperation"))
                    .and_then(|a| a.strings(msrc, &["summary", "value"], consts).into_iter().next())
                    .filter(|s| !s.is_empty())
                    .and_then(|s| first_sentence(&s))
                    .or_else(|| m.doc.clone());
                let (request_declared, response_declared) = describe(idx, models, *owner, cl.fi, flavor, m, &mut op);
                let own_auth = auth_from(msrc, &m.anns, consts);
                let permit = m.anns.iter().any(|a| matches!(a.name.as_str(), "PermitAll" | "Anonymous"));
                let reqs = if permit {
                    vec![]
                } else if own_auth.is_empty() {
                    class_auth.clone()
                } else {
                    own_auth
                };
                let declared = permit || !reqs.is_empty();
                for r in reqs {
                    add_auth(&mut op, r);
                }
                ops.push(JavaOp {
                    draft: Draft { op, request_declared, response_declared },
                    rel_path: rel,
                    auth_declared: declared,
                });
            }
        }
    }
}

fn is_mapping(a: &Ann, flavor: Flavor) -> bool {
    match flavor {
        Flavor::Spring => a.name == "RequestMapping" || SPRING_VERBS.iter().any(|(n, _)| *n == a.name),
        Flavor::JaxRs => JAXRS_VERBS.contains(&a.name.as_str()),
        Flavor::Micronaut => MICRONAUT_VERBS.iter().any(|(n, _)| *n == a.name),
    }
}

/// Parameters, body, response, statuses and errors of one handler. Returns (request declared,
/// response declared).
fn describe(
    idx: &Index,
    models: &mut Models,
    ann_ci: usize,
    body_fi: usize,
    flavor: Flavor,
    m: &Sig,
    op: &mut Operation,
) -> (bool, bool) {
    let msrc = idx.src(ann_ci);
    let ann_fi = idx.classes[ann_ci].fi;
    let unit = op.unit.clone();
    let consts = idx.consts(&unit);
    let mut request_declared = !matches!(op.method.as_str(), "POST" | "PUT" | "PATCH");
    let path_vars = path_params(&op.path);
    for p in &m.params {
        if injected(&p.type_name) {
            continue;
        }
        let ev = msrc.ev(p.at);
        let find = |names: &[&str]| p.anns.iter().find(|a| names.contains(&a.name.as_str()));
        let named = |a: &Ann| {
            a.strings(msrc, &["", "value", "name"], consts)
                .into_iter()
                .next()
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| p.name.clone())
        };
        let default = p.anns.iter().any(|a| a.name == "DefaultValue");
        let (location, name, mut required) = if let Some(a) = find(&["PathVariable", "PathParam"]) {
            ("path", named(a), true)
        } else if let Some(a) = find(&["RequestParam", "QueryValue", "QueryParam"]) {
            let req = match flavor {
                Flavor::Spring => {
                    a.raw(msrc, &["required"]).is_none_or(|r| r != "false") && !a.has_arg(msrc, "defaultValue")
                }
                Flavor::Micronaut => {
                    !a.has_arg(msrc, "defaultValue")
                        && !p.type_name.starts_with("Optional")
                        && !p.anns.iter().any(|x| x.name == "Nullable")
                }
                Flavor::JaxRs => false,
            };
            let multipart = p.type_name.contains("MultipartFile");
            (if multipart { "form" } else { "query" }, named(a), req && !default)
        } else if let Some(a) = find(&["RequestHeader", "HeaderParam", "Header"]) {
            let req = flavor == Flavor::Spring
                && a.raw(msrc, &["required"]).is_none_or(|r| r != "false")
                && !a.has_arg(msrc, "defaultValue");
            ("header", named(a), req)
        } else if let Some(a) = find(&["CookieValue", "CookieParam"]) {
            ("cookie", named(a), false)
        } else if let Some(a) = find(&["FormParam", "RequestPart", "Part"]) {
            ("form", named(a), true)
        } else if let Some(a) = find(&["RequestBody", "Body"]) {
            let (inner, _) = unwrap_java(&p.type_name);
            let mut tr = type_ref(&inner);
            tr.collection = unwrap_java(&p.type_name).1;
            tr.model = models.model_for(ann_fi, &p.type_name, 0);
            request_declared =
                !matches!(inner.as_str(), "String" | "Object" | "Map" | "JsonNode" | "ObjectNode" | "byte[]")
                    || tr.model.is_some();
            op.request_body = Some(tr);
            let _ = a;
            continue;
        } else if p.anns.iter().any(|a| {
            matches!(
                a.name.as_str(),
                "Context"
                    | "Suspended"
                    | "BeanParam"
                    | "AuthenticationPrincipal"
                    | "CurrentUser"
                    | "ModelAttribute"
                    | "RequestAttribute"
                    | "SessionAttribute"
                    | "Parameter"
            )
        }) {
            continue;
        } else if flavor == Flavor::Micronaut && path_vars.contains(&p.name) {
            ("path", p.name.clone(), true)
        } else if flavor == Flavor::JaxRs && !simple_type(&p.type_name) {
            // An un-annotated entity parameter is the request body.
            let (inner, coll) = unwrap_java(&p.type_name);
            let mut tr = type_ref(&inner);
            tr.collection = coll;
            tr.model = models.model_for(ann_fi, &p.type_name, 0);
            request_declared =
                tr.model.is_some() || !matches!(inner.as_str(), "String" | "Object" | "byte[]" | "InputStream");
            op.request_body = Some(tr);
            continue;
        } else if flavor == Flavor::Spring && simple_type(&p.type_name) {
            ("query", p.name.clone(), false)
        } else {
            continue;
        };
        if p.type_name.starts_with("Optional") {
            required = false;
        }
        let enum_vals = idx
            .resolve(ann_fi, &p.type_name)
            .filter(|&e| idx.classes[e].kind == "enum")
            .map(|e| idx.detail(e).enum_constants.clone());
        let (rules, req2) = bean_rules(msrc, &p.anns, &p.type_name, enum_vals.as_deref(), &ev);
        add_param(
            op,
            Param {
                code_name: (name != p.name).then(|| p.name.clone()),
                name,
                location: location.into(),
                type_name: p.type_name.clone(),
                required: required || req2 && location != "path",
                rules,
                evidence: ev,
            },
        );
    }

    // Response type.
    let mut response_declared = false;
    if let Some(rt) = m.return_type.as_deref() {
        let (inner, coll) = unwrap_java(rt);
        let raw = rt.split('<').next().unwrap_or(rt).trim();
        match inner.as_str() {
            "void" | "Void" => {
                response_declared = true;
                if op.success_status.is_none() {
                    op.success_status = Some(if flavor == Flavor::JaxRs && raw == "void" { 204 } else { 200 });
                }
            }
            "?" | "Object" | "Response" | "ResponseEntity" | "HttpResponse" | "MutableHttpResponse" | "String"
                if inner == raw || inner == "?" || inner == "Object" => {}
            _ => {
                let mut tr = type_ref(&inner);
                tr.collection = coll;
                tr.model = models.model_for(ann_fi, &inner, 0);
                response_declared = true;
                op.response = Some(tr);
            }
        }
    }
    for a in &m.anns {
        if matches!(a.name.as_str(), "ResponseStatus" | "Status") {
            if let Some(s) =
                a.raw(msrc, &["", "value", "code"]).and_then(|r| status_code(&r).or_else(|| r.parse().ok()))
            {
                op.success_status = Some(s);
            }
        }
    }

    // Body: statuses, thrown errors (and one call deep into the unit's own methods).
    if let Some((bs, be)) = m.body {
        let bsrc = &idx.files[body_fi].src;
        for (s, at) in body_statuses(bsrc, bs, be) {
            if s >= 400 {
                add_error(op, Some(s), None, bsrc.ev(at));
            } else if op.success_status.is_none() || op.success_status == Some(200) && s != 200 {
                op.success_status = Some(s);
            }
        }
        throws(idx, body_fi, bs, be, op);
        for (fi, (cs, ce)) in called_methods(idx, &unit, bsrc, bs, be) {
            throws(idx, fi, cs, ce, op);
        }
    }
    (request_declared, response_declared)
}

/// `throw new X("msg")` sites between `s` and `e` whose exception maps to a status.
fn throws(idx: &Index, fi: usize, s: usize, e: usize, op: &mut Operation) {
    let src = &idx.files[fi].src;
    let code = &src.code;
    for at in find_word(&code[s..e], "throw") {
        let abs = s + at;
        let Some((ns, ne)) = ident_at(code, abs + 5) else { continue };
        if &code[ns..ne] != "new" {
            continue;
        }
        let tstart = skip_ws(code, ne);
        let mut tend = tstart;
        while tend < e && (is_ident(code.as_bytes()[tend]) || code.as_bytes()[tend] == b'.') {
            tend += 1;
        }
        let ex = code[tstart..tend].rsplit('.').next().unwrap_or("").to_string();
        let open = skip_ws(code, tend);
        let args = if code.as_bytes().get(open) == Some(&b'(') {
            matching(code, open).map(|c| split_args(code, open + 1, c)).unwrap_or_default()
        } else {
            vec![]
        };
        let arg_status = args.iter().find_map(|&(a, b)| {
            let t = src.slice(a, b);
            (t.contains("HttpStatus")
                || t.contains("Status.")
                || t.contains("Response.Status")
                || t.trim().parse::<u16>().is_ok_and(|n| (400..600).contains(&n)))
            .then(|| status_code(t))
            .flatten()
        });
        let status = match ex.as_str() {
            "ResponseStatusException"
            | "WebApplicationException"
            | "ClientErrorException"
            | "ServerErrorException"
            | "HttpStatusException" => arg_status,
            _ => arg_status.or_else(|| idx.status_of(&op.unit, fi, &ex).map(|x| x.0)),
        };
        let Some(status) = status.filter(|s| *s >= 400) else { continue };
        let message = args.iter().find_map(|&(a, b)| string_lit(src.slice(a, b).trim()));
        add_error(op, Some(status), message, src.ev(abs));
    }
}

/// Bodies of the unit's own methods called from a handler (`service.find(…)`), when the name is
/// unique among the unit's methods that throw.
fn called_methods(idx: &Index, unit: &str, src: &Src, s: usize, e: usize) -> Vec<(usize, (usize, usize))> {
    let code = &src.code;
    let b = code.as_bytes();
    let throwing = idx.throwing_methods(unit);
    let mut names: BTreeSet<&str> = BTreeSet::new();
    let mut i = s;
    while i < e {
        if b[i] == b'.' {
            if let Some((ns, ne)) = ident_at(code, i + 1) {
                if b.get(skip_ws(code, ne)) == Some(&b'(') {
                    names.insert(&code[ns..ne]);
                }
                i = ne;
                continue;
            }
        }
        i += 1;
    }
    names
        .into_iter()
        .filter_map(|n| match throwing.get(n).map(Vec::as_slice) {
            Some([one]) => Some(*one),
            _ => None,
        })
        .collect()
}

/// Security annotations: `@PreAuthorize`, `@Secured`, `@RolesAllowed`, `@Authenticated`, `@DenyAll`.
fn auth_from(src: &Src, anns: &[Ann], consts: &HashMap<String, String>) -> Vec<Requirement> {
    let mut out = Vec::new();
    for a in anns {
        let ev = src.ev(a.at);
        match a.name.as_str() {
            "PreAuthorize" | "PostAuthorize" => {
                let expr = a.strings(src, &["", "value"], consts).join(" ");
                if expr.is_empty() || expr.contains("permitAll") {
                    continue;
                }
                let lower = expr.to_lowercase();
                let kind = if lower.contains("hasscope") || lower.contains("scope") {
                    "scope"
                } else if lower.contains("role") || lower.contains("authority") {
                    "role"
                } else if lower.contains("isauthenticated") || lower.contains("isfullyauthenticated") {
                    "authenticated"
                } else {
                    "custom"
                };
                out.push(Requirement { kind: kind.into(), detail: expr, evidence: ev });
            }
            "Secured" | "RolesAllowed" => {
                let vals = a.strings(src, &["", "value"], consts);
                let raw = a.raw(src, &["", "value"]).unwrap_or_default();
                if raw.contains("IS_ANONYMOUS") || vals.iter().any(|v| v == "isAnonymous()") {
                    continue;
                }
                if raw.contains("IS_AUTHENTICATED") || vals.iter().any(|v| v == "isAuthenticated()") {
                    out.push(Requirement {
                        kind: "authenticated".into(),
                        detail: format!("@{}", a.name),
                        evidence: ev,
                    });
                } else if !vals.is_empty() {
                    out.push(Requirement { kind: "role".into(), detail: vals.join(", "), evidence: ev });
                }
            }
            "Authenticated" => {
                out.push(Requirement { kind: "authenticated".into(), detail: "@Authenticated".into(), evidence: ev })
            }
            "DenyAll" => out.push(Requirement { kind: "custom".into(), detail: "@DenyAll".into(), evidence: ev }),
            _ => {}
        }
    }
    out
}

// ─────────────────────────────── functional routes (WebFlux / WebMvc.fn) ───────────────────────────────

fn functional_routes(idx: &Index, models: &mut Models, unit: &str, classes: &[usize], ops: &mut Vec<JavaOp>) {
    let mut seen_files = BTreeSet::new();
    for &ci in classes {
        let fi = idx.classes[ci].fi;
        let f = &idx.files[fi];
        if !seen_files.insert(fi) || !f.facts.imports.iter().any(|i| i.specifier.contains(".function.server")) {
            continue;
        }
        let src = &f.src;
        let code = &src.code;
        let consts = idx.consts(unit);
        // Prefixes: `nest(path("/x"), …)` / `.nest(path("/x"), …)` / `.path("/x", builder -> …)`.
        let mut prefixes: Vec<(usize, usize, String)> = Vec::new();
        for at in find_word(code, "nest") {
            let open = skip_ws(code, at + 4);
            if code.as_bytes().get(open) != Some(&b'(') {
                continue;
            }
            let Some(close) = matching(code, open) else { continue };
            let Some(&(s, e)) = split_args(code, open + 1, close).first() else { continue };
            let first = src.slice(s, e);
            if let Some(p) = first.find("path(") {
                let inner_open = s + p + 4;
                if let Some(ic) = matching(code, inner_open) {
                    if let Some(v) = concat_value(src, inner_open + 1, ic, consts) {
                        prefixes.push((open, close, v));
                    }
                }
            }
        }
        for (rs, ms, open) in method_calls(code, &["path"]) {
            let _ = rs;
            let Some(close) = matching(code, open) else { continue };
            let args = split_args(code, open + 1, close);
            if args.len() == 2 && src.slice(args[1].0, args[1].1).contains("->") {
                if let Some(v) = concat_value(src, args[0].0, args[0].1, consts) {
                    prefixes.push((ms, close, v));
                }
            }
        }
        for verb in JAXRS_VERBS {
            for at in find_word(code, verb) {
                let open = skip_ws(code, at + verb.len());
                if code.as_bytes().get(open) != Some(&b'(') {
                    continue;
                }
                let before = code[..at].trim_end();
                if !(before.ends_with('.') || before.ends_with('(') || before.ends_with(',')) {
                    continue;
                }
                let Some(close) = matching(code, open) else { continue };
                let args = split_args(code, open + 1, close);
                let Some(&(ps, pe)) = args.first() else { continue };
                let Some(sub) = concat_value(src, ps, pe, consts) else { continue };
                // Handler: second argument (`.GET("/x", h::m)`), or the argument after `GET(...)` in `route(GET("/x"), h::m)`.
                let handler_expr = if args.len() >= 2 {
                    Some(src.slice(args[1].0, args[1].1).to_string())
                } else {
                    let after = skip_ws(code, close + 1);
                    (code.as_bytes().get(after) == Some(&b',')).then(|| {
                        let s2 = skip_ws(code, after + 1);
                        let mut e2 = s2;
                        while e2 < code.len() && !matches!(code.as_bytes()[e2], b')' | b',' | b'\n') {
                            e2 += 1;
                        }
                        src.slice(s2, e2).trim().to_string()
                    })
                };
                let prefix: String = {
                    let mut ps: Vec<&(usize, usize, String)> =
                        prefixes.iter().filter(|(s, e, _)| *s < at && at < *e).collect();
                    ps.sort_by_key(|(s, _, _)| *s);
                    ps.iter().fold(String::new(), |acc, (_, _, p)| join_path(&acc, p))
                };
                let rel = join_path(&prefix, &sub);
                let handler_name = handler_expr
                    .as_deref()
                    .and_then(|h| h.split("::").nth(1))
                    .map(|x| x.trim().to_string())
                    .unwrap_or_else(|| "lambda".into());
                let handler_class = handler_expr.as_deref().and_then(|h| h.split("::").next()).map(str::trim);
                let (ctx, partial) =
                    idx.placeholders(unit, &idx.config_value(unit, "spring.webflux.base-path").unwrap_or_default());
                let mut op = new_op(
                    unit,
                    "spring-webflux",
                    verb,
                    join_path(&ctx, &rel),
                    SymbolRef { name: handler_name.clone(), evidence: src.ev(at) },
                    src.ev(at),
                );
                op.path_partial = partial;
                let mut request_declared = !matches!(*verb, "POST" | "PUT" | "PATCH");
                let mut response_declared = false;
                // Find the handler method in the unit to read bodyToMono / body types / statuses.
                let target = classes.iter().copied().find_map(|hc| {
                    let hc_name = &idx.classes[hc].name;
                    let d = idx.detail(hc);
                    // `handler::m` names a variable; `BookHandler::m` / `this::m` the class.
                    let fits = handler_class.is_none_or(|c| {
                        let c = c.trim_start_matches("this").to_lowercase();
                        c.is_empty() || hc_name.to_lowercase().contains(&c)
                    });
                    if !fits {
                        return None;
                    }
                    d.methods.iter().find(|m| m.name == handler_name).cloned().map(|m| (hc, m))
                });
                if let Some((hc, m)) = target {
                    let hfi = idx.classes[hc].fi;
                    let hsrc = &idx.files[hfi].src;
                    op.handler.evidence = EvidenceRef {
                        file_path: hsrc.path.clone(),
                        start_line: m.start_line,
                        end_line: m.end_line,
                        symbol_name: Some(m.name.clone()),
                        note: None,
                        repo: None,
                    };
                    if let Some((bs, be)) = m.body {
                        let hcode = &hsrc.code;
                        for (needle, is_req) in [("bodyToMono", true), ("bodyToFlux", true), ("body", false)] {
                            for (_, ms2, open2) in method_calls(&hcode[..be], &[needle]) {
                                if ms2 < bs {
                                    continue;
                                }
                                let Some(close2) = matching(hcode, open2) else { continue };
                                let Some(&(ls, le)) = split_args(hcode, open2 + 1, close2).last() else { continue };
                                let Some(t) = hsrc.slice(ls, le).trim().strip_suffix(".class") else { continue };
                                let mut tr = type_ref(t);
                                let publisher = split_args(hcode, open2 + 1, close2)
                                    .first()
                                    .map(|&(a, b)| hsrc.slice(a, b))
                                    .unwrap_or("");
                                tr.collection = needle == "bodyToFlux" || !is_req && publisher.starts_with("Flux");
                                tr.model = models.model_for(hfi, t, 0);
                                if is_req {
                                    op.request_body = Some(tr);
                                    request_declared = true;
                                } else {
                                    op.response = Some(tr);
                                    response_declared = true;
                                }
                                break;
                            }
                        }
                        for (s, at2) in body_statuses(hsrc, bs, be) {
                            if s >= 400 {
                                add_error(&mut op, Some(s), None, hsrc.ev(at2));
                            } else if op.success_status.is_none() {
                                op.success_status = Some(s);
                            }
                        }
                        throws(idx, hfi, bs, be, &mut op);
                    }
                }
                ops.push(JavaOp {
                    draft: Draft { op, request_declared, response_declared },
                    rel_path: rel,
                    auth_declared: false,
                });
            }
        }
    }
}

// ─────────────────────────────── Spring Security matchers ───────────────────────────────

struct Matcher {
    method: Option<String>,
    patterns: Vec<String>,
    /// `None` = permitted.
    requirement: Option<(String, String)>,
    evidence: EvidenceRef,
}

fn apply_security(idx: &Index, unit: &str, classes: &[usize], ops: &mut [JavaOp]) {
    let mut matchers: Vec<Matcher> = Vec::new();
    let mut files: Vec<usize> = classes.iter().map(|&c| idx.classes[c].fi).collect();
    files.sort();
    files.dedup();
    for fi in files {
        let src = &idx.files[fi].src;
        if !src.text.contains("Matchers(") && !src.text.contains("anyRequest(") && !src.text.contains("anyExchange(") {
            continue;
        }
        let code = &src.code;
        let consts = idx.consts(unit);
        let mut sites = method_calls(
            code,
            &[
                "requestMatchers",
                "antMatchers",
                "mvcMatchers",
                "pathMatchers",
                "regexMatchers",
                "anyRequest",
                "anyExchange",
            ],
        );
        sites.sort_by_key(|s| s.1);
        for (_, ms, open) in sites {
            let Some(close) = matching(code, open) else { continue };
            let name_end = ms + code[ms..].bytes().take_while(|&b| is_ident(b)).count();
            let any = matches!(&code[ms..name_end], "anyRequest" | "anyExchange");
            let mut method = None;
            let mut patterns = Vec::new();
            for (s, e) in split_args(code, open + 1, close) {
                let t = src.slice(s, e).trim();
                if t.starts_with("HttpMethod.") {
                    method = Some(t.trim_start_matches("HttpMethod.").to_string());
                } else if let Some(v) = concat_value(src, s, e, consts) {
                    patterns.push(v);
                }
            }
            if any {
                patterns.push("/**".into());
            }
            if patterns.is_empty() {
                continue;
            }
            // The authorisation call chained right after.
            let after = skip_ws(code, close + 1);
            if code.as_bytes().get(after) != Some(&b'.') {
                continue;
            }
            let Some((ns, ne)) = ident_at(code, after + 1) else { continue };
            let call = &code[ns..ne];
            let open2 = skip_ws(code, ne);
            let arg = (code.as_bytes().get(open2) == Some(&b'('))
                .then(|| matching(code, open2).map(|c| src.slice(open2 + 1, c).trim().to_string()))
                .flatten()
                .unwrap_or_default();
            let requirement = match call {
                "permitAll" => None,
                "authenticated" | "fullyAuthenticated" => {
                    Some(("authenticated".to_string(), "authenticated()".to_string()))
                }
                "hasRole" | "hasAnyRole" | "hasAuthority" | "hasAnyAuthority" => {
                    Some(("role".to_string(), format!("{call}({arg})")))
                }
                "access" => Some(("custom".to_string(), format!("access({arg})"))),
                "denyAll" => Some(("custom".to_string(), "denyAll()".to_string())),
                _ => continue,
            };
            matchers.push(Matcher { method, patterns, requirement, evidence: src.ev(ms) });
        }
    }
    if matchers.is_empty() {
        return;
    }
    for o in ops.iter_mut().filter(|o| !o.auth_declared) {
        let hit = matchers.iter().find(|m| {
            m.method.as_ref().is_none_or(|x| x.eq_ignore_ascii_case(&o.draft.op.method))
                && m.patterns.iter().any(|p| ant_match(p, &o.rel_path) || ant_match(p, &o.draft.op.path))
        });
        if let Some(Matcher { requirement: Some((kind, detail)), evidence, .. }) = hit {
            add_auth(
                &mut o.draft.op,
                Requirement {
                    kind: kind.clone(),
                    detail: detail.trim_matches('"').to_string(),
                    evidence: evidence.clone(),
                },
            );
        }
    }
}

/// Ant-style pattern match (`/api/**`, `/users/*`, `/x/{id}`) against a route path.
fn ant_match(pattern: &str, path: &str) -> bool {
    let ps: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let xs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    fn go(ps: &[&str], xs: &[&str]) -> bool {
        match (ps.first(), xs.first()) {
            (None, None) => true,
            (Some(&"**"), _) => (0..=xs.len()).any(|k| go(&ps[1..], &xs[k..])),
            (Some(p), Some(x)) => {
                let seg_ok = *p == "*"
                    || p.starts_with('{')
                    || p == x
                    || (p.contains('*') && x.starts_with(p.trim_end_matches('*')));
                seg_ok && go(&ps[1..], &xs[1..])
            }
            _ => false,
        }
    }
    go(&ps, &xs)
}

// ─────────────────────────────── client calls ───────────────────────────────

/// Declarative clients: `@FeignClient`, MicroProfile `@RegisterRestClient`, Micronaut `@Client` interfaces.
fn client_calls(idx: &Index, models: &mut Models, ci: usize, h: &mut Harvest) {
    let cl = idx.classes[ci].clone();
    if cl.kind != "interface" {
        return;
    }
    let src = idx.src(ci);
    let d = idx.detail(ci);
    let consts = idx.consts(&cl.unit);
    let Some(client) =
        d.anns.iter().find(|a| matches!(a.name.as_str(), "FeignClient" | "RegisterRestClient" | "Client"))
    else {
        return;
    };
    let flavor = match client.name.as_str() {
        "FeignClient" => Flavor::Spring,
        "RegisterRestClient" => Flavor::JaxRs,
        _ => Flavor::Micronaut,
    };
    let (target_name, base) = match client.name.as_str() {
        "FeignClient" => {
            let name = client.strings(src, &["", "value", "name"], consts).into_iter().next();
            let url = client.strings(src, &["url"], consts).into_iter().next();
            let path = client.strings(src, &["path"], consts).into_iter().next().unwrap_or_default();
            (name.filter(|n| !n.contains("${")).or_else(|| url.as_deref().and_then(crate::jvm::url_host)), path)
        }
        "RegisterRestClient" => {
            let key = client.strings(src, &["configKey"], consts).into_iter().next();
            let uri = client.strings(src, &["baseUri"], consts).into_iter().next();
            (key.or_else(|| uri.as_deref().and_then(crate::jvm::url_host)), String::new())
        }
        _ => {
            let v = client.strings(src, &["", "value", "id"], consts).into_iter().next().unwrap_or_default();
            if v.starts_with('/') {
                (None, v)
            } else {
                (Some(crate::jvm::url_host(&v).unwrap_or(v)), String::new())
            }
        }
    };
    let class_path = d
        .anns
        .iter()
        .find(|a| matches!(a.name.as_str(), "RequestMapping" | "Path"))
        .and_then(|a| a.strings(src, &["", "value", "path"], consts).into_iter().next())
        .unwrap_or_default();
    // Only calls to services in this repository are contracts we can match; an external API
    // (`url = "${rates.url}"`) is not one of them.
    let Some(target_unit) = target_name.as_deref().and_then(|n| idx.unit_named(n)).filter(|u| *u != cl.unit) else {
        return;
    };
    let target_unit = Some(target_unit);
    for m in &d.methods {
        for a in &m.anns {
            let (verb, sub) = match flavor {
                Flavor::Spring => {
                    if let Some((_, v)) = SPRING_VERBS.iter().find(|(n, _)| *n == a.name) {
                        (
                            v.to_string(),
                            a.strings(src, &["", "value", "path"], consts).into_iter().next().unwrap_or_default(),
                        )
                    } else if a.name == "RequestMapping" {
                        let ms = a.idents(src, &["method"]);
                        (
                            ms.first().cloned().unwrap_or_else(|| "GET".into()),
                            a.strings(src, &["", "value", "path"], consts).into_iter().next().unwrap_or_default(),
                        )
                    } else {
                        continue;
                    }
                }
                Flavor::JaxRs => {
                    if !JAXRS_VERBS.contains(&a.name.as_str()) {
                        continue;
                    }
                    let p = m
                        .anns
                        .iter()
                        .find(|x| x.name == "Path")
                        .and_then(|x| x.strings(src, &["", "value"], consts).into_iter().next())
                        .unwrap_or_default();
                    (a.name.clone(), p)
                }
                Flavor::Micronaut => {
                    let Some((_, v)) = MICRONAUT_VERBS.iter().find(|(n, _)| *n == a.name) else { continue };
                    (
                        v.to_string(),
                        a.strings(src, &["", "value", "uri"], consts).into_iter().next().unwrap_or_default(),
                    )
                }
            };
            let (path, _) = idx.placeholders(&cl.unit, &join_path(&join_path(&base, &class_path), &sub));
            let expects =
                m.return_type.as_deref().map(|r| unwrap_java(r).0).filter(|t| !matches!(t.as_str(), "void" | "Void"));
            if let Some(t) = &expects {
                let _ = models.model_for(cl.fi, t, 0);
            }
            h.clients.push(ClientDraft {
                call: ClientCall {
                    unit: cl.unit.clone(),
                    method: verb.to_uppercase(),
                    path: join_path("", &path),
                    target_host: None,
                    target_unit: target_unit.clone(),
                    operation: None,
                    caller: Some(format!("{}.{}", cl.name, m.name)),
                    drift: vec![],
                    evidence: src.ev(a.at),
                },
                expects,
                expects_fields: Vec::new(),
            });
        }
    }
}

/// `restTemplate.getForObject("http://svc/x/{id}", T.class, id)`, `webClient.get().uri("/x")…`,
/// `restClient.post().uri(…)`.
fn template_calls(idx: &Index, models: &mut Models, fi: usize, f: &Loaded, h: &mut Harvest) {
    let src = &f.src;
    let text = &src.text;
    if !(text.contains("RestTemplate")
        || text.contains("WebClient")
        || text.contains("RestClient")
        || text.contains("restTemplate"))
    {
        return;
    }
    let code = &src.code;
    let consts = idx.consts(f.unit);
    let mut found: Vec<(usize, String, String, Option<String>)> = Vec::new();
    const TEMPLATE: &[(&str, &str)] = &[
        ("getForObject", "GET"),
        ("getForEntity", "GET"),
        ("postForObject", "POST"),
        ("postForEntity", "POST"),
        ("postForLocation", "POST"),
        ("put", "PUT"),
        ("patchForObject", "PATCH"),
        ("delete", "DELETE"),
        ("exchange", ""),
    ];
    let names: Vec<&str> = TEMPLATE.iter().map(|(n, _)| *n).collect();
    for (rs, ms, open) in method_calls(code, &names) {
        let recv = src.code_slice(rs, ms - 1).to_lowercase();
        if !(recv.contains("resttemplate") || recv.contains("template") || recv.ends_with("rest")) {
            continue;
        }
        let name_end = ms + code[ms..].bytes().take_while(|&b| is_ident(b)).count();
        let name = &code[ms..name_end];
        let Some(close) = matching(code, open) else { continue };
        let args = split_args(code, open + 1, close);
        let Some(&(us, ue)) = args.first() else { continue };
        let Some(url) = java_url(src, us, ue, consts) else { continue };
        let mut verb = TEMPLATE.iter().find(|(n, _)| *n == name).map(|(_, v)| v.to_string()).unwrap_or_default();
        if verb.is_empty() {
            verb = args
                .get(1)
                .map(|&(s, e)| src.slice(s, e).trim().rsplit('.').next().unwrap_or("GET").to_uppercase())
                .unwrap_or_else(|| "GET".into());
        }
        let expects = args.iter().rev().find_map(|&(s, e)| response_type(src.slice(s, e)));
        found.push((rs, verb, url, expects));
    }
    // Fluent clients: `.get().uri("…")`, `.method(HttpMethod.POST).uri("…")`.
    for (rs, ms, open) in method_calls(code, &["uri"]) {
        let before = code[..ms.saturating_sub(1)].trim_end();
        let verb = ["get", "post", "put", "delete", "patch"]
            .iter()
            .find(|v| before.ends_with(&format!(".{v}()")))
            .map(|v| v.to_uppercase())
            .or_else(|| {
                let p = before.rfind(".method(")?;
                Some(before[p + 8..].trim_end_matches(')').rsplit('.').next()?.to_uppercase())
            });
        let Some(verb) = verb else { continue };
        let Some(close) = matching(code, open) else { continue };
        let Some(&(us, ue)) = split_args(code, open + 1, close).first() else { continue };
        let Some(url) = java_url(src, us, ue, consts) else { continue };
        let tail_end = code[close..].find(';').map(|x| close + x).unwrap_or(close);
        let tail = src.slice(close, tail_end);
        let expects = ["bodyToMono(", "bodyToFlux(", "body(", "toEntity("].iter().find_map(|n| {
            let p = tail.find(n)?;
            let arg = &tail[p + n.len()..];
            response_type(arg.split(&[')', ';'][..]).next()?).or_else(|| response_type(arg))
        });
        let _ = rs;
        found.push((ms, verb, url, expects));
    }
    // Base URLs: `.baseUrl("http://svc")` / `@Value("${svc.url}")` hosts in the same file.
    let base_host = find_word(code, "baseUrl").into_iter().find_map(|at| {
        let open = skip_ws(code, at + 7);
        let close = matching(code, open)?;
        let v = concat_value(src, open + 1, close, consts)?;
        crate::jvm::url_host(&idx.placeholders(f.unit, &v).0)
    });
    for (at, method, url, expects) in found {
        let (path, host) = split_host(&url);
        let line = src.line(at);
        let caller = f.enclosing(line).map(|s| s.name.clone());
        let target_unit = host.or(base_host.clone()).and_then(|h| idx.unit_named(&h)).filter(|u| u != f.unit);
        if let Some(t) = &expects {
            let _ = models.model_for(fi, t, 0);
        }
        let mut evidence = src.ev(at);
        evidence.symbol_name = None;
        h.clients.push(ClientDraft {
            call: ClientCall {
                unit: f.unit.to_string(),
                method,
                path,
                // A declarative client names its service, not a host.
                target_host: None,
                target_unit,
                operation: None,
                caller,
                drift: vec![],
                evidence,
            },
            expects,
            expects_fields: Vec::new(),
        });
    }
}

/// The response type an argument names: `Account.class`, or the type argument of
/// `new ParameterizedTypeReference<List<Account>>() {}` / `new TypeReference<Account>()`.
fn response_type(arg: &str) -> Option<String> {
    let a = arg.trim();
    if let Some(t) = a.strip_suffix(".class") {
        return Some(t.trim().rsplit('.').next().unwrap_or(t).to_string());
    }
    for r in ["ParameterizedTypeReference<", "TypeReference<", "TypeRef<"] {
        if let Some(p) = a.find(r) {
            let inner = &a[p + r.len()..];
            let mut depth = 1usize;
            let mut end = inner.len();
            for (i, c) in inner.char_indices() {
                match c {
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
            let (t, _) = unwrap_java(&inner[..end]);
            return (!t.is_empty()).then_some(t);
        }
    }
    None
}

/// Java URL expression → raw URL text with `{var}` for dynamic parts.
fn java_url(src: &Src, s: usize, e: usize, consts: &HashMap<String, String>) -> Option<String> {
    let text = src.slice(s, e).trim();
    let mut out = String::new();
    let mut any_lit = false;
    for part in split_top(text, &['+']) {
        let p = part.trim();
        if let Some(l) = string_lit(p) {
            out.push_str(&l);
            any_lit = true;
        } else if let Some(v) = consts.get(p.rsplit('.').next().unwrap_or(p)) {
            out.push_str(v);
            any_lit = true;
        } else if let Some(args) = p.strip_prefix("String.format(").and_then(|r| r.strip_suffix(')')) {
            let parts = split_top(args, &[',']);
            let fmt = string_lit(parts.first()?)?;
            let mut i = 1;
            let mut rest = fmt.as_str();
            while let Some(q) = rest.find('%') {
                out.push_str(&rest[..q]);
                let name: String = parts
                    .get(i)
                    .map(|x| x.rsplit(['.', '(']).next().unwrap_or(x).chars().filter(|c| c.is_alphanumeric()).collect())
                    .unwrap_or_else(|| "param".into());
                out.push_str(&format!("{{{name}}}"));
                i += 1;
                rest = rest.get(q + 2..).unwrap_or("");
            }
            out.push_str(rest);
            any_lit = true;
        } else {
            let name: String =
                p.rsplit(['.', '(']).next().unwrap_or(p).chars().filter(|c| c.is_alphanumeric() || *c == '_').collect();
            if out.is_empty() {
                // A dynamic base (`baseUrl + "/x"`) contributes no path.
                continue;
            }
            out.push_str(&format!("{{{}}}", if name.is_empty() { "param".into() } else { name }));
        }
    }
    any_lit.then_some(out)
}

/// `http://svc:8080/x/{id}?q` → ("/x/{id}", Some("svc")); `/x` → ("/x", None).
fn split_host(url: &str) -> (String, Option<String>) {
    let host = crate::jvm::url_host(url);
    let path = match url.split_once("://") {
        Some((_, rest)) => rest.find('/').map(|i| rest[i..].to_string()).unwrap_or_else(|| "/".into()),
        None => url.to_string(),
    };
    let path = path.split(['?', '#']).next().unwrap_or("").to_string();
    (join_path("", &path), host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_types_unwrap() {
        assert_eq!(unwrap_java("ResponseEntity<List<AccountDto>>"), ("AccountDto".into(), true));
        assert_eq!(unwrap_java("Mono<ResponseEntity<Order>>"), ("Order".into(), false));
        assert_eq!(unwrap_java("Flux<Order>"), ("Order".into(), true));
        assert_eq!(unwrap_java("PageData<Device>"), ("Device".into(), true));
        assert_eq!(unwrap_java("Uni<RestResponse<Item>>"), ("Item".into(), false));
        assert_eq!(unwrap_java("byte[]"), ("byte".into(), true));
    }

    #[test]
    fn ant_patterns() {
        assert!(ant_match("/api/**", "/api/users/{id}"));
        assert!(ant_match("/users/*", "/users/{id}"));
        assert!(!ant_match("/users/*", "/users/{id}/roles"));
        assert!(ant_match("/**", "/"));
        assert!(ant_match("/admin/**", "/admin"));
    }

    #[test]
    fn signatures_parse_with_annotations_and_generics() {
        let text = "  @GetMapping(value = {\"/a\", \"/b\"}, produces = \"application/json\")\n  public <T> ResponseEntity<List<Item>> list(@RequestParam(name = \"q\", required = false) String query, @PathVariable(\"id\") Long id) throws X {\n    return null;\n  }\n";
        let src = Src::new("A.java", text.to_string(), Style::CLike);
        let sig = parse_sig(&src, 0, text.len()).unwrap();
        assert_eq!(sig.name, "list");
        assert_eq!(sig.return_type.as_deref(), Some("ResponseEntity<List<Item>>"));
        assert_eq!(sig.params.len(), 2);
        assert_eq!((sig.params[0].name.as_str(), sig.params[0].type_name.as_str()), ("query", "String"));
        let a = &sig.anns[0];
        assert_eq!(a.strings(&src, &["", "value", "path"], &HashMap::new()), ["/a", "/b"]);
        assert_eq!(sig.params[0].anns[0].strings(&src, &["", "value", "name"], &HashMap::new()), ["q"]);
        assert!(sig.body.is_some());
        assert_eq!((sig.start_line, sig.end_line), (1, 4));
    }
}
