import { execFileSync } from "node:child_process";
import { cpSync, mkdtempSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
export const PATHS_FILE = join(here, ".e2e-paths.json");

/** Builds a git-backed copy of the demo repo and renders every view with the real binary. */
export default function globalSetup(): void {
  const bin = process.env.NUNKI_BIN ?? resolve(here, "../../target/debug/nunki");
  if (!existsSync(bin)) throw new Error(`nunki binary not found at ${bin}; build it or set NUNKI_BIN`);
  const work = mkdtempSync(join(tmpdir(), "nunki-e2e-"));
  const repo = join(work, "polyglot-shop");
  cpSync(resolve(here, "../fixtures/polyglot-shop"), repo, { recursive: true });
  const git = (...args: string[]) =>
    execFileSync("git", ["-C", repo, "-c", "user.email=e2e@example.com", "-c", "user.name=e2e", ...args], { stdio: "pipe" });
  git("init", "-q", "-b", "main");
  git("remote", "add", "origin", "git@github.com:acme/polyglot-shop.git");
  git("add", ".");
  git("commit", "-qm", "init");

  // Single-diagram pages: analyze → IR → render (the standalone viewer).
  const out = join(work, "out");
  const view = (depth: string, name: string, extra: string[] = [], renderExtra: string[] = [], dir = out) => {
    const ir = join(dir, `${name}.ir.json`);
    execFileSync(bin, ["analyze", repo, "--depth", depth, "--emit-ir", ir, ...extra], { stdio: "pipe" });
    const html = join(dir, `${name}.html`);
    execFileSync(bin, ["render", ir, "-o", html, "--repo", repo, ...renderExtra], { stdio: "pipe" });
    return html;
  };
  const containers = view("container", "containers");
  const components = view("component", "components");
  const system = view("system", "system-context");
  const dark = view("container", "containers", ["--theme", "dark"], ["--accent", "coral"], join(work, "out-dark"));

  // Purpose-built views: typed IR fixtures compiled against the same repository.
  const typed = (name: string) => {
    const html = join(out, `${name}.html`);
    execFileSync(bin, ["render", resolve(here, `../contract/valid/${name}.json`), "-o", html, "--repo", repo], { stdio: "pipe" });
    return html;
  };
  const sequence = typed("sequence-checkout");
  const entities = typed("entity-relationship-orders");
  const lifecycle = typed("lifecycle-order-status");

  // The architecture book.
  const book = join(work, "book");
  execFileSync(bin, ["generate", repo, "--out", book], { stdio: "pipe" });

  // A second book of a different shape. Every reader spec ran against the demo
  // repository alone, so nothing exercised the pages a JVM system produces:
  // several containers, a gateway and config server, an API reference and a
  // runtime page built from compose.
  const jvm = join(work, "shop-cloud");
  cpSync(resolve(here, "../fixtures/real-world/spring-cloud"), jvm, { recursive: true });
  const jvmGit = (...args: string[]) =>
    execFileSync("git", ["-C", jvm, "-c", "user.email=e2e@example.com", "-c", "user.name=e2e", ...args], {
      stdio: "pipe",
    });
  jvmGit("init", "-q", "-b", "main");
  jvmGit("remote", "add", "origin", "git@github.com:acme/shop-cloud.git");
  jvmGit("add", ".");
  jvmGit("commit", "-qm", "init");
  const jvmBook = join(work, "book-jvm");
  execFileSync(bin, ["generate", jvm, "--out", jvmBook], { stdio: "pipe" });

  const url = (p: string) => pathToFileURL(p).href;
  writeFileSync(
    PATHS_FILE,
    JSON.stringify({
      containers: url(containers),
      components: url(components),
      system: url(system),
      dark: url(dark),
      sequence: url(sequence),
      entities: url(entities),
      lifecycle: url(lifecycle),
      bookIndex: url(join(book, "index.html")),
      bookJvm: url(join(jvmBook, "index.html")),
    }),
  );
}
