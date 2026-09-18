//! Per-file extraction over tree-sitter syntax trees. One linear walk per file
//! collects symbols, imports, string literals and call sites; everything
//! downstream (C4 decomposition, relationships, evidence) is built from these
//! facts rather than from regexes over raw text.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Parser};

use crate::lang::{Grammar, Language};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Trait,
    Interface,
    TypeAlias,
    Impl,
    Module,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub start_line: u32,
    pub end_line: u32,
    pub exported: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Import {
    pub specifier: String,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StringLit {
    pub value: String,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Call {
    /// Last path segment of the callee (`producer.send` → `send`).
    pub name: String,
    /// Full callee text, truncated.
    pub callee: String,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EntryPoint {
    pub symbol: String,
    pub start_line: u32,
    pub end_line: u32,
    pub reason: String,
}

/// A Java annotation (`@GetMapping("/x")`) and the declaration it annotates.
/// Frameworks on the JVM are declared this way, so routes, entities, clients
/// and listeners are all read from these.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    /// Simple name (`GetMapping`), without package qualification.
    pub name: String,
    /// Raw argument list without the parentheses (`value = "/x", method = GET`), truncated.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub arguments: String,
    pub line: u32,
    /// `class`, `interface`, `enum`, `record`, `annotation`, `method`, `constructor`, `field`, `parameter`.
    pub target_kind: String,
    /// Name of the annotated declaration (field/parameter name for those).
    pub target: String,
    /// Enclosing type for members and parameters; method name is `target` for methods.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Lines of the annotated declaration.
    pub target_start: u32,
    pub target_end: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileFacts {
    pub symbols: Vec<Symbol>,
    pub imports: Vec<Import>,
    pub strings: Vec<StringLit>,
    pub calls: Vec<Call>,
    pub entry_points: Vec<EntryPoint>,
    /// Module-level doc (Rust `//!`, Python module docstring, leading JSDoc).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_doc: Option<String>,
    pub line_count: u32,
    pub has_syntax_errors: bool,
    /// Java package (`com.acme.orders`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<Annotation>,
    /// In-process application events (Spring `publishEvent` / `@EventListener`,
    /// Spring Modulith `@ApplicationModuleListener`, Micronaut events).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<EventFact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventFact {
    /// `publish` or `listen`.
    pub kind: String,
    /// Simple type name of the event (`OrderCompleted`).
    pub event_type: String,
    pub line: u32,
    /// Method that publishes or handles it.
    pub method: String,
}

/// Static helpers that read like a type but never name an event
/// (`Mockito.any()`, `Objects.requireNonNull(e)`).
const NOT_EVENT_TYPES: &[&str] = &[
    "Mockito",
    "ArgumentMatchers",
    "Objects",
    "Optional",
    "List",
    "Set",
    "Map",
    "Arrays",
    "Collections",
    "String",
    "Stream",
];

const LISTENER_ANNOTATIONS: &[&str] =
    &["EventListener", "ApplicationModuleListener", "TransactionalEventListener", "ApplicationEventListener"];

const MAX_ANNOTATION_ARGS: usize = 400;

const MAX_STRING: usize = 240;

pub fn extract(grammar: Grammar, language: Language, file_name: &str, src: &str) -> FileFacts {
    let mut parser = Parser::new();
    parser.set_language(&grammar.ts_language()).expect("bundled grammar is ABI compatible");
    let Some(tree) = parser.parse(src, None) else {
        return FileFacts { line_count: src.lines().count() as u32, ..Default::default() };
    };
    let root = tree.root_node();
    let mut ex = Extractor { src: src.as_bytes(), language, file_name, facts: FileFacts::default() };
    ex.facts.line_count = src.lines().count() as u32;
    ex.facts.has_syntax_errors = root.has_error();
    ex.facts.module_doc = ex.module_doc(root);
    ex.walk(root, 0);
    ex.facts
}

struct Extractor<'a> {
    src: &'a [u8],
    language: Language,
    file_name: &'a str,
    facts: FileFacts,
}

fn line(n: Node) -> u32 {
    n.start_position().row as u32 + 1
}

fn end_line(n: Node) -> u32 {
    let p = n.end_position();
    // A node ending at column 0 really ends on the previous line.
    if p.column == 0 && p.row > n.start_position().row {
        p.row as u32
    } else {
        p.row as u32 + 1
    }
}

impl<'a> Extractor<'a> {
    fn text(&self, n: Node) -> &'a str {
        n.utf8_text(self.src).unwrap_or("")
    }

    fn field_text(&self, n: Node, field: &str) -> Option<String> {
        n.child_by_field_name(field).map(|c| self.text(c).to_string())
    }

    fn walk(&mut self, node: Node, depth: usize) {
        if depth > 400 {
            return;
        }
        match self.language {
            Language::Rust => self.visit_rust(node),
            Language::TypeScript | Language::JavaScript => self.visit_ts(node),
            Language::Go => self.visit_go(node),
            Language::Python => self.visit_python(node),
            Language::Java => self.visit_java(node),
            Language::Kotlin => self.visit_kotlin(node),
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk(child, depth + 1);
        }
    }

    fn push_symbol(&mut self, node: Node, name: String, kind: SymbolKind, exported: bool, doc: Option<String>) {
        if name.is_empty() {
            return;
        }
        self.facts.symbols.push(Symbol { name, kind, start_line: line(node), end_line: end_line(node), exported, doc });
    }

    fn push_string(&mut self, node: Node, raw: &str) {
        let v = unquote(raw);
        if v.trim().is_empty() || v.len() > MAX_STRING * 4 {
            return;
        }
        let value: String = v.chars().take(MAX_STRING).collect();
        self.facts.strings.push(StringLit { value, line: line(node) });
    }

    fn push_call(&mut self, node: Node, callee: Node) {
        let full = self.text(callee);
        let name = full.rsplit(['.', ':', '>']).next().unwrap_or(full).trim_end_matches('!').trim().to_string();
        if name.is_empty() || name.len() > 64 {
            return;
        }
        self.facts.calls.push(Call { name, callee: full.chars().take(80).collect(), line: line(node) });
    }

    /// Contiguous comment siblings immediately above `node`.
    fn leading_comments(&self, node: Node, prefixes: &[&str]) -> Option<String> {
        let mut lines = Vec::new();
        let mut cur = node.prev_sibling();
        let mut expected_row = node.start_position().row;
        while let Some(c) = cur {
            // Attributes and decorators sit between a doc comment and its item.
            if matches!(c.kind(), "attribute_item" | "decorator" | "annotated_expression") {
                expected_row = c.start_position().row;
                cur = c.prev_sibling();
                continue;
            }
            if !c.kind().contains("comment") || c.end_position().row + 1 < expected_row {
                break;
            }
            let t = self.text(c).trim();
            let Some(p) = prefixes.iter().find(|p| t.starts_with(**p)) else { break };
            lines.push(clean_comment(t, p));
            expected_row = c.start_position().row;
            cur = c.prev_sibling();
        }
        lines.reverse();
        let doc = lines.join("\n").trim().to_string();
        (!doc.is_empty()).then_some(doc)
    }

    fn module_doc(&self, root: Node) -> Option<String> {
        let mut cursor = root.walk();
        let children: Vec<Node> = root.children(&mut cursor).collect();
        match self.language {
            Language::Rust => {
                let lines: Vec<String> = children
                    .iter()
                    .take_while(|c| c.kind().contains("comment"))
                    .map(|c| self.text(*c).trim())
                    .filter(|t| t.starts_with("//!"))
                    .map(|t| clean_comment(t, "//!"))
                    .collect();
                non_empty(lines.join("\n"))
            }
            Language::Python => children
                .first()
                .filter(|c| c.kind() == "expression_statement")
                .and_then(|c| c.named_child(0))
                .filter(|s| s.kind() == "string")
                .map(|s| unquote(self.text(s)))
                .and_then(non_empty),
            Language::Go => children
                .iter()
                .position(|c| c.kind() == "package_clause")
                .and_then(|i| self.leading_comments(children[i], &["//"])),
            // A leading JSDoc is a file doc only when a blank line separates it
            // from the first statement; otherwise it documents that statement.
            // `package-info.java`: the package's Javadoc.
            Language::Java => children
                .iter()
                .position(|c| c.kind() == "package_declaration")
                .and_then(|i| self.leading_comments(children[i], &["/**"])),
            // A KDoc above the package header documents the file.
            Language::Kotlin => children
                .iter()
                .position(|c| c.kind() == "package_header")
                .and_then(|i| self.leading_comments(children[i], &["/**"])),
            Language::CSharp | Language::Ruby | Language::Php | Language::Elixir | Language::Other => None,
            Language::TypeScript | Language::JavaScript => children
                .first()
                .filter(|c| c.kind() == "comment" && self.text(**c).starts_with("/**"))
                .filter(|c| children.get(1).is_none_or(|next| next.start_position().row > c.end_position().row + 1))
                .map(|c| clean_comment(self.text(*c), "/**"))
                .and_then(non_empty),
        }
    }

    // ───────────────────────────── Rust ─────────────────────────────

    fn visit_rust(&mut self, n: Node) {
        let exported = || {
            let mut c = n.walk();
            let vis = n.children(&mut c).any(|ch| ch.kind() == "visibility_modifier");
            vis
        };
        let kind = match n.kind() {
            "function_item" => {
                let in_impl = n.parent().and_then(|p| p.parent()).is_some_and(|g| g.kind() == "impl_item");
                Some(if in_impl { SymbolKind::Method } else { SymbolKind::Function })
            }
            "struct_item" => Some(SymbolKind::Struct),
            "enum_item" => Some(SymbolKind::Enum),
            "trait_item" => Some(SymbolKind::Trait),
            "type_item" => Some(SymbolKind::TypeAlias),
            "mod_item" => Some(SymbolKind::Module),
            "impl_item" => Some(SymbolKind::Impl),
            _ => None,
        };
        if let Some(kind) = kind {
            let name = if kind == SymbolKind::Impl {
                let ty = self.field_text(n, "type").unwrap_or_default();
                match self.field_text(n, "trait") {
                    Some(tr) => format!("impl {tr} for {ty}"),
                    None => format!("impl {ty}"),
                }
            } else {
                self.field_text(n, "name").unwrap_or_default()
            };
            let doc = self.leading_comments(n, &["///", "/**"]);
            self.push_symbol(n, name.clone(), kind, exported(), doc);
            if kind == SymbolKind::Module && n.child_by_field_name("body").is_none() {
                self.facts.imports.push(Import { specifier: format!("mod {name}"), line: line(n) });
            }
            if kind == SymbolKind::Function && name == "main" {
                let is_bin = self.file_name.ends_with("main.rs") || self.file_name.contains("/bin/");
                if is_bin {
                    self.facts.entry_points.push(EntryPoint {
                        symbol: "main".into(),
                        start_line: line(n),
                        end_line: end_line(n),
                        reason: "binary `fn main`".into(),
                    });
                }
            }
        }
        match n.kind() {
            "use_declaration" => {
                if let Some(arg) = n.child_by_field_name("argument") {
                    let spec = self.text(arg).split_whitespace().collect::<String>();
                    self.facts.imports.push(Import { specifier: spec, line: line(n) });
                }
            }
            "string_literal" | "raw_string_literal" => {
                let t = self.text(n);
                self.push_string(n, t);
            }
            "call_expression" => {
                if let Some(f) = n.child_by_field_name("function") {
                    self.push_call(n, f);
                }
            }
            "macro_invocation" => {
                if let Some(m) = n.child_by_field_name("macro") {
                    self.push_call(n, m);
                }
            }
            _ => {}
        }
    }

    // ────────────────────────── TypeScript / JS ──────────────────────────

    fn visit_ts(&mut self, n: Node) {
        let exported = |n: Node| n.parent().is_some_and(|p| p.kind() == "export_statement");
        let kind = match n.kind() {
            "function_declaration" | "generator_function_declaration" => Some(SymbolKind::Function),
            "class_declaration" | "abstract_class_declaration" => Some(SymbolKind::Class),
            "interface_declaration" => Some(SymbolKind::Interface),
            "type_alias_declaration" => Some(SymbolKind::TypeAlias),
            "enum_declaration" => Some(SymbolKind::Enum),
            "method_definition" => Some(SymbolKind::Method),
            _ => None,
        };
        if let Some(kind) = kind {
            let name = self.field_text(n, "name").unwrap_or_default();
            let doc = self.leading_comments(export_wrapper(n), &["/**"]);
            self.push_symbol(n, name, kind, exported(n), doc);
        }
        match n.kind() {
            // const handler = async () => {}  /  const x = function() {}
            "variable_declarator" => {
                let is_fn = n
                    .child_by_field_name("value")
                    .is_some_and(|v| matches!(v.kind(), "arrow_function" | "function_expression" | "function"));
                if is_fn {
                    let decl = n.parent().unwrap_or(n);
                    let name = self.field_text(n, "name").unwrap_or_default();
                    let doc = self.leading_comments(export_wrapper(decl), &["/**"]);
                    self.push_symbol(decl, name, SymbolKind::Function, exported(decl), doc);
                }
            }
            "import_statement" | "export_statement" => {
                if let Some(s) = n.child_by_field_name("source") {
                    self.facts.imports.push(Import { specifier: unquote(self.text(s)), line: line(n) });
                }
            }
            "string" => {
                let t = self.text(n);
                if n.parent().is_some_and(|p| matches!(p.kind(), "import_statement" | "export_statement")) {
                    return;
                }
                self.push_string(n, t);
            }
            "template_string" => {
                let t = self.text(n);
                self.push_string(n, t);
            }
            // process.env.PAYMENTS_URL → the env var name is recorded like a
            // string literal, matching os.Getenv("X") / os.environ["X"].
            "member_expression" => {
                let t = self.text(n);
                if let Some(var) = t.strip_prefix("process.env.").or_else(|| t.strip_prefix("import.meta.env.")) {
                    if var.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                        self.facts.strings.push(StringLit { value: var.to_string(), line: line(n) });
                    }
                }
            }
            "call_expression" => {
                let Some(f) = n.child_by_field_name("function") else { return };
                let callee = self.text(f);
                if matches!(callee, "require" | "import") {
                    if let Some(arg) = n.child_by_field_name("arguments").and_then(|a| a.named_child(0)) {
                        if arg.kind() == "string" {
                            self.facts.imports.push(Import { specifier: unquote(self.text(arg)), line: line(n) });
                        }
                    }
                }
                self.push_call(n, f);
                let last = callee.rsplit('.').next().unwrap_or(callee);
                if last == "listen" && !callee.contains("addEventListener") {
                    let (s, e, sym) = self.enclosing_function(n);
                    self.facts.entry_points.push(EntryPoint {
                        symbol: sym,
                        start_line: s,
                        end_line: e,
                        reason: format!("server bootstrap `{callee}()`"),
                    });
                } else if callee.ends_with("createRoot") || callee == "ReactDOM.render" {
                    self.facts.entry_points.push(EntryPoint {
                        symbol: "createRoot".into(),
                        start_line: line(n),
                        end_line: end_line(n.parent().unwrap_or(n)),
                        reason: "client mount point".into(),
                    });
                }
            }
            _ => {}
        }
    }

    // ───────────────────────────── Java ─────────────────────────────

    fn visit_java(&mut self, n: Node) {
        let kind = match n.kind() {
            "class_declaration" => Some((SymbolKind::Class, "class")),
            "interface_declaration" => Some((SymbolKind::Interface, "interface")),
            "enum_declaration" => Some((SymbolKind::Enum, "enum")),
            "record_declaration" => Some((SymbolKind::Class, "record")),
            "annotation_type_declaration" => Some((SymbolKind::Interface, "annotation")),
            "method_declaration" => Some((SymbolKind::Method, "method")),
            "constructor_declaration" => Some((SymbolKind::Method, "constructor")),
            _ => None,
        };
        if let Some((sym_kind, target_kind)) = kind {
            let name = self.field_text(n, "name").unwrap_or_default();
            let modifiers = self.java_modifiers(n);
            let exported = modifiers.is_some_and(|m| self.text(m).split_whitespace().any(|w| w == "public"))
                || n.parent().is_some_and(|p| p.kind() == "interface_body");
            let doc = self.leading_comments(n, &["/**"]);
            self.push_symbol(n, name.clone(), sym_kind, exported, doc);
            let owner = if target_kind == "method" || target_kind == "constructor" { self.java_owner(n) } else { None };
            self.java_annotations(n, target_kind, &name, owner.clone());
            if target_kind == "method" {
                if let Some(params) = n.child_by_field_name("parameters") {
                    let mut c = params.walk();
                    for p in params.named_children(&mut c).filter(|p| p.kind() == "formal_parameter") {
                        let pname = self.field_text(p, "name").unwrap_or_default();
                        self.java_annotations(p, "parameter", &pname, Some(name.clone()));
                    }
                }
                let is_static = modifiers.is_some_and(|m| self.text(m).split_whitespace().any(|w| w == "static"));
                let args_array = n.child_by_field_name("parameters").is_some_and(|p| {
                    let t = self.text(p);
                    t.contains("String") && (t.contains('[') || t.contains("..."))
                });
                // Event handlers: the annotated method's first parameter type is the event.
                let listens = modifiers.is_some_and(|m| {
                    let t = self.text(m);
                    LISTENER_ANNOTATIONS.iter().any(|a| t.contains(&format!("@{a}")))
                });
                if listens {
                    // `@TransactionalEventListener(classes = X.class)` names the event
                    // even when the method takes `Object`; otherwise the parameter type does.
                    let mut declared = modifiers.map(|m| self.java_listener_classes(m)).unwrap_or_default();
                    if declared.is_empty() {
                        let first = n.child_by_field_name("parameters").and_then(|p| {
                            let mut c = p.walk();
                            let param = p.named_children(&mut c).find(|x| x.kind() == "formal_parameter");
                            param
                        });
                        declared.extend(
                            first.and_then(|p| p.child_by_field_name("type")).map(|ty| simple_type(self.text(ty))),
                        );
                    }
                    for simple in declared.into_iter().filter(|t| !t.is_empty()) {
                        self.facts.events.push(EventFact {
                            kind: "listen".into(),
                            event_type: simple,
                            line: line(n),
                            method: name.clone(),
                        });
                    }
                }
                if name == "main" && is_static && args_array {
                    let class = self.java_owner(n).unwrap_or_default();
                    let class_node = self.java_owner_node(n);
                    let app = class_node.and_then(|c| self.java_modifiers(c)).map(|m| self.text(m)).and_then(|m| {
                        [
                            "SpringBootApplication",
                            "QuarkusMain",
                            "MicronautApplication",
                            "EnableEurekaServer",
                            "EnableConfigServer",
                        ]
                        .into_iter()
                        .find(|a| m.contains(&format!("@{a}")))
                    });
                    let body = self.text(n);
                    let runtime =
                        ["SpringApplication.run", "Micronaut.run", "Quarkus.run", "new SpringApplicationBuilder"]
                            .into_iter()
                            .find(|c| body.contains(c));
                    let reason = match (app, runtime) {
                        (Some(a), _) => format!("application object `@{a}` {class}"),
                        (None, Some(r)) => format!("application object `{r}` in {class}"),
                        (None, None) => format!("Java `main` in {class}"),
                    };
                    let target = class_node.unwrap_or(n);
                    self.facts.entry_points.push(EntryPoint {
                        symbol: if class.is_empty() { "main".into() } else { class },
                        start_line: line(target),
                        end_line: end_line(target),
                        reason,
                    });
                }
            }
        }
        match n.kind() {
            "field_declaration" => {
                let mut c = n.walk();
                let names: Vec<String> =
                    n.children_by_field_name("declarator", &mut c).filter_map(|d| self.field_text(d, "name")).collect();
                let owner = self.java_owner(n);
                for name in names {
                    self.java_annotations(n, "field", &name, owner.clone());
                }
            }
            "package_declaration" => {
                let mut c = n.walk();
                let id = n.named_children(&mut c).find(|c| c.kind().ends_with("identifier"));
                if let Some(id) = id {
                    self.facts.package = Some(self.text(id).to_string());
                }
            }
            "import_declaration" => {
                let spec: String = self
                    .text(n)
                    .trim_start_matches("import")
                    .trim()
                    .trim_start_matches("static ")
                    .trim_end_matches(';')
                    .split_whitespace()
                    .collect();
                self.facts.imports.push(Import { specifier: spec, line: line(n) });
            }
            "string_literal" | "text_block" => {
                let t = self.text(n);
                self.push_string(n, t);
            }
            "method_invocation" => {
                let name = self.field_text(n, "name").unwrap_or_default();
                if name.is_empty() || name.len() > 64 {
                    return;
                }
                let callee = match n.child_by_field_name("object") {
                    Some(o) => format!("{}.{name}", self.text(o)),
                    None => name.clone(),
                };
                // System.getenv("X") → the variable name, like os.Getenv / process.env.
                if callee == "System.getenv" {
                    if let Some(arg) = n.child_by_field_name("arguments").and_then(|a| a.named_child(0)) {
                        if arg.kind() == "string_literal" {
                            let v = unquote(self.text(arg));
                            self.facts.strings.push(StringLit { value: v, line: line(n) });
                        }
                    }
                }
                // `events.publishEvent(new OrderCompleted(id))` / `publisher.publishEvent(event)`.
                let publisher = name == "publishEvent"
                    || (name == "publish"
                        && n.child_by_field_name("object").is_some_and(|o| {
                            let t = self.text(o).to_lowercase();
                            t.contains("event") || t.contains("publisher")
                        }));
                if publisher {
                    let arg = n.child_by_field_name("arguments").and_then(|a| a.named_child(0));
                    if let Some(ty) = arg.and_then(|a| self.java_event_type(a, true)) {
                        let method = self.java_enclosing_method(n).unwrap_or_default();
                        self.facts.events.push(EventFact {
                            kind: "publish".into(),
                            event_type: ty,
                            line: line(n),
                            method,
                        });
                    }
                }
                self.facts.calls.push(Call { name, callee: callee.chars().take(80).collect(), line: line(n) });
            }
            _ => {}
        }
    }

    fn java_enclosing_method(&self, n: Node) -> Option<String> {
        let mut cur = n.parent();
        while let Some(p) = cur {
            if matches!(p.kind(), "method_declaration" | "constructor_declaration") {
                return self.field_text(p, "name");
            }
            cur = p.parent();
        }
        None
    }

    /// Event type named by a `publishEvent(…)` argument: `new X(…)`, a builder or
    /// static factory chain (`X.builder()….build()`, `X.from(…)`), or a variable
    /// whose type is declared in the same method.
    fn java_event_type(&self, arg: Node, resolve_variables: bool) -> Option<String> {
        match arg.kind() {
            "object_creation_expression" => arg.child_by_field_name("type").map(|t| simple_type(self.text(t))),
            "method_invocation" => {
                // The receiver chain roots at the type: `X.builder().a(1).build()` → `X`.
                let mut root = arg;
                while let Some(o) = root.child_by_field_name("object") {
                    root = o;
                }
                let t = self.text(root);
                let named_type = root.kind() == "identifier"
                    && t.starts_with(|c: char| c.is_ascii_uppercase())
                    && !NOT_EVENT_TYPES.contains(&t);
                named_type.then(|| simple_type(t))
            }
            "cast_expression" | "parenthesized_expression" => arg
                .named_child(arg.named_child_count().saturating_sub(1))
                .and_then(|c| self.java_event_type(c, resolve_variables)),
            "identifier" if resolve_variables => self.java_variable_type(arg),
            _ => None,
        }
    }

    /// Type of a local variable, from its declaration in the enclosing method
    /// (`X evt = …`, `var evt = new X(…)`, `evt = X.builder()…`).
    fn java_variable_type(&self, var: Node) -> Option<String> {
        let name = self.text(var);
        let mut scope = var.parent();
        while let Some(p) = scope {
            if matches!(p.kind(), "method_declaration" | "constructor_declaration" | "program") {
                break;
            }
            scope = p.parent();
        }
        let mut found = None;
        Self::walk_java_scope(scope?, &mut |node| {
            if found.is_some() {
                return;
            }
            match node.kind() {
                "local_variable_declaration" => {
                    let mut c = node.walk();
                    let declares = node
                        .children_by_field_name("declarator", &mut c)
                        .any(|d| d.child_by_field_name("name").is_some_and(|x| self.text(x) == name));
                    if !declares {
                        return;
                    }
                    let declared = node.child_by_field_name("type").map(|t| self.text(t));
                    match declared.filter(|t| *t != "var") {
                        Some(t) => found = Some(simple_type(t)),
                        // `var` keeps no type: read it off the initialiser.
                        None => {
                            let mut c = node.walk();
                            found = node
                                .children_by_field_name("declarator", &mut c)
                                .find_map(|d| d.child_by_field_name("value"))
                                .and_then(|v| self.java_event_type(v, false));
                        }
                    }
                }
                "assignment_expression" => {
                    let assigns = node.child_by_field_name("left").is_some_and(|l| self.text(l) == name);
                    if assigns {
                        found = node.child_by_field_name("right").and_then(|v| self.java_event_type(v, false));
                    }
                }
                _ => {}
            }
        });
        found
    }

    fn walk_java_scope(node: Node, visit: &mut dyn FnMut(Node)) {
        visit(node);
        let mut c = node.walk();
        for child in node.named_children(&mut c) {
            Self::walk_java_scope(child, visit);
        }
    }

    /// Event types listed as `classes = X.class` / `{A.class, B.class}` on a listener annotation.
    fn java_listener_classes(&self, modifiers: Node) -> Vec<String> {
        let mut c = modifiers.walk();
        modifiers
            .children(&mut c)
            .filter(|a| a.kind() == "annotation")
            .filter(|a| {
                a.child_by_field_name("name")
                    .map(|n| self.text(n).rsplit('.').next().unwrap_or_default().to_string())
                    .is_some_and(|n| LISTENER_ANNOTATIONS.contains(&n.as_str()))
            })
            .filter_map(|a| a.child_by_field_name("arguments"))
            .flat_map(|args| {
                let text = self.text(args);
                text.match_indices(".class")
                    .filter_map(|(i, _)| {
                        let before = &text[..i];
                        let start =
                            before.rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.')).map_or(0, |x| x + 1);
                        let ty = simple_type(&before[start..]);
                        (!ty.is_empty()).then_some(ty)
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn java_modifiers<'t>(&self, n: Node<'t>) -> Option<Node<'t>> {
        let mut c = n.walk();
        let m = n.children(&mut c).find(|ch| ch.kind() == "modifiers");
        m
    }

    fn java_owner_node<'t>(&self, n: Node<'t>) -> Option<Node<'t>> {
        let mut cur = n.parent();
        while let Some(p) = cur {
            if matches!(
                p.kind(),
                "class_declaration" | "interface_declaration" | "enum_declaration" | "record_declaration"
            ) {
                return Some(p);
            }
            cur = p.parent();
        }
        None
    }

    fn java_owner(&self, n: Node) -> Option<String> {
        self.java_owner_node(n).and_then(|p| self.field_text(p, "name"))
    }

    fn java_annotations(&mut self, decl: Node, target_kind: &str, target: &str, owner: Option<String>) {
        let Some(mods) = self.java_modifiers(decl) else { return };
        let mut c = mods.walk();
        let anns: Vec<Node> =
            mods.children(&mut c).filter(|a| matches!(a.kind(), "annotation" | "marker_annotation")).collect();
        for a in anns {
            let name = self.field_text(a, "name").unwrap_or_default();
            let name = name.rsplit('.').next().unwrap_or(&name).to_string();
            let arguments = a
                .child_by_field_name("arguments")
                .map(|x| {
                    let t = self.text(x);
                    let inner = t.strip_prefix('(').and_then(|t| t.strip_suffix(')')).unwrap_or(t);
                    inner.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_ANNOTATION_ARGS).collect()
                })
                .unwrap_or_default();
            self.facts.annotations.push(Annotation {
                name,
                arguments,
                line: line(a),
                target_kind: target_kind.to_string(),
                target: target.to_string(),
                owner: owner.clone(),
                target_start: line(decl),
                target_end: end_line(decl),
            });
        }
    }

    // ──────────────────────────── Kotlin ────────────────────────────

    fn visit_kotlin(&mut self, n: Node) {
        match n.kind() {
            "class_declaration" | "object_declaration" => self.kotlin_type(n),
            "function_declaration" => self.kotlin_function(n),
            "property_declaration" => {
                let owner = self.kotlin_owner(n);
                for name in self.kotlin_property_names(n) {
                    self.kotlin_annotations(n, "field", &name, owner.clone());
                }
            }
            // Constructor `val`/`var` parameters are properties of the class.
            "class_parameter" => {
                let name = self.kotlin_binding_name(n).unwrap_or_default();
                let owner = self.kotlin_owner(n);
                self.kotlin_annotations(n, "field", &name, owner);
            }
            "package_header" => {
                if let Some(id) = self.kotlin_child(n, "qualified_identifier") {
                    self.facts.package = Some(self.text(id).split_whitespace().collect());
                }
            }
            "import" => {
                let spec: String = self
                    .text(n)
                    .trim_start_matches("import")
                    .trim()
                    .split(" as ")
                    .next()
                    .unwrap_or_default()
                    .split_whitespace()
                    .collect();
                if !spec.is_empty() {
                    self.facts.imports.push(Import { specifier: spec, line: line(n) });
                }
            }
            "string_literal" => {
                let t = self.text(n);
                self.push_string(n, t);
            }
            "call_expression" => self.kotlin_call(n),
            _ => {}
        }
    }

    fn kotlin_child<'t>(&self, n: Node<'t>, kind: &str) -> Option<Node<'t>> {
        let mut c = n.walk();
        let found = n.children(&mut c).find(|ch| ch.kind() == kind);
        found
    }

    /// True when `n` has an anonymous token child with this text (`interface`, `class`).
    fn kotlin_has_token(&self, n: Node, token: &str) -> bool {
        let mut c = n.walk();
        let hit = n.children(&mut c).any(|ch| !ch.is_named() && self.text(ch) == token);
        hit
    }

    fn kotlin_modifier(&self, n: Node, word: &str) -> bool {
        self.kotlin_child(n, "modifiers").is_some_and(|m| self.text(m).split_whitespace().any(|w| w == word))
    }

    fn kotlin_type(&mut self, n: Node) {
        let name = self.field_text(n, "name").unwrap_or_default();
        let object = n.kind() == "object_declaration";
        let (sym_kind, target_kind) = if object {
            (SymbolKind::Class, "object")
        } else if self.kotlin_has_token(n, "interface") {
            (SymbolKind::Interface, "interface")
        } else if self.kotlin_modifier(n, "enum") {
            (SymbolKind::Enum, "enum")
        } else {
            (SymbolKind::Class, "class")
        };
        // Kotlin declarations are public unless a modifier says otherwise.
        let exported = !["private", "internal", "protected"].iter().any(|m| self.kotlin_modifier(n, m));
        let doc = self.leading_comments(n, &["/**"]);
        self.push_symbol(n, name.clone(), sym_kind, exported, doc);
        self.kotlin_annotations(n, target_kind, &name, None);
        if self.kotlin_modifier(n, "enum") {
            return;
        }
        // `@SpringBootApplication class App` with `fun main` elsewhere in the file.
        let app = self.kotlin_child(n, "modifiers").map(|m| self.text(m)).and_then(|m| {
            ["SpringBootApplication", "QuarkusMain", "MicronautApplication", "EnableEurekaServer", "EnableConfigServer"]
                .into_iter()
                .find(|a| m.contains(&format!("@{a}")))
        });
        if let Some(a) = app {
            self.facts.entry_points.push(EntryPoint {
                symbol: name.clone(),
                start_line: line(n),
                end_line: end_line(n),
                reason: format!("application object `@{a}` {name}"),
            });
        }
    }

    fn kotlin_function(&mut self, n: Node) {
        let name = self.field_text(n, "name").unwrap_or_default();
        let owner = self.kotlin_owner(n);
        let kind = if owner.is_some() { SymbolKind::Method } else { SymbolKind::Function };
        let exported = !["private", "internal", "protected"].iter().any(|m| self.kotlin_modifier(n, m));
        let doc = self.leading_comments(n, &["/**"]);
        self.push_symbol(n, name.clone(), kind, exported, doc);
        self.kotlin_annotations(n, "method", &name, owner.clone());
        // `function_value_parameters` lists `parameter_modifiers` before the parameter they apply to.
        if let Some(params) = self.kotlin_child(n, "function_value_parameters") {
            let mut c = params.walk();
            let children: Vec<Node> = params.named_children(&mut c).collect();
            for (i, p) in children.iter().enumerate() {
                if p.kind() != "parameter" {
                    continue;
                }
                let pname = self.kotlin_binding_name(*p).unwrap_or_default();
                let mods = (i > 0 && children[i - 1].kind() == "parameter_modifiers").then(|| children[i - 1]);
                if let Some(m) = mods {
                    self.kotlin_annotations_in(m, *p, "parameter", &pname, Some(name.clone()));
                }
            }
        }
        let listens = self
            .kotlin_child(n, "modifiers")
            .map(|m| self.text(m))
            .is_some_and(|m| LISTENER_ANNOTATIONS.iter().any(|a| m.contains(&format!("@{a}"))));
        if listens {
            if let Some(ty) = self.kotlin_first_param_type(n) {
                self.facts.events.push(EventFact {
                    kind: "listen".into(),
                    event_type: ty,
                    line: line(n),
                    method: name.clone(),
                });
            }
        }
        if name == "main" && owner.is_none() {
            let body = self.text(n);
            // `runApplication<App>(*args)` parses as comparisons, so it is matched textually.
            let runtime = ["runApplication<", "SpringApplication.run", "Micronaut.run", "embeddedServer("]
                .into_iter()
                .find(|c| body.contains(c));
            let reason = match runtime {
                Some(r) => format!("application object `{}` in {}", r.trim_end_matches(['<', '(']), self.file_name),
                None => format!("Kotlin `main` in {}", self.file_name),
            };
            self.facts.entry_points.push(EntryPoint {
                symbol: "main".into(),
                start_line: line(n),
                end_line: end_line(n),
                reason,
            });
        }
    }

    fn kotlin_call(&mut self, n: Node) {
        let Some(callee_node) = n.child(0) else { return };
        let full = self.text(callee_node);
        // The function of a trailing-lambda call is itself a call expression.
        if callee_node.kind() == "call_expression" {
            return;
        }
        let name = full.rsplit(['.', '?']).next().unwrap_or(full).trim().to_string();
        if name.is_empty() || name.len() > 64 {
            return;
        }
        if name == "System.getenv" || full.ends_with("getenv") {
            if let Some(arg) = self.kotlin_first_string_arg(n) {
                self.facts.strings.push(StringLit { value: arg, line: line(n) });
            }
        }
        let publisher = name == "publishEvent"
            || (name == "publish" && {
                let t = full.to_lowercase();
                t.contains("event") || t.contains("publisher")
            });
        if publisher {
            if let Some(ty) = self.kotlin_event_argument(n) {
                let method = self.kotlin_enclosing_function(n).unwrap_or_default();
                self.facts.events.push(EventFact { kind: "publish".into(), event_type: ty, line: line(n), method });
            }
        }
        self.facts.calls.push(Call { name, callee: full.chars().take(80).collect(), line: line(n) });
    }

    /// Type constructed in the first argument: `publishEvent(OrderCompleted(id))`.
    fn kotlin_event_argument(&self, n: Node) -> Option<String> {
        let args = self.kotlin_child(n, "value_arguments")?;
        let mut c = args.walk();
        let first = args.named_children(&mut c).find(|a| a.kind() == "value_argument")?;
        let inner = first.named_child(0)?;
        let ty = match inner.kind() {
            "call_expression" => self.text(inner.child(0)?).to_string(),
            _ => return None,
        };
        let simple = ty.split('<').next().unwrap_or(&ty).rsplit('.').next().unwrap_or(&ty).trim().to_string();
        // A constructor call, not `service.doThing(...)`.
        (!simple.is_empty() && simple.starts_with(|c: char| c.is_uppercase())).then_some(simple)
    }

    fn kotlin_first_string_arg(&self, n: Node) -> Option<String> {
        let args = self.kotlin_child(n, "value_arguments")?;
        let mut c = args.walk();
        let first = args.named_children(&mut c).find(|a| a.kind() == "value_argument")?;
        let lit = first.named_child(0).filter(|x| x.kind() == "string_literal")?;
        non_empty(unquote(self.text(lit)))
    }

    fn kotlin_first_param_type(&self, n: Node) -> Option<String> {
        let params = self.kotlin_child(n, "function_value_parameters")?;
        let mut c = params.walk();
        let p = params.named_children(&mut c).find(|x| x.kind() == "parameter")?;
        let mut pc = p.walk();
        let ty = p.named_children(&mut pc).find(|x| x.kind().ends_with("type"))?;
        let t = self.text(ty);
        let simple = t.split('<').next().unwrap_or(t).rsplit('.').next().unwrap_or(t).trim_end_matches('?').trim();
        non_empty(simple.to_string())
    }

    /// `name: Type` of a `parameter` / `class_parameter` / `variable_declaration`.
    fn kotlin_binding_name(&self, n: Node) -> Option<String> {
        let mut c = n.walk();
        let id = n.children(&mut c).find(|ch| ch.kind() == "identifier")?;
        non_empty(self.text(id).to_string())
    }

    fn kotlin_property_names(&self, n: Node) -> Vec<String> {
        let mut c = n.walk();
        n.named_children(&mut c)
            .filter(|ch| ch.kind() == "variable_declaration")
            .filter_map(|d| self.kotlin_binding_name(d))
            .collect()
    }

    fn kotlin_enclosing_function(&self, n: Node) -> Option<String> {
        let mut cur = n.parent();
        while let Some(p) = cur {
            if p.kind() == "function_declaration" {
                return self.field_text(p, "name");
            }
            cur = p.parent();
        }
        None
    }

    fn kotlin_owner_node<'t>(&self, n: Node<'t>) -> Option<Node<'t>> {
        let mut cur = n.parent();
        while let Some(p) = cur {
            if matches!(p.kind(), "class_declaration" | "object_declaration") {
                return Some(p);
            }
            cur = p.parent();
        }
        None
    }

    fn kotlin_owner(&self, n: Node) -> Option<String> {
        self.kotlin_owner_node(n).and_then(|p| self.field_text(p, "name"))
    }

    fn kotlin_annotations(&mut self, decl: Node, target_kind: &str, target: &str, owner: Option<String>) {
        match self.kotlin_child(decl, "modifiers") {
            Some(mods) => self.kotlin_annotations_in(mods, decl, target_kind, target, owner),
            // The grammar sometimes parses leading annotations as an expression
            // before the declaration instead of as its modifiers.
            None => {
                let mut cur = decl.prev_sibling();
                while let Some(prev) = cur.filter(|p| p.kind() == "annotated_expression") {
                    self.kotlin_detached_annotations(prev, decl, target_kind, target, owner.clone());
                    cur = prev.prev_sibling();
                }
            }
        }
    }

    /// `annotated_expression` nests one annotation per level; an argument list
    /// appears as a `parenthesized_expression` beside its annotation.
    fn kotlin_detached_annotations(
        &mut self,
        node: Node,
        decl: Node,
        target_kind: &str,
        target: &str,
        owner: Option<String>,
    ) {
        let mut c = node.walk();
        let children: Vec<Node> = node.named_children(&mut c).collect();
        for (i, ch) in children.iter().enumerate() {
            match ch.kind() {
                "annotation" => {
                    let args = children
                        .get(i + 1)
                        .filter(|nx| nx.kind() == "parenthesized_expression")
                        .map(|nx| self.text(*nx));
                    self.kotlin_push_annotation(*ch, args, decl, target_kind, target, owner.clone());
                }
                "annotated_expression" => {
                    self.kotlin_detached_annotations(*ch, decl, target_kind, target, owner.clone())
                }
                _ => {}
            }
        }
    }

    /// Annotations inside a `modifiers` / `parameter_modifiers` node, recorded
    /// against `decl`. Use-site targets (`@field:NotBlank`) keep the bare name.
    fn kotlin_annotations_in(
        &mut self,
        mods: Node,
        decl: Node,
        target_kind: &str,
        target: &str,
        owner: Option<String>,
    ) {
        let mut c = mods.walk();
        let anns: Vec<Node> = mods.children(&mut c).filter(|a| a.kind() == "annotation").collect();
        for a in anns {
            self.kotlin_push_annotation(a, None, decl, target_kind, target, owner.clone());
        }
    }

    /// Records one annotation; `args` overrides the arguments when they sit
    /// outside the annotation node (the detached form).
    fn kotlin_push_annotation(
        &mut self,
        a: Node,
        args: Option<&str>,
        decl: Node,
        target_kind: &str,
        target: &str,
        owner: Option<String>,
    ) {
        let invocation = self.kotlin_child(a, "constructor_invocation");
        let name_node = match invocation {
            Some(inv) => self.kotlin_child(inv, "user_type"),
            None => self.kotlin_child(a, "user_type"),
        };
        let Some(name_node) = name_node else { return };
        let raw = self.text(name_node);
        let name = raw.split('<').next().unwrap_or(raw).rsplit('.').next().unwrap_or(raw).trim().to_string();
        if name.is_empty() {
            return;
        }
        let clean = |t: &str| -> String {
            let inner = t.trim().strip_prefix('(').and_then(|t| t.strip_suffix(')')).unwrap_or(t);
            inner.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_ANNOTATION_ARGS).collect()
        };
        let arguments = match args {
            Some(t) => clean(t),
            None => invocation
                .and_then(|inv| self.kotlin_child(inv, "value_arguments"))
                .map(|x| clean(self.text(x)))
                .unwrap_or_default(),
        };
        self.facts.annotations.push(Annotation {
            name,
            arguments,
            line: line(a),
            target_kind: target_kind.to_string(),
            target: target.to_string(),
            owner,
            target_start: line(decl),
            target_end: end_line(decl),
        });
    }

    /// Range of the top-level statement or function holding `n`.
    fn enclosing_function(&self, n: Node) -> (u32, u32, String) {
        let mut cur = n;
        let mut best: Option<(Node, String)> = None;
        while let Some(p) = cur.parent() {
            match p.kind() {
                "function_declaration" | "method_definition" | "function_definition" | "function_item" => {
                    best = Some((p, self.field_text(p, "name").unwrap_or_default()));
                }
                "program" | "source_file" | "module" => {
                    return match best {
                        Some((b, name)) => (line(b), end_line(b), name),
                        None => (line(cur), end_line(cur), "<module>".into()),
                    };
                }
                _ => {}
            }
            cur = p;
        }
        (line(n), end_line(n), "<module>".into())
    }

    // ───────────────────────────── Go ─────────────────────────────

    fn visit_go(&mut self, n: Node) {
        let is_exported = |name: &str| name.chars().next().is_some_and(char::is_uppercase);
        match n.kind() {
            "function_declaration" | "method_declaration" => {
                let name = self.field_text(n, "name").unwrap_or_default();
                let kind = if n.kind() == "method_declaration" { SymbolKind::Method } else { SymbolKind::Function };
                let doc = self.leading_comments(n, &["//"]);
                let exported = is_exported(&name);
                if name == "main" && kind == SymbolKind::Function && self.go_package(n).as_deref() == Some("main") {
                    self.facts.entry_points.push(EntryPoint {
                        symbol: "main".into(),
                        start_line: line(n),
                        end_line: end_line(n),
                        reason: "`func main` in package main".into(),
                    });
                }
                self.push_symbol(n, name, kind, exported, doc);
            }
            "type_spec" => {
                let name = self.field_text(n, "name").unwrap_or_default();
                let kind = match n.child_by_field_name("type").map(|t| t.kind()) {
                    Some("struct_type") => SymbolKind::Struct,
                    Some("interface_type") => SymbolKind::Interface,
                    _ => SymbolKind::TypeAlias,
                };
                // Doc comments attach to the enclosing `type` declaration.
                let decl = n.parent().filter(|p| p.kind() == "type_declaration").unwrap_or(n);
                let doc = self.leading_comments(decl, &["//"]);
                let exported = is_exported(&name);
                self.push_symbol(decl, name, kind, exported, doc);
            }
            "import_spec" => {
                if let Some(p) = n.child_by_field_name("path") {
                    self.facts.imports.push(Import { specifier: unquote(self.text(p)), line: line(n) });
                }
            }
            "interpreted_string_literal" | "raw_string_literal" => {
                if n.parent().is_some_and(|p| p.kind() == "import_spec") {
                    return;
                }
                let t = self.text(n);
                self.push_string(n, t);
            }
            "call_expression" => {
                if let Some(f) = n.child_by_field_name("function") {
                    self.push_call(n, f);
                }
            }
            _ => {}
        }
    }

    fn go_package(&self, n: Node) -> Option<String> {
        let mut root = n;
        while let Some(p) = root.parent() {
            root = p;
        }
        let mut c = root.walk();
        let pkg = root.children(&mut c).find(|ch| ch.kind() == "package_clause")?;
        let name = pkg.named_child(0)?;
        Some(self.text(name).to_string())
    }

    // ─────────────────────────── Python ───────────────────────────

    fn visit_python(&mut self, n: Node) {
        match n.kind() {
            "function_definition" | "class_definition" => {
                let name = self.field_text(n, "name").unwrap_or_default();
                let in_class = n.parent().and_then(|b| b.parent()).is_some_and(|c| c.kind() == "class_definition");
                let kind = match (n.kind(), in_class) {
                    ("class_definition", _) => SymbolKind::Class,
                    (_, true) => SymbolKind::Method,
                    _ => SymbolKind::Function,
                };
                let range_node = n.parent().filter(|p| p.kind() == "decorated_definition").unwrap_or(n);
                let doc = self.python_docstring(n);
                let exported = !name.starts_with('_');
                self.push_symbol(range_node, name, kind, exported, doc);
            }
            "import_statement" => {
                let mut c = n.walk();
                for name in n.children_by_field_name("name", &mut c) {
                    let target = name.child_by_field_name("name").unwrap_or(name);
                    self.facts.imports.push(Import { specifier: self.text(target).to_string(), line: line(n) });
                }
            }
            "import_from_statement" => {
                if let Some(m) = n.child_by_field_name("module_name") {
                    let module = self.text(m).to_string();
                    let mut c = n.walk();
                    let names: Vec<String> =
                        n.children_by_field_name("name", &mut c).map(|x| self.text(x).to_string()).collect();
                    if module.chars().all(|ch| ch == '.') {
                        // `from . import shipping` → `.shipping`
                        for name in names {
                            self.facts.imports.push(Import { specifier: format!("{module}{name}"), line: line(n) });
                        }
                    } else {
                        self.facts.imports.push(Import { specifier: module, line: line(n) });
                    }
                }
            }
            "string" => {
                let is_docstring = n.parent().is_some_and(|p| p.kind() == "expression_statement");
                if !is_docstring {
                    let t = self.text(n);
                    self.push_string(n, t);
                }
            }
            "call" => {
                if let Some(f) = n.child_by_field_name("function") {
                    self.push_call(n, f);
                    let callee = self.text(f);
                    if matches!(callee, "FastAPI" | "Flask" | "Starlette" | "Sanic") {
                        let stmt = n.parent().and_then(|p| p.parent()).unwrap_or(n);
                        self.facts.entry_points.push(EntryPoint {
                            symbol: callee.to_string(),
                            start_line: line(stmt),
                            end_line: end_line(stmt),
                            reason: format!("`{callee}` application object"),
                        });
                    }
                }
            }
            "if_statement" => {
                let cond = n.child_by_field_name("condition").map(|c| self.text(c)).unwrap_or("");
                if cond.contains("__name__") && cond.contains("__main__") {
                    self.facts.entry_points.push(EntryPoint {
                        symbol: "__main__".into(),
                        start_line: line(n),
                        end_line: end_line(n),
                        reason: "`if __name__ == \"__main__\"` guard".into(),
                    });
                }
            }
            _ => {}
        }
    }

    fn python_docstring(&self, def: Node) -> Option<String> {
        let body = def.child_by_field_name("body")?;
        let first = body.named_child(0)?;
        if first.kind() != "expression_statement" {
            return None;
        }
        let s = first.named_child(0).filter(|s| s.kind() == "string")?;
        non_empty(unquote(self.text(s)))
    }
}

fn export_wrapper(n: Node<'_>) -> Node<'_> {
    match n.parent() {
        Some(p) if p.kind() == "export_statement" => p,
        _ => n,
    }
}

fn non_empty(s: String) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

fn clean_comment(t: &str, prefix: &str) -> String {
    let body = t.strip_prefix(prefix).unwrap_or(t).trim_end_matches("*/");
    body.lines()
        .map(|l| l.trim().trim_start_matches('*').trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Strips quotes from Rust/TS/Go/Python string syntax (including prefixes,
/// raw strings, triple quotes and template backticks).
/// `com.acme.OrderPaid<T>` → `OrderPaid`.
fn simple_type(raw: &str) -> String {
    raw.split('<').next().unwrap_or(raw).rsplit('.').next().unwrap_or(raw).trim().to_string()
}

pub fn unquote(raw: &str) -> String {
    let mut s = raw.trim();
    s = s.trim_start_matches(['r', 'b', 'f', 'u', 'R', 'B', 'F', 'U']);
    s = s.trim_matches('#');
    for q in ["\"\"\"", "'''", "\"", "'", "`"] {
        if s.len() >= 2 * q.len() && s.starts_with(q) && s.ends_with(q) {
            return s[q.len()..s.len() - q.len()].to_string();
        }
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(grammar: Grammar, lang: Language, file: &str, src: &str) -> FileFacts {
        extract(grammar, lang, file, src)
    }

    fn names(f: &FileFacts) -> Vec<(&str, SymbolKind)> {
        f.symbols.iter().map(|s| (s.name.as_str(), s.kind)).collect()
    }

    #[test]
    fn rust_symbols_docs_modules_and_main() {
        let src = "//! Word counter.\nmod tokenizer;\nuse crate::report::{render, Summary};\n\n/// Entry.\n/// Parses args.\nfn main() {\n    let q = \"SELECT 1\";\n    tokenizer::split(q);\n    println!(\"hi\");\n}\n\npub struct Config { pub n: usize }\nimpl Config {\n    pub fn new() -> Self { Self { n: 0 } }\n}\n";
        let f = run(Grammar::Rust, Language::Rust, "src/main.rs", src);
        assert_eq!(f.module_doc.as_deref(), Some("Word counter."));
        let main = f.symbols.iter().find(|s| s.name == "main").unwrap();
        assert_eq!((main.start_line, main.end_line), (7, 11));
        assert_eq!(main.doc.as_deref(), Some("Entry.\nParses args."));
        assert!(names(&f).contains(&("new", SymbolKind::Method)));
        assert!(names(&f).contains(&("impl Config", SymbolKind::Impl)));
        let specs: Vec<_> = f.imports.iter().map(|i| i.specifier.as_str()).collect();
        assert_eq!(specs, vec!["mod tokenizer", "crate::report::{render,Summary}"]);
        assert_eq!(f.entry_points.len(), 1);
        assert!(f.strings.iter().any(|s| s.value == "SELECT 1" && s.line == 8));
        assert!(f.calls.iter().any(|c| c.name == "split"));
        assert!(f.calls.iter().any(|c| c.name == "println"));
    }

    #[test]
    fn typescript_exports_arrow_functions_imports_and_listen() {
        let src = "import express from \"express\";\nimport { pay } from './clients/payments';\n\n/** Builds the app. */\nexport const buildApp = () => express();\n\nexport class Store {\n  save() { return db.query(`INSERT INTO t VALUES (1)`); }\n}\n\nfunction start() {\n  buildApp().listen(3000);\n}\n";
        let f = run(Grammar::TypeScript, Language::TypeScript, "src/server.ts", src);
        let build = f.symbols.iter().find(|s| s.name == "buildApp").unwrap();
        assert!(build.exported);
        assert_eq!(build.doc.as_deref(), Some("Builds the app."));
        assert!(names(&f).contains(&("save", SymbolKind::Method)));
        assert_eq!(
            f.imports.iter().map(|i| i.specifier.as_str()).collect::<Vec<_>>(),
            vec!["express", "./clients/payments"]
        );
        assert!(f.strings.iter().any(|s| s.value.starts_with("INSERT INTO")));
        let ep = &f.entry_points[0];
        assert_eq!((ep.symbol.as_str(), ep.start_line, ep.end_line), ("start", 11, 13));
    }

    #[test]
    fn go_package_main_types_and_imports() {
        let src = "// Package main runs the server.\npackage main\n\nimport (\n\t\"net/http\"\n\t\"example.com/x/internal/store\"\n)\n\n// Handler serves requests.\ntype Handler struct{}\n\nfunc main() {\n\thttp.ListenAndServe(\":8080\", nil)\n}\n";
        let f = run(Grammar::Go, Language::Go, "main.go", src);
        assert_eq!(f.module_doc.as_deref(), Some("Package main runs the server."));
        let h = f.symbols.iter().find(|s| s.name == "Handler").unwrap();
        assert_eq!(
            (h.kind, h.exported, h.doc.as_deref()),
            (SymbolKind::Struct, true, Some("Handler serves requests."))
        );
        assert_eq!(f.imports.len(), 2);
        assert_eq!(f.entry_points[0].start_line, 12);
        assert!(f.calls.iter().any(|c| c.name == "ListenAndServe"));
        assert!(!f.strings.iter().any(|s| s.value == "net/http"));
    }

    #[test]
    fn python_docstrings_relative_imports_and_main_guard() {
        let src = "\"\"\"Fulfillment worker.\"\"\"\nfrom . import shipping\nfrom .consumer import OrderConsumer\nimport psycopg\n\nclass Worker:\n    \"\"\"Ships orders.\"\"\"\n    def run(self):\n        self.c.subscribe([\"order.placed\"])\n\nif __name__ == \"__main__\":\n    Worker().run()\n";
        let f = run(Grammar::Python, Language::Python, "fulfillment/__main__.py", src);
        assert_eq!(f.module_doc.as_deref(), Some("Fulfillment worker."));
        let specs: Vec<_> = f.imports.iter().map(|i| i.specifier.as_str()).collect();
        assert_eq!(specs, vec![".shipping", ".consumer", "psycopg"]);
        assert!(names(&f).contains(&("run", SymbolKind::Method)));
        assert_eq!(f.symbols[0].doc.as_deref(), Some("Ships orders."));
        assert_eq!(f.entry_points[0].start_line, 11);
        assert!(f.strings.iter().any(|s| s.value == "order.placed"));
        assert!(!f.strings.iter().any(|s| s.value.contains("Ships")));
    }

    #[test]
    fn java_symbols_annotations_imports_and_entry_points() {
        let src = r#"package com.acme.account;

import org.springframework.web.bind.annotation.*;
import static java.util.Objects.requireNonNull;

/** Account endpoints. */
@RestController
@RequestMapping(value = "/accounts", produces = "application/json")
public class AccountController {
    @Autowired private AccountService service;

    /** Current account. */
    @GetMapping("/current")
    public Account get(@PathVariable("name") String name) {
        return service.findByName(name).orElseThrow();
    }
}

@FeignClient(name = "statistics-service")
interface StatisticsClient { @RequestMapping(method = RequestMethod.PUT, value = "/statistics/{accountName}") void update(); }

public record Money(long cents) {}
enum Status { ACTIVE, CLOSED }

@SpringBootApplication
public class App { public static void main(String[] args) { SpringApplication.run(App.class, args); String t = """
 text block
 """; } }
"#;
        let f = run(Grammar::Java, Language::Java, "account/src/main/java/com/acme/account/App.java", src);
        assert_eq!(f.package.as_deref(), Some("com.acme.account"));
        assert_eq!(f.imports[0].specifier, "org.springframework.web.bind.annotation.*");
        assert_eq!(f.imports[1].specifier, "java.util.Objects.requireNonNull");
        let names = names(&f);
        for expected in [
            ("AccountController", SymbolKind::Class),
            ("get", SymbolKind::Method),
            ("StatisticsClient", SymbolKind::Interface),
            ("Money", SymbolKind::Class),
            ("Status", SymbolKind::Enum),
            ("main", SymbolKind::Method),
        ] {
            assert!(names.contains(&expected), "{expected:?} in {names:?}");
        }
        let controller = f.symbols.iter().find(|s| s.name == "AccountController").unwrap();
        assert_eq!(controller.doc.as_deref(), Some("Account endpoints."));
        assert!(controller.exported);

        let ann = |name: &str| f.annotations.iter().find(|a| a.name == name).unwrap_or_else(|| panic!("@{name}"));
        let mapping = ann("RequestMapping");
        assert_eq!((mapping.target_kind.as_str(), mapping.target.as_str()), ("class", "AccountController"));
        assert_eq!(mapping.arguments, r#"value = "/accounts", produces = "application/json""#);
        let get = ann("GetMapping");
        assert_eq!(
            (get.target.as_str(), get.owner.as_deref(), get.arguments.as_str()),
            ("get", Some("AccountController"), r#""/current""#)
        );
        let path = ann("PathVariable");
        assert_eq!(
            (path.target_kind.as_str(), path.target.as_str(), path.owner.as_deref()),
            ("parameter", "name", Some("get"))
        );
        let field = ann("Autowired");
        assert_eq!((field.target_kind.as_str(), field.target.as_str()), ("field", "service"));
        assert_eq!(ann("FeignClient").arguments, r#"name = "statistics-service""#);

        let ep = &f.entry_points[0];
        assert_eq!(ep.symbol, "App");
        assert!(
            ep.reason.contains("application object") && ep.reason.contains("SpringBootApplication"),
            "{}",
            ep.reason
        );
        assert!(f.calls.iter().any(|c| c.callee == "SpringApplication.run" && c.name == "run"));
        assert!(f.calls.iter().any(|c| c.callee == "service.findByName(name).orElseThrow" || c.name == "findByName"));
    }

    #[test]
    fn unquote_variants() {
        assert_eq!(unquote("r#\"a\"#"), "a");
        assert_eq!(unquote("\"\"\"doc\"\"\""), "doc");
        assert_eq!(unquote("f'x{y}'"), "x{y}");
        assert_eq!(unquote("`t`"), "t");
    }
}
