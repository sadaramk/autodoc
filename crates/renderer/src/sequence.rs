//! Sequence layout: participants in IR order as header cards, a lifeline under
//! each, and messages as horizontal arrows stacked top to bottom in `sequence`
//! order. Gaps between lifelines widen until every message label fits over its
//! span, so a label never has to leave its own row.

use autodoc_ir::{DiagramIR, EdgeStyle};

use crate::layout::{
    edge_label_text, edge_label_width, AttrRow, EdgeRoute, LabelBox, Layout, Lifeline, NodeBox, Rect, RouteKind,
    TitleMetrics, CARD_MAX_W, CARD_PAD_X, EDGE_LABEL_H, EVIDENCE_GLYPH_W, LABEL_GAP, LABEL_SIZE, MARGIN, SUBTITLE_SIZE,
    TECH_SIZE,
};
use crate::text::{fit, mono_width, sans_width};

const HEAD_MIN_W: f64 = 132.0;
const HEAD_GAP: f64 = 48.0;
/// Clearance between a label and the lifelines it spans.
const SPAN_PAD: f64 = 20.0;
const SELF_W: f64 = 28.0;
const SELF_H: f64 = 20.0;
const DETAIL_SIZE: f64 = 10.5;
const DETAIL_H: f64 = 14.0;
const ROW_GAP: f64 = 16.0;

struct Message {
    edge: usize,
    s: usize,
    t: usize,
    text: String,
    detail: Option<String>,
    w: f64,
    h: f64,
}

pub fn layout(ir: &DiagramIR, title: TitleMetrics) -> Layout {
    let n = ir.nodes.len();
    let index_of = |id: &str| ir.nodes.iter().position(|node| node.id == id);

    // ── Messages in order ────────────────────────────────────────────────────
    let mut skipped = Vec::new();
    let mut ordered: Vec<(u32, usize, usize, usize)> = Vec::new();
    for (ei, e) in ir.edges.iter().enumerate() {
        match (index_of(&e.source), index_of(&e.target)) {
            (Some(s), Some(t)) => ordered.push((e.sequence.unwrap_or(u32::MAX), ei, s, t)),
            _ => skipped.push(e.id.clone()),
        }
    }
    ordered.sort();
    let messages: Vec<Message> = ordered
        .iter()
        .enumerate()
        .map(|(pos, &(_, ei, s, t))| {
            let e = &ir.edges[ei];
            let text = edge_label_text(ir, e, pos + 1).unwrap_or_default();
            let detail = e.payload.as_deref().map(str::trim).filter(|p| !p.is_empty()).map(|p| fit(p, 280.0, mono));
            let w = edge_label_width(&text).max(detail.as_deref().map(|d| mono(d) + 14.0).unwrap_or(0.0));
            let h = EDGE_LABEL_H + if detail.is_some() { DETAIL_H } else { 0.0 };
            Message { edge: ei, s, t, text, detail, w, h }
        })
        .collect();

    // ── Participant cards ────────────────────────────────────────────────────
    let widths: Vec<f64> = ir
        .nodes
        .iter()
        .map(|node| {
            let evidence = if node.evidence.is_some() { EVIDENCE_GLYPH_W } else { 0.0 };
            let mut want = sans_width(&node.label, LABEL_SIZE, true) + 2.0 * CARD_PAD_X + evidence;
            if let Some(s) = &node.subtitle {
                want = want.max(sans_width(s, SUBTITLE_SIZE, false) + 2.0 * CARD_PAD_X);
            }
            if let Some(t) = &node.tech_stack {
                want = want.max(mono_width(t, TECH_SIZE) + 16.0 + 2.0 * CARD_PAD_X);
            }
            want.clamp(HEAD_MIN_W, CARD_MAX_W)
        })
        .collect();
    let mut boxes: Vec<NodeBox> = ir
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let w = widths[i];
            let evidence = if node.evidence.is_some() { EVIDENCE_GLYPH_W } else { 0.0 };
            let subtitle = node
                .subtitle
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .map(|s| fit(s, w - 2.0 * CARD_PAD_X, |x| sans_width(x, SUBTITLE_SIZE, false)));
            let tech = node
                .tech_stack
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .map(|s| fit(s, w - 2.0 * CARD_PAD_X - 16.0, |x| mono_width(x, TECH_SIZE)));
            NodeBox {
                id: node.id.clone(),
                index: i,
                rect: Rect { x: 0.0, y: 0.0, w, h: 0.0 },
                label: fit(&node.label, w - 2.0 * CARD_PAD_X - evidence, |s| sans_width(s, LABEL_SIZE, true)),
                tech_w: tech.as_ref().map(|t| mono_width(t, TECH_SIZE) + 16.0).unwrap_or(0.0),
                subtitle,
                tech,
                rows: Vec::<AttrRow>::new(),
            }
        })
        .collect();
    let head_h = boxes.iter().map(NodeBox::content_h).fold(0.0, f64::max);

    // ── Horizontal: lifeline centres, widened for labels ─────────────────────
    let mut gap = vec![0.0f64; n.saturating_sub(1)];
    for k in 0..gap.len() {
        gap[k] = widths[k] / 2.0 + HEAD_GAP + widths[k + 1] / 2.0;
    }
    let mut right_extra = 0.0f64;
    let mut spans: Vec<&Message> = messages.iter().collect();
    spans.sort_by_key(|m| (m.s.abs_diff(m.t), m.edge));
    for m in spans {
        if m.s == m.t {
            let need = SELF_W + 12.0 + m.w + 12.0;
            match gap.get_mut(m.s) {
                Some(g) => *g = g.max(need + widths[m.s + 1] / 2.0),
                None => right_extra = right_extra.max(need - widths[m.s] / 2.0),
            }
            continue;
        }
        let (a, b) = (m.s.min(m.t), m.s.max(m.t));
        let have: f64 = gap[a..b].iter().sum();
        let need = m.w + 2.0 * SPAN_PAD;
        if have < need {
            let add = (need - have) / (b - a) as f64;
            gap[a..b].iter_mut().for_each(|g| *g += add);
        }
    }
    let mut cx = vec![0.0f64; n];
    if n > 0 {
        cx[0] = widths[0] / 2.0;
    }
    for k in 1..n {
        cx[k] = cx[k - 1] + gap[k - 1];
    }
    for (i, b) in boxes.iter_mut().enumerate() {
        b.rect.x = cx[i] - widths[i] / 2.0;
        b.rect.h = head_h;
    }

    // ── Vertical: one row per message ────────────────────────────────────────
    let mut cursor = head_h + 20.0;
    let mut routes = Vec::new();
    for m in &messages {
        let e = &ir.edges[m.edge];
        let (points, label_rect, kind) = if m.s == m.t {
            let x = cx[m.s];
            let y = cursor + (m.h / 2.0 - SELF_H / 2.0).max(4.0);
            let rect = Rect { x: x + SELF_W + 12.0, y: y + SELF_H / 2.0 - m.h / 2.0, w: m.w, h: m.h };
            cursor = y + SELF_H.max(m.h) + ROW_GAP;
            (vec![(x, y), (x + SELF_W, y), (x + SELF_W, y + SELF_H), (x, y + SELF_H)], rect, RouteKind::SelfMessage)
        } else {
            let y = cursor + m.h + LABEL_GAP + 1.0;
            let mid = (cx[m.s] + cx[m.t]) / 2.0;
            let rect = Rect { x: mid - m.w / 2.0, y: cursor, w: m.w, h: m.h };
            cursor = y + ROW_GAP;
            (vec![(cx[m.s], y), (cx[m.t], y)], rect, RouteKind::Message)
        };
        routes.push(EdgeRoute {
            id: e.id.clone(),
            index: m.edge,
            source: e.source.clone(),
            target: e.target.clone(),
            kind,
            points,
            label: Some(LabelBox { rect: label_rect, text: m.text.clone(), detail: m.detail.clone() }),
            primary: e.primary(),
            edge_type: e.edge_type,
            dashed: e.resolved_style() == EdgeStyle::Dashed,
            hops: vec![],
        });
    }
    let bottom = cursor + 8.0;
    let mut lifelines: Vec<Lifeline> =
        boxes.iter().map(|b| Lifeline { id: b.id.clone(), x: b.rect.cx(), top: head_h, bottom }).collect();

    // ── Page frame ───────────────────────────────────────────────────────────
    let mut body_w = boxes.last().map(|b| b.rect.right()).unwrap_or(0.0) + right_extra;
    for r in &routes {
        if let Some(l) = &r.label {
            body_w = body_w.max(l.rect.right());
        }
    }
    let min_x = routes.iter().filter_map(|r| r.label.as_ref()).map(|l| l.rect.x).fold(0.0, f64::min);
    body_w -= min_x;
    let width = (body_w + 2.0 * MARGIN).max(title.min_width).max(720.0).ceil();
    let body_top = MARGIN + title.header_h;
    let dx = (width - body_w) / 2.0 - min_x;
    let dy = body_top;
    for b in &mut boxes {
        b.rect.x += dx;
        b.rect.y += dy;
    }
    for r in &mut routes {
        for p in &mut r.points {
            p.0 += dx;
            p.1 += dy;
        }
        if let Some(l) = &mut r.label {
            l.rect.x += dx;
            l.rect.y += dy;
        }
    }
    for l in &mut lifelines {
        l.x += dx;
        l.top += dy;
        l.bottom += dy;
    }
    let legend_top = body_top + bottom + 32.0;
    let height = (legend_top + title.legend_h + MARGIN).ceil();
    Layout {
        width,
        height,
        body_top,
        legend_top,
        nodes: boxes,
        containers: vec![],
        edges: routes,
        skipped_edges: skipped,
        lifelines,
    }
}

fn mono(s: &str) -> f64 {
    mono_width(s, DETAIL_SIZE)
}
