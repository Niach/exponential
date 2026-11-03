// The crypto-free half of eMCP's references: safe in a browser bundle (the
// web app's host and settings import `@exp/emcp/upstream`, which must never
// pull `node:crypto`). `audit.ts` and `exposes.ts` re-export these.
import exposesFixture from "../fixtures/exposes.json" with { type: "json" }

function compareCodePoints(a: string, b: string): number {
  const x = Array.from(a)
  const y = Array.from(b)
  for (let i = 0; i < Math.min(x.length, y.length); i++) {
    const d = x[i]!.codePointAt(0)! - y[i]!.codePointAt(0)!
    if (d !== 0) return d
  }
  return x.length - y.length
}

/** Canonical JSON: object keys sorted by code point, recursively, no
 *  whitespace; `undefined` object members dropped; -0 prints as 0. */
export function canonicalJson(value: unknown): string {
  if (value === null || value === undefined) return `null`
  if (typeof value === `number`) {
    if (!Number.isFinite(value)) return `null`
    return Object.is(value, -0) ? `0` : JSON.stringify(value)
  }
  if (typeof value === `string` || typeof value === `boolean`) return JSON.stringify(value)
  if (Array.isArray(value)) return `[${value.map((v) => canonicalJson(v)).join(`,`)}]`
  if (typeof value === `object`) {
    const entries = Object.entries(value as Record<string, unknown>)
      .filter(([, v]) => v !== undefined)
      .sort(([a], [b]) => compareCodePoints(a, b))
    return `{${entries.map(([k, v]) => `${JSON.stringify(k)}:${canonicalJson(v)}`).join(`,`)}}`
  }
  return `null`
}

const DESTRUCTIVE_WORDS: readonly string[] = exposesFixture.destructiveWords

/** The MCP annotations of an upstream tool (from its `tools/list`). */
export interface UpstreamAnnotations {
  readOnlyHint?: boolean
  destructiveHint?: boolean
}

/** An upstream tool is destructive when it says so (`destructiveHint:
 *  true`) or a word of its name is a destructive word. */
export function upstreamDestructive(tool: string, annotations: UpstreamAnnotations | null | undefined): boolean {
  if (annotations?.destructiveHint === true) return true
  return tool.toLowerCase().split(/[^a-z]+/).some((word) => DESTRUCTIVE_WORDS.includes(word))
}
