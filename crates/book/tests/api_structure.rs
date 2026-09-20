//! Split API reference, capability maps, access matrices and split functional
//! specifications. Its own test binary: it lowers the page-splitting
//! thresholds through the environment for the whole process.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use autodoc_book::model::{Block, Book, Inline};
use autodoc_book::{plan, BookOptions};

fn fixtures() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures")).to_path_buf()
}

fn inlines(b: &Block) -> Vec<&Inline> {
    match b {
        Block::Para { inl } | Block::Callout { inl, .. } => inl.iter().collect(),
        Block::Table { rows, .. } => rows.iter().flatten().flatten().collect(),
        Block::Figure { caption, .. } => caption.iter().collect(),
        Block::List { items } => items.iter().flatten().collect(),
        Block::Steps { steps, .. } => steps.iter().flat_map(|s| s.title.iter().chain(s.body.iter())).collect(),
        _ => vec![],
    }
}

fn assert_links_resolve(book: &Book) {
    for page in &book.pages {
        for block in &page.blocks {
            for i in inlines(block) {
                if let Inline::Link { page: p, anchor, .. } = i {
                    let target =
                        book.pages.iter().find(|x| x.id == *p).unwrap_or_else(|| panic!("{}: link → {p}", page.id));
                    if let Some(a) = anchor {
                        assert!(
                            target.blocks.iter().any(|b| matches!(b, Block::Heading { id, .. } if id == a)),
                            "{}: anchor {p}#{a} has no heading",
                            page.id
                        );
                    }
                }
            }
        }
    }
}

/// Flattened text of the first table on `id` whose first column is `column`.
fn table_text(book: &Book, id: &str, column: &str) -> String {
    book.pages
        .iter()
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("page {id}"))
        .blocks
        .iter()
        .find(|b| matches!(b, Block::Table { columns, .. } if columns.first().map(String::as_str) == Some(column)))
        .map(|b| {
            inlines(b)
                .iter()
                .filter_map(|i| match i {
                    Inline::Text { v } | Inline::Code { v } => Some(v.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .unwrap_or_else(|| panic!("table with a {column:?} column on {id}"))
}

fn headings(book: &Book, id: &str) -> Vec<String> {
    book.pages
        .iter()
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("page {id}; have {:?}", book.pages.iter().map(|p| &p.id).collect::<Vec<_>>()))
        .blocks
        .iter()
        .filter_map(|b| if let Block::Heading { text, .. } = b { Some(text.clone()) } else { None })
        .collect()
}

#[test]
fn large_services_split_into_capability_pages_with_access_matrices() {
    std::env::set_var("AUTODOC_API_SPLIT_OVER", "3");
    std::env::set_var("AUTODOC_FR_SPLIT_OVER", "3");
    std::env::set_var("AUTODOC_RULES_SPLIT_OVER", "2");
    let out = tempfile::tempdir().unwrap();
    let planned = plan(&fixtures().join("api-frameworks/spring-mvc"), out.path(), &BookOptions::default()).unwrap();
    let book = &planned.built.book;
    let ids: BTreeSet<&str> = book.pages.iter().map(|p| p.id.as_str()).collect();

    // orders-service (4 operations) splits; inventory-service (2) stays on one page.
    assert!(ids.contains("api/orders-service") && ids.contains("api/orders-service/order"), "{ids:?}");
    assert!(ids.contains("api/inventory-service") && !ids.iter().any(|i| i.starts_with("api/inventory-service/")));
    let overview = headings(book, "api/orders-service");
    assert!(overview.contains(&"Capabilities".to_string()) && overview.contains(&"Access".to_string()), "{overview:?}");
    assert!(!overview.iter().any(|h| h.starts_with("GET ")), "contracts live on group pages: {overview:?}");
    let group = headings(book, "api/orders-service/order");
    assert!(group.contains(&"GET /api/orders/{id}".to_string()), "{group:?}");

    // Routes this service answers but the reference does not document are named on
    // the overview with a reason. A reader counting @GetMapping annotations should
    // find the difference accounted for, not have to guess it was an oversight.
    assert!(overview.contains(&"Routes not documented above".to_string()), "{overview:?}");
    let excluded = table_text(book, "api/orders-service", "Route");
    for want in ["GET /api/internal/health", "GET /api/ui/orders", "server-rendered view, not an API operation"] {
        assert!(excluded.contains(want), "excluded table should say {want:?}: {excluded}");
    }

    // Access matrix: role and authentication columns, cited.
    let access = book
        .pages
        .iter()
        .find(|p| p.id == "api/orders-service")
        .unwrap()
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Table { columns, rows } if columns.first().map(String::as_str) == Some("Operations") => {
                Some((columns, rows))
            }
            _ => None,
        })
        .expect("access matrix");
    assert!(
        access.0.contains(&"ADMIN".to_string()) && access.0.contains(&"authenticated".to_string()),
        "{:?}",
        access.0
    );
    assert!(access.1.iter().flatten().flatten().any(|i| matches!(i, Inline::Cite { .. })));

    // Functional spec split per service, rules on their own page; every link lands.
    // A service over the threshold gets a page per capability group; smaller ones one page.
    assert!(
        ids.contains("functional") && ids.contains("functional/orders-service/order") && ids.contains("rules"),
        "{ids:?}"
    );
    assert!(ids.contains("functional/inventory-service") && !ids.contains("functional/orders-service"), "{ids:?}");
    assert!(headings(book, "functional/orders-service/order").iter().any(|h| h.starts_with("FR-")));
    assert!(planned.files.contains_key("pages/07-functional-orders-service--order.md"));
    assert!(headings(book, "rules").contains(&"Business rules".to_string()));
    // More than twice the threshold: one page per rule kind, linked from the catalog.
    assert!(ids.iter().any(|i| i.starts_with("rules/")), "{ids:?}");
    assert_links_resolve(book);

    // Every page has a Markdown file.
    for p in &book.pages {
        assert!(planned.files.contains_key(&p.md_path), "{} → {}", p.id, p.md_path);
    }
    assert!(planned.files.contains_key("pages/06-api-orders-service--order.md"));
}

#[test]
fn capability_maps_render_with_clean_geometry() {
    for fixture in ["polyglot-shop", "api-frameworks/go-chi", "api-frameworks/fastapi"] {
        let out = tempfile::tempdir().unwrap();
        let planned = plan(&fixtures().join(fixture), out.path(), &BookOptions::default()).unwrap();
        let caps: Vec<_> = planned.built.diagrams.iter().filter(|d| d.id.starts_with("capabilities-")).collect();
        assert!(!caps.is_empty(), "{fixture}: no capability map");
        for d in caps {
            let r = autodoc_renderer::render_svg(&d.ir, &autodoc_renderer::RenderOptions::default());
            let issues = r.layout.check_geometry();
            assert!(issues.is_empty(), "{fixture}/{}: {issues:#?}", d.id);
        }
    }
}
