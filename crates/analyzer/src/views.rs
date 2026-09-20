//! Purpose-built drafts from the behaviour model: a sequence diagram per traced
//! flow, an entity-relationship diagram of the persisted data, and a lifecycle
//! diagram per state machine. Same contract as `draft_ir`: a valid IR plus
//! notes on what was left out.

use std::collections::{BTreeMap, BTreeSet};

use nunki_ir::{
    now_rfc3339, visual_density, Attribute, Cardinality, DiagramIR, DiagramMetadata, DiagramType, Edge, EdgeType,
    IrVersion, KeyKind, Node, StateKind,
};

use crate::data::{DataModel, StateMachine};
use crate::draft::{Draft, DraftOptions};
use crate::scan::{slug, ScanReport};
use crate::trace::{Flow, StepKind};

const LABEL_MAX: usize = 32;
/// Entities drawn in one ER figure (each carries its relations as edges).
pub const MAX_ENTITIES: usize = 12;

fn base(report: &ScanReport, opts: &DraftOptions, diagram_type: DiagramType, title: String) -> DiagramIR {
    DiagramIR {
        version: IrVersion::V1_1_0,
        diagram_type,
        title,
        subtitle: None,
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
    }
}

fn short(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

fn node(id: String, label: &str) -> Node {
    Node {
        id,
        container_id: None,
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

fn edge(id: String, source: String, target: String, label: Option<String>, edge_type: EdgeType) -> Edge {
    Edge {
        id,
        source,
        target,
        label: label.map(|l| short(&l, LABEL_MAX)),
        edge_type,
        style: None,
        is_primary_path: None,
        sequence: None,
        reply: None,
        payload: None,
        cardinality: None,
        guard: None,
        evidence: None,
    }
}

fn finish(mut ir: DiagramIR, notes: Vec<String>) -> Draft {
    ir.metadata.visual_density_score = Some(visual_density(ir.nodes.len(), ir.edges.len()));
    Draft { ir, notes }
}

/// Sequence diagram of one traced flow. The operation's own service is the
/// focal participant; the request and its reply form the primary path.
pub fn draft_sequence_ir(report: &ScanReport, flow: &Flow, opts: &DraftOptions) -> Draft {
    let entry_unit = flow.steps.first().map(|s| s.to.clone());
    let mut ir = base(report, opts, DiagramType::Sequence, flow.title.clone());
    let op = report.api.as_ref().and_then(|a| a.operations.iter().find(|o| o.id == flow.entry));
    ir.subtitle = op.and_then(|o| o.summary.clone());
    for p in &flow.participants {
        let mut n = node(p.id.clone(), &p.label);
        n.is_key_focal_point = Some(&p.id) == entry_unit.as_ref();
        n.tech_stack = (p.kind == "unit")
            .then(|| report.containers.iter().find(|u| u.id == p.id).map(|u| short(&u.tech_stack, 40)))
            .flatten();
        n.evidence = report.evidence_map.get(&p.id).map(|e| e.to_ir());
        ir.nodes.push(n);
    }
    // The synchronous story is the primary path: the trigger and the last step
    // before any asynchronous continuation (the reply, for requests).
    let last = flow.steps.iter().rposition(|s| !s.asynchronous).unwrap_or(0);
    for (i, s) in flow.steps.iter().enumerate() {
        let edge_type = match s.kind {
            StepKind::Call | StepKind::Reply | StepKind::Trigger => EdgeType::Sync,
            StepKind::Deliver => EdgeType::Event,
            StepKind::Read => EdgeType::Read,
            StepKind::Write => EdgeType::Write,
            StepKind::Publish => EdgeType::Event,
        };
        let mut e = edge(format!("m{}", i + 1), s.from.clone(), s.to.clone(), Some(s.label.clone()), edge_type);
        e.sequence = Some(i as u32 + 1);
        e.reply = (s.kind == StepKind::Reply).then_some(true);
        e.payload = s.payload.as_ref().map(|p| short(p, 48));
        e.is_primary_path = (i == 0 || i == last).then_some(true);
        // Work done later by another handler reads as dashed; deliveries keep the event dots.
        if s.asynchronous && !matches!(s.kind, StepKind::Deliver | StepKind::Publish) {
            e.style = Some(nunki_ir::EdgeStyle::Dashed);
        }
        e.evidence = Some(s.evidence.to_ir());
        ir.edges.push(e);
    }
    finish(ir, flow.notes.clone())
}

/// Entity-relationship diagram. Above `MAX_ENTITIES`, keeps the most related
/// entities.
pub fn draft_entity_ir(report: &ScanReport, data: &DataModel, opts: &DraftOptions) -> Option<Draft> {
    if data.entities.is_empty() {
        return None;
    }
    let mut notes = Vec::new();
    let degree = |id: &str| {
        data.entities
            .iter()
            .map(|e| {
                e.relations.iter().filter(|r| r.target == id).count() + if e.id == id { e.relations.len() } else { 0 }
            })
            .sum::<usize>()
    };
    let mut ranked: Vec<&crate::data::Entity> = data.entities.iter().collect();
    ranked.sort_by(|a, b| {
        (degree(&b.id), b.writes.len() + b.reads.len())
            .cmp(&(degree(&a.id), a.writes.len() + a.reads.len()))
            .then(a.id.cmp(&b.id))
    });
    if ranked.len() > MAX_ENTITIES {
        notes.push(format!(
            "{} of {} entities drawn (most related); the data page lists all of them",
            MAX_ENTITIES,
            ranked.len()
        ));
        ranked.truncate(MAX_ENTITIES);
    }
    let kept: BTreeSet<&str> = ranked.iter().map(|e| e.id.as_str()).collect();
    let node_id = |id: &str| slug(id.trim_start_matches("entity:")).replace('-', "_");

    // No grouping by service: tables usually share one database, and which
    // service "owns" a table is not something declarations state.
    let mut ir = base(report, opts, DiagramType::EntityRelationship, "Data model".into());
    // Keep the original entity order for a stable layout.
    for e in data.entities.iter().filter(|e| kept.contains(e.id.as_str())) {
        let mut n = node(node_id(&e.id), &e.table);
        if !e.name.eq_ignore_ascii_case(&e.table) {
            n.subtitle = Some(short(&e.name, 40));
        }
        n.evidence = Some(e.evidence.to_ir());
        n.attributes = Some(
            e.columns
                .iter()
                .map(|c| Attribute {
                    name: c.name.clone(),
                    type_name: short(&c.type_name, 24),
                    key: match (c.primary_key, c.references.is_some()) {
                        (true, true) => Some(KeyKind::PkFk),
                        (true, false) => Some(KeyKind::Pk),
                        (false, true) => Some(KeyKind::Fk),
                        _ => None,
                    },
                    nullable: c.nullable,
                    note: c.references.as_ref().map(|r| format!("→ {r}")),
                })
                .collect(),
        );
        ir.nodes.push(n);
    }
    if let Some(top) = ranked.first().filter(|e| degree(&e.id) > 0) {
        let id = node_id(&top.id);
        ir.nodes.iter_mut().filter(|n| n.id == id).for_each(|n| n.is_key_focal_point = true);
    }
    let mut pairs = BTreeSet::new();
    for e in data.entities.iter().filter(|e| kept.contains(e.id.as_str())) {
        for r in e.relations.iter().filter(|r| kept.contains(r.target.as_str()) && r.target != e.id) {
            // Draw parent → child so "1:n" reads left to right.
            let (s, t, card) = match r.kind.as_str() {
                "many-to-one" => (&r.target, &e.id, Cardinality::OneToMany),
                "one-to-many" => (&e.id, &r.target, Cardinality::OneToMany),
                "one-to-one" => (&e.id, &r.target, Cardinality::OneToOne),
                _ => (&e.id, &r.target, Cardinality::ManyToMany),
            };
            let key = (s.clone().min(t.clone()), s.clone().max(t.clone()));
            if !pairs.insert(key) {
                continue;
            }
            let mut ed = edge(
                format!("rel-{}", ir.edges.len() + 1),
                node_id(s),
                node_id(t),
                Some(r.via.clone()).filter(|v| !v.is_empty()),
                EdgeType::Sync,
            );
            ed.cardinality = Some(card);
            ed.evidence = Some(r.evidence.to_ir());
            ir.edges.push(ed);
        }
    }
    let budget = nunki_ir::element_budget();
    if ir.nodes.len() + ir.edges.len() > budget {
        let drop = ir.nodes.len() + ir.edges.len() - budget;
        notes.push(format!("{drop} relationship(s) left off the figure to stay within the density budget"));
        ir.edges.truncate(ir.edges.len().saturating_sub(drop));
    }
    Some(finish(ir, notes))
}

/// Triggers, first guard and first evidence of transitions between one pair of states.
type Merged = (Vec<String>, Option<String>, crate::scan::EvidenceRef);

/// Lifecycle diagram of one state machine. Transitions the code performs from
/// any state are drawn from an explicit "any state" node rather than guessed.
pub fn draft_lifecycle_ir(report: &ScanReport, sm: &StateMachine, opts: &DraftOptions) -> Draft {
    let mut notes = Vec::new();
    let mut ir = base(report, opts, DiagramType::Lifecycle, short(&format!("{} lifecycle", sm.subject), 80));
    let state_id = |name: &str| {
        let s = slug(name);
        if s.is_empty() {
            "state".to_string()
        } else {
            format!("s-{s}")
        }
    };
    for st in &sm.states {
        let mut n = node(state_id(&st.name), &st.name);
        n.state_kind = Some(if st.initial {
            StateKind::Initial
        } else if st.terminal {
            StateKind::Terminal
        } else {
            StateKind::Normal
        });
        n.evidence = Some(st.evidence.to_ir());
        ir.nodes.push(n);
    }
    // Merge transitions between the same pair; the first site is the evidence.
    let mut merged: BTreeMap<(String, String), Merged> = BTreeMap::new();
    let mut order = Vec::new();
    for t in &sm.transitions {
        let from = t.from.as_deref().map(state_id).unwrap_or_else(|| "any-state".into());
        let to = state_id(&t.to);
        if !ir.nodes.iter().any(|n| n.id == to) {
            notes.push(format!("transition to unknown state `{}` skipped", t.to));
            continue;
        }
        let key = (from, to);
        let entry = merged.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            (vec![], t.guard.clone(), t.evidence.clone())
        });
        if let Some(trigger) = &t.trigger {
            if !entry.0.contains(trigger) {
                entry.0.push(trigger.clone());
            }
        }
    }
    if order.iter().any(|(f, _)| f == "any-state") {
        ir.nodes.insert(0, node("any-state".into(), "any state"));
    }
    for (i, key) in order.iter().enumerate() {
        let (triggers, guard, evidence) = &merged[key];
        let label = match triggers.len() {
            0 => None,
            1 => Some(triggers[0].clone()),
            n => Some(format!("{} +{}", triggers[0], n - 1)),
        };
        let mut e = edge(format!("t{}", i + 1), key.0.clone(), key.1.clone(), label, EdgeType::Sync);
        e.guard = guard.as_ref().map(|g| short(g, 40));
        e.evidence = Some(evidence.to_ir());
        ir.edges.push(e);
    }
    // A terminal state the code transitions out of is not terminal.
    let sources: BTreeSet<String> = ir.edges.iter().map(|e| e.source.clone()).collect();
    for n in ir.nodes.iter_mut() {
        if n.state_kind == Some(StateKind::Terminal) && sources.contains(&n.id) {
            n.state_kind = Some(StateKind::Normal);
            notes.push(format!("`{}` has outgoing transitions; not drawn as terminal", n.label));
        }
    }
    finish(ir, notes)
}
