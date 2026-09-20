//! Cross-language contract: the same fixtures are parsed by
//! `packages/ir-spec-ts/test/contract.test.ts`. Both sides must accept every
//! valid file and reject every invalid one at the same JSON path.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

fn contract_dir(kind: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/contract").join(kind)
}

fn json_files(kind: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(contract_dir(kind))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter(|p| p.file_name().unwrap() != "expected-paths.json")
        .collect();
    files.sort();
    files
}

#[test]
fn valid_fixtures_parse() {
    let files = json_files("valid");
    assert!(files.len() >= 4);
    for f in files {
        let text = fs::read_to_string(&f).unwrap();
        if let Err(e) = nunki_ir::parse_ir(&text) {
            panic!("{} should parse: {e}", f.display());
        }
    }
}

#[test]
fn invalid_fixtures_fail_at_the_same_path() {
    let expected: BTreeMap<String, String> =
        serde_json::from_str(&fs::read_to_string(contract_dir("invalid").join("expected-paths.json")).unwrap())
            .unwrap();
    let files = json_files("invalid");
    assert_eq!(files.len(), expected.len(), "every invalid fixture needs an expected path");
    for f in files {
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let path = expected.get(&name).unwrap_or_else(|| panic!("no expected path for {name}"));
        let err = match nunki_ir::parse_ir(&fs::read_to_string(&f).unwrap()) {
            Ok(_) => panic!("{name} should be rejected"),
            Err(e) => e,
        };
        assert_eq!(&err.path, path, "{name}: {}", err.message);
    }
}
