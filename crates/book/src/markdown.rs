//! Markdown mirror (GitHub, Obsidian, docs sites) and `llms.txt`.

use std::collections::BTreeMap;

use crate::model::*;

/// Path of the Markdown file that renders `page`, relative to `from_dir`.
fn page_href(book: &Book, from_dir: &str, page: &str, anchor: Option<&str>) -> String {
    let target = book.pages.iter().find(|p| p.id == page).map(|p| p.md_path.as_str()).unwrap_or("README.md");
    let rel = relative(from_dir, target);
    match anchor {
        Some(a) => format!("{rel}#{a}"),
        None => rel,
    }
}

fn relative(from_dir: &str, target: &str) -> String {
    let depth = if from_dir.is_empty() { 0 } else { from_dir.split('/').count() };
    format!("{}{}", "../".repeat(depth), target)
}

fn escape(s: &str, in_table: bool) -> String {
    // Brackets make a link label: a route path or symbol name carrying `](…)`
    // closes the label the renderer opened and puts an attacker-chosen
    // destination in the book. A newline ends the block it sits in — a list
    // item, a table row, a blockquote — so it never survives as itself.
    let mut out = s
        .replace('\\', "\\\\")
        .replace('*', "\\*")
        .replace('_', "\\_")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('<', "&lt;")
        .replace('\n', " ");
    if in_table {
        out = out.replace('|', "\\|");
    }
    out
}

/// A link destination that cannot end the link early.
///
/// A permalink's path comes from `.git/config` and a citation's from the scanned
/// repository, so a space or a bracket in either would close the destination the
/// renderer opened and leave the rest as text. Markdown allows a destination to
/// be wrapped in angle brackets, which is the escape hatch for exactly this.
/// Wrapped only when it has to be, so the common case reads as it always did.
fn dest(h: &str) -> String {
    if h.contains([' ', '(', ')', '<', '>']) {
        format!("<{}>", h.replace('<', "%3C").replace('>', "%3E"))
    } else {
        h.to_string()
    }
}

/// Text inside a code span. A backtick would end the span and hand the rest of
/// the line to the renderer as markup.
fn code_span(v: &str) -> String {
    v.replace('`', "'").replace('\n', " ")
}

struct Ctx<'a> {
    book: &'a Book,
    dir: &'a str,
    repo_rel: Option<&'a str>,
    in_table: bool,
}

fn inlines(c: &Ctx, inl: &[Inline]) -> String {
    let mut out = String::new();
    for i in inl {
        match i {
            Inline::Text { v } => out.push_str(&escape(v, c.in_table)),
            Inline::Code { v } if v.is_empty() => {}
            Inline::Code { v } => out.push_str(&format!("`{}`", code_span(v).replace('|', "\\|"))),
            // Markdown bold can't end in whitespace: keep it outside the markers.
            Inline::Strong { v } => {
                let trimmed = v.trim_end();
                out.push_str(&format!("**{}**{}", escape(trimmed, c.in_table), &v[trimmed.len()..]));
            }
            Inline::Link { page, anchor, v } => out.push_str(&format!(
                "[{}]({})",
                escape(v, c.in_table),
                dest(&page_href(c.book, c.dir, page, anchor.as_deref()))
            )),
            Inline::Badge { v, .. } => out.push_str(&format!("_{}_", escape(v, c.in_table))),
            Inline::Cite { id } => {
                let Some(cite) = c.book.cites.get(id) else { continue };
                let label = if cite.start == cite.end {
                    format!("{}:{}", cite.file, cite.start)
                } else {
                    format!("{}:{}-{}", cite.file, cite.start, cite.end)
                };
                let mark = if cite.state == "verified" { "" } else { " ⚠" };
                let href = cite.permalink_in(&c.book.meta).or_else(|| {
                    c.repo_rel.map(|root| {
                        let anchor = if cite.start == cite.end {
                            format!("#L{}", cite.start)
                        } else {
                            format!("#L{}-L{}", cite.start, cite.end)
                        };
                        format!(
                            "{}{}{}{}",
                            "../".repeat(c.dir.split('/').filter(|s| !s.is_empty()).count()),
                            root,
                            cite.file,
                            anchor
                        )
                    })
                });
                match href {
                    Some(h) => out.push_str(&format!(" [`{}`]({}){mark}", code_span(&label), dest(&h))),
                    None => out.push_str(&format!(" `{label}`{mark}")),
                }
            }
        }
    }
    out
}

fn blocks(c: &mut Ctx, bs: &[Block]) -> String {
    let mut out = String::new();
    for b in bs {
        match b {
            Block::Heading { level, id: _, text } => {
                out.push_str(&format!("\n{} {}\n\n", "#".repeat(*level as usize), escape(text, false)));
            }
            Block::Para { inl } => {
                out.push_str(&inlines(c, inl));
                out.push_str("\n\n");
            }
            Block::Stats { items } => {
                let parts: Vec<String> = items
                    .iter()
                    .map(|s| match &s.page {
                        Some(p) => format!(
                            "**{}** [{}]({})",
                            escape(&s.value, false),
                            escape(&s.label, false),
                            page_href(c.book, c.dir, p, None)
                        ),
                        None => format!("**{}** {}", escape(&s.value, false), escape(&s.label, false)),
                    })
                    .collect();
                out.push_str(&parts.join(" · "));
                out.push_str("\n\n");
            }
            Block::Table { columns, rows } => {
                c.in_table = true;
                let header: Vec<String> =
                    columns.iter().map(|h| if h.is_empty() { " ".into() } else { escape(h, true) }).collect();
                out.push_str(&format!("| {} |\n", header.join(" | ")));
                out.push_str(&format!("|{}\n", "---|".repeat(columns.len())));
                for row in rows {
                    let cells: Vec<String> = row.iter().map(|cell| inlines(c, cell).trim().to_string()).collect();
                    out.push_str(&format!("| {} |\n", cells.join(" | ")));
                }
                c.in_table = false;
                out.push('\n');
            }
            Block::Figure { diagram, caption } => figure(c, &mut out, diagram, caption),
            Block::Callout { tone, title, inl } => {
                let kind = match tone.as_str() {
                    "warning" => "WARNING",
                    _ => "NOTE",
                };
                out.push_str(&format!("> [!{kind}]\n> **{}** — {}\n\n", escape(title, false), inlines(c, inl)));
            }
            Block::List { items } => {
                for it in items {
                    out.push_str(&format!("- {}\n", inlines(c, it)));
                }
                out.push('\n');
            }
            Block::Cards { cards } => {
                for card in cards {
                    out.push_str(&format!(
                        "- [{}]({}) — {}\n",
                        escape(&card.title, false),
                        dest(&page_href(c.book, c.dir, &card.page, None)),
                        escape(&card.text, false)
                    ));
                }
                out.push('\n');
            }
            Block::Steps { diagram, caption, steps } => {
                // A walkthrough is the only thing that draws its figure, so the
                // mirror has to draw it here or the diagram is in the book and
                // not in the Markdown — and the Markdown is what an agent reads.
                figure(c, &mut out, diagram, caption);
                for (i, s) in steps.iter().enumerate() {
                    out.push_str(&format!("{}. {} — {}\n", i + 1, inlines(c, &s.title), inlines(c, &s.body).trim()));
                }
                out.push('\n');
            }
        }
    }
    out
}

/// A figure: the rendered SVG, its caption and a link to the IR it came from.
/// Shared by `Figure` and `Steps`, which mounts its own figure.
fn figure(c: &mut Ctx, out: &mut String, diagram: &str, caption: &[Inline]) {
    let Some(f) = c.book.diagrams.get(diagram) else { return };
    let (svg, ir) = (relative(c.dir, &f.svg_path), relative(c.dir, &f.ir_path));
    out.push_str(&format!("![{}]({})\n\n", escape(&f.title, false), svg));
    match inlines(c, caption).trim() {
        "" => out.push_str(&format!("[IR]({ir})\n\n")),
        cap => out.push_str(&format!("_{cap}_ · [IR]({ir})\n\n")),
    }
}

fn meta_line(book: &Book) -> String {
    let m = &book.meta;
    let mut parts = vec![format!("`{}`", code_span(&m.repo))];
    if let Some(c) = &m.commit {
        parts.push(format!("commit `{}`", &c[..c.len().min(10)]));
    }
    if let Some(d) = &m.commit_date {
        parts.push(d.clone());
    }
    parts.push(format!("{}/{} citations verified", m.evidence.verified, m.evidence.total));
    parts.join(" · ")
}

/// README.md, pages/*.md, llms.txt and llms-full.txt.
pub fn render(book: &Book, repo_rel: Option<&str>) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let mut full = String::new();

    let mut readme = format!("# {} — architecture\n\n", escape(&book.meta.name, false));
    if let Some(d) = &book.meta.description {
        readme.push_str(&format!("> {}\n\n", escape(d, false)));
    }
    readme.push_str(&format!("{}\n\n", meta_line(book)));
    readme.push_str("Open [`index.html`](index.html) for the interactive book. Generated by nunki; figures are compiled from the `diagrams/*.ir.json` files, which may be edited by hand.\n\n");
    for group in &book.nav {
        readme.push_str(&format!("## {}\n\n", escape(&group.title, false)));
        for item in &group.items {
            let summary = book
                .pages
                .iter()
                .find(|p| p.id == item.page)
                .map(|p| plain(&p.summary, &book.cites))
                .unwrap_or_default();
            let href = dest(&page_href(book, "", &item.page, None));
            readme.push_str(&format!("- [{}]({href}) — {}\n", escape(&item.title, false), escape(&summary, false)));
        }
        readme.push('\n');
    }
    files.insert("README.md".into(), readme.clone());
    full.push_str(&readme);

    for page in &book.pages {
        let dir = page.md_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let mut c = Ctx { book, dir, repo_rel, in_table: false };
        let mut md = format!("# {}\n\n", escape(&page.title, false));
        md.push_str(&format!(
            "_{}_ · [Book index]({})\n\n",
            escape(&page.section, false),
            dest(&relative(dir, "README.md"))
        ));
        let summary = inlines(&c, &page.summary);
        if !summary.trim().is_empty() {
            md.push_str(&format!("{summary}\n\n"));
        }
        md.push_str(&blocks(&mut c, &page.blocks));
        let md = md.replace("\n\n\n", "\n\n");
        full.push_str(&format!("\n\n---\n\n{md}"));
        files.insert(page.md_path.clone(), md);
    }

    let mut llms = format!("# {}\n\n", escape(&book.meta.name, false));
    if let Some(d) = &book.meta.description {
        llms.push_str(&format!("> {}\n\n", escape(d, false)));
    }
    llms.push_str(&format!(
        "Architecture book generated from source by nunki. Every claim links to file:line evidence verified against the commit. {}\n\n",
        meta_line(book)
    ));
    llms.push_str("## Pages\n\n");
    for page in &book.pages {
        llms.push_str(&format!(
            "- [{}]({}): {}\n",
            escape(&page.title, false),
            dest(&page.md_path),
            escape(&plain(&page.summary, &book.cites), false)
        ));
    }
    llms.push_str("\n## Diagrams (typed DiagramIR JSON)\n\n");
    for f in book.diagrams.values() {
        llms.push_str(&format!("- [{}]({}): {}\n", escape(&f.id, false), dest(&f.ir_path), escape(&f.title, false)));
    }
    llms.push_str("\n## Optional\n\n- [Full text](llms-full.txt): every page concatenated\n");
    files.insert("llms.txt".into(), llms);
    files.insert("llms-full.txt".into(), full);
    files
}
