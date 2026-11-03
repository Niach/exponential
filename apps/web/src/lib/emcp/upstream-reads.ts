// #VAPP-230 (eApps M1, apps.md §6.3): a template's `upstream:<server>/<tool>`
// read binding, run by the server broker as the VIEWING member with their own
// connection (@exp/emcp upstream.json `binding`: its `$comment` is the spec).
// One read = one tool of one of the app's own upstreams; nothing chains. A
// good answer is cached 60 s per (member, server, tool, args); errors are not.
import { and, eq } from "drizzle-orm"
import { PROVENANCE_KEYS } from "@exp/emcp/exposes"
import {
  UPSTREAM_BINDING,
  UPSTREAM_ERRORS,
  parseUpstreamSource,
  upstreamBindingValue,
  upstreamCacheKey,
  upstreamReadError,
  type UpstreamReadAnswer,
} from "@exp/emcp/upstream"
import { sourceAllowed } from "@exponential_app/eui/host"
import { db } from "@/db/connection"
import { mcpServers, type App } from "@/db/schema"
import { resolveForLaunch } from "@/lib/emcp/upstream/credentials"
import { callUpstreamTool } from "@/lib/emcp/upstream/mcp-client"

/** What a host gets back: the answer + the provenance of an ok one. */
export type UpstreamRead = (UpstreamReadAnswer & { message?: string }) & { meta?: Record<string, string> }

interface Cached {
  at: number
  value: unknown
}

/** The cache's cap: the oldest entry goes first beyond it. */
export const UPSTREAM_CACHE_MAX = 2000
const cache = new Map<string, Cached>()
const inFlight = new Map<string, Promise<UpstreamRead>>()

/** Tests: start from an empty cache. */
export function clearUpstreamReadCache(): void {
  cache.clear()
  inFlight.clear()
}

function remember(key: string, value: unknown, now: number) {
  cache.delete(key)
  cache.set(key, { at: now, value })
  while (cache.size > UPSTREAM_CACHE_MAX) cache.delete(cache.keys().next().value!)
}

type PackageView = { upstreams?: unknown; sources?: unknown }

const strings = (v: unknown): string[] => (Array.isArray(v) ? v.filter((s): s is string => typeof s === `string`) : [])

/** The package that permits the read: the published one, else the head
 *  (the editor's draft preview), whichever declares the server and lists
 *  the source. */
function permittingPackage(app: Pick<App, `package` | `draftPackage`>, source: string, server: string): PackageView | null {
  for (const pkg of [app.package, app.draftPackage] as (PackageView | null)[]) {
    if (!pkg) continue
    if (!strings(pkg.upstreams).includes(server)) continue
    const sources = Array.isArray(pkg.sources) ? strings(pkg.sources) : undefined
    if (sources !== undefined && !sourceAllowed(sources, source)) continue
    return pkg
  }
  return null
}

const refusal = (error: string, server: string, message: string): UpstreamRead => ({ ok: false, error, server, message })

/** One read for `userId` (a member with read access to `app`: the caller
 *  checks). `now` = ms (tests). */
export async function readUpstreamBinding(
  userId: string,
  app: Pick<App, `id` | `slug` | `teamId` | `package` | `draftPackage`>,
  source: string,
  now: number = Date.now()
): Promise<UpstreamRead> {
  const binding = parseUpstreamSource(source)
  if (!binding) return refusal(UPSTREAM_BINDING.errors.invalid, ``, `not an upstream:<server>/<tool> source`)
  const pkg = permittingPackage(app, source, binding.server)
  if (!pkg) return refusal(UPSTREAM_BINDING.errors.notDeclared, binding.server, `the app does not declare ${binding.server} in its upstreams and sources`)
  const [server] = await db
    .select({ id: mcpServers.id, transport: mcpServers.transport })
    .from(mcpServers)
    .where(and(eq(mcpServers.teamId, app.teamId), eq(mcpServers.name, binding.server)))
    .limit(1)
  if (!server) return refusal(UPSTREAM_ERRORS.missing, binding.server, `the team has no MCP server ${binding.server}`)
  if (server.transport !== `http`) return refusal(UPSTREAM_ERRORS.failed, binding.server, `${binding.server} runs on a device (stdio); the server reaches only http servers`)
  const meta = {
    [PROVENANCE_KEYS.app]: app.slug,
    [PROVENANCE_KEYS.tool]: `${binding.server}/${binding.tool}`,
    [PROVENANCE_KEYS.via]: `upstream`,
    [PROVENANCE_KEYS.upstream]: binding.server,
  }
  const key = upstreamCacheKey(userId, server.id, binding.tool, binding.args)
  const hit = cache.get(key)
  if (hit && now - hit.at < UPSTREAM_BINDING.cacheSeconds * 1000) return { ok: true, value: hit.value, meta }
  const pending = inFlight.get(key)
  if (pending) return pending
  const run = (async (): Promise<UpstreamRead> => {
    const launch = await resolveForLaunch(db, { userId, teamIds: [app.teamId], serverIds: [server.id] })
    const entry = launch.servers[0]
    if (!entry?.url) return refusal(UPSTREAM_ERRORS.disconnected, binding.server, `connect your ${binding.server} account first`)
    const headers = Object.fromEntries(entry.headers.map((h) => [h.name, h.value]))
    const sources = strings(pkg.sources)
    const called = await callUpstreamTool(entry.url, headers, binding.tool, binding.args, (annotations) => upstreamReadError(binding, annotations, sources))
    if (!called.ok) {
      if (called.reason === `vetoed`) return refusal(called.error, binding.server, `${binding.server}'s ${binding.tool} is not a read-only tool`)
      if (called.reason === `refused`) return refusal(UPSTREAM_ERRORS.disconnected, binding.server, `${binding.server} refused your connection; reconnect`)
      return refusal(UPSTREAM_ERRORS.failed, binding.server, `${binding.server}: ${called.error}`)
    }
    const value = upstreamBindingValue(called.result)
    if (`error` in value) return refusal(value.error, binding.server, `${binding.server}'s ${binding.tool} answered an error`)
    remember(key, value.value, now)
    return { ok: true, value: value.value, meta }
  })()
  inFlight.set(key, run)
  try {
    return await run
  } finally {
    inFlight.delete(key)
  }
}
