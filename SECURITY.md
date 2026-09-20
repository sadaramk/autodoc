# Security

nunki reads a repository and writes documentation. The repository it reads is **untrusted input**:
documenting code you did not write is the point of the tool, so file contents, paths, symbol names,
configuration and `.git/config` all come from somewhere else.

nunki does not run the code it scans, and the generated book makes no network requests — figures,
citations and snippets are inlined.

It does run `git`, which is where the sharp edges are. A repository can carry configuration that
asks git to execute a command — `core.fsmonitor` turns `git status` into arbitrary execution — so
every invocation overrides the keys that can do that, and diffs run with `--no-textconv`
`--no-ext-diff`. Note this only matters for a repository you obtained as an archive, a tarball or a
vendored copy: `git clone` does not carry the source repository's config.

Residual risk worth knowing: a `.gitattributes` clean filter is still applied by `git diff` when one
is configured for the repository. If you are documenting something you actively distrust, run
nunki in a container.

## Publishing a book

- **Snippets are excerpts of your source.** A citation embeds up to 40 lines of the file it points
  at. Don't publish a book for a private repository to a public site without reading what it
  included.
- **Evidence stays inside the repository.** A citation that resolves outside it — by traversal or
  through a symlinked directory — is refused rather than quoted.
- **The output directory is confined.** `output.dir` in a repository's `nunki.toml` must be a
  relative path inside that repository; `--out` is your own instruction and is not restricted.
- **`authored.json` is yours.** nunki creates it once and never overwrites it; anything you write
  there is published with the book.

## Reporting a vulnerability

Please report suspected vulnerabilities privately through GitHub's
[security advisories](https://github.com/sadaramk/nunki/security/advisories/new) rather than a
public issue. Include the repository shape or input that triggers it. I'll acknowledge within a few
days and credit you in the fix unless you'd rather stay anonymous.
