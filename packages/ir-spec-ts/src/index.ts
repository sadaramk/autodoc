/**
 * TypeScript mirror of `crates/ir-spec` (Rust/serde). Every object is strict to
 * match serde's `deny_unknown_fields`, and every Rust `Option` is `.nullish()`
 * because serde accepts both a missing key and `null`. Parity is enforced by
 * the shared fixtures in `tests/contract`.
 */
import { z } from "zod";

export const IR_VERSION = "1.1.0" as const;
export const IR_VERSIONS = ["1.0.0", "1.1.0"] as const;

/** Density ceiling from the editorial standard: at most 4/10. */
export const MAX_VISUAL_DENSITY = 0.4;

export const DiagramTypeSchema = z.enum([
  "system-context",
  "container",
  "component",
  "data-flow",
  "lifecycle",
  "sequence",
  "entity-relationship",
]);
export const StateKindSchema = z.enum(["initial", "normal", "terminal"]);
export const CardinalitySchema = z.enum(["1:1", "1:n", "n:1", "n:m"]);
export const KeyKindSchema = z.enum(["pk", "fk", "pk-fk"]);
export const ThemeSchema = z.enum(["editorial-light", "editorial-dark"]);
export const BoundaryTypeSchema = z.enum(["trust-zone", "internal-service", "third-party", "storage", "client"]);
export const EdgeTypeSchema = z.enum(["sync", "async", "event", "read", "write"]);
export const EdgeStyleSchema = z.enum(["solid", "dashed"]);

/** Rust `u32`. */
const u32 = z.number().int().nonnegative().max(4_294_967_295);

export const DiagramMetadataSchema = z.strictObject({
  targetRepo: z.string(),
  commitHash: z.string().nullish(),
  generatedAt: z.string(),
  /** Target: <= 0.40. Recomputed by the validator; an agent-supplied value is advisory. */
  visualDensityScore: z.number().nullish(),
});

export const ContainerSchema = z.strictObject({
  id: z.string(),
  label: z.string(),
  boundaryType: BoundaryTypeSchema,
  roleDescription: z.string().nullish(),
});

export const EvidenceSchema = z.strictObject({
  filePath: z.string(),
  startLine: u32,
  endLine: u32,
  symbolName: z.string().nullish(),
  /** The repository this line was read from; absent means the one being documented. */
  repo: z.string().nullish(),
});

export const AttributeSchema = z.strictObject({
  name: z.string(),
  typeName: z.string(),
  key: KeyKindSchema.nullish(),
  nullable: z.boolean().nullish(),
  note: z.string().nullish(),
});

export const NodeSchema = z.strictObject({
  id: z.string(),
  containerId: z.string().nullish(),
  label: z.string(),
  subtitle: z.string().nullish(),
  techStack: z.string().nullish(),
  /** Max 1-2 nodes. Triggers the primary accent color. */
  isKeyFocalPoint: z.boolean(),
  evidence: EvidenceSchema.nullish(),
  metadata: z.record(z.string(), z.string()).nullish(),
  /** Entity-relationship diagrams: columns / fields. */
  attributes: z.array(AttributeSchema).nullish(),
  /** Lifecycle diagrams: initial / terminal states. */
  stateKind: StateKindSchema.nullish(),
});

export const EdgeSchema = z.strictObject({
  id: z.string(),
  source: z.string(),
  target: z.string(),
  label: z.string().nullish(),
  edgeType: EdgeTypeSchema,
  style: EdgeStyleSchema.nullish(),
  /** Accent highlight for the critical transaction path. */
  isPrimaryPath: z.boolean().nullish(),
  /** Sequence diagrams: 1-based message order. */
  sequence: u32.nullish(),
  reply: z.boolean().nullish(),
  payload: z.string().nullish(),
  cardinality: CardinalitySchema.nullish(),
  guard: z.string().nullish(),
  evidence: EvidenceSchema.nullish(),
});

export const DiagramIRSchema = z.strictObject({
  version: z.enum(IR_VERSIONS),
  diagramType: DiagramTypeSchema,
  title: z.string(),
  subtitle: z.string().nullish(),
  theme: ThemeSchema,
  metadata: DiagramMetadataSchema,
  containers: z.array(ContainerSchema),
  nodes: z.array(NodeSchema),
  edges: z.array(EdgeSchema),
});

export type DiagramType = z.infer<typeof DiagramTypeSchema>;
export type Theme = z.infer<typeof ThemeSchema>;
export type BoundaryType = z.infer<typeof BoundaryTypeSchema>;
export type EdgeType = z.infer<typeof EdgeTypeSchema>;
export type EdgeStyle = z.infer<typeof EdgeStyleSchema>;
export type DiagramMetadata = z.infer<typeof DiagramMetadataSchema>;
export type Container = z.infer<typeof ContainerSchema>;
export type Evidence = z.infer<typeof EvidenceSchema>;
export type Node = z.infer<typeof NodeSchema>;
export type Edge = z.infer<typeof EdgeSchema>;
export type DiagramIR = z.infer<typeof DiagramIRSchema>;

export interface IrIssue {
  /** JSONPath-style location, identical in format to the Rust `SchemaError.path`. */
  path: string;
  message: string;
}

export type ParseResult = { ok: true; ir: DiagramIR } | { ok: false; issues: IrIssue[] };

/** Formats `["edges", 0, "edgeType"]` as `$.edges[0].edgeType`. */
export function formatPath(segments: ReadonlyArray<PropertyKey>): string {
  let out = "$";
  for (const seg of segments) {
    out += typeof seg === "number" ? `[${seg}]` : `.${String(seg)}`;
  }
  return out;
}

function isMissingKey(input: unknown, path: ReadonlyArray<PropertyKey>): boolean {
  if (path.length === 0) return false;
  let cursor: unknown = input;
  for (const seg of path.slice(0, -1)) {
    if (cursor === null || typeof cursor !== "object") return false;
    cursor = (cursor as Record<PropertyKey, unknown>)[seg];
  }
  if (cursor === null || typeof cursor !== "object" || Array.isArray(cursor)) return false;
  return !Object.prototype.hasOwnProperty.call(cursor, path[path.length - 1]!);
}

/**
 * Parses an unknown value (or a JSON string) into a `DiagramIR`. A missing
 * required key is reported at its parent object, matching serde's behaviour.
 */
export function parseDiagramIR(json: unknown): ParseResult {
  let input = json;
  if (typeof json === "string") {
    try {
      input = JSON.parse(json);
    } catch (e) {
      return { ok: false, issues: [{ path: "$", message: (e as Error).message }] };
    }
  }
  const result = DiagramIRSchema.safeParse(input);
  if (result.success) return { ok: true, ir: result.data };
  // serde fails on an unknown key as soon as it reads it, but only reports a
  // missing key once the object closes, so unknown keys go first.
  const ordered = [...result.error.issues].sort(
    (a, b) => Number(b.code === "unrecognized_keys") - Number(a.code === "unrecognized_keys"),
  );
  const issues = ordered.map((issue): IrIssue => {
    // serde names the offending key for unknown fields; zod names the object.
    if (issue.code === "unrecognized_keys") {
      const key = issue.keys[0] ?? "";
      return { path: formatPath([...issue.path, key]), message: `unknown field \`${key}\`` };
    }
    // serde reports a missing required key at its parent object.
    if (isMissingKey(input, issue.path)) {
      const key = String(issue.path[issue.path.length - 1]);
      return { path: formatPath(issue.path.slice(0, -1)), message: `missing field \`${key}\`` };
    }
    return { path: formatPath(issue.path), message: issue.message };
  });
  return { ok: false, issues };
}
