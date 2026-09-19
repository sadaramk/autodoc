//! Integration: evidence verification against a real git repository.

use std::path::Path;
use std::process::Command;

use autodoc_git::*;

fn sh(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().expect("git runs");
    assert!(out.status.success(), "git {:?}: {}", args, String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn init_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    sh(p, &["init", "-q", "-b", "main"]);
    sh(p, &["config", "user.email", "test@example.com"]);
    sh(p, &["config", "user.name", "Test"]);
    sh(p, &["remote", "add", "origin", "git@github.com:acme/shop.git"]);
    std::fs::create_dir_all(p.join("svc/src")).unwrap();
    let body: String = (1..=20).map(|i| format!("fn f{i}() {{}}\n")).collect();
    std::fs::write(p.join("svc/src/lib.rs"), body).unwrap();
    sh(p, &["add", "."]);
    sh(p, &["commit", "-q", "-m", "init"]);
    dir
}

fn q(file: &str, line: u32, end: u32, sym: Option<&str>) -> EvidenceQuery {
    EvidenceQuery { file_path: file.into(), line, end_line: Some(end), symbol_name: sym.map(Into::into) }
}

#[test]
fn context_reports_commit_branch_remote_and_prefix() {
    let repo = init_repo();
    let ctx = repo_context(&repo.path().join("svc"));
    assert!(ctx.is_git());
    assert_eq!(ctx.prefix, "svc");
    assert_eq!(ctx.branch.as_deref(), Some("main"));
    assert_eq!(ctx.head_commit.as_ref().unwrap().len(), 40);
    let link = ctx.permalink("src/lib.rs", 2, 3, None);
    assert!(link.starts_with("https://github.com/acme/shop/blob/"), "{link}");
    assert!(link.ends_with("/svc/src/lib.rs#L2-L3"), "{link}");
}

#[test]
fn pinned_evidence_goes_stale_only_when_its_lines_change() {
    let repo = init_repo();
    let ctx = repo_context(repo.path());
    let pinned = ctx.head_commit.clone().unwrap();

    let before = verify_evidence(&ctx, &q("svc/src/lib.rs", 2, 4, Some("f3")), Some(&pinned));
    assert_eq!(before.state, EvidenceState::Verified, "{}", before.detail);

    // Touch line 15 only: evidence at 2-4 must stay verified.
    let path = repo.path().join("svc/src/lib.rs");
    let edited = std::fs::read_to_string(&path).unwrap().replace("fn f15() {}", "fn f15() { todo!() }");
    std::fs::write(&path, edited).unwrap();
    let ctx = repo_context(repo.path());
    assert_eq!(ctx.dirty_files, vec!["svc/src/lib.rs".to_string()]);
    assert_eq!(verify_evidence(&ctx, &q("svc/src/lib.rs", 2, 4, None), Some(&pinned)).state, EvidenceState::Verified);

    let stale = verify_evidence(&ctx, &q("svc/src/lib.rs", 14, 16, None), Some(&pinned));
    assert_eq!(stale.state, EvidenceState::Stale);
    assert_eq!(stale.changed_ranges, vec![(15, 15)]);

    assert_eq!(changed_files_since(&ctx, &pinned).unwrap(), vec!["svc/src/lib.rs".to_string()]);
}

#[test]
fn untracked_files_and_unknown_commits_are_flagged() {
    let repo = init_repo();
    std::fs::write(repo.path().join("new.rs"), "fn x() {}\n").unwrap();
    let ctx = repo_context(repo.path());
    assert_eq!(verify_evidence(&ctx, &q("new.rs", 1, 1, None), None).state, EvidenceState::Untracked);
    let bogus = verify_evidence(&ctx, &q("svc/src/lib.rs", 1, 1, None), Some("deadbeefdeadbeef"));
    assert_eq!(bogus.state, EvidenceState::Stale);
    assert!(bogus.detail.contains("not in this repository"));
}

/// A repository's own `.git/config` can name a command for git to run —
/// `core.fsmonitor` turns an innocent `git status` into arbitrary execution as
/// whoever ran autodoc. Config travels with a tarball, an archive or a vendored
/// copy, and documenting code you did not write is the whole point of the tool,
/// so every invocation must override the keys that can execute something.
#[test]
fn a_repository_cannot_make_git_run_its_own_command() {
    let dir = init_repo();
    let p = dir.path();
    let marker = p.join("EXECUTED");
    let payload = format!("touch {}; false", marker.display());

    for key in ["core.fsmonitor", "diff.external"] {
        sh(p, &["config", key, &payload]);
    }
    std::fs::write(p.join("svc/src/lib.rs"), "fn f1() {}\nfn changed() {}\n").unwrap();

    // Everything the analyzer does against a repository, on the hostile config.
    let ctx = repo_context(p);
    let _ = ctx.head_commit.clone();
    let _ = tracked_at_head(&ctx, "svc/src/lib.rs");
    let _ = changed_files_since(&ctx, "HEAD");
    let _ = changed_ranges(&ctx, "HEAD", "svc/src/lib.rs");
    let _ = commit_describing(&ctx, Some("docs".into()));

    assert!(!marker.exists(), "a repository's git config executed a command of its choosing");
}

/// A citation can name a path that stays inside the repository by spelling but
/// leaves it through a symlinked directory. The verifier quotes the file it
/// finds into the book as a snippet, and books get committed and published, so
/// resolution has to end inside the repository and not merely look like it.
#[test]
fn a_symlinked_directory_cannot_lead_evidence_out_of_the_repository() {
    let dir = init_repo();
    let p = dir.path();
    let outside = dir.path().parent().unwrap().join("outside-secrets");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("credentials"), "aws_secret_access_key = hunter2\n").unwrap();

    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, p.join("vendor")).unwrap();
    #[cfg(not(unix))]
    return;

    assert!(resolve_in_repo(p, "svc/src/lib.rs").is_some(), "an ordinary path still resolves");
    assert_eq!(resolve_in_repo(p, "vendor/credentials"), None, "a symlink led evidence out of the repository");
    assert_eq!(resolve_in_repo(p, "../outside-secrets/credentials"), None, "plain traversal is still refused");
}
