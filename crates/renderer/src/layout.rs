//! Deterministic layered layout with orthogonal routing.
//!
//! Containers become blocks; blocks are ranked left→right along the dominant
//! flow and stacked in columns. Nodes inside a block form a single column, so
//! every card touches open canvas on both sides. Connectors never pass behind
//! a box: they leave a card into the gutter beside its column, travel
//! vertically on their own track, and cross intermediate columns only through
//! free horizontal lanes between blocks.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use nunki_ir::{DiagramIR, DiagramType, Edge, EdgeStyle, EdgeType, KeyKind};
use serde::Serialize;

use crate::text::{fit, mono_width, sans_width};

pub const CARD_MIN_W: f64 = 196.0;
pub const CARD_MAX_W: f64 = 264.0;
pub const CARD_PAD_X: f64 = 16.0;
pub const LABEL_SIZE: f64 = 14.0;
pub const SUBTITLE_SIZE: f64 = 12.0;
pub const TECH_SIZE: f64 = 11.0;
pub const EDGE_LABEL_SIZE: f64 = 11.0;
pub const EDGE_LABEL_H: f64 = 18.0;
pub const LABEL_GAP: f64 = 6.0;
const TRACK: f64 = 12.0;
/// Pitch between gutter lanes once sharing has thinned them out. 12px reads as
/// one thick line from a metre away; 20px reads as separate connectors.
const TRACK_WIDE: f64 = 20.0;
/// Vertical air between two runs sharing a lane, so they read as two
/// connectors that happen to line up rather than one broken line.
const TRACK_CLEAR: f64 = 24.0;
const BLOCK_GAP: f64 = 56.0;
const NODE_GAP: f64 = 28.0;
const NODE_GAP_LABELLED: f64 = 56.0;
const PAD_X: f64 = 20.0;
const PAD_TOP: f64 = 44.0;
const PAD_BOTTOM: f64 = 20.0;
const LANE_CLEAR: f64 = 18.0;
const PORT_SPACING: f64 = 14.0;
pub const MARGIN: f64 = 48.0;
pub const EVIDENCE_GLYPH_W: f64 = 22.0;
/// Entity attribute rows (entity-relationship cards).
pub const ROW_H: f64 = 20.0;
pub const ROW_SIZE: f64 = 12.0;
pub const ROW_TYPE_SIZE: f64 = 11.0;
pub const KEY_W: f64 = 40.0;
/// Rows beyond this collapse into a "+ N more" row.
pub const MAX_ROWS: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn right(&self) -> f64 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }
    pub fn cx(&self) -> f64 {
        self.x + self.w / 2.0
    }
    pub fn cy(&self) -> f64 {
        self.y + self.h / 2.0
    }
    pub fn inflate(&self, d: f64) -> Rect {
        Rect { x: self.x - d, y: self.y - d, w: self.w + 2.0 * d, h: self.h + 2.0 * d }
    }
    pub fn intersects(&self, o: &Rect) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }
    pub fn overlap_area(&self, o: &Rect) -> f64 {
        let w = self.right().min(o.right()) - self.x.max(o.x);
        let h = self.bottom().min(o.bottom()) - self.y.max(o.y);
        if w > 0.0 && h > 0.0 {
            w * h
        } else {
            0.0
        }
    }
    fn translate(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeBox {
    pub id: String,
    pub index: usize,
    pub rect: Rect,
    pub label: String,
    pub subtitle: Option<String>,
    pub tech: Option<String>,
    pub tech_w: f64,
    /// Attribute rows of an entity card (entity-relationship diagrams).
    pub rows: Vec<AttrRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttrRow {
    pub name: String,
    pub type_name: String,
    /// "PK", "FK", "PK FK"; `None` for plain columns and the overflow row.
    pub key: Option<&'static str>,
    pub nullable: bool,
}

impl NodeBox {
    /// Height of the card's content before port spacing stretches it.
    pub fn content_h(&self) -> f64 {
        card_content_h(self.subtitle.is_some(), self.tech.is_some(), self.rows.len())
    }
}

fn card_content_h(subtitle: bool, tech: bool, rows: usize) -> f64 {
    let mut h: f64 = 16.0 + 18.0 + 14.0;
    if subtitle {
        h += 18.0;
    }
    if tech {
        h += 30.0;
    }
    if rows > 0 {
        h += 6.0 + rows as f64 * ROW_H;
    }
    h
}

pub fn key_label(k: KeyKind) -> &'static str {
    match k {
        KeyKind::Pk => "PK",
        KeyKind::Fk => "FK",
        KeyKind::PkFk => "PK FK",
    }
}

fn attr_rows(node: &nunki_ir::Node, w: f64) -> Vec<AttrRow> {
    let Some(attrs) = &node.attributes else { return vec![] };
    let name_max = (w - 2.0 * CARD_PAD_X - KEY_W) * 0.55;
    let mut rows: Vec<AttrRow> = attrs
        .iter()
        .take(if attrs.len() > MAX_ROWS { MAX_ROWS - 1 } else { MAX_ROWS })
        .map(|a| {
            let name = fit(&a.name, name_max, |x| sans_width(x, ROW_SIZE, a.key.is_some()));
            let type_max = w - 2.0 * CARD_PAD_X - KEY_W - sans_width(&name, ROW_SIZE, a.key.is_some()) - 12.0;
            let ty = format!("{}{}", a.type_name, if a.nullable { "?" } else { "" });
            AttrRow {
                name,
                type_name: fit(&ty, type_max, |x| mono_width(x, ROW_TYPE_SIZE)),
                key: a.key.map(key_label),
                nullable: a.nullable,
            }
        })
        .collect();
    if attrs.len() > MAX_ROWS {
        rows.push(AttrRow {
            name: format!("+ {} more", attrs.len() - (MAX_ROWS - 1)),
            type_name: String::new(),
            key: None,
            nullable: false,
        });
    }
    rows
}

/// Canvas text of an edge label: sequence messages are numbered, lifecycle
/// transitions carry their guard in brackets.
pub fn edge_label_text(ir: &DiagramIR, e: &Edge, order: usize) -> Option<String> {
    let label = e.label.as_deref().map(str::trim).filter(|l| !l.is_empty());
    match ir.diagram_type {
        DiagramType::Sequence => {
            let n = e.sequence.map(|s| s as usize).unwrap_or(order);
            Some(match label {
                Some(l) => format!("{n}. {l}"),
                None => format!("{n}."),
            })
        }
        DiagramType::Lifecycle => {
            let guard = e.guard.as_deref().map(str::trim).filter(|g| !g.is_empty());
            match (label, guard) {
                (Some(l), Some(g)) => Some(format!("{l} [{g}]")),
                (None, Some(g)) => Some(format!("[{g}]")),
                (l, None) => l.map(str::to_string),
            }
        }
        _ => label.map(str::to_string),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerBox {
    pub id: String,
    pub index: usize,
    pub rect: Rect,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelBox {
    pub rect: Rect,
    pub text: String,
    /// Second, monospace line (a sequence message's payload).
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouteKind {
    VerticalAdjacent,
    Channel,
    ForwardAdjacent,
    BackwardAdjacent,
    ForwardLane,
    BackwardLane,
    SameColumn,
    /// Sequence message between two lifelines.
    Message,
    /// Sequence message a participant sends to itself.
    SelfMessage,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EdgeRoute {
    pub id: String,
    pub index: usize,
    pub source: String,
    pub target: String,
    pub kind: RouteKind,
    pub points: Vec<(f64, f64)>,
    pub label: Option<LabelBox>,
    pub primary: bool,
    pub edge_type: EdgeType,
    pub dashed: bool,
    /// Points on horizontal segments where this edge hops over another.
    pub hops: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub width: f64,
    pub height: f64,
    /// Top of the diagram body (below the title block).
    pub body_top: f64,
    /// Top of the legend strip.
    pub legend_top: f64,
    pub nodes: Vec<NodeBox>,
    pub containers: Vec<ContainerBox>,
    pub edges: Vec<EdgeRoute>,
    /// Edges dropped because an endpoint is missing or they loop to themselves.
    pub skipped_edges: Vec<String>,
    /// Sequence diagrams: one lifeline per participant, below its header card.
    pub lifelines: Vec<Lifeline>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lifeline {
    pub id: String,
    pub x: f64,
    pub top: f64,
    pub bottom: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

struct Block {
    container: Option<usize>,
    nodes: Vec<usize>,
    /// Global (block-level) column.
    col: usize,
    /// Node stacks per sub-column; busy containers spread across several.
    stacks: Vec<Vec<usize>>,
    /// First leaf column this block occupies.
    leaf: usize,
    rect: Rect,
}

struct Plan {
    edge: usize,
    s: usize,
    t: usize,
    kind: RouteKind,
    s_side: Side,
    t_side: Side,
    lane_y: Option<f64>,
}

pub struct TitleMetrics {
    pub header_h: f64,
    pub min_width: f64,
    pub legend_h: f64,
}

pub fn layout(ir: &DiagramIR, title: TitleMetrics) -> Layout {
    if ir.diagram_type == DiagramType::Sequence {
        return crate::sequence::layout(ir, title);
    }
    let n = ir.nodes.len();
    let node_idx: HashMap<&str, usize> = ir.nodes.iter().enumerate().map(|(i, n)| (n.id.as_str(), i)).collect();
    let mut skipped = Vec::new();
    let valid_edges: Vec<(usize, usize, usize)> = ir
        .edges
        .iter()
        .enumerate()
        .filter_map(|(ei, e)| {
            let pair = (node_idx.get(e.source.as_str()), node_idx.get(e.target.as_str()));
            match pair {
                (Some(&s), Some(&t)) if s != t => Some((ei, s, t)),
                _ => {
                    skipped.push(e.id.clone());
                    None
                }
            }
        })
        .collect();

    // ── 1. Blocks ────────────────────────────────────────────────────────────
    let container_idx: HashMap<&str, usize> =
        ir.containers.iter().enumerate().map(|(i, c)| (c.id.as_str(), i)).collect();
    let mut blocks: Vec<Block> = Vec::new();
    let mut block_of = vec![0usize; n];
    let mut by_container: BTreeMap<usize, usize> = BTreeMap::new();
    let new_block = |container| Block { container, nodes: vec![], col: 0, stacks: vec![], leaf: 0, rect: zero() };
    for (i, node) in ir.nodes.iter().enumerate() {
        let c = node.container_id.as_deref().and_then(|c| container_idx.get(c)).copied();
        let bi = match c {
            Some(c) => *by_container.entry(c).or_insert_with(|| {
                blocks.push(new_block(Some(c)));
                blocks.len() - 1
            }),
            None => {
                blocks.push(new_block(None));
                blocks.len() - 1
            }
        };
        blocks[bi].nodes.push(i);
        block_of[i] = bi;
    }
    let nb = blocks.len();

    // ── 2. Rank blocks (cycle-broken longest path) ───────────────────────────
    let mut block_succ: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); nb];
    for &(_, s, t) in &valid_edges {
        let (bs, bt) = (block_of[s], block_of[t]);
        if bs != bt {
            block_succ[bs].insert(bt);
        }
    }
    let rank = longest_path_ranks(&break_cycles(&block_succ));
    for (b, block) in blocks.iter_mut().enumerate() {
        block.col = rank[b];
    }
    let ncols = rank.iter().copied().max().unwrap_or(0) + 1;

    // ── 3. Arrange nodes inside blocks, then blocks inside columns ───────────
    for block in blocks.iter_mut() {
        arrange_block(block, &valid_edges);
    }
    let mut columns: Vec<Vec<usize>> = vec![vec![]; ncols];
    for b in 0..nb {
        columns[blocks[b].col].push(b);
    }
    order_columns(&mut columns, &block_succ);
    let mut leaf_start = vec![0usize; ncols + 1];
    for c in 0..ncols {
        let width = columns[c].iter().map(|&b| blocks[b].stacks.len()).max().unwrap_or(1).max(1);
        leaf_start[c + 1] = leaf_start[c] + width;
    }
    let nleaves = leaf_start[ncols];
    let mut leaf_of = vec![0usize; n];
    let mut stack_pos: HashMap<usize, (usize, usize)> = HashMap::new();
    for block in blocks.iter_mut() {
        block.leaf = leaf_start[block.col];
        for (sc, stack) in block.stacks.iter().enumerate() {
            for (k, &ni) in stack.iter().enumerate() {
                leaf_of[ni] = block.leaf + sc;
                stack_pos.insert(ni, (sc, k));
            }
        }
    }

    // ── 4. Classify edges by leaf columns ────────────────────────────────────
    let mut plans: Vec<Plan> = valid_edges
        .iter()
        .map(|&(ei, s, t)| {
            let (ls, lt) = (leaf_of[s], leaf_of[t]);
            let kind = if ls == lt {
                let same_stack = block_of[s] == block_of[t] && stack_pos[&s].0 == stack_pos[&t].0;
                if same_stack && stack_pos[&t].1 == stack_pos[&s].1 + 1 {
                    RouteKind::VerticalAdjacent
                } else if same_stack {
                    RouteKind::Channel
                } else {
                    RouteKind::SameColumn
                }
            } else if lt == ls + 1 {
                RouteKind::ForwardAdjacent
            } else if lt > ls {
                RouteKind::ForwardLane
            } else if lt + 1 == ls {
                RouteKind::BackwardAdjacent
            } else {
                RouteKind::BackwardLane
            };
            let (s_side, t_side) = match kind {
                RouteKind::VerticalAdjacent => (Side::Bottom, Side::Top),
                RouteKind::Channel | RouteKind::SameColumn => (Side::Right, Side::Right),
                RouteKind::ForwardAdjacent | RouteKind::ForwardLane => (Side::Right, Side::Left),
                RouteKind::BackwardAdjacent | RouteKind::BackwardLane => (Side::Left, Side::Right),
                RouteKind::Message | RouteKind::SelfMessage => unreachable!("sequence layout routes messages"),
            };
            Plan { edge: ei, s, t, kind, s_side, t_side, lane_y: None }
        })
        .collect();
    let mut port_count: HashMap<(usize, Side), usize> = HashMap::new();
    let mut channel_tracks = vec![0usize; nleaves];
    for p in &plans {
        *port_count.entry((p.s, p.s_side)).or_default() += 1;
        *port_count.entry((p.t, p.t_side)).or_default() += 1;
        if p.kind == RouteKind::Channel {
            channel_tracks[leaf_of[p.s]] += 1;
        }
    }

    // ── 5. Card sizes (uniform width per leaf column) and paddings ───────────
    let mut card_w = vec![CARD_MIN_W; nleaves];
    for (i, node) in ir.nodes.iter().enumerate() {
        let evidence = if node.evidence.is_some() { EVIDENCE_GLYPH_W } else { 0.0 };
        let mut want = sans_width(&node.label, LABEL_SIZE, true) + 2.0 * CARD_PAD_X + evidence;
        if let Some(s) = &node.subtitle {
            want = want.max(sans_width(s, SUBTITLE_SIZE, false) + 2.0 * CARD_PAD_X);
        }
        if let Some(t) = &node.tech_stack {
            want = want.max(mono_width(t, TECH_SIZE) + 16.0 + 2.0 * CARD_PAD_X);
        }
        for a in node.attributes.iter().flatten().take(MAX_ROWS) {
            let row = sans_width(&a.name, ROW_SIZE, a.key.is_some())
                + mono_width(&a.type_name, ROW_TYPE_SIZE)
                + if a.nullable { 7.0 } else { 0.0 };
            want = want.max(row + KEY_W + 24.0 + 2.0 * CARD_PAD_X);
        }
        card_w[leaf_of[i]] = card_w[leaf_of[i]].max(want.min(CARD_MAX_W));
    }
    let mut left_pad = vec![0.0f64; nleaves];
    let mut right_pad = vec![0.0f64; nleaves];
    for block in blocks.iter().filter(|b| b.container.is_some()) {
        left_pad[block.leaf] = PAD_X;
        right_pad[block.leaf + block.stacks.len() - 1] = PAD_X;
    }
    for l in 0..nleaves {
        if channel_tracks[l] > 0 {
            right_pad[l] = right_pad[l].max(PAD_X) + 12.0 + channel_tracks[l] as f64 * TRACK;
        }
    }
    let mut leaf_w: Vec<f64> = (0..nleaves).map(|l| left_pad[l] + card_w[l] + right_pad[l]).collect();
    for block in blocks.iter().filter(|b| b.container.is_some() && b.stacks.len() == 1) {
        let label = mono_width(&ir.containers[block.container.unwrap()].label.to_uppercase(), 10.5) * 1.1 + 2.0 * PAD_X;
        leaf_w[block.leaf] = leaf_w[block.leaf].max(label);
    }

    let mut boxes: Vec<NodeBox> = ir
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let w = card_w[leaf_of[i]];
            let evidence = if node.evidence.is_some() { EVIDENCE_GLYPH_W } else { 0.0 };
            let label = fit(&node.label, w - 2.0 * CARD_PAD_X - evidence, |s| sans_width(s, LABEL_SIZE, true));
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
            let rows = attr_rows(node, w);
            let mut h = card_content_h(subtitle.is_some(), tech.is_some(), rows.len());
            let side_ports = [Side::Left, Side::Right]
                .iter()
                .map(|s| port_count.get(&(i, *s)).copied().unwrap_or(0))
                .max()
                .unwrap_or(0);
            h = h.max((side_ports as f64 + 1.0) * PORT_SPACING);
            let tech_w = tech.as_ref().map(|t| mono_width(t, TECH_SIZE) + 16.0).unwrap_or(0.0);
            NodeBox {
                id: node.id.clone(),
                index: i,
                rect: Rect { x: 0.0, y: 0.0, w, h },
                label,
                subtitle,
                tech,
                tech_w,
                rows,
            }
        })
        .collect();

    // ── 6. Vertical placement ────────────────────────────────────────────────
    let labelled_vertical: BTreeSet<(usize, usize)> = plans
        .iter()
        .filter(|p| {
            p.kind == RouteKind::VerticalAdjacent && edge_label_text(ir, &ir.edges[p.edge], p.edge + 1).is_some()
        })
        .map(|p| (p.s, p.t))
        .collect();
    for block in blocks.iter_mut() {
        let contained = block.container.is_some();
        let heights: Vec<f64> = block
            .stacks
            .iter()
            .map(|stack| {
                let mut y = 0.0;
                for (k, &ni) in stack.iter().enumerate() {
                    if k > 0 {
                        y += if labelled_vertical.contains(&(stack[k - 1], ni)) { NODE_GAP_LABELLED } else { NODE_GAP };
                    }
                    boxes[ni].rect.y = y;
                    y += boxes[ni].rect.h;
                }
                y
            })
            .collect();
        let inner = heights.iter().copied().fold(0.0, f64::max);
        let top = if contained { PAD_TOP } else { 0.0 };
        for (stack, h) in block.stacks.iter().zip(&heights) {
            let offset = top + (inner - h) / 2.0;
            for &ni in stack {
                boxes[ni].rect.y += offset;
            }
        }
        block.rect.h = top + inner + if contained { PAD_BOTTOM } else { 0.0 };
    }
    let col_h: Vec<f64> = (0..ncols)
        .map(|c| {
            columns[c].iter().map(|&b| blocks[b].rect.h).sum::<f64>()
                + BLOCK_GAP * columns[c].len().saturating_sub(1) as f64
        })
        .collect();
    let max_h = col_h.iter().copied().fold(0.0, f64::max);
    for c in 0..ncols {
        let mut y = (max_h - col_h[c]) / 2.0;
        for &b in &columns[c] {
            blocks[b].rect.y = y;
            for &ni in &blocks[b].nodes {
                boxes[ni].rect.y += y;
            }
            y += blocks[b].rect.h + BLOCK_GAP;
        }
    }

    // ── 7. Side-port y coordinates ───────────────────────────────────────────
    let mut side_lists: HashMap<(usize, Side), Vec<(usize, bool)>> = HashMap::new();
    for (pi, p) in plans.iter().enumerate() {
        side_lists.entry((p.s, p.s_side)).or_default().push((pi, true));
        side_lists.entry((p.t, p.t_side)).or_default().push((pi, false));
    }
    let mut port_y: HashMap<(usize, bool), f64> = HashMap::new();
    let mut side_keys: Vec<_> = side_lists.keys().copied().collect();
    side_keys.sort();
    for key in &side_keys {
        let (node, side) = *key;
        if !matches!(side, Side::Left | Side::Right) {
            continue;
        }
        let list = side_lists.get_mut(key).unwrap();
        list.sort_by(|a, b| {
            let other = |&(pi, is_src): &(usize, bool)| {
                let p = &plans[pi];
                boxes[if is_src { p.t } else { p.s }].rect.cy()
            };
            other(a).total_cmp(&other(b)).then_with(|| ir.edges[plans[a.0].edge].id.cmp(&ir.edges[plans[b.0].edge].id))
        });
        let r = boxes[node].rect;
        let count = list.len() as f64;
        for (k, &(pi, is_src)) in list.iter().enumerate() {
            port_y.insert((pi, is_src), r.y + r.h * (k as f64 + 1.0) / (count + 1.0));
        }
    }

    // ── 8. Lanes across intermediate leaf columns ────────────────────────────
    let mut lane_order: Vec<usize> = plans
        .iter()
        .enumerate()
        .filter(|(_, p)| matches!(p.kind, RouteKind::ForwardLane | RouteKind::BackwardLane))
        .map(|(i, _)| i)
        .collect();
    lane_order.sort_by_key(|&pi| {
        let p = &plans[pi];
        (std::cmp::Reverse(leaf_of[p.s].abs_diff(leaf_of[p.t])), ir.edges[p.edge].id.clone())
    });
    let mut lanes: Vec<(usize, usize, f64)> = Vec::new();
    for pi in lane_order {
        let p = &plans[pi];
        let (ls, lt) = (leaf_of[p.s], leaf_of[p.t]);
        let (lo, hi) = (ls.min(lt), ls.max(lt));
        let own = [block_of[p.s], block_of[p.t]];
        let mut allowed: Intervals = vec![(f64::NEG_INFINITY, f64::INFINITY)];
        for leaf in lo + 1..hi {
            let mut obstacles: Vec<(f64, f64)> = Vec::new();
            for (bi, b) in blocks.iter().enumerate() {
                if leaf < b.leaf || leaf >= b.leaf + b.stacks.len() {
                    continue;
                }
                if b.container.is_some() && own.contains(&bi) {
                    // Inside its own container a lane may run between cards, but not through the header.
                    obstacles.push((b.rect.y, b.rect.y + PAD_TOP - 8.0));
                    for &ni in b.stacks[leaf - b.leaf].iter() {
                        obstacles.push((boxes[ni].rect.y, boxes[ni].rect.bottom()));
                    }
                } else {
                    obstacles.push((b.rect.y, b.rect.bottom()));
                }
            }
            allowed = intersect(&allowed, &complement(obstacles));
        }
        let mid = (port_y[&(pi, true)] + port_y[&(pi, false)]) / 2.0;
        let y = pick_lane(&allowed, mid, &lanes, (lo, hi));
        lanes.push((lo, hi, y));
        plans[pi].lane_y = Some(y);
    }

    // ── 9. Gutter tracks and widths ──────────────────────────────────────────
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum Flow {
        UTurn,
        Forward,
        Backward,
    }
    struct Use {
        plan: usize,
        leg: u8,
        y1: f64,
        y2: f64,
        flow: Flow,
    }
    let mut gutter_uses: Vec<Vec<Use>> = (0..nleaves).map(|_| vec![]).collect();
    let mut left_need = vec![28.0f64; nleaves];
    let mut right_need = vec![24.0f64; nleaves];
    let mut straight: BTreeSet<usize> = BTreeSet::new();
    let label_w = |ei: usize| -> f64 {
        edge_label_text(ir, &ir.edges[ei], ei + 1).map(|l| edge_label_width(&l) + 20.0).unwrap_or(0.0)
    };
    for (pi, p) in plans.iter().enumerate() {
        let (ls, lt) = (leaf_of[p.s], leaf_of[p.t]);
        let (sy, ty) = (port_y.get(&(pi, true)).copied(), port_y.get(&(pi, false)).copied());
        let lw = label_w(p.edge);
        match p.kind {
            RouteKind::Message | RouteKind::SelfMessage => unreachable!("sequence layout routes messages"),
            RouteKind::ForwardAdjacent => {
                let (sy, ty) = (sy.unwrap(), ty.unwrap());
                if (sy - ty).abs() < 0.5 {
                    straight.insert(pi);
                    left_need[ls] = left_need[ls].max(lw + 20.0);
                } else {
                    gutter_uses[ls].push(Use { plan: pi, leg: 0, y1: sy, y2: ty, flow: Flow::Forward });
                    left_need[ls] = left_need[ls].max(lw);
                }
            }
            RouteKind::BackwardAdjacent => {
                let (sy, ty) = (sy.unwrap(), ty.unwrap());
                if (sy - ty).abs() < 0.5 {
                    straight.insert(pi);
                    right_need[lt] = right_need[lt].max(lw + 20.0);
                } else {
                    gutter_uses[lt].push(Use { plan: pi, leg: 0, y1: sy, y2: ty, flow: Flow::Backward });
                    right_need[lt] = right_need[lt].max(lw);
                }
            }
            RouteKind::SameColumn => {
                gutter_uses[ls].push(Use { plan: pi, leg: 0, y1: sy.unwrap(), y2: ty.unwrap(), flow: Flow::UTurn });
                left_need[ls] = left_need[ls].max(lw);
            }
            RouteKind::ForwardLane => {
                let ly = p.lane_y.unwrap();
                gutter_uses[ls].push(Use { plan: pi, leg: 0, y1: sy.unwrap(), y2: ly, flow: Flow::Forward });
                gutter_uses[lt - 1].push(Use { plan: pi, leg: 1, y1: ly, y2: ty.unwrap(), flow: Flow::Forward });
            }
            RouteKind::BackwardLane => {
                let ly = p.lane_y.unwrap();
                gutter_uses[ls - 1].push(Use { plan: pi, leg: 0, y1: sy.unwrap(), y2: ly, flow: Flow::Backward });
                gutter_uses[lt].push(Use { plan: pi, leg: 1, y1: ly, y2: ty.unwrap(), flow: Flow::Backward });
            }
            RouteKind::VerticalAdjacent | RouteKind::Channel => {}
        }
    }
    let mut track_offset: HashMap<(usize, u8), f64> = HashMap::new();
    let mut gutter_w = vec![0.0f64; nleaves];
    for g in 0..nleaves {
        let uses = &mut gutter_uses[g];
        uses.sort_by(|a, b| {
            let key = |u: &Use| -> (Flow, i8, f64, f64) {
                let down = u.y2 > u.y1;
                match u.flow {
                    Flow::UTurn => (Flow::UTurn, 0, (u.y2 - u.y1).abs(), u.y1),
                    Flow::Forward => (Flow::Forward, if down { 0 } else { 1 }, if down { -u.y1 } else { u.y1 }, u.y2),
                    Flow::Backward => (Flow::Backward, if down { 1 } else { 0 }, if down { u.y1 } else { -u.y1 }, u.y2),
                }
            };
            let (ka, kb) = (key(a), key(b));
            ka.0.cmp(&kb.0)
                .then(ka.1.cmp(&kb.1))
                .then(ka.2.total_cmp(&kb.2))
                .then(ka.3.total_cmp(&kb.3))
                .then(a.plan.cmp(&b.plan))
        });
        // A lane is only busy for the span it actually turns through, so two
        // connectors whose vertical runs do not overlap can share one. Giving
        // every edge its own lane put nine parallel lines 12px apart through
        // one gutter of the demo book, which is a bundle, not a diagram.
        let mut track_of: Vec<usize> = Vec::with_capacity(uses.len());
        let mut occupied: Vec<Vec<(f64, f64)>> = Vec::new();
        for u in uses.iter() {
            let (lo, hi) = (u.y1.min(u.y2) - TRACK_CLEAR, u.y1.max(u.y2) + TRACK_CLEAR);
            let free = occupied.iter().position(|spans: &Vec<(f64, f64)>| spans.iter().all(|&(a, b)| hi < a || lo > b));
            let k = free.unwrap_or_else(|| {
                occupied.push(Vec::new());
                occupied.len() - 1
            });
            occupied[k].push((lo, hi));
            track_of.push(k);
        }
        let tracks = occupied.len();
        // Sharing leaves fewer lanes, so they can be spaced wide enough to
        // follow without making the gutter wider than it was.
        let pitch = if tracks <= 8 { TRACK_WIDE } else { TRACK };
        let band = if tracks > 0 { 12.0 + tracks as f64 * pitch } else { 0.0 };
        let used = tracks > 0 || straight.iter().any(|&pi| leaf_of[plans[pi].s].min(leaf_of[plans[pi].t]) == g);
        let is_last = g + 1 == nleaves;
        gutter_w[g] = match (is_last, used) {
            (true, false) => 0.0,
            (true, true) => left_need[g] + band + 12.0,
            _ => (left_need[g] + band + right_need[g]).max(72.0),
        };
        for (i, u) in uses.iter().enumerate() {
            let k = track_of[i];
            track_offset.insert((u.plan, u.leg), left_need[g] + 12.0 + k as f64 * pitch - pitch / 2.0);
        }
    }

    // ── 10. Horizontal placement ─────────────────────────────────────────────
    let mut leaf_left = vec![0.0f64; nleaves];
    for l in 1..nleaves {
        leaf_left[l] = leaf_left[l - 1] + leaf_w[l - 1] + gutter_w[l - 1];
    }
    let leaf_right: Vec<f64> = (0..nleaves).map(|l| leaf_left[l] + leaf_w[l]).collect();
    for (i, b) in boxes.iter_mut().enumerate() {
        let l = leaf_of[i];
        b.rect.x = leaf_left[l] + left_pad[l] + (card_w[l] - b.rect.w) / 2.0;
    }
    for block in blocks.iter_mut() {
        let last = block.leaf + block.stacks.len() - 1;
        block.rect.x = leaf_left[block.leaf];
        block.rect.w = leaf_right[last] - leaf_left[block.leaf];
    }

    // ── 11. Routes ───────────────────────────────────────────────────────────
    let mut bottom_top_lists: HashMap<(usize, Side), Vec<usize>> = HashMap::new();
    for (pi, p) in plans.iter().enumerate() {
        if p.kind == RouteKind::VerticalAdjacent {
            bottom_top_lists.entry((p.s, Side::Bottom)).or_default().push(pi);
            bottom_top_lists.entry((p.t, Side::Top)).or_default().push(pi);
        }
    }
    let port_x = |pi: usize, node: usize, side: Side| -> f64 {
        let list = &bottom_top_lists[&(node, side)];
        let k = list.iter().position(|&x| x == pi).unwrap() as f64;
        let r = boxes[node].rect;
        r.x + r.w * (k + 1.0) / (list.len() as f64 + 1.0)
    };
    let mut channel_next = vec![0usize; nleaves];
    let mut routes: Vec<EdgeRoute> = Vec::with_capacity(plans.len());
    for (pi, p) in plans.iter().enumerate() {
        let e = &ir.edges[p.edge];
        let (sr, tr) = (boxes[p.s].rect, boxes[p.t].rect);
        let (ls, lt) = (leaf_of[p.s], leaf_of[p.t]);
        let gx = |g: usize, leg: u8| leaf_right[g] + track_offset[&(pi, leg)];
        let ports = || (port_y[&(pi, true)], port_y[&(pi, false)]);
        let points: Vec<(f64, f64)> = match p.kind {
            RouteKind::Message | RouteKind::SelfMessage => unreachable!("sequence layout routes messages"),
            RouteKind::VerticalAdjacent => {
                let (x1, x2) = (port_x(pi, p.s, Side::Bottom), port_x(pi, p.t, Side::Top));
                if (x1 - x2).abs() < 0.5 {
                    vec![(x1, sr.bottom()), (x2, tr.y)]
                } else {
                    let my = (sr.bottom() + tr.y) / 2.0;
                    vec![(x1, sr.bottom()), (x1, my), (x2, my), (x2, tr.y)]
                }
            }
            RouteKind::Channel => {
                let x = sr.right() + 12.0 + channel_next[ls] as f64 * TRACK;
                channel_next[ls] += 1;
                let (sy, ty) = ports();
                vec![(sr.right(), sy), (x, sy), (x, ty), (tr.right(), ty)]
            }
            RouteKind::ForwardAdjacent => {
                let (sy, ty) = ports();
                if straight.contains(&pi) {
                    vec![(sr.right(), sy), (tr.x, sy)]
                } else {
                    let x = gx(ls, 0);
                    vec![(sr.right(), sy), (x, sy), (x, ty), (tr.x, ty)]
                }
            }
            RouteKind::BackwardAdjacent => {
                let (sy, ty) = ports();
                if straight.contains(&pi) {
                    vec![(sr.x, sy), (tr.right(), sy)]
                } else {
                    let x = gx(lt, 0);
                    vec![(sr.x, sy), (x, sy), (x, ty), (tr.right(), ty)]
                }
            }
            RouteKind::SameColumn => {
                let (sy, ty) = ports();
                let x = gx(ls, 0);
                vec![(sr.right(), sy), (x, sy), (x, ty), (tr.right(), ty)]
            }
            RouteKind::ForwardLane => {
                let ((sy, ty), ly) = (ports(), p.lane_y.unwrap());
                let (x1, x2) = (gx(ls, 0), gx(lt - 1, 1));
                vec![(sr.right(), sy), (x1, sy), (x1, ly), (x2, ly), (x2, ty), (tr.x, ty)]
            }
            RouteKind::BackwardLane => {
                let ((sy, ty), ly) = (ports(), p.lane_y.unwrap());
                let (x1, x2) = (gx(ls - 1, 0), gx(lt, 1));
                vec![(sr.x, sy), (x1, sy), (x1, ly), (x2, ly), (x2, ty), (tr.right(), ty)]
            }
        };
        routes.push(EdgeRoute {
            id: e.id.clone(),
            index: p.edge,
            source: e.source.clone(),
            target: e.target.clone(),
            kind: p.kind,
            points: simplify(points),
            label: None,
            primary: e.primary(),
            edge_type: e.edge_type,
            dashed: e.resolved_style() == EdgeStyle::Dashed,
            hops: vec![],
        });
    }

    let containers: Vec<ContainerBox> = blocks
        .iter()
        .filter_map(|b| {
            let c = b.container?;
            Some(ContainerBox {
                id: ir.containers[c].id.clone(),
                index: c,
                rect: b.rect,
                label: ir.containers[c].label.clone(),
            })
        })
        .collect();

    // ── 12. Labels ───────────────────────────────────────────────────────────
    place_labels(ir, &mut routes, &boxes, &containers);
    add_hops(&mut routes);

    // ── 13. Normalise into the page frame ────────────────────────────────────
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut grow = |r: &Rect| {
        min_x = min_x.min(r.x);
        min_y = min_y.min(r.y);
        max_x = max_x.max(r.right());
        max_y = max_y.max(r.bottom());
    };
    boxes.iter().for_each(|b| grow(&b.rect));
    containers.iter().for_each(|c| grow(&c.rect));
    for r in &routes {
        for &(x, y) in &r.points {
            grow(&Rect { x: x - 6.0, y: y - 6.0, w: 12.0, h: 12.0 });
        }
        if let Some(l) = &r.label {
            grow(&l.rect);
        }
    }
    if !min_x.is_finite() {
        (min_x, min_y, max_x, max_y) = (0.0, 0.0, 0.0, 0.0);
    }
    let body_w = max_x - min_x;
    let body_h = max_y - min_y;
    let width = (body_w + 2.0 * MARGIN).max(title.min_width).max(720.0).ceil();
    let body_top = MARGIN + title.header_h;
    let dx = (width - body_w) / 2.0 - min_x;
    let dy = body_top - min_y;
    let mut containers = containers;
    for b in &mut boxes {
        b.rect.translate(dx, dy);
    }
    for c in &mut containers {
        c.rect.translate(dx, dy);
    }
    for r in &mut routes {
        for p in &mut r.points {
            p.0 += dx;
            p.1 += dy;
        }
        for h in &mut r.hops {
            h.0 += dx;
            h.1 += dy;
        }
        if let Some(l) = &mut r.label {
            l.rect.translate(dx, dy);
        }
    }
    let legend_top = body_top + body_h + 40.0;
    let height = (legend_top + title.legend_h + MARGIN).ceil();
    Layout {
        width,
        height,
        body_top,
        legend_top,
        nodes: boxes,
        containers,
        edges: routes,
        skipped_edges: skipped,
        lifelines: vec![],
    }
}

fn zero() -> Rect {
    Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }
}

pub fn edge_label_width(text: &str) -> f64 {
    sans_width(text, EDGE_LABEL_SIZE, false) + 14.0
}

/// DFS in index order; edges to a node on the current stack are dropped.
fn break_cycles(succ: &[BTreeSet<usize>]) -> Vec<BTreeSet<usize>> {
    let n = succ.len();
    let mut state = vec![0u8; n];
    let mut dag: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
    for root in 0..n {
        if state[root] != 0 {
            continue;
        }
        let mut stack: Vec<(usize, Vec<usize>)> = vec![(root, succ[root].iter().copied().collect())];
        state[root] = 1;
        while let Some((v, rest)) = stack.last_mut() {
            let v = *v;
            if let Some(w) = rest.pop() {
                match state[w] {
                    0 => {
                        dag[v].insert(w);
                        state[w] = 1;
                        let next: Vec<usize> = succ[w].iter().copied().collect();
                        stack.push((w, next));
                    }
                    1 => {} // back edge
                    _ => {
                        dag[v].insert(w);
                    }
                }
            } else {
                state[v] = 2;
                stack.pop();
            }
        }
    }
    dag
}

fn longest_path_ranks(dag: &[BTreeSet<usize>]) -> Vec<usize> {
    let n = dag.len();
    let mut indeg = vec![0usize; n];
    for s in dag {
        for &t in s {
            indeg[t] += 1;
        }
    }
    let mut rank = vec![0usize; n];
    let mut queue: std::collections::VecDeque<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    while let Some(v) = queue.pop_front() {
        for &w in &dag[v] {
            rank[w] = rank[w].max(rank[v] + 1);
            indeg[w] -= 1;
            if indeg[w] == 0 {
                queue.push_back(w);
            }
        }
    }
    rank
}

/// Places a block's nodes into sub-columns. Small or sparsely connected
/// containers stay a single stack in depth-first preorder (a callee lands
/// directly under its caller). Containers with several internal calls spread
/// by internal rank so their connectors route through gutters instead of
/// detouring past unrelated cards.
fn arrange_block(block: &mut Block, edges: &[(usize, usize, usize)]) {
    let k = block.nodes.len();
    if k < 2 {
        block.stacks = vec![block.nodes.clone()];
        return;
    }
    let local: HashMap<usize, usize> = block.nodes.iter().enumerate().map(|(i, &n)| (n, i)).collect();
    let mut succ: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); k];
    let mut has_pred = vec![false; k];
    let mut intra = 0;
    for &(_, s, t) in edges {
        if let (Some(&a), Some(&b)) = (local.get(&s), local.get(&t)) {
            if succ[a].insert(b) {
                has_pred[b] = true;
                intra += 1;
            }
        }
    }
    let rank = longest_path_ranks(&break_cycles(&succ));
    let depth = rank.iter().copied().max().unwrap_or(0);
    let spread = block.container.is_some() && k >= 3 && intra >= 3 && depth >= 1;
    if !spread {
        let mut roots: Vec<usize> = (0..k).filter(|&i| !has_pred[i]).collect();
        roots.extend((0..k).filter(|&i| has_pred[i]));
        let mut seen = vec![false; k];
        let mut order = Vec::with_capacity(k);
        for root in roots {
            let mut stack = vec![root];
            while let Some(v) = stack.pop() {
                if std::mem::replace(&mut seen[v], true) {
                    continue;
                }
                order.push(v);
                let mut next: Vec<usize> = succ[v].iter().copied().filter(|w| !seen[*w]).collect();
                next.sort_by_key(|&w| std::cmp::Reverse((rank[w], w)));
                stack.extend(next);
            }
        }
        block.stacks = vec![order.into_iter().map(|i| block.nodes[i]).collect()];
        return;
    }
    let cols = depth.min(3) + 1;
    let mut stacks: Vec<Vec<usize>> = vec![vec![]; cols];
    for i in 0..k {
        stacks[rank[i].min(cols - 1)].push(i);
    }
    // Order each stack by the mean position of its predecessors.
    let mut position = vec![0.0f64; k];
    for stack in stacks.iter_mut() {
        let keys: Vec<(f64, usize)> = stack
            .iter()
            .map(|&v| {
                let preds: Vec<f64> =
                    (0..k).filter(|&u| succ[u].contains(&v) && rank[u] < rank[v]).map(|u| position[u]).collect();
                let key = if preds.is_empty() { v as f64 } else { preds.iter().sum::<f64>() / preds.len() as f64 };
                (key, v)
            })
            .collect();
        let mut sorted = keys;
        sorted.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        *stack = sorted.iter().map(|&(_, v)| v).collect();
        for (i, &v) in stack.iter().enumerate() {
            position[v] = i as f64;
        }
    }
    block.stacks = stacks.into_iter().map(|s| s.into_iter().map(|i| block.nodes[i]).collect()).collect();
}

type Intervals = Vec<(f64, f64)>;

/// Barycentric sweeps so connected blocks sit at similar heights.
fn order_columns(columns: &mut [Vec<usize>], succ: &[BTreeSet<usize>]) {
    let nb = succ.len();
    let mut neighbours: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); nb];
    for (s, ts) in succ.iter().enumerate() {
        for &t in ts {
            neighbours[s].insert(t);
            neighbours[t].insert(s);
        }
    }
    for sweep in 0..4 {
        let mut pos = vec![0.0f64; nb];
        for col in columns.iter() {
            for (i, &b) in col.iter().enumerate() {
                pos[b] = (i as f64 + 0.5) / col.len() as f64;
            }
        }
        let cols: Vec<usize> =
            if sweep % 2 == 0 { (0..columns.len()).collect() } else { (0..columns.len()).rev().collect() };
        for c in cols {
            let col = &mut columns[c];
            let keys: HashMap<usize, f64> = col
                .iter()
                .map(|&b| {
                    let ns: Vec<f64> = neighbours[b].iter().filter(|n| !col.contains(n)).map(|&n| pos[n]).collect();
                    let k = if ns.is_empty() { pos[b] } else { ns.iter().sum::<f64>() / ns.len() as f64 };
                    (b, k)
                })
                .collect();
            col.sort_by(|a, b| keys[a].total_cmp(&keys[b]).then(a.cmp(b)));
            for (i, &b) in col.iter().enumerate() {
                pos[b] = (i as f64 + 0.5) / col.len() as f64;
            }
        }
    }
}

/// Free vertical intervals given occupied spans (each padded by `LANE_CLEAR`).
fn complement(mut spans: Vec<(f64, f64)>) -> Intervals {
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = Vec::new();
    let mut cursor = f64::NEG_INFINITY;
    for (top, bottom) in spans {
        if top - LANE_CLEAR > cursor {
            out.push((cursor, top - LANE_CLEAR));
        }
        cursor = cursor.max(bottom + LANE_CLEAR);
    }
    out.push((cursor, f64::INFINITY));
    out
}

fn intersect(a: &Intervals, b: &Intervals) -> Intervals {
    let mut out = Vec::new();
    for &(a0, a1) in a {
        for &(b0, b1) in b {
            let (lo, hi) = (a0.max(b0), a1.min(b1));
            if hi >= lo {
                out.push((lo, hi));
            }
        }
    }
    out
}

fn pick_lane(allowed: &Intervals, mid: f64, taken: &[(usize, usize, f64)], span: (usize, usize)) -> f64 {
    let clash = |y: f64| taken.iter().any(|&(lo, hi, ty)| lo <= span.1 && span.0 <= hi && (ty - y).abs() < TRACK);
    let mut best: Option<f64> = None;
    for &(lo, hi) in allowed {
        let centre = mid.clamp(lo, hi);
        let candidate = (0..60)
            .flat_map(|k| [centre + k as f64 * TRACK, centre - k as f64 * TRACK])
            .find(|&y| y >= lo && y <= hi && !clash(y));
        if let Some(y) = candidate {
            if best.is_none_or(|b| (y - mid).abs() < (b - mid).abs()) {
                best = Some(y);
            }
        }
    }
    best.unwrap_or(mid)
}

fn simplify(points: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(points.len());
    for p in points {
        if out.last().is_some_and(|l: &(f64, f64)| (l.0 - p.0).abs() < 0.01 && (l.1 - p.1).abs() < 0.01) {
            continue;
        }
        if out.len() >= 2 {
            let a = out[out.len() - 2];
            let b = out[out.len() - 1];
            let collinear = ((a.0 - b.0).abs() < 0.01 && (b.0 - p.0).abs() < 0.01)
                || ((a.1 - b.1).abs() < 0.01 && (b.1 - p.1).abs() < 0.01);
            if collinear {
                out.pop();
            }
        }
        out.push(p);
    }
    out
}

pub fn segment_rect(a: (f64, f64), b: (f64, f64), pad: f64) -> Rect {
    Rect {
        x: a.0.min(b.0) - pad,
        y: a.1.min(b.1) - pad,
        w: (a.0 - b.0).abs() + 2.0 * pad,
        h: (a.1 - b.1).abs() + 2.0 * pad,
    }
}

pub fn container_chip(c: &ContainerBox) -> Rect {
    let w = mono_width(&c.label.to_uppercase(), 10.5) * 1.1 + 24.0;
    Rect { x: c.rect.x + 8.0, y: c.rect.y + 8.0, w, h: 24.0 }
}

fn place_labels(ir: &DiagramIR, routes: &mut [EdgeRoute], boxes: &[NodeBox], containers: &[ContainerBox]) {
    let mut obstacles: Vec<Rect> = boxes.iter().map(|b| b.rect.inflate(4.0)).collect();
    obstacles.extend(containers.iter().map(container_chip));
    let segments: Vec<(usize, Rect)> = routes
        .iter()
        .enumerate()
        .flat_map(|(ri, r)| r.points.windows(2).map(move |w| (ri, segment_rect(w[0], w[1], 2.0))))
        .collect();
    let mut order: Vec<usize> = (0..routes.len()).collect();
    order.sort_by_key(|&i| (!routes[i].primary, routes[i].id.clone()));
    let mut placed: Vec<Rect> = Vec::new();
    for ri in order {
        let Some(text) = edge_label_text(ir, &ir.edges[routes[ri].index], routes[ri].index + 1) else {
            continue;
        };
        let w = edge_label_width(&text);
        let h = EDGE_LABEL_H;
        let pts = &routes[ri].points;
        let nseg = pts.len() - 1;
        let prefer: Vec<usize> = match routes[ri].kind {
            RouteKind::ForwardLane | RouteKind::BackwardLane if nseg >= 5 => vec![2, 0, nseg - 1, 1, 3],
            RouteKind::VerticalAdjacent | RouteKind::Channel => (0..nseg).rev().collect(),
            _ => {
                let mut v = vec![0, nseg - 1];
                v.extend(1..nseg.saturating_sub(1));
                v
            }
        };
        let mut seen = BTreeSet::new();
        let mut candidates: Vec<Rect> = Vec::new();
        for si in prefer.into_iter().filter(|s| *s < nseg && seen.insert(*s)) {
            let (a, b) = (pts[si], pts[si + 1]);
            let is_last = si == nseg - 1;
            let arrow = if is_last { 12.0 } else { 0.0 };
            if (a.1 - b.1).abs() < 0.01 {
                let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
                let len = x1 - x0 - arrow - 8.0;
                if len < w {
                    continue;
                }
                let start = if b.0 < a.0 && is_last { x0 + arrow + 4.0 } else { x0 + 4.0 };
                let xs = [start + (len - w) / 2.0, start, start + len - w];
                for x in xs {
                    candidates.push(Rect { x, y: a.1 - LABEL_GAP - h, w, h });
                    candidates.push(Rect { x, y: a.1 + LABEL_GAP, w, h });
                }
            } else {
                let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
                let len = y1 - y0 - arrow - 8.0;
                if len < h {
                    continue;
                }
                let y = y0 + 4.0 + (if b.1 < a.1 && is_last { arrow } else { 0.0 }) + (len - h) / 2.0;
                candidates.push(Rect { x: a.0 + LABEL_GAP + 2.0, y, w, h });
                candidates.push(Rect { x: a.0 - LABEL_GAP - 2.0 - w, y, w, h });
            }
        }
        // Wider search when the preferred spots are crowded: step away from
        // each segment's midpoint in both directions.
        let mut extra = Vec::new();
        for w2 in pts.windows(2) {
            let (a, b) = (w2[0], w2[1]);
            let (mx, my) = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
            for k in 1..=6 {
                let d = LABEL_GAP + k as f64 * 16.0;
                extra.push(Rect { x: mx - w / 2.0, y: my - d - h, w, h });
                extra.push(Rect { x: mx - w / 2.0, y: my + d, w, h });
                extra.push(Rect { x: mx + d, y: my - h / 2.0, w, h });
                extra.push(Rect { x: mx - d - w, y: my - h / 2.0, w, h });
            }
        }
        candidates.extend(extra);
        let own: Vec<Rect> = pts.windows(2).map(|w2| segment_rect(w2[0], w2[1], LABEL_GAP)).collect();
        let cost = |r: &Rect| -> f64 {
            let mut c = 0.0;
            for o in obstacles.iter().chain(placed.iter()) {
                c += r.overlap_area(o) * 10.0;
            }
            for o in &own {
                c += r.overlap_area(o) * 50.0;
            }
            for (owner, s) in &segments {
                if *owner != ri {
                    c += r.overlap_area(s);
                }
            }
            c
        };
        let best = candidates
            .iter()
            .enumerate()
            .map(|(i, r)| (cost(r), i, *r))
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        if let Some((_, _, rect)) = best {
            placed.push(rect);
            routes[ri].label = Some(LabelBox { rect, text, detail: None });
        }
    }
}

/// Where a less important horizontal segment crosses another edge's vertical
/// segment, it hops over it (primary edges never hop).
fn add_hops(routes: &mut [EdgeRoute]) {
    let verticals: Vec<(usize, f64, f64, f64)> = routes
        .iter()
        .enumerate()
        .flat_map(|(ri, r)| {
            r.points
                .windows(2)
                .filter(|w| (w[0].0 - w[1].0).abs() < 0.01)
                .map(move |w| (ri, w[0].0, w[0].1.min(w[1].1), w[0].1.max(w[1].1)))
        })
        .collect();
    for ri in 0..routes.len() {
        let mut hops = Vec::new();
        {
            let r = &routes[ri];
            for w in r.points.windows(2).filter(|w| (w[0].1 - w[1].1).abs() < 0.01) {
                let (y, x0, x1) = (w[0].1, w[0].0.min(w[1].0), w[0].0.max(w[1].0));
                for &(vi, vx, vy0, vy1) in &verticals {
                    if vi == ri || vx <= x0 + 10.0 || vx >= x1 - 10.0 || y <= vy0 + 1.0 || y >= vy1 - 1.0 {
                        continue;
                    }
                    let other = &routes[vi];
                    let yields = !r.primary && (other.primary || ri > vi);
                    if yields {
                        hops.push((vx, y));
                    }
                }
            }
        }
        hops.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        routes[ri].hops = hops;
    }
}

/// Geometric contract checks used by tests and `nunki render --check`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GeometryIssue {
    pub kind: &'static str,
    pub detail: String,
}

impl Layout {
    pub fn check_geometry(&self) -> Vec<GeometryIssue> {
        let mut issues = Vec::new();
        let rect_of: HashMap<&str, Rect> = self.nodes.iter().map(|n| (n.id.as_str(), n.rect)).collect();
        for e in &self.edges {
            for w in e.points.windows(2) {
                let (a, b) = (w[0], w[1]);
                if (a.0 - b.0).abs() > 0.01 && (a.1 - b.1).abs() > 0.01 {
                    issues.push(GeometryIssue {
                        kind: "diagonal-segment",
                        detail: format!("{} has a diagonal segment", e.id),
                    });
                }
                let seg = segment_rect(a, b, 0.0);
                for n in &self.nodes {
                    if n.id == e.source || n.id == e.target {
                        continue;
                    }
                    let inner = n.rect.inflate(-1.0);
                    if seg_hits(&seg, &inner) {
                        issues.push(GeometryIssue {
                            kind: "passes-behind-node",
                            detail: format!("{} crosses {}", e.id, n.id),
                        });
                    }
                }
            }
            for (end, node) in [(e.points[0], &e.source), (*e.points.last().unwrap(), &e.target)] {
                let on_lifeline = self.lifelines.iter().any(|l| {
                    l.id == *node && (end.0 - l.x).abs() < 0.5 && end.1 >= l.top - 0.5 && end.1 <= l.bottom + 0.5
                });
                if on_lifeline {
                    continue;
                }
                let r = rect_of[node.as_str()];
                let on_border = ((end.0 - r.x).abs() < 0.5 || (end.0 - r.right()).abs() < 0.5)
                    && end.1 >= r.y - 0.5
                    && end.1 <= r.bottom() + 0.5
                    || ((end.1 - r.y).abs() < 0.5 || (end.1 - r.bottom()).abs() < 0.5)
                        && end.0 >= r.x - 0.5
                        && end.0 <= r.right() + 0.5;
                if !on_border {
                    issues.push(GeometryIssue {
                        kind: "detached-endpoint",
                        detail: format!("{} does not attach to {}", e.id, node),
                    });
                }
            }
            if let Some(l) = &e.label {
                for n in &self.nodes {
                    if l.rect.intersects(&n.rect) {
                        issues.push(GeometryIssue {
                            kind: "label-over-node",
                            detail: format!("label of {} overlaps {}", e.id, n.id),
                        });
                    }
                }
                for w in e.points.windows(2) {
                    if l.rect.intersects(&segment_rect(w[0], w[1], LABEL_GAP - 1.0)) {
                        issues.push(GeometryIssue {
                            kind: "label-on-own-connector",
                            detail: format!("label of {} touches its connector", e.id),
                        });
                        break;
                    }
                }
            }
        }
        let mut ports: HashMap<(i64, i64), &str> = HashMap::new();
        for e in &self.edges {
            for p in [e.points[0], *e.points.last().unwrap()] {
                let key = ((p.0 * 2.0).round() as i64, (p.1 * 2.0).round() as i64);
                if let Some(prev) = ports.insert(key, &e.id) {
                    issues.push(GeometryIssue {
                        kind: "shared-attach-point",
                        detail: format!("{} and {} share an attach point", prev, e.id),
                    });
                }
            }
        }
        for (i, a) in self.edges.iter().enumerate() {
            for b in &self.edges[i + 1..] {
                if let (Some(la), Some(lb)) = (&a.label, &b.label) {
                    if la.rect.intersects(&lb.rect) {
                        issues.push(GeometryIssue {
                            kind: "label-over-label",
                            detail: format!("labels of {} and {} overlap", a.id, b.id),
                        });
                    }
                }
            }
        }
        for (i, a) in self.nodes.iter().enumerate() {
            for b in &self.nodes[i + 1..] {
                if a.rect.intersects(&b.rect) {
                    issues.push(GeometryIssue { kind: "node-overlap", detail: format!("{} overlaps {}", a.id, b.id) });
                }
            }
        }
        for (i, a) in self.containers.iter().enumerate() {
            for b in &self.containers[i + 1..] {
                if a.rect.intersects(&b.rect) {
                    issues.push(GeometryIssue {
                        kind: "container-overlap",
                        detail: format!("{} overlaps {}", a.id, b.id),
                    });
                }
            }
        }
        issues
    }
}

fn seg_hits(seg: &Rect, r: &Rect) -> bool {
    seg.x <= r.right() && r.x <= seg.right() && seg.y <= r.bottom() && r.y <= seg.bottom()
}
