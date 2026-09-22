//! How the architecture changed, release by release: `history.json` beside
//! the book.
//!
//! A book records the commit it describes and nothing before it. `nunki diff`
//! has always been able to answer "what changed between these two releases",
//! and every answer was discarded the moment it was read — on every pull
//! request, and at every tag. This keeps them.
//!
//! Entries are **appended at release time and committed**, never recomputed.
//! That is not a convenience: rebuilding the history at build time would make
//! the book depend on the repository's tags rather than on its own commit,
//! which `check` cannot allow, and would cost a full scan of every release on
//! every build. It is the same rule `authored.json` follows — written once,
//! and never rewritten by a later run.
//!
//! An entry names the two commits it was computed between, so a reader can
//! reproduce it rather than take it on faith. The changes themselves carry no
//! line citations, because `diff` compares two models rather than reading
//! lines, and claiming otherwise would be the kind of borrowed authority this
//! project exists to avoid.

use std::path::Path;

use nunki_analyzer::diff::Diff;
use serde::{Deserialize, Serialize};

pub const HISTORY: &str = "history.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// The release this describes, as it was tagged.
    pub release: String,
    /// When it was released, from the commit it points at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// What it was compared against — usually the previous release.
    pub base: String,
    pub diff: Diff,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    #[serde(default)]
    pub entries: Vec<Entry>,
}

impl History {
    /// Reads `history.json`; a malformed file is reported and treated as empty
    /// so a hand-edit never blocks a build.
    pub fn load(out_dir: &Path) -> (History, Option<String>) {
        let path = out_dir.join(HISTORY);
        let Ok(text) = std::fs::read_to_string(&path) else { return (History::default(), None) };
        match serde_json::from_str(&text) {
            Ok(h) => (h, None),
            Err(e) => (History::default(), Some(format!("ignored {HISTORY}: {e}"))),
        }
    }

    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default() + "\n"
    }

    /// Adds an entry, newest first, replacing one for the same release.
    ///
    /// Re-recording a release overwrites that entry and leaves every other one
    /// untouched — a tag that had to be moved should not silently keep the
    /// diff from the build that was thrown away.
    pub fn record(&mut self, entry: Entry) -> bool {
        let existed = self.entries.iter().any(|e| e.release == entry.release);
        self.entries.retain(|e| e.release != entry.release);
        self.entries.insert(0, entry);
        existed
    }
}
