//! The behaviour model as data: `behaviour.json` beside the book.
//!
//! The book renders requirements, rules, contracts and entities as prose. That
//! is what a person reads, and it is useless to everything else — a Markdown
//! table cannot be compared against a specification someone else wrote, handed
//! to an agent, or diffed by anything that wants more than page text.
//!
//! The API and data models already serialise; they were simply never written
//! out. The requirements layer did not exist as data at all: it was derived
//! inside the page builder and rendered straight to prose.
//!
//! Every item carries the identifier it is cited by and the evidence it was
//! verified against, so a consumer can follow any claim back to the line and
//! the commit rather than trusting this file.

use nunki_analyzer::scan::EvidenceRef;
use serde::Serialize;

/// Identifies the build so a consumer can tell what it is looking at, and
/// re-verify against the same commit rather than against whatever is checked
/// out now.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    pub generator: String,
    pub repository: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// How much of the source the scan could read, so a consumer can weigh a
    /// silence: nothing found is different from nothing looked at.
    pub files_read: usize,
    pub citations_verified: usize,
    pub citations_total: usize,
}

/// One thing the system does, derived from an operation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Requirement {
    /// Stable across additions and removals elsewhere; safe to cite.
    pub id: String,
    /// The operation this was derived from, in the analyzer's own vocabulary.
    pub operation: String,
    pub unit: String,
    pub method: String,
    pub path: String,
    /// True when the path could not be fully resolved, so a consumer does not
    /// compare a partial path against a declared one and call it a mismatch.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub path_partial: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success_status: Option<u16>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub error_statuses: Vec<u16>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub auth: Vec<String>,
    /// Rules this operation must satisfy, by rule id.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<String>,
    pub evidence: EvidenceRef,
}

/// A constraint the code enforces.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: String,
    pub statement: String,
    pub kind: String,
    /// Requirement ids this constrains — the requirement's id, not the raw
    /// operation, so a consumer only has to understand one kind of key.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<String>,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Behaviour {
    pub provenance: Provenance,
    pub requirements: Vec<Requirement>,
    pub rules: Vec<Rule>,
}

impl Behaviour {
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default() + "\n"
    }
}
