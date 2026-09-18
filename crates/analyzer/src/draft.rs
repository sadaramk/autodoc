//! Deterministic baseline `DiagramIR` from a scan. Agents refine this rather
//! than starting from a blank page; the CLI can render it directly.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use autodoc_ir::*;

use crate::catalog::InfraCategory;
use crate::scan::{display_name, ComponentView, Depth, InfraSummary, ScanReport, UnitKind, UnitSummary};

#[derive(Debug, Clone)]
pub struct DraftOptions {
    pub theme: Theme,
    /// Fixed timestamp for reproducible output; `None` uses the clock.
    pub generated_at: Option<String>,
}

impl Default for DraftOptions {
    fn default() -> Self {
        DraftOptions { theme: Theme::EditorialLight, generated_at: None }
    }
}

#[derive(Debug, Clone)]
pub struct Draft {
    pub ir: DiagramIR,
    /// What the drafter decided on the agent's behalf (merges, drops, picks).
    pub notes: Vec<String>,
}

pub fn draft_ir(report: &ScanReport, opts: &DraftOptions) -> Draft {
    let mut notes = Vec::new();
    let mut ir = DiagramIR {
        version: IrVersion::V1_0_0,
        diagram_type: DiagramType::Container,
        title: String::new(),
        subtitle: report.system.description.as_deref().map(first_sentence),
        theme: opts.theme,
        metadata: DiagramMetadata {
            target_repo: report.repo.root.clone(),
            commit_hash: report.repo.commit_hash.clone(),
            generated_at: opts.generated_at.clone().unwrap_or_else(now_rfc3339),
            visual_density_score: None,
        },
        containers: vec![],
        nodes: vec![],
        edges: vec![],
    };
    match report.depth {
        Depth::System => system_context(report, &mut ir, &mut notes),
        Depth::Container => containers(report, &mut ir, &mut notes),
        Depth::Component => components(report, report.components.as_ref(), &mut ir, &mut notes),
    }
    drop_orphans(&mut ir, &mut notes);
    compact(&mut ir, &mut notes);
    drop_unused_containers(&mut ir);
    ir.metadata.visual_density_score = Some(visual_density(ir.nodes.len(), ir.edges.len()));
    Draft { ir, notes }
}

fn first_sentence(s: &str) -> String {
    let end = s.find(". ").map(|i| i + 1).unwrap_or(s.len());
    let out: String = s[..end].chars().take(140).collect();
    out.trim().trim_end_matches('.').to_string()
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n - 1).collect();
        out = out.trim_end().to_string();
        out.push('…');
        out
    }
}

fn boundary(id: &str, label: &str, t: BoundaryType, role: &str) -> Container {
    Container { id: id.into(), label: label.into(), boundary_type: t, role_description: Some(role.into()) }
}

fn meta(pairs: &[(&str, Option<String>)]) -> Option<BTreeMap<String, String>> {
    let m: BTreeMap<String, String> = pairs
        .iter()
        .filter_map(|(k, v)| v.as_ref().filter(|v| !v.is_empty()).map(|v| (k.to_string(), v.clone())))
        .collect();
    (!m.is_empty()).then_some(m)
}

fn unit_node(u: &UnitSummary, report: &ScanReport, container: &str) -> Node {
    let entry = u.entry_points.first();
    Node {
        id: u.id.clone(),
        container_id: Some(container.into()),
        label: truncate(&u.name, 28),
        subtitle: Some(
            u.frameworks
                .first()
                .and_then(|f| {
                    crate::catalog::platform_service(f)
                        .map(|(role, _)| role)
                        .or_else(|| crate::catalog::gateway_framework(f).then_some("API gateway"))
                })
                .unwrap_or(u.kind.role())
                .into(),
        ),
        tech_stack: Some(u.tech_stack.clone()),
        is_key_focal_point: false,
        evidence: report.evidence_map.get(&u.id).map(|e| e.to_ir()),
        metadata: meta(&[
            ("docstring", u.description.clone()),
            ("path", Some(if u.root.is_empty() { ".".into() } else { format!("{}/", u.root) })),
            ("language", Some(u.language.display().into())),
            ("frameworks", Some(u.frameworks.join(", "))),
            ("size", Some(format!("{} files · {} lines", u.files, u.lines))),
            ("entryPoint", entry.and_then(|e| e.note.clone())),
            ("composeService", u.compose_service.clone()),
        ]),
        attributes: None,
        state_kind: None,
    }
}

fn infra_node(i: &InfraSummary, report: &ScanReport, container: &str) -> Node {
    Node {
        id: i.id.clone(),
        container_id: Some(container.into()),
        label: i.label.clone(),
        subtitle: Some(i.role.clone()),
        tech_stack: i.compose_service.as_ref().map(|s| format!("compose: {s}")),
        is_key_focal_point: false,
        evidence: report.evidence_map.get(&i.id).map(|e| e.to_ir()),
        metadata: meta(&[
            ("usedBy", Some(i.used_by.join(", "))),
            ("topics", Some(i.topics.join(", "))),
            ("category", Some(format!("{:?}", i.category).to_lowercase())),
        ]),
        attributes: None,
        state_kind: None,
    }
}

fn infra_container(i: &InfraSummary) -> &'static str {
    match i.category {
        InfraCategory::ThirdParty => "external",
        _ => "data",
    }
}

fn containers(report: &ScanReport, ir: &mut DiagramIR, notes: &mut Vec<String>) {
    ir.diagram_type = DiagramType::Container;
    ir.title = format!("{} — containers", report.system.name);
    ir.containers = vec![
        boundary("clients", "Clients", BoundaryType::Client, "User-facing applications"),
        boundary(
            "platform",
            &format!("{} platform", report.system.name),
            BoundaryType::TrustZone,
            "Services deployed from this repository",
        ),
        boundary("data", "Data & messaging", BoundaryType::Storage, "Stateful infrastructure"),
        boundary(
            "operations",
            "Platform services",
            BoundaryType::InternalService,
            "Configuration, discovery and monitoring the services depend on",
        ),
        boundary(
            "external",
            "Third-party services",
            BoundaryType::ThirdParty,
            "Vendor APIs outside the trust boundary",
        ),
    ];
    ir.containers.insert(
        2,
        boundary(
            "libraries",
            "Shared libraries",
            BoundaryType::InternalService,
            "Workspace packages linked into the deployables",
        ),
    );
    let runnable = report.containers.iter().filter(|u| u.kind != UnitKind::Library).count();
    // A workspace library earns a place when a deployable (or another library) links it.
    let linked: BTreeSet<&str> =
        report.relationships.iter().filter(|r| r.label == "uses").map(|r| r.target.as_str()).collect();
    for u in &report.containers {
        let c = match u.kind {
            UnitKind::WebClient => "clients",
            UnitKind::Library if runnable == 0 => "platform",
            UnitKind::Library if linked.contains(u.id.as_str()) => "libraries",
            UnitKind::Library => {
                notes.push(format!("omitted library `{}`: nothing in the repository links it", u.id));
                continue;
            }
            _ if u.frameworks.first().is_some_and(|f| crate::catalog::platform_service(f).is_some()) => "operations",
            _ => "platform",
        };
        ir.nodes.push(unit_node(u, report, c));
    }
    for i in &report.infrastructure {
        ir.nodes.push(infra_node(i, report, infra_container(i)));
    }
    add_relationship_edges(report, ir);
    lift_library_infrastructure(report, ir, notes);
    // Edges into platform services say what the dependency is for.
    for e in ir.edges.iter_mut() {
        let Some(t) = report.containers.iter().find(|u| u.id == e.target) else { continue };
        if let Some((_, verb)) = t.frameworks.first().and_then(|f| crate::catalog::platform_service(f)) {
            if e.label.as_deref().is_none_or(|l| l == "HTTP") {
                e.label = Some(verb.into());
            }
        }
    }
    // A platform service most deployables depend on (config server, registry) is
    // cross-cutting: one edge per service says nothing and crowds out the data
    // stores. The Architecture page lists it with its role.
    let services = ir.nodes.iter().filter(|n| n.container_id.as_deref() == Some("platform")).count();
    let ops: Vec<String> =
        ir.nodes.iter().filter(|n| n.container_id.as_deref() == Some("operations")).map(|n| n.id.clone()).collect();
    for id in ops {
        let users = ir.edges.iter().filter(|e| e.target == id).count();
        if services >= 3 && users * 2 >= services {
            let label = ir.nodes.iter().find(|n| n.id == id).map(|n| n.label.clone()).unwrap_or_default();
            ir.edges.retain(|e| e.target != id && e.source != id);
            ir.nodes.retain(|n| n.id != id);
            notes.push(format!("left out `{id}` ({label}): cross-cutting platform service used by {users} services"));
        }
    }
    group_libraries(report, ir, notes);
    reduce_transitive_uses(ir, notes);
    // Without a client in the repository, the front door (nothing in the
    // platform calls it) tells the story better than the busiest back end.
    let has_clients = ir.nodes.iter().any(|n| n.container_id.as_deref() == Some("clients"));
    let in_platform =
        |ir: &DiagramIR, id: &str| ir.nodes.iter().any(|n| n.id == id && n.container_id.as_deref() == Some("platform"));
    let called: BTreeSet<String> =
        ir.edges.iter().filter(|e| in_platform(ir, &e.source)).map(|e| e.target.clone()).collect();
    let front_doors: Vec<String> = ir
        .nodes
        .iter()
        .filter(|n| n.container_id.as_deref() == Some("platform") && !called.contains(&n.id))
        .filter(|n| ir.edges.iter().any(|e| e.source == n.id && in_platform(ir, &e.target)))
        .map(|n| n.id.clone())
        .collect();
    let focal = if !has_clients && front_doors.len() == 1 {
        let id = front_doors[0].clone();
        ir.nodes.iter_mut().filter(|n| n.id == id).for_each(|n| n.is_key_focal_point = true);
        Some(id)
    } else {
        pick_focal(ir, |n| n.container_id.as_deref() == Some("platform"))
    };
    mark_primary_path(ir, focal.as_deref(), &["clients"]);
}

/// Modules that import each other (directly or around a loop) are a design
/// fact worth naming; an unlabeled loop would also leave readers guessing which
/// way the dependency runs.
fn label_import_cycles(ir: &mut DiagramIR) {
    let reaches = |ir: &DiagramIR, from: &str, to: &str| {
        let mut stack = vec![from.to_string()];
        let mut seen = BTreeSet::new();
        while let Some(cur) = stack.pop() {
            if cur == to {
                return true;
            }
            if !seen.insert(cur.clone()) {
                continue;
            }
            stack.extend(ir.edges.iter().filter(|e| e.source == cur).map(|e| e.target.clone()));
        }
        false
    };
    let cyclic: Vec<usize> = (0..ir.edges.len())
        .filter(|&i| ir.edges[i].label.is_none() && reaches(ir, &ir.edges[i].target, &ir.edges[i].source))
        .collect();
    for i in cyclic {
        ir.edges[i].label = Some("cyclic import".into());
    }
}

/// A deployable that links a library reaches whatever that library talks to
/// (`app → dao → PostgreSQL`). Those edges are added to the deployable so the
/// data stores stay connected when libraries are grouped or compacted away.
fn lift_library_infrastructure(report: &ScanReport, ir: &mut DiagramIR, notes: &mut Vec<String>) {
    let libraries: BTreeSet<&str> =
        report.containers.iter().filter(|u| u.kind == UnitKind::Library).map(|u| u.id.as_str()).collect();
    let infra: BTreeSet<&str> = report.infrastructure.iter().map(|i| i.id.as_str()).collect();
    let on_canvas: BTreeSet<String> = ir.nodes.iter().map(|n| n.id.clone()).collect();
    let mut lifted = 0;
    for u in report.containers.iter().filter(|u| u.kind != UnitKind::Library && on_canvas.contains(&u.id)) {
        // Libraries reachable through `uses` links, breadth first.
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut queue: VecDeque<&str> = VecDeque::from([u.id.as_str()]);
        while let Some(cur) = queue.pop_front() {
            for r in report.relationships.iter().filter(|r| r.source == cur && r.label == "uses") {
                if libraries.contains(r.target.as_str()) && seen.insert(r.target.as_str()) {
                    queue.push_back(r.target.as_str());
                }
            }
        }
        for lib in seen {
            // Outbound (writes, publishes) and inbound (event delivery) alike.
            let touching = report.relationships.iter().filter_map(|r| {
                if r.source == lib && infra.contains(r.target.as_str()) {
                    Some((u.id.clone(), r.target.clone(), r))
                } else if r.target == lib && infra.contains(r.source.as_str()) {
                    Some((r.source.clone(), u.id.clone(), r))
                } else {
                    None
                }
            });
            for (source, target, r) in touching.collect::<Vec<_>>() {
                let other = if source == u.id { &target } else { &source };
                if !on_canvas.contains(other) || ir.edges.iter().any(|e| e.source == source && e.target == target) {
                    continue;
                }
                ir.edges.push(Edge {
                    id: format!("{source}--{target}--via-{lib}"),
                    source,
                    target,
                    label: Some(truncate(&format!("{} (via {lib})", r.label), 32)),
                    edge_type: r.edge_type,
                    style: None,
                    is_primary_path: None,
                    sequence: None,
                    reply: None,
                    payload: None,
                    cardinality: None,
                    guard: None,
                    evidence: None,
                });
                lifted += 1;
            }
        }
    }
    if lifted > 0 {
        notes.push(format!(
            "attributed {lifted} datastore/queue/vendor link(s) made in libraries to the deployables that link them"
        ));
    }
}

fn add_relationship_edges(report: &ScanReport, ir: &mut DiagramIR) {
    let ids: BTreeSet<&str> = ir.nodes.iter().map(|n| n.id.as_str()).collect();
    for r in &report.relationships {
        if ids.contains(r.source.as_str()) && ids.contains(r.target.as_str()) {
            ir.edges.push(Edge {
                id: r.id.clone(),
                source: r.source.clone(),
                target: r.target.clone(),
                label: (!r.label.is_empty()).then(|| truncate(&r.label, 28)),
                edge_type: r.edge_type,
                style: None,
                is_primary_path: None,
                sequence: None,
                reply: None,
                payload: None,
                cardinality: None,
                guard: None,
                evidence: None,
            });
        }
    }
}

const MAX_LIBRARY_NODES: usize = 5;

/// Large workspaces (dozens of crates or packages) collapse libraries that
/// share a parent directory into one node: `crates/db_views/*` becomes
/// "db_views (24 crates)".
fn group_libraries(report: &ScanReport, ir: &mut DiagramIR, notes: &mut Vec<String>) {
    let libs: Vec<String> =
        ir.nodes.iter().filter(|n| n.container_id.as_deref() == Some("libraries")).map(|n| n.id.clone()).collect();
    if libs.len() <= MAX_LIBRARY_NODES {
        return;
    }
    let parent_of = |id: &str| {
        let root = report.containers.iter().find(|u| u.id == id).map(|u| u.root.clone()).unwrap_or_default();
        root.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default()
    };
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for id in &libs {
        groups.entry(parent_of(id)).or_default().push(id.clone());
    }
    let mut rename: HashMap<String, String> = HashMap::new();
    for (parent, members) in groups.into_iter().filter(|(p, m)| m.len() >= 2 && !p.is_empty()) {
        let group_id = format!("libs-{}", crate::scan::slug(&parent));
        let last = parent.rsplit('/').next().unwrap_or(&parent);
        let lang = members
            .first()
            .and_then(|m| report.containers.iter().find(|u| &u.id == m))
            .map(|u| u.language)
            .unwrap_or(crate::lang::Language::Other);
        let unit_word = match lang {
            crate::lang::Language::Rust => "crates",
            crate::lang::Language::Go => "packages",
            crate::lang::Language::Python => "packages",
            _ => "packages",
        };
        ir.nodes.retain(|n| !members.contains(&n.id));
        ir.nodes.push(Node {
            id: group_id.clone(),
            container_id: Some("libraries".into()),
            label: truncate(
                &if matches!(last, "crates" | "packages" | "libs" | "pkg" | "internal") {
                    format!("Core {unit_word} ({})", members.len())
                } else {
                    format!("{} ({} {unit_word})", display_name(last), members.len())
                },
                32,
            ),
            subtitle: Some(format!("{parent}/*")),
            tech_stack: Some(lang.display().to_string()),
            is_key_focal_point: false,
            evidence: None,
            metadata: meta(&[("members", Some(members.join(", ")))]),
            attributes: None,
            state_kind: None,
        });
        notes.push(format!("grouped {} libraries under `{parent}/` into `{group_id}`", members.len()));
        for m in members {
            rename.insert(m, group_id.clone());
        }
    }
    if rename.is_empty() {
        return;
    }
    for e in &mut ir.edges {
        if let Some(g) = rename.get(&e.source) {
            e.source = g.clone();
        }
        if let Some(g) = rename.get(&e.target) {
            e.target = g.clone();
        }
    }
    ir.edges.retain(|e| e.source != e.target);
    dedupe_edges(ir);
}

/// `a uses c` is implied when `a uses b` and `b uses c`; drawing it adds ink,
/// not information. Edges are removed one at a time against the current
/// graph, so a cycle between libraries never erases every path through it.
fn reduce_transitive_uses(ir: &mut DiagramIR, notes: &mut Vec<String>) {
    let is_uses = |e: &Edge| e.label.as_deref() == Some("uses");
    let mut ids: Vec<String> = ir.edges.iter().filter(|e| is_uses(e)).map(|e| e.id.clone()).collect();
    ids.sort();
    let mut removed = 0;
    for id in ids {
        let Some(edge) = ir.edges.iter().find(|e| e.id == id).cloned() else { continue };
        let mut stack = vec![edge.source.clone()];
        let mut seen = BTreeSet::new();
        let mut reachable = false;
        while let Some(n) = stack.pop() {
            for e in ir.edges.iter().filter(|e| is_uses(e) && e.source == n && e.id != id) {
                if e.target == edge.target {
                    reachable = true;
                    break;
                }
                if seen.insert(e.target.clone()) {
                    stack.push(e.target.clone());
                }
            }
            if reachable {
                break;
            }
        }
        if reachable {
            ir.edges.retain(|e| e.id != id);
            removed += 1;
        }
    }
    if removed > 0 {
        notes.push(format!("dropped {removed} transitive `uses` edge(s) implied by other links"));
    }
}

fn system_context(report: &ScanReport, ir: &mut DiagramIR, notes: &mut Vec<String>) {
    ir.diagram_type = DiagramType::SystemContext;
    ir.title = format!("{} — system context", report.system.name);
    ir.containers = vec![
        boundary("clients", "Clients", BoundaryType::Client, "How people reach the system"),
        boundary("system", &report.system.name, BoundaryType::TrustZone, "Everything built from this repository"),
        boundary(
            "external",
            "Third-party services",
            BoundaryType::ThirdParty,
            "Vendor APIs outside the trust boundary",
        ),
    ];
    let system_id = crate::scan::slug(&report.repo.name);
    let system_id = if system_id.is_empty() { "system".to_string() } else { system_id };
    let clients: Vec<&UnitSummary> = report.containers.iter().filter(|u| u.kind == UnitKind::WebClient).collect();
    let internal: Vec<&UnitSummary> = report.containers.iter().filter(|u| u.kind != UnitKind::WebClient).collect();
    let anchor = internal
        .iter()
        .max_by_key(|u| {
            let degree = report.relationships.iter().filter(|r| r.source == u.id || r.target == u.id).count();
            (degree, u.lines)
        })
        .copied();
    let stack: BTreeSet<&str> = internal.iter().map(|u| u.language.display()).collect();
    ir.nodes.push(Node {
        id: system_id.clone(),
        container_id: Some("system".into()),
        label: truncate(&report.system.name, 28),
        subtitle: Some(format!("{} deployable units", internal.len())),
        tech_stack: Some(stack.into_iter().collect::<Vec<_>>().join(" · ")),
        is_key_focal_point: true,
        evidence: anchor.and_then(|u| report.evidence_map.get(&u.id)).map(|e| e.to_ir()),
        metadata: meta(&[
            ("docstring", report.system.description.clone()),
            ("units", Some(internal.iter().map(|u| u.id.as_str()).collect::<Vec<_>>().join(", "))),
        ]),
        attributes: None,
        state_kind: None,
    });
    let internal_ids: BTreeSet<&str> = internal
        .iter()
        .map(|u| u.id.as_str())
        .chain(report.infrastructure.iter().filter(|i| i.category != InfraCategory::ThirdParty).map(|i| i.id.as_str()))
        .collect();
    for c in &clients {
        ir.nodes.push(unit_node(c, report, "clients"));
        ir.edges.push(Edge {
            id: format!("{}--{}", c.id, system_id),
            source: c.id.clone(),
            target: system_id.clone(),
            label: Some("uses".into()),
            edge_type: EdgeType::Sync,
            style: None,
            is_primary_path: Some(true),
            sequence: None,
            reply: None,
            payload: None,
            cardinality: None,
            guard: None,
            evidence: None,
        });
    }
    for i in report.infrastructure.iter().filter(|i| i.category == InfraCategory::ThirdParty) {
        ir.nodes.push(infra_node(i, report, "external"));
        let label = report
            .relationships
            .iter()
            .find(|r| r.target == i.id && internal_ids.contains(r.source.as_str()))
            .map(|r| r.label.clone());
        ir.edges.push(Edge {
            id: format!("{}--{}", system_id, i.id),
            source: system_id.clone(),
            target: i.id.clone(),
            label,
            edge_type: EdgeType::Sync,
            style: None,
            is_primary_path: None,
            sequence: None,
            reply: None,
            payload: None,
            cardinality: None,
            guard: None,
            evidence: None,
        });
    }
    if clients.is_empty() {
        notes.push("no web client detected; the system context shows outbound dependencies only".into());
    }
    if let Some(first) = ir.edges.iter_mut().find(|e| e.source == system_id) {
        if !clients.is_empty() {
            first.is_primary_path = Some(true);
        }
    }
}

/// Component-level draft for one unit's view (from `ScanReport::component_views`).
pub fn draft_component_ir(report: &ScanReport, view: &ComponentView, opts: &DraftOptions) -> Draft {
    let mut single = report.clone();
    single.depth = Depth::Component;
    single.components = Some(view.clone());
    draft_ir(&single, opts)
}

fn components(report: &ScanReport, view: Option<&ComponentView>, ir: &mut DiagramIR, notes: &mut Vec<String>) {
    ir.diagram_type = DiagramType::Component;
    let Some(view) = view else {
        ir.title = format!("{} — components", report.system.name);
        notes.push("no unit to decompose".into());
        return;
    };
    let unit = report.containers.iter().find(|u| u.id == view.unit).expect("component view refers to a scanned unit");
    ir.title = format!("{} — components", unit.name);
    ir.subtitle = unit.description.clone().or_else(|| Some(format!("{} · {}", unit.kind.role(), unit.tech_stack)));
    ir.containers = vec![
        boundary(
            &unit.id,
            &unit.name,
            BoundaryType::InternalService,
            &format!("{} ({})", unit.kind.role(), unit.tech_stack),
        ),
        boundary("data", "Data & messaging", BoundaryType::Storage, "Stateful infrastructure"),
        boundary(
            "external",
            "Third-party services",
            BoundaryType::ThirdParty,
            "Vendor APIs outside the trust boundary",
        ),
    ];
    for m in &view.modules {
        ir.nodes.push(Node {
            id: m.id.clone(),
            container_id: Some(unit.id.clone()),
            label: truncate(&m.name, 28),
            subtitle: m.doc.as_deref().map(|d| truncate(&first_sentence(d), 44)),
            tech_stack: Some(if m.files.len() == 1 {
                m.path.clone()
            } else {
                format!("{} · {} files", m.path, m.files.len())
            }),
            is_key_focal_point: false,
            evidence: m.evidence.as_ref().map(|e| e.to_ir()),
            metadata: meta(&[
                ("docstring", m.doc.clone()),
                ("files", Some(m.files.join("\n"))),
                ("symbols", Some(m.symbols.to_string())),
                ("entryPoint", m.is_entry.then(|| "yes".to_string())),
            ]),
            attributes: None,
            state_kind: None,
        });
    }
    for d in &view.dependencies {
        let event = !d.events.is_empty();
        ir.edges.push(Edge {
            id: format!("{}--{}", d.source, d.target),
            source: d.source.clone(),
            target: d.target.clone(),
            label: event.then(|| truncate(&d.events.join(", "), 32)),
            edge_type: if event { EdgeType::Event } else { EdgeType::Sync },
            style: None,
            is_primary_path: None,
            sequence: None,
            reply: None,
            payload: None,
            cardinality: None,
            guard: None,
            evidence: None,
        });
    }
    label_import_cycles(ir);
    let used: BTreeSet<_> = view.infra_usage.iter().map(|(_, k, _)| *k).collect();
    for i in report.infrastructure.iter().filter(|i| used.contains(&i.kind)) {
        ir.nodes.push(infra_node(i, report, infra_container(i)));
    }
    for (module, kind, _) in &view.infra_usage {
        let rel = report
            .relationships
            .iter()
            .find(|r| (r.source == unit.id && r.target == kind.id()) || (r.target == unit.id && r.source == kind.id()));
        let (source, target, edge_type, label) = match rel {
            Some(r) if r.target == unit.id => (kind.id().to_string(), module.clone(), r.edge_type, r.label.clone()),
            Some(r) => (module.clone(), kind.id().to_string(), r.edge_type, r.label.clone()),
            None => (module.clone(), kind.id().to_string(), EdgeType::Sync, String::new()),
        };
        ir.edges.push(Edge {
            id: format!("{source}--{target}"),
            source,
            target,
            label: (!label.is_empty()).then(|| truncate(&label, 28)),
            edge_type,
            style: None,
            is_primary_path: None,
            sequence: None,
            reply: None,
            payload: None,
            cardinality: None,
            guard: None,
            evidence: None,
        });
    }
    let out_degree = |id: &str| ir.edges.iter().filter(|e| e.source == id).count();
    let entry = view
        .modules
        .iter()
        .filter(|m| m.is_entry && out_degree(&m.id) > 0)
        .max_by_key(|m| (out_degree(&m.id), std::cmp::Reverse(m.id.clone())))
        .map(|m| m.id.clone());
    let focal = pick_focal(ir, |n| n.container_id.as_deref() == Some(unit.id.as_str()));
    let start = entry.or(focal.clone());
    if let Some(f) = &focal {
        mark_primary_from(ir, start.as_deref().unwrap_or(f));
    }
}

/// Highest-degree node satisfying `eligible` becomes the single focal point.
fn pick_focal(ir: &mut DiagramIR, eligible: impl Fn(&Node) -> bool) -> Option<String> {
    let degree = degrees(ir);
    let id = ir
        .nodes
        .iter()
        .filter(|n| eligible(n))
        .max_by_key(|n| (degree.get(n.id.as_str()).copied().unwrap_or(0), std::cmp::Reverse(n.id.clone())))
        .map(|n| n.id.clone())?;
    if let Some(n) = ir.nodes.iter_mut().find(|n| n.id == id) {
        n.is_key_focal_point = true;
    }
    Some(id)
}

fn degrees(ir: &DiagramIR) -> HashMap<&str, usize> {
    let mut d: HashMap<&str, usize> = HashMap::new();
    for e in &ir.edges {
        *d.entry(e.source.as_str()).or_default() += 1;
        *d.entry(e.target.as_str()).or_default() += 1;
    }
    d
}

/// Client → focal (shortest path), then onward from the focal node.
fn mark_primary_path(ir: &mut DiagramIR, focal: Option<&str>, client_containers: &[&str]) {
    let Some(focal) = focal else { return };
    let clients: Vec<String> = ir
        .nodes
        .iter()
        .filter(|n| n.container_id.as_deref().is_some_and(|c| client_containers.contains(&c)))
        .map(|n| n.id.clone())
        .collect();
    let mut best: Option<Vec<usize>> = None;
    for c in &clients {
        if let Some(path) = shortest_path(ir, c, focal) {
            if best.as_ref().is_none_or(|b| path.len() < b.len()) {
                best = Some(path);
            }
        }
    }
    for i in best.unwrap_or_default() {
        ir.edges[i].is_primary_path = Some(true);
    }
    mark_primary_from(ir, focal);
}

fn shortest_path(ir: &DiagramIR, from: &str, to: &str) -> Option<Vec<usize>> {
    let mut prev: HashMap<String, usize> = HashMap::new();
    let mut q = VecDeque::from([from.to_string()]);
    let mut seen = BTreeSet::from([from.to_string()]);
    while let Some(cur) = q.pop_front() {
        if cur == to {
            let mut path = Vec::new();
            let mut at = to.to_string();
            while let Some(&ei) = prev.get(&at) {
                path.push(ei);
                at = ir.edges[ei].source.clone();
            }
            path.reverse();
            return Some(path);
        }
        for (i, e) in ir.edges.iter().enumerate().filter(|(_, e)| e.source == cur) {
            if seen.insert(e.target.clone()) {
                prev.insert(e.target.clone(), i);
                q.push_back(e.target.clone());
            }
        }
    }
    None
}

/// Greedy walk along synchronous calls: services first, then vendors, then writes.
fn mark_primary_from(ir: &mut DiagramIR, start: &str) {
    let mut cur = start.to_string();
    let mut visited = BTreeSet::from([cur.clone()]);
    for _ in 0..4 {
        let container_of: HashMap<&str, Option<&str>> =
            ir.nodes.iter().map(|n| (n.id.as_str(), n.container_id.as_deref())).collect();
        let next = ir
            .edges
            .iter()
            .enumerate()
            .filter(|(_, e)| e.source == cur && !visited.contains(&e.target))
            // Linking a library isn't a step in a transaction.
            .filter(|(_, e)| {
                !matches!(container_of.get(e.target.as_str()).copied().flatten(), Some("libraries" | "operations"))
            })
            .filter(|(_, e)| matches!(e.edge_type, EdgeType::Sync | EdgeType::Write))
            .min_by_key(|(_, e)| {
                let is_infra =
                    |id: &str| matches!(container_of.get(id).copied().flatten(), Some("external") | Some("data"));
                let reaches_infra = ir.edges.iter().any(|x| x.source == e.target && is_infra(&x.target));
                let rank = match container_of.get(e.target.as_str()).copied().flatten() {
                    Some("platform") => 0,
                    Some("external") => 1,
                    Some("data") => 4,
                    _ if reaches_infra => 2,
                    _ => 3,
                };
                let outgoing = ir.edges.iter().filter(|x| x.source == e.target).count();
                (rank, std::cmp::Reverse(outgoing), e.target.clone())
            })
            .map(|(i, e)| (i, e.target.clone()));
        let Some((i, target)) = next else { break };
        ir.edges[i].is_primary_path = Some(true);
        visited.insert(target.clone());
        let is_terminal =
            matches!(container_of.get(target.as_str()).copied().flatten(), Some("external") | Some("data"));
        cur = target;
        if is_terminal {
            break;
        }
    }
}

fn drop_orphans(ir: &mut DiagramIR, notes: &mut Vec<String>) {
    if ir.nodes.len() <= 1 {
        return;
    }
    let connected: BTreeSet<String> = ir.edges.iter().flat_map(|e| [e.source.clone(), e.target.clone()]).collect();
    let before: Vec<String> = ir.nodes.iter().map(|n| n.id.clone()).collect();
    ir.nodes.retain(|n| connected.contains(&n.id) || n.is_key_focal_point);
    for id in before.iter().filter(|id| !ir.nodes.iter().any(|n| &n.id == *id)) {
        notes.push(format!("dropped `{id}`: no detected relationships"));
    }
}

fn drop_unused_containers(ir: &mut DiagramIR) {
    let used: BTreeSet<&str> = ir.nodes.iter().filter_map(|n| n.container_id.as_deref()).collect();
    let keep: Vec<Container> = ir.containers.iter().filter(|c| used.contains(c.id.as_str())).cloned().collect();
    ir.containers = keep;
}

/// Deterministic density reduction mirroring the validator's suggestions.
fn compact(ir: &mut DiagramIR, notes: &mut Vec<String>) {
    let budget = element_budget();
    let over = |ir: &DiagramIR| ir.nodes.len() + ir.edges.len() > budget;
    if !over(ir) {
        return;
    }
    // 1. Collapse vendors into one node.
    let vendors: Vec<String> =
        ir.nodes.iter().filter(|n| n.container_id.as_deref() == Some("external")).map(|n| n.id.clone()).collect();
    if vendors.len() > 1 {
        let merged = "third-party-apis".to_string();
        let labels: Vec<String> = vendors.iter().filter_map(|v| ir.node(v).map(|n| n.label.clone())).collect();
        ir.nodes.retain(|n| !vendors.contains(&n.id));
        ir.nodes.push(Node {
            id: merged.clone(),
            container_id: Some("external".into()),
            label: "Third-party APIs".into(),
            subtitle: Some(truncate(&labels.join(", "), 44)),
            tech_stack: None,
            is_key_focal_point: false,
            evidence: None,
            metadata: meta(&[("merged", Some(vendors.join(", ")))]),
            attributes: None,
            state_kind: None,
        });
        for e in &mut ir.edges {
            if vendors.contains(&e.target) {
                e.target = merged.clone();
            }
            if vendors.contains(&e.source) {
                e.source = merged.clone();
            }
        }
        dedupe_edges(ir);
        notes.push(format!("merged vendors [{}] into `third-party-apis` to meet density", vendors.join(", ")));
    }
    // 2. Sibling deployables built from one directory (`transport/coap`,
    // `transport/http`, …) usually share a shape: draw them as one group.
    if over(ir) {
        let mut by_parent: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for n in ir.nodes.iter().filter(|n| n.container_id.as_deref() == Some("platform") && !n.is_key_focal_point) {
            let path = n.metadata.as_ref().and_then(|m| m.get("path")).cloned().unwrap_or_default();
            let trimmed = path.trim_end_matches('/');
            if let Some((parent, _)) = trimmed.rsplit_once('/') {
                by_parent.entry(parent.to_string()).or_default().push(n.id.clone());
            }
        }
        for (parent, members) in by_parent.into_iter().filter(|(_, m)| m.len() >= 3) {
            let group_id = format!("group-{}", crate::scan::slug(&parent));
            let last = parent.rsplit('/').next().unwrap_or(&parent);
            let labels: Vec<String> = members.iter().filter_map(|m| ir.node(m).map(|n| n.label.clone())).collect();
            let tech = members.first().and_then(|m| ir.node(m)).and_then(|n| n.tech_stack.clone());
            ir.nodes.retain(|n| !members.contains(&n.id));
            ir.nodes.push(Node {
                id: group_id.clone(),
                container_id: Some("platform".into()),
                label: truncate(&format!("{} ({} services)", display_name(last), members.len()), 32),
                subtitle: Some(truncate(&labels.join(", "), 44)),
                tech_stack: tech,
                is_key_focal_point: false,
                evidence: None,
                metadata: meta(&[("members", Some(members.join(", "))), ("path", Some(format!("{parent}/*")))]),
                attributes: None,
                state_kind: None,
            });
            for e in &mut ir.edges {
                if members.contains(&e.source) {
                    e.source = group_id.clone();
                }
                if members.contains(&e.target) {
                    e.target = group_id.clone();
                }
            }
            ir.edges.retain(|e| e.source != e.target);
            dedupe_edges(ir);
            notes.push(format!(
                "grouped {} services under `{parent}/` into `{group_id}` to meet density",
                members.len()
            ));
        }
    }
    // 3. Remove the least-connected nodes that aren't on the story's spine.
    while over(ir) {
        let degree = degrees(ir);
        let on_primary: BTreeSet<&str> =
            ir.edges.iter().filter(|e| e.primary()).flat_map(|e| [e.source.as_str(), e.target.as_str()]).collect();
        // Libraries go first, then infrastructure; deployable units last.
        // A store something writes to is a system of record: keep it with the deployables.
        let written: BTreeSet<&str> =
            ir.edges.iter().filter(|e| e.edge_type == EdgeType::Write).map(|e| e.target.as_str()).collect();
        let tier = |n: &Node| match n.container_id.as_deref() {
            Some("libraries") => 0,
            Some("data") if written.contains(n.id.as_str()) => 2,
            Some("data") | Some("external") => 1,
            _ => 2,
        };
        let victim = ir
            .nodes
            .iter()
            .filter(|n| !n.is_key_focal_point && !on_primary.contains(n.id.as_str()))
            .min_by_key(|n| (tier(n), degree.get(n.id.as_str()).copied().unwrap_or(0), n.id.clone()))
            .map(|n| n.id.clone());
        let Some(victim) = victim else { break };
        ir.nodes.retain(|n| n.id != victim);
        ir.edges.retain(|e| e.source != victim && e.target != victim);
        notes.push(format!("dropped low-connectivity node `{victim}` to meet density ≤ {MAX_VISUAL_DENSITY}"));
    }
    drop_orphans(ir, notes);
}

fn dedupe_edges(ir: &mut DiagramIR) {
    let mut seen: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut keep = Vec::new();
    for e in ir.edges.drain(..) {
        let key = (e.source.clone(), e.target.clone());
        match seen.get(&key) {
            Some(&i) => {
                let existing: &mut Edge = &mut keep[i];
                existing.is_primary_path = Some(existing.primary() || e.primary()).filter(|p| *p);
                if existing.label != e.label {
                    existing.label = None;
                }
            }
            None => {
                seen.insert(key, keep.len());
                let mut e = e;
                e.id = format!("{}--{}", e.source, e.target);
                keep.push(e);
            }
        }
    }
    ir.edges = keep;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentence_and_truncation() {
        assert_eq!(first_sentence("A small shop. Used as demo."), "A small shop");
        assert_eq!(truncate("abcdefgh", 5), "abcd…");
        assert_eq!(truncate("abc", 5), "abc");
    }
}
