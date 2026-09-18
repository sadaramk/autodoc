//! Read-only view of the parsed workspace handed to behaviour extractors
//! (API contracts, data models, state machines, call traces).

use std::path::PathBuf;

use crate::extract::FileFacts;
use crate::lang::Language;
use crate::scan::{EvidenceRef, UnitSummary};

pub struct SourceFile<'a> {
    /// Path relative to the repository root.
    pub path: &'a str,
    pub language: Language,
    /// Id of the unit (container) that owns the file.
    pub unit: &'a str,
    pub facts: &'a FileFacts,
}

pub struct SourceIndex<'a> {
    pub root: PathBuf,
    pub units: &'a [UnitSummary],
    pub files: Vec<SourceFile<'a>>,
}

impl<'a> SourceIndex<'a> {
    pub fn read(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(self.root.join(path)).ok()
    }

    pub fn unit(&self, id: &str) -> Option<&'a UnitSummary> {
        self.units.iter().find(|u| u.id == id)
    }

    pub fn files_of<'s>(&'s self, unit: &'s str) -> impl Iterator<Item = &'s SourceFile<'a>> + 's {
        self.files.iter().filter(move |f| f.unit == unit)
    }
}

pub fn line_ref(path: &str, line: u32) -> EvidenceRef {
    EvidenceRef { file_path: path.to_string(), start_line: line, end_line: line, symbol_name: None, note: None }
}
