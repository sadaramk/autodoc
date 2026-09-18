//! The IR examples shipped in the agent skill must stay valid.

use autodoc_validator::{validate_json, ValidateOptions};

#[test]
fn reference_container_example_is_valid() {
    let doc = include_str!("../../../skills/autodoc/reference.md");
    let start = doc.find("```json\n{\n  \"version\"").expect("example block") + "```json\n".len();
    let end = start + doc[start..].find("```").unwrap();
    let (ir, report) =
        validate_json(&doc[start..end], &ValidateOptions { verify_evidence: false, ..Default::default() });
    assert!(report.valid, "{:#?}", report.diagnostics);
    assert_eq!(report.warning_count, 0, "{:#?}", report.diagnostics);
    assert_eq!(ir.unwrap().nodes.len(), 5);
    assert_eq!(report.density.unwrap().score, 0.125);
}
