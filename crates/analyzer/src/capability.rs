//! Capability groups: the business areas a service's API is organised into
//! (a controller, a router file, an OpenAPI tag), and the capability map that
//! shows who uses each area and what each area touches.

use std::collections::{BTreeMap, BTreeSet};

use autodoc_ir::{
    now_rfc3339, visual_density, BoundaryType, Container, DiagramIR, DiagramMetadata, DiagramType, Edge, EdgeType,
    IrVersion, Node,
};

use crate::api::{ApiModel, Operation};
use crate::draft::{Draft, DraftOptions};
use crate::lang::Language;
use crate::scan::ScanReport;
use crate::source::SourceIndex;
use crate::trace::StepKind;

const LABEL_MAX: usize = 32;

/// File stems that name a technical role, not a business area.
const GENERIC_STEMS: &[&str] = &[
    "index",
    "main",
    "app",
    "server",
    "router",
    "routes",
    "route",
    "api",
    "apis",
    "handler",
    "handlers",
    "controller",
    "controllers",
    "views",
    "urls",
    "mod",
    "lib",
    "http",
    "httpapi",
    "web",
    "endpoints",
    "resources",
    "service",
    "rest",
];

const ACRONYMS: &[&str] = &[
    "api", "http", "id", "url", "oauth", "jwt", "2fa", "rpc", "ota", "qr", "mfa", "sms", "ai", "ui", "ldap", "saml",
    "sso", "json", "csv", "io",
];

/// Sets `Operation.group` for every operation (deterministic; see `group_for`).
pub(crate) fn assign_groups(api: &mut ApiModel, index: &SourceIndex) {
    let mut tags: BTreeMap<String, Option<String>> = BTreeMap::new();
    for op in api.operations.iter_mut() {
        let path = op.handler.evidence.file_path.clone();
        let python = index.files.iter().any(|f| f.path == path && f.language == Language::Python);
        let tag = if python {
            tags.entry(path.clone()).or_insert_with(|| index.read(&path).as_deref().and_then(fastapi_tag)).clone()
        } else {
            None
        };
        op.group = Some(tag.unwrap_or_else(|| group_for(op)));
    }
}

/// The group an operation belongs to: its controller / resource class, its
/// router module, or — when the file name says nothing — the first
/// meaningful path segment (`/api/v1/items/{id}` → `Items`).
pub fn group_for(op: &Operation) -> String {
    let file = op.handler.evidence.file_path.rsplit('/').next().unwrap_or("");
    let mut stem = file.split('.').next().unwrap_or("").to_string();
    // `orders.controller.ts`, `users.routes.ts`: the first dotted part is the area.
    for suffix in
        ["Controller", "Resource", "Endpoint", "Endpoints", "Handler", "Handlers", "Api", "Rest", "Router", "Routes"]
    {
        if stem.len() > suffix.len() && stem.ends_with(suffix) {
            stem.truncate(stem.len() - suffix.len());
            break;
        }
    }
    let lower = stem.to_lowercase();
    if !stem.is_empty() && !GENERIC_STEMS.contains(&lower.as_str()) {
        return humanize(&stem);
    }
    path_group(&op.path).unwrap_or_else(|| "General".to_string())
}

fn path_group(path: &str) -> Option<String> {
    path.split('/')
        .filter(|s| !s.is_empty() && !s.starts_with('{') && !s.starts_with(':') && !s.starts_with('.'))
        .find(|s| {
            let l = s.to_lowercase();
            !(l == "api"
                || l == "rest"
                || l == "internal"
                || l == "public"
                || (l.starts_with('v') && l[1..].chars().all(|c| c.is_ascii_digit()) && l.len() > 1))
        })
        .map(humanize)
}

/// `TwoFactorAuthConfig` / `order_items` / `alarm-rule` → `Two Factor Auth Config` / `Order Items` / `Alarm Rule`.
pub fn humanize(s: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' || c == '-' || c == ' ' || c == '.' {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            continue;
        }
        // `OAuth2`: a lone capital before a capitalised word stays with it.
        let boundary = c.is_uppercase()
            && cur.chars().count() > usize::from(cur.chars().all(|x| x.is_uppercase()))
            && (cur.chars().last().is_some_and(|p| p.is_lowercase() || p.is_ascii_digit())
                || chars.get(i + 1).is_some_and(|n| n.is_lowercase()));
        if boundary {
            words.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
        .iter()
        .map(|w| {
            let l = w.to_lowercase();
            if ACRONYMS.contains(&l.as_str()) {
                l.to_uppercase()
            } else {
                // Mixed case (`OAuth2`) is kept; lowercase words are capitalised.
                let base = if *w == l { l.as_str() } else { w.as_str() };
                let mut c = base.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// First tag of an `APIRouter(..., tags=["Items"])` in a FastAPI module.
fn fastapi_tag(text: &str) -> Option<String> {
    let at = text.find("APIRouter(")?;
    let rest = &text[at..];
    let end = rest.find(')').unwrap_or(rest.len());
    let call = &rest[..end];
    let tags = call.find("tags")?;
    let after = &call[tags..];
    let open = after.find('[')?;
    let inner = &after[open + 1..];
    let q = inner.find(['"', '\''])?;
    let quote = inner[q..].chars().next()?;
    let body = &inner[q + 1..];
    let close = body.find(quote)?;
    let tag = body[..close].trim();
    (!tag.is_empty()).then(|| humanize(tag))
}

/// A service's capability groups in display order (most operations first,
/// then name), with their operations sorted by path and method.
pub fn groups_of<'a>(api: &'a ApiModel, unit: &str) -> Vec<(String, Vec<&'a Operation>)> {
    let mut by: BTreeMap<String, Vec<&Operation>> = BTreeMap::new();
    for op in api.operations.iter().filter(|o| o.unit == unit) {
        by.entry(op.group.clone().unwrap_or_else(|| group_for(op))).or_default().push(op);
    }
    let mut out: Vec<(String, Vec<&Operation>)> = by.into_iter().collect();
    for (_, ops) in out.iter_mut() {
        ops.sort_by(|a, b| (&a.path, &a.method).cmp(&(&b.path, &b.method)));
    }
    out.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    out
}

fn short(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

fn node(id: String, label: &str, container: &str) -> Node {
    Node {
        id,
        container_id: Some(container.into()),
        label: short(label, LABEL_MAX),
        subtitle: None,
        tech_stack: None,
        is_key_focal_point: false,
        evidence: None,
        metadata: None,
        attributes: None,
        state_kind: None,
    }
}

fn group_node_id(name: &str) -> String {
    let s = crate::scan::slug(name);
    format!("cap-{}", if s.is_empty() { "group".into() } else { s })
}

/// Verbs a group's operations use on one target, and the first step's evidence.
type Touch = (BTreeSet<&'static str>, Option<crate::scan::EvidenceRef>);

/// One group as drawn: possibly several source groups merged into "Other".
struct Area<'a> {
    name: String,
    ops: Vec<&'a Operation>,
}

/// Capability map of one service: callers → capability groups → what the
/// groups' operations read, write, publish and call. `None` when the service
/// has no operations.
/// Where a service's data access actually happens when its own operations
/// reach nothing traceable: the library units it links that read or write
/// stores, with those stores. Empty when the operations touch stores directly.
pub fn data_access_through_libraries(report: &ScanReport, unit: &str) -> Vec<(String, Vec<String>)> {
    let traced = report
        .flows
        .iter()
        .filter(|f| f.entry.starts_with(&format!("{unit}:")))
        .any(|f| f.steps.iter().any(|s| s.from == unit && matches!(s.kind, StepKind::Read | StepKind::Write)));
    if traced {
        return vec![];
    }
    let libraries: Vec<&str> = report
        .relationships
        .iter()
        .filter(|r| r.source == unit && r.label == "uses")
        .map(|r| r.target.as_str())
        .collect();
    let mut out = Vec::new();
    for lib in libraries {
        let stores: Vec<String> = report
            .relationships
            .iter()
            .filter(|r| r.source == lib)
            .filter_map(|r| report.infrastructure.iter().find(|i| i.id == r.target))
            .filter(|i| i.category != crate::catalog::InfraCategory::ThirdParty)
            .map(|i| i.label.clone())
            .collect();
        if !stores.is_empty() {
            let name = report
                .containers
                .iter()
                .find(|u| u.id == lib)
                .map(|u| u.name.clone())
                .unwrap_or_else(|| lib.to_string());
            let mut stores = stores;
            stores.sort();
            stores.dedup();
            out.push((name, stores));
        }
    }
    out.sort();
    out
}

pub fn draft_capability_ir(report: &ScanReport, unit: &str, opts: &DraftOptions) -> Option<Draft> {
    let api = report.api.as_ref()?;
    let groups = groups_of(api, unit);
    if groups.is_empty() {
        return None;
    }
    // Keep the largest groups; fold the rest into "Other (k)" until the figure fits the density budget.
    let budget = autodoc_ir::element_budget();
    let mut keep = groups.len();
    loop {
        let (ir, notes) = build(report, api, unit, &groups, keep, opts);
        if ir.nodes.len() + ir.edges.len() <= budget || keep <= 1 {
            let mut ir = ir;
            ir.metadata.visual_density_score = Some(visual_density(ir.nodes.len(), ir.edges.len()));
            return Some(Draft { ir, notes });
        }
        keep -= 1;
    }
}

fn build(
    report: &ScanReport,
    api: &ApiModel,
    unit: &str,
    groups: &[(String, Vec<&Operation>)],
    keep: usize,
    opts: &DraftOptions,
) -> (DiagramIR, Vec<String>) {
    let mut notes = Vec::new();
    let mut areas: Vec<Area> =
        groups.iter().take(keep).map(|(n, ops)| Area { name: n.clone(), ops: ops.clone() }).collect();
    if keep < groups.len() {
        let rest: Vec<&(String, Vec<&Operation>)> = groups.iter().skip(keep).collect();
        let ops: Vec<&Operation> = rest.iter().flat_map(|(_, o)| o.iter().copied()).collect();
        notes.push(format!(
            "{} smaller capability groups ({} operations) drawn as one `Other` node: {}",
            rest.len(),
            ops.len(),
            rest.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ")
        ));
        areas.push(Area { name: format!("Other ({} groups)", rest.len()), ops });
    }
    let unit_name =
        report.containers.iter().find(|u| u.id == unit).map(|u| u.name.clone()).unwrap_or_else(|| unit.to_string());
    let mut ir = DiagramIR {
        version: IrVersion::V1_1_0,
        diagram_type: DiagramType::Component,
        title: short(&format!("{unit_name} — capabilities"), 80),
        subtitle: Some(format!(
            "{} operations in {} capability groups: who calls each area and what it touches",
            groups.iter().map(|(_, o)| o.len()).sum::<usize>(),
            groups.len()
        )),
        theme: opts.theme,
        metadata: DiagramMetadata {
            target_repo: report.repo.root.clone(),
            commit_hash: report.repo.commit_hash.clone(),
            generated_at: opts.generated_at.clone().unwrap_or_else(now_rfc3339),
            visual_density_score: None,
        },
        containers: vec![
            Container {
                id: "callers".into(),
                label: "Callers".into(),
                boundary_type: BoundaryType::Client,
                role_description: Some("Code that calls this service's operations".into()),
            },
            Container {
                id: "service".into(),
                label: short(&unit_name, LABEL_MAX),
                boundary_type: BoundaryType::InternalService,
                role_description: Some("Capability groups of the service's API".into()),
            },
            Container {
                id: "dependencies".into(),
                label: "Reaches".into(),
                boundary_type: BoundaryType::Storage,
                role_description: Some("Stores, brokers and services the operations use".into()),
            },
        ],
        nodes: vec![],
        edges: vec![],
    };

    // Group nodes.
    let mut area_of_op: BTreeMap<&str, usize> = BTreeMap::new();
    for (ai, a) in areas.iter().enumerate() {
        for op in &a.ops {
            area_of_op.insert(op.id.as_str(), ai);
        }
        let mut methods: BTreeMap<&str, usize> = BTreeMap::new();
        for op in &a.ops {
            *methods.entry(op.method.as_str()).or_default() += 1;
        }
        let order = ["GET", "POST", "PUT", "PATCH", "DELETE"];
        let mut ms: Vec<(&str, usize)> = methods.into_iter().collect();
        ms.sort_by_key(|(m, _)| (order.iter().position(|o| o == m).unwrap_or(order.len()), m.to_string()));
        let mut n = node(group_node_id(&a.name), &a.name, "service");
        n.subtitle = Some(format!("{} operation{}", a.ops.len(), if a.ops.len() == 1 { "" } else { "s" }));
        n.tech_stack = Some(short(&ms.iter().map(|(m, c)| format!("{m} {c}")).collect::<Vec<_>>().join(" · "), 40));
        n.evidence = a.ops.first().map(|o| o.handler.evidence.to_ir());
        ir.nodes.push(n);
    }

    // Callers: in-repo units with client calls into these operations.
    let mut calls: BTreeMap<(String, usize), (usize, crate::scan::EvidenceRef)> = BTreeMap::new();
    for c in api.client_calls.iter().filter(|c| c.unit != unit) {
        let Some(ai) = c.operation.as_deref().and_then(|id| area_of_op.get(id)) else { continue };
        let e = calls.entry((c.unit.clone(), *ai)).or_insert((0, c.evidence.clone()));
        e.0 += 1;
    }
    let called_areas: BTreeSet<usize> = calls.keys().map(|(_, a)| *a).collect();
    let mut callers: BTreeSet<String> = BTreeSet::new();
    for ((caller, ai), (count, ev)) in &calls {
        if callers.insert(caller.clone()) {
            let label =
                report.containers.iter().find(|u| &u.id == caller).map(|u| u.name.clone()).unwrap_or(caller.clone());
            let mut n = node(caller.clone(), &label, "callers");
            n.subtitle = report.containers.iter().find(|u| &u.id == caller).map(|u| u.kind.role().to_string());
            n.evidence = report.evidence_map.get(caller).map(|e| e.to_ir());
            ir.nodes.push(n);
        }
        ir.edges.push(Edge {
            id: crate::scan::edge_id(caller, &group_node_id(&areas[*ai].name)),
            source: caller.clone(),
            target: group_node_id(&areas[*ai].name),
            label: Some(format!("{count} call{}", if *count == 1 { "" } else { "s" })),
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
    // Areas nobody in the repository calls are reached from outside it.
    let uncalled: Vec<usize> = (0..areas.len()).filter(|a| !called_areas.contains(a)).collect();
    if !uncalled.is_empty() {
        let label = if callers.is_empty() { "Clients" } else { "Other clients" };
        let mut n = node("external-clients".into(), label, "callers");
        n.subtitle = Some("callers outside this repository".into());
        ir.nodes.push(n);
        for ai in uncalled {
            ir.edges.push(Edge {
                id: crate::scan::edge_id("external-clients", &group_node_id(&areas[ai].name)),
                source: "external-clients".into(),
                target: group_node_id(&areas[ai].name),
                label: None,
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
    }

    // What each area's traced operations touch, one hop from this service.
    let mut touches: BTreeMap<(usize, String), Touch> = BTreeMap::new();
    for f in &report.flows {
        let Some(&ai) = area_of_op.get(f.entry.as_str()) else { continue };
        for s in f.steps.iter().skip(1).filter(|s| s.from == unit && s.to != unit) {
            let verb = match s.kind {
                StepKind::Read => "reads",
                StepKind::Write => "writes",
                StepKind::Publish => "publishes",
                StepKind::Call => "calls",
                StepKind::Reply | StepKind::Trigger | StepKind::Deliver => continue,
            };
            let e = touches.entry((ai, s.to.clone())).or_default();
            e.0.insert(verb);
            e.1.get_or_insert_with(|| s.evidence.clone());
        }
    }
    let mut targets: BTreeSet<String> = BTreeSet::new();
    for ((ai, target), (verbs, ev)) in &touches {
        if targets.insert(target.clone()) {
            let (label, role, ev_map) = match report.infrastructure.iter().find(|i| &i.id == target) {
                Some(i) => (i.label.clone(), i.role.clone(), report.evidence_map.get(target)),
                None => match report.containers.iter().find(|u| &u.id == target) {
                    Some(u) => (u.name.clone(), u.kind.role().to_string(), report.evidence_map.get(target)),
                    None => (target.clone(), String::new(), None),
                },
            };
            let mut n = node(target.clone(), &label, "dependencies");
            n.subtitle = (!role.is_empty()).then_some(role);
            n.evidence = ev_map.map(|e| e.to_ir());
            ir.nodes.push(n);
        }
        let order = ["reads", "writes", "publishes", "calls"];
        let mut vs: Vec<&str> = verbs.iter().copied().collect();
        vs.sort_by_key(|v| order.iter().position(|o| o == v));
        let edge_type = if vs == ["reads"] {
            EdgeType::Read
        } else if vs.contains(&"writes") {
            EdgeType::Write
        } else if vs == ["publishes"] {
            EdgeType::Event
        } else {
            EdgeType::Sync
        };
        ir.edges.push(Edge {
            id: crate::scan::edge_id(&group_node_id(&areas[*ai].name), target),
            source: group_node_id(&areas[*ai].name),
            target: target.clone(),
            label: Some(short(&vs.join(", "), LABEL_MAX)),
            edge_type,
            style: None,
            is_primary_path: None,
            sequence: None,
            reply: None,
            payload: None,
            cardinality: None,
            guard: None,
            evidence: ev.as_ref().map(|e| e.to_ir()),
        });
    }

    // Focal: the area most callers use, then the largest, then the one that reaches most.
    // A merged "Other" bucket is never the story.
    let merged = keep < groups.len();
    let focal = (0..areas.len())
        .filter(|&ai| !(merged && ai + 1 == areas.len() && areas.len() > 1))
        .max_by_key(|&ai| {
            let callers_in = calls.keys().filter(|(_, a)| *a == ai).count();
            let reach = touches.keys().filter(|(a, _)| *a == ai).count();
            (callers_in, areas[ai].ops.len(), reach, std::cmp::Reverse(areas[ai].name.clone()))
        })
        .map(|ai| group_node_id(&areas[ai].name));
    for n in ir.nodes.iter_mut() {
        n.is_key_focal_point = Some(&n.id) == focal.as_ref();
    }
    ir.containers.retain(|c| ir.nodes.iter().any(|n| n.container_id.as_deref() == Some(c.id.as_str())));
    (ir, notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_human_readable() {
        assert_eq!(humanize("TwoFactorAuthConfig"), "Two Factor Auth Config");
        assert_eq!(humanize("order_items"), "Order Items");
        assert_eq!(humanize("OAuth2ConfigTemplate"), "OAuth2 Config Template");
        assert_eq!(humanize("QrCodeSettings"), "QR Code Settings");
        assert_eq!(humanize("apiUsage"), "API Usage");
        assert_eq!(path_group("/api/v1/items/{id}").as_deref(), Some("Items"));
        assert_eq!(path_group("/{id}"), None);
        assert_eq!(fastapi_tag("router = APIRouter(prefix=\"/items\", tags=[\"items\"])\n").as_deref(), Some("Items"));
    }
}
