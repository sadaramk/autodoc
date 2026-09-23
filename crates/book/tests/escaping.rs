//! A book is rendered from a repository nobody here wrote.
//!
//! Every label, path and description in a book comes from the scanned
//! repository, which `SECURITY.md` names as untrusted input. The reader assigns
//! exactly one thing as markup — `canvas.innerHTML = d.svg` in
//! `crates/book/src/assets/book.js` — so an unescaped `<` anywhere in the
//! renderer is script execution in a published book, and a book is the kind of
//! file people put on a web host.
//!
//! `crates/renderer/tests/geometry.rs::html_is_self_contained_and_escapes_untrusted_text`
//! covers the standalone diagram page from a hand-built IR. This covers the book
//! path end to end, with nothing hand-built in between: a repository whose own
//! name is markup, scanned, drafted, validated and rendered.
//!
//! The hostile string is the repository's **directory name**, which is what
//! actually becomes a node label, a figure title and the system's name. An
//! earlier version of this test put it in `package.json` and in the README, and
//! passed while proving nothing: the unit name comes from the directory, and
//! `summary_from_markdown` strips markup out of a README summary already.

use std::path::{Path, PathBuf};

/// Legal in a path on every platform this runs on, and live in HTML: attribute
/// names are case-insensitive, so the label-casing this passes through on the
/// way to a figure does not disarm it.
const HOSTILE: &str = "<img src=x onerror=alert(1)>";

fn hostile_repo(parent: &Path) -> PathBuf {
    let root = parent.join(HOSTILE);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("package.json"),
        r#"{"name":"shop","version":"1.0.0","dependencies":{"express":"^4.19.2"}}"#,
    )
    .unwrap();
    // A route path and a doc comment, so an operation, a requirement and a
    // citation carry markup too.
    std::fs::write(
        root.join("src/server.ts"),
        r#"import express from "express";

const app = express();

/** <img src=x onerror=alert(1)> documents the handler. */
app.get("/items/</script><script>alert(1)</script>", (req, res) => {
  res.json({ ok: true });
});

app.listen(3000);
"#,
    )
    .unwrap();
    root
}

/// The text with every code span's contents removed.
///
/// A code span is literal in every Markdown renderer, so what sits inside one is
/// not markup however it is spelled. Checking the whole file would fail on text
/// the book is quoting correctly.
fn outside_code_spans(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let mut in_span = false;
        for part in line.split('`') {
            if !in_span {
                out.push_str(part);
            }
            in_span = !in_span;
        }
        out.push('\n');
    }
    out
}

#[test]
fn a_hostile_repository_cannot_put_markup_into_its_own_book() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let repo = hostile_repo(tmp.path());

    let planned = nunki_book::plan(&repo, out.path(), &nunki_book::BookOptions::default()).unwrap();
    let html = planned.files.get("index.html").expect("the book renders a page").to_lowercase();

    // The page carries two scripts of its own: the JSON data island and the
    // reader. A third came from the repository.
    assert_eq!(html.matches("<script").count(), 2, "an extra <script reached the page");
    assert!(!html.contains("<img "), "an img tag from the scanned repository is live in the book");

    // Escaped, not dropped. Without this the assertions above would also pass on
    // a book that quietly discarded the name, describing a repository that does
    // not exist.
    assert!(html.contains("\\u003cimg"), "the hostile name never reached the page, so this proves nothing");

    // The figures: the one place the reader hands a string to `innerHTML`.
    let mut seen_in_a_figure = false;
    for d in &planned.built.diagrams {
        let svg = d.standalone_svg.to_lowercase();
        assert!(!svg.contains("<script"), "{}: a script reached the SVG", d.id);
        assert!(!svg.contains("<img "), "{}: an img tag reached the SVG", d.id);
        if svg.contains("&lt;img") {
            seen_in_a_figure = true;
        }
    }
    assert!(seen_in_a_figure, "no figure carries the hostile name, so the SVG assertions prove nothing");
}

/// The Markdown beside the page draws a line the HTML does not.
///
/// A *snippet* quotes the scanned source, so a file that really contains
/// `<img src=x onerror=…>` should say so — escaping it would misreport the code.
/// A *title* is structure: it lands in a heading, a link label or a list item,
/// and a raw `<` there is live HTML the moment someone renders the file with
/// mdBook, MDX, Obsidian or Jekyll, none of which sanitise the way GitHub does.
///
/// So this asserts both halves. Asserting only the first would have let the
/// second rot, which is what it did until now.
#[test]
fn markdown_escapes_structure_and_quotes_source_verbatim() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let repo = hostile_repo(tmp.path());

    let planned = nunki_book::plan(&repo, out.path(), &nunki_book::BookOptions::default()).unwrap();

    // Structure: the repository's name reaches a heading, a link label and a list
    // item in README.md and llms.txt. None of them may carry a raw tag.
    //
    // Outside code spans only. Markdown renders a code span literally, so
    // `` `<img …>` `` is inert — and escaping it there would be wrong twice over:
    // it would misreport a name the book is quoting exactly, which is the same
    // reasoning that keeps snippets verbatim.
    let mut checked = 0;
    for (name, text) in planned.files.iter().filter(|(p, _)| p.ends_with(".md") || p.ends_with(".txt")) {
        let prose = outside_code_spans(text);
        assert!(!prose.contains("<img "), "{name}: a raw img tag outside a code span");
        checked += 1;
    }
    assert!(checked >= 3, "only {checked} Markdown files were checked");
    // Every page's own file carries the name in its heading, so the page files are
    // covered too — an earlier version checked only README.md and llms.txt, and a
    // revert of the page-title escaping went unnoticed.
    for name in ["README.md", "llms.txt"] {
        let text = planned.files.get(name).unwrap_or_else(|| panic!("{name} is generated"));
        assert!(text.contains("&lt;img"), "{name}: the name is absent entirely, so this proves nothing");
    }
    let pages: Vec<&String> = planned.files.keys().filter(|p| p.starts_with("pages/")).collect();
    assert!(!pages.is_empty(), "the book has page files");
    assert!(
        pages.iter().any(|p| planned.files[*p].contains("&lt;img")),
        "no page file carries the escaped name, so the page heading is not covered"
    );

    // A link label cannot be closed early, and a destination cannot be ended by a
    // space or a bracket.
    for (path, text) in planned.files.iter().filter(|(p, _)| p.ends_with(".md") || p.ends_with(".txt")) {
        for line in text.lines().filter(|l| l.starts_with("- [")) {
            let opens = line.matches('[').count();
            let closes = line.matches(']').count();
            assert_eq!(opens, closes, "{path}: unbalanced link brackets in `{line}`");
        }
    }

    // Verbatim: a snippet of the hostile file keeps what the file says. The
    // repository's `src/server.ts` contains the payload in a doc comment, and a
    // citation that quoted it differently would be misreporting the source.
    let full = planned.files.get("llms-full.txt").expect("llms-full.txt is generated");
    assert!(full.contains("documents the handler"), "the doc comment reached the book at all");
}
