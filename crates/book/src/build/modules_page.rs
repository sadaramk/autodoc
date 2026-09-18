//! Modules of a deployable. For modular monoliths (Spring Modulith, or any
//! unit whose modules expose APIs and exchange events) the section documents
//! each module's contract — public types, events published and handled, what
//! it depends on — and every import that crosses another module's boundary.
//! Units with many modules get a dedicated page, `containers/<unit>/modules`.

use autodoc_analyzer::scan::{ComponentView, ModuleSummary, UnitSummary};

use super::Builder;
use crate::model::*;

/// Modules beyond which the detailed table moves to its own page.
const MODULES_ON_PAGE: usize = 12;

pub(super) fn modules_page_id(unit: &str) -> String {
    format!("containers/{unit}/modules")
}

fn is_modular(view: &ComponentView) -> bool {
    view.boundaries_declared
        || !view.violations.is_empty()
        || view.modules.iter().any(|m| !m.publishes.is_empty() || !m.consumes.is_empty())
}

impl<'a> Builder<'a> {
    /// The "Modules" section of a container page. Pushes the dedicated modules
    /// page when the unit is modular and large.
    pub(super) fn modules_section(&mut self, u: &UnitSummary, view: &ComponentView) -> Vec<Block> {
        if view.modules.is_empty() {
            return vec![];
        }
        let mut blocks = vec![Block::Heading { level: 2, id: "modules".into(), text: "Modules".into() }];
        let modular = is_modular(view) && view.modules.len() >= 2;
        if !modular {
            blocks.push(self.plain_modules_table(view));
            return blocks;
        }
        if view.modules.len() > MODULES_ON_PAGE {
            blocks.push(Block::Para {
                inl: vec![
                    Inline::text(format!(
                        "{} modules with declared boundaries. Their public types, events and dependencies are on ",
                        view.modules.len()
                    )),
                    Inline::link(modules_page_id(&u.id), format!("{} modules", u.name)),
                    Inline::text("."),
                ],
            });
            blocks.push(self.boundary_callout(view));
            return blocks;
        }
        blocks.extend(self.module_detail_blocks(u, view, 3));
        blocks
    }

    /// The dedicated modules page, for modular units with many modules. Push it
    /// after the container page.
    pub(super) fn modules_page(&mut self, u: &UnitSummary, view: &ComponentView) {
        if !(is_modular(view) && view.modules.len() > MODULES_ON_PAGE) {
            return;
        }
        let detail = self.module_detail_blocks(u, view, 2);
        let summary = vec![Inline::text(format!(
            "The {} modules of {}: what each exposes, the events it publishes and handles, and imports that cross a module boundary.",
            view.modules.len(),
            u.name
        ))];
        self.push_page(&modules_page_id(&u.id), format!("{} modules", u.name), "Container", summary, detail);
    }

    fn module_detail_blocks(&mut self, u: &UnitSummary, view: &ComponentView, level: u8) -> Vec<Block> {
        let mut blocks = Vec::new();
        if level == 2 {
            if let Some(fig) = self.figure(
                &format!("components-{}", u.id),
                vec![Inline::text("Modules, their imports, and the events between them.")],
            ) {
                blocks.push(fig);
            }
        }
        blocks.push(Block::Para {
            inl: vec![Inline::text(
                "A module's public API is the public types in its root package and in any named interface; its sub-packages are internal. Events are application events it publishes or handles.",
            )],
        });
        let mut modules: Vec<&ModuleSummary> = view.modules.iter().collect();
        modules.sort_by_key(|m| (!m.is_entry, m.name.clone()));
        let mut rows = Vec::new();
        for m in modules {
            let mut name = vec![Inline::strong(m.name.clone())];
            if m.is_entry {
                name.push(Inline::text(" "));
                name.push(Inline::badge("muted", "entry"));
            }
            if let Some(ev) = &m.evidence {
                name.push(self.cite(ev));
            }
            let mut api = Vec::new();
            for (i, t) in m.public_types.iter().enumerate() {
                if i > 0 {
                    api.push(Inline::text(", "));
                }
                api.push(Inline::code(t.name.clone()));
                api.push(self.cite(&t.evidence));
            }
            if api.is_empty() {
                api.push(Inline::text("—"));
            }
            let events = |list: &[String]| -> Vec<Inline> {
                if list.is_empty() {
                    return vec![Inline::text("—")];
                }
                let mut out = Vec::new();
                for (i, e) in list.iter().enumerate() {
                    if i > 0 {
                        out.push(Inline::text(", "));
                    }
                    out.push(Inline::code(e.clone()));
                }
                out
            };
            let deps: Vec<String> = view
                .dependencies
                .iter()
                .filter(|d| d.source == m.id)
                .filter_map(|d| view.modules.iter().find(|x| x.id == d.target))
                .map(|x| x.name.clone())
                .collect();
            let mut depends = Vec::new();
            for (i, d) in deps.iter().enumerate() {
                if i > 0 {
                    depends.push(Inline::text(", "));
                }
                depends.push(Inline::text(d.clone()));
            }
            if depends.is_empty() {
                depends.push(Inline::text("—"));
            }
            rows.push(vec![
                name,
                vec![Inline::text(m.doc.clone().unwrap_or_default())],
                api,
                events(&m.publishes),
                events(&m.consumes),
                depends,
            ]);
        }
        blocks.push(Block::Table {
            columns: vec![
                "Module".into(),
                "Responsibility".into(),
                "Public API".into(),
                "Publishes".into(),
                "Handles".into(),
                "Depends on".into(),
            ],
            rows,
        });
        blocks.push(Block::Heading { level, id: "boundary-violations".into(), text: "Boundary violations".into() });
        blocks.push(self.boundary_callout(view));
        if !view.violations.is_empty() {
            let mut rows = Vec::new();
            for v in &view.violations {
                let name = |id: &str| {
                    view.modules.iter().find(|m| m.id == id).map(|m| m.name.clone()).unwrap_or_else(|| id.to_string())
                };
                rows.push(vec![
                    vec![Inline::strong(name(&v.from))],
                    vec![Inline::strong(name(&v.to))],
                    vec![Inline::badge(
                        "warn",
                        if v.kind == "internal" { "uses internal type" } else { "dependency not allowed" },
                    )],
                    vec![Inline::code(v.specifier.clone())],
                    vec![self.cite(&v.evidence)],
                ]);
            }
            blocks.push(Block::Table {
                columns: vec!["From".into(), "Into".into(), "Kind".into(), "Reference".into(), "Code".into()],
                rows,
            });
        }
        blocks
    }

    fn boundary_callout(&self, view: &ComponentView) -> Block {
        if view.violations.is_empty() {
            Block::Callout {
                tone: "note".into(),
                title: "No boundary violations found".into(),
                inl: vec![Inline::text(if view.boundaries_declared {
                    "No module imports another module's internal packages or depends on a module its declaration doesn't allow."
                } else {
                    "No module imports another module's internal packages. Boundaries aren't declared, so allowed dependencies weren't checked."
                })],
            }
        } else {
            Block::Callout {
                tone: "warning".into(),
                title: format!(
                    "{} boundary violation{}",
                    view.violations.len(),
                    if view.violations.len() == 1 { "" } else { "s" }
                ),
                inl: vec![Inline::text(
                    "Imports reaching into another module's internal packages, or depending on a module outside its allowed dependencies.",
                )],
            }
        }
    }

    /// The plain modules table for units that aren't modular monoliths.
    fn plain_modules_table(&mut self, view: &ComponentView) -> Block {
        let mut rows = Vec::new();
        let mut modules: Vec<_> = view.modules.iter().collect();
        modules.sort_by_key(|m| (!m.is_entry, std::cmp::Reverse(m.symbols), m.id.clone()));
        for m in modules.into_iter().take(24) {
            let ev = m.evidence.as_ref().map(|e| vec![self.cite(e)]).unwrap_or_default();
            let mut name = vec![Inline::strong(m.name.clone())];
            if m.is_entry {
                name.push(Inline::text(" "));
                name.push(Inline::badge("muted", "entry"));
            }
            rows.push(vec![
                name,
                vec![Inline::code(m.path.clone())],
                vec![Inline::text(m.doc.clone().unwrap_or_default())],
                vec![Inline::text(m.symbols.to_string())],
                ev,
            ]);
        }
        Block::Table {
            columns: vec!["Module".into(), "Path".into(), "Responsibility".into(), "Symbols".into(), "Evidence".into()],
            rows,
        }
    }
}
