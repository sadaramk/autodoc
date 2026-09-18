//! Kotlin persistence: JPA / Hibernate entities written in Kotlin (Spring Data
//! JPA, Micronaut Data, Quarkus Panache), Spring Data MongoDB documents,
//! Spring Data JDBC / R2DBC mapped entities, Exposed and Ktorm table objects;
//! Kotlin enums; repository, table-object and `EntityManager` access.
//!
//! Kotlin declares persisted state as primary-constructor `val` / `var`
//! properties as often as body properties, and nullability is part of the type:
//! a non-null Kotlin type means `NOT NULL` with no `@Column(nullable = false)`
//! to say so. Physical names follow the same framework defaults as Java —
//! Spring Boot snake-cases, plain Hibernate doesn't, Spring Data MongoDB
//! uncapitalises the class name — and column types without DDL are inferred
//! from the Kotlin type (DDL, when present, wins in the merge).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use tree_sitter::Node;

use super::java_syntax::{ann_arg, generic, simple};
use super::raw::*;
use super::states::Src as StateSrc;
use super::Column;

pub struct KotlinOutput {
    pub entities: Vec<RawEntity>,
    pub enums: Vec<RawEnum>,
    pub enum_uses: Vec<RawEnumUse>,
    pub model: KotlinModel,
}

#[derive(Default)]
pub struct KotlinModel {
    /// Repository type → (entity class, custom method → write?).
    repos: HashMap<String, (String, HashMap<String, bool>)>,
    /// Entity classes by simple name.
    entities: BTreeSet<String>,
    /// Active-record entities (Panache): static finders on the companion.
    active_record: BTreeSet<String>,
    /// Exposed / Ktorm table objects → physical table.
    tables: BTreeMap<String, String>,
    /// Exposed DAO entity classes → the table their companion maps.
    dao: BTreeMap<String, String>,
}

/// One read or write of an entity, at the line that performs it.
pub struct Access {
    /// Entity class, or `#table` when the table object names the table.
    pub name: String,
    pub write: bool,
    /// Index into the sources given to [`access`].
    pub src: usize,
    pub line: u32,
    /// Why this line touches this entity when the code doesn't name it.
    pub note: Option<String>,
}

// ---------------------------------------------------------------------------
// Declarations, read from the Kotlin grammar.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct KAnn {
    name: String,
    /// Arguments as written, without the surrounding parentheses.
    args: String,
    line: u32,
}

#[derive(Debug, Clone)]
struct KProp {
    name: String,
    /// Declared type without a trailing `?`; empty when inferred.
    ty: String,
    nullable: bool,
    anns: Vec<KAnn>,
    init: Option<String>,
    line: u32,
    /// `val`/`var` in the primary constructor, or a body property.
    property: bool,
    /// Has a custom getter: computed, not stored.
    computed: bool,
}

#[derive(Debug, Clone)]
struct KFun {
    name: String,
    anns: Vec<KAnn>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Decl {
    Class,
    Object,
    Interface,
    Enum,
}

#[derive(Debug, Clone)]
struct KClass {
    file: usize,
    name: String,
    decl: Decl,
    anns: Vec<KAnn>,
    /// Delegation specifiers as written (`JpaRepository<Order, Long>`, `Table("users")`).
    supers: Vec<String>,
    props: Vec<KProp>,
    funs: Vec<KFun>,
    /// Enum entries, in declaration order.
    entries: Vec<String>,
    /// Body text, for patterns the AST doesn't need to be walked for.
    body: String,
    line: u32,
}

impl KClass {
    fn ann(&self, name: &str) -> Option<&KAnn> {
        self.anns.iter().find(|a| a.name == name)
    }

    fn has(&self, name: &str) -> bool {
        self.ann(name).is_some()
    }
}

impl KProp {
    fn ann(&self, name: &str) -> Option<&KAnn> {
        self.anns.iter().find(|a| a.name == name)
    }

    fn has(&self, name: &str) -> bool {
        self.ann(name).is_some()
    }
}

fn txt<'a>(src: &'a str, n: Node) -> &'a str {
    n.utf8_text(src.as_bytes()).unwrap_or("")
}

fn line_of(n: Node) -> u32 {
    n.start_position().row as u32 + 1
}

fn child<'t>(n: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut c = n.walk();
    let found = n.children(&mut c).find(|x| x.kind() == kind);
    found
}

fn children<'t>(n: Node<'t>, kind: &str) -> Vec<Node<'t>> {
    let mut c = n.walk();
    n.children(&mut c).filter(|x| x.kind() == kind).collect()
}

const TYPE_KINDS: &[&str] = &["user_type", "nullable_type", "function_type", "parenthesized_type", "dynamic_type"];

/// Declared type of a parameter / variable declaration: `(text without `?`, nullable)`.
fn declared_type(src: &str, n: Node) -> (String, bool) {
    let mut c = n.walk();
    let Some(t) = n.children(&mut c).find(|x| TYPE_KINDS.contains(&x.kind())) else {
        return (String::new(), false);
    };
    let text = collapse(txt(src, t));
    let nullable = t.kind() == "nullable_type" || text.ends_with('?');
    (text.trim_end_matches('?').trim().to_string(), nullable)
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Expression after the `=` of a declaration.
fn initializer(src: &str, n: Node) -> Option<String> {
    let mut c = n.walk();
    let kids: Vec<Node> = n.children(&mut c).collect();
    let eq = kids.iter().position(|k| k.kind() == "=")?;
    let init = kids[eq + 1..].iter().find(|k| k.is_named())?;
    Some(collapse(txt(src, *init)))
}

/// One `annotation` node: `@field:Size(max = 80)` → (`Size`, `max = 80`).
fn annotation_of(src: &str, n: Node) -> Option<KAnn> {
    let (head, args) = match child(n, "constructor_invocation") {
        Some(ci) => {
            let ty = child(ci, "user_type").map(|t| txt(src, t)).unwrap_or("");
            let args = child(ci, "value_arguments").map(|a| txt(src, a)).unwrap_or("");
            (ty, args.trim().trim_start_matches('(').trim_end_matches(')').to_string())
        }
        None => (child(n, "user_type").map(|t| txt(src, t)).unwrap_or(""), String::new()),
    };
    let name = simple(head);
    if name.is_empty() {
        return None;
    }
    Some(KAnn { name, args: collapse(&args), line: line_of(n) })
}

fn annotations_in(src: &str, mods: Node) -> Vec<KAnn> {
    children(mods, "annotation").iter().filter_map(|a| annotation_of(src, *a)).collect()
}

/// The grammar sometimes parses annotations that precede a declaration as an
/// expression of their own instead of as its modifiers; the arguments of each
/// then sit in a sibling `parenthesized_expression`.
fn detached_annotations(src: &str, n: Node, out: &mut Vec<KAnn>) {
    let mut c = n.walk();
    let kids: Vec<Node> = n.children(&mut c).collect();
    for (i, k) in kids.iter().enumerate() {
        match k.kind() {
            "annotation" => {
                if let Some(mut a) = annotation_of(src, *k) {
                    if a.args.is_empty() {
                        if let Some(p) = kids.get(i + 1).filter(|p| p.kind() == "parenthesized_expression") {
                            let raw = txt(src, *p);
                            a.args = collapse(raw.trim().trim_start_matches('(').trim_end_matches(')'));
                        }
                    }
                    out.push(a);
                }
            }
            "annotated_expression" => detached_annotations(src, *k, out),
            _ => {}
        }
    }
}

/// Annotations of a declaration, from its modifiers or the detached form.
fn leading_annotations(src: &str, n: Node) -> Vec<KAnn> {
    if let Some(mods) = child(n, "modifiers") {
        let anns = annotations_in(src, mods);
        if !anns.is_empty() {
            return anns;
        }
    }
    let mut out = vec![];
    let mut cur = n.prev_sibling();
    while let Some(p) = cur.filter(|p| p.kind() == "annotated_expression") {
        let mut here = vec![];
        detached_annotations(src, p, &mut here);
        here.extend(std::mem::take(&mut out));
        out = here;
        cur = p.prev_sibling();
    }
    out
}

fn property_of(src: &str, n: Node) -> Option<KProp> {
    let var = child(n, "variable_declaration")?;
    let id = child(var, "identifier")?;
    let name = txt(src, id).to_string();
    if name.is_empty() {
        return None;
    }
    let (ty, nullable) = declared_type(src, var);
    Some(KProp {
        name,
        ty,
        nullable,
        anns: child(n, "modifiers").map(|m| annotations_in(src, m)).unwrap_or_default(),
        init: initializer(src, n),
        // The `val` / `var` line, not the first annotation above it.
        line: line_of(id),
        property: true,
        computed: child(n, "getter").is_some(),
    })
}

fn class_parameter_of(src: &str, n: Node) -> Option<KProp> {
    let id = child(n, "identifier")?;
    let name = txt(src, id).to_string();
    if name.is_empty() {
        return None;
    }
    let (ty, nullable) = declared_type(src, n);
    Some(KProp {
        name,
        ty,
        nullable,
        anns: child(n, "modifiers").map(|m| annotations_in(src, m)).unwrap_or_default(),
        init: initializer(src, n),
        line: line_of(id),
        // Only `val` / `var` constructor parameters become properties.
        property: child(n, "val").is_some() || child(n, "var").is_some(),
        computed: false,
    })
}

fn class_of(file: usize, src: &str, n: Node) -> Option<KClass> {
    let id = child(n, "identifier")?;
    let name = txt(src, id).to_string();
    if name.is_empty() {
        return None;
    }
    let mods = child(n, "modifiers");
    let decl = if n.kind() == "object_declaration" {
        Decl::Object
    } else if child(n, "interface").is_some() {
        Decl::Interface
    } else if children(mods.unwrap_or(n), "class_modifier").iter().any(|m| txt(src, *m).trim() == "enum") {
        Decl::Enum
    } else {
        Decl::Class
    };
    let body = child(n, "class_body").or_else(|| child(n, "enum_class_body"));
    let mut props: Vec<KProp> = vec![];
    if let Some(pc) = child(n, "primary_constructor").and_then(|p| child(p, "class_parameters")) {
        props.extend(children(pc, "class_parameter").iter().filter_map(|p| class_parameter_of(src, *p)));
    }
    let mut funs = vec![];
    let mut entries = vec![];
    if let Some(b) = body {
        props.extend(children(b, "property_declaration").iter().filter_map(|p| property_of(src, *p)));
        for f in children(b, "function_declaration") {
            if let Some(id) = child(f, "identifier") {
                funs.push(KFun {
                    name: txt(src, id).to_string(),
                    anns: child(f, "modifiers").map(|m| annotations_in(src, m)).unwrap_or_default(),
                });
            }
        }
        for e in children(b, "enum_entry") {
            if let Some(id) = child(e, "identifier") {
                entries.push(txt(src, id).to_string());
            }
        }
    }
    let supers = child(n, "delegation_specifiers")
        .map(|d| children(d, "delegation_specifier").iter().map(|s| collapse(txt(src, *s))).collect())
        .unwrap_or_default();
    Some(KClass {
        file,
        name,
        decl,
        anns: leading_annotations(src, n),
        supers,
        props,
        funs,
        entries,
        body: body.map(|b| txt(src, b).to_string()).unwrap_or_default(),
        // The declaration line, not the first annotation above it.
        line: line_of(id),
    })
}

fn walk(file: usize, src: &str, n: Node, depth: usize, out: &mut Vec<KClass>) {
    if depth > 120 {
        return;
    }
    let mut c = n.walk();
    for ch in n.named_children(&mut c) {
        match ch.kind() {
            "class_declaration" | "object_declaration" => {
                if let Some(k) = class_of(file, src, ch) {
                    out.push(k);
                }
                for b in ["class_body", "enum_class_body"] {
                    if let Some(body) = child(ch, b) {
                        walk(file, src, body, depth + 1, out);
                    }
                }
            }
            _ => walk(file, src, ch, depth + 1, out),
        }
    }
}

fn parse_file(file: usize, text: &str, out: &mut Vec<KClass>) {
    let mut p = tree_sitter::Parser::new();
    if p.set_language(&crate::lang::Grammar::Kotlin.ts_language()).is_err() {
        return;
    }
    let Some(tree) = p.parse(text, None) else { return };
    walk(file, text, tree.root_node(), 0, out);
}

// ---------------------------------------------------------------------------
// Mapping.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Jpa,
    Jdbc,
    Mongo,
    Micronaut,
    PanacheMongo,
    Exposed,
    Ktorm,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Jpa => "jpa",
            Kind::Jdbc => "spring-data-jdbc",
            Kind::Mongo => "spring-data-mongodb",
            Kind::Micronaut => "micronaut-data",
            Kind::PanacheMongo => "panache-mongodb",
            Kind::Exposed => "exposed",
            Kind::Ktorm => "ktorm",
        }
    }

    fn document(self) -> bool {
        matches!(self, Kind::Mongo | Kind::PanacheMongo)
    }
}

const REPO_BASES: &[&str] = &[
    "Repository",
    "CrudRepository",
    "ListCrudRepository",
    "PagingAndSortingRepository",
    "ListPagingAndSortingRepository",
    "JpaRepository",
    "JpaSpecificationExecutor",
    "MongoRepository",
    "ReactiveMongoRepository",
    "ReactiveCrudRepository",
    "ReactiveSortingRepository",
    "R2dbcRepository",
    "CoroutineCrudRepository",
    "CoroutineSortingRepository",
    "CoroutinePagingAndSortingRepository",
    "KotlinCrudRepository",
    "RevisionRepository",
    "PanacheRepository",
    "PanacheRepositoryBase",
    "PanacheMongoRepository",
    "PanacheMongoRepositoryBase",
    "ReactivePanacheMongoRepository",
    "GenericRepository",
    "PageableRepository",
    "AsyncCrudRepository",
    "ReactorCrudRepository",
];

const PANACHE_BASES: &[&str] = &[
    "PanacheEntity",
    "PanacheEntityBase",
    "PanacheMongoEntity",
    "PanacheMongoEntityBase",
    "ReactivePanacheMongoEntity",
];

/// Exposed table bases, with the primary key each implies.
const EXPOSED_TABLES: &[(&str, Option<&str>)] = &[
    ("Table", None),
    ("IdTable", None),
    ("CompositeIdTable", None),
    ("IntIdTable", Some("integer")),
    ("UIntIdTable", Some("integer")),
    ("LongIdTable", Some("bigint")),
    ("ULongIdTable", Some("bigint")),
    ("UUIDTable", Some("uuid")),
];

const COLLECTIONS: &[&str] = &[
    "List",
    "MutableList",
    "Set",
    "MutableSet",
    "Collection",
    "MutableCollection",
    "Iterable",
    "Sequence",
    "Map",
    "MutableMap",
    "SortedSet",
    "ArrayList",
    "HashSet",
    "LinkedHashSet",
    "TreeSet",
    "Array",
    "Flow",
    "Flux",
];

fn uncapitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// `"orders"` / `ORDERS` / `Names.ORDERS` → `orders`.
fn resolve_str(expr: &str, consts: &HashMap<String, String>) -> Option<String> {
    let mut out = String::new();
    for part in split_args(&expr.replace('+', ",")) {
        let p = part.trim();
        if p.len() >= 2 && p.starts_with('"') && p.ends_with('"') {
            out.push_str(&p[1..p.len() - 1]);
        } else if let Some(v) = consts.get(p).or_else(|| consts.get(p.rsplit('.').next().unwrap_or(p))) {
            out.push_str(v);
        } else {
            return None;
        }
    }
    (!out.is_empty()).then_some(out)
}

fn str_arg(args: &str, keys: &[&str], consts: &HashMap<String, String>) -> Option<String> {
    ann_arg(args, keys).and_then(|v| resolve_str(&v, consts))
}

/// `const val ORDERS = "orders"` constants, keyed `ORDERS` and `Owner.ORDERS`.
fn constants(classes: &[KClass]) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();
    for _ in 0..2 {
        for c in classes {
            for p in &c.props {
                let Some(init) = &p.init else { continue };
                if let Some(v) = resolve_str(init, &out) {
                    out.entry(format!("{}.{}", c.name, p.name)).or_insert_with(|| v.clone());
                    out.entry(p.name.clone()).or_insert(v);
                }
            }
        }
    }
    out
}

/// `org.ktorm.schema.Table<Nothing>("accounts")` → (`Table`, [`Nothing`], `"accounts"`).
fn spec_parts(spec: &str) -> (String, Vec<String>, String) {
    let mut depth = 0i32;
    let mut open = None;
    for (i, ch) in spec.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth -= 1,
            '(' if depth == 0 => {
                open = Some(i);
                break;
            }
            _ => {}
        }
    }
    let (head, args) = match open {
        Some(i) => (&spec[..i], spec[i..].trim_start_matches('(').trim_end_matches(')')),
        None => (spec, ""),
    };
    let (base, targs) = generic(head.trim());
    (base, targs, args.trim().to_string())
}

/// Bean Validation (Jakarta / javax / Hibernate Validator) as constraint statements.
fn validation(p: &KProp) -> Vec<String> {
    let mut out = vec![];
    let num = |a: &KAnn, keys: &[&str]| ann_arg(&a.args, keys).map(|v| v.trim_matches('"').to_string());
    for a in &p.anns {
        match a.name.as_str() {
            "Size" | "Length" => {
                if let Some(m) = num(a, &["min"]).filter(|m| m != "0") {
                    out.push(format!("min length {m}"));
                }
                if let Some(m) = num(a, &["max"]) {
                    out.push(format!("max length {m}"));
                }
            }
            "Min" | "DecimalMin" => {
                if let Some(v) = num(a, &["value", ""]) {
                    out.push(format!("≥ {v}"));
                }
            }
            "Max" | "DecimalMax" => {
                if let Some(v) = num(a, &["value", ""]) {
                    out.push(format!("≤ {v}"));
                }
            }
            "Positive" => out.push("> 0".into()),
            "PositiveOrZero" => out.push("≥ 0".into()),
            "Negative" => out.push("< 0".into()),
            "NegativeOrZero" => out.push("≤ 0".into()),
            "Pattern" => {
                if let Some(r) = num(a, &["regexp"]) {
                    out.push(format!("matches {r}"));
                }
            }
            "Email" => out.push("valid email".into()),
            "NotBlank" => out.push("not blank".into()),
            "NotEmpty" => out.push("not empty".into()),
            "Past" => out.push("in the past".into()),
            "PastOrPresent" => out.push("not in the future".into()),
            "Future" => out.push("in the future".into()),
            "FutureOrPresent" => out.push("not in the past".into()),
            _ => {}
        }
    }
    out
}

/// Column type from the Kotlin type, the way each framework's DDL generator would.
fn sql_type(p: &KProp, base: &str, length: Option<&str>) -> String {
    if let Some(e) = p.ann("Enumerated") {
        return if e.args.contains("STRING") { "varchar".into() } else { "integer".into() };
    }
    if p.has("Lob") {
        return if base == "String" { "text".into() } else { "blob".into() };
    }
    let precision = p.ann("Column").and_then(|a| ann_arg(&a.args, &["precision"]));
    let scale = p.ann("Column").and_then(|a| ann_arg(&a.args, &["scale"]));
    match base {
        "String" => format!("varchar({})", length.unwrap_or("255")),
        "Long" | "ULong" => "bigint".into(),
        "Int" | "UInt" | "Integer" => "integer".into(),
        "Short" | "UShort" => "smallint".into(),
        "Byte" | "UByte" => "tinyint".into(),
        "Boolean" => "boolean".into(),
        "Double" => "double precision".into(),
        "Float" => "real".into(),
        "BigDecimal" | "BigInteger" => match (precision, scale) {
            (Some(p), Some(s)) => format!("numeric({p},{s})"),
            (Some(p), None) => format!("numeric({p})"),
            _ => "numeric".into(),
        },
        "UUID" => "uuid".into(),
        "Instant" | "OffsetDateTime" | "ZonedDateTime" => "timestamp with time zone".into(),
        "LocalDateTime" | "Date" | "Timestamp" => "timestamp".into(),
        "LocalDate" => "date".into(),
        "LocalTime" | "Time" => "time".into(),
        "Duration" => "interval".into(),
        "Char" | "Character" => "char(1)".into(),
        "ByteArray" => "bytea".into(),
        _ => p.ty.clone(),
    }
}

/// Constructor defaults that also describe the stored value.
fn literal_default(init: &str) -> Option<String> {
    let t = init.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        return Some(t[1..t.len() - 1].to_string());
    }
    if t == "true" || t == "false" || t.trim_end_matches(['L', 'u', 'U', 'f', 'F']).parse::<f64>().is_ok() {
        return Some(t.trim_end_matches(['L', 'u', 'U']).to_string());
    }
    // `OrderStatus.NEW`
    let parts: Vec<&str> = t.split('.').collect();
    if parts.len() == 2
        && parts[0].starts_with(|c: char| c.is_uppercase())
        && !parts[1].is_empty()
        && parts[1].chars().all(|c| c.is_uppercase() || c.is_ascii_digit() || c == '_')
    {
        return Some(parts[1].to_string());
    }
    None
}

/// Single-column `uniqueConstraints = [UniqueConstraint(columnNames = ["number"])]`.
fn single_unique_columns(args: &str, consts: &HashMap<String, String>) -> Vec<String> {
    let mut out = vec![];
    let mut from = 0;
    while let Some(p) = args[from..].find("columnNames").map(|x| from + x) {
        from = p + 1;
        let rest = args[p..].split_once('=').map(|(_, r)| r.trim()).unwrap_or("");
        let list = match rest.strip_prefix('[') {
            Some(inner) => inner.split(']').next().unwrap_or(""),
            None => rest.split([',', ')']).next().unwrap_or(""),
        };
        let items: Vec<String> = split_args(list).iter().filter_map(|x| resolve_str(x, consts)).collect();
        if items.len() == 1 {
            out.push(items[0].clone());
        }
    }
    out
}

struct FieldCtx<'a> {
    kind: Kind,
    snake: bool,
    consts: &'a HashMap<String, String>,
    owner_table: &'a str,
    classes: &'a [KClass],
    by_name: &'a HashMap<String, Vec<usize>>,
    files: &'a [(String, String, String)],
    file: usize,
}

impl<'a> FieldCtx<'a> {
    fn naming(&self, n: &str) -> String {
        if self.snake {
            snake_case(n)
        } else {
            n.to_string()
        }
    }

    /// Declaration of `name`, preferring the same file, then the same unit.
    fn find(&self, name: &str, file: usize) -> Option<&'a KClass> {
        let ids = self.by_name.get(name)?;
        ids.iter()
            .map(|&i| &self.classes[i])
            .find(|c| c.file == file)
            .or_else(|| ids.iter().map(|&i| &self.classes[i]).find(|c| self.files[c.file].1 == self.files[file].1))
            .or_else(|| ids.first().map(|&i| &self.classes[i]))
    }
}

/// Physical column name and the annotation line that states it.
fn column_name(ctx: &FieldCtx, p: &KProp) -> (String, Option<u32>) {
    let named = match ctx.kind {
        Kind::Jpa => p.ann("Column").and_then(|a| ann_arg(&a.args, &["name"]).map(|v| (v, a.line))),
        Kind::Jdbc => p.ann("Column").and_then(|a| ann_arg(&a.args, &["value", "name", ""]).map(|v| (v, a.line))),
        Kind::Micronaut => p.ann("MappedProperty").and_then(|a| ann_arg(&a.args, &["value", ""]).map(|v| (v, a.line))),
        Kind::Mongo => p.ann("Field").and_then(|a| ann_arg(&a.args, &["value", "name", ""]).map(|v| (v, a.line))),
        Kind::PanacheMongo => p.ann("BsonProperty").and_then(|a| ann_arg(&a.args, &["value", ""]).map(|v| (v, a.line))),
        Kind::Exposed | Kind::Ktorm => None,
    };
    match named.and_then(|(v, l)| resolve_str(&v, ctx.consts).map(|n| (n, l))) {
        Some((n, l)) => (n, Some(l)),
        None => (ctx.naming(&p.name), None),
    }
}

const RELATION_ANNS: &[&str] =
    &["ManyToOne", "OneToOne", "OneToMany", "ManyToMany", "DBRef", "DocumentReference", "Relation", "MappedCollection"];

#[allow(clippy::too_many_arguments)]
fn prop_columns(
    ctx: &FieldCtx,
    path: &str,
    p: &KProp,
    embedded_pk: Option<bool>,
    columns: &mut Vec<Column>,
    relations: &mut Vec<RawRelation>,
    unique_sets: &[String],
    depth: usize,
) {
    if !p.property || p.computed || p.has("Transient") || p.has("Ignore") {
        return;
    }
    let (base, args) = generic(&p.ty);
    let collection = COLLECTIONS.contains(&base.as_str()) && base != "Array" || base == "Array" && p.ty.contains('<');
    let relation_ann = RELATION_ANNS.iter().find_map(|n| p.ann(n));
    let target = relation_ann
        .and_then(|a| ann_arg(&a.args, &["targetEntity"]))
        .map(|t| simple(t.trim_end_matches("::class").trim_end_matches(".java")))
        .or_else(|| if collection { args.last().cloned() } else { Some(base.clone()) })
        .unwrap_or_default();

    if let Some(a) = relation_ann {
        let mapped_by = str_arg(&a.args, &["mappedBy"], ctx.consts);
        let kind = match a.name.as_str() {
            "ManyToOne" => "many-to-one",
            "OneToOne" => "one-to-one",
            "OneToMany" | "MappedCollection" => "one-to-many",
            "ManyToMany" => "many-to-many",
            "Relation" if a.args.contains("MANY_TO_ONE") => "many-to-one",
            "Relation" if a.args.contains("ONE_TO_ONE") => "one-to-one",
            "Relation" if a.args.contains("ONE_TO_MANY") => "one-to-many",
            "Relation" if a.args.contains("MANY_TO_MANY") => "many-to-many",
            "DBRef" | "DocumentReference" => {
                if collection {
                    "one-to-many"
                } else {
                    "many-to-one"
                }
            }
            _ => return,
        };
        if target.is_empty() {
            return;
        }
        let owning = mapped_by.is_none();
        match kind {
            "many-to-one" | "one-to-one" if owning => {
                let join = p.ann("JoinColumn");
                let (col, line) = match join.and_then(|j| str_arg(&j.args, &["name"], ctx.consts).map(|n| (n, j.line)))
                {
                    Some((n, l)) => (n, l),
                    None if ctx.kind.document() => (p.name.clone(), p.line),
                    None => (format!("{}_id", ctx.naming(&p.name)), p.line),
                };
                let nullable = p.nullable
                    && !(join.is_some_and(|j| j.args.replace(' ', "").contains("nullable=false"))
                        || a.args.replace(' ', "").contains("optional=false")
                        || p.has("NotNull"));
                columns.retain(|c| c.name != col);
                columns.push(Column {
                    name: col.clone(),
                    type_name: if ctx.kind.document() { format!("ref {target}") } else { "bigint".into() },
                    primary_key: p.has("Id") || p.has("MapsId") && embedded_pk.is_some(),
                    nullable,
                    unique: kind == "one-to-one" || unique_sets.contains(&col),
                    references: Some(format!("@{target}.?")),
                    default: None,
                    constraints: vec![],
                    evidence: line_ref(path, line),
                });
                relations.push(RawRelation {
                    kind: kind.into(),
                    target: format!("@{target}"),
                    via: col,
                    evidence: line_ref(path, p.line),
                });
            }
            "one-to-many" => {
                let via = mapped_by
                    .or_else(|| p.ann("JoinColumn").and_then(|j| str_arg(&j.args, &["name"], ctx.consts)))
                    .unwrap_or_else(|| p.name.clone());
                relations.push(RawRelation {
                    kind: kind.into(),
                    target: format!("@{target}"),
                    via,
                    evidence: line_ref(path, p.line),
                });
            }
            "many-to-many" if owning => {
                let via = p
                    .ann("JoinTable")
                    .and_then(|j| str_arg(&j.args, &["name"], ctx.consts))
                    .unwrap_or_else(|| format!("{}_{}", ctx.owner_table, ctx.naming(&p.name)));
                relations.push(RawRelation {
                    kind: kind.into(),
                    target: format!("@{target}"),
                    via,
                    evidence: line_ref(path, p.line),
                });
            }
            _ => {}
        }
        return;
    }
    if p.has("ElementCollection") {
        return;
    }
    // Spring Data JDBC states a reference in the type: `AggregateReference<User, Long>`.
    if base == "AggregateReference" && matches!(ctx.kind, Kind::Jdbc | Kind::Micronaut) {
        let target = args.first().cloned().unwrap_or_default();
        if !target.is_empty() {
            let (name, name_line) = column_name(ctx, p);
            columns.retain(|c| c.name != name);
            columns.push(Column {
                name: name.clone(),
                type_name: "bigint".into(),
                primary_key: false,
                nullable: p.nullable,
                unique: unique_sets.contains(&name),
                references: Some(format!("@{target}.?")),
                default: None,
                constraints: vec![],
                evidence: line_ref(path, name_line.unwrap_or(p.line)),
            });
            relations.push(RawRelation {
                kind: "many-to-one".into(),
                target: format!("@{target}"),
                via: name,
                evidence: line_ref(path, p.line),
            });
        }
        return;
    }
    // Embedded value objects flatten into the owning table.
    let embeddable = ctx.find(&base, ctx.file).filter(|e| e.has("Embeddable"));
    if (p.has("Embedded") || p.has("EmbeddedId") || embeddable.is_some())
        && matches!(ctx.kind, Kind::Jpa | Kind::Micronaut | Kind::Jdbc)
        && depth < 3
    {
        if let Some(e) = embeddable.or_else(|| ctx.find(&base, ctx.file)) {
            let pk = p.has("EmbeddedId");
            let epath = ctx.files[e.file].0.clone();
            let ectx = FieldCtx { file: e.file, ..*ctx };
            for ep in &e.props {
                let before = columns.len();
                prop_columns(&ectx, &epath, ep, Some(pk), columns, relations, unique_sets, depth + 1);
                if pk {
                    for c in columns[before..].iter_mut() {
                        c.primary_key = true;
                        c.nullable = false;
                    }
                }
            }
            return;
        }
    }
    if collection && ctx.kind == Kind::Jpa {
        // JPA doesn't map a plain collection without `@ElementCollection`.
        return;
    }
    let (name, name_line) = column_name(ctx, p);
    let pk = p.has("Id") || p.has("EmbeddedId") || embedded_pk == Some(true);
    let col_args = p.ann("Column").map(|a| a.args.replace(' ', "")).unwrap_or_default();
    let not_null = ["NotNull", "NonNull", "NotBlank", "NotEmpty"].iter().any(|n| p.has(n));
    // Kotlin states nullability in the type; the annotation still wins when present.
    let nullable = if col_args.contains("nullable=false") {
        false
    } else if col_args.contains("nullable=true") {
        true
    } else {
        p.nullable && !pk && !not_null
    };
    let mut unique = col_args.contains("unique=true")
        || unique_sets.contains(&name)
        || p.ann("Indexed").is_some_and(|a| a.args.replace(' ', "").contains("unique=true"));
    if pk && embedded_pk.is_none() {
        unique = true;
    }
    let length =
        p.ann("Column").and_then(|a| ann_arg(&a.args, &["length"])).filter(|l| l.chars().all(|c| c.is_ascii_digit()));
    let mut constraints = validation(p);
    let type_name = match ctx.kind {
        Kind::Jpa | Kind::Jdbc | Kind::Micronaut => {
            match p.ann("Column").and_then(|a| str_arg(&a.args, &["columnDefinition"], ctx.consts)) {
                Some(def) => def,
                None => sql_type(p, &base, length.as_deref()),
            }
        }
        _ => p.ty.clone(),
    };
    if let Some(l) = &length {
        if base == "String" && !constraints.iter().any(|c| c.starts_with("max length")) {
            constraints.push(format!("max length {l}"));
        }
    }
    if p.has("Version") {
        constraints.push("optimistic lock version".into());
    }
    let default = if p.has("GeneratedValue") || p.has("AutoIncrement") {
        Some("generated".into())
    } else if p.has("CreationTimestamp") || p.has("CreatedDate") {
        Some("creation time".into())
    } else if pk {
        // `@Id val id: Long = 0` is how Kotlin spells "not yet persisted", not a schema default.
        None
    } else {
        p.init.as_deref().and_then(literal_default)
    };
    columns.retain(|c| c.name != name);
    columns.push(Column {
        name,
        type_name,
        primary_key: pk,
        nullable,
        unique,
        references: None,
        default,
        constraints,
        evidence: line_ref(path, name_line.unwrap_or(p.line)),
    });
}

// ---------------------------------------------------------------------------
// Exposed / Ktorm table objects.
// ---------------------------------------------------------------------------

/// A column built by a call chain: `varchar("email", 255).uniqueIndex()`.
struct ChainCol {
    func: String,
    /// Type arguments, for `enumerationByName<EntryState>("state", 16)`.
    targs: Vec<String>,
    args: Vec<String>,
    /// `(method, arguments)` applied after it.
    chain: Vec<(String, String)>,
}

impl ChainCol {
    /// The enum a column stores, from a type argument or a `::class` argument.
    fn enum_name(&self) -> Option<String> {
        if !matches!(self.func.as_str(), "enumeration" | "enumerationByName" | "customEnumeration") {
            return None;
        }
        self.args
            .iter()
            .find_map(|a| a.trim().strip_suffix("::class").map(simple))
            .or_else(|| self.targs.first().map(|t| simple(t)))
            .filter(|t| !t.is_empty())
    }
}

fn chain_col(init: &str) -> Option<ChainCol> {
    let open = init.find('(')?;
    let (func, targs) = generic(init[..open].trim());
    if func.is_empty() {
        return None;
    }
    let (args, mut rest) = balanced(&init[open..])?;
    let mut chain = vec![];
    while let Some(dot) = rest.find('.') {
        let after = &rest[dot + 1..];
        let name: String = after.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if name.is_empty() {
            break;
        }
        let tail = after[name.len()..].trim_start();
        match tail.strip_prefix('(').map(|_| ()).and(balanced(tail)) {
            Some((a, r)) => {
                chain.push((name, a));
                rest = r;
            }
            None => {
                chain.push((name, String::new()));
                rest = after;
            }
        }
    }
    Some(ChainCol { func, targs, args: split_args(&args), chain })
}

/// Content of the leading balanced `(...)` and the text after it.
fn balanced(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    if !s.starts_with('(') {
        return None;
    }
    let mut depth = 0i32;
    let mut quote = false;
    for (i, ch) in s.char_indices() {
        match ch {
            '"' => quote = !quote,
            '(' if !quote => depth += 1,
            ')' if !quote => {
                depth -= 1;
                if depth == 0 {
                    return Some((s[1..i].to_string(), &s[i + 1..]));
                }
            }
            _ => {}
        }
    }
    None
}

fn exposed_type(func: &str, args: &[String]) -> String {
    let n = |i: usize| args.get(i).map(|a| a.trim().to_string()).filter(|a| a.chars().all(|c| c.is_ascii_digit()));
    match func {
        "integer" | "int" | "uinteger" | "enumeration" if func != "enumeration" => "integer".into(),
        "long" | "ulong" => "bigint".into(),
        "short" | "ushort" => "smallint".into(),
        "byte" | "ubyte" => "tinyint".into(),
        "bool" | "boolean" => "boolean".into(),
        "double" => "double precision".into(),
        "float" => "real".into(),
        "decimal" => match (n(1), n(2)) {
            (Some(p), Some(s)) => format!("numeric({p},{s})"),
            (Some(p), None) => format!("numeric({p})"),
            _ => "numeric".into(),
        },
        "varchar" | "char" if func == "varchar" => format!("varchar({})", n(1).unwrap_or_else(|| "255".into())),
        "char" => "char(1)".into(),
        "text" | "largeText" | "mediumText" => "text".into(),
        "uuid" => "uuid".into(),
        "date" => "date".into(),
        "datetime" => "timestamp".into(),
        "timestamp" | "timestampWithTimeZone" => "timestamp with time zone".into(),
        "time" => "time".into(),
        "duration" => "interval".into(),
        "binary" | "bytes" => "bytea".into(),
        "blob" => "blob".into(),
        "json" | "jsonb" => "jsonb".into(),
        "enumeration" | "enumerationByName" | "customEnumeration" => "varchar".into(),
        "reference" | "optReference" => "integer".into(),
        _ => func.to_string(),
    }
}

/// `java.math.BigDecimal.ZERO` → `ZERO`; a literal stays as written.
fn chain_default(expr: &str) -> String {
    let t = expr.trim();
    if let Some(v) = literal_default(t) {
        return v;
    }
    let last = t.rsplit('.').next().unwrap_or(t);
    if !last.is_empty() && last.chars().all(|c| c.is_alphanumeric() || c == '_') {
        last.to_string()
    } else {
        t.trim_matches('"').to_string()
    }
}

/// Columns of an Exposed / Ktorm table object, with the enum each stores.
fn chain_columns(
    path: &str,
    c: &KClass,
    columns: &mut Vec<Column>,
    relations: &mut Vec<RawRelation>,
    enum_uses: &mut Vec<(String, String, Option<String>)>,
) {
    // `override val primaryKey = PrimaryKey(id, code)` names the key properties.
    let key_props: Vec<String> = c
        .props
        .iter()
        .find(|p| p.name == "primaryKey")
        .and_then(|p| p.init.as_deref())
        .and_then(chain_col)
        .map(|k| k.args.iter().map(|a| simple(a.trim())).collect())
        .unwrap_or_default();
    for p in &c.props {
        if p.name == "primaryKey" {
            continue;
        }
        let Some(col) = p.init.as_deref().and_then(chain_col) else { continue };
        let Some(name) = col.args.first().and_then(|a| resolve_str(a, &HashMap::new())) else { continue };
        let method = |m: &str| col.chain.iter().find(|(n, _)| n == m);
        let reference = match col.func.as_str() {
            "reference" | "optReference" => col.args.get(1).map(|t| simple(t.split('.').next().unwrap_or(t))),
            _ => method("references").map(|(_, a)| simple(a.split('.').next().unwrap_or(a))),
        }
        .filter(|t| !t.is_empty());
        let pk = key_props.contains(&p.name) || method("primaryKey").is_some() || method("entityId").is_some();
        let mut constraints = vec![];
        if let Some((_, a)) = method("check") {
            if !a.is_empty() {
                constraints.push(format!("check {a}"));
            }
        }
        columns.push(Column {
            name: name.clone(),
            type_name: exposed_type(&col.func, &col.args),
            primary_key: pk,
            nullable: method("nullable").is_some() || col.func == "optReference",
            unique: pk || method("uniqueIndex").is_some(),
            references: reference.as_ref().map(|t| format!("@{t}.?")),
            default: method("default")
                .map(|(_, a)| chain_default(a))
                .or_else(|| method("autoIncrement").map(|_| "generated".into()))
                .or_else(|| method("clientDefault").map(|_| "set by the application".into())),
            constraints,
            evidence: line_ref(path, p.line),
        });
        if let Some(e) = col.enum_name() {
            enum_uses.push((name.clone(), e, method("default").map(|(_, a)| chain_default(a))));
        }
        if let Some(t) = reference {
            relations.push(RawRelation {
                kind: "many-to-one".into(),
                target: format!("@{t}"),
                via: name,
                evidence: line_ref(path, p.line),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Parse.
// ---------------------------------------------------------------------------

/// Parses every Kotlin file once; `spring_units` selects Spring Boot's naming strategy.
pub fn parse(files: &[(String, String, String)], spring_units: &BTreeSet<String>) -> KotlinOutput {
    let mut classes: Vec<KClass> = vec![];
    for (i, (_, _, text)) in files.iter().enumerate() {
        parse_file(i, text, &mut classes);
    }
    let consts = constants(&classes);
    let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, c) in classes.iter().enumerate() {
        by_name.entry(c.name.clone()).or_default().push(i);
    }

    let mut out = KotlinOutput { entities: vec![], enums: vec![], enum_uses: vec![], model: KotlinModel::default() };
    let enum_names: HashSet<&str> = classes.iter().filter(|c| c.decl == Decl::Enum).map(|c| c.name.as_str()).collect();
    for c in classes.iter().filter(|c| c.decl == Decl::Enum && c.entries.len() >= 2) {
        out.enums.push(RawEnum {
            name: c.name.clone(),
            values: c.entries.iter().map(|n| (n.clone(), n.clone())).collect(),
            evidence: line_ref(&files[c.file].0, c.line),
            unit: Some(files[c.file].1.clone()),
            sql_type: false,
        });
    }

    for c in &classes {
        let path = &files[c.file].0;
        let text = &files[c.file].2;
        let unit = &files[c.file].1;
        let spring = spring_units.contains(unit);
        let base_of = |spec: &str| spec_parts(spec).0;
        let extends_base = |bases: &[&str]| c.supers.iter().any(|s| bases.contains(&base_of(s).as_str()));
        let exposed = c.decl == Decl::Object
            && text.contains("org.jetbrains.exposed")
            && c.supers.iter().any(|s| EXPOSED_TABLES.iter().any(|(b, _)| *b == base_of(s)));
        let ktorm = c.decl == Decl::Object
            && text.contains("org.ktorm")
            && c.supers.iter().any(|s| base_of(s) == "Table" || base_of(s) == "BaseTable");
        let kind = if c.has("Entity") {
            Some(Kind::Jpa)
        } else if c.has("Document") && text.contains("springframework.data.mongodb") {
            Some(Kind::Mongo)
        } else if c.has("MappedEntity") {
            Some(Kind::Micronaut)
        } else if c.has("MongoEntity")
            || extends_base(&["PanacheMongoEntity", "PanacheMongoEntityBase", "ReactivePanacheMongoEntity"])
        {
            Some(Kind::PanacheMongo)
        } else if c.has("Table") && text.contains("data.relational") {
            Some(Kind::Jdbc)
        } else if ktorm {
            Some(Kind::Ktorm)
        } else if exposed {
            Some(Kind::Exposed)
        } else {
            None
        };
        let Some(kind) = kind else { continue };
        match kind {
            Kind::Exposed | Kind::Ktorm if c.decl != Decl::Object => continue,
            Kind::Exposed | Kind::Ktorm => {}
            _ if c.decl != Decl::Class => continue,
            _ => {}
        }
        let snake = match kind {
            Kind::Jpa => spring,
            Kind::Jdbc | Kind::Micronaut => true,
            Kind::Mongo | Kind::PanacheMongo | Kind::Exposed | Kind::Ktorm => false,
        };
        let naming = |n: &str| if snake { snake_case(n) } else { n.to_string() };
        let spec = c.supers.iter().find(|s| {
            let b = base_of(s);
            EXPOSED_TABLES.iter().any(|(t, _)| *t == b) || b == "BaseTable"
        });
        let table = match kind {
            Kind::Jpa => c
                .ann("Table")
                .and_then(|a| str_arg(&a.args, &["name"], &consts))
                .or_else(|| c.ann("Entity").and_then(|a| str_arg(&a.args, &["name"], &consts)).map(|n| naming(&n)))
                .unwrap_or_else(|| naming(&c.name))
                .to_lowercase(),
            Kind::Jdbc => c
                .ann("Table")
                .and_then(|a| str_arg(&a.args, &["value", "name", ""], &consts))
                .unwrap_or_else(|| snake_case(&c.name))
                .to_lowercase(),
            Kind::Micronaut => c
                .ann("MappedEntity")
                .and_then(|a| str_arg(&a.args, &["value", ""], &consts))
                .unwrap_or_else(|| snake_case(&c.name))
                .to_lowercase(),
            Kind::Mongo => c
                .ann("Document")
                .and_then(|a| str_arg(&a.args, &["collection", "value", ""], &consts))
                .unwrap_or_else(|| uncapitalize(&c.name)),
            Kind::PanacheMongo => c
                .ann("MongoEntity")
                .and_then(|a| str_arg(&a.args, &["collection"], &consts))
                .unwrap_or_else(|| c.name.clone()),
            // Exposed and Ktorm name the table in the supertype call; both
            // default to the object's own name. `Table("\"quoted\"")` forces a
            // quoted identifier — the table itself is the name inside.
            Kind::Exposed | Kind::Ktorm => spec
                .map(|s| spec_parts(s).2)
                .and_then(|a| split_args(&a).first().and_then(|x| resolve_str(x, &consts)))
                .map(|n| n.trim_matches(|c| c == '"' || c == '\\' || c == '`').to_string())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| c.name.trim_end_matches("Table").to_string())
                .to_lowercase(),
        };
        if table.is_empty() {
            continue;
        }

        let mut columns: Vec<Column> = vec![];
        let mut relations: Vec<RawRelation> = vec![];
        if matches!(kind, Kind::Exposed | Kind::Ktorm) {
            // `IntIdTable("posts")` brings a generated key of its own.
            if let Some(ty) = spec
                .map(|s| base_of(s))
                .and_then(|b| EXPOSED_TABLES.iter().find(|(t, _)| *t == b).and_then(|(_, pk)| *pk))
            {
                columns.push(Column {
                    name: "id".into(),
                    type_name: ty.into(),
                    primary_key: true,
                    nullable: false,
                    unique: true,
                    references: None,
                    default: Some("generated".into()),
                    constraints: vec![],
                    evidence: line_ref(path, c.line),
                });
            }
            let mut stored_enums = vec![];
            chain_columns(path, c, &mut columns, &mut relations, &mut stored_enums);
            if columns.is_empty() {
                continue;
            }
            // A composite key is not unique column by column.
            if columns.iter().filter(|x| x.primary_key).count() > 1 {
                for x in columns.iter_mut().filter(|x| x.primary_key) {
                    x.unique = false;
                }
            }
            for (column, enum_name, default) in
                stored_enums.into_iter().filter(|(_, e, _)| enum_names.contains(e.as_str()))
            {
                out.enum_uses.push(RawEnumUse { table: table.clone(), column, enum_name, default });
            }
            out.model.tables.insert(c.name.clone(), table.clone());
            out.entities.push(RawEntity {
                name: c.name.clone(),
                table,
                source: kind.label().into(),
                unit: Some(unit.clone()),
                columns,
                relations,
                evidence: line_ref(path, c.line),
            });
            continue;
        }

        let unique_sets: Vec<String> =
            c.ann("Table").map(|a| single_unique_columns(&a.args, &consts)).unwrap_or_default();
        // Mapped superclasses contribute their properties, base first.
        let mut chain: Vec<&KClass> = vec![];
        let mut cur = Some(c);
        while let Some(k) = cur {
            if chain.len() > 6 || chain.iter().any(|x| std::ptr::eq(*x, k)) {
                break;
            }
            chain.push(k);
            let ctx = FieldCtx {
                kind,
                snake,
                consts: &consts,
                owner_table: &table,
                classes: &classes,
                by_name: &by_name,
                files,
                file: k.file,
            };
            cur = k.supers.first().and_then(|s| ctx.find(&base_of(s), k.file)).filter(|p| p.decl == Decl::Class);
        }
        chain.reverse();
        if extends_base(&["PanacheEntity", "PanacheMongoEntity", "ReactivePanacheMongoEntity"]) {
            columns.push(Column {
                name: "id".into(),
                type_name: if kind == Kind::PanacheMongo { "ObjectId".into() } else { "bigint".into() },
                primary_key: true,
                nullable: false,
                unique: true,
                references: None,
                default: Some("generated".into()),
                constraints: vec![],
                evidence: line_ref(path, c.line),
            });
        }
        for k in &chain {
            let kpath = files[k.file].0.clone();
            let ctx = FieldCtx {
                kind,
                snake,
                consts: &consts,
                owner_table: &table,
                classes: &classes,
                by_name: &by_name,
                files,
                file: k.file,
            };
            for p in &k.props {
                prop_columns(&ctx, &kpath, p, None, &mut columns, &mut relations, &unique_sets, 0);
            }
        }
        if columns.is_empty() && relations.is_empty() {
            continue;
        }
        let pks = columns.iter().filter(|x| x.primary_key).count();
        if pks > 1 {
            for x in columns.iter_mut().filter(|x| x.primary_key) {
                x.unique = false;
            }
        }
        let ctx = FieldCtx {
            kind,
            snake,
            consts: &consts,
            owner_table: &table,
            classes: &classes,
            by_name: &by_name,
            files,
            file: c.file,
        };
        for k in &chain {
            for p in k.props.iter().filter(|p| p.property && !p.computed && !p.has("Transient")) {
                let ty = simple(&p.ty);
                if enum_names.contains(ty.as_str()) {
                    let column = column_name(&ctx, p).0;
                    let default = p.init.as_deref().map(|i| i.rsplit('.').next().unwrap_or(i).to_string());
                    out.enum_uses.push(RawEnumUse { table: table.clone(), column, enum_name: ty, default });
                }
            }
        }
        out.model.entities.insert(c.name.clone());
        if kind == Kind::PanacheMongo || extends_base(PANACHE_BASES) {
            out.model.active_record.insert(c.name.clone());
        }
        out.entities.push(RawEntity {
            name: c.name.clone(),
            table,
            source: kind.label().into(),
            unit: Some(unit.clone()),
            columns,
            relations,
            evidence: line_ref(path, c.line),
        });
    }

    // References name the target's primary key, and an Exposed reference is
    // typed by the column it points at.
    let pk_of: HashMap<String, (String, String)> = out
        .entities
        .iter()
        .map(|e| {
            let pk = e.columns.iter().find(|c| c.primary_key);
            (
                e.name.clone(),
                (
                    pk.map(|c| c.name.clone()).unwrap_or_else(|| "id".into()),
                    pk.map(|c| c.type_name.clone()).unwrap_or_default(),
                ),
            )
        })
        .collect();
    for e in &mut out.entities {
        let exposed = e.source == "exposed";
        for col in &mut e.columns {
            let Some(r) = col.references.clone() else { continue };
            let Some(t) = r.strip_prefix('@').and_then(|t| t.strip_suffix(".?")) else { continue };
            let (name, ty) = pk_of.get(t).cloned().unwrap_or_else(|| ("id".into(), String::new()));
            col.references = Some(format!("@{t}.{name}"));
            if exposed && !ty.is_empty() {
                col.type_name = ty;
            }
        }
    }

    // Repositories: `interface OrderRepository : JpaRepository<Order, Long>`.
    let mut repos: HashMap<String, (String, HashMap<String, bool>)> = HashMap::new();
    for c in classes.iter().filter(|c| matches!(c.decl, Decl::Interface | Decl::Class)) {
        for s in &c.supers {
            let (base, targs, _) = spec_parts(s);
            if REPO_BASES.contains(&base.as_str()) {
                if let Some(entity) = targs.first().filter(|a| !a.is_empty() && *a != "T") {
                    repos.insert(c.name.clone(), (entity.clone(), repo_methods(c)));
                }
            }
        }
    }
    // Interfaces extending a custom repository inherit its entity.
    for _ in 0..3 {
        let found: Vec<(String, String, HashMap<String, bool>)> = classes
            .iter()
            .filter(|c| c.decl == Decl::Interface && !repos.contains_key(&c.name))
            .filter_map(|c| {
                let (entity, _) = c.supers.iter().find_map(|s| repos.get(&spec_parts(s).0).cloned())?;
                Some((c.name.clone(), entity, repo_methods(c)))
            })
            .collect();
        for (name, entity, methods) in found {
            repos.insert(name, (entity, methods));
        }
    }
    out.model.repos = repos;

    // Exposed DAO: `companion object : IntEntityClass<UserEntity>(Users)` binds
    // the entity class to a table object, so calls on it are calls on the table.
    for c in classes.iter().filter(|c| c.decl == Decl::Class) {
        let Some(at) = c.body.find("EntityClass") else { continue };
        let after = &c.body[at..];
        let Some(open) = after.find('(') else { continue };
        let Some((args, _)) = balanced(&after[open..]) else { continue };
        let Some(t) = split_args(&args).first().map(|a| simple(a.trim())) else { continue };
        if let Some(table) = out.model.tables.get(&t) {
            out.model.dao.insert(c.name.clone(), table.clone());
        }
    }

    out
}

/// Custom repository methods: `@Query` / `@Modifying` / a derived write name.
fn repo_methods(c: &KClass) -> HashMap<String, bool> {
    let mut out = HashMap::new();
    for m in &c.funs {
        let q = m.anns.iter().find(|a| a.name == "Query" || a.name == "NativeQuery");
        let modifying = m.anns.iter().any(|a| a.name == "Modifying");
        let write = modifying
            || q.is_some_and(|a| {
                let t = a.args.to_lowercase();
                let t = t.split_once('"').map(|(_, r)| r).unwrap_or(&t).trim_start().to_string();
                t.starts_with("update") || t.starts_with("delete") || t.starts_with("insert")
            })
            || write_verb(&m.name);
        out.insert(m.name.clone(), write);
    }
    out
}

// ---------------------------------------------------------------------------
// Access.
// ---------------------------------------------------------------------------

fn write_verb(method: &str) -> bool {
    ["save", "insert", "update", "delete", "remove", "persist", "merge", "upsert", "store", "create", "replace"]
        .iter()
        .any(|v| method.starts_with(v))
}

fn read_verb(method: &str) -> bool {
    ["find", "get", "read", "query", "count", "exists", "search", "stream", "list", "load", "fetch", "select", "all"]
        .iter()
        .any(|v| method.starts_with(v))
}

/// Exposed / Ktorm statements on a table object.
fn table_write(method: &str) -> bool {
    method.starts_with("insert")
        || method.starts_with("update")
        || method.starts_with("delete")
        || matches!(method, "replace" | "upsert" | "batchInsert" | "batchReplace" | "batchUpsert" | "new")
}

fn table_read(method: &str) -> bool {
    matches!(
        method,
        "select"
            | "selectAll"
            | "selectBatched"
            | "slice"
            | "join"
            | "innerJoin"
            | "leftJoin"
            | "rightJoin"
            | "crossJoin"
            | "exists"
            | "count"
            | "all"
            | "find"
            | "findById"
            | "findSingleByAndUpdate"
    )
}

fn identifiers(text: &str) -> HashSet<&str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|w| !w.is_empty()).collect()
}

/// Variables declared `name: Ty` (constructor properties, parameters, locals)
/// and `name = Ty(`.
fn vars_of_type(text: &str, ty: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let b = text.as_bytes();
    let mut from = 0;
    while let Some(p) = text[from..].find(ty).map(|x| from + x) {
        from = p + ty.len();
        if b.get(p + ty.len()).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_') {
            continue;
        }
        // `: Ty`, possibly qualified (`: com.acme.Ty`) — walk back over the name.
        let mut i = p;
        while i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_' || b[i - 1] == b'.') {
            i -= 1;
        }
        let before = text[..i].trim_end();
        let Some(decl) = before.strip_suffix(':') else { continue };
        let name: String =
            decl.trim_end().chars().rev().take_while(|c| c.is_alphanumeric() || *c == '_').collect::<String>();
        let name: String = name.chars().rev().collect();
        if name.is_empty() || matches!(name.as_str(), "val" | "var" | "fun" | "return") {
            continue;
        }
        out.insert(name);
    }
    // `val order = Order(`
    let pat = format!("= {ty}(");
    for l in text.lines() {
        if let Some(p) = l.find(&pat) {
            let name: String = l[..p]
                .trim_end()
                .chars()
                .rev()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            if !name.is_empty() {
                out.insert(name);
            }
        }
    }
    out
}

const MANAGERS: &[&str] = &[
    "EntityManager",
    "Session",
    "StatelessSession",
    "MongoTemplate",
    "ReactiveMongoTemplate",
    "MongoOperations",
    "ReactiveMongoOperations",
    "R2dbcEntityTemplate",
    "JdbcAggregateTemplate",
];

fn manager_write(method: &str) -> bool {
    matches!(
        method,
        "persist"
            | "merge"
            | "remove"
            | "save"
            | "insert"
            | "update"
            | "upsert"
            | "delete"
            | "updateFirst"
            | "updateMulti"
    )
}

fn manager_read(method: &str) -> bool {
    matches!(method, "find" | "getReference" | "findById" | "findAll" | "findOne" | "exists" | "count" | "select")
}

/// Entity names in a JPQL string: `from Order o`, `update Order`, `join o.items`.
fn jpql_entities(q: &str) -> (Vec<String>, bool) {
    let lower = q.trim_start().to_lowercase();
    let write = lower.starts_with("update") || lower.starts_with("delete");
    let words: Vec<&str> = q.split_whitespace().collect();
    let mut out = vec![];
    for (i, w) in words.iter().enumerate() {
        if matches!(w.to_lowercase().as_str(), "from" | "update" | "join") {
            if let Some(n) = words.get(i + 1) {
                let n: String = n.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if n.starts_with(|c: char| c.is_uppercase()) {
                    out.push(n);
                }
            }
        }
    }
    (out, write)
}

/// Reads and writes of entities and table objects performed by Kotlin code.
pub fn access(model: &KotlinModel, sources: &[StateSrc]) -> Vec<Access> {
    let mut out = vec![];
    for (si, src) in sources.iter().enumerate() {
        if !src.path.ends_with(".kt") || src.migration {
            continue;
        }
        let words = identifiers(&src.text);
        let lines: Vec<&str> = src.text.lines().collect();
        let mut repo_vars: HashMap<String, (String, &HashMap<String, bool>)> = HashMap::new();
        for (repo, (entity, methods)) in &model.repos {
            if words.contains(repo.as_str()) {
                for v in vars_of_type(&src.text, repo) {
                    repo_vars.insert(v, (entity.clone(), methods));
                }
            }
        }
        let mut entity_vars: HashMap<String, String> = HashMap::new();
        for e in &model.entities {
            if words.contains(e.as_str()) {
                for v in vars_of_type(&src.text, e) {
                    entity_vars.insert(v, e.clone());
                }
            }
        }
        let mut manager_vars: BTreeSet<String> = BTreeSet::new();
        for t in MANAGERS {
            if words.contains(t) {
                manager_vars.extend(vars_of_type(&src.text, t));
            }
        }
        for call in &src.facts.calls {
            let callee = call.callee.trim_start_matches("this.");
            let Some((recv, method)) = callee.rsplit_once('.') else { continue };
            if recv.contains(['(', ')', ' ']) || method != call.name {
                continue;
            }
            if let Some((entity, methods)) = repo_vars.get(recv) {
                let custom = methods.get(method).copied();
                let write = custom.unwrap_or_else(|| write_verb(method));
                if write || read_verb(method) || custom.is_some() {
                    out.push(Access { name: entity.clone(), write, src: si, line: call.line, note: None });
                }
                continue;
            }
            // `Users.insert { … }` / `Users.selectAll()`, and the DAO class over it.
            if let Some(table) = model.tables.get(recv).or_else(|| model.dao.get(recv)) {
                let write = table_write(method);
                if write || table_read(method) {
                    let note = model.dao.contains_key(recv).then(|| format!("through `{recv}`"));
                    out.push(Access { name: format!("#{table}"), write, src: si, line: call.line, note });
                }
                continue;
            }
            if model.active_record.contains(recv) && (write_verb(method) || read_verb(method)) {
                out.push(Access {
                    name: recv.to_string(),
                    write: write_verb(method),
                    src: si,
                    line: call.line,
                    note: None,
                });
                continue;
            }
            if let Some(e) = entity_vars.get(recv) {
                if model.active_record.contains(e)
                    && matches!(method, "persist" | "delete" | "update" | "persistOrUpdate")
                {
                    out.push(Access { name: e.clone(), write: true, src: si, line: call.line, note: None });
                }
                continue;
            }
            if manager_vars.contains(recv) {
                if matches!(method, "createQuery" | "createNativeQuery") {
                    if let Some(q) = src.facts.strings.iter().find(|s| s.line == call.line || s.line == call.line + 1) {
                        let (names, write) = jpql_entities(&q.value);
                        for n in names.into_iter().filter(|n| model.entities.contains(n)) {
                            out.push(Access { name: n, write, src: si, line: call.line, note: None });
                        }
                    }
                    continue;
                }
                let write = manager_write(method);
                if !(write || manager_read(method)) {
                    continue;
                }
                let line_text = lines.get(call.line as usize - 1).copied().unwrap_or("");
                let args =
                    line_text.find(&format!(".{method}(")).and_then(|p| paren_args(&line_text[p..])).unwrap_or("");
                let parts = split_args(args);
                // `find(Order::class.java, id)` / `persist(order)` / `find<Order>(id)`
                let named = parts
                    .iter()
                    .find_map(|a| a.trim().strip_suffix("::class.java").or_else(|| a.trim().strip_suffix("::class")))
                    .map(simple)
                    .or_else(|| {
                        line_text
                            .split_once(&format!(".{method}<"))
                            .and_then(|(_, r)| r.split_once('>'))
                            .map(|(t, _)| simple(t))
                    })
                    .or_else(|| parts.first().and_then(|a| entity_vars.get(a.trim()).cloned()));
                if let Some(n) = named.filter(|n| model.entities.contains(n)) {
                    out.push(Access { name: n, write, src: si, line: call.line, note: None });
                }
            }
        }
        // Ktorm reaches tables through the database object: `database.from(Accounts)`.
        for call in src.facts.calls.iter().filter(|c| matches!(c.name.as_str(), "from" | "sequenceOf")) {
            let line_text = lines.get(call.line as usize - 1).copied().unwrap_or("");
            let Some(args) = line_text.find(&format!(".{}(", call.name)).and_then(|p| paren_args(&line_text[p..]))
            else {
                continue;
            };
            let arg = simple(args.split(',').next().unwrap_or("").trim());
            if let Some(table) = model.tables.get(&arg) {
                let write = ["insert", "update", "delete", "add", "removeIf"]
                    .iter()
                    .any(|v| line_text.contains(&format!(".{v}(")))
                    || line_text.contains(".add(");
                out.push(Access { name: format!("#{table}"), write, src: si, line: call.line, note: None });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(text: &str) -> KotlinOutput {
        let files = vec![("src/X.kt".to_string(), "app".to_string(), text.to_string())];
        parse(&files, &BTreeSet::from(["app".to_string()]))
    }

    #[test]
    fn maps_a_jpa_entity_written_in_kotlin() {
        let out = one(r#"package x
import jakarta.persistence.*
import org.springframework.data.jpa.repository.JpaRepository

@Entity
@Table(name = "orders")
class Order(
    @Id @GeneratedValue val id: Long = 0,
    @Column(name = "customer_name", nullable = false, length = 80) val customerName: String = "",
    @Enumerated(EnumType.STRING) var status: OrderStatus = OrderStatus.NEW,
    @ManyToOne @JoinColumn(name = "customer_id") val customer: Customer? = null,
    val note: String? = null,
)

enum class OrderStatus { NEW, PAID }

interface OrderRepository : JpaRepository<Order, Long>
"#);
        let e = out.entities.iter().find(|e| e.table == "orders").expect("orders entity");
        assert_eq!(e.source, "jpa");
        let col = |n: &str| e.columns.iter().find(|c| c.name == n).unwrap_or_else(|| panic!("no column {n}"));
        assert!(col("id").primary_key && col("id").default.as_deref() == Some("generated"));
        assert_eq!(col("customer_name").type_name, "varchar(80)");
        assert!(!col("customer_name").nullable);
        assert_eq!(col("status").type_name, "varchar");
        // Kotlin's `?` is the nullability statement.
        assert!(col("note").nullable && !col("customer_name").nullable);
        assert_eq!(col("customer_id").references.as_deref(), Some("@Customer.id"));
        assert_eq!(e.relations[0].kind, "many-to-one");
        assert_eq!(out.enums[0].values.len(), 2);
        assert_eq!(out.enum_uses[0].column, "status");
        assert_eq!(out.enum_uses[0].default.as_deref(), Some("NEW"));
    }

    #[test]
    fn maps_exposed_table_objects() {
        let out = one(r#"package x
import org.jetbrains.exposed.sql.Table
import org.jetbrains.exposed.dao.id.IntIdTable

object Users : Table("users") {
    val id = integer("id").autoIncrement()
    val email = varchar("email", 255).uniqueIndex()
    val bio = text("bio").nullable()
    override val primaryKey = PrimaryKey(id)
}

object Posts : IntIdTable("posts") {
    val title = varchar("title", 120)
    val author = reference("author_id", Users)
}
"#);
        let users = out.entities.iter().find(|e| e.table == "users").expect("users");
        assert_eq!(users.source, "exposed");
        let col = |e: &RawEntity, n: &str| e.columns.iter().find(|c| c.name == n).cloned().unwrap();
        assert!(col(users, "id").primary_key);
        assert_eq!(col(users, "email").type_name, "varchar(255)");
        assert!(col(users, "email").unique);
        assert!(col(users, "bio").nullable);
        let posts = out.entities.iter().find(|e| e.table == "posts").expect("posts");
        assert!(col(posts, "id").primary_key && col(posts, "id").type_name == "integer");
        // The reference takes the type of the key it points at.
        assert_eq!(col(posts, "author_id").references.as_deref(), Some("@Users.id"));
        assert_eq!(col(posts, "author_id").type_name, "integer");
    }

    #[test]
    fn reads_repositories_and_mongo_documents() {
        let out = one(r#"package x
import org.springframework.data.mongodb.core.mapping.Document
import org.springframework.data.mongodb.repository.MongoRepository
import org.springframework.data.jpa.repository.Modifying
import org.springframework.data.jpa.repository.Query

@Document(collection = "audit_log")
data class AuditEvent(@Id val id: String? = null, val actor: String)

interface AuditRepository : MongoRepository<AuditEvent, String> {
    @Modifying
    @Query("update AuditEvent a set a.actor = :x")
    fun rename(x: String): Int
}
"#);
        let e = out.entities.iter().find(|e| e.table == "audit_log").expect("audit_log");
        assert_eq!(e.source, "spring-data-mongodb");
        assert!(e.columns.iter().any(|c| c.name == "actor" && !c.nullable));
        let (entity, methods) = out.model.repos.get("AuditRepository").expect("repo");
        assert_eq!(entity, "AuditEvent");
        assert_eq!(methods.get("rename"), Some(&true));
    }

    #[test]
    fn maps_ktorm_table_objects() {
        let out = one(r#"package x
import org.ktorm.schema.Table
import org.ktorm.schema.int
import org.ktorm.schema.varchar

object Accounts : Table<Nothing>("accounts") {
    val id = int("id").primaryKey()
    val name = varchar("name")
}
"#);
        let e = out.entities.iter().find(|e| e.table == "accounts").expect("accounts");
        assert_eq!(e.source, "ktorm");
        assert!(e.columns.iter().any(|c| c.name == "id" && c.primary_key && c.type_name == "integer"));
        assert!(e.columns.iter().any(|c| c.name == "name" && c.type_name == "varchar(255)"));
    }

    #[test]
    fn survives_unparsable_input() {
        let out = one("class ( { @Entity @Table( val");
        assert!(out.entities.is_empty());
    }
}
