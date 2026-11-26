// #NEW-148 / #NEW-143 D1: the device hub as the MCP CLIENT of team servers
// (host side): tool names on the one `team` entry, the elicit results, the
// task poll clamp + trace rows, and the skills it pulls: verification, the
// digest set (the cache key), the session dir name and the rewritten
// SKILL.md. Pure and BROWSER-SAFE (@noble, never `node:crypto`). Locked by
// `fixtures/hub-client.json` (its `$comment` is the spec); the `emcp` crate
// replays the same fixture.
import { sha256 } from "@noble/hashes/sha2.js"
import { bytesToHex } from "@noble/hashes/utils.js"

import fixture from "../fixtures/hub-client.json" with { type: "json" }
import { canonicalJson } from "./pure"

export const HUB_TEAM_ENTRY: string = fixture.entry.name
export const HUB_TEAM_TOKEN_ENV: string = fixture.entry.tokenEnv
export const HUB_CLIENT_CAPABILITIES = fixture.clientCapabilities
export const HUB_MRTR_MAX_ROUNDS: number = fixture.mrtr.maxRounds
export const HUB_TASKS = fixture.tasks
export const HUB_SKILLS = fixture.skills
export const HUB_COPY = fixture.copy

const utf8 = new TextEncoder()
const hex = (bytes: Uint8Array) => bytesToHex(sha256(bytes))
const asBytes = (bytes: string | Uint8Array) => (typeof bytes === `string` ? utf8.encode(bytes) : bytes)
const compare = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0)

/** A team tool's name as each CLI sees it on the hub's one `team` entry. */
export function hubToolNames(server: string, tool: string) {
  const hubTool = `${server}__${tool}`
  return { claude: `mcp__${HUB_TEAM_ENTRY}__${hubTool}`, codex: { server: HUB_TEAM_ENTRY, tool: hubTool } }
}

/** One manifest row of a skill (skills.json `resources`). */
export interface SkillResource {
  uri: string
  digest: string
  size: number
}

/** The skill's digest set: `sha256:` + hex SHA-256 of the canonical JSON of
 *  its manifest rows {digest, size, uri} sorted by uri. */
export function digestSet(resources: readonly SkillResource[]): string {
  const rows = [...resources]
    .sort((a, b) => compare(a.uri, b.uri))
    .map((r) => ({ digest: r.digest, size: r.size, uri: r.uri }))
  return `sha256:${hex(utf8.encode(canonicalJson(rows)))}`
}

/** One `resources/read` answer against its manifest row: size first, then
 *  the digest. null = verified. */
export function verifySkillFile(entry: SkillResource, bytes: string | Uint8Array): `size` | `digest` | null {
  const data = asBytes(bytes)
  if (data.length !== entry.size) return `size`
  if (`sha256:${hex(data)}` !== entry.digest) return `digest`
  return null
}

/** The session skill dir of a server's skill: `mcp-<server>-<name>`; past
 *  64 chars cut to 55, trailing `-` trimmed, `-` + 8 hex of the full name's SHA-256. */
export function skillDirName(server: string, name: string): string {
  const full = `${HUB_SKILLS.dirPrefix}${server}-${name}`
  if (full.length <= HUB_SKILLS.nameMax) return full
  const head = full.slice(0, HUB_SKILLS.cutAt).replace(/-+$/, ``)
  return `${head}-${hex(utf8.encode(full)).slice(0, HUB_SKILLS.hashHex)}`
}

/** A served SKILL.md rewritten for the session: frontmatter exactly
 *  {name: dirName, description}, every other key (`allowed-tools`) dropped,
 *  the body kept. null = no frontmatter (not a skill). */
export function hubSkillMd(dirName: string, text: string): string | null {
  const src = text.replace(/\r\n/g, `\n`)
  if (!src.startsWith(`---\n`)) return null
  const end = src.indexOf(`\n---`, 3)
  if (end < 0) return null
  const after = src.slice(end + 4)
  if (after !== `` && !after.startsWith(`\n`)) return null
  const front = src.slice(4, end)
  let description = ``
  for (const line of front.split(`\n`)) {
    const m = /^description:\s*(.*)$/.exec(line)
    if (!m) continue
    const raw = m[1]!.trim()
    if (raw.startsWith(`"`)) {
      try {
        description = String(JSON.parse(raw))
      } catch {
        description = raw
      }
    } else if (raw.startsWith(`'`) && raw.endsWith(`'`) && raw.length >= 2) {
      description = raw.slice(1, -1).replace(/''/g, `'`)
    } else {
      description = raw
    }
  }
  return `---\nname: ${dirName}\ndescription: ${JSON.stringify(description)}\n---\n\n${after.trim()}\n`
}

/** A task's poll interval, clamped to [min, max]; absent = the default. */
export function clampPollMs(pollIntervalMs: number | null | undefined): number {
  const { default: fallback, min, max } = HUB_TASKS.poll
  if (typeof pollIntervalMs !== `number` || !Number.isFinite(pollIntervalMs)) return fallback
  return Math.min(max, Math.max(min, pollIntervalMs))
}

export type HubTaskStatus = keyof typeof fixture.tasks.traceStatus

/** The session TraceItem of a hub-driven task. */
export function taskTrace(args: { server: string; taskId: string; tool: string; statusMessage?: string | null; status: HubTaskStatus }) {
  const message = args.statusMessage?.trim()
  return {
    id: `${HUB_TASKS.traceIdPrefix}${args.server}:${args.taskId}`,
    title: `${args.server}: ${message ? message : args.tool}`,
    status: HUB_TASKS.traceStatus[args.status],
  }
}

export type ElicitAction = { accept: { content?: Record<string, unknown> } } | `decline` | `cancel` | null

/** The person's answer to an elicitation as the ElicitResult (no live
 *  session to ask = cancel). */
export function elicitResult(action: ElicitAction): { action: `accept` | `decline` | `cancel`; content?: Record<string, unknown> } {
  if (action === `decline` || action === `cancel`) return { action }
  if (action && typeof action === `object` && action.accept) {
    return action.accept.content !== undefined ? { action: `accept`, content: action.accept.content } : { action: `accept` }
  }
  return { action: `cancel` }
}
