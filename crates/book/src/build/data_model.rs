//! The data model section: entities, their columns and relationships, and the
//! lifecycles of their status fields.
//!
//! A model that fits one figure (≤ `MAX_ENTITIES`) is drawn as one. A larger one
//! is presented by business domain (see `nunki_analyzer::domains`): an
//! overview of the domains and how they reference each other, then one
//! entity-relationship figure per domain. Past `DOMAIN_PAGE_THRESHOLD` entities
//! each domain gets its own page, `data/<domain>`, so the data page stays
//! readable.

use std::collections::{BTreeMap, BTreeSet};

use nunki_analyzer::data::{Access, DataModel, Entity, StateMachine};
use nunki_analyzer::domains::{domains, draft_domain_irs, draft_domain_overview_ir, DomainSummary};
use nunki_analyzer::scan::{slug, EvidenceRef};
use nunki_analyzer::{draft_entity_ir, DraftOptions};
use nunki_validator::ValidateOptions;

use super::Builder;
use crate::model::*;

/// Entities beyond which each domain moves to its own page.
pub(super) const DOMAIN_PAGE_THRESHOLD: usize = 40;

pub(super) fn domain_page_id(d: &DomainSummary) -> String {
    format!("data/{}", d.slug)
}

fn domain_figure_id(d: &DomainSummary) -> String {
    format!("data-model-{}", d.slug)
}

fn entity_anchor(e: &Entity) -> String {
    format!("entity-{}", slug(&e.table))
}

fn lifecycle_figure_id(sm: &StateMachine) -> String {
    format!("lifecycle-{}", slug(sm.id.trim_start_matches("state:")))
}

impl<'a> Builder<'a> {
    fn data_model(&self) -> Option<&'a DataModel> {
        self.report.data.as_ref().filter(|d| !d.entities.is_empty() || !d.state_machines.is_empty())
    }

    fn data_domains(&self) -> Vec<DomainSummary> {
        self.data_model().map(domains).unwrap_or_default()
    }

    fn domain_pages_split(&self) -> bool {
        let doms = self.data_domains();
        !doms.is_empty() && self.data_model().is_some_and(|d| d.entities.len() > DOMAIN_PAGE_THRESHOLD)
    }

    fn page_of_entity(&self, e: &Entity) -> String {
        if self.domain_pages_split() {
            if let Some(d) = self.data_domains().iter().find(|d| d.entities.contains(&e.id)) {
                return domain_page_id(d);
            }
        }
        "data".into()
    }

    /// The entity a state machine's subject (`orders.status`) belongs to.
    fn machine_entity(&self, sm: &StateMachine) -> Option<&'a Entity> {
        let data = self.data_model()?;
        let table = sm.subject.rsplit_once('.').map(|(t, _)| t).unwrap_or(&sm.subject);
        data.entities.iter().find(|e| e.table == table || e.name == table)
    }

    // ── figures ──────────────────────────────────────────────────────────────

    pub(super) fn add_data_model_figures(
        &mut self,
        curated: &BTreeMap<String, String>,
        validate: &ValidateOptions,
        draft: &DraftOptions,
    ) {
        let report = self.report;
        let Some(data) = report.data.as_ref() else { return };
        if data.entities.is_empty() {
            return;
        }
        let split = draft_domain_irs(report, data, draft);
        if split.is_empty() {
            if let Some(d) = draft_entity_ir(report, data, draft) {
                self.add_figure("data-model", d, curated, validate);
            }
            return;
        }
        if let Some(overview) = draft_domain_overview_ir(report, data, draft) {
            self.add_figure("data-model", overview, curated, validate);
        }
        for (d, ir) in split {
            self.add_figure(&domain_figure_id(&d), ir, curated, validate);
        }
    }

    // ── data page ────────────────────────────────────────────────────────────

    pub(super) fn data_model_blocks_impl(&mut self) -> Vec<Block> {
        let mut blocks = Vec::new();
        let Some(data) = self.data_model() else { return blocks };
        let doms = self.data_domains();
        let machines = self.business_machines();
        if !data.entities.is_empty() {
            blocks.push(Block::Heading { level: 2, id: "data-model".into(), text: "Data model".into() });
            let sources: BTreeSet<&str> = data.entities.iter().map(|e| e.source.as_str()).collect();
            let mut intro = format!(
                "{} persisted entit{} declared in {}. Keys, nullability and relationships are read from the declarations; crow's feet mark the many side.",
                data.entities.len(),
                if data.entities.len() == 1 { "y" } else { "ies" },
                sources.into_iter().collect::<Vec<_>>().join(", ")
            );
            if !doms.is_empty() {
                intro.push_str(&format!(
                    " They are grouped into {} domains by the packages and modules that declare and use them; references that cross a domain boundary end at a card in “Other domains”.",
                    doms.len()
                ));
            }
            blocks.push(Block::Para { inl: vec![Inline::text(intro)] });
            if doms.is_empty() {
                if let Some(f) = self.figure("data-model", vec![Inline::text("Entities and relationships")]) {
                    blocks.push(f);
                }
                let all: Vec<&Entity> = data.entities.iter().collect();
                blocks.push(self.entity_table(&all));
                for e in &all {
                    blocks.extend(self.column_blocks(e, 3));
                }
            } else {
                if let Some(f) = self.figure(
                    "data-model",
                    vec![Inline::text(
                        "Domains, the references between them, and the services that read and write them",
                    )],
                ) {
                    blocks.push(f);
                }
                blocks.push(self.domains_table(&doms));
                if !self.domain_pages_split() {
                    for d in &doms {
                        blocks.extend(self.domain_blocks(d, 3, &machines));
                    }
                }
            }
        }
        // Lifecycles not tied to a domain page are listed here.
        let here: Vec<&StateMachine> = machines
            .iter()
            .copied()
            .filter(|sm| {
                if doms.is_empty() {
                    return true;
                }
                match self.machine_entity(sm) {
                    Some(e) => !doms.iter().any(|d| d.entities.contains(&e.id)),
                    None => true,
                }
            })
            .collect();
        blocks.extend(self.lifecycle_blocks(&here, 2));
        blocks
    }

    /// Separate pages per domain for large models. Call right after the data page.
    pub(super) fn data_domain_pages(&mut self) {
        if !self.domain_pages_split() {
            return;
        }
        let machines = self.business_machines();
        for d in self.data_domains() {
            let blocks = self.domain_blocks(&d, 2, &machines);
            let summary = vec![Inline::text(format!(
                "{} entit{} of the {} domain: relationships, columns, constraints, who reads and writes them, and their lifecycles.",
                d.entities.len(),
                if d.entities.len() == 1 { "y" } else { "ies" },
                d.label
            ))];
            self.push_page(&domain_page_id(&d), format!("{} data", d.label), "Data", summary, blocks);
        }
    }

    fn domains_table(&mut self, doms: &[DomainSummary]) -> Block {
        let data = self.data_model().expect("data model");
        let split = self.domain_pages_split();
        let mut rows = Vec::new();
        for d in doms {
            let ents: Vec<&Entity> = data.entities.iter().filter(|e| d.entities.contains(&e.id)).collect();
            let link = if split {
                Inline::link(domain_page_id(d), d.label.clone())
            } else {
                Inline::Link { page: "data".into(), anchor: Some(format!("domain-{}", d.slug)), v: d.label.clone() }
            };
            let tables: Vec<String> = ents.iter().take(4).map(|e| e.table.clone()).collect();
            let mut tables_cell = vec![Inline::code(tables.join(", "))];
            if ents.len() > 4 {
                tables_cell.push(Inline::text(format!(" +{}", ents.len() - 4)));
            }
            let writes: Vec<Access> = ents.iter().flat_map(|e| e.writes.iter().cloned()).collect();
            let reads: Vec<Access> = ents.iter().flat_map(|e| e.reads.iter().cloned()).collect();
            rows.push(vec![
                vec![link],
                vec![Inline::text(ents.len().to_string())],
                tables_cell,
                self.access_units(&writes),
                self.access_units(&reads),
            ]);
        }
        Block::Table {
            columns: vec!["Domain".into(), "Entities".into(), "Tables".into(), "Written by".into(), "Read by".into()],
            rows,
        }
    }

    fn domain_blocks(&mut self, d: &DomainSummary, level: u8, machines: &[&'a StateMachine]) -> Vec<Block> {
        let data = self.data_model().expect("data model");
        let ents: Vec<&Entity> = data.entities.iter().filter(|e| d.entities.contains(&e.id)).collect();
        let mut blocks =
            vec![Block::Heading { level, id: format!("domain-{}", d.slug), text: format!("{} domain", d.label) }];
        let crossing: usize = ents
            .iter()
            .flat_map(|e| e.relations.iter())
            .filter(|r| !d.entities.contains(&r.target) && data.entities.iter().any(|x| x.id == r.target))
            .count();
        let mut caption =
            vec![Inline::text(format!("{} entit{}", ents.len(), if ents.len() == 1 { "y" } else { "ies" }))];
        if crossing > 0 {
            caption.push(Inline::text(format!(
                ", {crossing} reference{} to other domains",
                if crossing == 1 { "" } else { "s" }
            )));
        }
        if let Some(f) = self.figure(&domain_figure_id(d), caption) {
            blocks.push(f);
        }
        blocks.push(self.entity_table(&ents));
        for e in &ents {
            blocks.extend(self.column_blocks(e, level + 1));
        }
        let mine: Vec<&StateMachine> = machines
            .iter()
            .copied()
            .filter(|sm| self.machine_entity(sm).is_some_and(|e| d.entities.contains(&e.id)))
            .collect();
        blocks.extend(self.lifecycle_blocks(&mine, level + 1));
        blocks
    }

    fn entity_table(&mut self, ents: &[&Entity]) -> Block {
        let mut rows = Vec::new();
        for e in ents {
            let mut name =
                vec![Inline::Link { page: self.page_of_entity(e), anchor: Some(entity_anchor(e)), v: e.table.clone() }];
            name.push(Inline::text(" "));
            name.push(self.cite(&e.evidence));
            rows.push(vec![
                name,
                vec![Inline::text(e.source.clone())],
                vec![Inline::text(e.columns.len().to_string())],
                self.access_cell(&e.writes),
                self.access_cell(&e.reads),
            ]);
        }
        Block::Table {
            columns: vec![
                "Entity".into(),
                "Declared in".into(),
                "Columns".into(),
                "Written by".into(),
                "Read by".into(),
            ],
            rows,
        }
    }

    fn column_blocks(&mut self, e: &Entity, level: u8) -> Vec<Block> {
        let mut blocks = vec![Block::Heading { level, id: entity_anchor(e), text: e.table.clone() }];
        let mut rows = Vec::new();
        for c in &e.columns {
            let mut key = Vec::new();
            if c.primary_key {
                key.push(Inline::badge("observed", "PK"));
            }
            if let Some(r) = &c.references {
                key.push(Inline::badge("declared", "FK"));
                key.push(Inline::text(format!(" → {r}")));
            }
            if c.unique {
                key.push(Inline::badge("muted", "unique"));
            }
            let mut rules: Vec<Inline> = Vec::new();
            if let Some(d) = &c.default {
                rules.push(Inline::text("default "));
                rules.push(Inline::code(d.clone()));
            }
            for (i, k) in c.constraints.iter().enumerate() {
                if i > 0 || c.default.is_some() {
                    rules.push(Inline::text("; "));
                }
                rules.push(Inline::code(k.clone()));
            }
            rows.push(vec![
                vec![Inline::code(c.name.clone()), Inline::text(" "), self.cite(&c.evidence)],
                vec![Inline::code(c.type_name.clone())],
                key,
                vec![Inline::text(if c.nullable { "yes" } else { "no" })],
                rules,
            ]);
        }
        blocks.push(Block::Table {
            columns: vec![
                "Column".into(),
                "Type".into(),
                "Key".into(),
                "Nullable".into(),
                "Default & constraints".into(),
            ],
            rows,
        });
        blocks
    }

    fn lifecycle_blocks(&mut self, machines: &[&StateMachine], level: u8) -> Vec<Block> {
        let mut blocks = Vec::new();
        if machines.is_empty() {
            return blocks;
        }
        let id =
            if level == 2 { "lifecycles".to_string() } else { format!("lifecycles-{}", slug(&machines[0].subject)) };
        blocks.push(Block::Heading { level, id, text: "Lifecycles".into() });
        blocks.push(Block::Para {
            inl: vec![Inline::text(
                "States come from enum and check-constraint declarations; transitions from the code that assigns them, with the guard it checks first. A transition drawn from “any state” is one the code performs without checking the current state.",
            )],
        });
        for sm in machines {
            let fig = lifecycle_figure_id(sm);
            blocks.push(Block::Heading { level: level + 1, id: fig.clone(), text: sm.subject.clone() });
            let mut caption =
                vec![Inline::text(format!("{} states, {} transitions ", sm.states.len(), sm.transitions.len()))];
            caption.push(self.cite(&sm.evidence));
            if let Some(f) = self.figure(&fig, caption) {
                blocks.push(f);
            }
            if sm.transitions.is_empty() {
                blocks.push(Block::Callout {
                    tone: "note".into(),
                    title: "No transitions found".into(),
                    inl: vec![Inline::text("The states are declared, but no code assigning them was recognised.")],
                });
                continue;
            }
            let mut rows = Vec::new();
            for t in &sm.transitions {
                rows.push(vec![
                    vec![t.from.clone().map(Inline::code).unwrap_or_else(|| Inline::badge("muted", "any"))],
                    vec![Inline::code(t.to.clone())],
                    vec![
                        self.element(&t.unit),
                        Inline::text(" "),
                        t.trigger.clone().map(Inline::code).unwrap_or_else(|| Inline::text("")),
                    ],
                    vec![t.guard.clone().map(Inline::code).unwrap_or_else(|| Inline::text("—"))],
                    vec![self.cite(&t.evidence)],
                ]);
            }
            blocks.push(Block::Table {
                columns: vec!["From".into(), "To".into(), "Performed by".into(), "Guard".into(), "Code".into()],
                rows,
            });
        }
        blocks
    }

    fn access_cell(&mut self, list: &[Access]) -> Vec<Inline> {
        let mut units: BTreeMap<&str, &EvidenceRef> = BTreeMap::new();
        for a in list {
            units.entry(a.unit.as_str()).or_insert(&a.evidence);
        }
        if units.is_empty() {
            return vec![Inline::badge("muted", "none found")];
        }
        let mut out = Vec::new();
        for (u, ev) in units {
            if !out.is_empty() {
                out.push(Inline::text(", "));
            }
            out.push(self.element(u));
            out.push(self.cite(ev));
        }
        out
    }

    /// Units only (no citations): the domains table summarises many entities.
    fn access_units(&self, list: &[Access]) -> Vec<Inline> {
        let units: BTreeSet<&str> = list.iter().map(|a| a.unit.as_str()).collect();
        if units.is_empty() {
            return vec![Inline::badge("muted", "none found")];
        }
        let mut out = Vec::new();
        for u in units {
            if !out.is_empty() {
                out.push(Inline::text(", "));
            }
            out.push(self.element(u));
        }
        out
    }
}
