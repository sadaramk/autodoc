//! Repository analyzer: tree-sitter extraction for Rust, TypeScript/JavaScript,
//! Go and Python, lifted into a C4 model (system → containers → components)
//! where every element and relationship is pinned to file + line evidence.

pub mod api;
pub mod capability;
pub mod catalog;
pub mod compose;
pub mod data;
pub mod diff;
pub mod domains;
pub mod draft;
mod entry;
pub mod extract;
mod jvm;
pub mod lang;
pub mod manifest;
pub mod modules;
pub mod scan;
pub mod source;
pub mod topology;
pub mod trace;
pub mod views;

pub use capability::draft_capability_ir;
pub use draft::{draft_component_ir, draft_ir, Draft, DraftOptions};
pub use scan::{scan, Depth, EvidenceRef, Relationship, ScanOptions, ScanReport, UnitKind};
pub use views::{draft_entity_ir, draft_lifecycle_ir, draft_sequence_ir};

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("cannot scan {0}: {1}")]
    Root(String, String),
    #[error(
        "nothing to document in {0}: no build manifest, container file or source \
         in a language autodoc reads (Rust, TypeScript, Go, Python, Java, Kotlin)"
    )]
    Empty(String),
    #[error("unknown focus unit `{0}`; available: {available}", available = .1.join(", "))]
    UnknownFocus(String, Vec<String>),
}
