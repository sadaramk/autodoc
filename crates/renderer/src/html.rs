//! Standalone interactive HTML: one file, inline SVG, inline CSS and vanilla
//! JS, no network requests.

use std::collections::BTreeMap;

use autodoc_ir::DiagramIR;
use serde::Serialize;
use serde_json::json;

use crate::layout::Layout;
use crate::text::{script_json, xml_escape as esc};
use crate::theme::{theme_attr, token_css, Accent};

/// Evidence details resolved at compile time (the page never touches disk).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceView {
    /// `verified`, `stale`, `untracked`, `unverified`, `file-missing`, …
    pub state: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    pub snippet_start: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permalink: Option<String>,
    /// Text placed on the clipboard by "Copy Git Reference".
    pub git_ref: String,
}

const ICONS: &[(&str, &str, &str, &str)] = &[
    ("zoom-out", "Zoom out (−)", r#"<svg viewBox="0 0 16 16"><path d="M3.5 8h9"/></svg>"#, ""),
    ("zoom-in", "Zoom in (+)", r#"<svg viewBox="0 0 16 16"><path d="M3.5 8h9M8 3.5v9"/></svg>"#, ""),
    (
        "fit",
        "Fit to screen (0)",
        r#"<svg viewBox="0 0 16 16"><path d="M2.5 6V2.5H6M10 2.5h3.5V6M13.5 10v3.5H10M6 13.5H2.5V10"/></svg>"#,
        "",
    ),
    ("sep", "", "", ""),
    (
        "theme",
        "Toggle light/dark (t)",
        r#"<svg viewBox="0 0 16 16"><circle cx="8" cy="8" r="5.5"/><path d="M8 2.5v11a5.5 5.5 0 0 0 0-11z" fill="currentColor" stroke="none"/></svg>"#,
        "",
    ),
    ("sep", "", "", ""),
    (
        "export-svg",
        "Download SVG",
        r#"<svg viewBox="0 0 16 16"><path d="M8 2.5v8M4.5 7 8 10.5 11.5 7M3 13.5h10"/></svg>"#,
        "SVG",
    ),
    (
        "export-png",
        "Download PNG",
        r#"<svg viewBox="0 0 16 16"><path d="M8 2.5v8M4.5 7 8 10.5 11.5 7M3 13.5h10"/></svg>"#,
        "PNG",
    ),
];

pub fn render(
    ir: &DiagramIR,
    layout: &Layout,
    svg: &str,
    accent: &Accent,
    evidence: &BTreeMap<String, EvidenceView>,
    generator: &str,
) -> String {
    let data = json!({
        "ir": ir,
        "layout": { "width": layout.width, "height": layout.height },
        "evidence": evidence,
        "generator": generator,
    });
    let mut toolbar = String::new();
    for (action, title, icon, label) in ICONS {
        if *action == "sep" {
            toolbar.push_str(r#"<span class="sep" aria-hidden="true"></span>"#);
            if icon.is_empty() && toolbar.matches("sep").count() == 1 {
                toolbar.push_str(r#"<span class="zoom" data-zoom-label aria-live="polite">100%</span>"#);
            }
            continue;
        }
        let text = if label.is_empty() { String::new() } else { format!(r#"<span class="label">{label}</span>"#) };
        toolbar.push_str(&format!(
            r#"<button type="button" data-action="{action}" title="{title}" aria-label="{title}">{icon}{text}</button>"#
        ));
    }
    let theme = theme_attr(ir.theme);
    format!(
        r#"<!doctype html>
<html lang="en" data-theme="{theme}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="generator" content="{generator}">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data: blob:; connect-src 'none'">
<title>{title}</title>
<style>
{tokens}{page_css}{interactive_css}
</style>
</head>
<body data-theme="{theme}">
<main class="stage" aria-label="Diagram canvas">{svg}</main>
<nav class="toolbar" aria-label="Diagram controls">{toolbar}</nav>
<p class="hint">Scroll to zoom · drag to pan · hover to trace · click a node for source</p>
<aside class="drawer" role="dialog" aria-labelledby="drawer-title" aria-hidden="true"><header></header><div class="body"></div></aside>
<div class="toast" role="status" aria-live="polite"></div>
<script type="application/json" id="autodoc-data">{data}</script>
<script>{js}</script>
</body>
</html>
"#,
        title = esc(&ir.title),
        generator = esc(generator),
        tokens = token_css("body", accent),
        page_css = include_str!("assets/page.css"),
        interactive_css = include_str!("assets/interactive.css"),
        data = script_json(&data),
        js = include_str!("assets/app.js"),
    )
}
