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
    let mut out = s.replace('\\', "\\\\").replace('*', "\\*").replace('_', "\\_").replace('<', "&lt;");
    if in_table {
        out = out.replace('|', "\\|").replace('\n', " ");
    }
    out
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
            Inline::Code { v } => out.push_str(&format!("`{}`", v.replace('`', "'").replace('|', "\\|"))),
            // Markdown bold can't end in whitespace: keep it outside the markers.
            Inline::Strong { v } => {
                let trimmed = v.trim_end();
                out.push_str(&format!("**{}**{}", escape(trimmed, c.in_table), &v[trimmed.len()..]));
            }
            Inline::Link { page, anchor, v } => out.push_str(&format!(
                "[{}]({})",
                escape(v, c.in_table),
                page_href(c.book, c.dir, page, anchor.as_deref())
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
                    Some(h) => out.push_str(&format!(" [`{label}`]({h}){mark}")),
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
                out.push_str(&format!("\n{} {}\n\n", "#".repeat(*level as usize), text));
            }
            Block::Para { inl } => {
                out.push_str(&inlines(c, inl));
                out.push_str("\n\n");
            }
            Block::Stats { items } => {
                let parts: Vec<String> = items
                    .iter()
                    .map(|s| match &s.page {
                        Some(p) => format!("**{}** [{}]({})", s.value, s.label, page_href(c.book, c.dir, p, None)),
                        None => format!("**{}** {}", s.value, s.label),
                    })
                    .collect();
                out.push_str(&parts.join(" · "));
                out.push_str("\n\n");
            }
            Block::Table { columns, rows } => {
                c.in_table = true;
                let header: Vec<String> =
                    columns.iter().map(|h| if h.is_empty() { " ".into() } else { h.clone() }).collect();
                out.push_str(&format!("| {} |\n", header.join(" | ")));
                out.push_str(&format!("|{}\n", "---|".repeat(columns.len())));
                for row in rows {
                    let cells: Vec<String> = row.iter().map(|cell| inlines(c, cell).trim().to_string()).collect();
                    out.push_str(&format!("| {} |\n", cells.join(" | ")));
                }
                c.in_table = false;
                out.push('\n');
            }
            Block::Figure { diagram, caption } => {
                if let Some(f) = c.book.diagrams.get(diagram) {
                    out.push_str(&format!("![{}]({})\n\n", escape(&f.title, false), relative(c.dir, &f.svg_path)));
                    out.push_str(&format!(
                        "_{}_ · [IR]({})\n\n",
                        inlines(c, caption).trim(),
                        relative(c.dir, &f.ir_path)
                    ));
                }
            }
            Block::Callout { tone, title, inl } => {
                let kind = match tone.as_str() {
                    "warning" => "WARNING",
                    _ => "NOTE",
                };
                out.push_str(&format!("> [!{kind}]\n> **{}** — {}\n\n", title, inlines(c, inl)));
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
                        card.title,
                        page_href(c.book, c.dir, &card.page, None),
                        escape(&card.text, false)
                    ));
                }
                out.push('\n');
            }
            Block::Steps { diagram: _, steps } => {
                for (i, s) in steps.iter().enumerate() {
                    out.push_str(&format!("{}. {} — {}\n", i + 1, inlines(c, &s.title), inlines(c, &s.body).trim()));
                }
                out.push('\n');
            }
        }
    }
    out
}

fn meta_line(book: &Book) -> String {
    let m = &book.meta;
    let mut parts = vec![format!("`{}`", m.repo)];
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

    let mut readme = format!("# {} — architecture\n\n", book.meta.name);
    if let Some(d) = &book.meta.description {
        readme.push_str(&format!("> {d}\n\n"));
    }
    readme.push_str(&format!("{}\n\n", meta_line(book)));
    readme.push_str("Open [`index.html`](index.html) for the interactive book. Generated by autodoc-engine; figures are compiled from the `diagrams/*.ir.json` files, which may be edited by hand.\n\n");
    for group in &book.nav {
        readme.push_str(&format!("## {}\n\n", group.title));
        for item in &group.items {
            let summary = book
                .pages
                .iter()
                .find(|p| p.id == item.page)
                .map(|p| plain(&p.summary, &book.cites))
                .unwrap_or_default();
            readme.push_str(&format!("- [{}]({}) — {}\n", item.title, page_href(book, "", &item.page, None), summary));
        }
        readme.push('\n');
    }
    files.insert("README.md".into(), readme.clone());
    full.push_str(&readme);

    for page in &book.pages {
        let dir = page.md_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let mut c = Ctx { book, dir, repo_rel, in_table: false };
        let mut md = format!("# {}\n\n", page.title);
        md.push_str(&format!("_{}_ · [Book index]({})\n\n", page.section, relative(dir, "README.md")));
        let summary = inlines(&c, &page.summary);
        if !summary.trim().is_empty() {
            md.push_str(&format!("{summary}\n\n"));
        }
        md.push_str(&blocks(&mut c, &page.blocks));
        let md = md.replace("\n\n\n", "\n\n");
        full.push_str(&format!("\n\n---\n\n{md}"));
        files.insert(page.md_path.clone(), md);
    }

    let mut llms = format!("# {}\n\n", book.meta.name);
    if let Some(d) = &book.meta.description {
        llms.push_str(&format!("> {d}\n\n"));
    }
    llms.push_str(&format!(
        "Architecture book generated from source by autodoc-engine. Every claim links to file:line evidence verified against the commit. {}\n\n",
        meta_line(book)
    ));
    llms.push_str("## Pages\n\n");
    for page in &book.pages {
        llms.push_str(&format!("- [{}]({}): {}\n", page.title, page.md_path, plain(&page.summary, &book.cites)));
    }
    llms.push_str("\n## Diagrams (typed DiagramIR JSON)\n\n");
    for f in book.diagrams.values() {
        llms.push_str(&format!("- [{}]({}): {}\n", f.id, f.ir_path, f.title));
    }
    llms.push_str("\n## Optional\n\n- [Full text](llms-full.txt): every page concatenated\n");
    files.insert("llms.txt".into(), llms);
    files.insert("llms-full.txt".into(), full);
    files
}
