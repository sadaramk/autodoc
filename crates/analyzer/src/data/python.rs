//! Python models: SQLModel, SQLAlchemy declarative, Django, and Enum classes.

use std::collections::HashMap;

use super::raw::*;

/// (name, annotation, value, line)
type Field = (String, String, String, u32);

struct PyClass {
    name: String,
    bases: String,
    line: u32,
    /// (name, annotation, value, line)
    fields: Vec<(String, String, String, u32)>,
    tablename: Option<String>,
    meta_table: Option<String>,
}

fn classes(text: &str) -> Vec<PyClass> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i];
        let t = l.trim_start();
        if let Some(rest) = t.strip_prefix("class ") {
            let ind = indent(l);
            let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            let bases =
                rest.find('(').and_then(|p| rest.rfind(')').map(|q| rest[p + 1..q].to_string())).unwrap_or_default();
            let mut cls =
                PyClass { name, bases, line: i as u32 + 1, fields: vec![], tablename: None, meta_table: None };
            let mut j = i + 1;
            let mut in_meta = None::<usize>;
            let mut in_def = None::<usize>;
            while j < lines.len() {
                let bl = lines[j];
                if bl.trim().is_empty() || bl.trim_start().starts_with('#') {
                    j += 1;
                    continue;
                }
                let bi = indent(bl);
                if bi <= ind {
                    break;
                }
                let bt = bl.trim();
                if let Some(d) = in_def {
                    if bi > d {
                        j += 1;
                        continue;
                    }
                    in_def = None;
                }
                if let Some(m) = in_meta {
                    if bi > m {
                        if let Some(v) = bt.strip_prefix("db_table") {
                            cls.meta_table = Some(unquote(v.trim_start_matches([' ', '='])));
                        }
                        j += 1;
                        continue;
                    }
                    in_meta = None;
                }
                if bt.starts_with("def ") || bt.starts_with("async def ") || bt.starts_with('@') {
                    if !bt.starts_with('@') {
                        in_def = Some(bi);
                    }
                } else if bt.starts_with("class Meta") {
                    in_meta = Some(bi);
                } else if let Some(v) = bt.strip_prefix("__tablename__") {
                    cls.tablename = Some(unquote(
                        v.trim_start_matches([' ', '=', ':']).trim_start_matches("str").trim_start_matches([' ', '=']),
                    ));
                } else if !bt.starts_with("class ") && !bt.starts_with("model_config") && !bt.starts_with('"') {
                    // Collect a (possibly multi-line) `name: ann = value` / `name = value`.
                    let mut stmt = bt.to_string();
                    let mut k = j;
                    while stmt.matches('(').count() > stmt.matches(')').count() && k + 1 < lines.len() {
                        k += 1;
                        stmt.push(' ');
                        stmt.push_str(lines[k].trim());
                    }
                    let (lhs, value) = match stmt.split_once('=') {
                        Some((l, v)) if !l.contains('(') => (l.trim().to_string(), v.trim().to_string()),
                        _ => (stmt.clone(), String::new()),
                    };
                    let (name, ann) = match lhs.split_once(':') {
                        Some((n, a)) => (n.trim().to_string(), a.trim().to_string()),
                        None => (lhs.trim().to_string(), String::new()),
                    };
                    if name.chars().all(|c| c.is_alphanumeric() || c == '_') && !name.is_empty() {
                        cls.fields.push((name, ann, value, j as u32 + 1));
                    }
                    j = k;
                }
                j += 1;
            }
            out.push(cls);
            i += 1;
            continue;
        }
        i += 1;
    }
    out
}

fn is_enum(bases: &str) -> bool {
    bases.split(',').any(|b| {
        matches!(b.trim(), "Enum" | "StrEnum" | "IntEnum" | "enum.Enum" | "enum.StrEnum" | "models.TextChoices")
    })
}

pub struct PyOutput {
    pub entities: Vec<RawEntity>,
    pub enums: Vec<RawEnum>,
    pub enum_uses: Vec<RawEnumUse>,
}

/// Parses every Python file together so SQLModel base classes resolve across files.
pub fn parse(files: &[(String, String, String)]) -> PyOutput {
    let mut out = PyOutput { entities: vec![], enums: vec![], enum_uses: vec![] };
    let mut all: Vec<(String, String, PyClass)> = Vec::new();
    for (path, unit, text) in files {
        for c in classes(text) {
            all.push((path.clone(), unit.clone(), c));
        }
    }
    let by_name: HashMap<String, usize> = all.iter().enumerate().map(|(i, (_, _, c))| (c.name.clone(), i)).collect();

    for (path, unit, c) in &all {
        if is_enum(&c.bases) {
            let values: Vec<(String, String)> = c
                .fields
                .iter()
                .filter(|(n, _, v, _)| {
                    n.chars().all(|ch| ch.is_uppercase() || ch == '_' || ch.is_ascii_digit()) && !v.is_empty()
                })
                .map(|(n, _, v, _)| {
                    let val = if v.starts_with("auto(") {
                        n.to_lowercase()
                    } else {
                        unquote(v.split(',').next().unwrap_or(v).trim_start_matches('('))
                    };
                    (val, n.clone())
                })
                .collect();
            if values.len() >= 2 {
                out.enums.push(RawEnum {
                    name: c.name.clone(),
                    values,
                    evidence: line_ref(path, c.line),
                    unit: Some(unit.clone()),
                    sql_type: false,
                });
            }
        }
    }

    let enum_names: Vec<String> = out.enums.iter().map(|e| e.name.clone()).collect();
    for (path, unit, c) in &all {
        let sqlmodel = c.bases.contains("table=True");
        let django = c.bases.split(',').any(|b| matches!(b.trim(), "models.Model" | "Model"));
        let sqlalchemy = !sqlmodel
            && c.tablename.is_some()
            && c.fields.iter().any(|(_, a, v, _)| {
                v.starts_with("Column(") || v.starts_with("mapped_column(") || a.starts_with("Mapped[")
            });
        if !(sqlmodel || django || sqlalchemy) {
            continue;
        }
        let source = if sqlmodel {
            "sqlmodel"
        } else if django {
            "django"
        } else {
            "sqlalchemy"
        };
        let table = c
            .tablename
            .clone()
            .or_else(|| c.meta_table.clone())
            .unwrap_or_else(|| {
                if django {
                    let app = path.rsplit('/').nth(1).unwrap_or("app");
                    format!("{}_{}", app, c.name.to_lowercase())
                } else {
                    c.name.to_lowercase()
                }
            })
            .to_lowercase();
        let mut entity = RawEntity {
            name: c.name.clone(),
            table: table.clone(),
            source: source.into(),
            unit: Some(unit.clone()),
            columns: vec![],
            relations: vec![],
            evidence: line_ref(path, c.line),
        };
        // Fields from base classes first (SQLModel `class Item(ItemBase, table=True)`).
        let mut chain: Vec<&(String, String, PyClass)> = vec![];
        let mut stack: Vec<String> = c.bases.split(',').map(|b| b.trim().to_string()).collect();
        let mut guard = 0;
        while let Some(b) = stack.pop() {
            guard += 1;
            if guard > 20 {
                break;
            }
            if let Some(&i) = by_name.get(&b) {
                if all[i].2.name != c.name {
                    chain.insert(0, &all[i]);
                    stack.extend(all[i].2.bases.split(',').map(|x| x.trim().to_string()));
                }
            }
        }
        let own = (
            path.clone(),
            unit.clone(),
            PyClass {
                name: c.name.clone(),
                bases: String::new(),
                line: c.line,
                fields: c.fields.clone(),
                tablename: None,
                meta_table: None,
            },
        );
        let mut sources: Vec<(String, Vec<Field>)> =
            chain.iter().map(|(p, _, k)| (p.clone(), k.fields.clone())).collect();
        sources.push((own.0.clone(), own.2.fields.clone()));
        if django && !c.fields.iter().any(|(_, _, v, _)| v.contains("primary_key=True")) {
            entity.columns.push(RawColumn {
                name: "id".into(),
                type_name: "AutoField".into(),
                primary_key: true,
                nullable: false,
                unique: true,
                references: None,
                default: None,
                constraints: vec![],
                evidence: line_ref(path, c.line),
            });
        }
        for (fpath, fields) in sources {
            for (name, ann, value, line) in fields {
                if name.starts_with("__") || name == "objects" || name == "Meta" {
                    continue;
                }
                let args = paren_args(&value).map(split_args).unwrap_or_default();
                let ev = line_ref(&fpath, line);
                // Relations.
                let is_rel = value.starts_with("Relationship(") || value.starts_with("relationship(");
                if is_rel {
                    let target_raw =
                        if ann.is_empty() { args.first().cloned().unwrap_or_default() } else { ann.clone() };
                    let collection = ann.starts_with("list[")
                        || ann.starts_with("List[")
                        || ann.contains("Mapped[list[")
                        || ann.contains("Mapped[List[")
                        || kwarg(&args, "uselist") == Some("True");
                    let target = clean_type(&target_raw);
                    entity.relations.push(RawRelation {
                        kind: if collection { "one-to-many".into() } else { "many-to-one".into() },
                        target: format!("@{target}"),
                        via: name.clone(),
                        evidence: ev,
                    });
                    continue;
                }
                if django {
                    let Some(kind) = value.strip_prefix("models.").and_then(|v| v.split('(').next()) else { continue };
                    let target = args.first().map(|a| unquote(a)).unwrap_or_default();
                    match kind {
                        "ForeignKey" | "OneToOneField" => {
                            let t = target.rsplit('.').next().unwrap_or(&target).to_string();
                            entity.columns.push(RawColumn {
                                name: format!("{name}_id"),
                                type_name: kind.into(),
                                primary_key: false,
                                nullable: kwarg(&args, "null") == Some("True"),
                                unique: kind == "OneToOneField",
                                references: Some(format!("@{t}.id")),
                                default: None,
                                constraints: vec![],
                                evidence: ev.clone(),
                            });
                            entity.relations.push(RawRelation {
                                kind: if kind == "OneToOneField" { "one-to-one".into() } else { "many-to-one".into() },
                                target: format!("@{t}"),
                                via: format!("{name}_id"),
                                evidence: ev,
                            });
                        }
                        "ManyToManyField" => {
                            let t = target.rsplit('.').next().unwrap_or(&target).to_string();
                            entity.relations.push(RawRelation {
                                kind: "many-to-many".into(),
                                target: format!("@{t}"),
                                via: name.clone(),
                                evidence: ev,
                            });
                        }
                        k if k.ends_with("Field") => {
                            let mut constraints = vec![];
                            if let Some(m) = kwarg(&args, "max_length") {
                                constraints.push(format!("max length {m}"));
                            }
                            let choices = kwarg(&args, "choices").map(|c| c.trim_end_matches(".choices").to_string());
                            if let Some(ch) = &choices {
                                if enum_names.contains(ch) {
                                    out.enum_uses.push(RawEnumUse {
                                        table: table.clone(),
                                        column: name.clone(),
                                        enum_name: ch.clone(),
                                        default: kwarg(&args, "default").map(str::to_string),
                                    });
                                }
                            }
                            entity.columns.push(RawColumn {
                                name: name.clone(),
                                type_name: k.into(),
                                primary_key: kwarg(&args, "primary_key") == Some("True"),
                                nullable: kwarg(&args, "null") == Some("True"),
                                unique: kwarg(&args, "unique") == Some("True"),
                                references: None,
                                default: kwarg(&args, "default").map(unquote),
                                constraints,
                                evidence: ev,
                            });
                        }
                        _ => {}
                    }
                    continue;
                }
                // SQLModel / SQLAlchemy columns.
                let sa_col = value.starts_with("Column(") || value.starts_with("mapped_column(");
                let field_call = value.starts_with("Field(");
                if sqlalchemy && !sa_col && !ann.starts_with("Mapped[") {
                    continue;
                }
                if sqlmodel && !field_call && ann.is_empty() {
                    continue;
                }
                if ann.starts_with("ClassVar") {
                    continue;
                }
                let inner_ann =
                    ann.strip_prefix("Mapped[").map(|a| a.trim_end_matches(']').to_string()).unwrap_or(ann.clone());
                let mut type_name = if !inner_ann.is_empty() {
                    inner_ann.clone()
                } else {
                    args.first().map(|a| a.to_string()).unwrap_or_default()
                };
                if sa_col
                    && !args.is_empty()
                    && !args[0].contains('=')
                    && !args[0].starts_with("ForeignKey")
                    && inner_ann.is_empty()
                {
                    type_name = args[0].clone();
                }
                let optional = inner_ann.contains("None") || inner_ann.starts_with("Optional[");
                let primary_key = kwarg(&args, "primary_key") == Some("True");
                let fk = kwarg(&args, "foreign_key").map(unquote).or_else(|| {
                    args.iter()
                        .find(|a| a.starts_with("ForeignKey("))
                        .and_then(|a| paren_args(a))
                        .map(|x| unquote(x.split(',').next().unwrap_or("")))
                });
                let nullable = match kwarg(&args, "nullable") {
                    Some("True") => true,
                    Some("False") => false,
                    _ => optional && !primary_key,
                };
                let mut constraints = vec![];
                for (k, label) in [
                    ("max_length", "max length"),
                    ("min_length", "min length"),
                    ("gt", ">"),
                    ("ge", "≥"),
                    ("lt", "<"),
                    ("le", "≤"),
                ] {
                    if let Some(v) = kwarg(&args, k) {
                        constraints.push(format!("{label} {v}"));
                    }
                }
                if let Some(len) = args.iter().chain(std::iter::once(&type_name)).find_map(|a| {
                    let a = a.trim_start_matches("sa.");
                    ["String(", "VARCHAR(", "Unicode(", "CHAR("]
                        .iter()
                        .any(|p| a.starts_with(p))
                        .then(|| paren_args(a))
                        .flatten()
                }) {
                    if len.chars().all(|c| c.is_ascii_digit())
                        && !len.is_empty()
                        && !constraints.iter().any(|c| c.starts_with("max length"))
                    {
                        constraints.push(format!("max length {len}"));
                    }
                }
                let plain = !field_call && !sa_col && !value.is_empty() && !value.contains('(');
                let default = if plain { Some(value.clone()) } else { kwarg(&args, "default").map(|d| d.to_string()) }
                    .filter(|d| d != "None");
                let base_type = clean_type(&type_name);
                if enum_names.contains(&base_type) {
                    out.enum_uses.push(RawEnumUse {
                        table: table.clone(),
                        column: name.clone(),
                        enum_name: base_type.clone(),
                        default: default.clone(),
                    });
                }
                if let Some(fk) = &fk {
                    let t = fk.split('.').next().unwrap_or("").to_lowercase();
                    entity.relations.push(RawRelation {
                        kind: "many-to-one".into(),
                        target: t,
                        via: name.clone(),
                        evidence: ev.clone(),
                    });
                }
                entity.columns.retain(|x| x.name != name);
                entity.columns.push(RawColumn {
                    name: name.clone(),
                    type_name: type_name.replace(" | None", "").trim().to_string(),
                    primary_key,
                    nullable,
                    unique: kwarg(&args, "unique") == Some("True") || primary_key,
                    references: fk.map(|f| f.to_lowercase()),
                    default: default.map(|d| unquote(&d)),
                    constraints,
                    evidence: ev,
                });
            }
        }
        // A relation attribute with a matching FK column gets that column as `via`.
        let fk_cols: Vec<(String, String)> = entity
            .columns
            .iter()
            .filter_map(|c| {
                c.references.as_ref().map(|r| (r.split('.').next().unwrap_or("").to_string(), c.name.clone()))
            })
            .collect();
        let mut rels = vec![];
        for r in entity.relations.drain(..) {
            if r.kind == "many-to-one" && r.target.starts_with('@') {
                // Resolved later; keep only when no FK column already models it.
                let tname = r.target.trim_start_matches('@').to_lowercase();
                if fk_cols.iter().any(|(t, _)| *t == tname) {
                    continue;
                }
            }
            rels.push(r);
        }
        entity.relations = rels;
        out.entities.push(entity);
    }
    out
}

/// `list["Item"]`, `"User" | None`, `Optional[User]` → `Item` / `User`.
fn clean_type(t: &str) -> String {
    let mut s = t.trim().to_string();
    for wrap in ["Mapped[", "list[", "List[", "Optional[", "Sequence[", "set["] {
        if let Some(inner) = s.strip_prefix(wrap) {
            s = inner.trim_end_matches(']').to_string();
        }
    }
    let s = s.split('|').map(str::trim).find(|p| *p != "None").unwrap_or("").to_string();
    unquote(&s)
}
