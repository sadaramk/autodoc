//! `DiagramIR` — the typed intermediate representation every diagram passes
//! through. Agents never emit SVG; they emit this, and deterministic compiler
//! passes (validator → layout → renderer) turn it into an artifact.
//!
//! The TypeScript mirror lives in `packages/ir-spec-ts` (zod). Both are pinned
//! to the same fixtures by contract tests, so a field renamed on one side fails
//! the build on the other.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const IR_VERSION: &str = "1.0.0";

/// Density ceiling from the editorial standard: at most 4/10.
pub const MAX_VISUAL_DENSITY: f64 = 0.40;

/// Grid columns × rows of the canonical 16:10 editorial canvas (1600×1000 at
/// 160×125 px cells). This is the "available area" in
/// `Density = (Edges + Nodes) / (Available Area Grid Units)`; the value is
/// fixed so a score means the same thing on every diagram and every machine.
pub const GRID_COLUMNS: u32 = 10;
pub const GRID_ROWS: u32 = 8;
pub const AVAILABLE_GRID_UNITS: u32 = GRID_COLUMNS * GRID_ROWS;

/// Largest `nodes + edges` that still satisfies `MAX_VISUAL_DENSITY`.
pub fn element_budget() -> usize {
    (MAX_VISUAL_DENSITY * AVAILABLE_GRID_UNITS as f64 + 1e-9).floor() as usize
}

pub fn visual_density(nodes: usize, edges: usize) -> f64 {
    let raw = (nodes + edges) as f64 / AVAILABLE_GRID_UNITS as f64;
    (raw * 1000.0).round() / 1000.0
}

/// RFC 3339 UTC timestamp without pulling in a date crate.
pub fn now_rfc3339() -> String {
    let secs =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    rfc3339_from_unix(secs)
}

pub fn rfc3339_from_unix(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum IrVersion {
    #[serde(rename = "1.0.0")]
    V1_0_0,
    /// Adds sequence and entity-relationship diagrams, entity attributes,
    /// state kinds, message order, cardinality and edge evidence.
    #[serde(rename = "1.1.0")]
    V1_1_0,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DiagramType {
    SystemContext,
    Container,
    Component,
    DataFlow,
    /// State machine: nodes are states, edges are transitions.
    Lifecycle,
    /// Nodes are participants (in order), edges are messages ordered by `sequence`.
    Sequence,
    /// Nodes are entities with `attributes`, edges are relations with `cardinality`.
    EntityRelationship,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum StateKind {
    Initial,
    Normal,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum Cardinality {
    #[serde(rename = "1:1")]
    OneToOne,
    #[serde(rename = "1:n")]
    OneToMany,
    #[serde(rename = "n:1")]
    ManyToOne,
    #[serde(rename = "n:m")]
    ManyToMany,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum KeyKind {
    Pk,
    Fk,
    PkFk,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Attribute {
    pub name: String,
    pub type_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<KeyKind>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub nullable: bool,
    /// Constraint or reference note (`→ orders.id`, `max 255`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    #[default]
    EditorialLight,
    EditorialDark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum BoundaryType {
    TrustZone,
    InternalService,
    ThirdParty,
    Storage,
    Client,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeType {
    Sync,
    Async,
    Event,
    Read,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeStyle {
    Solid,
    Dashed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiagramIR {
    pub version: IrVersion,
    pub diagram_type: DiagramType,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    pub theme: Theme,
    pub metadata: DiagramMetadata,
    pub containers: Vec<Container>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiagramMetadata {
    pub target_repo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,
    pub generated_at: String,
    /// Target: <= 0.40. Recomputed by the validator; an agent-supplied value is advisory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visual_density_score: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Container {
    pub id: String,
    pub label: String,
    pub boundary_type: BoundaryType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Node {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_id: Option<String>,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tech_stack: Option<String>,
    /// Max 1-2 nodes. Triggers the primary accent color.
    pub is_key_focal_point: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Evidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BTreeMap<String, String>>,
    /// Entity-relationship diagrams: columns / fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<Vec<Attribute>>,
    /// Lifecycle diagrams: initial / terminal states.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_kind: Option<StateKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Evidence {
    pub file_path: String,
    pub start_line: u32,
    pub end_line: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Edge {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub edge_type: EdgeType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<EdgeStyle>,
    /// Accent highlight for the critical transaction path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_primary_path: Option<bool>,
    /// Sequence diagrams: 1-based message order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u32>,
    /// Sequence diagrams: a response travelling back to the caller.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<bool>,
    /// Sequence diagrams: what the message carries (`CheckoutRequest`, `order.placed`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
    /// Entity-relationship diagrams.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cardinality: Option<Cardinality>,
    /// Lifecycle diagrams: condition that must hold for the transition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guard: Option<String>,
    /// Where the interaction happens in code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Evidence>,
}

impl Edge {
    pub fn is_reply(&self) -> bool {
        self.reply.unwrap_or(false)
    }

    pub fn primary(&self) -> bool {
        self.is_primary_path.unwrap_or(false)
    }

    /// Explicit style wins; otherwise asynchronous kinds and replies read as dashed.
    pub fn resolved_style(&self) -> EdgeStyle {
        if self.style.is_none() && self.is_reply() {
            return EdgeStyle::Dashed;
        }
        self.style.unwrap_or(match self.edge_type {
            EdgeType::Async | EdgeType::Event => EdgeStyle::Dashed,
            EdgeType::Sync | EdgeType::Read | EdgeType::Write => EdgeStyle::Solid,
        })
    }
}

impl DiagramIR {
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn container(&self, id: &str) -> Option<&Container> {
        self.containers.iter().find(|c| c.id == id)
    }

    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).expect("DiagramIR always serializes")
    }
}

/// A parse failure pinned to the JSON path that caused it, so an agent can
/// fix exactly one field instead of regenerating the whole payload.
#[derive(Debug, Clone, thiserror::Error, Serialize)]
#[serde(rename_all = "camelCase")]
#[error("{path}: {message}")]
pub struct SchemaError {
    pub path: String,
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

pub fn parse_ir(json: &str) -> Result<DiagramIR, SchemaError> {
    let de = &mut serde_json::Deserializer::from_str(json);
    serde_path_to_error::deserialize(de).map_err(|e| {
        let inner = e.inner();
        let (line, column) =
            if inner.is_syntax() || inner.is_eof() { (Some(inner.line()), Some(inner.column())) } else { (None, None) };
        SchemaError {
            path: normalize_path(&e.path().to_string()),
            message: strip_position(&inner.to_string()),
            line,
            column,
        }
    })
}

pub fn parse_ir_value(value: serde_json::Value) -> Result<DiagramIR, SchemaError> {
    serde_path_to_error::deserialize(value).map_err(|e| SchemaError {
        path: normalize_path(&e.path().to_string()),
        message: e.inner().to_string(),
        line: None,
        column: None,
    })
}

fn normalize_path(p: &str) -> String {
    if p == "." || p.is_empty() {
        "$".to_string()
    } else {
        format!("$.{p}")
    }
}

fn strip_position(msg: &str) -> String {
    match msg.rfind(" at line ") {
        Some(i) => msg[..i].to_string(),
        None => msg.to_string(),
    }
}

/// JSON Schema (draft 2020-12) for `DiagramIR`, committed at `schema/diagram-ir.schema.json`.
pub fn json_schema() -> serde_json::Value {
    let schema = schemars::schema_for!(DiagramIR);
    serde_json::to_value(schema).expect("schema serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> &'static str {
        r#"{
          "version": "1.0.0",
          "diagramType": "container",
          "title": "Shop",
          "theme": "editorial-light",
          "metadata": { "targetRepo": ".", "generatedAt": "2026-01-01T00:00:00Z" },
          "containers": [{ "id": "platform", "label": "Platform", "boundaryType": "trust-zone" }],
          "nodes": [
            { "id": "api", "containerId": "platform", "label": "API", "isKeyFocalPoint": true,
              "evidence": { "filePath": "api/main.ts", "startLine": 1, "endLine": 9 } },
            { "id": "db", "label": "Postgres", "isKeyFocalPoint": false }
          ],
          "edges": [{ "id": "e1", "source": "api", "target": "db", "edgeType": "write", "isPrimaryPath": true }]
        }"#
    }

    #[test]
    fn parses_and_round_trips() {
        let ir = parse_ir(minimal()).unwrap();
        assert_eq!(ir.diagram_type, DiagramType::Container);
        assert_eq!(ir.nodes[0].evidence.as_ref().unwrap().end_line, 9);
        let again = parse_ir(&ir.to_json_pretty()).unwrap();
        assert_eq!(ir, again);
    }

    #[test]
    fn error_points_at_offending_field() {
        let bad = minimal().replace(r#""edgeType": "write""#, r#""edgeType": "rpc""#);
        let err = parse_ir(&bad).unwrap_err();
        assert_eq!(err.path, "$.edges[0].edgeType");
        assert!(err.message.contains("unknown variant `rpc`"), "{}", err.message);
    }

    #[test]
    fn rejects_unknown_fields_to_catch_typos() {
        let bad = minimal().replace("isKeyFocalPoint\": true", "isKeyFocalpoint\": true");
        let err = parse_ir(&bad).unwrap_err();
        assert!(err.path.starts_with("$.nodes[0]"), "{}", err.path);
    }

    #[test]
    fn syntax_errors_carry_position() {
        let err = parse_ir("{\n  \"version\": ").unwrap_err();
        assert!(err.line.is_some());
    }

    #[test]
    fn dashed_is_default_for_async_kinds() {
        let ir = parse_ir(minimal()).unwrap();
        let mut e = ir.edges[0].clone();
        assert_eq!(e.resolved_style(), EdgeStyle::Solid);
        e.edge_type = EdgeType::Event;
        assert_eq!(e.resolved_style(), EdgeStyle::Dashed);
        e.style = Some(EdgeStyle::Solid);
        assert_eq!(e.resolved_style(), EdgeStyle::Solid);
    }

    #[test]
    fn density_budget_and_timestamps() {
        assert_eq!(element_budget(), 32);
        assert_eq!(visual_density(10, 22), 0.4);
        assert!(visual_density(10, 23) > MAX_VISUAL_DENSITY);
        assert_eq!(rfc3339_from_unix(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_from_unix(1_709_210_096), "2024-02-29T12:34:56Z");
    }

    #[test]
    fn committed_schema_is_current() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schema/diagram-ir.schema.json");
        let committed = std::fs::read_to_string(path).unwrap_or_default();
        let fresh = serde_json::to_string_pretty(&json_schema()).unwrap() + "\n";
        assert!(
            committed == fresh,
            "schema/diagram-ir.schema.json is stale; run `autodoc schema > schema/diagram-ir.schema.json`"
        );
    }
}
