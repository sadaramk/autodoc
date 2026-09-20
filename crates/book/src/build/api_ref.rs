//! API reference: one page per service, split into an overview (capability
//! map, groups, access matrix) and one page per capability group when a
//! service has more operations than a single page reads well.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use nunki_analyzer::api::{ApiModel, Confidence, Model, Operation, TypeRef};
use nunki_analyzer::capability::groups_of;
use nunki_analyzer::scan::{slug, ScanReport};
use nunki_analyzer::trace::worth_drawing;
use nunki_analyzer::{draft_capability_ir, DraftOptions};
use nunki_validator::ValidateOptions;

use super::access::Row;
use super::behavior::{confidence_badge, flow_figure_id};
use super::Builder;
use crate::model::*;

/// Services with more operations than this get an overview and group pages.
pub(super) const SPLIT_OVER: usize = 25;

/// A page-splitting threshold, overridable through the environment (used by
/// tests to exercise split layouts on small fixtures).
pub(super) fn threshold(var: &str, default: usize) -> usize {
    std::env::var(var).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

pub(super) fn api_page_id(unit: &str) -> String {
    format!("api/{unit}")
}

pub(super) fn group_page_id(unit: &str, group_slug: &str) -> String {
    format!("api/{unit}/{group_slug}")
}

pub(super) fn capability_figure_id(unit: &str) -> String {
    format!("capabilities-{unit}")
}

/// A capability group as documented.
pub(super) struct GroupInfo {
    pub name: String,
    pub slug: String,
    pub ops: Vec<String>,
}

/// Where every operation and model is documented.
#[derive(Default)]
pub(super) struct ApiIndex {
    pub groups: BTreeMap<String, Vec<GroupInfo>>,
    pub split: BTreeSet<String>,
    pub op_page: HashMap<String, String>,
    pub model_page: HashMap<String, String>,
    pub op_group: HashMap<String, String>,
    /// Anchors are derived from human text, and `GET /users` and `GET /users/`
    /// slug alike. Two headings with one id send every link to the first, so a
    /// reader following a contract link lands on a different operation. Held
    /// here so the heading and every link to it agree on one unique anchor.
    op_anchors: HashMap<String, String>,
    model_anchors: HashMap<String, String>,
}

impl ApiIndex {
    pub(super) fn new(report: &ScanReport) -> ApiIndex {
        let mut ix = ApiIndex::default();
        // Anchors only have to be unique within the page that carries them.
        let mut used: BTreeSet<(String, String)> = BTreeSet::new();
        let Some(api) = report.api.as_ref() else { return ix };
        let units: Vec<&str> = report
            .containers
            .iter()
            .map(|u| u.id.as_str())
            .filter(|u| api.operations.iter().any(|o| o.unit == *u))
            .collect();
        for unit in units {
            let groups = groups_of(api, unit);
            let total: usize = groups.iter().map(|(_, o)| o.len()).sum();
            let split = total > threshold("NUNKI_API_SPLIT_OVER", SPLIT_OVER);
            if split {
                ix.split.insert(unit.to_string());
            }
            let mut slugs: BTreeSet<String> = BTreeSet::new();
            let mut infos = Vec::new();
            for (name, ops) in groups {
                let base = {
                    let s = slug(&name);
                    if s.is_empty() {
                        "general".to_string()
                    } else {
                        s
                    }
                };
                let mut s = base.clone();
                let mut n = 2;
                while !slugs.insert(s.clone()) {
                    s = format!("{base}-{n}");
                    n += 1;
                }
                let page = if split { group_page_id(unit, &s) } else { api_page_id(unit) };
                for op in &ops {
                    ix.op_page.insert(op.id.clone(), page.clone());
                    ix.op_group.insert(op.id.clone(), name.clone());
                    let anchor = unique(&mut used, &page, super::behavior::op_anchor(op));
                    ix.op_anchors.insert(op.id.clone(), anchor);
                    for m in models_used(api, op) {
                        ix.model_page.entry(m.clone()).or_insert_with(|| page.clone());
                        if let std::collections::hash_map::Entry::Vacant(e) = ix.model_anchors.entry(m.clone()) {
                            e.insert(unique(&mut used, &page, format!("model-{}", slug(&m))));
                        }
                    }
                }
                infos.push(GroupInfo { name, slug: s, ops: ops.iter().map(|o| o.id.clone()).collect() });
            }
            ix.groups.insert(unit.to_string(), infos);
        }
        ix
    }

    /// The anchor the heading for this operation carries, and that every link
    /// to it must use.
    pub(super) fn op_anchor(&self, op: &Operation) -> String {
        self.op_anchors.get(&op.id).cloned().unwrap_or_else(|| super::behavior::op_anchor(op))
    }

    pub(super) fn model_anchor(&self, name: &str) -> String {
        self.model_anchors.get(name).cloned().unwrap_or_else(|| format!("model-{}", slug(name)))
    }
}

/// `base`, or `base-2`, `base-3`… when that page already carries it.
fn unique(used: &mut BTreeSet<(String, String)>, page: &str, base: String) -> String {
    let mut candidate = base.clone();
    let mut n = 2;
    while !used.insert((page.to_string(), candidate.clone())) {
        candidate = format!("{base}-{n}");
        n += 1;
    }
    candidate
}

/// Models an operation's request and response use, with nested field models.
fn models_used(api: &ApiModel, op: &Operation) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in [&op.request_body, &op.response].into_iter().flatten() {
        if let Some(m) = &t.model {
            if !out.contains(m) {
                out.push(m.clone());
            }
        }
    }
    let mut i = 0;
    while i < out.len() {
        if let Some(m) = api.models.iter().find(|x| x.id == out[i]) {
            for f in &m.fields {
                if let Some(nested) = &f.model {
                    if !out.contains(nested) {
                        out.push(nested.clone());
                    }
                }
            }
        }
        i += 1;
    }
    out
}

impl<'a> Builder<'a> {
    pub(super) fn api_operation(&self, id: &str) -> Option<&'a Operation> {
        self.api().and_then(|a| a.operations.iter().find(|o| o.id == id))
    }

    /// Page slug of a capability group within a service.
    pub(super) fn group_slug(&self, unit: &str, group: &str) -> String {
        self.api_index
            .groups
            .get(unit)
            .and_then(|gs| gs.iter().find(|g| g.name == group))
            .map(|g| g.slug.clone())
            .unwrap_or_else(|| {
                let s = slug(group);
                if s.is_empty() {
                    "general".into()
                } else {
                    s
                }
            })
    }

    /// Link to the operation's contract, wherever it is documented.
    pub(super) fn op_link(&self, op: &Operation) -> Inline {
        Inline::Link {
            page: self.api_index.op_page.get(&op.id).cloned().unwrap_or_else(|| api_page_id(&op.unit)),
            anchor: Some(self.api_index.op_anchor(op)),
            v: route_label(op),
        }
    }

    pub(super) fn type_inline(&self, t: &TypeRef) -> Inline {
        let text = if t.collection { format!("{}[]", t.type_name) } else { t.type_name.clone() };
        match t.model.as_deref().and_then(|id| self.model(id).map(|m| (id, m))) {
            Some((id, m)) => match self.api_index.model_page.get(id) {
                Some(page) => {
                    Inline::Link { page: page.clone(), anchor: Some(self.api_index.model_anchor(&m.name)), v: text }
                }
                None => Inline::code(text),
            },
            None => Inline::code(text),
        }
    }

    pub(super) fn add_capability_figures(
        &mut self,
        curated: &BTreeMap<String, String>,
        validate: &ValidateOptions,
        draft: &DraftOptions,
    ) {
        let report = self.report;
        let units: Vec<String> =
            self.api_index.groups.iter().filter(|(_, g)| g.len() >= 2).map(|(u, _)| u.clone()).collect();
        for unit in units {
            if let Some(d) = draft_capability_ir(report, &unit, draft) {
                self.add_figure(&capability_figure_id(&unit), d, curated, validate);
            }
        }
    }

    /// API reference pages for one service.
    pub(super) fn api_pages(&mut self, unit: &str) {
        let Some(api) = self.api() else { return };
        let mut ops: Vec<&Operation> = api.operations.iter().filter(|o| o.unit == unit).collect();
        ops.sort_by(|a, b| (&a.path, &a.method).cmp(&(&b.path, &b.method)));
        if ops.is_empty() {
            return;
        }
        let name = self.unit(unit).map(|u| self.unit_name(u)).unwrap_or_else(|| unit.to_string());
        let frameworks: BTreeSet<&str> = ops.iter().map(|o| o.framework.as_str()).collect();
        let excluded: Vec<_> = api.excluded.iter().filter(|x| x.operation.starts_with(&format!("{unit}:"))).collect();
        let split = self.api_index.split.contains(unit);

        let mut blocks = self.coverage_blocks(&ops, excluded.len());
        let fig = capability_figure_id(unit);
        let group_count = self.api_index.groups.get(unit).map(Vec::len).unwrap_or(0);
        if let Some(f) = self.figure(
            &fig,
            vec![Inline::text(format!(
                "{} capability group{}: who calls each and what its operations touch",
                group_count,
                super::plural(group_count)
            ))],
        ) {
            blocks.push(Block::Heading { level: 2, id: "capabilities".into(), text: "Capabilities".into() });
            blocks.push(f);
            // When persistence sits in a shared library, no operation reaches a
            // store directly: say where the data access happens instead.
            let through = nunki_analyzer::capability::data_access_through_libraries(self.report, unit);
            if !through.is_empty() {
                let mut inl = vec![Inline::text(
                    "These operations reach no store directly. Data access happens in the libraries this service links: ",
                )];
                for (i, (lib, stores)) in through.iter().enumerate() {
                    if i > 0 {
                        inl.push(Inline::text("; "));
                    }
                    inl.push(Inline::strong(lib.clone()));
                    inl.push(Inline::text(format!(" ({})", stores.join(", "))));
                }
                inl.push(Inline::text("."));
                blocks.push(Block::Callout { tone: "note".into(), title: "Where the data is".into(), inl });
            }
        }

        if split {
            // Overview: groups table and a group-level access matrix; contracts live on group pages.
            let groups: Vec<(String, String, Vec<&Operation>)> = self
                .api_index
                .groups
                .get(unit)
                .map(|gs| {
                    gs.iter()
                        .map(|g| {
                            let gops: Vec<&Operation> = g.ops.iter().filter_map(|id| self.api_operation(id)).collect();
                            (g.name.clone(), g.slug.clone(), gops)
                        })
                        .collect()
                })
                .unwrap_or_default();
            if !self.figures.contains_key(&fig) {
                blocks.push(Block::Heading { level: 2, id: "capabilities".into(), text: "Capabilities".into() });
            }
            let mut rows = Vec::new();
            for (gname, gslug, gops) in &groups {
                let mut methods: BTreeMap<&str, usize> = BTreeMap::new();
                for o in gops {
                    *methods.entry(o.method.as_str()).or_default() += 1;
                }
                let callers: BTreeSet<&str> = api
                    .client_calls
                    .iter()
                    .filter(|c| c.operation.as_deref().is_some_and(|id| gops.iter().any(|o| o.id == id)))
                    .map(|c| c.unit.as_str())
                    .collect();
                let mut caller_cell = Vec::new();
                for (i, c) in callers.iter().enumerate() {
                    if i > 0 {
                        caller_cell.push(Inline::text(", "));
                    }
                    caller_cell.push(self.element(c));
                }
                if caller_cell.is_empty() {
                    caller_cell.push(Inline::text("—"));
                }
                rows.push(vec![
                    vec![Inline::Link { page: group_page_id(unit, gslug), anchor: None, v: gname.clone() }],
                    vec![Inline::text(gops.len().to_string())],
                    vec![Inline::text({
                        let order = ["GET", "POST", "PUT", "PATCH", "DELETE"];
                        let mut ms: Vec<(&&str, &usize)> = methods.iter().collect();
                        ms.sort_by_key(|(m, _)| {
                            (order.iter().position(|o| o == *m).unwrap_or(order.len()), m.to_string())
                        });
                        ms.iter().map(|(m, n)| format!("{m} {n}")).collect::<Vec<_>>().join(" · ")
                    })],
                    caller_cell,
                ]);
            }
            blocks.push(Block::Table {
                columns: vec!["Group".into(), "Operations".into(), "Methods".into(), "Called by".into()],
                rows,
            });
            let access_rows: Vec<Row> = groups
                .iter()
                .map(|(gname, gslug, gops)| Row {
                    label: vec![Inline::Link { page: group_page_id(unit, gslug), anchor: None, v: gname.clone() }],
                    ops: gops.clone(),
                })
                .collect();
            if let Some(t) = self.access_matrix(&access_rows) {
                blocks.push(Block::Heading { level: 2, id: "access".into(), text: "Access".into() });
                blocks.push(Builder::access_intro());
                blocks.push(t);
            }
            self.excluded_blocks(&mut blocks, &excluded);
            let summary = vec![Inline::text(format!(
                "{} {} operations {} serves, in {} capability groups: what each group covers, who calls it and who may.",
                ops.len(),
                frameworks.iter().copied().collect::<Vec<_>>().join(" / "),
                name,
                groups.len()
            ))];
            self.push_page(&api_page_id(unit), format!("{name} API"), "API reference", summary, blocks);

            for (gname, gslug, gops) in groups {
                let page = group_page_id(unit, &gslug);
                let mut gblocks = Vec::new();
                gblocks.push(Block::Para {
                    inl: vec![
                        Inline::text(format!("{} operation{} in the ", gops.len(), super::plural(gops.len()))),
                        Inline::strong(gname.clone()),
                        Inline::text(" group of "),
                        Inline::Link { page: api_page_id(unit), anchor: None, v: format!("{name} API") },
                        Inline::text("."),
                    ],
                });
                self.operations_index(&mut gblocks, &gops, false);
                let access_rows: Vec<Row> =
                    gops.iter().map(|o| Row { label: vec![self.op_link(o)], ops: vec![*o] }).collect();
                if let Some(t) = self.access_matrix(&access_rows) {
                    gblocks.push(Block::Heading { level: 2, id: "access".into(), text: "Access".into() });
                    gblocks.push(t);
                }
                for op in &gops {
                    self.operation_blocks(op, &mut gblocks);
                }
                self.models_blocks(&mut gblocks, &page);
                let summary = vec![Inline::text(format!(
                    "Contracts of the {} operations in {}'s {} group.",
                    gops.len(),
                    name,
                    gname
                ))];
                self.push_page(&page, format!("{name} API · {gname}"), "API reference", summary, gblocks);
            }
            return;
        }

        // Single page.
        self.operations_index(&mut blocks, &ops, group_count >= 2);
        let access_rows: Vec<Row> = ops.iter().map(|o| Row { label: vec![self.op_link(o)], ops: vec![*o] }).collect();
        if let Some(t) = self.access_matrix(&access_rows) {
            blocks.push(Block::Heading { level: 2, id: "access".into(), text: "Access".into() });
            blocks.push(Builder::access_intro());
            blocks.push(t);
        }
        for op in &ops {
            self.operation_blocks(op, &mut blocks);
        }
        self.models_blocks(&mut blocks, &api_page_id(unit));
        self.excluded_blocks(&mut blocks, &excluded);
        let mut summary = vec![Inline::text(format!(
            "The contract of each {} operation documented for {}: parameters with their wire names and rules, request and response models, errors, authentication and who calls it.",
            frameworks.into_iter().collect::<Vec<_>>().join(" / "),
            name
        ))];
        // Claiming "every operation" is a claim of completeness, and routes get
        // left out — operational endpoints, server-rendered views. Say so here
        // rather than let the reader discover the gap by counting annotations.
        if !excluded.is_empty() {
            summary.push(Inline::text(format!(
                " {} route{} handled by this service {} left out, listed at the end with the reason.",
                excluded.len(),
                super::plural(excluded.len()),
                if excluded.len() == 1 { "is" } else { "are" }
            )));
        }
        self.push_page(&api_page_id(unit), format!("{name} API"), "API reference", summary, blocks);
    }

    fn coverage_blocks(&self, ops: &[&Operation], excluded: usize) -> Vec<Block> {
        let typed = ops.iter().filter(|o| o.confidence == Confidence::Typed).count();
        let partial = ops.iter().filter(|o| o.confidence == Confidence::Partial).count();
        let opaque = ops.len() - typed - partial;
        let mut blocks = vec![Block::Stats {
            items: vec![
                Stat { label: "operations".into(), value: ops.len().to_string(), page: None },
                Stat { label: "typed contracts".into(), value: typed.to_string(), page: None },
                Stat { label: "partial".into(), value: partial.to_string(), page: None },
                Stat { label: "opaque".into(), value: opaque.to_string(), page: None },
            ],
        }];
        let mut coverage = vec![Inline::text(format!(
            "{} of {} operation{} declare request and response types; {} partially; {} not at all (the handler reads the request at runtime).",
            typed,
            ops.len(),
            super::plural(ops.len()),
            partial,
            opaque
        ))];
        if excluded > 0 {
            let (verb, noun) = if excluded == 1 { ("is", "an operation") } else { ("are", "operations") };
            coverage.push(Inline::text(format!(
                " {excluded} further route{} {verb} not documented as {noun}, and listed at the end with the reason.",
                super::plural(excluded)
            )));
        }
        blocks.push(Block::Callout { tone: "note".into(), title: "Contract coverage".into(), inl: coverage });
        // A column of "none found" down a security-relevant field reads as "these
        // are open". Authentication applied outside the service is not visible
        // here, so say what the absence means before the table says it.
        if !ops.is_empty() && ops.iter().all(|o| o.auth.is_empty()) {
            blocks.push(Block::Callout {
                tone: "warning".into(),
                title: "Authentication not detected".into(),
                inl: vec![Inline::text(
                    "No authentication requirement was recognised on any of these operations. What a gateway, a service mesh, or a shared server package applies before the request arrives is not visible in this service's code, so this is not evidence that the operations are public — check how the service is deployed.",
                )],
            });
        }
        blocks
    }

    fn operations_index(&self, blocks: &mut Vec<Block>, ops: &[&Operation], with_group: bool) {
        let mut rows = Vec::new();
        for op in ops {
            let mut row = vec![
                vec![Inline::code(op.method.clone())],
                vec![Inline::Link {
                    page: self.api_index.op_page.get(&op.id).cloned().unwrap_or_else(|| api_page_id(&op.unit)),
                    anchor: Some(self.api_index.op_anchor(op)),
                    v: match &op.selector {
                        Some(sel) => format!("{} ?{sel}", op.path),
                        None => op.path.clone(),
                    },
                }],
            ];
            if with_group {
                row.push(vec![Inline::text(self.api_index.op_group.get(&op.id).cloned().unwrap_or_default())]);
            }
            row.extend([
                vec![op.request_body.as_ref().map(|t| self.type_inline(t)).unwrap_or_else(|| Inline::text("—"))],
                vec![op.response.as_ref().map(|t| self.type_inline(t)).unwrap_or_else(|| Inline::text("—"))],
                vec![if op.auth.is_empty() {
                    Inline::badge("muted", "none found")
                } else {
                    Inline::badge("observed", op.auth[0].kind.clone())
                }],
                vec![confidence_badge(op.confidence)],
            ]);
            rows.push(row);
        }
        let mut columns = vec!["Method".to_string(), "Path".to_string()];
        if with_group {
            columns.push("Group".into());
        }
        columns.extend(["Request".into(), "Response".into(), "Auth".into(), "Contract".into()]);
        blocks.push(Block::Heading { level: 2, id: "operations".into(), text: "Operations".into() });
        blocks.push(Block::Table { columns, rows });
    }

    fn models_blocks(&mut self, blocks: &mut Vec<Block>, page: &str) {
        let Some(api) = self.api() else { return };
        let mut models: Vec<&Model> =
            api.models.iter().filter(|m| self.api_index.model_page.get(&m.id).is_some_and(|p| p == page)).collect();
        if models.is_empty() {
            return;
        }
        models.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
        blocks.push(Block::Heading { level: 2, id: "models".into(), text: "Models".into() });
        let mut anchors = BTreeSet::new();
        for m in models {
            // Two distinct models can slug alike; the index gives each its own
            // anchor, so this only skips a model documented twice.
            let anchor = self.api_index.model_anchor(&m.name);
            if !anchors.insert(anchor.clone()) {
                continue;
            }
            blocks.push(Block::Heading { level: 3, id: anchor, text: m.name.clone() });
            let mut intro = Vec::new();
            if let Some(doc) = &m.doc {
                intro.push(Inline::text(format!("{doc} ")));
            }
            intro.push(Inline::text("Declared"));
            intro.push(self.cite(&m.evidence));
            blocks.push(Block::Para { inl: intro });
            blocks.push(self.fields_table(m));
        }
    }

    fn excluded_blocks(&mut self, blocks: &mut Vec<Block>, excluded: &[&nunki_analyzer::api::Excluded]) {
        if excluded.is_empty() {
            return;
        }
        blocks.push(Block::Heading { level: 2, id: "excluded".into(), text: "Routes not documented above".into() });
        let mut rows = Vec::new();
        for x in excluded {
            rows.push(vec![
                vec![Inline::code(x.operation.split_once(':').map(|(_, r)| r).unwrap_or(&x.operation).to_string())],
                vec![Inline::text(x.reason.clone())],
                vec![self.cite(&x.evidence)],
            ]);
        }
        blocks.push(Block::Table { columns: vec!["Route".into(), "Why excluded".into(), "Code".into()], rows });
    }

    fn operation_blocks(&mut self, op: &Operation, blocks: &mut Vec<Block>) {
        blocks.push(Block::Heading { level: 2, id: self.api_index.op_anchor(op), text: route_label(op) });
        let mut lead = Vec::new();
        if let Some(s) = &op.summary {
            lead.push(Inline::text(format!("{s} ")));
        }
        lead.push(Inline::text("Handled by "));
        lead.push(Inline::code(op.handler.name.clone()));
        lead.push(self.cite(&op.handler.evidence));
        lead.push(Inline::text(" · "));
        lead.push(confidence_badge(op.confidence));
        if let Some(sel) = &op.selector {
            lead.push(Inline::text(" · selected when the request carries "));
            lead.push(Inline::code(sel.clone()));
        }
        if op.path_partial {
            lead.push(Inline::text(" "));
            lead.push(Inline::badge("warn", "path prefix not resolved"));
        }
        blocks.push(Block::Para { inl: lead });

        if !op.params.is_empty() {
            let mut rows = Vec::new();
            for p in &op.params {
                let mut name = vec![Inline::code(p.name.clone())];
                if let Some(c) = &p.code_name {
                    name.push(Inline::text(format!(" (code: {c})")));
                }
                name.push(self.cite(&p.evidence));
                rows.push(vec![
                    name,
                    vec![Inline::text(p.location.clone())],
                    vec![Inline::code(p.type_name.clone())],
                    vec![Inline::text(if p.required { "yes" } else { "no" })],
                    self.rules_cell(&p.rules),
                ]);
            }
            blocks.push(Block::Table {
                columns: vec!["Parameter".into(), "In".into(), "Type".into(), "Required".into(), "Rules".into()],
                rows,
            });
        }

        let mut io = Vec::new();
        if let Some(t) = &op.request_body {
            io.push(vec![Inline::strong("Request body "), self.type_inline(t)]);
        }
        let status = op.success_status.map(|s| s.to_string()).unwrap_or_else(|| "success".into());
        match &op.response {
            Some(t) => io.push(vec![Inline::strong(format!("Response {status} ")), self.type_inline(t)]),
            None => io
                .push(vec![Inline::strong(format!("Response {status} ")), Inline::badge("muted", "type not declared")]),
        }
        for r in &op.auth {
            let mut inl = vec![Inline::strong("Requires "), Inline::text(format!("{}: {} ", r.kind, r.detail))];
            inl.push(self.cite(&r.evidence));
            io.push(inl);
        }
        blocks.push(Block::List { items: io });

        // Inline field tables for the request body, so the contract reads in one place.
        if let Some(m) = op.request_body.as_ref().and_then(|t| t.model.as_deref()).and_then(|id| self.model(id)) {
            blocks.push(self.fields_table(m));
        }

        if !op.errors.is_empty() {
            let mut rows = Vec::new();
            for e in &op.errors {
                rows.push(vec![
                    vec![Inline::code(e.status.map(|s| s.to_string()).unwrap_or_else(|| "error".into()))],
                    vec![e.message.clone().map(Inline::text).unwrap_or_else(|| Inline::text("—"))],
                    vec![self.cite(&e.evidence)],
                ]);
            }
            blocks.push(Block::Table { columns: vec!["Error".into(), "Message".into(), "Raised at".into()], rows });
        }

        let callers: Vec<_> = self
            .api()
            .map(|a| a.client_calls.iter().filter(|c| c.operation.as_deref() == Some(op.id.as_str())).collect())
            .unwrap_or_default();
        if !callers.is_empty() {
            let mut items = Vec::new();
            for c in callers {
                let mut inl = vec![self.element(&c.unit)];
                if let Some(f) = &c.caller {
                    inl.push(Inline::text(" in "));
                    inl.push(Inline::code(f.clone()));
                }
                inl.push(self.cite(&c.evidence));
                if !c.drift.is_empty() {
                    inl.push(Inline::text(" "));
                    inl.push(Inline::badge("warn", format!("reads undeclared: {}", c.drift.join(", "))));
                }
                items.push(inl);
            }
            blocks.push(Block::Para { inl: vec![Inline::strong("Called by")] });
            blocks.push(Block::List { items });
        }
        if let Some(flow) = self.flow_for(&op.id).filter(|f| worth_drawing(f)) {
            if self.figures.contains_key(&flow_figure_id(flow)) {
                blocks.push(Block::Para {
                    inl: vec![
                        Inline::text("Behaviour: "),
                        Inline::Link {
                            page: "flows".into(),
                            anchor: Some(flow_figure_id(flow)),
                            v: "sequence diagram".into(),
                        },
                    ],
                });
            }
        }
    }

    pub(super) fn fields_table(&mut self, m: &Model) -> Block {
        let mut rows = Vec::new();
        for f in &m.fields {
            let mut name = vec![Inline::code(f.name.clone())];
            if let Some(c) = &f.code_name {
                name.push(Inline::text(format!(" (code: {c})")));
            }
            name.push(self.cite(&f.evidence));
            let ty = match f.model.as_deref().and_then(|id| self.model(id).map(|n| (id, n))) {
                Some((id, nested)) => match self.api_index.model_page.get(id) {
                    Some(page) => Inline::Link {
                        page: page.clone(),
                        anchor: Some(self.api_index.model_anchor(&nested.name)),
                        v: f.type_name.clone(),
                    },
                    None => Inline::code(f.type_name.clone()),
                },
                None => Inline::code(f.type_name.clone()),
            };
            let mut rules = self.rules_cell(&f.rules);
            if let Some(doc) = &f.doc {
                if !rules.is_empty() && rules != vec![Inline::text("—")] {
                    rules.push(Inline::text(" · "));
                } else {
                    rules.clear();
                }
                rules.push(Inline::text(doc.clone()));
            }
            rows.push(vec![name, vec![ty], vec![Inline::text(if f.required { "yes" } else { "no" })], rules]);
        }
        Block::Table {
            columns: vec![format!("{} field", m.name), "Type".into(), "Required".into(), "Rules".into()],
            rows,
        }
    }

    fn rules_cell(&mut self, rules: &[nunki_analyzer::api::Rule]) -> Vec<Inline> {
        if rules.is_empty() {
            return vec![Inline::text("—")];
        }
        let mut out = Vec::new();
        for (i, r) in rules.iter().enumerate() {
            if i > 0 {
                out.push(Inline::text("; "));
            }
            out.push(Inline::text(r.statement.clone()));
            out.push(self.cite(&r.evidence));
        }
        out
    }

    /// Combined access page when several services enforce authentication.
    pub(super) fn access_page(&mut self) {
        let Some(api) = self.api() else { return };
        let units: Vec<String> = self
            .api_index
            .groups
            .keys()
            .filter(|u| api.operations.iter().any(|o| &o.unit == *u && !o.auth.is_empty()))
            .cloned()
            .collect();
        if units.len() < 2 {
            return;
        }
        let mut blocks = vec![Builder::access_intro()];
        for unit in &units {
            let name = self.element_name(unit);
            let groups: Vec<(String, Vec<&Operation>)> = self
                .api_index
                .groups
                .get(unit)
                .map(|gs| {
                    gs.iter()
                        .map(|g| (g.name.clone(), g.ops.iter().filter_map(|id| self.api_operation(id)).collect()))
                        .collect()
                })
                .unwrap_or_default();
            let rows: Vec<Row> = groups
                .iter()
                .map(|(gname, gops)| Row {
                    label: vec![match gops.first() {
                        Some(o) if self.api_index.split.contains(unit) => Inline::Link {
                            page: self.api_index.op_page.get(&o.id).cloned().unwrap_or_else(|| api_page_id(unit)),
                            anchor: None,
                            v: gname.clone(),
                        },
                        _ => Inline::Link { page: api_page_id(unit), anchor: Some("access".into()), v: gname.clone() },
                    }],
                    ops: gops.clone(),
                })
                .collect();
            if let Some(t) = self.access_matrix(&rows) {
                blocks.push(Block::Heading { level: 2, id: format!("access-{}", slug(unit)), text: name });
                blocks.push(t);
            }
        }
        let summary = vec![Inline::text(format!(
            "Who may call what, across the {} services that enforce authentication: roles, scopes and authentication per capability group.",
            units.len()
        ))];
        self.push_page("access", "Access control", "API reference", summary, blocks);
    }
}

/// `GET /api/tenant/devices ?deviceName` — the route, plus the request
/// condition when several handlers share it.
fn route_label(op: &Operation) -> String {
    match &op.selector {
        Some(sel) => format!("{} {} ?{sel}", op.method, op.path, sel = sel),
        None => format!("{} {}", op.method, op.path),
    }
}
