//! Architecture diff: what two scans of the same repository disagree about.
//!
//! A book tells you what the architecture is. A pull request asks a different
//! question — what does this change? — and answering it from two books means
//! diffing rendered HTML, which reports a reordered table as a change and a new
//! datastore as three. This compares the models instead, so the answer is about
//! services, routes and tables rather than about text.
//!
//! Only facts the analyzer already carries evidence for appear here. There is no
//! severity and no judgement: a removed operation is reported, not graded.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::api::{ApiModel, Operation};
use crate::data::DataModel;
use crate::scan::ScanReport;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Verb {
    Added,
    Removed,
    Changed,
}

impl Verb {
    pub fn word(self) -> &'static str {
        match self {
            Verb::Added => "added",
            Verb::Removed => "removed",
            Verb::Changed => "changed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub verb: Verb,
    /// What changed, named as the book names it: `payments`, `POST /charges`.
    pub subject: String,
    /// How it changed. Only set for `Changed`, and only for facts that are
    /// worth a reviewer's attention.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub title: String,
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    /// Revisions as the caller named them.
    pub base: String,
    pub head: String,
    /// Resolved commits, when the revisions were resolvable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_commit: Option<String>,
    pub sections: Vec<Section>,
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.sections.iter().all(|s| s.changes.is_empty())
    }

    pub fn total(&self) -> usize {
        self.sections.iter().map(|s| s.changes.len()).sum()
    }
}

/// Added / removed subjects between two keyed sets, in the order of the keys.
fn added_removed<'a, T>(
    before: &BTreeMap<String, &'a T>,
    after: &BTreeMap<String, &'a T>,
    label: impl Fn(&T) -> String,
) -> Vec<Change> {
    let mut out = Vec::new();
    for (k, v) in after {
        if !before.contains_key(k) {
            out.push(Change { verb: Verb::Added, subject: label(v), details: vec![] });
        }
    }
    for (k, v) in before {
        if !after.contains_key(k) {
            out.push(Change { verb: Verb::Removed, subject: label(v), details: vec![] });
        }
    }
    out
}

fn by_id<'a, T>(items: &'a [T], id: impl Fn(&'a T) -> &'a str) -> BTreeMap<String, &'a T> {
    items.iter().map(|i| (id(i).to_string(), i)).collect()
}

/// `a → b`, or just `b` when there was nothing before.
fn transition(before: Option<&str>, after: Option<&str>) -> String {
    match (before, after) {
        (Some(a), Some(b)) => format!("`{a}` → `{b}`"),
        (None, Some(b)) => format!("now `{b}`"),
        (Some(a), None) => format!("no longer `{a}`"),
        (None, None) => String::new(),
    }
}

fn containers(before: &ScanReport, after: &ScanReport) -> Section {
    let b = by_id(&before.containers, |u| u.id.as_str());
    let a = by_id(&after.containers, |u| u.id.as_str());
    let mut changes = added_removed(&b, &a, |u| u.name.clone());
    for (id, after_unit) in &a {
        let Some(before_unit) = b.get(id) else { continue };
        let mut details = Vec::new();
        if before_unit.kind != after_unit.kind {
            details.push(format!("kind {}", transition(Some(before_unit.kind.role()), Some(after_unit.kind.role()))));
        }
        if before_unit.language != after_unit.language {
            details.push(format!(
                "language {}",
                transition(Some(before_unit.language.display()), Some(after_unit.language.display()))
            ));
        }
        let gained: Vec<&String> =
            after_unit.frameworks.iter().filter(|f| !before_unit.frameworks.contains(f)).collect();
        let lost: Vec<&String> = before_unit.frameworks.iter().filter(|f| !after_unit.frameworks.contains(f)).collect();
        if !gained.is_empty() {
            details.push(format!("uses {}", join_code(&gained)));
        }
        if !lost.is_empty() {
            details.push(format!("no longer uses {}", join_code(&lost)));
        }
        if !details.is_empty() {
            changes.push(Change { verb: Verb::Changed, subject: after_unit.name.clone(), details });
        }
    }
    Section { title: "Services".into(), changes }
}

fn join_code<S: AsRef<str>>(items: &[S]) -> String {
    items.iter().map(|s| format!("`{}`", s.as_ref())).collect::<Vec<_>>().join(", ")
}

fn infrastructure(before: &ScanReport, after: &ScanReport) -> Section {
    let b = by_id(&before.infrastructure, |i| i.id.as_str());
    let a = by_id(&after.infrastructure, |i| i.id.as_str());
    Section {
        title: "Datastores, queues and external services".into(),
        changes: added_removed(&b, &a, |i| i.label.clone()),
    }
}

fn relationships(before: &ScanReport, after: &ScanReport) -> Section {
    // Keyed by endpoints and kind, not by the generated id: an id carries a hash
    // of the evidence, so moving a call to another line would read as a rewire.
    let key = |r: &crate::scan::Relationship| format!("{}\u{1}{}\u{1}{:?}", r.source, r.target, r.edge_type);
    let b: BTreeMap<String, &crate::scan::Relationship> = before.relationships.iter().map(|r| (key(r), r)).collect();
    let a: BTreeMap<String, &crate::scan::Relationship> = after.relationships.iter().map(|r| (key(r), r)).collect();
    let name = |report: &ScanReport, id: &str| -> String {
        report
            .containers
            .iter()
            .find(|u| u.id == id)
            .map(|u| u.name.clone())
            .or_else(|| report.infrastructure.iter().find(|i| i.id == id).map(|i| i.label.clone()))
            .unwrap_or_else(|| id.to_string())
    };
    let mut changes = Vec::new();
    for (k, r) in &a {
        if !b.contains_key(k) {
            changes.push(Change {
                verb: Verb::Added,
                subject: format!("{} → {}", name(after, &r.source), name(after, &r.target)),
                details: vec![],
            });
        }
    }
    for (k, r) in &b {
        if !a.contains_key(k) {
            changes.push(Change {
                verb: Verb::Removed,
                subject: format!("{} → {}", name(before, &r.source), name(before, &r.target)),
                details: vec![],
            });
        }
    }
    Section { title: "Connections".into(), changes }
}

fn model_of(t: Option<&crate::api::TypeRef>) -> Option<&str> {
    t.and_then(|t| t.model.as_deref().or(Some(t.type_name.as_str())))
}

fn param_names(op: &Operation) -> BTreeSet<String> {
    op.params.iter().map(|p| format!("{} {}", p.location, p.name)).collect()
}

/// Field names of a model, or `None` when the id names no model we have.
fn field_names<'a>(api: &'a ApiModel, id: Option<&str>) -> Option<BTreeSet<&'a str>> {
    let id = id?;
    let m = api.models.iter().find(|m| m.id == id)?;
    Some(m.fields.iter().map(|f| f.name.as_str()).collect())
}

/// Fields a request or response gained and lost, when both sides name a model we
/// have. An inline response literal keeps its generated name while its shape
/// changes, so comparing the name alone reports nothing.
fn shape_change(
    what: &str,
    before_api: &ApiModel,
    after_api: &ApiModel,
    before_id: Option<&str>,
    after_id: Option<&str>,
) -> Option<String> {
    let b = field_names(before_api, before_id)?;
    let a = field_names(after_api, after_id)?;
    let gained: Vec<&str> = a.difference(&b).copied().collect();
    let lost: Vec<&str> = b.difference(&a).copied().collect();
    if gained.is_empty() && lost.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if !gained.is_empty() {
        parts.push(format!("gains {}", join_code(&gained)));
    }
    if !lost.is_empty() {
        parts.push(format!("loses {}", join_code(&lost)));
    }
    Some(format!("{what} {}", parts.join(" and ")))
}

fn operation_details(
    before: &Operation,
    after: &Operation,
    before_api: &ApiModel,
    after_api: &ApiModel,
) -> Vec<String> {
    let mut details = Vec::new();
    let bq = model_of(before.request_body.as_ref());
    let aq = model_of(after.request_body.as_ref());
    if bq != aq {
        details.push(format!("request {}", transition(bq, aq)));
    } else if let Some(d) = shape_change("request", before_api, after_api, bq, aq) {
        details.push(d);
    }
    let bs = model_of(before.response.as_ref());
    let as_ = model_of(after.response.as_ref());
    if bs != as_ {
        details.push(format!("response {}", transition(bs, as_)));
    } else if let Some(d) = shape_change("response", before_api, after_api, bs, as_) {
        details.push(d);
    }
    if before.success_status != after.success_status {
        details.push(format!(
            "success status {}",
            transition(
                before.success_status.map(|s| s.to_string()).as_deref(),
                after.success_status.map(|s| s.to_string()).as_deref()
            )
        ));
    }
    let ba: BTreeSet<String> = before.auth.iter().map(|a| format!("{}: {}", a.kind, a.detail)).collect();
    let aa: BTreeSet<String> = after.auth.iter().map(|a| format!("{}: {}", a.kind, a.detail)).collect();
    if ba != aa {
        // Losing every requirement is the one difference worth spelling out:
        // a reviewer reading "authentication changed" may not notice it went away.
        if aa.is_empty() {
            details.push("no authentication requirement is recognised any more".into());
        } else if ba.is_empty() {
            details.push(format!("now requires {}", join_code(&aa.iter().collect::<Vec<_>>())));
        } else {
            details.push(format!(
                "authentication {}",
                transition(
                    Some(&ba.iter().cloned().collect::<Vec<_>>().join(", ")),
                    Some(&aa.iter().cloned().collect::<Vec<_>>().join(", "))
                )
            ));
        }
    }
    let bp = param_names(before);
    let ap = param_names(after);
    let gained: Vec<&String> = ap.difference(&bp).collect();
    let lost: Vec<&String> = bp.difference(&ap).collect();
    if !gained.is_empty() {
        details.push(format!("takes {}", join_code(&gained)));
    }
    if !lost.is_empty() {
        details.push(format!("no longer takes {}", join_code(&lost)));
    }
    details
}

fn label_op(op: &Operation) -> String {
    format!("{} {} {}", op.unit, op.method, op.path)
}

fn operations(before: Option<&ApiModel>, after: Option<&ApiModel>) -> Section {
    let empty = ApiModel::default();
    let before = before.unwrap_or(&empty);
    let after = after.unwrap_or(&empty);
    let b = by_id(&before.operations, |o| o.id.as_str());
    let a = by_id(&after.operations, |o| o.id.as_str());
    let mut changes = added_removed(&b, &a, label_op);
    for (id, after_op) in &a {
        let Some(before_op) = b.get(id) else { continue };
        let details = operation_details(before_op, after_op, before, after);
        if !details.is_empty() {
            changes.push(Change { verb: Verb::Changed, subject: label_op(after_op), details });
        }
    }
    Section { title: "API operations".into(), changes }
}

fn entities(before: Option<&DataModel>, after: Option<&DataModel>) -> Section {
    let empty = DataModel::default();
    let before = before.unwrap_or(&empty);
    let after = after.unwrap_or(&empty);
    let b = by_id(&before.entities, |e| e.id.as_str());
    let a = by_id(&after.entities, |e| e.id.as_str());
    let mut changes = added_removed(&b, &a, |e| e.name.clone());
    for (id, after_entity) in &a {
        let Some(before_entity) = b.get(id) else { continue };
        let bc: BTreeSet<&str> = before_entity.columns.iter().map(|c| c.name.as_str()).collect();
        let ac: BTreeSet<&str> = after_entity.columns.iter().map(|c| c.name.as_str()).collect();
        let mut details = Vec::new();
        let gained: Vec<&&str> = ac.difference(&bc).collect();
        let lost: Vec<&&str> = bc.difference(&ac).collect();
        if !gained.is_empty() {
            details.push(format!("gains {}", join_code(&gained.iter().map(|s| **s).collect::<Vec<_>>())));
        }
        if !lost.is_empty() {
            details.push(format!("loses {}", join_code(&lost.iter().map(|s| **s).collect::<Vec<_>>())));
        }
        // A column whose type changed is a migration; one that became nullable
        // or stopped being a key is a contract change for every reader.
        for after_col in &after_entity.columns {
            let Some(before_col) = before_entity.columns.iter().find(|c| c.name == after_col.name) else { continue };
            if before_col.type_name != after_col.type_name {
                details.push(format!(
                    "`{}` {}",
                    after_col.name,
                    transition(Some(&before_col.type_name), Some(&after_col.type_name))
                ));
            }
            if before_col.nullable != after_col.nullable {
                details.push(format!(
                    "`{}` is {} nullable",
                    after_col.name,
                    if after_col.nullable { "now" } else { "no longer" }
                ));
            }
        }
        if !details.is_empty() {
            changes.push(Change { verb: Verb::Changed, subject: after_entity.name.clone(), details });
        }
    }
    Section { title: "Data model".into(), changes }
}

/// What changed between two scans of the same repository.
pub fn diff(before: &ScanReport, after: &ScanReport) -> Diff {
    let sections = vec![
        containers(before, after),
        relationships(before, after),
        operations(before.api.as_ref(), after.api.as_ref()),
        entities(before.data.as_ref(), after.data.as_ref()),
        infrastructure(before, after),
    ];
    Diff {
        base: String::new(),
        head: String::new(),
        base_commit: before.repo.commit_hash.clone(),
        head_commit: after.repo.commit_hash.clone(),
        sections: sections.into_iter().filter(|s| !s.changes.is_empty()).collect(),
    }
}

/// The diff as a PR comment: a heading, one list per section, nothing else.
///
/// Rendered here rather than in the CLI so `autodoc diff` and the MCP server
/// produce the same comment.
pub fn markdown(d: &Diff) -> String {
    let mut out = String::new();
    let range = match (&d.base_commit, &d.head_commit) {
        (Some(b), Some(h)) => format!("`{}` → `{}`", &b[..b.len().min(8)], &h[..h.len().min(8)]),
        _ => format!("`{}` → `{}`", d.base, d.head),
    };
    if d.is_empty() {
        out.push_str(&format!("### Architecture unchanged\n\nNothing this book describes differs between {range}.\n"));
        return out;
    }
    let n = d.total();
    out.push_str(&format!(
        "### Architecture changes\n\n{n} change{} between {range}.\n",
        if n == 1 { "" } else { "s" }
    ));
    for section in &d.sections {
        out.push_str(&format!("\n**{}**\n\n", section.title));
        for c in &section.changes {
            out.push_str(&format!("- {} **{}**", c.verb.word(), c.subject));
            if !c.details.is_empty() {
                out.push_str(&format!(" — {}", c.details.join("; ")));
            }
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identical_report_has_no_changes() {
        let d = Diff::default();
        assert!(d.is_empty());
        assert_eq!(d.total(), 0);
        assert!(markdown(&d).contains("Architecture unchanged"));
    }

    #[test]
    fn a_transition_reads_in_both_directions() {
        assert_eq!(transition(Some("a"), Some("b")), "`a` → `b`");
        assert_eq!(transition(None, Some("b")), "now `b`");
        assert_eq!(transition(Some("a"), None), "no longer `a`");
    }
}
