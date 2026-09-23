//! Geometric contract: connectors are orthogonal, attach to their cards, never
//! pass behind other cards, never share an attach point; labels never cover a
//! card, another label, or their own connector. Checked on the contract
//! fixtures, on analyzer drafts of every fixture repo, and on seeded random IRs.

use std::path::Path;

use nunki_analyzer::{draft_ir, scan, Depth, DraftOptions, ScanOptions};
use nunki_ir::*;
use nunki_renderer::{render_html, render_svg, RenderOptions};

fn repo_root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn assert_clean(name: &str, ir: &DiagramIR) {
    let r = render_svg(ir, &RenderOptions::default());
    let issues = r.layout.check_geometry();
    assert!(issues.is_empty(), "{name}: {issues:#?}");
    assert_eq!(r.layout.edges.len() + r.layout.skipped_edges.len(), ir.edges.len());
}

#[test]
fn contract_fixtures_render_cleanly() {
    let dir = repo_root().join("tests/contract/valid");
    let mut count = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let ir = parse_ir(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_clean(&path.display().to_string(), &ir);
        count += 1;
    }
    assert!(count >= 4);
}

#[test]
fn analyzer_drafts_of_all_fixtures_render_cleanly() {
    for fixture in [
        "polyglot-shop",
        "rust-mini",
        "ts-mini",
        "go-mini",
        "python-mini",
        "real-world/compose-subdir",
        "real-world/grpc-env-file",
        "real-world/unparsed-worker",
        "real-world/rust-workspace",
        "real-world/uv-workspace",
        "real-world/spring-cloud",
        "real-world/spring-modulith",
    ] {
        for depth in [Depth::System, Depth::Container, Depth::Component] {
            let report =
                scan(&repo_root().join("tests/fixtures").join(fixture), &ScanOptions { depth, ..Default::default() })
                    .unwrap();
            let draft = draft_ir(
                &report,
                &DraftOptions { generated_at: Some("2026-01-01T00:00:00Z".into()), ..Default::default() },
            );
            assert_clean(&format!("{fixture}/{depth:?}"), &draft.ir);
        }
    }
}

#[test]
fn every_component_focus_renders_cleanly() {
    let root = repo_root().join("tests/fixtures/polyglot-shop");
    for focus in ["web", "api-gateway", "payments", "fulfillment", "ledger-audit"] {
        let report =
            scan(&root, &ScanOptions { depth: Depth::Component, focus: Some(focus.into()), ..Default::default() })
                .unwrap();
        let draft = draft_ir(&report, &DraftOptions::default());
        assert_clean(focus, &draft.ir);
    }
}

/// Small deterministic PRNG (no external crates).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

fn random_ir(seed: u64) -> DiagramIR {
    let mut rng = Lcg(seed);
    let containers: Vec<Container> = (0..rng.below(4))
        .map(|i| Container {
            id: format!("c{i}"),
            label: format!("Zone {i}"),
            boundary_type: [
                BoundaryType::TrustZone,
                BoundaryType::Storage,
                BoundaryType::ThirdParty,
                BoundaryType::Client,
            ][i as usize % 4],
            role_description: None,
        })
        .collect();
    let n = 2 + rng.below(11) as usize;
    let nodes: Vec<Node> = (0..n)
        .map(|i| Node {
            id: format!("n{i}"),
            container_id: if containers.is_empty() || rng.below(3) == 0 {
                None
            } else {
                Some(format!("c{}", rng.below(containers.len() as u64)))
            },
            label: ["Gateway", "Billing service", "Orders", "A much longer node label here", "DB"]
                [rng.below(5) as usize]
                .to_string(),
            subtitle: (rng.below(2) == 0).then(|| "Handles requests".to_string()),
            tech_stack: (rng.below(2) == 0).then(|| "Rust · Axum".to_string()),
            is_key_focal_point: i == 0,
            evidence: None,
            metadata: None,
            attributes: None,
            state_kind: None,
        })
        .collect();
    let m = rng.below((n * 2) as u64 + 1) as usize;
    let mut edges = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for k in 0..m {
        let s = rng.below(n as u64) as usize;
        let t = rng.below(n as u64) as usize;
        if s == t || !seen.insert((s, t)) {
            continue;
        }
        edges.push(Edge {
            id: format!("e{k}"),
            source: format!("n{s}"),
            target: format!("n{t}"),
            label: (rng.below(2) == 0).then(|| {
                ["calls", "writes orders", "publishes order.placed", "reads"][rng.below(4) as usize].to_string()
            }),
            edge_type: [EdgeType::Sync, EdgeType::Async, EdgeType::Event, EdgeType::Read, EdgeType::Write]
                [rng.below(5) as usize],
            style: None,
            is_primary_path: Some(rng.below(4) == 0),
            sequence: None,
            reply: None,
            payload: None,
            cardinality: None,
            guard: None,
            evidence: None,
        });
    }
    DiagramIR {
        version: IrVersion::V1_0_0,
        diagram_type: DiagramType::Container,
        title: format!("Random {seed}"),
        subtitle: None,
        theme: Theme::EditorialLight,
        metadata: DiagramMetadata {
            target_repo: ".".into(),
            commit_hash: None,
            generated_at: "2026-01-01T00:00:00Z".into(),
            visual_density_score: None,
        },
        containers,
        nodes,
        edges,
    }
}

#[test]
fn random_graphs_keep_connectors_off_cards() {
    // Label placement is best-effort under extreme crowding; structural
    // guarantees (no pass-behind, attachment, orthogonality) are absolute.
    let hard = [
        "diagonal-segment",
        "passes-behind-node",
        "detached-endpoint",
        "shared-attach-point",
        "node-overlap",
        "container-overlap",
        "label-over-node",
        "label-on-own-connector",
    ];
    let mut label_collisions = 0;
    for seed in 0..300 {
        let ir = random_ir(seed);
        let r = render_svg(&ir, &RenderOptions::default());
        for issue in r.layout.check_geometry() {
            if hard.contains(&issue.kind) {
                panic!("seed {seed}: {issue:?}\n{}", ir.to_json_pretty());
            }
            label_collisions += 1;
        }
    }
    assert!(label_collisions < 30, "too many label collisions: {label_collisions}");
}

#[test]
fn output_is_deterministic() {
    let ir = random_ir(42);
    assert_eq!(
        render_html(&ir, &RenderOptions::default()).content,
        render_html(&ir, &RenderOptions::default()).content
    );
}

#[test]
fn html_is_self_contained_and_escapes_untrusted_text() {
    let mut ir = random_ir(7);
    ir.title = "</script><script>alert(1)</script>".into();
    ir.nodes[0].label = "<img src=x onerror=alert(1)>".into();
    // A bare quote is the payload that catches a missing `esc` in an attribute
    // rather than in text: `aria-label="…"`, `data-id="…"`, a `<title>`. A tag
    // payload passes straight through an unescaped attribute without breaking it.
    ir.nodes[1].label = "\" onmouseover=alert(1) x=\"".into();
    if let Some(sub) = ir.nodes[1].tech_stack.as_mut() {
        *sub = "\" onfocus=alert(1) y=\"".into();
    }
    let html = render_html(&ir, &RenderOptions::default()).content;
    assert!(!html.contains("<script>alert"));
    assert!(!html.contains("<img src=x"));
    assert!(!html.contains("<script src"));
    assert!(!html.contains("<link"));
    assert!(!html.contains("@import"));
    assert_eq!(html.matches("<script").count(), 2, "one JSON data island, one inline app script");
    assert!(html.contains("Content-Security-Policy"));
    // The attribute payload would not break a tag if it leaked, so a tag-shaped
    // assertion cannot see it. What makes it live is an *unescaped* quote closing
    // the attribute it sits in — and the words themselves are the label's own
    // text, which the book is supposed to print.
    //
    // Checked on the markup outside the JSON data island. Inside it the quote is
    // `\"`, which is JSON's own escaping and inert — the island's safety is that
    // `<` and `>` are escaped, which the `<script` count above already asserts.
    let island = html.find("<script type=\"application/json\"").expect("the page has a data island");
    let island_end = html[island..].find("</script>").expect("unterminated island") + island;
    let markup = format!("{}{}", &html[..island], &html[island_end..]);
    assert!(!markup.contains("\" onmouseover=alert"), "an unescaped quote closed an attribute");
    assert!(!markup.contains("\" onfocus=alert"), "an unescaped quote closed an attribute");
    assert!(
        markup.contains("&quot; onmouseover=alert(1) x=&quot;"),
        "the quote payload never reached the markup, so this proves nothing"
    );
}

/// The draw.io export lands in another application, so nothing downstream sees
/// it again. Every shape style it writes sets `html=1`, which means draw.io
/// renders the label as markup.
///
/// The two formats need different amounts of escaping, because a different
/// number of parsers stand between the file and the HTML renderer. The XML file
/// is parsed as XML first, and that undoes one round — so a label has to be
/// escaped twice, and `&lt;img` in the file means `<img` reaches the renderer.
/// The CSV file is not, so one round is right and `&lt;img` in the file is
/// exactly what makes draw.io *display* the characters. Asserting the same thing
/// of both would call one of them a bug.
#[test]
fn drawio_export_does_not_hand_draw_io_a_live_tag() {
    let mut ir = random_ir(5);
    ir.title = "</script><script>alert(1)</script>".into();
    ir.nodes[0].label = "<img src=x onerror=alert(1)>".into();
    if let Some(e) = ir.edges.first_mut() {
        e.label = Some("<img src=x onerror=alert(1)>".into());
    }
    let no_links = |_: &Evidence| None;

    let xml = nunki_renderer::drawio::to_xml(&ir, &no_links);
    assert!(!xml.contains("<img src=x"), "a raw img tag is in the XML");
    assert!(!xml.contains("<script>alert"), "a raw script tag is in the XML");
    assert!(!xml.contains("&lt;img"), "the XML parser would hand draw.io a live img tag");
    // Escaped, not dropped: without this the assertions above would pass on an
    // export that silently lost every label.
    assert!(xml.contains("&amp;lt;img"), "the label is absent from the XML, so this proves nothing");

    let csv = nunki_renderer::drawio::to_csv(&ir, &no_links);
    // Rows only. The header comment prints the diagram's title for a person
    // reading the file, and draw.io renders nothing from a `#` line — escaping it
    // would only make the comment harder to read. What matters for a comment is
    // that it cannot become a directive, which is the last assertion here.
    let rows: String = csv.lines().filter(|l| !l.starts_with('#')).collect::<Vec<_>>().join("\n");
    assert!(!rows.contains("<img src=x"), "a raw img tag is in a CSV row");
    assert!(!rows.contains("<script>alert"), "a raw script tag is in a CSV row");
    assert!(rows.contains("&lt;img"), "the label is absent from the CSV, so this proves nothing");

    // An edge label travels inside the `connect` directive, which is a `#` line —
    // so the row scan above cannot see it, and draw.io renders it as a label like
    // any other. Without this, removing the escaping there changes nothing any
    // assertion notices.
    for line in csv.lines().filter(|l| l.starts_with("# connect:")) {
        assert!(!line.contains("<img src=x"), "a raw img tag is in a connect directive: {line}");
        assert!(!line.contains("<script>alert"), "a raw script tag is in a connect directive: {line}");
    }

    // A newline in a title or a label must not add a line the CSV reader reads as
    // a directive of its own. The title and the edge label do appear on `#` lines
    // — the header comment and the `connect` rule — so the invariant is not that
    // they are absent, but that no *directive* exists which the exporter did not
    // write.
    let directives: std::collections::BTreeSet<&str> = csv
        .lines()
        .filter_map(|l| l.strip_prefix("# "))
        .filter_map(|l| l.split_once(':'))
        .map(|(key, _)| key)
        .filter(|k| !k.contains(' '))
        .collect();
    let expected: std::collections::BTreeSet<&str> = [
        "label",
        "style",
        "namespace",
        "identity",
        "parent",
        "parentstyle",
        "left",
        "top",
        "width",
        "height",
        "layout",
        "connect",
        "ignore",
    ]
    .into_iter()
    .collect();
    assert!(
        directives.is_subset(&expected),
        "a label wrote a directive the exporter did not: {:?}",
        directives.difference(&expected).collect::<Vec<_>>()
    );
}

#[test]
fn standalone_svg_has_viewbox_and_no_interaction_attributes() {
    let ir = random_ir(3);
    let svg = render_svg(&ir, &RenderOptions::default()).content;
    assert!(svg.starts_with("<?xml"));
    assert!(svg.contains("viewBox=\"0 0 "));
    assert!(!svg.contains("tabindex"));
}

const HARD: [&str; 8] = [
    "diagonal-segment",
    "passes-behind-node",
    "detached-endpoint",
    "shared-attach-point",
    "node-overlap",
    "container-overlap",
    "label-over-node",
    "label-on-own-connector",
];

fn random_sequence(seed: u64) -> DiagramIR {
    let mut rng = Lcg(seed ^ 0x5eed);
    let mut ir = random_ir(seed);
    ir.version = IrVersion::V1_1_0;
    ir.diagram_type = DiagramType::Sequence;
    ir.containers.clear();
    ir.nodes.truncate(8);
    for n in &mut ir.nodes {
        n.container_id = None;
    }
    let n = ir.nodes.len() as u64;
    let labels = ["GET /orders/{id}", "validate", "201 Created", "publish order.placed with a long topic name", ""];
    ir.edges = (0..1 + rng.below(30))
        .map(|k| {
            let s = rng.below(n);
            let t = if rng.below(5) == 0 { s } else { rng.below(n) };
            let label = labels[rng.below(labels.len() as u64) as usize];
            Edge {
                id: format!("m{k}"),
                source: format!("n{s}"),
                target: format!("n{t}"),
                label: (!label.is_empty()).then(|| label.to_string()),
                edge_type: [EdgeType::Sync, EdgeType::Async, EdgeType::Event][rng.below(3) as usize],
                style: None,
                is_primary_path: Some(rng.below(5) == 0),
                sequence: Some(k as u32 + 1),
                reply: Some(rng.below(3) == 0),
                payload: (rng.below(2) == 0).then(|| "CheckoutRequest".to_string()),
                cardinality: None,
                guard: None,
                evidence: None,
            }
        })
        .collect();
    ir
}

#[test]
fn random_sequences_stack_messages_between_lifelines() {
    for seed in 0..200 {
        let ir = random_sequence(seed);
        let r = render_svg(&ir, &RenderOptions::default());
        let issues = r.layout.check_geometry();
        assert!(issues.is_empty(), "seed {seed}: {issues:#?}\n{}", ir.to_json_pretty());
        let lay = &r.layout;
        assert_eq!(lay.lifelines.len(), ir.nodes.len());
        let head_bottom = lay.nodes.iter().map(|b| b.rect.bottom()).fold(0.0, f64::max);
        let mut last_y = head_bottom;
        for e in &lay.edges {
            let label = e.label.as_ref().expect("messages are numbered");
            assert!(label.rect.y > head_bottom, "seed {seed}: label of {} above the lifelines", e.id);
            assert!(label.rect.x >= 0.0 && label.rect.right() <= lay.width, "seed {seed}: {} off canvas", e.id);
            let y = e.points[0].1;
            assert!(y > last_y, "seed {seed}: {} is not below the previous message", e.id);
            last_y = e.points.last().unwrap().1;
            if e.source != e.target {
                let span = (e.points[0].0 - e.points[1].0).abs();
                assert!(label.rect.w <= span, "seed {seed}: label of {} wider than its span", e.id);
            }
        }
        assert!(lay.lifelines.iter().all(|l| l.bottom >= last_y), "seed {seed}: lifeline ends above a message");
        assert!(lay.legend_top > last_y);
    }
}

#[test]
fn random_entity_and_state_diagrams_keep_geometry() {
    for seed in 0..150 {
        let mut rng = Lcg(seed ^ 0xe7);
        let mut er = random_ir(seed);
        er.version = IrVersion::V1_1_0;
        er.diagram_type = DiagramType::EntityRelationship;
        for node in &mut er.nodes {
            node.attributes = Some(
                (0..rng.below(20))
                    .map(|i| Attribute {
                        name: ["id", "customer_id", "a_really_long_column_name_here", "status"][i as usize % 4].into(),
                        type_name: ["uuid", "timestamp with time zone", "text"][rng.below(3) as usize].into(),
                        key: [None, Some(KeyKind::Pk), Some(KeyKind::Fk), Some(KeyKind::PkFk)][rng.below(4) as usize],
                        nullable: rng.below(3) == 0,
                        note: None,
                    })
                    .collect(),
            );
        }
        for e in &mut er.edges {
            e.cardinality =
                Some([Cardinality::OneToMany, Cardinality::OneToOne, Cardinality::ManyToMany][rng.below(3) as usize]);
        }
        let r = render_svg(&er, &RenderOptions::default());
        for issue in r.layout.check_geometry() {
            assert!(!HARD.contains(&issue.kind), "ER seed {seed}: {issue:?}");
        }
        for b in &r.layout.nodes {
            assert!(b.rows.len() <= nunki_renderer::layout::MAX_ROWS);
            assert!(b.rect.h >= b.content_h(), "ER seed {seed}: {} rows overflow the card", b.id);
        }
        assert!(r.content.contains("ad-er"));

        let mut states = random_ir(seed);
        states.version = IrVersion::V1_1_0;
        states.diagram_type = DiagramType::Lifecycle;
        states.nodes[0].state_kind = Some(StateKind::Initial);
        if let Some(last) = states.nodes.last_mut() {
            last.state_kind = Some(StateKind::Terminal);
        }
        for e in &mut states.edges {
            e.guard = (rng.below(2) == 0).then(|| "amount > 0".to_string());
        }
        let r = render_svg(&states, &RenderOptions::default());
        for issue in r.layout.check_geometry() {
            assert!(!HARD.contains(&issue.kind), "lifecycle seed {seed}: {issue:?}");
        }
    }
}

#[test]
fn typed_renders_mark_their_semantics() {
    let load = |name: &str| {
        let path = repo_root().join("tests/contract/valid").join(name);
        parse_ir(&std::fs::read_to_string(path).unwrap()).unwrap()
    };
    let seq = render_svg(&load("sequence-checkout.json"), &RenderOptions::default()).content;
    assert_eq!(seq.matches(r#"class="ad-lifeline""#).count(), 4);
    assert!(seq.contains(">1. POST /api/checkout<") && seq.contains(">CheckoutRequest<"));
    assert!(seq.contains("is-reply"));
    assert!(seq.contains(">Reply<"));

    let er = render_svg(&load("entity-relationship-orders.json"), &RenderOptions::default()).content;
    assert!(er.contains("c-s-one c-t-many"));
    assert!(er.contains(r#"class="ad-row-key""#) && er.contains(">PK FK<"));
    assert!(er.contains(">timestamptz?<"));
    assert!(er.contains("<title>one to many</title>"));

    let life = render_svg(&load("lifecycle-order-status.json"), &RenderOptions::default()).content;
    assert!(life.contains("state-initial") && life.contains("state-terminal"));
    assert!(life.contains("ship_order [status = &#39;paid&#39;]"));
}

/// Connectors crossing the same gutter share a lane when their vertical runs do
/// not overlap, and the lanes that remain are spaced far enough apart to follow.
///
/// Reviewed feedback on the demo book's container diagram: "the narrow area
/// between the application and infrastructure contains many overlapping
/// vertical lines; tracing a service to PostgreSQL requires deliberate
/// attention." It did — nine parallel lines, 12px apart, each edge holding a
/// full-height lane of its own whether it needed one or not.
#[test]
fn gutter_lanes_are_shared_and_spaced() {
    let root = repo_root().join("tests/fixtures/polyglot-shop");
    let report = scan(&root, &ScanOptions { depth: Depth::Container, ..Default::default() }).unwrap();
    let draft =
        draft_ir(&report, &DraftOptions { generated_at: Some("2026-01-01T00:00:00Z".into()), ..Default::default() });
    let r = render_svg(&draft.ir, &RenderOptions::default());
    assert!(r.layout.check_geometry().is_empty());

    // Vertical runs of two or more points: the lanes through a gutter.
    let mut lanes: Vec<f64> = Vec::new();
    for e in &r.layout.edges {
        for w in e.points.windows(2) {
            if (w[0].0 - w[1].0).abs() < 0.01 && (w[0].1 - w[1].1).abs() > 1.0 {
                lanes.push(w[0].0);
            }
        }
    }
    lanes.sort_by(f64::total_cmp);
    lanes.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    assert!(lanes.len() >= 3, "this diagram routes through a gutter: {lanes:?}");

    // One lane per edge would be nine here. Sharing has to actually happen.
    let vertical_edges = r.layout.edges.iter().filter(|e| e.points.len() > 2).count();
    assert!(
        lanes.len() < vertical_edges,
        "lanes ({}) should be fewer than the edges needing them ({vertical_edges}): {lanes:?}",
        lanes.len()
    );

    // Neighbouring lanes inside one gutter need air. 12px reads as a bundle;
    // a jump between gutters is large and is not a pitch.
    for (a, b) in lanes.iter().zip(lanes.iter().skip(1)) {
        let pitch = b - a;
        assert!(pitch >= 19.0, "lanes {a} and {b} are {pitch}px apart, too close to tell apart");
    }
}
