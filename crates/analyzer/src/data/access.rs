//! Code sites that touch tables: SQL string literals and ORM calls.

use crate::extract::{FileFacts, SymbolKind};

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(String),
    Str(String),
    Sym(char),
}

fn tokens(s: &str) -> Vec<Tok> {
    let mut out = vec![];
    let cs: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '\'' {
            let mut v = String::new();
            i += 1;
            while i < cs.len() {
                if cs[i] == '\'' {
                    if cs.get(i + 1) == Some(&'\'') {
                        v.push('\'');
                        i += 2;
                        continue;
                    }
                    break;
                }
                v.push(cs[i]);
                i += 1;
            }
            i += 1;
            out.push(Tok::Str(v));
        } else if c.is_alphanumeric() || matches!(c, '_' | '"' | '`' | '$' | '.' | '[' | ']' | '%' | '?' | ':') {
            let mut w = String::new();
            while i < cs.len()
                && (cs[i].is_alphanumeric()
                    || matches!(cs[i], '_' | '"' | '`' | '$' | '.' | '[' | ']' | '%' | '?' | ':'))
            {
                w.push(cs[i]);
                i += 1;
            }
            let w = w.split("::").next().unwrap_or("").to_string();
            out.push(Tok::Word(w));
        } else {
            out.push(Tok::Sym(c));
            i += 1;
        }
    }
    out
}

fn name_of(w: &str) -> String {
    w.rsplit('.').next().unwrap_or(w).trim_matches(|c| c == '"' || c == '`' || c == '[' || c == ']').to_lowercase()
}

fn kw(t: &Tok, k: &str) -> bool {
    matches!(t, Tok::Word(w) if w.eq_ignore_ascii_case(k))
}

#[derive(Debug, Default, Clone)]
pub struct SqlStmt {
    /// `select`, `insert`, `update`, `delete`.
    pub kind: String,
    pub target: Option<String>,
    pub reads: Vec<String>,
    /// `SET col = 'literal'` (None when not a literal).
    pub sets: Vec<(String, Option<String>)>,
    /// `WHERE col = 'lit'` / `col IN ('a','b')`.
    pub where_values: Vec<(String, Vec<String>)>,
    pub where_text: Option<String>,
    /// INSERT column → literal value.
    pub insert_values: Vec<(String, Option<String>)>,
}

/// `WHERE` predicates joined by AND that are neither identity lookups (`id = $1`,
/// `id = %s`, `id = ?`) nor about `state_cols`.
pub fn business_conditions(where_text: &str, state_cols: &[String]) -> Option<String> {
    let upper = where_text.to_ascii_uppercase();
    let mut parts = vec![];
    let mut last = 0;
    let mut depth = 0;
    let bytes = upper.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b' ' if depth == 0 && upper[i..].starts_with(" AND ") => {
                parts.push(&where_text[last..i]);
                last = i + 5;
                i += 4;
            }
            _ => {}
        }
        i += 1;
    }
    parts.push(&where_text[last..]);
    let kept: Vec<&str> = parts
        .into_iter()
        .map(str::trim)
        .filter(|p| {
            let lhs = p.split(['=', '<', '>', ' ']).next().unwrap_or("");
            let col = name_of(lhs);
            let rhs = p.split_once('=').map(|x| x.1.trim()).unwrap_or("");
            let param =
                rhs.starts_with('$') || rhs == "?" || rhs == "%s" || rhs.starts_with(':') || rhs.starts_with('@');
            !(state_cols.contains(&col) || (param && p.contains('=') && !p.contains("<") && !p.contains(">")))
        })
        .collect();
    (!kept.is_empty()).then(|| kept.join(" AND "))
}

pub fn looks_like_sql(s: &str) -> bool {
    let u = s.trim_start().to_uppercase();
    (u.starts_with("SELECT ") && u.contains(" FROM "))
        || u.starts_with("INSERT INTO ")
        || (u.starts_with("UPDATE ") && u.contains(" SET "))
        || u.starts_with("DELETE FROM ")
        || (u.starts_with("WITH ") && u.contains(" AS ("))
}

pub fn parse_sql(s: &str) -> Option<SqlStmt> {
    if !looks_like_sql(s) {
        return None;
    }
    let t = tokens(s);
    let mut st = SqlStmt::default();
    let first = t.iter().position(|x| kw(x, "SELECT") || kw(x, "INSERT") || kw(x, "UPDATE") || kw(x, "DELETE"))?;
    // For WITH … the main statement follows the CTEs; pick the first DML verb at depth 0.
    let mut depth = 0;
    let mut main = first;
    for (i, x) in t.iter().enumerate() {
        match x {
            Tok::Sym('(') => depth += 1,
            Tok::Sym(')') => depth -= 1,
            _ if depth == 0 && (kw(x, "SELECT") || kw(x, "INSERT") || kw(x, "UPDATE") || kw(x, "DELETE")) => {
                main = i;
                break;
            }
            _ => {}
        }
    }
    let Tok::Word(verb) = &t[main] else { return None };
    st.kind = verb.to_lowercase();
    let word_after = |i: usize| -> Option<String> {
        let mut j = i;
        while j < t.len()
            && (kw(&t[j], "ONLY")
                || kw(&t[j], "INTO")
                || kw(&t[j], "FROM")
                || kw(&t[j], "IGNORE")
                || kw(&t[j], "LOW_PRIORITY"))
        {
            j += 1;
        }
        match t.get(j) {
            Some(Tok::Word(w)) => Some(name_of(w)),
            _ => None,
        }
    };
    for (i, x) in t.iter().enumerate() {
        if (kw(x, "FROM") || kw(x, "JOIN")) && !(i > 0 && kw(&t[i - 1], "DELETE")) {
            if let Some(Tok::Word(w)) = t.get(i + 1) {
                let n = name_of(w);
                if !n.is_empty()
                    && !["select", "lateral", "unnest", "only"].contains(&n.as_str())
                    && !n.starts_with('$')
                {
                    st.reads.push(n);
                }
            }
        }
    }
    match st.kind.as_str() {
        "insert" | "update" | "delete" => st.target = word_after(main + 1),
        _ => {}
    }
    if let Some(target) = &st.target {
        st.reads.retain(|r| r != target || st.kind == "select");
    }
    // CTE names are not tables.
    let ctes: Vec<String> = t
        .windows(3)
        .filter(|w| kw(&w[1], "AS") && w[2] == Tok::Sym('('))
        .filter_map(|w| if let Tok::Word(n) = &w[0] { Some(name_of(n)) } else { None })
        .collect();
    st.reads.retain(|r| !ctes.contains(r));
    st.reads.sort();
    st.reads.dedup();

    let upper_pos = |k: &str, from: usize| t.iter().enumerate().skip(from).find(|(_, x)| kw(x, k)).map(|(i, _)| i);
    if st.kind == "update" {
        if let Some(set) = upper_pos("SET", main) {
            let end = ["WHERE", "RETURNING", "FROM"].iter().filter_map(|k| upper_pos(k, set)).min().unwrap_or(t.len());
            let mut i = set + 1;
            while i + 2 < end + 1 && i < end {
                if let (Some(Tok::Word(c)), Some(Tok::Sym('='))) = (t.get(i), t.get(i + 1)) {
                    let v = match t.get(i + 2) {
                        Some(Tok::Str(v)) => Some(v.clone()),
                        _ => None,
                    };
                    st.sets.push((name_of(c), v));
                    i += 3;
                    while i < end && t[i] != Tok::Sym(',') {
                        i += 1;
                    }
                }
                i += 1;
            }
        }
    }
    if let Some(w) = upper_pos("WHERE", main) {
        let end =
            ["RETURNING", "ORDER", "LIMIT", "GROUP"].iter().filter_map(|k| upper_pos(k, w)).min().unwrap_or(t.len());
        let mut i = w + 1;
        while i < end {
            if let Tok::Word(c) = &t[i] {
                if t.get(i + 1) == Some(&Tok::Sym('=')) {
                    if let Some(Tok::Str(v)) = t.get(i + 2) {
                        st.where_values.push((name_of(c), vec![v.clone()]));
                    }
                } else if t.get(i + 1).is_some_and(|x| kw(x, "IN")) && t.get(i + 2) == Some(&Tok::Sym('(')) {
                    let vals: Vec<String> = t[i + 3..end]
                        .iter()
                        .take_while(|x| **x != Tok::Sym(')'))
                        .filter_map(|x| if let Tok::Str(v) = x { Some(v.clone()) } else { None })
                        .collect();
                    if !vals.is_empty() {
                        st.where_values.push((name_of(c), vals));
                    }
                }
            }
            i += 1;
        }
        let norm = s.split_whitespace().collect::<Vec<_>>().join(" ");
        let upper = norm.to_ascii_uppercase();
        if let Some(p) = upper.find("WHERE ") {
            let tail = &norm[p + 6..];
            let tail_u = &upper[p + 6..];
            let cut = [" RETURNING", " ORDER BY", " LIMIT", " GROUP BY"]
                .iter()
                .filter_map(|k| tail_u.find(k))
                .min()
                .unwrap_or(tail.len());
            st.where_text = Some(tail[..cut].trim().to_string());
        }
    }
    if st.kind == "insert" {
        let open = t.iter().skip(main).position(|x| *x == Tok::Sym('(')).map(|p| p + main);
        let values = upper_pos("VALUES", main);
        if let (Some(o), Some(v)) = (open, values) {
            if o < v {
                let cols: Vec<String> = t[o + 1..v]
                    .iter()
                    .filter_map(|x| if let Tok::Word(w) = x { Some(name_of(w)) } else { None })
                    .collect();
                let mut vals: Vec<Option<String>> = vec![];
                let mut depth = 0;
                let mut cur: Option<String> = None;
                let mut seen = false;
                for x in &t[v + 1..] {
                    match x {
                        Tok::Sym('(') => {
                            depth += 1;
                            if depth > 1 {
                                seen = true;
                            }
                        }
                        Tok::Sym(')') => {
                            depth -= 1;
                            if depth == 0 {
                                vals.push(cur.take());
                                break;
                            }
                        }
                        Tok::Sym(',') if depth == 1 => {
                            vals.push(if seen { None } else { cur.take() });
                            cur = None;
                            seen = false;
                        }
                        Tok::Str(s) if depth == 1 && !seen && cur.is_none() => cur = Some(s.clone()),
                        _ if depth >= 1 => seen = true,
                        _ => {}
                    }
                }
                if vals.len() == cols.len() {
                    st.insert_values = cols.into_iter().zip(vals).collect();
                }
            }
        }
    }
    Some(st)
}

/// The full literal starting on `line` whose text begins with `value` (facts truncate long strings).
pub fn full_literal(text: &str, line: u32, value: &str) -> String {
    let start: usize = text.split_inclusive('\n').take(line.saturating_sub(1) as usize).map(str::len).sum();
    let head: String = value.lines().next().unwrap_or("").chars().take(24).collect();
    let Some(rel) = text.get(start..).and_then(|t| t.find(head.as_str())) else { return value.to_string() };
    let begin = start + rel;
    let before = text[..begin].trim_end_matches(['#', 'r', 'f', 'b']);
    let delim = if before.ends_with("\"\"\"") {
        "\"\"\""
    } else if before.ends_with("'''") {
        "'''"
    } else if before.ends_with('`') {
        "`"
    } else if before.ends_with('"') {
        "\""
    } else if before.ends_with('\'') {
        "'"
    } else {
        return value.to_string();
    };
    match text[begin..].find(delim) {
        Some(end) if end >= value.len().min(end) => text[begin..begin + end].to_string(),
        _ => value.to_string(),
    }
}

/// Innermost function/method containing `line`.
pub fn enclosing(facts: &FileFacts, line: u32) -> Option<(String, u32, u32)> {
    facts
        .symbols
        .iter()
        .filter(|s| {
            matches!(s.kind, SymbolKind::Function | SymbolKind::Method) && s.start_line <= line && line <= s.end_line
        })
        .min_by_key(|s| s.end_line - s.start_line)
        .map(|s| (s.name.clone(), s.start_line, s.end_line))
}

pub fn is_word_at(line: &str, word: &str) -> bool {
    let b = line.as_bytes();
    let mut from = 0;
    while let Some(p) = line[from..].find(word) {
        let s = from + p;
        let e = s + word.len();
        let before = s == 0 || !(b[s - 1].is_ascii_alphanumeric() || b[s - 1] == b'_');
        let after = e >= b.len() || !(b[e].is_ascii_alphanumeric() || b[e] == b'_');
        if before && after {
            return true;
        }
        from = s + 1;
    }
    false
}

fn lower_camel(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// ORM access on one line of code for an entity with code `name` and `table`:
/// Some(true) = write, Some(false) = read.
pub fn orm_access(line: &str, name: &str, table: &str, source: &str) -> Option<bool> {
    let l = line.trim();
    if l.starts_with("class ")
        || l.starts_with("import ")
        || l.starts_with("from ")
        || l.starts_with("//")
        || l.starts_with('#')
        || l.starts_with("type ")
        || l.starts_with("use ")
    {
        return None;
    }
    const WRITE: &[&str] = &[
        ".create(",
        ".createMany(",
        ".update(",
        ".updateMany(",
        ".upsert(",
        ".delete(",
        ".deleteMany(",
        ".save(",
        ".insert(",
        ".add(",
        ".bulk_create(",
        ".get_or_create(",
        ".update_or_create(",
        ".merge(",
        ".remove(",
        "Create(",
        "Save(",
        "Updates(",
        "Update(",
        "Delete(",
        "insert_into(",
        "diesel::update(",
        "diesel::delete(",
        "insert(",
        "update(",
        "delete(",
    ];
    const READ: &[&str] = &[
        "select(",
        ".get(",
        ".query(",
        ".filter(",
        ".find",
        ".first(",
        ".all(",
        ".exclude(",
        ".count(",
        ".aggregate(",
        ".groupBy(",
        "Find(",
        "First(",
        "Take(",
        "Where(",
        ".load(",
        ".get_result",
        "query_as",
        ".from(",
        ".scalars(",
        ".exec(",
    ];
    let has = |ps: &[&str]| ps.iter().any(|p| l.contains(p));
    let mentions = match source {
        s if s.contains("prisma") => l.contains(&format!(".{}.", lower_camel(name))),
        s if s.contains("diesel") => l.contains(&format!("{table}::")) || is_word_at(l, table),
        s if s.contains("drizzle") => is_word_at(l, name),
        s if s.contains("gorm") => {
            l.contains(&format!("&{name}{{")) || l.contains(&format!("[]{name}")) || l.contains(&format!(".{name}{{"))
        }
        s if s.contains("django") => l.contains(&format!("{name}.objects")),
        _ => {
            is_word_at(l, name)
                && (l.contains(&format!("{name}("))
                    || l.contains(&format!("({name}"))
                    || l.contains(&format!("{name})"))
                    || l.contains(&format!("{name},"))
                    || l.contains(&format!("<{name}>"))
                    || l.contains(&format!(", {name}")))
        }
    };
    if !mentions {
        return None;
    }
    if source.contains("drizzle") {
        return if l.contains(&format!("insert({name}"))
            || l.contains(&format!("update({name}"))
            || l.contains(&format!("delete({name}"))
        {
            Some(true)
        } else if l.contains(&format!("from({name}")) {
            Some(false)
        } else {
            None
        };
    }
    if has(WRITE) {
        Some(true)
    } else if has(READ) {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_tables_and_values() {
        let st = parse_sql(
            "UPDATE orders SET status = 'shipped', tracking_number = %s WHERE id = %s AND status = 'paid' RETURNING id",
        )
        .unwrap();
        assert_eq!((st.kind.as_str(), st.target.as_deref()), ("update", Some("orders")));
        assert_eq!(st.sets, vec![("status".into(), Some("shipped".into())), ("tracking_number".into(), None)]);
        assert_eq!(st.where_values, vec![("status".into(), vec!["paid".into()])]);
        let ins = parse_sql("INSERT INTO orders (total_cents, status, items) VALUES ($1, 'pending', $2)").unwrap();
        assert_eq!(ins.insert_values[1], ("status".into(), Some("pending".into())));
        let sel = parse_sql("WITH recent AS (SELECT * FROM payments) SELECT o.id FROM \"public\".orders o JOIN recent r ON r.order_id = o.id").unwrap();
        assert_eq!(sel.reads, vec!["orders", "payments"]);
        assert!(parse_sql("please update your profile").is_none());
    }

    #[test]
    fn guards_keep_business_conditions_only() {
        let cols = vec!["status".to_string()];
        assert_eq!(business_conditions("id = $1 AND status = 'pending'", &cols), None);
        assert_eq!(
            business_conditions("id = %s AND status = 'paid' AND total_cents > 0", &cols).as_deref(),
            Some("total_cents > 0")
        );
        assert_eq!(
            business_conditions("o.id = ? AND (paid_at IS NULL OR retries < 3)", &cols).as_deref(),
            Some("(paid_at IS NULL OR retries < 3)")
        );
    }
}
