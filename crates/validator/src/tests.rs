use super::*;

fn node(id: &str, container: Option<&str>) -> Node {
    Node {
        id: id.into(),
        container_id: container.map(Into::into),
        label: id.to_uppercase(),
        subtitle: None,
        tech_stack: None,
        is_key_focal_point: false,
        evidence: None,
        metadata: None,
        attributes: None,
        state_kind: None,
    }
}

fn edge(s: &str, t: &str) -> Edge {
    Edge {
        id: format!("{s}-{t}"),
        source: s.into(),
        target: t.into(),
        label: None,
        edge_type: EdgeType::Sync,
        style: None,
        is_primary_path: None,
        sequence: None,
        reply: None,
        payload: None,
        cardinality: None,
        guard: None,
        evidence: None,
    }
}

fn base() -> DiagramIR {
    let mut api = node("api", Some("platform"));
    api.is_key_focal_point = true;
    api.evidence = Some(Evidence {
        file_path: "src/api.ts".into(),
        start_line: 3,
        end_line: 5,
        symbol_name: Some("handler".into()),
    });
    DiagramIR {
        version: IrVersion::V1_0_0,
        diagram_type: DiagramType::Container,
        title: "Shop".into(),
        subtitle: None,
        theme: Theme::EditorialLight,
        metadata: DiagramMetadata {
            target_repo: ".".into(),
            commit_hash: None,
            generated_at: "2026-01-01T00:00:00Z".into(),
            visual_density_score: None,
        },
        containers: vec![Container {
            id: "platform".into(),
            label: "Platform".into(),
            boundary_type: BoundaryType::TrustZone,
            role_description: None,
        }],
        nodes: vec![node("web", None), api, node("db", None)],
        edges: vec![edge("web", "api"), edge("api", "db")],
    }
}

fn opts() -> ValidateOptions {
    ValidateOptions { verify_evidence: false, ..Default::default() }
}

fn codes_of(r: &ValidationReport) -> Vec<&str> {
    r.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

/// Applies every diagnostic's patch (highest array index first) and re-validates.
fn heal(ir: &DiagramIR, r: &ValidationReport) -> ValidationReport {
    let mut doc = serde_json::to_value(ir).unwrap();
    let mut ops: Vec<Value> = r.diagnostics.iter().flat_map(|d| d.patch.clone()).collect();
    ops.sort_by_key(|o| std::cmp::Reverse(o["path"].as_str().unwrap().to_string()));
    apply_patch(&mut doc, &ops).unwrap();
    validate(&parse_ir_value(doc).unwrap(), &opts())
}

#[test]
fn clean_ir_is_valid() {
    let r = validate(&base(), &opts());
    assert!(r.valid, "{:#?}", r.diagnostics);
    assert_eq!(r.warning_count, 0, "{:#?}", r.diagnostics);
    assert_eq!(r.density.unwrap().score, 0.063);
}

#[test]
fn accent_overuse_keeps_most_connected_and_patch_heals() {
    let mut ir = base();
    for n in &mut ir.nodes {
        n.is_key_focal_point = true;
    }
    let r = validate(&ir, &opts());
    assert!(!r.valid);
    let d = r.diagnostics.iter().find(|d| d.code == codes::ACCENT_OVERUSE).unwrap();
    assert!(d.suggestions[0].contains("Keep `api`"), "{}", d.suggestions[0]);
    assert_eq!(d.patch.len(), 2);
    assert!(heal(&ir, &r).valid);
}

#[test]
fn two_focal_points_are_allowed() {
    let mut ir = base();
    ir.nodes[2].is_key_focal_point = true;
    assert!(validate(&ir, &opts()).valid);
}

#[test]
fn missing_endpoint_suggests_typo_fix() {
    let mut ir = base();
    ir.edges[1].target = "dbb".into();
    let r = validate(&ir, &opts());
    let d = r.diagnostics.iter().find(|d| d.code == codes::MISSING_ENDPOINT).unwrap();
    assert_eq!(d.path, "$.edges[1].target");
    assert_eq!(d.patch[0]["value"], "db");
    assert!(!codes_of(&r).contains(&codes::ORPHAN_NODE), "db is the proposed fix, not an orphan");
    assert!(heal(&ir, &r).valid);
}

#[test]
fn orphans_self_loops_duplicates_and_unknown_containers() {
    let mut ir = base();
    ir.nodes.push(node("lonely", Some("platfrom")));
    ir.edges.push(edge("api", "api"));
    let mut dup = edge("web", "api");
    dup.id = "dup".into();
    ir.edges.push(dup);
    let r = validate(&ir, &opts());
    let c = codes_of(&r);
    for code in [codes::ORPHAN_NODE, codes::SELF_LOOP, codes::DUPLICATE_EDGE, codes::UNKNOWN_CONTAINER] {
        assert!(c.contains(&code), "missing {code} in {c:?}");
    }
    let uc = r.diagnostics.iter().find(|d| d.code == codes::UNKNOWN_CONTAINER).unwrap();
    assert_eq!(uc.patch[0]["value"], "platform");
    // Errors sort before warnings.
    let first_warning = r.diagnostics.iter().position(|d| d.severity == Severity::Warning).unwrap();
    assert!(r.diagnostics[..first_warning].iter().all(|d| d.severity == Severity::Error));
}

#[test]
fn duplicate_and_invalid_ids() {
    let mut ir = base();
    ir.nodes[2].id = "platform".into();
    ir.edges[1].target = "platform".into();
    ir.edges[0].id = "web api".into();
    let r = validate(&ir, &opts());
    let c = codes_of(&r);
    assert!(c.contains(&codes::DUPLICATE_ID));
    assert!(c.contains(&codes::INVALID_ID));
}

#[test]
fn unlabeled_cycles_are_rejected_labelled_ones_pass() {
    let mut ir = base();
    ir.edges.push(edge("db", "web"));
    let r = validate(&ir, &opts());
    let d = r.diagnostics.iter().find(|d| d.code == codes::UNLABELED_CYCLE).expect("cycle detected");
    assert_eq!(d.element_ids.len(), 3);
    assert!(heal(&ir, &r).valid);
    ir.edges[2].label = Some("replicates".into());
    assert!(validate(&ir, &opts()).valid);
}

#[test]
fn high_density_offers_grouping_and_budget() {
    let mut ir = base();
    ir.containers.push(Container {
        id: "workers".into(),
        label: "Workers".into(),
        boundary_type: BoundaryType::InternalService,
        role_description: None,
    });
    for i in 0..10 {
        let id = format!("w{i}");
        ir.nodes.push(node(&id, Some("workers")));
        ir.edges.push(edge("api", &id));
        ir.edges.push(edge(&id, "db"));
    }
    // 13 nodes + 22 edges = 35 > 32
    let r = validate(&ir, &opts());
    let d = r.diagnostics.iter().find(|d| d.code == codes::HIGH_DENSITY).unwrap();
    let density = r.density.unwrap();
    assert_eq!((density.budget, density.excess), (32, 3));
    assert!(d.suggestions[0].starts_with("Group nodes [w0, w1,"), "{:?}", d.suggestions);
    assert!(d.suggestions[0].contains("saves 27"), "{}", d.suggestions[0]);
}

#[test]
fn stricter_density_is_honoured_but_looser_is_clamped() {
    let ir = base();
    let strict = ValidateOptions { max_density: 0.05, ..opts() };
    assert!(codes_of(&validate(&ir, &strict)).contains(&codes::HIGH_DENSITY));
    let loose = ValidateOptions { max_density: 0.9, ..opts() };
    assert_eq!(validate(&ir, &loose).density.unwrap().max, MAX_VISUAL_DENSITY);
}

#[test]
fn primary_path_overuse_and_breaks_warn() {
    let mut ir = base();
    ir.nodes.push(node("x", None));
    ir.nodes.push(node("y", None));
    ir.edges.push(edge("x", "y"));
    for e in &mut ir.edges {
        e.is_primary_path = Some(true);
    }
    let r = validate(&ir, &opts());
    assert!(r.valid);
    let c = codes_of(&r);
    assert!(c.contains(&codes::PRIMARY_PATH_OVERUSE));
    assert!(c.contains(&codes::PRIMARY_PATH_BROKEN));
    let strict = validate(&ir, &ValidateOptions { strict: true, ..opts() });
    assert!(!strict.valid);
}

#[test]
fn schema_errors_become_diagnostics() {
    let (ir, r) = validate_json(r#"{"version":"1.0.0","diagramType":"flowchart"}"#, &opts());
    assert!(ir.is_none());
    assert_eq!(r.diagnostics[0].code, codes::SCHEMA);
    assert_eq!(r.diagnostics[0].path, "$.diagramType");
    assert!(r.agent_hint.contains("ERR_SCHEMA"));
}

#[test]
fn density_mismatch_and_long_labels_warn() {
    let mut ir = base();
    ir.metadata.visual_density_score = Some(0.9);
    ir.nodes[0].label = "A label that is far too long for an editorial card".into();
    let r = validate(&ir, &opts());
    assert!(r.valid);
    let c = codes_of(&r);
    assert!(c.contains(&codes::DENSITY_MISMATCH));
    assert!(c.contains(&codes::LABEL_TOO_LONG));
}

#[test]
fn evidence_is_verified_against_the_filesystem() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(
        dir.path().join("src/api.ts"),
        "import x from 'y';\n\nexport function handler() {\n  return 1;\n}\n",
    )
    .unwrap();
    let mut ir = base();
    ir.nodes[0].evidence =
        Some(Evidence { file_path: "src/web.ts".into(), start_line: 1, end_line: 2, symbol_name: None });
    ir.nodes[2].evidence =
        Some(Evidence { file_path: "src/api.ts".into(), start_line: 4, end_line: 40, symbol_name: None });
    let o = ValidateOptions { repo_root: Some(dir.path().into()), ..Default::default() };
    let r = validate(&ir, &o);
    let c = codes_of(&r);
    assert!(c.contains(&codes::EVIDENCE_FILE_MISSING), "{c:?}");
    assert!(c.contains(&codes::EVIDENCE_OUT_OF_RANGE), "{c:?}");
    assert_eq!(r.evidence.iter().filter(|e| e.is_ok()).count(), 1);
    let range = r.diagnostics.iter().find(|d| d.code == codes::EVIDENCE_OUT_OF_RANGE).unwrap();
    assert_eq!(range.patch[1]["value"], 5);

    ir.nodes[1].evidence.as_mut().unwrap().symbol_name = Some("render".into());
    let r = validate(&ir, &o);
    assert!(codes_of(&r).contains(&codes::EVIDENCE_SYMBOL_MISMATCH));

    let no_repo = validate(&ir, &ValidateOptions::default());
    assert!(codes_of(&no_repo).contains(&codes::EVIDENCE_UNVERIFIED));
}

#[test]
fn json_patch_ops() {
    let mut v = json!({"a": [1, 2, 3], "b": {"c": 1}});
    apply_patch(
        &mut v,
        &[
            json!({"op":"remove","path":"/a/1"}),
            json!({"op":"replace","path":"/b/c","value":2}),
            json!({"op":"add","path":"/a/-","value":9}),
        ],
    )
    .unwrap();
    assert_eq!(v, json!({"a": [1, 3, 9], "b": {"c": 2}}));
    assert!(apply_patch(&mut v, &[json!({"op":"remove","path":"/a/9"})]).is_err());
}

#[test]
fn tarjan_finds_components() {
    let adj = vec![vec![1], vec![2], vec![0, 3], vec![]];
    let mut comps: Vec<Vec<usize>> = tarjan(&adj)
        .into_iter()
        .map(|mut c| {
            c.sort();
            c
        })
        .collect();
    comps.sort();
    assert_eq!(comps, vec![vec![0, 1, 2], vec![3]]);
}

fn fixture(name: &str) -> DiagramIR {
    let path = format!("{}/../../tests/contract/valid/{name}", env!("CARGO_MANIFEST_DIR"));
    parse_ir(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn typed_contract_fixtures_are_valid() {
    for name in ["sequence-checkout.json", "entity-relationship-orders.json", "lifecycle-order-status.json"] {
        let r = validate(&fixture(name), &opts());
        assert!(r.valid, "{name}: {:#?}", r.diagnostics);
        let unexpected: Vec<_> = r.diagnostics.iter().filter(|d| d.code != codes::FOCAL_WITHOUT_EVIDENCE).collect();
        assert!(unexpected.is_empty(), "{name}: {unexpected:#?}");
    }
}

#[test]
fn sequence_messages_need_unique_order_and_bounded_size() {
    let mut ir = fixture("sequence-checkout.json");
    ir.edges[1].sequence = None;
    ir.edges[3].sequence = Some(1);
    let r = validate(&ir, &opts());
    let c = codes_of(&r);
    assert!(c.contains(&codes::SEQUENCE_MISSING) && c.contains(&codes::SEQUENCE_DUPLICATE), "{c:?}");
    assert!(!c.contains(&codes::SELF_LOOP), "self-calls are allowed in sequences");
    assert!(!c.contains(&codes::UNLABELED_CYCLE), "call/reply loops are allowed");
    let missing = r.diagnostics.iter().find(|d| d.code == codes::SEQUENCE_MISSING).unwrap();
    assert_eq!(missing.patch[0]["value"], 2);

    let mut big = fixture("sequence-checkout.json");
    for i in 0..10 {
        big.nodes.push(node(&format!("p{i}"), None));
        let mut e = edge("web", &format!("p{i}"));
        e.sequence = Some(10 + i);
        big.edges.push(e);
    }
    assert!(codes_of(&validate(&big, &opts())).contains(&codes::TOO_MANY_PARTICIPANTS));
}

#[test]
fn lifecycle_rules() {
    let mut ir = fixture("lifecycle-order-status.json");
    ir.edges.push(Edge {
        id: "t3".into(),
        source: "shipped".into(),
        target: "pending".into(),
        label: None,
        ..ir.edges[0].clone()
    });
    ir.nodes.push(Node { id: "archived".into(), label: "archived".into(), state_kind: None, ..ir.nodes[1].clone() });
    ir.nodes[3].is_key_focal_point = false;
    let r = validate(&ir, &opts());
    let c = codes_of(&r);
    for code in [codes::TERMINAL_HAS_TRANSITIONS, codes::UNREACHABLE_STATE, codes::UNLABELED_TRANSITION] {
        assert!(c.contains(&code), "missing {code}: {c:?}");
    }
    ir.nodes[0].state_kind = None;
    assert!(codes_of(&validate(&ir, &opts())).contains(&codes::NO_INITIAL_STATE));
}

#[test]
fn entity_relationship_rules_and_misplaced_fields() {
    let mut ir = fixture("entity-relationship-orders.json");
    ir.edges[0].cardinality = None;
    ir.nodes[1].attributes = None;
    let c = codes_of(&validate(&ir, &opts())).into_iter().map(String::from).collect::<Vec<_>>();
    assert!(
        c.contains(&codes::MISSING_CARDINALITY.to_string())
            && c.contains(&codes::ENTITY_WITHOUT_ATTRIBUTES.to_string())
    );

    let mut arch = base();
    arch.edges[0].sequence = Some(1);
    assert!(codes_of(&validate(&arch, &opts())).contains(&codes::FIELD_IGNORED));
}

#[test]
fn edge_evidence_is_verified_and_healed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.ts"), "export function pay() {\n  return 1;\n}\n").unwrap();
    let mut ir = fixture("sequence-checkout.json");
    ir.nodes[1].is_key_focal_point = false;
    ir.edges[0].evidence =
        Some(Evidence { file_path: "a.ts".into(), start_line: 1, end_line: 3, symbol_name: Some("pay".into()) });
    ir.edges[1].evidence =
        Some(Evidence { file_path: "missing.ts".into(), start_line: 1, end_line: 1, symbol_name: None });
    let o = ValidateOptions { repo_root: Some(dir.path().into()), ..Default::default() };
    let r = validate(&ir, &o);
    let d = r.diagnostics.iter().find(|d| d.code == codes::EVIDENCE_FILE_MISSING).expect("edge evidence checked");
    assert_eq!(d.path, "$.edges[1].evidence.filePath");
    let (healed, notes) = heal_evidence(&mut ir, &o);
    assert!(healed.valid, "{:#?}", healed.diagnostics);
    assert!(ir.edges[1].evidence.is_none() && ir.edges[0].evidence.is_some());
    assert_eq!(notes.len(), 1);
}
