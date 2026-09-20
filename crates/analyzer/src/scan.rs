//! Repository scan: walk → parse (parallel) → units → infrastructure →
//! relationships → module graph. Every element carries file+line evidence.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

use autodoc_ir::EdgeType;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::catalog::{self, FrameworkRole, InfraCategory, InfraKind};
use crate::compose::{self, ComposeFile};
use crate::extract::{self, FileFacts, SymbolKind};
use crate::lang::{Grammar, Language};
use crate::manifest::{self, Manifest};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Depth {
    System,
    #[default]
    Container,
    Component,
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub depth: Depth,
    /// Unit id to decompose at component depth (defaults to the focal unit).
    pub focus: Option<String>,
    pub max_files: usize,
    pub max_file_bytes: u64,
    /// Scan test, fixture and example code too (excluded by default: it isn't
    /// part of the deployed architecture and usually imports everything).
    pub include_tests: bool,
    /// Also build a component view for every unit with parsed source (books need them all).
    pub all_components: bool,
    /// Directories skipped silently (e.g. the book's own output directory).
    pub ignore_dirs: Vec<PathBuf>,
    /// Extract API contracts and the data model (operations, models, entities, state machines).
    pub behavior: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        ScanOptions {
            depth: Depth::Container,
            focus: None,
            max_files: 25_000,
            max_file_bytes: 1_000_000,
            include_tests: false,
            all_components: false,
            ignore_dirs: Vec::new(),
            behavior: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceRef {
    pub file_path: String,
    pub start_line: u32,
    pub end_line: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl EvidenceRef {
    pub fn to_ir(&self) -> autodoc_ir::Evidence {
        autodoc_ir::Evidence {
            file_path: self.file_path.clone(),
            start_line: self.start_line,
            end_line: self.end_line,
            symbol_name: self.symbol_name.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum UnitKind {
    WebClient,
    HttpService,
    Worker,
    McpServer,
    Cli,
    Library,
}

impl UnitKind {
    pub fn role(self) -> &'static str {
        match self {
            UnitKind::WebClient => "Web client",
            UnitKind::HttpService => "HTTP service",
            UnitKind::Worker => "Background worker",
            UnitKind::McpServer => "MCP server",
            UnitKind::Cli => "Command-line tool",
            UnitKind::Library => "Library",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SymbolRef {
    pub name: String,
    pub kind: SymbolKind,
    pub evidence: EvidenceRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnitSummary {
    pub id: String,
    pub name: String,
    pub kind: UnitKind,
    pub language: Language,
    /// Directory relative to the repository root ("" = root).
    pub root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<String>,
    pub frameworks: Vec<String>,
    pub tech_stack: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub entry_points: Vec<EvidenceRef>,
    pub files: usize,
    pub lines: u64,
    pub key_symbols: Vec<SymbolRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compose_service: Option<String>,
    /// Names the unit is addressed by in service discovery (`spring.application.name`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InfraSummary {
    pub id: String,
    pub kind: InfraKind,
    pub label: String,
    pub category: InfraCategory,
    pub role: String,
    pub used_by: Vec<String>,
    pub evidence: Vec<EvidenceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compose_service: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RelationSource {
    Code,
    Compose,
    Manifest,
    /// Application configuration (gateway routes, service ids in YAML/properties).
    Config,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Relationship {
    pub id: String,
    pub source: String,
    pub target: String,
    pub edge_type: EdgeType,
    pub label: String,
    pub sources: Vec<RelationSource>,
    pub evidence: Vec<EvidenceRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModuleSummary {
    pub id: String,
    pub name: String,
    pub path: String,
    pub files: Vec<String>,
    pub symbols: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<EvidenceRef>,
    pub is_entry: bool,
    pub infra: Vec<InfraKind>,
    /// Types other modules may use: public types in the module's root package
    /// and in Spring Modulith named interfaces (JVM), exported symbols of an
    /// index module (TypeScript).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_types: Vec<SymbolRef>,
    /// Application event types this module publishes / handles.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub publishes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consumes: Vec<String>,
    /// Sub-packages that are the module's internals (JVM).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub internal_packages: Vec<String>,
}

/// A module reaching past another module's boundary: importing its internal
/// package, or depending on a module its `@ApplicationModule` doesn't allow.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Violation {
    /// Module ids.
    pub from: String,
    pub to: String,
    /// `internal` or `not-allowed`.
    pub kind: String,
    /// What was referenced (import specifier).
    pub specifier: String,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModuleDependency {
    pub source: String,
    pub target: String,
    pub weight: u32,
    pub evidence: EvidenceRef,
    /// Event types delivered from `source` to `target` (publisher → listener);
    /// empty for plain import dependencies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComponentView {
    pub unit: String,
    pub modules: Vec<ModuleSummary>,
    pub dependencies: Vec<ModuleDependency>,
    /// module id → infra it touches, with the import site.
    pub infra_usage: Vec<(String, InfraKind, EvidenceRef)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub violations: Vec<Violation>,
    /// Module boundaries are declared (Spring Modulith on the classpath, or
    /// `@ApplicationModule` annotations), so "no violations" is meaningful.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub boundaries_declared: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LangStats {
    pub files: usize,
    pub lines: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScanStats {
    pub files: usize,
    pub lines: u64,
    pub symbols: usize,
    pub files_with_parse_errors: usize,
    pub languages: BTreeMap<String, LangStats>,
    /// Source files recognised by language but with no grammar to read them,
    /// so nothing they declare reaches the book.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub unparsed: BTreeMap<String, usize>,
    pub duration_ms: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepoSummary {
    pub root: String,
    pub name: String,
    pub is_git: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    pub dirty_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub repo: RepoSummary,
    pub depth: Depth,
    pub stats: ScanStats,
    pub system: SystemSummary,
    pub containers: Vec<UnitSummary>,
    pub infrastructure: Vec<InfraSummary>,
    pub relationships: Vec<Relationship>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub components: Option<ComponentView>,
    /// One view per unit with parsed source, when `all_components` was requested.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub component_views: Vec<ComponentView>,
    /// Diagram element id → its primary source evidence.
    pub evidence_map: BTreeMap<String, EvidenceRef>,
    pub notes: Vec<String>,
    /// Operations, request/response models and client calls (when `behavior` was requested).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<crate::api::ApiModel>,
    /// Entities and state machines (when `behavior` was requested).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<crate::data::DataModel>,
    /// Traced request flows, most involved first (when `behavior` was requested).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flows: Vec<crate::trace::Flow>,
    /// Compose and Kubernetes environments (when `behavior` was requested).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topology: Option<crate::topology::Topology>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SystemSummary {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    "vendor",
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    "coverage",
    ".git",
    ".idea",
    ".vscode",
    ".turbo",
    ".cache",
];

pub(crate) struct FileRec {
    pub path: String,
    pub language: Language,
    pub facts: FileFacts,
}

pub(crate) struct Unit {
    pub id: String,
    pub dir: String,
    pub manifest: Option<Manifest>,
    pub files: Vec<usize>,
    pub summary: UnitSummary,
    /// Other names the unit is addressed by (service-discovery / application names).
    pub aliases: Vec<String>,
}

#[derive(Default)]
struct InfraUse {
    manifest_evidence: Vec<EvidenceRef>,
    import_evidence: Vec<EvidenceRef>,
    importing_files: BTreeSet<usize>,
    reads: Vec<EvidenceRef>,
    writes: Vec<EvidenceRef>,
    produces: Vec<EvidenceRef>,
    consumes: Vec<EvidenceRef>,
    topics: BTreeSet<String>,
}

pub fn scan(root: &Path, opts: &ScanOptions) -> Result<ScanReport, crate::ScanError> {
    let started = Instant::now();
    let root = root.canonicalize().map_err(|e| crate::ScanError::Root(root.display().to_string(), e.to_string()))?;
    if !root.is_dir() {
        return Err(crate::ScanError::Root(root.display().to_string(), "not a directory".into()));
    }
    let ctx = autodoc_git::repo_context(&root);
    let mut notes = Vec::new();

    let Walked {
        manifests: manifest_paths,
        sources: source_paths,
        truncated,
        skipped_test_dirs,
        skipped_test_files,
        oversized,
        unparsed,
    } = walk(&root, opts);
    if skipped_test_dirs + skipped_test_files > 0 {
        notes.push(format!(
            "excluded {skipped_test_dirs} test/example/tooling director{} and {skipped_test_files} test file(s); pass include_tests to scan them",
            if skipped_test_dirs == 1 { "y" } else { "ies" }
        ));
    }
    if !unparsed.is_empty() {
        let parsed = source_paths.len();
        let skipped: usize = unparsed.values().sum();
        let census: Vec<String> = unparsed.iter().map(|(l, n)| format!("{n} {l}")).collect();
        // A repository that is mostly a language with no grammar produces a
        // book about the remainder, which reads as a book about the system.
        if skipped > parsed {
            notes.push(format!(
                "most of this repository was not read: {} file(s) parsed, {skipped} not ({}). What those files declare — services, routes, entities — is absent, not missing from the code",
                parsed,
                census.join(", ")
            ));
        } else {
            notes.push(format!("{skipped} file(s) have no grammar and were not read ({})", census.join(", ")));
        }
    }
    if !oversized.is_empty() {
        let shown: Vec<&str> = oversized.iter().take(5).filter_map(|p| p.to_str()).collect();
        notes.push(format!(
            "{} source file(s) above {} KB were not parsed, so anything they declare is missing: {}{}",
            oversized.len(),
            opts.max_file_bytes / 1024,
            shown.join(", "),
            if oversized.len() > shown.len() { ", …" } else { "" }
        ));
    }
    if truncated {
        notes.push(format!("scan truncated at {} files; raise max_files for full coverage", opts.max_files));
    }
    let (files, unreadable) = parse_files(&root, &source_paths);
    if !unreadable.is_empty() {
        let shown: Vec<&str> = unreadable.iter().take(5).map(String::as_str).collect();
        notes.push(format!(
            "{} source file(s) could not be read as text, so nothing they declare is documented: {}{}",
            unreadable.len(),
            shown.join(", "),
            if unreadable.len() > shown.len() { ", …" } else { "" }
        ));
    }
    let manifests: Vec<Manifest> = manifest_paths.iter().filter_map(|p| manifest::parse(&root, p)).collect();

    let repo_name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "repo".into());
    let mut units = build_units(&repo_name, &manifests, &files);
    // JVM services are addressed by their configured application names.
    let jvm_docs = if units.iter().any(|u| matches!(u.summary.language, Language::Java | Language::Kotlin)) {
        crate::jvm::config_documents(&root)
    } else {
        vec![]
    };
    let mut jvm_names: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    let unit_dirs: Vec<String> = units.iter().map(|u| u.dir.clone()).collect();
    for (ui, u) in units.iter_mut().enumerate() {
        let names = crate::jvm::application_names(u, &jvm_docs, &unit_dirs);
        for (n, _) in &names {
            if !u.aliases.contains(n) {
                u.aliases.push(n.clone());
                u.summary.aliases.push(n.clone());
            }
        }
        if !names.is_empty() {
            jvm_names.insert(ui, names.into_iter().map(|(n, _)| n).collect());
        }
    }
    let compose = compose::find_all(&root);
    let repo_slug = slug(&repo_name);
    add_compose_only_units(&root, &repo_slug, &compose, &mut units);

    let mut infra_uses: BTreeMap<(usize, InfraKind), InfraUse> = BTreeMap::new();
    for (ui, unit) in units.iter().enumerate() {
        detect_infra(ui, unit, &files, &mut infra_uses);
    }
    for (ui, unit) in units.iter_mut().enumerate() {
        classify_unit(ui, unit, &files, &infra_uses);
    }

    let mut rels = RelBuilder::default();
    let mut infra = collect_infra(&units, &files, &infra_uses, &mut rels);
    link_workspace_libraries(&units, &files, &mut rels);
    for w in crate::jvm::wires(&units, &files, &jvm_names, &jvm_docs) {
        let Some(t) = unit_for_name(&w.target, &repo_slug, &units) else { continue };
        let source = if w.from_code { RelationSource::Code } else { RelationSource::Config };
        rels.add(&units[w.source].id.clone(), &units[t].id.clone(), w.edge_type, &w.label, source, vec![w.evidence]);
    }
    demote_linked_libraries(&mut units, &files, &rels);
    if compose.is_empty() {
        infer_http_links(&units, &files, &mut rels);
    } else {
        apply_compose(&root, &compose, &repo_slug, &mut units, &files, &mut infra, &mut rels, &infra_uses);
    }
    link_web_clients(&units, &files, &mut rels);
    let relationships = rels.finish();

    let mut stats = ScanStats {
        files: files.len(),
        lines: 0,
        symbols: 0,
        files_with_parse_errors: 0,
        languages: BTreeMap::new(),
        unparsed: unparsed.clone(),
        duration_ms: 0,
        truncated,
    };
    for f in &files {
        stats.lines += f.facts.line_count as u64;
        stats.symbols += f.facts.symbols.len();
        stats.files_with_parse_errors += f.facts.has_syntax_errors as usize;
        let l = stats.languages.entry(f.language.display().to_string()).or_default();
        l.files += 1;
        l.lines += f.facts.line_count as u64;
    }
    if stats.files_with_parse_errors > 0 {
        notes.push(format!("{} file(s) had syntax errors; their facts may be partial", stats.files_with_parse_errors));
    }

    let mut evidence_map = BTreeMap::new();
    for u in &units {
        if let Some(e) = unit_evidence(u, &files) {
            evidence_map.insert(u.id.clone(), e);
        }
    }
    for i in &infra {
        if let Some(e) = i.evidence.first() {
            evidence_map.insert(i.id.clone(), e.clone());
        }
    }

    let components = if opts.depth == Depth::Component {
        let focus = match &opts.focus {
            Some(f) => {
                Some(units.iter().position(|u| &u.id == f || u.summary.name == *f || u.dir == *f).ok_or_else(|| {
                    crate::ScanError::UnknownFocus(f.clone(), units.iter().map(|u| u.id.clone()).collect())
                })?)
            }
            None => busiest_unit(&units, &files, &relationships),
        };
        focus.map(|ui| {
            let mut view = crate::modules::component_view(&units[ui], &files, &infra_uses_for(ui, &infra_uses));
            crate::domains::module_facts(&root, &units[ui], &files, &mut view);
            for m in &view.modules {
                if let Some(e) = &m.evidence {
                    evidence_map.insert(m.id.clone(), e.clone());
                }
            }
            view
        })
    } else {
        None
    };

    let mut component_views = Vec::new();
    if opts.all_components {
        for (ui, unit) in units.iter().enumerate().filter(|(_, u)| !u.files.is_empty()) {
            let mut view = crate::modules::component_view(unit, &files, &infra_uses_for(ui, &infra_uses));
            crate::domains::module_facts(&root, unit, &files, &mut view);
            for m in &view.modules {
                if let Some(e) = &m.evidence {
                    evidence_map.entry(m.id.clone()).or_insert_with(|| e.clone());
                }
            }
            component_views.push(view);
        }
    }

    let (api, data, flows) = if opts.behavior {
        let unit_ids: HashMap<usize, &str> =
            units.iter().flat_map(|u| u.files.iter().map(move |&fi| (fi, u.id.as_str()))).collect();
        let summaries: Vec<UnitSummary> = units.iter().map(|u| u.summary.clone()).collect();
        let index = crate::source::SourceIndex {
            root: root.clone(),
            units: &summaries,
            files: files
                .iter()
                .enumerate()
                .filter_map(|(fi, f)| {
                    Some(crate::source::SourceFile {
                        path: &f.path,
                        language: f.language,
                        unit: unit_ids.get(&fi)?,
                        facts: &f.facts,
                    })
                })
                .collect(),
        };
        // Behaviour extraction is heuristic over arbitrary source: a defect in it
        // costs that section of the book, never the whole scan.
        let api = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::api::extract(&index))) {
            Ok(api) => api,
            Err(_) => {
                notes.push("API contract extraction failed on this repository and was skipped (please report)".into());
                Default::default()
            }
        };
        let data = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::data::extract(&index))) {
            Ok(data) => data,
            Err(_) => {
                notes.push("data model extraction failed on this repository and was skipped (please report)".into());
                Default::default()
            }
        };
        let flows = crate::trace::trace(&index, &api, Some(&data), &infra, &relationships);
        (Some(api), Some(data), flows)
    } else {
        (None, None, Vec::new())
    };
    let topology = opts.behavior.then(|| {
        let resolve = |name: &str, image: Option<&str>, build: Option<&str>| -> Option<String> {
            if let Some(u) = build.and_then(|b| units.iter().find(|u| u.dir == b && !u.files.is_empty())) {
                return Some(u.id.clone());
            }
            let image_base =
                image.map(|i| i.rsplit('/').next().unwrap_or(i).split([':', '@']).next().unwrap_or("").to_string());
            std::iter::once(name.to_string())
                .chain(image_base)
                .filter(|n| !n.is_empty() && !n.contains("${"))
                .find_map(|n| unit_for_name(&n, &repo_slug, &units))
                .map(|i| units[i].id.clone())
        };
        crate::topology::discover(&root, &resolve)
    });

    let description = read_readme_summary(&root)
        .or_else(|| units.iter().find_map(|u| u.summary.description.clone().filter(|d| !d.trim().is_empty())));
    stats.duration_ms = started.elapsed().as_millis() as u64;
    Ok(ScanReport {
        repo: RepoSummary {
            root: root.display().to_string(),
            name: repo_name.clone(),
            is_git: ctx.is_git(),
            // The commit the scan describes, which is not HEAD when the output
            // directory lives in the repository: committing generated
            // documentation moves HEAD without changing anything described,
            // and pinning to it would leave the output stale from birth.
            commit_hash: describing_commit(&ctx, opts),
            branch: ctx.branch.clone(),
            remote_url: ctx.remote_url.clone(),
            dirty_files: ctx.dirty_files.clone(),
        },
        depth: opts.depth,
        stats,
        system: SystemSummary { name: display_name(&repo_name), description },
        containers: units.into_iter().map(|u| u.summary).collect(),
        infrastructure: infra,
        relationships,
        components,
        component_views,
        evidence_map,
        notes,
        api,
        data,
        flows,
        topology,
    })
}

fn infra_uses_for(ui: usize, all: &BTreeMap<(usize, InfraKind), InfraUse>) -> Vec<(InfraKind, BTreeSet<usize>)> {
    all.iter().filter(|((u, _), _)| *u == ui).map(|((_, k), v)| (*k, v.importing_files.clone())).collect()
}

fn busiest_unit(units: &[Unit], files: &[FileRec], rels: &[Relationship]) -> Option<usize> {
    units
        .iter()
        .enumerate()
        .filter(|(_, u)| !u.files.is_empty())
        .max_by_key(|(_, u)| {
            // A component view needs internal structure to show.
            let modules = crate::modules::module_count(u, files).min(12);
            // Deployed services are the interesting insides; among equals, the
            // most connected and then the largest unit.
            let service = matches!(u.summary.kind, UnitKind::HttpService | UnitKind::Worker | UnitKind::McpServer);
            let degree = rels.iter().filter(|r| r.source == u.id || r.target == u.id).count();
            (modules >= 4, service, degree, u.summary.lines, std::cmp::Reverse(u.id.clone()))
        })
        .map(|(i, _)| i)
}

/// Directory names holding tests, fixtures, examples and benchmarks.
const TEST_DIRS: &[&str] = &[
    "test",
    "tests",
    "__tests__",
    "__mocks__",
    // mockery and gomock generate into `mocks/`; a test double is not
    // architecture, and tracing into one cites a mock as if it were the code.
    "mocks",
    "testdata",
    "test-data",
    "test_data",
    "fixtures",
    "__fixtures__",
    "examples",
    "example",
    "samples",
    "e2e",
    "integration-tests",
    "benches",
    "benchmarks",
    "cypress",
    "playwright",
    "spec",
    "specs",
];

/// Directories that hold tooling rather than deployed code.
const NON_ARCHITECTURE_DIRS: &[&str] = &["docs", "doc", "scripts", "hack", "tools", "misc", "design", "website"];

fn is_non_architecture_dir(name: &str) -> bool {
    let n = name.to_lowercase();
    TEST_DIRS.contains(&n.as_str())
        || NON_ARCHITECTURE_DIRS.contains(&n.as_str())
        || n.starts_with("e2e-")
        || n.ends_with("-e2e")
        || n.ends_with("-tests")
        || n.ends_with("_tests")
        || n == "api_tests"
}

pub fn is_test_file(name: &str) -> bool {
    let stem_ext = |suffixes: &[&str]| suffixes.iter().any(|s| name.ends_with(s));
    stem_ext(&["_test.go", "_test.py", "_tests.rs", "_test.rs", "Test.java", "Tests.java", "IT.java", "TestCase.java"])
        || (name.starts_with("test_") && name.ends_with(".py"))
        || name == "conftest.py"
        || [".test.", ".spec.", ".stories.", ".e2e."].iter().any(|m| name.contains(m))
}

struct Walked {
    manifests: Vec<PathBuf>,
    sources: Vec<(PathBuf, Grammar, Language)>,
    truncated: bool,
    skipped_test_dirs: usize,
    skipped_test_files: usize,
    /// Source files past `max_file_bytes`, or whose size could not be read.
    oversized: Vec<PathBuf>,
    /// Files of a language the scanner recognises but cannot parse.
    unparsed: BTreeMap<String, usize>,
}

fn walk(root: &Path, opts: &ScanOptions) -> Walked {
    let mut manifests = Vec::new();
    let mut sources = Vec::new();
    let mut truncated = false;
    let mut oversized: Vec<PathBuf> = Vec::new();
    let mut unparsed: BTreeMap<String, usize> = BTreeMap::new();
    let skipped_dirs = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut skipped_files = 0;
    let counter = skipped_dirs.clone();
    let include_tests = opts.include_tests;
    let ignored: Vec<PathBuf> = opts
        .ignore_dirs
        .iter()
        .map(|d| {
            let abs = if d.is_absolute() { d.clone() } else { std::env::current_dir().unwrap_or_default().join(d) };
            manifest::normalize(&abs.canonicalize().unwrap_or(abs))
        })
        .collect();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(false)
        .require_git(false)
        .sort_by_file_path(|a, b| a.cmp(b))
        .filter_entry(move |e| {
            if e.depth() == 0 || !e.file_type().is_some_and(|t| t.is_dir()) {
                return true;
            }
            if ignored.iter().any(|d| e.path() == d) {
                return false;
            }
            let name = e.file_name().to_string_lossy();
            if SKIP_DIRS.contains(&name.as_ref()) {
                return false;
            }
            // Inside a JVM source root, directories are packages (`com/example/…`,
            // `…/test/support`), not example or test trees.
            let p = e.path().to_string_lossy().replace('\\', "/");
            if ["/src/main/java/", "/src/main/kotlin/"].iter().any(|m| p.contains(m)) {
                return true;
            }
            // A tooling directory that only exists to hold an ignored output
            // (docs/ holding docs/architecture) isn't worth reporting.
            if !include_tests && is_non_architecture_dir(&name) && ignored.iter().any(|d| d.starts_with(e.path())) {
                return false;
            }
            if !include_tests && is_non_architecture_dir(&name) {
                counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return false;
            }
            true
        })
        .build();
    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(root) else { continue };
        let name = entry.file_name().to_string_lossy();
        if manifest::MANIFEST_FILES.contains(&name.as_ref()) {
            manifests.push(rel.to_path_buf());
            continue;
        }
        if name.ends_with(".d.ts") || name.ends_with(".min.js") {
            continue;
        }
        // JVM resources are packaged assets and templates (bundled JS, vendored
        // libraries), not the service's architecture.
        let rel_str = rel.to_string_lossy();
        if (rel_str.starts_with("src/main/resources/") || rel_str.contains("/src/main/resources/"))
            && !name.ends_with(".java")
        {
            continue;
        }
        if !opts.include_tests && is_test_file(&name) {
            skipped_files += 1;
            continue;
        }
        let Some((grammar, language)) = Grammar::for_path(rel) else {
            // Recognised, unreadable: counted so the book can say how much of
            // the repository it never looked at.
            if let Some(l) = rel.extension().and_then(|e| e.to_str()).and_then(Language::unparsed_for_extension) {
                *unparsed.entry(l.display().to_string()).or_default() += 1;
            }
            continue;
        };
        // Silently dropping a source file makes the documentation confidently
        // incomplete, so record what was left out and report it.
        if entry.metadata().map(|m| m.len() > opts.max_file_bytes).unwrap_or(true) {
            oversized.push(rel.to_path_buf());
            continue;
        }
        if sources.len() >= opts.max_files {
            truncated = true;
            break;
        }
        sources.push((rel.to_path_buf(), grammar, language));
    }
    oversized.sort();
    Walked {
        manifests,
        sources,
        truncated,
        skipped_test_dirs: skipped_dirs.load(std::sync::atomic::Ordering::Relaxed),
        skipped_test_files: skipped_files,
        oversized,
        unparsed,
    }
}

/// Parses every source file, and reports the ones it could not read.
///
/// A file that is not valid UTF-8, or that cannot be opened, used to disappear
/// without a word: its routes and entities were simply absent and the book read
/// as though the repository did not contain them.
fn parse_files(root: &Path, paths: &[(PathBuf, Grammar, Language)]) -> (Vec<FileRec>, Vec<String>) {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(16);
    let chunk = paths.len().div_ceil(threads).max(1);
    let mut out: Vec<FileRec> = Vec::with_capacity(paths.len());
    let mut unreadable: Vec<String> = Vec::new();
    std::thread::scope(|s| {
        let handles: Vec<_> = paths
            .chunks(chunk)
            .map(|batch| {
                s.spawn(move || {
                    batch
                        .iter()
                        .map(|(rel, grammar, language)| {
                            let path = rel.to_string_lossy().replace('\\', "/");
                            match std::fs::read_to_string(root.join(rel)) {
                                Ok(src) => {
                                    let facts = extract::extract(*grammar, *language, &path, &src);
                                    Ok(FileRec { path, language: *language, facts })
                                }
                                Err(_) => Err(path),
                            }
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for h in handles {
            for parsed in h.join().expect("parser thread panicked") {
                match parsed {
                    Ok(rec) => out.push(rec),
                    Err(path) => unreadable.push(path),
                }
            }
        }
    });
    unreadable.sort();
    (out, unreadable)
}

/// The last commit that touched something the scan looked at, ignoring the
/// directories it was told to skip (the book's own output among them).
fn describing_commit(ctx: &autodoc_git::RepoContext, opts: &ScanOptions) -> Option<String> {
    let Some(top) = ctx.git_root.as_ref() else { return ctx.head_commit.clone() };
    let excluded: Vec<String> = opts
        .ignore_dirs
        .iter()
        .filter_map(|d| {
            let d = d.canonicalize().unwrap_or_else(|_| d.clone());
            let rel = d.strip_prefix(top).ok()?.to_string_lossy().replace('\\', "/");
            (!rel.is_empty()).then_some(rel)
        })
        .collect();
    match excluded.as_slice() {
        [] => ctx.head_commit.clone(),
        // One exclusion is the common case; git takes several just as well.
        many => autodoc_git::commit_describing(ctx, Some(many.join(","))),
    }
}

/// Whether what follows `FROM` reads as a table rather than English. "Select a
/// workspace from the list" satisfies a naive SELECT/FROM test and was counted
/// as a query against every SQL store the unit declares.
fn names_a_table(upper: &str) -> bool {
    const DETERMINERS: &[&str] =
        &["THE", "A", "AN", "THIS", "THAT", "THESE", "THOSE", "YOUR", "MY", "OUR", "THEIR", "ITS", "HIS", "HER"];
    upper
        .split(" FROM ")
        .skip(1)
        .filter_map(|rest| rest.split_whitespace().next())
        .any(|word| !DETERMINERS.contains(&word.trim_matches(|c: char| !c.is_alphanumeric())))
}

pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// The longest id the IR accepts; see `validator::valid_id`.
pub const MAX_ID: usize = 80;

/// `{source}--{target}`, kept within [`MAX_ID`].
///
/// Deeply nested package layouts (Go vertical slices, feature folders) make node
/// ids long enough that the joined pair overruns the limit and the whole diagram
/// fails validation. Shorten both halves and pin a hash of the full pair on the
/// end, so the id stays unique and stable across runs.
pub fn edge_id(source: &str, target: &str) -> String {
    let joined = format!("{source}--{target}");
    if joined.len() <= MAX_ID {
        return joined;
    }
    let hash = format!("{:08x}", fnv1a(&joined));
    // 2 separators + the hash; split what is left evenly between the two halves.
    let budget = MAX_ID - hash.len() - 3;
    let half = budget / 2;
    format!("{}--{}-{hash}", clip(source, budget - half), clip(target, half))
}

/// Trim to `max` bytes on a character boundary, without a trailing separator.
fn clip(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].trim_end_matches(['-', '.', '_', ':'])
}

fn fnv1a(s: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in s.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

const ACRONYMS: &[&str] =
    &["ddd", "ml", "api", "db", "ui", "http", "sql", "cli", "sdk", "id", "grpc", "io", "mcp", "ir", "url"];

pub fn display_name(s: &str) -> String {
    s.split(['-', '_', ' ', '/'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            if ACRONYMS.contains(&w.to_lowercase().as_str()) {
                w.to_uppercase()
            } else {
                let mut c = w.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn build_units(repo_name: &str, manifests: &[Manifest], files: &[FileRec]) -> Vec<Unit> {
    // One unit per directory; merge manifests that share a directory.
    let mut by_dir: BTreeMap<String, Manifest> = BTreeMap::new();
    for m in manifests.iter().filter(|m| !m.is_workspace_root) {
        by_dir
            .entry(m.dir.clone())
            .and_modify(|e| e.dependencies.extend(m.dependencies.iter().cloned()))
            .or_insert_with(|| m.clone());
    }
    let mut dirs: Vec<String> = by_dir.keys().cloned().collect();
    // Files outside every manifest: group by top-level directory (a service
    // without a manifest), with loose root-level files forming the root unit.
    let manifest_dirs = dirs.clone();
    let mut extra: BTreeSet<String> = BTreeSet::new();
    for f in files.iter().filter(|f| manifest::owner_dir(&manifest_dirs, &f.path).is_none()) {
        extra.insert(f.path.split_once('/').map(|(top, _)| top.to_string()).unwrap_or_default());
    }
    dirs.extend(extra);
    dirs.sort();
    dirs.dedup();
    let mut units: Vec<Unit> = dirs
        .iter()
        .map(|dir| {
            let m = by_dir.get(dir).cloned();
            let base = if dir.is_empty() { repo_name.to_string() } else { dir.rsplit('/').next().unwrap().to_string() };
            let id = slug(&base);
            Unit {
                id: if id.is_empty() { "app".into() } else { id },
                dir: dir.clone(),
                summary: UnitSummary {
                    id: String::new(),
                    name: display_name(&base),
                    kind: UnitKind::Library,
                    language: m.as_ref().map(|m| m.language).unwrap_or(Language::Rust),
                    root: dir.clone(),
                    manifest: m.as_ref().map(|m| m.file.clone()),
                    frameworks: vec![],
                    tech_stack: String::new(),
                    description: m.as_ref().and_then(|m| m.description.clone()),
                    entry_points: vec![],
                    files: 0,
                    lines: 0,
                    key_symbols: vec![],
                    compose_service: None,
                    aliases: vec![],
                },
                manifest: m,
                files: vec![],
                aliases: vec![],
            }
        })
        .collect();
    for (fi, f) in files.iter().enumerate() {
        if let Some(owner) = manifest::owner_dir(&dirs, &f.path) {
            if let Some(u) = units.iter_mut().find(|u| &u.dir == owner) {
                u.files.push(fi);
            }
        }
    }
    units.retain(|u| !u.files.is_empty());
    // Units sharing a directory name are told apart by their path: each takes
    // the fewest trailing path segments that make it unique (`transport-coap`
    // vs `common-transport-coap`), never an opaque counter.
    let mut by_id: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, u) in units.iter().enumerate() {
        by_id.entry(u.id.clone()).or_default().push(i);
    }
    for group in by_id.values().filter(|g| g.len() > 1) {
        let segs: Vec<Vec<String>> =
            group.iter().map(|&i| units[i].dir.split('/').map(str::to_string).collect()).collect();
        let longest = segs.iter().map(Vec::len).max().unwrap_or(1);
        for k in 2..=longest.max(2) {
            let names: Vec<String> = segs.iter().map(|s| s[s.len().saturating_sub(k)..].join("-")).collect();
            let distinct: BTreeSet<&String> = names.iter().collect();
            if distinct.len() == names.len() || k == longest.max(2) {
                for (&i, name) in group.iter().zip(&names) {
                    let id = slug(if name.is_empty() { repo_name } else { name });
                    units[i].summary.name = display_name(name);
                    units[i].id = id;
                }
                break;
            }
        }
    }
    let mut seen: HashMap<String, usize> = HashMap::new();
    for u in &mut units {
        let n = seen.entry(u.id.clone()).or_default();
        *n += 1;
        if *n > 1 {
            u.id = format!("{}-{}", u.id, n);
        }
        u.summary.id = u.id.clone();
        let mut by_lang: BTreeMap<Language, u64> = BTreeMap::new();
        for &fi in &u.files {
            *by_lang.entry(files[fi].language).or_default() += files[fi].facts.line_count as u64;
        }
        if let Some((lang, _)) = by_lang.iter().max_by_key(|(l, n)| (**n, std::cmp::Reverse(**l))) {
            u.summary.language = *lang;
        }
        u.summary.files = u.files.len();
        u.summary.lines = u.files.iter().map(|&fi| files[fi].facts.line_count as u64).sum();
    }
    units
}

/// Smallest symbol enclosing `line`, as evidence.
pub(crate) fn enclosing_evidence(f: &FileRec, line: u32, note: Option<String>) -> EvidenceRef {
    let sym = f
        .facts
        .symbols
        .iter()
        .filter(|s| {
            s.start_line <= line && s.end_line >= line && s.kind != SymbolKind::Impl && s.kind != SymbolKind::Module
        })
        .min_by_key(|s| s.end_line - s.start_line);
    match sym {
        Some(s) if s.end_line - s.start_line <= 80 => EvidenceRef {
            file_path: f.path.clone(),
            start_line: s.start_line,
            end_line: s.end_line,
            symbol_name: Some(s.name.clone()),
            note,
        },
        _ => EvidenceRef { file_path: f.path.clone(), start_line: line, end_line: line, symbol_name: None, note },
    }
}

fn line_evidence(file: &str, line: u32, note: &str) -> EvidenceRef {
    EvidenceRef {
        file_path: file.to_string(),
        start_line: line,
        end_line: line,
        symbol_name: None,
        note: Some(note.into()),
    }
}

const HTTP_CALLS: &[&str] = &[
    "fetch",
    "axios",
    "request",
    "urlopen",
    "NewRequest",
    "NewRequestWithContext",
    "Get",
    "Post",
    "Do",
    "post",
    "get",
    "put",
    "patch",
    "send",
];
const REDIS_READS: &[&str] = &[
    "get",
    "hget",
    "mget",
    "hgetall",
    "hmget",
    "exists",
    "lrange",
    "lpop",
    "rpop",
    "blpop",
    "brpop",
    "smembers",
    "zrange",
    "xread",
    "xreadgroup",
    "ttl",
    "keys",
    "scan",
];
const REDIS_WRITES: &[&str] = &[
    "set", "setex", "setnx", "hset", "hmset", "del", "expire", "incr", "incrby", "decr", "lpush", "rpush", "sadd",
    "srem", "zadd", "xadd", "publish", "lrem", "ltrim",
];
const SQL_WRITE: &[&str] = &["INSERT INTO ", "DELETE FROM ", "UPSERT ", "MERGE INTO "];
const PRODUCER_CALLS: &[&str] =
    &["publish", "produce", "Publish", "Produce", "WriteMessages", "basic_publish", "sendBatch", "send_message"];
const CONSUMER_CALLS: &[&str] =
    &["subscribe", "Subscribe", "consume", "Consume", "ReadMessage", "FetchMessage", "basic_consume", "eachMessage"];

fn is_sql_update(upper: &str) -> bool {
    upper.find("UPDATE ").is_some_and(|i| upper[i..].contains(" SET "))
}

fn is_topic(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() >= 2
        && parts.len() <= 4
        && parts.iter().all(|p| {
            !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        })
        && s.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && !s.ends_with(".js")
        && !s.ends_with(".ts")
        && !s.ends_with(".py")
        && !s.ends_with(".com")
        && !CLIENT_CONFIG_PREFIXES.iter().any(|p| s.starts_with(p))
}

/// Dotted keys that are client configuration, not topic names.
const CLIENT_CONFIG_PREFIXES: &[&str] = &[
    "group.",
    "bootstrap.",
    "auto.",
    "enable.",
    "client.",
    "session.",
    "max.",
    "fetch.",
    "security.",
    "sasl.",
    "ssl.",
    "request.",
    "retry.",
    "linger.",
    "batch.",
    "acks.",
    "compression.",
    "heartbeat.",
    "isolation.",
    "metadata.",
    // Client and framework configuration namespaces (`kafka.acks`, `spring.kafka.…`).
    "kafka.",
    "spring.",
    "rabbitmq.",
    "amqp.",
    "confluent.",
    "jms.",
    "producer.",
    "consumer.",
    "transactional.",
    "partitioner.",
    "value.",
    "key.",
    "receive.",
    "send.",
    "reconnect.",
    "delivery.",
    "logger.",
    "logging.",
    "log.",
    "queue.",
    "server.",
    "management.",
];

fn detect_infra(ui: usize, unit: &Unit, files: &[FileRec], out: &mut BTreeMap<(usize, InfraKind), InfraUse>) {
    if let Some(m) = &unit.manifest {
        for d in m.dependencies.iter().filter(|d| !d.dev && !d.indirect) {
            if let Some(kind) = catalog::infra_for_package(&d.name) {
                out.entry((ui, kind)).or_default().manifest_evidence.push(line_evidence(
                    &m.file,
                    d.line,
                    &format!("declares dependency `{}`", d.name),
                ));
            }
        }
    }
    for &fi in &unit.files {
        let f = &files[fi];
        let mut kinds_here = BTreeSet::new();
        for imp in &f.facts.imports {
            let pkg = catalog::import_package(&imp.specifier);
            let kind = if f.language.is_jvm() {
                catalog::infra_for_java_import(&imp.specifier)
            } else {
                catalog::infra_for_package(pkg).or_else(|| catalog::infra_for_package(&imp.specifier))
            };
            if let Some(kind) = kind {
                let u = out.entry((ui, kind)).or_default();
                u.importing_files.insert(fi);
                if kinds_here.insert(kind) {
                    u.import_evidence.push(line_evidence(&f.path, imp.line, &format!("imports `{}`", imp.specifier)));
                }
            }
        }
        // SQL access is attributed to whichever SQL store this unit uses.
        // Migrations run once at deploy time; they aren't runtime access.
        let lower_path = f.path.to_lowercase();
        let migration =
            ["migration", "alembic", "migrate", "/versions/", "seed"].iter().any(|w| lower_path.contains(w));
        for s in f.facts.strings.iter().filter(|_| !migration) {
            let upper = s.value.to_uppercase();
            let write = SQL_WRITE.iter().any(|k| upper.contains(k)) || is_sql_update(&upper);
            let read = upper.contains("SELECT ") && upper.contains(" FROM ") && names_a_table(&upper);
            if !(write || read) {
                continue;
            }
            let keys: Vec<InfraKind> = out.keys().filter(|(u, k)| *u == ui && k.is_sql()).map(|(_, k)| *k).collect();
            for kind in keys {
                let ev = enclosing_evidence(f, s.line, Some(first_words(&s.value, 12)));
                let u = out.get_mut(&(ui, kind)).unwrap();
                if write {
                    u.writes.push(ev)
                } else {
                    u.reads.push(ev)
                }
            }
        }
        for kind in kinds_here.iter().copied().filter(|k| k.category() == InfraCategory::EventBus) {
            let u = out.get_mut(&(ui, kind)).unwrap();
            for c in &f.facts.calls {
                let callee_l = c.callee.to_lowercase();
                let produce = PRODUCER_CALLS.contains(&c.name.as_str())
                    || (c.name == "send" && (callee_l.contains("producer") || callee_l.contains("publisher")))
                    || (f.language.is_jvm() && crate::jvm::is_jvm_producer_call(&c.name, &c.callee));
                let consume = CONSUMER_CALLS.contains(&c.name.as_str())
                    || (c.name == "run" && callee_l.contains("consumer"))
                    || (c.name == "poll" && callee_l.contains("consumer"));
                if produce {
                    u.produces.push(enclosing_evidence(f, c.line, Some(format!("`{}()` publishes", c.callee))));
                } else if consume {
                    u.consumes.push(enclosing_evidence(f, c.line, Some(format!("`{}()` consumes", c.callee))));
                }
            }
            for a in &f.facts.annotations {
                let note = format!(
                    "`@{}` {}",
                    a.name,
                    a.owner.as_deref().map(|o| format!("{o}.{}", a.target)).unwrap_or(a.target.clone())
                );
                let ev = EvidenceRef {
                    file_path: f.path.clone(),
                    start_line: a.line,
                    end_line: a.target_end,
                    symbol_name: Some(a.target.clone()),
                    note: Some(note),
                };
                if crate::jvm::LISTENER_ANNOTATIONS.contains(&a.name.as_str()) {
                    u.consumes.push(ev);
                } else if crate::jvm::PRODUCER_ANNOTATIONS.contains(&a.name.as_str()) {
                    u.produces.push(ev);
                }
            }
            u.topics.extend(f.facts.strings.iter().filter(|s| is_topic(&s.value)).map(|s| s.value.clone()));
        }
        if kinds_here.contains(&InfraKind::Smtp) {
            let u = out.get_mut(&(ui, InfraKind::Smtp)).unwrap();
            for c in &f.facts.calls {
                let receiver = c.callee.to_lowercase();
                let sends = matches!(
                    c.name.as_str(),
                    "send"
                        | "sendMail"
                        | "sendMessage"
                        | "send_message"
                        | "sendEmail"
                        | "sendmail"
                        | "SendMail"
                        | "sendAndConfirm"
                ) && ["mail", "smtp", "transporter", "mailer", "emailsender", "postman"]
                    .iter()
                    .any(|w| receiver.contains(w));
                if sends {
                    // The send line itself, so the step lands where it happens in the flow.
                    u.writes.push(line_evidence(&f.path, c.line, &format!("`{}()` sends email", c.callee)));
                }
            }
        }
        if kinds_here.contains(&InfraKind::Redis) {
            let u = out.get_mut(&(ui, InfraKind::Redis)).unwrap();
            for c in &f.facts.calls {
                let n = c.name.to_lowercase();
                // `get`/`set` are everywhere (dicts, cookies, maps): require a cache-like receiver.
                let generic =
                    matches!(n.as_str(), "get" | "set" | "del" | "exists" | "keys" | "scan" | "ttl" | "publish");
                let receiver = c.callee.to_lowercase();
                if generic && !["redis", "cache", "rdb", "valkey", "kv"].iter().any(|w| receiver.contains(w)) {
                    continue;
                }
                if REDIS_READS.contains(&n.as_str()) {
                    u.reads.push(enclosing_evidence(f, c.line, Some(format!("`{}()`", c.callee))));
                } else if REDIS_WRITES.contains(&n.as_str()) {
                    u.writes.push(enclosing_evidence(f, c.line, Some(format!("`{}()`", c.callee))));
                }
            }
        }
    }
}

fn first_words(s: &str, n: usize) -> String {
    let words: Vec<&str> = s.split_whitespace().take(n).collect();
    let mut out = words.join(" ");
    if s.split_whitespace().count() > n {
        out.push('…');
    }
    out
}

fn classify_unit(ui: usize, unit: &mut Unit, files: &[FileRec], infra: &BTreeMap<(usize, InfraKind), InfraUse>) {
    if unit.files.is_empty() {
        return; // discovered from compose; classified at creation
    }
    let mut frameworks: BTreeMap<&'static str, FrameworkRole> = BTreeMap::new();
    if let Some(m) = &unit.manifest {
        for d in &m.dependencies {
            if let Some((label, role)) = catalog::framework_for_package(&d.name) {
                frameworks.insert(label, role);
            }
        }
    }
    let mut serves_http = false;
    for &fi in &unit.files {
        let f = &files[fi];
        for imp in &f.facts.imports {
            let found = if f.language.is_jvm() {
                catalog::framework_for_java_import(&imp.specifier)
            } else {
                catalog::framework_for_package(catalog::import_package(&imp.specifier))
                    .or_else(|| catalog::framework_for_package(&imp.specifier))
            };
            if let Some((label, role)) = found {
                if label != "net/http" {
                    frameworks.insert(label, role);
                }
            }
        }
        if f.facts.calls.iter().any(|c| matches!(c.name.as_str(), "ListenAndServe" | "ListenAndServeTLS")) {
            frameworks.insert("net/http", FrameworkRole::HttpServer);
            serves_http = true;
        }
        for ep in &f.facts.entry_points {
            unit.summary.entry_points.push(EvidenceRef {
                file_path: f.path.clone(),
                start_line: ep.start_line,
                end_line: ep.end_line,
                symbol_name: Some(ep.symbol.clone()),
                note: Some(ep.reason.clone()),
            });
        }
    }
    // The deployable's real entry first: app objects and server bootstraps
    // beat `main` functions, which beat script guards; scripts and
    // migrations come last.
    unit.summary.entry_points.sort_by_key(|e| {
        let note = e.note.as_deref().unwrap_or("");
        let rank = if note.contains("application object") || note.contains("server bootstrap") {
            0
        } else if note.contains("fn main") || note.contains("func main") || note.contains("mount point") {
            1
        } else {
            2
        };
        let incidental = ["script", "migration", "alembic", "seed", "export", "bench"]
            .iter()
            .any(|w| e.file_path.to_lowercase().contains(w));
        (incidental, rank, e.file_path.matches('/').count(), e.file_path.clone(), e.start_line)
    });
    for e in &mut unit.summary.entry_points {
        if e.symbol_name.as_deref().is_some_and(|s| s.starts_with('<')) {
            e.symbol_name = None;
        }
    }
    // Quarkus / Open Liberty / Payara apps have no `main`: the container starts
    // them, and their REST resources are the entry.
    if unit.summary.entry_points.is_empty() && unit.summary.language.is_jvm() {
        let managed =
            frameworks.keys().any(|k| matches!(*k, "Quarkus" | "JAX-RS" | "MicroProfile" | "Helidon MP" | "Jersey"));
        let resource = unit.files.iter().find_map(|&fi| {
            let f = &files[fi];
            f.facts
                .annotations
                .iter()
                .find(|a| matches!(a.name.as_str(), "Path" | "ApplicationPath") && a.target_kind == "class")
                .map(|a| (f, a))
        });
        if let (true, Some((f, a))) = (managed, resource) {
            unit.summary.entry_points.push(EvidenceRef {
                file_path: f.path.clone(),
                start_line: a.line,
                end_line: a.target_end,
                symbol_name: Some(a.target.clone()),
                note: Some(format!("application object: container-managed `@{}` resource", a.name)),
            });
        }
    }
    let has_entry = !unit.summary.entry_points.is_empty();
    let mcp_sdk = unit.manifest.as_ref().is_some_and(|m| m.dependencies.iter().any(|d| catalog::is_mcp_sdk(&d.name)))
        || unit.files.iter().any(|&fi| {
            files[fi].facts.imports.iter().any(|i| catalog::is_mcp_sdk(catalog::import_package(&i.specifier)))
        });
    // A hand-rolled JSON-RPC server that dispatches the MCP method names.
    let mcp_protocol = unit.files.iter().any(|&fi| {
        let strings = &files[fi].facts.strings;
        ["tools/list", "tools/call", "initialize"].iter().all(|m| strings.iter().any(|s| s.value == *m))
    });
    let has = |r: FrameworkRole| frameworks.values().any(|v| *v == r);
    let consumes = infra.iter().any(|((u, _), v)| *u == ui && !v.consumes.is_empty());
    // Vite alone builds CLIs and libraries too; a web client needs a UI framework.
    let ui = frameworks.keys().any(|k| matches!(*k, "React" | "Vue" | "Svelte" | "Angular" | "Solid" | "Next.js"));
    // JVM modules carry framework code (listeners, controllers) inside shared
    // libraries; only an application object makes a deployable.
    let jvm = unit.summary.language.is_jvm();
    let app_object =
        unit.summary.entry_points.iter().any(|e| e.note.as_deref().is_some_and(|n| n.contains("application object")));
    let has_entry =
        if jvm { app_object || (has_entry && !has(FrameworkRole::HttpServer) && !consumes) } else { has_entry };
    let consumes = consumes && (!jvm || app_object);
    unit.summary.kind = if ui && has(FrameworkRole::WebClient) && !frameworks.contains_key("Express") {
        UnitKind::WebClient
    } else if (has(FrameworkRole::HttpServer) || serves_http) && has_entry {
        // A library depending on a web framework (route definitions, extractors) isn't a service.
        UnitKind::HttpService
    } else if has_entry && (mcp_sdk || mcp_protocol) {
        UnitKind::McpServer
    } else if consumes {
        UnitKind::Worker
    } else if has_entry {
        UnitKind::Cli
    } else {
        UnitKind::Library
    };
    // Order frameworks by relevance to the role: the defining one first.
    let mut fw: Vec<(&str, FrameworkRole)> = frameworks.into_iter().collect();
    fw.sort_by_key(|(l, r)| (*r != role_of(unit.summary.kind), *l == "Vite", l.to_string()));
    unit.summary.frameworks = fw.iter().map(|(l, _)| l.to_string()).collect();
    let mut stack = vec![unit.summary.language.display().to_string()];
    stack.extend(unit.summary.frameworks.iter().take(1).cloned());
    unit.summary.tech_stack = stack.join(" · ");
    if unit.summary.description.is_none() {
        let with_entry = unit.files.iter().filter(|&&fi| !files[fi].facts.entry_points.is_empty());
        unit.summary.description = with_entry
            .chain(unit.files.iter())
            .find_map(|&fi| files[fi].facts.module_doc.clone())
            .map(|d| d.lines().next().unwrap_or("").to_string());
    }
    let mut key: Vec<SymbolRef> = unit
        .files
        .iter()
        .flat_map(|&fi| {
            let f = &files[fi];
            f.facts.symbols.iter().filter(|s| s.exported && s.doc.is_some()).map(move |s| SymbolRef {
                name: s.name.clone(),
                kind: s.kind,
                doc: s.doc.clone(),
                evidence: EvidenceRef {
                    file_path: f.path.clone(),
                    start_line: s.start_line,
                    end_line: s.end_line,
                    symbol_name: Some(s.name.clone()),
                    note: None,
                },
            })
        })
        .collect();
    key.sort_by_key(|s| {
        let k = match s.kind {
            SymbolKind::Class | SymbolKind::Struct | SymbolKind::Trait | SymbolKind::Interface => 0,
            SymbolKind::Function => 1,
            _ => 2,
        };
        (k, s.evidence.file_path.clone(), s.evidence.start_line)
    });
    key.truncate(12);
    unit.summary.key_symbols = key;
}

fn role_of(kind: UnitKind) -> FrameworkRole {
    match kind {
        UnitKind::WebClient => FrameworkRole::WebClient,
        UnitKind::Cli | UnitKind::Library => FrameworkRole::Cli,
        _ => FrameworkRole::HttpServer,
    }
}

/// An entry point when there is one; otherwise the unit's root module (crate
/// root, package index, `__init__`), which is what a library "is".
fn unit_evidence(u: &Unit, files: &[FileRec]) -> Option<EvidenceRef> {
    if let Some(ep) = u.summary.entry_points.first() {
        return Some(ep.clone());
    }
    let rel = |fi: usize| {
        let p = &files[fi].path;
        if u.dir.is_empty() {
            p.as_str()
        } else {
            &p[u.dir.len() + 1..]
        }
    };
    let dir_name = u.dir.rsplit('/').next().unwrap_or("");
    let rank = |fi: usize| -> Option<(usize, usize)> {
        let r = rel(fi);
        let depth = r.matches('/').count();
        let name = r.rsplit('/').next().unwrap_or(r);
        let score = match name {
            "lib.rs" => 0,
            "index.ts" | "index.tsx" | "index.js" | "index.mjs" => 1,
            "__init__.py" => 2,
            "main.ts" | "main.js" | "mod.ts" => 3,
            n if n == format!("{dir_name}.go") || n == "doc.go" => 1,
            _ => return None,
        };
        Some((depth, score))
    };
    let root = u
        .files
        .iter()
        .copied()
        .filter(|&fi| files[fi].facts.line_count > 0)
        .filter_map(|fi| rank(fi).map(|k| (k, fi)))
        .min()
        .map(|(_, fi)| fi);
    match root {
        Some(fi) => {
            let f = &files[fi];
            let doc_lines = f.facts.module_doc.as_ref().map(|d| d.lines().count() as u32).unwrap_or(0);
            let end = if doc_lines > 0 { doc_lines } else { f.facts.line_count.min(20) }.clamp(1, 40);
            Some(EvidenceRef {
                file_path: f.path.clone(),
                start_line: 1,
                end_line: end.min(f.facts.line_count.max(1)),
                symbol_name: None,
                note: Some("module root".into()),
            })
        }
        None => u.summary.key_symbols.first().map(|s| s.evidence.clone()),
    }
}

#[derive(Default)]
struct RelBuilder {
    map: BTreeMap<(String, String), Relationship>,
}

impl RelBuilder {
    fn add(
        &mut self,
        source: &str,
        target: &str,
        edge_type: EdgeType,
        label: &str,
        src: RelationSource,
        ev: Vec<EvidenceRef>,
    ) {
        if source == target {
            return;
        }
        let r = self.map.entry((source.to_string(), target.to_string())).or_insert_with(|| Relationship {
            id: String::new(),
            source: source.to_string(),
            target: target.to_string(),
            edge_type,
            label: label.to_string(),
            sources: vec![],
            evidence: vec![],
        });
        // Code evidence is more specific than compose/manifest evidence.
        if src == RelationSource::Code && !r.sources.contains(&RelationSource::Code) {
            r.edge_type = edge_type;
            r.label = label.to_string();
            let mut merged = ev.clone();
            merged.append(&mut r.evidence);
            r.evidence = merged;
        } else {
            r.evidence.extend(ev);
        }
        if !r.sources.contains(&src) {
            r.sources.push(src);
            r.sources.sort();
        }
        r.evidence.dedup();
        r.evidence.truncate(6);
    }

    /// Adds evidence to an existing relationship in either direction (a
    /// consumer's `depends_on: kafka` backs the `kafka → consumer` edge).
    fn corroborate(&mut self, a: &str, b: &str, src: RelationSource, ev: EvidenceRef) {
        let key = [(a, b), (b, a)]
            .into_iter()
            .map(|(x, y)| (x.to_string(), y.to_string()))
            .find(|k| self.map.contains_key(k));
        if let Some(k) = key {
            let r = self.map.get_mut(&k).unwrap();
            if !r.sources.contains(&src) {
                r.sources.push(src);
                r.sources.sort();
            }
            if r.evidence.len() < 6 && !r.evidence.contains(&ev) {
                r.evidence.push(ev);
            }
        }
    }

    fn finish(self) -> Vec<Relationship> {
        self.map
            .into_values()
            .map(|mut r| {
                r.id = edge_id(&r.source, &r.target);
                r
            })
            .collect()
    }
}

fn third_party_verb(kind: InfraKind) -> &'static str {
    match kind {
        InfraKind::Stripe => "charges cards",
        InfraKind::Sendgrid => "sends email",
        InfraKind::Twilio => "sends SMS",
        InfraKind::Openai | InfraKind::Anthropic => "completions",
        InfraKind::Slack => "posts messages",
        InfraKind::Smtp => "sends email",
        _ => "calls API",
    }
}

fn collect_infra(
    units: &[Unit],
    files: &[FileRec],
    uses: &BTreeMap<(usize, InfraKind), InfraUse>,
    rels: &mut RelBuilder,
) -> Vec<InfraSummary> {
    let mut by_kind: BTreeMap<InfraKind, InfraSummary> = BTreeMap::new();
    for ((ui, kind), u) in uses {
        let unit = &units[*ui];
        let entry = by_kind.entry(*kind).or_insert_with(|| InfraSummary {
            id: kind.id().to_string(),
            kind: *kind,
            label: kind.label().to_string(),
            category: kind.category(),
            role: kind.role().to_string(),
            used_by: vec![],
            evidence: vec![],
            compose_service: None,
            topics: vec![],
        });
        entry.used_by.push(unit.id.clone());
        entry.evidence.extend(u.import_evidence.iter().take(2).cloned());
        entry.evidence.extend(u.manifest_evidence.iter().take(1).cloned());
        for t in &u.topics {
            if !entry.topics.contains(t) {
                entry.topics.push(t.clone());
            }
        }
        // A SQL store reached through an ORM: the ORM import is the code evidence.
        let orm = if u.import_evidence.is_empty() && kind.is_sql() { orm_import_sites(unit, files) } else { vec![] };
        let has_code = !u.import_evidence.is_empty() || !orm.is_empty();
        let src = if has_code { RelationSource::Code } else { RelationSource::Manifest };
        let base_ev: Vec<EvidenceRef> = if !u.import_evidence.is_empty() {
            u.import_evidence.clone()
        } else if !orm.is_empty() {
            orm.clone()
        } else {
            u.manifest_evidence.clone()
        };
        match kind.category() {
            InfraCategory::Storage => {
                let (edge, label, mut ev) = if !u.writes.is_empty() {
                    let label = if kind.is_sql() {
                        sql_label(&u.writes, "writes")
                    } else if !u.reads.is_empty() {
                        "reads & writes".into()
                    } else {
                        "writes".into()
                    };
                    (EdgeType::Write, label, u.writes.clone())
                } else if !u.reads.is_empty() {
                    let label = if kind.is_sql() { sql_label(&u.reads, "reads") } else { "reads".into() };
                    (EdgeType::Read, label, u.reads.clone())
                } else if !orm.is_empty() {
                    (EdgeType::Sync, "queries".to_string(), vec![])
                } else {
                    (EdgeType::Sync, "connects".to_string(), vec![])
                };
                ev.extend(base_ev);
                rels.add(&unit.id, kind.id(), edge, &label, src, ev);
            }
            InfraCategory::EventBus => {
                // A topic seen on both the producing and consuming side is the real one.
                let shared = u
                    .topics
                    .iter()
                    .find(|t| uses.iter().any(|((other, k), ou)| other != ui && k == kind && ou.topics.contains(*t)));
                let topic = shared.or(u.topics.iter().next()).cloned();
                if !u.produces.is_empty() {
                    let label = topic.clone().map(|t| format!("publishes {t}")).unwrap_or_else(|| "publishes".into());
                    rels.add(&unit.id, kind.id(), EdgeType::Event, &label, RelationSource::Code, u.produces.clone());
                }
                if !u.consumes.is_empty() {
                    let label = topic.map(|t| format!("delivers {t}")).unwrap_or_else(|| "delivers events".into());
                    rels.add(kind.id(), &unit.id, EdgeType::Event, &label, RelationSource::Code, u.consumes.clone());
                }
                if u.produces.is_empty() && u.consumes.is_empty() {
                    rels.add(&unit.id, kind.id(), EdgeType::Async, "connects", src, base_ev);
                }
            }
            InfraCategory::ThirdParty => {
                // Call sites first: they say where the vendor is actually used.
                let mut ev = u.writes.clone();
                ev.extend(base_ev);
                rels.add(&unit.id, kind.id(), EdgeType::Sync, third_party_verb(*kind), src, ev);
            }
        }
    }
    by_kind
        .into_values()
        .map(|mut i| {
            i.used_by.sort();
            i.used_by.dedup();
            i.evidence.dedup();
            i
        })
        .collect()
}

/// "writes orders" from `INSERT INTO orders …`.
fn sql_label(evs: &[EvidenceRef], verb: &str) -> String {
    let mut tables: BTreeSet<String> = BTreeSet::new();
    for e in evs {
        let Some(note) = &e.note else { continue };
        let words: Vec<&str> = note.split_whitespace().collect();
        for (i, w) in words.iter().enumerate() {
            let wu = w.to_uppercase();
            if matches!(wu.as_str(), "INTO" | "FROM" | "UPDATE") {
                if let Some(t) = words.get(i + 1) {
                    let t: String = t.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.').collect();
                    let keyword = matches!(t.to_uppercase().as_str(), "SET" | "SELECT" | "VALUES" | "ONLY" | "LATERAL");
                    if t.len() >= 3 && !keyword {
                        tables.insert(t.to_lowercase());
                    }
                }
            }
        }
    }
    let t: Vec<String> = tables.into_iter().take(2).collect();
    if t.is_empty() {
        verb.to_string()
    } else {
        format!("{verb} {}", t.join(", "))
    }
}

/// Maps a compose service to a scanned unit: build context, Dockerfile
/// directory, then the service or image name with the repository prefix and
/// role suffixes (`-http`, `-grpc`, …) removed.
fn unit_for_service(s: &compose::ComposeService, repo_slug: &str, units: &[Unit]) -> Option<usize> {
    let by_dir = |dir: &str| units.iter().position(|u| u.dir == dir);
    if let Some(i) = s.dockerfile_dir.as_deref().and_then(by_dir) {
        return Some(i);
    }
    if let Some(i) = s.build_dir.as_deref().filter(|d| !d.is_empty()).and_then(by_dir) {
        return Some(i);
    }
    let image_base = s
        .image
        .as_deref()
        .map(|i| i.rsplit('/').next().unwrap_or(i).split([':', '@']).next().unwrap_or("").to_string());
    for name in std::iter::once(s.name.clone()).chain(image_base) {
        if let Some(i) = units.iter().position(|u| u.aliases.contains(&name)) {
            return Some(i);
        }
        for candidate in name_variants(&name, repo_slug) {
            if let Some(i) = units.iter().position(|u| u.id == candidate) {
                return Some(i);
            }
        }
    }
    s.build_dir.as_deref().filter(|d| d.is_empty()).and_then(by_dir)
}

/// A unit addressed by a service id or host name: aliases first, then id variants.
fn unit_for_name(name: &str, repo_slug: &str, units: &[Unit]) -> Option<usize> {
    if let Some(i) = units.iter().position(|u| u.aliases.iter().any(|a| a == name)) {
        return Some(i);
    }
    name_variants(name, repo_slug).iter().find_map(|c| units.iter().position(|u| &u.id == c))
}

fn name_variants(name: &str, repo_slug: &str) -> Vec<String> {
    const ROLE_SUFFIXES: &[&str] =
        &["-http", "-grpc", "-api", "-service", "-svc", "-app", "-dev", "-prod", "-server", "-worker"];
    let base = slug(name);
    let mut out = vec![base.clone()];
    let unprefixed = [format!("{repo_slug}-"), "app-".to_string()]
        .iter()
        .find_map(|p| base.strip_prefix(p.as_str()).map(str::to_string))
        .filter(|s| !s.is_empty());
    for n in [Some(base.clone()), unprefixed].into_iter().flatten() {
        if !out.contains(&n) {
            out.push(n.clone());
        }
        for suffix in ROLE_SUFFIXES {
            if let Some(stripped) = n.strip_suffix(suffix).filter(|s| !s.is_empty()) {
                if !out.contains(&stripped.to_string()) {
                    out.push(stripped.to_string());
                }
            }
        }
    }
    out
}

fn is_test_service(name: &str) -> bool {
    let n = name.to_lowercase();
    [
        "test",
        "e2e",
        "migrat",
        "seed",
        "init",
        "setup",
        "adminer",
        "pgadmin",
        "proxy",
        "traefik",
        "nginx",
        "prometheus",
        "grafana",
        "mailhog",
        "mailpit",
    ]
    .iter()
    .any(|w| n.contains(w))
}

/// Services built from source the analyzer can't parse (C#, Java, …) still
/// belong on the map: create a unit from the build directory.
fn add_compose_only_units(root: &Path, repo_slug: &str, compose: &[ComposeFile], units: &mut Vec<Unit>) {
    for c in compose {
        for s in &c.services {
            let Some(dir) = s.dockerfile_dir.clone().or_else(|| s.build_dir.clone()).filter(|d| !d.is_empty()) else {
                continue;
            };
            if is_test_service(&s.name) || unit_for_service(s, repo_slug, units).is_some() {
                continue;
            }
            if s.image.as_deref().and_then(catalog::infra_for_image).is_some()
                || catalog::infra_for_image(&s.name).is_some()
            {
                continue;
            }
            let census = language_census(&root.join(&dir));
            let Some(language) = census else { continue };
            let id = name_variants(&s.name, repo_slug).pop().unwrap_or_else(|| slug(&s.name));
            if units.iter().any(|u| u.id == id) {
                continue;
            }
            let dockerfile =
                [format!("{dir}/Dockerfile"), format!("{dir}/dockerfile")].into_iter().find(|p| root.join(p).is_file());
            let evidence = match &dockerfile {
                Some(p) => {
                    let lines =
                        std::fs::read_to_string(root.join(p)).map(|t| t.lines().count() as u32).unwrap_or(1).max(1);
                    EvidenceRef {
                        file_path: p.clone(),
                        start_line: 1,
                        end_line: lines.min(20),
                        symbol_name: None,
                        note: Some("Dockerfile".into()),
                    }
                }
                None => line_evidence(&c.file, s.line, &format!("compose service `{}`", s.name)),
            };
            units.push(Unit {
                id: id.clone(),
                dir: dir.clone(),
                manifest: None,
                files: vec![],
                summary: UnitSummary {
                    id,
                    name: display_name(&s.name),
                    kind: if s.publishes_ports { UnitKind::HttpService } else { UnitKind::Worker },
                    language,
                    root: dir,
                    manifest: None,
                    frameworks: vec![],
                    tech_stack: format!("{} · not parsed", language.display()),
                    description: Some(format!("Built from `{}`; {} source is not parsed", s.name, language.display())),
                    entry_points: vec![evidence],
                    files: 0,
                    lines: 0,
                    key_symbols: vec![],
                    compose_service: Some(s.name.clone()),
                    aliases: vec![],
                },
                aliases: vec![],
            });
        }
    }
}

/// Dominant unparsed language in a directory (bounded walk).
fn language_census(dir: &Path) -> Option<Language> {
    let mut counts: BTreeMap<Language, usize> = BTreeMap::new();
    let walker = ignore::WalkBuilder::new(dir).hidden(true).require_git(false).max_depth(Some(6)).build();
    for entry in walker.flatten().take(5000) {
        if let Some(lang) = entry.path().extension().and_then(|e| e.to_str()).and_then(Language::unparsed_for_extension)
        {
            *counts.entry(lang).or_default() += 1;
        }
    }
    counts.into_iter().max_by_key(|(l, n)| (*n, std::cmp::Reverse(*l))).map(|(l, _)| l)
}

#[allow(clippy::too_many_arguments)]
fn apply_compose(
    repo_root: &Path,
    compose: &[ComposeFile],
    repo_slug: &str,
    units: &mut [Unit],
    files: &[FileRec],
    infra: &mut Vec<InfraSummary>,
    rels: &mut RelBuilder,
    uses: &BTreeMap<(usize, InfraKind), InfraUse>,
) {
    // service name → element id, across all compose files (first definition wins).
    let mut service_ids: HashMap<String, String> = HashMap::new();
    for c in compose {
        for s in &c.services {
            if service_ids.contains_key(&s.name) {
                continue;
            }
            if let Some(kind) = s.image.as_deref().and_then(catalog::infra_for_image).or_else(|| {
                (s.build_dir.is_some() && !is_test_service(&s.name))
                    .then(|| catalog::infra_for_image(&s.name))
                    .flatten()
            }) {
                let ev = line_evidence(
                    &c.file,
                    s.line,
                    &format!("compose service `{}` ({})", s.name, s.image.as_deref().unwrap_or("built locally")),
                );
                match infra.iter_mut().find(|i| i.kind == kind) {
                    Some(i) => {
                        i.compose_service.get_or_insert_with(|| s.name.clone());
                        i.evidence.push(ev);
                    }
                    None => infra.push(InfraSummary {
                        id: kind.id().to_string(),
                        kind,
                        label: kind.label().to_string(),
                        category: kind.category(),
                        role: kind.role().to_string(),
                        used_by: vec![],
                        evidence: vec![ev],
                        compose_service: Some(s.name.clone()),
                        topics: vec![],
                    }),
                }
                service_ids.insert(s.name.clone(), kind.id().to_string());
                continue;
            }
            if is_test_service(&s.name) {
                continue;
            }
            if let Some(ui) = unit_for_service(s, repo_slug, units) {
                units[ui].summary.compose_service.get_or_insert_with(|| s.name.clone());
                service_ids.insert(s.name.clone(), units[ui].id.clone());
            }
        }
    }
    infra.sort_by_key(|i| i.kind);
    for c in compose {
        for s in &c.services {
            let Some(src_id) = service_ids.get(&s.name).cloned() else { continue };
            let Some(ui) = units.iter().position(|u| u.id == src_id) else { continue };
            // (host, evidence, env key, must be confirmed in code)
            let mut targets: Vec<(String, EvidenceRef, Option<String>, bool)> = Vec::new();
            for d in &s.depends_on {
                let line = find_line_after(c, s.line, &format!("- {d}"))
                    .or_else(|| find_line_after(c, s.line, &format!("{d}:")))
                    .unwrap_or(s.line);
                targets.push((
                    d.clone(),
                    line_evidence(&c.file, line, &format!("`{}` depends_on `{d}`", s.name)),
                    None,
                    false,
                ));
            }
            for (k, v) in &s.environment {
                for host in compose::hosts_in(v) {
                    let line = find_line_after(c, s.line, k).unwrap_or(s.line);
                    targets.push((host, line_evidence(&c.file, line, &format!("{k}={v}")), Some(k.clone()), false));
                }
            }
            // A shared env_file hands every service every address; only keep
            // the ones this service's code (or a library it uses) reads.
            for f in &s.env_files {
                let text = std::fs::read_to_string(repo_root.join(f)).unwrap_or_default();
                for (k, v) in &s.env_file_vars {
                    let line = text
                        .lines()
                        .position(|l| l.trim_start().starts_with(&format!("{k}=")))
                        .map(|i| i as u32 + 1)
                        .unwrap_or(1);
                    for host in compose::hosts_in(v) {
                        targets.push((host, line_evidence(f, line, &format!("{k}={v}")), Some(k.clone()), true));
                    }
                }
            }
            // Addresses hard-coded as defaults in the service's own code
            // (`'http://immich-machine-learning:3003'`).
            for f in units[ui].files.iter().map(|&fi| &files[fi]) {
                for lit in &f.facts.strings {
                    for host in literal_hosts(&lit.value) {
                        if service_ids.contains_key(&host) {
                            targets.push((
                                host,
                                enclosing_evidence(f, lit.line, Some(format!("references `{}`", lit.value))),
                                None,
                                false,
                            ));
                        }
                    }
                }
            }
            for (host, ev, env_key, needs_code) in targets {
                let Some(target_id) = service_ids.get(&host).cloned() else { continue };
                if target_id == src_id {
                    continue;
                }
                if let Some(ti) = units.iter().position(|u| u.id == target_id) {
                    let mut evidence = code_reference(&units[ui], files, env_key.as_deref(), &host);
                    if evidence.is_empty() {
                        evidence = library_code_reference(&units[ui], units, files, rels, env_key.as_deref());
                    }
                    if needs_code && evidence.is_empty() {
                        continue;
                    }
                    let src = if evidence.is_empty() { RelationSource::Compose } else { RelationSource::Code };
                    evidence.push(ev);
                    let grpc =
                        env_key.as_deref().is_some_and(|k| k.to_uppercase().contains("GRPC")) || host.contains("grpc");
                    let label = match units[ti].summary.kind {
                        _ if grpc => "gRPC",
                        UnitKind::HttpService => "HTTP",
                        _ => "calls",
                    };
                    rels.add(&src_id, &target_id, EdgeType::Sync, label, src, evidence);
                    if src == RelationSource::Code {
                        rels.add(&src_id, &target_id, EdgeType::Sync, label, RelationSource::Compose, vec![]);
                    }
                } else if let Some(i) = infra.iter_mut().find(|i| i.id == target_id) {
                    if needs_code {
                        continue;
                    }
                    if !i.used_by.contains(&src_id) {
                        i.used_by.push(src_id.clone());
                        i.used_by.sort();
                    }
                    if uses.contains_key(&(ui, i.kind)) {
                        rels.corroborate(&src_id, &target_id, RelationSource::Compose, ev);
                    } else {
                        // Reached through an ORM or driver the catalog doesn't name the store for.
                        let orm = if i.kind.is_sql() { orm_import_sites(&units[ui], files) } else { vec![] };
                        let edge = if i.category == InfraCategory::EventBus { EdgeType::Async } else { EdgeType::Sync };
                        let (label, source) = if orm.is_empty() {
                            ("connects", RelationSource::Compose)
                        } else {
                            ("queries", RelationSource::Code)
                        };
                        let mut evidence = orm;
                        evidence.push(ev);
                        rels.add(&src_id, &target_id, edge, label, source, evidence);
                    }
                }
            }
        }
    }
}

/// Hostnames in URL literals (`scheme://host[:port]`).
fn literal_hosts(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = value;
    while let Some(i) = rest.find("://") {
        rest = &rest[i + 3..];
        let host: String =
            rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.').collect();
        if !host.is_empty() && host.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            out.push(host);
        }
    }
    out
}

fn orm_import_sites(unit: &Unit, files: &[FileRec]) -> Vec<EvidenceRef> {
    let mut out = Vec::new();
    for &fi in &unit.files {
        let f = &files[fi];
        if let Some(i) = f
            .facts
            .imports
            .iter()
            .find(|i| catalog::is_orm(&i.specifier) || catalog::is_orm(catalog::import_package(&i.specifier)))
        {
            out.push(line_evidence(&f.path, i.line, &format!("imports `{}`", i.specifier)));
            if out.len() == 2 {
                break;
            }
        }
    }
    out
}

/// A web client with no observed link to a backend: when its code references
/// an API base URL and there is one obvious HTTP service, draw the call.
fn link_web_clients(units: &[Unit], files: &[FileRec], rels: &mut RelBuilder) {
    let services: Vec<&Unit> = units.iter().filter(|u| u.summary.kind == UnitKind::HttpService).collect();
    for web in units.iter().filter(|u| u.summary.kind == UnitKind::WebClient) {
        let already = rels.map.keys().any(|(s, t)| s == &web.id && services.iter().any(|x| &x.id == t));
        if already || services.is_empty() {
            continue;
        }
        let target = if services.len() == 1 {
            Some(services[0])
        } else {
            let named: Vec<&&Unit> = services
                .iter()
                .filter(|u| ["api", "backend", "server", "gateway"].iter().any(|w| u.id.split('-').any(|p| p == *w)))
                .collect();
            (named.len() == 1).then(|| *named[0])
        };
        let Some(target) = target else { continue };
        let mut evidence = Vec::new();
        for &fi in &web.files {
            let f = &files[fi];
            for s in &f.facts.strings {
                let upper = s.value.to_uppercase();
                let api_ref = (upper.contains("API") || upper.contains("BACKEND"))
                    && (upper.ends_with("_URL")
                        || upper.ends_with("_BASE")
                        || upper.ends_with("_BASE_URL")
                        || upper.ends_with("_HOST")
                        || upper.ends_with("_ORIGIN"));
                if api_ref || s.value.starts_with("/api/") || s.value == "/api" {
                    evidence.push(enclosing_evidence(f, s.line, Some(format!("references `{}`", s.value))));
                    break;
                }
            }
            if evidence.len() == 2 {
                break;
            }
        }
        if !evidence.is_empty() {
            rels.add(&web.id, &target.id, EdgeType::Sync, "HTTP", RelationSource::Code, evidence);
        }
    }
}

/// Line of `needle` inside the indented block of the service declared at `service_line`.
fn find_line_after(c: &ComposeFile, service_line: u32, needle: &str) -> Option<u32> {
    let indent = |l: &str| l.len() - l.trim_start().len();
    let lines: Vec<&str> = c.text.lines().collect();
    let base = indent(lines.get(service_line as usize - 1)?);
    lines
        .iter()
        .enumerate()
        .skip(service_line as usize)
        .take_while(|(_, l)| l.trim().is_empty() || indent(l) > base)
        .find(|(_, l)| l.trim_start().starts_with(needle))
        .map(|(i, _)| i as u32 + 1)
}

/// Env-var reads that live in a workspace library count for a unit when the
/// unit calls the library function that reads them (`NewTrainerClient` reads
/// `TRAINER_GRPC_ADDR`; the service that calls it talks to the trainer).
fn library_code_reference(
    unit: &Unit,
    units: &[Unit],
    files: &[FileRec],
    rels: &RelBuilder,
    env_key: Option<&str>,
) -> Vec<EvidenceRef> {
    let Some(key) = env_key else { return vec![] };
    let libs: Vec<&Unit> = rels
        .map
        .values()
        .filter(|r| r.source == unit.id && r.label == "uses")
        .filter_map(|r| units.iter().find(|u| u.id == r.target))
        .collect();
    let mut out = Vec::new();
    for lib in libs {
        for &fi in &lib.files {
            let f = &files[fi];
            for s in f.facts.strings.iter().filter(|s| s.value == key) {
                let reader = enclosing_evidence(f, s.line, Some(format!("reads `{key}`")));
                let Some(func) = reader.symbol_name.clone() else { continue };
                let call = unit.files.iter().find_map(|&ufi| {
                    files[ufi].facts.calls.iter().find(|c| c.name == func).map(|c| (ufi, c.line, c.callee.clone()))
                });
                if let Some((ufi, line, callee)) = call {
                    out.push(enclosing_evidence(&files[ufi], line, Some(format!("calls `{callee}()`"))));
                    out.push(reader);
                    return out;
                }
            }
        }
    }
    out
}

/// Where in the caller's code the dependency is actually used: the env var
/// name or the target hostname appearing as a literal.
fn code_reference(unit: &Unit, files: &[FileRec], env_key: Option<&str>, host: &str) -> Vec<EvidenceRef> {
    let host_prefix = format!("://{host}");
    let mut out = Vec::new();
    for &fi in &unit.files {
        let f = &files[fi];
        let mut file_hit = false;
        for s in &f.facts.strings {
            let hit = env_key.is_some_and(|k| s.value == k) || s.value.contains(&host_prefix);
            if hit {
                file_hit = true;
                let ev = enclosing_evidence(f, s.line, Some(format!("references `{}`", s.value)));
                if !out.contains(&ev) {
                    out.push(ev);
                }
            }
        }
        // The address is usually a module constant; the call site that uses it
        // is the better pin.
        if file_hit {
            if let Some(c) = f.facts.calls.iter().find(|c| HTTP_CALLS.contains(&c.name.as_str())) {
                let ev = enclosing_evidence(f, c.line, Some(format!("`{}()` call", c.callee)));
                if !out.contains(&ev) {
                    out.insert(0, ev);
                }
            }
        }
    }
    // Prefer references inside a named symbol over bare module-level constants.
    out.sort_by_key(|e| (e.symbol_name.is_none(), e.file_path.clone(), e.start_line));
    out.truncate(3);
    out
}

/// Without compose: `http://payments:8080` literals and `PAYMENTS_URL`-style
/// env names that name another unit become sync edges.
fn infer_http_links(units: &[Unit], files: &[FileRec], rels: &mut RelBuilder) {
    for (ui, u) in units.iter().enumerate() {
        for (ti, t) in units.iter().enumerate() {
            if ui == ti || t.summary.kind != UnitKind::HttpService {
                continue;
            }
            let upper = t.id.to_uppercase().replace('-', "_");
            let env_names = [
                format!("{upper}_URL"),
                format!("{upper}_SERVICE_URL"),
                format!("{upper}_HOST"),
                format!("{upper}_ADDR"),
            ];
            let mut ev = Vec::new();
            for &fi in &u.files {
                let f = &files[fi];
                for s in &f.facts.strings {
                    if env_names.contains(&s.value) || s.value.contains(&format!("://{}", t.id)) {
                        ev.push(enclosing_evidence(f, s.line, Some(format!("references `{}`", s.value))));
                    }
                }
            }
            if !ev.is_empty() {
                ev.dedup();
                rels.add(&u.id, &t.id, EdgeType::Sync, "HTTP", RelationSource::Code, ev);
            }
        }
    }
}

fn link_workspace_libraries(units: &[Unit], files: &[FileRec], rels: &mut RelBuilder) {
    for u in units {
        let Some(m) = &u.manifest else { continue };
        for d in m.dependencies.iter().filter(|d| !d.dev && !d.indirect) {
            let target = units.iter().find(|t| {
                t.id != u.id
                    && t.manifest.as_ref().is_some_and(|tm| same_ecosystem(tm.kind, m.kind))
                    && (t.manifest.as_ref().and_then(|tm| tm.name.as_deref()) == Some(d.name.as_str())
                        // Maven coordinates `group:artifact` name a sibling module by artifactId.
                        || matches!(m.kind, manifest::ManifestKind::Maven | manifest::ManifestKind::Gradle)
                            && d.name.contains(':')
                            && t.manifest.as_ref().and_then(|tm| tm.name.as_deref()) == d.name.rsplit(':').next()
                        || d.path.as_deref().is_some_and(|p| {
                            let joined = manifest::normalize(&Path::new(&u.dir).join(p));
                            joined.to_string_lossy().trim_matches('/') == t.dir
                        }))
            });
            let Some(t) = target else { continue };
            let mut evidence = import_sites(u, files, t, &d.name);
            let source = if evidence.is_empty() { RelationSource::Manifest } else { RelationSource::Code };
            evidence.push(line_evidence(&m.file, d.line, &format!("depends on `{}`", d.name)));
            rels.add(&u.id, &t.id, EdgeType::Sync, "uses", source, evidence);
            if source == RelationSource::Code {
                rels.add(&u.id, &t.id, EdgeType::Sync, "uses", RelationSource::Manifest, vec![]);
            }
        }
    }
}

/// A crate/package that other units link against and that has a library
/// root is a library, even if it also ships a small helper binary.
fn demote_linked_libraries(units: &mut [Unit], files: &[FileRec], rels: &RelBuilder) {
    let linked: BTreeSet<String> = rels.map.values().filter(|r| r.label == "uses").map(|r| r.target.clone()).collect();
    for u in units.iter_mut() {
        if u.summary.kind == UnitKind::Library || u.summary.kind == UnitKind::WebClient || !linked.contains(&u.id) {
            continue;
        }
        let has_lib_root = u.files.iter().any(|&fi| {
            let name = files[fi].path.rsplit('/').next().unwrap_or("");
            matches!(name, "lib.rs" | "index.ts" | "index.js" | "__init__.py")
        }) || (u.summary.language.is_jvm()
            && !u
                .summary
                .entry_points
                .iter()
                .any(|e| e.note.as_deref().is_some_and(|n| n.contains("application object"))));
        if has_lib_root {
            u.summary.kind = UnitKind::Library;
        }
    }
}

fn same_ecosystem(a: manifest::ManifestKind, b: manifest::ManifestKind) -> bool {
    use manifest::ManifestKind::*;
    let family = |k| match k {
        Cargo => 0,
        PackageJson => 1,
        GoMod => 2,
        Pyproject | Requirements => 3,
        Maven | Gradle => 4,
    };
    family(a) == family(b)
}

/// Where `unit` actually imports the workspace package `dep` (Rust crate
/// names use underscores, Python packages likewise, Go uses the module path).
fn import_sites(unit: &Unit, files: &[FileRec], target: &Unit, dep: &str) -> Vec<EvidenceRef> {
    let snake = dep.replace('-', "_");
    // JVM imports name packages, so a module is identified by the packages its sources declare.
    let target_packages: BTreeSet<&str> =
        target.files.iter().filter_map(|&fi| files[fi].facts.package.as_deref()).collect();
    let module = target.manifest.as_ref().and_then(|m| m.name.clone()).unwrap_or_else(|| dep.to_string());
    let mut out = Vec::new();
    for &fi in &unit.files {
        let f = &files[fi];
        let hit = f.facts.imports.iter().find(|i| {
            let s = i.specifier.as_str();
            let seg = |name: &str| {
                s == name
                    || s.starts_with(&format!("{name}::"))
                    || s.starts_with(&format!("{name}/"))
                    || s.starts_with(&format!("{name}."))
            };
            match f.language {
                Language::Rust => seg(&snake),
                Language::Python => seg(&snake),
                Language::Go => seg(&module),
                Language::TypeScript | Language::JavaScript => seg(dep),
                Language::Java | Language::Kotlin => target_packages
                    .iter()
                    .any(|p| s.strip_prefix(p).is_some_and(|rest| rest.starts_with('.') && !rest[1..].contains('.'))),
                _ => false,
            }
        });
        // Rust code often calls `other_crate::f()` without a `use`.
        let qualified = (f.language == Language::Rust)
            .then(|| f.facts.calls.iter().find(|c| c.callee.starts_with(&format!("{snake}::"))))
            .flatten();
        if hit.is_none() {
            if let Some(c) = qualified {
                out.push(line_evidence(&f.path, c.line, &format!("calls `{}`", c.callee)));
            }
        }
        if let Some(i) = hit {
            out.push(line_evidence(&f.path, i.line, &format!("imports `{}`", i.specifier)));
            if out.len() == 3 {
                break;
            }
        }
    }
    out
}

fn read_readme_summary(root: &Path) -> Option<String> {
    let text = ["README.md", "readme.md", "README"].iter().find_map(|f| std::fs::read_to_string(root.join(f)).ok())?;
    summary_from_markdown(&text)
}

/// First prose paragraph of a README: skips headings, badges, HTML blocks and
/// code fences, and strips inline markup.
/// A Markdown list item, bulleted or numbered.
fn structural_list(line: &str) -> bool {
    line.starts_with("- ")
        || line.starts_with("* ")
        || line.starts_with("+ ")
        || line.split_once(". ").is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

pub fn summary_from_markdown(text: &str) -> Option<String> {
    let mut in_fence = false;
    let mut in_list = false;
    let mut paragraph: Vec<String> = Vec::new();
    let mut candidates: Vec<String> = Vec::new();
    let mut taglines: Vec<String> = Vec::new();
    for raw in text.lines().chain(std::iter::once("")) {
        let line = raw.trim();
        // Centered HTML headers often carry the one-line tagline.
        if line.starts_with("<h") && line.contains("</h") {
            if let Some(t) = prose_of(line) {
                taglines.push(t);
            }
        }
        if line.starts_with("```") || line.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        // A list item wrapped over several lines continues at an indent. Without
        // this its tail reads as a paragraph and the summary starts mid-sentence.
        let indented = raw.starts_with([' ', '\t']) && !line.is_empty();
        if in_list && indented {
            continue;
        }
        let structural = line.starts_with('#')
            || line.starts_with('<')
            || line.starts_with('|')
            || line.starts_with('>')
            || line.starts_with("- ")
            || line.starts_with("* ")
            || line.starts_with("---")
            || line.starts_with("===");
        in_list = !line.is_empty() && (structural_list(line) || (in_list && indented));
        if line.is_empty() || structural {
            if let Some(prose) = prose_of(&paragraph.join(" ")) {
                candidates.push(prose);
            }
            paragraph.clear();
            if candidates.len() == 8 {
                break;
            }
            continue;
        }
        paragraph.push(line.to_string());
    }
    // A description says what the thing *is*; instructions ("Click…", "Read…") don't.
    const IMPERATIVE: &[&str] = &[
        "click",
        "read",
        "see",
        "access",
        "note",
        "check",
        "please",
        "run",
        "to ",
        "use ",
        "install",
        "for ",
        "if ",
        "this is a fork",
        "warning",
    ];
    let descriptive = |p: &String| {
        let l = p.to_lowercase();
        [" is a ", " is an ", " is the ", " are ", " provides ", " lets ", " helps ", " allows "]
            .iter()
            .any(|w| l.contains(w))
            || l.starts_with("a ")
            || l.starts_with("an ")
    };
    let imperative = |p: &String| IMPERATIVE.iter().any(|w| p.to_lowercase().starts_with(w));
    // Many projects open with a noun phrase rather than a sentence — "Verifiable
    // architecture documentation from source code" — which says what the thing is
    // just as well as "X is a …". Prefer a copular sentence, but do not discard a
    // short opening paragraph for lacking a verb; instructions are still excluded.
    // Only the opening paragraph: a tagline sits at the top, and prose further
    // down a README is as likely to be setup instructions as a description.
    let tagline_like = |p: &&String| !imperative(p) && p.len() <= 160 && !p.ends_with(':');
    // A description introduces the project, so it is near the top. Searching the
    // whole file finds sentences that merely read like one: Caddy's install
    // instructions ("…if you know what you are doing") match on " are ".
    const NEAR_TOP: usize = 3;
    candidates
        .iter()
        .take(NEAR_TOP)
        .find(|p| descriptive(p) && !imperative(p))
        .or_else(|| taglines.first())
        .or_else(|| candidates.first().filter(tagline_like))
        .cloned()
}

fn prose_of(paragraph: &str) -> Option<String> {
    let chars: Vec<char> = paragraph.chars().collect();
    let prose = strip_inline(&chars).split_whitespace().collect::<Vec<_>>().join(" ");
    let words = prose.split_whitespace().filter(|w| w.chars().filter(|c| c.is_alphabetic()).count() >= 2).count();
    let has_url = prose.contains("://");
    (words >= 4 && !has_url).then(|| prose.chars().take(280).collect())
}

/// Index just past the bracket matching `chars[open]` (`[`→`]`, `(`→`)`).
fn matching(chars: &[char], open: usize) -> usize {
    let (o, c) = (chars[open], if chars[open] == '[' { ']' } else { ')' });
    let mut depth = 0;
    for (i, ch) in chars.iter().enumerate().skip(open) {
        if *ch == o {
            depth += 1;
        } else if *ch == c {
            depth -= 1;
            if depth == 0 {
                return i + 1;
            }
        }
    }
    chars.len()
}

fn strip_inline(chars: &[char]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '<' => {
                while i < chars.len() && chars[i] != '>' {
                    i += 1;
                }
                i += 1;
            }
            '!' if chars.get(i + 1) == Some(&'[') => {
                // Image or badge: dropped with its target.
                i = matching(chars, i + 1);
                if chars.get(i) == Some(&'(') {
                    i = matching(chars, i);
                }
            }
            '[' => {
                let close = matching(chars, i);
                let label = strip_inline(&chars[i + 1..close.saturating_sub(1).max(i + 1)]);
                i = close;
                if chars.get(i) == Some(&'(') {
                    i = matching(chars, i);
                }
                out.push_str(&label);
            }
            '*' | '_' | '`' => i += 1,
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readme_summary_skips_markup() {
        let md = "<p align=\"center\"><img src=\"logo.png\"></p>\n\n# Title\n\n[![CI](https://x/badge.svg)](https://x)\n\nWild Workouts is an **example Go DDD** project [that shows](https://x) how to build `apps`.\n\nMore.";
        assert_eq!(
            summary_from_markdown(md).as_deref(),
            Some("Wild Workouts is an example Go DDD project that shows how to build apps.")
        );
        let badges = "[![tag (latest)](https://x)](https://y) ![Build (ci)](https://z)\n\nLemmy is a link aggregator and forum for the fediverse.";
        assert_eq!(
            summary_from_markdown(badges).as_deref(),
            Some("Lemmy is a link aggregator and forum for the fediverse.")
        );
        assert_eq!(summary_from_markdown("# Only a title\n\n```\ncode block with many words here\n```\n"), None);

        // A bullet wrapped over two lines is one item: its tail is not a
        // paragraph, and taking it as the summary starts mid-sentence.
        let wrapped = "# autodoc\n\n- **Opinionated.** Density budgets, orthogonal connectors that never pass behind\n  a card. A cluttered IR is rejected with diagnostics.\n\nautodoc is a tool that turns source code into architecture documentation.";
        assert_eq!(
            summary_from_markdown(wrapped).as_deref(),
            Some("autodoc is a tool that turns source code into architecture documentation.")
        );
        let noun_phrase = "# autodoc\n\n[![CI](https://x/b.svg)](https://x)\n\nVerifiable, editorial architecture documentation from source code — as a CLI, an MCP server, and an agent skill.\n\nMore words here.";
        assert_eq!(
            summary_from_markdown(noun_phrase).as_deref(),
            Some("Verifiable, editorial architecture documentation from source code — as a CLI, an MCP server, and an agent skill.")
        );
        // Caddy: install prose deep in the README matches " are " but describes
        // nothing. The opening line introduces the project; the buried one does not.
        let buried = "# Caddy\n\nCaddy is an extensible server platform that uses TLS by default.\n\n## Install\n\nBuild the binary from source with the Go toolchain available on your machine.\n\nFetch the modules the build needs before compiling anything else here.\n\nGrant the binary permission to bind low ports on your host system now.\n\nreplacing username with your actual username. Be careful if you know what you are doing.";
        assert_eq!(
            summary_from_markdown(buried).as_deref(),
            Some("Caddy is an extensible server platform that uses TLS by default.")
        );
        let numbered = "# t\n\n1. Install the binary and then run it against\n   your repository to see the book.\n\nThe tool is a documenter of repositories.";
        assert_eq!(summary_from_markdown(numbered).as_deref(), Some("The tool is a documenter of repositories."));
    }

    #[test]
    fn names_and_slugs() {
        assert_eq!(display_name("api-gateway"), "API Gateway");
        assert_eq!(display_name("ledger_audit"), "Ledger Audit");
        assert_eq!(slug("@shop/API Gateway"), "shop-api-gateway");
    }

    #[test]
    fn topics_are_dotted_lowercase_identifiers() {
        assert!(is_topic("order.placed"));
        assert!(is_topic("payments.v1.captured"));
        assert!(!is_topic("auto.offset.reset"));
        assert!(!is_topic("bootstrap.servers"));
        assert!(!is_topic("./server.js"));
        assert!(!is_topic("Order.Placed"));
        assert!(!is_topic("example.com"));
    }

    #[test]
    fn sql_labels_name_tables() {
        let ev = |n: &str| EvidenceRef {
            file_path: "a".into(),
            start_line: 1,
            end_line: 1,
            symbol_name: None,
            note: Some(n.into()),
        };
        assert_eq!(sql_label(&[ev("INSERT INTO orders (id, total) VALUES…")], "writes"), "writes orders");
        assert_eq!(sql_label(&[ev("UPDATE orders SET status = %s")], "writes"), "writes orders");
        assert_eq!(sql_label(&[ev("SELECT 1")], "reads"), "reads");
        assert!(is_sql_update("UPDATE ORDERS SET X"));
        assert!(!is_sql_update("PLEASE UPDATE YOUR PROFILE"));
    }
}
