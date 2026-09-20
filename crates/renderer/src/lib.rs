//! Editorial renderer: `DiagramIR` → deterministic layout → SVG, and a
//! standalone interactive HTML page. No runtime dependencies in the output.

pub mod drawio;
pub mod html;
pub mod layout;
pub mod sequence;
pub mod svg;
pub mod text;
pub mod theme;

use std::collections::BTreeMap;

use nunki_ir::DiagramIR;

pub use html::EvidenceView;
pub use layout::{GeometryIssue, Layout};
pub use theme::Accent;

pub const GENERATOR: &str = concat!("nunki ", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Default)]
pub struct RenderOptions {
    pub accent: Accent,
    /// Per-node evidence resolved by the caller (snippets, permalinks, state).
    pub evidence: BTreeMap<String, EvidenceView>,
    /// Footer line (repo · commit · density · generated). Built from the IR when empty.
    pub footer: Option<String>,
}

pub struct Rendered {
    pub layout: Layout,
    pub content: String,
}

pub fn default_footer(ir: &DiagramIR) -> String {
    let repo = ir.metadata.target_repo.trim_end_matches('/').rsplit('/').next().unwrap_or(&ir.metadata.target_repo);
    let mut parts = vec![repo.to_string()];
    if let Some(c) = &ir.metadata.commit_hash {
        parts[0] = format!("{repo}@{}", &c[..c.len().min(8)]);
    }
    let density = nunki_ir::visual_density(ir.nodes.len(), ir.edges.len());
    parts.push(format!("density {density:.2}"));
    parts.push(ir.metadata.generated_at.clone());
    parts.join(" · ")
}

pub fn render_svg(ir: &DiagramIR, opts: &RenderOptions) -> Rendered {
    let layout = svg::layout_ir(ir);
    let footer = opts.footer.clone().unwrap_or_else(|| default_footer(ir));
    let content = svg::render(
        ir,
        &layout,
        &svg::SvgOptions {
            accent: &opts.accent,
            footer,
            interactive: false,
            fill_viewport: false,
            header: true,
            id_prefix: "ad",
        },
    );
    Rendered { layout, content: format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{content}\n") }
}

/// Keyboard-operable SVG without a title block, scaled by its viewBox, for
/// embedding several figures in one document.
pub fn render_embedded_svg(ir: &DiagramIR, opts: &RenderOptions, id_prefix: &str) -> Rendered {
    let layout = svg::layout_embedded(ir);
    let footer = opts.footer.clone().unwrap_or_else(|| default_footer(ir));
    let content = svg::render(
        ir,
        &layout,
        &svg::SvgOptions {
            accent: &opts.accent,
            footer,
            interactive: true,
            fill_viewport: false,
            header: false,
            id_prefix,
        },
    );
    Rendered { layout, content }
}

pub fn render_html(ir: &DiagramIR, opts: &RenderOptions) -> Rendered {
    let layout = svg::layout_ir(ir);
    let footer = opts.footer.clone().unwrap_or_else(|| default_footer(ir));
    let svg = svg::render(
        ir,
        &layout,
        &svg::SvgOptions {
            accent: &opts.accent,
            footer,
            interactive: true,
            fill_viewport: true,
            header: true,
            id_prefix: "ad",
        },
    );
    let content = html::render(ir, &layout, &svg, &opts.accent, &opts.evidence, GENERATOR);
    Rendered { layout, content }
}
