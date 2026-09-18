//! Access matrix: which roles, scopes and authentication states the code
//! requires for each operation or capability group.
//!
//! Only what the code states is shown: an operation with no requirement found
//! is "no auth found", never "public", and expressions that aren't a plain
//! role/scope check are shown verbatim.

use std::collections::{BTreeMap, BTreeSet};

use autodoc_analyzer::api::{Operation, Requirement};

use super::Builder;
use crate::model::*;

/// A principal an operation can be reached by.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Principal {
    Authenticated,
    Role(String),
    Scope(String),
}

impl Principal {
    fn column(&self) -> String {
        match self {
            Principal::Authenticated => "authenticated".into(),
            Principal::Role(r) => r.clone(),
            Principal::Scope(s) => format!("scope: {s}"),
        }
    }
}

/// What one requirement grants, and the part that isn't a plain check.
pub(super) struct Parsed {
    pub principals: Vec<Principal>,
    /// The expression, when it has conditions beyond the named principals
    /// (`… or #name.equals('demo')`) or names none at all (`requireAdmin`).
    pub condition: Option<String>,
}

fn quoted(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find(['\'', '"']) {
        let q = rest[i..].chars().next().unwrap();
        let body = &rest[i + 1..];
        let Some(end) = body.find(q) else { break };
        let v = body[..end].trim();
        if !v.is_empty() {
            out.push(v.to_string());
        }
        rest = &body[end + 1..];
    }
    out
}

/// Arguments of every `name(...)` call in `expr`.
fn call_args<'e>(expr: &'e str, name: &str) -> Vec<&'e str> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = expr[from..].find(name) {
        let at = from + i + name.len();
        from = at;
        let before = expr[..from - name.len()].chars().last();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let Some(rest) = expr[at..].strip_prefix('(') else { continue };
        let Some(close) = rest.find(')') else { continue };
        out.push(&rest[..close]);
    }
    out
}

pub(super) fn parse(req: &Requirement) -> Parsed {
    let d = req.detail.trim();
    let mut principals: Vec<Principal> = Vec::new();
    for name in ["hasAnyAuthority", "hasAuthority", "hasAnyRole", "hasRole", "Roles", "RolesAllowed", "Secured"] {
        for args in call_args(d, name) {
            principals.extend(quoted(args).into_iter().map(Principal::Role));
        }
    }
    for name in ["hasScope", "hasAnyScope"] {
        for args in call_args(d, name) {
            principals.extend(quoted(args).into_iter().map(Principal::Scope));
        }
    }
    let lower = d.to_lowercase();
    let plain_identifier = !d.is_empty() && d.chars().all(|c| c.is_alphanumeric() || c == '_');
    match req.kind.as_str() {
        "authenticated" => {
            if principals.is_empty() {
                principals.push(Principal::Authenticated);
            }
        }
        "role"
            if principals.is_empty()
                && plain_identifier
                && d.chars().any(|c| c.is_uppercase())
                && !d.contains(char::is_lowercase) =>
        {
            // `ROLE_ADMIN`, `SYS_ADMIN`
            principals.push(Principal::Role(d.to_string()));
        }
        "role" if principals.is_empty() && plain_identifier && d.chars().all(|c| c.is_lowercase() || c == '_') => {
            // `@RolesAllowed("admin")` reported as `admin`
            principals.push(Principal::Role(d.to_string()));
        }
        "role" if principals.is_empty() => {
            // `requireRole("admin")`
            principals.extend(quoted(d).into_iter().map(Principal::Role));
        }
        "scope" if principals.is_empty() => principals.extend(quoted(d).into_iter().map(Principal::Scope)),
        _ => {}
    }
    principals.sort();
    principals.dedup();
    // A SpEL reference other than `#oauth2` (`#name.equals('demo')`) is a condition too.
    let compound =
        [" or ", " and ", "||", "&&"].iter().any(|op| lower.contains(op)) || d.replace("#oauth2.", "").contains('#');
    let condition = (principals.is_empty() || compound).then(|| d.to_string());
    Parsed { principals, condition }
}

/// Rows of the matrix: an operation, or a group of operations.
pub(super) struct Row<'a> {
    pub label: Vec<Inline>,
    pub ops: Vec<&'a Operation>,
}

impl<'a> Builder<'a> {
    /// Access matrix table for these rows. `None` when no row has any requirement.
    pub(super) fn access_matrix(&mut self, rows: &[Row]) -> Option<Block> {
        let mut columns: BTreeSet<Principal> = BTreeSet::new();
        let mut any_auth = false;
        let mut any_condition = false;
        for row in rows {
            for op in &row.ops {
                for req in &op.auth {
                    any_auth = true;
                    let p = parse(req);
                    any_condition |= p.condition.is_some();
                    columns.extend(p.principals);
                }
            }
        }
        if !any_auth {
            return None;
        }
        let columns: Vec<Principal> = columns.into_iter().collect();
        let any_unprotected = rows.iter().any(|r| r.ops.iter().any(|o| o.auth.is_empty()));
        let mut header = vec!["Operations".to_string()];
        header.extend(columns.iter().map(Principal::column));
        if any_unprotected {
            header.push("no auth found".into());
        }
        if any_condition {
            header.push("Other conditions".into());
        }
        let mut table_rows = Vec::new();
        for row in rows {
            let total = row.ops.len();
            let mut cells: Vec<Vec<Inline>> = vec![row.label.clone()];
            // For each column: operations granting it, and the first requirement as evidence.
            let mut granted: BTreeMap<&Principal, (usize, Option<autodoc_analyzer::scan::EvidenceRef>)> =
                BTreeMap::new();
            let mut conditions: Vec<(String, autodoc_analyzer::scan::EvidenceRef)> = Vec::new();
            let mut unprotected = 0;
            for op in &row.ops {
                if op.auth.is_empty() {
                    unprotected += 1;
                }
                let mut seen_here: BTreeSet<&Principal> = BTreeSet::new();
                for req in &op.auth {
                    let p = parse(req);
                    for pr in &p.principals {
                        let Some(col) = columns.iter().find(|c| *c == pr) else { continue };
                        if seen_here.insert(col) {
                            let e = granted.entry(col).or_insert((0, None));
                            e.0 += 1;
                            e.1.get_or_insert_with(|| req.evidence.clone());
                        }
                    }
                    if let Some(c) = p.condition {
                        if !conditions.iter().any(|(x, _)| *x == c) {
                            conditions.push((c, req.evidence.clone()));
                        }
                    }
                }
            }
            for col in &columns {
                match granted.get(col) {
                    Some((n, ev)) => {
                        let mut cell =
                            vec![Inline::text(if *n == total { "✓".to_string() } else { format!("{n} of {total}") })];
                        if let Some(ev) = ev {
                            cell.push(self.cite(ev));
                        }
                        cells.push(cell);
                    }
                    None => cells.push(vec![Inline::text("")]),
                }
            }
            if any_unprotected {
                cells.push(match unprotected {
                    0 => vec![Inline::text("")],
                    n if n == total => vec![Inline::badge("muted", "none found")],
                    n => vec![Inline::badge("muted", format!("{n} of {total}"))],
                });
            }
            if any_condition {
                let mut cell = Vec::new();
                for (i, (c, ev)) in conditions.iter().take(3).enumerate() {
                    if i > 0 {
                        cell.push(Inline::text("; "));
                    }
                    cell.push(Inline::code(c.clone()));
                    cell.push(self.cite(ev));
                }
                if conditions.len() > 3 {
                    cell.push(Inline::text(format!(" +{} more", conditions.len() - 3)));
                }
                cells.push(cell);
            }
            table_rows.push(cells);
        }
        Some(Block::Table { columns: header, rows: table_rows })
    }

    pub(super) fn access_intro() -> Block {
        Block::Para {
            inl: vec![Inline::text(
                "Roles, scopes and authentication the code requires, read from annotations, guards, dependencies and security configuration. \"none found\" means no requirement was recognised in code, not that the operation is public; expressions that are more than a plain role or scope check are shown as written.",
            )],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autodoc_analyzer::scan::EvidenceRef;

    fn req(kind: &str, detail: &str) -> Requirement {
        Requirement {
            kind: kind.into(),
            detail: detail.into(),
            evidence: EvidenceRef { file_path: "x".into(), start_line: 1, end_line: 1, symbol_name: None, note: None },
        }
    }

    fn names(p: &Parsed) -> Vec<String> {
        p.principals.iter().map(Principal::column).collect()
    }

    #[test]
    fn roles_scopes_and_conditions() {
        let p = parse(&req("role", "hasAnyAuthority('SYS_ADMIN', 'TENANT_ADMIN')"));
        assert_eq!(names(&p), ["SYS_ADMIN", "TENANT_ADMIN"]);
        assert!(p.condition.is_none());
        let p = parse(&req("scope", "#oauth2.hasScope('server') or #name.equals('demo')"));
        assert_eq!(names(&p), ["scope: server"]);
        assert_eq!(p.condition.as_deref(), Some("#oauth2.hasScope('server') or #name.equals('demo')"));
        let p = parse(&req("scope", "#oauth2.hasScope('ui')"));
        assert_eq!((names(&p), p.condition), (vec!["scope: ui".to_string()], None));
        assert_eq!(names(&parse(&req("authenticated", "authenticated()"))), ["authenticated"]);
        assert_eq!(names(&parse(&req("role", "hasRole('ADMIN')"))), ["ADMIN"]);
        assert_eq!(names(&parse(&req("role", "ROLE_ADMIN"))), ["ROLE_ADMIN"]);
        assert_eq!(names(&parse(&req("role", "admin"))), ["admin"]);
        assert_eq!(names(&parse(&req("role", "@Roles(\"admin\")"))), ["admin"]);
        assert_eq!(names(&parse(&req("role", "requireRole(\"admin\")"))), ["admin"]);
        let custom = parse(&req("role", "requireAdmin"));
        assert!(custom.principals.is_empty());
        assert_eq!(custom.condition.as_deref(), Some("requireAdmin"));
        assert_eq!(names(&parse(&req("authenticated", "@UseGuards(AuthGuard(\"jwt\"))"))), ["authenticated"]);
    }
}
