//! The book reader: one self-contained HTML file. The page shell is static;
//! `book.js` renders pages from the embedded JSON model with hash routing.

use nunki_renderer::text::{script_json, xml_escape as esc};
use nunki_renderer::theme::{theme_attr, token_css};
use nunki_renderer::Accent;

use crate::model::Book;

pub fn render(book: &Book, accent: &Accent) -> String {
    let data = serde_json::to_value(book).expect("book serializes");
    let theme = theme_attr(nunki_ir::Theme::EditorialLight);
    let diagram_css = nunki_renderer::svg::svg_style(accent);
    format!(
        r#"<!doctype html>
<html lang="en" data-theme="{theme}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="generator" content="{generator}">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data: blob:; connect-src 'none'">
<title>{title} — architecture</title>
<style>
{tokens}{diagram_css}
{book_css}
</style>
</head>
<body data-theme="{theme}">
<noscript><p style="padding:24px">This book needs JavaScript. The same content is available as Markdown in <code>README.md</code> and <code>pages/</code>.</p></noscript>
<div id="book" class="book"></div>
<script type="application/json" id="book-data">{data}</script>
<script>{js}</script>
</body>
</html>
"#,
        title = esc(&book.meta.name),
        generator = esc(&book.meta.generator),
        tokens = token_css("html", accent),
        book_css = include_str!("assets/book.css"),
        data = script_json(&data),
        js = include_str!("assets/book.js"),
    )
}
