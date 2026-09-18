//! Alembic migrations: `op.create_table` / `op.add_column` / `op.drop_table`.

use super::raw::*;
use super::sql::line_at;

fn call_at(text: &str, from: usize) -> Option<&str> {
    let open = from + text[from..].find('(')?;
    let mut depth = 0;
    let mut quote: Option<char> = None;
    for (i, c) in text[open..].char_indices() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[open + 1..open + i]);
                }
            }
            _ => {}
        }
    }
    None
}

fn column(path: &str, line: u32, args: &str) -> Option<RawColumn> {
    let a = split_args(args);
    let name = unquote(a.first()?);
    let ty = a
        .get(1)
        .map(|t| t.trim_start_matches("sa.").trim_start_matches("sqlmodel.sql.sqltypes.").to_string())
        .unwrap_or_default();
    let mut constraints = vec![];
    if let Some(len) =
        paren_args(&ty).and_then(|p| split_args(p).iter().find_map(|x| x.strip_prefix("length=").map(str::to_string)))
    {
        constraints.push(format!("max length {len}"));
    }
    let references = a
        .iter()
        .find(|x| x.starts_with("sa.ForeignKey("))
        .and_then(|x| paren_args(x))
        .map(|r| unquote(split_args(r).first().map(String::as_str).unwrap_or("")));
    let pk = kwarg(&a, "primary_key") == Some("True");
    Some(RawColumn {
        name,
        type_name: ty.split('(').next().unwrap_or("").to_string(),
        primary_key: pk,
        nullable: !pk && kwarg(&a, "nullable") != Some("False"),
        unique: pk || kwarg(&a, "unique") == Some("True"),
        references,
        default: kwarg(&a, "server_default").map(unquote),
        constraints,
        evidence: line_ref(path, line),
    })
}

pub fn parse(path: &str, full: &str, entities: &mut Vec<RawEntity>) {
    if !full.contains("alembic") {
        return;
    }
    // Only `upgrade()`: `downgrade()` drops what upgrade created. Blank the rest, keeping offsets.
    let up = full.find("def upgrade").unwrap_or(0);
    let down = full.find("def downgrade").filter(|d| *d > up).unwrap_or(full.len());
    let masked: String =
        full.char_indices().map(|(i, c)| if (i < up || i >= down) && c != '\n' { ' ' } else { c }).collect();
    let text = masked.as_str();
    let mut pos = 0;
    while let Some(off) =
        ["op.create_table(", "op.add_column(", "op.drop_table(", "op.drop_column(", "op.alter_column("]
            .iter()
            .filter_map(|k| text[pos..].find(k).map(|p| (p, *k)))
            .min()
    {
        let (rel, kind) = off;
        let at = pos + rel;
        pos = at + kind.len();
        let Some(body) = call_at(text, at) else { continue };
        let body_offset = at + kind.len();
        let args = split_args(body);
        let Some(table) = args.first().map(|t| unquote(t).to_lowercase()) else { continue };
        let line = line_at(text, at);
        match kind {
            "op.drop_table(" => entities.retain(|e| !(e.table == table && e.source == "alembic")),
            "op.drop_column(" => {
                let col = args.get(1).map(|c| unquote(c)).unwrap_or_default();
                if let Some(e) = entities.iter_mut().find(|e| e.table == table) {
                    e.columns.retain(|c| c.name != col);
                    e.relations.retain(|r| r.via != col);
                }
            }
            "op.alter_column(" => {
                let col = args.get(1).map(|c| unquote(c)).unwrap_or_default();
                if let Some(c) = entities
                    .iter_mut()
                    .find(|e| e.table == table)
                    .and_then(|e| e.columns.iter_mut().find(|c| c.name == col))
                {
                    if let Some(t) = kwarg(&args, "type_") {
                        c.type_name = t
                            .trim_start_matches("sa.")
                            .trim_start_matches("sqlmodel.sql.sqltypes.")
                            .split('(')
                            .next()
                            .unwrap_or("")
                            .to_string();
                        if let Some(len) = paren_args(t).and_then(|p| {
                            split_args(p).iter().find_map(|x| x.strip_prefix("length=").map(str::to_string))
                        }) {
                            c.constraints.retain(|k| !k.starts_with("max length"));
                            c.constraints.push(format!("max length {len}"));
                        }
                    }
                    match kwarg(&args, "nullable") {
                        Some("True") => c.nullable = true,
                        Some("False") => c.nullable = false,
                        _ => {}
                    }
                    c.evidence = line_ref(path, line);
                }
            }
            "op.add_column(" => {
                if let Some(c) = args.get(1).and_then(|c| paren_args(c)).and_then(|c| column(path, line, c)) {
                    if let Some(e) = entities.iter_mut().find(|e| e.table == table) {
                        e.columns.retain(|x| x.name != c.name);
                        e.columns.push(c);
                    }
                }
            }
            _ => {
                let mut e = RawEntity {
                    name: table.clone(),
                    table: table.clone(),
                    source: "alembic".into(),
                    unit: None,
                    columns: vec![],
                    relations: vec![],
                    evidence: line_ref(path, line),
                };
                let mut search = 0;
                for arg in args.iter().skip(1) {
                    let arg_at = body[search..].find(arg.as_str()).map(|p| p + search).unwrap_or(search);
                    search = arg_at + arg.len();
                    let aline = line_at(text, body_offset + arg_at + 1);
                    let inner = paren_args(arg).unwrap_or("");
                    if arg.starts_with("sa.Column(") {
                        if let Some(c) = column(path, aline, inner) {
                            if let Some(r) = &c.references {
                                e.relations.push(RawRelation {
                                    kind: "many-to-one".into(),
                                    target: r.split('.').next().unwrap_or("").to_lowercase(),
                                    via: c.name.clone(),
                                    evidence: line_ref(path, aline),
                                });
                            }
                            e.columns.push(c);
                        }
                    } else if arg.starts_with("sa.PrimaryKeyConstraint(") {
                        let cols: Vec<String> =
                            split_args(inner).iter().filter(|x| !x.contains('=')).map(|x| unquote(x)).collect();
                        for c in e.columns.iter_mut().filter(|c| cols.contains(&c.name)) {
                            c.primary_key = true;
                            c.nullable = false;
                            c.unique = cols.len() == 1;
                        }
                    } else if arg.starts_with("sa.UniqueConstraint(") {
                        let cols: Vec<String> =
                            split_args(inner).iter().filter(|x| !x.contains('=')).map(|x| unquote(x)).collect();
                        if cols.len() == 1 {
                            if let Some(c) = e.columns.iter_mut().find(|c| c.name == cols[0]) {
                                c.unique = true;
                            }
                        }
                    } else if arg.starts_with("sa.ForeignKeyConstraint(") {
                        let parts = split_args(inner);
                        let list = |s: Option<&String>| {
                            s.map(|s| {
                                s.trim_matches(|c| c == '[' || c == ']')
                                    .split(',')
                                    .map(unquote)
                                    .filter(|x| !x.is_empty())
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default()
                        };
                        let (from, to) = (list(parts.first()), list(parts.get(1)));
                        for (k, f) in from.iter().enumerate() {
                            if let (Some(c), Some(t)) = (e.columns.iter_mut().find(|c| &c.name == f), to.get(k)) {
                                c.references = Some(t.to_lowercase());
                            }
                        }
                        if let Some(t) = to.first() {
                            e.relations.push(RawRelation {
                                kind: "many-to-one".into(),
                                target: t.split('.').next().unwrap_or("").to_lowercase(),
                                via: from.join(", "),
                                evidence: line_ref(path, aline),
                            });
                        }
                    }
                }
                entities.retain(|x| x.table != table);
                entities.push(e);
            }
        }
    }
}
