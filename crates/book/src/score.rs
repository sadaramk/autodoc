//! How much of a system the book actually specifies, as a number.
//!
//! nunki already measures the right things and reports them in four places: the
//! share of the source it could read, how many citations verify against the
//! pinned commit, how many operations declared a contract, and where an answer
//! can only come from a person. None of that says, in one figure, *how much of
//! this system is specified* — so there is nothing to watch and nothing to
//! regress against.
//!
//! Every dimension here is a ratio whose **denominator is read from the
//! source**. That is the whole discipline: a score built from the shape of a
//! document says how well the document was filled in, which a document can
//! satisfy while being wrong about the system. A score built from what the code
//! contains says how much of the code the document accounts for.
//!
//! Two consequences worth stating, because both are places a score of this kind
//! usually goes wrong:
//!
//! * **Not applicable is not zero.** A library has no HTTP operations and a
//!   command-line tool has no entities. Scoring either against a dimension it
//!   cannot have measures the repository's shape, not the book's quality, so a
//!   dimension with nothing to count is left out of the total rather than
//!   counted as failure.
//! * **What the code did not declare and what nunki could not read are
//!   different failures.** An Express handler that destructures its request at
//!   runtime has no contract to find; a Vue file has one nunki cannot parse.
//!   The first is a fact about the repository and the second is a limit of this
//!   tool, and a reader deciding whether to trust the book needs to tell them
//!   apart — so they stay separate dimensions and the total never merges them
//!   into a single excuse.

use nunki_analyzer::api::Confidence;
use nunki_analyzer::ScanReport;
use serde::Serialize;

use crate::authored::{given, Authored};
use crate::model::Cite;

/// One measured ratio. `of` is what the source contains; `got` is how much of
/// it the book accounts for.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dimension {
    pub name: &'static str,
    pub got: usize,
    pub of: usize,
    /// What the denominator counts, in the reader's terms.
    pub unit: &'static str,
    /// Why the number is what it is, when a bare ratio would mislead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl Dimension {
    fn new(name: &'static str, unit: &'static str, got: usize, of: usize) -> Self {
        Self { name, got, of, unit, note: None }
    }

    fn noted(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// 0.0..=1.0, or `None` when the source has nothing of this kind to count.
    pub fn share(&self) -> Option<f64> {
        (self.of > 0).then(|| self.got as f64 / self.of as f64)
    }

    pub fn percent(&self) -> Option<u32> {
        // Truncated, not rounded: 99.6% of citations verified is not 100%, and
        // a book that prints 100% while one citation is stale is exactly the
        // failure this project exists to prevent.
        self.share().map(|s| (s * 100.0) as u32)
    }
}

/// Every dimension, and the one number they make.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    pub dimensions: Vec<Dimension>,
    /// 0..=100, the mean of the dimensions the source has anything to say
    /// about. `None` for a repository with nothing to score at all.
    pub total: Option<u32>,
}

impl Score {
    /// The dimensions that counted toward the total — the rest had no
    /// denominator, which is a fact about the repository, not a failing.
    pub fn counted(&self) -> impl Iterator<Item = &Dimension> {
        self.dimensions.iter().filter(|d| d.of > 0)
    }

    /// The weakest counted dimension: what to fix first, and the reason a total
    /// on its own is not worth printing.
    pub fn weakest(&self) -> Option<&Dimension> {
        self.counted().min_by(|a, b| {
            a.share().unwrap_or(1.0).partial_cmp(&b.share().unwrap_or(1.0)).unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// `specified 76% of 5 measures · source read 100%, …`
    ///
    /// The count is not decoration. A total drawn from two measures is not the
    /// same claim as one drawn from six, and a reader comparing two books has
    /// no way to tell without being told.
    pub fn line(&self) -> String {
        let parts: Vec<String> =
            self.counted().map(|d| format!("{} {}%", d.name.to_lowercase(), d.percent().unwrap_or(0))).collect();
        match self.total {
            Some(t) => format!(
                "specified {t}% across {} of {} measures · {}",
                parts.len(),
                self.dimensions.len(),
                parts.join(", ")
            ),
            None => "nothing to score".into(),
        }
    }
}

/// Measure a built book against the source it was read from.
///
/// Deliberately takes the pieces rather than `Built`: the score is a function
/// of the report, the citations and the authored answers, and nothing else. A
/// dimension that needed the rendered pages would be measuring the renderer.
pub fn score(report: &ScanReport, cites: &[&Cite], authored: &Authored) -> Score {
    let mut dimensions = vec![read(report), contracts(report), traced(report), described(report)];
    dimensions.push(verified(cites));
    dimensions.push(intent(report, authored));

    let shares: Vec<f64> = dimensions.iter().filter_map(Dimension::share).collect();
    let total = (!shares.is_empty()).then(|| (shares.iter().sum::<f64>() / shares.len() as f64 * 100.0) as u32);
    Score { dimensions, total }
}

/// How much of the source nunki could read. A limit of this tool, not of the
/// repository — the one dimension here that is nunki's own failing.
fn read(report: &ScanReport) -> Dimension {
    let unread: usize = report.stats.unparsed.values().sum();
    let d = Dimension::new("Source read", "source files", report.stats.files, report.stats.files + unread);
    match unread {
        0 => d,
        _ => d.noted(format!("{} unread: {}", unread, report.stats.unread_census())),
    }
}

/// Entry points whose contract the code declares. A fact about the repository:
/// a handler that destructures its request at runtime has no contract to find,
/// and saying so is more useful than guessing one.
///
/// An entry point is an operation *or* a command. A tool run from a terminal
/// publishes its contract in `clap` derives rather than in request and response
/// types, and counting only the HTTP kind would score every command-line tool
/// on an API it was never going to have — which is how nunki's own book first
/// scored 100% on two measures while saying nothing about the thing it is.
fn contracts(report: &ScanReport) -> Dimension {
    let ops = report.api.as_ref().map(|a| a.operations.as_slice()).unwrap_or_default();
    let typed = ops.iter().filter(|o| o.confidence == Confidence::Typed).count();
    let partial = ops.iter().filter(|o| o.confidence == Confidence::Partial).count();

    // A command declares its contract when it says what it is for and every
    // argument says what it takes — that is what makes a command reference
    // worth reading rather than a list of names.
    let commands = || report.cli.iter().flat_map(|c| c.commands.iter());
    let documented = commands().filter(|c| c.about.is_some() && c.args.iter().all(|a| a.doc.is_some())).count();
    let total = ops.len() + commands().count();

    let d = Dimension::new("Contract declared", "entry points", typed + documented, total);
    match partial {
        0 => d,
        _ => d.noted(format!("{partial} operation(s) declared part of a contract and are counted as neither")),
    }
}

/// Operations whose behaviour was followed through the call graph. An operation
/// nobody traced is a name in a table; a traced one is a story.
///
/// Counted over operations, not over the entry points `contracts` counts: a
/// command is traced by reading its arguments, which is the same act as
/// declaring it, so counting commands here would score the same fact twice.
fn traced(report: &ScanReport) -> Dimension {
    let Some(api) = &report.api else {
        return Dimension::new("Behaviour traced", "operations", 0, 0);
    };
    let traced = api.operations.iter().filter(|o| report.flows.iter().any(|f| f.entry == o.id)).count();
    Dimension::new("Behaviour traced", "operations", traced, api.operations.len())
}

/// Entities whose fields were read. A table name with no columns names a store;
/// it does not describe one.
fn described(report: &ScanReport) -> Dimension {
    let Some(data) = &report.data else {
        return Dimension::new("Data described", "entities", 0, 0);
    };
    let described = data.entities.iter().filter(|e| !e.columns.is_empty()).count();
    Dimension::new("Data described", "entities", described, data.entities.len())
}

/// Citations that still say what they said at the pinned commit. The dimension
/// `check` already enforces, counted here so it sits beside the others.
fn verified(cites: &[&Cite]) -> Dimension {
    let ok = cites.iter().filter(|c| c.state == "verified").count();
    Dimension::new("Evidence verified", "citations", ok, cites.len())
}

/// Questions only a person can answer, that a person has answered. nunki
/// refuses to invent these, so an empty `authored.json` is not a defect in the
/// book — but it is the difference between what was built and why.
fn intent(report: &ScanReport, authored: &Authored) -> Dimension {
    let Some(api) = &report.api else {
        return Dimension::new("Intent authored", "operations", 0, 0);
    };
    let answered = api
        .operations
        .iter()
        .filter(|o| authored.operation(&o.id).is_some_and(|i| given(&i.actor).is_some() && given(&i.purpose).is_some()))
        .count();
    let d = Dimension::new("Intent authored", "operations", answered, api.operations.len());
    match answered < api.operations.len() {
        true => d.noted("actor and purpose come from `authored.json`, never from the code"),
        false => d,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dim(got: usize, of: usize) -> Dimension {
        Dimension::new("D", "things", got, of)
    }

    #[test]
    fn a_dimension_the_source_has_nothing_of_is_not_a_failure() {
        // The distinction the whole score rests on: a library with no HTTP
        // operations must not be scored as if it had failed to document them.
        assert_eq!(dim(0, 0).share(), None);
        assert_eq!(dim(0, 3).share(), Some(0.0));
    }

    #[test]
    fn the_total_is_the_mean_of_what_could_be_counted() {
        let s = Score { dimensions: vec![dim(1, 1), dim(1, 2), dim(0, 0)], total: None };
        let shares: Vec<f64> = s.dimensions.iter().filter_map(Dimension::share).collect();
        assert_eq!(shares, vec![1.0, 0.5]);
        assert_eq!(s.counted().count(), 2);
    }

    #[test]
    fn an_almost_verified_book_does_not_round_up_to_whole() {
        // 249 of 250 is 99.6%. Printing 100% for a book with a stale citation
        // is the one thing this project exists to prevent.
        assert_eq!(dim(249, 250).percent(), Some(99));
    }

    #[test]
    fn the_weakest_dimension_is_the_one_to_fix() {
        let s = Score { dimensions: vec![dim(9, 10), dim(1, 10), dim(0, 0)], total: None };
        assert_eq!(s.weakest().map(|d| d.got), Some(1));
    }
}
