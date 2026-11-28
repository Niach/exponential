import { readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { gzipSync } from "node:zlib"
import { parseAst, type Plugin } from "vite"
import interSymbols from "../../../exponential-ui/fonts/inter-symbols.json" with { type: "json" }

// #NEW-140 — what keeps a view's document small. The build's half only
// (vite.config.ts); nothing here runs in a view.

/** The size budget: bytes a view's built document may have. OpenClaw caps a
 *  `ui://` resource at 2 MiB (2,097,152); the server's stamps (origin, font
 *  URLs) take the rest. Raise one only on purpose, never past the cap. */
export const MCP_APP_MAX_BYTES = {
  issues: 1_350_000,
  inbox: 1_350_000,
  devices: 700_000,
  genui: 2_000_000,
} as const

/** The big tables (`packages/<file>`) a document carries gzipped, unpacked
 *  before the modules that read them run (no request: the sandbox has no
 *  network). Everything else stays plain code. */
const PACKED = [
  `exponential-ui/catalog/macros.json`,
  `exponential-ui/generated/core.runtime.json`,
  `exponential-ui/themes/exponential.theme.json`,
  `exponential-ui/themes/neutral.theme.json`,
  `exponential-ui/themes/orange.theme.json`,
  `exponential-ui/themes/playful.theme.json`,
  `ui/exponential-ui/extension.json`,
  `domain-contract/contract.json`,
] as const

type Node = { type: string; [key: string]: unknown }

/** True when every use of the binding `name` in `ast` reads a plain member
 *  (`name.copy`), so a namespace import reads the same values. `name.default`
 *  would be the namespace's own default, not the file's key. */
export function readsMembersOnly(ast: Node, name: string): boolean {
  let only = true
  const walk = (node: unknown, parent: Node | null, key: string): void => {
    if (!only || !node || typeof node !== `object`) return
    if (Array.isArray(node)) {
      for (const item of node) walk(item, parent, key)
      return
    }
    const n = node as Node
    if (typeof n.type !== `string` || n.type === `ImportDeclaration`) return
    if (n.type === `Identifier` && n.name === name) {
      const member = parent?.type === `MemberExpression` && !parent.computed
      // `x.name` and `{ name: … }` are not the binding.
      const label = (member && key === `property`) || (parent?.type === `Property` && key === `key` && !parent.computed && !parent.shorthand)
      if (!label && !(member && key === `object` && (parent.property as { name?: string }).name !== `default`)) only = false
      return
    }
    for (const [k, value] of Object.entries(n)) walk(value, n, k)
  }
  walk(ast, null, ``)
  return only
}

/** `import fixture from "….json"` keeps the whole file: its `$comment` and
 *  the replay cases only tests read. Where a module reads members only, the
 *  import becomes a namespace, and the document keeps just those members. */
export function namespaceJsonImports(code: string): string {
  if (!code.includes(`.json`)) return code
  const ast = parseAst(code) as unknown as Node & { body: Node[] }
  let out = code
  for (const statement of [...ast.body].reverse()) {
    if (statement.type !== `ImportDeclaration`) continue
    const source = (statement.source as { value: string }).value
    const specifiers = statement.specifiers as (Node & { local: { name: string } })[]
    if (!/\.json$/.test(source) || PACKED.some((file) => source.endsWith(file.slice(file.indexOf(`/`)))) || specifiers.length !== 1 || specifiers[0].type !== `ImportDefaultSpecifier`) continue
    const { name } = specifiers[0].local
    if (!readsMembersOnly(ast, name)) continue
    out = `${out.slice(0, statement.start as number)}import * as ${name} from ${JSON.stringify(source)}\n${out.slice(statement.end as number)}`
  }
  return out
}

/** A JSON object as a module of its members (identifier keys) + the default.
 *  `$comment` (the spec's prose) stays out. */
export function jsonModule(json: string): string {
  const value = JSON.parse(json) as unknown
  if (!value || typeof value !== `object` || Array.isArray(value)) return `export default ${json}`
  const entries = Object.entries(value).filter(([key]) => key !== `$comment` && key !== `_comment`)
  const named = entries.map(([key, member], i) => `const m${i} = ${literal(member)}\n${/^[A-Za-z_$][\w$]*$/.test(key) && key !== `default` ? `export { m${i} as ${key} }\n` : ``}`)
  return `${named.join(``)}export default { ${entries.map(([key], i) => `${JSON.stringify(key)}: m${i}`).join(`, `)} }\n`
}

const literal = (value: unknown): string => JSON.stringify(value).replace(/"__proto__":/g, `["__proto__"]:`)

/** A JSON file as its gzipped self, parsed when the document starts. */
export function packedModule(json: string): string {
  const packed = gzipSync(JSON.stringify(JSON.parse(json)), { level: 9 }).toString(`base64`)
  return `const bytes = Uint8Array.from(atob("${packed}"), (c) => c.charCodeAt(0))
export default await new Response(new Blob([bytes]).stream().pipeThrough(new DecompressionStream("gzip"))).json()
`
}

/** What the packed tables say: names only they hold still count as used. */
export const packedText = (): string => PACKED.map((file) => readFileSync(fileURLToPath(new URL(`../../../${file}`, import.meta.url)), `utf8`)).join(`\n`)

/** The JSON a view imports: only what its modules read of it. */
export function leanJson(): Plugin[] {
  return [
    {
      name: `exp-mcp-apps-json`,
      enforce: `pre`,
      load(id) {
        if (!/\/packages\/[^?]*\.json$/.test(id)) return null
        const json = readFileSync(id, `utf8`)
        const code = PACKED.some((file) => id.endsWith(`/packages/${file}`)) ? packedModule(json) : jsonModule(json)
        return { code, moduleType: `js`, moduleSideEffects: false }
      },
    },
    {
      name: `exp-mcp-apps-json-imports`,
      enforce: `post`,
      transform: (code, id) => (/\/packages\/[^?]*\.[jt]sx?$/.test(id) ? { code: namespaceJsonImports(code), map: null } : null),
    },
  ]
}

/** genUI's `Inter Symbols` faces (packages/exponential-ui/src/web/inter-symbols.generated.ts)
 *  as the app's own `/fonts/` files instead of four inlined woff2 data URLs,
 *  which put genui.html over its budget. The files in apps/web/public/fonts are
 *  byte-equal to packages/exponential-ui/fonts (lean.test.ts); the server points
 *  `/fonts/` at the app's origin, which serves them with CORS, like JetBrains
 *  Mono (apps/web/src/lib/emcp/apps.ts `mcpAppHtml`). */
export function interSymbolsAsFiles(code: string): string {
  return code.replace(/font-weight:(\d+);src:url\(data:font\/woff2;base64,[A-Za-z0-9+/=]+\)/g, (_, weight: string) => {
    const face = INTER_SYMBOLS_FACES.find((f) => f.weight === Number(weight))
    if (!face) throw new Error(`Inter Symbols: no face for weight ${weight} in packages/exponential-ui/fonts/inter-symbols.json`)
    return `font-weight:${weight};src:url(/fonts/${interSymbolsFile(face.style)})`
  })
}

export const INTER_SYMBOLS_FACES: readonly { weight: number; style: string }[] = interSymbols.faces

export const interSymbolsFile = (style: string): string => `InterSymbols-${style}.woff2`

export function leanInterSymbols(): Plugin {
  return {
    name: `exp-mcp-apps-inter-symbols`,
    transform: (code, id) => (id.endsWith(`/packages/exponential-ui/src/web/inter-symbols.generated.ts`) ? { code: interSymbolsAsFiles(code), map: null } : null),
  }
}
