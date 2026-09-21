//! Model Context Protocol server over stdio (newline-delimited JSON-RPC 2.0).
//!
//! Tools:
//! - `nunki_scan_repository` — C4 summary, entry points, evidence map, draft IR
//! - `nunki_compile_diagram` — validate + render, or structured diagnostics
//! - `nunki_verify_evidence` — file/line checks against the working tree and HEAD

pub mod engine;

use std::io::{BufRead, Write};
use std::path::PathBuf;

use nunki_analyzer::Depth;
use nunki_git::EvidenceQuery;
use nunki_ir::Theme;
use nunki_renderer::Accent;
use serde_json::{json, Value};

use engine::{CompileOutcome, CompileRequest, IrInput, OutputFormat};

pub const SERVER_NAME: &str = "nunki";
pub const SUPPORTED_PROTOCOLS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "nunki turns source code into verifiable, editorial architecture diagrams. \
Workflow: (1) nunki_scan_repository to get containers, relationships, an evidence map and a draft DiagramIR; \
(2) refine the IR — never emit SVG yourself; keep density <= 0.40 and at most 1-2 isKeyFocalPoint nodes; \
(3) nunki_compile_diagram; if it returns status=rejected, fix exactly the elements named in each diagnostic \
(apply its JSON Patch when present) and retry; (4) return the output path with a short architectural brief.";

/// JSON-RPC error codes.
mod rpc {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
}

pub struct Server {
    cwd: PathBuf,
    initialized: bool,
}

impl Default for Server {
    fn default() -> Self {
        Server { cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")), initialized: false }
    }
}

impl Server {
    pub fn with_cwd(cwd: PathBuf) -> Self {
        Server { cwd, initialized: false }
    }

    /// Blocking stdio loop; returns when stdin closes.
    pub fn serve<R: BufRead, W: Write>(&mut self, input: R, mut output: W) -> std::io::Result<()> {
        for line in input.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Some(response) = self.handle_line(&line) {
                writeln!(output, "{response}")?;
                output.flush()?;
            }
        }
        Ok(())
    }

    /// Handles one framed message; `None` for notifications.
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                return Some(error_response(Value::Null, rpc::PARSE_ERROR, &format!("parse error: {e}")).to_string())
            }
        };
        match msg {
            Value::Array(batch) => {
                let replies: Vec<Value> = batch.into_iter().filter_map(|m| self.handle_message(m)).collect();
                (!replies.is_empty()).then(|| Value::Array(replies).to_string())
            }
            other => self.handle_message(other).map(|v| v.to_string()),
        }
    }

    fn handle_message(&mut self, msg: Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let Some(method) = msg.get("method").and_then(Value::as_str) else {
            // Responses from the client (e.g. to sampling) are ignored.
            return id
                .filter(|_| msg.get("result").is_none() && msg.get("error").is_none())
                .map(|id| error_response(id, rpc::INVALID_REQUEST, "missing method"));
        };
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let is_notification = id.is_none();
        let result = match method {
            "initialize" => Ok(self.initialize(&params)),
            "notifications/initialized" | "initialized" => {
                self.initialized = true;
                return None;
            }
            "notifications/cancelled" => return None,
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tool_definitions() })),
            "tools/call" => self.call_tool(&params),
            "resources/list" => Ok(json!({ "resources": [] })),
            "resources/templates/list" => Ok(json!({ "resourceTemplates": [] })),
            "prompts/list" => Ok(json!({ "prompts": [] })),
            other => Err((rpc::METHOD_NOT_FOUND, format!("method `{other}` not found"))),
        };
        if is_notification {
            return None;
        }
        let id = id.unwrap_or(Value::Null);
        Some(match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err((code, message)) => error_response(id, code, &message),
        })
    }

    fn initialize(&mut self, params: &Value) -> Value {
        let requested = params.get("protocolVersion").and_then(Value::as_str).unwrap_or(SUPPORTED_PROTOCOLS[0]);
        let version = if SUPPORTED_PROTOCOLS.contains(&requested) { requested } else { SUPPORTED_PROTOCOLS[0] };
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": SERVER_NAME, "title": "nunki", "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        })
    }

    fn call_tool(&self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or((rpc::INVALID_PARAMS, "tools/call requires `name`".to_string()))?;
        let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
        let outcome = match name {
            "nunki_scan_repository" => self.scan(&args),
            "nunki_compile_diagram" => self.compile(&args),
            "nunki_verify_evidence" => self.verify(&args),
            "nunki_generate_book" => self.book(&args),
            other => return Err((rpc::INVALID_PARAMS, format!("unknown tool `{other}`"))),
        };
        // Tool-level failures are results with isError, so the model can read and react.
        Ok(match outcome {
            Ok((structured, is_error)) => tool_result(structured, is_error),
            Err(message) => tool_result(json!({ "error": message }), true),
        })
    }

    fn path_arg(&self, args: &Value, key: &str) -> Option<PathBuf> {
        args.get(key).and_then(Value::as_str).filter(|s| !s.trim().is_empty()).map(|p| {
            let p = PathBuf::from(shellexpand_home(p));
            if p.is_absolute() {
                p
            } else {
                self.cwd.join(p)
            }
        })
    }

    fn scan(&self, args: &Value) -> Result<(Value, bool), String> {
        let repo = self.path_arg(args, "repoPath").ok_or("`repoPath` is required")?;
        let depth = match args.get("depth").and_then(Value::as_str).unwrap_or("container") {
            "system" => Depth::System,
            "container" => Depth::Container,
            "component" => Depth::Component,
            other => return Err(format!("`depth` must be system | container | component, got `{other}`")),
        };
        let focus = args.get("focus").and_then(Value::as_str).map(str::to_string);
        let theme = parse_theme(args.get("theme"))?;
        let include_tests = args.get("includeTests").and_then(Value::as_bool).unwrap_or(false);
        let res = engine::scan_repository(&repo, depth, focus, include_tests, theme).map_err(|e| e.to_string())?;
        Ok((serde_json::to_value(res).map_err(|e| e.to_string())?, false))
    }

    fn compile(&self, args: &Value) -> Result<(Value, bool), String> {
        let ir = match args.get("ir") {
            Some(Value::String(s)) => IrInput::Json(s.clone()),
            Some(v @ Value::Object(_)) => IrInput::Value(v.clone()),
            _ => return Err("`ir` is required (a DiagramIR object)".into()),
        };
        let output_path = self.path_arg(args, "outputPath").ok_or("`outputPath` is required")?;
        let format = match args.get("format").and_then(Value::as_str).unwrap_or("html") {
            "html" => OutputFormat::Html,
            "svg" => OutputFormat::Svg,
            other => return Err(format!("`format` must be html | svg, got `{other}`")),
        };
        let accent = match args.get("accent").and_then(Value::as_str) {
            Some(a) => Accent::parse(a)?,
            None => Accent::default(),
        };
        let req = CompileRequest {
            ir,
            output_path,
            format,
            repo_path: self.path_arg(args, "repoPath"),
            accent,
            strict: args.get("strict").and_then(Value::as_bool).unwrap_or(false),
            verify_evidence: args.get("verifyEvidence").and_then(Value::as_bool).unwrap_or(true),
            max_density: nunki_ir::MAX_VISUAL_DENSITY,
        };
        let outcome = engine::compile_diagram(&req).map_err(|e| e.to_string())?;
        let rejected = matches!(outcome, CompileOutcome::Rejected { .. });
        Ok((serde_json::to_value(outcome).map_err(|e| e.to_string())?, rejected))
    }

    fn book(&self, args: &Value) -> Result<(Value, bool), String> {
        let repo = self.path_arg(args, "repoPath").ok_or("`repoPath` is required")?;
        let out = self.path_arg(args, "outDir").unwrap_or_else(|| repo.join("docs/architecture"));
        let opts = nunki_book::BookOptions {
            theme: parse_theme(args.get("theme"))?,
            accent: match args.get("accent").and_then(Value::as_str) {
                Some(a) => Accent::parse(a)?,
                None => Accent::default(),
            },
            include_tests: args.get("includeTests").and_then(Value::as_bool).unwrap_or(false),
            allow_partial: args.get("allowPartial").and_then(Value::as_bool).unwrap_or(false),
            max_density: nunki_ir::MAX_VISUAL_DENSITY,
        };
        if args.get("checkOnly").and_then(Value::as_bool).unwrap_or(false) {
            let report = nunki_book::check(&repo, &out, &opts).map_err(|e| e.to_string())?;
            let failed = !report.ok;
            return Ok((serde_json::to_value(report).map_err(|e| e.to_string())?, failed));
        }
        let report = nunki_book::generate(&repo, &out, &opts).map_err(|e| e.to_string())?;
        Ok((serde_json::to_value(report).map_err(|e| e.to_string())?, false))
    }

    fn verify(&self, args: &Value) -> Result<(Value, bool), String> {
        let list = args.get("evidenceList").and_then(Value::as_array).ok_or("`evidenceList` is required")?;
        let items: Vec<EvidenceQuery> = list
            .iter()
            .enumerate()
            .map(|(i, v)| serde_json::from_value(v.clone()).map_err(|e| format!("evidenceList[{i}]: {e}")))
            .collect::<Result<_, _>>()?;
        let repo = self.path_arg(args, "repoPath").unwrap_or_else(|| self.cwd.clone());
        let commit = args.get("commitHash").and_then(Value::as_str);
        let res = engine::verify_evidence(&repo, &items, commit).map_err(|e| e.to_string())?;
        Ok((serde_json::to_value(res).map_err(|e| e.to_string())?, false))
    }
}

fn shellexpand_home(p: &str) -> String {
    match (p.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(home)) => format!("{home}/{rest}"),
        _ => p.to_string(),
    }
}

fn parse_theme(v: Option<&Value>) -> Result<Theme, String> {
    match v.and_then(Value::as_str) {
        None | Some("editorial-light") => Ok(Theme::EditorialLight),
        Some("editorial-dark") => Ok(Theme::EditorialDark),
        Some(other) => Err(format!("`theme` must be editorial-light | editorial-dark, got `{other}`")),
    }
}

fn tool_result(structured: Value, is_error: bool) -> Value {
    let text = serde_json::to_string_pretty(&structured).unwrap_or_default();
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": structured,
        "isError": is_error,
    })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// DiagramIR schema with `$defs` hoisted so it can nest inside a tool schema.
fn ir_schema_parts() -> (Value, Value) {
    let mut schema = nunki_ir::json_schema();
    let defs = schema.as_object_mut().and_then(|o| o.remove("$defs")).unwrap_or_else(|| json!({}));
    if let Some(o) = schema.as_object_mut() {
        o.remove("$schema");
        o.insert(
            "description".into(),
            json!("Typed diagram intermediate representation (see SKILL.md). Never raw SVG."),
        );
    }
    (schema, defs)
}

pub fn tool_definitions() -> Value {
    let (ir_schema, defs) = ir_schema_parts();
    json!([
        {
            "name": "nunki_scan_repository",
            "title": "Scan repository architecture",
            "description": "Parse a repository with tree-sitter (Rust, TypeScript/JavaScript, Go, Python) and return a C4 decomposition: containers (deployable units) with entry points, infrastructure (datastores, event buses, vendor APIs), relationships with file+line evidence, a Git evidence map (element id → evidence), and a validated draft DiagramIR to refine.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "repoPath": { "type": "string", "description": "Repository directory (absolute, or relative to the server's working directory)." },
                    "depth": { "type": "string", "enum": ["system", "container", "component"], "default": "container", "description": "C4 level for the draft IR." },
                    "focus": { "type": "string", "description": "Container id to decompose at component depth (defaults to the busiest one)." },
                    "theme": { "type": "string", "enum": ["editorial-light", "editorial-dark"], "default": "editorial-light" },
                    "includeTests": { "type": "boolean", "default": false, "description": "Also scan test, fixture and example directories (excluded by default)." }
                },
                "required": ["repoPath", "depth"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "idempotentHint": true, "openWorldHint": false }
        },
        {
            "name": "nunki_compile_diagram",
            "title": "Compile DiagramIR",
            "description": "Validate a DiagramIR (schema, topology, density <= 0.40, accent budget, evidence against the repository) and render a standalone interactive HTML page or an SVG. On failure returns status=rejected with diagnostics: stable codes (ERR_HIGH_DENSITY, ERR_ACCENT_OVERUSE, ERR_MISSING_ENDPOINT, ERR_ORPHAN_NODE, ERR_UNLABELED_CYCLE, ERR_EVIDENCE_*), JSON paths, ranked suggestions and RFC 6902 patches. Fix only what is named and retry.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "ir": ir_schema,
                    "outputPath": { "type": "string", "description": "Destination file; must end in .html for html or .svg for svg." },
                    "format": { "type": "string", "enum": ["html", "svg"], "default": "html" },
                    "repoPath": { "type": "string", "description": "Repository for evidence verification and source snippets (defaults to metadata.targetRepo)." },
                    "accent": { "type": "string", "description": "`indigo` (default), `coral`, or #RRGGBB. One accent hue only." },
                    "strict": { "type": "boolean", "default": false, "description": "Treat warnings as errors." },
                    "verifyEvidence": { "type": "boolean", "default": true }
                },
                "required": ["ir", "outputPath", "format"],
                "additionalProperties": false,
                "$defs": defs
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        },
        {
            "name": "nunki_generate_book",
            "title": "Generate architecture book",
            "description": "Write the repository's architecture book: index.html (overview, architecture, one page per deployable with its component diagram, data & integrations, critical flows, evidence & unknowns), a Markdown mirror, llms.txt, manifest.json and diagrams/*.ir.json + .svg. Every citation is verified against the commit. Hand-edited diagram IR files are kept and re-rendered. With checkOnly, reports whether the book is out of date or cites stale code without writing.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "repoPath": { "type": "string" },
                    "outDir": { "type": "string", "description": "Book directory (default <repoPath>/docs/architecture)." },
                    "accent": { "type": "string" },
                    "theme": { "type": "string", "enum": ["editorial-light", "editorial-dark"] },
                    "includeTests": { "type": "boolean", "default": false },
                    "checkOnly": { "type": "boolean", "default": false }
                },
                "required": ["repoPath"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        },
        {
            "name": "nunki_verify_evidence",
            "title": "Verify source evidence",
            "description": "Check that file+line references exist in the repository and have not drifted: reports verified, stale (lines changed since the pinned commit or uncommitted), untracked, file-missing, line-out-of-range, symbol-mismatch, with forge permalinks against HEAD.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "evidenceList": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "filePath": { "type": "string" },
                                "line": { "type": "integer", "minimum": 1 },
                                "endLine": { "type": "integer", "minimum": 1 },
                                "symbolName": { "type": "string" }
                            },
                            "required": ["filePath", "line"],
                            "additionalProperties": false
                        }
                    },
                    "repoPath": { "type": "string", "description": "Repository root (defaults to the server's working directory)." },
                    "commitHash": { "type": "string", "description": "Pinned commit to diff against (defaults to HEAD)." }
                },
                "required": ["evidenceList"],
                "additionalProperties": false
            },
            "annotations": { "readOnlyHint": true, "idempotentHint": true, "openWorldHint": false }
        }
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(server: &mut Server, msg: Value) -> Value {
        serde_json::from_str(&server.handle_line(&msg.to_string()).expect("response")).unwrap()
    }

    #[test]
    fn handshake_negotiates_protocol() {
        let mut s = Server::default();
        let r = call(
            &mut s,
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}),
        );
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(r["result"]["serverInfo"]["name"], SERVER_NAME);
        let r = call(
            &mut s,
            json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}),
        );
        assert_eq!(r["result"]["protocolVersion"], SUPPORTED_PROTOCOLS[0]);
        assert!(s.handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
    }

    #[test]
    fn lists_three_tools_with_self_contained_schemas() {
        let mut s = Server::default();
        let r = call(&mut s, json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}));
        let tools = r["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            ["nunki_scan_repository", "nunki_compile_diagram", "nunki_generate_book", "nunki_verify_evidence"]
        );
        let compile = &tools[1]["inputSchema"];
        assert!(compile["$defs"]["Node"].is_object());
        assert_eq!(compile["properties"]["ir"]["properties"]["nodes"]["items"]["$ref"], "#/$defs/Node");
    }

    #[test]
    fn protocol_errors() {
        let mut s = Server::default();
        let r: Value = serde_json::from_str(&s.handle_line("{not json").unwrap()).unwrap();
        assert_eq!(r["error"]["code"], rpc::PARSE_ERROR);
        let r = call(&mut s, json!({"jsonrpc":"2.0","id":7,"method":"nope"}));
        assert_eq!(r["error"]["code"], rpc::METHOD_NOT_FOUND);
        let r = call(&mut s, json!({"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"nope"}}));
        assert_eq!(r["error"]["code"], rpc::INVALID_PARAMS);
        let r = call(
            &mut s,
            json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"nunki_scan_repository","arguments":{}}}),
        );
        assert_eq!(r["result"]["isError"], true);
        assert!(r["result"]["structuredContent"]["error"].as_str().unwrap().contains("repoPath"));
    }

    #[test]
    fn compile_rejects_wrong_extension_and_bad_ir() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Server::with_cwd(dir.path().into());
        let r = call(
            &mut s,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nunki_compile_diagram","arguments":{"ir":{"version":"1.0.0"},"outputPath":"x.txt","format":"html"}}}),
        );
        assert_eq!(r["result"]["isError"], true);
        assert!(r["result"]["structuredContent"]["error"].as_str().unwrap().contains(".html"));
        let r = call(
            &mut s,
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"nunki_compile_diagram","arguments":{"ir":{"version":"1.0.0"},"outputPath":"x.html","format":"html"}}}),
        );
        assert_eq!(r["result"]["isError"], true);
        assert_eq!(r["result"]["structuredContent"]["status"], "rejected");
        assert_eq!(r["result"]["structuredContent"]["validation"]["diagnostics"][0]["code"], "ERR_SCHEMA");
        assert!(!dir.path().join("x.html").exists());
    }
}
