//! SQL DDL: CREATE TABLE / CREATE TYPE … AS ENUM / ALTER TABLE, applied in
//! migration (file name) order.

use super::raw::{RawCheck, RawColumn, RawEntity, RawEnum, RawRelation};
use crate::source::line_ref;

/// Blanks out `--` and `/* */` comments and string contents are kept, so
/// byte offsets (and therefore line numbers) stay aligned with the source.
fn strip_comments(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    let mut in_str = false;
    while i < b.len() {
        let c = b[i];
        if in_str {
            out.push(c);
            if c == b'\'' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == b'\'' {
            in_str = true;
            out.push(c);
            i += 1;
        } else if c == b'-' && b.get(i + 1) == Some(&b'-') {
            while i < b.len() && b[i] != b'\n' {
                out.push(b' ');
                i += 1;
            }
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            while i < b.len() && !(b[i] == b'*' && b.get(i + 1) == Some(&b'/')) {
                out.push(if b[i] == b'\n' { b'\n' } else { b' ' });
                i += 1;
            }
            out.extend_from_slice(b"  ");
            i += 2;
        } else {
            out.push(c);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Keeps only string-literal contents of source code (everything else blanked,
/// newlines kept, a `;` where each literal closes) so embedded DDL parses with true line numbers.
pub fn string_contents(code: &str, hash_comments: bool) -> String {
    let b: Vec<char> = code.chars().collect();
    let mut out = String::with_capacity(code.len());
    let mut i = 0;
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    while i < b.len() {
        let c = b[i];
        let triple = i + 2 < b.len() && (c == '"' || c == '\'') && b[i + 1] == c && b[i + 2] == c;
        if c == '/' && b.get(i + 1) == Some(&'/') || (hash_comments && c == '#') {
            while i < b.len() && b[i] != '\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        if matches!(c, '"' | '\'' | '`') {
            let close_len = if triple { 3 } else { 1 };
            for _ in 0..close_len {
                out.push(' ');
            }
            i += close_len;
            while i < b.len() {
                if b[i] == '\\' && i + 1 < b.len() {
                    out.push(' ');
                    out.push(blank(b[i + 1]));
                    i += 2;
                    continue;
                }
                let closes =
                    if triple { i + 2 < b.len() && b[i] == c && b[i + 1] == c && b[i + 2] == c } else { b[i] == c };
                if closes {
                    out.push(';');
                    for _ in 1..close_len {
                        out.push(' ');
                    }
                    i += close_len;
                    break;
                }
                out.push(b[i]);
                i += 1;
            }
            continue;
        }
        out.push(blank(c));
        i += 1;
    }
    out
}

/// Column identifier: quoted keeps its case, unquoted folds to lowercase.
fn col_ident(raw: &str) -> String {
    let raw = raw.trim().trim_end_matches(',');
    if raw.starts_with('"') {
        ident(raw)
    } else {
        ident(raw).to_lowercase()
    }
}

pub fn line_at(text: &str, offset: usize) -> u32 {
    text[..offset.min(text.len())].bytes().filter(|b| *b == b'\n').count() as u32 + 1
}

/// Unquotes and drops a schema prefix: `"public"."Orders"` → `Orders`.
pub fn ident(raw: &str) -> String {
    let last = split_top(raw.trim(), '.').pop().unwrap_or_default();
    last.trim().trim_matches(|c| c == '"' || c == '`' || c == '[' || c == ']').to_string()
}

/// Splits on `sep` outside parentheses and quotes.
fn split_top(s: &str, sep: char) -> Vec<String> {
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
            '\'' | '"' | '`' => {
                quote = Some(c);
                cur.push(c);
            }
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' => {
                depth -= 1;
                cur.push(c);
            }
            c if c == sep && depth == 0 => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Matching close paren for the `(` at `open`.
fn close_paren(s: &str, open: usize) -> Option<usize> {
    let mut depth = 0;
    let mut quote: Option<u8> = None;
    for (i, c) in s.bytes().enumerate().skip(open) {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            b'\'' | b'"' | b'`' => quote = Some(c),
            b'(' => depth += 1,
            b')' => {
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

fn words_upper(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_uppercase()
}

/// Quoted literals in a list: `('a', 'b')` → [a, b].
pub fn quoted_values(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find('\'') {
        let after = &rest[i + 1..];
        let Some(j) = after.find('\'') else { break };
        out.push(after[..j].to_string());
        rest = &after[j + 1..];
    }
    out
}

/// `col IN ('a','b')` inside a CHECK expression.
fn check_in(expr: &str) -> Option<(String, Vec<String>)> {
    let upper = expr.to_uppercase();
    let at = upper.find(" IN ")?;
    let col = expr[..at].trim().trim_start_matches('(').trim();
    let col = ident(col.rsplit(|c: char| c.is_whitespace() || c == '(').next().unwrap_or(col));
    let values = quoted_values(&expr[at..]);
    (values.len() >= 2 && !col.is_empty()).then_some((col.to_lowercase(), values))
}

fn references(clause: &str) -> Option<String> {
    let upper = clause.to_uppercase();
    let at = upper.find("REFERENCES")?;
    let rest = clause[at + "REFERENCES".len()..].trim();
    let (table, cols) = match rest.find('(') {
        Some(p) => {
            let close = close_paren(rest, p)?;
            (ident(&rest[..p]), rest[p + 1..close].to_string())
        }
        None => (ident(rest.split_whitespace().next().unwrap_or("")), "id".to_string()),
    };
    let col = ident(cols.split(',').next().unwrap_or("id"));
    Some(format!("{}.{}", table.to_lowercase(), col.to_lowercase()))
}

#[derive(Default)]
pub struct SqlOutput {
    pub entities: Vec<RawEntity>,
    pub enums: Vec<RawEnum>,
    pub checks: Vec<RawCheck>,
}

pub fn parse_file(path: &str, text: &str, out: &mut SqlOutput) {
    let clean = strip_comments(text);
    for stmt in statements(&clean) {
        let (start, body) = stmt;
        let upper = words_upper(&body[..body.len().min(120)]);
        if upper.starts_with("CREATE TABLE")
            || upper.starts_with("CREATE UNLOGGED TABLE")
            || upper.starts_with("CREATE TEMP")
        {
            if upper.starts_with("CREATE TEMP") {
                continue;
            }
            create_table(path, text, start, body, out);
        } else if upper.starts_with("CREATE TYPE") && upper.contains(" AS ENUM") {
            let name_part = body.trim_start()["CREATE TYPE".len()..].trim_start();
            let name = ident(name_part.split_whitespace().next().unwrap_or(""));
            let values_at = body.find('(').unwrap_or(0);
            let values = quoted_values(&body[values_at..]);
            let line = line_at(text, start + body.len() - body.trim_start().len());
            out.enums.push(RawEnum {
                name: name.clone(),
                values: values.iter().map(|v| (v.clone(), v.clone())).collect(),
                evidence: line_ref(path, line),
                unit: None,
                sql_type: true,
            });
        } else if upper.starts_with("ALTER TABLE") {
            alter_table(path, text, start, body, out);
        } else if upper.starts_with("DROP TABLE") {
            let names = body.trim_start()["DROP TABLE".len()..].trim_start();
            let names = names.strip_prefix("IF EXISTS").or_else(|| names.strip_prefix("if exists")).unwrap_or(names);
            for n in split_top(names, ',') {
                let t = ident(n.split_whitespace().next().unwrap_or("")).to_lowercase();
                out.entities.retain(|e| e.table != t);
            }
        }
    }
}

/// Statements split on top-level `;`, with their byte offset.
fn statements(clean: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut quote = false;
    let mut last = 0;
    for (i, c) in clean.bytes().enumerate() {
        if c == b'\'' {
            quote = !quote;
        } else if c == b';' && !quote {
            out.push((last, &clean[last..i]));
            last = i + 1;
        }
    }
    if !clean[last..].trim().is_empty() {
        out.push((last, &clean[last..]));
    }
    out.into_iter()
        .map(|(s, b)| {
            let lead = b.len() - b.trim_start().len();
            (s + lead, b.trim_start())
        })
        .collect()
}

fn create_table(path: &str, text: &str, start: usize, body: &str, out: &mut SqlOutput) {
    // `CREATE TABLE x AS SELECT …`: the name is known, the columns come from later statements.
    let words: Vec<&str> = body.split_whitespace().take(8).collect();
    if let Some(p) = words.iter().position(|w| w.eq_ignore_ascii_case("AS")) {
        if p >= 3 && !words[..p].iter().any(|w| w.contains('(')) {
            let name = ident(words[p - 1]);
            let table = name.to_lowercase();
            out.entities.retain(|e| e.table != table);
            out.entities.push(RawEntity {
                name,
                table,
                source: "sql-ddl".into(),
                unit: None,
                columns: vec![],
                relations: vec![],
                evidence: line_ref(path, line_at(text, start)),
            });
            return;
        }
    }
    let Some(open) = body.find('(') else { return };
    let Some(close) = close_paren(body, open) else { return };
    let head = &body[..open];
    let mut name_tokens: Vec<&str> = head.split_whitespace().collect();
    let name = ident(name_tokens.pop().unwrap_or(""));
    if name.is_empty() {
        return;
    }
    let table = name.to_lowercase();
    let mut entity = RawEntity {
        name: name.clone(),
        table: table.clone(),
        source: "sql-ddl".into(),
        unit: None,
        columns: vec![],
        relations: vec![],
        evidence: line_ref(path, line_at(text, start)),
    };
    let inner = &body[open + 1..close];
    let mut offset = start + open + 1;
    let mut table_pks: Vec<String> = vec![];
    for item in split_top(inner, ',') {
        let lead = item.len() - item.trim_start().len();
        let item_start = offset + lead;
        offset += item.len() + 1;
        let def = item.trim();
        if def.is_empty() {
            continue;
        }
        let line = line_at(text, item_start);
        let upper = words_upper(def);
        let constraint_body = if upper.starts_with("CONSTRAINT ") {
            // CONSTRAINT name <kind> …
            let mut it = def.splitn(3, char::is_whitespace);
            it.next();
            it.next();
            it.next().unwrap_or("").trim().to_string()
        } else {
            def.to_string()
        };
        let cu = words_upper(&constraint_body);
        if cu.starts_with("PRIMARY KEY") {
            if let (Some(p), true) = (constraint_body.find('('), true) {
                if let Some(c) = close_paren(&constraint_body, p) {
                    table_pks.extend(constraint_body[p + 1..c].split(',').map(|x| ident(x).to_lowercase()));
                }
            }
        } else if cu.starts_with("FOREIGN KEY") {
            let p = constraint_body.find('(').unwrap_or(0);
            let c = close_paren(&constraint_body, p).unwrap_or(p);
            let cols: Vec<String> = constraint_body[p + 1..c].split(',').map(|x| ident(x).to_lowercase()).collect();
            if let Some(r) = references(&constraint_body[c..]) {
                for col in &cols {
                    if let Some(existing) = entity.columns.iter_mut().find(|x| &x.name == col) {
                        existing.references = Some(r.clone());
                    }
                }
                entity.relations.push(RawRelation {
                    kind: "many-to-one".into(),
                    target: r.split('.').next().unwrap_or("").to_string(),
                    via: cols.join(", "),
                    evidence: line_ref(path, line),
                });
            }
        } else if cu.starts_with("UNIQUE") {
            if let Some(p) = constraint_body.find('(') {
                if let Some(c) = close_paren(&constraint_body, p) {
                    let cols: Vec<String> =
                        constraint_body[p + 1..c].split(',').map(|x| ident(x).to_lowercase()).collect();
                    if cols.len() == 1 {
                        if let Some(existing) = entity.columns.iter_mut().find(|x| x.name == cols[0]) {
                            existing.unique = true;
                        }
                    }
                }
            }
        } else if cu.starts_with("CHECK") {
            if let Some((col, values)) = check_in(&constraint_body) {
                out.checks.push(RawCheck {
                    table: table.clone(),
                    column: col.clone(),
                    values,
                    evidence: line_ref(path, line),
                });
            }
            let expr = constraint_body.trim_start()[5..].trim().to_string();
            if let Some(col) = entity.columns.iter_mut().find(|c| expr.to_lowercase().contains(&c.name)) {
                col.constraints.push(format!("CHECK {expr}"));
            }
        } else if cu.starts_with("EXCLUDE") || cu.starts_with("LIKE ") {
            continue;
        } else {
            let col = column_def(path, line, def, &table, out);
            if let Some(r) = &col.references {
                entity.relations.push(RawRelation {
                    kind: "many-to-one".into(),
                    target: r.split('.').next().unwrap_or("").to_string(),
                    via: col.name.clone(),
                    evidence: line_ref(path, line),
                });
            }
            entity.columns.push(col);
        }
    }
    let single = table_pks.len() == 1;
    for pk in table_pks {
        if let Some(c) = entity.columns.iter_mut().find(|c| c.name == pk) {
            c.primary_key = true;
            c.nullable = false;
            c.unique = single;
        }
    }
    out.entities.push(entity);
}

fn column_def(path: &str, line: u32, def: &str, table: &str, out: &mut SqlOutput) -> RawColumn {
    let mut parts = def.splitn(2, char::is_whitespace);
    let raw_name = parts.next().unwrap_or("");
    // Postgres folds unquoted identifiers; quoted ones keep their case.
    let name = if raw_name.starts_with('"') { ident(raw_name) } else { ident(raw_name).to_lowercase() };
    let rest_norm = parts.next().unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ");
    let rest = rest_norm.as_str();
    // ASCII-only uppercasing keeps byte offsets aligned with `rest`.
    let upper = rest.to_ascii_uppercase();
    // Type: tokens until the first constraint keyword (keeping parenthesised args).
    const KEYWORDS: &[&str] = &[
        "NOT",
        "NULL",
        "PRIMARY",
        "UNIQUE",
        "DEFAULT",
        "REFERENCES",
        "CHECK",
        "CONSTRAINT",
        "GENERATED",
        "COLLATE",
        "AUTO_INCREMENT",
        "AUTOINCREMENT",
        "IDENTITY",
        "ON",
    ];
    let mut type_name = String::new();
    for tok in split_top(rest, ' ') {
        let t = tok.trim();
        if t.is_empty() {
            continue;
        }
        if KEYWORDS.contains(&t.to_uppercase().as_str()) {
            break;
        }
        if !type_name.is_empty() {
            type_name.push(' ');
        }
        type_name.push_str(t);
    }
    let primary_key = upper.contains("PRIMARY KEY");
    let not_null = upper.contains("NOT NULL") || primary_key;
    // `GENERATED BY DEFAULT AS IDENTITY` is an identity column, not a `DEFAULT` value.
    let identity = upper.contains("AS IDENTITY");
    let default =
        upper.match_indices("DEFAULT ").map(|(i, _)| i).find(|&i| !upper[..i].trim_end().ends_with(" BY")).map(|i| {
            let v = rest[i + 8..].trim();
            let end = split_top(v, ' ').first().map(|s| s.len()).unwrap_or(v.len());
            v[..end].trim().trim_matches('\'').to_string()
        });
    let default = if identity && default.is_none() { Some("generated".to_string()) } else { default };
    let mut constraints = vec![];
    // MySQL inline `ENUM ('a', 'b')` is a closed value set like a CHECK … IN.
    if type_name.to_ascii_uppercase().starts_with("ENUM") {
        let values = quoted_values(&type_name);
        if values.len() >= 2 {
            out.checks.push(RawCheck {
                table: table.to_string(),
                column: name.clone(),
                values,
                evidence: line_ref(path, line),
            });
        }
    }
    if let Some(i) = upper.find("CHECK") {
        let from = &rest[i..];
        if let Some(p) = from.find('(') {
            if let Some(c) = close_paren(from, p) {
                let expr = &from[p..=c];
                constraints.push(format!("CHECK {expr}"));
                if let Some((col, values)) = check_in(&expr[1..expr.len() - 1]) {
                    out.checks.push(RawCheck {
                        table: table.to_string(),
                        column: col,
                        values,
                        evidence: line_ref(path, line),
                    });
                }
            }
        }
    }
    RawColumn {
        name,
        type_name,
        primary_key,
        nullable: !not_null,
        unique: upper.contains("UNIQUE") || primary_key,
        references: references(rest),
        default,
        constraints,
        evidence: line_ref(path, line),
    }
}

fn alter_table(path: &str, text: &str, start: usize, body: &str, out: &mut SqlOutput) {
    let rest = body.trim_start()["ALTER TABLE".len()..].trim_start();
    let rest = rest.strip_prefix("IF EXISTS").or_else(|| rest.strip_prefix("if exists")).unwrap_or(rest).trim_start();
    let rest = rest.strip_prefix("ONLY ").unwrap_or(rest).trim_start();
    let name_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let table = ident(&rest[..name_end]).to_lowercase();
    let actions = &rest[name_end..];
    let action_offset = start + (body.len() - body.trim_start().len()) + (body.trim_start().len() - actions.len());
    let mut offset = action_offset;
    for action in split_top(actions, ',') {
        let lead = action.len() - action.trim_start().len();
        let line = line_at(text, offset + lead);
        offset += action.len() + 1;
        let a = action.trim();
        let upper = words_upper(a);
        let Some(entity_idx) = out.entities.iter().position(|e| e.table == table) else { continue };
        if upper.starts_with("ADD PRIMARY KEY") {
            if let Some(p) = a.find('(') {
                let c = close_paren(a, p).unwrap_or(a.len() - 1);
                let cols: Vec<String> = a[p + 1..c].split(',').map(col_ident).collect();
                for col in out.entities[entity_idx].columns.iter_mut().filter(|x| cols.contains(&x.name)) {
                    col.primary_key = true;
                    col.nullable = false;
                    col.unique = cols.len() == 1;
                }
            }
        } else if upper.starts_with("ADD UNIQUE") {
            if let Some(p) = a.find('(') {
                let c = close_paren(a, p).unwrap_or(a.len() - 1);
                let cols: Vec<String> = a[p + 1..c].split(',').map(col_ident).collect();
                if cols.len() == 1 {
                    if let Some(col) = out.entities[entity_idx].columns.iter_mut().find(|x| x.name == cols[0]) {
                        col.unique = true;
                    }
                }
            }
        } else if upper.starts_with("ADD CONSTRAINT")
            || upper.starts_with("ADD FOREIGN KEY")
            || upper.starts_with("ADD CHECK")
        {
            let body = if upper.starts_with("ADD CONSTRAINT") {
                a.splitn(4, char::is_whitespace).nth(3).unwrap_or("").to_string()
            } else {
                a[4..].to_string()
            };
            let bu = words_upper(&body);
            if bu.starts_with("FOREIGN KEY") {
                let p = body.find('(').unwrap_or(0);
                let c = close_paren(&body, p).unwrap_or(p);
                let cols: Vec<String> = body[p + 1..c].split(',').map(|x| ident(x).to_lowercase()).collect();
                if let Some(r) = references(&body[c..]) {
                    let e = &mut out.entities[entity_idx];
                    for col in &cols {
                        if let Some(existing) = e.columns.iter_mut().find(|x| &x.name == col) {
                            existing.references = Some(r.clone());
                        }
                    }
                    e.relations.push(RawRelation {
                        kind: "many-to-one".into(),
                        target: r.split('.').next().unwrap_or("").into(),
                        via: cols.join(", "),
                        evidence: line_ref(path, line),
                    });
                }
            } else if bu.starts_with("CHECK") {
                if let Some((col, values)) = check_in(&body) {
                    out.checks.push(RawCheck {
                        table: table.clone(),
                        column: col,
                        values,
                        evidence: line_ref(path, line),
                    });
                }
            } else if bu.starts_with("UNIQUE") {
                if let Some(p) = body.find('(') {
                    let c = close_paren(&body, p).unwrap_or(p);
                    let cols: Vec<String> = body[p + 1..c].split(',').map(|x| ident(x).to_lowercase()).collect();
                    if cols.len() == 1 {
                        if let Some(col) = out.entities[entity_idx].columns.iter_mut().find(|x| x.name == cols[0]) {
                            col.unique = true;
                        }
                    }
                }
            }
        } else if upper.starts_with("ADD ") {
            let def = a[4..].trim_start();
            let def = if words_upper(def).starts_with("COLUMN") { def[6..].trim_start() } else { def };
            let def = if words_upper(def).starts_with("IF NOT EXISTS") { def[13..].trim_start() } else { def };
            let col = column_def(path, line, def, &table, out);
            let e = &mut out.entities[entity_idx];
            if let Some(r) = &col.references {
                e.relations.push(RawRelation {
                    kind: "many-to-one".into(),
                    target: r.split('.').next().unwrap_or("").into(),
                    via: col.name.clone(),
                    evidence: line_ref(path, line),
                });
            }
            e.columns.retain(|c| c.name != col.name);
            e.columns.push(col);
        } else if upper.starts_with("DROP COLUMN") {
            let name = col_ident(a.split_whitespace().last().unwrap_or(""));
            out.entities[entity_idx].columns.retain(|c| c.name != name);
            out.entities[entity_idx].relations.retain(|r| r.via != name);
        } else if upper.starts_with("RENAME TO ") {
            let new_name = ident(a.split_whitespace().nth(2).unwrap_or(""));
            let new_table = new_name.to_lowercase();
            if !new_table.is_empty() {
                out.entities.retain(|e| e.table != new_table);
                let idx = out.entities.iter().position(|e| e.table == table).unwrap();
                let e = &mut out.entities[idx];
                e.name = new_name;
                e.table = new_table.clone();
                e.evidence = line_ref(path, line);
                for other in out.entities.iter_mut() {
                    for c in other.columns.iter_mut() {
                        if let Some(r) = c.references.as_mut().filter(|r| r.split('.').next() == Some(table.as_str())) {
                            *r = format!("{new_table}.{}", r.split_once('.').map(|x| x.1).unwrap_or("id"));
                        }
                    }
                    for r in other.relations.iter_mut().filter(|r| r.target == table) {
                        r.target = new_table.clone();
                    }
                }
                for c in out.checks.iter_mut().filter(|c| c.table == table) {
                    c.table = new_table.clone();
                }
            }
            return;
        } else if upper.starts_with("RENAME COLUMN ") || (upper.starts_with("RENAME ") && upper.contains(" TO ")) {
            let words: Vec<&str> = a.split_whitespace().collect();
            let skip = if upper.starts_with("RENAME COLUMN ") { 2 } else { 1 };
            if let (Some(old), Some(new)) = (words.get(skip), words.get(skip + 2)) {
                let (old, new) = (col_ident(old), col_ident(new));
                let e = &mut out.entities[entity_idx];
                if let Some(c) = e.columns.iter_mut().find(|c| c.name == old) {
                    c.name = new.clone();
                }
                for r in e.relations.iter_mut().filter(|r| r.via == old) {
                    r.via = new.clone();
                }
                for c in out.checks.iter_mut().filter(|c| c.table == table && c.column == old) {
                    c.column = new.clone();
                }
            }
        } else if upper.starts_with("ALTER COLUMN ") || upper.starts_with("ALTER ") {
            let words: Vec<&str> = a.split_whitespace().collect();
            let skip = if upper.starts_with("ALTER COLUMN ") { 2 } else { 1 };
            let Some(name) = words.get(skip).map(|w| col_ident(w)) else { continue };
            let rest_u = words[skip + 1..].join(" ").to_ascii_uppercase();
            if let Some(c) = out.entities[entity_idx].columns.iter_mut().find(|c| c.name == name) {
                if rest_u.starts_with("SET NOT NULL") {
                    c.nullable = false;
                } else if rest_u.starts_with("DROP NOT NULL") {
                    c.nullable = true;
                } else if rest_u.starts_with("DROP DEFAULT") {
                    c.default = None;
                } else if rest_u.starts_with("SET DEFAULT ") {
                    c.default = words.get(skip + 3).map(|v| v.trim_matches('\'').to_string());
                } else if rest_u.starts_with("TYPE ") || rest_u.starts_with("SET DATA TYPE ") {
                    let at = if rest_u.starts_with("TYPE ") { skip + 2 } else { skip + 4 };
                    if let Some(t) = words.get(at) {
                        c.type_name = t.to_string();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_alter_and_enums() {
        let sql = "-- orders\nCREATE TYPE order_status AS ENUM ('pending', 'paid');\n\nCREATE TABLE IF NOT EXISTS public.\"orders\" (\n  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),\n  status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','paid','shipped')),\n  total_cents integer NOT NULL CHECK (total_cents > 0),\n  note varchar(255)\n);\nCREATE TABLE payments (\n  id bigserial,\n  order_id uuid NOT NULL,\n  charge_id text,\n  PRIMARY KEY (id),\n  CONSTRAINT fk_order FOREIGN KEY (order_id) REFERENCES orders(id),\n  UNIQUE (charge_id)\n);\nALTER TABLE payments ADD COLUMN refunded boolean NOT NULL DEFAULT false;\n";
        let mut out = SqlOutput::default();
        parse_file("m.sql", sql, &mut out);
        assert_eq!(out.enums[0].values.len(), 2);
        let orders = &out.entities[0];
        assert_eq!(orders.table, "orders");
        assert_eq!(orders.evidence.start_line, 4);
        let status = orders.columns.iter().find(|c| c.name == "status").unwrap();
        assert_eq!(
            (status.type_name.as_str(), status.nullable, status.default.as_deref(), status.evidence.start_line),
            ("text", false, Some("pending"), 6)
        );
        assert!(orders.columns[0].primary_key);
        assert_eq!(orders.columns.iter().find(|c| c.name == "note").unwrap().type_name, "varchar(255)");
        assert_eq!(out.checks[0].values, vec!["pending", "paid", "shipped"]);
        let payments = &out.entities[1];
        assert!(payments.columns[0].primary_key);
        let order_id = payments.columns.iter().find(|c| c.name == "order_id").unwrap();
        assert_eq!(order_id.references.as_deref(), Some("orders.id"));
        assert!(payments.columns.iter().find(|c| c.name == "charge_id").unwrap().unique);
        assert_eq!(payments.relations[0].target, "orders");
        let refunded = payments.columns.iter().find(|c| c.name == "refunded").unwrap();
        assert_eq!(refunded.evidence.start_line, 18);
    }
}
