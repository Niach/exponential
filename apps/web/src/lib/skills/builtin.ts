import { and, eq, inArray, sql } from "drizzle-orm"
import { BUILTIN_SKILLS } from "@exp/domain-contract/skills"
import {
  BUILTIN_SKILL_SOURCES,
  BUILTIN_SKILLS_VERSION,
  type BuiltinSkillFile,
} from "@exponential_app/eui/skills"
import { skills, teams } from "@/db/schema"
import type { Tx } from "@/lib/sessions/tx"

// #VAPP-213: the BUILT-IN skills (fixtures/skills.json `builtin`): a
// generated set shipped with the server (packages/exponential-ui/skills,
// versioned with eUI) held as an ordinary `skills` row with builtin = true
// in EVERY team, so picks, agents, the shape and resolveForLaunch need
// nothing new. Rows are seeded on team create and reconciled on every boot
// (name, description, body); a team's own skill already holding a built-in
// name keeps it (that team gets no copy). Body and files are always served
// from the shipped set, never the row.

export interface BuiltinSkill {
  name: string
  description: string
  /** The SKILL.md body (after the frontmatter). */
  body: string
  files: BuiltinSkillFile[]
}

/** The SKILL.md body: what follows the `---` frontmatter. */
function stripFrontmatter(skillMd: string): string {
  const m = /^---\n[\s\S]*?\n---\n+/.exec(skillMd)
  return m ? skillMd.slice(m[0].length) : skillMd
}

const BY_NAME = new Map<string, BuiltinSkill>(
  BUILTIN_SKILL_SOURCES.map((source) => [
    source.name,
    { name: source.name, description: source.description, body: stripFrontmatter(source.skillMd), files: source.files },
  ])
)

export const BUILTIN_VERSION = BUILTIN_SKILLS_VERSION

/** Every built-in, in the fixture's name order. */
export const BUILTINS: BuiltinSkill[] = BUILTIN_SKILLS.names.map((name) => {
  const skill = BY_NAME.get(name)
  if (!skill) throw new Error(`built-in skill "${name}" is not in the shipped set (regenerate packages/exponential-ui/skills)`)
  return skill
})

export function builtinSkill(name: string): BuiltinSkill | null {
  return BY_NAME.get(name) ?? null
}

export const isBuiltinName = (name: string) => BY_NAME.has(name)

/** The names an app editor session always runs with, in delivery order. */
export const EDITOR_DELIVERY: readonly string[] = BUILTIN_SKILLS.editorDelivery

type Db = Tx | (typeof import("@/db/connection"))[`db`]

/** Insert the built-ins a team lacks (a name it already uses is skipped). */
export async function seedBuiltinSkills(tx: Db, teamIds: readonly string[]): Promise<void> {
  if (teamIds.length === 0) return
  const values = teamIds.flatMap((teamId) =>
    BUILTINS.map((skill) => ({
      teamId,
      name: skill.name,
      description: skill.description,
      body: skill.body,
      enabledByDefault: false,
      builtin: true,
      createdById: null,
    }))
  )
  await tx.insert(skills).values(values).onConflictDoNothing({ target: [skills.teamId, skills.name] })
}

/** Boot: every team gets every built-in, and the built-in rows follow the
 *  shipped set (description, body; a retired built-in's rows go). Idempotent;
 *  a row that already matches is not touched (no Electric churn). `teamIds`
 *  narrows it (tests); unset = every team. */
export async function reconcileBuiltinSkills(teamIds?: readonly string[]): Promise<void> {
  const { db } = await import(`@/db/connection`)
  await db.transaction(async (tx) => {
    const scope = teamIds ? [inArray(skills.teamId, [...teamIds])] : []
    const ids = teamIds ? [...teamIds] : (await tx.select({ id: teams.id }).from(teams)).map((row) => row.id)
    await seedBuiltinSkills(tx, ids)
    for (const skill of BUILTINS) {
      await tx
        .update(skills)
        .set({ description: skill.description, body: skill.body, enabledByDefault: false, updatedAt: new Date() })
        .where(
          and(
            ...scope,
            eq(skills.builtin, true),
            eq(skills.name, skill.name),
            sql`(${skills.description} IS DISTINCT FROM ${skill.description} OR ${skills.body} IS DISTINCT FROM ${skill.body} OR ${skills.enabledByDefault})`
          )
        )
    }
    const retired = await tx
      .select({ name: skills.name })
      .from(skills)
      .where(and(...scope, eq(skills.builtin, true)))
      .groupBy(skills.name)
    const gone = retired.map((row) => row.name).filter((name) => !isBuiltinName(name))
    if (gone.length > 0) await tx.delete(skills).where(and(...scope, eq(skills.builtin, true), inArray(skills.name, gone)))
  })
}

/** A session's effective skill pick, in launch order: its own picks, then
 *  (an app editor session) the team's built-ins in `editorDelivery` order,
 *  deduped. `skills.resolveForLaunch` resolves it and `sessions.skillPicks`
 *  reads it back (sorted), so the device's between-turn comparison holds. */
export async function sessionSkillIds(
  tx: Db,
  session: { teamId: string; skillIds: string[] | null; appId: string | null }
): Promise<string[]> {
  const ids = [...new Set(session.skillIds ?? [])]
  if (!session.appId) return ids
  const rows = await tx
    .select({ id: skills.id, name: skills.name })
    .from(skills)
    .where(and(eq(skills.teamId, session.teamId), eq(skills.builtin, true), inArray(skills.name, [...EDITOR_DELIVERY])))
  for (const name of EDITOR_DELIVERY) {
    const row = rows.find((candidate) => candidate.name === name)
    if (row && !ids.includes(row.id)) ids.push(row.id)
  }
  return ids
}
