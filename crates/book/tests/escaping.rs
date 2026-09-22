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

    // Deliberately not asserted for the Markdown beside the page: it quotes
    // excerpts of the scanned source verbatim, and a snippet of a file that
    // really does contain `<img src=x onerror=...>` should say so. Markdown is
    // not the sink — the page is, because that is what a browser parses.
}
