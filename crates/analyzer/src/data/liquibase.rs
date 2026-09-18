//! Liquibase changelogs (XML, YAML, JSON): `createTable`, `addColumn`,
//! foreign keys, unique / primary key / not-null constraints, renames and
//! drops, applied in changelog order (following `include` / `includeAll`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::raw::*;
use super::Column;

/// Changelog files under `root`, in execution order: masters (files nothing
/// includes) in path order, each expanded through its includes.
pub fn changelog_files(root: &Path, candidates: &[String]) -> Vec<String> {
    let mut includes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let set: BTreeSet<&String> = candidates.iter().collect();
    for f in candidates {
        let Ok(text) = std::fs::read_to_string(root.join(f)) else { continue };
        includes.insert(f.clone(), included(f, &text, candidates));
    }
    let referenced: BTreeSet<String> = includes.values().flatten().cloned().collect();
    let mut order = vec![];
    let mut seen = BTreeSet::new();
    fn visit(f: &str, includes: &BTreeMap<String, Vec<String>>, seen: &mut BTreeSet<String>, order: &mut Vec<String>) {
        if !seen.insert(f.to_string()) {
            return;
        }
        order.push(f.to_string());
        for i in includes.get(f).into_iter().flatten() {
            visit(i, includes, seen, order);
        }
    }
    for f in candidates.iter().filter(|f| !referenced.contains(*f)) {
        visit(f, &includes, &mut seen, &mut order);
    }
    for f in set {
        visit(f, &includes, &mut seen, &mut order);
    }
    order
}

fn included(from: &str, text: &str, candidates: &[String]) -> Vec<String> {
    let dir = from.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let mut out = vec![];
    let resolve = |p: &str| -> Vec<String> {
        let p = p.trim_start_matches("classpath:").trim_start_matches('/');
        let rel = if dir.is_empty() { p.to_string() } else { format!("{dir}/{p}") };
        // Relative to the changelog, or classpath-relative (`db/changelog/x.xml` under any resources dir).
        candidates
            .iter()
            .filter(|c| **c == rel || c.ends_with(&format!("/{p}")) || **c == p)
            .cloned()
            .collect::<Vec<_>>()
    };
    let resolve_dir = |p: &str| -> Vec<String> {
        let p = p.trim_start_matches("classpath:").trim_start_matches('/').trim_end_matches('/');
        let mut v: Vec<String> = candidates
            .iter()
            .filter(|c| {
                c.rsplit_once('/').is_some_and(|(d, _)| {
                    d == p || d.ends_with(&format!("/{p}")) || (!dir.is_empty() && d == format!("{dir}/{p}"))
                })
            })
            .cloned()
            .collect();
        v.sort();
        v
    };
    for (k, v) in attr_pairs(text) {
        match k.as_str() {
            "file" => out.extend(resolve(&v)),
            "path" => out.extend(resolve_dir(&v)),
            _ => {}
        }
    }
    out.retain(|x| x != from);
    out
}

/// `file="x"` (XML) and `file: x` (YAML) / `"file": "x"` (JSON) pairs on include lines.
fn attr_pairs(text: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    for line in text.lines() {
        let l = line.trim();
        if !(l.contains("include")
            || l.starts_with("file")
            || l.starts_with("path")
            || l.starts_with("\"file\"")
            || l.starts_with("\"path\""))
        {
            continue;
        }
        for key in ["file", "path"] {
            for pat in [format!("{key}=\""), format!("{key}: "), format!("\"{key}\": \""), format!("{key}:")] {
                if let Some(p) = l.find(&pat) {
                    let rest = &l[p + pat.len()..];
                    let v: String = rest
                        .trim()
                        .trim_start_matches('"')
                        .trim_start_matches('\'')
                        .chars()
                        .take_while(|c| !matches!(c, '"' | '\'' | ',' | '}' | ' '))
                        .collect();
                    if !v.is_empty() && (v.contains('.') || v.contains('/')) {
                        out.push((key.to_string(), v));
                    }
                    break;
                }
            }
        }
    }
    out
}

type Attrs = BTreeMap<String, String>;

/// One change operation, normalised across formats.
#[derive(Debug, Default, Clone)]
struct Change {
    kind: String,
    attrs: BTreeMap<String, String>,
    /// (column attributes, constraints, line)
    columns: Vec<(Attrs, Attrs, u32)>,
    line: u32,
    /// Line of each attribute (YAML/JSON spread attributes over lines; XML keeps them on the element).
    attr_lines: BTreeMap<String, u32>,
}

impl Change {
    fn at(&self, key: &str) -> u32 {
        self.attr_lines.get(key).copied().unwrap_or(self.line)
    }
}

pub fn parse(path: &str, text: &str, unit: Option<String>, tables: &mut Vec<RawEntity>) {
    let changes = if path.ends_with(".xml") { xml_changes(text) } else { yaml_changes(text) };
    for ch in changes {
        apply(path, &ch, unit.clone(), tables);
    }
}

fn xml_changes(text: &str) -> Vec<Change> {
    let Ok(doc) = roxmltree::Document::parse(text) else { return vec![] };
    let mut out = vec![];
    let line_of = |n: roxmltree::Node| doc.text_pos_at(n.range().start).row;
    for cs in doc.descendants().filter(|n| n.tag_name().name() == "changeSet") {
        for ch in cs.children().filter(|n| n.is_element()) {
            let kind = ch.tag_name().name().to_string();
            if matches!(kind.as_str(), "rollback" | "preConditions" | "comment" | "validCheckSum") {
                continue;
            }
            let attrs: BTreeMap<String, String> =
                ch.attributes().map(|a| (a.name().to_string(), a.value().to_string())).collect();
            let mut columns = vec![];
            for col in ch.children().filter(|n| n.tag_name().name() == "column") {
                let ca: BTreeMap<String, String> =
                    col.attributes().map(|a| (a.name().to_string(), a.value().to_string())).collect();
                let cons: BTreeMap<String, String> = col
                    .children()
                    .find(|n| n.tag_name().name() == "constraints")
                    .map(|c| c.attributes().map(|a| (a.name().to_string(), a.value().to_string())).collect())
                    .unwrap_or_default();
                columns.push((ca, cons, line_of(col)));
            }
            out.push(Change { kind, attrs, columns, line: line_of(ch), attr_lines: BTreeMap::new() });
        }
    }
    out
}

fn yaml_changes(text: &str) -> Vec<Change> {
    let Ok(v) = serde_yaml::from_str::<serde_yaml::Value>(text) else { return vec![] };
    let lines: Vec<&str> = text.lines().collect();
    let mut cursor = 0usize;
    let mut find_line = |needles: &[String]| -> u32 {
        let hit = lines[cursor.min(lines.len())..]
            .iter()
            .position(|l| needles.iter().any(|n| !n.is_empty() && l.contains(n.as_str())));
        match hit {
            Some(p) => {
                cursor += p;
                cursor as u32 + 1
            }
            None => cursor as u32 + 1,
        }
    };
    let scalar = |v: &serde_yaml::Value| -> Option<String> {
        match v {
            serde_yaml::Value::String(s) => Some(s.clone()),
            serde_yaml::Value::Bool(b) => Some(b.to_string()),
            serde_yaml::Value::Number(n) => Some(n.to_string()),
            _ => None,
        }
    };
    let map_of = |v: &serde_yaml::Value| -> BTreeMap<String, String> {
        v.as_mapping()
            .map(|m| m.iter().filter_map(|(k, v)| Some((k.as_str()?.to_string(), scalar(v)?))).collect())
            .unwrap_or_default()
    };
    let mut out = vec![];
    let Some(log) = v.get("databaseChangeLog").and_then(|l| l.as_sequence()) else { return out };
    for item in log {
        let Some(cs) = item.get("changeSet") else { continue };
        let Some(changes) = cs.get("changes").and_then(|c| c.as_sequence()) else { continue };
        for c in changes {
            let Some(m) = c.as_mapping() else { continue };
            for (k, body) in m {
                let Some(kind) = k.as_str() else { continue };
                let attrs = map_of(body);
                let line = find_line(&[format!("{kind}:"), format!("\"{kind}\"")]);
                let mut attr_lines = BTreeMap::new();
                for key in attrs.keys() {
                    let start = line as usize;
                    if let Some(p) = lines.iter().skip(start).take(40).position(|l| {
                        let t = l.trim_start().trim_start_matches("- ");
                        t.starts_with(&format!("{key}:")) || t.starts_with(&format!("\"{key}\""))
                    }) {
                        attr_lines.insert(key.clone(), (start + p) as u32 + 1);
                    }
                }
                let mut columns = vec![];
                for col in body.get("columns").and_then(|c| c.as_sequence()).into_iter().flatten() {
                    let Some(cv) = col.get("column") else { continue };
                    let ca = map_of(cv);
                    let cons = cv.get("constraints").map(map_of).unwrap_or_default();
                    let name = ca.get("name").cloned().unwrap_or_default();
                    let cl = find_line(&[
                        format!("name: {name}"),
                        format!("\"name\": \"{name}\""),
                        format!("name: \"{name}\""),
                    ]);
                    columns.push((ca, cons, cl));
                }
                out.push(Change { kind: kind.to_string(), attrs, columns, line, attr_lines });
            }
        }
    }
    out
}

fn truthy(m: &BTreeMap<String, String>, k: &str) -> bool {
    m.get(k).is_some_and(|v| v.eq_ignore_ascii_case("true"))
}

fn column_from(
    path: &str,
    ca: &BTreeMap<String, String>,
    cons: &BTreeMap<String, String>,
    line: u32,
) -> Option<(Column, Option<(String, String)>)> {
    let name = ca.get("name")?.trim_matches('"').to_string();
    let pk = truthy(cons, "primaryKey");
    let mut references = None;
    let mut rel = None;
    if let Some(t) = cons.get("referencedTableName") {
        let rc = cons.get("referencedColumnNames").cloned().unwrap_or_else(|| "id".into());
        references = Some(format!("{}.{}", t.to_lowercase(), rc));
        rel = Some((t.to_lowercase(), name.clone()));
    } else if let Some(r) = cons.get("references") {
        // `customers(id)`
        let (t, c) = r
            .split_once('(')
            .map(|(t, c)| (t.trim().to_lowercase(), c.trim_end_matches(')').to_string()))
            .unwrap_or((r.to_lowercase(), "id".into()));
        references = Some(format!("{t}.{c}"));
        rel = Some((t, name.clone()));
    }
    let default =
        ["defaultValue", "defaultValueNumeric", "defaultValueBoolean", "defaultValueComputed", "defaultValueDate"]
            .iter()
            .find_map(|k| ca.get(*k).cloned());
    Some((
        Column {
            name: name.clone(),
            type_name: ca.get("type").cloned().unwrap_or_default(),
            primary_key: pk,
            nullable: !pk && !cons.get("nullable").is_some_and(|v| v.eq_ignore_ascii_case("false")),
            unique: truthy(cons, "unique") || pk,
            references,
            default,
            constraints: cons.get("checkConstraint").map(|c| vec![format!("CHECK ({c})")]).unwrap_or_default(),
            evidence: line_ref(path, line),
        },
        rel,
    ))
}

fn apply(path: &str, ch: &Change, unit: Option<String>, tables: &mut Vec<RawEntity>) {
    let a = &ch.attrs;
    let table_attr = |k: &str| a.get(k).map(|t| t.trim_matches('"').to_lowercase());
    let find = |tables: &mut Vec<RawEntity>, t: &str| tables.iter().position(|e| e.table == t);
    match ch.kind.as_str() {
        "createTable" => {
            let Some(t) = table_attr("tableName") else { return };
            let mut e = RawEntity {
                name: t.clone(),
                table: t.clone(),
                source: "liquibase".into(),
                unit,
                columns: vec![],
                relations: vec![],
                evidence: line_ref(path, ch.line),
            };
            for (ca, cons, line) in &ch.columns {
                if let Some((c, rel)) = column_from(path, ca, cons, *line) {
                    if let Some((target, via)) = rel {
                        e.relations.push(RawRelation {
                            kind: "many-to-one".into(),
                            target,
                            via,
                            evidence: line_ref(path, *line),
                        });
                    }
                    e.columns.push(c);
                }
            }
            let pks = e.columns.iter().filter(|c| c.primary_key).count();
            if pks > 1 {
                for c in e.columns.iter_mut().filter(|c| c.primary_key) {
                    c.unique = false;
                }
            }
            if let Some(i) = find(tables, &t) {
                tables.remove(i);
            }
            tables.push(e);
        }
        "addColumn" => {
            let Some(t) = table_attr("tableName") else { return };
            let Some(i) = find(tables, &t) else { return };
            for (ca, cons, line) in &ch.columns {
                if let Some((c, rel)) = column_from(path, ca, cons, *line) {
                    if let Some((target, via)) = rel {
                        tables[i].relations.push(RawRelation {
                            kind: "many-to-one".into(),
                            target,
                            via,
                            evidence: line_ref(path, *line),
                        });
                    }
                    tables[i].columns.retain(|x| x.name != c.name);
                    tables[i].columns.push(c);
                }
            }
        }
        "dropColumn" => {
            let Some(t) = table_attr("tableName") else { return };
            let Some(i) = find(tables, &t) else { return };
            let mut names: Vec<String> = a.get("columnName").into_iter().cloned().collect();
            names.extend(ch.columns.iter().filter_map(|(ca, _, _)| ca.get("name").cloned()));
            tables[i].columns.retain(|c| !names.contains(&c.name));
            tables[i].relations.retain(|r| !names.contains(&r.via));
        }
        "renameColumn" => {
            let Some(t) = table_attr("tableName") else { return };
            let Some(i) = find(tables, &t) else { return };
            let (Some(old), Some(new)) = (a.get("oldColumnName"), a.get("newColumnName")) else { return };
            for c in tables[i].columns.iter_mut().filter(|c| &c.name == old) {
                c.name = new.clone();
                c.evidence = line_ref(path, ch.at("newColumnName"));
            }
            for r in tables[i].relations.iter_mut().filter(|r| &r.via == old) {
                r.via = new.clone();
            }
        }
        "renameTable" => {
            let (Some(old), Some(new)) = (table_attr("oldTableName"), table_attr("newTableName")) else { return };
            if let Some(i) = find(tables, &old) {
                tables[i].table = new.clone();
                tables[i].name = new;
                tables[i].evidence = line_ref(path, ch.at("newTableName"));
            }
        }
        "dropTable" => {
            if let Some(t) = table_attr("tableName") {
                tables.retain(|e| e.table != t);
            }
        }
        "addForeignKeyConstraint" => {
            let (Some(base), Some(target)) = (table_attr("baseTableName"), table_attr("referencedTableName")) else {
                return;
            };
            let Some(i) = find(tables, &base) else { return };
            let cols = a.get("baseColumnNames").cloned().unwrap_or_default();
            let rcols = a.get("referencedColumnNames").cloned().unwrap_or_else(|| "id".into());
            let pairs: Vec<(String, String)> = cols
                .split(',')
                .map(|s| s.trim().to_string())
                .zip(rcols.split(',').map(|s| s.trim().to_string()).chain(std::iter::repeat("id".to_string())))
                .collect();
            for (c, rc) in &pairs {
                if let Some(col) = tables[i].columns.iter_mut().find(|x| &x.name == c) {
                    col.references = Some(format!("{target}.{rc}"));
                }
            }
            tables[i].relations.push(RawRelation {
                kind: "many-to-one".into(),
                target,
                via: cols.replace(' ', ""),
                evidence: line_ref(path, ch.at("baseColumnNames")),
            });
        }
        "addPrimaryKey" => {
            let Some(t) = table_attr("tableName") else { return };
            let Some(i) = find(tables, &t) else { return };
            let cols: Vec<String> =
                a.get("columnNames").map(|c| c.split(',').map(|s| s.trim().to_string()).collect()).unwrap_or_default();
            for c in tables[i].columns.iter_mut().filter(|c| cols.contains(&c.name)) {
                c.primary_key = true;
                c.nullable = false;
                c.unique = cols.len() == 1;
            }
        }
        "addUniqueConstraint" => {
            let Some(t) = table_attr("tableName") else { return };
            let Some(i) = find(tables, &t) else { return };
            let cols: Vec<String> =
                a.get("columnNames").map(|c| c.split(',').map(|s| s.trim().to_string()).collect()).unwrap_or_default();
            if cols.len() == 1 {
                for c in tables[i].columns.iter_mut().filter(|c| c.name == cols[0]) {
                    c.unique = true;
                }
            }
        }
        "addNotNullConstraint" | "dropNotNullConstraint" => {
            let Some(t) = table_attr("tableName") else { return };
            let Some(i) = find(tables, &t) else { return };
            let Some(col) = a.get("columnName") else { return };
            for c in tables[i].columns.iter_mut().filter(|c| &c.name == col) {
                c.nullable = ch.kind == "dropNotNullConstraint";
            }
        }
        "modifyDataType" => {
            let Some(t) = table_attr("tableName") else { return };
            let Some(i) = find(tables, &t) else { return };
            let (Some(col), Some(ty)) = (a.get("columnName"), a.get("newDataType")) else { return };
            for c in tables[i].columns.iter_mut().filter(|c| &c.name == col) {
                c.type_name = ty.clone();
            }
        }
        "addDefaultValue" => {
            let Some(t) = table_attr("tableName") else { return };
            let Some(i) = find(tables, &t) else { return };
            let Some(col) = a.get("columnName") else { return };
            let v = ["defaultValue", "defaultValueNumeric", "defaultValueBoolean", "defaultValueComputed"]
                .iter()
                .find_map(|k| a.get(*k).cloned());
            for c in tables[i].columns.iter_mut().filter(|c| &c.name == col) {
                c.default = v.clone();
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_and_yaml_changesets() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<databaseChangeLog xmlns="http://www.liquibase.org/xml/ns/dbchangelog">
  <changeSet id="1" author="a">
    <createTable tableName="customers">
      <column name="id" type="BIGINT" autoIncrement="true">
        <constraints primaryKey="true" nullable="false"/>
      </column>
      <column name="email" type="VARCHAR(255)">
        <constraints nullable="false" unique="true"/>
      </column>
    </createTable>
    <createTable tableName="orders">
      <column name="id" type="BIGINT"><constraints primaryKey="true"/></column>
      <column name="customer" type="BIGINT"/>
    </createTable>
  </changeSet>
  <changeSet id="2" author="a">
    <renameColumn tableName="orders" oldColumnName="customer" newColumnName="customer_id"/>
    <addForeignKeyConstraint baseTableName="orders" baseColumnNames="customer_id" referencedTableName="customers" referencedColumnNames="id" constraintName="fk"/>
  </changeSet>
</databaseChangeLog>"#;
        let mut tables = vec![];
        parse("db/changelog/1.xml", xml, None, &mut tables);
        let orders = tables.iter().find(|t| t.table == "orders").unwrap();
        let c = orders.columns.iter().find(|c| c.name == "customer_id").unwrap();
        assert_eq!((c.references.as_deref(), c.evidence.start_line), (Some("customers.id"), 18));
        assert_eq!(orders.relations[0].via, "customer_id");
        let email = tables[0].columns.iter().find(|c| c.name == "email").unwrap();
        assert!(!email.nullable && email.unique);
        assert_eq!(email.evidence.start_line, 8);

        let yaml = "databaseChangeLog:\n  - changeSet:\n      id: 3\n      author: a\n      changes:\n        - addColumn:\n            tableName: orders\n            columns:\n              - column:\n                  name: status\n                  type: varchar(20)\n                  defaultValue: NEW\n";
        parse("db/changelog/2.yaml", yaml, None, &mut tables);
        let orders = tables.iter().find(|t| t.table == "orders").unwrap();
        let s = orders.columns.iter().find(|c| c.name == "status").unwrap();
        assert_eq!((s.default.as_deref(), s.evidence.start_line), (Some("NEW"), 10));
    }
}
