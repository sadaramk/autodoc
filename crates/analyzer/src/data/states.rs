//! State machines on closed-set status columns. Transitions are only what the
//! code states: `SET status = 'x'` / assignments give the target, a `WHERE` or
//! a nearby comparison gives the source; otherwise `from` stays empty.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::access::{enclosing, is_word_at, SqlStmt};
use super::raw::*;
use super::{Entity, State, StateMachine, Transition};
use crate::extract::FileFacts;
use crate::scan::EvidenceRef;

pub struct Src<'a> {
    pub path: String,
    pub unit: String,
    pub text: String,
    pub facts: &'a FileFacts,
    pub migration: bool,
    /// Lines covered by SQL literals (code patterns skip them).
    pub sql_lines: BTreeSet<u32>,
    /// First line of a `#[cfg(test)]` module, if any.
    pub test_from: Option<u32>,
}

impl Src<'_> {
    /// Test code: test files, `test_*` functions, Rust `#[cfg(test)]` modules.
    pub fn is_test(&self, line: u32, func: Option<&str>) -> bool {
        let file = self.path.rsplit('/').next().unwrap_or("");
        self.path.split('/').any(|s| matches!(s, "test" | "tests" | "__tests__" | "spec" | "testdata"))
            || file.contains(".test.")
            || file.contains(".spec.")
            || file.ends_with("_test.go")
            || file.starts_with("test_")
            || func.is_some_and(|f| f.starts_with("test_") || f == "test" || f.starts_with("Test"))
            || self.test_from.is_some_and(|t| line >= t)
    }
}

/// Lifecycles only: a closed set on a status-like field (not `type`, `kind`, `key`, …).
fn is_lifecycle_candidate(m: &Machine) -> bool {
    let lifecycle = |s: &str| {
        let s = s.to_lowercase();
        ["status", "state", "stage", "phase", "lifecycle"].iter().any(|k| s.contains(k))
    };
    m.fields.iter().any(|f| lifecycle(f)) || m.enum_names.iter().any(|n| lifecycle(n)) || m.table.is_none()
}

struct Machine {
    table: Option<String>,
    fields: Vec<String>,
    subject: String,
    id: String,
    values: Vec<String>,
    /// member → value
    members: Vec<(String, String)>,
    /// Code enums backing it (qualified values must name one of them).
    enum_names: Vec<String>,
    evidence: EvidenceRef,
    value_evidence: HashMap<String, EvidenceRef>,
    initial: BTreeSet<String>,
    transitions: Vec<Transition>,
}

impl Machine {
    fn resolve(&self, expr: &str) -> Option<String> {
        let e = expr.trim().trim_end_matches([',', ';', ')', '}']).trim();
        let e = if e.ends_with("::") { e } else { e.trim_end_matches(':') };
        let e = e.strip_suffix(".value").unwrap_or(e).trim_end_matches(".to_string()").trim_end_matches(".into()");
        if e.is_empty() {
            return None;
        }
        let quoted = (e.starts_with('\'') && e.ends_with('\''))
            || (e.starts_with('"') && e.ends_with('"'))
            || (e.starts_with('`') && e.ends_with('`'));
        if quoted && e.len() >= 2 {
            let v = &e[1..e.len() - 1];
            return self
                .values
                .iter()
                .find(|x| *x == v)
                .or_else(|| self.values.iter().find(|x| x.eq_ignore_ascii_case(v)))
                .cloned();
        }
        if e.contains(|c: char| c.is_whitespace() || c == '(' || c == '=' || c == '[' || c == '"' || c == '\'') {
            return None;
        }
        if let Some((owner, _)) = e.rsplit_once(['.', ':']) {
            let owner = owner.trim_end_matches(':').rsplit(['.', ':']).next().unwrap_or("");
            if owner.starts_with(|c: char| c.is_uppercase())
                && !self.enum_names.is_empty()
                && !self.enum_names.iter().any(|n| n == owner)
            {
                return None;
            }
        }
        let last = e.rsplit([':', '.']).next().unwrap_or(e);
        if !e.contains(['.', ':']) && !self.members.iter().any(|(m, _)| m == last) {
            // A bare identifier must be a known member (Go const), never a variable.
            return None;
        }
        self.members.iter().find(|(m, _)| m == last).map(|(_, v)| v.clone())
    }
}

fn member_line(text: &str, from: u32, member: &str) -> Option<u32> {
    text.lines()
        .enumerate()
        .skip(from.saturating_sub(1) as usize)
        .take(80)
        .find(|(_, l)| {
            is_word_at(l, member) || l.contains(&format!("'{member}'")) || l.contains(&format!("\"{member}\""))
        })
        .map(|(i, _)| i as u32 + 1)
}

/// True when the text ends inside a query criteria object: an open `where: { …`,
/// or the first argument of TypeORM `update({ criteria }, { values })`.
fn inside_criteria(prefix: &str) -> bool {
    // Whether the first `{…}` object in `tail` is still open at its end.
    let first_object_open = |tail: &str| {
        let mut depth = 0;
        let mut started = false;
        for c in tail.chars() {
            match c {
                '{' => {
                    depth += 1;
                    started = true;
                }
                '}' => {
                    depth -= 1;
                    if started && depth == 0 {
                        return false;
                    }
                }
                _ => {}
            }
        }
        started && depth > 0
    };
    let open_where = prefix.rfind("where").is_some_and(|w| first_object_open(&prefix[w..]));
    let criteria_arg = prefix.rfind("update({").is_some_and(|u| {
        let tail = &prefix[u + "update(".len()..];
        first_object_open(tail) && !tail.contains("data:") && !tail.contains("where")
    });
    open_where || criteria_arg
}

/// Value expression after `pat` on the line, as a simple token.
fn rhs_after(line: &str, at: usize) -> String {
    let rest = line[at..].trim_start();
    let mut out = String::new();
    let mut quote: Option<char> = None;
    for c in rest.chars() {
        if let Some(q) = quote {
            out.push(c);
            if c == q {
                break;
            }
            continue;
        }
        if matches!(c, '\'' | '"' | '`') && out.is_empty() {
            quote = Some(c);
            out.push(c);
            continue;
        }
        if c.is_alphanumeric() || matches!(c, '_' | '.' | ':') {
            out.push(c);
        } else {
            break;
        }
    }
    out
}

/// Positions right after `field` followed by `op` (e.g. ` = `), guarding identifier boundaries.
fn after_field(line: &str, field: &str, ops: &[&str], reject: &[&str]) -> Vec<(usize, String)> {
    let mut out = vec![];
    let b = line.as_bytes();
    let mut from = 0;
    while let Some(p) = line[from..].find(field) {
        let s = from + p;
        let e = s + field.len();
        from = s + 1;
        if s > 0 && (b[s - 1].is_ascii_alphanumeric() || b[s - 1] == b'_') {
            continue;
        }
        if e < b.len() && (b[e].is_ascii_alphanumeric() || b[e] == b'_') {
            continue;
        }
        let rest = &line[e..];
        let trimmed = rest.trim_start();
        if reject.iter().any(|r| trimmed.starts_with(r)) {
            continue;
        }
        for op in ops {
            if let Some(after) = trimmed.strip_prefix(op) {
                out.push((line.len() - after.len(), op.to_string()));
                break;
            }
        }
    }
    out
}

pub struct Inputs<'a> {
    pub entities: &'a [Entity],
    pub enums: &'a [RawEnum],
    pub checks: &'a [RawCheck],
    pub enum_uses: &'a [RawEnumUse],
    pub aliases: &'a HashMap<String, String>,
    pub code_names: &'a BTreeMap<String, Vec<(String, String)>>,
    pub sources: &'a [Src<'a>],
    pub sql: &'a [(usize, u32, SqlStmt)],
    pub read_text: &'a dyn Fn(&str) -> Option<String>,
    /// Transitions declared in configuration (Spring Statemachine).
    pub extra: &'a [super::java::ExtraTransition],
}

pub fn extract(inp: &Inputs) -> Vec<StateMachine> {
    let mut machines: Vec<Machine> = vec![];
    let alias = |t: &str| inp.aliases.get(&t.to_lowercase()).cloned().unwrap_or_else(|| t.to_lowercase());
    let add = |machines: &mut Vec<Machine>,
               table: Option<String>,
               field: String,
               values: Vec<String>,
               members: Vec<(String, String)>,
               ev: EvidenceRef,
               default: Option<String>| {
        if values.len() < 2 {
            return;
        }
        if let Some(m) = machines.iter_mut().find(|m| m.table == table && table.is_some() && m.fields.contains(&field))
        {
            for (mem, v) in members {
                let v = m.values.iter().find(|x| x.eq_ignore_ascii_case(&v)).cloned().unwrap_or(v);
                if !m.members.iter().any(|(a, _)| *a == mem) {
                    m.members.push((mem, v));
                }
            }
            if let Some(d) = default {
                m.initial.insert(d);
            }
            return;
        }
        let (id, subject) = match &table {
            Some(t) => (format!("state:{t}.{field}"), format!("{t}.{field}")),
            None => (format!("state:{field}"), field.clone()),
        };
        let mut m = Machine {
            table,
            fields: vec![field],
            subject,
            id,
            values: values.clone(),
            members,
            enum_names: vec![],
            evidence: ev,
            value_evidence: HashMap::new(),
            initial: BTreeSet::new(),
            transitions: vec![],
        };
        if let Some(d) = default {
            m.initial.insert(d);
        }
        machines.push(m);
    };

    for c in inp.checks {
        let t = alias(&c.table);
        let default = inp
            .entities
            .iter()
            .find(|e| e.table == t)
            .and_then(|e| e.columns.iter().find(|x| x.name == c.column))
            .and_then(|x| x.default.clone());
        add(
            &mut machines,
            Some(t),
            c.column.clone(),
            c.values.clone(),
            c.values.iter().map(|v| (v.clone(), v.clone())).collect(),
            c.evidence.clone(),
            default,
        );
    }
    for en in inp.enums.iter().filter(|e| e.sql_type) {
        for ent in inp.entities {
            for col in ent.columns.iter().filter(|c| c.type_name.trim_matches('"').eq_ignore_ascii_case(&en.name)) {
                add(
                    &mut machines,
                    Some(ent.table.clone()),
                    col.name.clone(),
                    en.values.iter().map(|v| v.0.clone()).collect(),
                    en.values.iter().map(|(v, m)| (m.clone(), v.clone())).collect(),
                    en.evidence.clone(),
                    col.default.clone(),
                );
            }
        }
    }
    let mut linked: BTreeSet<String> = BTreeSet::new();
    for u in inp.enum_uses {
        let Some(en) = inp.enums.iter().find(|e| e.name == u.enum_name) else { continue };
        linked.insert(en.name.clone());
        let t = alias(&u.table);
        if !inp.entities.iter().any(|e| e.table == t) {
            continue;
        }
        add(
            &mut machines,
            Some(t),
            u.column.clone(),
            en.values.iter().map(|v| v.0.clone()).collect(),
            en.values.iter().map(|(v, m)| (m.clone(), v.clone())).collect(),
            en.evidence.clone(),
            None,
        );
        let m = machines
            .iter_mut()
            .find(|m| m.table.as_deref() == Some(&alias(&u.table)) && m.fields.contains(&u.column))
            .unwrap();
        if !m.enum_names.contains(&en.name) {
            m.enum_names.push(en.name.clone());
        }
        if let Some(d) = &u.default {
            if let Some(v) =
                m.resolve(d).or_else(|| m.resolve(&format!("'{}'", d.trim_matches(|c| c == '"' || c == '\''))))
            {
                m.initial.insert(v);
            }
        }
    }
    // Code enums named like a status that no model column declares.
    for en in inp.enums.iter().filter(|e| !e.sql_type && !linked.contains(&e.name)) {
        let lname = en.name.to_lowercase();
        if !(lname.ends_with("status") || lname.ends_with("state")) {
            continue;
        }
        let mut fields: BTreeSet<String> = BTreeSet::new();
        let mut defaults: Vec<String> = vec![];
        for s in inp.sources.iter().filter(|s| !s.migration && s.text.contains(en.name.as_str())) {
            for l in s.text.lines() {
                if !is_word_at(l, &en.name) {
                    continue;
                }
                let t = l
                    .trim()
                    .trim_start_matches("pub ")
                    .trim_start_matches("readonly ")
                    .trim_start_matches("private ")
                    .trim_start_matches("public ");
                let field: String = t.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if field.is_empty()
                    || field == en.name
                    || matches!(
                        field.as_str(),
                        "type"
                            | "enum"
                            | "class"
                            | "const"
                            | "let"
                            | "var"
                            | "export"
                            | "import"
                            | "from"
                            | "return"
                            | "if"
                            | "fn"
                            | "def"
                            | "func"
                    )
                {
                    continue;
                }
                let rest = t[field.len()..].trim_start().trim_start_matches('?').trim_start();
                let declared = rest
                    .strip_prefix(':')
                    .map(|r| r.trim_start().trim_start_matches("Mapped[").trim_start_matches("Option<"))
                    .is_some_and(|r| r.starts_with(&en.name))
                    || rest.starts_with(&en.name);
                if declared {
                    if let Some((_, d)) = rest.split_once('=') {
                        defaults.push(d.trim().to_string());
                    }
                    fields.insert(field);
                }
            }
        }
        if fields.is_empty() {
            continue;
        }
        let n = machines.len();
        add(
            &mut machines,
            None,
            en.name.clone(),
            en.values.iter().map(|v| v.0.clone()).collect(),
            en.values.iter().map(|(v, m)| (m.clone(), v.clone())).collect(),
            en.evidence.clone(),
            None,
        );
        if machines.len() > n {
            let m = machines.last_mut().unwrap();
            m.fields = fields.into_iter().collect();
            m.enum_names.push(en.name.clone());
            m.initial.extend(defaults);
        }
    }
    // Declared state machines (Spring Statemachine) on a code enum.
    for x in inp.extra {
        let Some(en) = inp.enums.iter().find(|e| e.name == x.enum_name) else { continue };
        if !machines.iter().any(|m| m.enum_names.contains(&en.name)) {
            let n = machines.len();
            add(
                &mut machines,
                None,
                en.name.clone(),
                en.values.iter().map(|v| v.0.clone()).collect(),
                en.values.iter().map(|(v, m)| (m.clone(), v.clone())).collect(),
                en.evidence.clone(),
                None,
            );
            if machines.len() == n {
                continue;
            }
            machines.last_mut().unwrap().enum_names.push(en.name.clone());
        }
        let m = machines.iter_mut().find(|m| m.enum_names.contains(&en.name)).unwrap();
        let (Some(to), from) = (m.resolve(&x.to), x.from.as_deref().map(|f| m.resolve(f))) else { continue };
        match x.marker {
            Some("initial") => {
                m.initial.insert(to);
            }
            Some(_) => {}
            None => {
                let Some(from) = from.flatten() else { continue };
                m.transitions.push(Transition {
                    from: Some(from),
                    to,
                    unit: x.unit.clone(),
                    trigger: x.trigger.clone(),
                    guard: None,
                    evidence: x.evidence.clone(),
                });
            }
        }
    }

    // Per-value evidence from the defining file.
    for m in &mut machines {
        if m.value_evidence.len() == m.values.len() {
            continue;
        }
        let text = (inp.read_text)(&m.evidence.file_path).unwrap_or_default();
        for v in m.values.clone() {
            let member = m.members.iter().find(|(_, x)| *x == v).map(|(k, _)| k.clone()).unwrap_or(v.clone());
            let quoted = [format!("'{v}'"), format!("\"{v}\"")];
            let line = if m.evidence.file_path.ends_with(".sql") {
                text.lines()
                    .enumerate()
                    .skip(m.evidence.start_line as usize - 1)
                    .take(20)
                    .find(|(_, l)| quoted.iter().any(|q| l.contains(q.as_str())))
                    .map(|(i, _)| i as u32 + 1)
            } else {
                member_line(&text, m.evidence.start_line, &member)
                    .or_else(|| member_line(&text, m.evidence.start_line, &v))
            }
            .unwrap_or(m.evidence.start_line);
            m.value_evidence.entry(v).or_insert_with(|| crate::source::line_ref(&m.evidence.file_path, line));
        }
        // Normalise defaults (`'pending'::order_status`, `OrderStatus.PENDING`).
        let raw: Vec<String> = m.initial.iter().cloned().collect();
        m.initial = raw
            .iter()
            .filter_map(|d| {
                m.resolve(d).or_else(|| {
                    m.resolve(&format!(
                        "'{}'",
                        d.split("::").next().unwrap_or(d).trim_matches(|c| c == '\'' || c == '"')
                    ))
                })
            })
            .collect();
    }
    for m in &mut machines {
        if m.value_evidence.len() == m.values.len() {
            let raw: Vec<String> = m.initial.iter().cloned().collect();
            m.initial = raw
                .iter()
                .filter_map(|d| {
                    m.resolve(d).or_else(|| {
                        m.resolve(&format!(
                            "'{}'",
                            d.split("::").next().unwrap_or(d).trim_matches(|c| c == '\'' || c == '"')
                        ))
                    })
                })
                .collect();
        }
    }

    // SQL literals.
    for (si, line, st) in inp.sql {
        let src = &inp.sources[*si];
        let Some(target) = st.target.as_ref().map(|t| alias(t)) else { continue };
        let trigger = enclosing(src.facts, *line).map(|s| s.0);
        if src.is_test(*line, trigger.as_deref()) {
            continue;
        }
        let ev = crate::source::line_ref(&src.path, *line);
        for m in machines.iter_mut().filter(|m| m.table.as_deref() == Some(target.as_str())) {
            if st.kind == "insert" {
                for (c, v) in &st.insert_values {
                    if m.fields.contains(c) {
                        if let Some(v) = v.as_ref().and_then(|v| m.resolve(&format!("'{v}'"))) {
                            m.initial.insert(v);
                        }
                    }
                }
            }
            if st.kind != "update" {
                continue;
            }
            for (c, v) in &st.sets {
                if !m.fields.contains(c) {
                    continue;
                }
                let Some(to) = v.as_ref().and_then(|v| m.resolve(&format!("'{v}'"))) else { continue };
                let froms: Vec<String> = st
                    .where_values
                    .iter()
                    .filter(|(wc, _)| m.fields.contains(wc))
                    .flat_map(|(_, vs)| vs.iter().filter_map(|v| m.resolve(&format!("'{v}'"))))
                    .collect();
                let guard = st.where_text.as_deref().and_then(|w| super::access::business_conditions(w, &m.fields));
                if froms.is_empty() {
                    m.transitions.push(Transition {
                        from: None,
                        to: to.clone(),
                        unit: src.unit.clone(),
                        trigger: trigger.clone(),
                        guard: guard.clone(),
                        evidence: ev.clone(),
                    });
                }
                for f in froms {
                    m.transitions.push(Transition {
                        from: Some(f),
                        to: to.clone(),
                        unit: src.unit.clone(),
                        trigger: trigger.clone(),
                        guard: guard.clone(),
                        evidence: ev.clone(),
                    });
                }
            }
        }
    }

    // Assignments in code.
    let creations: Vec<String> = inp.code_names.values().flatten().map(|(n, _)| format!("new {n}(")).collect();
    // A hit resolves to a state value named on the same line: lowercase values and members per field.
    // Only lifecycles are reported (see the final filter below). Dropping the rest
    // before the transition scan matters: `type`, `scope` or `final` appear on
    // nearly every line of a Java codebase.
    machines.retain(is_lifecycle_candidate);
    // Distinct lowercase state names per field: many machines share a field
    // name (`status`), and every line mentioning it is checked against these.
    let mut named: HashMap<String, Vec<String>> = HashMap::new();
    {
        let mut sets: HashMap<String, BTreeSet<String>> = HashMap::new();
        for m in &machines {
            for f in &m.fields {
                let e = sets.entry(f.clone()).or_default();
                e.extend(m.values.iter().map(|v| v.to_lowercase()));
                e.extend(m.members.iter().map(|(k, _)| k.to_lowercase()));
            }
        }
        for (f, vs) in sets {
            named.insert(f, vs.into_iter().collect());
        }
    }
    for src in inp.sources.iter().filter(|s| !s.migration) {
        let lines: Vec<&str> = src.text.lines().collect();
        let java = src.path.ends_with(".java");
        let mut field_names: BTreeSet<(String, String)> = BTreeSet::new();
        for f in machines.iter().flat_map(|m| m.fields.iter()) {
            let mut cap = f.chars();
            let capital = cap.next().map(|c| c.to_uppercase().collect::<String>() + cap.as_str()).unwrap_or_default();
            let camel: String = f
                .split('_')
                .enumerate()
                .map(|(k, p)| {
                    if k == 0 {
                        p.to_string()
                    } else {
                        let mut c = p.chars();
                        c.next().map(|x| x.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
                    }
                })
                .collect();
            let pascal: String = f
                .split('_')
                .map(|p| {
                    let mut c = p.chars();
                    c.next().map(|x| x.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
                })
                .collect();
            for v in [f.clone(), capital, camel, pascal] {
                field_names.insert((v, f.clone()));
            }
        }
        // Only the field spellings this file mentions at all.
        let present: Vec<&(String, String)> =
            field_names.iter().filter(|(f, _)| src.text.contains(f.as_str())).collect();
        if present.is_empty() {
            continue;
        }
        for (i, l) in lines.iter().enumerate() {
            let ln = i as u32 + 1;
            if src.sql_lines.contains(&ln) {
                continue;
            }
            let lt = l.trim_start();
            if lt.starts_with("//") || lt.starts_with('#') || lt.starts_with("--") || lt.starts_with('*') {
                continue;
            }
            let mut lower: Option<String> = None;
            for (field, canon) in present.iter().map(|x| (&x.0, &x.1)) {
                if !l.contains(field.as_str()) {
                    continue;
                }
                let ll = lower.get_or_insert_with(|| l.to_lowercase());
                if !named.get(canon).is_some_and(|vs| vs.iter().any(|v| ll.contains(v.as_str()))) {
                    continue;
                }
                let Some((func, fstart, fend)) = enclosing(src.facts, ln) else { continue };
                if src.is_test(ln, Some(&func)) {
                    continue;
                }
                let body: String =
                    lines[(fstart as usize).saturating_sub(1)..(fend as usize).min(lines.len())].join("\n");
                let body_l = body.to_lowercase();
                // Java: constructors, `@PrePersist` callbacks and code right after `new Entity(` create.
                let java_initial = java
                    && (func.starts_with(|c: char| c.is_uppercase())
                        || src.facts.annotations.iter().any(|a| a.name == "PrePersist" && a.target == func)
                        || lines[(fstart as usize).saturating_sub(1)..i]
                            .iter()
                            .any(|x| x.contains("new ") && creations.iter().any(|t| x.contains(t.as_str()))));
                // (value expression, is a creation → initial state)
                let mut hits: Vec<(String, bool)> = vec![];
                let ctx_lo = (fstart as usize).saturating_sub(1).max(i.saturating_sub(6));
                // Which kind of statement the value sits in: an update (transition) or a create (initial state).
                let context_kind = |prefix: &str| -> Option<bool> {
                    let p = prefix.to_lowercase();
                    let upd = ["update", ".set(", "save", ".values("].iter().filter_map(|k| p.rfind(k)).max();
                    let new = ["insert", "create", "new(", "::new", "form {", "build("]
                        .iter()
                        .filter_map(|k| p.rfind(k))
                        .max();
                    match (upd, new) {
                        (Some(u), Some(n)) => Some(n > u),
                        (Some(_), None) => Some(false),
                        (None, Some(_)) => Some(true),
                        (None, None) => None,
                    }
                };
                for (at, _) in after_field(l, field, &["=", "+="], &["==", "=>", "=~"]) {
                    // `x.status = V` or `status=V` keyword argument.
                    if l[..at].trim_end().ends_with("==") {
                        continue;
                    }
                    // Inside a string (`sql\`asset.type = 'IMAGE'\``) it is a comparison, not an assignment.
                    if ['`', '"'].iter().any(|q| l[..at].matches(*q).count() % 2 == 1) {
                        continue;
                    }
                    let before = &l[..at];
                    let in_call = before.matches('(').count() > before.matches(')').count();
                    if in_call {
                        // Keyword argument: `Order(status=…)` creates, `.update(status=…)` / `.values(status=…)` updates.
                        let call = before[..before.rfind('(').unwrap_or(0)]
                            .rsplit(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
                            .next()
                            .unwrap_or("");
                        let is_model = call.starts_with(|c: char| c.is_uppercase())
                            && inp.code_names.values().flatten().any(|(n, _)| n == call);
                        match context_kind(call) {
                            Some(initial) => hits.push((rhs_after(l, at), initial)),
                            None if is_model => hits.push((rhs_after(l, at), true)),
                            None => {}
                        }
                        continue;
                    }
                    hits.push((rhs_after(l, at), java_initial));
                }
                if java {
                    if field.starts_with(|c: char| c.is_uppercase()) {
                        // `order.setStatus(OrderStatus.PAID)`
                        let pat = format!("set{field}(");
                        let mut from = 0;
                        while let Some(p) = l[from..].find(&pat).map(|x| from + x) {
                            from = p + 1;
                            let b = l.as_bytes();
                            if p > 0 && (b[p - 1].is_ascii_alphanumeric() || b[p - 1] == b'_') {
                                continue;
                            }
                            hits.push((rhs_after(l, p + pat.len()), java_initial));
                        }
                    } else if let Some(p) = l.find(&format!(".{field}(")) {
                        // Lombok builder: `Order.builder().status(OrderStatus.NEW)` creates.
                        let ctx = lines[i.saturating_sub(4)..=i].join(" ");
                        if ctx.contains("builder()") {
                            hits.push((rhs_after(l, p + field.len() + 2), true));
                        }
                    }
                }
                if body_l.contains("update")
                    || body_l.contains(".set(")
                    || body_l.contains("save")
                    || body_l.contains("data:")
                    || body_l.contains("insert")
                    || body_l.contains("create")
                {
                    for (at, _) in after_field(l, field, &[":"], &["::"]) {
                        let before = l[..at].trim_end_matches(':').trim_end();
                        if before.ends_with('?') {
                            continue;
                        }
                        let prefix = format!("{} {}", lines[i.saturating_sub(2)..i].join(" "), &l[..at]).to_lowercase();
                        if inside_criteria(&prefix) {
                            continue;
                        }
                        let prefix = format!("{}\n{}", lines[ctx_lo..i].join("\n"), &l[..at]);
                        if let Some(initial) = context_kind(&prefix) {
                            hits.push((rhs_after(l, at), initial));
                        }
                    }
                }
                for pat in [format!("Update(\"{field}\","), format!("{field}.eq(")] {
                    if let Some(p) = l.find(&pat) {
                        if pat.ends_with(".eq(")
                            && !(l.contains(".set(")
                                || lines[i.saturating_sub(2)..i].iter().any(|x| x.contains(".set(")))
                        {
                            continue;
                        }
                        hits.push((rhs_after(l, p + pat.len()), false));
                    }
                }
                if hits.is_empty() {
                    continue;
                }
                for (hit, initial) in hits {
                    let candidates: Vec<usize> = machines
                        .iter()
                        .enumerate()
                        .filter(|(_, m)| m.fields.contains(canon) && m.resolve(&hit).is_some())
                        .map(|(k, _)| k)
                        .collect();
                    let chosen = match candidates.len() {
                        0 => continue,
                        1 => candidates[0],
                        _ => {
                            let score = |m: &Machine| -> i32 {
                                let mut s = 0;
                                if let Some(t) = &m.table {
                                    if is_word_at(&body, t) {
                                        s += 2;
                                    }
                                    if inp
                                        .code_names
                                        .get(t)
                                        .is_some_and(|ns| ns.iter().any(|(n, _)| is_word_at(&body, n)))
                                    {
                                        s += 2;
                                    }
                                    if inp.entities.iter().any(|e| &e.table == t && e.units.contains(&src.unit)) {
                                        s += 1;
                                    }
                                }
                                // `OrderStatus.PAID`: the enum named in the value decides.
                                if let Some((owner, _)) = hit.rsplit_once(['.', ':']) {
                                    let owner = owner.trim_end_matches(':').rsplit(['.', ':']).next().unwrap_or("");
                                    if inp.enum_uses.iter().any(|u| {
                                        u.enum_name == owner
                                            && m.table.as_deref() == Some(alias(&u.table).as_str())
                                            && m.fields.contains(&u.column)
                                    }) {
                                        s += 3;
                                    }
                                }
                                s
                            };
                            let scored: Vec<(usize, i32)> =
                                candidates.iter().map(|&k| (k, score(&machines[k]))).collect();
                            let best = scored.iter().map(|x| x.1).max().unwrap_or(0);
                            let top: Vec<&(usize, i32)> = scored.iter().filter(|x| x.1 == best).collect();
                            if top.len() != 1 {
                                continue;
                            }
                            top[0].0
                        }
                    };
                    let m = &mut machines[chosen];
                    let to = m.resolve(&hit).unwrap();
                    if initial {
                        m.initial.insert(to);
                        continue;
                    }
                    // Source state: the nearest comparison above (within the function), or a where-clause nearby.
                    let mut froms: Vec<String> = vec![];
                    let lo = (fstart as usize).max(1) - 1;
                    for k in (lo..i).rev().take(6) {
                        let cl = lines[k];
                        let mut vals = vec![];
                        for (at, _) in after_field(cl, field, &["===", "==", "!==", "!=", "is not", "is", "in"], &[]) {
                            let rest = cl[at..].trim_start();
                            if let Some(inner) = rest
                                .strip_prefix('(')
                                .or_else(|| rest.strip_prefix('['))
                                .or_else(|| rest.strip_prefix('{'))
                            {
                                for part in inner.split([',', ')', ']', '}']) {
                                    if let Some(v) = m.resolve(part) {
                                        vals.push(v);
                                    }
                                }
                            } else if let Some(v) = m.resolve(&rhs_after(cl, at)) {
                                vals.push(v);
                            }
                        }
                        if java {
                            // `order.getStatus() != OrderStatus.PENDING`, `getStatus().equals(OrderStatus.PAID)`.
                            let pas: String = canon
                                .split('_')
                                .map(|p| {
                                    let mut c = p.chars();
                                    c.next()
                                        .map(|x| x.to_uppercase().collect::<String>() + c.as_str())
                                        .unwrap_or_default()
                                })
                                .collect();
                            for g in [format!("get{pas}()"), format!("is{pas}()")] {
                                for (at, _) in after_field(cl, &g, &["==", "!="], &[]) {
                                    if let Some(v) = m.resolve(&rhs_after(cl, at)) {
                                        vals.push(v);
                                    }
                                }
                                let eq = format!("{g}.equals(");
                                if let Some(p) = cl.find(&eq) {
                                    if let Some(v) = m.resolve(&rhs_after(cl, p + eq.len())) {
                                        vals.push(v);
                                    }
                                }
                            }
                        }
                        if !vals.is_empty() {
                            froms = vals;
                            break;
                        }
                    }
                    if froms.is_empty() {
                        let hi = (i + 4).min(fend as usize).min(lines.len());
                        let from_ctx = (fstart as usize).max(1) - 1;
                        for cl in lines.iter().take(hi).skip(from_ctx.max(i.saturating_sub(4))) {
                            let lower = cl.to_lowercase();
                            if !(lower.contains("where")
                                || lower.contains("filter")
                                || lower.contains(".eq(")
                                || lower.contains("update({"))
                            {
                                continue;
                            }
                            for (at, _) in after_field(cl, field, &[":", "==", "=", ".eq("], &["::"]) {
                                if let Some(v) = m.resolve(&rhs_after(cl, at)) {
                                    if v != to {
                                        froms.push(v);
                                    }
                                }
                            }
                            // GORM: `.Where("id = ? AND status = ?", id, OrderNew)`.
                            if let Some(p) =
                                cl.find(&format!("{field} = ?")).or_else(|| cl.find(&format!("{canon} = ?")))
                            {
                                if let Some(q0) = cl[..p].rfind('"') {
                                    if let Some(q1) = cl[p..].find('"').map(|x| x + p) {
                                        let nth = cl[q0..p].matches('?').count();
                                        let args = super::raw::split_args(
                                            cl[q1 + 1..].trim_start_matches(',').split(')').next().unwrap_or(""),
                                        );
                                        if let Some(v) = args.get(nth).and_then(|a| m.resolve(a)) {
                                            froms.push(v);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    froms.retain(|f| *f != to);
                    froms.dedup();
                    let ev = crate::source::line_ref(&src.path, ln);
                    if froms.is_empty() {
                        m.transitions.push(Transition {
                            from: None,
                            to,
                            unit: src.unit.clone(),
                            trigger: Some(func.clone()),
                            guard: None,
                            evidence: ev,
                        });
                    } else {
                        for f in froms {
                            m.transitions.push(Transition {
                                from: Some(f),
                                to: to.clone(),
                                unit: src.unit.clone(),
                                trigger: Some(func.clone()),
                                guard: None,
                                evidence: ev.clone(),
                            });
                        }
                    }
                }
            }
        }
    }

    let mut out = vec![];
    for mut m in machines {
        if !is_lifecycle_candidate(&m) {
            continue;
        }
        let mut seen = BTreeSet::new();
        m.transitions.retain(|t| {
            seen.insert((
                t.from.clone(),
                t.to.clone(),
                t.unit.clone(),
                t.trigger.clone(),
                t.evidence.start_line,
                t.evidence.file_path.clone(),
            ))
        });
        if m.transitions.is_empty() || m.values.len() < 2 {
            continue;
        }
        let states = m
            .values
            .iter()
            .map(|v| {
                let incoming = m.transitions.iter().any(|t| &t.to == v);
                let outgoing =
                    m.transitions.iter().any(|t| t.from.as_ref() == Some(v) || (t.from.is_none() && &t.to != v));
                State {
                    name: v.clone(),
                    initial: m.initial.contains(v),
                    terminal: incoming && !outgoing,
                    evidence: m.value_evidence.get(v).cloned().unwrap_or_else(|| m.evidence.clone()),
                }
            })
            .collect();
        out.push(StateMachine {
            id: m.id,
            subject: m.subject,
            states,
            transitions: m.transitions,
            evidence: m.evidence,
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}
