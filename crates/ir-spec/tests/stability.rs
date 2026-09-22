//! `DiagramIR` is a published contract. This compares the schema against the
//! frozen baseline for the current minor series and fails on a **breaking**
//! change while allowing an additive one.
//!
//! The drift test in `lib.rs` is a different promise: it fails on *any*
//! difference, which keeps the committed file honest but says nothing about
//! whether a consumer would break. Adding an optional field must stay free, and
//! removing a field must not — so the difference has to be classified, not just
//! detected.
//!
//! What counts as breaking, from the point of view of something *reading* our
//! output:
//!
//! | breaking                              | additive                    |
//! |---------------------------------------|-----------------------------|
//! | a property or definition disappears   | a new optional property     |
//! | an optional property becomes required | a new definition            |
//! | a required property becomes optional  | an enum gains a variant     |
//! | a type changes                        | prose changes               |
//! | an enum loses a variant               |                             |
//!
//! A required property becoming optional is breaking too, and that direction is
//! easy to miss: a reader that has always found the field present may not
//! handle its absence.

use std::collections::BTreeSet;

use serde_json::Value;

const BASELINE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schema/stable/diagram-ir-0.4.schema.json");
const CURRENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schema/diagram-ir.schema.json");

fn load(path: &str) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn obj<'a>(v: &'a Value, key: &str) -> Option<&'a serde_json::Map<String, Value>> {
    v.get(key).and_then(Value::as_object)
}

fn strings(v: Option<&Value>) -> BTreeSet<String> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

/// Every difference that would break a reader, described where it is.
fn breaking_changes(base: &Value, cur: &Value) -> Vec<String> {
    let mut out = Vec::new();
    compare(base, cur, "DiagramIR", &mut out);

    let (bd, cd) = (obj(base, "$defs").cloned().unwrap_or_default(), obj(cur, "$defs").cloned().unwrap_or_default());
    for (name, b) in &bd {
        match cd.get(name) {
            // A new definition is additive; a removed one is not, because
            // something references it.
            None => out.push(format!("definition `{name}` was removed")),
            Some(c) => compare(b, c, name, &mut out),
        }
    }
    out
}

fn compare(base: &Value, cur: &Value, at: &str, out: &mut Vec<String>) {
    // Types are compared as written: `"string"` and `["string","null"]` differ,
    // and a reader can tell the difference too.
    if let (Some(b), Some(c)) = (base.get("type"), cur.get("type")) {
        if b != c {
            out.push(format!("{at}: type changed from {b} to {c}"));
        }
    }

    let (breq, creq) = (strings(base.get("required")), strings(cur.get("required")));
    for f in breq.difference(&creq) {
        out.push(format!("{at}.{f}: was required and is not any more — a reader may rely on it being present"));
    }
    for f in creq.difference(&breq) {
        out.push(format!("{at}.{f}: is newly required — anything writing this shape now fails validation"));
    }

    let benum = strings(base.get("enum"));
    let cenum = strings(cur.get("enum"));
    for v in benum.difference(&cenum) {
        out.push(format!("{at}: enum lost the value `{v}`"));
    }

    let (bp, cp) =
        (obj(base, "properties").cloned().unwrap_or_default(), obj(cur, "properties").cloned().unwrap_or_default());
    for (name, b) in &bp {
        match cp.get(name) {
            None => out.push(format!("{at}.{name}: property was removed")),
            Some(c) => compare(b, c, &format!("{at}.{name}"), out),
        }
    }

    // `oneOf` carries the nullable and tagged-union shapes schemars emits. A
    // branch disappearing is a value the consumer can no longer receive — or,
    // worse, one it can no longer send.
    if let (Some(b), Some(c)) =
        (base.get("oneOf").and_then(Value::as_array), cur.get("oneOf").and_then(Value::as_array))
    {
        if b.len() > c.len() {
            out.push(format!("{at}: oneOf lost {} branch(es)", b.len() - c.len()));
        }
    }
}

/// The promise in issue #8: no breaking field change for a full minor cycle.
///
/// When this fails the change is not necessarily wrong — it is a decision. Take
/// the deprecation path in `CONTRIBUTING.md`, or, if the series is ending,
/// freeze a new baseline under `schema/stable/`.
#[test]
fn the_schema_makes_no_breaking_change_within_the_minor_series() {
    let changes = breaking_changes(&load(BASELINE), &load(CURRENT));
    assert!(
        changes.is_empty(),
        "DiagramIR broke its published contract for the 0.4 series:\n  - {}\n\nSee CONTRIBUTING.md \
         (\"Changing DiagramIR\"). Additive changes need no baseline update.",
        changes.join("\n  - ")
    );
}

/// The comparison has to be able to see each kind of break, or the test above
/// passes for the wrong reason. Baseline on the left, a mutated copy on the right.
#[test]
fn every_kind_of_break_is_recognised() {
    let base = load(BASELINE);
    let case = |mutate: &dyn Fn(&mut Value), expect: &str| {
        let mut cur = base.clone();
        mutate(&mut cur);
        let found = breaking_changes(&base, &cur);
        assert!(
            found.iter().any(|c| c.contains(expect)),
            "mutation should have been reported as breaking (looking for {expect:?}), got {found:?}"
        );
    };

    case(
        &|v| {
            v["properties"].as_object_mut().unwrap().remove("nodes");
        },
        "property was removed",
    );

    case(
        &|v| {
            v["$defs"].as_object_mut().unwrap().remove("Node");
        },
        "definition `Node` was removed",
    );

    case(
        &|v| {
            v["required"] = serde_json::json!([]);
        },
        "was required and is not any more",
    );

    case(
        &|v| {
            let r = v["required"].as_array_mut().unwrap();
            r.push(serde_json::json!("somethingNew"));
        },
        "is newly required",
    );

    case(
        &|v| {
            v["$defs"]["Theme"]["enum"] = serde_json::json!(["editorial-light"]);
        },
        "enum lost the value `editorial-dark`",
    );

    case(
        &|v| {
            v["$defs"]["Theme"]["type"] = serde_json::json!("integer");
        },
        "type changed",
    );

    case(
        &|v| {
            v["$defs"]["DiagramType"]["oneOf"].as_array_mut().unwrap().pop();
        },
        "oneOf lost",
    );

    // And the other direction: adding an optional field must stay free, or the
    // gate blocks the changes it is meant to allow.
    let mut additive = base.clone();
    additive["properties"]["somethingOptional"] = serde_json::json!({"type": "string"});
    additive["$defs"]["BrandNew"] = serde_json::json!({"type": "object"});
    additive["$defs"]["Theme"]["enum"].as_array_mut().unwrap().push(serde_json::json!("editorial-sepia"));
    assert!(
        breaking_changes(&base, &additive).is_empty(),
        "additive changes must pass: {:?}",
        breaking_changes(&base, &additive)
    );
}
