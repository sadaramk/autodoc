//! JVM service wiring that lives outside ordinary code paths: application
//! names and routes in Spring / Micronaut / Quarkus configuration (including
//! Spring Cloud Config's shared files), declarative HTTP clients (`@FeignClient`,
//! MicroProfile `@RegisterRestClient`, Micronaut `@Client`), and message
//! listeners declared by annotation.

use std::collections::BTreeMap;
use std::path::Path;

use autodoc_ir::EdgeType;

use crate::extract::Annotation;
use crate::scan::{EvidenceRef, FileRec, Unit};

/// One configuration file and the flattened `a.b.c` keys it sets, with lines.
#[derive(Debug, Clone)]
pub(crate) struct ConfigDoc {
    /// Repository-relative path.
    pub path: String,
    /// File stem without profile (`account-service` for `account-service-dev.yml`).
    pub stem: String,
    /// Directory holding the file.
    pub dir: String,
    pub entries: Vec<(String, String, u32)>,
}

impl ConfigDoc {
    pub fn get(&self, key: &str) -> Option<(&str, u32)> {
        self.entries.iter().find(|(k, _, _)| k == key).map(|(_, v, l)| (v.as_str(), *l))
    }

    pub fn evidence(&self, line: u32, note: &str) -> EvidenceRef {
        EvidenceRef {
            file_path: self.path.clone(),
            start_line: line,
            end_line: line,
            symbol_name: None,
            note: Some(note.to_string()),
        }
    }
}

const CONFIG_NAMES: &[&str] = &["application", "bootstrap"];

/// Every `.yml`/`.yaml`/`.properties` under a `src/main/resources` directory
/// (Spring Cloud Config repositories keep per-service files there too) and in
/// `config-repo` style directories.
pub(crate) fn config_documents(root: &Path) -> Vec<ConfigDoc> {
    let mut out = Vec::new();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .require_git(false)
        .sort_by_file_path(|a, b| a.cmp(b))
        .build();
    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(root) else { continue };
        let rel_s = rel.to_string_lossy().replace('\\', "/");
        let in_resources = rel_s.contains("src/main/resources/");
        let in_config_repo = rel_s.split('/').any(|s| matches!(s, "config-repo" | "config-server-repo"));
        if !(in_resources || in_config_repo) || rel_s.contains("/test/") || rel_s.contains("node_modules") {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let (stem, ext) = match name.rsplit_once('.') {
            Some(x) => x,
            None => continue,
        };
        if !matches!(ext, "yml" | "yaml" | "properties") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else { continue };
        let entries = if ext == "properties" { parse_properties(&text) } else { parse_yaml(&text) };
        if entries.is_empty() {
            continue;
        }
        let base = strip_profile(stem);
        out.push(ConfigDoc {
            path: rel_s.clone(),
            stem: base,
            dir: rel_s.rsplit_once('/').map(|(d, _)| d.to_string()).unwrap_or_default(),
            entries,
        });
    }
    out
}

/// `application-dev` → `application`; `account-service-docker` keeps its name
/// unless the suffix is a common profile.
fn strip_profile(stem: &str) -> String {
    const PROFILES: &[&str] =
        &["dev", "prod", "production", "local", "docker", "test", "staging", "k8s", "kubernetes", "default"];
    for p in PROFILES {
        if let Some(b) = stem.strip_suffix(&format!("-{p}")) {
            return b.to_string();
        }
    }
    stem.to_string()
}

fn parse_properties(text: &str) -> Vec<(String, String, u32)> {
    text.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let t = l.trim();
            if t.starts_with('#') || t.starts_with('!') || t.is_empty() {
                return None;
            }
            let (k, v) = t.split_once(['=', ':'])?;
            Some((k.trim().to_string(), v.trim().to_string(), i as u32 + 1))
        })
        .collect()
}

/// Flattens YAML into `a.b[0].c` keys. Lines come from matching each scalar's
/// key (and value) in document order, which is exact for configuration files.
fn parse_yaml(text: &str) -> Vec<(String, String, u32)> {
    let mut out = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut cursor = 0usize;
    for doc in text.split("\n---") {
        let Ok(v) = serde_yaml::from_str::<serde_yaml::Value>(doc) else { continue };
        flatten(&v, String::new(), &mut |key, value| {
            // A list item (`predicates[0]`) is found by its value, a mapping entry by its key.
            let last = key.rsplit('.').next().unwrap_or(&key);
            let leaf = if last.ends_with(']') { String::new() } else { last.to_string() };
            let found = lines[cursor.min(lines.len())..].iter().position(|l| {
                let t = l.trim_start().trim_start_matches("- ");
                (!leaf.is_empty() && t.starts_with(&format!("{leaf}:")))
                    || (leaf.is_empty() && t.contains(value.as_str()))
            });
            let line = match found {
                Some(p) => {
                    cursor += p;
                    cursor as u32 + 1
                }
                None => 1,
            };
            out.push((key, value, line));
        });
    }
    out
}

fn flatten(v: &serde_yaml::Value, prefix: String, emit: &mut dyn FnMut(String, String)) {
    match v {
        serde_yaml::Value::Mapping(m) => {
            for (k, val) in m {
                let key = match k {
                    serde_yaml::Value::String(s) => s.clone(),
                    serde_yaml::Value::Number(n) => n.to_string(),
                    serde_yaml::Value::Bool(b) => b.to_string(),
                    _ => continue,
                };
                let full = if prefix.is_empty() { key } else { format!("{prefix}.{key}") };
                flatten(val, full, emit);
            }
        }
        serde_yaml::Value::Sequence(items) => {
            for (i, item) in items.iter().enumerate() {
                flatten(item, format!("{prefix}[{i}]"), emit);
            }
        }
        serde_yaml::Value::String(s) => emit(prefix, s.clone()),
        serde_yaml::Value::Number(n) => emit(prefix, n.to_string()),
        serde_yaml::Value::Bool(b) => emit(prefix, b.to_string()),
        _ => {}
    }
}

/// Application names a JVM unit is known by (service discovery ids, config
/// file names), from its own `application`/`bootstrap` configuration.
pub(crate) fn application_names(unit: &Unit, docs: &[ConfigDoc], others: &[String]) -> Vec<(String, EvidenceRef)> {
    let mut out: Vec<(String, EvidenceRef)> = Vec::new();
    let prefix = if unit.dir.is_empty() { String::new() } else { format!("{}/", unit.dir) };
    // A unit at the repository root has an empty prefix, which every path
    // starts with — so it would claim every service's application name as its
    // own alias, and a `@FeignClient("account-service")` anywhere would resolve
    // to it. A configuration file inside another unit belongs to that unit.
    let owned_by_another = |path: &str| {
        others.iter().any(|d| !d.is_empty() && d.len() > unit.dir.len() && path.starts_with(&format!("{d}/")))
    };
    for d in docs
        .iter()
        .filter(|d| d.path.starts_with(&prefix) && CONFIG_NAMES.contains(&d.stem.as_str()))
        .filter(|d| !owned_by_another(&d.path))
    {
        // A config server's `shared/application.yml` is not this unit's identity.
        if d.dir.ends_with("/shared") || d.dir.contains("config-repo") {
            continue;
        }
        for key in ["spring.application.name", "micronaut.application.name", "quarkus.application.name"] {
            if let Some((v, line)) = d.get(key) {
                if !v.contains("${") && !out.iter().any(|(n, _)| n == v) {
                    out.push((v.to_string(), d.evidence(line, &format!("application name `{v}`"))));
                }
            }
        }
    }
    out
}

/// Configuration that applies to a unit: its own files, plus centralised
/// Spring Cloud Config files named after its application name (and the
/// shared `application.yml`).
pub(crate) fn documents_for<'d>(unit: &Unit, names: &[String], docs: &'d [ConfigDoc]) -> Vec<&'d ConfigDoc> {
    let prefix = if unit.dir.is_empty() { String::new() } else { format!("{}/", unit.dir) };
    docs.iter()
        .filter(|d| {
            let own =
                d.path.starts_with(&prefix) && CONFIG_NAMES.contains(&d.stem.as_str()) && !d.dir.ends_with("/shared");
            let central = (d.dir.ends_with("/shared") || d.dir.contains("config-repo")) && names.contains(&d.stem);
            own || central
        })
        .collect()
}

/// Property values a unit's configuration sets, for resolving `${…}` placeholders.
#[derive(Debug, Default, Clone)]
pub(crate) struct Properties {
    /// key → (value, file, line)
    entries: BTreeMap<String, (String, String, u32)>,
}

/// A `${key}` placeholder resolved to the value the configuration declares.
#[derive(Debug, Clone)]
pub(crate) struct Resolved {
    pub value: String,
    pub key: String,
}

impl Properties {
    /// Resolves a single `${key}` / `${key:default}` placeholder; text without
    /// one, or a key nothing declares, stays as written.
    pub fn resolve(&self, raw: &str) -> Option<Resolved> {
        let start = raw.find("${")?;
        let end = raw[start..].find('}')? + start;
        let inner = &raw[start + 2..end];
        let (key, default) = match inner.split_once(':') {
            Some((k, d)) => (k.trim(), Some(d.trim())),
            None => (inner.trim(), None),
        };
        let value = match self.entries.get(key) {
            Some((v, _, _)) => v.clone(),
            None => {
                return default.map(|d| Resolved {
                    value: format!("{}{d}{}", &raw[..start], &raw[end + 1..]),
                    key: key.to_string(),
                })
            }
        };
        Some(Resolved { value: format!("{}{value}{}", &raw[..start], &raw[end + 1..]), key: key.to_string() })
    }
}

/// Configuration visible to a unit at `dir`, known by `aliases`: its own
/// `application`/`bootstrap` files plus centralised Spring Cloud Config files
/// named after it (and the shared `application.yml`).
pub(crate) fn properties_for(dir: &str, aliases: &[String], docs: &[ConfigDoc]) -> Properties {
    let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
    let mut props = Properties::default();
    for d in docs {
        let central = d.dir.ends_with("/shared") || d.dir.contains("config-repo");
        let own = d.path.starts_with(&prefix) && CONFIG_NAMES.contains(&d.stem.as_str()) && !central;
        let shared = central && (aliases.iter().any(|a| a == &d.stem) || d.stem == "application");
        if !(own || shared) {
            continue;
        }
        for (k, v, line) in &d.entries {
            // A unit's own file wins over the shared one; otherwise first wins.
            let entry = props.entries.entry(k.clone());
            match entry {
                std::collections::btree_map::Entry::Vacant(e) => {
                    e.insert((v.clone(), d.path.clone(), *line));
                }
                std::collections::btree_map::Entry::Occupied(mut e) if own => {
                    e.insert((v.clone(), d.path.clone(), *line));
                }
                _ => {}
            }
        }
    }
    props
}

/// A gateway route under construction: path pattern and target (host or service id, line).
type Route = (Option<String>, Option<(String, u32)>);

/// A relationship discovered from JVM wiring, before it is resolved to units.
pub(crate) struct Wire {
    pub source: usize,
    /// Service id / application name / host as written.
    pub target: String,
    pub edge_type: EdgeType,
    pub label: String,
    pub from_code: bool,
    pub evidence: EvidenceRef,
}

/// Gateway routes (Zuul, Spring Cloud Gateway) and declarative HTTP clients.
pub(crate) fn wires(
    units: &[Unit],
    files: &[FileRec],
    names: &BTreeMap<usize, Vec<String>>,
    docs: &[ConfigDoc],
) -> Vec<Wire> {
    let mut out = Vec::new();
    for (ui, unit) in units.iter().enumerate() {
        let unit_names = names.get(&ui).cloned().unwrap_or_default();
        for d in documents_for(unit, &unit_names, docs) {
            // zuul.routes.<route>.serviceId / .url ; path gives the label
            let mut routes: BTreeMap<String, Route> = BTreeMap::new();
            for (k, v, line) in &d.entries {
                let Some(rest) = k.strip_prefix("zuul.routes.") else { continue };
                let Some((route, field)) = rest.rsplit_once('.') else { continue };
                let e = routes.entry(route.to_string()).or_default();
                match field {
                    "path" => e.0 = Some(v.clone()),
                    "serviceId" | "service-id" => e.1 = Some((v.clone(), *line)),
                    "url" if e.1.is_none() => {
                        if let Some(h) = url_host(v) {
                            e.1 = Some((h, *line));
                        }
                    }
                    _ => {}
                }
            }
            for (_, (path, target)) in routes {
                let Some((target, line)) = target else { continue };
                out.push(Wire {
                    source: ui,
                    target,
                    edge_type: EdgeType::Sync,
                    label: path.map(|p| format!("routes {p}")).unwrap_or_else(|| "routes".into()),
                    from_code: false,
                    evidence: d.evidence(line, "gateway route"),
                });
            }
            // spring.cloud.gateway.routes[i].uri = lb://service ; predicates[j] = Path=/x/**
            let mut gw: BTreeMap<String, Route> = BTreeMap::new();
            for (k, v, line) in &d.entries {
                let Some(rest) = k
                    .strip_prefix("spring.cloud.gateway.routes")
                    .or_else(|| k.strip_prefix("spring.cloud.gateway.server.webflux.routes"))
                    .or_else(|| k.strip_prefix("spring.cloud.gateway.mvc.routes"))
                else {
                    continue;
                };
                let Some(idx) = rest.split(']').next() else { continue };
                let e = gw.entry(idx.to_string()).or_default();
                if rest.ends_with(".uri") {
                    if let Some(h) = url_host(v) {
                        e.1 = Some((h, *line));
                    }
                } else if rest.contains(".predicates") && v.starts_with("Path=") {
                    e.0 = Some(v.trim_start_matches("Path=").to_string());
                }
            }
            for (_, (path, target)) in gw {
                let Some((target, line)) = target else { continue };
                out.push(Wire {
                    source: ui,
                    target,
                    edge_type: EdgeType::Sync,
                    label: path.map(|p| format!("routes {p}")).unwrap_or_else(|| "routes".into()),
                    from_code: false,
                    evidence: d.evidence(line, "gateway route"),
                });
            }
        }
        for &fi in &unit.files {
            let f = &files[fi];
            if !f.language.is_jvm() {
                continue;
            }
            for a in f.facts.annotations.iter().filter(|a| a.target_kind == "interface" || a.target_kind == "class") {
                let target = match a.name.as_str() {
                    "FeignClient" => arg(a, &["name", "value", ""])
                        .filter(|n| !n.contains("${"))
                        .or_else(|| arg(a, &["url"]).and_then(|u| url_host(&u))),
                    "RegisterRestClient" => arg(a, &["configKey", "baseUri"]).map(|v| url_host(&v).unwrap_or(v)),
                    "Client" => arg(a, &["id", "value", ""]).map(|v| url_host(&v).unwrap_or(v)),
                    _ => None,
                };
                let Some(target) = target.filter(|t| !t.is_empty() && !t.starts_with('/')) else { continue };
                out.push(Wire {
                    source: ui,
                    target,
                    edge_type: EdgeType::Sync,
                    label: "HTTP".into(),
                    from_code: true,
                    evidence: EvidenceRef {
                        file_path: f.path.clone(),
                        start_line: a.line,
                        end_line: a.target_end,
                        symbol_name: Some(a.target.clone()),
                        note: Some(format!("`@{}` {}", a.name, a.target)),
                    },
                });
            }
        }
    }
    out
}

/// Annotation argument by name (`""` = the unnamed `value`).
pub fn arg(a: &Annotation, names: &[&str]) -> Option<String> {
    for part in split_top_level(&a.arguments) {
        let (key, value) = match part.split_once('=') {
            Some((k, v)) if !k.contains('"') => (k.trim(), v.trim()),
            _ => ("", part.trim()),
        };
        if names.contains(&key) {
            let v = value.trim_start_matches('{').trim_end_matches('}').trim();
            let first = split_top_level(v).into_iter().next().unwrap_or_default();
            let unq = first.trim().trim_matches('"').to_string();
            if first.trim().starts_with('"') {
                return Some(unq);
            }
        }
    }
    None
}

fn split_top_level(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '"' => in_str = !in_str,
            '(' | '{' | '[' if !in_str => depth += 1,
            ')' | '}' | ']' if !in_str => depth -= 1,
            ',' if !in_str && depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// `lb://account-service` / `http://auth-service:5000/x` → host.
pub fn url_host(v: &str) -> Option<String> {
    let rest = v.split_once("://").map(|(_, r)| r)?;
    let host: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')).collect();
    (!host.is_empty() && !host.starts_with("localhost") && host.chars().next().is_some_and(|c| c.is_ascii_alphabetic()))
        .then_some(host)
}

/// Message listeners declared by annotation, and the producer calls of JVM
/// messaging templates.
pub const LISTENER_ANNOTATIONS: &[&str] = &[
    "KafkaListener",
    "RabbitListener",
    "JmsListener",
    "SqsListener",
    "StreamListener",
    "Incoming",
    "KafkaHandler",
    "RabbitHandler",
];
pub const PRODUCER_ANNOTATIONS: &[&str] = &["Outgoing", "SendTo"];

pub fn is_jvm_producer_call(name: &str, callee: &str) -> bool {
    let c = callee.to_lowercase();
    matches!(name, "convertAndSend" | "convertSendAndReceive")
        || (matches!(name, "send" | "sendDefault" | "sendAndReceive" | "publish")
            && ["template", "bridge", "emitter", "producer", "publisher"].iter().any(|w| c.contains(w)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ann(name: &str, args: &str) -> Annotation {
        Annotation {
            name: name.into(),
            arguments: args.into(),
            line: 1,
            target_kind: "interface".into(),
            target: "X".into(),
            owner: None,
            target_start: 1,
            target_end: 3,
        }
    }

    #[test]
    fn annotation_arguments() {
        assert_eq!(
            arg(&ann("FeignClient", r#"name = "statistics-service", fallback = F.class"#), &["name"]).as_deref(),
            Some("statistics-service")
        );
        assert_eq!(arg(&ann("Client", r#""payments""#), &["id", "value", ""]).as_deref(), Some("payments"));
        assert_eq!(
            arg(&ann("KafkaListener", r#"topics = {"orders", "refunds"}, groupId = "g""#), &["topics"]).as_deref(),
            Some("orders")
        );
        assert_eq!(
            arg(&ann("FeignClient", r#"url = "${rates.url}", name = "rates-client""#), &["url"]).as_deref(),
            Some("${rates.url}")
        );
    }

    #[test]
    fn yaml_flattens_with_lines_and_profiles() {
        let y = "server:\n  port: 4000\nspring:\n  application:\n    name: account-service\n  cloud:\n    gateway:\n      routes:\n        - id: a\n          uri: lb://account-service\n          predicates:\n            - Path=/accounts/**\n";
        let e = parse_yaml(y);
        let get = |k: &str| e.iter().find(|(key, _, _)| key == k).map(|(_, v, l)| (v.as_str(), *l));
        assert_eq!(get("server.port"), Some(("4000", 2)));
        assert_eq!(get("spring.application.name"), Some(("account-service", 5)));
        assert_eq!(get("spring.cloud.gateway.routes[0].uri"), Some(("lb://account-service", 10)));
        assert_eq!(get("spring.cloud.gateway.routes[0].predicates[0]"), Some(("Path=/accounts/**", 12)));
        assert_eq!(strip_profile("application-docker"), "application");
        assert_eq!(url_host("http://auth-service:5000/uaa").as_deref(), Some("auth-service"));
        assert_eq!(url_host("http://localhost:8080"), None);
    }
}
