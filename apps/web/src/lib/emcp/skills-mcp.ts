// #NEW-147 (packages/emcp fixtures/skills.json, D2-D3 of #NEW-143): Skills
// over MCP (`io.modelcontextprotocol/skills`) on `/api/mcp`. SDK 2.4 ships a
// `SkillsExtension`, but one catalog carries ONE cache hint while the
// fixture wants `private` for a team skill and `public` for a built-in, and
// its error wording and directory listing differ; so the methods are hand
// rolled on the per-request server: `skills/list`, `skills/get`,
// `resources/directory/read` as request handlers, `resources/read` of a
// `skill://` URI as a resource template. Every answer is the pure
// reference's (`@exp/emcp/skills`) over what this caller is served: the own
// skills of every team its credential reaches (`resolveTeamAccess`, a
// confined grant's clamp included), then the built-ins genUI and workflow.
// #EXP-1212: never inside our own sessions (no resources there).
import { ProtocolError, ResourceTemplate, type McpServer } from "@modelcontextprotocol/server"
import { z } from "zod"
import { and, asc, eq, inArray } from "drizzle-orm"
import {
  MCP_BUILTIN_NAMES,
  servedSkills,
  skillDirectoryRead,
  skillResourceRead,
  skillsGet,
  skillsList,
  SKILLS_EXTENSION,
  type BuiltinSource,
  type ReachableTeam,
  type ServedSkill,
} from "@exp/emcp/skills"
import { BUILTIN_SKILL_SOURCES } from "@exponential_app/eui/skills"
import { db } from "@/db/connection"
import { skills, teams } from "@/db/schema"
import { resolveTeamAccess } from "@/lib/auth/access"
import { getUserTeamIds } from "@/lib/auth/membership"
import type { McpAccess } from "./scope"

const BUILTINS: readonly BuiltinSource[] = BUILTIN_SKILL_SOURCES.filter((source) => MCP_BUILTIN_NAMES.includes(source.name)).map(
  (source) => ({ name: source.name, description: source.description, skillMd: source.skillMd, files: source.files })
)

/** The teams whose own skills this caller is served: its member teams its
 *  grant reaches whole, each passing `resolveTeamAccess` (read). */
export async function reachableSkillTeams(userId: string, access: McpAccess): Promise<string[]> {
  const memberOf = await getUserTeamIds(userId)
  const candidates = access.full ? memberOf : memberOf.filter((id) => access.fullTeamIds.has(id))
  const reached: string[] = []
  for (const teamId of candidates) {
    const ok = await resolveTeamAccess(userId, teamId, `read`).then(
      () => true,
      () => false
    )
    if (ok) reached.push(teamId)
  }
  return reached
}

/** Everything this caller is served, in the fixture's order. */
export async function loadServedSkills(userId: string, access: McpAccess): Promise<ServedSkill[]> {
  const teamIds = await reachableSkillTeams(userId, access)
  const reachable: ReachableTeam[] = []
  if (teamIds.length > 0) {
    const teamRows = await db.select({ id: teams.id, slug: teams.slug }).from(teams).where(inArray(teams.id, teamIds))
    const rows = await db
      .select({ teamId: skills.teamId, name: skills.name, description: skills.description, body: skills.body, builtin: skills.builtin })
      .from(skills)
      .where(and(inArray(skills.teamId, teamIds), eq(skills.builtin, false)))
      .orderBy(asc(skills.name))
    for (const team of teamRows) {
      reachable.push({ slug: team.slug, skills: rows.filter((row) => row.teamId === team.id) })
    }
  }
  return servedSkills(reachable, BUILTINS)
}

type Answer = Record<string, unknown> | { error: { code: number; message: string } }

/** A reference answer as the handler's result, or its JSON-RPC error. */
function answer<T extends Answer>(out: T): Record<string, unknown> {
  if (`error` in out && out.error && typeof out.error === `object`) {
    const error = out.error as { code: number; message: string }
    throw new ProtocolError(error.code, error.message)
  }
  return out as Record<string, unknown>
}

const looseParams = z.looseObject({})

/** Serves the skills methods on this request's server (external callers only). */
export function registerSkills(server: McpServer, userId: string, access: McpAccess): void {
  let served: Promise<ServedSkill[]> | null = null
  const load = () => (served ??= loadServedSkills(userId, access))
  const low = server.server
  low.setRequestHandler(`skills/list`, { params: looseParams }, async (params: Record<string, unknown>) =>
    answer(skillsList(await load(), { cursor: params.cursor }))
  )
  low.setRequestHandler(`skills/get`, { params: looseParams }, async (params: Record<string, unknown>) =>
    answer(skillsGet(await load(), params.uri))
  )
  low.setRequestHandler(`resources/directory/read`, { params: looseParams }, async (params: Record<string, unknown>) =>
    answer(skillDirectoryRead(await load(), params.uri))
  )
  server.registerResource(
    SKILLS_EXTENSION,
    new ResourceTemplate(`skill://{+path}`, { list: undefined }),
    { title: `Skill file` },
    async (uri) => answer(skillResourceRead(await load(), uri.href)) as never
  )
}
