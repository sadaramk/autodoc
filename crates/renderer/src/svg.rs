//! SVG emission. Paint order: canvas → zones → connectors → labels → cards →
//! legend, so connectors sit behind boxes and label masks never hide a card.

use std::collections::BTreeSet;
use std::fmt::Write;

use nunki_ir::{BoundaryType, Cardinality, DiagramIR, DiagramType, EdgeType, StateKind};

use crate::layout::{self, container_chip, EdgeRoute, Layout, NodeBox, TitleMetrics, KEY_W, MARGIN, ROW_H};
use crate::text::{fit, mono_width, sans_width, xml_escape as esc};
use crate::theme::{theme_attr, token_css, Accent};

pub const TITLE_SIZE: f64 = 26.0;
pub const SUBTITLE_SIZE: f64 = 15.0;
const LEGEND_H: f64 = 64.0;

pub struct SvgOptions<'a> {
    pub accent: &'a Accent,
    /// Rendered below the legend: repo, commit, density, timestamp.
    pub footer: String,
    /// Adds `tabindex`/`role` so cards are keyboard-operable in HTML.
    pub interactive: bool,
    /// Size to the parent element (full-page viewer) instead of a viewBox.
    pub fill_viewport: bool,
    /// Draw the eyebrow/title/subtitle block (off when a page already titles the figure).
    pub header: bool,
    /// Prefix for element ids, so several diagrams can share one document.
    pub id_prefix: &'a str,
}

pub fn title_metrics(ir: &DiagramIR) -> TitleMetrics {
    let title_w = sans_width(&ir.title, TITLE_SIZE, true);
    let sub_w = ir.subtitle.as_deref().map(|s| sans_width(s, SUBTITLE_SIZE, false)).unwrap_or(0.0);
    TitleMetrics {
        header_h: if ir.subtitle.is_some() { 104.0 } else { 80.0 },
        min_width: (title_w.max(sub_w.min(900.0)) + 2.0 * MARGIN).min(1400.0),
        legend_h: LEGEND_H,
    }
}

pub fn diagram_type_label(t: DiagramType) -> &'static str {
    match t {
        DiagramType::SystemContext => "System context",
        DiagramType::Container => "Container diagram",
        DiagramType::Component => "Component diagram",
        DiagramType::DataFlow => "Data flow",
        DiagramType::Lifecycle => "Lifecycle",
        DiagramType::Sequence => "Sequence diagram",
        DiagramType::EntityRelationship => "Entity relationships",
    }
}

pub fn boundary_label(t: BoundaryType) -> &'static str {
    match t {
        BoundaryType::TrustZone => "Trust zone",
        BoundaryType::InternalService => "Internal service",
        BoundaryType::ThirdParty => "Third party",
        BoundaryType::Storage => "Storage",
        BoundaryType::Client => "Client",
    }
}

fn boundary_class(t: BoundaryType) -> &'static str {
    match t {
        BoundaryType::TrustZone => "b-trust",
        BoundaryType::InternalService => "b-internal",
        BoundaryType::ThirdParty => "b-third",
        BoundaryType::Storage => "b-storage",
        BoundaryType::Client => "b-client",
    }
}

fn type_class_of(t: DiagramType) -> &'static str {
    match t {
        DiagramType::SystemContext => "ad-system-context",
        DiagramType::Container => "ad-container",
        DiagramType::Component => "ad-component",
        DiagramType::DataFlow => "ad-data-flow",
        DiagramType::Lifecycle => "ad-lifecycle",
        DiagramType::Sequence => "ad-sequence",
        DiagramType::EntityRelationship => "ad-er",
    }
}

/// Crow's-foot classes for the source and target ends of a relationship.
fn cardinality_class(c: Option<Cardinality>) -> &'static str {
    match c {
        Some(Cardinality::OneToOne) => " c-s-one c-t-one",
        Some(Cardinality::OneToMany) => " c-s-one c-t-many",
        Some(Cardinality::ManyToOne) => " c-s-many c-t-one",
        Some(Cardinality::ManyToMany) => " c-s-many c-t-many",
        None => "",
    }
}

fn type_class(t: EdgeType) -> &'static str {
    match t {
        EdgeType::Sync => "t-sync",
        EdgeType::Async => "t-async",
        EdgeType::Event => "t-event",
        EdgeType::Read => "t-read",
        EdgeType::Write => "t-write",
    }
}

pub fn svg_style(accent: &Accent) -> String {
    let mut css = token_css(".nunki", accent);
    css.push_str(include_str!("assets/diagram.css"));
    css
}

pub fn render(ir: &DiagramIR, lay: &Layout, opts: &SvgOptions) -> String {
    let mut s = String::with_capacity(32_000);
    let (w, h) = (lay.width, lay.height);
    let desc = format!(
        "{} with {} nodes and {} connections.",
        diagram_type_label(ir.diagram_type),
        ir.nodes.len(),
        lay.edges.len()
    );
    // Interactive pages size the SVG to the viewport and pan/zoom the content
    // group themselves; a viewBox would scale it a second time.
    let frame = if opts.fill_viewport {
        r#"width="100%" height="100%""#.to_string()
    } else {
        format!(r#"viewBox="0 0 {w} {h}" width="{w}" height="{h}""#)
    };
    let _ = write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" class="nunki {kind}" data-theme="{theme}" data-diagram-type="{kind}" {frame} data-width="{w}" data-height="{h}" role="img" aria-labelledby="{p}-title {p}-desc">"#,
        kind = type_class_of(ir.diagram_type),
        theme = theme_attr(ir.theme),
        p = esc(opts.id_prefix)
    );
    let _ = write!(
        s,
        r#"<title id="{p}-title">{}</title><desc id="{p}-desc">{}</desc>"#,
        esc(&ir.title),
        esc(&desc),
        p = esc(opts.id_prefix)
    );
    let _ = write!(s, "<style>{}</style>", svg_style(opts.accent));
    s.push_str(MARKERS);
    let _ = write!(s, r#"<rect class="ad-canvas" x="0" y="0" width="{w}" height="{h}"/>"#);
    let _ = write!(s, r#"<g id="{}-viewport" class="ad-viewport">"#, esc(opts.id_prefix));
    if opts.header {
        header(&mut s, ir);
    }
    zones(&mut s, ir, lay);
    lifelines(&mut s, lay);
    edges(&mut s, ir, lay, opts.interactive);
    labels(&mut s, ir, lay);
    nodes(&mut s, ir, lay, opts.interactive);
    legend(&mut s, ir, lay, opts);
    s.push_str("</g></svg>");
    s
}

fn header(s: &mut String, ir: &DiagramIR) {
    let x = MARGIN;
    let _ = write!(
        s,
        r#"<g class="ad-header"><text class="ad-eyebrow" x="{x}" y="{}">{}</text><text class="ad-title" x="{x}" y="{}">{}</text>"#,
        MARGIN + 10.0,
        esc(&diagram_type_label(ir.diagram_type).to_uppercase()),
        MARGIN + 46.0,
        esc(&ir.title)
    );
    if let Some(sub) = &ir.subtitle {
        let sub = fit(sub, 1000.0, |t| sans_width(t, SUBTITLE_SIZE, false));
        let _ = write!(s, r#"<text class="ad-subtitle" x="{x}" y="{}">{}</text>"#, MARGIN + 72.0, esc(&sub));
    }
    s.push_str("</g>");
}

fn zones(s: &mut String, ir: &DiagramIR, lay: &Layout) {
    s.push_str(r#"<g class="ad-zones">"#);
    for c in &lay.containers {
        let spec = &ir.containers[c.index];
        let r = c.rect;
        let chip = container_chip(c);
        let label = fit(&c.label.to_uppercase(), r.w - 40.0, |t| mono_width(t, 10.5) * 1.1);
        let _ = write!(
            s,
            r#"<g class="ad-zone {cls}" data-id="{id}"><rect class="ad-zone-box" x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="10"/><text class="ad-zone-label" x="{lx:.1}" y="{ly:.1}">{label}</text>"#,
            cls = boundary_class(spec.boundary_type),
            id = esc(&c.id),
            x = r.x,
            y = r.y,
            w = r.w,
            h = r.h,
            lx = chip.x + 12.0,
            ly = chip.y + 17.0,
            label = esc(&label)
        );
        let kind = boundary_label(spec.boundary_type).to_uppercase();
        let kind_w = mono_width(&kind, 9.5) * 1.1;
        if mono_width(&label, 10.5) * 1.1 + kind_w + 60.0 < r.w {
            let _ = write!(
                s,
                r#"<text class="ad-zone-kind" x="{:.1}" y="{:.1}">{}</text>"#,
                r.right() - 20.0,
                chip.y + 17.0,
                esc(&kind)
            );
        }
        if let Some(role) = &spec.role_description {
            let _ = write!(s, "<title>{}</title>", esc(role));
        }
        s.push_str("</g>");
    }
    s.push_str("</g>");
}

/// Orthogonal path with quarter-arc corners and hop arcs over crossings.
pub fn path_d(points: &[(f64, f64)], hops: &[(f64, f64)]) -> String {
    let mut d = String::new();
    if points.is_empty() {
        return d;
    }
    let _ = write!(d, "M{:.1},{:.1}", points[0].0, points[0].1);
    let n = points.len();
    for i in 1..n {
        let (px, py) = points[i - 1];
        let (x, y) = points[i];
        let horizontal = (py - y).abs() < 0.01;
        // Where this segment ends (shortened by the next corner's radius).
        let (ex, ey, corner) = if i + 1 < n {
            let (nx, ny) = points[i + 1];
            let len_in = ((x - px).abs()).max((y - py).abs());
            let len_out = ((nx - x).abs()).max((ny - y).abs());
            let r = 8.0f64.min(len_in / 2.0).min(len_out / 2.0);
            let (dx, dy) = (sign(x - px), sign(y - py));
            let (ox, oy) = (sign(nx - x), sign(ny - y));
            (x - dx * r, y - dy * r, Some((x, y, x + ox * r, y + oy * r)))
        } else {
            (x, y, None)
        };
        if horizontal {
            let dir = sign(ex - px);
            let mut seg_hops: Vec<f64> = hops
                .iter()
                .filter(|h| (h.1 - y).abs() < 0.01 && (h.0 - px) * dir > 16.0 && (ex - h.0) * dir > 16.0)
                .map(|h| h.0)
                .collect();
            seg_hops.sort_by(|a, b| (a * dir).total_cmp(&(b * dir)));
            for hx in seg_hops {
                let _ =
                    write!(d, "H{:.1}a5,5 0 0,{} {:.1},0", hx - dir * 5.0, if dir > 0.0 { 1 } else { 0 }, dir * 10.0);
            }
            let _ = write!(d, "H{ex:.1}");
        } else {
            let _ = write!(d, "V{ey:.1}");
        }
        if let Some((cx, cy, qx, qy)) = corner {
            let _ = write!(d, "Q{cx:.1},{cy:.1} {qx:.1},{qy:.1}");
        }
    }
    d
}

/// `f64::signum` maps 0.0 to 1.0; corners need a true zero for straight axes.
fn sign(v: f64) -> f64 {
    if v.abs() < 0.01 {
        0.0
    } else {
        v.signum()
    }
}

fn lifelines(s: &mut String, lay: &Layout) {
    if lay.lifelines.is_empty() {
        return;
    }
    s.push_str(r#"<g class="ad-lifelines">"#);
    for l in &lay.lifelines {
        let _ = write!(
            s,
            r#"<line class="ad-lifeline" data-id="{}" x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"/>"#,
            esc(&l.id),
            l.x,
            l.top,
            l.x,
            l.bottom
        );
    }
    s.push_str("</g>");
}

fn edges(s: &mut String, ir: &DiagramIR, lay: &Layout, interactive: bool) {
    s.push_str(r#"<g class="ad-edges">"#);
    // Primary path paints last so the accent is never crossed by neutral ink.
    let mut order: Vec<&EdgeRoute> = lay.edges.iter().collect();
    order.sort_by_key(|e| e.primary);
    for e in order {
        let spec = &ir.edges[e.index];
        let classes = format!(
            "ad-edge {}{}{}{}{}{}",
            type_class(e.edge_type),
            if spec.is_reply() { " is-reply" } else { "" },
            if spec.evidence.is_some() { " has-evidence" } else { "" },
            if ir.diagram_type == DiagramType::EntityRelationship { cardinality_class(spec.cardinality) } else { "" },
            if e.primary { " is-primary" } else { "" },
            if e.dashed {
                if e.edge_type == EdgeType::Event {
                    " is-dotted"
                } else {
                    " is-dashed"
                }
            } else {
                ""
            }
        );
        let d = path_d(&e.points, &e.hops);
        let title = match (spec.cardinality, ir.diagram_type) {
            (Some(c), DiagramType::EntityRelationship) => format!("<title>{}</title>", cardinality_text(c)),
            _ => String::new(),
        };
        let _ = write!(
            s,
            r#"<g class="{classes}" data-id="{id}" data-source="{src}" data-target="{tgt}"{extra}>{title}<path class="ad-edge-hit" d="{d}"/><path class="ad-edge-line" d="{d}"/></g>"#,
            id = esc(&e.id),
            src = esc(&e.source),
            tgt = esc(&e.target),
            extra = if interactive && spec.evidence.is_some() {
                format!(
                    r#" tabindex="0" role="button" aria-label="{}""#,
                    esc(&format!("{}, source evidence", spec.label.as_deref().unwrap_or(&spec.id)))
                )
            } else {
                String::new()
            },
        );
    }
    s.push_str("</g>");
}

fn labels(s: &mut String, ir: &DiagramIR, lay: &Layout) {
    s.push_str(r#"<g class="ad-labels">"#);
    for e in &lay.edges {
        let Some(l) = &e.label else { continue };
        let r = l.rect;
        let _ = write!(
            s,
            r#"<g class="ad-edge-label{}{}" data-edge="{}"><rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="4"/><text x="{:.1}" y="{:.1}">{}</text>"#,
            if e.primary { " is-primary" } else { "" },
            if ir.edges[e.index].evidence.is_some() { " has-evidence" } else { "" },
            esc(&e.id),
            r.x,
            r.y,
            r.w,
            r.h,
            r.cx(),
            r.y + 12.5,
            esc(&l.text)
        );
        if let Some(detail) = &l.detail {
            let _ = write!(
                s,
                r#"<text class="ad-edge-detail" x="{:.1}" y="{:.1}">{}</text>"#,
                r.cx(),
                r.y + 27.0,
                esc(detail)
            );
        }
        s.push_str("</g>");
    }
    s.push_str("</g>");
}

fn node_class(ir: &DiagramIR, b: &NodeBox) -> String {
    let n = &ir.nodes[b.index];
    let boundary = n.container_id.as_deref().and_then(|c| ir.container(c)).map(|c| boundary_class(c.boundary_type));
    let mut cls = String::from("ad-node");
    if let Some(bc) = boundary {
        cls.push(' ');
        cls.push_str(bc);
    }
    if n.is_key_focal_point {
        cls.push_str(" is-focal");
    }
    if n.evidence.is_some() {
        cls.push_str(" has-evidence");
    }
    match ir.diagram_type {
        DiagramType::Lifecycle => {
            cls.push_str(" is-state");
            match n.state_kind {
                Some(StateKind::Initial) => cls.push_str(" state-initial"),
                Some(StateKind::Terminal) => cls.push_str(" state-terminal"),
                _ => {}
            }
        }
        DiagramType::Sequence => cls.push_str(" is-participant"),
        DiagramType::EntityRelationship => cls.push_str(" is-entity"),
        _ => {}
    }
    cls
}

fn nodes(s: &mut String, ir: &DiagramIR, lay: &Layout, interactive: bool) {
    s.push_str(r#"<g class="ad-nodes">"#);
    for b in &lay.nodes {
        let n = &ir.nodes[b.index];
        let r = b.rect;
        let natural = b.content_h();
        let top = if b.rows.is_empty() { r.y + (r.h - natural) / 2.0 } else { r.y };
        let state = if ir.diagram_type == DiagramType::Lifecycle { n.state_kind } else { None };
        let rx = if ir.diagram_type == DiagramType::Lifecycle { (r.h / 2.0).min(22.0) } else { 8.0 };
        let aria = format!(
            "{}{}{}",
            n.label,
            n.subtitle.as_deref().map(|s| format!(", {s}")).unwrap_or_default(),
            if n.is_key_focal_point { ", focal point" } else { "" }
        );
        let _ = write!(
            s,
            r#"<g class="{cls}" data-id="{id}"{extra}><rect class="ad-card" x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="{rx:.1}"/>"#,
            cls = node_class(ir, b),
            id = esc(&n.id),
            extra = if interactive {
                format!(r#" tabindex="0" role="button" aria-label="{}""#, esc(&aria))
            } else {
                String::new()
            },
            x = r.x,
            y = r.y,
            w = r.w,
            h = r.h
        );
        let mut label_x = r.x + 16.0;
        match state {
            Some(StateKind::Initial) => {
                let _ =
                    write!(s, r#"<circle class="ad-state-mark" cx="{:.1}" cy="{:.1}" r="5"/>"#, r.x + 21.0, top + 24.5);
                label_x += 16.0;
            }
            Some(StateKind::Terminal) => {
                let _ = write!(
                    s,
                    r#"<rect class="ad-state-inner" x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="{:.1}"/>"#,
                    r.x + 3.5,
                    r.y + 3.5,
                    r.w - 7.0,
                    r.h - 7.0,
                    (rx - 3.5).max(2.0)
                );
            }
            _ => {}
        }
        if b.label != n.label {
            let _ = write!(s, "<title>{}</title>", esc(&n.label));
        }
        let _ = write!(
            s,
            r#"<text class="ad-node-label" x="{:.1}" y="{:.1}">{}</text>"#,
            label_x,
            top + 29.0,
            esc(&b.label)
        );
        if n.evidence.is_some() {
            let _ = write!(
                s,
                r#"<text class="ad-evidence-glyph" x="{:.1}" y="{:.1}">&lt;/&gt;</text>"#,
                r.right() - 14.0,
                top + 28.0
            );
        }
        let mut cursor = top + 34.0;
        if let Some(sub) = &b.subtitle {
            let _ = write!(
                s,
                r#"<text class="ad-node-subtitle" x="{:.1}" y="{:.1}">{}</text>"#,
                r.x + 16.0,
                cursor + 13.0,
                esc(sub)
            );
            cursor += 18.0;
        }
        if let Some(tech) = &b.tech {
            let by = cursor + 8.0;
            let _ = write!(
                s,
                r#"<rect class="ad-badge" x="{:.1}" y="{by:.1}" width="{:.1}" height="20" rx="4"/><text class="ad-badge-text" x="{:.1}" y="{:.1}">{}</text>"#,
                r.x + 16.0,
                b.tech_w,
                r.x + 24.0,
                by + 14.0,
                esc(tech)
            );
            cursor += 30.0;
        }
        if !b.rows.is_empty() {
            let _ = write!(
                s,
                r#"<line class="ad-rows-rule" x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"/>"#,
                r.x + 1.0,
                cursor + 4.0,
                r.right() - 1.0,
                cursor + 4.0
            );
            let mut ry = cursor + 6.0;
            for row in &b.rows {
                let _ = write!(s, r#"<g class="ad-row{}">"#, if row.key.is_some() { " is-key" } else { "" });
                if let Some(k) = row.key {
                    let _ = write!(
                        s,
                        r#"<text class="ad-row-key" x="{:.1}" y="{:.1}">{}</text>"#,
                        r.x + 16.0,
                        ry + 14.0,
                        k
                    );
                }
                let _ = write!(
                    s,
                    r#"<text class="ad-row-name" x="{:.1}" y="{:.1}">{}</text>"#,
                    r.x + 16.0 + KEY_W,
                    ry + 14.0,
                    esc(&row.name)
                );
                if !row.type_name.is_empty() {
                    let _ = write!(
                        s,
                        r#"<text class="ad-row-type" x="{:.1}" y="{:.1}">{}</text>"#,
                        r.right() - 16.0,
                        ry + 14.0,
                        esc(&row.type_name)
                    );
                }
                s.push_str("</g>");
                ry += ROW_H;
            }
        }
        s.push_str("</g>");
    }
    s.push_str("</g>");
}

fn legend(s: &mut String, ir: &DiagramIR, lay: &Layout, opts: &SvgOptions) {
    let y = lay.legend_top;
    let x0 = MARGIN;
    let _ = write!(
        s,
        r#"<g class="ad-legend"><line class="ad-rule" x1="{x0}" y1="{y}" x2="{:.1}" y2="{y}"/>"#,
        lay.width - MARGIN
    );
    let present: BTreeSet<u8> = lay.edges.iter().map(|e| e.edge_type as u8).collect();
    let mut x = x0;
    let row = y + 28.0;
    let items: [(EdgeType, &str); 5] = [
        (EdgeType::Sync, "Sync call"),
        (EdgeType::Async, "Async"),
        (EdgeType::Event, "Event"),
        (EdgeType::Read, "Read"),
        (EdgeType::Write, "Write"),
    ];
    let er = ir.diagram_type == DiagramType::EntityRelationship;
    for (t, label) in items {
        if er || !present.contains(&(t as u8)) {
            continue;
        }
        let dash = match t {
            EdgeType::Async => " is-dashed",
            EdgeType::Event => " is-dotted",
            _ => "",
        };
        let _ = write!(
            s,
            r#"<g class="ad-edge ad-legend-sample {}{dash}"><path class="ad-edge-line" d="M{:.1},{:.1}H{:.1}"/></g><text class="ad-legend-text" x="{:.1}" y="{:.1}">{label}</text>"#,
            type_class(t),
            x,
            row - 4.0,
            x + 30.0,
            x + 40.0,
            row
        );
        x += 40.0 + sans_width(label, 12.0, false) + 24.0;
    }
    let sample = |s: &mut String, x: &mut f64, cls: &str, label: &str| {
        let _ = write!(
            s,
            r#"<g class="ad-edge ad-legend-sample {cls}"><path class="ad-edge-line" d="M{:.1},{:.1}H{:.1}"/></g><text class="ad-legend-text" x="{:.1}" y="{:.1}">{label}</text>"#,
            *x,
            row - 4.0,
            *x + 30.0,
            *x + 40.0,
            row
        );
        *x += 40.0 + sans_width(label, 12.0, false) + 24.0;
    };
    if ir.diagram_type == DiagramType::Sequence && ir.edges.iter().any(|e| e.is_reply()) {
        sample(s, &mut x, "t-sync is-reply is-dashed", "Reply");
    }
    if er && ir.edges.iter().any(|e| e.cardinality.is_some()) {
        sample(s, &mut x, "c-s-one c-t-one", "Exactly one");
        sample(s, &mut x, "c-s-one c-t-many", "Many");
    }
    if lay.edges.iter().any(|e| e.primary) {
        let _ = write!(
            s,
            r#"<g class="ad-edge ad-legend-sample t-sync is-primary"><path class="ad-edge-line" d="M{:.1},{:.1}H{:.1}"/></g><text class="ad-legend-text" x="{:.1}" y="{:.1}">Primary path</text>"#,
            x,
            row - 4.0,
            x + 30.0,
            x + 40.0,
            row
        );
        x += 40.0 + sans_width("Primary path", 12.0, false) + 24.0;
    }
    if ir.nodes.iter().any(|n| n.is_key_focal_point) {
        let _ = write!(
            s,
            r#"<rect class="ad-legend-focal" x="{:.1}" y="{:.1}" width="14" height="14" rx="3"/><text class="ad-legend-text" x="{:.1}" y="{:.1}">Focal point</text>"#,
            x,
            row - 11.0,
            x + 22.0,
            row
        );
        x += 22.0 + sans_width("Focal point", 12.0, false) + 24.0;
    }
    if ir.nodes.iter().any(|n| n.evidence.is_some()) {
        let _ = write!(
            s,
            r#"<text class="ad-evidence-glyph ad-legend-glyph" x="{:.1}" y="{:.1}">&lt;/&gt;</text><text class="ad-legend-text" x="{:.1}" y="{:.1}">Source evidence</text>"#,
            x,
            row,
            x + 26.0,
            row
        );
        x += 26.0 + sans_width("Source evidence", 12.0, false) + 24.0;
    }
    let footer = fit(&opts.footer, lay.width - 2.0 * MARGIN, |t| mono_width(t, 11.0));
    let footer_w = mono_width(&footer, 11.0);
    let (fx, fy, anchor) = if x + footer_w + 24.0 < lay.width - MARGIN {
        (lay.width - MARGIN, row, "end")
    } else {
        (x0, row + 24.0, "start")
    };
    let _ = write!(
        s,
        r#"<text class="ad-footer" x="{fx:.1}" y="{fy:.1}" text-anchor="{anchor}">{}</text></g>"#,
        esc(&footer)
    );
}

fn cardinality_text(c: Cardinality) -> &'static str {
    match c {
        Cardinality::OneToOne => "one to one",
        Cardinality::OneToMany => "one to many",
        Cardinality::ManyToOne => "many to one",
        Cardinality::ManyToMany => "many to many",
    }
}

const MARKERS: &str = r#"<defs><marker id="ad-one" viewBox="0 0 14 14" refX="14" refY="7" markerWidth="14" markerHeight="14" orient="auto-start-reverse" markerUnits="userSpaceOnUse"><path class="ad-mk-crow" d="M8,1 L8,13"/></marker><marker id="ad-many" viewBox="0 0 14 14" refX="14" refY="7" markerWidth="14" markerHeight="14" orient="auto-start-reverse" markerUnits="userSpaceOnUse"><path class="ad-mk-crow" d="M2,7 L14,1 M2,7 L14,13"/></marker><marker id="ad-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse" markerUnits="userSpaceOnUse"><path class="ad-mk" d="M0,1 L9,5 L0,9 z"/></marker><marker id="ad-arrow-strong" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse" markerUnits="userSpaceOnUse"><path class="ad-mk-strong" d="M0,1 L9,5 L0,9 z"/></marker><marker id="ad-arrow-accent" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse" markerUnits="userSpaceOnUse"><path class="ad-mk-accent" d="M0,1 L9,5 L0,9 z"/></marker><marker id="ad-open" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse" markerUnits="userSpaceOnUse"><path class="ad-mk-open" d="M1,1 L9,5 L1,9"/></marker><marker id="ad-open-strong" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse" markerUnits="userSpaceOnUse"><path class="ad-mk-open-strong" d="M1,1 L9,5 L1,9"/></marker><marker id="ad-open-accent" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse" markerUnits="userSpaceOnUse"><path class="ad-mk-open-accent" d="M1,1 L9,5 L1,9"/></marker><marker id="ad-dot" viewBox="0 0 10 10" refX="5" refY="5" markerWidth="6" markerHeight="6" markerUnits="userSpaceOnUse"><circle class="ad-mk" cx="5" cy="5" r="4"/></marker><marker id="ad-dot-accent" viewBox="0 0 10 10" refX="5" refY="5" markerWidth="6" markerHeight="6" markerUnits="userSpaceOnUse"><circle class="ad-mk-accent" cx="5" cy="5" r="4"/></marker></defs>"#;

pub fn layout_ir(ir: &DiagramIR) -> Layout {
    layout::layout(ir, title_metrics(ir))
}

/// Layout without the title block, for figures embedded in a titled page.
pub fn layout_embedded(ir: &DiagramIR) -> Layout {
    layout::layout(ir, TitleMetrics { header_h: -24.0, min_width: 0.0, legend_h: LEGEND_H })
}
