//! What a command-line tool can be asked to do, read from its `clap`
//! declarations.
//!
//! A book about a CLI used to prove its dependency graph and say nothing about
//! the tool — no API reference, no contract, because behaviour was modelled as
//! HTTP operations and a CLI serves none. Yet `clap`'s derive style declares the
//! same kind of thing a route table does, on lines that can be cited:
//!
//! ```ignore
//! /// Scan a repository: C4 containers, relationships, evidence, draft IR.
//! Analyze {
//!     #[arg(default_value = ".")]
//!     path: PathBuf,
//!     /// Also scan test, fixture and example directories.
//!     #[arg(long)]
//!     include_tests: bool,
//! },
//! ```
//!
//! A command name, what it is for, its arguments with their kind, type and
//! default — and each one's own description, written by whoever wrote the flag.
//!
//! Deliberately its own model rather than an `Operation` with `protocol: "cli"`.
//! An operation has a method and a path; a command has arguments, defaults and
//! exit codes. Bending one into the other would produce a functional requirement
//! reading "the system SHALL expose CLI /generate", which is the kind of borrowed
//! shape this project avoids. See #53.
//!
//! Only the derive style is read. `clap`'s builder API expresses the same thing
//! as chained calls, and other languages have their own libraries; each is a
//! separate reading and should wait until this one has earned it.

use serde::{Deserialize, Serialize};

use crate::api::text::{self, Src};
use crate::lang::Language;
use crate::scan::EvidenceRef;
use crate::source::SourceIndex;

/// How a value reaches the program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ArgKind {
    /// `--out FILE`, taking a value.
    Option,
    /// `--force`, present or absent.
    Flag,
    /// `nunki generate PATH`, taken by position.
    Positional,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CliArg {
    /// As typed: `--include-tests`, or the placeholder for a positional.
    pub name: String,
    pub kind: ArgKind,
    /// The declared Rust type, which says what the value is.
    pub type_name: String,
    /// What happens when it is left out, when the declaration says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    pub required: bool,
    /// The argument's own doc comment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CliCommand {
    /// As typed on the command line.
    pub name: String,
    /// The variant's doc comment: what the command is for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<CliArg>,
    pub evidence: EvidenceRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CliSurface {
    /// The unit that ships the binary.
    pub unit: String,
    /// `#[command(name = "nunki")]`, else the unit's own name.
    pub program: String,
    /// `#[command(about = "…")]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    pub commands: Vec<CliCommand>,
    pub evidence: EvidenceRef,
}

/// Reads every `clap` derive surface in the repository.
pub fn extract(index: &SourceIndex) -> Vec<CliSurface> {
    let mut out: Vec<CliSurface> = Vec::new();
    for f in index.files.iter().filter(|f| f.language == Language::Rust) {
        let Some(text) = index.read(f.path) else { continue };
        if !text.contains("clap") {
            continue;
        }
        let src = Src::new(f.path, text, text::style_for(Language::Rust));
        let Some(mut surface) = program(&src, f.unit) else { continue };
        surface.commands = commands(&src);
        if surface.commands.is_empty() {
            continue;
        }
        out.push(surface);
    }
    out.sort_by(|a, b| a.program.cmp(&b.program));
    out
}

/// The `#[derive(Parser)]` type and what `#[command(...)]` says about it.
fn program(src: &Src, unit: &str) -> Option<CliSurface> {
    let at = derive_at(src, "Parser")?;
    let cmd = src.code[at..].find("#[command(").map(|i| at + i);
    let (mut program, mut about) = (None, None);
    if let Some(open) = cmd {
        let args_open = open + "#[command".len();
        if let Some(close) = text::matching(&src.code, args_open) {
            let attr = src.slice(args_open + 1, close);
            program = attr_string(attr, "name");
            about = attr_string(attr, "about");
        }
    }
    Some(CliSurface {
        unit: unit.to_string(),
        program: program.unwrap_or_else(|| unit.to_string()),
        about,
        commands: Vec::new(),
        evidence: src.ev(at),
    })
}

/// Every variant of the `#[derive(Subcommand)]` enum.
fn commands(src: &Src) -> Vec<CliCommand> {
    let Some(at) = derive_at(src, "Subcommand") else { return Vec::new() };
    let Some(brace) = src.code[at..].find('{').map(|i| at + i) else { return Vec::new() };
    let Some(end) = text::matching(&src.code, brace) else { return Vec::new() };

    let mut out = Vec::new();
    let mut i = brace + 1;
    while i < end {
        // A variant begins with an identifier at this nesting level; anything
        // else here is an attribute or whitespace.
        // A variant ends with `},` or `,`; the separator has to be stepped over
        // or the scan stops after the first one, which is what it did.
        let at = skip_separators(&src.code, i);
        let Some((s, e)) = text::ident_at(&src.code, at) else { break };
        if s >= end {
            break;
        }
        let name = src.slice(s, e).to_string();
        if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
            i = e + 1;
            continue;
        }
        let rest = &src.code[e..end.min(src.code.len())];
        // `Init { … },` carries arguments; `Schema,` takes none.
        let body_open = rest.find(['{', ',']).map(|k| e + k);
        let (args, next) = match body_open {
            Some(k) if src.code.as_bytes().get(k) == Some(&b'{') => {
                let close = text::matching(&src.code, k).unwrap_or(end);
                (arguments(src, k + 1, close), close + 1)
            }
            Some(k) => (Vec::new(), k + 1),
            None => (Vec::new(), end),
        };
        out.push(CliCommand {
            name: kebab(&name),
            about: text::doc_above(src, src.line(s), text::style_for(Language::Rust)),
            args,
            evidence: src.ev(s),
        });
        i = next;
    }
    out
}

/// The fields of one variant, each an argument.
fn arguments(src: &Src, from: usize, to: usize) -> Vec<CliArg> {
    let mut out = Vec::new();
    let mut i = from;
    while i < to {
        let j = text::skip_ws(&src.code, i);
        if j >= to {
            break;
        }
        // `#[arg(long, default_value = ".")]` sits above the field it describes.
        let mut attr = String::new();
        let mut k = j;
        while src.code[k..].starts_with("#[") {
            let open = k + 1;
            let Some(close) = text::matching(&src.code, open) else { break };
            attr.push_str(src.slice(open + 1, close));
            k = text::skip_ws(&src.code, close + 1);
        }
        let Some((s, e)) = text::ident_at(&src.code, k) else { break };
        if s >= to {
            break;
        }
        // `name: Type,`
        let Some(colon) = src.code[e..to].find(':').map(|x| e + x) else { break };
        let end = src.code[colon..to].find(',').map(|x| colon + x).unwrap_or(to);
        let field = src.slice(s, e).to_string();
        let type_name = src.slice(colon + 1, end).trim().to_string();

        let long = attr.contains("long");
        let optional = type_name.starts_with("Option<");
        let is_bool = type_name == "bool";
        let default = attr_string(&attr, "default_value");
        let placeholder = attr_string(&attr, "value_name");
        let kind = match (long, is_bool) {
            (true, true) => ArgKind::Flag,
            (true, false) => ArgKind::Option,
            (false, _) => ArgKind::Positional,
        };
        let name = match kind {
            ArgKind::Positional => placeholder.clone().unwrap_or_else(|| field.to_uppercase()),
            _ => format!("--{}", kebab(&field)),
        };
        out.push(CliArg {
            name,
            kind,
            type_name: type_name.clone(),
            // A flag is absent by default; saying so is noise.
            default: default.clone(),
            required: !optional && !is_bool && default.is_none(),
            doc: text::doc_above(src, src.line(s), text::style_for(Language::Rust)),
            evidence: src.ev(s),
        });
        i = end + 1;
    }
    out
}

/// Past whitespace, commas and any attribute, to the next thing that names
/// something.
fn skip_separators(code: &str, mut i: usize) -> usize {
    loop {
        i = text::skip_ws(code, i);
        if code[i..].starts_with(',') {
            i += 1;
            continue;
        }
        if code[i..].starts_with("#[") {
            match text::matching(code, i + 1) {
                Some(close) => {
                    i = close + 1;
                    continue;
                }
                None => return i,
            }
        }
        return i;
    }
}

/// Where `#[derive(…, What, …)]` appears, if it does.
fn derive_at(src: &Src, what: &str) -> Option<usize> {
    text::find_word(&src.code, "derive").into_iter().find_map(|at| {
        let open = src.code[at..].find('(').map(|i| at + i)?;
        let close = text::matching(&src.code, open)?;
        src.slice(open + 1, close).split(',').any(|d| d.trim() == what).then_some(at)
    })
}

/// `name = "value"` inside an attribute. The value is read from `text` rather
/// than `code`, because `code` blanks string contents — which is what makes the
/// attribute parse reliable in the first place.
fn attr_string(attr: &str, key: &str) -> Option<String> {
    // At a word boundary: a request for `name` must not be answered by the
    // `name` inside `value_name`.
    let at =
        attr.match_indices(key).find(|(i, _)| *i == 0 || !text::is_ident(attr.as_bytes()[i - 1])).map(|(i, _)| i)?;
    let rest = &attr[at + key.len()..];
    let eq = rest.find('=')?;
    if rest[..eq].trim() != "" {
        return None;
    }
    // Not `split(',')`: `about = "Verifiable, editorial architecture diagrams"`
    // holds a comma inside the literal, and splitting there leaves an
    // unterminated string that parses as nothing.
    let value = rest[eq + 1..].trim_start();
    let quote = value.find('"')?;
    let end = value[quote + 1..].find('"')? + quote + 1;
    Some(value[quote + 1..end].to_string())
}

/// `include_tests` → `include-tests`, `DrawioCsv` → `drawio-csv`: how clap spells
/// a field or a variant on the command line.
fn kebab(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for (i, c) in s.chars().enumerate() {
        if c == '_' {
            out.push('-');
        } else if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spells_names_the_way_clap_does() {
        assert_eq!(kebab("include_tests"), "include-tests");
        assert_eq!(kebab("DrawioCsv"), "drawio-csv");
        assert_eq!(kebab("Init"), "init");
        assert_eq!(kebab("emit_ir"), "emit-ir");
    }

    #[test]
    fn reads_every_argument_of_a_variant() {
        let text = r#"
#[derive(Subcommand)]
enum Command {
    /// Scan a repository.
    Analyze {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, value_enum)]
        depth: Option<DepthArg>,
        /// Container id to decompose at component depth.
        #[arg(long)]
        focus: Option<String>,
        /// Write the report.
        #[arg(long, value_name = "FILE")]
        json: Option<PathBuf>,
    },
}
"#;
        let src = Src::new("m.rs", text.to_string(), text::style_for(Language::Rust));
        let cmds = commands(&src);
        assert_eq!(cmds.len(), 1, "{cmds:?}");
        let names: Vec<&str> = cmds[0].args.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["PATH", "--depth", "--focus", "--json"], "an argument was dropped");
    }

    #[test]
    fn reads_a_value_out_of_an_attribute() {
        assert_eq!(attr_string(r#"long, default_value = ".""#, "default_value"), Some(".".into()));
        assert_eq!(attr_string(r#"name = "nunki", version"#, "name"), Some("nunki".into()));
        // `value_name` must not be answered by a request for `name`.
        assert_eq!(attr_string(r#"long, value_name = "FILE""#, "name"), None);
        assert_eq!(attr_string("long", "default_value"), None);
    }
}
