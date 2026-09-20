//! draw.io export, one way, as a CSV import.
//!
//! A generated diagram is sometimes the starting point for a drawing a person
//! then owns — a slide, an annotated review, a diagram with a boundary nunki
//! cannot see. draw.io's CSV import is the only interchange format it reads that
//! survives being edited: it produces real shapes with a layout applied, not a
//! flattened image.
//!
//! One way on purpose. A round trip would mean parsing back a file a person has
//! since moved, restyled and annotated, and deciding which side is right about
//! the architecture. The source is right about the architecture; the drawing is
//! right about the drawing. What travels is the evidence: every shape carries
//! its `file:line` and, when the repository has a forge remote, a link to the
//! line it came from, so a diagram pasted into a review can still be checked.
//!
//! Import in draw.io with Extras → Insert → Advanced → CSV.

use nunki_ir::{DiagramIR, EdgeStyle, EdgeType, Evidence};

use std::collections::BTreeMap;

use crate::layout::{self, Rect};

/// A CSV field: quoted when it has to be, escaped the way RFC 4180 says.
fn field(v: &str) -> String {
    // draw.io splits on commas and honours double quotes; a newline inside a
    // quoted field would end the row for its parser, so they become spaces.
    let flat = v.replace(['\n', '\r'], " ");
    if flat.contains([',', '"']) {
        format!("\"{}\"", flat.replace('"', "\"\""))
    } else {
        flat
    }
}

fn evidence_ref(e: &Evidence) -> String {
    if e.start_line == e.end_line {
        format!("{}:{}", e.file_path, e.start_line)
    } else {
        format!("{}:{}-{}", e.file_path, e.start_line, e.end_line)
    }
}

/// `left,top,width,height`, rounded: draw.io parses these as numbers and a long
/// fraction only makes the file harder to read.
fn geometry(r: &Rect) -> Vec<String> {
    [r.x, r.y, r.w, r.h].iter().map(|v| format!("{}", v.round() as i64)).collect()
}

/// draw.io needs an identifier it can use in `connect`; ours may hold anything.
fn safe_id(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect()
}

fn shape_style(focal: bool) -> String {
    let fill = if focal { "#eef2ff" } else { "#ffffff" };
    let stroke = if focal { "#4f46e5" } else { "#94a3b8" };
    format!(
        "rounded=1;whiteSpace=wrap;html=1;fillColor={fill};strokeColor={stroke};\
         fontColor=#0f172a;strokeWidth={};align=center;verticalAlign=middle;",
        if focal { 2 } else { 1 }
    )
}

fn edge_style(e: &nunki_ir::Edge) -> String {
    let dashed = matches!(e.style, Some(EdgeStyle::Dashed)) || matches!(e.edge_type, EdgeType::Async);
    let color = if e.is_primary_path == Some(true) { "#4f46e5" } else { "#64748b" };
    // A white label background, or several edges leaving one node print their
    // labels over each other and over the node's own name.
    format!(
        "edgeStyle=orthogonalEdgeStyle;rounded=1;html=1;strokeColor={color};dashed={};endArrow=blockThin;\
         endFill=1;labelBackgroundColor=#ffffff;fontSize=11;fontColor=#475569;",
        u8::from(dashed)
    )
}

/// The diagram as a draw.io CSV import, with evidence on every shape.
pub fn to_csv(ir: &DiagramIR, permalink: &dyn Fn(&Evidence) -> Option<String>) -> String {
    let l = layout::layout(ir, crate::svg::title_metrics(ir));
    let at: BTreeMap<&str, Rect> = l
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.rect))
        .chain(l.containers.iter().map(|c| (c.id.as_str(), c.rect)))
        .collect();
    // An empty `link` column would put an empty link on every shape, which
    // reads as a broken one. The directive is only worth emitting when at least
    // one node actually resolves to a URL.
    let links = ir.nodes.iter().filter_map(|n| n.evidence.as_ref()).any(|e| permalink(e).is_some());
    let mut out = String::new();
    out.push_str(&format!("# {} — exported from nunki, one way.\n", ir.title));
    out.push_str("# Import with Extras → Insert → Advanced → CSV. Evidence travels as shape data:\n");
    out.push_str("# right-click a shape → Edit Data to see the file and line it came from.\n");
    out.push_str("#\n");
    out.push_str("# label: %label%\n");
    out.push_str("# style: %style%\n");
    out.push_str("# namespace: nunki-\n");
    out.push_str("# identity: id\n");
    // `parent` groups nodes into the boundaries the model already has.
    if !ir.containers.is_empty() {
        out.push_str("# parent: parent\n");
        out.push_str(
            "# parentstyle: rounded=0;html=1;fillColor=none;strokeColor=#cbd5e1;dashed=1;\
                      verticalAlign=top;align=left;spacingLeft=8;fontColor=#475569;\n",
        );
    }
    if links {
        out.push_str("# link: url\n");
    }
    // Positions come from nunki's own layout, not from draw.io's. Asking
    // draw.io to lay this out produced crossed edges and labels printed over one
    // another, because its flow layouts do not respect the boundary groups the
    // model has. The book's layout already places these boxes well, so the
    // exported diagram looks like the figure it came from.
    // `left`/`top` name a column; `width`/`height` need `@` or draw.io ignores
    // them and auto-sizes every shape to its label.
    out.push_str("# left: left\n");
    out.push_str("# top: top\n");
    out.push_str("# width: @width\n");
    out.push_str("# height: @height\n");
    out.push_str("# layout: none\n");
    // draw.io matches a `connect` rule against a column rather than a row, so
    // edges cannot be listed one by one: outgoing edges become one column per
    // distinct label and style, each holding the targets it points at.
    let mut edge_columns: Vec<(String, String)> = Vec::new();
    for e in &ir.edges {
        let style = edge_style(e);
        let label = e.label.clone().unwrap_or_default();
        let key = format!("{style}\u{1}{label}");
        if !edge_columns.iter().any(|(k, _)| *k == key) {
            edge_columns.push((key, format!("edge{}", edge_columns.len() + 1)));
        }
    }
    for (key, col) in &edge_columns {
        let (style, label) = key.split_once('\u{1}').unwrap_or((key.as_str(), ""));
        out.push_str(&format!(
            "# connect: {{\"from\":\"{col}\",\"to\":\"id\",\"invert\":true,\"label\":\"{}\",\"style\":\"{}\"}}\n",
            label.replace('"', "'"),
            style
        ));
    }
    // `tech` and `evidence` are deliberately not ignored: they become the shape's
    // data, which is the whole point of exporting rather than screenshotting.
    let mut ignored = vec!["id", "style", "parent", "left", "top", "width", "height"];
    if links {
        ignored.push("url");
    }
    let cols: Vec<&str> = edge_columns.iter().map(|(_, c)| c.as_str()).collect();
    ignored.extend(cols);
    out.push_str(&format!("# ignore: {}\n", ignored.join(",")));

    let mut header = vec![
        "id".to_string(),
        "label".to_string(),
        "style".to_string(),
        "left".to_string(),
        "top".to_string(),
        "width".to_string(),
        "height".to_string(),
    ];
    if !ir.containers.is_empty() {
        header.push("parent".into());
    }
    header.push("tech".into());
    header.push("evidence".into());
    if links {
        header.push("url".into());
    }
    for (_, col) in &edge_columns {
        header.push(col.clone());
    }
    out.push_str(&header.join(","));
    out.push('\n');

    // Boundaries first: a parent has to exist before a child names it.
    for c in &ir.containers {
        let r = at.get(c.id.as_str()).copied().unwrap_or(Rect { x: 0.0, y: 0.0, w: 240.0, h: 160.0 });
        let mut row = vec![
            field(&safe_id(&c.id)),
            field(&c.label),
            field("rounded=0;html=1;fillColor=none;strokeColor=#cbd5e1;dashed=1;verticalAlign=top;align=left;spacingLeft=8;fontColor=#475569;"),
        ];
        row.extend(geometry(&r));
        row.push(String::new()); // a boundary has no parent
        row.push(String::new()); // tech
        row.push(String::new()); // evidence
        if links {
            row.push(String::new());
        }
        for _ in &edge_columns {
            row.push(String::new());
        }
        out.push_str(&row.join(","));
        out.push('\n');
    }

    for n in &ir.nodes {
        let mut row = vec![field(&safe_id(&n.id)), field(&n.label), field(&shape_style(n.is_key_focal_point))];
        // A child's position is relative to its parent in draw.io, so a node
        // inside a boundary is offset by that boundary's origin.
        let r = at.get(n.id.as_str()).copied().unwrap_or(Rect { x: 0.0, y: 0.0, w: 160.0, h: 60.0 });
        let parent = n.container_id.as_deref().filter(|_| !ir.containers.is_empty());
        let origin = parent.and_then(|c| at.get(c)).copied().unwrap_or(Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 });
        row.extend(geometry(&Rect { x: r.x - origin.x, y: r.y - origin.y, w: r.w, h: r.h }));
        if !ir.containers.is_empty() {
            row.push(field(&parent.map(safe_id).unwrap_or_default()));
        }
        row.push(field(n.tech_stack.as_deref().unwrap_or("")));
        let ev = n.evidence.as_ref();
        row.push(field(&ev.map(evidence_ref).unwrap_or_default()));
        if links {
            row.push(field(&ev.and_then(permalink).unwrap_or_default()));
        }
        for (key, _) in &edge_columns {
            let (style, label) = key.split_once('\u{1}').unwrap_or((key.as_str(), ""));
            let targets: Vec<String> = ir
                .edges
                .iter()
                .filter(|e| e.source == n.id && edge_style(e) == style && e.label.as_deref().unwrap_or("") == label)
                .map(|e| safe_id(&e.target))
                .collect();
            row.push(field(&targets.join(",")));
        }
        out.push_str(&row.join(","));
        out.push('\n');
    }
    out
}

/// XML escaping for an attribute value.
fn xml(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for c in v.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' | '\r' => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

/// Point halfway along a polyline, by length rather than by index: the middle
/// vertex of an L-shaped route is its corner, not its middle.
fn midpoint(points: &[(f64, f64)]) -> (f64, f64) {
    if points.is_empty() {
        return (0.0, 0.0);
    }
    if points.len() == 1 {
        return points[0];
    }
    let seg: Vec<f64> =
        points.windows(2).map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt()).collect();
    let half = seg.iter().sum::<f64>() / 2.0;
    let mut run = 0.0;
    for (i, len) in seg.iter().enumerate() {
        if run + len >= half && *len > 0.0 {
            let t = (half - run) / len;
            let (a, b) = (points[i], points[i + 1]);
            return (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        }
        run += len;
    }
    *points.last().unwrap()
}

/// The diagram as a draw.io file: `.drawio` XML, opened by double-clicking.
///
/// The CSV import cannot carry a route or a label position, so draw.io re-routes
/// every edge and places every label at its own midpoint — which sends
/// connectors through boxes and prints edge labels over node names. This format
/// carries the layout the book already computed, so the file opens looking like
/// the figure it came from.
pub fn to_xml(ir: &DiagramIR, permalink: &dyn Fn(&Evidence) -> Option<String>) -> String {
    let l = layout::layout(ir, crate::svg::title_metrics(ir));
    let at: BTreeMap<&str, Rect> = l
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.rect))
        .chain(l.containers.iter().map(|c| (c.id.as_str(), c.rect)))
        .collect();

    let mut out = String::new();
    out.push_str(&format!(
        "<mxfile host=\"nunki\" agent=\"nunki {}\">\n  <diagram name=\"{}\">\n",
        env!("CARGO_PKG_VERSION"),
        xml(&ir.title)
    ));
    out.push_str(&format!(
        "    <mxGraphModel dx=\"{}\" dy=\"{}\" grid=\"1\" gridSize=\"10\" guides=\"1\" tooltips=\"1\" \
         connect=\"1\" arrows=\"1\" fold=\"1\" page=\"1\" pageScale=\"1\" math=\"0\" shadow=\"0\">\n      \
         <root>\n        <mxCell id=\"0\" />\n        <mxCell id=\"1\" parent=\"0\" />\n",
        l.width.round() as i64,
        l.height.round() as i64
    ));

    let cell = |id: &str| format!("nunki-{}", safe_id(id));
    for c in &ir.containers {
        let r = at.get(c.id.as_str()).copied().unwrap_or(Rect { x: 0.0, y: 0.0, w: 240.0, h: 160.0 });
        out.push_str(&format!(
            "        <mxCell id=\"{}\" value=\"{}\" style=\"rounded=0;html=1;fillColor=none;\
             strokeColor=#cbd5e1;dashed=1;verticalAlign=top;align=left;spacingLeft=8;fontColor=#475569;\" \
             vertex=\"1\" parent=\"1\">\n          <mxGeometry x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" \
             as=\"geometry\" />\n        </mxCell>\n",
            cell(&c.id),
            xml(&c.label),
            r.x.round() as i64,
            r.y.round() as i64,
            r.w.round() as i64,
            r.h.round() as i64
        ));
    }

    for n in &ir.nodes {
        let r = at.get(n.id.as_str()).copied().unwrap_or(Rect { x: 0.0, y: 0.0, w: 160.0, h: 60.0 });
        let parent = n.container_id.as_deref().filter(|c| at.contains_key(c));
        let origin = parent.and_then(|c| at.get(c)).copied().unwrap_or(Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 });
        let ev = n.evidence.as_ref();
        // An `<object>` rather than a bare cell: its attributes are the shape's
        // data, which is where the evidence has to live to survive editing.
        let mut attrs = format!("label=\"{}\"", xml(&n.label));
        if let Some(t) = n.tech_stack.as_deref() {
            attrs.push_str(&format!(" tech=\"{}\"", xml(t)));
        }
        if let Some(e) = ev {
            attrs.push_str(&format!(" evidence=\"{}\"", xml(&evidence_ref(e))));
        }
        if let Some(url) = ev.and_then(permalink) {
            attrs.push_str(&format!(" link=\"{}\"", xml(&url)));
        }
        out.push_str(&format!(
            "        <object {attrs} id=\"{}\">\n          <mxCell style=\"{}\" vertex=\"1\" parent=\"{}\">\n            \
             <mxGeometry x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" as=\"geometry\" />\n          </mxCell>\n        \
             </object>\n",
            cell(&n.id),
            xml(&shape_style(n.is_key_focal_point)),
            parent.map(cell).unwrap_or_else(|| "1".into()),
            (r.x - origin.x).round() as i64,
            (r.y - origin.y).round() as i64,
            r.w.round() as i64,
            r.h.round() as i64
        ));
    }

    for (i, route) in l.edges.iter().enumerate() {
        let Some(e) = ir.edges.iter().find(|e| e.id == route.id) else { continue };
        let label = route.label.as_ref().map(|b| b.text.clone()).or_else(|| e.label.clone()).unwrap_or_default();
        // The label goes on the edge so it travels with it, offset from where
        // draw.io would otherwise put it — the midpoint of the route.
        let mid = midpoint(&route.points);
        let offset = route
            .label
            .as_ref()
            .map(|b| ((b.rect.cx() - mid.0).round() as i64, (b.rect.cy() - mid.1).round() as i64))
            .unwrap_or((0, 0));
        out.push_str(&format!(
            "        <mxCell id=\"nunki-e{i}\" value=\"{}\" style=\"{}\" edge=\"1\" parent=\"1\" \
             source=\"{}\" target=\"{}\">\n          <mxGeometry relative=\"1\" as=\"geometry\">\n",
            xml(&label),
            xml(&edge_style(e)),
            cell(&route.source),
            cell(&route.target)
        ));
        // Only the turns: mxGraph works out where the line meets each box.
        let inner = if route.points.len() > 2 { &route.points[1..route.points.len() - 1] } else { &[][..] };
        if !inner.is_empty() {
            out.push_str("            <Array as=\"points\">\n");
            for (x, y) in inner {
                out.push_str(&format!(
                    "              <mxPoint x=\"{}\" y=\"{}\" />\n",
                    x.round() as i64,
                    y.round() as i64
                ));
            }
            out.push_str("            </Array>\n");
        }
        out.push_str(&format!(
            "            <mxPoint as=\"offset\" x=\"{}\" y=\"{}\" />\n          </mxGeometry>\n        </mxCell>\n",
            offset.0, offset.1
        ));
    }

    out.push_str("      </root>\n    </mxGraphModel>\n  </diagram>\n</mxfile>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_with_a_comma_or_quote_is_quoted() {
        assert_eq!(field("plain"), "plain");
        assert_eq!(field("a,b"), "\"a,b\"");
        assert_eq!(field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(field("two\nlines"), "two lines", "a newline would end the row");
    }

    #[test]
    fn an_id_draw_io_cannot_use_is_made_safe() {
        assert_eq!(safe_id("api-gateway"), "api-gateway");
        assert_eq!(safe_id("db:orders"), "db-orders");
    }

    #[test]
    fn a_line_range_reads_as_a_range() {
        let one = Evidence { file_path: "a.rs".into(), start_line: 3, end_line: 3, symbol_name: None };
        let many = Evidence { file_path: "a.rs".into(), start_line: 3, end_line: 9, symbol_name: None };
        assert_eq!(evidence_ref(&one), "a.rs:3");
        assert_eq!(evidence_ref(&many), "a.rs:3-9");
    }
}
