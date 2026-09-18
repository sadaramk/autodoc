import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { formatPath, parseDiagramIR } from "../src/index.js";

const contractDir = fileURLToPath(new URL("../../../tests/contract/", import.meta.url));

const jsonFiles = (kind: string): string[] =>
  readdirSync(join(contractDir, kind))
    .filter((f) => f.endsWith(".json") && f !== "expected-paths.json")
    .sort();

const load = (kind: string, file: string): unknown => JSON.parse(readFileSync(join(contractDir, kind, file), "utf8"));

describe("contract: valid fixtures", () => {
  const files = jsonFiles("valid");
  it("has fixtures", () => expect(files.length).toBeGreaterThanOrEqual(4));
  it.each(files)("%s parses", (file) => {
    const result = parseDiagramIR(load("valid", file));
    expect(result.ok ? [] : result.issues).toEqual([]);
  });
});

describe("contract: invalid fixtures", () => {
  const expected = load("invalid", "expected-paths.json") as Record<string, string>;
  const files = jsonFiles("invalid");
  it("every invalid fixture has an expected path", () => {
    expect(files).toEqual(Object.keys(expected).sort());
  });
  it.each(files)("%s is rejected at the expected path", (file) => {
    const result = parseDiagramIR(load("invalid", file));
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.issues[0]!.path, JSON.stringify(result.issues)).toBe(expected[file]);
  });
});

describe("parseDiagramIR", () => {
  it("accepts a JSON string and reports syntax errors at the root", () => {
    const result = parseDiagramIR('{"version": ');
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.issues[0]!.path).toBe("$");
  });

  it("reports a missing key at its parent, like serde", () => {
    const ir = load("valid", "minimal.json") as Record<string, unknown>;
    delete ir.title;
    const result = parseDiagramIR(ir);
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.issues[0]).toEqual({ path: "$", message: "missing field `title`" });
  });

  it("formats paths like the Rust SchemaError", () => {
    expect(formatPath(["nodes", 1, "evidence", "startLine"])).toBe("$.nodes[1].evidence.startLine");
    expect(formatPath([])).toBe("$");
  });
});
