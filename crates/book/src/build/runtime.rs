//! Runtime & deployment: the environments the repository declares (Compose,
//! Kubernetes), what runs in each, how traffic gets in, and what starts first.

use autodoc_analyzer::topology::{draft_topology_ir, Environment};
use autodoc_analyzer::DraftOptions;
use autodoc_validator::ValidateOptions;
use std::collections::BTreeMap;

use super::Builder;
use crate::model::*;

pub(super) fn figure_id(env: &Environment) -> String {
    format!("runtime-{}", autodoc_analyzer::scan::slug(&env.id))
}

impl<'a> Builder<'a> {
    fn environments(&self) -> &'a [Environment] {
        self.report.topology.as_ref().map(|t| t.environments.as_slice()).unwrap_or(&[])
    }

    pub(super) fn add_runtime_figures(
        &mut self,
        curated: &BTreeMap<String, String>,
        validate: &ValidateOptions,
        draft: &DraftOptions,
    ) {
        let report = self.report;
        for env in self.environments() {
            let d = draft_topology_ir(report, env, draft);
            // Workloads that reach nothing and are reached by nothing are an
            // inventory, and the table below already lists them with their
            // image, ports and configuration. A figure earns its place when
            // something connects — a port published, a dependency, a link.
            if d.ir.nodes.len() >= 2 && !d.ir.edges.is_empty() {
                self.add_figure(&figure_id(env), d, curated, validate);
            }
        }
    }

    pub(super) fn runtime_page(&mut self) {
        let envs = self.environments();
        if envs.is_empty() {
            return;
        }
        let mut blocks = Vec::new();
        let workloads: usize = envs.iter().map(|e| e.workloads.len()).sum();
        blocks.push(Block::Para {
            inl: vec![Inline::text(format!(
                "{} environment{} declared in the repository, running {} workload{}. Everything here is read from the deployment files; nothing is inferred about hosts or clusters they don't describe.",
                envs.len(),
                super::plural(envs.len()),
                workloads,
                super::plural(workloads)
            ))],
        });
        if let Some(notes) = self.report.topology.as_ref().map(|t| &t.notes).filter(|n| !n.is_empty()) {
            blocks.push(Block::Callout {
                tone: "warning".into(),
                title: "Not read".into(),
                inl: vec![Inline::text(notes.join("; "))],
            });
        }
        for env in envs {
            blocks.push(Block::Heading { level: 2, id: figure_id(env), text: env.name.clone() });
            let mut intro = vec![Inline::text("Defined in ")];
            for (i, f) in env.files.iter().take(6).enumerate() {
                if i > 0 {
                    intro.push(Inline::text(", "));
                }
                intro.push(Inline::code(f.clone()));
            }
            if env.files.len() > 6 {
                intro.push(Inline::text(format!(" and {} more", env.files.len() - 6)));
            }
            intro.push(Inline::text("."));
            blocks.push(Block::Para { inl: intro });
            let fig = figure_id(env);
            let entries = env.links.iter().filter(|l| l.from == "external").count();
            if let Some(f) = self.figure(
                &fig,
                vec![Inline::text(format!(
                    "{} workloads · {} entry point{} · dashed: starts after · connections from environment variables",
                    env.workloads.len(),
                    entries,
                    super::plural(entries)
                ))],
            ) {
                blocks.push(f);
            }

            let mut rows = Vec::new();
            for w in &env.workloads {
                let mut name = vec![Inline::strong(w.name.clone()), Inline::text(" ")];
                name.push(self.cite(&w.evidence));
                let runs = match (&w.unit, &w.infra) {
                    (Some(u), _) => vec![self.element(u)],
                    (None, Some(i)) => vec![self.element(i)],
                    (None, None) => vec![Inline::badge("muted", "external image")],
                };
                let mut source = Vec::new();
                if let Some(b) = &w.build {
                    source.push(Inline::text("build "));
                    source.push(Inline::code(if b.is_empty() { ".".to_string() } else { format!("{b}/") }));
                }
                if let Some(i) = &w.image {
                    if !source.is_empty() {
                        source.push(Inline::text(" · "));
                    }
                    source.push(Inline::code(i.clone()));
                }
                let ports: Vec<String> = w
                    .ports
                    .iter()
                    .map(|p| match (&p.published, p.external) {
                        (Some(h), true) => format!("{h}→{} (public)", p.target),
                        (Some(h), false) => format!("{h}→{}", p.target),
                        (None, _) => p.target.clone(),
                    })
                    .collect();
                let mut ops = Vec::new();
                if let Some(r) = &w.replicas {
                    ops.push(format!("replicas {r}"));
                }
                if let Some(s) = &w.schedule {
                    ops.push(format!("schedule {s}"));
                }
                if w.health_check {
                    ops.push("health check".into());
                }
                if let Some(r) = &w.resources {
                    ops.push(format!("limits {r}"));
                }
                if !w.volumes.is_empty() {
                    ops.push(format!("volumes {}", w.volumes.join(", ")));
                }
                if !w.profiles.is_empty() {
                    ops.push(format!("profiles {}", w.profiles.join(", ")));
                }
                match (w.env_vars, w.env_files.len()) {
                    (0, 0) => {}
                    (v, 0) => ops.push(format!("{v} env variable{}", super::plural(v))),
                    (0, f) => ops.push(format!("{f} env file{}", super::plural(f))),
                    (v, f) => ops.push(format!("{v} env variables, {f} env file{}", super::plural(f))),
                }
                rows.push(vec![
                    name,
                    runs,
                    if source.is_empty() { vec![Inline::text("—")] } else { source },
                    vec![Inline::text(if ports.is_empty() { "—".to_string() } else { ports.join(", ") })],
                    vec![Inline::text(if ops.is_empty() { "—".to_string() } else { ops.join("; ") })],
                ]);
            }
            blocks.push(Block::Table {
                columns: vec![
                    "Workload".into(),
                    "Runs".into(),
                    "Image / build".into(),
                    "Ports".into(),
                    "Operations & configuration".into(),
                ],
                rows,
            });

            let routes: Vec<_> = env.links.iter().filter(|l| l.from == "external").collect();
            if !routes.is_empty() {
                blocks.push(Block::Heading {
                    level: 3,
                    id: format!("{fig}-entry"),
                    text: "How traffic gets in".into(),
                });
                let mut rows = Vec::new();
                for l in routes {
                    rows.push(vec![
                        vec![Inline::code(l.label.clone())],
                        vec![Inline::strong(l.to.clone())],
                        vec![Inline::badge("muted", l.kind.replace('-', " "))],
                        vec![self.cite(&l.evidence)],
                    ]);
                }
                blocks.push(Block::Table {
                    columns: vec!["Route / port".into(), "Workload".into(), "Kind".into(), "Declared".into()],
                    rows,
                });
            }
            if !env.overlays.is_empty() {
                blocks.push(Block::Heading { level: 3, id: format!("{fig}-overlays"), text: "Variants".into() });
                blocks.push(Block::Para {
                    inl: vec![Inline::text(
                        "Override files layered on the base with `-f`; each adds services or changes existing ones.",
                    )],
                });
                let rows = env
                    .overlays
                    .iter()
                    .map(|o| {
                        vec![
                            vec![Inline::code(o.file.clone())],
                            vec![Inline::text(if o.adds.is_empty() { "—".into() } else { o.adds.join(", ") })],
                            vec![Inline::text(if o.changes.is_empty() { "—".into() } else { o.changes.join(", ") })],
                        ]
                    })
                    .collect();
                blocks.push(Block::Table { columns: vec!["File".into(), "Adds".into(), "Changes".into()], rows });
            }
        }
        let summary = vec![Inline::text(
            "Where the system runs: each declared environment, its workloads, ports, replicas and health checks, and how traffic enters.",
        )];
        self.push_page("runtime", "Runtime & deployment", "Runtime", summary, blocks);
    }
}
