//! Journey tests: drive the built `autodoc` binary exactly as a user or an
//! agent would, against a real git repository copied from the fixtures.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_autodoc");

fn fixtures() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures")).to_path_buf()
}

fn git(repo: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "user.email=ci@example.com", "-c", "user.name=CI"])
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// A fixture as a committed git repo with a GitHub remote.
fn repo_from(fixture: &str, name: &str, remote: &str) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join(name);
    copy_dir(&fixtures().join(fixture), &repo);
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["remote", "add", "origin", remote]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "init"]);
    (tmp, repo)
}

/// polyglot-shop as a committed git repo with a GitHub remote.
fn shop_repo() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("polyglot-shop");
    copy_dir(&fixtures().join("polyglot-shop"), &repo);
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["remote", "add", "origin", "git@github.com:acme/polyglot-shop.git"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "init"]);
    (tmp, repo)
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dest = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &dest);
        } else {
            std::fs::copy(e.path(), dest).unwrap();
        }
    }
}

fn autodoc(args: &[&str], cwd: &Path) -> Output {
    Command::new(BIN).args(args).current_dir(cwd).output().unwrap()
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[test]
fn journey_init_generate_book_edit_and_check_in_ci() {
    let (_tmp, repo) = shop_repo();
    let o = autodoc(&["init", "."], &repo);
    assert!(o.status.success(), "{}", text(&o));
    assert!(repo.join("autodoc.toml").is_file());
    let again = autodoc(&["init", "."], &repo);
    assert_eq!(again.status.code(), Some(2), "init refuses to overwrite");

    // 1. Generate the book.
    let o = autodoc(&["generate", ".", "--json"], &repo);
    assert!(o.status.success(), "{}", text(&o));
    let report: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(report["evidence"]["verified"], report["evidence"]["total"], "{report}");
    let out = repo.join("docs/architecture");
    for f in [
        "index.html",
        "manifest.json",
        "README.md",
        "llms.txt",
        "llms-full.txt",
        "pages/01-overview.md",
        "pages/02-architecture.md",
        "pages/03-containers-api-gateway.md",
        "pages/04-data-and-integrations.md",
        "pages/05-critical-flows.md",
        "pages/06-api-api-gateway.md",
        "pages/07-functional-specification.md",
        "pages/08-business-requirements.md",
        "pages/09-evidence-and-unknowns.md",
        "authored.json",
        "diagrams/containers.ir.json",
        "diagrams/data-model.ir.json",
        "diagrams/lifecycle-orders-status.ir.json",
        "diagrams/flow-api-gateway-post-checkout.ir.json",
        "diagrams/containers.svg",
        "diagrams/components-payments.ir.json",
    ] {
        assert!(out.join(f).is_file(), "missing {f}");
    }
    let html = std::fs::read_to_string(out.join("index.html")).unwrap();
    // Citations carry the repository, commit and prefix once; the reader and the
    // Markdown mirror build each forge permalink from them.
    assert!(html.contains(r#""webUrl":"https://github.com/acme/polyglot-shop""#), "repository for permalinks");
    assert!(html.contains(r#""state":"verified""#));
    let md = std::fs::read_to_string(out.join("pages/02-architecture.md")).unwrap();
    assert!(md.contains("https://github.com/acme/polyglot-shop/blob/"), "forge permalinks in Markdown citations: {md}");

    // 2. Regenerating an unchanged commit rewrites nothing.
    let o = autodoc(&["generate", ".", "--json"], &repo);
    let again: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(again["written"].as_array().unwrap().len(), 0, "{again}");
    let o = autodoc(&["check", "."], &repo);
    assert!(o.status.success(), "{}", text(&o));

    // 2b. Committing the book is the first thing CI does with it, and it moves
    // HEAD without changing anything the book describes. Pinning to HEAD left
    // the documentation a commit behind itself and `check` red however often it
    // was regenerated — the loop had no exit.
    git(&repo, &["add", "docs"]);
    git(&repo, &["commit", "-qm", "add generated documentation"]);
    let o = autodoc(&["check", "."], &repo);
    assert!(o.status.success(), "committing the book must not invalidate it: {}", text(&o));

    git(&repo, &["commit", "-q", "--allow-empty", "-m", "unrelated"]);
    let o = autodoc(&["check", "."], &repo);
    assert!(o.status.success(), "a commit touching nothing documented must not invalidate it: {}", text(&o));

    // 3. A hand-edited diagram survives regeneration and is rendered.
    let ir_path = out.join("diagrams/containers.ir.json");
    let edited =
        std::fs::read_to_string(&ir_path).unwrap().replacen("Polyglot Shop — containers", "Checkout platform", 1);
    std::fs::write(&ir_path, &edited).unwrap();
    let o = autodoc(&["generate", "."], &repo);
    assert!(text(&o).contains("kept hand-edited diagrams/containers.ir.json"), "{}", text(&o));
    assert_eq!(std::fs::read_to_string(&ir_path).unwrap(), edited);
    assert!(std::fs::read_to_string(out.join("index.html")).unwrap().contains("Checkout platform"));

    // 4. CI catches drift: a changed entry point makes citations stale.
    let server = repo.join("api-gateway/src/server.ts");
    std::fs::write(&server, std::fs::read_to_string(&server).unwrap().replace("listen(", "listen( /* moved */ "))
        .unwrap();
    let o = autodoc(&["check", "."], &repo);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(text(&o).contains("api-gateway/src/server.ts"), "{}", text(&o));

    // 5. …and a deleted page.
    std::fs::remove_file(out.join("pages/05-critical-flows.md")).unwrap();
    let o = autodoc(&["check", ".", "--json"], &repo);
    let check: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(check["missing"].as_array().unwrap().iter().any(|m| m == "pages/05-critical-flows.md"), "{check}");
}

#[test]
fn journey_agent_self_heals_a_rejected_ir() {
    let (tmp, repo) = shop_repo();
    let ir_path = tmp.path().join("arch.ir.json");
    let o = autodoc(&["analyze", repo.to_str().unwrap(), "--emit-ir", ir_path.to_str().unwrap()], tmp.path());
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("api-gateway → payments"));

    // Simulate a sloppy agent: accent everywhere and a typo in an edge target.
    let mut ir: Value = serde_json::from_str(&std::fs::read_to_string(&ir_path).unwrap()).unwrap();
    for n in ir["nodes"].as_array_mut().unwrap() {
        n["isKeyFocalPoint"] = json!(true);
    }
    ir["edges"][0]["target"] = json!("kafak");
    std::fs::write(&ir_path, serde_json::to_string_pretty(&ir).unwrap()).unwrap();

    let html = tmp.path().join("arch.html");
    let o = autodoc(&["render", ir_path.to_str().unwrap(), "-o", html.to_str().unwrap(), "--json"], tmp.path());
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(!html.exists(), "nothing is written when validation fails");
    let outcome: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(outcome["status"], "rejected");
    let diags = outcome["validation"]["diagnostics"].as_array().unwrap();
    let codes: Vec<&str> = diags.iter().map(|d| d["code"].as_str().unwrap()).collect();
    assert!(codes.contains(&"ERR_ACCENT_OVERUSE"), "{codes:?}");
    assert!(codes.contains(&"ERR_MISSING_ENDPOINT"), "{codes:?}");

    // Apply the machine-readable patches, highest index first, and retry.
    let mut ops: Vec<Value> = diags.iter().flat_map(|d| d["patch"].as_array().cloned().unwrap_or_default()).collect();
    ops.sort_by_key(|o| std::cmp::Reverse(o["path"].as_str().unwrap().to_string()));
    autodoc_validator::apply_patch(&mut ir, &ops).unwrap();
    std::fs::write(&ir_path, serde_json::to_string_pretty(&ir).unwrap()).unwrap();

    let o = autodoc(&["render", ir_path.to_str().unwrap(), "-o", html.to_str().unwrap()], tmp.path());
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("evidence 10/10 verified"), "{}", text(&o));
    assert!(html.is_file());
}

#[test]
fn journey_evidence_drift_is_detected_after_code_changes() {
    let (tmp, repo) = shop_repo();
    let ir_path = tmp.path().join("arch.ir.json");
    assert!(autodoc(&["analyze", repo.to_str().unwrap(), "--emit-ir", ir_path.to_str().unwrap()], tmp.path())
        .status
        .success());
    let o = autodoc(&["validate", ir_path.to_str().unwrap()], tmp.path());
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("evidence: 10/10 verified"), "{}", text(&o));

    // Rewrite the gateway's entry point; its pinned lines are now stale.
    let server = repo.join("api-gateway/src/server.ts");
    let src = std::fs::read_to_string(&server).unwrap().replace("listen(", "listen( /* moved */ ");
    std::fs::write(&server, src).unwrap();

    let o = autodoc(&["validate", ir_path.to_str().unwrap()], tmp.path());
    assert!(o.status.success(), "stale evidence warns but still renders: {}", text(&o));
    assert!(text(&o).contains("WARN_EVIDENCE_STALE"), "{}", text(&o));

    let ir: Value = serde_json::from_str(&std::fs::read_to_string(&ir_path).unwrap()).unwrap();
    let ev = &ir["nodes"].as_array().unwrap().iter().find(|n| n["id"] == "api-gateway").unwrap()["evidence"];
    let reference = format!("{}:{}-{}", ev["filePath"].as_str().unwrap(), ev["startLine"], ev["endLine"]);
    let o = autodoc(&["verify", &reference, "api-gateway/src/db.ts:1", "--repo", repo.to_str().unwrap()], tmp.path());
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(text(&o).contains("stale"), "{}", text(&o));
    assert!(text(&o).contains("verified"), "{}", text(&o));

    let o = autodoc(&["verify", "api-gateway/src/nope.ts:3", "--repo", repo.to_str().unwrap(), "--json"], tmp.path());
    let res: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(res["results"][0]["state"], "file-missing");
}

#[test]
fn journey_svg_output_and_schema() {
    let (tmp, repo) = shop_repo();
    let ir_path = tmp.path().join("arch.ir.json");
    assert!(autodoc(
        &["analyze", repo.to_str().unwrap(), "--depth", "system", "--emit-ir", ir_path.to_str().unwrap()],
        tmp.path()
    )
    .status
    .success());
    let svg = tmp.path().join("arch.svg");
    let o =
        autodoc(&["render", ir_path.to_str().unwrap(), "-o", svg.to_str().unwrap(), "--accent", "coral"], tmp.path());
    assert!(o.status.success(), "{}", text(&o));
    let content = std::fs::read_to_string(&svg).unwrap();
    assert!(content.contains("--ad-accent:#E11D48"));
    assert!(content.contains("System context".to_uppercase().as_str()));

    let o = autodoc(&["schema"], tmp.path());
    let committed =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schema/diagram-ir.schema.json")).unwrap();
    assert_eq!(String::from_utf8_lossy(&o.stdout), committed);

    let o = autodoc(&["render", "missing.json"], tmp.path());
    assert_eq!(o.status.code(), Some(2));
}

struct Mcp {
    child: std::process::Child,
    reader: BufReader<std::process::ChildStdout>,
    next_id: i64,
}

impl Mcp {
    fn start(cwd: &Path) -> Mcp {
        let mut child = Command::new(BIN)
            .arg("serve")
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let reader = BufReader::new(child.stdout.take().unwrap());
        Mcp { child, reader, next_id: 1 }
    }

    fn send(&mut self, v: Value) {
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin, "{v}").unwrap();
        stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let mut line = String::new();
        self.reader.read_line(&mut line).unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["id"], id);
        v
    }

    fn tool(&mut self, name: &str, args: Value) -> Value {
        let r = self.request("tools/call", json!({"name": name, "arguments": args}));
        let result = r["result"].clone();
        let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text, result["structuredContent"], "text and structured content agree");
        result
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[test]
fn journey_mcp_agent_scans_compiles_and_verifies() {
    let (tmp, repo) = shop_repo();
    let mut mcp = Mcp::start(tmp.path());

    let init = mcp.request(
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "journey", "version": "1"}}),
    );
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    mcp.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let tools = mcp.request("tools/list", json!({}));
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 4);

    let scan = mcp.tool("autodoc_scan_repository", json!({"repoPath": "polyglot-shop", "depth": "container"}));
    assert_eq!(scan["isError"], false);
    let sc = &scan["structuredContent"];
    assert_eq!(sc["report"]["containers"].as_array().unwrap().len(), 5);
    assert!(sc["report"]["evidenceMap"]["payments"]["filePath"].as_str().unwrap().ends_with("main.go"));
    assert_eq!(sc["draftValidation"]["valid"], true);

    // Agent refinement: tighten a label, keep one focal point.
    let mut ir = sc["draftIr"].clone();
    ir["title"] = json!("Checkout platform");
    let out = tmp.path().join("docs/checkout.html");
    let compiled = mcp.tool(
        "autodoc_compile_diagram",
        json!({"ir": ir, "outputPath": "docs/checkout.html", "format": "html", "repoPath": repo}),
    );
    assert_eq!(compiled["isError"], false, "{compiled}");
    assert_eq!(compiled["structuredContent"]["status"], "compiled");
    assert_eq!(compiled["structuredContent"]["evidence"]["verified"], 10);
    assert!(out.is_file());

    // Cluttered IR: 30 extra nodes blow the density budget.
    let mut cluttered = ir.clone();
    for i in 0..30 {
        cluttered["nodes"].as_array_mut().unwrap().push(json!({"id": format!("extra-{i}"), "label": format!("Extra {i}"), "isKeyFocalPoint": false, "containerId": "platform"}));
        cluttered["edges"].as_array_mut().unwrap().push(json!({"id": format!("x-{i}"), "source": "api-gateway", "target": format!("extra-{i}"), "edgeType": "sync"}));
    }
    let rejected = mcp.tool(
        "autodoc_compile_diagram",
        json!({"ir": cluttered, "outputPath": "docs/cluttered.html", "format": "html"}),
    );
    assert_eq!(rejected["isError"], true);
    let diags = rejected["structuredContent"]["validation"]["diagnostics"].as_array().unwrap();
    let density = diags.iter().find(|d| d["code"] == "ERR_HIGH_DENSITY").expect("density diagnostic");
    assert!(density["suggestions"][0].as_str().unwrap().starts_with("Group nodes ["), "{density}");
    assert!(!tmp.path().join("docs/cluttered.html").exists());

    let verified = mcp.tool(
        "autodoc_verify_evidence",
        json!({"repoPath": repo, "evidenceList": [
            {"filePath": "payments/internal/charge/charge.go", "line": 24, "endLine": 38, "symbolName": "Charge"},
            {"filePath": "payments/internal/charge/charge.go", "line": 400}
        ]}),
    );
    let res = &verified["structuredContent"];
    assert_eq!(res["isGit"], true);
    assert_eq!(res["results"][0]["state"], "verified", "{res}");
    assert_eq!(res["results"][1]["state"], "line-out-of-range");
    assert_eq!(res["allVerified"], false);
}

/// Journey 1 again, on a repository shaped nothing like the demo.
///
/// Every command journey ran against polyglot-shop alone: npm, Cargo, go.mod
/// and pyproject, four services, one compose file. A Maven multi-module JVM
/// system exercises different code for the same promises — modules instead of
/// packages, a gateway and a config server, configuration that names services,
/// and a runtime page — and nothing was checking that a book of that shape
/// generates, stays put, and fails the build when the code moves.
#[test]
fn journey_generate_and_check_a_jvm_multi_module_repository() {
    let (_tmp, repo) = repo_from("real-world/spring-cloud", "shop-cloud", "git@github.com:acme/shop-cloud.git");

    let o = autodoc(&["generate", ".", "--json"], &repo);
    assert!(o.status.success(), "{}", text(&o));
    let report: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(report["evidence"]["verified"], report["evidence"]["total"], "{report}");

    // The shape a JVM system produces, which the demo repository never has.
    let out = repo.join("docs/architecture");
    for f in [
        "pages/03-containers-accounts.md",
        "pages/03-containers-gateway.md",
        "pages/05-runtime-and-deployment.md",
        "pages/06-api-accounts.md",
    ] {
        assert!(out.join(f).is_file(), "missing {f}");
    }
    let md = std::fs::read_to_string(out.join("pages/02-architecture.md")).unwrap();
    assert!(md.contains("https://github.com/acme/shop-cloud/blob/"), "forge permalinks: {md}");

    // Committing the book leaves the check green, as it must for CI.
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "docs"]);
    let o = autodoc(&["check", "."], &repo);
    assert!(o.status.success(), "{}", text(&o));

    // Changing a documented Java source turns it red, naming the file.
    let controller = repo.join("accounts/src/main/java/com/acme/accounts/web/AccountController.java");
    let src = std::fs::read_to_string(&controller).unwrap_or_else(|_| panic!("{} missing", controller.display()));
    std::fs::write(
        &controller,
        src.replace("public class AccountController", "public class AccountController /* moved */"),
    )
    .unwrap();
    let o = autodoc(&["check", "."], &repo);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(text(&o).contains("AccountController.java"), "{}", text(&o));
}
