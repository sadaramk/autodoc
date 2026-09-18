//! Human-authored intent: `authored.json` next to the book.
//!
//! Code tells us what the system does; it cannot tell us who it is for, why
//! it exists or how success is measured. Those answers live here, written by
//! people, and the book shows them apart from everything measured, badged
//! "authored". Anything left empty is rendered as an explicit gap, never
//! filled in by inference. The generator creates the file once, listing every
//! operation it found, and never overwrites it.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const AUTHORED: &str = "authored.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Authored {
    #[serde(rename = "$comment", skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    pub business: Business,
    /// Keyed by operation id (`<unit>:<METHOD> <path>`).
    pub operations: BTreeMap<String, OperationIntent>,
    pub glossary: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Business {
    pub problem: String,
    pub goals: Vec<String>,
    pub non_goals: Vec<String>,
    pub stakeholders: Vec<Stakeholder>,
    pub success_metrics: Vec<Metric>,
    pub assumptions: Vec<String>,
    pub constraints: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Stakeholder {
    pub name: String,
    pub role: String,
    pub interest: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Metric {
    pub metric: String,
    pub target: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OperationIntent {
    /// Capability name in business language ("Place order").
    pub name: String,
    /// Who performs it ("Shopper", "Back-office clerk", "Nightly job").
    pub actor: String,
    /// Why it exists.
    pub purpose: String,
    /// Acceptance criteria beyond what the code enforces.
    pub acceptance: Vec<String>,
}

/// Non-empty trimmed text.
pub fn given(s: &str) -> Option<&str> {
    Some(s.trim()).filter(|s| !s.is_empty())
}

pub fn given_list(items: &[String]) -> Vec<&str> {
    items.iter().filter_map(|s| given(s)).collect()
}

impl Authored {
    /// Reads `authored.json`; a malformed file is reported and treated as empty
    /// so a typo never blocks regeneration.
    pub fn load(out_dir: &Path) -> (Authored, Option<String>) {
        let path = out_dir.join(AUTHORED);
        let Ok(text) = std::fs::read_to_string(&path) else { return (Authored::default(), None) };
        match serde_json::from_str(&text) {
            Ok(a) => (a, None),
            Err(e) => (Authored::default(), Some(format!("ignored {AUTHORED}: {e}"))),
        }
    }

    /// Starter file listing every operation with empty answers.
    pub fn template(operation_ids: &[String]) -> String {
        let a = Authored {
            comment: Some(
                "Intent the code cannot tell. Fill in what you know; empty answers are shown as open questions. autodoc never writes to this file after creating it.".into(),
            ),
            business: Business {
                stakeholders: vec![Stakeholder::default()],
                success_metrics: vec![Metric::default()],
                ..Default::default()
            },
            operations: operation_ids.iter().map(|id| (id.clone(), OperationIntent::default())).collect(),
            glossary: BTreeMap::new(),
        };
        serde_json::to_string_pretty(&a).unwrap() + "\n"
    }

    pub fn operation(&self, id: &str) -> Option<&OperationIntent> {
        self.operations.get(id)
    }
}
