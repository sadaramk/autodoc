//! Intermediate facts collected per source before tables are merged.

use crate::scan::EvidenceRef;

pub type RawColumn = super::Column;

#[derive(Debug, Clone)]
pub struct RawEntity {
    /// Code name (class/struct/model) or the table name for DDL.
    pub name: String,
    /// Physical table name, lowercase.
    pub table: String,
    pub source: String,
    pub unit: Option<String>,
    pub columns: Vec<RawColumn>,
    pub relations: Vec<RawRelation>,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone)]
pub struct RawRelation {
    pub kind: String,
    /// Table name, or `@ClassName` when only the code name is known.
    pub target: String,
    pub via: String,
    pub evidence: EvidenceRef,
}

/// A closed set of values: SQL enum type or a code enum / literal union.
#[derive(Debug, Clone)]
pub struct RawEnum {
    pub name: String,
    /// (stored value, code member name)
    pub values: Vec<(String, String)>,
    pub evidence: EvidenceRef,
    pub unit: Option<String>,
    pub sql_type: bool,
}

/// `CHECK (col IN ('a', 'b'))`.
#[derive(Debug, Clone)]
pub struct RawCheck {
    pub table: String,
    pub column: String,
    pub values: Vec<String>,
    pub evidence: EvidenceRef,
}

/// A model field typed with a named enum: links the enum to `table.column`.
#[derive(Debug, Clone)]
pub struct RawEnumUse {
    pub table: String,
    pub column: String,
    pub enum_name: String,
    /// Declared default value (member or literal), if any.
    pub default: Option<String>,
}

pub fn snake_case(name: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = name.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev_lower = i > 0 && (chars[i - 1].is_lowercase() || chars[i - 1].is_ascii_digit());
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            if i > 0 && (prev_lower || (next_lower && chars[i - 1].is_uppercase())) {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(*c);
        }
    }
    out
}

pub fn line_ref(path: &str, line: u32) -> EvidenceRef {
    crate::source::line_ref(path, line)
}

/// Indentation width of a line.
pub fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Text inside the first balanced `(...)` starting at or after `from`.
pub fn paren_args(s: &str) -> Option<&str> {
    let open = s.find('(')?;
    let mut depth = 0;
    for (i, c) in s.char_indices().skip(open) {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[open + 1..i]);
                }
            }
            _ => {}
        }
    }
    Some(&s[open + 1..])
}

/// Splits call arguments at top-level commas.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut cur = String::new();
    for c in s.chars() {
        if let Some(q) = quote {
            cur.push(c);
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' | '`' => {
                quote = Some(c);
                cur.push(c);
            }
            '(' | '[' | '{' => {
                depth += 1;
                cur.push(c);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => out.push(std::mem::take(&mut cur).trim().to_string()),
            c => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// `key=value` / `key: value` argument lookup.
pub fn kwarg<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    args.iter().find_map(|a| {
        let (k, v) = a.split_once('=').or_else(|| a.split_once(':'))?;
        (k.trim() == key).then(|| v.trim())
    })
}

pub fn unquote(s: &str) -> String {
    s.trim().trim_matches(|c| c == '"' || c == '\'' || c == '`').to_string()
}
