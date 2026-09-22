//! Journey: documenting a system that spans several repositories.
//!
//! Two checkouts, one book. The gateway calls a notification service that lives
//! next door; configured as a workspace member, the call resolves, its citation
//! names the other repository, and its permalink points into *that* repository
//! at *that* repository's commit. When the member moves, the book is out of date
//! and `check` says so — the cost of documenting a system rather than a
//! repository, made visible rather than hidden.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

fn head(repo: &Path) -> String {
    let o = Command::new("git").arg("-C").arg(repo).args(["rev-parse", "HEAD"]).output().unwrap();
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

/// The gateway and the service it calls, as two sibling git repositories.
fn system() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let mut roots = Vec::new();
    for (fixture, name) in [("cross-repo", "gateway"), ("notifications", "notifications")] {
        let repo = tmp.path().join(name);
        copy_dir(&fixtures().join(fixture), &repo);
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["remote", "add", "origin", &format!("git@github.com:acme/{name}.git")]);
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "init"]);
        roots.push(repo);
    }
    let (gateway, notifications) = (roots[0].clone(), roots[1].clone());
    std::fs::write(gateway.join("nunki.toml"), "[workspace]\nmembers = [\"../notifications\"]\n").unwrap();
    git(&gateway, &["add", "."]);
    git(&gateway, &["commit", "-qm", "configure the workspace"]);
    (tmp, gateway, notifications)
}

/// The book's own data, as the reader gets it: embedded in `index.html`.
fn book_json(gateway: &Path) -> serde_json::Value {
    let html = std::fs::read_to_string(gateway.join("docs/architecture/index.html")).unwrap();
    let open = r#"<script type="application/json" id="book-data">"#;
    let start = html.find(open).expect("the book data is embedded in the page") + open.len();
    let end = start + html[start..].find("</script>").expect("unterminated book data");
    serde_json::from_str(&html[start..end]).expect("the embedded book data is JSON")
}

#[test]
fn journey_a_system_documented_from_two_repositories() {
    let (_tmp, gateway, notifications) = system();

    let o = nunki(&["generate", "."], &gateway);
    assert!(o.status.success(), "{}", text(&o));

    let book = book_json(&gateway);
    let member_commit = head(&notifications);

    // The book records what it read, so a reader can reproduce it.
    assert_eq!(book["meta"]["members"][0]["name"], "notifications", "{}", book["meta"]);
    assert_eq!(book["meta"]["members"][0]["commit"], member_commit, "read at the member's commit");

    // Citations into the member name it and link into it — not into this
    // repository, where the same path holds different code.
    let cites = book["cites"].as_object().expect("cites");
    let from_member: Vec<&serde_json::Value> = cites.values().filter(|c| c["repo"] == "notifications").collect();
    assert!(!from_member.is_empty(), "the member's lines are cited: {:?}", cites.keys().collect::<Vec<_>>());
    for c in &from_member {
        let link = c["permalink"].as_str().unwrap_or_default();
        assert!(
            link.starts_with("https://github.com/acme/notifications/blob/"),
            "a member citation links into the member's repository: {c}"
        );
        assert!(link.contains(&member_commit), "at the commit it was read at: {c}");
        assert_eq!(c["state"], "verified", "and is verified there: {c}");
    }

    // Nothing of ours was relabelled.
    assert!(
        cites.values().any(|c| c["repo"].is_null() && c["file"].as_str().unwrap_or("").starts_with("src/")),
        "this repository's own citations stay unqualified"
    );

    // Generated twice in a row, nothing changes: the second repository does not
    // make the book non-deterministic.
    let o = nunki(&["check", "."], &gateway);
    assert!(o.status.success(), "{}", text(&o));

    // The member moves. The book describes lines at a commit that is no longer
    // what is checked out, so it is out of date — and says which repository.
    std::fs::write(
        notifications.join("src/server.ts"),
        format!(
            "// A licence header nobody thought about.\n{}",
            std::fs::read_to_string(notifications.join("src/server.ts")).unwrap()
        ),
    )
    .unwrap();
    git(&notifications, &["add", "."]);
    git(&notifications, &["commit", "-qm", "add a header"]);

    let o = nunki(&["check", "."], &gateway);
    assert_eq!(o.status.code(), Some(1), "a member at an unexpected commit fails the check: {}", text(&o));
    let out = text(&o);
    assert!(
        out.contains("member    notifications:") && out.contains("moved since this book was built"),
        "the report names the repository that moved, not only the files it affected: {out}"
    );

    // Regenerating takes the new commit, and the book is consistent again.
    let o = nunki(&["generate", "."], &gateway);
    assert!(o.status.success(), "{}", text(&o));
    let book = book_json(&gateway);
    assert_eq!(book["meta"]["members"][0]["commit"], head(&notifications));
    let o = nunki(&["check", "."], &gateway);
    assert!(o.status.success(), "{}", text(&o));
}

/// A member named in `nunki.toml` that is not checked out.
///
/// The common case in CI: one repository cloned, the others not. It must not
/// fail the build — a book that reports the call as unresolved is the honest
/// answer — but it must be said out loud.
#[test]
fn journey_a_member_that_is_not_cloned_is_reported_and_does_not_fail_generation() {
    let (_tmp, gateway, notifications) = system();
    std::fs::remove_dir_all(&notifications).unwrap();

    let o = nunki(&["generate", "."], &gateway);
    assert!(o.status.success(), "generation still succeeds: {}", text(&o));
    let out = text(&o);
    assert!(out.contains("../notifications"), "and says the member was not read: {out}");

    let book = book_json(&gateway);
    assert!(book["meta"]["members"].as_array().is_none_or(|m| m.is_empty()), "nothing is claimed about it");
    let cites = book["cites"].as_object().unwrap();
    assert!(cites.values().all(|c| c["repo"].is_null()), "and no citation claims to come from it");
}
