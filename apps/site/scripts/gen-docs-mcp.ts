/* Writes the MCP tool surface as JSON: the 22 tools in their groups, each
   op with its own description and parameters, read from the live
   definitions in apps/web/src/lib/emcp (nouns.ts routes every op to its
   tool in tools.ts). Run by scripts/reference/docs.ts with cwd apps/web
   (its tsconfig paths) and throwaway env: registering the tools never
   touches the database. */
import { z } from "zod"

const WEB = process.cwd()
const { NOUN_TOOLS, collectLegacyTools } = await import(`${WEB}/src/lib/emcp/nouns.ts`)
const { ALL_MCP_TOOL_GATES } = await import(`${WEB}/src/lib/emcp/gates.ts`)
const { FULL_ACCESS } = await import(`${WEB}/src/lib/emcp/scope.ts`)
const { contract } = await import(`@exp/domain-contract`)

interface JsonProp {
  type?: string | string[]
  description?: string
  enum?: unknown[]
  anyOf?: JsonProp[]
  items?: JsonProp
  const?: unknown
  format?: string
}

function typeOf(p: JsonProp): string {
  if (p.enum) return p.enum.map((v) => JSON.stringify(v)).join(` \\| `)
  if (p.const !== undefined) return JSON.stringify(p.const)
  if (p.anyOf) return [...new Set(p.anyOf.map(typeOf))].filter((t) => t !== `null`).join(` \\| `)
  if (p.type === `array`) return `${p.items ? typeOf(p.items) : `unknown`}[]`
  if (Array.isArray(p.type)) return p.type.filter((t) => t !== `null`).join(` \\| `)
  if (p.format === `uuid`) return `uuid`
  return p.type ?? `unknown`
}

function params(schema: z.ZodType | undefined) {
  if (!schema) return []
  const json = z.toJSONSchema(schema, { io: `input`, target: `draft-7`, unrepresentable: `any` }) as {
    properties?: Record<string, JsonProp>
    required?: string[]
  }
  const required = new Set(json.required ?? [])
  return Object.entries(json.properties ?? {}).map(([name, p]) => ({
    name,
    type: typeOf(p),
    required: required.has(name),
    description: p.description ?? p.anyOf?.find((b) => b.description)?.description ?? ``,
  }))
}

const legacy = collectLegacyTools(
  { id: `u` } as never,
  new Request(`https://x.test/api/mcp`),
  FULL_ACCESS,
  `session`,
  ALL_MCP_TOOL_GATES
)
const tool = (name: string) => legacy.get(`exponential_${name}`)

const groups = contract.expToolDisplay.groups.map((g: { id: string; label: string; tools: string[] }) => ({
  id: g.id,
  label: g.label,
  tools: g.tools.map((name) => {
    const spec = NOUN_TOOLS.find((s: { name: string }) => s.name === name)
    if (!spec) throw new Error(`group ${g.id} names ${name}, which nouns.ts lacks`)
    if (spec.verb) {
      const t = tool(spec.verb)
      return { name: `exponential_${name}`, alwaysLoad: !!spec.alwaysLoad, blurb: t?.def.description ?? ``, ops: [], params: params(t?.def.inputSchema) }
    }
    return {
      name: `exponential_${name}`,
      alwaysLoad: !!spec.alwaysLoad,
      blurb: spec.blurb,
      ops: Object.entries(spec.ops ?? {}).map(([op, legacyName]) => {
        const t = tool(legacyName as string)
        return { op, description: t?.def.description ?? ``, params: params(t?.def.inputSchema) }
      }),
    }
  }),
}))

// stdout carries dotenvx's banner, so the JSON goes to the file named in argv.
await Bun.write(process.argv[2]!, JSON.stringify({ groups }))
process.exit(0)
