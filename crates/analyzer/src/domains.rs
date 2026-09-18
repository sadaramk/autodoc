//! Business domains in the data model, and module boundaries in modular
//! monoliths.
//!
//! **Domains.** A model with dozens of entities reads as a wall of boxes, so
//! entities are grouped the way the code groups them:
//!
//! 1. JVM: the package below the application's base package (`com.acme.shop.
//!    orders.Order` → `orders`). Technical segments (`model`, `entity`, `dao`,
//!    `sql`, `jpa`, `domain`, `repository`, …) are skipped, so
//!    `org.thingsboard.server.dao.model.sql.DeviceEntity` doesn't become
//!    "model".
//! 2. Other languages: the module the declaring file belongs to, with the same
//!    technical names skipped.
//! 3. When the declaration says nothing (DDL-only tables, entities in a flat
//!    `model` package), the domain of the code that writes and reads the
//!    entity, then the domain of the entities it references.
//! 4. Whatever is still unplaced forms domains by foreign-key connectivity,
//!    named after the most referenced table.
//! 5. A domain larger than [`MAX_ENTITIES`] splits by foreign-key component,
//!    then by table-name prefix, then alphabetically — deterministic, named.
//!
//! **Modules.** For each module of a component view: the public types other
//! modules may use (root package and Spring Modulith named interfaces), the
//! application events it publishes and handles, its internal sub-packages,
//! and imports that reach past another module's boundary.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use autodoc_ir::{Attribute, BoundaryType, Cardinality, Container, DiagramType, Edge, EdgeType, KeyKind, Node};

use crate::data::{DataModel, Entity};
use crate::draft::{Draft, DraftOptions};
use crate::extract::SymbolKind;
use crate::lang::Language;
use crate::scan::{display_name, slug, ComponentView, EvidenceRef, FileRec, ScanReport, SymbolRef, Unit, Violation};
use crate::source::SourceIndex;
use crate::views::MAX_ENTITIES;

/// Package or module names that describe a technical layer, not a business domain.
const TECHNICAL: &[&str] = &[
    "model",
    "models",
    "entity",
    "entities",
    "domain",
    "domains",
    "persistence",
    "dao",
    "daos",
    "repository",
    "repositories",
    "repo",
    "repos",
    "jpa",
    "sql",
    "nosql",
    "data",
    "db",
    "database",
    "schema",
    "schemas",
    "orm",
    "impl",
    "internal",
    "common",
    "core",
    "api",
    "service",
    "services",
    "dto",
    "dtos",
    "util",
    "utils",
    "support",
    "shared",
    "base",
    "config",
    "sqlts",
    "cassandra",
    "mongo",
    "hibernate",
    "jdbc",
    "r2dbc",
    "mapper",
    "mappers",
    "store",
    "storage",
    "src",
    "lib",
    "app",
    "application",
    "server",
    "main",
    "java",
    "migrations",
    "types",
    "query",
    "queries",
    "timescale",
    "psql",
    "postgres",
    "sqlite",
    "view",
    "views",
    "dictionary",
];

/// Domains beyond which small ones are folded into their neighbours.
const MAX_DOMAINS: usize = 16;

fn technical(segment: &str) -> bool {
    TECHNICAL.contains(&segment.to_lowercase().as_str())
}

// ── domain assignment ────────────────────────────────────────────────────────

/// Sets `Entity::domain` for every entity (see the module docs for the rule).
pub(crate) fn assign(entities: &mut [Entity], index: &SourceIndex) {
    if entities.is_empty() {
        return;
    }
    let by_path: BTreeMap<&str, &crate::source::SourceFile> = index.files.iter().map(|f| (f.path, f)).collect();
    let bases = java_bases(index);
    let domain_of_file = |path: &str| -> Option<String> {
        let f = by_path.get(path)?;
        match f.language {
            Language::Java | Language::Kotlin => {
                let pkg = f.facts.package.as_deref()?;
                java_domain(pkg, bases.get(f.unit).map(String::as_str))
            }
            lang => {
                let unit = index.unit(f.unit)?;
                let rel = if unit.root.is_empty() { path } else { path.strip_prefix(&format!("{}/", unit.root))? };
                let key = crate::modules::module_key(lang, rel, None);
                key.split('/').rev().find(|s| !technical(s) && !s.is_empty()).map(|s| s.to_lowercase())
            }
        }
    };

    let ids: BTreeMap<String, usize> = entities.iter().enumerate().map(|(i, e)| (e.id.clone(), i)).collect();
    // Entities declared by several deployables: each service owns its data, so
    // the service is the domain (packages then only subdivide large ones).
    let declaring_unit = |e: &Entity| by_path.get(e.evidence.file_path.as_str()).map(|f| f.unit.to_string());
    let units: BTreeSet<String> = entities.iter().filter_map(declaring_unit).collect();
    let mut domain: Vec<Option<String>> = if units.len() >= 2 {
        entities
            .iter()
            .map(|e| declaring_unit(e).or_else(|| e.units.first().cloned()).map(|u| service_name(&u)))
            .collect()
    } else {
        entities.iter().map(|e| domain_of_file(&e.evidence.file_path)).collect()
    };

    // Where the declaration is silent, the code that uses the entity speaks.
    for (i, e) in entities.iter().enumerate() {
        if domain[i].is_some() {
            continue;
        }
        let mut votes: BTreeMap<String, usize> = BTreeMap::new();
        for (weight, list) in [(2, &e.writes), (1, &e.reads)] {
            for a in list.iter() {
                if let Some(d) = domain_of_file(&a.evidence.file_path) {
                    *votes.entry(d).or_default() += weight;
                }
            }
        }
        domain[i] = best(&votes);
    }

    // Then its neighbours by foreign key (both directions), until stable.
    let neighbours = neighbour_lists(entities, &ids);
    for _ in 0..3 {
        let mut changed = false;
        for i in 0..entities.len() {
            if domain[i].is_some() {
                continue;
            }
            let mut votes: BTreeMap<String, usize> = BTreeMap::new();
            for &n in &neighbours[i] {
                if let Some(d) = &domain[n] {
                    *votes.entry(d.clone()).or_default() += 1;
                }
            }
            if let Some(d) = best(&votes) {
                domain[i] = Some(d);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // Still unplaced: foreign-key components among themselves.
    let unplaced: Vec<usize> = (0..entities.len()).filter(|&i| domain[i].is_none()).collect();
    for component in components(&unplaced, &neighbours) {
        let name = if component.len() == 1 && entities.len() > 1 {
            "other".to_string()
        } else {
            hub_name(entities, &component, &neighbours)
        };
        for i in component {
            domain[i] = Some(name.clone());
        }
    }

    // Oversized domains split; too many tiny ones fold into neighbours.
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, d) in domain.iter().enumerate() {
        groups.entry(d.clone().unwrap_or_else(|| "schema".into())).or_default().push(i);
    }
    let mut final_groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (name, members) in groups {
        for (sub, m) in split(&name, &members, entities, &neighbours) {
            final_groups.entry(sub).or_default().extend(m);
        }
    }
    fold_small(&mut final_groups, &neighbours, entities.len());
    for (name, members) in final_groups {
        for i in members {
            entities[i].domain = Some(name.clone());
        }
    }
}

/// `account-service` → `account`: the service suffix adds nothing to a domain name.
fn service_name(unit: &str) -> String {
    ["-service", "-svc", "-api", "-server", "-app"]
        .iter()
        .find_map(|s| unit.strip_suffix(s).filter(|b| !b.is_empty()))
        .unwrap_or(unit)
        .to_string()
}

fn best(votes: &BTreeMap<String, usize>) -> Option<String> {
    votes.iter().max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0))).map(|(d, _)| d.clone())
}

/// Base package per JVM unit: the application class's package, else the
/// deepest package all its sources share.
fn java_bases<'a>(index: &SourceIndex<'a>) -> BTreeMap<&'a str, String> {
    let mut by_unit: BTreeMap<&'a str, Vec<&crate::source::SourceFile>> = BTreeMap::new();
    for f in index.files.iter().filter(|f| f.language.is_jvm() && f.facts.package.is_some()) {
        by_unit.entry(f.unit).or_default().push(f);
    }
    by_unit
        .into_iter()
        .map(|(unit, files)| {
            let app = files
                .iter()
                .find(|f| f.facts.entry_points.iter().any(|e| e.reason.contains("application object")))
                .and_then(|f| f.facts.package.clone());
            let base = app.unwrap_or_else(|| {
                let pkgs: Vec<Vec<&str>> =
                    files.iter().filter_map(|f| f.facts.package.as_deref()).map(|p| p.split('.').collect()).collect();
                common_prefix(&pkgs).join(".")
            });
            (unit, base)
        })
        .collect()
}

fn common_prefix<'s>(items: &[Vec<&'s str>]) -> Vec<&'s str> {
    let Some(first) = items.first() else { return vec![] };
    let mut n = first.len();
    for other in &items[1..] {
        n = n.min(first.iter().zip(other).take_while(|(a, b)| a == b).count());
    }
    first[..n].to_vec()
}

/// First business-named package segment below the base package.
pub(crate) fn java_domain(pkg: &str, base: Option<&str>) -> Option<String> {
    let segs: Vec<&str> = pkg.split('.').collect();
    let base_segs: Vec<&str> = base.filter(|b| !b.is_empty()).map(|b| b.split('.').collect()).unwrap_or_default();
    let shared = segs.iter().zip(&base_segs).take_while(|(a, b)| a == b).count();
    // Outside the base package entirely (a sibling library): keep at least the last two segments.
    let start = if base_segs.is_empty() { segs.len().saturating_sub(2) } else { shared };
    segs[start..].iter().find(|s| !technical(s)).map(|s| s.to_lowercase())
}

fn neighbour_lists(entities: &[Entity], ids: &BTreeMap<String, usize>) -> Vec<BTreeSet<usize>> {
    let mut out = vec![BTreeSet::new(); entities.len()];
    for (i, e) in entities.iter().enumerate() {
        for r in &e.relations {
            if let Some(&j) = ids.get(&r.target) {
                if i != j {
                    out[i].insert(j);
                    out[j].insert(i);
                }
            }
        }
    }
    out
}

/// Connected components of `members` under `neighbours`, largest first.
fn components(members: &[usize], neighbours: &[BTreeSet<usize>]) -> Vec<Vec<usize>> {
    let set: BTreeSet<usize> = members.iter().copied().collect();
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for &m in members {
        if !seen.insert(m) {
            continue;
        }
        let mut comp = vec![m];
        let mut stack = vec![m];
        while let Some(cur) = stack.pop() {
            for &n in &neighbours[cur] {
                if set.contains(&n) && seen.insert(n) {
                    comp.push(n);
                    stack.push(n);
                }
            }
        }
        comp.sort();
        out.push(comp);
    }
    out.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    out
}

/// The most referenced table in a group, without a shared `tb_`-style prefix.
fn hub_name(entities: &[Entity], members: &[usize], neighbours: &[BTreeSet<usize>]) -> String {
    let set: BTreeSet<usize> = members.iter().copied().collect();
    let hub = members
        .iter()
        .max_by(|&&a, &&b| {
            let da = neighbours[a].intersection(&set).count();
            let db = neighbours[b].intersection(&set).count();
            da.cmp(&db).then_with(|| entities[b].table.cmp(&entities[a].table))
        })
        .copied()
        .unwrap_or(members[0]);
    table_stem(&entities[hub].table)
}

fn table_stem(table: &str) -> String {
    let t = table.trim_matches('"').to_lowercase();
    let t = t.rsplit('.').next().unwrap_or(&t).to_string();
    t.strip_prefix("tb_").map(str::to_string).unwrap_or(t)
}

fn split(
    name: &str,
    members: &[usize],
    entities: &[Entity],
    neighbours: &[BTreeSet<usize>],
) -> Vec<(String, Vec<usize>)> {
    if members.len() <= MAX_ENTITIES {
        return vec![(name.to_string(), members.to_vec())];
    }
    let mut out = Vec::new();
    let mut leftovers: Vec<usize> = Vec::new();
    let comps = components(members, neighbours);
    for comp in comps {
        if comp.len() == 1 {
            leftovers.extend(comp);
        } else if comp.len() <= MAX_ENTITIES {
            out.push((format!("{name}-{}", hub_name(entities, &comp, neighbours)), comp));
        } else {
            out.extend(by_prefix(name, &comp, entities));
        }
    }
    if !leftovers.is_empty() {
        out.extend(by_prefix(name, &leftovers, entities));
    }
    // Names must stay unique.
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for (n, _) in out.iter_mut() {
        let c = seen.entry(n.clone()).or_default();
        *c += 1;
        if *c > 1 {
            *n = format!("{n}-{c}");
        }
    }
    out
}

/// Groups by the first table-name token (`rule_chain`, `rule_node` → `rule`),
/// then alphabetical chunks of at most `MAX_ENTITIES`.
fn by_prefix(name: &str, members: &[usize], entities: &[Entity]) -> Vec<(String, Vec<usize>)> {
    let mut prefixes: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for &m in members {
        let stem = table_stem(&entities[m].table);
        let first = stem.split(['_', '-']).next().unwrap_or(&stem).to_string();
        prefixes.entry(first).or_default().push(m);
    }
    let mut out = Vec::new();
    let mut rest: Vec<usize> = Vec::new();
    for (prefix, mut ms) in prefixes {
        if ms.len() >= 2 {
            ms.sort_by(|a, b| entities[*a].table.cmp(&entities[*b].table));
            for (k, chunk) in ms.chunks(MAX_ENTITIES).enumerate() {
                let suffix = if k == 0 { String::new() } else { format!("-{}", k + 1) };
                out.push((format!("{name}-{prefix}{suffix}"), chunk.to_vec()));
            }
        } else {
            rest.extend(ms);
        }
    }
    rest.sort_by(|a, b| entities[*a].table.cmp(&entities[*b].table));
    for (k, chunk) in rest.chunks(MAX_ENTITIES).enumerate() {
        let suffix = if k == 0 { String::new() } else { format!("-{}", k + 1) };
        out.push((format!("{name}-other{suffix}"), chunk.to_vec()));
    }
    out
}

/// Folds single-entity domains into the domain they reference most while
/// there are too many domains to navigate; unrelated ones share "other"
/// buckets of at most `MAX_ENTITIES`.
fn fold_small(groups: &mut BTreeMap<String, Vec<usize>>, neighbours: &[BTreeSet<usize>], _total: usize) {
    if groups.len() <= MAX_DOMAINS {
        return;
    }
    let singles: Vec<String> = groups.iter().filter(|(_, m)| m.len() == 1).map(|(g, _)| g.clone()).collect();
    let mut orphans: Vec<usize> = groups.remove("other").unwrap_or_default();
    for g in singles {
        if groups.len() + orphans.len().div_ceil(MAX_ENTITIES) <= MAX_DOMAINS {
            break;
        }
        let Some(members) = groups.get(&g) else { continue };
        let m = members[0];
        let own: BTreeMap<usize, &String> = groups.iter().flat_map(|(k, ms)| ms.iter().map(move |x| (*x, k))).collect();
        let mut votes: BTreeMap<String, usize> = BTreeMap::new();
        for n in &neighbours[m] {
            if let Some(o) = own.get(n).filter(|o| ***o != g) {
                *votes.entry((*o).clone()).or_default() += 1;
            }
        }
        groups.remove(&g);
        match best(&votes).filter(|t| groups.get(t).is_some_and(|ms| ms.len() < MAX_ENTITIES)) {
            Some(t) => groups.entry(t).or_default().push(m),
            None => orphans.push(m),
        }
    }
    orphans.sort();
    for (k, chunk) in orphans.chunks(MAX_ENTITIES).enumerate() {
        let name = if k == 0 { "other".to_string() } else { format!("other-{}", k + 1) };
        groups.entry(name).or_default().extend(chunk.iter().copied());
    }
}

// ── domain summaries and drafts ──────────────────────────────────────────────

/// A domain with its entities, in model order.
#[derive(Debug, Clone)]
pub struct DomainSummary {
    pub name: String,
    /// Human label (`Rule chain`).
    pub label: String,
    pub slug: String,
    pub entities: Vec<String>,
}

/// Domains worth presenting separately: only when the model is too large to
/// read as one figure. Largest first, then by name.
pub fn domains(data: &DataModel) -> Vec<DomainSummary> {
    if data.entities.len() <= MAX_ENTITIES {
        return vec![];
    }
    let mut by: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for e in &data.entities {
        by.entry(e.domain.clone().unwrap_or_else(|| "schema".into())).or_default().push(e.id.clone());
    }
    if by.len() < 2 {
        return vec![];
    }
    let mut out: Vec<DomainSummary> = by
        .into_iter()
        .map(|(name, entities)| DomainSummary {
            label: display_name(&name.replace('-', " ")),
            slug: slug(&name),
            name,
            entities,
        })
        .collect();
    // Named domains by size; the catch-all "other" buckets last.
    let other = |d: &DomainSummary| d.name == "other" || d.name.starts_with("other-") || d.name == "schema";
    out.sort_by(|a, b| other(a).cmp(&other(b)).then(b.entities.len().cmp(&a.entities.len())).then(a.name.cmp(&b.name)));
    out
}

fn entity_node_id(id: &str) -> String {
    slug(id.trim_start_matches("entity:")).replace('-', "_")
}

/// One entity-relationship figure per domain. References to entities in other
/// domains end at a stub card in an "Other domains" boundary, so where the
/// domain touches the rest of the model stays visible.
pub fn draft_domain_irs(report: &ScanReport, data: &DataModel, opts: &DraftOptions) -> Vec<(DomainSummary, Draft)> {
    let doms = domains(data);
    let domain_of: BTreeMap<&str, &str> =
        data.entities.iter().map(|e| (e.id.as_str(), e.domain.as_deref().unwrap_or("schema"))).collect();
    let by_id: BTreeMap<&str, &Entity> = data.entities.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut out = Vec::new();
    for d in doms {
        let subset = DataModel {
            entities: data.entities.iter().filter(|e| d.entities.contains(&e.id)).cloned().collect(),
            state_machines: vec![],
        };
        let Some(mut draft) = crate::views::draft_entity_ir(report, &subset, opts) else { continue };
        draft.ir.title = format!("{} data model", d.label);
        let mut stubs: BTreeSet<String> = BTreeSet::new();
        let budget = autodoc_ir::element_budget();
        for e in &subset.entities {
            for r in &e.relations {
                let Some(target) = by_id.get(r.target.as_str()) else { continue };
                if d.entities.contains(&r.target) {
                    continue;
                }
                let stub_id = format!("ext_{}", entity_node_id(&target.id));
                if draft.ir.nodes.len() + draft.ir.edges.len() + 2 > budget {
                    draft.notes.push(format!("reference {} → {} not drawn (density)", e.table, target.table));
                    continue;
                }
                if stubs.insert(stub_id.clone()) {
                    let other = domain_of.get(target.id.as_str()).copied().unwrap_or("schema");
                    let pk: Vec<Attribute> = target
                        .columns
                        .iter()
                        .filter(|c| c.primary_key)
                        .map(|c| Attribute {
                            name: c.name.clone(),
                            type_name: c.type_name.chars().take(24).collect(),
                            key: Some(KeyKind::Pk),
                            nullable: false,
                            note: None,
                        })
                        .collect();
                    draft.ir.nodes.push(Node {
                        id: stub_id.clone(),
                        container_id: Some("other-domains".into()),
                        label: truncate(&target.table, 32),
                        subtitle: Some(truncate(&format!("in {}", display_name(&other.replace('-', " "))), 40)),
                        tech_stack: None,
                        is_key_focal_point: false,
                        evidence: Some(target.evidence.to_ir()),
                        metadata: None,
                        attributes: (!pk.is_empty()).then_some(pk),
                        state_kind: None,
                    });
                }
                let (source, target_id, card) = match r.kind.as_str() {
                    "many-to-one" => (stub_id.clone(), entity_node_id(&e.id), Cardinality::OneToMany),
                    "one-to-many" => (entity_node_id(&e.id), stub_id.clone(), Cardinality::OneToMany),
                    "one-to-one" => (entity_node_id(&e.id), stub_id.clone(), Cardinality::OneToOne),
                    _ => (entity_node_id(&e.id), stub_id.clone(), Cardinality::ManyToMany),
                };
                if draft.ir.edges.iter().any(|x| {
                    (x.source == source && x.target == target_id) || (x.source == target_id && x.target == source)
                }) {
                    continue;
                }
                draft.ir.edges.push(Edge {
                    id: format!("xref-{}", draft.ir.edges.len() + 1),
                    source,
                    target: target_id,
                    label: Some(truncate(&r.via, 32)).filter(|v| !v.is_empty()),
                    edge_type: EdgeType::Sync,
                    style: Some(autodoc_ir::EdgeStyle::Dashed),
                    is_primary_path: None,
                    sequence: None,
                    reply: None,
                    payload: None,
                    cardinality: Some(card),
                    guard: None,
                    evidence: Some(r.evidence.to_ir()),
                });
            }
        }
        if !stubs.is_empty() {
            draft.ir.containers.push(Container {
                id: "other-domains".into(),
                label: "Other domains".into(),
                boundary_type: BoundaryType::ThirdParty,
                role_description: Some("Entities of other domains referenced from this one".into()),
            });
        }
        draft.ir.metadata.visual_density_score =
            Some(autodoc_ir::visual_density(draft.ir.nodes.len(), draft.ir.edges.len()));
        out.push((d, draft));
    }
    out
}

/// Domains as components: how many references cross between them and which
/// services write and read each. `None` when the model isn't split.
/// Service cards on the overview; beyond this the domains table carries them.
const MAX_OVERVIEW_SERVICES: usize = 6;
const OTHER_DOMAINS: &str = "domain-other-grouped";

pub fn draft_domain_overview_ir(report: &ScanReport, data: &DataModel, opts: &DraftOptions) -> Option<Draft> {
    let doms = domains(data);
    if doms.len() < 2 {
        return None;
    }
    let mut notes = Vec::new();
    let domain_of: BTreeMap<&str, &str> =
        data.entities.iter().map(|e| (e.id.as_str(), e.domain.as_deref().unwrap_or("schema"))).collect();
    let dom_id = |name: &str| format!("domain-{}", slug(name));
    let mut ir = autodoc_ir::DiagramIR {
        version: autodoc_ir::IrVersion::V1_1_0,
        diagram_type: DiagramType::Component,
        title: "Data domains".into(),
        subtitle: Some(format!("{} entities in {} domains", data.entities.len(), doms.len())),
        theme: opts.theme,
        metadata: autodoc_ir::DiagramMetadata {
            target_repo: report.repo.root.clone(),
            commit_hash: report.repo.commit_hash.clone(),
            generated_at: opts.generated_at.clone().unwrap_or_else(autodoc_ir::now_rfc3339),
            visual_density_score: None,
        },
        containers: vec![
            Container {
                id: "services".into(),
                label: "Services".into(),
                boundary_type: BoundaryType::InternalService,
                role_description: Some("Code that reads and writes the data".into()),
            },
            Container {
                id: "domains".into(),
                label: "Data domains".into(),
                boundary_type: BoundaryType::Storage,
                role_description: Some("Entities grouped by business domain".into()),
            },
        ],
        nodes: vec![],
        edges: vec![],
    };
    for d in &doms {
        let tables: Vec<String> =
            data.entities.iter().filter(|e| d.entities.contains(&e.id)).take(3).map(|e| e.table.clone()).collect();
        let first = data.entities.iter().find(|e| d.entities.contains(&e.id));
        ir.nodes.push(Node {
            id: dom_id(&d.name),
            container_id: Some("domains".into()),
            label: truncate(&d.label, 32),
            subtitle: Some(format!("{} entit{}", d.entities.len(), if d.entities.len() == 1 { "y" } else { "ies" })),
            tech_stack: Some(truncate(&tables.join(", "), 40)),
            is_key_focal_point: false,
            evidence: first.map(|e| e.evidence.to_ir()),
            metadata: None,
            attributes: None,
            state_kind: None,
        });
    }
    // Cross-domain references, weighted.
    let mut refs: BTreeMap<(String, String), (usize, EvidenceRef)> = BTreeMap::new();
    for e in &data.entities {
        let from = domain_of[e.id.as_str()];
        for r in &e.relations {
            let Some(to) = domain_of.get(r.target.as_str()) else { continue };
            if *to == from {
                continue;
            }
            let entry = refs.entry((from.to_string(), to.to_string())).or_insert((0, r.evidence.clone()));
            entry.0 += 1;
        }
    }
    let mut ref_edges: Vec<((String, String), (usize, EvidenceRef))> = refs.into_iter().collect();
    ref_edges.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(&b.0)));
    // Services writing / reading each domain.
    let mut access: BTreeMap<(String, String, bool), EvidenceRef> = BTreeMap::new();
    for e in &data.entities {
        let dom = domain_of[e.id.as_str()];
        for (write, list) in [(true, &e.writes), (false, &e.reads)] {
            for a in list {
                access.entry((a.unit.clone(), dom.to_string(), write)).or_insert_with(|| a.evidence.clone());
            }
        }
    }
    let units: BTreeSet<&str> = access.keys().map(|(u, _, _)| u.as_str()).collect();
    let budget = autodoc_ir::element_budget();

    // One edge per service/domain pair (a write implies the read): these connect
    // domains that no foreign key reaches.
    let mut pairs: BTreeMap<(String, String), (bool, EvidenceRef)> = BTreeMap::new();
    for ((u, d, write), ev) in &access {
        let entry = pairs.entry((u.clone(), d.clone())).or_insert((*write, ev.clone()));
        if *write && !entry.0 {
            *entry = (true, ev.clone());
        }
    }
    // Services that touch the most domains earn a card; the rest are counted.
    let mut by_unit: BTreeMap<&str, usize> = BTreeMap::new();
    for (u, _) in pairs.keys() {
        *by_unit.entry(u.as_str()).or_default() += 1;
    }
    let mut ranked: Vec<(&str, usize)> = by_unit.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let drawn_units: BTreeSet<&str> = ranked.iter().take(MAX_OVERVIEW_SERVICES).map(|(u, _)| *u).collect();
    if units.len() > drawn_units.len() {
        notes.push(format!(
            "{} of {} services drawn (most domains touched); the domains table lists every reader and writer",
            drawn_units.len(),
            units.len()
        ));
    }
    for u in &drawn_units {
        let summary = report.containers.iter().find(|c| c.id == *u);
        ir.nodes.push(Node {
            id: u.to_string(),
            container_id: Some("services".into()),
            label: truncate(&summary.map(|s| s.name.clone()).unwrap_or_else(|| u.to_string()), 32),
            subtitle: summary.map(|s| s.kind.role().to_string()),
            tech_stack: summary.map(|s| truncate(&s.tech_stack, 40)),
            is_key_focal_point: false,
            evidence: report.evidence_map.get(*u).map(|e| e.to_ir()),
            metadata: None,
            attributes: None,
            state_kind: None,
        });
    }

    let mut edges: Vec<Edge> = Vec::new();
    for ((from, to), (count, ev)) in ref_edges {
        edges.push(Edge {
            id: crate::scan::edge_id(&dom_id(&from), &dom_id(&to)),
            source: dom_id(&from),
            target: dom_id(&to),
            label: Some(format!("{count} reference{}", if count == 1 { "" } else { "s" })),
            edge_type: EdgeType::Sync,
            style: None,
            is_primary_path: None,
            sequence: None,
            reply: None,
            payload: None,
            cardinality: None,
            guard: None,
            evidence: Some(ev.to_ir()),
        });
    }
    for ((u, d), (write, ev)) in pairs.iter().filter(|((u, _), _)| drawn_units.contains(u.as_str())) {
        edges.push(Edge {
            id: crate::scan::edge_id(u, &dom_id(d)),
            source: u.clone(),
            target: dom_id(d),
            label: Some(if *write { "reads & writes" } else { "reads" }.into()),
            edge_type: if *write { EdgeType::Write } else { EdgeType::Read },
            style: None,
            is_primary_path: None,
            sequence: None,
            reply: None,
            payload: None,
            cardinality: None,
            guard: None,
            evidence: Some(ev.to_ir()),
        });
    }

    // Over budget, the smallest domains fold into one card rather than vanish.
    let mut folded: Vec<String> = Vec::new();
    let mut order: Vec<(String, usize)> = doms.iter().map(|d| (d.name.clone(), d.entities.len())).collect();
    order.sort_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)));
    let mut queue = order.into_iter().map(|(n, _)| n);
    while ir.nodes.len() + edges.len() > budget {
        let Some(name) = queue.next() else { break };
        if doms.len() - folded.len() <= 2 {
            break;
        }
        let from = dom_id(&name);
        ir.nodes.retain(|n| n.id != from);
        for e in edges.iter_mut() {
            if e.source == from {
                e.source = OTHER_DOMAINS.into();
            }
            if e.target == from {
                e.target = OTHER_DOMAINS.into();
            }
        }
        edges.retain(|e| e.source != e.target);
        let mut seen = BTreeSet::new();
        edges.retain(|e| seen.insert((e.source.clone(), e.target.clone())));
        folded.push(name);
        if !ir.nodes.iter().any(|n| n.id == OTHER_DOMAINS) {
            ir.nodes.push(Node {
                id: OTHER_DOMAINS.into(),
                container_id: Some("domains".into()),
                label: "Other domains".into(),
                subtitle: Some("grouped to fit".into()),
                tech_stack: None,
                is_key_focal_point: false,
                evidence: None,
                metadata: None,
                attributes: None,
                state_kind: None,
            });
        }
        let entities: usize = doms.iter().filter(|d| folded.contains(&d.name)).map(|d| d.entities.len()).sum();
        if let Some(n) = ir.nodes.iter_mut().find(|n| n.id == OTHER_DOMAINS) {
            n.label = truncate(&format!("Other domains ({})", folded.len()), 32);
            n.subtitle = Some(format!("{entities} entities"));
            n.tech_stack = Some(truncate(&folded.join(", "), 40));
        }
    }
    if !folded.is_empty() {
        notes.push(format!("grouped the smallest domains to meet density: {}", folded.join(", ")));
    }
    ir.edges = edges;

    // A card with no edge would be rejected as an orphan; the domains table
    // still lists every domain, with its entities and who touches them.
    let connected: BTreeSet<String> = ir.edges.iter().flat_map(|e| [e.source.clone(), e.target.clone()]).collect();
    let dropped: Vec<String> = ir
        .nodes
        .iter()
        .filter(|n| !connected.contains(&n.id) && n.container_id.as_deref() == Some("domains"))
        .map(|n| n.label.clone())
        .collect();
    ir.nodes.retain(|n| connected.contains(&n.id));
    if !dropped.is_empty() {
        notes.push(format!(
            "nothing in the repository reads or references {}; listed in the domains table",
            dropped.join(", ")
        ));
    }
    if ir.nodes.len() < 2 {
        return None;
    }
    if let Some(top) = ir
        .nodes
        .iter()
        .filter(|n| n.container_id.as_deref() == Some("domains") && n.id != OTHER_DOMAINS)
        .max_by_key(|n| {
            (ir.edges.iter().filter(|e| e.source == n.id || e.target == n.id).count(), std::cmp::Reverse(n.id.clone()))
        })
        .map(|n| n.id.clone())
    {
        ir.nodes.iter_mut().filter(|n| n.id == top).for_each(|n| n.is_key_focal_point = true);
    }
    ir.containers.retain(|c| ir.nodes.iter().any(|n| n.container_id.as_deref() == Some(c.id.as_str())));
    ir.metadata.visual_density_score = Some(autodoc_ir::visual_density(ir.nodes.len(), ir.edges.len()));
    Some(Draft { ir, notes })
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

// ── module boundaries ────────────────────────────────────────────────────────

const MAX_PUBLIC_TYPES: usize = 12;

/// Adds public types, events, internal packages and boundary violations to a
/// component view.
pub(crate) fn module_facts(root: &Path, unit: &Unit, files: &[FileRec], view: &mut ComponentView) {
    let by_path: BTreeMap<&str, usize> = unit.files.iter().map(|&fi| (files[fi].path.as_str(), fi)).collect();
    let module_files: Vec<Vec<usize>> = view
        .modules
        .iter()
        .map(|m| m.files.iter().filter_map(|p| by_path.get(p.as_str()).copied()).collect())
        .collect();
    let read = |fi: usize| std::fs::read_to_string(root.join(&files[fi].path)).unwrap_or_default();

    let modulith_dep = unit
        .manifest
        .as_ref()
        .is_some_and(|m| m.dependencies.iter().any(|d| d.name.starts_with("org.springframework.modulith")));
    let mut declared = modulith_dep;

    // Per module: root package, named-interface packages, allowed dependencies.
    struct JavaModule {
        root: Option<String>,
        packages: BTreeSet<String>,
        named: BTreeSet<String>,
        allowed: Option<Vec<String>>,
        allowed_evidence: Option<EvidenceRef>,
    }
    let mut java: Vec<JavaModule> = Vec::new();
    for fis in &module_files {
        let packages: BTreeSet<String> = fis.iter().filter_map(|&fi| files[fi].facts.package.clone()).collect();
        let root_pkg = packages.iter().min_by_key(|p| (p.len(), (*p).clone())).cloned();
        let mut named = BTreeSet::new();
        let mut allowed = None;
        let mut allowed_evidence = None;
        for &fi in fis.iter().filter(|&&fi| files[fi].path.ends_with("package-info.java")) {
            let text = read(fi);
            let pkg = files[fi].facts.package.clone().unwrap_or_default();
            if text.contains("@NamedInterface") || text.contains(".NamedInterface") {
                named.insert(pkg.clone());
                declared = true;
            }
            if text.contains("@ApplicationModule") || text.contains(".ApplicationModule(") {
                declared = true;
                if Some(&pkg) == root_pkg.as_ref() {
                    if let Some((list, line)) = allowed_dependencies(&text) {
                        allowed = Some(list);
                        allowed_evidence = Some(EvidenceRef {
                            file_path: files[fi].path.clone(),
                            start_line: line,
                            end_line: line,
                            symbol_name: None,
                            note: Some("`@ApplicationModule(allowedDependencies)`".into()),
                        });
                    }
                }
            }
        }
        java.push(JavaModule { root: root_pkg, packages, named, allowed, allowed_evidence });
    }

    // Without declared boundaries (Spring Modulith) a sub-package is internal
    // only when its name says so; package-by-layer apps have no module rules.
    let is_internal = |jm: &JavaModule, p: &String| {
        jm.root.as_ref() != Some(p)
            && !jm.named.contains(p)
            && (declared || p.split('.').any(|seg| seg == "internal" || seg == "impl"))
    };
    for (i, m) in view.modules.iter_mut().enumerate() {
        let fis = &module_files[i];
        let jm = &java[i];
        let mut publishes = BTreeSet::new();
        let mut consumes = BTreeSet::new();
        for &fi in fis {
            for ev in &files[fi].facts.events {
                if ev.kind == "publish" {
                    publishes.insert(ev.event_type.clone());
                } else {
                    consumes.insert(ev.event_type.clone());
                }
            }
        }
        m.publishes = publishes.into_iter().collect();
        m.consumes = consumes.into_iter().collect();
        m.internal_packages = jm.packages.iter().filter(|p| is_internal(jm, p)).cloned().collect();
        let mut public = Vec::new();
        for &fi in fis {
            let f = &files[fi];
            let exposed = match f.language {
                Language::Java | Language::Kotlin => {
                    f.facts.package.as_ref().is_some_and(|p| Some(p) == jm.root.as_ref() || jm.named.contains(p))
                }
                Language::TypeScript | Language::JavaScript => {
                    let name = f.path.rsplit('/').next().unwrap_or("");
                    name.starts_with("index.")
                }
                _ => false,
            };
            if !exposed {
                continue;
            }
            for s in f.facts.symbols.iter().filter(|s| {
                s.exported
                    && matches!(
                        s.kind,
                        SymbolKind::Class
                            | SymbolKind::Interface
                            | SymbolKind::Enum
                            | SymbolKind::Struct
                            | SymbolKind::TypeAlias
                            | SymbolKind::Function
                    )
            }) {
                if public.len() == MAX_PUBLIC_TYPES {
                    break;
                }
                // Top-level types only for Java (nested public classes aren't the module's API).
                if f.language.is_jvm()
                    && f.facts.symbols.iter().any(|o| {
                        matches!(o.kind, SymbolKind::Class | SymbolKind::Interface | SymbolKind::Enum)
                            && o.start_line < s.start_line
                            && o.end_line >= s.end_line
                            && o.name != s.name
                    })
                {
                    continue;
                }
                if f.language.is_jvm() && s.kind == SymbolKind::Function {
                    continue;
                }
                public.push(SymbolRef {
                    name: s.name.clone(),
                    kind: s.kind,
                    evidence: EvidenceRef {
                        file_path: f.path.clone(),
                        start_line: s.start_line,
                        end_line: s.end_line,
                        symbol_name: Some(s.name.clone()),
                        note: None,
                    },
                    doc: s.doc.as_deref().and_then(|d| d.lines().next()).map(str::to_string),
                });
            }
        }
        public.sort_by(|a, b| a.name.cmp(&b.name));
        public.dedup_by(|a, b| a.name == b.name);
        m.public_types = public;
    }

    // Violations: imports into another module's internals, and dependencies
    // outside `allowedDependencies`.
    let module_name = |jm: &JavaModule| jm.root.as_deref().and_then(|r| r.rsplit('.').next()).unwrap_or("").to_string();
    let mut violations = Vec::new();
    let mut seen = BTreeSet::new();
    for (a, fis) in module_files.iter().enumerate() {
        for &fi in fis {
            let f = &files[fi];
            if !f.language.is_jvm() {
                continue;
            }
            for imp in &f.facts.imports {
                let spec = imp.specifier.as_str();
                let inside = |p: &str| spec.strip_prefix(p).is_some_and(|r| r.starts_with('.'));
                // The module owning the referenced type: longest root package that contains it.
                let owner = java
                    .iter()
                    .enumerate()
                    .filter_map(|(b, jb)| jb.root.as_deref().filter(|r| inside(r)).map(|r| (r.len(), b)))
                    .max()
                    .map(|(_, b)| b);
                if let Some(b) = owner.filter(|b| *b != a) {
                    let jb = &java[b];
                    let evidence = EvidenceRef {
                        file_path: f.path.clone(),
                        start_line: imp.line,
                        end_line: imp.line,
                        symbol_name: None,
                        note: Some(format!("imports `{spec}`")),
                    };
                    let internal = jb
                        .packages
                        .iter()
                        .filter(|p| is_internal(jb, p))
                        .any(|p| inside(p) && !spec[p.len() + 1..].contains('.'));
                    if internal && seen.insert((a, b, "internal", spec.to_string())) {
                        violations.push(Violation {
                            from: view.modules[a].id.clone(),
                            to: view.modules[b].id.clone(),
                            kind: "internal".into(),
                            specifier: spec.to_string(),
                            evidence: evidence.clone(),
                        });
                    }
                    if let Some(allowed) = &java[a].allowed {
                        let target = module_name(jb);
                        let ok = allowed.iter().any(|x| x.split("::").next() == Some(target.as_str()));
                        if !ok && seen.insert((a, b, "not-allowed", String::new())) {
                            let mut ev = evidence.clone();
                            if let Some(decl) = &java[a].allowed_evidence {
                                ev.note = Some(format!(
                                    "imports `{spec}`; `{}` allows only {}",
                                    decl.file_path,
                                    if allowed.is_empty() { "none".to_string() } else { allowed.join(", ") }
                                ));
                            }
                            violations.push(Violation {
                                from: view.modules[a].id.clone(),
                                to: view.modules[b].id.clone(),
                                kind: "not-allowed".into(),
                                specifier: spec.to_string(),
                                evidence: ev,
                            });
                        }
                    }
                }
            }
        }
    }
    violations.sort_by(|x, y| (&x.from, &x.to, &x.kind, &x.specifier).cmp(&(&y.from, &y.to, &y.kind, &y.specifier)));
    view.violations = violations;
    view.boundaries_declared = declared;
}

/// `allowedDependencies = {"order", "inventory::api"}` / `= "order"` / `= {}`, with its line.
fn allowed_dependencies(text: &str) -> Option<(Vec<String>, u32)> {
    let at = text.find("allowedDependencies")?;
    let line = text[..at].matches('\n').count() as u32 + 1;
    let rest = text[at + "allowedDependencies".len()..].trim_start().strip_prefix('=')?.trim_start();
    let value = if rest.starts_with('{') { &rest[1..rest.find('}')?] } else { rest.split([',', ')']).next()? };
    let items = value.split(',').map(|s| s.trim().trim_matches('"').to_string()).filter(|s| !s.is_empty()).collect();
    Some((items, line))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_domains_skip_technical_packages() {
        assert_eq!(java_domain("com.acme.shop.orders.domain", Some("com.acme.shop")).as_deref(), Some("orders"));
        assert_eq!(java_domain("org.thingsboard.server.dao.model.sql", Some("org.thingsboard.server.dao")), None);
        assert_eq!(
            java_domain("org.thingsboard.server.dao.sql.device", Some("org.thingsboard.server.dao")).as_deref(),
            Some("device")
        );
        assert_eq!(java_domain("com.acme.shop", Some("com.acme.shop")), None);
        assert_eq!(java_domain("com.other.billing", Some("com.acme.shop")).as_deref(), Some("other"));
    }

    fn ev(line: u32) -> EvidenceRef {
        EvidenceRef { file_path: "schema.sql".into(), start_line: line, end_line: line, symbol_name: None, note: None }
    }

    fn entity(table: &str, refs: &[&str]) -> Entity {
        Entity {
            id: format!("entity:{table}"),
            name: table.into(),
            table: table.into(),
            source: "sql-ddl".into(),
            units: vec![],
            columns: vec![],
            relations: refs
                .iter()
                .map(|t| crate::data::Relation {
                    kind: "many-to-one".into(),
                    target: format!("entity:{t}"),
                    via: format!("{t}_id"),
                    evidence: ev(1),
                })
                .collect(),
            reads: vec![],
            writes: vec![],
            evidence: ev(1),
            domain: None,
        }
    }

    #[test]
    fn large_undeclared_models_split_by_foreign_keys_then_prefix_deterministically() {
        // Two FK clusters of 8 around `tb_device` and `order`, one 14-table
        // `rule_*` family without keys, and 20 unrelated singletons.
        let mut es = vec![entity("tb_device", &[])];
        for i in 0..7 {
            es.push(entity(&format!("device_part_{i}"), &["tb_device"]));
        }
        es.push(entity("order", &[]));
        for i in 0..7 {
            es.push(entity(&format!("order_line_{i}"), &["order"]));
        }
        for i in 0..14 {
            es.push(entity(&format!("rule_{i:02}"), &[]));
        }
        for i in 0..20 {
            es.push(entity(&format!("misc{i:02}"), &[]));
        }
        let index = SourceIndex { root: std::path::PathBuf::from("."), units: &[], files: vec![] };
        let run = |mut es: Vec<Entity>| {
            assign(&mut es, &index);
            let mut by: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for e in &es {
                by.entry(e.domain.clone().unwrap()).or_default().push(e.table.clone());
            }
            by
        };
        let by = run(es.clone());
        assert_eq!(by["device"].len(), 8, "{by:#?}");
        assert_eq!(by["order"].len(), 8, "{by:#?}");
        assert!(by.values().all(|v| v.len() <= MAX_ENTITIES), "{by:#?}");
        assert!(by.len() <= MAX_DOMAINS, "{} domains: {:?}", by.len(), by.keys().collect::<Vec<_>>());
        let placed: usize = by.values().map(Vec::len).sum();
        assert_eq!(placed, es.len());
        assert_eq!(run(es), by, "deterministic");
    }

    #[test]
    fn allowed_dependencies_parse() {
        let t = "/** x */\n@ApplicationModule(\n  allowedDependencies = {\"order\", \"inventory::api\"})\npackage com.example.shipping;\n";
        assert_eq!(allowed_dependencies(t), Some((vec!["order".into(), "inventory::api".into()], 3)));
        assert_eq!(
            allowed_dependencies("@ApplicationModule(allowedDependencies = \"order\")"),
            Some((vec!["order".into()], 1))
        );
        assert_eq!(allowed_dependencies("@ApplicationModule(allowedDependencies = {})"), Some((vec![], 1)));
    }
}
