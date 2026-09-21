//! Data model: persisted entities (SQL DDL, ORM models) with columns, keys and
//! relations, and the state machines their status fields implement.

use std::collections::BTreeSet;
use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod access;
mod alembic;
mod csharp;
mod go;
mod inherit;
mod java;
mod java_syntax;
mod kotlin;
mod liquibase;
mod merge;
mod prisma;
mod python;
mod raw;
mod rust_orm;
mod sql;
mod states;
mod ts;

use crate::scan::EvidenceRef;
use crate::source::SourceIndex;

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DataModel {
    pub entities: Vec<Entity>,
    pub state_machines: Vec<StateMachine>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Entity {
    /// `entity:<table>`.
    pub id: String,
    /// Code-level name (class / struct / model); table name when only DDL exists.
    pub name: String,
    /// Physical table / collection name.
    pub table: String,
    /// `sql-ddl`, `prisma`, `sqlmodel`, `sqlalchemy`, `django`, `typeorm`, `gorm`, `diesel`, …
    pub source: String,
    /// Units whose code declares or uses it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<String>,
    pub columns: Vec<Column>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<Relation>,
    /// Code sites that read / write it (queries, ORM calls), with the unit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reads: Vec<Access>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub writes: Vec<Access>,
    pub evidence: EvidenceRef,
    /// Business domain the entity belongs to (package / module below the
    /// application root, FK cluster otherwise); `None` when the model is small
    /// enough to read as one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub name: String,
    pub type_name: String,
    #[serde(default)]
    pub primary_key: bool,
    #[serde(default)]
    pub nullable: bool,
    #[serde(default)]
    pub unique: bool,
    /// `table.column` this references.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub references: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// Validation / check constraints (`CHECK (amount > 0)`, `max_length=255`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<String>,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Relation {
    /// `one-to-one`, `one-to-many`, `many-to-one`, `many-to-many`.
    pub kind: String,
    /// Target entity id.
    pub target: String,
    /// Column(s) or join table carrying it.
    pub via: String,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Access {
    pub unit: String,
    /// Enclosing function.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StateMachine {
    /// `state:<entity>.<field>` or `state:<EnumName>`.
    pub id: String,
    /// What carries the state, e.g. `orders.status`.
    pub subject: String,
    pub states: Vec<State>,
    pub transitions: Vec<Transition>,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub name: String,
    #[serde(default)]
    pub initial: bool,
    #[serde(default)]
    pub terminal: bool,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Transition {
    /// Source state when the code states it (`WHERE status = 'paid'`); `None` = from any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    pub to: String,
    pub unit: String,
    /// Function that performs it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    /// Guard condition as written, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guard: Option<String>,
    pub evidence: EvidenceRef,
}

const SKIP_DIRS: &[&str] =
    &["node_modules", "target", "vendor", ".git", "dist", "build", ".venv", "venv", "__pycache__", ".next", "coverage"];

fn is_migration(path: &str) -> bool {
    path.split('/')
        .any(|seg| matches!(seg, "migrations" | "migration" | "migrate" | "alembic" | "versions" | "seeds" | "seeders"))
}

/// Splits on `;` outside quotes.
fn split_statements(sql: &str) -> Vec<&str> {
    let mut out = vec![];
    let mut quote = false;
    let mut last = 0;
    for (i, c) in sql.char_indices() {
        match c {
            '\'' => quote = !quote,
            ';' if !quote => {
                out.push(&sql[last..i]);
                last = i + 1;
            }
            _ => {}
        }
    }
    out.push(&sql[last..]);
    out.into_iter().filter(|s| !s.trim().is_empty()).collect()
}

/// `*.sql` and `*.prisma` files (not part of the parsed source index), sorted by path.
fn schema_files(root: &Path) -> Vec<String> {
    let mut out = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let p = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) && !name.starts_with('.') {
                    stack.push(p);
                }
                continue;
            }
            let lower = name.to_lowercase();
            let down = lower == "down.sql" || lower.ends_with(".down.sql") || lower.ends_with("_down.sql");
            if (lower.ends_with(".sql") && !down) || lower.ends_with(".prisma") {
                if let Ok(rel) = p.strip_prefix(root) {
                    let rel = rel.to_string_lossy().replace('\\', "/");
                    // JVM test resources hold fixture schemas, not the application's.
                    if !rel.contains("src/test/") {
                        out.push(rel);
                    }
                }
            }
        }
    }
    out.sort();
    out
}

/// Liquibase changelog candidates: XML / YAML / JSON files declaring a `databaseChangeLog`.
fn changelog_candidates(root: &Path) -> Vec<String> {
    let mut out = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let p = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) && !name.starts_with('.') && name != "test" {
                    stack.push(p);
                }
                continue;
            }
            let lower = name.to_lowercase();
            if !(lower.ends_with(".xml")
                || lower.ends_with(".yaml")
                || lower.ends_with(".yml")
                || lower.ends_with(".json"))
            {
                continue;
            }
            let Ok(rel) = p.strip_prefix(root) else { continue };
            let rel = rel.to_string_lossy().replace('\\', "/");
            let rl = rel.to_lowercase();
            if !(rl.contains("changelog") || rl.contains("liquibase")) {
                continue;
            }
            if entry.metadata().is_ok_and(|m| m.len() > 4_000_000) {
                continue;
            }
            if std::fs::read_to_string(&p).is_ok_and(|t| t.contains("databaseChangeLog")) {
                out.push(rel);
            }
        }
    }
    out.sort();
    out
}

/// Extracts entities and state machines from DDL, ORM models and data-access code.
pub fn extract(index: &SourceIndex) -> DataModel {
    use crate::lang::Language;

    let unit_for = |path: &str| -> Option<String> {
        index
            .units
            .iter()
            .filter(|u| u.root.is_empty() || path.starts_with(&format!("{}/", u.root)))
            .max_by_key(|u| u.root.len())
            .map(|u| u.id.clone())
    };

    let mut raw: Vec<raw::RawEntity> = vec![];
    let mut rows: Vec<raw::RawEntity> = vec![];
    let mut enums: Vec<raw::RawEnum> = vec![];
    let mut uses: Vec<raw::RawEnumUse> = vec![];
    let mut sql_out = sql::SqlOutput { entities: vec![], enums: vec![], checks: vec![] };
    let mut prisma_entities = vec![];
    let mut texts: Vec<(usize, String)> = vec![];
    for (i, f) in index.files.iter().enumerate() {
        if let Some(t) = index.read(f.path) {
            texts.push((i, t));
        }
    }
    // DDL sources in path order: `.sql` / `.prisma` files and SQL strings inside code migrations.
    let mut ddl: Vec<(String, Option<usize>)> = schema_files(&index.root).into_iter().map(|p| (p, None)).collect();
    for (k, (i, t)) in texts.iter().enumerate() {
        if is_migration(index.files[*i].path) && t.to_uppercase().contains("CREATE TABLE") {
            ddl.push((index.files[*i].path.to_string(), Some(k)));
        }
    }
    ddl.sort();
    for (path, code) in ddl {
        if let Some(k) = code {
            let full = &texts[k].1;
            // Only the forward migration: blank `down()` / `downgrade()` onwards (it recreates dropped tables).
            let down = ["function down(", "async down(", "def downgrade", "func Down", "fn down("]
                .iter()
                .filter_map(|m| full.find(m))
                .min()
                .unwrap_or(full.len());
            let up_only: String =
                full.char_indices().map(|(i, c)| if i >= down && c != '\n' { ' ' } else { c }).collect();
            let masked = sql::string_contents(&up_only, index.files[texts[k].0].language == Language::Python);
            sql::parse_file(&path, &masked, &mut sql_out);
            continue;
        }
        let Some(text) = index.read(&path) else { continue };
        if path.ends_with(".prisma") {
            let unit = unit_for(&path);
            let (n, ne) = (prisma_entities.len(), enums.len());
            prisma::parse(&path, &text, &mut prisma_entities, &mut enums, &mut uses);
            for e in &mut prisma_entities[n..] {
                e.unit = unit.clone();
            }
            for e in &mut enums[ne..] {
                e.unit = unit.clone();
            }
        } else {
            // goose / dbmate files carry both directions: keep the up part.
            let lower = text.to_lowercase();
            let down = ["-- +goose down", "-- migrate:down", "-- +migrate down"]
                .iter()
                .filter_map(|m| lower.find(m))
                .min()
                .unwrap_or(text.len());
            sql::parse_file(&path, &text[..down], &mut sql_out);
        }
    }
    raw.extend(std::mem::take(&mut sql_out.entities));
    raw.extend(prisma_entities);
    let mut liquibase_tables = vec![];
    for path in liquibase::changelog_files(&index.root, &changelog_candidates(&index.root)) {
        if let Some(text) = index.read(&path) {
            liquibase::parse(&path, &text, unit_for(&path), &mut liquibase_tables);
        }
    }
    raw.extend(liquibase_tables);
    enums.extend(std::mem::take(&mut sql_out.enums));

    let group = |langs: &[Language], migrations: bool| -> Vec<(String, String, String)> {
        texts
            .iter()
            .filter(|(i, _)| {
                langs.contains(&index.files[*i].language) && is_migration(index.files[*i].path) == migrations
            })
            .map(|(i, t)| (index.files[*i].path.to_string(), index.files[*i].unit.to_string(), t.clone()))
            .collect()
    };

    let mut alembic_entities = vec![];
    for (path, _, text) in group(&[Language::Python], true) {
        alembic::parse(&path, &text, &mut alembic_entities);
    }
    raw.extend(alembic_entities);

    let py = python::parse(&group(&[Language::Python], false));
    raw.extend(py.entities);
    enums.extend(py.enums);
    uses.extend(py.enum_uses);

    let ts_out = ts::parse(&group(&[Language::TypeScript, Language::JavaScript], false));
    raw.extend(ts_out.entities);
    enums.extend(ts_out.enums);
    uses.extend(ts_out.enum_uses);

    let go_out = go::parse(&group(&[Language::Go], false));
    raw.extend(go_out.entities);
    rows.extend(go_out.row_structs);
    enums.extend(go_out.enums);
    uses.extend(go_out.enum_uses);

    // Spring Boot applies its snake_case naming strategy to JPA entities.
    let spring_units: BTreeSet<String> = index
        .files
        .iter()
        .filter(|f| {
            matches!(f.language, Language::Java | Language::Kotlin)
                && f.facts.imports.iter().any(|i| i.specifier.starts_with("org.springframework"))
        })
        .map(|f| f.unit.to_string())
        .collect();
    let jv = java::parse(&group(&[Language::Java], false), &spring_units);
    raw.extend(jv.entities);
    enums.extend(jv.enums);
    uses.extend(jv.enum_uses);

    let kt = kotlin::parse(&group(&[Language::Kotlin], false), &spring_units);
    raw.extend(kt.entities);
    enums.extend(kt.enums);
    uses.extend(kt.enum_uses);

    let cs = csharp::parse(&group(&[Language::CSharp], false));
    raw.extend(cs.entities);
    enums.extend(cs.enums);
    uses.extend(cs.enum_uses);

    let rs = rust_orm::parse(&group(&[Language::Rust], false));
    raw.extend(rs.entities);
    rows.extend(rs.row_structs);
    enums.extend(rs.enums);
    uses.extend(rs.enum_uses);

    let merged = merge::merge(raw, rows);
    let mut entities = merged.entities;
    let table_of = |name: &str| merged.aliases.get(&name.to_lowercase()).cloned();

    // Sources for access and transitions.
    let mut sources: Vec<states::Src> = vec![];
    let mut sql_stmts: Vec<(usize, u32, access::SqlStmt)> = vec![];
    for (i, text) in &texts {
        let f = &index.files[*i];
        let migration = is_migration(f.path);
        let mut sql_lines = BTreeSet::new();
        let si = sources.len();
        if !migration {
            for s in &f.facts.strings {
                if !access::looks_like_sql(&s.value) {
                    continue;
                }
                let full = access::full_literal(text, s.line, &s.value);
                for k in 0..=full.matches('\n').count() as u32 {
                    sql_lines.insert(s.line + k);
                }
                // One literal may hold several statements (`BEGIN; INSERT …; INSERT …; COMMIT`).
                for part in split_statements(&full) {
                    if let Some(st) = access::parse_sql(part) {
                        sql_stmts.push((si, s.line, st));
                    }
                }
            }
        }
        let test_from = text.lines().position(|l| l.trim() == "#[cfg(test)]").map(|p| p as u32 + 1);
        sources.push(states::Src {
            path: f.path.to_string(),
            unit: f.unit.to_string(),
            text: text.clone(),
            facts: f.facts,
            migration,
            sql_lines,
            test_from,
        });
    }

    let push_access =
        |entities: &mut Vec<Entity>, table: &str, write: bool, src: &states::Src, line: u32, note: Option<&str>| {
            // Two services can declare the same table name; attribute to the one
            // this code belongs to, and to nothing when that is ambiguous.
            let idx = entities.iter().position(|e| e.table == table && e.units.contains(&src.unit)).or_else(|| {
                let mut same = entities.iter().enumerate().filter(|(_, e)| e.table == table);
                match (same.next(), same.next()) {
                    (Some((i, _)), None) => Some(i),
                    _ => None,
                }
            });
            let Some(e) = idx.map(|i| &mut entities[i]) else { return };
            let symbol = access::enclosing(src.facts, line).map(|s| s.0);
            if src.is_test(line, symbol.as_deref()) {
                return;
            }
            let mut evidence = crate::source::line_ref(&src.path, line);
            evidence.note = note.map(str::to_string);
            let a = Access { unit: src.unit.clone(), symbol, evidence };
            let list = if write { &mut e.writes } else { &mut e.reads };
            if !list.iter().any(|x| x.evidence == a.evidence) {
                list.push(a);
            }
            if !e.units.contains(&src.unit) {
                e.units.push(src.unit.clone());
            }
        };
    // Kysely: `.selectFrom('album')`, `.insertInto('asset')`, `.updateTable(…)`, `.deleteFrom(…)`.
    for (si, src) in sources.iter().enumerate() {
        if src.migration
            || !matches!(index.files[texts[si].0].language, Language::TypeScript | Language::JavaScript)
            || !src.text.contains("selectFrom(")
                && !src.text.contains("insertInto(")
                && !src.text.contains("updateTable(")
                && !src.text.contains("deleteFrom(")
        {
            continue;
        }
        for (li, l) in src.text.lines().enumerate() {
            for (call, write) in [
                ("selectFrom(", false),
                ("innerJoin(", false),
                ("leftJoin(", false),
                ("insertInto(", true),
                ("updateTable(", true),
                ("deleteFrom(", true),
            ] {
                let mut from = 0;
                while let Some(p) = l[from..].find(call) {
                    let at = from + p + call.len();
                    from = at;
                    let arg = l[at..].trim_start();
                    if !arg.starts_with(['\'', '"']) {
                        continue;
                    }
                    let name = raw::unquote(arg.split([',', ')']).next().unwrap_or(""));
                    let name =
                        name.split_whitespace().next().unwrap_or("").rsplit('.').next().unwrap_or("").to_string();
                    if let Some(t) = table_of(&name) {
                        push_access(&mut entities, &t, write, src, li as u32 + 1, None);
                    }
                }
            }
        }
    }
    for a in java::access(&jv.model, &sources) {
        let table = match a.name.strip_prefix('#') {
            Some(t) => table_of(t),
            None => table_of(&a.name),
        };
        if let Some(t) = table {
            push_access(&mut entities, &t, a.write, &sources[a.src], a.line, a.note.as_deref());
        }
    }
    for a in kotlin::access(&kt.model, &sources) {
        let table = match a.name.strip_prefix('#') {
            Some(t) => table_of(t),
            None => table_of(&a.name),
        };
        if let Some(t) = table {
            push_access(&mut entities, &t, a.write, &sources[a.src], a.line, a.note.as_deref());
        }
    }
    for (si, line, st) in &sql_stmts {
        let src = &sources[*si];
        if let Some(t) = st.target.as_deref().and_then(table_of) {
            push_access(&mut entities, &t, true, src, *line, None);
        }
        for r in &st.reads {
            if let Some(t) = table_of(r) {
                push_access(&mut entities, &t, false, src, *line, None);
            }
        }
    }
    for (si, src) in sources.iter().enumerate() {
        if src.migration {
            continue;
        }
        let lang = index.files[texts[si].0].language;
        for (table, names) in &merged.code_names {
            for (name, source) in names {
                let fits = match source.as_str() {
                    "prisma" | "typeorm" | "drizzle" => matches!(lang, Language::TypeScript | Language::JavaScript),
                    "sqlmodel" | "sqlalchemy" | "django" => lang == Language::Python,
                    "gorm" => lang == Language::Go,
                    "diesel" | "diesel-model" | "sqlx" => lang == Language::Rust,
                    "ef-core" => lang == Language::CSharp,
                    _ => false,
                };
                if !fits {
                    continue;
                }
                let camel = {
                    let mut c = name.chars();
                    c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default()
                };
                // Every match below needs the name (or its camelCase / table form) on
                // some line; most files never mention a given entity.
                let mentioned = src.text.contains(name.as_str())
                    || (source == "prisma" && src.text.contains(&camel))
                    || (source == "diesel" && src.text.contains(table.as_str()));
                if !mentioned {
                    continue;
                }
                // `item = Item(...)` … `session.add(item)`, `order := Order{…}` … `db.Create(&order)`,
                // `repo = ds.getRepository(Invoice)` … `repo.save(…)`.
                let mut instances: Vec<String> = vec![];
                let mut repos: Vec<String> = vec![];
                let mut last_write: Option<usize> = None;
                for (li, l) in src.text.lines().enumerate() {
                    let t = l.trim();
                    let decl = t
                        .trim_start_matches("var ")
                        .trim_start_matches("let ")
                        .trim_start_matches("const ")
                        .trim_start_matches("mut ");
                    let assign = [" = ", " := "].iter().find_map(|sep| decl.split_once(sep));
                    if let Some((lhs, rhs)) = assign {
                        let var = lhs.split(':').next().unwrap_or("").trim().trim_start_matches("this.");
                        let rhs = rhs.trim_start_matches("await ").trim_start_matches('&').trim_start_matches("new ");
                        // `models.Order{…}` → `Order{…}`
                        let rhs = match rhs.split_once('.') {
                            Some((pkg, rest))
                                if pkg.chars().all(|c| c.is_ascii_lowercase()) && rest.starts_with(name.as_str()) =>
                            {
                                rest
                            }
                            _ => rhs,
                        };
                        if !var.is_empty() && var.chars().all(|c| c.is_alphanumeric() || c == '_') {
                            if rhs.starts_with(&format!("{name}("))
                                || rhs.starts_with(&format!("{name}{{"))
                                || rhs.starts_with(&format!("[]{name}{{"))
                                || rhs.starts_with(&format!("{name}.objects.get("))
                                || rhs.starts_with(&format!("{name}.model_validate("))
                                || rhs.contains(&format!(".get({name},"))
                            {
                                instances.push(var.to_string());
                            }
                            if rhs.contains(&format!("getRepository({name})"))
                                || rhs.contains(&format!("Repository<{name}>"))
                            {
                                repos.push(var.to_string());
                            }
                        }
                    } else if l.trim_start().starts_with("var ") {
                        // Go: `var orders []Order` / `var order Order`
                        let mut p = decl.split_whitespace();
                        if let (Some(var), Some(ty)) = (p.next(), p.next()) {
                            if ty.trim_start_matches("[]").rsplit('.').next() == Some(name.as_str()) {
                                instances.push(var.to_string());
                            }
                        }
                    }
                    if let Some((var, rest)) = t.split_once(": ").filter(|_| t.contains(&format!("Repository<{name}>")))
                    {
                        let var = var
                            .trim_start_matches("private ")
                            .trim_start_matches("readonly ")
                            .trim_start_matches("public ")
                            .trim();
                        if !rest.is_empty() && var.chars().all(|c| c.is_alphanumeric() || c == '_') {
                            repos.push(var.to_string());
                        }
                    }
                    let ln = li as u32 + 1;
                    let repo_hit = repos.iter().find_map(|r| {
                        let at = t.find(&format!("{r}."))?;
                        let verb: String = t[at + r.len() + 1..].chars().take_while(|c| c.is_alphanumeric()).collect();
                        match verb.as_str() {
                            "save" | "insert" | "update" | "delete" | "remove" | "upsert" | "softDelete" => Some(true),
                            v if v.starts_with("find") || v == "count" || v == "exist" || v == "createQueryBuilder" => {
                                Some(false)
                            }
                            _ => None,
                        }
                    });
                    if let Some(write) = repo_hit {
                        push_access(&mut entities, table, write, src, ln, None);
                        continue;
                    }
                    if !instances.is_empty() {
                        let used = |verbs: &[&str]| {
                            instances.iter().any(|v| {
                                verbs.iter().any(|verb| {
                                    t.contains(&format!("{verb}{v})")) || t.contains(&format!("{verb}{v},"))
                                }) || t.starts_with(&format!("{v}.save(")) && verbs.contains(&".save(")
                            })
                        };
                        if used(&[
                            ".add(",
                            ".save(",
                            ".merge(",
                            ".delete(",
                            "Create(&",
                            "Save(&",
                            "Delete(&",
                            ".persist(",
                            ".insert(",
                        ]) {
                            push_access(&mut entities, table, true, src, ln, None);
                            continue;
                        }
                        if used(&["Find(&", "First(&", "Take(&", "Last(&"]) {
                            push_access(&mut entities, table, false, src, ln, None);
                            continue;
                        }
                    }
                    let quick = l.contains(name.as_str())
                        || (source == "prisma" && l.contains(&camel))
                        || (source == "diesel" && l.contains(table.as_str()));
                    if !quick || src.sql_lines.contains(&(li as u32 + 1)) {
                        continue;
                    }
                    let found = if source == "sqlx" {
                        (l.contains("query_as") && access::is_word_at(l, name)).then_some(false)
                    } else {
                        access::orm_access(l, name, table, source)
                    };
                    match found {
                        // A chained statement (`diesel::update(…)\n.filter(…)`) is one write.
                        Some(false)
                            if last_write.is_some_and(|w| li - w <= 3)
                                || entities.iter().find(|e| &e.table == table).is_some_and(|e| {
                                    e.writes.iter().any(|w| {
                                        w.evidence.file_path == src.path
                                            && w.evidence.start_line < li as u32 + 1
                                            && li as u32 + 1 - w.evidence.start_line <= 3
                                    })
                                }) => {}
                        Some(write) => {
                            if write {
                                last_write = Some(li);
                            }
                            push_access(&mut entities, table, write, src, li as u32 + 1, None);
                        }
                        None => {}
                    }
                }
            }
        }
    }
    for e in &mut entities {
        e.units.sort();
        e.units.dedup();
    }
    // Tables later renamed by statements we cannot evaluate (`RENAME TO "${newName}"`) linger as
    // DDL-only, unused entities next to their code-declared successor (`albums` → `album`): drop them.
    let singular = |t: &str| {
        t.split('_').map(|p| p.strip_suffix('s').filter(|x| x.len() > 2).unwrap_or(p)).collect::<Vec<_>>().join("_")
    };
    let declared: BTreeSet<String> = entities
        .iter()
        .filter(|e| e.source.split('+').any(|s| !matches!(s, "sql-ddl" | "alembic" | "liquibase")))
        .map(|e| singular(&e.table))
        .collect();
    let stale: BTreeSet<String> = entities
        .iter()
        .filter(|e| {
            matches!(e.source.as_str(), "sql-ddl" | "alembic" | "liquibase")
                && e.reads.is_empty()
                && e.writes.is_empty()
                && declared.contains(&singular(&e.table))
        })
        .map(|e| e.id.clone())
        .collect();
    entities.retain(|e| !stale.contains(&e.id));
    for e in &mut entities {
        e.relations.retain(|r| !stale.contains(&r.target));
    }

    let read_text = |p: &str| index.read(p);
    let machines = states::extract(&states::Inputs {
        entities: &entities,
        enums: &enums,
        checks: &sql_out.checks,
        enum_uses: &uses,
        aliases: &merged.aliases,
        code_names: &merged.code_names,
        sources: &sources,
        sql: &sql_stmts,
        read_text: &read_text,
        extra: &jv.transitions,
    });
    crate::domains::assign(&mut entities, index);
    DataModel { entities, state_machines: machines }
}
