// VAPP-161: a package's TOOLS (docs/package-format.md "Tools"): the MCP
// servers and scripts its functions call (`tools.<id>.<name>` for an MCP
// tool, `tools.<id>` for a script), each behind the consent card the host
// MUST show before the first call. Pure, so the Rust core mirrors it
// (`host/tools.rs`); fixtures/host-router.json (validation) and
// fixtures/host-package.json `consent` replay it.

import type { PackageIssue } from "./package"
import { canonicalJson } from "./signature"

/** `^[a-z][a-z0-9_-]{0,31}$`: a tool's id. */
export const TOOL_ID_PATTERN = /^[a-z][a-z0-9_-]{0,31}$/
export const TOOL_KINDS: readonly string[] = [`mcp`, `script`]
const ENV_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/

export interface PackageTool {
  kind: `mcp` | `script`
  title: string
  description?: string
  /** A remote MCP server (Streamable HTTP). */
  url?: string
  /** A local MCP server (stdio) or the script's process. */
  command?: string
  args?: string[]
  /** Variable NAMES the host passes through, never values. */
  env?: string[]
}

/** One row of the consent card. */
export interface ToolConsentRow {
  id: string
  kind: `mcp` | `script`
  transport: `http` | `stdio` | `script`
  title: string
  description: string | null
  /** The url, or the command line exactly as it runs. */
  target: string
  env: string[]
}

const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)
const isStringList = (v: unknown): v is string[] => Array.isArray(v) && v.every((s) => typeof s === `string`)

function isHttpUrl(s: string): boolean {
  try {
    const u = new URL(s)
    return (u.protocol === `https:` || u.protocol === `http:`) && !!u.host
  } catch {
    return false
  }
}

/** Every problem with a package's `tools`, ids in sorted order. */
export function validateTools(tools: unknown): PackageIssue[] {
  if (!isObject(tools)) return [{ path: `/tools`, message: `tools is an object of tools` }]
  const issues: PackageIssue[] = []
  for (const id of Object.keys(tools).sort()) {
    const tool = tools[id]
    const at = `/tools/${id}`
    if (!TOOL_ID_PATTERN.test(id)) issues.push({ path: at, message: `a tool id is lowercase letters, digits, - and _` })
    if (!isObject(tool)) {
      issues.push({ path: at, message: `a tool is {kind, title, url | command}` })
      continue
    }
    if (typeof tool.kind !== `string` || !TOOL_KINDS.includes(tool.kind)) issues.push({ path: `${at}/kind`, message: `kind is mcp or script` })
    if (typeof tool.title !== `string` || !tool.title.trim()) issues.push({ path: `${at}/title`, message: `title is a required string` })
    if (tool.description !== undefined && typeof tool.description !== `string`) issues.push({ path: `${at}/description`, message: `description is a string` })
    const hasUrl = tool.url !== undefined
    const hasCommand = tool.command !== undefined
    if (tool.kind === `mcp` && hasUrl === hasCommand) issues.push({ path: at, message: `an mcp tool has a url or a command` })
    if (tool.kind === `script` && hasUrl) issues.push({ path: `${at}/url`, message: `a script runs a command, not a url` })
    if (tool.kind === `script` && !hasCommand) issues.push({ path: `${at}/command`, message: `command is a non-empty string` })
    if (hasUrl && (typeof tool.url !== `string` || !isHttpUrl(tool.url))) issues.push({ path: `${at}/url`, message: `url is an absolute http(s) url` })
    if (hasCommand && (typeof tool.command !== `string` || !tool.command.trim())) issues.push({ path: `${at}/command`, message: `command is a non-empty string` })
    if (tool.args !== undefined && (!isStringList(tool.args) || hasUrl)) issues.push({ path: `${at}/args`, message: `args is a list of strings beside a command` })
    if (tool.env !== undefined && (!isStringList(tool.env) || tool.env.some((n) => !ENV_NAME.test(n)) || hasUrl)) issues.push({ path: `${at}/env`, message: `env is a list of variable names beside a command` })
  }
  return issues
}

/** An arg as the consent card shows it: as is, or JSON-quoted when it is
 *  empty or holds a space, a quote or a backslash. */
function shellArg(arg: string): string {
  return arg === `` || /[\s"'\\]/.test(arg) ? JSON.stringify(arg) : arg
}

/** The consent card's rows for a (valid) package's tools, by id. */
export function toolConsent(pkg: { tools?: Record<string, PackageTool> }): ToolConsentRow[] {
  return Object.keys(pkg.tools ?? {})
    .sort()
    .map((id) => {
      const tool = pkg.tools![id]!
      const transport = tool.kind === `script` ? `script` : tool.url !== undefined ? `http` : `stdio`
      const target = tool.url ?? [tool.command ?? ``, ...(tool.args ?? [])].map(shellArg).join(` `)
      return { id, kind: tool.kind, transport, title: tool.title, description: tool.description ?? null, target, env: [...(tool.env ?? [])] }
    })
}

/** The key a remembered grant is stored under: ANY change to the tool's
 *  definition is a new key (the card asks again). */
export function toolConsentKey(packageId: string, toolId: string, tool: PackageTool): string {
  return `${packageId}/${toolId}:${canonicalJson(tool)}`
}

/** The tool a host function name calls: `tools.<id>` (a script) or
 *  `tools.<id>.<name>` (an MCP server's tool); null = not a tool call. */
export function toolCall(name: string): { toolId: string; tool: string | null } | null {
  const m = /^tools\.([a-z][a-z0-9_-]{0,31})(?:\.(.+))?$/.exec(name)
  return m ? { toolId: m[1]!, tool: m[2] ?? null } : null
}
