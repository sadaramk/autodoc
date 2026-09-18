//! docker-compose: the most honest runtime topology a repository declares.

use std::path::Path;

use serde_yaml::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct ComposeService {
    pub name: String,
    /// Build context relative to the repository root, normalised ("" = root).
    pub build_dir: Option<String>,
    /// Directory of `build.dockerfile` when it differs from the context (`context: .`, `dockerfile: api/Dockerfile`).
    pub dockerfile_dir: Option<String>,
    pub image: Option<String>,
    pub publishes_ports: bool,
    pub depends_on: Vec<String>,
    pub environment: Vec<(String, String)>,
    /// Paths of `env_file` entries, relative to the repository root.
    pub env_files: Vec<String>,
    /// Variables loaded from `env_files` (shared files over-approximate; confirm in code).
    pub env_file_vars: Vec<(String, String)>,
    /// 1-based line of the service key.
    pub line: u32,
}

#[derive(Debug, Clone)]
pub struct ComposeFile {
    pub file: String,
    pub text: String,
    pub services: Vec<ComposeService>,
}

pub const COMPOSE_FILES: &[&str] = &["docker-compose.yml", "docker-compose.yaml", "compose.yml", "compose.yaml"];
const COMPOSE_DIRS: &[&str] = &["", "docker", "deploy", "deployment", "deployments", ".docker", "infra", "ops"];

/// Every compose file in the usual places: the canonical names first, then
/// variants (`docker-compose.dev.yml`), so base definitions win on conflicts.
pub fn find_all(root: &Path) -> Vec<ComposeFile> {
    let mut out = Vec::new();
    for dir in COMPOSE_DIRS {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else { continue };
        let mut names: Vec<String> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| {
                (n.starts_with("docker-compose") || n.starts_with("compose"))
                    && (n.ends_with(".yml") || n.ends_with(".yaml"))
            })
            .collect();
        names
            .sort_by_key(|n| (!COMPOSE_FILES.contains(&n.as_str()), n.contains("test") || n.contains("ci"), n.clone()));
        for name in names {
            let rel = if dir.is_empty() { name.clone() } else { format!("{dir}/{name}") };
            if let Some(mut c) = std::fs::read_to_string(root.join(&rel)).ok().and_then(|t| parse(&rel, &t)) {
                for s in &mut c.services {
                    for f in &s.env_files {
                        let Ok(text) = std::fs::read_to_string(root.join(f)) else { continue };
                        s.env_file_vars.extend(parse_env_file(&text));
                    }
                }
                out.push(c);
            }
        }
    }
    out
}

pub fn parse(file: &str, text: &str) -> Option<ComposeFile> {
    let doc: Value = serde_yaml::from_str(text).ok()?;
    let services = doc.get("services")?.as_mapping()?;
    let base = Path::new(file).parent().unwrap_or(Path::new(""));
    let mut out = Vec::new();
    for (k, v) in services {
        let Some(name) = k.as_str() else { continue };
        let build_ctx = match v.get("build") {
            Some(Value::String(s)) => Some(s.clone()),
            Some(b) => Some(b.get("context").and_then(Value::as_str).unwrap_or(".").to_string()),
            None => None,
        };
        let norm =
            |p: &Path| crate::manifest::normalize(p).to_string_lossy().replace('\\', "/").trim_matches('/').to_string();
        let build_dir = build_ctx.as_ref().map(|c| norm(&base.join(c)));
        let dockerfile_dir =
            match (build_ctx.as_ref(), v.get("build").and_then(|b| b.get("dockerfile")).and_then(Value::as_str)) {
                (Some(ctx), Some(df)) => {
                    Path::new(df).parent().filter(|p| !p.as_os_str().is_empty()).map(|p| norm(&base.join(ctx).join(p)))
                }
                _ => None,
            };
        let depends_on = match v.get("depends_on") {
            Some(Value::Sequence(s)) => s.iter().filter_map(Value::as_str).map(str::to_string).collect(),
            Some(Value::Mapping(m)) => m.keys().filter_map(Value::as_str).map(str::to_string).collect(),
            _ => vec![],
        };
        let env_files: Vec<String> = match v.get("env_file") {
            Some(Value::String(s)) => vec![s.clone()],
            Some(Value::Sequence(seq)) => seq
                .iter()
                .filter_map(|e| {
                    e.as_str().map(str::to_string).or_else(|| e.get("path").and_then(Value::as_str).map(str::to_string))
                })
                .collect(),
            _ => vec![],
        }
        .into_iter()
        .map(|f| norm(&base.join(f)))
        .collect();
        let environment = match v.get("environment") {
            Some(Value::Sequence(s)) => s
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|kv| kv.split_once('='))
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
            Some(Value::Mapping(m)) => m
                .iter()
                .filter_map(|(a, b)| {
                    let val = match b {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        Value::Bool(x) => x.to_string(),
                        _ => String::new(),
                    };
                    Some((a.as_str()?.to_string(), val))
                })
                .collect(),
            _ => vec![],
        };
        let line = service_line(text, name).unwrap_or(1);
        out.push(ComposeService {
            name: name.to_string(),
            build_dir,
            dockerfile_dir,
            publishes_ports: v.get("ports").is_some() || v.get("expose").is_some(),
            env_files,
            env_file_vars: vec![],
            image: v.get("image").and_then(Value::as_str).map(str::to_string),
            depends_on,
            environment,
            line,
        });
    }
    Some(ComposeFile { file: file.to_string(), text: text.to_string(), services: out })
}

/// 1-based line of `name:` at the indentation of the direct children of
/// `services:` (so a `depends_on: { db: … }` key isn't mistaken for it).
fn service_line(text: &str, name: &str) -> Option<u32> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|l| l.trim_end() == "services:")?;
    let indent = |l: &str| l.len() - l.trim_start().len();
    let child_indent = lines[start + 1..]
        .iter()
        .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .map(|l| indent(l))?;
    lines[start + 1..]
        .iter()
        .enumerate()
        .take_while(|(_, l)| l.trim().is_empty() || indent(l) >= child_indent)
        .find(|(_, l)| {
            indent(l) == child_indent
                && l.trim_start().trim_end().strip_suffix(':').is_some_and(|k| k.trim_matches('"') == name)
        })
        .map(|(i, _)| (start + 1 + i) as u32 + 1)
}

/// `KEY=value` lines of a dotenv file (comments, `export` and quotes handled).
pub fn parse_env_file(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.strip_prefix("export ").unwrap_or(l).split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().trim_matches(['"', '\'']).to_string()))
        .collect()
}

/// Hostnames referenced by a connection string or URL
/// (`postgres://u@db:5432/x` → `db`, `kafka:9092,kafka2:9092` → both).
pub fn hosts_in(value: &str) -> Vec<String> {
    value
        .split(',')
        .filter_map(|part| {
            let part = part.trim();
            let after_scheme = part.split_once("://").map(|(_, r)| r).unwrap_or(part);
            let authority = after_scheme.split('/').next()?;
            let host_port = authority.rsplit('@').next()?;
            let host = host_port.split(':').next()?;
            let valid = !host.is_empty()
                && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
                && host.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
            valid.then(|| host.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_environment_shapes_and_depends_on_forms() {
        let yaml = "services:\n  api:\n    build:\n      context: ./svc/api\n    environment:\n      - PAYMENTS_URL=http://payments:8080\n    depends_on:\n      db:\n        condition: service_healthy\n  db:\n    image: postgres:16\n    environment:\n      POSTGRES_DB: shop\n";
        let c = parse("deploy/docker-compose.yml", yaml).unwrap();
        let api = &c.services[0];
        assert_eq!(api.build_dir.as_deref(), Some("deploy/svc/api"));
        assert_eq!(api.dockerfile_dir, None);
        assert_eq!(api.depends_on, vec!["db"]);
        assert_eq!(api.environment[0], ("PAYMENTS_URL".into(), "http://payments:8080".into()));
        assert_eq!(api.line, 2);
        assert_eq!(c.services[1].image.as_deref(), Some("postgres:16"));
        assert_eq!(c.services[1].line, 10);
    }

    #[test]
    fn dockerfile_directory_identifies_root_context_builds() {
        let yaml = "services:\n  backend:\n    build:\n      context: .\n      dockerfile: backend/Dockerfile\n    ports: [\"8000:8000\"]\n";
        let c = parse("compose.yml", yaml).unwrap();
        assert_eq!(c.services[0].build_dir.as_deref(), Some(""));
        assert_eq!(c.services[0].dockerfile_dir.as_deref(), Some("backend"));
        assert!(c.services[0].publishes_ports);
    }

    #[test]
    fn host_extraction() {
        assert_eq!(hosts_in("postgres://shop:pw@db:5432/shop"), vec!["db"]);
        assert_eq!(hosts_in("kafka:9092,kafka-2:9092"), vec!["kafka", "kafka-2"]);
        assert_eq!(hosts_in("${STRIPE_API_KEY}"), Vec::<String>::new());
        assert_eq!(hosts_in("http://payments:8080/v1"), vec!["payments"]);
    }
}
