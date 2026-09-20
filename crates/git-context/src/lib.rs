//! Git context: pins diagrams to commits, tracks drift between a pinned commit
//! and the working tree, and verifies that source evidence (file + line range)
//! still exists and still says what it said.
//!
//! Shells out to the `git` binary rather than linking libgit2: it's always
//! present where a repository is, it honours the user's config, and it keeps
//! the build free of native TLS/ssh dependencies.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git is not installed or not on PATH")]
    GitMissing,
    #[error("git {args}: {stderr}")]
    Command { args: String, stderr: String },
}

/// Snapshot of the repository the scan ran against.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RepoContext {
    /// Absolute path the caller asked about.
    pub root: PathBuf,
    /// Top of the enclosing git work tree, when there is one.
    pub git_root: Option<PathBuf>,
    /// `root` relative to `git_root` ("" when they coincide).
    pub prefix: String,
    pub head_commit: Option<String>,
    pub branch: Option<String>,
    pub remote_url: Option<String>,
    /// Paths (relative to `root`) with uncommitted changes.
    pub dirty_files: Vec<String>,
}

impl RepoContext {
    pub fn is_git(&self) -> bool {
        self.git_root.is_some()
    }

    /// `https://github.com/org/repo/blob/<commit>/<path>#L10-L20` when the
    /// remote is a recognised forge, else a portable `path#L10-L20@commit`.
    pub fn permalink(&self, file: &str, start: u32, end: u32, commit: Option<&str>) -> String {
        let commit = commit.or(self.head_commit.as_deref());
        let repo_path = join_prefix(&self.prefix, file);
        let anchor = if start == end { format!("#L{start}") } else { format!("#L{start}-L{end}") };
        match (self.remote_url.as_deref().and_then(web_base), commit) {
            (Some(base), Some(c)) => format!("{base}/blob/{c}/{repo_path}{anchor}"),
            (_, Some(c)) => format!("{repo_path}{anchor}@{}", short(c)),
            _ => format!("{repo_path}{anchor}"),
        }
    }
}

pub fn short(commit: &str) -> &str {
    &commit[..commit.len().min(12)]
}

fn join_prefix(prefix: &str, file: &str) -> String {
    if prefix.is_empty() {
        file.to_string()
    } else {
        format!("{}/{}", prefix.trim_end_matches('/'), file)
    }
}

/// Converts `git@github.com:org/repo.git` / `https://gitlab.com/org/repo.git`
/// into a browsable base URL.
pub fn web_base(remote: &str) -> Option<String> {
    let r = remote.trim().trim_end_matches(".git");
    let hostpath = if let Some(rest) = r.strip_prefix("git@") {
        rest.replacen(':', "/", 1)
    } else if let Some(rest) = r.strip_prefix("ssh://git@") {
        rest.to_string()
    } else {
        let rest = r.strip_prefix("https://").or_else(|| r.strip_prefix("http://"))?;
        rest.split_once('@').map(|(_, h)| h.to_string()).unwrap_or_else(|| rest.to_string())
    };
    let host = hostpath.split('/').next()?;
    let forge = ["github.com", "gitlab.com", "bitbucket.org", "codeberg.org"];
    forge.contains(&host).then(|| format!("https://{hostpath}"))
}

/// Configuration a repository can set to make git run a command of its choosing.
///
/// The repository being documented is untrusted input, and `.git/config` travels
/// with a tarball, a vendored copy or an archive — anything but a fresh clone.
/// `core.fsmonitor` alone turns `git status` into arbitrary code execution as the
/// user running autodoc, so every invocation overrides these keys: `-c` beats
/// repository configuration.
const NO_EXEC: &[&str] = &[
    "-c",
    "core.fsmonitor=false",
    "-c",
    "core.hooksPath=/dev/null",
    "-c",
    "core.pager=cat",
    "-c",
    "core.sshCommand=false",
    "-c",
    "core.alternateRefsCommand=",
    "-c",
    "diff.external=",
    "-c",
    "uploadpack.packObjectsHook=",
    "-c",
    "protocol.ext.allow=never",
];

fn git(dir: &Path, args: &[&str]) -> Result<String, GitError> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(NO_EXEC)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .map_err(|_| GitError::GitMissing)?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(GitError::Command { args: args.join(" "), stderr: String::from_utf8_lossy(&out.stderr).trim().to_string() })
    }
}

fn git_opt(dir: &Path, args: &[&str]) -> Option<String> {
    git(dir, args).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Collects git context for `root`. Never fails: a directory outside git
/// simply yields a context without commit information.
pub fn repo_context(root: &Path) -> RepoContext {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let git_root = git_opt(&root, &["rev-parse", "--show-toplevel"]).map(PathBuf::from);
    let Some(top) = git_root.clone() else {
        return RepoContext {
            root,
            git_root: None,
            prefix: String::new(),
            head_commit: None,
            branch: None,
            remote_url: None,
            dirty_files: vec![],
        };
    };
    let top = top.canonicalize().unwrap_or(top);
    let prefix = root.strip_prefix(&top).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default();
    let dirty_files = git(&root, &["status", "--porcelain", "--untracked-files=no", "--", "."])
        .map(|s| {
            s.lines()
                .filter_map(|l| l.get(3..))
                .map(|p| p.rsplit(" -> ").next().unwrap_or(p))
                .filter_map(|p| strip_repo_prefix(&prefix, p))
                .collect()
        })
        .unwrap_or_default();
    RepoContext {
        head_commit: git_opt(&root, &["rev-parse", "HEAD"]),
        branch: git_opt(&root, &["rev-parse", "--abbrev-ref", "HEAD"]).filter(|b| b != "HEAD"),
        remote_url: git_opt(&root, &["remote", "get-url", "origin"]),
        root,
        git_root: Some(top),
        prefix,
        dirty_files,
    }
}

fn strip_repo_prefix(prefix: &str, path: &str) -> Option<String> {
    if prefix.is_empty() {
        return Some(path.to_string());
    }
    path.strip_prefix(&format!("{}/", prefix.trim_end_matches('/'))).map(str::to_string)
}

/// Line ranges (new-side, 1-based inclusive) touched in `file` between
/// `commit` and the working tree.
pub fn changed_ranges(ctx: &RepoContext, commit: &str, file: &str) -> Result<Vec<(u32, u32)>, GitError> {
    let out =
        git(&ctx.root, &["diff", "--no-color", "--no-textconv", "--no-ext-diff", "--unified=0", commit, "--", file])?;
    Ok(parse_hunks(&out))
}

/// Parses `@@ -a,b +c,d @@` headers. A pure deletion (d == 0) is reported as
/// the single line at which it happened so overlap checks still see it.
pub fn parse_hunks(diff: &str) -> Vec<(u32, u32)> {
    diff.lines()
        .filter_map(|l| l.strip_prefix("@@ "))
        .filter_map(|l| l.split(' ').find(|t| t.starts_with('+')))
        .filter_map(|t| {
            let t = &t[1..];
            let (start, len) = match t.split_once(',') {
                Some((s, n)) => (s.parse::<u32>().ok()?, n.parse::<u32>().ok()?),
                None => (t.parse::<u32>().ok()?, 1),
            };
            Some(if len == 0 { (start.max(1), start.max(1)) } else { (start, start + len - 1) })
        })
        .collect()
}

/// Committer date of `commit` in strict ISO 8601, for reproducible timestamps.
pub fn commit_date(ctx: &RepoContext, commit: &str) -> Option<String> {
    git_opt(&ctx.root, &["show", "-s", "--format=%cI", commit])
}

pub fn commit_exists(ctx: &RepoContext, commit: &str) -> bool {
    git(&ctx.root, &["cat-file", "-e", &format!("{commit}^{{commit}}")]).is_ok()
}

/// The last commit that changed anything outside `exclude`, relative to the
/// repository root.
///
/// A book records the commit it describes, and committing the book itself would
/// otherwise move that commit — leaving the documentation permanently one commit
/// behind, and `check` failing however many times it is regenerated. What the
/// documentation describes is the last commit that touched something it reads.
pub fn commit_describing(ctx: &RepoContext, exclude: Option<String>) -> Option<String> {
    let head = ctx.head_commit.clone();
    let specs: Vec<String> = exclude
        .iter()
        .flat_map(|e| e.split(','))
        .map(str::trim)
        .filter(|r| !r.is_empty() && !r.starts_with(".."))
        .map(|r| format!(":(exclude){}", r.trim_end_matches('/')))
        .collect();
    if specs.is_empty() {
        return head;
    }
    let mut args: Vec<&str> = vec!["log", "-1", "--format=%H", "--", "."];
    args.extend(specs.iter().map(String::as_str));
    // An empty result means every commit so far only touched what is excluded.
    git_opt(&ctx.root, &args).filter(|c| !c.is_empty()).or(head)
}

/// Resolve a revision (`main`, `HEAD~3`, a tag, a short hash) to a full commit.
pub fn resolve_rev(root: &Path, rev: &str) -> Option<String> {
    git_opt(root, &["rev-parse", "--verify", &format!("{rev}^{{commit}}")])
}

/// A detached worktree of `root` at `rev`, removed when dropped.
///
/// Comparing two revisions means reading both, and `git stash` / `git checkout`
/// in place would fight the user's working tree. A worktree leaves it alone.
/// Checkout runs under the same hardened configuration as every other call, so a
/// repository that carries a `post-checkout` hook does not get to run it.
pub struct Worktree {
    repo: PathBuf,
    path: PathBuf,
}

impl Worktree {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Worktree {
    fn drop(&mut self) {
        let path = self.path.display().to_string();
        let _ = git(&self.repo, &["worktree", "remove", "--force", &path]);
        // `remove` leaves the directory behind if it was already gone from the
        // administrative list; the temporary parent goes either way.
        let _ = std::fs::remove_dir_all(&self.path);
        let _ = git(&self.repo, &["worktree", "prune"]);
    }
}

/// Check `rev` out into a fresh worktree under `parent`.
pub fn worktree_at(root: &Path, rev: &str, parent: &Path) -> Result<Worktree, GitError> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let commit = resolve_rev(&root, rev)
        .ok_or_else(|| GitError::Command { args: format!("rev-parse {rev}"), stderr: "no such revision".into() })?;
    let path = parent.join(short(&commit));
    let path_str = path.display().to_string();
    git(&root, &["worktree", "add", "--detach", "--quiet", &path_str, &commit])?;
    Ok(Worktree { repo: root, path })
}

pub fn tracked_at_head(ctx: &RepoContext, file: &str) -> bool {
    git(&ctx.root, &["cat-file", "-e", &format!("HEAD:./{file}")]).is_ok()
}

/// Files (relative to root) changed since `commit`, including the working tree.
pub fn changed_files_since(ctx: &RepoContext, commit: &str) -> Result<Vec<String>, GitError> {
    let out =
        git(&ctx.root, &["diff", "--name-only", "--no-textconv", "--no-ext-diff", "--relative", commit, "--", "."])?;
    Ok(out.lines().map(str::to_string).collect())
}

/// Pointer to a line or range the caller wants confirmed.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceQuery {
    pub file_path: String,
    pub line: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol_name: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceState {
    /// File exists, lines in range, unchanged since the pinned commit.
    Verified,
    /// Present but the lines changed since the pinned commit (or are uncommitted).
    Stale,
    /// Present on disk but not committed at HEAD.
    Untracked,
    FileMissing,
    LineOutOfRange,
    /// Lines exist but the named symbol isn't in them.
    SymbolMismatch,
    /// Path escapes the repository root.
    OutsideRepo,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceReport {
    pub file_path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub state: EvidenceState,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permalink: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed_ranges: Vec<(u32, u32)>,
}

impl EvidenceReport {
    pub fn is_ok(&self) -> bool {
        matches!(self.state, EvidenceState::Verified)
    }
}

/// Resolves a repo-relative path, refusing anything that escapes `root`.
pub fn resolve_in_repo(root: &Path, file: &str) -> Option<PathBuf> {
    let rel = Path::new(file.trim_start_matches("./"));
    if rel.is_absolute() {
        return None;
    }
    let mut depth: i32 = 0;
    for c in rel.components() {
        match c {
            std::path::Component::ParentDir => depth -= 1,
            std::path::Component::Normal(_) => depth += 1,
            std::path::Component::CurDir => {}
            _ => return None,
        }
        if depth < 0 {
            return None;
        }
    }
    let path = root.join(rel);
    // Counting components is not enough: a symlinked directory inside the
    // repository resolves outside it, and the file's contents are quoted into
    // the book as a snippet. A book is often committed and published, so this
    // is how `~/.aws/credentials` would end up on a web page.
    let (Ok(top), Ok(real)) = (root.canonicalize(), path.canonicalize()) else {
        // Nothing to read yet; the caller reports it missing.
        return Some(path);
    };
    real.starts_with(&top).then_some(path)
}

/// Verifies one piece of evidence against the working tree and, when the
/// repository is under git, against HEAD and the pinned commit.
pub fn verify_evidence(ctx: &RepoContext, q: &EvidenceQuery, pinned_commit: Option<&str>) -> EvidenceReport {
    Verifier::new(ctx, pinned_commit).verify(q)
}

/// Verifies many pieces of evidence against one repository with a constant
/// number of git processes: one `ls-files` for tracked paths and one `diff`
/// for every changed range, instead of several processes per citation.
pub struct Verifier<'a> {
    ctx: &'a RepoContext,
    pinned: Option<String>,
    base: Option<String>,
    pinned_missing: bool,
    tracked: Option<Arc<HashSet<String>>>,
    changed: Option<Arc<ChangedRanges>>,
    texts: std::collections::HashMap<String, Option<String>>,
    cache: Option<&'a EvidenceCache>,
}

type ChangedRanges = HashMap<String, Vec<(u32, u32)>>;

/// Repository snapshots shared by every verification in one operation (a book
/// build, one MCP request): repository context, tracked paths and the diff
/// against a base commit are read from git once per repository and base, not
/// once per diagram. Scope it to the operation — it does not see later edits.
#[derive(Debug, Default)]
pub struct EvidenceCache {
    contexts: Mutex<HashMap<PathBuf, RepoContext>>,
    tracked: Mutex<HashMap<PathBuf, Arc<HashSet<String>>>>,
    changed: Mutex<HashMap<(PathBuf, String), Arc<ChangedRanges>>>,
    commits: Mutex<HashMap<(PathBuf, String), bool>>,
}

impl EvidenceCache {
    pub fn context(&self, root: &Path) -> RepoContext {
        let mut map = self.contexts.lock().unwrap();
        map.entry(root.to_path_buf()).or_insert_with(|| repo_context(root)).clone()
    }
}

impl<'a> Verifier<'a> {
    pub fn new(ctx: &'a RepoContext, pinned_commit: Option<&str>) -> Self {
        Self::build(ctx, pinned_commit, None)
    }

    /// Like `new`, reusing git snapshots already read in this operation.
    pub fn cached(ctx: &'a RepoContext, pinned_commit: Option<&str>, cache: Option<&'a EvidenceCache>) -> Self {
        Self::build(ctx, pinned_commit, cache)
    }

    fn build(ctx: &'a RepoContext, pinned_commit: Option<&str>, cache: Option<&'a EvidenceCache>) -> Self {
        let base = pinned_commit.map(str::to_string).or_else(|| ctx.head_commit.clone());
        let exists = |c: &str| match cache {
            Some(cache) => *cache
                .commits
                .lock()
                .unwrap()
                .entry((ctx.root.clone(), c.to_string()))
                .or_insert_with(|| commit_exists(ctx, c)),
            None => commit_exists(ctx, c),
        };
        let pinned_missing = pinned_commit.is_some_and(|c| ctx.is_git() && !exists(c));
        Verifier {
            ctx,
            pinned: pinned_commit.map(str::to_string),
            base,
            pinned_missing,
            tracked: None,
            changed: None,
            texts: std::collections::HashMap::new(),
            cache,
        }
    }

    fn tracked(&mut self) -> &HashSet<String> {
        if self.tracked.is_none() {
            let root = self.ctx.root.clone();
            let load = || -> Arc<HashSet<String>> {
                let out = git(&root, &["ls-files", "-z", "--", "."]).unwrap_or_default();
                Arc::new(out.split('\0').filter(|p| !p.is_empty()).map(str::to_string).collect())
            };
            self.tracked = Some(match self.cache {
                Some(cache) => cache.tracked.lock().unwrap().entry(root.clone()).or_insert_with(load).clone(),
                None => load(),
            });
        }
        self.tracked.as_ref().unwrap()
    }

    fn changed(&mut self) -> &ChangedRanges {
        if self.changed.is_none() {
            let base = self.base.clone().filter(|_| !self.pinned_missing);
            let root = self.ctx.root.clone();
            let load = || Arc::new(diff_ranges(&root, base.as_deref()));
            self.changed = Some(match (self.cache, &base) {
                (Some(cache), Some(b)) => {
                    cache.changed.lock().unwrap().entry((root.clone(), b.clone())).or_insert_with(load).clone()
                }
                _ => load(),
            });
        }
        self.changed.as_ref().unwrap()
    }
}

/// Changed line ranges per file in the working tree relative to `base`.
fn diff_ranges(root: &Path, base: Option<&str>) -> ChangedRanges {
    let mut map: ChangedRanges = HashMap::new();
    if let Some(base) = base {
        let out = git(
            root,
            &["diff", "--no-color", "--no-textconv", "--no-ext-diff", "--unified=0", "--relative", base, "--", "."],
        )
        .unwrap_or_default();
        let mut current: Option<String> = None;
        for line in out.lines() {
            if let Some(path) = line.strip_prefix("+++ ") {
                current = path.strip_prefix("b/").map(str::to_string);
            } else if line.starts_with("@@ ") {
                if let Some(file) = &current {
                    map.entry(file.clone()).or_default().extend(parse_hunks(line));
                }
            }
        }
    }
    map
}

impl Verifier<'_> {
    fn text(&mut self, file: &str) -> Option<String> {
        if !self.texts.contains_key(file) {
            let text = resolve_in_repo(&self.ctx.root, file)
                .and_then(|abs| std::fs::read(abs).ok())
                .map(|b| String::from_utf8_lossy(&b).into_owned());
            self.texts.insert(file.to_string(), text);
        }
        self.texts[file].clone()
    }

    pub fn verify(&mut self, q: &EvidenceQuery) -> EvidenceReport {
        let end = q.end_line.unwrap_or(q.line).max(q.line);
        let mut report = EvidenceReport {
            file_path: q.file_path.clone(),
            start_line: q.line,
            end_line: end,
            state: EvidenceState::Verified,
            detail: String::new(),
            line_count: None,
            permalink: None,
            changed_ranges: vec![],
        };
        if resolve_in_repo(&self.ctx.root, &q.file_path).is_none() {
            report.state = EvidenceState::OutsideRepo;
            report.detail = "path must be relative to the repository root and stay inside it".into();
            return report;
        }
        let Some(text) = self.text(&q.file_path) else {
            report.state = EvidenceState::FileMissing;
            report.detail = format!("{} does not exist in {}", q.file_path, self.ctx.root.display());
            return report;
        };
        let line_count = text.lines().count() as u32;
        report.line_count = Some(line_count);
        if q.line == 0 || end > line_count {
            report.state = EvidenceState::LineOutOfRange;
            report.detail = format!("lines {}-{} requested but file has {} lines (1-based)", q.line, end, line_count);
            return report;
        }
        if let Some(sym) = q.symbol_name.as_deref().filter(|s| !s.is_empty()) {
            let bare = sym.rsplit(['.', ':']).next().unwrap_or(sym);
            let found =
                text.lines().skip(q.line as usize - 1).take((end - q.line + 1) as usize).any(|l| l.contains(bare));
            if !found {
                report.state = EvidenceState::SymbolMismatch;
                report.detail = match find_symbol_line(&text, bare) {
                    Some(n) => format!("`{sym}` not within lines {}-{}; first seen at line {n}", q.line, end),
                    None => format!("`{sym}` does not appear in {}", q.file_path),
                };
                return report;
            }
        }
        if !self.ctx.is_git() {
            report.detail = "verified against working tree (not a git repository)".into();
            return report;
        }
        let pinned = self.pinned.clone();
        report.permalink = Some(self.ctx.permalink(&q.file_path, q.line, end, pinned.as_deref()));
        let normalized = q.file_path.trim_start_matches("./").to_string();
        if !self.tracked().contains(&normalized) {
            report.state = EvidenceState::Untracked;
            report.detail = "file exists on disk but is not committed at HEAD".into();
            return report;
        }
        let Some(base) = self.base.clone() else {
            report.detail = "verified against working tree".into();
            return report;
        };
        if self.pinned_missing {
            report.state = EvidenceState::Stale;
            report.detail = format!("pinned commit {} is not in this repository", short(&base));
            return report;
        }
        let overlapping: Vec<(u32, u32)> = self
            .changed()
            .get(&normalized)
            .map(|r| r.iter().copied().filter(|&(a, b)| a <= end && b >= q.line).collect())
            .unwrap_or_default();
        if !overlapping.is_empty() {
            report.state = EvidenceState::Stale;
            report.detail = format!(
                "lines changed since {}: {}",
                short(&base),
                overlapping.iter().map(|(a, b)| format!("{a}-{b}")).collect::<Vec<_>>().join(", ")
            );
            report.changed_ranges = overlapping;
            return report;
        }
        report.detail = format!("verified against {}", short(&base));
        report
    }
}

fn find_symbol_line(text: &str, sym: &str) -> Option<usize> {
    text.lines().position(|l| l.contains(sym)).map(|i| i + 1)
}

/// Reads lines `start..=end` (1-based) for embedding in the evidence drawer.
pub fn read_snippet(root: &Path, file: &str, start: u32, end: u32, max_lines: u32) -> Option<String> {
    let abs = resolve_in_repo(root, file)?;
    let text = std::fs::read_to_string(abs).ok()?;
    let start = start.max(1);
    let end = end.max(start).min(start + max_lines.saturating_sub(1));
    let lines: Vec<&str> = text.lines().skip(start as usize - 1).take((end - start + 1) as usize).collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hunk_parsing_handles_all_header_shapes() {
        let diff = "@@ -3 +3 @@ fn a\n-x\n+y\n@@ -10,0 +11,4 @@\n@@ -20,2 +24,0 @@\n";
        assert_eq!(parse_hunks(diff), vec![(3, 3), (11, 14), (24, 24)]);
    }

    #[test]
    fn web_base_normalises_forge_remotes() {
        assert_eq!(web_base("git@github.com:acme/shop.git").as_deref(), Some("https://github.com/acme/shop"));
        assert_eq!(web_base("https://token@gitlab.com/acme/shop.git").as_deref(), Some("https://gitlab.com/acme/shop"));
        assert_eq!(web_base("/srv/git/shop.git"), None);
        assert_eq!(web_base("git@internal.corp:shop.git"), None);
    }

    #[test]
    fn permalink_prefers_forge_then_portable_form() {
        let mut ctx = repo_context(Path::new("/nonexistent-autodoc"));
        ctx.prefix = "services/api".into();
        ctx.head_commit = Some("0123456789abcdef0123".into());
        assert_eq!(ctx.permalink("src/a.ts", 4, 9, None), "services/api/src/a.ts#L4-L9@0123456789ab");
        ctx.remote_url = Some("git@github.com:acme/shop.git".into());
        assert_eq!(
            ctx.permalink("src/a.ts", 4, 4, None),
            "https://github.com/acme/shop/blob/0123456789abcdef0123/services/api/src/a.ts#L4"
        );
    }

    #[test]
    fn path_escape_is_rejected() {
        let root = Path::new("/repo");
        assert!(resolve_in_repo(root, "../etc/passwd").is_none());
        assert!(resolve_in_repo(root, "a/../../b").is_none());
        assert!(resolve_in_repo(root, "/etc/passwd").is_none());
        assert_eq!(resolve_in_repo(root, "./a/../b.rs"), Some(PathBuf::from("/repo/a/../b.rs")));
    }

    #[test]
    fn non_git_directory_verifies_against_disk() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.py"), "import os\n\ndef handler():\n    return 1\n").unwrap();
        let ctx = repo_context(dir.path());
        assert!(!ctx.is_git());
        let q = |line, end: u32, sym: &str| EvidenceQuery {
            file_path: "a.py".into(),
            line,
            end_line: Some(end),
            symbol_name: Some(sym.into()),
        };
        assert_eq!(verify_evidence(&ctx, &q(3, 4, "handler"), None).state, EvidenceState::Verified);
        assert_eq!(verify_evidence(&ctx, &q(3, 9, "handler"), None).state, EvidenceState::LineOutOfRange);
        let r = verify_evidence(&ctx, &q(1, 1, "handler"), None);
        assert_eq!(r.state, EvidenceState::SymbolMismatch);
        assert!(r.detail.contains("line 3"), "{}", r.detail);
        let missing = EvidenceQuery { file_path: "b.py".into(), line: 1, end_line: None, symbol_name: None };
        assert_eq!(verify_evidence(&ctx, &missing, None).state, EvidenceState::FileMissing);
    }

    #[test]
    fn snippet_is_capped() {
        let dir = tempfile::tempdir().unwrap();
        let body: String = (1..=50).map(|i| format!("line{i}\n")).collect();
        std::fs::write(dir.path().join("f.txt"), body).unwrap();
        let s = read_snippet(dir.path(), "f.txt", 10, 49, 5).unwrap();
        assert_eq!(s, "line10\nline11\nline12\nline13\nline14");
    }
}
