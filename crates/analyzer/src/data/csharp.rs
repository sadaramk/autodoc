//! C#: Entity Framework Core models.
//!
//! EF Core says almost nothing in the entity class itself. What is stored, and
//! under what name, is decided in three places at once — the `DbContext`'s
//! `DbSet<T>` properties name the entities, `OnModelCreating`'s fluent calls
//! override table and column names, and data annotations on the properties
//! supply keys and lengths. All three are read here, because taking any one
//! alone gives the wrong table name.

use std::collections::HashMap;

use super::raw::*;

pub struct CsOutput {
    pub entities: Vec<RawEntity>,
    pub enums: Vec<RawEnum>,
    pub enum_uses: Vec<RawEnumUse>,
}

/// C# type → the column type a provider would give it. Only the shape matters
/// downstream, so this stays coarse rather than guessing provider specifics.
fn column_type(ty: &str) -> String {
    let t = ty.trim().trim_end_matches('?').trim();
    let bare = t.split('<').next().unwrap_or(t).rsplit('.').next().unwrap_or(t);
    match bare {
        "string" | "String" | "char" | "Guid" => "text",
        "int" | "Int32" | "short" | "uint" | "ushort" | "byte" => "integer",
        "long" | "Int64" | "ulong" => "bigint",
        "decimal" | "Decimal" => "decimal",
        "double" | "float" | "Double" | "Single" => "double",
        "bool" | "Boolean" => "boolean",
        "DateTime" | "DateTimeOffset" => "timestamp",
        "DateOnly" => "date",
        "TimeOnly" | "TimeSpan" => "time",
        "byte[]" => "bytea",
        _ => return bare.to_string(),
    }
    .to_string()
}

/// A property declaration: `public string Name { get; set; } = "";`
struct Prop {
    name: String,
    type_name: String,
    line: u32,
    attrs: Vec<(String, String)>,
    default: Option<String>,
}

const MODIFIERS: &[&str] = &[
    "public",
    "private",
    "protected",
    "internal",
    "virtual",
    "override",
    "required",
    "static",
    "readonly",
    "new",
    "abstract",
    "sealed",
];

/// Collection types whose element is the other side of a one-to-many.
fn collection_of(ty: &str) -> Option<String> {
    let t = ty.trim().trim_end_matches('?').trim();
    let open = t.find('<')?;
    let close = t.rfind('>')?;
    let outer = t[..open].rsplit('.').next().unwrap_or(&t[..open]);
    let collection = matches!(
        outer,
        "ICollection" | "IList" | "List" | "IEnumerable" | "HashSet" | "ISet" | "IQueryable" | "Collection"
    );
    collection.then(|| t[open + 1..close].trim().trim_end_matches('?').to_string())
}

/// Type names declared as `enum` anywhere in the assembly, with their members.
fn enums_in(files: &[(String, String, String)]) -> (Vec<RawEnum>, HashMap<String, String>) {
    let mut out = Vec::new();
    let mut named: HashMap<String, String> = HashMap::new();
    for (path, unit, text) in files {
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let t = lines[i].trim();
            let decl = t.split_whitespace().collect::<Vec<_>>();
            let at = decl.iter().position(|w| *w == "enum");
            let Some(at) = at.filter(|at| decl[..*at].iter().all(|w| MODIFIERS.contains(w))) else {
                i += 1;
                continue;
            };
            let Some(name) = decl.get(at + 1).map(|n| n.trim_end_matches([':', '{']).to_string()) else {
                i += 1;
                continue;
            };
            // Members run to the closing brace; an explicit `= 3` is the stored value.
            let mut values = Vec::new();
            let mut j = i + 1;
            let mut depth = 0i32;
            while j < lines.len() {
                let l = lines[j].trim();
                depth += l.matches('{').count() as i32 - l.matches('}').count() as i32;
                for part in l.split(',') {
                    let part = part.trim().trim_start_matches('{').trim_end_matches('}').trim();
                    let member = part.split('=').next().unwrap_or(part).trim();
                    if member.is_empty() || !member.chars().all(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    // EF Core stores an enum as its integer by default, so the
                    // stored value is the ordinal unless one is written down.
                    let stored = part
                        .split_once('=')
                        .map(|(_, v)| v.trim().to_string())
                        .unwrap_or_else(|| values.len().to_string());
                    values.push((stored, member.to_string()));
                }
                if depth < 0 || (j > i && l.starts_with('}')) {
                    break;
                }
                j += 1;
            }
            if !values.is_empty() {
                named.insert(name.clone(), name.clone());
                out.push(RawEnum {
                    name,
                    values,
                    evidence: line_ref(path, i as u32 + 1),
                    unit: Some(unit.clone()),
                    sql_type: false,
                });
            }
            i = j.max(i + 1);
        }
    }
    (out, named)
}

/// `[Attr(args)]` written above a declaration, gathered upwards.
fn attrs_above(lines: &[&str], at: usize) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut i = at;
    while i > 0 {
        let t = lines[i - 1].trim();
        if !t.starts_with('[') || !t.ends_with(']') {
            break;
        }
        for a in t.trim_start_matches('[').trim_end_matches(']').split(',') {
            let a = a.trim();
            let name = a.split('(').next().unwrap_or(a).trim();
            let args =
                a.find('(').and_then(|o| a.rfind(')').map(|c| a[o + 1..c].trim().to_string())).unwrap_or_default();
            if !name.is_empty() {
                out.push((name.to_string(), args));
            }
        }
        i -= 1;
    }
    out
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs.iter().find(|(n, _)| n == name).map(|(_, a)| a.as_str())
}

fn unquote_arg(v: &str) -> String {
    v.trim().trim_matches('"').to_string()
}

/// Properties of the class whose body starts at `open`, plus the line its
/// declaration is on.
fn properties(lines: &[&str], start: usize, end: usize) -> Vec<Prop> {
    let mut out = Vec::new();
    for (i, raw) in lines.iter().enumerate().take(end).skip(start) {
        let t = raw.trim();
        let Some(brace) = t.find('{') else { continue };
        if !t[brace..].contains("get") {
            continue;
        }
        let decl = t[..brace].trim();
        let mut tokens: Vec<&str> = decl.split_whitespace().collect();
        while tokens.first().is_some_and(|x| MODIFIERS.contains(x)) {
            tokens.remove(0);
        }
        let Some(name) = tokens.pop() else { continue };
        let type_name = tokens.join(" ");
        if type_name.is_empty() || !name.chars().next().is_some_and(char::is_alphabetic) {
            continue;
        }
        // `} = string.Empty;` — an initialiser, which EF reads as no default at
        // the database level, but which does say the column is not null.
        let default = t[brace..]
            .rsplit_once('=')
            .map(|(_, v)| v.trim().trim_end_matches(';').trim().to_string())
            .filter(|v| !v.is_empty() && !v.starts_with('{'));
        out.push(Prop { name: name.to_string(), type_name, line: i as u32 + 1, attrs: attrs_above(lines, i), default });
    }
    out
}

/// Class and record declarations, as (name, declaration line, body line range).
fn type_bodies(lines: &[&str]) -> Vec<(String, usize, usize, usize)> {
    let mut out = Vec::new();
    for (i, raw) in lines.iter().enumerate() {
        let t = raw.trim();
        let words: Vec<&str> = t.split_whitespace().collect();
        let Some(at) = words.iter().position(|w| matches!(*w, "class" | "record" | "struct")) else { continue };
        if !words[..at].iter().all(|w| MODIFIERS.contains(w) || *w == "partial") {
            continue;
        }
        let Some(name) = words.get(at + 1).map(|n| n.trim_end_matches([':', '{', '(']).to_string()) else { continue };
        if name.is_empty() {
            continue;
        }
        // The body runs from the first `{` at or after the declaration to the
        // line where brace depth returns to zero.
        let mut depth = 0i32;
        let mut started = false;
        let mut end = lines.len();
        for (j, l) in lines.iter().enumerate().skip(i) {
            depth += l.matches('{').count() as i32 - l.matches('}').count() as i32;
            started |= l.contains('{');
            if started && depth <= 0 {
                end = j + 1;
                break;
            }
        }
        out.push((name, i, i + 1, end));
    }
    out
}

/// `modelBuilder.Entity<Order>().ToTable("orders")` and the column renames in
/// `OnModelCreating`, which override anything the class says.
#[derive(Default)]
struct Fluent {
    tables: HashMap<String, String>,
    /// `(entity, property)` → column name.
    columns: HashMap<(String, String), String>,
    keys: HashMap<String, Vec<String>>,
}

fn fluent_config(files: &[(String, String, String)]) -> Fluent {
    let mut out = Fluent::default();
    for (_, _, text) in files {
        if !text.contains(".Entity<") {
            continue;
        }
        // The fluent API chains across lines, so the statement is what matters,
        // not the line: everything from `.Entity<T>()` to the terminating `;`.
        for stmt in text.split(';') {
            let Some(at) = stmt.find(".Entity<") else { continue };
            let rest = &stmt[at + ".Entity<".len()..];
            let Some(close) = rest.find('>') else { continue };
            let entity = rest[..close].trim().rsplit('.').next().unwrap_or("").to_string();
            if entity.is_empty() {
                continue;
            }
            if let Some(t) = call_string(stmt, "ToTable") {
                out.tables.insert(entity.clone(), t.to_lowercase());
            }
            if let Some(cols) = call_string(stmt, "HasKey").or_else(|| call_string(stmt, "HasIndex")) {
                let _ = cols;
            }
            // `.HasKey(o => o.Reference)` names the property, not a string.
            if let Some(prop) = lambda_member(stmt, "HasKey") {
                out.keys.entry(entity.clone()).or_default().push(prop);
            }
            // `.Property(o => o.CustomerEmail).HasColumnName("customer_email")`
            let mut from = 0;
            while let Some(p) = stmt[from..].find(".Property(") {
                let abs = from + p;
                let tail = &stmt[abs..];
                let Some(prop) = lambda_member(tail, "Property") else { break };
                if let Some(col) = call_string(tail, "HasColumnName") {
                    out.columns.insert((entity.clone(), prop), col);
                }
                from = abs + ".Property(".len();
            }
        }
    }
    out
}

/// The first string argument of `name(...)` in `stmt`.
fn call_string(stmt: &str, name: &str) -> Option<String> {
    let at = stmt.find(&format!(".{name}("))?;
    let rest = &stmt[at + name.len() + 2..];
    let q = rest.find('"')?;
    let end = rest[q + 1..].find('"')?;
    Some(rest[q + 1..q + 1 + end].to_string())
}

/// `x => x.Member` inside `name(...)` — the property a fluent call configures.
fn lambda_member(stmt: &str, name: &str) -> Option<String> {
    let at = stmt.find(&format!(".{name}("))?;
    let rest = &stmt[at + name.len() + 2..];
    let arrow = rest.find("=>")?;
    let body = &rest[arrow + 2..];
    let close = body.find(')').unwrap_or(body.len());
    let expr = body[..close].trim();
    // Only a single member access is unambiguous; `new { a, b }` is a composite key.
    let member = expr.rsplit('.').next()?.trim();
    (!member.is_empty() && member.chars().all(|c| c.is_alphanumeric() || c == '_')).then(|| member.to_string())
}

/// `public DbSet<Order> Orders { get; set; }` — the entity types EF persists.
/// A class that no `DbSet` names is not a table, however entity-shaped it looks.
fn db_sets(files: &[(String, String, String)]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for (_, _, text) in files {
        for l in text.lines() {
            let t = l.trim();
            let Some(at) = t.find("DbSet<") else { continue };
            let rest = &t[at + "DbSet<".len()..];
            let Some(close) = rest.find('>') else { continue };
            let entity = rest[..close].trim().rsplit('.').next().unwrap_or("").to_string();
            let set_name =
                rest[close + 1..].trim_start().split([' ', '=', '{']).find(|s| !s.is_empty()).unwrap_or("").to_string();
            if !entity.is_empty() {
                out.insert(entity, set_name);
            }
        }
    }
    out
}

pub fn parse(files: &[(String, String, String)]) -> CsOutput {
    let mut out = CsOutput { entities: vec![], enums: vec![], enum_uses: vec![] };
    let sets = db_sets(files);
    if sets.is_empty() {
        return out;
    }
    let fluent = fluent_config(files);
    let (enums, enum_names) = enums_in(files);
    out.enums = enums;

    for (path, unit, text) in files {
        let lines: Vec<&str> = text.lines().collect();
        for (name, decl_line, body_start, body_end) in type_bodies(&lines) {
            let Some(set_name) = sets.get(&name) else { continue };
            // `[Table("orders")]` loses to the fluent `ToTable`, which is applied
            // last; without either, EF pluralises the `DbSet` property name.
            let class_attrs = attrs_above(&lines, decl_line);
            let table = fluent
                .tables
                .get(&name)
                .cloned()
                .or_else(|| attr(&class_attrs, "Table").map(|a| unquote_arg(a).to_lowercase()))
                .unwrap_or_else(|| set_name.to_lowercase())
                .to_string();
            let fluent_keys = fluent.keys.get(&name).cloned().unwrap_or_default();
            let mut columns = Vec::new();
            let mut relations = Vec::new();
            for p in properties(&lines, body_start, body_end) {
                let ev = line_ref(path, p.line);
                // A collection of another entity is the many side, not a column.
                if let Some(target) = collection_of(&p.type_name) {
                    if sets.contains_key(&target) {
                        relations.push(RawRelation {
                            kind: "one-to-many".into(),
                            target: format!("@{target}"),
                            via: p.name.clone(),
                            evidence: ev,
                        });
                        continue;
                    }
                }
                let bare = p.type_name.trim_end_matches('?').rsplit('.').next().unwrap_or(&p.type_name).to_string();
                // A navigation property to another entity: EF adds the FK column.
                if sets.contains_key(&bare) {
                    relations.push(RawRelation {
                        kind: "many-to-one".into(),
                        target: format!("@{bare}"),
                        via: p.name.clone(),
                        evidence: ev,
                    });
                    continue;
                }
                if attr(&p.attrs, "NotMapped").is_some() {
                    continue;
                }
                let column = fluent
                    .columns
                    .get(&(name.clone(), p.name.clone()))
                    .cloned()
                    .or_else(|| attr(&p.attrs, "Column").map(|a| unquote_arg(a.split(',').next().unwrap_or(a))))
                    .unwrap_or_else(|| snake_case(&p.name));
                // EF's convention: a property called `Id` or `<Type>Id` is the key.
                let primary_key = attr(&p.attrs, "Key").is_some()
                    || fluent_keys.contains(&p.name)
                    || (fluent_keys.is_empty()
                        && (p.name == "Id" || p.name == format!("{name}Id"))
                        && attr(&p.attrs, "Key").is_none());
                let nullable =
                    p.type_name.trim_end().ends_with('?') && attr(&p.attrs, "Required").is_none() && !primary_key;
                let mut constraints = Vec::new();
                for (a, args) in &p.attrs {
                    let first = args.split(',').next().unwrap_or(args).trim();
                    match a.as_str() {
                        "MaxLength" | "StringLength" => constraints.push(format!("max length {first}")),
                        "MinLength" => constraints.push(format!("min length {first}")),
                        "Range" => {
                            let mut it = args.split(',');
                            if let (Some(lo), Some(hi)) = (it.next(), it.next()) {
                                constraints.push(format!("{} ≤ value ≤ {}", lo.trim(), hi.trim()));
                            }
                        }
                        "RegularExpression" => constraints.push(format!("pattern {first}")),
                        "EmailAddress" => constraints.push("email".into()),
                        "ConcurrencyCheck" | "Timestamp" => constraints.push("optimistic concurrency".into()),
                        _ => {}
                    }
                }
                if enum_names.contains_key(&bare) {
                    out.enum_uses.push(RawEnumUse {
                        table: table.clone(),
                        column: column.clone(),
                        enum_name: bare.clone(),
                        default: p.default.clone(),
                    });
                }
                columns.push(RawColumn {
                    name: column,
                    type_name: column_type(&p.type_name),
                    primary_key,
                    nullable,
                    unique: false,
                    references: None,
                    default: p.default.clone().filter(|d| d != "string.Empty" && !d.contains("new ")),
                    constraints,
                    evidence: ev,
                });
            }
            if columns.is_empty() && relations.is_empty() {
                continue;
            }
            out.entities.push(RawEntity {
                name: name.clone(),
                table,
                source: "ef-core".into(),
                unit: Some(unit.clone()),
                columns,
                relations,
                evidence: line_ref(path, decl_line as u32 + 1),
            });
        }
    }
    out
}
