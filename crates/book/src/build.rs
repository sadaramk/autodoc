//! Scan → book. The information architecture is fixed and derived from the
//! scan, so every repository gets the same sections in the same order, and
//! two runs on the same commit produce the same book.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use nunki_analyzer::catalog::InfraCategory;
use nunki_analyzer::scan::{EvidenceRef, InfraSummary, RelationSource, Relationship, UnitSummary};
use nunki_analyzer::{draft_component_ir, draft_ir, Depth, DraftOptions, ScanOptions, ScanReport, UnitKind};
use nunki_git::{EvidenceQuery, EvidenceState, RepoContext};
use nunki_ir::{DiagramIR, EdgeType, Theme};
use nunki_renderer::{Accent, RenderOptions};
use nunki_validator::ValidateOptions;

use crate::authored::Authored;
use crate::model::*;

mod access;
mod api_ref;
mod behavior;
mod data_model;
mod flows;
mod modules_page;
mod runtime;

#[derive(Debug, Clone)]
pub struct BookOptions {
    pub theme: Theme,
    pub accent: Accent,
    pub include_tests: bool,
    pub max_density: f64,
    /// Write the book even when almost nothing could be read.
    pub allow_partial: bool,
}

impl Default for BookOptions {
    fn default() -> Self {
        BookOptions {
            theme: Theme::EditorialLight,
            accent: Accent::Indigo,
            include_tests: false,
            max_density: 0.40,
            allow_partial: false,
        }
    }
}

/// A diagram as it will be written: IR (possibly hand-edited) plus renders.
#[derive(Debug, Clone)]
pub struct BuiltDiagram {
    pub id: String,
    pub ir: DiagramIR,
    pub curated: bool,
    pub notes: Vec<String>,
    pub standalone_svg: String,
}

pub struct Built {
    pub book: Book,
    pub diagrams: Vec<BuiltDiagram>,
    pub report: ScanReport,
    pub warnings: Vec<String>,
    /// Starter `authored.json` (written only when none exists).
    pub authored_template: String,
    /// Authored intents naming an operation this build did not find.
    pub authored_orphans: Vec<String>,
    /// Authored intents with prose but no evidence pin. Not an error — the
    /// file is the human's — but the book should be able to say how much of
    /// its authored prose is checkable.
    pub authored_unpinned: Vec<String>,
    /// The behaviour model as data, for everything that cannot read prose.
    pub behaviour: crate::behaviour::Behaviour,
}

const SNIPPET_LINES: u32 = 40;
const MAX_INDEXED_CITES: usize = 400;
const LIBRARY_LINK_LIMIT: usize = 25;

/// `curated` maps diagram id → IR text found on disk that differs from what
/// the previous run generated; those are rendered as-is instead of redrafted.
/// The book's own directory, relative to the repository root, when it sits
/// inside it — the part of the tree the documentation does not describe.
fn book_dir_in_repo(ctx: &nunki_git::RepoContext, out_dir: &Path) -> Option<String> {
    let top = ctx.git_root.as_ref()?;
    let out = out_dir.canonicalize().unwrap_or_else(|_| out_dir.to_path_buf());
    let rel = out.strip_prefix(top).ok()?;
    Some(rel.to_string_lossy().replace('\\', "/")).filter(|r| !r.is_empty())
}

pub fn build(
    repo: &Path,
    out_dir: &Path,
    opts: &BookOptions,
    curated: &BTreeMap<String, String>,
) -> Result<Built, crate::BookError> {
    let report = nunki_analyzer::scan(
        repo,
        &ScanOptions {
            depth: Depth::Container,
            include_tests: opts.include_tests,
            all_components: true,
            behavior: true,
            allow_partial: opts.allow_partial,
            ignore_dirs: vec![out_dir.to_path_buf()],
            ..Default::default()
        },
    )?;
    let root = PathBuf::from(&report.repo.root);
    // One git snapshot for every citation and every figure in this build.
    let cache = std::sync::Arc::new(nunki_git::EvidenceCache::default());
    let mut ctx = cache.context(&root);
    // The book records the commit it describes, not HEAD. Committing the book
    // moves HEAD, and pinning to that would leave the documentation one commit
    // behind itself for ever: `check` would fail however often it was
    // regenerated, which is exactly the loop CI runs.
    ctx.head_commit = nunki_git::commit_describing(&ctx, book_dir_in_repo(&ctx, out_dir));
    let ctx = ctx;
    let commit_date = ctx.head_commit.as_deref().and_then(|c| nunki_git::commit_date(&ctx, c));
    let draft_opts = DraftOptions {
        theme: opts.theme,
        // Pinned to the commit so regeneration is byte-identical; outside git
        // there is no stable clock to pin to.
        generated_at: Some(commit_date.clone().unwrap_or_else(|| "unversioned".to_string())),
    };
    let validate_opts = ValidateOptions {
        repo_root: Some(root.clone()),
        verify_evidence: true,
        max_density: opts.max_density,
        strict: false,
        evidence_cache: Some(cache.clone()),
    };

    let (authored, authored_warning) = Authored::load(out_dir);
    // Read, never written by a build: a release records an entry, and a
    // rebuild of an old book must not rewrite what it said.
    let (history, history_warning) = crate::history::History::load(out_dir);
    // Kept for the post-build audit: the builder consumes its copy.
    let authored_for_audit = authored.clone();
    let mut b = Builder {
        authored,
        history,
        report: &report,
        ctx: &ctx,
        verifier: nunki_git::Verifier::cached(&ctx, None, Some(&cache)),
        cites: BTreeMap::new(),
        cite_keys: BTreeMap::new(),
        pages: Vec::new(),
        figures: BTreeMap::new(),
        diagrams: Vec::new(),
        warnings: authored_warning.into_iter().chain(history_warning).collect(),
        opts,
        api_index: api_ref::ApiIndex::new(&report),
    };

    // ── Figures ──────────────────────────────────────────────────────────────
    let mut system = report.clone();
    system.depth = Depth::System;
    b.add_figure("system-context", draft_ir(&system, &draft_opts), curated, &validate_opts);
    b.add_figure("containers", draft_ir(&report, &draft_opts), curated, &validate_opts);
    for u in b.deployables() {
        if let Some(view) = report.component_views.iter().find(|v| v.unit == u.id) {
            let draft = draft_component_ir(&report, view, &draft_opts);
            if draft.ir.nodes.len() >= 2 {
                b.add_figure(&format!("components-{}", u.id), draft, curated, &validate_opts);
            }
        }
    }
    b.add_behavior_figures(curated, &validate_opts, &draft_opts);
    b.add_capability_figures(curated, &validate_opts, &draft_opts);
    b.add_runtime_figures(curated, &validate_opts, &draft_opts);

    // ── Pages (evidence last: it indexes every citation the others made) ─────
    b.overview_page();
    b.architecture_page();
    let deployables: Vec<UnitSummary> = b.deployables().into_iter().cloned().collect();
    for u in &deployables {
        b.container_page(u);
    }
    b.data_page();
    b.data_domain_pages();
    b.flows_page();
    b.runtime_page();
    for unit in b.api_units() {
        b.api_pages(&unit);
    }
    b.access_page();
    b.functional_page();
    b.requirements_page();
    // Figure nodes register citations of their own, so they have to be linked
    // before the evidence page counts and indexes them — otherwise the page
    // reports fewer citations than the book does, and a stale one among them
    // never reaches the page's warning.
    b.link_figure_nodes();
    b.history_page();
    b.evidence_page();

    let nav = b.nav(&deployables);
    let health = b.health();
    let meta = BookMeta {
        name: report.system.name.clone(),
        repo: report.repo.name.clone(),
        description: report.system.description.clone(),
        commit: ctx.head_commit.clone(),
        commit_date,
        branch: ctx.branch.clone(),
        release: ctx.tag.clone(),
        web_url: ctx.remote_url.as_deref().and_then(nunki_git::web_base),
        path_prefix: if ctx.prefix.is_empty() { String::new() } else { format!("{}/", ctx.prefix.trim_matches('/')) },
        files: report.stats.files,
        lines: report.stats.lines,
        languages: report.stats.languages.iter().map(|(l, s)| (l.clone(), s.files)).collect(),
        evidence: health,
        generator: nunki_renderer::GENERATOR.to_string(),
    };
    // Assembled before the builder is taken apart: it reads the same two
    // layers the functional page does.
    let behaviour = b.behaviour_export();
    let Builder { pages, figures, cites, diagrams, warnings, .. } = b;
    let operation_ids: Vec<String> =
        report.api.as_ref().map(|a| a.operations.iter().map(|o| o.id.clone()).collect()).unwrap_or_default();
    let authored_orphans = authored_for_audit.orphans(&operation_ids);
    let authored_unpinned: Vec<String> = authored_for_audit
        .operations
        .iter()
        .filter(|(id, i)| i.says_something() && i.evidence.is_empty() && operation_ids.contains(id))
        .map(|(id, _)| id.clone())
        .collect();
    Ok(Built {
        book: Book { meta, nav, pages, diagrams: figures, cites },
        diagrams,
        warnings,
        authored_template: Authored::template(&operation_ids),
        authored_orphans,
        authored_unpinned,
        behaviour,
        report,
    })
}

struct Builder<'a> {
    authored: Authored,
    history: crate::history::History,
    report: &'a ScanReport,
    ctx: &'a RepoContext,
    verifier: nunki_git::Verifier<'a>,
    cites: BTreeMap<String, Cite>,
    cite_keys: BTreeMap<(String, u32, u32), String>,
    pages: Vec<Page>,
    figures: BTreeMap<String, Figure>,
    diagrams: Vec<BuiltDiagram>,
    warnings: Vec<String>,
    opts: &'a BookOptions,
    /// Where each operation and model is documented (API pages may be split by capability group).
    api_index: api_ref::ApiIndex,
}

fn slug_md(id: &str) -> String {
    id.replace('/', "-")
}

impl<'a> Builder<'a> {
    // ── lookups ──────────────────────────────────────────────────────────────

    fn deployables(&self) -> Vec<&'a UnitSummary> {
        let runnable: Vec<&UnitSummary> =
            self.report.containers.iter().filter(|u| u.kind != UnitKind::Library).collect();
        if !runnable.is_empty() {
            return runnable;
        }
        // A library-only repository: its largest packages stand in.
        let mut libs: Vec<&UnitSummary> = self.report.containers.iter().collect();
        libs.sort_by_key(|u| (std::cmp::Reverse(u.lines), u.id.clone()));
        libs.truncate(8);
        libs.sort_by_key(|u| u.id.clone());
        libs
    }

    fn unit(&self, id: &str) -> Option<&'a UnitSummary> {
        self.report.containers.iter().find(|u| u.id == id)
    }

    fn infra(&self, id: &str) -> Option<&'a InfraSummary> {
        self.report.infrastructure.iter().find(|i| i.id == id)
    }

    fn has_page(&self, id: &str) -> bool {
        self.deployables().iter().any(|u| u.id == id)
    }

    /// Unit name, qualified by its parent directory when another unit shares it.
    fn unit_name(&self, u: &UnitSummary) -> String {
        let clash = self.report.containers.iter().filter(|o| o.name == u.name).count() > 1;
        match u.root.rsplit_once('/') {
            Some((parent, _)) if clash => format!("{} ({})", u.name, parent.rsplit('/').next().unwrap_or(parent)),
            _ => u.name.clone(),
        }
    }

    /// A name that links to the element's page when it has one.
    fn element(&self, id: &str) -> Inline {
        if self.has_page(id) {
            let name = self.unit(id).map(|u| self.unit_name(u)).unwrap_or_else(|| id.to_string());
            return Inline::link(format!("containers/{id}"), name);
        }
        // Infrastructure first: a directory of database init scripts can share the store's id.
        match (self.infra(id), self.unit(id)) {
            (Some(i), _) => Inline::strong(i.label.clone()),
            (_, Some(u)) => Inline::strong(self.unit_name(u)),
            _ => Inline::text(id),
        }
    }

    fn element_name(&self, id: &str) -> String {
        self.infra(id)
            .map(|i| i.label.clone())
            .or_else(|| self.unit(id).map(|u| u.name.clone()))
            .unwrap_or_else(|| id.into())
    }

    fn observed(r: &Relationship) -> bool {
        r.sources.contains(&RelationSource::Code)
    }

    fn basis(r: &Relationship) -> Inline {
        if Self::observed(r) {
            Inline::badge("observed", "observed in code")
        } else if r.sources.contains(&RelationSource::Config) {
            Inline::badge("declared", "declared in config")
        } else if r.sources.contains(&RelationSource::Compose) {
            Inline::badge("declared", "declared in compose")
        } else {
            Inline::badge("declared", "declared in manifest")
        }
    }

    fn interaction(r: &Relationship) -> String {
        let kind = match r.edge_type {
            EdgeType::Sync => "calls",
            EdgeType::Async => "async",
            EdgeType::Event => "event",
            EdgeType::Read => "reads",
            EdgeType::Write => "writes",
        };
        if r.label.is_empty() || r.label == kind {
            kind.to_string()
        } else {
            r.label.clone()
        }
    }

    // ── citations ────────────────────────────────────────────────────────────

    /// Registers evidence once (by file + range) and verifies it against the
    /// working tree and HEAD.
    fn cite(&mut self, e: &EvidenceRef) -> Inline {
        let key = (e.file_path.clone(), e.start_line, e.end_line);
        if let Some(id) = self.cite_keys.get(&key) {
            return Inline::Cite { id: id.clone() };
        }
        let id = format!("c{}", self.cite_keys.len() + 1);
        let q = EvidenceQuery {
            file_path: e.file_path.clone(),
            line: e.start_line,
            end_line: Some(e.end_line),
            symbol_name: e.symbol_name.clone(),
        };
        let r = self.verifier.verify(&q);
        let state = serde_json::to_value(r.state).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
        let anchor = if e.start_line == e.end_line {
            format!("#L{}", e.start_line)
        } else {
            format!("#L{}-L{}", e.start_line, e.end_line)
        };
        let snippet = matches!(r.state, EvidenceState::Verified | EvidenceState::Stale | EvidenceState::Untracked)
            .then(|| nunki_git::read_snippet(&self.ctx.root, &e.file_path, e.start_line, e.end_line, SNIPPET_LINES))
            .flatten();
        // What a reader can rebuild from the book's repository, commit and path
        // prefix isn't stored: on a large book these three fields are ~40% of it.
        let prefix =
            if self.ctx.prefix.is_empty() { String::new() } else { format!("{}/", self.ctx.prefix.trim_matches('/')) };
        let derived_permalink =
            match (self.ctx.remote_url.as_deref().and_then(nunki_git::web_base), self.ctx.head_commit.as_deref()) {
                (Some(base), Some(commit)) => Some(format!("{base}/blob/{commit}/{prefix}{}{anchor}", e.file_path)),
                _ => None,
            };
        let derived_ref = derived_permalink.clone().unwrap_or_else(|| format!("{}{anchor}", e.file_path));
        let git_ref = r.permalink.clone().unwrap_or_else(|| format!("{}{anchor}", e.file_path));
        self.cites.insert(
            id.clone(),
            Cite {
                id: id.clone(),
                file: e.file_path.clone(),
                start: e.start_line,
                end: e.end_line,
                symbol: e.symbol_name.clone(),
                state,
                // A verified citation's detail repeats the state; in a large book
                // that sentence, the permalink and the reference are ~40% of the payload.
                detail: (r.state != EvidenceState::Verified).then(|| r.detail.clone()),
                snippet,
                permalink: r
                    .permalink
                    .clone()
                    .filter(|p| p.starts_with("https://") && Some(p) != derived_permalink.as_ref()),
                git_ref: Some(git_ref).filter(|g| *g != derived_ref),
            },
        );
        self.cite_keys.insert(key, id.clone());
        Inline::Cite { id }
    }

    fn cites(&mut self, evidence: &[EvidenceRef], max: usize) -> Vec<Inline> {
        let mut out: Vec<Inline> = Vec::new();
        for e in evidence {
            if out.len() == max {
                break;
            }
            let c = self.cite(e);
            if !out.contains(&c) {
                out.push(c);
            }
        }
        out
    }

    fn health(&self) -> EvidenceHealth {
        let mut h = EvidenceHealth { total: self.cites.len(), ..Default::default() };
        for c in self.cites.values() {
            match c.state.as_str() {
                "verified" => h.verified += 1,
                "stale" | "untracked" => h.stale += 1,
                "unverified" => h.unverified += 1,
                _ => h.broken += 1,
            }
        }
        h
    }

    // ── figures ──────────────────────────────────────────────────────────────

    fn add_figure(
        &mut self,
        id: &str,
        draft: nunki_analyzer::Draft,
        curated: &BTreeMap<String, String>,
        validate: &ValidateOptions,
    ) {
        let (mut ir, mut notes, is_curated) = match curated.get(id).map(|text| nunki_ir::parse_ir(text)) {
            Some(Ok(ir)) => (ir, vec![format!("rendered hand-edited `{id}.ir.json`")], true),
            Some(Err(e)) => {
                self.warnings.push(format!("ignored hand-edited `{id}.ir.json`: {e}"));
                (draft.ir, draft.notes, false)
            }
            None => (draft.ir, draft.notes, false),
        };
        let (report, healed) = nunki_validator::heal_evidence(&mut ir, validate);
        notes.extend(healed);
        if !report.valid {
            let codes: Vec<&str> = report
                .diagnostics
                .iter()
                .filter(|d| d.severity == nunki_validator::Severity::Error)
                .map(|d| d.code.as_str())
                .collect();
            self.warnings.push(format!("figure `{id}` has validation errors [{}]; rendered anyway", codes.join(", ")));
        }
        if ir.nodes.is_empty() {
            return;
        }
        let footer = format!(
            "{}{}",
            self.report.repo.name,
            self.ctx.head_commit.as_deref().map(|c| format!("@{}", &c[..c.len().min(8)])).unwrap_or_default()
        );
        let render_opts =
            RenderOptions { accent: self.opts.accent.clone(), evidence: BTreeMap::new(), footer: Some(footer) };
        let embedded = nunki_renderer::render_embedded_svg(&ir, &render_opts, &format!("fig-{id}"));
        let standalone = nunki_renderer::render_svg(&ir, &render_opts);
        self.figures.insert(
            id.to_string(),
            Figure {
                id: id.to_string(),
                title: ir.title.clone(),
                svg: embedded.content,
                width: embedded.layout.width,
                height: embedded.layout.height,
                nodes: BTreeMap::new(),
                ir_path: format!("diagrams/{id}.ir.json"),
                svg_path: format!("diagrams/{id}.svg"),
                density: nunki_ir::visual_density(ir.nodes.len(), ir.edges.len()),
            },
        );
        self.diagrams.push(BuiltDiagram {
            id: id.to_string(),
            ir,
            curated: is_curated,
            notes,
            standalone_svg: standalone.content,
        });
    }

    /// Clicking a node opens its page when there is one, otherwise its evidence.
    fn link_figure_nodes(&mut self) {
        let diagrams = self.diagrams.clone();
        for d in &diagrams {
            let mut map = BTreeMap::new();
            for n in &d.ir.nodes {
                let page = self.has_page(&n.id).then(|| format!("containers/{}", n.id));
                let cite = n.evidence.as_ref().map(|e| {
                    let ev = EvidenceRef {
                        file_path: e.file_path.clone(),
                        start_line: e.start_line,
                        end_line: e.end_line,
                        symbol_name: e.symbol_name.clone(),
                        note: None,
                    };
                    match self.cite(&ev) {
                        Inline::Cite { id } => id,
                        _ => unreachable!(),
                    }
                });
                map.insert(n.id.clone(), NodeTarget { page, cite });
            }
            if let Some(f) = self.figures.get_mut(&d.id) {
                f.nodes = map;
            }
        }
    }

    fn figure(&self, id: &str, caption: Vec<Inline>) -> Option<Block> {
        self.figures.contains_key(id).then(|| Block::Figure { diagram: id.to_string(), caption })
    }

    fn push_page(
        &mut self,
        id: &str,
        title: impl Into<String>,
        section: &str,
        summary: Vec<Inline>,
        blocks: Vec<Block>,
    ) {
        let md_path = match id {
            "overview" => "pages/01-overview.md".to_string(),
            "architecture" => "pages/02-architecture.md".to_string(),
            "data" => "pages/04-data-and-integrations.md".to_string(),
            domain if domain.starts_with("data/") => format!("pages/04-data--{}.md", slug_md(&domain[5..])),
            "flows" => "pages/05-critical-flows.md".to_string(),
            "runtime" => "pages/05-runtime-and-deployment.md".to_string(),
            "functional" => "pages/07-functional-specification.md".to_string(),
            "rules" => "pages/07-business-rules.md".to_string(),
            r if r.starts_with("rules/") => format!("pages/07-business-rules--{}.md", &r["rules/".len()..]),
            "access" => "pages/06-access-control.md".to_string(),
            f if f.starts_with("functional/") => {
                format!("pages/07-functional-{}.md", f["functional/".len()..].replace('/', "--"))
            }
            api if api.starts_with("api/") && api.matches('/').count() == 2 => {
                let (unit, group) = api["api/".len()..].split_once('/').unwrap();
                format!("pages/06-api-{unit}--{group}.md")
            }
            "requirements" => "pages/08-business-requirements.md".to_string(),
            "evidence" => "pages/09-evidence-and-unknowns.md".to_string(),
            "history" => "pages/09-architecture-history.md".to_string(),
            api if api.starts_with("api/") => format!("pages/06-{}.md", slug_md(api)),
            other => format!("pages/03-{}.md", slug_md(other)),
        };
        self.pages.push(Page { id: id.into(), title: title.into(), section: section.into(), summary, blocks, md_path });
    }

    // ── pages ────────────────────────────────────────────────────────────────

    fn overview_page(&mut self) {
        let r = self.report;
        let deployables = self.deployables();
        let stores: Vec<&InfraSummary> =
            r.infrastructure.iter().filter(|i| i.category == InfraCategory::Storage).collect();
        let buses: Vec<&InfraSummary> =
            r.infrastructure.iter().filter(|i| i.category == InfraCategory::EventBus).collect();
        let vendors: Vec<&InfraSummary> =
            r.infrastructure.iter().filter(|i| i.category == InfraCategory::ThirdParty).collect();
        let languages: Vec<String> = {
            let mut l: Vec<(&String, usize)> = r.stats.languages.iter().map(|(k, v)| (k, v.files)).collect();
            l.sort_by_key(|(k, n)| (std::cmp::Reverse(*n), (*k).clone()));
            l.into_iter().map(|(k, _)| k.clone()).collect()
        };

        let summary = match &r.system.description {
            Some(d) => vec![Inline::text(d.clone())],
            None => vec![Inline::text(format!(
                "{} is built from {} {} in {}.",
                r.system.name,
                deployables.len(),
                if deployables.len() == 1 { "deployable unit" } else { "deployable units" },
                languages.join(", ")
            ))],
        };

        let mut blocks = vec![Block::Stats {
            items: vec![
                Stat {
                    label: "Deployables".into(),
                    value: deployables.len().to_string(),
                    page: Some("architecture".into()),
                },
                Stat {
                    label: "Datastores & queues".into(),
                    value: (stores.len() + buses.len()).to_string(),
                    page: Some("data".into()),
                },
                Stat { label: "External services".into(), value: vendors.len().to_string(), page: Some("data".into()) },
                Stat { label: "Source".into(), value: format!("{} files", r.stats.files), page: None },
            ],
        }];

        // What the book could not read belongs on its first page, not in a log.
        // A reader cannot tell a book about a whole system from a book about
        // the third of it that happened to be in a language nunki parses.
        let unread: usize = r.stats.unparsed.values().sum();
        if unread > 0 {
            let share = r.stats.read_share();
            let census = r.stats.unread_census();
            let line = format!(
                "{} of {} source files were read. {census} could not be: nunki has no parser for {}, \
                 so whatever they declare — services, routes, entities — is absent here rather than absent from the code.",
                r.stats.files,
                r.stats.files + unread,
                if r.stats.unparsed.len() == 1 { "it" } else { "them" }
            );
            // Below four fifths the gap is a fact about the book, not a
            // footnote about it.
            let tone = if share < 0.80 { "warning" } else { "note" };
            blocks.push(Block::Callout {
                tone: tone.into(),
                title: format!("{:.0}% of the source was read", share * 100.0),
                inl: vec![Inline::text(line)],
            });
        }

        blocks.push(Block::Heading { level: 2, id: "what-it-is".into(), text: "What it is".into() });
        if let Some(desc) = &r.system.description {
            let mut inl = vec![Inline::text(format!("{desc} "))];
            if let Some(e) = readme_evidence(&self.ctx.root, desc) {
                inl.push(self.cite(&e));
            }
            blocks.push(Block::Para { inl });
        }
        let mut inl = vec![Inline::text(format!("{} builds ", r.system.name))];
        push_list(&mut inl, deployables.iter().map(|u| self.element(&u.id)).collect());
        inl.push(Inline::text(format!(
            " — {} written mainly in {}.",
            if deployables.len() == 1 { "one deployable" } else { "deployables" },
            languages.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
        )));
        if !stores.is_empty() || !buses.is_empty() {
            inl.push(Inline::text(" State lives in "));
            push_list(&mut inl, stores.iter().chain(buses.iter()).map(|i| Inline::strong(i.label.clone())).collect());
            inl.push(Inline::text("."));
        }
        if !vendors.is_empty() {
            inl.push(Inline::text(" It calls "));
            push_list(&mut inl, vendors.iter().map(|i| Inline::strong(i.label.clone())).collect());
            inl.push(Inline::text(" outside its trust boundary."));
        }
        blocks.push(Block::Para { inl });

        if let Some(fig) = self.figure(
            "system-context",
            vec![Inline::text("System context: who uses the system and what it depends on outside the repository.")],
        ) {
            blocks.push(Block::Heading { level: 2, id: "system-context".into(), text: "System context".into() });
            blocks.push(fig);
        }

        blocks.push(Block::Heading { level: 2, id: "read-next".into(), text: "Read next".into() });
        let mut cards = vec![Card {
            page: "architecture".into(),
            title: "Architecture".into(),
            text: "Every deployable, datastore and external service, and how they connect.".into(),
            meta: format!("{} relationships", r.relationships.len()),
        }];
        for u in deployables.iter().take(6) {
            cards.push(Card {
                page: format!("containers/{}", u.id),
                title: u.name.clone(),
                text: u.description.clone().unwrap_or_else(|| format!("{} · {}", u.kind.role(), u.tech_stack)),
                meta: u.tech_stack.clone(),
            });
        }
        cards.push(Card {
            page: "data".into(),
            title: "Data & integrations".into(),
            text: "Who reads and writes which store, which events flow where, and which vendors are called.".into(),
            meta: format!("{} stores & queues", stores.len() + buses.len()),
        });
        cards.push(Card {
            page: "flows".into(),
            title: "Critical flows".into(),
            text: "The main path through the system, hop by hop, each step pinned to code.".into(),
            meta: "walkthrough".into(),
        });
        cards.push(Card {
            page: "evidence".into(),
            title: "Evidence & unknowns".into(),
            text: "What was verified, what was only declared, and what static analysis cannot see.".into(),
            meta: "citations".into(),
        });
        blocks.push(Block::Cards { cards });
        self.push_page("overview", r.system.name.clone(), "Overview", summary, blocks);
    }

    fn architecture_page(&mut self) {
        let r = self.report;
        let deployables = self.deployables();
        let mut blocks = Vec::new();
        if let Some(fig) = self.figure(
            "containers",
            vec![Inline::text(
                "Container view. Hover a card to trace what it depends on; click it to open its page or its source.",
            )],
        ) {
            blocks.push(fig);
        }

        blocks.push(Block::Heading { level: 2, id: "containers".into(), text: "Deployables".into() });
        let mut rows = Vec::new();
        for u in &deployables {
            let entry = match u.entry_points.first() {
                Some(e) => vec![self.cite(e)],
                None => vec![Inline::badge("muted", "none found")],
            };
            rows.push(vec![
                vec![self.element(&u.id)],
                vec![Inline::text(u.kind.role())],
                vec![Inline::code(u.tech_stack.clone())],
                entry,
                vec![Inline::code(if u.root.is_empty() { ".".to_string() } else { format!("{}/", u.root) })],
            ]);
        }
        blocks.push(Block::Table {
            columns: vec!["Container".into(), "Role".into(), "Technology".into(), "Entry point".into(), "Path".into()],
            rows,
        });

        if !r.relationships.is_empty() {
            blocks.push(Block::Heading { level: 2, id: "relationships".into(), text: "Relationships".into() });
            blocks.push(Block::Para {
                inl: vec![
                    Inline::text("Each relationship is marked "),
                    Inline::badge("observed", "observed in code"),
                    Inline::text(" when a call site, import or query was found, or "),
                    Inline::badge("declared", "declared"),
                    Inline::text(" when it only appears in compose files or manifests."),
                ],
            });
            let mut rows = Vec::new();
            // Large workspaces link dozens of packages; those links belong in
            // the libraries table, not between the runtime relationships.
            let library_links = r.relationships.iter().filter(|x| x.label == "uses").count();
            let summarize_links = library_links > LIBRARY_LINK_LIMIT;
            if summarize_links {
                blocks.push(Block::Para {
                    inl: vec![
                        Inline::text(format!(
                            "{library_links} package links inside the workspace are summarised under "
                        )),
                        Inline::Link {
                            page: "architecture".into(),
                            anchor: Some("libraries".into()),
                            v: "Shared libraries".into(),
                        },
                        Inline::text("."),
                    ],
                });
            }
            let mut rels: Vec<&Relationship> =
                r.relationships.iter().filter(|x| !(summarize_links && x.label == "uses")).collect();
            rels.sort_by_key(|x| (!Self::observed(x), x.source.clone(), x.target.clone()));
            for rel in rels {
                let ev = self.cites(&rel.evidence, 2);
                rows.push(vec![
                    vec![self.element(&rel.source)],
                    vec![self.element(&rel.target)],
                    vec![Inline::text(Self::interaction(rel))],
                    vec![Self::basis(rel)],
                    ev,
                ]);
            }
            blocks.push(Block::Table {
                columns: vec!["From".into(), "To".into(), "Interaction".into(), "Basis".into(), "Evidence".into()],
                rows,
            });
        }

        let libraries: Vec<&UnitSummary> = r
            .containers
            .iter()
            .filter(|u| u.kind == UnitKind::Library && !deployables.iter().any(|d| d.id == u.id))
            .collect();
        if !libraries.is_empty() {
            blocks.push(Block::Heading { level: 2, id: "libraries".into(), text: "Shared libraries".into() });
            let mut rows = Vec::new();
            for l in libraries {
                let users: Vec<String> = r
                    .relationships
                    .iter()
                    .filter(|x| x.target == l.id && x.label == "uses")
                    .map(|x| x.source.clone())
                    .collect();
                let mut used = Vec::new();
                push_list(&mut used, users.iter().map(|u| self.element(u)).collect());
                if used.is_empty() {
                    used.push(Inline::badge("muted", "not linked"));
                }
                rows.push(vec![
                    vec![Inline::strong(l.name.clone())],
                    vec![Inline::code(format!("{}/", l.root))],
                    vec![Inline::text(l.language.display())],
                    used,
                ]);
            }
            blocks.push(Block::Table {
                columns: vec!["Library".into(), "Path".into(), "Language".into(), "Used by".into()],
                rows,
            });
        }

        let notes: Vec<String> =
            self.diagrams.iter().find(|d| d.id == "containers").map(|d| d.notes.clone()).unwrap_or_default();
        if !notes.is_empty() {
            blocks.push(Block::Callout {
                tone: "note".into(),
                title: "How this view was simplified".into(),
                inl: vec![Inline::text(notes.join(" · "))],
            });
        }
        let summary = vec![Inline::text(format!(
            "How the {} deployable{}, {} datastore{} and queue{}, and {} external service{} connect.",
            deployables.len(),
            plural(deployables.len()),
            r.infrastructure.iter().filter(|i| i.category != InfraCategory::ThirdParty).count(),
            plural(r.infrastructure.iter().filter(|i| i.category != InfraCategory::ThirdParty).count()),
            "s",
            r.infrastructure.iter().filter(|i| i.category == InfraCategory::ThirdParty).count(),
            plural(r.infrastructure.iter().filter(|i| i.category == InfraCategory::ThirdParty).count()),
        ))];
        self.push_page("architecture", "Architecture", "Architecture", summary, blocks);
    }

    fn container_page(&mut self, u: &UnitSummary) {
        let r = self.report;
        let figure_id = format!("components-{}", u.id);
        let view = r.component_views.iter().find(|v| v.unit == u.id);
        let inbound: Vec<&Relationship> = r.relationships.iter().filter(|x| x.target == u.id).collect();
        let outbound: Vec<&Relationship> = r.relationships.iter().filter(|x| x.source == u.id).collect();
        let summary = vec![Inline::text(
            u.description.clone().unwrap_or_else(|| format!("{} built with {}.", u.kind.role(), u.tech_stack)),
        )];

        let mut blocks = vec![Block::Stats {
            items: vec![
                Stat { label: "Files".into(), value: u.files.to_string(), page: None },
                Stat { label: "Lines".into(), value: u.lines.to_string(), page: None },
                Stat {
                    label: "Modules".into(),
                    value: view.map(|v| v.modules.len()).unwrap_or(0).to_string(),
                    page: None,
                },
                Stat { label: "Depends on".into(), value: outbound.len().to_string(), page: None },
                Stat { label: "Used by".into(), value: inbound.len().to_string(), page: None },
            ],
        }];

        if u.tech_stack.ends_with("not parsed") {
            blocks.push(Block::Callout {
                tone: "warning".into(),
                title: "Source not parsed".into(),
                inl: vec![Inline::text(format!(
                    "This unit was found from its compose service and Dockerfile. {} source isn't analysed, so modules, symbols and outbound calls from code are unknown; relationships below come from compose.",
                    u.language.display()
                ))],
            });
        }

        blocks.push(Block::Heading { level: 2, id: "at-a-glance".into(), text: "At a glance".into() });
        let mut rows = vec![
            vec![vec![Inline::text("Role")], vec![Inline::text(u.kind.role())]],
            vec![vec![Inline::text("Technology")], vec![Inline::code(u.tech_stack.clone())]],
            vec![
                vec![Inline::text("Path")],
                vec![Inline::code(if u.root.is_empty() { ".".to_string() } else { format!("{}/", u.root) })],
            ],
        ];
        if !u.frameworks.is_empty() {
            rows.push(vec![vec![Inline::text("Frameworks")], vec![Inline::text(u.frameworks.join(", "))]]);
        }
        if let Some(m) = &u.manifest {
            rows.push(vec![vec![Inline::text("Manifest")], vec![Inline::code(m.clone())]]);
        }
        if let Some(s) = &u.compose_service {
            rows.push(vec![vec![Inline::text("Compose service")], vec![Inline::code(s.clone())]]);
        }
        if !u.entry_points.is_empty() {
            let mut inl = Vec::new();
            for e in u.entry_points.iter().take(3) {
                if !inl.is_empty() {
                    inl.push(Inline::text(" "));
                }
                if let Some(n) = &e.note {
                    inl.push(Inline::text(format!("{n} ")));
                }
                inl.push(self.cite(e));
            }
            rows.push(vec![vec![Inline::text("Entry points")], inl]);
        }
        blocks.push(Block::Table { columns: vec!["".into(), "".into()], rows });

        if let Some(fig) = self.figure(
            &figure_id,
            vec![Inline::text(format!("Components of {}: modules and the imports between them.", u.name))],
        ) {
            blocks.push(Block::Heading { level: 2, id: "components".into(), text: "Components".into() });
            blocks.push(fig);
        }

        if let Some(v) = view {
            let section = self.modules_section(u, v);
            blocks.extend(section);
        }

        for (title, anchor, rels, outward) in
            [("Depends on", "depends-on", &outbound, true), ("Used by", "used-by", &inbound, false)]
        {
            if rels.is_empty() {
                continue;
            }
            blocks.push(Block::Heading { level: 2, id: anchor.into(), text: title.into() });
            let mut rows = Vec::new();
            for rel in rels.iter() {
                let other = if outward { &rel.target } else { &rel.source };
                let ev = self.cites(&rel.evidence, 2);
                rows.push(vec![
                    vec![self.element(other)],
                    vec![Inline::text(Self::interaction(rel))],
                    vec![Self::basis(rel)],
                    ev,
                ]);
            }
            blocks.push(Block::Table {
                columns: vec![
                    if outward { "Target" } else { "Caller" }.into(),
                    "Interaction".into(),
                    "Basis".into(),
                    "Evidence".into(),
                ],
                rows,
            });
        }

        if !u.key_symbols.is_empty() {
            blocks.push(Block::Heading { level: 2, id: "key-symbols".into(), text: "Key symbols".into() });
            let mut rows = Vec::new();
            for s in u.key_symbols.iter().take(10) {
                let ev = self.cite(&s.evidence);
                rows.push(vec![
                    vec![Inline::code(s.name.clone())],
                    vec![Inline::text(format!("{:?}", s.kind).to_lowercase())],
                    vec![Inline::text(s.doc.as_deref().and_then(|d| d.lines().next()).unwrap_or("").to_string())],
                    vec![ev],
                ]);
            }
            blocks.push(Block::Table {
                columns: vec!["Symbol".into(), "Kind".into(), "Description".into(), "Evidence".into()],
                rows,
            });
        }
        self.push_page(&format!("containers/{}", u.id), u.name.clone(), "Container", summary, blocks);
        if let Some(v) = view {
            self.modules_page(u, v);
        }
    }

    fn data_page(&mut self) {
        let r = self.report;
        let stores: Vec<&InfraSummary> =
            r.infrastructure.iter().filter(|i| i.category == InfraCategory::Storage).collect();
        let buses: Vec<&InfraSummary> =
            r.infrastructure.iter().filter(|i| i.category == InfraCategory::EventBus).collect();
        let vendors: Vec<&InfraSummary> =
            r.infrastructure.iter().filter(|i| i.category == InfraCategory::ThirdParty).collect();
        let mut blocks = Vec::new();

        if stores.is_empty() && buses.is_empty() && vendors.is_empty() {
            blocks.push(Block::Callout {
                tone: "note".into(),
                title: "No datastores, queues or external services detected".into(),
                inl: vec![Inline::text(
                    "Nothing in manifests, compose files or imports matched a known database, cache, event bus or vendor SDK.",
                )],
            });
        }

        if !stores.is_empty() {
            blocks.push(Block::Heading { level: 2, id: "access-matrix".into(), text: "Datastore access".into() });
            blocks.push(Block::Para {
                inl: vec![Inline::text(
                    "Rows are the services that touch state; columns are the stores. Reads and writes come from SQL literals and cache commands; queries means an ORM or driver without literal SQL.",
                )],
            });
            let mut users: BTreeSet<String> = BTreeSet::new();
            for s in &stores {
                users.extend(r.relationships.iter().filter(|x| x.target == s.id).map(|x| x.source.clone()));
            }
            let mut rows = Vec::new();
            for user in &users {
                let mut row = vec![vec![self.element(user)]];
                for s in &stores {
                    match r.relationships.iter().find(|x| &x.source == user && x.target == s.id) {
                        Some(rel) => {
                            let mut cell = vec![Inline::badge(
                                match rel.edge_type {
                                    EdgeType::Write => "write",
                                    EdgeType::Read => "read",
                                    _ => "muted",
                                },
                                Self::interaction(rel),
                            )];
                            cell.extend(self.cites(&rel.evidence, 1));
                            row.push(cell);
                        }
                        None => row.push(vec![Inline::text("—")]),
                    }
                }
                rows.push(row);
            }
            let mut columns = vec!["Service".to_string()];
            columns.extend(stores.iter().map(|s| s.label.clone()));
            blocks.push(Block::Table { columns, rows });
        }

        if !buses.is_empty() {
            blocks.push(Block::Heading { level: 2, id: "events".into(), text: "Events".into() });
            let mut rows = Vec::new();
            for bus in &buses {
                let producers: Vec<&Relationship> =
                    r.relationships.iter().filter(|x| x.target == bus.id && x.edge_type == EdgeType::Event).collect();
                let consumers: Vec<&Relationship> =
                    r.relationships.iter().filter(|x| x.source == bus.id && x.edge_type == EdgeType::Event).collect();
                let topic_of =
                    |label: &str| label.split_whitespace().last().filter(|t| t.contains('.')).map(str::to_string);
                let topics: BTreeSet<String> = producers
                    .iter()
                    .chain(consumers.iter())
                    .filter_map(|x| topic_of(&x.label))
                    .chain(bus.topics.iter().cloned())
                    .collect();
                let topics: Vec<String> =
                    if topics.is_empty() { vec!["(unnamed)".into()] } else { topics.into_iter().collect() };
                for topic in topics {
                    let mut prod = Vec::new();
                    for p in producers.iter().filter(|x| topic_of(&x.label).is_none_or(|t| t == topic)) {
                        if !prod.is_empty() {
                            prod.push(Inline::text(", "));
                        }
                        prod.push(self.element(&p.source));
                        prod.extend(self.cites(&p.evidence, 1));
                    }
                    let mut cons = Vec::new();
                    for c in consumers.iter().filter(|x| topic_of(&x.label).is_none_or(|t| t == topic)) {
                        if !cons.is_empty() {
                            cons.push(Inline::text(", "));
                        }
                        cons.push(self.element(&c.target));
                        cons.extend(self.cites(&c.evidence, 1));
                    }
                    if prod.is_empty() {
                        prod.push(Inline::badge("muted", "none found"));
                    }
                    if cons.is_empty() {
                        cons.push(Inline::badge("muted", "none found"));
                    }
                    rows.push(vec![vec![Inline::code(topic)], vec![Inline::strong(bus.label.clone())], prod, cons]);
                }
            }
            blocks.push(Block::Table {
                columns: vec!["Topic".into(), "Bus".into(), "Published by".into(), "Consumed by".into()],
                rows,
            });
        }

        if !vendors.is_empty() {
            blocks.push(Block::Heading { level: 2, id: "external".into(), text: "External services".into() });
            let mut rows = Vec::new();
            for v in &vendors {
                let callers: Vec<&Relationship> = r.relationships.iter().filter(|x| x.target == v.id).collect();
                let mut by = Vec::new();
                for c in &callers {
                    if !by.is_empty() {
                        by.push(Inline::text(", "));
                    }
                    by.push(self.element(&c.source));
                    by.extend(self.cites(&c.evidence, 1));
                }
                rows.push(vec![
                    vec![Inline::strong(v.label.clone())],
                    vec![Inline::text(v.role.clone())],
                    vec![Inline::text(callers.first().map(|c| c.label.clone()).unwrap_or_default())],
                    by,
                ]);
            }
            blocks.push(Block::Table {
                columns: vec!["Service".into(), "Category".into(), "Used for".into(), "Called by".into()],
                rows,
            });
        }

        let summary = vec![Inline::text(
            "Where state lives, who reads and writes it, which events flow where, and which vendors are called.",
        )];
        let model = self.data_model_blocks();
        if !model.is_empty() {
            // Measured structure first when there is one; the no-infrastructure note is moot.
            blocks.retain(|b| !matches!(b, Block::Callout { title, .. } if title.starts_with("No datastores")));
            blocks.extend(model);
        }
        self.push_page("data", "Data & integrations", "Data", summary, blocks);
    }

    fn flows_page(&mut self) {
        let r = self.report;
        let mut blocks = Vec::new();
        let container = self.diagrams.iter().find(|d| d.id == "containers").map(|d| d.ir.clone());
        let chain = container.as_ref().map(primary_chain).unwrap_or_default();

        if chain.is_empty() {
            blocks.push(Block::Callout {
                tone: "note".into(),
                title: "No critical path identified".into(),
                inl: vec![Inline::text(
                    "The container view has no primary path: there is no client-to-service chain with observed calls. Edit containers.ir.json to mark one with isPrimaryPath.",
                )],
            });
        } else {
            let ir = container.as_ref().unwrap();
            blocks.push(Block::Heading { level: 2, id: "primary-path".into(), text: "Primary path".into() });
            let names: Vec<String> = std::iter::once(&chain[0].source)
                .chain(chain.iter().map(|e| &e.target))
                .map(|id| ir.node(id).map(|n| n.label.clone()).unwrap_or_else(|| id.clone()))
                .collect();
            blocks.push(Block::Para {
                inl: vec![
                    Inline::text("The critical transaction runs "),
                    Inline::strong(names.join(" → ")),
                    Inline::text(". Step through it to see each hop highlighted on the diagram with the code that makes the call."),
                ],
            });
            let mut steps = Vec::new();
            for e in &chain {
                let rel = r.relationships.iter().find(|x| x.source == e.source && x.target == e.target);
                let mut body = Vec::new();
                match rel {
                    Some(rel) => {
                        body.push(Self::basis(rel));
                        body.push(Inline::text(" "));
                        let ev = self.cites(&rel.evidence, 3);
                        body.extend(ev);
                    }
                    None => body.push(Inline::badge("muted", "edited in IR")),
                }
                let title = vec![
                    self.element(&e.source),
                    Inline::text(" → "),
                    self.element(&e.target),
                    Inline::text(format!(
                        " · {}",
                        e.label.clone().unwrap_or_else(|| format!("{:?}", e.edge_type).to_lowercase())
                    )),
                ];
                steps.push(Step { edge: e.id.clone(), title, body });
            }
            blocks.push(Block::Steps { diagram: "containers".into(), steps });
        }

        let request_flows = self.request_flow_blocks();
        if !request_flows.is_empty() {
            blocks.retain(|b| !matches!(b, Block::Callout { title, .. } if title == "No critical path identified"));
            blocks.extend(request_flows);
        }

        // Event-driven flows: producer → bus → consumer, per topic.
        let mut event_items = Vec::new();
        for bus in r.infrastructure.iter().filter(|i| i.category == InfraCategory::EventBus) {
            for p in r.relationships.iter().filter(|x| x.target == bus.id && x.edge_type == EdgeType::Event) {
                for c in r.relationships.iter().filter(|x| x.source == bus.id && x.edge_type == EdgeType::Event) {
                    let mut inl = vec![self.element(&p.source), Inline::text(format!(" {} → ", p.label))];
                    inl.push(Inline::strong(bus.label.clone()));
                    inl.push(Inline::text(" → "));
                    inl.push(self.element(&c.target));
                    inl.push(Inline::text(" "));
                    inl.extend(self.cites(&p.evidence, 1));
                    inl.extend(self.cites(&c.evidence, 1));
                    event_items.push(inl);
                }
            }
        }
        if !event_items.is_empty() {
            blocks.push(Block::Heading { level: 2, id: "event-flows".into(), text: "Asynchronous flows".into() });
            blocks.push(Block::List { items: event_items });
        }
        let summary =
            vec![Inline::text("The paths that matter most, hop by hop, each pinned to the code that makes the call.")];
        self.push_page("flows", "Critical flows", "Flows", summary, blocks);
    }

    /// How the architecture changed, release by release.
    ///
    /// Read from `history.json` and never recomputed. Rebuilding an old book
    /// must not rewrite what a release recorded, and a build must not depend
    /// on which tags happen to exist in the clone it is running in.
    fn history_page(&mut self) {
        if self.history.entries.is_empty() {
            return;
        }
        let mut blocks = vec![Block::Para {
            inl: vec![Inline::text(
                "What each release changed about the architecture, recorded when it was cut by comparing \
                 it with the release before. Entries are written once and never recomputed, so this says \
                 what was true then rather than what a rebuild would conclude now.",
            )],
        }];

        for entry in &self.history.entries {
            let when = entry.date.as_deref().and_then(|d| d.split('T').next()).unwrap_or("");
            let heading = if when.is_empty() { entry.release.clone() } else { format!("{} · {when}", entry.release) };
            blocks.push(Block::Heading {
                level: 2,
                id: format!("release-{}", nunki_analyzer::scan::slug(&entry.release)),
                text: heading,
            });

            // The two commits it was computed between, so a reader can repeat
            // it instead of taking it on faith.
            let range = match (&entry.diff.base_commit, &entry.diff.head_commit) {
                (Some(b), Some(h)) => {
                    format!("`{}` → `{}`", &b[..b.len().min(8)], &h[..h.len().min(8)])
                }
                _ => format!("`{}` → `{}`", entry.base, entry.release),
            };
            let total: usize = entry.diff.sections.iter().map(|x| x.changes.len()).sum();
            blocks.push(Block::Para {
                inl: vec![Inline::text(if total == 0 {
                    format!("Nothing this book describes differs from {}. {range}", entry.base)
                } else {
                    format!("{total} change{} against {}. {range}", if total == 1 { "" } else { "s" }, entry.base)
                })],
            });

            for section in entry.diff.sections.iter().filter(|x| !x.changes.is_empty()) {
                let mut rows = Vec::new();
                for c in &section.changes {
                    rows.push(vec![
                        vec![Inline::text(c.verb.word().to_string())],
                        vec![Inline::strong(c.subject.clone())],
                        vec![Inline::text(c.details.join("; "))],
                    ]);
                }
                blocks.push(Block::Table { columns: vec![String::new(), section.title.clone(), String::new()], rows });
            }
        }

        self.push_page(
            "history",
            "Architecture history",
            "Trust",
            vec![Inline::text("What each release changed, recorded when it was cut.")],
            blocks,
        );
    }

    fn evidence_page(&mut self) {
        let r = self.report;
        let mut blocks = Vec::new();
        let observed = r.relationships.iter().filter(|x| Self::observed(x)).count();
        let declared = r.relationships.len() - observed;

        blocks.push(Block::Heading {
            level: 2,
            id: "observed-vs-declared".into(),
            text: "Observed versus declared".into(),
        });
        blocks.push(Block::Para {
            inl: vec![
                Inline::text(format!("{observed} of {} relationships were ", r.relationships.len())),
                Inline::badge("observed", "observed in code"),
                Inline::text(format!("; {declared} are only ")),
                Inline::badge("declared", "declared"),
                Inline::text(" in compose files or manifests and may not be exercised at runtime."),
            ],
        });
        let declared_rels: Vec<&Relationship> = r.relationships.iter().filter(|x| !Self::observed(x)).collect();
        if !declared_rels.is_empty() {
            let mut rows = Vec::new();
            for rel in declared_rels {
                let ev = self.cites(&rel.evidence, 1);
                rows.push(vec![
                    vec![self.element(&rel.source)],
                    vec![self.element(&rel.target)],
                    vec![Self::basis(rel)],
                    ev,
                ]);
            }
            blocks.push(Block::Table {
                columns: vec!["From".into(), "To".into(), "Basis".into(), "Declared at".into()],
                rows,
            });
        }

        blocks.push(Block::Heading { level: 2, id: "unknowns".into(), text: "What remains unknown".into() });
        let mut unknown: Vec<Vec<Inline>> = Vec::new();
        for u in r.containers.iter().filter(|u| u.tech_stack.ends_with("not parsed")) {
            unknown.push(vec![
                self.element(&u.id),
                Inline::text(format!(
                    " is written in {}, which isn't parsed: its internals and code-level calls are not shown.",
                    u.language.display()
                )),
            ]);
        }
        for u in r.containers.iter().filter(|u| u.kind == UnitKind::WebClient) {
            let linked = r
                .relationships
                .iter()
                .any(|x| x.source == u.id && self.unit(&x.target).is_some_and(|t| t.kind == UnitKind::HttpService));
            if !linked {
                unknown.push(vec![
                    self.element(&u.id),
                    Inline::text(" has no detected backend: its API base URL isn't resolvable statically."),
                ]);
            }
        }
        for n in &r.notes {
            unknown.push(vec![Inline::text(format!("Scan: {n}."))]);
        }
        for d in &self.diagrams {
            for n in d.notes.iter().filter(|n| n.starts_with("removed") || n.starts_with("dropped")) {
                unknown.push(vec![Inline::text(format!("Figure {}: {n}.", d.id))]);
            }
        }
        unknown.push(vec![Inline::text(
            "This book is read from the source, not from the running system. Static analysis doesn't see runtime service discovery, reflection, dynamically built URLs, configuration injected at deploy time, generated code, or infrastructure provisioned outside this repository. Treat it as a well-evidenced starting point to review, not as the authority on the architecture.",
        )]);
        blocks.push(Block::List { items: unknown });

        // Index last so every citation made above is included.
        let health = self.health();
        blocks.insert(
            0,
            Block::Stats {
                items: vec![
                    Stat { label: "Citations".into(), value: health.total.to_string(), page: None },
                    Stat { label: "Verified".into(), value: health.verified.to_string(), page: None },
                    Stat { label: "Stale".into(), value: health.stale.to_string(), page: None },
                    Stat { label: "Broken".into(), value: health.broken.to_string(), page: None },
                ],
            },
        );
        if health.stale + health.broken > 0 {
            blocks.insert(
                1,
                Block::Callout {
                    tone: "warning".into(),
                    title: "Some citations no longer match the code".into(),
                    inl: vec![Inline::text(format!(
                        "{} stale and {} broken citation(s). Regenerate the book after committing, or review the entries marked below.",
                        health.stale, health.broken
                    ))],
                },
            );
        }
        blocks.push(Block::Heading { level: 2, id: "citation-index".into(), text: "Citation index".into() });
        let mut rows = Vec::new();
        let ids: Vec<Cite> = self.cites.values().cloned().collect();
        let mut sorted = ids;
        sorted.sort_by_key(|c| (c.file.clone(), c.start, c.end));
        for c in sorted.iter().take(MAX_INDEXED_CITES) {
            rows.push(vec![
                vec![Inline::Cite { id: c.id.clone() }],
                vec![Inline::code(c.symbol.clone().unwrap_or_default())],
                vec![Inline::badge(
                    match c.state.as_str() {
                        "verified" => "observed",
                        "stale" | "untracked" => "warn",
                        _ => "muted",
                    },
                    c.state.clone(),
                )],
                vec![Inline::text(c.detail.clone().unwrap_or_else(|| "verified against the pinned commit".into()))],
            ]);
        }
        blocks.push(Block::Table {
            columns: vec!["Location".into(), "Symbol".into(), "State".into(), "Detail".into()],
            rows,
        });
        let summary = vec![Inline::text(format!(
            "{} of {} citations verified{}. What was observed, what was only declared, and what static analysis can't see.",
            health.verified,
            health.total,
            self.ctx.head_commit.as_deref().map(|c| format!(" against {}", &c[..c.len().min(8)])).unwrap_or_default()
        ))];
        self.push_page("evidence", "Evidence & unknowns", "Evidence", summary, blocks);
    }

    fn nav(&self, deployables: &[UnitSummary]) -> Vec<NavGroup> {
        let item = |page: &str, title: &str| NavItem { page: page.into(), title: title.into(), hint: None };
        let mut groups = vec![
            NavGroup {
                title: "Start".into(),
                items: vec![item("overview", "Overview"), item("architecture", "Architecture")],
            },
            NavGroup {
                title: "Containers".into(),
                items: deployables
                    .iter()
                    .map(|u| NavItem {
                        page: format!("containers/{}", u.id),
                        title: u.name.clone(),
                        hint: Some(u.language.display().to_string()),
                    })
                    .collect(),
            },
            NavGroup {
                title: "System".into(),
                items: {
                    let mut items = vec![item("data", "Data & integrations")];
                    // Domain pages of a large data model follow the overview; beyond a
                    // handful they're reached from the overview's domains table instead.
                    let domains = self.pages.iter().filter(|p| p.id.starts_with("data/")).count();
                    items.extend(self.pages.iter().filter(|p| p.id.starts_with("data/") && domains <= 6).map(|p| {
                        NavItem {
                            page: p.id.clone(),
                            title: p.title.trim_end_matches(" data").to_string(),
                            hint: Some("data".into()),
                        }
                    }));
                    items.push(item("flows", "Critical flows"));
                    if self.pages.iter().any(|p| p.id == "runtime") {
                        items.push(item("runtime", "Runtime & deployment"));
                    }
                    items
                },
            },
            NavGroup {
                title: "API reference".into(),
                items: self
                    .api_units()
                    .iter()
                    .filter(|u| self.pages.iter().any(|p| p.id == api_ref::api_page_id(u)))
                    .map(|u| NavItem {
                        page: api_ref::api_page_id(u),
                        title: self.element_name(u),
                        hint: self.report.api.as_ref().map(|a| {
                            let n = a.operations.iter().filter(|o| &o.unit == u).count();
                            format!("{n} op{}", if n == 1 { "" } else { "s" })
                        }),
                    })
                    .chain(self.pages.iter().filter(|p| p.id == "access").map(|p| item(&p.id, &p.title)))
                    .collect(),
            },
            NavGroup {
                title: "Product".into(),
                items: ["functional", "rules", "requirements"]
                    .iter()
                    .filter_map(|id| self.pages.iter().find(|p| p.id == *id))
                    .map(|p| item(&p.id, &p.title))
                    .collect(),
            },
            NavGroup { title: "Trust".into(), items: vec![item("evidence", "Evidence & unknowns")] },
        ];
        groups.retain(|g| !g.items.is_empty());
        let _ = self.element_name("");
        groups
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// "a, b and c" into inlines.
fn push_list(out: &mut Vec<Inline>, items: Vec<Inline>) {
    let n = items.len();
    for (i, it) in items.into_iter().enumerate() {
        if i > 0 {
            out.push(Inline::text(if i + 1 == n { " and " } else { ", " }));
        }
        out.push(it);
    }
}

/// The primary-path edges in walking order (one chain; ties by id).
pub fn primary_chain(ir: &DiagramIR) -> Vec<nunki_ir::Edge> {
    let primary: Vec<&nunki_ir::Edge> = ir.edges.iter().filter(|e| e.primary()).collect();
    if primary.is_empty() {
        return vec![];
    }
    let targets: BTreeSet<&str> = primary.iter().map(|e| e.target.as_str()).collect();
    let mut start: Vec<&&nunki_ir::Edge> = primary.iter().filter(|e| !targets.contains(e.source.as_str())).collect();
    start.sort_by_key(|e| e.id.clone());
    let Some(first) = start.first() else { return primary.into_iter().cloned().collect() };
    let mut chain = vec![(**first).clone()];
    let mut used = BTreeSet::from([first.id.clone()]);
    loop {
        let cur = chain.last().unwrap().target.clone();
        let mut next: Vec<&&nunki_ir::Edge> =
            primary.iter().filter(|e| e.source == cur && !used.contains(&e.id)).collect();
        next.sort_by_key(|e| e.id.clone());
        match next.first() {
            Some(e) => {
                used.insert(e.id.clone());
                chain.push((**e).clone());
            }
            None => break,
        }
    }
    chain
}

/// Line of the README sentence the description was taken from.
fn readme_evidence(root: &Path, description: &str) -> Option<EvidenceRef> {
    let norm = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    let needle: String = norm(description).chars().take(32).collect();
    if needle.len() < 12 {
        return None;
    }
    for name in ["README.md", "readme.md", "README"] {
        let Ok(text) = std::fs::read_to_string(root.join(name)) else { continue };
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if norm(line).is_empty() {
                continue;
            }
            let window: String = lines[i..(i + 3).min(lines.len())].iter().map(|l| norm(l)).collect();
            if window.starts_with(&needle) || norm(lines[i]).contains(&needle) {
                return Some(EvidenceRef {
                    file_path: name.into(),
                    start_line: i as u32 + 1,
                    end_line: i as u32 + 1,
                    symbol_name: None,
                    note: None,
                });
            }
        }
    }
    None
}
