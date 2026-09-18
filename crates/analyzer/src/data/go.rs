//! Go: GORM models, `db:"…"`-tagged row structs (paired with DDL later) and
//! typed string constants (`type OrderStatus string` + `const (…)`).

use std::collections::HashMap;

use super::raw::*;

pub struct GoOutput {
    pub entities: Vec<RawEntity>,
    /// Row structs without ORM metadata: only merged when a DDL table matches.
    pub row_structs: Vec<RawEntity>,
    pub enums: Vec<RawEnum>,
    pub enum_uses: Vec<RawEnumUse>,
}

fn tag(tags: &str, key: &str) -> Option<String> {
    let p = tags.find(&format!("{key}:\""))?;
    let rest = &tags[p + key.len() + 2..];
    Some(rest[..rest.find('"')?].to_string())
}

pub fn plural(s: &str) -> String {
    if s.ends_with('s') || s.ends_with('x') || s.ends_with("ch") || s.ends_with("sh") {
        format!("{s}es")
    } else if s.ends_with('y') && !s.ends_with("ay") && !s.ends_with("ey") && !s.ends_with("oy") {
        format!("{}ies", &s[..s.len() - 1])
    } else {
        format!("{s}s")
    }
}

pub fn parse(files: &[(String, String, String)]) -> GoOutput {
    let mut out = GoOutput { entities: vec![], row_structs: vec![], enums: vec![], enum_uses: vec![] };
    let mut table_names: HashMap<String, String> = HashMap::new();
    let mut string_types: Vec<String> = vec![];
    for (_, _, text) in files {
        let lines: Vec<&str> = text.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            let t = l.trim();
            if t.starts_with("func (") && t.contains(") TableName() string") {
                let recv = t["func (".len()..].split(')').next().unwrap_or("");
                let ty = recv.split_whitespace().last().unwrap_or("").trim_start_matches('*').to_string();
                if let Some(ret) =
                    lines[i..lines.len().min(i + 4)].iter().find_map(|x| x.trim().strip_prefix("return "))
                {
                    table_names.insert(ty, unquote(ret));
                }
            }
            if let Some(r) = t.strip_prefix("type ") {
                let mut p = r.split_whitespace();
                if let (Some(n), Some("string")) = (p.next(), p.next()) {
                    string_types.push(n.to_string());
                }
            }
        }
    }
    // Typed constants.
    for (path, unit, text) in files {
        let mut groups: HashMap<String, (u32, Vec<(String, String)>)> = HashMap::new();
        let mut in_const = false;
        for (i, l) in text.lines().enumerate() {
            let t = l.trim();
            if t.starts_with("const (") {
                in_const = true;
                continue;
            }
            if in_const && t.starts_with(')') {
                in_const = false;
                continue;
            }
            let decl = if in_const { Some(t) } else { t.strip_prefix("const ") };
            let Some(decl) = decl else { continue };
            let Some((lhs, rhs)) = decl.split_once('=') else { continue };
            let mut lp = lhs.split_whitespace();
            let (Some(name), Some(ty)) = (lp.next(), lp.next()) else { continue };
            if !string_types.iter().any(|s| s == ty) || !rhs.trim().starts_with('"') {
                continue;
            }
            let g = groups.entry(ty.to_string()).or_insert((i as u32 + 1, vec![]));
            g.1.push((unquote(rhs.trim().split("//").next().unwrap_or("")), name.to_string()));
        }
        for (ty, (line, values)) in groups {
            if values.len() >= 2 {
                out.enums.push(RawEnum {
                    name: ty,
                    values,
                    evidence: line_ref(path, line),
                    unit: Some(unit.clone()),
                    sql_type: false,
                });
            }
        }
    }
    let enum_names: Vec<String> = out.enums.iter().map(|e| e.name.clone()).collect();

    for (path, unit, text) in files {
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let t = lines[i].trim();
            let Some(rest) = t.strip_prefix("type ") else {
                i += 1;
                continue;
            };
            if !rest.contains(" struct {") {
                i += 1;
                continue;
            }
            let name = rest.split_whitespace().next().unwrap_or("").to_string();
            let start = i;
            let mut fields: Vec<(u32, &str)> = vec![];
            i += 1;
            while i < lines.len() && lines[i].trim() != "}" {
                fields.push((i as u32 + 1, lines[i].trim()));
                i += 1;
            }
            let gorm = fields.iter().any(|(_, f)| f.contains("gorm:\"") || f.starts_with("gorm.Model"));
            let dbtag = fields.iter().any(|(_, f)| f.contains("db:\""));
            if !gorm && !dbtag {
                continue;
            }
            let table = table_names.get(&name).cloned().unwrap_or_else(|| plural(&snake_case(&name)));
            let mut e = RawEntity {
                name: name.clone(),
                table: table.to_lowercase(),
                source: if gorm { "gorm" } else { "go-struct" }.into(),
                unit: Some(unit.clone()),
                columns: vec![],
                relations: vec![],
                evidence: line_ref(path, start as u32 + 1),
            };
            for (line, f) in fields {
                let ev = line_ref(path, line);
                if f.starts_with("gorm.Model") {
                    for (c, ty) in [
                        ("id", "uint"),
                        ("created_at", "time.Time"),
                        ("updated_at", "time.Time"),
                        ("deleted_at", "gorm.DeletedAt"),
                    ] {
                        e.columns.push(RawColumn {
                            name: c.into(),
                            type_name: ty.into(),
                            primary_key: c == "id",
                            nullable: c == "deleted_at",
                            unique: c == "id",
                            references: None,
                            default: None,
                            constraints: vec![],
                            evidence: ev.clone(),
                        });
                    }
                    continue;
                }
                let tags = f.find('`').map(|p| &f[p..]).unwrap_or("");
                let decl = f.split('`').next().unwrap_or("").split("//").next().unwrap_or("").trim();
                let mut dp = decl.split_whitespace();
                let (Some(fname), Some(fty)) = (dp.next(), dp.next()) else { continue };
                if !fname.chars().next().is_some_and(|c| c.is_uppercase()) {
                    continue;
                }
                let g = tag(tags, "gorm").unwrap_or_default();
                if g == "-" || tag(tags, "db").as_deref() == Some("-") {
                    continue;
                }
                let base = fty.trim_start_matches(['*', '[', ']']);
                if gorm
                    && (fty.starts_with("[]")
                        || (base.chars().next().is_some_and(|c| c.is_uppercase())
                            && !base.contains('.')
                            && !enum_names.iter().any(|n| n == base)
                            && files.iter().any(|(_, _, t)| t.contains(&format!("type {base} struct")))))
                {
                    let kind = if fty.starts_with("[]") { "one-to-many" } else { "many-to-one" };
                    let via =
                        g.split(';').find_map(|p| p.strip_prefix("foreignKey:")).map(snake_case).unwrap_or_else(|| {
                            if kind == "many-to-one" {
                                format!("{}_id", snake_case(fname))
                            } else {
                                snake_case(fname)
                            }
                        });
                    e.relations.push(RawRelation { kind: kind.into(), target: format!("@{base}"), via, evidence: ev });
                    continue;
                }
                let col = g
                    .split(';')
                    .find_map(|p| p.strip_prefix("column:").map(str::to_string))
                    .or_else(|| tag(tags, "db").map(|d| d.split(',').next().unwrap_or("").to_string()))
                    .unwrap_or_else(|| snake_case(fname));
                if enum_names.iter().any(|n| n == base) {
                    out.enum_uses.push(RawEnumUse {
                        table: table.to_lowercase(),
                        column: col.clone(),
                        enum_name: base.into(),
                        default: g.split(';').find_map(|p| p.strip_prefix("default:")).map(str::to_string),
                    });
                }
                let parts: Vec<&str> = g.split(';').collect();
                let pk = parts.iter().any(|p| p.eq_ignore_ascii_case("primaryKey")) || (gorm && fname == "ID");
                let mut constraints = vec![];
                if let Some(sz) = parts.iter().find_map(|p| p.strip_prefix("size:")) {
                    constraints.push(format!("max length {sz}"));
                }
                if let Some(c) = parts.iter().find_map(|p| p.strip_prefix("check:")) {
                    constraints.push(format!("CHECK ({c})"));
                }
                e.columns.push(RawColumn {
                    name: col,
                    type_name: fty.to_string(),
                    primary_key: pk,
                    nullable: fty.starts_with('*') || fty.starts_with("sql.Null"),
                    unique: pk || parts.iter().any(|p| p.starts_with("unique")),
                    references: None,
                    default: parts.iter().find_map(|p| p.strip_prefix("default:")).map(unquote),
                    constraints,
                    evidence: ev,
                });
            }
            if gorm {
                out.entities.push(e);
            } else {
                out.row_structs.push(e);
            }
        }
    }
    out
}
