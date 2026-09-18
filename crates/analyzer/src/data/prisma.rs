//! Prisma schema: `model` and `enum` blocks.

use super::raw::*;

const SCALARS: &[&str] =
    &["String", "Int", "BigInt", "Float", "Decimal", "Boolean", "DateTime", "Json", "Bytes", "Unsupported"];

pub fn parse(
    path: &str,
    text: &str,
    entities: &mut Vec<RawEntity>,
    enums: &mut Vec<RawEnum>,
    uses: &mut Vec<RawEnumUse>,
) {
    let lines: Vec<&str> = text.lines().collect();
    let enum_names: Vec<String> = lines
        .iter()
        .filter_map(|l| l.trim().strip_prefix("enum ").map(|r| r.split_whitespace().next().unwrap_or("").to_string()))
        .collect();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim();
        let (kind, rest) = match t.split_once(' ') {
            Some((k @ ("model" | "enum"), r)) if t.ends_with('{') => (k, r),
            _ => {
                i += 1;
                continue;
            }
        };
        let name = rest.trim_end_matches('{').trim().to_string();
        let start = i;
        let mut body: Vec<(u32, &str)> = vec![];
        i += 1;
        while i < lines.len() && !lines[i].trim().starts_with('}') {
            let b = lines[i].split("//").next().unwrap_or("").trim();
            if !b.is_empty() {
                body.push((i as u32 + 1, b));
            }
            i += 1;
        }
        i += 1;
        if kind == "enum" {
            let values = body
                .iter()
                .filter(|(_, b)| !b.starts_with('@'))
                .map(|(_, b)| {
                    let member = b.split_whitespace().next().unwrap_or("").to_string();
                    let stored = b
                        .find("@map(")
                        .and_then(|p| paren_args(&b[p..]))
                        .map(unquote)
                        .unwrap_or_else(|| member.clone());
                    (stored, member)
                })
                .collect::<Vec<_>>();
            if values.len() >= 2 {
                enums.push(RawEnum {
                    name,
                    values,
                    evidence: line_ref(path, start as u32 + 1),
                    unit: None,
                    sql_type: false,
                });
            }
            continue;
        }
        let table = body
            .iter()
            .find_map(|(_, b)| b.strip_prefix("@@map(").map(|r| unquote(r.trim_end_matches(')'))))
            .unwrap_or_else(|| name.clone())
            .to_lowercase();
        let compound_id: Vec<String> = body
            .iter()
            .find_map(|(_, b)| {
                b.strip_prefix("@@id(").map(|r| {
                    r.trim_matches(|c| c == '[' || c == ']' || c == ')')
                        .split(',')
                        .map(|x| x.trim().to_string())
                        .collect()
                })
            })
            .unwrap_or_default();
        let mut e = RawEntity {
            name: name.clone(),
            table: table.clone(),
            source: "prisma".into(),
            unit: None,
            columns: vec![],
            relations: vec![],
            evidence: line_ref(path, start as u32 + 1),
        };
        let mut fk_targets: Vec<(Vec<String>, String, Vec<String>)> = vec![];
        let mut field_cols: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for (line, b) in &body {
            if b.starts_with("@@") {
                continue;
            }
            let mut parts = b.split_whitespace();
            let (Some(field), Some(ty)) = (parts.next(), parts.next()) else { continue };
            let attrs = &b[b.find(ty).unwrap_or(0) + ty.len()..];
            let optional = ty.ends_with('?');
            let list = ty.ends_with("[]");
            let base = ty.trim_end_matches(['?']).trim_end_matches("[]");
            let ev = line_ref(path, *line);
            let is_scalar = SCALARS.iter().any(|s| base.starts_with(s)) || enum_names.iter().any(|n| n == base);
            if !is_scalar {
                let kind = if list {
                    "one-to-many"
                } else if attrs.contains("fields:") {
                    "many-to-one"
                } else {
                    "one-to-one"
                };
                if let Some(p) = attrs.find("@relation(") {
                    let args = split_args(paren_args(&attrs[p..]).unwrap_or(""));
                    let list_of = |k: &str| {
                        kwarg(&args, k)
                            .map(|v| {
                                v.trim_matches(|c| c == '[' || c == ']')
                                    .split(',')
                                    .map(|x| x.trim().to_string())
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default()
                    };
                    let fields = list_of("fields");
                    if !fields.is_empty() {
                        fk_targets.push((fields.clone(), base.to_string(), list_of("references")));
                        e.relations.push(RawRelation {
                            kind: kind.into(),
                            target: format!("@{base}"),
                            via: fields.join(", "),
                            evidence: ev,
                        });
                        continue;
                    }
                }
                if kind != "one-to-one" || list {
                    e.relations.push(RawRelation {
                        kind: kind.into(),
                        target: format!("@{base}"),
                        via: field.into(),
                        evidence: ev,
                    });
                }
                continue;
            }
            let col = attrs
                .find("@map(")
                .and_then(|p| paren_args(&attrs[p..]))
                .map(unquote)
                .unwrap_or_else(|| field.to_string());
            field_cols.insert(field.to_string(), col.clone());
            let default = attrs.find("@default(").and_then(|p| paren_args(&attrs[p..])).map(unquote);
            if enum_names.iter().any(|n| n == base) {
                uses.push(RawEnumUse {
                    table: table.clone(),
                    column: col.clone(),
                    enum_name: base.into(),
                    default: default.clone(),
                });
            }
            let pk = attrs.contains("@id") || compound_id.iter().any(|c| c == field);
            let mut constraints = vec![];
            if let Some(p) = attrs.find("@db.VarChar(") {
                if let Some(n) = paren_args(&attrs[p..]) {
                    constraints.push(format!("max length {n}"));
                }
            }
            e.columns.push(RawColumn {
                name: col,
                type_name: base.to_string(),
                primary_key: pk,
                nullable: optional,
                unique: attrs.contains("@unique") || attrs.contains("@id"),
                references: None,
                default,
                constraints,
                evidence: ev,
            });
        }
        for r in &mut e.relations {
            r.via = r
                .via
                .split(", ")
                .map(|f| field_cols.get(f).cloned().unwrap_or_else(|| f.to_string()))
                .collect::<Vec<_>>()
                .join(", ");
        }
        for (fields, target, refs) in fk_targets {
            for (k, f) in fields.iter().enumerate() {
                let r = refs.get(k).cloned().unwrap_or_else(|| "id".into());
                let f = field_cols.get(f).cloned().unwrap_or_else(|| f.clone());
                if let Some(c) = e.columns.iter_mut().find(|c| c.name == f) {
                    c.references = Some(format!("@{target}.{r}"));
                }
            }
        }
        entities.push(e);
    }
}
