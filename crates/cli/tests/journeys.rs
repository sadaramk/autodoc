//! Journey tests: drive the built `nunki` binary exactly as a user or an
//! agent would, against a real git repository copied from the fixtures.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_nunki");

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

fn nunki(args: &[&str], cwd: &Path) -> Output {
    Command::new(BIN).args(args).current_dir(cwd).output().unwrap()
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[test]
fn journey_init_generate_book_edit_and_check_in_ci() {
    let (_tmp, repo) = shop_repo();
    let o = nunki(&["init", "."], &repo);
    assert!(o.status.success(), "{}", text(&o));
    assert!(repo.join("nunki.toml").is_file());
    let again = nunki(&["init", "."], &repo);
    assert_eq!(again.status.code(), Some(2), "init refuses to overwrite");

    // 1. Generate the book.
    let o = nunki(&["generate", ".", "--json"], &repo);
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
    let o = nunki(&["generate", ".", "--json"], &repo);
    let again: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(again["written"].as_array().unwrap().len(), 0, "{again}");
    let o = nunki(&["check", "."], &repo);
    assert!(o.status.success(), "{}", text(&o));

    // 2b. Committing the book is the first thing CI does with it, and it moves
    // HEAD without changing anything the book describes. Pinning to HEAD left
    // the documentation a commit behind itself and `check` red however often it
    // was regenerated — the loop had no exit.
    git(&repo, &["add", "docs"]);
    git(&repo, &["commit", "-qm", "add generated documentation"]);
    let o = nunki(&["check", "."], &repo);
    assert!(o.status.success(), "committing the book must not invalidate it: {}", text(&o));

    git(&repo, &["commit", "-q", "--allow-empty", "-m", "unrelated"]);
    let o = nunki(&["check", "."], &repo);
    assert!(o.status.success(), "a commit touching nothing documented must not invalidate it: {}", text(&o));

    // 3. A hand-edited diagram survives regeneration and is rendered.
    let ir_path = out.join("diagrams/containers.ir.json");
    let edited =
        std::fs::read_to_string(&ir_path).unwrap().replacen("Polyglot Shop — containers", "Checkout platform", 1);
    std::fs::write(&ir_path, &edited).unwrap();
    let o = nunki(&["generate", "."], &repo);
    assert!(text(&o).contains("kept hand-edited diagrams/containers.ir.json"), "{}", text(&o));
    assert_eq!(std::fs::read_to_string(&ir_path).unwrap(), edited);
    assert!(std::fs::read_to_string(out.join("index.html")).unwrap().contains("Checkout platform"));

    // 4. CI catches drift: a changed entry point makes citations stale.
    let server = repo.join("api-gateway/src/server.ts");
    std::fs::write(&server, std::fs::read_to_string(&server).unwrap().replace("listen(", "listen( /* moved */ "))
        .unwrap();
    let o = nunki(&["check", "."], &repo);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(text(&o).contains("api-gateway/src/server.ts"), "{}", text(&o));

    // 5. …and a deleted page.
    std::fs::remove_file(out.join("pages/05-critical-flows.md")).unwrap();
    let o = nunki(&["check", ".", "--json"], &repo);
    let check: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(check["missing"].as_array().unwrap().iter().any(|m| m == "pages/05-critical-flows.md"), "{check}");
}

#[test]
fn journey_agent_self_heals_a_rejected_ir() {
    let (tmp, repo) = shop_repo();
    let ir_path = tmp.path().join("arch.ir.json");
    let o = nunki(&["analyze", repo.to_str().unwrap(), "--emit-ir", ir_path.to_str().unwrap()], tmp.path());
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
    let o = nunki(&["render", ir_path.to_str().unwrap(), "-o", html.to_str().unwrap(), "--json"], tmp.path());
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
    nunki_validator::apply_patch(&mut ir, &ops).unwrap();
    std::fs::write(&ir_path, serde_json::to_string_pretty(&ir).unwrap()).unwrap();

    let o = nunki(&["render", ir_path.to_str().unwrap(), "-o", html.to_str().unwrap()], tmp.path());
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("evidence 10/10 verified"), "{}", text(&o));
    assert!(html.is_file());
}

#[test]
fn journey_evidence_drift_is_detected_after_code_changes() {
    let (tmp, repo) = shop_repo();
    let ir_path = tmp.path().join("arch.ir.json");
    assert!(nunki(&["analyze", repo.to_str().unwrap(), "--emit-ir", ir_path.to_str().unwrap()], tmp.path())
        .status
        .success());
    let o = nunki(&["validate", ir_path.to_str().unwrap()], tmp.path());
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("evidence: 10/10 verified"), "{}", text(&o));

    // Rewrite the gateway's entry point; its pinned lines are now stale.
    let server = repo.join("api-gateway/src/server.ts");
    let src = std::fs::read_to_string(&server).unwrap().replace("listen(", "listen( /* moved */ ");
    std::fs::write(&server, src).unwrap();

    let o = nunki(&["validate", ir_path.to_str().unwrap()], tmp.path());
    assert!(o.status.success(), "stale evidence warns but still renders: {}", text(&o));
    assert!(text(&o).contains("WARN_EVIDENCE_STALE"), "{}", text(&o));

    let ir: Value = serde_json::from_str(&std::fs::read_to_string(&ir_path).unwrap()).unwrap();
    let ev = &ir["nodes"].as_array().unwrap().iter().find(|n| n["id"] == "api-gateway").unwrap()["evidence"];
    let reference = format!("{}:{}-{}", ev["filePath"].as_str().unwrap(), ev["startLine"], ev["endLine"]);
    let o = nunki(&["verify", &reference, "api-gateway/src/db.ts:1", "--repo", repo.to_str().unwrap()], tmp.path());
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(text(&o).contains("stale"), "{}", text(&o));
    assert!(text(&o).contains("verified"), "{}", text(&o));

    let o = nunki(&["verify", "api-gateway/src/nope.ts:3", "--repo", repo.to_str().unwrap(), "--json"], tmp.path());
    let res: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(res["results"][0]["state"], "file-missing");
}

#[test]
fn journey_svg_output_and_schema() {
    let (tmp, repo) = shop_repo();
    let ir_path = tmp.path().join("arch.ir.json");
    assert!(nunki(
        &["analyze", repo.to_str().unwrap(), "--depth", "system", "--emit-ir", ir_path.to_str().unwrap()],
        tmp.path()
    )
    .status
    .success());
    let svg = tmp.path().join("arch.svg");
    let o = nunki(&["render", ir_path.to_str().unwrap(), "-o", svg.to_str().unwrap(), "--accent", "coral"], tmp.path());
    assert!(o.status.success(), "{}", text(&o));
    let content = std::fs::read_to_string(&svg).unwrap();
    assert!(content.contains("--ad-accent:#E11D48"));
    assert!(content.contains("System context".to_uppercase().as_str()));

    let o = nunki(&["schema"], tmp.path());
    let committed =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schema/diagram-ir.schema.json")).unwrap();
    assert_eq!(String::from_utf8_lossy(&o.stdout), committed);

    let o = nunki(&["render", "missing.json"], tmp.path());
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

    let scan = mcp.tool("nunki_scan_repository", json!({"repoPath": "polyglot-shop", "depth": "container"}));
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
        "nunki_compile_diagram",
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
    let rejected = mcp
        .tool("nunki_compile_diagram", json!({"ir": cluttered, "outputPath": "docs/cluttered.html", "format": "html"}));
    assert_eq!(rejected["isError"], true);
    let diags = rejected["structuredContent"]["validation"]["diagnostics"].as_array().unwrap();
    let density = diags.iter().find(|d| d["code"] == "ERR_HIGH_DENSITY").expect("density diagnostic");
    assert!(density["suggestions"][0].as_str().unwrap().starts_with("Group nodes ["), "{density}");
    assert!(!tmp.path().join("docs/cluttered.html").exists());

    let verified = mcp.tool(
        "nunki_verify_evidence",
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

    let o = nunki(&["generate", ".", "--json"], &repo);
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
    let o = nunki(&["check", "."], &repo);
    assert!(o.status.success(), "{}", text(&o));

    // Changing a documented Java source turns it red, naming the file.
    let controller = repo.join("accounts/src/main/java/com/acme/accounts/web/AccountController.java");
    let src = std::fs::read_to_string(&controller).unwrap_or_else(|_| panic!("{} missing", controller.display()));
    std::fs::write(
        &controller,
        src.replace("public class AccountController", "public class AccountController /* moved */"),
    )
    .unwrap();
    let o = nunki(&["check", "."], &repo);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(text(&o).contains("AccountController.java"), "{}", text(&o));
}

/// `EngineError` derived its message with `{0}` while `#[from]` also made the
/// inner error its source, so `{e:#}` printed the same sentence twice:
/// "cannot scan /nope: … : cannot scan /nope: …". A tool whose first output on
/// a typo is a stutter reads as broken.
#[test]
fn a_failure_is_reported_once() {
    let tmp = tempfile::tempdir().unwrap();
    let o = nunki(&["generate", "does-not-exist"], tmp.path());
    let out = text(&o);
    assert_eq!(o.status.code(), Some(2), "{out}");
    assert_eq!(out.matches("cannot scan").count(), 1, "the reason is stated once: {out}");
}

/// Generating for a directory with nothing in it produced a six-page book around
/// an empty containers diagram, warned `ERR_EMPTY_DIAGRAM`, and exited 0. Pointed
/// at the wrong path — a typo, a misconfigured CI checkout — that looks like a
/// tool that cannot read the repository, and CI stays green on a book of headings.
#[test]
fn an_empty_repository_is_refused_rather_than_documented() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("nothing");
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    std::fs::write(repo.join("docs/notes.txt"), "no source here\n").unwrap();
    let out = tmp.path().join("book");
    let o = nunki(&["generate", repo.to_str().unwrap(), "--out", out.to_str().unwrap()], tmp.path());
    let text = text(&o);
    assert_eq!(o.status.code(), Some(2), "{text}");
    assert!(text.contains("nothing to document"), "{text}");
    assert!(text.contains("Rust, TypeScript, Go, Python, Java, Kotlin"), "names what it reads: {text}");
    assert!(!out.exists(), "no half-book is left behind: {text}");
}

/// A pull request asks what a change does to the architecture, which a book
/// cannot answer: two rendered books diff as text, so a reordered table reads as
/// a change and a new route reads as five. `nunki diff` compares the models.
#[test]
fn journey_diff_reports_what_changed_between_two_revisions() {
    let (_tmp, repo) = shop_repo();

    // An unchanged range is the common case in CI and has to say so plainly.
    let o = nunki(&["diff", ".", "--base", "HEAD", "--head", "HEAD"], &repo);
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("Architecture unchanged"), "{}", text(&o));

    // Add a route that takes a new query parameter and drops authentication,
    // and a column on an entity: the three things a reviewer most wants named.
    let handler = repo.join("payments/internal/httpapi/handler.go");
    let src = std::fs::read_to_string(&handler).unwrap();
    std::fs::write(
        &handler,
        src.replace(
            "\tmux.HandleFunc(\"POST /charges\", h.createCharge)\n",
            "\tmux.HandleFunc(\"POST /charges\", h.createCharge)\n\tmux.HandleFunc(\"GET /charges/{id}\", h.getCharge)\n",
        )
        .replace(
            "// createCharge charges",
            "// getCharge returns one charge attempt.\nfunc (h *Handler) getCharge(w http.ResponseWriter, r *http.Request) {\n\tw.WriteHeader(http.StatusOK)\n}\n\n// createCharge charges",
        ),
    )
    .unwrap();
    git(&repo, &["commit", "-aqm", "read a charge"]);

    let o = nunki(&["diff", ".", "--base", "HEAD~1"], &repo);
    let out = text(&o);
    assert!(o.status.success(), "{out}");
    assert!(out.contains("### Architecture changes"), "{out}");
    assert!(out.contains("added **payments GET /charges/{id}**"), "the new route is named: {out}");
    assert!(!out.contains("POST /charges**"), "an untouched route is not listed: {out}");

    // A response literal keeps its generated model name while its shape changes,
    // so the fields have to be compared, not just the type's name. Dropping the
    // authentication middleware is the other thing a reviewer must not miss.
    let checkout = repo.join("api-gateway/src/routes/checkout.ts");
    let src = std::fs::read_to_string(&checkout).unwrap();
    std::fs::write(
        &checkout,
        src.replace(
            "checkoutRouter.post(\"/\", requireCustomer, async (req, res) => {",
            "checkoutRouter.post(\"/\", async (req, res) => {",
        )
        .replace(
            "res.status(201).json({ orderId: order.id, status: \"placed\" });",
            "res.status(202).json({ orderId: order.id, status: \"placed\", trackingUrl: order.tracking });",
        ),
    )
    .unwrap();
    git(&repo, &["commit", "-aqm", "accept checkout asynchronously"]);

    let o = nunki(&["diff", ".", "--base", "HEAD~1"], &repo);
    let out = text(&o);
    assert!(out.contains("changed **api-gateway POST /checkout**"), "{out}");
    for want in [
        "response gains `trackingUrl`",
        "success status `201` → `202`",
        "no authentication requirement is recognised any more",
    ] {
        assert!(out.contains(want), "should report {want:?}: {out}");
    }

    // `--exit-code` is what turns the report into a gate.
    let o = nunki(&["diff", ".", "--base", "HEAD~2", "--exit-code"], &repo);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));

    // The JSON is the same finding, for a bot that wants to group or filter.
    let o = nunki(&["diff", ".", "--base", "HEAD~2", "--json"], &repo);
    let v: Value = serde_json::from_str(&String::from_utf8_lossy(&o.stdout)).unwrap();
    assert_eq!(v["base"], "HEAD~2");
    let api = v["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["title"] == "API operations")
        .unwrap_or_else(|| panic!("an API section: {v:#}"));
    assert_eq!(api["changes"][0]["verb"], "added");
    assert_eq!(api["changes"][0]["subject"], "payments GET /charges/{id}");

    // The comparison must not disturb the caller's tree: CI checks out the head
    // commit and then runs this, and a leftover worktree breaks the next step.
    let o = nunki(&["diff", ".", "--base", "HEAD~1"], &repo);
    assert!(o.status.success(), "{}", text(&o));
    let wt = Command::new("git").arg("-C").arg(&repo).args(["worktree", "list"]).output().unwrap();
    let list = String::from_utf8_lossy(&wt.stdout);
    assert_eq!(list.lines().count(), 1, "no worktree is left behind: {list}");
}

/// A generated diagram is sometimes the start of a drawing someone then owns.
/// The export is one way — the source stays authoritative about the architecture
/// — so what has to survive the trip is the evidence.
#[test]
fn journey_export_carries_evidence_into_drawio() {
    let (_tmp, repo) = shop_repo();
    let out = repo.join("docs/architecture");
    let o = nunki(&["generate", ".", "--out", "docs/architecture"], &repo);
    assert!(o.status.success(), "{}", text(&o));

    let ir = out.join("diagrams/containers.ir.json");
    let o = nunki(&["export", ir.to_str().unwrap(), "--format", "drawio-csv", "--output", "-"], &repo);
    let csv = String::from_utf8_lossy(&o.stdout).to_string();
    assert!(o.status.success(), "{}", text(&o));

    // The directives draw.io needs to make shapes rather than one text blob.
    for want in ["# label: %label%", "# style: %style%", "# link: url", "# connect: {"] {
        assert!(csv.contains(want), "CSV needs {want:?}:\n{csv}");
    }
    // Positions come from nunki's layout, so draw.io must not run its own.
    // `width`/`height` need the `@` form: with a bare column name draw.io
    // silently ignores them and auto-sizes every shape to its label, which is
    // how the first version of this shipped.
    for want in ["# layout: none", "# left: left", "# top: top", "# width: @width", "# height: @height"] {
        assert!(csv.contains(want), "CSV needs {want:?}:\n{csv}");
    }

    let row =
        csv.lines().find(|l| l.starts_with("api-gateway,")).unwrap_or_else(|| panic!("a row for the gateway:\n{csv}"));
    assert!(row.contains("api-gateway/src/server.ts:"), "file:line travels as shape data: {row}");
    // Geometry: the gateway is a real box, not a zero-sized one.
    let cols: Vec<&str> = row.split(',').collect();
    let head: Vec<&str> = csv.lines().find(|l| l.starts_with("id,")).unwrap().split(',').collect();
    for name in ["left", "top", "width", "height"] {
        let i = head.iter().position(|h| *h == name).unwrap_or_else(|| panic!("a {name} column: {head:?}"));
        let v: i64 = cols[i].parse().unwrap_or_else(|_| panic!("{name} is a number, got {:?}", cols[i]));
        if name == "width" || name == "height" {
            assert!(v > 0, "{name} must be a real size, got {v}");
        }
    }
    // Evidence paths are relative to the scanned root. Building the link from the
    // directory holding the IR prepended `docs/architecture/diagrams` to every
    // one of them — a link to the right path in the wrong place.
    assert!(row.contains("https://github.com/acme/polyglot-shop/blob/"), "a forge remote gives a permalink: {row}");
    assert!(!row.contains("diagrams/api-gateway"), "the book's own path must not leak into the link: {row}");

    // Every id used in a connect column has to name a row, or draw.io drops the edge.
    let header: Vec<&str> = csv.lines().find(|l| l.starts_with("id,")).unwrap().split(',').collect();
    let rows: Vec<Vec<&str>> = csv
        .lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with("id,") && !l.is_empty())
        .map(|l| l.split(',').collect())
        .collect();
    let ids: Vec<&str> = rows.iter().map(|r| r[0]).collect();
    let edge_cols: Vec<usize> =
        header.iter().enumerate().filter(|(_, h)| h.starts_with("edge")).map(|(i, _)| i).collect();
    assert!(!edge_cols.is_empty(), "the containers diagram has edges:\n{csv}");
    let mut linked = 0;
    for r in &rows {
        for &c in &edge_cols {
            for target in r.get(c).unwrap_or(&"").split(',').filter(|t| !t.trim_matches('"').is_empty()) {
                let target = target.trim_matches('"');
                assert!(ids.contains(&target), "edge points at {target:?}, which is not a row: {ids:?}");
                linked += 1;
            }
        }
    }
    assert!(linked >= 5, "the gateway alone has four dependencies, found {linked}");

    // Without a recognised remote there is no link worth writing, and the export
    // says so rather than inventing one.
    let (_bare, plain) = repo_from("polyglot-shop", "no-remote", "/srv/git/shop.git");
    let o = nunki(&["generate", ".", "--out", "docs/architecture"], &plain);
    assert!(o.status.success(), "{}", text(&o));
    let o = nunki(
        &[
            "export",
            plain.join("docs/architecture/diagrams/containers.ir.json").to_str().unwrap(),
            "--format",
            "drawio-csv",
            "--output",
            "-",
        ],
        &plain,
    );
    let out2 = text(&o);
    assert!(out2.contains("no permalinks"), "{out2}");
    assert!(out2.contains("Shapes still carry file:line"), "{out2}");
    let row = out2.lines().find(|l| l.starts_with("api-gateway,")).unwrap();
    assert!(row.contains("api-gateway/src/server.ts:"), "{row}");
    assert!(!row.contains("http"), "no link is better than a broken one: {row}");

    // The default output path sits beside the IR, named for the tool.
    let o = nunki(&["export", ir.to_str().unwrap(), "--format", "drawio-csv"], &repo);
    assert!(o.status.success(), "{}", text(&o));
    assert!(out.join("diagrams/containers.drawio.csv").is_file(), "{}", text(&o));
}

/// CSV cannot carry a route or a label position, so draw.io re-routes every edge
/// and drops every label on its own midpoint: connectors cut through boxes and
/// edge labels print over node names. The `.drawio` file carries the layout the
/// book already computed, which is why it is the default.
#[test]
fn journey_the_drawio_file_carries_the_books_layout() {
    let (_tmp, repo) = shop_repo();
    let o = nunki(&["generate", ".", "--out", "docs/architecture"], &repo);
    assert!(o.status.success(), "{}", text(&o));
    let ir = repo.join("docs/architecture/diagrams/containers.ir.json");

    let o = nunki(&["export", ir.to_str().unwrap(), "--output", "-"], &repo);
    let xml = String::from_utf8_lossy(&o.stdout).to_string();
    assert!(o.status.success(), "{}", text(&o));
    assert!(xml.starts_with("<mxfile"), "a draw.io file: {}", &xml[..xml.len().min(80)]);

    // Routes and label positions: without these draw.io lays the diagram out
    // itself, which is what made the CSV export unusable.
    let waypoints = xml.matches("<Array as=\"points\">").count();
    let offsets = xml.matches("as=\"offset\"").count();
    assert!(waypoints >= 5, "edges carry their route, found {waypoints}:\n{xml}");
    assert!(offsets >= 5, "labels carry their position, found {offsets}");

    // Evidence rides on the shape, as an `<object>` attribute rather than a
    // style, so it survives someone restyling the diagram.
    assert!(xml.contains("evidence=\"api-gateway/src/server.ts:"), "{xml}");
    assert!(xml.contains("link=\"https://github.com/acme/polyglot-shop/blob/"), "{xml}");
    // Boundaries are real parents, so dragging one moves what it contains.
    assert!(xml.contains("parent=\"nunki-platform\""), "{xml}");
    // `&` in a label has to be escaped or the file will not parse.
    assert!(!xml.contains("reads & writes"), "raw ampersand in XML:\n{xml}");

    // Well-formed: an unescaped character anywhere makes draw.io refuse the file.
    let mut depth = 0i32;
    for tag in xml.split('<').skip(1) {
        if tag.starts_with('/') {
            depth -= 1;
        } else if !tag.starts_with('?') && !tag.contains("/>") {
            depth += 1;
        }
    }
    assert_eq!(depth, 0, "tags balance");

    let o = nunki(&["export", ir.to_str().unwrap()], &repo);
    assert!(o.status.success(), "{}", text(&o));
    assert!(repo.join("docs/architecture/diagrams/containers.drawio").is_file(), "{}", text(&o));
}

/// Authored prose is the one thing in a book nunki does not derive, and until
/// now the one thing it never checked. Two ways it rots, both silent before
/// this: the operation it describes is renamed, so the lookup misses and the
/// prose simply stops appearing; or the code it describes moves, so the prose
/// stays on the page and quietly stops being true.
#[test]
fn journey_authored_prose_is_reported_when_it_goes_stale() {
    let (_tmp, repo) = shop_repo();
    assert!(nunki(&["generate", "."], &repo).status.success());
    let out = repo.join("docs/architecture");
    let authored_path = out.join("authored.json");

    // The generator writes the starter file listing every operation, untouched.
    let mut authored: Value = serde_json::from_str(&std::fs::read_to_string(&authored_path).unwrap()).unwrap();
    let ops = authored["operations"].as_object().unwrap();
    let real_id = ops.keys().next().expect("the demo has operations").clone();

    // An untouched template must not be reported: every operation is listed
    // there with empty answers, and none of it is a claim about anything.
    let o = nunki(&["check", "."], &repo);
    assert!(o.status.success(), "an untouched authored.json is not stale: {}", text(&o));

    // 1. Prose about an operation that no longer exists.
    authored["operations"]["payments:POST /charges/vanished"] = serde_json::json!({
        "name": "Refund a charge",
        "actor": "Back-office clerk",
        "purpose": "Return money for a disputed order.",
        "acceptance": []
    });
    std::fs::write(&authored_path, serde_json::to_string_pretty(&authored).unwrap()).unwrap();
    // Regenerate first, so the book is current and the only thing left to fail
    // on is the orphan itself. Without this the check fails because an authored
    // actor changed the rendered page — which would pass this test for the
    // wrong reason, and did until reverting the orphan rule did not break it.
    assert!(nunki(&["generate", "."], &repo).status.success());
    let o = nunki(&["check", "."], &repo);
    let s = text(&o);
    assert!(
        !s.contains("outdated ") && !s.contains("missing "),
        "the book itself must be current, so the orphan is the only thing failing: {s}"
    );
    assert_eq!(o.status.code(), Some(1), "orphaned authored prose fails check: {s}");
    assert!(s.contains("payments:POST /charges/vanished"), "and names the entry: {s}");
    assert!(s.contains("names no operation"), "and says why: {s}");

    // 2. Prose pinned to code, where the code still matches.
    authored["operations"].as_object_mut().unwrap().remove("payments:POST /charges/vanished");
    authored["operations"][&real_id] = serde_json::json!({
        "name": "Place an order",
        "actor": "Shopper",
        "purpose": "Take payment and start fulfilment.",
        "acceptance": [],
        "evidence": ["api-gateway/src/routes/checkout.ts:1-3"]
    });
    std::fs::write(&authored_path, serde_json::to_string_pretty(&authored).unwrap()).unwrap();
    assert!(nunki(&["generate", "."], &repo).status.success());
    let o = nunki(&["check", "."], &repo);
    assert!(o.status.success(), "a pin that matches the code is fine: {}", text(&o));

    // 3. The pinned lines change. The prose is still on the page, so the book
    //    is still "up to date" — but it now cites code that moved, which is
    //    exactly the state this issue exists to surface.
    let routes = repo.join("api-gateway/src/routes/checkout.ts");
    let body = std::fs::read_to_string(&routes).unwrap();
    // Not committed: evidence is pinned to a commit, so the drift a reader
    // cares about is the working tree no longer matching what was cited.
    std::fs::write(&routes, format!("// a new line at the top, shifting everything\n{body}")).unwrap();
    let o = nunki(&["check", "."], &repo);
    let s = text(&o);
    assert_eq!(o.status.code(), Some(1), "a drifted pin fails check: {s}");
    // The exact pinned range, not merely "some citation went stale" — shifting
    // a real source file also moves the book's own citations into it, which
    // would satisfy a looser assertion whether or not pins are cited at all.
    assert!(
        s.contains("api-gateway/src/routes/checkout.ts:1-3"),
        "the authored pin itself is reported, as a citation like any other: {s}"
    );

    // 4. Prose with no pin is published and listed, never failed on: an
    //    authored.json written before pinning existed must still pass.
    std::fs::write(&routes, body).unwrap();
    authored["operations"][&real_id]["evidence"] = serde_json::json!([]);
    std::fs::write(&authored_path, serde_json::to_string_pretty(&authored).unwrap()).unwrap();
    assert!(nunki(&["generate", "."], &repo).status.success());
    let o = nunki(&["check", "."], &repo);
    let s = text(&o);
    assert!(o.status.success(), "unpinned prose does not fail: {s}");
    assert!(s.contains("no evidence pin"), "but the gap is stated: {s}");

    // 5. The starter file lists every operation with empty answers. When a
    //    route goes, its untouched entry is orphaned but says nothing, and
    //    failing CI over a blank the generator itself wrote would be absurd.
    authored["operations"]["payments:POST /removed-but-never-filled-in"] =
        serde_json::json!({ "name": "", "actor": "", "purpose": "", "acceptance": [] });
    std::fs::write(&authored_path, serde_json::to_string_pretty(&authored).unwrap()).unwrap();
    assert!(nunki(&["generate", "."], &repo).status.success());
    let o = nunki(&["check", "."], &repo);
    let s = text(&o);
    assert!(o.status.success(), "an orphaned but empty entry says nothing, so it is not stale: {s}");
    assert!(!s.contains("removed-but-never-filled-in"), "and is not reported: {s}");
}

/// A requirement identifier is a reference. People put it in a commit message,
/// a ticket and a test name, and the architecture history will put it in a
/// changelog entry that outlives the release.
///
/// It used to be a position in a list: `FR-{:03}` straight from `enumerate()`.
/// Adding one endpoint to the first service renumbered five of the demo's six
/// requirements — so every identifier still *existed*, and every one of them
/// had quietly come to mean a different requirement. That is what this checks:
/// not that the strings survive, but that they still point at the same thing.
#[test]
fn journey_requirement_ids_survive_adding_a_requirement() {
    let (_tmp, repo) = shop_repo();
    assert!(nunki(&["generate", "."], &repo).status.success());
    let spec = repo.join("docs/architecture/pages/07-functional-specification.md");

    // id -> what it describes. `### FR-x · Subject` and `| **BR-x** | Statement |`.
    let subjects = |text: &str| -> std::collections::BTreeMap<String, String> {
        let mut m = std::collections::BTreeMap::new();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("### FR-") {
                let (id, subject) = rest.split_once(" · ").unwrap_or((rest, ""));
                m.insert(format!("FR-{id}"), subject.trim().to_string());
            }
            if let Some(rest) = line.strip_prefix("| **BR-") {
                if let Some((id, tail)) = rest.split_once("** | ") {
                    let statement = tail.split(" | ").next().unwrap_or("").trim().to_string();
                    m.insert(format!("BR-{id}"), statement);
                }
            }
        }
        m
    };

    let before = subjects(&std::fs::read_to_string(&spec).unwrap());
    assert!(before.keys().filter(|k| k.starts_with("FR-")).count() >= 5, "{before:?}");
    assert!(before.keys().any(|k| k.starts_with("BR-")), "{before:?}");

    // A new endpoint on the first service, which is where it does most damage:
    // every requirement sorted after it used to shift by one.
    let catalog = repo.join("api-gateway/src/routes/catalog.ts");
    let src = std::fs::read_to_string(&catalog).unwrap();
    let marker = "catalogRouter.get(\"/\"";
    assert!(src.contains(marker), "fixture shape changed");
    // Guarded, so it adds a business rule too — otherwise the rule ordering
    // never moves and a positional BR identifier would survive by luck.
    let added = format!(
        "catalogRouter.get(\"/brands\", requireCustomer, async (_req, res) => {{\n  res.json([]);\n}});\n\n{marker}"
    );
    let src = src.replacen(marker, &added, 1).replace(
        "import { cached } from \"../cache\";",
        "import { cached } from \"../cache\";\nimport { requireCustomer } from \"./auth\";",
    );
    std::fs::write(&catalog, src).unwrap();
    git(&repo, &["commit", "-aqm", "add a brands endpoint"]);
    assert!(nunki(&["generate", "."], &repo).status.success());

    let after = subjects(&std::fs::read_to_string(&spec).unwrap());
    assert!(after.len() > before.len(), "the new endpoint is a new requirement");

    // Every identifier that existed before still describes what it described.
    let moved: Vec<String> = before
        .iter()
        .filter_map(|(id, was)| match after.get(id) {
            Some(now) if now == was => None,
            Some(now) => Some(format!("{id}: was {was:?}, now {now:?}")),
            None => Some(format!("{id}: gone (was {was:?})")),
        })
        .collect();
    assert!(moved.is_empty(), "identifiers now point somewhere else:\n  {}", moved.join("\n  "));
}

/// Checking code against a specification someone else wrote.
///
/// The fixture reproduces what a real repository actually looks like — it was
/// built from `trilogy-group/ttv-pipeline`, whose Kiro spec declares five
/// endpoints against an implementation that has seven. Two traps in that pair
/// are the whole difficulty, and neither appears in a fixture designed to
/// succeed: the spec writes `{id}` where the code writes `{job_id}`, and most
/// of a real spec declares no endpoint at all.
#[test]
fn journey_conform_checks_code_against_a_specification() {
    let (_tmp, repo) = repo_from("conform", "jobs", "git@github.com:acme/jobs.git");
    let o = nunki(&["conform", "."], &repo);
    let s = text(&o);
    assert!(o.status.success(), "reporting gaps is not itself a failure: {s}");

    // Declared and implemented. `{id}` and `{job_id}` are the same route; a
    // literal comparison would report this endpoint as both missing and
    // unrequested, which is two false findings for working code.
    assert!(s.contains("2 matched"), "{s}");
    assert!(
        !s.contains("no operation answers GET /v1/jobs/{}  "),
        "the status endpoint matched despite `{{id}}` vs `{{job_id}}`, so it must not also be reported missing: {s}"
    );

    // Declared and absent.
    assert!(s.contains("no operation answers GET /v1/jobs/{}/artifact"), "{s}");
    assert!(s.contains("no operation answers POST /v1/jobs/{}/cancel"), "{s}");

    // Declared with a status the handler was never measured to return.
    assert!(s.contains("declares 429"), "{s}");
    assert!(s.contains("measured to return 202, 400"), "the measured statuses are named, not just the gap: {s}");

    // Implemented and asked for by nobody — the finding a spec-first tool
    // cannot produce, because it only looks where its own artifacts point.
    assert!(s.contains("/v1/plans is implemented and declared nowhere"), "{s}");
    assert!(s.contains("artifact-url is implemented and declared nowhere"), "{s}");

    // And the one that matters most: a requirement about load shedding and
    // restarts names no endpoint. Reporting it as missing would be the
    // loudest possible false positive, and wrong about every non-endpoint
    // requirement in every specification ever written.
    assert!(s.contains("Requirement 3 names no endpoint"), "{s}");
    assert!(s.contains("names `/healthz` with no method"), "a path with no verb is undecided, not missing: {s}");
    assert!(!s.contains("missing      Requirement 3"), "{s}");
    assert!(!s.contains("no operation answers  /healthz"), "{s}");

    // Every finding about our side carries a citation back to the code.
    let json = nunki(&["conform", ".", "--json"], &repo);
    let report: Value = serde_json::from_slice(&json.stdout).unwrap();
    for f in report["findings"].as_array().unwrap() {
        if f["verdict"] == "unrequested" || f["verdict"] == "matched" || f["verdict"] == "partial" {
            assert!(f["evidence"]["filePath"].as_str().is_some_and(|p| !p.is_empty()), "{f}");
            assert!(f["requirement"].as_str().is_some_and(|r| r.starts_with("FR-")), "{f}");
        }
        if f["verdict"] == "missing" || f["verdict"] == "partial" {
            assert!(f["declared"]["source"].as_str().is_some(), "a gap says where it was declared: {f}");
        }
    }

    // `--exit-code` gates a pipeline on what is actually actionable: something
    // declared and absent, or contradicted. Not on scope judgements.
    let gated = nunki(&["conform", ".", "--exit-code"], &repo);
    assert_eq!(gated.status.code(), Some(1), "{}", text(&gated));

    // Nothing to read is a usage error, not a clean bill of health.
    let (_t2, empty) = repo_from("go-mini", "gm", "git@github.com:acme/gm.git");
    let none = nunki(&["conform", "."], &empty);
    assert_eq!(none.status.code(), Some(2), "{}", text(&none));
    assert!(text(&none).contains(".kiro/specs"), "it says where it looked: {}", text(&none));
}

/// How the architecture changed, release by release.
///
/// `diff` has always been able to answer this between any two tags, and every
/// answer was discarded. What makes the recorded form worth having is that it
/// is written once: a rebuild must say what the release said, not what a
/// rebuild would conclude now.
#[test]
fn journey_architecture_history_is_recorded_once_and_never_recomputed() {
    let (_tmp, repo) = shop_repo();
    git(&repo, &["tag", "v1.0.0"]);

    // A release that adds an endpoint.
    let catalog = repo.join("api-gateway/src/routes/catalog.ts");
    let src = std::fs::read_to_string(&catalog).unwrap();
    let marker = "catalogRouter.get(\"/\"";
    std::fs::write(
        &catalog,
        src.replacen(
            marker,
            &format!("catalogRouter.get(\"/brands\", async (_req, res) => {{ res.json([]); }});\n\n{marker}"),
            1,
        ),
    )
    .unwrap();
    git(&repo, &["commit", "-aqm", "add a brands endpoint"]);
    git(&repo, &["tag", "v1.1.0"]);

    assert!(nunki(&["generate", "."], &repo).status.success());
    let out = repo.join("docs/architecture");
    let page = out.join("pages/09-architecture-history.md");
    assert!(!page.exists(), "no history recorded yet, so no page invented");

    // Record the release.
    let o = nunki(&["diff", ".", "--base", "v1.0.0", "--head", "v1.1.0", "--record", "v1.1.0"], &repo);
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("recorded v1.1.0"), "{}", text(&o));

    assert!(nunki(&["generate", "."], &repo).status.success());
    let rendered = std::fs::read_to_string(&page).unwrap();
    assert!(rendered.contains("## v1.1.0"), "{rendered}");
    assert!(rendered.contains("GET /catalog/brands"), "the change itself, not just a count: {rendered}");
    assert!(rendered.contains("1 change against v1.0.0"), "{rendered}");

    // The two commits it was computed between, so a reader can repeat it.
    let history: Value = serde_json::from_str(&std::fs::read_to_string(out.join("history.json")).unwrap()).unwrap();
    let entry = &history["entries"][0];
    assert_eq!(entry["release"], "v1.1.0");
    assert!(entry["diff"]["baseCommit"].as_str().is_some_and(|c| c.len() >= 7), "{entry}");
    assert!(entry["diff"]["headCommit"].as_str().is_some_and(|c| c.len() >= 7), "{entry}");

    // The book says which release it documents, not only which commit.
    let manifest: Value = serde_json::from_str(&std::fs::read_to_string(out.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["release"], "v1.1.0", "generated from a tagged commit");

    // Never recomputed. The code moves on; the entry does not.
    let before = std::fs::read_to_string(out.join("history.json")).unwrap();
    std::fs::write(&catalog, "export const catalogRouter = {};\n").unwrap();
    git(&repo, &["commit", "-aqm", "gut the router"]);
    assert!(nunki(&["generate", "."], &repo).status.success());
    assert_eq!(
        std::fs::read_to_string(out.join("history.json")).unwrap(),
        before,
        "a rebuild rewrote what a release recorded"
    );
    assert!(
        std::fs::read_to_string(&page).unwrap().contains("GET /catalog/brands"),
        "the page still says what the release said"
    );

    // Past the tag, the book stops claiming to be that release.
    let manifest: Value = serde_json::from_str(&std::fs::read_to_string(out.join("manifest.json")).unwrap()).unwrap();
    assert!(manifest["release"].is_null(), "three commits after v1.1.0 is not v1.1.0: {manifest}");

    // Re-recording a release replaces its entry and leaves the rest alone.
    git(&repo, &["tag", "-f", "v1.1.0"]);
    let again = nunki(&["diff", ".", "--base", "v1.0.0", "--head", "v1.1.0", "--record", "v1.1.0"], &repo);
    assert!(text(&again).contains("replaced v1.1.0"), "{}", text(&again));
    let history: Value = serde_json::from_str(&std::fs::read_to_string(out.join("history.json")).unwrap()).unwrap();
    assert_eq!(history["entries"].as_array().unwrap().len(), 1, "replaced, not appended twice");
}
