//! Rust: Diesel `table!` / `joinable!`, `sqlx::FromRow` / Diesel `Queryable`
//! row structs (paired with tables later) and closed enums.

use super::raw::*;

pub struct RustOutput {
    pub entities: Vec<RawEntity>,
    pub row_structs: Vec<RawEntity>,
    pub enums: Vec<RawEnum>,
    pub enum_uses: Vec<RawEnumUse>,
}

fn rename(member: &str, rule: Option<&str>) -> String {
    match rule {
        Some("snake_case") => snake_case(member),
        Some("lowercase") => member.to_lowercase(),
        Some("UPPERCASE") => member.to_uppercase(),
        Some("SCREAMING_SNAKE_CASE") => snake_case(member).to_uppercase(),
        Some("kebab-case") => snake_case(member).replace('_', "-"),
        Some("camelCase") => {
            let mut c = member.chars();
            c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default()
        }
        _ => member.to_string(),
    }
}

pub fn parse(files: &[(String, String, String)]) -> RustOutput {
    let mut out = RustOutput { entities: vec![], row_structs: vec![], enums: vec![], enum_uses: vec![] };
    for (path, unit, text) in files {
        let lines: Vec<&str> = text.lines().collect();
        let mut attrs: Vec<(usize, String)> = vec![];
        let mut i = 0;
        while i < lines.len() {
            let t = lines[i].trim();
            if t.starts_with("#[") {
                let mut a = t.to_string();
                while a.matches('[').count() > a.matches(']').count() && i + 1 < lines.len() {
                    i += 1;
                    a.push_str(lines[i].trim());
                }
                attrs.push((i, a));
                i += 1;
                continue;
            }
            if t.starts_with("//") || t.is_empty() {
                i += 1;
                continue;
            }
            // diesel::table! { orders (id) { id -> Int4, … } }
            if t.starts_with("table!") || t.starts_with("diesel::table!") {
                i = diesel_table(path, unit, &lines, i, &mut out);
                attrs.clear();
                continue;
            }
            if let Some(rest) = t.strip_prefix("joinable!(").or_else(|| t.strip_prefix("diesel::joinable!(")) {
                // joinable!(posts -> users (user_id));
                if let Some((child, rest)) = rest.split_once("->") {
                    let parent = rest.split('(').next().unwrap_or("").trim().to_string();
                    let col = paren_args(rest).unwrap_or("").trim().to_string();
                    let child = child.trim().to_lowercase();
                    let ev = line_ref(path, i as u32 + 1);
                    if let Some(e) = out.entities.iter_mut().find(|e| e.table == child) {
                        if let Some(c) = e.columns.iter_mut().find(|c| c.name == col) {
                            c.references = Some(format!("{}.id", parent.to_lowercase()));
                        }
                        e.relations.push(RawRelation {
                            kind: "many-to-one".into(),
                            target: parent.to_lowercase(),
                            via: col,
                            evidence: ev,
                        });
                    }
                }
                i += 1;
                continue;
            }
            let decl = t.trim_start_matches("pub(crate) ").trim_start_matches("pub ");
            let joined: String = attrs.iter().map(|(_, a)| a.as_str()).collect::<Vec<_>>().join(" ");
            if let Some(rest) = decl.strip_prefix("enum ") {
                let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                let rule = ["rename_all = \"", "rename_all=\""]
                    .iter()
                    .find_map(|k| joined.find(k).map(|p| &joined[p + k.len()..]))
                    .and_then(|r| r.split('"').next());
                let start = i;
                let mut values = vec![];
                let mut depth = t.matches('{').count() as i32 - t.matches('}').count() as i32;
                let mut member_rename: Option<String> = None;
                i += 1;
                while i < lines.len() && depth > 0 {
                    let l = lines[i].trim();
                    if l.starts_with("#[") && l.contains("rename = \"") {
                        member_rename =
                            l.split("rename = \"").nth(1).and_then(|r| r.split('"').next()).map(str::to_string);
                    } else if depth == 1
                        && !l.starts_with("//")
                        && !l.starts_with("#[")
                        && !l.is_empty()
                        && !l.starts_with('}')
                    {
                        let m: String = l.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                        let data = l[m.len()..].trim_start();
                        if !m.is_empty() && (data.is_empty() || data.starts_with(',') || data.starts_with('=')) {
                            let stored = member_rename.take().unwrap_or_else(|| rename(&m, rule));
                            values.push((stored, m));
                        } else if !m.is_empty() {
                            // Carries data: not a closed value set.
                            values.clear();
                            depth = -100;
                        }
                    }
                    depth += l.matches('{').count() as i32 - l.matches('}').count() as i32;
                    i += 1;
                }
                if values.len() >= 2 && depth > -50 {
                    out.enums.push(RawEnum {
                        name,
                        values,
                        evidence: line_ref(path, start as u32 + 1),
                        unit: Some(unit.clone()),
                        sql_type: false,
                    });
                }
                attrs.clear();
                continue;
            }
            if let Some(rest) = decl.strip_prefix("struct ") {
                let row = joined.contains("FromRow")
                    || joined.contains("Queryable")
                    || joined.contains("Insertable")
                    || joined.contains("Selectable");
                if !row || !t.ends_with('{') {
                    attrs.clear();
                    i += 1;
                    continue;
                }
                let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                let table = ["table_name = ", "table_name="]
                    .iter()
                    .find_map(|k| joined.find(k).map(|p| &joined[p + k.len()..]))
                    .map(|r| {
                        r.trim_start_matches("crate::schema::")
                            .trim_start_matches("schema::")
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
                            .collect::<String>()
                    })
                    .map(|s| s.rsplit("::").next().unwrap_or("").to_lowercase());
                let start = i;
                let mut e = RawEntity {
                    name: name.clone(),
                    table: table.clone().unwrap_or_else(|| super::go::plural(&snake_case(&name))),
                    source: if joined.contains("FromRow") { "sqlx" } else { "diesel-model" }.into(),
                    unit: Some(unit.clone()),
                    columns: vec![],
                    relations: vec![],
                    evidence: line_ref(path, start as u32 + 1),
                };
                let mut col_rename: Option<String> = None;
                i += 1;
                while i < lines.len() && !lines[i].trim().starts_with('}') {
                    let l = lines[i].trim();
                    if l.starts_with("#[") {
                        if let Some(r) = l.split("rename = \"").nth(1).and_then(|r| r.split('"').next()) {
                            col_rename = Some(r.to_string());
                        }
                    } else if let Some((f, ty)) = l.trim_start_matches("pub ").split_once(':') {
                        let f = f.trim().to_string();
                        let ty = ty.trim().trim_end_matches(',').to_string();
                        e.columns.push(RawColumn {
                            name: col_rename.take().unwrap_or(f),
                            nullable: ty.starts_with("Option<"),
                            type_name: ty,
                            primary_key: false,
                            unique: false,
                            references: None,
                            default: None,
                            constraints: vec![],
                            evidence: line_ref(path, i as u32 + 1),
                        });
                    }
                    i += 1;
                }
                if table.is_some() || joined.contains("FromRow") || joined.contains("Queryable") {
                    out.row_structs.push(e);
                }
                attrs.clear();
                continue;
            }
            attrs.clear();
            i += 1;
        }
    }
    let names: Vec<String> = out.enums.iter().map(|e| e.name.clone()).collect();
    for s in &out.row_structs {
        for c in &s.columns {
            let base = c.type_name.trim_start_matches("Option<").trim_end_matches('>');
            if names.iter().any(|n| n == base) {
                out.enum_uses.push(RawEnumUse {
                    table: s.table.clone(),
                    column: c.name.clone(),
                    enum_name: base.into(),
                    default: None,
                });
            }
        }
    }
    out
}

fn diesel_table(path: &str, unit: &str, lines: &[&str], start: usize, out: &mut RustOutput) -> usize {
    let mut i = start;
    // Header may be on the macro line or the next: `orders (id) {`
    let mut header = None;
    while i < lines.len() {
        let l = lines[i].trim();
        let h = l.trim_start_matches("diesel::").trim_start_matches("table!").trim_start_matches(['{', ' ']).trim();
        if h.contains('(') && !h.starts_with("use ") && !h.starts_with("#[") {
            header = Some((i, h.to_string()));
            break;
        }
        i += 1;
    }
    let Some((hline, h)) = header else { return lines.len() };
    let name = h.split(['(', ' ']).next().unwrap_or("").trim().rsplit('.').next().unwrap_or("").to_string();
    let pks: Vec<String> = paren_args(&h).unwrap_or("").split(',').map(|s| s.trim().to_string()).collect();
    let mut e = RawEntity {
        name: name.clone(),
        table: name.to_lowercase(),
        source: "diesel".into(),
        unit: Some(unit.into()),
        columns: vec![],
        relations: vec![],
        evidence: line_ref(path, hline as u32 + 1),
    };
    let mut sql_name: Option<String> = None;
    i = hline + 1;
    let mut depth = 1;
    while i < lines.len() {
        let l = lines[i].trim();
        if l.starts_with('}') {
            depth -= 1;
            i += 1;
            if depth <= 0 {
                // closing brace of the macro
                if i < lines.len() && lines[i].trim().starts_with('}') {
                    i += 1;
                }
                break;
            }
            continue;
        }
        if l.starts_with("#[sql_name") {
            sql_name = l.split('"').nth(1).map(str::to_string);
        } else if let Some((c, ty)) = l.split_once("->") {
            let c = c.trim().to_string();
            let ty = ty.trim().trim_end_matches(',').to_string();
            let col = sql_name.take().unwrap_or(c.clone());
            e.columns.push(RawColumn {
                primary_key: pks.contains(&c),
                unique: pks.contains(&c) && pks.len() == 1,
                nullable: ty.starts_with("Nullable<"),
                name: col,
                type_name: ty,
                references: None,
                default: None,
                constraints: vec![],
                evidence: line_ref(path, i as u32 + 1),
            });
        }
        i += 1;
    }
    out.entities.push(e);
    i
}
