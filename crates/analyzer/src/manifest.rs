//! Build manifests define deployable/buildable units — the C4 "containers".

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::lang::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestKind {
    Cargo,
    PackageJson,
    GoMod,
    Pyproject,
    Requirements,
    Maven,
    Gradle,
    MsBuild,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    pub name: String,
    /// 1-based line in the manifest where the dependency is declared.
    pub line: u32,
    /// Local path dependency (workspace sibling).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Transitively required, not used by this module directly: `// indirect`
    /// in a `go.mod`. Like `dev`, it never implies architecture.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub indirect: bool,
    /// Test/build-only dependency: informs tooling detection, never architecture.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dev: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub kind: ManifestKind,
    /// Manifest path relative to the repository root.
    pub file: String,
    /// Directory holding the manifest, relative to the repository root ("" = root).
    pub dir: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub language: Language,
    pub dependencies: Vec<Dependency>,
    /// Pure aggregators (Cargo `[workspace]` without `[package]`, npm workspaces root).
    pub is_workspace_root: bool,
}

pub const MANIFEST_FILES: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "go.mod",
    "pyproject.toml",
    "requirements.txt",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
];

/// MSBuild names a project after its file, so `.csproj` is matched by suffix
/// rather than by name. `.sln` is an aggregator only — it carries no
/// dependencies, and every project it lists has its own `.csproj`.
pub fn is_manifest(name: &str) -> bool {
    MANIFEST_FILES.contains(&name) || name.ends_with(".csproj")
}

pub fn parse(root: &Path, file: &Path) -> Option<Manifest> {
    let text = std::fs::read_to_string(root.join(file)).ok()?;
    let rel = file.to_string_lossy().replace('\\', "/");
    let dir = file.parent().map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
    let fname = file.file_name()?.to_str()?;
    let mut m = match fname {
        "Cargo.toml" => parse_cargo(&text)?,
        "package.json" => parse_package_json(&text)?,
        "go.mod" => parse_go_mod(&text),
        "pyproject.toml" => parse_pyproject(&text)?,
        "requirements.txt" => parse_requirements(&text),
        "pom.xml" => parse_pom(&text)?,
        "build.gradle" | "build.gradle.kts" => parse_gradle(root, &dir, &text),
        _ if fname.ends_with(".csproj") => parse_csproj(fname, &text)?,
        _ => return None,
    };
    m.file = rel;
    m.dir = dir;
    Some(m)
}

fn blank(kind: ManifestKind, language: Language) -> Manifest {
    Manifest {
        kind,
        file: String::new(),
        dir: String::new(),
        name: None,
        description: None,
        language,
        dependencies: vec![],
        is_workspace_root: false,
    }
}

/// Line of the first occurrence of `needle` as a key/quoted token.
fn line_of(text: &str, needle: &str) -> u32 {
    let quoted = format!("\"{needle}\"");
    let quoted_spec = format!("\"{needle}");
    let is_boundary =
        |c: Option<char>| c.is_none_or(|c| matches!(c, ' ' | '=' | '.' | '[' | '<' | '>' | '~' | '!' | '"'));
    text.lines()
        .position(|l| {
            let t = l.trim_start();
            t.contains(&quoted)
                || (t.starts_with(needle) && is_boundary(t[needle.len()..].chars().next()))
                || t.find(&quoted_spec).is_some_and(|i| is_boundary(t[i + quoted_spec.len()..].chars().next()))
        })
        .map(|i| i as u32 + 1)
        .unwrap_or(1)
}

fn parse_cargo(text: &str) -> Option<Manifest> {
    let v: toml::Value = toml::from_str(text).ok()?;
    let mut m = blank(ManifestKind::Cargo, Language::Rust);
    let pkg = v.get("package");
    m.name = pkg.and_then(|p| p.get("name")).and_then(|n| n.as_str()).map(str::to_string);
    m.description = pkg.and_then(|p| p.get("description")).and_then(|n| n.as_str()).map(str::to_string);
    m.is_workspace_root = pkg.is_none() && v.get("workspace").is_some();
    for table in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let Some(deps) = v.get(table).and_then(|d| d.as_table()) else { continue };
        for (name, spec) in deps {
            let real = spec.get("package").and_then(|p| p.as_str()).unwrap_or(name);
            m.dependencies.push(Dependency {
                dev: table != "dependencies",
                indirect: false,
                name: real.to_string(),
                line: line_of(text, name),
                path: spec.get("path").and_then(|p| p.as_str()).map(str::to_string),
            });
        }
    }
    Some(m)
}

fn parse_package_json(text: &str) -> Option<Manifest> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let mut m = blank(ManifestKind::PackageJson, Language::JavaScript);
    m.name = v.get("name").and_then(|n| n.as_str()).map(str::to_string);
    m.description = v.get("description").and_then(|n| n.as_str()).map(str::to_string);
    for table in ["dependencies", "devDependencies", "peerDependencies"] {
        let Some(deps) = v.get(table).and_then(|d| d.as_object()) else { continue };
        for (name, spec) in deps {
            let path = spec
                .as_str()
                .and_then(|s| s.strip_prefix("file:").or_else(|| s.strip_prefix("link:")))
                .map(str::to_string);
            m.dependencies.push(Dependency {
                name: name.clone(),
                line: line_of(text, name),
                path,
                dev: table == "devDependencies",
                indirect: false,
            });
        }
    }
    if m.dependencies.iter().any(|d| d.name == "typescript") || text.contains("\"types\"") {
        m.language = Language::TypeScript;
    }
    m.is_workspace_root = v.get("workspaces").is_some();
    Some(m)
}

fn parse_go_mod(text: &str) -> Manifest {
    let mut m = blank(ManifestKind::GoMod, Language::Go);
    let mut in_require = false;
    for (i, raw) in text.lines().enumerate() {
        // The marker lives in the comment, so read it before stripping.
        let indirect = raw.contains("// indirect");
        let l = raw.split("//").next().unwrap_or("").trim();
        let line = i as u32 + 1;
        if let Some(module) = l.strip_prefix("module ") {
            m.name = Some(module.trim().to_string());
        } else if l.starts_with("require (") {
            in_require = true;
        } else if in_require && l == ")" {
            in_require = false;
        } else if let Some(dep) = l.strip_prefix("require ").or(in_require.then_some(l)) {
            if let Some(name) = dep.split_whitespace().next().filter(|n| n.contains('.')) {
                m.dependencies.push(Dependency { name: name.to_string(), line, path: None, dev: false, indirect });
            }
        } else if let Some(rep) = l.strip_prefix("replace ") {
            if let Some((from, to)) = rep.split_once("=>") {
                let to = to.trim();
                if to.starts_with('.') {
                    let from = from.split_whitespace().next().unwrap_or("").to_string();
                    m.dependencies.push(Dependency {
                        name: from,
                        line,
                        path: Some(to.to_string()),
                        dev: false,
                        indirect: false,
                    });
                }
            }
        }
    }
    m
}

fn parse_pyproject(text: &str) -> Option<Manifest> {
    let v: toml::Value = toml::from_str(text).ok()?;
    let mut m = blank(ManifestKind::Pyproject, Language::Python);
    let project = v.get("project");
    let poetry = v.get("tool").and_then(|t| t.get("poetry"));
    m.name = project.or(poetry).and_then(|p| p.get("name")).and_then(|n| n.as_str()).map(str::to_string);
    m.description = project.or(poetry).and_then(|p| p.get("description")).and_then(|n| n.as_str()).map(str::to_string);
    // A root pyproject with only a uv workspace and tooling groups aggregates members.
    let uv_workspace = v.get("tool").and_then(|t| t.get("uv")).and_then(|u| u.get("workspace")).is_some();
    m.is_workspace_root = project.is_none() && poetry.is_none() && uv_workspace;
    if let Some(deps) = project.and_then(|p| p.get("dependencies")).and_then(|d| d.as_array()) {
        for d in deps.iter().filter_map(|d| d.as_str()) {
            let name = requirement_name(d);
            m.dependencies.push(Dependency {
                line: line_of(text, &name),
                name,
                path: None,
                dev: false,
                indirect: false,
            });
        }
    }
    if let Some(deps) = poetry.and_then(|p| p.get("dependencies")).and_then(|d| d.as_table()) {
        for name in deps.keys().filter(|k| *k != "python") {
            m.dependencies.push(Dependency {
                name: name.clone(),
                line: line_of(text, name),
                path: None,
                dev: false,
                indirect: false,
            });
        }
    }
    Some(m)
}

fn parse_requirements(text: &str) -> Manifest {
    let mut m = blank(ManifestKind::Requirements, Language::Python);
    for (i, l) in text.lines().enumerate() {
        let l = l.split('#').next().unwrap_or("").trim();
        if l.is_empty() || l.starts_with('-') {
            continue;
        }
        m.dependencies.push(Dependency {
            name: requirement_name(l),
            line: i as u32 + 1,
            path: None,
            dev: false,
            indirect: false,
        });
    }
    m
}

/// `psycopg[binary]>=3.1` → `psycopg`
fn requirement_name(spec: &str) -> String {
    spec.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_' || c == '.'))
        .next()
        .unwrap_or(spec)
        .to_lowercase()
}

/// Deepest manifest directory that contains `file` (both repo-relative).
/// Maven `pom.xml`. Dependencies are `groupId:artifactId`; a `pom`-packaged
/// project listing `<modules>` is an aggregator, not a unit.
fn parse_pom(text: &str) -> Option<Manifest> {
    let doc = roxmltree::Document::parse(text).ok()?;
    let project = doc.root_element();
    fn child<'a, 'i>(parent: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
        parent.children().find(|c| c.has_tag_name(name))
    }
    let text_of = |parent: roxmltree::Node, name: &str| {
        child(parent, name).and_then(|c| c.text()).map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
    };
    let line_at = |n: roxmltree::Node| doc.text_pos_at(n.range().start).row;
    let mut m = blank(ManifestKind::Maven, Language::Java);
    m.name = text_of(project, "artifactId");
    m.description = text_of(project, "description").or_else(|| text_of(project, "name"));
    let packaging = text_of(project, "packaging").unwrap_or_else(|| "jar".into());
    let modules =
        child(project, "modules").map(|ms| ms.children().filter(|c| c.has_tag_name("module")).count()).unwrap_or(0);
    m.is_workspace_root = packaging == "pom" && modules > 0;
    if let Some(parent) = child(project, "parent") {
        // A parent POM declares the framework for its children (spring-boot-starter-parent).
        if let (Some(g), Some(a)) = (text_of(parent, "groupId"), text_of(parent, "artifactId")) {
            m.dependencies.push(Dependency {
                name: format!("{g}:{a}"),
                line: line_at(parent),
                path: None,
                dev: false,
                indirect: false,
            });
        }
    }
    if let Some(deps) = child(project, "dependencies") {
        for d in deps.children().filter(|c| c.has_tag_name("dependency")) {
            let (Some(g), Some(a)) = (text_of(d, "groupId"), text_of(d, "artifactId")) else { continue };
            let scope = text_of(d, "scope").unwrap_or_default();
            m.dependencies.push(Dependency {
                name: format!("{g}:{a}"),
                line: line_at(d),
                path: None,
                dev: matches!(scope.as_str(), "test" | "provided"),
                indirect: false,
            });
        }
    }
    if let Some(build) = child(project, "build").and_then(|b| child(b, "plugins")) {
        for p in build.children().filter(|c| c.has_tag_name("plugin")) {
            if let Some(a) = text_of(p, "artifactId") {
                let g = text_of(p, "groupId").unwrap_or_else(|| "org.apache.maven.plugins".into());
                // Build plugins identify the runtime (spring-boot-maven-plugin, quarkus-maven-plugin).
                m.dependencies.push(Dependency {
                    name: format!("{g}:{a}"),
                    line: line_at(p),
                    path: None,
                    dev: true,
                    indirect: false,
                });
            }
        }
    }
    Some(m)
}

/// Gradle `build.gradle(.kts)`: string coordinates, `project(':x')` path
/// dependencies and applied plugins. A build with subprojects and no sources
/// of its own is an aggregator.
fn parse_gradle(root: &Path, dir: &str, text: &str) -> Manifest {
    let mut m = blank(ManifestKind::Gradle, Language::Java);
    let abs = root.join(dir);
    m.name = Some(if dir.is_empty() {
        root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
    } else {
        dir.rsplit('/').next().unwrap_or(dir).to_string()
    });
    let settings =
        ["settings.gradle", "settings.gradle.kts"].iter().find_map(|f| std::fs::read_to_string(abs.join(f)).ok());
    let includes = settings.as_deref().is_some_and(|s| s.lines().any(|l| l.trim_start().starts_with("include")));
    m.is_workspace_root = includes && !abs.join("src/main").is_dir();
    const CONFIGS: &[&str] = &[
        "implementation",
        "api",
        "compile",
        "compileOnly",
        "runtimeOnly",
        "annotationProcessor",
        "kapt",
        "testImplementation",
        "testCompile",
        "testRuntimeOnly",
        "testCompileOnly",
        "developmentOnly",
    ];
    let depth = if dir.is_empty() { 0 } else { dir.split('/').count() };
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        let Some(config) =
            CONFIGS.iter().find(|c| line.strip_prefix(**c).is_some_and(|rest| rest.starts_with([' ', '(', '\'', '"'])))
        else {
            // plugins { id 'org.springframework.boot' } / apply plugin: 'io.quarkus'
            let plugin = line
                .strip_prefix("id")
                .filter(|r| r.starts_with([' ', '(']))
                .or_else(|| line.strip_prefix("apply plugin:"))
                .and_then(first_quoted);
            if let Some(p) = plugin {
                m.dependencies.push(Dependency { name: p, line: i as u32 + 1, path: None, dev: true, indirect: false });
            }
            continue;
        };
        let dev = config.starts_with("test")
            || matches!(*config, "compileOnly" | "annotationProcessor" | "kapt" | "developmentOnly");
        let rest = &line[config.len()..];
        if let Some(p) = rest.find("project(") {
            if let Some(path) = first_quoted(&rest[p..]) {
                let rel = path.trim_start_matches(':').replace(':', "/");
                m.dependencies.push(Dependency {
                    name: path.rsplit(':').next().unwrap_or(&path).to_string(),
                    line: i as u32 + 1,
                    path: Some(format!("{}{rel}", "../".repeat(depth))),
                    dev,
                    indirect: false,
                });
            }
            continue;
        }
        if let Some(coord) = first_quoted(rest) {
            let mut parts = coord.split(':');
            if let (Some(g), Some(a)) = (parts.next(), parts.next()) {
                m.dependencies.push(Dependency {
                    name: format!("{g}:{a}"),
                    line: i as u32 + 1,
                    path: None,
                    dev,
                    indirect: false,
                });
            }
        }
    }
    m
}

fn first_quoted(s: &str) -> Option<String> {
    let start = s.find(['\'', '"'])?;
    let q = s[start..].chars().next()?;
    let end = s[start + 1..].find(q)?;
    Some(s[start + 1..start + 1 + end].to_string())
}

pub fn owner_dir<'a>(dirs: &'a [String], file: &str) -> Option<&'a String> {
    dirs.iter().filter(|d| d.is_empty() || file.starts_with(&format!("{d}/"))).max_by_key(|d| d.len())
}

pub fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_workspace_and_renamed_path_deps() {
        let m = parse_cargo("[workspace]\nmembers=[\"a\"]\n").unwrap();
        assert!(m.is_workspace_root);
        let m = parse_cargo("[package]\nname = \"svc\"\n\n[dependencies]\nsqlx = { version = \"0.8\" }\ncore = { package = \"shop-core\", path = \"../core\" }\n").unwrap();
        assert_eq!(m.name.as_deref(), Some("svc"));
        let sqlx = m.dependencies.iter().find(|d| d.name == "sqlx").unwrap();
        assert_eq!(sqlx.line, 5);
        let core = m.dependencies.iter().find(|d| d.name == "shop-core").unwrap();
        assert_eq!(core.path.as_deref(), Some("../core"));
    }

    #[test]
    fn dependency_lines_match_whole_names() {
        let text = "[dependencies]\nserde_json = \"1\"\nserde = \"1\"\n\n[project]\ndependencies = [\n  \"psycopg[binary]>=3\",\n]\n";
        assert_eq!(line_of(text, "serde"), 3);
        assert_eq!(line_of(text, "serde_json"), 2);
        assert_eq!(line_of(text, "psycopg"), 7);
    }

    #[test]
    fn go_mod_require_block_and_replace() {
        let m = parse_go_mod("module github.com/acme/pay\n\ngo 1.22\n\nrequire (\n\tgithub.com/stripe/stripe-go/v76 v76.0.0\n\tgithub.com/jackc/pgx/v5 v5.5.0 // indirect\n)\nrequire golang.org/x/sync v0.1.0\nreplace github.com/acme/core => ../core\n");
        assert_eq!(m.name.as_deref(), Some("github.com/acme/pay"));
        // `// indirect` means another module needs it, not this one — it is no
        // evidence that this code talks to a database, so it is recorded and
        // flagged rather than treated as a direct dependency.
        let names: Vec<_> = m.dependencies.iter().map(|d| (d.name.as_str(), d.line, d.indirect)).collect();
        assert_eq!(
            names,
            vec![
                ("github.com/stripe/stripe-go/v76", 6, false),
                ("github.com/jackc/pgx/v5", 7, true),
                ("golang.org/x/sync", 9, false),
                ("github.com/acme/core", 10, false)
            ]
        );
    }

    #[test]
    fn python_requirement_names() {
        assert_eq!(requirement_name("psycopg[binary]>=3.1"), "psycopg");
        assert_eq!(requirement_name("confluent-kafka==2.3"), "confluent-kafka");
        let m = parse_requirements("# deps\nFlask>=3\n-r base.txt\n");
        assert_eq!(m.dependencies.len(), 1);
        assert_eq!((m.dependencies[0].name.as_str(), m.dependencies[0].line), ("flask", 2));
    }

    #[test]
    fn package_json_detects_typescript() {
        let m = parse_package_json("{\n  \"name\": \"web\",\n  \"dependencies\": {\n    \"react\": \"^18\"\n  },\n  \"devDependencies\": { \"typescript\": \"^5\" }\n}").unwrap();
        assert_eq!(m.language, Language::TypeScript);
        assert_eq!(m.dependencies[0].line, 4);
    }

    #[test]
    fn owner_is_deepest_dir() {
        let dirs = vec!["".to_string(), "svc".to_string(), "svc/api".to_string()];
        assert_eq!(owner_dir(&dirs, "svc/api/src/x.ts").unwrap(), "svc/api");
        assert_eq!(owner_dir(&dirs, "svc/lib.rs").unwrap(), "svc");
        assert_eq!(owner_dir(&dirs, "svcx/a.rs").unwrap(), "");
    }
}

#[cfg(test)]
mod jvm_tests {
    use super::*;

    #[test]
    fn maven_pom_dependencies_parent_and_aggregator() {
        let pom = r#"<?xml version="1.0"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <parent>
    <groupId>org.springframework.boot</groupId>
    <artifactId>spring-boot-starter-parent</artifactId>
  </parent>
  <artifactId>account-service</artifactId>
  <description>Accounts</description>
  <dependencyManagement><dependencies><dependency><groupId>x</groupId><artifactId>managed</artifactId></dependency></dependencies></dependencyManagement>
  <dependencies>
    <dependency>
      <groupId>org.springframework.boot</groupId>
      <artifactId>spring-boot-starter-web</artifactId>
    </dependency>
    <dependency>
      <groupId>org.junit.jupiter</groupId>
      <artifactId>junit-jupiter</artifactId>
      <scope>test</scope>
    </dependency>
  </dependencies>
</project>"#;
        let m = parse_pom(pom).unwrap();
        assert_eq!(m.name.as_deref(), Some("account-service"));
        assert!(!m.is_workspace_root);
        let names: Vec<(&str, u32, bool)> = m.dependencies.iter().map(|d| (d.name.as_str(), d.line, d.dev)).collect();
        assert_eq!(
            names,
            vec![
                ("org.springframework.boot:spring-boot-starter-parent", 3, false),
                ("org.springframework.boot:spring-boot-starter-web", 11, false),
                ("org.junit.jupiter:junit-jupiter", 15, true),
            ]
        );
        let agg = parse_pom("<project><artifactId>root</artifactId><packaging>pom</packaging><modules><module>a</module></modules></project>").unwrap();
        assert!(agg.is_workspace_root);
    }

    #[test]
    fn gradle_coordinates_projects_and_plugins() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("services/orders/src/main")).unwrap();
        let text = "plugins {\n  id 'org.springframework.boot' version '3.2.0'\n}\ndependencies {\n  implementation 'org.springframework.boot:spring-boot-starter-web'\n  implementation(project(\":libs:core\"))\n  testImplementation(\"org.junit.jupiter:junit-jupiter:5.10.0\")\n}\n";
        let m = parse_gradle(dir.path(), "services/orders", text);
        let deps: Vec<(&str, Option<&str>, bool)> =
            m.dependencies.iter().map(|d| (d.name.as_str(), d.path.as_deref(), d.dev)).collect();
        assert_eq!(
            deps,
            vec![
                ("org.springframework.boot", None, true),
                ("org.springframework.boot:spring-boot-starter-web", None, false),
                ("core", Some("../../libs/core"), false),
                ("org.junit.jupiter:junit-jupiter", None, true),
            ]
        );
        assert_eq!(m.name.as_deref(), Some("orders"));
        assert!(!m.is_workspace_root);
    }
}

/// MSBuild `*.csproj`. The project is named after the file unless
/// `<AssemblyName>` overrides it; dependencies are `<PackageReference>` NuGet
/// ids plus `<ProjectReference>` sibling paths. `Microsoft.NET.Sdk.Web` in the
/// `Sdk` attribute is what marks the project as an ASP.NET Core service, so it
/// is recorded as a dependency too — nothing else in the file says so.
fn parse_csproj(fname: &str, text: &str) -> Option<Manifest> {
    let doc = roxmltree::Document::parse(text).ok()?;
    let project = doc.root_element();
    let line_at = |n: roxmltree::Node| doc.text_pos_at(n.range().start).row;
    let mut m = blank(ManifestKind::MsBuild, Language::CSharp);
    m.name = Some(fname.trim_end_matches(".csproj").to_string());
    for g in project.children().filter(|c| c.has_tag_name("PropertyGroup")) {
        for c in g.children().filter(|c| c.is_element()) {
            let v = c.text().map(str::trim).filter(|t| !t.is_empty());
            match c.tag_name().name() {
                "AssemblyName" => m.name = v.map(str::to_string).or(m.name.take()),
                "Description" => m.description = v.map(str::to_string),
                _ => {}
            }
        }
    }
    if let Some(sdk) = project.attribute("Sdk") {
        m.dependencies.push(Dependency {
            name: sdk.to_string(),
            line: line_at(project),
            path: None,
            dev: false,
            indirect: false,
        });
    }
    for g in project.children().filter(|c| c.has_tag_name("ItemGroup")) {
        for r in g.children().filter(|c| c.is_element()) {
            let Some(include) = r.attribute("Include") else { continue };
            match r.tag_name().name() {
                "PackageReference" => m.dependencies.push(Dependency {
                    name: include.to_string(),
                    line: line_at(r),
                    path: None,
                    dev: false,
                    indirect: false,
                }),
                // `..\\Other\\Other.csproj` — a sibling project, so the path is
                // normalized the way a Cargo/Gradle path dependency is.
                "ProjectReference" => {
                    let path = include.replace('\\', "/");
                    let name = path.rsplit('/').next().unwrap_or(&path).trim_end_matches(".csproj").to_string();
                    m.dependencies.push(Dependency {
                        name,
                        line: line_at(r),
                        path: Some(path),
                        dev: false,
                        indirect: false,
                    });
                }
                _ => {}
            }
        }
    }
    Some(m)
}
