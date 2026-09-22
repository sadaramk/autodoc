//! The book is data first: pages are block trees with inline citations. The
//! HTML reader, the Markdown mirror and `llms.txt` are all projections of it.

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Book {
    pub meta: BookMeta,
    pub nav: Vec<NavGroup>,
    pub pages: Vec<Page>,
    pub diagrams: BTreeMap<String, Figure>,
    pub cites: BTreeMap<String, Cite>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookMeta {
    pub name: String,
    pub repo: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The release this book documents, when it was generated from a tagged
    /// commit. A commit is what was read; a release is what people cite.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_url: Option<String>,
    /// Path of the scanned directory inside the repository ("" at the root).
    #[serde(default)]
    pub path_prefix: String,
    pub files: usize,
    pub lines: u64,
    pub languages: Vec<(String, usize)>,
    pub evidence: EvidenceHealth,
    /// The other repositories of this system that were read, and the commit each
    /// was read at. A citation into one of them is only meaningful at a commit,
    /// so the commit is part of the book: when a member moves, the book is out
    /// of date and `check` says so.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<MemberMeta>,
    pub generator: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemberMeta {
    /// As citations name it: the repository's directory.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceHealth {
    pub total: usize,
    pub verified: usize,
    pub stale: usize,
    pub broken: usize,
    pub unverified: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavGroup {
    pub title: String,
    pub items: Vec<NavItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavItem {
    pub page: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    /// Route id, e.g. `overview` or `containers/api-gateway`.
    pub id: String,
    pub title: String,
    /// Section eyebrow ("Architecture", "Container").
    pub section: String,
    pub summary: Vec<Inline>,
    pub blocks: Vec<Block>,
    /// Markdown mirror path relative to the book root.
    pub md_path: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum Inline {
    Text {
        v: String,
    },
    Code {
        v: String,
    },
    Strong {
        v: String,
    },
    /// Link to another page (and optional heading anchor).
    Link {
        page: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        anchor: Option<String>,
        v: String,
    },
    Cite {
        id: String,
    },
    /// `observed` (code), `declared` (compose/manifest), `muted`, `warn`.
    Badge {
        tone: String,
        v: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum Block {
    Heading {
        level: u8,
        id: String,
        text: String,
    },
    Para {
        inl: Vec<Inline>,
    },
    Stats {
        items: Vec<Stat>,
    },
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    Figure {
        diagram: String,
        caption: Vec<Inline>,
    },
    Callout {
        tone: String,
        title: String,
        inl: Vec<Inline>,
    },
    List {
        items: Vec<Vec<Inline>>,
    },
    Cards {
        cards: Vec<Card>,
    },
    /// A walkthrough bound to a figure: each step highlights one edge.
    Steps {
        diagram: String,
        steps: Vec<Step>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stat {
    pub label: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub page: String,
    pub title: String,
    pub text: String,
    pub meta: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub edge: String,
    pub title: Vec<Inline>,
    pub body: Vec<Inline>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Figure {
    pub id: String,
    pub title: String,
    /// Inline SVG (viewBox-scaled, keyboard-operable nodes).
    pub svg: String,
    pub width: f64,
    pub height: f64,
    /// Diagram node id → where clicking it goes.
    pub nodes: BTreeMap<String, NodeTarget>,
    pub ir_path: String,
    pub svg_path: String,
    pub density: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeTarget {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cite: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Cite {
    pub id: String,
    /// The member repository this was read from. Absent means the repository the
    /// book describes — a file path is only an address once you know its root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    pub file: String,
    pub start: u32,
    pub end: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// `verified`, `stale`, `untracked`, `unverified`, `file-missing`, …
    pub state: String,
    /// Omitted when it says no more than the state does (every verified citation
    /// in a large book would otherwise repeat the same sentence).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    /// Omitted when it is the forge URL derived from the book's repository,
    /// commit and path prefix; readers rebuild it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permalink: Option<String>,
    /// Omitted when it is `<file>#L<start>-L<end>` or the permalink.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
}

impl Cite {
    pub fn anchor(&self) -> String {
        if self.start == self.end {
            format!("#L{}", self.start)
        } else {
            format!("#L{}-L{}", self.start, self.end)
        }
    }

    /// Forge URL for a citation, from the book's repository metadata.
    pub fn permalink_in(&self, meta: &BookMeta) -> Option<String> {
        if self.permalink.is_some() {
            return self.permalink.clone();
        }
        // A citation from a member repository is never derivable from this
        // book's repository and commit: deriving one would produce a URL that
        // resolves, in the wrong repository, to whatever happens to sit at
        // those lines.
        if self.repo.is_some() {
            return None;
        }
        let (base, commit) = (meta.web_url.as_deref()?, meta.commit.as_deref()?);
        Some(format!("{base}/blob/{commit}/{}{}{}", meta.path_prefix, self.file, self.anchor()))
    }

    pub fn git_ref_in(&self, meta: &BookMeta) -> String {
        self.git_ref
            .clone()
            .or_else(|| self.permalink_in(meta))
            .unwrap_or_else(|| format!("{}{}", self.file, self.anchor()))
    }
}

impl Inline {
    pub fn text(v: impl Into<String>) -> Inline {
        Inline::Text { v: v.into() }
    }
    pub fn code(v: impl Into<String>) -> Inline {
        Inline::Code { v: v.into() }
    }
    pub fn strong(v: impl Into<String>) -> Inline {
        Inline::Strong { v: v.into() }
    }
    pub fn link(page: impl Into<String>, v: impl Into<String>) -> Inline {
        Inline::Link { page: page.into(), anchor: None, v: v.into() }
    }
    pub fn badge(tone: &str, v: impl Into<String>) -> Inline {
        Inline::Badge { tone: tone.into(), v: v.into() }
    }
}

/// Plain-text rendering of inlines (search index, alt text, llms.txt).
pub fn plain(inl: &[Inline], cites: &BTreeMap<String, Cite>) -> String {
    let mut out = String::new();
    for i in inl {
        match i {
            Inline::Text { v } | Inline::Code { v } | Inline::Strong { v } | Inline::Badge { v, .. } => out.push_str(v),
            Inline::Link { v, .. } => out.push_str(v),
            Inline::Cite { id } => {
                if let Some(c) = cites.get(id) {
                    out.push_str(&format!(" [{}:{}-{}]", c.file, c.start, c.end));
                }
            }
        }
    }
    out
}
