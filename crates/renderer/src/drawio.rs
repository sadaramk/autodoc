//! draw.io export, one way, as a CSV import.
//!
//! A generated diagram is sometimes the starting point for a drawing a person
//! then owns — a slide, an annotated review, a diagram with a boundary autodoc
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

use autodoc_ir::{DiagramIR, EdgeStyle, EdgeType, Evidence};

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

fn edge_style(e: &autodoc_ir::Edge) -> String {
    let dashed = matches!(e.style, Some(EdgeStyle::Dashed)) || matches!(e.edge_type, EdgeType::Async);
    let color = if e.is_primary_path == Some(true) { "#4f46e5" } else { "#64748b" };
    format!(
        "edgeStyle=orthogonalEdgeStyle;rounded=1;html=1;strokeColor={color};dashed={};endArrow=blockThin;endFill=1;",
        u8::from(dashed)
    )
}

/// The diagram as a draw.io CSV import, with evidence on every shape.
pub fn to_csv(ir: &DiagramIR, permalink: &dyn Fn(&Evidence) -> Option<String>) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {} — exported from autodoc, one way.\n", ir.title));
    out.push_str("# Import with Extras → Insert → Advanced → CSV. Evidence travels as shape data:\n");
    out.push_str("# right-click a shape → Edit Data to see the file and line it came from.\n");
    out.push_str("#\n");
    out.push_str("# label: %label%\n");
    out.push_str("# style: %style%\n");
    out.push_str("# namespace: autodoc-\n");
    out.push_str("# identity: id\n");
    // `parent` groups nodes into the boundaries the model already has.
    if !ir.containers.is_empty() {
        out.push_str("# parent: parent\n");
        out.push_str(
            "# parentstyle: rounded=0;html=1;fillColor=none;strokeColor=#cbd5e1;dashed=1;\
                      verticalAlign=top;align=left;spacingLeft=8;fontColor=#475569;\n",
        );
    }
    out.push_str("# link: url\n");
    out.push_str("# width: auto\n");
    out.push_str("# height: auto\n");
    out.push_str("# padding: 16\n");
    out.push_str("# nodespacing: 40\n");
    out.push_str("# levelspacing: 80\n");
    out.push_str("# edgespacing: 40\n");
    out.push_str("# layout: horizontalflow\n");
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
    let mut ignored = vec!["id", "style", "parent", "url"];
    let cols: Vec<&str> = edge_columns.iter().map(|(_, c)| c.as_str()).collect();
    ignored.extend(cols);
    out.push_str(&format!("# ignore: {}\n", ignored.join(",")));

    let mut header = vec!["id".to_string(), "label".to_string(), "style".to_string()];
    if !ir.containers.is_empty() {
        header.push("parent".into());
    }
    header.extend(["tech".to_string(), "evidence".to_string(), "url".to_string()]);
    for (_, col) in &edge_columns {
        header.push(col.clone());
    }
    out.push_str(&header.join(","));
    out.push('\n');

    // Boundaries first: a parent has to exist before a child names it.
    for c in &ir.containers {
        let mut row = vec![
            field(&safe_id(&c.id)),
            field(&c.label),
            field("rounded=0;html=1;fillColor=none;strokeColor=#cbd5e1;dashed=1;verticalAlign=top;align=left;spacingLeft=8;fontColor=#475569;"),
        ];
        row.push(String::new()); // a boundary has no parent
        row.extend([String::new(), String::new(), String::new()]);
        for _ in &edge_columns {
            row.push(String::new());
        }
        out.push_str(&row.join(","));
        out.push('\n');
    }

    for n in &ir.nodes {
        let mut row = vec![field(&safe_id(&n.id)), field(&n.label), field(&shape_style(n.is_key_focal_point))];
        if !ir.containers.is_empty() {
            row.push(field(&n.container_id.as_deref().map(safe_id).unwrap_or_default()));
        }
        row.push(field(n.tech_stack.as_deref().unwrap_or("")));
        let ev = n.evidence.as_ref();
        row.push(field(&ev.map(evidence_ref).unwrap_or_default()));
        row.push(field(&ev.and_then(permalink).unwrap_or_default()));
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
