//! Access performed by a shared base class on behalf of its subclasses.
//!
//! Large JVM codebases put persistence in an abstract DAO:
//!
//! ```java
//! public abstract class JpaAbstractDao<E extends BaseEntity<D>, D> {
//!     protected abstract JpaRepository<E, UUID> getRepository();
//!     public void removeById(UUID id) { getRepository().deleteById(id); }
//!     private E create(E entity) { getEntityManager().persist(entity); }
//! }
//! public class JpaDeviceDao extends JpaAbstractDao<DeviceEntity, Device> {
//!     @Autowired private DeviceRepository deviceRepository;
//!     protected JpaRepository<DeviceEntity, UUID> getRepository() { return deviceRepository; }
//! }
//! ```
//!
//! Read literally, `device` is never written. The write is real; which entity
//! it writes is visible only through the subclass, in two statically checkable
//! ways: what the accessor override returns, and what the `extends` clause
//! binds each type parameter to. Both are resolved here, and nothing is
//! attributed unless one of them names a concrete entity.

use std::collections::{BTreeMap, HashMap};

use super::java_syntax::{generic, simple, JClass, JMethod};

/// How a base class reaches the entity it operates on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Via {
    /// `getRepository().save(…)` — an abstract accessor the subclass overrides.
    Accessor(String),
    /// `entityManager.persist(entity)` where `entity` is declared with a type parameter.
    TypeParam(String),
}

/// What one subclass binds the base's persistence to.
#[derive(Debug, Clone)]
pub struct Binding {
    /// Concrete subclass (`JpaDeviceDao`).
    pub sub: String,
    /// Entity class per accessor method name (`getRepository` → `DeviceEntity`).
    pub by_accessor: BTreeMap<String, String>,
    /// Entity class per base type parameter (`E` → `DeviceEntity`).
    pub by_type_param: BTreeMap<String, String>,
}

/// An abstract base class whose persistence calls belong to its subclasses.
#[derive(Debug, Clone)]
pub struct DaoBase {
    /// Path of the file declaring the base.
    pub path: String,
    /// Line range of the declaration.
    pub lines: (u32, u32),
    /// Abstract accessors returning a repository type → the type parameter of
    /// that repository (`getRepository` → `E`), when it has one.
    pub accessors: BTreeMap<String, Option<String>>,
    /// Locals assigned from an accessor (`var repo = getRepository();`).
    pub accessor_vars: BTreeMap<String, String>,
    /// Variables and parameters declared with one of the base's type parameters.
    pub type_param_vars: BTreeMap<String, String>,
    pub bindings: Vec<Binding>,
}

impl DaoBase {
    /// The entity each subclass binds this call site to, with the subclass
    /// named so the citation says whose data it is.
    pub fn resolve(&self, via: &Via) -> Vec<(String, &str)> {
        let mut out = Vec::new();
        for b in &self.bindings {
            let entity = match via {
                // An override names the repository outright; otherwise the
                // accessor's own type parameter is bound by `extends`.
                Via::Accessor(m) => b
                    .by_accessor
                    .get(m)
                    .or_else(|| self.accessors.get(m).and_then(|p| p.as_ref()).and_then(|p| b.by_type_param.get(p))),
                Via::TypeParam(p) => b.by_type_param.get(p),
            };
            if let Some(e) = entity {
                out.push((e.clone(), b.sub.as_str()));
            }
        }
        out
    }

    /// What a call receiver refers to, when the base delegates it.
    pub fn via_of(&self, recv: &str) -> Option<Via> {
        if let Some(name) = recv.strip_suffix("()") {
            if self.accessors.contains_key(name) {
                return Some(Via::Accessor(name.to_string()));
            }
        }
        self.accessor_vars.get(recv).map(|m| Via::Accessor(m.clone()))
    }

    /// The type parameter a variable was declared with (`E entity` → `E`).
    pub fn type_param_of(&self, var: &str) -> Option<&str> {
        self.type_param_vars.get(var).map(String::as_str)
    }

    pub fn contains(&self, line: u32) -> bool {
        line >= self.lines.0 && line <= self.lines.1
    }
}

/// Everything the resolver needs from the rest of the Java model.
pub struct Ctx<'a> {
    /// Code view of each parsed file, indexed as `JClass::file` (string
    /// contents blanked, byte offsets aligned with `JClass::body`).
    pub code: &'a [String],
    /// Path of each parsed file.
    pub paths: &'a [String],
    /// Repository type → the entity class it manages.
    pub repo_entity: &'a dyn Fn(&str) -> Option<String>,
    /// Is this a persistent entity class?
    pub is_entity: &'a dyn Fn(&str) -> bool,
}

const REPOSITORY_HINTS: &[&str] = &["Repository", "Dao", "Store"];

fn repository_type(ret: &str) -> Option<Vec<String>> {
    let (base, args) = generic(ret);
    REPOSITORY_HINTS.iter().any(|h| base.ends_with(h)).then_some(args)
}

/// Base classes whose persistence work is attributable only through a subclass.
pub fn dao_bases(classes: &[JClass], ctx: &Ctx) -> Vec<DaoBase> {
    let by_name: HashMap<&str, &JClass> = classes.iter().map(|c| (c.name.as_str(), c)).collect();
    let mut out = Vec::new();
    for base in classes.iter().filter(|c| c.kind == "class") {
        let accessors: BTreeMap<String, Option<String>> = base
            .methods
            .iter()
            .filter(|m| m.is_abstract())
            .filter_map(|m| {
                let args = repository_type(&m.ret)?;
                let param = args.into_iter().find(|a| base.type_params.contains(a));
                Some((m.name.clone(), param))
            })
            .collect();
        if accessors.is_empty() && base.type_params.is_empty() {
            continue;
        }
        let Some(code) = ctx.code.get(base.file) else { continue };
        let Some(body) = code.get(base.body.0..base.body.1) else { continue };
        let bindings = subclass_bindings(base, classes, &by_name, ctx);
        if bindings.is_empty() {
            continue;
        }
        let accessor_vars = assigned_from_accessor(body, &accessors);
        let type_param_vars = vars_of_type_params(body, &base.type_params);
        if accessors.is_empty() && type_param_vars.is_empty() {
            continue;
        }
        out.push(DaoBase {
            path: ctx.paths.get(base.file).cloned().unwrap_or_default(),
            lines: (base.line, line_at(code, base.body.1)),
            accessors,
            accessor_vars,
            type_param_vars,
            bindings,
        });
    }
    out
}

fn line_at(text: &str, offset: usize) -> u32 {
    text[..offset.min(text.len())].bytes().filter(|b| *b == b'\n').count() as u32 + 1
}

fn subclass_bindings(base: &JClass, classes: &[JClass], by_name: &HashMap<&str, &JClass>, ctx: &Ctx) -> Vec<Binding> {
    let mut out = Vec::new();
    for sub in classes.iter().filter(|c| c.kind == "class" && c.name != base.name) {
        let Some((parent, args)) = sub.extends.first().map(|e| generic(e)) else { continue };
        if parent != base.name {
            continue;
        }
        let by_type_param = base
            .type_params
            .iter()
            .enumerate()
            .filter_map(|(i, param)| {
                let arg = simple(args.get(i)?);
                (ctx.is_entity)(&arg).then(|| (param.clone(), arg))
            })
            .collect();
        let by_accessor = sub
            .methods
            .iter()
            .filter_map(|m| Some((m.name.clone(), returned_repository_entity(sub, m, by_name, ctx)?)))
            .collect::<BTreeMap<_, _>>();
        let binding = Binding { sub: sub.name.clone(), by_accessor, by_type_param };
        if !binding.by_accessor.is_empty() || !binding.by_type_param.is_empty() {
            out.push(binding);
        }
    }
    out
}

/// `protected JpaRepository<DeviceEntity, UUID> getRepository() { return deviceRepository; }`
/// → the entity of `deviceRepository`'s declared type.
fn returned_repository_entity(
    sub: &JClass,
    method: &JMethod,
    by_name: &HashMap<&str, &JClass>,
    ctx: &Ctx,
) -> Option<String> {
    repository_type(&method.ret)?;
    let (start, end) = method.body?;
    let body = ctx.code.get(sub.file)?.get(start..end)?;
    let returned = body.split("return ").nth(1)?.split([';', '.', '(']).next()?.trim().trim_start_matches("this.");
    if returned.is_empty() {
        return None;
    }
    let mut owner = Some(sub);
    // The field may be declared on a class this one extends.
    for _ in 0..4 {
        let c = owner?;
        if let Some(f) = c.fields.iter().find(|f| f.name == returned) {
            return (ctx.repo_entity)(&simple(&f.ty));
        }
        owner = c.extends.first().map(|e| generic(e).0).and_then(|n| by_name.get(n.as_str()).copied());
    }
    None
}

/// `JpaRepository<E, UUID> repository = getRepository();` → `repository` → `getRepository`.
fn assigned_from_accessor(body: &str, accessors: &BTreeMap<String, Option<String>>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in body.lines() {
        let Some((lhs, rhs)) = line.split_once('=') else { continue };
        let rhs = rhs.trim();
        let Some(acc) = accessors.keys().find(|a| rhs.starts_with(&format!("{a}()"))) else { continue };
        let var = trailing_identifier(lhs);
        if !var.is_empty() {
            out.insert(var, acc.clone());
        }
    }
    out
}

fn trailing_identifier(s: &str) -> String {
    let t = s.trim_end();
    let start = t.rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).map(|i| i + 1).unwrap_or(0);
    t[start..].to_string()
}

/// Variables and parameters declared with a type parameter: `E entity`,
/// `List<E> entities`, `Optional<E> found`.
fn vars_of_type_params(body: &str, params: &[String]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let bytes = body.as_bytes();
    for param in params {
        let mut from = 0;
        while let Some(p) = body[from..].find(param.as_str()) {
            let at = from + p;
            from = at + param.len();
            if at > 0 && is_ident_byte(bytes[at - 1]) {
                continue;
            }
            // `E entity`, or a single-argument generic holding it (`List<E> items`).
            let after = body[from..].strip_prefix('>').unwrap_or(&body[from..]);
            let Some(next) = after.strip_prefix(' ') else { continue };
            let var: String = next.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            let tail = next[var.len()..].trim_start();
            let declares = tail.starts_with(['=', ';', ',', ')']) || tail.is_empty();
            if !var.is_empty() && declares && var.starts_with(|c: char| c.is_lowercase()) {
                out.insert(var, param.clone());
            }
        }
    }
    out
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::java_syntax::{new_src, parse_file};

    fn model(text: &str) -> (Vec<JClass>, Vec<String>) {
        let src = new_src("Dao.java", text);
        let mut classes = vec![];
        parse_file(0, &src, &mut classes);
        (classes, vec![src.code.clone()])
    }

    const SRC: &str = r#"
public abstract class JpaAbstractDao<E extends BaseEntity<D>, D> implements Dao<D> {
    protected abstract JpaRepository<E, UUID> getRepository();

    public void removeById(UUID id) {
        JpaRepository<E, UUID> repository = getRepository();
        repository.deleteById(id);
    }

    public D findById(UUID key) {
        return DaoUtil.getData(getRepository().findById(key));
    }

    private E create(E entity) {
        getEntityManager().persist(entity);
        return entity;
    }
}

public class JpaDeviceDao extends JpaAbstractDao<DeviceEntity, Device> implements DeviceDao {
    @Autowired
    private DeviceRepository deviceRepository;
    @Autowired
    private DeviceProfileRepository deviceProfileRepository;

    @Override
    protected JpaRepository<DeviceEntity, UUID> getRepository() {
        return deviceRepository;
    }
}

public class JpaAssetDao extends JpaAbstractDao<AssetEntity, Asset> {
    @Autowired
    private AssetRepository assetRepository;

    @Override
    protected JpaRepository<AssetEntity, UUID> getRepository() {
        return assetRepository;
    }
}
"#;

    fn bases(classes: &[JClass], code: &[String]) -> Vec<DaoBase> {
        let repo_entity = |r: &str| match r {
            "DeviceRepository" => Some("DeviceEntity".to_string()),
            "AssetRepository" => Some("AssetEntity".to_string()),
            "DeviceProfileRepository" => Some("DeviceProfileEntity".to_string()),
            _ => None,
        };
        let is_entity = |e: &str| matches!(e, "DeviceEntity" | "AssetEntity" | "DeviceProfileEntity");
        dao_bases(
            classes,
            &Ctx { code, paths: &["Dao.java".to_string()], repo_entity: &repo_entity, is_entity: &is_entity },
        )
    }

    #[test]
    fn accessor_overrides_and_type_arguments_bind_the_entity() {
        let (classes, code) = model(SRC);
        let bases = bases(&classes, &code);
        assert_eq!(bases.len(), 1, "{:?}", bases.iter().map(|b| &b.path).collect::<Vec<_>>());
        let base = &bases[0];
        assert_eq!(
            base.bindings.iter().map(|b| b.sub.as_str()).collect::<Vec<_>>(),
            vec!["JpaDeviceDao", "JpaAssetDao"]
        );
        assert_eq!(base.accessors.get("getRepository"), Some(&Some("E".to_string())), "accessor is parameterized by E");
        assert_eq!(base.accessor_vars.get("repository").map(String::as_str), Some("getRepository"));
        assert_eq!(base.type_param_vars.get("entity").map(String::as_str), Some("E"));

        // `getRepository().findById(…)` and `repository.deleteById(…)`.
        let via = base.via_of("getRepository()").expect("accessor receiver");
        let mut resolved = base.resolve(&via);
        resolved.sort();
        assert_eq!(
            resolved,
            vec![("AssetEntity".to_string(), "JpaAssetDao"), ("DeviceEntity".to_string(), "JpaDeviceDao")]
        );
        assert_eq!(base.via_of("repository"), Some(Via::Accessor("getRepository".into())));

        // `getEntityManager().persist(entity)` where `entity` is an `E`.
        let param = base.type_param_of("entity").expect("E-typed variable");
        let entities: Vec<String> =
            base.resolve(&Via::TypeParam(param.to_string())).into_iter().map(|(e, _)| e).collect();
        assert_eq!(entities, vec!["DeviceEntity".to_string(), "AssetEntity".to_string()]);
        assert!(base.via_of("deviceProfileRepository").is_none(), "only the overridden accessor binds");
    }

    #[test]
    fn a_base_nobody_extends_is_not_attributed() {
        let (classes, code) =
            model("public abstract class Orphan<E> { protected abstract JpaRepository<E, UUID> getRepository(); }");
        assert!(bases(&classes, &code).is_empty());
    }
}
