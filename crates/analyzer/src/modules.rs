//! Component-level view of one unit: modules and the import graph between them.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use crate::catalog::InfraKind;
use crate::extract::SymbolKind;
use crate::lang::Language;
use crate::scan::{slug, ComponentView, EvidenceRef, FileRec, ModuleDependency, ModuleSummary, Unit};

/// Number of modules a unit decomposes into.
pub(crate) fn module_count(unit: &Unit, files: &[FileRec]) -> usize {
    let rel = |fi: usize| -> String {
        let p = &files[fi].path;
        if unit.dir.is_empty() {
            p.clone()
        } else {
            p[unit.dir.len() + 1..].to_string()
        }
    };
    let python_pkg = single_python_package(unit, files, &rel).or_else(|| java_base_path(unit, files, &rel));
    unit.files
        .iter()
        .map(|&fi| module_key(files[fi].language, &rel(fi), python_pkg.as_deref()))
        .collect::<BTreeSet<_>>()
        .len()
}

pub(crate) fn component_view(unit: &Unit, files: &[FileRec], infra: &[(InfraKind, BTreeSet<usize>)]) -> ComponentView {
    let rel = |fi: usize| -> String {
        let p = &files[fi].path;
        if unit.dir.is_empty() {
            p.clone()
        } else {
            p[unit.dir.len() + 1..].to_string()
        }
    };
    let python_pkg = single_python_package(unit, files, &rel).or_else(|| java_base_path(unit, files, &rel));

    let mut key_of: HashMap<usize, String> = HashMap::new();
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for &fi in &unit.files {
        let key = module_key(files[fi].language, &rel(fi), python_pkg.as_deref());
        key_of.insert(fi, key.clone());
        groups.entry(key).or_default().push(fi);
    }
    let path_index: HashMap<String, usize> = unit.files.iter().map(|&fi| (rel(fi), fi)).collect();
    let go_module =
        unit.manifest.as_ref().filter(|m| m.kind == crate::manifest::ManifestKind::GoMod).and_then(|m| m.name.clone());

    // Application events: which module declares, publishes and handles each type.
    let mut listened_in: BTreeMap<&str, BTreeSet<&String>> = BTreeMap::new();
    for &fi in &unit.files {
        for ev in files[fi].facts.events.iter().filter(|e| e.kind == "listen") {
            listened_in.entry(ev.event_type.as_str()).or_default().insert(&key_of[&fi]);
        }
    }
    let mut deps: BTreeMap<(String, String), ModuleDependency> = BTreeMap::new();
    for &fi in &unit.files {
        let f = &files[fi];
        let from = &key_of[&fi];
        for ev in f.facts.events.iter().filter(|e| e.kind == "publish") {
            for to in listened_in.get(ev.event_type.as_str()).into_iter().flatten().filter(|m| **m != from) {
                let d = deps.entry((from.clone(), (*to).clone())).or_insert_with(|| ModuleDependency {
                    source: module_id(&unit.id, from),
                    target: module_id(&unit.id, to),
                    weight: 0,
                    evidence: EvidenceRef {
                        file_path: f.path.clone(),
                        start_line: ev.line,
                        end_line: ev.line,
                        symbol_name: None,
                        note: Some(format!("`{}` publishes {}", ev.method, ev.event_type)),
                    },
                    events: vec![],
                });
                d.weight += 1;
                if !d.events.contains(&ev.event_type) {
                    d.events.push(ev.event_type.clone());
                }
            }
        }
        // A listener imports the event type it handles; that import is the
        // subscription, already drawn as publisher → listener.
        let handled: BTreeSet<&str> =
            f.facts.events.iter().filter(|e| e.kind == "listen").map(|e| e.event_type.as_str()).collect();
        for imp in &f.facts.imports {
            if f.language.is_jvm() && imp.specifier.rsplit('.').next().is_some_and(|n| handled.contains(n)) {
                continue;
            }
            // `mod x;` only declares the module tree. It's a dependency when this
            // file also reaches into `x::` (calls or `use x::…`), not otherwise.
            if let Some(m) = imp.specifier.strip_prefix("mod ") {
                let prefix = format!("{m}::");
                let used = f.facts.calls.iter().any(|c| c.callee.starts_with(&prefix))
                    || f.facts.imports.iter().any(|i| {
                        i.specifier.starts_with(&prefix) || i.specifier.starts_with(&format!("self::{prefix}"))
                    });
                if !used {
                    continue;
                }
            }
            let Some(target_fi) = resolve_import(
                f.language,
                &rel(fi),
                &imp.specifier,
                &path_index,
                go_module.as_deref(),
                &groups,
                python_pkg.as_deref(),
            ) else {
                continue;
            };
            let to = match target_fi {
                Resolved::File(t) => key_of[&t].clone(),
                Resolved::Module(k) => k,
            };
            if &to == from {
                continue;
            }
            let d = deps.entry((from.clone(), to.clone())).or_insert_with(|| ModuleDependency {
                source: module_id(&unit.id, from),
                target: module_id(&unit.id, &to),
                weight: 0,
                evidence: EvidenceRef {
                    file_path: f.path.clone(),
                    start_line: imp.line,
                    end_line: imp.line,
                    symbol_name: None,
                    note: Some(format!("imports `{}`", imp.specifier)),
                },
                events: vec![],
            });
            d.weight += 1;
        }
    }

    let mut infra_usage = Vec::new();
    let mut infra_by_module: BTreeMap<String, BTreeSet<InfraKind>> = BTreeMap::new();
    for (kind, importing) in infra {
        let mut seen = BTreeSet::new();
        for fi in importing {
            let Some(key) = key_of.get(fi) else { continue };
            infra_by_module.entry(key.clone()).or_default().insert(*kind);
            if seen.insert(key.clone()) {
                let f = &files[*fi];
                let line = f
                    .facts
                    .imports
                    .iter()
                    .find(|i| {
                        crate::catalog::infra_for_package(crate::catalog::import_package(&i.specifier)) == Some(*kind)
                    })
                    .map(|i| i.line)
                    .unwrap_or(1);
                infra_usage.push((
                    module_id(&unit.id, key),
                    *kind,
                    EvidenceRef {
                        file_path: f.path.clone(),
                        start_line: line,
                        end_line: line,
                        symbol_name: None,
                        note: Some(format!("imports {}", kind.label())),
                    },
                ));
            }
        }
    }

    let modules = groups
        .iter()
        .map(|(key, fis)| {
            let mut fis = fis.clone();
            fis.sort_by_key(|fi| files[*fi].path.clone());
            let is_entry = fis.iter().any(|fi| !files[*fi].facts.entry_points.is_empty());
            let evidence = module_evidence(&fis, files);
            let doc = fis
                .iter()
                .find_map(|fi| files[*fi].facts.module_doc.clone())
                .or_else(|| {
                    // Behaviour (functions, classes) describes a module better than its data types.
                    fis.iter()
                        .flat_map(|fi| files[*fi].facts.symbols.iter())
                        .filter(|s| s.exported && s.doc.is_some())
                        .min_by_key(|s| match s.kind {
                            SymbolKind::Function | SymbolKind::Class | SymbolKind::Struct | SymbolKind::Trait => 0,
                            SymbolKind::Method => 1,
                            _ => 2,
                        })
                        .and_then(|s| s.doc.clone())
                })
                .map(|d| d.lines().next().unwrap_or("").to_string());
            ModuleSummary {
                id: module_id(&unit.id, key),
                name: module_name(key),
                path: common_path(&fis.iter().map(|fi| files[*fi].path.as_str()).collect::<Vec<_>>()),
                files: fis.iter().map(|fi| files[*fi].path.clone()).collect(),
                symbols: fis.iter().map(|fi| files[*fi].facts.symbols.len()).sum(),
                doc,
                evidence,
                is_entry,
                infra: infra_by_module.get(key).map(|s| s.iter().copied().collect()).unwrap_or_default(),
                public_types: vec![],
                publishes: vec![],
                consumes: vec![],
                internal_packages: vec![],
            }
        })
        .collect();

    ComponentView {
        unit: unit.id.clone(),
        modules,
        dependencies: deps.into_values().collect(),
        infra_usage,
        violations: vec![],
        boundaries_declared: false,
    }
}

/// A single file's path, or the deepest directory shared by several.
fn common_path(paths: &[&str]) -> String {
    match paths {
        [] => String::new(),
        [one] => one.to_string(),
        [first, rest @ ..] => {
            let mut parts: Vec<&str> = first.split('/').collect();
            parts.pop();
            for p in rest {
                let other: Vec<&str> = p.split('/').collect();
                let n = parts.iter().zip(&other).take_while(|(a, b)| a == b).count();
                parts.truncate(n);
            }
            format!("{}/", parts.join("/"))
        }
    }
}

pub fn module_id(unit: &str, key: &str) -> String {
    format!("{unit}.{}", slug(key))
}

fn module_name(key: &str) -> String {
    let last = key.rsplit('/').next().unwrap_or(key);
    let last = last.trim_start_matches("__").trim_end_matches("__");
    crate::scan::display_name(last)
}

fn module_evidence(fis: &[usize], files: &[FileRec]) -> Option<EvidenceRef> {
    for fi in fis {
        if let Some(ep) = files[*fi].facts.entry_points.first() {
            return Some(EvidenceRef {
                file_path: files[*fi].path.clone(),
                start_line: ep.start_line,
                end_line: ep.end_line,
                symbol_name: (!ep.symbol.starts_with('<')).then(|| ep.symbol.clone()),
                note: Some(ep.reason.clone()),
            });
        }
    }
    let best = fis
        .iter()
        .flat_map(|fi| files[*fi].facts.symbols.iter().map(move |s| (*fi, s)))
        .filter(|(_, s)| s.kind != crate::extract::SymbolKind::Impl && s.kind != crate::extract::SymbolKind::Module)
        .max_by_key(|(_, s)| (s.exported, s.doc.is_some(), std::cmp::Reverse(s.start_line)));
    match best {
        Some((fi, s)) => Some(EvidenceRef {
            file_path: files[fi].path.clone(),
            start_line: s.start_line,
            end_line: s.end_line,
            symbol_name: Some(s.name.clone()),
            note: s.doc.as_ref().map(|d| d.lines().next().unwrap_or("").to_string()),
        }),
        // Empty files (e.g. bare `__init__.py`) have no line to point at.
        None => fis.iter().map(|fi| &files[*fi]).find(|f| f.facts.line_count > 0).map(|f| EvidenceRef {
            file_path: f.path.clone(),
            start_line: 1,
            end_line: f.facts.line_count.min(20),
            symbol_name: None,
            note: None,
        }),
    }
}

fn single_python_package(unit: &Unit, files: &[FileRec], rel: &dyn Fn(usize) -> String) -> Option<String> {
    let pkgs: BTreeSet<String> = unit
        .files
        .iter()
        .filter(|&&fi| files[fi].language == Language::Python)
        .map(|&fi| rel(fi))
        .filter(|p| p.ends_with("/__init__.py") && p.matches('/').count() == 1)
        .map(|p| p.split('/').next().unwrap().to_string())
        .collect();
    (pkgs.len() == 1).then(|| pkgs.into_iter().next().unwrap())
}

/// Directory path (`com/acme/shop`) of a Java unit's base package: the
/// application class's package when there is one, else the deepest package
/// every source shares. Modules are the packages directly below it — the
/// layout Spring Modulith and package-by-feature codebases use.
fn java_base_path(unit: &Unit, files: &[FileRec], rel: &dyn Fn(usize) -> String) -> Option<String> {
    let java: Vec<usize> = unit.files.iter().copied().filter(|&fi| files[fi].language.is_jvm()).collect();
    if java.is_empty() {
        return None;
    }
    let pkg_dir = |fi: usize| -> Option<String> {
        let r = rel(fi);
        let inner = java_source_path(&r);
        Path::new(inner).parent().map(|p| p.to_string_lossy().replace('\\', "/"))
    };
    let app = java
        .iter()
        .copied()
        .find(|&fi| files[fi].facts.entry_points.iter().any(|e| e.reason.contains("application object")));
    if let Some(dir) = app.and_then(pkg_dir) {
        return Some(dir);
    }
    let dirs: Vec<String> = java.iter().filter_map(|&fi| pkg_dir(fi)).collect();
    let first: Vec<&str> = dirs.first()?.split('/').collect();
    let mut common = first.len();
    for d in &dirs[1..] {
        let segs: Vec<&str> = d.split('/').collect();
        common = common.min(first.iter().zip(&segs).take_while(|(a, b)| a == b).count());
    }
    Some(first[..common].join("/"))
}

/// Path below the source root (`src/main/java/`, or `src/` / `java/` layouts).
fn java_source_path(rel: &str) -> &str {
    for marker in ["src/main/java/", "src/main/kotlin/", "src/java/", "src/kotlin/", "java/", "kotlin/", "src/"] {
        if let Some(i) = rel.find(marker) {
            return &rel[i + marker.len()..];
        }
    }
    rel
}

/// The grouping key a file belongs to within its unit.
pub fn module_key(lang: Language, rel: &str, python_pkg: Option<&str>) -> String {
    let path = Path::new(rel);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let dir = path.parent().map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
    match lang {
        Language::Go => {
            if dir.is_empty() {
                "main".into()
            } else {
                dir
            }
        }
        Language::Rust => {
            let inner = rel.strip_prefix("src/").unwrap_or(rel);
            match inner.split_once('/') {
                Some((first, _)) => first.to_string(),
                None => stem.to_string(),
            }
        }
        Language::TypeScript | Language::JavaScript => {
            let inner = rel.strip_prefix("src/").unwrap_or(rel);
            match inner.split_once('/') {
                Some((first, _)) => first.to_string(),
                None => stem.to_string(),
            }
        }
        Language::Java | Language::Kotlin => {
            let inner = java_source_path(rel);
            let below = match python_pkg.filter(|b| !b.is_empty()) {
                Some(base) => inner.strip_prefix(&format!("{base}/")),
                None => Some(inner),
            };
            match below {
                Some(b) => match b.split_once('/') {
                    Some((first, _)) => first.to_string(),
                    // Classes in the base package itself: the application shell.
                    None => "application".to_string(),
                },
                // Outside the base package (shared code in a sibling package).
                None => Path::new(inner).parent().map(|p| p.to_string_lossy().replace('/', ".")).unwrap_or_default(),
            }
        }
        Language::CSharp | Language::Ruby | Language::Php | Language::Elixir | Language::Other => {
            if dir.is_empty() {
                stem.to_string()
            } else {
                dir
            }
        }
        Language::Python => {
            let inner = match python_pkg {
                Some(pkg) => rel.strip_prefix(&format!("{pkg}/")).unwrap_or(rel),
                None => rel.strip_prefix("src/").unwrap_or(rel),
            };
            match inner.split_once('/') {
                Some((first, _)) => first.to_string(),
                None if stem == "__init__" => python_pkg.unwrap_or("package").to_string(),
                None => stem.to_string(),
            }
        }
    }
}

enum Resolved {
    File(usize),
    Module(String),
}

fn resolve_import(
    lang: Language,
    from: &str,
    spec: &str,
    paths: &HashMap<String, usize>,
    go_module: Option<&str>,
    groups: &BTreeMap<String, Vec<usize>>,
    root_pkg: Option<&str>,
) -> Option<Resolved> {
    let dir = Path::new(from).parent().unwrap_or(Path::new(""));
    let lookup = |candidates: &[String]| {
        candidates.iter().find_map(|c| {
            let norm = crate::manifest::normalize(Path::new(c)).to_string_lossy().replace('\\', "/");
            paths.get(&norm).copied()
        })
    };
    match lang {
        Language::TypeScript | Language::JavaScript => {
            if !spec.starts_with('.') {
                return None;
            }
            let base = dir.join(spec).to_string_lossy().replace('\\', "/");
            let base = base.trim_end_matches(".js").trim_end_matches(".jsx").to_string();
            let exts = ["ts", "tsx", "js", "jsx", "mjs"];
            let mut c: Vec<String> = exts.iter().map(|e| format!("{base}.{e}")).collect();
            c.extend(exts.iter().map(|e| format!("{base}/index.{e}")));
            c.push(base.clone());
            lookup(&c).map(Resolved::File)
        }
        Language::Python => {
            let (base_dir, rest) = if spec.starts_with('.') {
                let dots = spec.chars().take_while(|c| *c == '.').count();
                let mut d = dir.to_path_buf();
                for _ in 1..dots {
                    d.pop();
                }
                (d, &spec[dots..])
            } else {
                (std::path::PathBuf::new(), spec)
            };
            let rest = rest.replace('.', "/");
            let b = base_dir.join(&rest).to_string_lossy().replace('\\', "/");
            let mut c = vec![format!("{b}.py"), format!("{b}/__init__.py")];
            if !spec.starts_with('.') {
                c.push(format!("src/{b}.py"));
                c.push(format!("src/{b}/__init__.py"));
            }
            // `from . import create_app` names a symbol, not a submodule:
            // fall back to the package (or module) that defines it.
            if let Some((parent, _)) = b.rsplit_once('/') {
                c.push(format!("{parent}/__init__.py"));
                c.push(format!("{parent}.py"));
            } else if spec.starts_with('.') {
                c.push("__init__.py".into());
            }
            lookup(&c).map(Resolved::File)
        }
        Language::Go => {
            let module = go_module?;
            let sub = spec.strip_prefix(module)?.trim_start_matches('/');
            groups.contains_key(sub).then(|| Resolved::Module(sub.to_string()))
        }
        Language::Java | Language::Kotlin => {
            // `com.acme.shop.orders.OrderService` / `com.acme.shop.orders.*` → module `orders`.
            let base = root_pkg?.replace('/', ".");
            let rest = spec.strip_prefix(&format!("{base}."))?;
            let first = rest.split('.').next()?;
            let key = if rest.contains('.') { first.to_string() } else { "application".to_string() };
            groups.contains_key(&key).then_some(Resolved::Module(key))
        }
        Language::CSharp | Language::Ruby | Language::Php | Language::Elixir | Language::Other => None,
        Language::Rust => {
            if let Some(m) = spec.strip_prefix("mod ") {
                let stem = Path::new(from).file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let owner = if matches!(stem, "main" | "lib" | "mod") { dir.to_path_buf() } else { dir.join(stem) };
                let o = owner.to_string_lossy().replace('\\', "/");
                let c = [format!("{o}/{m}.rs"), format!("{o}/{m}/mod.rs")];
                return lookup(&c).map(Resolved::File);
            }
            let rest = spec.strip_prefix("crate::")?;
            let first: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            groups.contains_key(&first).then_some(Resolved::Module(first))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_keys_per_language() {
        assert_eq!(module_key(Language::Rust, "src/audit.rs", None), "audit");
        assert_eq!(module_key(Language::Rust, "src/net/mod.rs", None), "net");
        assert_eq!(module_key(Language::Go, "internal/charge/charge.go", None), "internal/charge");
        assert_eq!(module_key(Language::Go, "main.go", None), "main");
        assert_eq!(module_key(Language::TypeScript, "src/routes/checkout.ts", None), "routes");
        assert_eq!(module_key(Language::TypeScript, "src/server.ts", None), "server");
        assert_eq!(module_key(Language::Python, "fulfillment/consumer.py", Some("fulfillment")), "consumer");
        assert_eq!(module_key(Language::Python, "fulfillment/__init__.py", Some("fulfillment")), "fulfillment");
        let base = Some("com/acme/shop");
        assert_eq!(
            module_key(Language::Java, "src/main/java/com/acme/shop/orders/web/OrderController.java", base),
            "orders"
        );
        assert_eq!(module_key(Language::Java, "src/main/java/com/acme/shop/ShopApplication.java", base), "application");
    }

    #[test]
    fn module_names_are_readable() {
        assert_eq!(module_name("internal/httpapi"), "Httpapi");
        assert_eq!(module_name("__main__"), "Main");
        assert_eq!(module_id("payments", "internal/charge"), "payments.internal-charge");
        assert_eq!(common_path(&["a/src/routes/x.ts", "a/src/routes/y.ts"]), "a/src/routes/");
        assert_eq!(common_path(&["a/src/db.ts"]), "a/src/db.ts");
    }
}
