// #AXC-12: the A2UI catalog definition the Express compiler loads: the core
// catalog (catalog/core.schema.json) without its prose (`description`,
// `instructions`, `x-deprecated-reason`, `examples`, `title`); a PROPERTY
// named `description` (EmptyState's) stays, only text values go. The same
// function writes eui-core's generated/express.catalog.json
// (scripts/generate.ts), so the TS and the Rust compiler load one catalog.

const PROSE = new Set([`description`, `instructions`, `x-deprecated-reason`, `examples`, `title`])

export function expressCatalog(schema: unknown, inProperties = false): unknown {
  if (Array.isArray(schema)) return schema.map((v) => expressCatalog(v))
  if (!schema || typeof schema !== `object`) return schema
  return Object.fromEntries(
    Object.entries(schema)
      .filter(([k, v]) => inProperties || !(PROSE.has(k) && (typeof v === `string` || k === `examples`)))
      .map(([k, v]) => [k, expressCatalog(v, !inProperties && (k === `properties` || k === `$defs` || k === `components` || k === `functions`))]),
  )
}
