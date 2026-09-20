//! Runtime topology: what actually runs, as the repository declares it.
//!
//! Two sources, never inferred beyond what they state:
//! - Compose files, grouped per directory: the base file (`docker-compose.yml`,
//!   `compose.yml`) is the environment; `docker-compose.<variant>.yml` files are
//!   overlays listed with the services they add or change.
//! - Kubernetes manifests (`k8s/`, `kubernetes/`, `deploy/`, `manifests/`, …):
//!   workloads, Services (by label selector), Ingress routes. Helm templates are
//!   not rendered and are reported as such.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;

use crate::scan::EvidenceRef;

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Topology {
    pub environments: Vec<Environment>,
    /// What could not be read (Helm templates, unparsable files).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    /// `compose:<dir>` or `k8s:<dir>`.
    pub id: String,
    /// `Docker Compose (docker/)`, `Kubernetes (k8s/)`.
    pub name: String,
    /// `compose` or `kubernetes`.
    pub kind: String,
    /// Files that define it, base first.
    pub files: Vec<String>,
    pub workloads: Vec<Workload>,
    pub links: Vec<Link>,
    /// Compose overlays: file → services it adds or changes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overlays: Vec<Overlay>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Overlay {
    pub file: String,
    pub adds: Vec<String>,
    pub changes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Workload {
    /// Service / workload name as declared.
    pub name: String,
    /// `service` (compose), `deployment`, `statefulset`, `daemonset`, `job`, `cronjob`.
    pub kind: String,
    /// Unit built or deployed here, when it can be identified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Infrastructure id when the image is a known datastore/broker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub infra: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Build context (compose `build:`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<PortMapping>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicas: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub networks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<String>,
    pub env_vars: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_files: Vec<String>,
    #[serde(default)]
    pub health_check: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<String>,
    /// Kubernetes namespace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    pub evidence: EvidenceRef,
    /// Environment values (name, value, line) scanned for hosts of other workloads.
    #[serde(skip)]
    pub env: Vec<(String, String, u32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PortMapping {
    /// Port exposed outside (host port, NodePort, Service port); `None` = container only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
    pub target: String,
    /// Reachable from outside the environment (compose published, NodePort/LoadBalancer, Ingress).
    pub external: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    /// Workload name, or `external` for traffic entering the environment.
    pub from: String,
    pub to: String,
    /// `depends-on`, `ingress`, `published-port`.
    pub kind: String,
    pub label: String,
    pub evidence: EvidenceRef,
}

/// Maps a workload to a unit id: (name, image, build context).
pub type UnitResolver<'a> = &'a dyn Fn(&str, Option<&str>, Option<&str>) -> Option<String>;

/// Kubernetes manifests are found anywhere in the repository (they live in
/// `k8s/`, `deploy/overlays/prod/`, a chart's `templates/`, or beside the code),
/// so files are classified by content rather than by directory name.
const MAX_MANIFEST_BYTES: u64 = 512 * 1024;
const SKIP_MANIFEST_DIRS: &[&str] =
    &["node_modules", "target", "vendor", "dist", "build", ".git", "testdata", "fixtures"];

pub fn discover(root: &Path, resolve: UnitResolver) -> Topology {
    let mut topo = Topology::default();
    compose_environments(root, resolve, &mut topo);
    kubernetes_environments(root, resolve, &mut topo);
    topo
}

fn ev(file: &str, line: u32, note: &str) -> EvidenceRef {
    EvidenceRef {
        file_path: file.to_string(),
        start_line: line,
        end_line: line,
        symbol_name: None,
        note: Some(note.to_string()),
    }
}

fn scalar(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// `${VAR}` / `${VAR:-default}` substituted from the directory's `.env`.
fn substitute(s: &str, env: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("${") {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find('}') else {
            out.push_str(&rest[i..]);
            return out;
        };
        let inner = &rest[i + 2..i + j];
        let (name, default) = match inner.split_once(":-") {
            Some((n, d)) => (n, Some(d)),
            None => (inner, None),
        };
        match env.get(name).map(String::as_str).or(default) {
            Some(v) => out.push_str(v),
            None => out.push_str(&rest[i..i + j + 1]),
        }
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    out
}

// ─────────────────────────────── compose ───────────────────────────────

fn compose_environments(root: &Path, resolve: UnitResolver, topo: &mut Topology) {
    let files = crate::compose::find_all(root);
    let mut by_dir: BTreeMap<String, Vec<&crate::compose::ComposeFile>> = BTreeMap::new();
    for f in &files {
        let dir = f.file.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
        by_dir.entry(dir).or_default().push(f);
    }
    for (dir, group) in by_dir {
        // `find_all` orders canonical names first.
        let base = group[0];
        let env_text =
            std::fs::read_to_string(root.join(if dir.is_empty() { ".env".into() } else { format!("{dir}/.env") }))
                .unwrap_or_default();
        let env: BTreeMap<String, String> = crate::compose::parse_env_file(&env_text).into_iter().collect();
        let mut environment = Environment {
            id: format!("compose:{}", if dir.is_empty() { "." } else { &dir }),
            name: format!(
                "Docker Compose ({})",
                if dir.is_empty() { "repository root".to_string() } else { format!("{dir}/") }
            ),
            kind: "compose".into(),
            files: group.iter().map(|f| f.file.clone()).collect(),
            workloads: vec![],
            links: vec![],
            overlays: vec![],
        };
        let Ok(doc) = serde_yaml::from_str::<Value>(&base.text) else { continue };
        let Some(services) = doc.get("services").and_then(Value::as_mapping) else { continue };
        for (k, v) in services {
            let Some(name) = k.as_str() else { continue };
            let parsed = base.services.iter().find(|s| s.name == name);
            let line = parsed.map(|s| s.line).unwrap_or(1);
            let image = v.get("image").and_then(Value::as_str).map(|i| substitute(i, &env));
            let build = parsed.and_then(|s| s.build_dir.clone());
            let ports: Vec<PortMapping> = v
                .get("ports")
                .and_then(Value::as_sequence)
                .map(|seq| seq.iter().filter_map(compose_port).collect())
                .unwrap_or_default();
            let expose: Vec<PortMapping> = v
                .get("expose")
                .and_then(Value::as_sequence)
                .map(|seq| {
                    seq.iter()
                        .filter_map(scalar)
                        .map(|p| PortMapping { published: None, target: p, external: false })
                        .collect()
                })
                .unwrap_or_default();
            let deploy = v.get("deploy");
            let replicas =
                deploy.and_then(|d| d.get("replicas")).and_then(scalar).or_else(|| v.get("scale").and_then(scalar));
            let resources = deploy
                .and_then(|d| d.get("resources"))
                .and_then(|r| r.get("limits"))
                .and_then(Value::as_mapping)
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| Some(format!("{} {}", k.as_str()?, scalar(v)?)))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .filter(|s| !s.is_empty());
            let list = |key: &str| -> Vec<String> {
                match v.get(key) {
                    Some(Value::Sequence(s)) => {
                        s.iter().filter_map(|x| scalar(x).or_else(|| x.get("source").and_then(scalar))).collect()
                    }
                    Some(Value::Mapping(m)) => m.keys().filter_map(scalar).collect(),
                    _ => vec![],
                }
            };
            let infra = image.as_deref().and_then(crate::catalog::infra_for_image).map(|k| k.id().to_string());
            let unit = if infra.is_some() { None } else { resolve(name, image.as_deref(), build.as_deref()) };
            let workload = Workload {
                name: name.to_string(),
                kind: "service".into(),
                unit,
                infra,
                image,
                build,
                ports: ports.into_iter().chain(expose).collect(),
                replicas,
                schedule: None,
                networks: list("networks"),
                volumes: list("volumes"),
                env_vars: parsed.map(|s| s.environment.len()).unwrap_or(0),
                env_files: parsed.map(|s| s.env_files.clone()).unwrap_or_default(),
                health_check: v.get("healthcheck").is_some(),
                resources,
                profiles: list("profiles"),
                namespace: None,
                evidence: ev(&base.file, line, &format!("compose service `{name}`")),
                env: parsed
                    .map(|s| {
                        s.environment
                            .iter()
                            .map(|(k, v)| {
                                let l = block_lines(&base.text, line)
                                    .find(|(_, t)| t.contains(k.as_str()))
                                    .map(|(i, _)| i)
                                    .unwrap_or(line);
                                (k.clone(), v.clone(), l)
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            for p in workload.ports.iter().filter(|p| p.external) {
                environment.links.push(Link {
                    from: "external".into(),
                    to: name.to_string(),
                    kind: "published-port".into(),
                    label: format!("{} → {}", p.published.clone().unwrap_or_default(), p.target),
                    evidence: ev(&base.file, port_line(&base.text, line, p).unwrap_or(line), "published port"),
                });
            }
            for dep in parsed.map(|s| s.depends_on.clone()).unwrap_or_default() {
                environment.links.push(Link {
                    from: name.to_string(),
                    to: dep.clone(),
                    kind: "depends-on".into(),
                    label: "depends on".into(),
                    evidence: ev(&base.file, depends_line(&base.text, line, &dep).unwrap_or(line), "depends_on"),
                });
            }
            environment.workloads.push(workload);
        }
        for overlay in &group[1..] {
            let base_names: BTreeSet<&str> = environment.workloads.iter().map(|w| w.name.as_str()).collect();
            let (adds, changes): (Vec<String>, Vec<String>) =
                overlay.services.iter().map(|s| s.name.clone()).partition(|n| !base_names.contains(n.as_str()));
            environment.overlays.push(Overlay { file: overlay.file.clone(), adds, changes });
        }
        if !environment.workloads.is_empty() {
            env_links(&mut environment, &BTreeMap::new());
            topo.environments.push(environment);
        }
    }
}

/// `DATABASE_URL=postgres://…@db:5432` names another workload: a declared connection.
fn env_links(environment: &mut Environment, service_targets: &BTreeMap<String, Vec<String>>) {
    let names: BTreeSet<String> = environment.workloads.iter().map(|w| w.name.clone()).collect();
    let mut links = Vec::new();
    for w in &environment.workloads {
        let file = w.evidence.file_path.clone();
        for (key, value, line) in &w.env {
            for host in crate::compose::hosts_in(value) {
                let targets: Vec<String> = if names.contains(&host) {
                    vec![host.clone()]
                } else {
                    service_targets.get(&host).cloned().unwrap_or_default()
                };
                for t in targets.into_iter().filter(|t| *t != w.name) {
                    let exists = environment
                        .links
                        .iter()
                        .chain(links.iter())
                        .any(|l: &Link| l.from == w.name && l.to == t && l.kind == "connects");
                    if !exists {
                        links.push(Link {
                            from: w.name.clone(),
                            to: t,
                            kind: "connects".into(),
                            label: format!("via {key}"),
                            evidence: ev(&file, *line, &format!("`{key}` names `{host}`")),
                        });
                    }
                }
            }
        }
    }
    environment.links.extend(links);
}

fn compose_port(v: &Value) -> Option<PortMapping> {
    match v {
        Value::Mapping(_) => {
            let target = v.get("target").and_then(scalar)?;
            let published = v.get("published").and_then(scalar);
            Some(PortMapping { external: published.is_some(), published, target })
        }
        other => {
            let s = scalar(other)?;
            let s = s.split('/').next().unwrap_or(&s).to_string();
            // `[ip:]host:container` or `container`
            let parts: Vec<&str> = s.rsplitn(3, ':').collect();
            match parts.as_slice() {
                [target] => Some(PortMapping { published: None, target: (*target).into(), external: false }),
                [target, host, ..] => {
                    Some(PortMapping { published: Some((*host).into()), target: (*target).into(), external: true })
                }
                _ => None,
            }
        }
    }
}

/// Line of a port entry inside the service block starting at `service_line`.
fn port_line(text: &str, service_line: u32, p: &PortMapping) -> Option<u32> {
    let needle = match &p.published {
        Some(h) => format!("{h}:{}", p.target),
        None => p.target.clone(),
    };
    block_lines(text, service_line).find(|(_, l)| l.contains(&needle)).map(|(i, _)| i)
}

fn depends_line(text: &str, service_line: u32, dep: &str) -> Option<u32> {
    let mut in_deps = false;
    for (i, l) in block_lines(text, service_line) {
        let t = l.trim();
        if t.starts_with("depends_on") {
            in_deps = true;
            if t.contains(dep) {
                return Some(i);
            }
            continue;
        }
        if in_deps && (t.trim_start_matches("- ").trim_matches(['"', '\'']) == dep || t.trim_end_matches(':') == dep) {
            return Some(i);
        }
    }
    None
}

/// (1-based line, text) of a compose service's block.
fn block_lines(text: &str, service_line: u32) -> impl Iterator<Item = (u32, &str)> {
    let lines: Vec<&str> = text.lines().collect();
    let start = service_line.saturating_sub(1) as usize;
    let indent = lines.get(start).map(|l| l.len() - l.trim_start().len()).unwrap_or(0);
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, l)| !l.trim().is_empty() && l.len() - l.trim_start().len() <= indent)
        .map(|(i, _)| i)
        .unwrap_or(lines.len());
    (start..end).map(move |i| (i as u32 + 1, lines[i]))
}

// ────────────────────────────── kubernetes ──────────────────────────────

/// Manifest files anywhere in the repository, grouped by the directory holding
/// them; a chart's `templates/` groups under the chart.
fn manifest_groups(root: &Path) -> BTreeMap<String, Vec<String>> {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .require_git(false)
        .sort_by_file_path(|a, b| a.cmp(b))
        .filter_entry(|e| {
            !e.file_type().is_some_and(|t| t.is_dir())
                || !SKIP_MANIFEST_DIRS.contains(&e.file_name().to_string_lossy().as_ref())
        })
        .build();
    for e in walker.flatten() {
        let p = e.path();
        if !p.is_file() || !p.extension().and_then(|x| x.to_str()).is_some_and(|x| x == "yml" || x == "yaml") {
            continue;
        }
        if e.metadata().map(|m| m.len() > MAX_MANIFEST_BYTES).unwrap_or(true) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(p) else { continue };
        let head: String = text.chars().take(4000).collect();
        if !(head.contains("apiVersion:") && head.contains("kind:")) {
            continue;
        }
        let Ok(rel) = p.strip_prefix(root) else { continue };
        let rel = rel.to_string_lossy().replace('\\', "/");
        let dir = rel.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default();
        let group = match dir.strip_suffix("/templates") {
            Some(chart) if root.join(chart).join("Chart.yaml").is_file() => chart.to_string(),
            _ => dir,
        };
        groups.entry(group).or_default().push(rel);
    }
    groups
}

fn kubernetes_environments(root: &Path, resolve: UnitResolver, topo: &mut Topology) {
    for (dir, files) in manifest_groups(root) {
        let chart = root.join(&dir).join("Chart.yaml").is_file();
        let values = if chart { helm_values(&root.join(&dir)) } else { BTreeMap::new() };
        let mut environment = Environment {
            id: format!("k8s:{dir}"),
            name: if chart {
                format!("Helm chart ({dir}/)")
            } else if dir.is_empty() {
                "Kubernetes (repository root)".to_string()
            } else {
                format!("Kubernetes ({dir}/)")
            },
            kind: "kubernetes".into(),
            files: vec![],
            workloads: vec![],
            links: vec![],
            overlays: vec![],
        };
        // Services and Ingresses resolve against workloads after all files are read.
        let mut services: Vec<ServiceDecl> = vec![];
        let mut ingresses: Vec<(String, String, EvidenceRef)> = vec![];
        let mut labels_of: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        let mut templated = 0;
        for file in files {
            let Ok(raw) = std::fs::read_to_string(root.join(&file)) else { continue };
            let text = if raw.contains("{{") {
                match render_template(&raw, &values) {
                    Some(rendered) => rendered,
                    None => {
                        templated += 1;
                        continue;
                    }
                }
            } else {
                raw
            };
            let mut counted = false;
            for (doc_line, body) in split_documents(&text) {
                let Ok(v) = serde_yaml::from_str::<Value>(body) else { continue };
                let Some(kind) = v.get("kind").and_then(Value::as_str) else { continue };
                let name =
                    v.get("metadata").and_then(|m| m.get("name")).and_then(Value::as_str).unwrap_or("").to_string();
                let namespace =
                    v.get("metadata").and_then(|m| m.get("namespace")).and_then(Value::as_str).map(str::to_string);
                let line = doc_line + line_of(body, "kind:").unwrap_or(1) - 1;
                let evidence = ev(&file, line.max(1), &format!("{kind} `{name}`"));
                match kind {
                    "Deployment" | "StatefulSet" | "DaemonSet" | "Job" | "CronJob" => {
                        let spec = v.get("spec");
                        let pod = if kind == "CronJob" {
                            spec.and_then(|s| s.get("jobTemplate"))
                                .and_then(|j| j.get("spec"))
                                .and_then(|s| s.get("template"))
                        } else {
                            spec.and_then(|s| s.get("template"))
                        };
                        let containers = pod
                            .and_then(|t| t.get("spec"))
                            .and_then(|s| s.get("containers"))
                            .and_then(Value::as_sequence);
                        let first = containers.and_then(|c| c.first());
                        let image = first.and_then(|c| c.get("image")).and_then(Value::as_str).map(str::to_string);
                        let ports: Vec<PortMapping> = containers
                            .into_iter()
                            .flatten()
                            .flat_map(|c| c.get("ports").and_then(Value::as_sequence).cloned().unwrap_or_default())
                            .filter_map(|p| p.get("containerPort").and_then(scalar))
                            .map(|t| PortMapping { published: None, target: t, external: false })
                            .collect();
                        let env_vars = containers
                            .into_iter()
                            .flatten()
                            .map(|c| c.get("env").and_then(Value::as_sequence).map(|s| s.len()).unwrap_or(0))
                            .sum();
                        let health_check = containers
                            .into_iter()
                            .flatten()
                            .any(|c| c.get("livenessProbe").is_some() || c.get("readinessProbe").is_some());
                        let resources = first
                            .and_then(|c| c.get("resources"))
                            .and_then(|r| r.get("limits"))
                            .and_then(Value::as_mapping)
                            .map(|m| {
                                m.iter()
                                    .filter_map(|(k, v)| Some(format!("{} {}", k.as_str()?, scalar(v)?)))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .filter(|s| !s.is_empty());
                        let labels: BTreeMap<String, String> = pod
                            .and_then(|t| t.get("metadata"))
                            .and_then(|m| m.get("labels"))
                            .and_then(Value::as_mapping)
                            .map(|m| {
                                m.iter().filter_map(|(k, v)| Some((k.as_str()?.to_string(), scalar(v)?))).collect()
                            })
                            .unwrap_or_default();
                        labels_of.insert(name.clone(), labels);
                        let infra =
                            image.as_deref().and_then(crate::catalog::infra_for_image).map(|k| k.id().to_string());
                        let unit = if infra.is_some() { None } else { resolve(&name, image.as_deref(), None) };
                        environment.workloads.push(Workload {
                            name: name.clone(),
                            kind: kind.to_lowercase(),
                            unit,
                            infra,
                            image,
                            build: None,
                            ports,
                            replicas: spec.and_then(|s| s.get("replicas")).and_then(scalar),
                            schedule: spec.and_then(|s| s.get("schedule")).and_then(scalar),
                            networks: vec![],
                            volumes: pod
                                .and_then(|t| t.get("spec"))
                                .and_then(|s| s.get("volumes"))
                                .and_then(Value::as_sequence)
                                .map(|s| s.iter().filter_map(|x| x.get("name").and_then(scalar)).collect())
                                .unwrap_or_default(),
                            env_vars,
                            env_files: vec![],
                            health_check,
                            resources,
                            profiles: vec![],
                            namespace,
                            env: containers
                                .into_iter()
                                .flatten()
                                .flat_map(|c| c.get("env").and_then(Value::as_sequence).cloned().unwrap_or_default())
                                .filter_map(|e| {
                                    let k = e.get("name").and_then(Value::as_str)?.to_string();
                                    let v = e.get("value").and_then(scalar)?;
                                    let l = doc_line + body.lines().position(|t| t.contains(&v)).unwrap_or(0) as u32;
                                    Some((k, v, l))
                                })
                                .collect(),
                            evidence: evidence.clone(),
                        });
                        counted = true;
                    }
                    "Service" => {
                        let spec = v.get("spec");
                        let external = spec
                            .and_then(|s| s.get("type"))
                            .and_then(Value::as_str)
                            .is_some_and(|t| matches!(t, "NodePort" | "LoadBalancer"));
                        let selector: BTreeMap<String, String> = spec
                            .and_then(|s| s.get("selector"))
                            .and_then(Value::as_mapping)
                            .map(|m| {
                                m.iter().filter_map(|(k, v)| Some((k.as_str()?.to_string(), scalar(v)?))).collect()
                            })
                            .unwrap_or_default();
                        let ports = spec
                            .and_then(|s| s.get("ports"))
                            .and_then(Value::as_sequence)
                            .map(|seq| {
                                seq.iter()
                                    .map(|p| PortMapping {
                                        published: p
                                            .get("nodePort")
                                            .and_then(scalar)
                                            .or_else(|| p.get("port").and_then(scalar)),
                                        target: p
                                            .get("targetPort")
                                            .and_then(scalar)
                                            .or_else(|| p.get("port").and_then(scalar))
                                            .unwrap_or_default(),
                                        external,
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        services.push((name, selector, ports, evidence));
                        counted = true;
                    }
                    "Ingress" => {
                        let rules = v
                            .get("spec")
                            .and_then(|s| s.get("rules"))
                            .and_then(Value::as_sequence)
                            .cloned()
                            .unwrap_or_default();
                        for r in rules {
                            let host = r.get("host").and_then(Value::as_str).unwrap_or("*").to_string();
                            let paths = r
                                .get("http")
                                .and_then(|h| h.get("paths"))
                                .and_then(Value::as_sequence)
                                .cloned()
                                .unwrap_or_default();
                            for p in paths {
                                let path = p.get("path").and_then(Value::as_str).unwrap_or("/");
                                let backend = p
                                    .get("backend")
                                    .and_then(|b| {
                                        b.get("service").and_then(|s| s.get("name")).or_else(|| b.get("serviceName"))
                                    })
                                    .and_then(Value::as_str);
                                if let Some(b) = backend {
                                    ingresses.push((b.to_string(), format!("{host}{path}"), evidence.clone()));
                                }
                            }
                        }
                        counted = true;
                    }
                    _ => {}
                }
            }
            if counted {
                environment.files.push(file);
            }
        }
        // Service → workloads it selects; its ports describe how they're reached.
        let mut service_targets: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (svc, selector, ports, evidence) in services {
            let targets: Vec<String> = labels_of
                .iter()
                .filter(|(_, labels)| !selector.is_empty() && selector.iter().all(|(k, v)| labels.get(k) == Some(v)))
                .map(|(n, _)| n.clone())
                .collect();
            service_targets.insert(svc.clone(), targets.clone());
            for t in &targets {
                if let Some(w) = environment.workloads.iter_mut().find(|w| &w.name == t) {
                    for p in &ports {
                        if !w.ports.iter().any(|x| x.target == p.target && x.published == p.published) {
                            w.ports.push(p.clone());
                        }
                    }
                }
                for p in ports.iter().filter(|p| p.external) {
                    environment.links.push(Link {
                        from: "external".into(),
                        to: t.clone(),
                        kind: "published-port".into(),
                        label: format!("{} → {}", p.published.clone().unwrap_or_default(), p.target),
                        evidence: evidence.clone(),
                    });
                }
            }
            for (_, route, iev) in ingresses.iter().filter(|(b, _, _)| *b == svc) {
                for t in &targets {
                    environment.links.push(Link {
                        from: "external".into(),
                        to: t.clone(),
                        kind: "ingress".into(),
                        label: route.clone(),
                        evidence: iev.clone(),
                    });
                }
            }
        }
        if templated > 0 {
            topo.notes.push(format!(
                "{templated} manifest(s) under `{dir}/` use template logic (conditions, loops, includes) and were not read"
            ));
        }
        if !environment.workloads.is_empty() {
            env_links(&mut environment, &service_targets);
            topo.environments.push(environment);
        }
    }
}

/// A Kubernetes Service: name, label selector, ports, declaration.
type ServiceDecl = (String, BTreeMap<String, String>, Vec<PortMapping>, EvidenceRef);

/// `values.yaml` of a chart, flattened to `Values.a.b` keys plus the chart's own name.
fn helm_values(chart: &Path) -> BTreeMap<String, String> {
    let text = std::fs::read_to_string(chart.join("values.yaml"))
        .or_else(|_| std::fs::read_to_string(chart.join("values.yml")))
        .unwrap_or_default();
    let mut out = BTreeMap::new();
    if let Ok(v) = serde_yaml::from_str::<Value>(&text) {
        flatten_values(&v, String::new(), &mut out);
    }
    let name = std::fs::read_to_string(chart.join("Chart.yaml"))
        .ok()
        .and_then(|t| serde_yaml::from_str::<Value>(&t).ok())
        .and_then(|c| c.get("name").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| chart.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
    out.insert("Chart.Name".into(), name.clone());
    out.insert("Release.Name".into(), name);
    out.insert("Release.Namespace".into(), "default".into());
    out
}

fn flatten_values(v: &Value, prefix: String, out: &mut BTreeMap<String, String>) {
    match v {
        Value::Mapping(m) => {
            for (k, val) in m {
                let Some(key) = k.as_str() else { continue };
                let full = if prefix.is_empty() { format!("Values.{key}") } else { format!("{prefix}.{key}") };
                flatten_values(val, full, out);
            }
        }
        other => {
            if let Some(s) = scalar(other) {
                out.insert(prefix, s);
            }
        }
    }
}

/// Substitutes `{{ .Values.x }}`-style expressions from the chart's values.
/// Templates with control flow (`if`, `range`, `include`, …) are not rendered:
/// choosing a branch would invent deployment facts.
fn render_template(text: &str, values: &BTreeMap<String, String>) -> Option<String> {
    if values.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("{{") {
        let j = rest[i..].find("}}")? + i;
        let expr = rest[i + 2..j].trim().trim_start_matches('-').trim_end_matches('-').trim();
        const CONTROL: &[&str] =
            &["if", "range", "with", "end", "else", "define", "template", "include", "toYaml", "tpl", "printf"];
        if CONTROL.iter().any(|c| expr == *c || expr.starts_with(&format!("{c} "))) || expr.starts_with('/') {
            return None;
        }
        out.push_str(&rest[..i]);
        let path = expr.trim_start_matches('.').split('|').next().unwrap_or("").trim().trim_start_matches('.');
        let fallback = expr
            .split('|')
            .map(str::trim)
            .find_map(|p| p.strip_prefix("default "))
            .map(|d| d.trim().trim_matches(['"', '\'']).to_string());
        match values.get(path).cloned().or(fallback) {
            Some(v) => out.push_str(&v),
            // An unset value is still a value: keep the manifest parseable.
            None => out.push_str("unset"),
        }
        rest = &rest[j + 2..];
    }
    out.push_str(rest);
    Some(out)
}

/// Documents of a multi-document YAML file with the 1-based line each starts at.
fn split_documents(text: &str) -> Vec<(u32, &str)> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut start_line = 1u32;
    let mut line_no = 1u32;
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        if line.trim_end() == "---" {
            if offset > start {
                out.push((start_line, &text[start..offset]));
            }
            start = offset + line.len();
            start_line = line_no + 1;
        }
        offset += line.len();
        line_no += 1;
    }
    if start < text.len() {
        out.push((start_line, &text[start..]));
    }
    out
}

fn line_of(body: &str, prefix: &str) -> Option<u32> {
    body.lines().position(|l| l.starts_with(prefix)).map(|i| i as u32 + 1)
}

// ─────────────────────────────── drafting ───────────────────────────────

/// Deployment figure for one environment: traffic entering from outside,
/// workloads grouped by compose network / Kubernetes namespace (datastores and
/// brokers apart), and the startup dependencies between them.
pub fn draft_topology_ir(
    report: &crate::scan::ScanReport,
    env: &Environment,
    opts: &crate::draft::DraftOptions,
) -> crate::draft::Draft {
    use nunki_ir::{
        now_rfc3339, visual_density, BoundaryType, Container, DiagramIR, DiagramMetadata, DiagramType, Edge, EdgeStyle,
        EdgeType, IrVersion, Node,
    };
    let mut notes = Vec::new();
    let slug = crate::scan::slug;
    let short = |s: &str, n: usize| {
        if s.chars().count() <= n {
            s.to_string()
        } else {
            s.chars().take(n - 1).collect::<String>() + "…"
        }
    };
    let mut ir = DiagramIR {
        version: IrVersion::V1_1_0,
        diagram_type: DiagramType::Container,
        title: short(&format!("{} — runtime ({})", report.system.name, env.kind), 80),
        subtitle: Some(short(&env.files.join(", "), 120)),
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
    let group_of = |w: &Workload| -> (String, String, BoundaryType) {
        if w.infra.is_some() {
            return ("data".into(), "Data & messaging".into(), BoundaryType::Storage);
        }
        match (&w.namespace, w.networks.first()) {
            (Some(ns), _) => (format!("ns-{}", slug(ns)), format!("namespace {ns}"), BoundaryType::TrustZone),
            (None, Some(net)) => (format!("net-{}", slug(net)), format!("network {net}"), BoundaryType::TrustZone),
            (None, None) => ("runtime".into(), env.name.clone(), BoundaryType::TrustZone),
        }
    };
    let external = env.links.iter().any(|l| l.from == "external");
    if external {
        ir.containers.push(Container {
            id: "outside".into(),
            label: "Outside".into(),
            boundary_type: BoundaryType::Client,
            role_description: Some("Traffic entering the environment".into()),
        });
        ir.nodes.push(Node {
            id: "external".into(),
            container_id: Some("outside".into()),
            label: "Clients".into(),
            subtitle: Some(if env.kind == "kubernetes" { "Ingress / node ports".into() } else { "Host ports".into() }),
            tech_stack: None,
            is_key_focal_point: false,
            evidence: None,
            metadata: None,
            attributes: None,
            state_kind: None,
        });
    }
    let id_of = |name: &str| format!("w-{}", slug(name));
    for w in &env.workloads {
        let (cid, label, boundary) = group_of(w);
        if !ir.containers.iter().any(|c| c.id == cid) {
            ir.containers.push(Container {
                id: cid.clone(),
                label: short(&label, 32),
                boundary_type: boundary,
                role_description: None,
            });
        }
        let unit_name =
            w.unit.as_deref().and_then(|u| report.containers.iter().find(|x| x.id == u)).map(|u| u.name.clone());
        let mut tech = Vec::new();
        if let Some(i) = &w.image {
            tech.push(short(i.rsplit('/').next().unwrap_or(i), 28));
        } else if w.build.is_some() {
            tech.push("built from source".to_string());
        }
        if let Some(r) = w.replicas.as_deref().filter(|r| *r != "1") {
            tech.push(format!("×{r}"));
        }
        let infra_role =
            w.infra.as_deref().and_then(|i| report.infrastructure.iter().find(|x| x.id == i)).map(|x| x.role.clone());
        let subtitle = match (&unit_name, &w.schedule, infra_role) {
            (_, Some(s), _) => format!("{} · {s}", w.kind),
            (Some(u), None, _) => format!("runs {u}"),
            (None, None, Some(role)) => role,
            (None, None, None) => w.kind.clone(),
        };
        let mut meta = BTreeMap::new();
        let ports: Vec<String> = w
            .ports
            .iter()
            .map(|p| match &p.published {
                Some(h) => format!("{h}→{}", p.target),
                None => p.target.clone(),
            })
            .collect();
        for (k, v) in [
            ("ports", ports.join(", ")),
            ("volumes", w.volumes.join(", ")),
            ("resources", w.resources.clone().unwrap_or_default()),
            ("healthCheck", if w.health_check { "yes".into() } else { String::new() }),
            ("envVars", if w.env_vars > 0 { w.env_vars.to_string() } else { String::new() }),
        ] {
            if !v.is_empty() {
                meta.insert(k.to_string(), v);
            }
        }
        ir.nodes.push(Node {
            id: id_of(&w.name),
            container_id: Some(cid),
            label: short(&w.name, 32),
            subtitle: Some(short(&subtitle, 48)),
            tech_stack: (!tech.is_empty()).then(|| short(&tech.join(" · "), 40)),
            is_key_focal_point: false,
            evidence: Some(w.evidence.to_ir()),
            metadata: (!meta.is_empty()).then_some(meta),
            attributes: None,
            state_kind: None,
        });
    }
    let names: BTreeSet<&str> = env.workloads.iter().map(|w| w.name.as_str()).collect();
    for (i, l) in env.links.iter().enumerate() {
        if !(l.from == "external" || names.contains(l.from.as_str())) || !names.contains(l.to.as_str()) {
            continue;
        }
        let source = if l.from == "external" { "external".to_string() } else { id_of(&l.from) };
        let target = id_of(&l.to);
        if ir.edges.iter().any(|e| e.source == source && e.target == target) {
            continue;
        }
        let startup = l.kind == "depends-on";
        ir.edges.push(Edge {
            id: format!("link-{}", i + 1),
            source,
            target,
            label: Some(short(&l.label, 32)),
            edge_type: EdgeType::Sync,
            style: startup.then_some(EdgeStyle::Dashed),
            is_primary_path: None,
            sequence: None,
            reply: None,
            payload: None,
            cardinality: None,
            guard: None,
            evidence: Some(l.evidence.to_ir()),
        });
    }
    // Workloads nothing links to still run: keep them visible via their group.
    let linked: BTreeSet<String> = ir.edges.iter().flat_map(|e| [e.source.clone(), e.target.clone()]).collect();
    let lone: Vec<String> = ir.nodes.iter().filter(|n| !linked.contains(&n.id)).map(|n| n.id.clone()).collect();
    let budget = nunki_ir::element_budget();
    if ir.nodes.len() + ir.edges.len() > budget {
        let before = ir.edges.len();
        ir.edges.retain(|e| e.style != Some(EdgeStyle::Dashed));
        notes.push(format!("{} startup dependencies left off the figure to meet density", before - ir.edges.len()));
    }
    // Unlinked workloads are left out only when a linked picture remains.
    let linked: BTreeSet<String> = ir.edges.iter().flat_map(|e| [e.source.clone(), e.target.clone()]).collect();
    if !lone.is_empty() && linked.len() >= 2 {
        let dropped: Vec<String> =
            ir.nodes.iter().filter(|n| !linked.contains(&n.id)).map(|n| n.label.clone()).collect();
        if !dropped.is_empty() {
            ir.nodes.retain(|n| linked.contains(&n.id));
            notes.push(format!("not drawn (no declared links): {}; listed in the workloads table", dropped.join(", ")));
        }
    }
    // The front door: an entry that runs code from the repository, most-linked first.
    let runs_code: BTreeSet<String> =
        env.workloads.iter().filter(|w| w.unit.is_some()).map(|w| id_of(&w.name)).collect();
    let focal = ir.edges.iter().filter(|e| e.source == "external").map(|e| e.target.clone()).max_by_key(|t| {
        (runs_code.contains(t), ir.edges.iter().filter(|e| &e.source == t).count(), std::cmp::Reverse(t.clone()))
    });
    if let Some(f) = focal {
        ir.nodes.iter_mut().filter(|n| n.id == f).for_each(|n| n.is_key_focal_point = true);
    }
    let used: BTreeSet<String> = ir.nodes.iter().filter_map(|n| n.container_id.clone()).collect();
    ir.containers.retain(|c| used.contains(&c.id));
    ir.metadata.visual_density_score = Some(visual_density(ir.nodes.len(), ir.edges.len()));
    crate::draft::Draft { ir, notes }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_ports_and_substitution() {
        let p = compose_port(&Value::String("127.0.0.1:8080:80/tcp".into())).unwrap();
        assert_eq!((p.published.as_deref(), p.target.as_str(), p.external), (Some("8080"), "80", true));
        let p = compose_port(&Value::String("8080".into())).unwrap();
        assert_eq!((p.published, p.target.as_str(), p.external), (None, "8080", false));
        let env: BTreeMap<String, String> = [("TB".to_string(), "thingsboard/tb-node".to_string())].into();
        assert_eq!(substitute("${TB}:${VERSION:-latest}", &env), "thingsboard/tb-node:latest");
        assert_eq!(substitute("${MISSING}", &env), "${MISSING}");
    }

    #[test]
    fn documents_split_with_start_lines() {
        let docs = split_documents("---\nkind: A\n---\nkind: B\nx: 1\n");
        assert_eq!(docs.iter().map(|(l, _)| *l).collect::<Vec<_>>(), vec![2, 4]);
    }
}
