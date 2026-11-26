import { ResourceTemplate, type McpServer } from "@modelcontextprotocol/server"
import type { LegacyTool } from "./nouns"

// #NEW-141 — OpenAI's MCP extensions beside the views (OpenAI's
// mcp-extensions spec; hosted by ChatGPT and OpenClaw), outside sessions only:
//   - entrypoints: `exponential_issues` opens on `{}` (apps.ts) = the caller's
//     open issues, as `op: "show"` answers them;
//   - composer mentions: `openai/mentions` names the SAME tool, so no tool is
//     added: a call of `{query}` alone answers `{items: ResourceLink[]}`;
//   - a link's issue is read as `exponential://issues/<IDENT>` through the
//     `get` op, with its access checks.

export { MCP_EXTENSION_CAPABILITIES, MENTIONS_TOOL } from "./extensions"
export const ISSUE_URI_TEMPLATE = `exponential://issues/{id}`
const MENTIONS_LIMIT = 10

type Result = { content: unknown[]; isError?: boolean; structuredContent?: unknown }

function textOf(result: Result): string {
  const block = result.content.find((item): item is { type: `text`; text: string } => (item as { type?: string }).type === `text`)
  return block?.text ?? ``
}

/** `{query}` and nothing else: the composer asks what `@<query>` may name. */
export function mentionQuery(args: Record<string, unknown>): string | null {
  const keys = Object.keys(args)
  return keys.length === 1 && typeof args.query === `string` ? args.query : null
}

/** The mention search: the newest open issues the list op finds. */
export async function mentionIssues(tools: ReadonlyMap<string, LegacyTool>, query: string, extra: unknown): Promise<Result> {
  const list = tools.get(`exponential_issues_list`)
  if (!list) return { content: [], structuredContent: { items: [] } }
  const search = query.trim().slice(0, 256)
  const args = list.def.inputSchema!.parse({ ...(search ? { search } : {}), sort: `-updatedAt`, limit: MENTIONS_LIMIT }) as Record<string, unknown>
  const result = await list.handler(args, extra)
  if (result.isError) return result
  const rows = JSON.parse(textOf(result)) as { identifier: string; title: string }[]
  const items = rows.map((row) => ({
    type: `resource_link` as const,
    uri: ISSUE_URI_TEMPLATE.replace(`{id}`, row.identifier),
    name: `${row.identifier} ${row.title}`,
    mimeType: `application/json`,
  }))
  return { content: [{ type: `text`, text: JSON.stringify({ items }) }], structuredContent: { items } }
}

/** `exponential://issues/<IDENT>`: what a mention attaches. */
export function registerIssueResource(server: McpServer, tools: ReadonlyMap<string, LegacyTool>): void {
  const get = tools.get(`exponential_issues_get`)
  if (!get) return
  server.registerResource(
    `exponential-issue`,
    new ResourceTemplate(ISSUE_URI_TEMPLATE, { list: undefined }),
    { title: `Exponential issue`, mimeType: `application/json` },
    async (uri, { id }, extra) => {
      const result = await get.handler({ id: decodeURIComponent(String(id)) }, extra)
      if (result.isError) throw new Error(textOf(result) || `Issue not found`)
      return { contents: [{ uri: uri.href, mimeType: `application/json`, text: textOf(result) }] }
    }
  )
}
