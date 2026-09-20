//! Pages and sections built from the behaviour model: request-flow sequence
//! diagrams, the data model and lifecycles, one API reference per service, the
//! functional specification (as built), the business rules catalog and the
//! business requirements scaffold.
//!
//! Measured and authored content never share a sentence: authored answers
//! carry an "authored" badge, missing ones an explicit "needs input" gap.

use std::collections::{BTreeMap, BTreeSet};

use nunki_analyzer::api::{ApiModel, Confidence, Model, Operation};
use nunki_analyzer::data::DataModel;
use nunki_analyzer::scan::{slug, EvidenceRef};
use nunki_analyzer::trace::{Flow, StepKind};
use nunki_analyzer::{draft_lifecycle_ir, DraftOptions};
use nunki_validator::ValidateOptions;

use super::Builder;
use crate::authored::{given, given_list};
use crate::model::*;

/// A business rule found in code, numbered for cross-reference.
pub(super) struct RuleEntry {
    pub id: String,
    pub statement: String,
    pub kind: &'static str,
    pub applies_to: Vec<Inline>,
    /// Operation ids the rule constrains.
    pub operations: BTreeSet<String>,
    pub evidence: EvidenceRef,
}

pub(super) fn op_anchor(op: &Operation) -> String {
    // Handlers can share a route and differ by request condition.
    let selector = op.selector.as_deref().map(|s| format!(" {s}")).unwrap_or_default();
    format!("op-{}", slug(&format!("{} {}{selector}", op.method, op.path)))
}

/// Requirements are split into per-service pages above this many.
const FR_SPLIT_OVER: usize = 150;
/// The business rules catalog gets its own page above this many rules.
const RULES_SPLIT_OVER: usize = 200;

pub(super) fn flow_figure_id(flow: &Flow) -> String {
    super::flows::flow_figure_id(flow)
}

pub(super) fn confidence_badge(c: Confidence) -> Inline {
    match c {
        Confidence::Typed => Inline::badge("observed", "typed"),
        Confidence::Partial => Inline::badge("declared", "partial"),
        Confidence::Opaque => Inline::badge("warn", "opaque"),
    }
}

fn gap(what: &str) -> Inline {
    Inline::badge("gap", format!("needs input: {what}"))
}

fn authored(text: &str) -> Vec<Inline> {
    vec![Inline::badge("authored", "authored"), Inline::text(format!(" {text}"))]
}

impl<'a> Builder<'a> {
    pub(super) fn api(&self) -> Option<&'a ApiModel> {
        self.report.api.as_ref().filter(|a| !a.operations.is_empty())
    }

    fn data(&self) -> Option<&'a DataModel> {
        self.report.data.as_ref()
    }

    pub(super) fn model(&self, id: &str) -> Option<&'a Model> {
        self.api().and_then(|a| a.models.iter().find(|m| m.id == id))
    }

    pub(super) fn flow_for(&self, op: &str) -> Option<&'a Flow> {
        self.report.flows.iter().find(|f| f.entry == op)
    }

    /// State machines of the domain: at least two states, and changed somewhere
    /// other than a web client (UI widget and animation states are not lifecycles).
    pub(super) fn business_machines(&self) -> Vec<&'a nunki_analyzer::data::StateMachine> {
        let Some(data) = self.data() else { return vec![] };
        data.state_machines
            .iter()
            .filter(|sm| sm.states.len() >= 2)
            .filter(|sm| {
                sm.transitions
                    .iter()
                    .any(|t| self.unit(&t.unit).is_none_or(|u| u.kind != nunki_analyzer::UnitKind::WebClient))
            })
            .collect()
    }

    /// Units with operations, in unit order.
    pub(super) fn api_units(&self) -> Vec<String> {
        let Some(api) = self.api() else { return vec![] };
        let with_ops: BTreeSet<&str> = api.operations.iter().map(|o| o.unit.as_str()).collect();
        self.report.containers.iter().filter(|u| with_ops.contains(u.id.as_str())).map(|u| u.id.clone()).collect()
    }

    // ── figures ──────────────────────────────────────────────────────────────

    pub(super) fn add_behavior_figures(
        &mut self,
        curated: &BTreeMap<String, String>,
        validate: &ValidateOptions,
        draft: &DraftOptions,
    ) {
        let report = self.report;
        self.add_flow_figures(curated, validate, draft);
        self.add_data_model_figures(curated, validate, draft);
        if report.data.is_some() {
            for sm in self.business_machines() {
                let id = format!("lifecycle-{}", slug(sm.id.trim_start_matches("state:")));
                self.add_figure(&id, draft_lifecycle_ir(report, sm, draft), curated, validate);
            }
        }
    }

    // ── flows page: request flows ────────────────────────────────────────────

    pub(super) fn request_flow_blocks(&mut self) -> Vec<Block> {
        self.flow_section_blocks()
    }

    // ── data page: model and lifecycles ──────────────────────────────────────

    pub(super) fn data_model_blocks(&mut self) -> Vec<Block> {
        self.data_model_blocks_impl()
    }

    // ── API reference ────────────────────────────────────────────────────────

    // ── functional specification ─────────────────────────────────────────────

    /// Rules found in code, numbered BR-001… in a stable order.
    pub(super) fn rule_catalog(&self) -> Vec<RuleEntry> {
        let mut out: Vec<RuleEntry> = Vec::new();
        let mut seen = BTreeSet::new();
        let mut push = |statement: String,
                        kind: &'static str,
                        applies_to: Vec<Inline>,
                        ops: BTreeSet<String>,
                        ev: &EvidenceRef,
                        key: String| {
            if seen.insert((key, statement.clone())) {
                out.push(RuleEntry {
                    id: String::new(),
                    statement,
                    kind,
                    applies_to,
                    operations: ops,
                    evidence: ev.clone(),
                });
            }
        };
        if let Some(api) = self.api() {
            let mut ops: Vec<&Operation> = api.operations.iter().collect();
            ops.sort_by(|a, b| (&a.unit, &a.path, &a.method).cmp(&(&b.unit, &b.path, &b.method)));
            // Which operations accept each model (directly or nested one level).
            let mut model_ops: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
            for op in &ops {
                if let Some(m) = op.request_body.as_ref().and_then(|t| t.model.as_deref()) {
                    model_ops.entry(m).or_default().insert(op.id.clone());
                    if let Some(model) = api.models.iter().find(|x| x.id == m) {
                        for f in model.fields.iter().filter_map(|f| f.model.as_deref()) {
                            model_ops.entry(f).or_default().insert(op.id.clone());
                        }
                    }
                }
            }
            for op in &ops {
                for p in &op.params {
                    for r in &p.rules {
                        push(
                            r.statement.clone(),
                            "validation",
                            vec![
                                Inline::code(format!("{} {}", op.method, op.path)),
                                Inline::text(format!(" · {} ", p.name)),
                            ],
                            [op.id.clone()].into(),
                            &r.evidence,
                            format!("{}#{}", op.id, p.name),
                        );
                    }
                }
                for a in &op.auth {
                    push(
                        format!("requires {}: {}", a.kind, a.detail),
                        "authorization",
                        vec![Inline::code(format!("{} {}", op.method, op.path))],
                        [op.id.clone()].into(),
                        &a.evidence,
                        op.id.clone(),
                    );
                }
            }
            let mut models: Vec<&Model> = api.models.iter().collect();
            models.sort_by(|a, b| (&a.unit, &a.name).cmp(&(&b.unit, &b.name)));
            for m in models {
                for f in &m.fields {
                    for r in &f.rules {
                        push(
                            r.statement.clone(),
                            "validation",
                            vec![Inline::code(format!("{}.{}", m.name, f.name))],
                            model_ops.get(m.id.as_str()).cloned().unwrap_or_default(),
                            &r.evidence,
                            format!("{}.{}", m.id, f.name),
                        );
                    }
                }
            }
        }
        if let Some(data) = self.data() {
            for e in &data.entities {
                for c in &e.columns {
                    for k in &c.constraints {
                        push(
                            k.clone(),
                            "data integrity",
                            vec![Inline::code(format!("{}.{}", e.table, c.name))],
                            BTreeSet::new(),
                            &c.evidence,
                            format!("{}.{}", e.id, c.name),
                        );
                    }
                }
            }
            for sm in &data.state_machines {
                for t in &sm.transitions {
                    let condition = match (&t.from, &t.guard) {
                        (Some(f), Some(g)) => Some(format!("from {f}, when {g}")),
                        (Some(f), None) => Some(format!("from {f}")),
                        (None, Some(g)) => Some(format!("when {g}")),
                        (None, None) => None,
                    };
                    if let Some(c) = condition {
                        push(
                            format!("{} may become {} only {}", sm.subject, t.to, c),
                            "state",
                            vec![Inline::code(sm.subject.clone())],
                            BTreeSet::new(),
                            &t.evidence,
                            format!("{}>{}", sm.id, t.to),
                        );
                    }
                }
            }
        }
        for (i, r) in out.iter_mut().enumerate() {
            r.id = format!("BR-{:03}", i + 1);
        }
        out
    }

    /// Operations in functional order (by service, then path), numbered FR-001….
    pub(super) fn functional_requirements(&self) -> Vec<(String, &'a Operation)> {
        let Some(api) = self.api() else { return vec![] };
        let mut ops: Vec<&Operation> = api.operations.iter().collect();
        ops.sort_by(|a, b| (&a.unit, &a.path, &a.method).cmp(&(&b.unit, &b.path, &b.method)));
        ops.into_iter().enumerate().map(|(i, o)| (format!("FR-{:03}", i + 1), o)).collect()
    }

    pub(super) fn functional_page(&mut self) {
        let frs = self.functional_requirements();
        if frs.is_empty() {
            return;
        }
        let rules = self.rule_catalog();
        let mut blocks = vec![Block::Callout {
            tone: "note".into(),
            title: "As built".into(),
            inl: vec![
                Inline::text("Each requirement below is derived from an operation the code serves and what its handler does. Who performs it and why cannot be read from code: those answers come from "),
                Inline::code(crate::authored::AUTHORED),
                Inline::text(" and are badged "),
                Inline::badge("authored", "authored"),
                Inline::text("; unanswered ones are marked "),
                gap("…"),
                Inline::text("."),
            ],
        }];
        let (split, rules_split) = self.functional_layout(frs.len(), rules.len());
        // Each requirement's blocks, with its service and capability group; assembled below.
        let mut per_fr: Vec<(String, String, Vec<Block>)> = Vec::new();
        for (fr, op) in &frs {
            let mut blocks: Vec<Block> = Vec::new();
            let group = self.api_index.op_group.get(&op.id).cloned().unwrap_or_default();
            let intent = self.authored.operation(&op.id).cloned().unwrap_or_default();
            let title = match given(&intent.name) {
                Some(n) => format!("{fr} · {n}"),
                None => format!("{fr} · {} {}", op.method, op.path),
            };
            blocks.push(Block::Heading { level: 3, id: fr.to_lowercase(), text: title });
            let mut items: Vec<Vec<Inline>> = Vec::new();

            let mut actor = vec![Inline::strong("Actor ")];
            match given(&intent.actor) {
                Some(a) => actor.extend(authored(a)),
                None => actor.push(gap("actor")),
            }
            let callers: Vec<_> = self
                .api()
                .map(|a| {
                    a.client_calls.iter().filter(|c| c.operation.as_deref() == Some(op.id.as_str())).collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if !callers.is_empty() {
                actor.push(Inline::text(" · called from "));
                let units: BTreeSet<&str> = callers.iter().map(|c| c.unit.as_str()).collect();
                for (i, u) in units.into_iter().enumerate() {
                    if i > 0 {
                        actor.push(Inline::text(", "));
                    }
                    actor.push(self.element(u));
                }
            }
            items.push(actor);

            let mut purpose = vec![Inline::strong("Purpose ")];
            match (given(&intent.purpose), &op.summary) {
                (Some(p), _) => purpose.extend(authored(p)),
                (None, Some(doc)) => {
                    purpose.push(Inline::text(format!("{doc} (handler documentation) ")));
                    purpose.push(self.cite(&op.handler.evidence));
                }
                (None, None) => purpose.push(gap("purpose")),
            }
            items.push(purpose);

            let mut trigger = vec![
                Inline::strong("Trigger "),
                self.op_link(op),
                Inline::text(" → "),
                Inline::code(op.handler.name.clone()),
            ];
            trigger.push(self.cite(&op.handler.evidence));
            items.push(trigger);
            if !group.is_empty() {
                items.push(vec![
                    Inline::strong("Capability "),
                    Inline::Link {
                        page: self.api_index.op_page.get(&op.id).cloned().unwrap_or_else(|| format!("api/{}", op.unit)),
                        anchor: None,
                        v: group.clone(),
                    },
                ]);
            }

            let mut accepts = vec![Inline::strong("Accepts ")];
            let required: Vec<String> = op.params.iter().filter(|p| p.required).map(|p| p.name.clone()).collect();
            let mut any = false;
            if let Some(t) = &op.request_body {
                accepts.push(self.type_inline(t));
                any = true;
                if let Some(m) = t.model.as_deref().and_then(|id| self.model(id)) {
                    let req: Vec<&str> = m.fields.iter().filter(|f| f.required).map(|f| f.name.as_str()).collect();
                    if !req.is_empty() {
                        accepts.push(Inline::text(format!(" with required {}", req.join(", "))));
                    }
                }
            }
            if !required.is_empty() {
                accepts.push(Inline::text(format!(
                    "{}parameters {}",
                    if any { "; " } else { "" },
                    required.join(", ")
                )));
                any = true;
            }
            if !any {
                accepts.push(Inline::text("no declared input"));
            }
            items.push(accepts);

            let applicable: Vec<&RuleEntry> = rules.iter().filter(|r| r.operations.contains(&op.id)).collect();
            if !applicable.is_empty() {
                let mut must = vec![Inline::strong("Must satisfy ")];
                for (i, r) in applicable.iter().enumerate() {
                    if i > 0 {
                        must.push(Inline::text(", "));
                    }
                    must.push(Inline::Link {
                        page: if rules_split { "rules".into() } else { "functional".into() },
                        anchor: Some("business-rules".into()),
                        v: r.id.clone(),
                    });
                }
                items.push(must);
            }

            if let Some(flow) = self.flow_for(&op.id) {
                let mut changes = Vec::new();
                let mut emits = Vec::new();
                let mut calls = Vec::new();
                for s in flow.steps.iter().skip(1) {
                    let target = match s.kind {
                        StepKind::Write => &mut changes,
                        StepKind::Publish => &mut emits,
                        StepKind::Call => &mut calls,
                        _ => continue,
                    };
                    if !target.is_empty() {
                        target.push(Inline::text(", "));
                    }
                    target.push(Inline::code(s.label.clone()));
                    if s.kind != StepKind::Write {
                        target.push(Inline::text(" → "));
                        target.push(self.element(&s.to));
                    }
                    target.push(self.cite(&s.evidence));
                }
                // State transitions performed by any function on the trace.
                if let Some(data) = self.data() {
                    for sm in &data.state_machines {
                        for t in &sm.transitions {
                            let performed =
                                t.trigger.as_ref().is_some_and(|f| flow.functions.contains(&format!("{}:{f}", t.unit)));
                            if performed {
                                if !changes.is_empty() {
                                    changes.push(Inline::text(", "));
                                }
                                changes.push(Inline::code(format!("{} → {}", sm.subject, t.to)));
                                changes.push(self.cite(&t.evidence));
                            }
                        }
                    }
                }
                for (label, list) in [("Changes state ", changes), ("Calls ", calls), ("Emits ", emits)] {
                    if !list.is_empty() {
                        let mut inl = vec![Inline::strong(label)];
                        inl.extend(list);
                        items.push(inl);
                    }
                }
            }

            let mut returns = vec![Inline::strong("Returns ")];
            returns.push(Inline::text(op.success_status.map(|s| format!("{s} ")).unwrap_or_default()));
            match &op.response {
                Some(t) => returns.push(self.type_inline(t)),
                None => returns.push(Inline::badge("muted", "type not declared")),
            }
            if !op.errors.is_empty() {
                let codes: BTreeSet<String> = op
                    .errors
                    .iter()
                    .map(|e| e.status.map(|s| s.to_string()).unwrap_or_else(|| "error".into()))
                    .collect();
                returns
                    .push(Inline::text(format!("; fails with {}", codes.into_iter().collect::<Vec<_>>().join(", "))));
            }
            items.push(returns);

            let acceptance = given_list(&intent.acceptance);
            if !acceptance.is_empty() {
                let mut inl = vec![Inline::strong("Acceptance ")];
                inl.extend(authored(&acceptance.join("; ")));
                items.push(inl);
            }
            blocks.push(Block::List { items });
            per_fr.push((op.unit.clone(), group, blocks));
        }
        // (page id, title, blocks) of requirement pages split off the overview.
        let mut unit_pages: Vec<(String, String, Vec<Block>)> = Vec::new();
        if split {
            // Overview: where each service's and group's requirements are; the requirements live on service pages.
            let mut rows = Vec::new();
            let mut units: Vec<String> = Vec::new();
            for (unit, _, _) in &per_fr {
                if !units.contains(unit) {
                    units.push(unit.clone());
                }
            }
            for unit in units {
                let page = format!("functional/{unit}");
                let unit_name = self.element_name(&unit);
                // A service with more requirements than a page holds gets one page per capability group.
                let per_group = frs.iter().filter(|(_, o)| o.unit == unit).count()
                    > super::api_ref::threshold("NUNKI_FR_SPLIT_OVER", FR_SPLIT_OVER);
                let mut order: Vec<String> = Vec::new();
                for (u, g, _) in &per_fr {
                    if *u == unit && !order.contains(g) {
                        order.push(g.clone());
                    }
                }
                order.sort();
                let mut ublocks = Vec::new();
                for g in &order {
                    let ids: Vec<&String> = frs
                        .iter()
                        .filter(|(_, o)| o.unit == unit && self.api_index.op_group.get(&o.id).is_some_and(|x| x == g))
                        .map(|(id, _)| id)
                        .collect();
                    let gname = if g.is_empty() { "General".to_string() } else { g.clone() };
                    let gid = format!("group-{}", slug(&gname));
                    let (link_page, link_anchor) = if per_group {
                        let gpage = format!("functional/{unit}/{}", self.group_slug(&unit, g));
                        let mut gblocks = Vec::new();
                        for (u, gg, fb) in per_fr.iter_mut() {
                            if *u == unit && gg == g {
                                gblocks.append(fb);
                            }
                        }
                        unit_pages.push((
                            gpage.clone(),
                            format!("Functional specification · {unit_name} · {gname}"),
                            gblocks,
                        ));
                        (gpage, None)
                    } else {
                        ublocks.push(Block::Heading { level: 2, id: gid.clone(), text: gname.clone() });
                        for (u, gg, fb) in per_fr.iter_mut() {
                            if *u == unit && gg == g {
                                ublocks.append(fb);
                            }
                        }
                        (page.clone(), Some(gid))
                    };
                    rows.push(vec![
                        vec![self.element(&unit)],
                        vec![Inline::Link { page: link_page, anchor: link_anchor, v: gname.clone() }],
                        vec![Inline::text(format!(
                            "{}{}",
                            ids.first().map(|s| s.as_str()).unwrap_or(""),
                            if ids.len() > 1 { format!(" … {}", ids.last().unwrap()) } else { String::new() }
                        ))],
                        vec![Inline::text(ids.len().to_string())],
                    ]);
                }
                if !per_group {
                    unit_pages.push((page.clone(), format!("Functional specification · {unit_name}"), ublocks));
                }
            }
            blocks.push(Block::Heading {
                level: 2,
                id: "requirements-index".into(),
                text: "Requirements by capability".into(),
            });
            blocks.push(Block::Table {
                columns: vec!["Service".into(), "Capability".into(), "Requirements".into(), "Count".into()],
                rows,
            });
        } else {
            let mut current_unit = String::new();
            for (unit, _, fb) in per_fr {
                if unit != current_unit {
                    current_unit = unit.clone();
                    let name = self.element_name(&unit);
                    blocks.push(Block::Heading { level: 2, id: format!("unit-{}", slug(&unit)), text: name });
                }
                blocks.extend(fb);
            }
        }
        blocks.extend(self.background_requirement_blocks(frs.len() + 1));

        let mut rule_blocks: Vec<Block> = Vec::new();
        let mut kind_pages: Vec<(String, String, Vec<Block>, usize)> = Vec::new();
        if !rules.is_empty() {
            rule_blocks.push(Block::Heading { level: 2, id: "business-rules".into(), text: "Business rules".into() });
            rule_blocks.push(Block::Para {
                inl: vec![Inline::text(
                    "Every constraint the code enforces on input, access, stored data and state changes, numbered for reference from the requirements above.",
                )],
            });
            // Very large catalogs get one page per rule kind (ids stay global).
            let by_kind =
                rules_split && rules.len() > 2 * super::api_ref::threshold("NUNKI_RULES_SPLIT_OVER", RULES_SPLIT_OVER);
            let mut kind_rows: Vec<(&'static str, Vec<Vec<Vec<Inline>>>)> = Vec::new();
            let mut rows = Vec::new();
            for r in &rules {
                let mut applies = r.applies_to.clone();
                let frs_for: Vec<&str> =
                    frs.iter().filter(|(_, o)| r.operations.contains(&o.id)).map(|(id, _)| id.as_str()).collect();
                if !frs_for.is_empty() {
                    applies.push(Inline::text(format!(" ({})", frs_for.join(", "))));
                }
                let row = vec![
                    vec![Inline::strong(r.id.clone())],
                    vec![Inline::text(r.statement.clone())],
                    vec![Inline::badge("muted", r.kind)],
                    applies,
                    vec![self.cite(&r.evidence)],
                ];
                if by_kind {
                    match kind_rows.iter_mut().find(|(k, _)| *k == r.kind) {
                        Some((_, rs)) => rs.push(row),
                        None => kind_rows.push((r.kind, vec![row])),
                    }
                } else {
                    rows.push(row);
                }
            }
            let columns: Vec<String> =
                vec!["Rule".into(), "Statement".into(), "Kind".into(), "Applies to".into(), "Code".into()];
            if by_kind {
                let mut index = Vec::new();
                for (kind, rs) in kind_rows {
                    let page = format!("rules/{}", slug(kind));
                    let (first, last) = (rs.first().cloned(), rs.last().cloned());
                    let range = |row: Option<Vec<Vec<Inline>>>| match row.as_ref().and_then(|r| r[0].first()) {
                        Some(Inline::Strong { v }) => v.clone(),
                        _ => String::new(),
                    };
                    index.push(vec![
                        vec![Inline::Link { page: page.clone(), anchor: None, v: kind.to_string() }],
                        vec![Inline::text(rs.len().to_string())],
                        vec![Inline::text(format!("{} … {}", range(first), range(last)))],
                    ]);
                    let n = rs.len();
                    kind_pages.push((
                        page,
                        format!("Business rules · {kind}"),
                        vec![Block::Table { columns: columns.clone(), rows: rs }],
                        n,
                    ));
                }
                rule_blocks
                    .push(Block::Table { columns: vec!["Kind".into(), "Rules".into(), "Ids".into()], rows: index });
            } else {
                rule_blocks.push(Block::Table { columns, rows });
            }
        }

        if !rules_split {
            blocks.append(&mut rule_blocks);
        } else if !rule_blocks.is_empty() {
            blocks.push(Block::Para {
                inl: vec![
                    Inline::text(format!("{} business rules enforced in code are catalogued on ", rules.len())),
                    Inline::Link {
                        page: "rules".into(),
                        anchor: Some("business-rules".into()),
                        v: "Business rules".into(),
                    },
                    Inline::text("."),
                ],
            });
        }
        let summary = vec![Inline::text(format!(
            "{} functional requirement{} and {} business rule{} as the code implements them, each traceable to the handler, rule or state change that realises it.",
            frs.len(),
            super::plural(frs.len()),
            rules.len(),
            super::plural(rules.len())
        ))];
        self.push_page("functional", "Functional specification", "Product", summary, blocks);
        for (page, title, ublocks) in unit_pages {
            let n = ublocks.iter().filter(|b| matches!(b, Block::Heading { level: 3, .. })).count();
            let summary = vec![Inline::text(format!(
                "{n} functional requirement{} as the code implements them.",
                super::plural(n)
            ))];
            self.push_page(&page, title, "Product", summary, ublocks);
        }
        if rules_split && !rule_blocks.is_empty() {
            let summary = vec![Inline::text(format!(
                "{} constraints the code enforces on input, access, stored data and state changes, numbered BR-001….",
                rules.len()
            ))];
            self.push_page("rules", "Business rules", "Product", summary, rule_blocks);
            for (page, title, kblocks, n) in kind_pages {
                let summary =
                    vec![Inline::text(format!("{n} rule{} of this kind enforced in code.", super::plural(n)))];
                self.push_page(&page, title, "Product", summary, kblocks);
            }
        }
    }

    /// (split requirements into per-service pages, move the rules catalog to its own page).
    pub(super) fn functional_layout(&self, frs: usize, rules: usize) -> (bool, bool) {
        (
            frs > super::api_ref::threshold("NUNKI_FR_SPLIT_OVER", FR_SPLIT_OVER),
            rules > super::api_ref::threshold("NUNKI_RULES_SPLIT_OVER", RULES_SPLIT_OVER),
        )
    }

    /// Page holding a requirement for an operation of `unit`.
    pub(super) fn fr_page_id(&self, op: &Operation, frs: usize, rules: usize) -> String {
        if !self.functional_layout(frs, rules).0 {
            return "functional".into();
        }
        let in_unit = self.api().map(|a| a.operations.iter().filter(|o| o.unit == op.unit).count()).unwrap_or(0);
        if in_unit > super::api_ref::threshold("NUNKI_FR_SPLIT_OVER", FR_SPLIT_OVER) {
            let group = self.api_index.op_group.get(&op.id).cloned().unwrap_or_default();
            format!("functional/{}/{}", op.unit, self.group_slug(&op.unit, &group))
        } else {
            format!("functional/{}", op.unit)
        }
    }

    pub(super) fn rules_page_id(&self, frs: usize, rules: usize) -> String {
        if self.functional_layout(frs, rules).1 {
            "rules".into()
        } else {
            "functional".into()
        }
    }

    // ── business requirements scaffold ───────────────────────────────────────

    pub(super) fn requirements_page(&mut self) {
        let frs = self.functional_requirements();
        let rules = self.rule_catalog().len();
        let biz = self.authored.business.clone();
        let mut blocks = vec![Block::Callout {
            tone: "warning".into(),
            title: "Business intent is authored, not generated".into(),
            inl: vec![
                Inline::text("Scope, capabilities, rules and integrations below are measured from code. Problem, goals, stakeholders and success metrics cannot be: they are read from "),
                Inline::code(crate::authored::AUTHORED),
                Inline::text(" and shown as open questions until someone answers them."),
            ],
        }];

        let section = |blocks: &mut Vec<Block>, id: &str, text: &str| {
            blocks.push(Block::Heading { level: 2, id: id.into(), text: text.into() });
        };

        section(&mut blocks, "problem", "1. Problem & purpose");
        match given(&biz.problem) {
            Some(p) => blocks.push(Block::Para { inl: authored(p) }),
            None => blocks.push(Block::Para { inl: vec![gap("the problem this system solves, for whom")] }),
        }
        if let Some(d) = self.report.system.description.clone() {
            let mut inl = vec![Inline::badge("observed", "from README"), Inline::text(format!(" {d}"))];
            if let Some(ev) = super::readme_evidence(std::path::Path::new(&self.report.repo.root), &d) {
                inl.push(self.cite(&ev));
            }
            blocks.push(Block::Para { inl });
        }

        section(&mut blocks, "goals", "2. Goals & non-goals");
        let lists = [
            ("Goals", &biz.goals, "measurable goals"),
            ("Non-goals", &biz.non_goals, "what is explicitly out of scope"),
        ];
        for (label, list, what) in lists {
            let items = given_list(list);
            blocks.push(Block::Para { inl: vec![Inline::strong(label)] });
            if items.is_empty() {
                blocks.push(Block::List { items: vec![vec![gap(what)]] });
            } else {
                blocks.push(Block::List { items: items.iter().map(|g| authored(g)).collect() });
            }
        }

        section(&mut blocks, "stakeholders", "3. Stakeholders & users");
        let people: Vec<_> = biz.stakeholders.iter().filter(|s| given(&s.name).is_some()).collect();
        if people.is_empty() {
            blocks.push(Block::Para { inl: vec![gap("stakeholders, their roles and what they need")] });
        } else {
            let rows = people
                .iter()
                .map(|s| {
                    vec![authored(&s.name), vec![Inline::text(s.role.clone())], vec![Inline::text(s.interest.clone())]]
                })
                .collect();
            blocks.push(Block::Table { columns: vec!["Stakeholder".into(), "Role".into(), "Interest".into()], rows });
        }
        let actors: BTreeSet<String> =
            self.authored.operations.values().filter_map(|o| given(&o.actor).map(str::to_string)).collect();
        let clients: Vec<String> = self
            .report
            .containers
            .iter()
            .filter(|u| matches!(u.kind, nunki_analyzer::UnitKind::WebClient))
            .map(|u| u.id.clone())
            .collect();
        if !actors.is_empty() || !clients.is_empty() {
            let mut inl = Vec::new();
            if !clients.is_empty() {
                inl.push(Inline::badge("observed", "from code"));
                inl.push(Inline::text(" user-facing clients: "));
                for (i, c) in clients.iter().enumerate() {
                    if i > 0 {
                        inl.push(Inline::text(", "));
                    }
                    inl.push(self.element(c));
                }
                inl.push(Inline::text(". "));
            }
            if !actors.is_empty() {
                inl.push(Inline::badge("authored", "authored"));
                inl.push(Inline::text(format!(
                    " actors named in requirements: {}.",
                    actors.into_iter().collect::<Vec<_>>().join(", ")
                )));
            }
            blocks.push(Block::Para { inl });
        }

        section(&mut blocks, "scope", "4. Scope: capabilities as built");
        if frs.is_empty() {
            blocks.push(Block::Para {
                inl: vec![Inline::text("No operations were recognised, so capabilities cannot be listed from code.")],
            });
        } else {
            let mut by_unit: BTreeMap<&str, Vec<Inline>> = BTreeMap::new();
            for (fr, op) in &frs {
                let intent = self.authored.operation(&op.id);
                let label = intent
                    .and_then(|i| given(&i.name))
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{} {}", op.method, op.path));
                let cell = by_unit.entry(op.unit.as_str()).or_default();
                if !cell.is_empty() {
                    cell.push(Inline::text(", "));
                }
                cell.push(Inline::Link {
                    page: self.fr_page_id(op, frs.len(), rules),
                    anchor: Some(fr.to_lowercase()),
                    v: format!("{fr} {label}"),
                });
            }
            let rows = by_unit.into_iter().map(|(u, caps)| vec![vec![self.element(u)], caps]).collect();
            blocks.push(Block::Table { columns: vec!["Service".into(), "Capabilities".into()], rows });
        }

        section(&mut blocks, "rules", "5. Business rules");
        let mut inl = vec![Inline::text(format!("{rules} rule{} enforced in code", super::plural(rules)))];
        if !frs.is_empty() && rules > 0 {
            inl.push(Inline::text("; see "));
            inl.push(Inline::Link {
                page: self.rules_page_id(frs.len(), rules),
                anchor: Some("business-rules".into()),
                v: "the rules catalog".into(),
            });
        }
        inl.push(Inline::text("."));
        blocks.push(Block::Para { inl });

        section(&mut blocks, "data", "6. Data & integrations");
        let entities = self.data().map(|d| d.entities.len()).unwrap_or(0);
        let vendors: Vec<String> = self
            .report
            .infrastructure
            .iter()
            .filter(|i| i.category == nunki_analyzer::catalog::InfraCategory::ThirdParty)
            .map(|i| i.label.clone())
            .collect();
        blocks.push(Block::Para {
            inl: vec![
                Inline::text(format!(
                    "{entities} persisted entit{}; external services: {}. Details on ",
                    if entities == 1 { "y" } else { "ies" },
                    if vendors.is_empty() { "none found".to_string() } else { vendors.join(", ") }
                )),
                Inline::link("data", "Data & integrations"),
                Inline::text("."),
            ],
        });

        section(&mut blocks, "metrics", "7. Success metrics");
        let metrics: Vec<_> = biz.success_metrics.iter().filter(|m| given(&m.metric).is_some()).collect();
        if metrics.is_empty() {
            blocks.push(Block::Para { inl: vec![gap("how success is measured, with targets")] });
        } else {
            let rows =
                metrics.iter().map(|m| vec![authored(&m.metric), vec![Inline::text(m.target.clone())]]).collect();
            blocks.push(Block::Table { columns: vec!["Metric".into(), "Target".into()], rows });
        }

        section(&mut blocks, "assumptions", "8. Assumptions & constraints");
        for (label, list, what) in [
            ("Assumptions", &biz.assumptions, "assumptions"),
            ("Constraints", &biz.constraints, "business, legal or operational constraints"),
        ] {
            let items = given_list(list);
            blocks.push(Block::Para { inl: vec![Inline::strong(label)] });
            if items.is_empty() {
                blocks.push(Block::List { items: vec![vec![gap(what)]] });
            } else {
                blocks.push(Block::List { items: items.iter().map(|g| authored(g)).collect() });
            }
        }

        section(&mut blocks, "open-questions", "9. Open questions");
        let mut questions: Vec<Vec<Inline>> = Vec::new();
        let unanswered = frs
            .iter()
            .filter(|(_, o)| {
                self.authored.operation(&o.id).is_none_or(|i| given(&i.actor).is_none() || given(&i.purpose).is_none())
            })
            .count();
        if unanswered > 0 {
            questions.push(vec![Inline::text(format!(
                "{unanswered} of {} requirement{} have no authored actor or purpose.",
                frs.len(),
                super::plural(frs.len())
            ))]);
        }
        for (fr, op) in &frs {
            if op.confidence == Confidence::Opaque {
                questions.push(vec![
                    Inline::Link {
                        page: self.fr_page_id(op, frs.len(), rules),
                        anchor: Some(fr.to_lowercase()),
                        v: fr.clone(),
                    },
                    Inline::text(format!(
                        " {} {} declares no request or response type: what does it accept and return?",
                        op.method, op.path
                    )),
                ]);
            }
            if op.path_partial {
                questions.push(vec![
                    Inline::Link {
                        page: self.fr_page_id(op, frs.len(), rules),
                        anchor: Some(fr.to_lowercase()),
                        v: fr.clone(),
                    },
                    Inline::text(format!(
                        " {} {}: the full route prefix could not be resolved from code.",
                        op.method, op.path
                    )),
                ]);
            }
        }
        if let Some(api) = self.api() {
            for c in api.client_calls.iter().filter(|c| !c.drift.is_empty()) {
                let mut inl = vec![self.element(&c.unit)];
                inl.push(Inline::text(format!(
                    " reads {} from {} {}, which the operation does not declare",
                    c.drift.join(", "),
                    c.method,
                    c.path
                )));
                inl.push(self.cite(&c.evidence));
                // The other half of the claim: where the response shape is declared.
                if let Some(m) = c
                    .operation
                    .as_deref()
                    .and_then(|id| api.operations.iter().find(|o| o.id == id))
                    .and_then(|o| o.response.as_ref())
                    .and_then(|r| r.model.as_deref())
                    .and_then(|id| api.models.iter().find(|m| m.id == id))
                {
                    inl.push(Inline::text(format!(" — {} declares ", m.name)));
                    inl.push(Inline::text(m.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(", ")));
                    inl.push(self.cite(&m.evidence));
                }
                questions.push(inl);
            }
        }
        if questions.is_empty() {
            questions.push(vec![Inline::text("None raised from code.")]);
        }
        blocks.push(Block::List { items: questions });

        let summary = vec![Inline::text(
            "A business requirements document scaffolded from the implementation: what is built is measured, why and for whom is authored, and every unanswered question is listed.",
        )];
        self.push_page("requirements", "Business requirements", "Product", summary, blocks);
    }
}
