//! JVM persistence: JPA / Hibernate entities (Spring Data JPA, Quarkus
//! Hibernate ORM with Panache, Micronaut Data JPA), Spring Data MongoDB
//! documents, Spring Data JDBC / R2DBC and Micronaut Data mapped entities,
//! Quarkus MongoDB Panache; Java enums; repository and EntityManager access;
//! Spring Statemachine transitions.
//!
//! Physical names follow each framework's default naming: Spring Boot's
//! JPA naming strategy snake-cases class and field names, plain Hibernate
//! keeps them, Spring Data JDBC and Micronaut Data snake-case, Spring Data
//! MongoDB uncapitalises the class name. Column types without DDL are inferred
//! from the Java type (DDL, when present, wins in the merge).

use std::collections::{BTreeSet, HashMap, HashSet};

use super::inherit::{self, DaoBase, Via};
use super::java_syntax::*;
use super::raw::*;
use super::states::Src as StateSrc;
use super::Column;
use crate::scan::EvidenceRef;

pub struct JavaOutput {
    pub entities: Vec<RawEntity>,
    pub enums: Vec<RawEnum>,
    pub enum_uses: Vec<RawEnumUse>,
    pub transitions: Vec<ExtraTransition>,
    pub model: JavaModel,
}

/// A transition declared outside entity code (Spring Statemachine configuration).
#[derive(Debug, Clone)]
pub struct ExtraTransition {
    pub enum_name: String,
    pub from: Option<String>,
    pub to: String,
    /// Member marked `.initial(…)` / `.end(…)` instead of a transition.
    pub marker: Option<&'static str>,
    pub trigger: Option<String>,
    pub unit: String,
    pub evidence: EvidenceRef,
}

/// Custom repository method → performs a write.
type RepoMethods = HashMap<String, bool>;

#[derive(Default)]
pub struct JavaModel {
    /// Repository type → (entity class, custom method → write?).
    repos: HashMap<String, (String, HashMap<String, bool>)>,
    /// Entity classes by simple name.
    entities: BTreeSet<String>,
    /// Active-record entities (Panache): static finders on the class.
    active_record: BTreeSet<String>,
    /// Shared DAO base classes and the entity each subclass binds them to.
    bases: Vec<DaoBase>,
}

/// One read or write of an entity, at the line that performs it.
pub struct Access {
    /// Entity class, or `#table` when the table is named directly.
    pub name: String,
    pub write: bool,
    /// Index into the sources given to [`access`].
    pub src: usize,
    pub line: u32,
    /// Why this line touches this entity, when the code doesn't name it
    /// (`through \`JpaDeviceDao\``).
    pub note: Option<String>,
}

impl Access {
    fn new(name: impl Into<String>, write: bool, src: usize, line: u32) -> Access {
        Access { name: name.into(), write, src, line, note: None }
    }

    fn via(name: impl Into<String>, write: bool, src: usize, line: u32, sub: &str) -> Access {
        Access { name: name.into(), write, src, line, note: Some(format!("through `{sub}`")) }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Jpa,
    Jdbc,
    Mongo,
    Micronaut,
    PanacheMongo,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Jpa => "jpa",
            Kind::Jdbc => "spring-data-jdbc",
            Kind::Mongo => "spring-data-mongodb",
            Kind::Micronaut => "micronaut-data",
            Kind::PanacheMongo => "panache-mongodb",
        }
    }
}

const REPO_BASES: &[&str] = &[
    "Repository",
    "CrudRepository",
    "ListCrudRepository",
    "PagingAndSortingRepository",
    "ListPagingAndSortingRepository",
    "JpaRepository",
    "JpaSpecificationExecutor",
    "MongoRepository",
    "ReactiveMongoRepository",
    "ReactiveCrudRepository",
    "ReactiveSortingRepository",
    "R2dbcRepository",
    "CoroutineCrudRepository",
    "RevisionRepository",
    "PanacheRepository",
    "PanacheRepositoryBase",
    "PanacheMongoRepository",
    "PanacheMongoRepositoryBase",
    "ReactivePanacheMongoRepository",
    "GenericRepository",
    "PageableRepository",
    "AsyncCrudRepository",
    "ReactorCrudRepository",
    "JpaSpecificationExecutor",
];

const PANACHE_BASES: &[&str] = &[
    "PanacheEntity",
    "PanacheEntityBase",
    "PanacheMongoEntity",
    "PanacheMongoEntityBase",
    "ReactivePanacheMongoEntity",
];

fn uncapitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn unquote_str(expr: &str, consts: &HashMap<String, String>) -> Option<String> {
    resolve_str(expr.trim(), consts)
        .map(|s| s.trim_matches('`').trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
}

/// Parses every Java file once; `spring_units` selects Spring Boot's naming strategy.
pub fn parse(files: &[(String, String, String)], spring_units: &BTreeSet<String>) -> JavaOutput {
    let mut classes: Vec<JClass> = vec![];
    let mut codes: Vec<String> = Vec::with_capacity(files.len());
    for (i, (path, _, text)) in files.iter().enumerate() {
        let src = new_src(path, text);
        parse_file(i, &src, &mut classes);
        codes.push(src.code);
    }
    let paths: Vec<String> = files.iter().map(|(p, _, _)| p.clone()).collect();
    let mut consts = HashMap::new();
    constants(&classes, &mut consts);
    let by_name: HashMap<&str, Vec<usize>> = classes.iter().enumerate().fold(HashMap::new(), |mut m, (i, c)| {
        m.entry(c.name.as_str()).or_insert_with(Vec::new).push(i);
        m
    });
    // Prefer the declaration in the same unit, then the same file.
    let find = |name: &str, file: usize| -> Option<&JClass> {
        let ids = by_name.get(name)?;
        ids.iter()
            .map(|&i| &classes[i])
            .find(|c| c.file == file)
            .or_else(|| ids.iter().map(|&i| &classes[i]).find(|c| files[c.file].1 == files[file].1))
            .or_else(|| ids.first().map(|&i| &classes[i]))
    };

    let mut out = JavaOutput {
        entities: vec![],
        enums: vec![],
        enum_uses: vec![],
        transitions: vec![],
        model: JavaModel::default(),
    };
    let enum_names: HashSet<&str> = classes.iter().filter(|c| c.kind == "enum").map(|c| c.name.as_str()).collect();
    for c in classes.iter().filter(|c| c.kind == "enum" && c.constants.len() >= 2) {
        out.enums.push(RawEnum {
            name: c.name.clone(),
            values: c.constants.iter().map(|(n, _)| (n.clone(), n.clone())).collect(),
            evidence: line_ref(&files[c.file].0, c.line),
            unit: Some(files[c.file].1.clone()),
            sql_type: false,
        });
    }

    for c in &classes {
        let path = &files[c.file].0;
        let text = &files[c.file].2;
        let unit = &files[c.file].1;
        let spring = spring_units.contains(unit);
        let extends_base = |bases: &[&str]| c.extends.iter().any(|e| bases.contains(&simple(e).as_str()));
        let kind = if c.has("Entity") {
            Some(Kind::Jpa)
        } else if c.has("Document") && text.contains("springframework.data.mongodb") {
            Some(Kind::Mongo)
        } else if c.has("MappedEntity") {
            Some(Kind::Micronaut)
        } else if c.has("MongoEntity")
            || extends_base(&["PanacheMongoEntity", "PanacheMongoEntityBase", "ReactivePanacheMongoEntity"])
        {
            Some(Kind::PanacheMongo)
        } else if c.has("Table")
            && (text.contains("springframework.data.relational") || text.contains("data.relational.core.mapping"))
        {
            Some(Kind::Jdbc)
        } else {
            None
        };
        let Some(kind) = kind else { continue };
        if c.kind != "class" && c.kind != "record" {
            continue;
        }
        let snake_naming = match kind {
            Kind::Jpa => spring,
            Kind::Jdbc | Kind::Micronaut => true,
            Kind::Mongo | Kind::PanacheMongo => false,
        };
        let naming = |n: &str| if snake_naming { snake_case(n) } else { n.to_string() };
        let table = match kind {
            Kind::Jpa => c
                .ann("Table")
                .and_then(|a| ann_arg(&a.args, &["name"]))
                .and_then(|v| unquote_str(&v, &consts))
                .or_else(|| {
                    c.ann("Entity")
                        .and_then(|a| ann_arg(&a.args, &["name"]))
                        .and_then(|v| unquote_str(&v, &consts))
                        .map(|n| naming(&n))
                })
                .unwrap_or_else(|| naming(&c.name))
                .to_lowercase(),
            Kind::Jdbc => c
                .ann("Table")
                .and_then(|a| ann_arg(&a.args, &["value", "name", ""]))
                .and_then(|v| unquote_str(&v, &consts))
                .unwrap_or_else(|| snake_case(&c.name))
                .to_lowercase(),
            Kind::Micronaut => c
                .ann("MappedEntity")
                .and_then(|a| ann_arg(&a.args, &["value", ""]))
                .and_then(|v| unquote_str(&v, &consts))
                .unwrap_or_else(|| snake_case(&c.name))
                .to_lowercase(),
            Kind::Mongo => c
                .ann("Document")
                .and_then(|a| ann_arg(&a.args, &["collection", "value", ""]))
                .and_then(|v| unquote_str(&v, &consts))
                .unwrap_or_else(|| uncapitalize(&c.name)),
            Kind::PanacheMongo => c
                .ann("MongoEntity")
                .and_then(|a| ann_arg(&a.args, &["collection"]))
                .and_then(|v| unquote_str(&v, &consts))
                .unwrap_or_else(|| c.name.clone()),
        };
        if table.is_empty() {
            continue;
        }
        let unique_sets: Vec<String> =
            c.ann("Table").map(|a| single_unique_columns(&a.args, &consts)).unwrap_or_default();

        let mut columns: Vec<Column> = vec![];
        let mut relations: Vec<RawRelation> = vec![];
        let mut chain: Vec<&JClass> = vec![];
        let mut cur = Some(c);
        while let Some(k) = cur {
            if chain.len() > 6 || chain.iter().any(|x| std::ptr::eq(*x, k)) {
                break;
            }
            chain.push(k);
            cur = k.extends.first().and_then(|e| find(&simple(e), k.file));
        }
        chain.reverse();
        // Panache active record: an implicit generated `id`.
        let panache_id = c.extends.iter().any(|e| {
            matches!(simple(e).as_str(), "PanacheEntity" | "PanacheMongoEntity" | "ReactivePanacheMongoEntity")
        });
        if panache_id {
            columns.push(Column {
                name: "id".into(),
                type_name: if kind == Kind::PanacheMongo { "ObjectId".into() } else { "bigint".into() },
                primary_key: true,
                nullable: false,
                unique: true,
                references: None,
                default: Some("generated".into()),
                constraints: vec![],
                evidence: line_ref(path, c.line),
            });
        }
        for k in &chain {
            let kpath = &files[k.file].0;
            for f in &k.fields {
                field_columns(
                    &FieldCtx {
                        kind,
                        naming: &naming,
                        consts: &consts,
                        owner_table: &table,
                        find: &find,
                        file: k.file,
                        files,
                    },
                    kpath,
                    f,
                    None,
                    &mut columns,
                    &mut relations,
                    &unique_sets,
                );
            }
        }
        if columns.is_empty() && relations.is_empty() {
            continue;
        }
        let pks = columns.iter().filter(|x| x.primary_key).count();
        if pks > 1 {
            for x in columns.iter_mut().filter(|x| x.primary_key) {
                x.unique = false;
            }
        }
        for k in &chain {
            for f in &k.fields {
                let ty = simple(&f.ty);
                if enum_names.contains(ty.as_str()) && !f.is_static() && !f.has("Transient") {
                    let column = column_name(kind, &naming, f, &consts).0;
                    let default = f.init.as_deref().map(|i| i.rsplit('.').next().unwrap_or(i).to_string());
                    out.enum_uses.push(RawEnumUse { table: table.clone(), column, enum_name: ty, default });
                }
            }
        }
        out.model.entities.insert(c.name.clone());
        if matches!(kind, Kind::PanacheMongo) || c.extends.iter().any(|e| PANACHE_BASES.contains(&simple(e).as_str())) {
            out.model.active_record.insert(c.name.clone());
        }
        out.entities.push(RawEntity {
            name: c.name.clone(),
            table,
            source: kind.label().into(),
            unit: Some(unit.clone()),
            columns,
            relations,
            evidence: line_ref(path, c.line),
        });
    }

    // References to a target's primary key.
    let pk_of: HashMap<String, String> = out
        .entities
        .iter()
        .map(|e| {
            (
                e.name.clone(),
                e.columns.iter().find(|c| c.primary_key).map(|c| c.name.clone()).unwrap_or_else(|| "id".into()),
            )
        })
        .collect();
    for e in &mut out.entities {
        for col in &mut e.columns {
            if let Some(r) = &col.references {
                if let Some(t) = r.strip_prefix('@').and_then(|t| t.strip_suffix(".?")) {
                    col.references = Some(format!("@{t}.{}", pk_of.get(t).cloned().unwrap_or_else(|| "id".into())));
                }
            }
        }
    }

    // Repositories.
    let mut repo_parent: HashMap<String, (String, HashMap<String, bool>)> = HashMap::new();
    for c in classes.iter().filter(|c| c.kind == "interface" || c.kind == "class") {
        let supers: Vec<&String> = c.extends.iter().chain(c.implements.iter()).collect();
        for s in supers {
            let (base, args) = generic(s);
            if REPO_BASES.contains(&base.as_str()) {
                if let Some(entity) = args.first().filter(|a| !a.is_empty() && *a != "T") {
                    let methods = repo_methods(c);
                    repo_parent.insert(c.name.clone(), (entity.clone(), methods));
                }
            }
        }
    }
    // Interfaces extending a custom repository inherit its entity.
    for _ in 0..3 {
        let found: Vec<(String, String, HashMap<String, bool>)> = classes
            .iter()
            .filter(|c| c.kind == "interface" && !repo_parent.contains_key(&c.name))
            .filter_map(|c| {
                let (entity, _) = c.extends.iter().find_map(|e| repo_parent.get(&simple(e)).cloned())?;
                Some((c.name.clone(), entity, repo_methods(c)))
            })
            .collect();
        for (name, entity, methods) in found {
            repo_parent.insert(name, (entity, methods));
        }
    }
    out.model.repos = repo_parent;
    // Persistence written once in an abstract DAO belongs to the entities its
    // subclasses bind it to.
    out.model.bases = {
        let repos = &out.model.repos;
        let entities = &out.model.entities;
        let repo_entity = |r: &str| repos.get(r).map(|(e, _)| e.clone());
        let is_entity = |e: &str| entities.contains(e);
        inherit::dao_bases(
            &classes,
            &inherit::Ctx { code: &codes, paths: &paths, repo_entity: &repo_entity, is_entity: &is_entity },
        )
    };

    // Spring Statemachine: `.withExternal().source(S.A).target(S.B).event(E.X)`.
    for (path, unit, text) in files.iter().filter(|(_, _, t)| t.contains("withExternal") || t.contains(".initial(")) {
        let lines: Vec<&str> = text.lines().collect();
        let joined = text.as_str();
        let mut from = 0;
        while let Some(p) = joined[from..].find("withExternal()").map(|x| from + x) {
            from = p + 1;
            let end =
                [".and()", ";"].iter().filter_map(|m| joined[p..].find(m).map(|x| p + x)).min().unwrap_or(joined.len());
            let seg = &joined[p..end];
            let arg = |name: &str| -> Option<(String, String, usize)> {
                let at = seg.find(&format!(".{name}("))?;
                let inner = paren_args(&seg[at..])?.trim().to_string();
                let (owner, member) = inner
                    .rsplit_once('.')
                    .map(|(o, m)| (o.rsplit('.').next().unwrap_or(o).to_string(), m.to_string()))
                    .unwrap_or((String::new(), inner.clone()));
                Some((owner, member, p + at))
            };
            let (Some(src), Some(tgt)) = (arg("source"), arg("target")) else { continue };
            let line = joined[..tgt.2].matches('\n').count() as u32 + 1;
            let _ = &lines;
            out.transitions.push(ExtraTransition {
                enum_name: if tgt.0.is_empty() { src.0.clone() } else { tgt.0.clone() },
                from: Some(src.1),
                to: tgt.1,
                marker: None,
                trigger: arg("event").map(|e| e.1),
                unit: unit.clone(),
                evidence: line_ref(path, line),
            });
        }
        for (marker, call) in [("initial", ".initial("), ("end", ".end(")] {
            let mut from = 0;
            while let Some(p) = joined[from..].find(call).map(|x| from + x) {
                from = p + 1;
                let Some(inner) = paren_args(&joined[p..]) else { continue };
                let inner = inner.trim();
                let Some((owner, member)) = inner.rsplit_once('.') else { continue };
                let owner = owner.rsplit('.').next().unwrap_or(owner);
                if !enum_names.contains(owner) || member.contains(|c: char| !(c.is_alphanumeric() || c == '_')) {
                    continue;
                }
                let line = joined[..p].matches('\n').count() as u32 + 1;
                out.transitions.push(ExtraTransition {
                    enum_name: owner.to_string(),
                    from: None,
                    to: member.to_string(),
                    marker: Some(marker),
                    trigger: None,
                    unit: unit.clone(),
                    evidence: line_ref(path, line),
                });
            }
        }
    }
    out
}

#[derive(Clone, Copy)]
struct FieldCtx<'a, 'c> {
    kind: Kind,
    naming: &'a dyn Fn(&str) -> String,
    consts: &'a HashMap<String, String>,
    owner_table: &'a str,
    find: &'a dyn Fn(&str, usize) -> Option<&'c JClass>,
    file: usize,
    files: &'a [(String, String, String)],
}

fn single_unique_columns(args: &str, consts: &HashMap<String, String>) -> Vec<String> {
    let mut out = vec![];
    let mut from = 0;
    while let Some(p) = args[from..].find("columnNames").map(|x| from + x) {
        from = p + 1;
        let rest = args[p..].split_once('=').map(|(_, r)| r.trim()).unwrap_or("");
        let list = match rest.strip_prefix('{') {
            Some(inner) => inner.split('}').next().unwrap_or(""),
            None => rest.split([',', ')']).next().unwrap_or(""),
        };
        let items: Vec<String> = split_args(list).iter().filter_map(|x| unquote_str(x, consts)).collect();
        if items.len() == 1 {
            out.push(items[0].clone());
        }
    }
    out
}

/// Physical column name and the annotation line that states it (if any).
fn column_name(
    kind: Kind,
    naming: &dyn Fn(&str) -> String,
    f: &JField,
    consts: &HashMap<String, String>,
) -> (String, Option<u32>) {
    let named = match kind {
        Kind::Jpa => f.ann("Column").and_then(|a| ann_arg(&a.args, &["name"]).map(|v| (v, a.line))),
        Kind::Jdbc => f.ann("Column").and_then(|a| ann_arg(&a.args, &["value", "name", ""]).map(|v| (v, a.line))),
        Kind::Micronaut => f.ann("MappedProperty").and_then(|a| ann_arg(&a.args, &["value", ""]).map(|v| (v, a.line))),
        Kind::Mongo => f.ann("Field").and_then(|a| ann_arg(&a.args, &["value", "name", ""]).map(|v| (v, a.line))),
        Kind::PanacheMongo => f.ann("BsonProperty").and_then(|a| ann_arg(&a.args, &["value", ""]).map(|v| (v, a.line))),
    };
    match named.and_then(|(v, l)| unquote_str(&v, consts).map(|n| (n, l))) {
        Some((n, l)) => (n.trim_matches('"').to_string(), Some(l)),
        None => (naming(&f.name), None),
    }
}

const COLLECTIONS: &[&str] =
    &["List", "Set", "Collection", "SortedSet", "Iterable", "Map", "ArrayList", "HashSet", "LinkedHashSet", "TreeSet"];

#[allow(clippy::too_many_arguments)]
fn field_columns(
    ctx: &FieldCtx,
    path: &str,
    f: &JField,
    embedded_pk: Option<bool>,
    columns: &mut Vec<Column>,
    relations: &mut Vec<RawRelation>,
    unique_sets: &[String],
) {
    if f.is_static()
        || f.modifiers.iter().any(|m| m == "transient")
        || f.has("Transient")
        || f.has("JsonIgnore") && ctx.kind != Kind::Jpa
    {
        return;
    }
    let (base, args) = generic(&f.ty);
    let collection = COLLECTIONS.contains(&base.as_str()) || f.ty.ends_with("[]") && base != "byte" && base != "Byte";
    let relation_ann = [
        "ManyToOne",
        "OneToOne",
        "OneToMany",
        "ManyToMany",
        "DBRef",
        "DocumentReference",
        "Relation",
        "MappedCollection",
    ]
    .iter()
    .find_map(|n| f.ann(n));
    let target = relation_ann
        .and_then(|a| ann_arg(&a.args, &["targetEntity"]))
        .map(|t| simple(t.trim_end_matches(".class")))
        .or_else(|| if collection { args.last().cloned() } else { Some(base.clone()) })
        .unwrap_or_default();

    if let Some(a) = relation_ann {
        let mapped_by = ann_arg(&a.args, &["mappedBy"]).and_then(|v| unquote_str(&v, ctx.consts));
        let micronaut_kind = (a.name == "Relation").then(|| a.args.to_string());
        let kind = match (a.name.as_str(), micronaut_kind.as_deref()) {
            ("ManyToOne", _) => "many-to-one",
            ("OneToOne", _) => "one-to-one",
            ("OneToMany", _) | ("MappedCollection", _) => "one-to-many",
            ("ManyToMany", _) => "many-to-many",
            ("Relation", Some(k)) if k.contains("MANY_TO_ONE") => "many-to-one",
            ("Relation", Some(k)) if k.contains("ONE_TO_ONE") => "one-to-one",
            ("Relation", Some(k)) if k.contains("ONE_TO_MANY") => "one-to-many",
            ("Relation", Some(k)) if k.contains("MANY_TO_MANY") => "many-to-many",
            ("DBRef", _) | ("DocumentReference", _) => {
                if collection {
                    "one-to-many"
                } else {
                    "many-to-one"
                }
            }
            _ => return,
        };
        if target.is_empty() {
            return;
        }
        let owning = mapped_by.is_none();
        match kind {
            "many-to-one" | "one-to-one" if owning => {
                let join = f.ann("JoinColumn");
                let (col, line) = match join.and_then(|j| {
                    ann_arg(&j.args, &["name"]).and_then(|v| unquote_str(&v, ctx.consts)).map(|n| (n, j.line))
                }) {
                    Some((n, l)) => (n, l),
                    None if matches!(ctx.kind, Kind::Mongo | Kind::PanacheMongo) => (f.name.clone(), f.line),
                    None => (format!("{}_{}", (ctx.naming)(&f.name), "id"), f.line),
                };
                let nullable = !(join.is_some_and(|j| j.args.replace(' ', "").contains("nullable=false"))
                    || a.args.replace(' ', "").contains("optional=false")
                    || f.has("NotNull"));
                columns.retain(|c| c.name != col);
                columns.push(Column {
                    name: col.clone(),
                    type_name: if matches!(ctx.kind, Kind::Mongo | Kind::PanacheMongo) {
                        format!("ref {target}")
                    } else {
                        "bigint".into()
                    },
                    primary_key: f.has("Id") || f.has("MapsId") && embedded_pk.is_some(),
                    nullable,
                    unique: kind == "one-to-one" || unique_sets.contains(&col),
                    references: Some(format!("@{target}.?")),
                    default: None,
                    constraints: vec![],
                    evidence: line_ref(path, line),
                });
                relations.push(RawRelation {
                    kind: kind.into(),
                    target: format!("@{target}"),
                    via: col,
                    evidence: line_ref(path, f.line),
                });
            }
            "one-to-many" => {
                let via = mapped_by
                    .or_else(|| {
                        f.ann("JoinColumn")
                            .and_then(|j| ann_arg(&j.args, &["name"]))
                            .and_then(|v| unquote_str(&v, ctx.consts))
                    })
                    .unwrap_or_else(|| f.name.clone());
                relations.push(RawRelation {
                    kind: kind.into(),
                    target: format!("@{target}"),
                    via,
                    evidence: line_ref(path, f.line),
                });
            }
            "many-to-many" if owning => {
                let via = f
                    .ann("JoinTable")
                    .and_then(|j| ann_arg(&j.args, &["name"]))
                    .and_then(|v| unquote_str(&v, ctx.consts))
                    .unwrap_or_else(|| format!("{}_{}", ctx.owner_table, (ctx.naming)(&f.name)));
                relations.push(RawRelation {
                    kind: kind.into(),
                    target: format!("@{target}"),
                    via,
                    evidence: line_ref(path, f.line),
                });
            }
            _ => {}
        }
        return;
    }
    if f.has("ElementCollection") {
        return;
    }
    // Embedded value objects flatten into the owning table.
    let embeddable = (ctx.find)(&base, ctx.file).filter(|e| e.has("Embeddable"));
    if (f.has("Embedded") || f.has("EmbeddedId") || embeddable.is_some())
        && matches!(ctx.kind, Kind::Jpa | Kind::Micronaut | Kind::Jdbc)
    {
        if let Some(e) = embeddable.or_else(|| (ctx.find)(&base, ctx.file)) {
            let pk = f.has("EmbeddedId");
            let epath = ctx.files[e.file].0.as_str();
            let ectx = FieldCtx { file: e.file, ..*ctx };
            for ef in &e.fields {
                let before = columns.len();
                field_columns(&ectx, epath, ef, Some(pk), columns, relations, unique_sets);
                if pk {
                    for c in columns[before..].iter_mut() {
                        c.primary_key = true;
                        c.nullable = false;
                    }
                }
            }
            return;
        }
    }
    if collection && ctx.kind == Kind::Jpa {
        // Unannotated collections aren't mapped by JPA without @ElementCollection.
        return;
    }
    let (name, name_line) = column_name(ctx.kind, ctx.naming, f, ctx.consts);
    let pk = f.has("Id") || f.has("EmbeddedId") || embedded_pk == Some(true);
    let primitive =
        matches!(f.ty.as_str(), "int" | "long" | "short" | "byte" | "boolean" | "double" | "float" | "char");
    let col_args = f.ann("Column").map(|a| a.args.replace(' ', "")).unwrap_or_default();
    let not_null = ["NotNull", "NonNull", "NotBlank", "NotEmpty"].iter().any(|n| f.has(n));
    let nullable = !(pk || primitive || not_null || col_args.contains("nullable=false"));
    let mut unique = col_args.contains("unique=true")
        || unique_sets.contains(&name)
        || f.ann("Indexed").is_some_and(|a| a.args.replace(' ', "").contains("unique=true"));
    if pk && embedded_pk.is_none() {
        unique = true;
    }
    let length =
        f.ann("Column").and_then(|a| ann_arg(&a.args, &["length"])).filter(|l| l.chars().all(|c| c.is_ascii_digit()));
    let mut constraints = validation(f);
    let type_name = match ctx.kind {
        Kind::Jpa | Kind::Jdbc | Kind::Micronaut => {
            if let Some(def) = f
                .ann("Column")
                .and_then(|a| ann_arg(&a.args, &["columnDefinition"]))
                .and_then(|v| unquote_str(&v, ctx.consts))
            {
                def
            } else {
                sql_type(f, &base, length.as_deref())
            }
        }
        _ => f.ty.clone(),
    };
    if let Some(l) = &length {
        if simple(&f.ty) == "String" && !constraints.iter().any(|c| c.starts_with("max length")) {
            constraints.push(format!("max length {l}"));
        }
    }
    if f.has("Version") {
        constraints.push("optimistic lock version".into());
    }
    let default = if f.has("GeneratedValue") {
        Some("generated".into())
    } else if f.has("CreationTimestamp") || f.has("CreatedDate") {
        Some("creation time".into())
    } else {
        f.init.as_deref().and_then(literal_default)
    };
    columns.retain(|c| c.name != name);
    columns.push(Column {
        name,
        type_name,
        primary_key: pk,
        nullable,
        unique,
        references: None,
        default,
        constraints,
        evidence: line_ref(path, name_line.unwrap_or(f.line)),
    });
}

fn literal_default(init: &str) -> Option<String> {
    let t = init.trim();
    if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
        return Some(t[1..t.len() - 1].to_string());
    }
    if t == "true" || t == "false" || t.trim_end_matches(['L', 'l', 'd', 'D', 'f', 'F']).parse::<f64>().is_ok() {
        return Some(t.trim_end_matches(['L', 'l']).to_string());
    }
    // `Status.NEW`
    let parts: Vec<&str> = t.split('.').collect();
    if parts.len() == 2
        && parts[0].starts_with(|c: char| c.is_uppercase())
        && parts[1].chars().all(|c| c.is_uppercase() || c.is_ascii_digit() || c == '_')
    {
        return Some(parts[1].to_string());
    }
    None
}

fn sql_type(f: &JField, base: &str, length: Option<&str>) -> String {
    if let Some(e) = f.ann("Enumerated") {
        return if e.args.contains("STRING") { "varchar".into() } else { "integer".into() };
    }
    if f.has("Lob") {
        return if base == "String" { "text".into() } else { "blob".into() };
    }
    let precision = f.ann("Column").and_then(|a| ann_arg(&a.args, &["precision"]));
    let scale = f.ann("Column").and_then(|a| ann_arg(&a.args, &["scale"]));
    match base {
        "String" => format!("varchar({})", length.unwrap_or("255")),
        "Long" | "long" => "bigint".into(),
        "Integer" | "int" => "integer".into(),
        "Short" | "short" => "smallint".into(),
        "Byte" | "byte" if !f.ty.ends_with("[]") => "tinyint".into(),
        "Boolean" | "boolean" => "boolean".into(),
        "Double" | "double" => "double precision".into(),
        "Float" | "float" => "real".into(),
        "BigDecimal" | "BigInteger" => match (precision, scale) {
            (Some(p), Some(s)) => format!("numeric({p},{s})"),
            (Some(p), None) => format!("numeric({p})"),
            _ => "numeric".into(),
        },
        "UUID" => "uuid".into(),
        "Instant" | "OffsetDateTime" | "ZonedDateTime" => "timestamp with time zone".into(),
        "LocalDateTime" | "Date" | "Timestamp" | "Calendar" => "timestamp".into(),
        "LocalDate" => "date".into(),
        "LocalTime" | "Time" => "time".into(),
        "Duration" => "interval".into(),
        "Character" | "char" => "char(1)".into(),
        _ if f.ty.ends_with("[]") && (base == "byte" || base == "Byte") => "bytea".into(),
        _ => f.ty.clone(),
    }
}

/// Bean Validation (Jakarta / javax / Hibernate Validator) as constraint statements.
fn validation(f: &JField) -> Vec<String> {
    let mut out = vec![];
    let num = |a: &JAnn, keys: &[&str]| ann_arg(&a.args, keys).map(|v| v.trim_matches('"').to_string());
    for a in &f.anns {
        match a.name.as_str() {
            "Size" | "Length" => {
                if let Some(m) = num(a, &["min"]).filter(|m| m != "0") {
                    out.push(format!("min length {m}"));
                }
                if let Some(m) = num(a, &["max"]) {
                    out.push(format!("max length {m}"));
                }
            }
            "Min" | "DecimalMin" => {
                if let Some(v) = num(a, &["value", ""]) {
                    out.push(format!("≥ {v}"));
                }
            }
            "Max" | "DecimalMax" => {
                if let Some(v) = num(a, &["value", ""]) {
                    out.push(format!("≤ {v}"));
                }
            }
            "Range" => {
                if let Some(v) = num(a, &["min"]) {
                    out.push(format!("≥ {v}"));
                }
                if let Some(v) = num(a, &["max"]) {
                    out.push(format!("≤ {v}"));
                }
            }
            "Positive" => out.push("> 0".into()),
            "PositiveOrZero" => out.push("≥ 0".into()),
            "Negative" => out.push("< 0".into()),
            "NegativeOrZero" => out.push("≤ 0".into()),
            "Pattern" => {
                if let Some(r) = num(a, &["regexp"]) {
                    out.push(format!("matches {r}"));
                }
            }
            "Email" => out.push("valid email".into()),
            "NotBlank" => out.push("not blank".into()),
            "NotEmpty" => out.push("not empty".into()),
            "Past" => out.push("in the past".into()),
            "PastOrPresent" => out.push("not in the future".into()),
            "Future" => out.push("in the future".into()),
            "FutureOrPresent" => out.push("not in the past".into()),
            "Digits" => {
                if let (Some(i), Some(fr)) = (num(a, &["integer"]), num(a, &["fraction"])) {
                    out.push(format!("at most {i} integer and {fr} fraction digits"));
                }
            }
            _ => {}
        }
    }
    out
}

/// Custom repository methods: `@Query` / `@Modifying` / derived `deleteBy…`.
fn repo_methods(c: &JClass) -> HashMap<String, bool> {
    let mut out = HashMap::new();
    for m in &c.methods {
        let q = m.anns.iter().find(|a| a.name == "Query" || a.name == "NativeQuery");
        let modifying = m.anns.iter().any(|a| a.name == "Modifying");
        let write = modifying
            || q.is_some_and(|a| {
                let t = a.args.trim_start_matches(['(', ' ']).to_lowercase();
                let t = t.split_once('"').map(|(_, r)| r).unwrap_or(&t).trim_start();
                t.starts_with("update") || t.starts_with("delete") || t.starts_with("insert")
            })
            || write_verb(&m.name);
        out.insert(m.name.clone(), write);
    }
    out
}

fn write_verb(method: &str) -> bool {
    ["save", "insert", "update", "delete", "remove", "persist", "merge", "upsert", "store", "create"]
        .iter()
        .any(|v| method.starts_with(v))
}

fn read_verb(method: &str) -> bool {
    ["find", "get", "read", "query", "count", "exists", "search", "stream", "list", "load", "fetch", "select"]
        .iter()
        .any(|v| method.starts_with(v))
}

fn identifiers(text: &str) -> HashSet<&str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).filter(|w| !w.is_empty()).collect()
}

/// Variables declared with `ty` (fields, parameters, locals): `Type name` / `var name = new Type(`.
fn vars_of_type(text: &str, ty: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let b = text.as_bytes();
    let mut from = 0;
    while let Some(p) = text[from..].find(ty).map(|x| from + x) {
        from = p + ty.len();
        if p > 0 && (b[p - 1].is_ascii_alphanumeric() || b[p - 1] == b'_' || b[p - 1] == b'.') {
            continue;
        }
        let mut e = p + ty.len();
        if e < b.len() && (b[e].is_ascii_alphanumeric() || b[e] == b'_') {
            continue;
        }
        // Generic arguments (`Repository<X, Long> repo` is rare; `Optional<Order> o`).
        if e < b.len() && b[e] == b'<' {
            let mut depth = 0;
            while e < b.len() {
                match b[e] {
                    b'<' => depth += 1,
                    b'>' => {
                        depth -= 1;
                        if depth == 0 {
                            e += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                e += 1;
            }
        }
        let rest = &text[e..];
        let trimmed = rest.trim_start();
        if trimmed.len() == rest.len() {
            continue;
        }
        let name: String = trimmed.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if name.is_empty() || matches!(name.as_str(), "extends" | "implements" | "class" | "interface" | "instanceof") {
            continue;
        }
        let after = trimmed[name.len()..].trim_start();
        if after.starts_with(['=', ';', ',', ')', ':']) || after.is_empty() {
            out.insert(name);
        }
    }
    // `var order = new Order(`
    let pat = format!("new {ty}(");
    for (i, l) in text.lines().enumerate() {
        let _ = i;
        if let Some(p) = l.find(&pat) {
            let before = l[..p].trim_end().trim_end_matches('=').trim_end();
            let name: String = before
                .chars()
                .rev()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            if !name.is_empty() {
                out.insert(name);
            }
        }
    }
    out
}

/// Receivers of persistence-manager accessors (`getEntityManager()`).
const MANAGER_ACCESSORS: &[&str] = &["EntityManager", "Session", "MongoTemplate", "MongoOperations"];

/// `EntityManager` / `MongoTemplate` methods that write.
fn manager_write(method: &str) -> bool {
    matches!(
        method,
        "persist"
            | "merge"
            | "remove"
            | "save"
            | "insert"
            | "update"
            | "upsert"
            | "saveOrUpdate"
            | "delete"
            | "updateFirst"
            | "updateMulti"
            | "findAndModify"
            | "findAndRemove"
    )
}

/// …and the ones that read.
fn manager_read(method: &str) -> bool {
    matches!(
        method,
        "find"
            | "getReference"
            | "get"
            | "findById"
            | "findAll"
            | "findOne"
            | "exists"
            | "count"
            | "load"
            | "select"
            | "selectOne"
    )
}

/// Entity names in a JPQL string: `from Order o`, `join o.items`, `update Order`, `delete from Order`.
fn jpql_entities(q: &str) -> (Vec<String>, bool) {
    let lower = q.to_lowercase();
    let write = lower.trim_start().starts_with("update") || lower.trim_start().starts_with("delete");
    let mut out = vec![];
    let words: Vec<&str> = q.split_whitespace().collect();
    for (i, w) in words.iter().enumerate() {
        if matches!(w.to_lowercase().as_str(), "from" | "update" | "join") {
            if let Some(n) = words.get(i + 1) {
                let n: String = n.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if n.starts_with(|c: char| c.is_uppercase()) {
                    out.push(n);
                }
            }
        }
    }
    (out, write)
}

/// Reads and writes of entities, including those a shared DAO base performs
/// on behalf of its subclasses.
pub fn access(model: &JavaModel, sources: &[StateSrc]) -> Vec<Access> {
    let mut out = vec![];
    for (si, src) in sources.iter().enumerate() {
        if !src.path.ends_with(".java") || src.migration {
            continue;
        }
        let words = identifiers(&src.text);
        let lines: Vec<&str> = src.text.lines().collect();
        // Receiver variable → what it accesses.
        let mut repo_vars: HashMap<String, (String, Option<&RepoMethods>)> = HashMap::new();
        for (repo, (entity, methods)) in &model.repos {
            if words.contains(repo.as_str()) {
                for v in vars_of_type(&src.text, repo) {
                    repo_vars.insert(v, (entity.clone(), Some(methods)));
                }
            }
        }
        let mut entity_vars: HashMap<String, String> = HashMap::new();
        for e in &model.entities {
            if words.contains(e.as_str()) {
                for v in vars_of_type(&src.text, e) {
                    entity_vars.insert(v, e.clone());
                }
            }
        }
        let mut manager_vars: BTreeSet<String> = BTreeSet::new();
        for t in [
            "EntityManager",
            "Session",
            "StatelessSession",
            "MongoTemplate",
            "ReactiveMongoTemplate",
            "MongoOperations",
            "ReactiveMongoOperations",
            "R2dbcEntityTemplate",
            "JdbcAggregateTemplate",
        ] {
            if words.contains(t) {
                manager_vars.extend(vars_of_type(&src.text, t));
            }
        }
        let jooq = words.contains("DSLContext");
        // Persistence this file performs for subclasses that bind its type parameters.
        let bases: Vec<&DaoBase> = model.bases.iter().filter(|b| b.path == src.path).collect();
        for call in &src.facts.calls {
            let callee = call.callee.trim_start_matches("this.");
            let Some((recv, method)) = callee.rsplit_once('.') else {
                // Inherited repository methods inside a repository implementation are rare; skip.
                continue;
            };
            let line_text = lines.get(call.line as usize - 1).copied().unwrap_or("");
            if let Some(base) = bases.iter().find(|b| b.contains(call.line)) {
                if let Some(via) = base.via_of(recv) {
                    let write = write_verb(method);
                    if write || read_verb(method) {
                        for (entity, sub) in base.resolve(&via) {
                            out.push(Access::via(entity, write, si, call.line, sub));
                        }
                        continue;
                    }
                }
                // `getEntityManager().persist(entity)` with `entity` declared `E entity`.
                let manager = manager_vars.contains(recv)
                    || recv.strip_suffix("()").is_some_and(|m| MANAGER_ACCESSORS.iter().any(|k| m.ends_with(k)));
                let write = manager_write(method);
                if manager && (write || manager_read(method)) {
                    let arg = line_text
                        .find(&format!(".{method}("))
                        .and_then(|p| paren_args(&line_text[p..]))
                        .and_then(|a| split_args(a).first().map(|x| x.trim().to_string()));
                    if let Some(param) = arg.as_deref().and_then(|a| base.type_param_of(a)) {
                        for (entity, sub) in base.resolve(&Via::TypeParam(param.to_string())) {
                            out.push(Access::via(entity, write, si, call.line, sub));
                        }
                        continue;
                    }
                }
            }
            if recv.contains(['(', ')', ' ']) || method != call.name {
                continue;
            }
            if let Some((entity, methods)) = repo_vars.get(recv) {
                let custom = methods.and_then(|m| m.get(method)).copied();
                let write = custom.unwrap_or_else(|| write_verb(method));
                if write || read_verb(method) || custom.is_some() {
                    out.push(Access::new(entity.clone(), write, si, call.line));
                }
                continue;
            }
            if model.active_record.contains(recv) {
                if write_verb(method) || read_verb(method) {
                    out.push(Access::new(recv.to_string(), write_verb(method), si, call.line));
                }
                continue;
            }
            if let Some(e) = entity_vars.get(recv) {
                if model.active_record.contains(e)
                    && matches!(method, "persist" | "persistAndFlush" | "delete" | "update" | "persistOrUpdate")
                {
                    out.push(Access::new(e.clone(), true, si, call.line));
                }
                continue;
            }
            if manager_vars.contains(recv) {
                let args =
                    line_text.find(&format!(".{method}(")).and_then(|p| paren_args(&line_text[p..])).unwrap_or("");
                let class_arg = split_args(args).iter().find_map(|a| a.trim().strip_suffix(".class").map(simple));
                let var_arg = split_args(args).first().and_then(|a| entity_vars.get(a.trim()).cloned());
                if matches!(method, "createQuery" | "query") {
                    if let Some(q) = src.facts.strings.iter().find(|s| s.line == call.line || s.line == call.line + 1) {
                        let (names, write) = jpql_entities(&q.value);
                        for n in names.into_iter().filter(|n| model.entities.contains(n)) {
                            out.push(Access::new(n, write, si, call.line));
                        }
                    }
                    continue;
                }
                let write = manager_write(method);
                let read = manager_read(method);
                if !(write || read) {
                    continue;
                }
                if let Some(e) = class_arg.or(var_arg) {
                    out.push(Access::new(e, write, si, call.line));
                }
                continue;
            }
            if jooq && matches!(method, "insertInto" | "update" | "deleteFrom" | "selectFrom" | "mergeInto") {
                let args =
                    line_text.find(&format!("{method}(")).and_then(|p| paren_args(&line_text[p..])).unwrap_or("");
                let t = args.split(',').next().unwrap_or("").trim();
                let t = t.rsplit('.').next().unwrap_or(t);
                if !t.is_empty() && t.chars().all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit()) {
                    out.push(Access::new(format!("#{}", t.to_lowercase()), method != "selectFrom", si, call.line));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities_relations_embeddables_and_repos() {
        let files = vec![(
            "src/main/java/a/Order.java".to_string(),
            "shop".to_string(),
            r#"package a;
@MappedSuperclass
abstract class Base { @Id @GeneratedValue private Long id; @Version private int version; }

@Entity
@Table(name = "purchase_orders")
public class PurchaseOrder extends Base {
    @Column(name = "order_no", nullable = false, unique = true, length = 20)
    private String number;
    @NotNull @Size(max = 500) private String note;
    @Enumerated(EnumType.STRING)
    private OrderStatus status = OrderStatus.NEW;
    @ManyToOne(optional = false)
    @JoinColumn(name = "customer_id")
    private Customer customer;
    @OneToMany(mappedBy = "order")
    private List<LineItem> items;
    @Embedded private Address shipTo;
    @Transient private BigDecimal total;
    @ManyToMany @JoinTable(name = "order_tags") private Set<Tag> tags;
}

@Embeddable class Address { private String street; @Column(name = "zip_code") private String zip; }
enum OrderStatus { NEW, PAID, SHIPPED }
@Entity class Customer { @Id private UUID id; private String fullName; }
interface OrderRepository extends JpaRepository<PurchaseOrder, Long> {
    @Modifying @Query("update PurchaseOrder o set o.status = :s") int markAll(OrderStatus s);
}
"#
            .to_string(),
        )];
        let spring: BTreeSet<String> = ["shop".to_string()].into();
        let out = parse(&files, &spring);
        let po = out.entities.iter().find(|e| e.name == "PurchaseOrder").unwrap();
        assert_eq!(po.table, "purchase_orders");
        let cols: Vec<(&str, &str, bool, bool)> =
            po.columns.iter().map(|c| (c.name.as_str(), c.type_name.as_str(), c.nullable, c.primary_key)).collect();
        assert_eq!(
            cols,
            vec![
                ("id", "bigint", false, true),
                ("version", "integer", false, false),
                ("order_no", "varchar(20)", false, false),
                ("note", "varchar(255)", false, false),
                ("status", "varchar", true, false),
                ("customer_id", "bigint", false, false),
                ("street", "varchar(255)", true, false),
                ("zip_code", "varchar(255)", true, false),
            ]
        );
        let order_no = po.columns.iter().find(|c| c.name == "order_no").unwrap();
        assert!(order_no.unique);
        assert_eq!(order_no.constraints, vec!["max length 20"]);
        assert_eq!(order_no.evidence.start_line, 8);
        assert_eq!(po.columns.iter().find(|c| c.name == "note").unwrap().constraints, vec!["max length 500"]);
        assert_eq!(po.columns.iter().find(|c| c.name == "status").unwrap().default.as_deref(), Some("NEW"));
        assert_eq!(
            po.columns.iter().find(|c| c.name == "customer_id").unwrap().references.as_deref(),
            Some("@Customer.id")
        );
        let rels: Vec<(&str, &str, &str)> =
            po.relations.iter().map(|r| (r.kind.as_str(), r.target.as_str(), r.via.as_str())).collect();
        assert_eq!(
            rels,
            vec![
                ("many-to-one", "@Customer", "customer_id"),
                ("one-to-many", "@LineItem", "order"),
                ("many-to-many", "@Tag", "order_tags")
            ]
        );
        let customer = out.entities.iter().find(|e| e.name == "Customer").unwrap();
        assert_eq!(customer.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["id", "full_name"]);
        assert_eq!(out.enum_uses[0].enum_name, "OrderStatus");
        assert_eq!(out.enum_uses[0].default.as_deref(), Some("NEW"));
        let (entity, methods) = &out.model.repos["OrderRepository"];
        assert_eq!(entity, "PurchaseOrder");
        assert!(methods["markAll"]);
    }

    #[test]
    fn jpql_and_statemachine() {
        assert_eq!(jpql_entities("select o from Order o join o.items i"), (vec!["Order".to_string()], false));
        assert!(jpql_entities("update Order o set o.status = :s").1);
        let files = vec![(
            "src/main/java/a/Cfg.java".to_string(),
            "u".to_string(),
            "enum States { NEW, PAID, DONE }\nclass Cfg {\n void configure(T t) {\n  t.withStates().initial(States.NEW);\n  t.withExternal()\n   .source(States.NEW).target(States.PAID)\n   .event(Events.PAY)\n   .and()\n   .withExternal().source(States.PAID).target(States.DONE);\n }\n}\n".to_string(),
        )];
        let out = parse(&files, &BTreeSet::new());
        let t: Vec<(Option<&str>, &str, Option<&str>, u32)> = out
            .transitions
            .iter()
            .map(|t| (t.from.as_deref(), t.to.as_str(), t.trigger.as_deref(), t.evidence.start_line))
            .collect();
        assert_eq!(
            t,
            vec![(Some("NEW"), "PAID", Some("PAY"), 6), (Some("PAID"), "DONE", None, 9), (None, "NEW", None, 4)]
        );
    }
}
