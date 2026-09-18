//! Groups raw entities by physical table: DDL supplies columns and keys,
//! ORM models supply code names, validation and relations.

use std::collections::{BTreeMap, HashMap};

use super::raw::*;
use super::{Column, Entity, Relation};

fn physical_rank(source: &str) -> Option<u8> {
    match source {
        "sql-ddl" => Some(0),
        "alembic" | "liquibase" => Some(1),
        "diesel" => Some(2),
        _ => None,
    }
}

/// Group key → table name (the key carries the declaring unit for split tables).
fn is_row(source: &str) -> bool {
    matches!(source, "sqlx" | "go-struct" | "diesel-model")
}

pub struct Merged {
    pub entities: Vec<Entity>,
    /// Lowercase code name / guessed table → table.
    pub aliases: HashMap<String, String>,
    /// table → [(code name, source)] for ORM access matching.
    pub code_names: BTreeMap<String, Vec<(String, String)>>,
}

/// Units that declare a table in code (not DDL): two services declaring the
/// same collection own two different collections, in their own databases.
fn split_units(raw: &[RawEntity]) -> BTreeMap<String, Vec<String>> {
    let mut by_table: BTreeMap<String, (Vec<String>, bool)> = BTreeMap::new();
    for e in raw {
        let entry = by_table.entry(e.table.clone()).or_default();
        if physical_rank(&e.source).is_some() {
            // One physical schema: every declaration describes the same table.
            entry.1 = true;
            continue;
        }
        if is_row(&e.source) {
            continue;
        }
        if let Some(u) = e.unit.clone().filter(|u| !entry.0.contains(u)) {
            entry.0.push(u);
        }
    }
    by_table
        .into_iter()
        .filter(|(_, (units, physical))| units.len() > 1 && !physical)
        .map(|(table, (mut units, _))| {
            units.sort();
            (table, units)
        })
        .collect()
}

/// `accounts` declared by `account-service` → `entity:account-service.accounts`.
fn split_id(table: &str, unit: &str) -> String {
    format!("entity:{unit}.{table}")
}

pub fn merge(raw: Vec<RawEntity>, row_structs: Vec<RawEntity>) -> Merged {
    let split = split_units(&raw);
    // Keyed by table, and by declaring unit when one table name means two tables.
    let mut groups: BTreeMap<(String, Option<String>), Vec<RawEntity>> = BTreeMap::new();
    for e in raw {
        // System catalogs some schemas declare for introspection are not the app's data.
        if e.table.is_empty()
            || e.table.starts_with("pg_")
            || e.table.starts_with("information_schema")
            || e.table.starts_with("sqlite_")
        {
            continue;
        }
        // A split table is grouped per declaring unit; anything without a unit
        // (a row struct paired later) stays with the table itself.
        let unit = match (split.get(&e.table), e.unit.clone()) {
            (Some(units), Some(u)) if units.contains(&u) => Some(u),
            _ => None,
        };
        groups.entry((e.table.clone(), unit)).or_default().push(e);
    }
    let mut aliases: HashMap<String, String> = HashMap::new();
    for ((table, _), es) in &groups {
        aliases.insert(table.clone(), table.clone());
        for e in es {
            aliases.entry(e.name.to_lowercase()).or_insert_with(|| table.clone());
        }
    }
    // Row structs pair with an existing table only.
    let mut paired: Vec<RawEntity> = vec![];
    for s in row_structs {
        let snake = snake_case(&s.name);
        let stem = snake.trim_end_matches("_row").trim_end_matches("_record").trim_end_matches("_model").to_string();
        let candidates =
            [s.table.clone(), snake.clone(), super::go::plural(&snake), stem.clone(), super::go::plural(&stem)];
        if let Some(t) = candidates.iter().find(|c| groups.keys().any(|(t, _)| t == *c)) {
            aliases.insert(s.table.clone(), t.clone());
            aliases.entry(s.name.to_lowercase()).or_insert_with(|| t.clone());
            let mut s = s;
            s.table = t.clone();
            paired.push(s);
        }
    }
    for s in paired {
        groups.entry((s.table.clone(), None)).or_default().push(s);
    }
    let resolve = |target: &str| -> Option<String> {
        let t = target.trim_start_matches('@');
        let (head, col) = match t.split_once('.') {
            Some((h, c)) => (h, Some(c)),
            None => (t, None),
        };
        let table = aliases.get(&head.to_lowercase()).cloned()?;
        Some(match col {
            Some(c) => format!("{table}.{c}"),
            None => table,
        })
    };

    let mut entities = vec![];
    let mut code_names: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for ((table, unit_of_key), mut es) in groups {
        es.sort_by_key(|e| physical_rank(&e.source).unwrap_or(9));
        let physical = es.iter().position(|e| physical_rank(&e.source).is_some() && !e.columns.is_empty());
        let orm: Vec<&RawEntity> =
            es.iter().filter(|e| physical_rank(&e.source).is_none() && !is_row(&e.source)).collect();
        let rows: Vec<&RawEntity> = es.iter().filter(|e| is_row(&e.source)).collect();
        let base: &RawEntity = match physical.map(|i| &es[i]).or(orm.first().copied()) {
            Some(b) => b,
            None => continue,
        };
        let mut columns: Vec<Column> = base.columns.clone();
        for o in orm.iter().filter(|o| !std::ptr::eq(**o, base)) {
            for oc in &o.columns {
                let norm = |s: &str| s.replace('_', "").to_lowercase();
                match columns.iter_mut().find(|c| norm(&c.name) == norm(&oc.name)) {
                    Some(c) => {
                        for k in &oc.constraints {
                            if !c.constraints.contains(k) {
                                c.constraints.push(k.clone());
                            }
                        }
                        if c.references.is_none() {
                            c.references = oc.references.clone();
                        }
                        if c.default.is_none() {
                            c.default = oc.default.clone();
                        }
                    }
                    None if physical.is_none() => columns.push(oc.clone()),
                    None => {}
                }
            }
        }
        if physical.is_none() {
            // Inherited ORM fields come first in declaration order; keys lead.
            columns.sort_by_key(|c| !c.primary_key);
        }
        // `Customer Customer` + `CustomerID uint`: the relation names the key column.
        for r in es.iter().flat_map(|e| e.relations.iter()).filter(|r| r.kind == "many-to-one" && !r.via.contains(','))
        {
            if let Some(c) = columns.iter_mut().find(|c| c.name == r.via && c.references.is_none()) {
                c.references = Some(format!("{}.id", r.target));
            }
        }
        for c in &mut columns {
            if let Some(r) = &c.references {
                c.references = resolve(r).or_else(|| Some(r.trim_start_matches('@').to_string()));
            }
        }
        let mut relations: Vec<Relation> = vec![];
        for r in es.iter().flat_map(|e| e.relations.iter()) {
            let Some(t) = resolve(&r.target) else { continue };
            let target =
                match unit_of_key.as_deref().filter(|u| split.get(&t).is_some_and(|us| us.iter().any(|x| x == u))) {
                    Some(u) => split_id(&t, u),
                    None => format!("entity:{t}"),
                };
            if relations.iter().any(|x| {
                x.target == target
                    && (x.via == r.via || x.kind == r.kind && r.target.starts_with('@') && x.kind == "many-to-one")
            }) {
                continue;
            }
            relations.push(Relation { kind: r.kind.clone(), target, via: r.via.clone(), evidence: r.evidence.clone() });
        }
        let code = orm.first().or(rows.first()).copied();
        let mut sources: Vec<String> = vec![];
        for e in es.iter() {
            if !sources.contains(&e.source) {
                sources.push(e.source.clone());
            }
        }
        let mut units: Vec<String> = match &unit_of_key {
            Some(u) => vec![u.clone()],
            None => es.iter().filter_map(|e| e.unit.clone()).collect(),
        };
        units.sort();
        units.dedup();
        code_names.insert(
            table.clone(),
            es.iter()
                .filter(|e| !matches!(e.source.as_str(), "sql-ddl" | "alembic" | "liquibase"))
                .map(|e| (e.name.clone(), e.source.clone()))
                .collect(),
        );
        entities.push(Entity {
            id: match &unit_of_key {
                Some(u) => split_id(&table, u),
                None => format!("entity:{table}"),
            },
            name: code.map(|c| c.name.clone()).unwrap_or_else(|| base.name.clone()),
            table: table.clone(),
            source: sources.join("+"),
            units,
            columns,
            relations,
            reads: vec![],
            writes: vec![],
            evidence: code
                .filter(|c| physical_rank(&c.source).is_none())
                .map(|c| c.evidence.clone())
                .unwrap_or_else(|| base.evidence.clone()),
            domain: None,
        });
    }
    Merged { entities, aliases, code_names }
}
