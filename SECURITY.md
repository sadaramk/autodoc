# Security

autodoc reads source code and writes documentation. It does not execute the code it scans, and the
generated book makes no network requests: figures, citations and snippets are inlined.

Two things are worth knowing when you publish a book:

- **Snippets are excerpts of your source.** A citation embeds up to 40 lines of the file it points
  at. Don't publish a book for a private repository to a public site without reading what it
  included.
- **`authored.json` is yours.** autodoc creates it once and never overwrites it; anything you write
  there is published with the book.

## Reporting a vulnerability

Please report suspected vulnerabilities privately through GitHub's
[security advisories](https://github.com/sadaramk/autodoc/security/advisories/new) rather than a
public issue. Include the repository shape or input that triggers it. I'll acknowledge within a few
days and credit you in the fix unless you'd rather stay anonymous.
