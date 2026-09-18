//! A declaration-level reader for Java sources: types, their annotations,
//! supertypes, fields (type, initializer) and abstract/annotated method
//! signatures, plus `static final String` constants. Structure is found in the
//! ASCII-safe code view (comments and strings blanked); names and literals are
//! read from the original text at the same offsets.

use std::collections::HashMap;

use crate::api::text::{matching, Src, Style};

#[derive(Debug, Clone)]
pub struct JAnn {
    /// Simple name (`Column` for `@jakarta.persistence.Column`).
    pub name: String,
    /// Raw text inside the parentheses.
    pub args: String,
    pub line: u32,
}

#[derive(Debug, Clone)]
pub struct JField {
    pub name: String,
    /// Declared type as written, whitespace collapsed (`List<OrderItem>`).
    pub ty: String,
    pub anns: Vec<JAnn>,
    pub modifiers: Vec<String>,
    /// Initializer expression, trimmed.
    pub init: Option<String>,
    /// Line of the field name.
    pub line: u32,
}

#[derive(Debug, Clone)]
pub struct JMethod {
    pub name: String,
    pub anns: Vec<JAnn>,
    /// Declared return type as written (`JpaRepository<E, UUID>`); empty for constructors.
    pub ret: String,
    /// Byte range of the body between the braces; `None` when abstract.
    pub body: Option<(usize, usize)>,
}

impl JMethod {
    pub fn is_abstract(&self) -> bool {
        self.body.is_none()
    }
}

#[derive(Debug, Clone)]
pub struct JClass {
    pub name: String,
    /// `class`, `interface`, `enum`, `record`.
    pub kind: String,
    /// Type parameter names as declared (`<E extends BaseEntity<D>, D>` → [`E`, `D`]).
    pub type_params: Vec<String>,
    pub anns: Vec<JAnn>,
    /// `extends` clause entries as written (`JpaRepository<Order, Long>`).
    pub extends: Vec<String>,
    pub implements: Vec<String>,
    pub fields: Vec<JField>,
    pub methods: Vec<JMethod>,
    /// Enum constants with their lines.
    pub constants: Vec<(String, u32)>,
    /// Line of the type name.
    pub line: u32,
    /// Byte range of the class body between the braces.
    pub body: (usize, usize),
    /// Index of the file in the parsed set.
    pub file: usize,
}

impl JClass {
    pub fn ann(&self, name: &str) -> Option<&JAnn> {
        self.anns.iter().find(|a| a.name == name)
    }
    pub fn has(&self, name: &str) -> bool {
        self.ann(name).is_some()
    }
}

impl JField {
    pub fn ann(&self, name: &str) -> Option<&JAnn> {
        self.anns.iter().find(|a| a.name == name)
    }
    pub fn has(&self, name: &str) -> bool {
        self.ann(name).is_some()
    }
    pub fn is_static(&self) -> bool {
        self.modifiers.iter().any(|m| m == "static")
    }
}

const MODIFIERS: &[&str] = &[
    "public",
    "private",
    "protected",
    "static",
    "final",
    "abstract",
    "transient",
    "volatile",
    "synchronized",
    "native",
    "strictfp",
    "default",
    "sealed",
    "non-sealed",
];

pub fn parse_file(file: usize, src: &Src, out: &mut Vec<JClass>) {
    let code = src.code.as_str();
    scan_body(file, src, code, 0, code.len(), None, out);
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Declarations directly inside `[start, end)`; nested type bodies recurse.
fn scan_body(
    file: usize,
    src: &Src,
    code: &str,
    start: usize,
    end: usize,
    owner: Option<&str>,
    out: &mut Vec<JClass>,
) -> (Vec<JField>, Vec<JMethod>) {
    let b = code.as_bytes();
    let mut fields = Vec::new();
    let mut methods = Vec::new();
    let mut pending: Vec<JAnn> = Vec::new();
    let mut i = start;
    while i < end {
        while i < end && (b[i].is_ascii_whitespace() || b[i] == b';') {
            if b[i] == b';' {
                pending.clear();
            }
            i += 1;
        }
        if i >= end {
            break;
        }
        // Annotation (but not `@interface`).
        if b[i] == b'@' {
            let mut j = i + 1;
            while j < end && (is_ident(b[j]) || b[j] == b'.') {
                j += 1;
            }
            let full = src.slice(i + 1, j);
            if full == "interface" {
                // Annotation type declaration: skip its body.
                match code[j..end].find('{').map(|p| j + p) {
                    Some(open) => i = matching(code, open).map(|c| c + 1).unwrap_or(end),
                    None => i = end,
                }
                pending.clear();
                continue;
            }
            let name = full.rsplit('.').next().unwrap_or(full).to_string();
            let line = src.line(i);
            let mut k = j;
            while k < end && b[k].is_ascii_whitespace() {
                k += 1;
            }
            let mut args = String::new();
            if k < end && b[k] == b'(' {
                let close = matching(code, k).unwrap_or(end.saturating_sub(1)).min(end.saturating_sub(1));
                args = collapse(src.slice(k + 1, close));
                j = close + 1;
            }
            if !name.is_empty() {
                pending.push(JAnn { name, args, line });
            }
            i = j.max(i + 1);
            continue;
        }
        // Statement up to `;` or a block `{` at depth 0 (parentheses/generics may contain braces in annotations).
        let stmt_start = i;
        let mut paren = 0i32;
        let mut has_eq = false;
        let mut j = i;
        let mut block: Option<usize> = None;
        while j < end {
            match b[j] {
                b'(' | b'[' => paren += 1,
                b')' | b']' => paren -= 1,
                b'=' if paren == 0 => has_eq = true,
                b';' if paren == 0 => break,
                b'{' if paren == 0 => {
                    if has_eq {
                        // Array initializer / lambda body in a field initializer.
                        j = matching(code, j).unwrap_or(end);
                    } else {
                        block = Some(j);
                        break;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        let stmt_end = j.min(end);
        let head_code = &code[stmt_start..stmt_end];
        let words: Vec<&str> =
            head_code.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '-')).collect();
        let first_word = head_code.split_whitespace().next().unwrap_or("");
        if matches!(first_word, "package" | "import") {
            pending.clear();
            i = stmt_end + 1;
            continue;
        }
        let kind_pos = head_code
            .split_whitespace()
            .position(|w| matches!(w, "class" | "interface" | "enum" | "record"))
            .filter(|&p| {
                // A type declaration keyword precedes any parenthesis (not `x.class`).
                let before: String = head_code.split_whitespace().take(p).collect::<Vec<_>>().join(" ");
                !before.contains('(') && !before.contains('=') && !before.contains('.')
            });
        if let (Some(p), Some(open)) = (kind_pos, block) {
            let tokens: Vec<&str> = head_code.split_whitespace().collect();
            let kind = tokens[p].to_string();
            // Name offset: first identifier after the keyword.
            let kw_off = nth_word_offset(head_code, p).unwrap_or(0);
            let mut n = stmt_start + kw_off + kind.len();
            while n < stmt_end && !is_ident(b[n]) {
                n += 1;
            }
            let mut ne = n;
            while ne < stmt_end && is_ident(b[ne]) {
                ne += 1;
            }
            let name = src.slice(n, ne).to_string();
            let full_header = collapse(src.slice(ne, stmt_end));
            let (type_params, header) = split_type_params(&full_header);
            let close = matching(code, open).unwrap_or(end.saturating_sub(1)).min(end.saturating_sub(1));
            let mut cls = JClass {
                name: name.clone(),
                kind: kind.clone(),
                type_params,
                anns: std::mem::take(&mut pending),
                extends: clause(header, "extends"),
                implements: clause(header, "implements"),
                fields: vec![],
                methods: vec![],
                constants: vec![],
                line: src.line(n),
                body: (open + 1, close),
                file,
            };
            // Record components are fields.
            if kind == "record" {
                if let Some(po) = code[ne..open].find('(').map(|x| ne + x) {
                    if let Some(pc) = matching(code, po) {
                        for (s, e) in crate::api::text::split_args(code, po + 1, pc) {
                            if let Some(f) = field_from(src, code, s, e, vec![], false) {
                                cls.fields.push(f);
                            }
                        }
                    }
                }
            }
            let mut body_start = open + 1;
            if kind == "enum" {
                // Constants up to the first top-level `;` (or the whole body).
                let mut depth = 0i32;
                let mut k = open + 1;
                let mut semi = close;
                while k < close {
                    match b[k] {
                        b'(' | b'{' | b'[' => depth += 1,
                        b')' | b'}' | b']' => depth -= 1,
                        b';' if depth == 0 => {
                            semi = k;
                            break;
                        }
                        _ => {}
                    }
                    k += 1;
                }
                for (s, e) in crate::api::text::split_args(code, open + 1, semi) {
                    let mut s2 = s;
                    // Skip annotations on constants.
                    while s2 < e && b[s2] == b'@' {
                        s2 += 1;
                        while s2 < e && (is_ident(b[s2]) || b[s2] == b'.') {
                            s2 += 1;
                        }
                        while s2 < e && b[s2].is_ascii_whitespace() {
                            s2 += 1;
                        }
                        if s2 < e && b[s2] == b'(' {
                            s2 = matching(code, s2).map(|c| c + 1).unwrap_or(e);
                        }
                        while s2 < e && b[s2].is_ascii_whitespace() {
                            s2 += 1;
                        }
                    }
                    let mut ie = s2;
                    while ie < e && is_ident(b[ie]) {
                        ie += 1;
                    }
                    let cname = src.slice(s2, ie);
                    if !cname.is_empty() && cname.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') {
                        cls.constants.push((cname.to_string(), src.line(s2)));
                    }
                }
                body_start = if semi < close { semi + 1 } else { close };
            }
            let (fs, ms) = scan_body(file, src, code, body_start, close, Some(&name), out);
            cls.fields.extend(fs);
            cls.methods = ms;
            out.push(cls);
            i = close + 1;
            continue;
        }
        let anns = std::mem::take(&mut pending);
        // Method / constructor / initializer: `(` before any `=` at depth 0.
        let paren_pos = head_code.find('(');
        let eq_pos = find_top_eq(head_code);
        let is_method = match (paren_pos, eq_pos) {
            (Some(p), Some(e)) => p < e,
            (Some(_), None) => true,
            _ => false,
        };
        let _ = words;
        if is_method {
            if let Some(p) = paren_pos {
                let mut ne = stmt_start + p;
                while ne > stmt_start && b[ne - 1].is_ascii_whitespace() {
                    ne -= 1;
                }
                let mut ns = ne;
                while ns > stmt_start && is_ident(b[ns - 1]) {
                    ns -= 1;
                }
                let name = src.slice(ns, ne).to_string();
                if !name.is_empty()
                    && !matches!(name.as_str(), "if" | "for" | "while" | "switch" | "catch" | "synchronized")
                {
                    let ret = return_type(&collapse(src.slice(stmt_start, ns)));
                    let body = block.map(|open| {
                        let close = matching(code, open).unwrap_or(end.saturating_sub(1)).min(end.saturating_sub(1));
                        (open + 1, close)
                    });
                    methods.push(JMethod { name, anns, ret, body });
                }
            }
            i = match block {
                Some(open) => matching(code, open).map(|c| c + 1).unwrap_or(end),
                None => stmt_end + 1,
            };
            continue;
        }
        if let Some(open) = block {
            // Initializer block (`static { … }`).
            i = matching(code, open).map(|c| c + 1).unwrap_or(end);
            continue;
        }
        if owner.is_some() {
            if let Some(f) = field_from(src, code, stmt_start, stmt_end, anns, true) {
                fields.push(f);
            }
        }
        i = stmt_end + 1;
    }
    (fields, methods)
}

fn nth_word_offset(s: &str, n: usize) -> Option<usize> {
    let mut count = 0;
    let mut in_word = false;
    for (i, c) in s.char_indices() {
        if c.is_whitespace() {
            in_word = false;
        } else if !in_word {
            if count == n {
                return Some(i);
            }
            count += 1;
            in_word = true;
        }
    }
    None
}

fn find_top_eq(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut depth = 0i32;
    for (i, &c) in b.iter().enumerate() {
        match c {
            b'(' | b'[' | b'<' => depth += 1,
            b')' | b']' | b'>' => depth -= 1,
            b'=' if depth <= 0 => {
                let next = b.get(i + 1).copied();
                let prev = if i > 0 { b[i - 1] } else { b' ' };
                if next != Some(b'=') && !matches!(prev, b'=' | b'!' | b'<' | b'>') {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// `private final List<Item> items = new ArrayList<>()` → field.
/// Leading annotations inside the range (record components) are collected too.
fn field_from(
    src: &Src,
    code: &str,
    start: usize,
    end: usize,
    mut anns: Vec<JAnn>,
    needs_semicolon: bool,
) -> Option<JField> {
    let _ = needs_semicolon;
    let b = code.as_bytes();
    let mut i = start;
    let mut modifiers = vec![];
    loop {
        while i < end && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= end {
            return None;
        }
        if b[i] == b'@' {
            let mut j = i + 1;
            while j < end && (is_ident(b[j]) || b[j] == b'.') {
                j += 1;
            }
            let full = src.slice(i + 1, j);
            let name = full.rsplit('.').next().unwrap_or(full).to_string();
            let line = src.line(i);
            let mut k = j;
            while k < end && b[k].is_ascii_whitespace() {
                k += 1;
            }
            let mut args = String::new();
            if k < end && b[k] == b'(' {
                let close = matching(code, k).unwrap_or(end).min(end);
                args = collapse(src.slice(k + 1, close));
                j = (close + 1).min(end);
            }
            anns.push(JAnn { name, args, line });
            i = j;
            continue;
        }
        let mut j = i;
        while j < end && (is_ident(b[j]) || b[j] == b'-') {
            j += 1;
        }
        let w = &code[i..j];
        if MODIFIERS.contains(&w) {
            modifiers.push(w.to_string());
            i = j;
            continue;
        }
        break;
    }
    let eq = find_top_eq(&code[i..end]).map(|p| i + p);
    let decl_end = eq.unwrap_or(end);
    // Name: last identifier before `=` (skipping array brackets).
    let mut ne = decl_end;
    while ne > i && (b[ne - 1].is_ascii_whitespace() || b[ne - 1] == b']' || b[ne - 1] == b'[') {
        ne -= 1;
    }
    let mut ns = ne;
    while ns > i && is_ident(b[ns - 1]) {
        ns -= 1;
    }
    if ns == ne || ns <= i {
        return None;
    }
    let ty = collapse(src.slice(i, ns));
    // Multiple declarators (`int a, b`) are rare in entities: take the last name only when the type is sane.
    if ty.is_empty() || ty.contains(',') && !ty.contains('<') || ty.contains('(') {
        return None;
    }
    let name = src.slice(ns, ne).to_string();
    if !name.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') {
        return None;
    }
    let init = eq.map(|e| collapse(src.slice(e + 1, end))).filter(|s| !s.is_empty());
    Some(JField { name, ty, anns, modifiers, init, line: src.line(ns) })
}

/// Splits a leading type-parameter list off a type header:
/// `<E extends BaseEntity<D>, D> extends Base<E>` → ([`E`, `D`], `extends Base<E>`).
/// Their bounds must not be mistaken for the class's own `extends` clause.
pub fn split_type_params(header: &str) -> (Vec<String>, &str) {
    let h = header.trim_start();
    if !h.starts_with('<') {
        return (vec![], header);
    }
    let mut depth = 0i32;
    for (i, c) in h.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    let names = split_generic_list(&h[1..i])
                        .iter()
                        .filter_map(|p| p.split_whitespace().next().map(str::to_string))
                        .filter(|p| !p.is_empty())
                        .collect();
                    return (names, h[i + 1..].trim_start());
                }
            }
            _ => {}
        }
    }
    (vec![], header)
}

/// The return type in `public static final JpaRepository<E, UUID> getRepository`:
/// everything left once the modifiers and any method type parameters are dropped.
fn return_type(prefix: &str) -> String {
    let (_, rest) = split_type_params(prefix.trim());
    let mut rest = rest.trim();
    loop {
        let word = rest.split_whitespace().next().unwrap_or("");
        if word.is_empty() || !MODIFIERS.contains(&word) {
            break;
        }
        rest = rest[word.len()..].trim_start();
        // A generic method's parameters follow its modifiers: `public <T> T find()`.
        let (_, after) = split_type_params(rest);
        rest = after.trim_start();
    }
    rest.trim().to_string()
}

fn clause(header: &str, kw: &str) -> Vec<String> {
    let Some(p) = find_kw(header, kw) else { return vec![] };
    let rest = &header[p + kw.len()..];
    let stop = ["implements", "permits", "extends"]
        .iter()
        .filter(|k| **k != kw)
        .filter_map(|k| find_kw(rest, k))
        .min()
        .unwrap_or(rest.len());
    split_generic_list(&rest[..stop])
}

fn find_kw(s: &str, kw: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut from = 0;
    while let Some(p) = s[from..].find(kw) {
        let at = from + p;
        let before = at == 0 || !is_ident(b[at - 1]);
        let after = at + kw.len() >= b.len() || !is_ident(b[at + kw.len()]);
        if before && after {
            return Some(at);
        }
        from = at + 1;
    }
    None
}

/// `A<B, C>, D` → [`A<B, C>`, `D`].
pub fn split_generic_list(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '<' | '(' => depth += 1,
            '>' | ')' => depth -= 1,
            ',' if depth == 0 => {
                if !cur.trim().is_empty() {
                    out.push(cur.trim().to_string());
                }
                cur.clear();
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

pub fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `List<OrderItem>` → (`List`, [`OrderItem`]); qualified names reduce to their last segment.
pub fn generic(ty: &str) -> (String, Vec<String>) {
    let ty = ty.trim();
    match ty.find('<') {
        Some(p) if ty.ends_with('>') => {
            let base = simple(&ty[..p]);
            (base, split_generic_list(&ty[p + 1..ty.len() - 1]).iter().map(|a| simple(a)).collect())
        }
        _ => (simple(ty), vec![]),
    }
}

pub fn simple(ty: &str) -> String {
    let t = ty.trim().trim_start_matches("? extends ").trim();
    let t = t.split('<').next().unwrap_or(t).trim_end_matches("[]").trim();
    t.rsplit('.').next().unwrap_or(t).to_string()
}

/// Annotation argument: `name = "x"` / the unnamed value (`key` = "").
/// Returns the raw expression (quotes kept).
pub fn ann_arg(args: &str, keys: &[&str]) -> Option<String> {
    for part in super::raw::split_args(args) {
        let (k, v) = match part.split_once('=') {
            Some((k, v)) if k.trim().chars().all(|c| c.is_alphanumeric() || c == '_') && !k.trim().is_empty() => {
                (k.trim(), v.trim())
            }
            _ => ("", part.trim()),
        };
        if keys.contains(&k) {
            return Some(v.to_string());
        }
    }
    None
}

/// Resolves a constant string expression: literals, `Class.CONST`, `CONST`, and `+` concatenation.
pub fn resolve_str(expr: &str, consts: &HashMap<String, String>) -> Option<String> {
    let mut out = String::new();
    for part in super::raw::split_args(&expr.replace('+', ",")) {
        let p = part.trim();
        if p.starts_with('"') && p.ends_with('"') && p.len() >= 2 {
            out.push_str(&p[1..p.len() - 1]);
        } else if let Some(v) = consts.get(p).or_else(|| consts.get(p.rsplit('.').next().unwrap_or(p))) {
            out.push_str(v);
        } else {
            return None;
        }
    }
    Some(out)
}

/// `static final String X = "…"` constants, keyed `X` and `Class.X`.
pub fn constants(classes: &[JClass], out: &mut HashMap<String, String>) {
    // Two passes so constants built from other constants resolve.
    for _ in 0..2 {
        for c in classes {
            for f in c.fields.iter().filter(|f| f.is_static() || c.kind == "interface") {
                if simple(&f.ty) != "String" {
                    continue;
                }
                let Some(init) = &f.init else { continue };
                if let Some(v) = resolve_str(init, out) {
                    out.entry(format!("{}.{}", c.name, f.name)).or_insert(v.clone());
                    out.entry(f.name.clone()).or_insert(v);
                }
            }
        }
    }
}

pub fn new_src(path: &str, text: &str) -> Src {
    Src::new(path, text.to_string(), Style::CLike)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_fields_annotations_and_enums() {
        let text = r#"package x;
import jakarta.persistence.*;

@Entity
@Table(name = Names.ORDERS, uniqueConstraints = {@UniqueConstraint(columnNames = {"number"})})
public class Order extends BaseEntity implements Serializable {
    public static final String T = "orders";
    @Id @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;

    @Column(name = "order_number", nullable = false, length = 32)
    private String number;

    @Enumerated(EnumType.STRING)
    private OrderStatus status = OrderStatus.NEW;

    @OneToMany(mappedBy = "order", cascade = CascadeType.ALL)
    private List<OrderItem> items = new ArrayList<>();

    private Map<String, Integer> counts = Map.of("a", 1);

    public void pay() {
        if (status != OrderStatus.NEW) { throw new IllegalStateException("x"); }
        this.status = OrderStatus.PAID;
    }

    enum Inner { A, B }
}

enum OrderStatus { NEW("n"), @Deprecated PAID("p"); private final String code; OrderStatus(String c) { code = c; } }

interface OrderRepository extends JpaRepository<Order, Long> {
    @Modifying @Query("update Order o set o.status = :s")
    int setStatus(OrderStatus s);
    List<Order> findByStatus(OrderStatus status);
}

record Money(@NotNull Long cents, String currency) {}
"#;
        let src = new_src("Order.java", text);
        let mut out = vec![];
        parse_file(0, &src, &mut out);
        let order = out.iter().find(|c| c.name == "Order").unwrap();
        assert_eq!(order.extends, vec!["BaseEntity"]);
        assert_eq!(order.implements, vec!["Serializable"]);
        assert!(order.has("Entity"));
        assert_eq!(ann_arg(&order.ann("Table").unwrap().args, &["name"]).as_deref(), Some("Names.ORDERS"));
        let names: Vec<(&str, &str, u32)> =
            order.fields.iter().map(|f| (f.name.as_str(), f.ty.as_str(), f.line)).collect();
        assert_eq!(
            names,
            vec![
                ("T", "String", 7),
                ("id", "Long", 9),
                ("number", "String", 12),
                ("status", "OrderStatus", 15),
                ("items", "List<OrderItem>", 18),
                ("counts", "Map<String, Integer>", 20),
            ]
        );
        let status = &order.fields[3];
        assert_eq!(status.init.as_deref(), Some("OrderStatus.NEW"));
        assert_eq!(status.anns[0].args, "EnumType.STRING");
        assert_eq!(
            order.fields[1].anns.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
            vec!["Id", "GeneratedValue"]
        );
        assert!(order.methods.iter().any(|m| m.name == "pay"));
        let st = out.iter().find(|c| c.name == "OrderStatus").unwrap();
        assert_eq!(st.constants, vec![("NEW".to_string(), 30), ("PAID".to_string(), 30)]);
        assert!(out.iter().any(|c| c.name == "Inner"));
        let repo = out.iter().find(|c| c.name == "OrderRepository").unwrap();
        assert_eq!(generic(&repo.extends[0]), ("JpaRepository".into(), vec!["Order".into(), "Long".into()]));
        let set = repo.methods.iter().find(|m| m.name == "setStatus").unwrap();
        assert_eq!(set.anns.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), vec!["Modifying", "Query"]);
        assert!(repo.methods.iter().any(|m| m.name == "findByStatus"));
        let money = out.iter().find(|c| c.name == "Money").unwrap();
        assert_eq!(
            money.fields.iter().map(|f| (f.name.as_str(), f.anns.len())).collect::<Vec<_>>(),
            vec![("cents", 1), ("currency", 0)]
        );

        let mut consts = HashMap::new();
        consts.insert("Names.ORDERS".to_string(), "orders".to_string());
        assert_eq!(resolve_str("Names.ORDERS + \"_v2\"", &consts).as_deref(), Some("orders_v2"));
    }
}
