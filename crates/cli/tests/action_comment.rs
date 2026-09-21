//! The action's pull-request comment, driven as GitHub drives it: the real
//! `scripts/action-comment.sh` against a stubbed `nunki` and a stubbed `gh`,
//! asserting the API calls it makes.
//!
//! The behaviour worth testing is not that a comment appears — it is that a
//! second push edits the first comment rather than adding another, and that an
//! architecture which stops differing takes its comment back down. Both are
//! invisible in a single run, so neither is covered by using the action once.

use std::path::{Path, PathBuf};
use std::process::Command;

fn script() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/action-comment.sh")).to_path_buf()
}

struct Run {
    /// `gh` invocations, delimited by the stub.
    log: String,
    /// The script's own output, plus the arguments the stubbed `nunki` was handed.
    stdout: String,
    stderr: String,
    code: i32,
}

/// Arguments a run is given and the world it runs in.
struct Case<'a> {
    /// Exit status of `nunki diff`: 0 unchanged, 1 changed, 2 a real failure.
    diff_exit: i32,
    /// What `nunki diff` prints.
    markdown: &'a str,
    /// Comment id the stubbed `gh` reports as already on the pull request.
    existing: Option<&'a str>,
    pr: Option<&'a str>,
    base_sha: Option<&'a str>,
    /// Everything after the binary, exactly as the entrypoint hands it over:
    /// the repository path first, then whatever `args:` supplied.
    args: &'a [&'a str],
}

impl Default for Case<'_> {
    fn default() -> Self {
        Case {
            diff_exit: 1,
            markdown: "### Architecture changes\n\n- added **payments**\n",
            existing: None,
            pr: Some("7"),
            base_sha: Some("bbbbbbb"),
            args: &["."],
        }
    }
}

fn write_stub(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

fn run(c: Case) -> Run {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = tmp.path().join("gh.log");
    let diff_log = tmp.path().join("diff.log");

    // `gh` records every invocation and answers the one query the script makes.
    // It deliberately does not parse `--jq`: what is under test is the script's
    // control flow, and `gh`'s own jq is not ours to re-implement.
    write_stub(
        &bin.join("gh"),
        &format!(
            r#"#!/usr/bin/env bash
printf '<<<call>>>%s' "$*" >>"{log}"
for a in "$@"; do
  case "$a" in
    */comments) printf '%s\n' "{existing}" ;;
  esac
done
exit 0
"#,
            log = log.display(),
            existing = c.existing.unwrap_or(""),
        ),
    );

    // The stub `nunki` records the arguments it was handed, so the defaulting of
    // `--base` and `--exit-code` is asserted on rather than assumed.
    let nunki = tmp.path().join("nunki");
    write_stub(
        &nunki,
        &format!(
            r#"#!/usr/bin/env bash
printf '%s\n' "$*" >>"{diff_log}"
cat <<'MARKDOWN_EOF'
{markdown}
MARKDOWN_EOF
exit {exit_code}
"#,
            diff_log = diff_log.display(),
            markdown = c.markdown.trim_end(),
            exit_code = c.diff_exit,
        ),
    );

    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
    let mut cmd = Command::new("bash");
    cmd.arg(script())
        .arg(&nunki)
        .args(c.args)
        .env("PATH", path)
        .env("GITHUB_REPOSITORY", "sadaramk/nunki")
        .env("GH_TOKEN", "x");
    match c.pr {
        Some(n) => cmd.env("NUNKI_PR", n),
        None => cmd.env_remove("NUNKI_PR"),
    };
    match c.base_sha {
        Some(s) => cmd.env("NUNKI_BASE_SHA", s),
        None => cmd.env_remove("NUNKI_BASE_SHA"),
    };
    let out = cmd.output().unwrap();
    Run {
        log: std::fs::read_to_string(&log).unwrap_or_default(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned()
            + &std::fs::read_to_string(&diff_log).unwrap_or_default(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code().unwrap_or(-1),
    }
}

/// `gh` calls the script chose to make, with the read that every run performs
/// dropped. Split on the stub's delimiter, not on lines: a body spans several.
fn writes(r: &Run) -> Vec<&str> {
    r.log.split("<<<call>>>").filter(|c| !c.is_empty()).filter(|c| !c.contains("--paginate")).collect()
}

const MARKER: &str = "<!-- nunki:architecture-diff -->";

#[test]
fn the_first_push_posts_a_comment_that_later_runs_can_find() {
    let r = run(Case::default());
    assert_eq!(r.code, 0, "commenting is not a verdict: {}", r.stderr);
    let w = writes(&r);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(
        w[0].starts_with("api --method POST repos/sadaramk/nunki/issues/7/comments "),
        "a comment on the pull request, not a review or a commit comment: {:?}",
        w[0]
    );
    // The marker has to lead the body: the next run looks for it with
    // `startswith`, and a comment it cannot find is a comment it duplicates.
    let body = w[0].split_once("-f body=").unwrap().1;
    assert!(body.starts_with(MARKER), "the body must open with the marker: {body:?}");
    assert!(body.contains("added **payments**"), "and carry the diff: {body:?}");
}

#[test]
fn the_second_push_edits_that_comment_instead_of_adding_another() {
    let r = run(Case { existing: Some("4242"), ..Case::default() });
    assert_eq!(r.code, 0, "{}", r.stderr);
    let w = writes(&r);
    assert_eq!(w.len(), 1, "exactly one write: {w:?}");
    assert!(w[0].starts_with("api --method PATCH repos/sadaramk/nunki/issues/comments/4242 "), "{:?}", w[0]);
    assert!(!w[0].contains("POST"), "a ten-commit branch must not leave ten comments: {:?}", w[0]);
}

#[test]
fn an_architecture_that_stops_differing_takes_its_comment_back_down() {
    let r =
        run(Case { diff_exit: 0, markdown: "### Architecture unchanged\n", existing: Some("4242"), ..Case::default() });
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        writes(&r),
        ["api --method DELETE repos/sadaramk/nunki/issues/comments/4242 --silent"],
        "a branch that added a route and reverted it must not keep claiming the route"
    );
}

#[test]
fn nothing_is_said_when_nothing_changed_and_nothing_was_said_before() {
    let r = run(Case { diff_exit: 0, markdown: "### Architecture unchanged\n", ..Case::default() });
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(writes(&r).is_empty(), "no comment on every unchanged pull request: {:?}", writes(&r));
    assert!(r.stdout.contains("saying nothing"), "but the log says why: {}", r.stdout);
}

/// `diff` defaults to `HEAD~1`, which describes the last push rather than the
/// branch — on a pull request that is the wrong question.
#[test]
fn the_base_defaults_to_the_commit_the_pull_request_merges_into() {
    let r = run(Case::default());
    assert!(r.stdout.contains("diff . --base bbbbbbb"), "path first, then the event's base sha: {}", r.stdout);
    assert!(r.stdout.contains("--exit-code"), "and --exit-code, to tell the three outcomes apart: {}", r.stdout);
}

#[test]
fn an_explicit_base_is_not_overridden() {
    let r = run(Case { args: &[".", "--base", "v1.0.0"], ..Case::default() });
    assert!(r.stdout.contains("--base v1.0.0"), "{}", r.stdout);
    assert!(!r.stdout.contains("bbbbbbb"), "the fallback must not be appended as well: {}", r.stdout);
}

/// A caller who asked for `--exit-code` wanted an architecture change to fail
/// the job; one who did not wanted a comment and a green build.
#[test]
fn only_a_caller_who_asked_for_exit_code_gets_a_failing_job() {
    let commented = run(Case::default());
    assert_eq!(commented.code, 0);
    assert_eq!(writes(&commented).len(), 1, "but it still commented");

    let gated = run(Case { args: &[".", "--exit-code"], ..Case::default() });
    assert_eq!(gated.code, 1, "an architecture change fails the job when asked");
    assert_eq!(writes(&gated).len(), 1, "and still comments");
}

/// A real failure — an unreachable revision, a scan that blew up — must not be
/// read as "the architecture changed" and posted as a diff.
#[test]
fn a_failing_diff_is_propagated_rather_than_commented() {
    let r = run(Case { diff_exit: 2, markdown: "", ..Case::default() });
    assert_eq!(r.code, 2, "the failure reaches the job");
    assert!(writes(&r).is_empty(), "and nothing is posted: {:?}", writes(&r));
}

#[test]
fn a_missing_pull_request_number_fails_loudly_rather_than_posting_nowhere() {
    let r = run(Case { pr: None, ..Case::default() });
    assert_ne!(r.code, 0, "a push build has no pull request to comment on");
    assert!(r.stderr.contains("pull request"), "the error should say so: {}", r.stderr);
    assert!(writes(&r).is_empty());
}

#[test]
fn a_missing_base_is_reported_rather_than_silently_compared_to_head_1() {
    let r = run(Case { base_sha: None, ..Case::default() });
    assert_ne!(r.code, 0);
    assert!(r.stderr.contains("--base"), "{}", r.stderr);
    assert!(writes(&r).is_empty());
}
