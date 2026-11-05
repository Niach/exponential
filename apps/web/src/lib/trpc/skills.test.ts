import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// #EXP-1656 skills router (fixtures/skills.json): member reads, `get`
// returns the body, owner-only writes, Claude's name rule, names unique per
// team; `resolveForLaunch` hands the X-Exp-Session-Id session's picks.
// Same queued select-chain harness as prompts.test.ts.

const h = vi.hoisted(() => {
  const selectResults: unknown[][] = []
  const inserts: Record<string, unknown>[] = []
  const updates: Record<string, unknown>[] = []
  const deletes: unknown[] = []
  const makeChain = () => {
    const chain = {
      from: () => chain,
      where: () => chain,
      orderBy: () => chain,
      limit: () => Promise.resolve(selectResults.shift() ?? []),
      // list awaits after orderBy (no .limit) — make the chain thenable.
      then: (resolve: (rows: unknown[]) => unknown, reject?: () => unknown) =>
        Promise.resolve(selectResults.shift() ?? []).then(resolve, reject),
    }
    return chain
  }
  return {
    assertTeamMember: vi.fn(async () => ({ role: `member` }) as unknown),
    assertTeamOwner: vi.fn(async () => ({ role: `owner` }) as unknown),
    isSessionPrincipal: vi.fn(async () => true),
    selectResults,
    inserts,
    updates,
    deletes,
    fakeDb: (() => {
      const fakeDb: Record<string, unknown> = {
        select: () => makeChain(),
        insert: () => ({
          values: (values: Record<string, unknown>) => {
            inserts.push(values)
            return {
              onConflictDoNothing: () => ({
                returning: async () => [{ id: `new-skill`, ...values }],
              }),
            }
          },
        }),
        update: () => ({
          set: (values: Record<string, unknown>) => {
            updates.push(values)
            return {
              where: () => ({
                returning: async () => [{ id: `updated-skill`, ...values }],
              }),
            }
          },
        }),
        delete: (table: unknown) => {
          deletes.push(table)
          return { where: async () => undefined }
        },
      }
      // EXP-707: writes run in a txId-minting transaction now.
      fakeDb.transaction = async (fn: (tx: unknown) => Promise<unknown>) =>
        fn(fakeDb)
      return fakeDb
    })(),
  }
})

vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: h.assertTeamMember,
  assertTeamOwner: h.assertTeamOwner,
}))
// loadSkill imports the db lazily — same fake.
vi.mock(`@/db/connection`, () => ({ db: h.fakeDb }))
// EXP-707: writes mint a txId sync barrier.
vi.mock(`@/lib/sessions/host`, () => ({ isSessionPrincipal: h.isSessionPrincipal }))
vi.mock(`@/lib/trpc`, async (importOriginal) => {
  const mod = await importOriginal<Record<string, unknown>>()
  return { ...mod, generateTxId: async () => 42 }
})

import { skillsRouter } from "@/lib/trpc/skills"
import { builtinSkill } from "@/lib/skills/builtin"
import { skills } from "@/db/schema"

const { selectResults, inserts, updates, deletes, fakeDb } = h

const TEAM_ID = `11111111-1111-4111-8111-111111111111`
const SKILL_ID = `22222222-2222-4222-8222-222222222222`
const OTHER_ID = `33333333-3333-4333-8333-333333333333`
const SESSION_ID = `44444444-4444-4444-8444-444444444444`

const callerWith = (headers: Record<string, string> = {}) =>
  skillsRouter.createCaller({
    session: { user: { id: `actor`, name: `Actor` } },
    db: fakeDb,
    request: new Request(`http://localhost/`, { headers }),
  } as never)
const caller = callerWith()

async function rejectionOf(promise: Promise<unknown>): Promise<unknown> {
  return promise.then(
    () => undefined,
    (e: unknown) => e
  )
}
const codeOf = async (promise: Promise<unknown>) => ((await rejectionOf(promise)) as TRPCError).code

const row = (over: Record<string, unknown> = {}) => ({
  id: SKILL_ID,
  teamId: TEAM_ID,
  name: `review-checklist`,
  description: `Review a PR against the team checklist.`,
  body: `# Review`,
  enabledByDefault: false,
  builtin: false,
  ...over,
})

const valid = { teamId: TEAM_ID, name: `review-checklist`, description: `Review PRs.`, body: `# Review` }

beforeEach(() => {
  selectResults.length = 0
  inserts.length = 0
  updates.length = 0
  deletes.length = 0
  h.assertTeamMember.mockClear()
  h.assertTeamOwner.mockClear()
  h.assertTeamOwner.mockResolvedValue({ role: `owner` })
  h.isSessionPrincipal.mockResolvedValue(true)
})

describe(`skills.list / get`, () => {
  it(`lists the team's rows, member-gated`, async () => {
    selectResults.push([{ id: SKILL_ID, teamId: TEAM_ID, name: `review-checklist` }])
    const { skills: rows } = await caller.list({ teamId: TEAM_ID })
    expect(rows).toEqual([{ id: SKILL_ID, teamId: TEAM_ID, name: `review-checklist` }])
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, TEAM_ID)
  })

  it(`get returns the body, member-gated; a vanished row 404s`, async () => {
    selectResults.push([row()])
    const { skill } = await caller.get({ id: SKILL_ID })
    expect(skill.body).toBe(`# Review`)
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, TEAM_ID)
    selectResults.push([])
    expect(await codeOf(caller.get({ id: SKILL_ID }))).toBe(`NOT_FOUND`)
  })
})

describe(`skills.create`, () => {
  it(`persists the fields, stamps the creator, owner-gated`, async () => {
    const { skill } = await caller.create({ ...valid, enabledByDefault: true })
    expect(inserts[0]).toMatchObject({
      teamId: TEAM_ID,
      name: `review-checklist`,
      description: `Review PRs.`,
      body: `# Review`,
      enabledByDefault: true,
      createdById: `actor`,
    })
    expect(skill).toMatchObject({ name: `review-checklist`, body: `# Review` })
    expect(h.assertTeamOwner).toHaveBeenCalledWith(`actor`, TEAM_ID)
  })

  it(`defaults enabledByDefault to false`, async () => {
    await caller.create(valid)
    expect(inserts[0]).toMatchObject({ enabledByDefault: false })
  })

  it(`refuses names outside Claude's rule, a blank description or body, and files`, async () => {
    for (const input of [
      { ...valid, name: `Review PR` },
      { ...valid, name: `under_score` },
      { ...valid, name: `a`.repeat(65) },
      { ...valid, description: `  ` },
      { ...valid, description: `d`.repeat(1025) },
      { ...valid, body: ` \n` },
      { ...valid, body: `é`.repeat(32769) },
      { ...valid, files: [{ path: `x.py`, content: `` }] },
    ]) {
      expect(await codeOf(caller.create(input as never))).toBe(`BAD_REQUEST`)
    }
    expect(inserts).toHaveLength(0)
  })

  it(`takes exactly the byte limit`, async () => {
    await caller.create({ ...valid, body: `é`.repeat(32768) })
    expect(inserts).toHaveLength(1)
  })

  it(`refuses a non-owner before any write`, async () => {
    h.assertTeamOwner.mockRejectedValueOnce(new TRPCError({ code: `FORBIDDEN`, message: `Owner only` }))
    expect(await codeOf(caller.create(valid))).toBe(`FORBIDDEN`)
    expect(inserts).toHaveLength(0)
  })

  it(`answers a duplicate name with CONFLICT`, async () => {
    const insert = fakeDb.insert as () => unknown
    fakeDb.insert = () => ({
      values: () => ({ onConflictDoNothing: () => ({ returning: async () => [] }) }),
    })
    try {
      expect(await codeOf(caller.create(valid))).toBe(`CONFLICT`)
    } finally {
      fakeDb.insert = insert
    }
  })
})

describe(`skills.update / delete`, () => {
  it(`writes only the given fields, owner-gated on the row's team`, async () => {
    selectResults.push([row()])
    await caller.update({ id: SKILL_ID, description: `New`, enabledByDefault: true })
    expect(updates[0]).toMatchObject({ description: `New`, enabledByDefault: true })
    expect(Object.keys(updates[0]!).sort()).toEqual([`description`, `enabledByDefault`, `updatedAt`])
    expect(h.assertTeamOwner).toHaveBeenCalledWith(`actor`, TEAM_ID)
  })

  it(`returns the row unchanged for an empty patch`, async () => {
    selectResults.push([row()])
    const { skill } = await caller.update({ id: SKILL_ID })
    expect(skill).toMatchObject({ id: SKILL_ID, body: `# Review` })
    expect(updates).toHaveLength(0)
  })

  it(`refuses a rename onto another skill's name`, async () => {
    selectResults.push([row()], [{ id: OTHER_ID }])
    expect(await codeOf(caller.update({ id: SKILL_ID, name: `taken` }))).toBe(`CONFLICT`)
    expect(updates).toHaveLength(0)
  })

  it(`delete removes the row, owner-gated; a non-owner is refused`, async () => {
    selectResults.push([row()])
    await caller.delete({ id: SKILL_ID })
    expect(deletes).toEqual([skills])
    selectResults.push([row()])
    h.assertTeamOwner.mockRejectedValueOnce(new TRPCError({ code: `FORBIDDEN`, message: `Owner only` }))
    expect(await codeOf(caller.delete({ id: SKILL_ID }))).toBe(`FORBIDDEN`)
    expect(deletes).toHaveLength(1)
  })
})

describe(`skills.resolveForLaunch`, () => {
  const run = (over: Record<string, unknown> = {}) => ({
    id: SESSION_ID,
    userId: `actor`,
    deviceId: `dev-1`,
    teamId: TEAM_ID,
    status: `running`,
    skillIds: [OTHER_ID, SKILL_ID],
    ...over,
  })
  const launcher = callerWith({ "x-exp-session-id": SESSION_ID })

  it(`needs the launcher's session header`, async () => {
    expect(await codeOf(caller.resolveForLaunch({}))).toBe(`FORBIDDEN`)
  })

  it(`hands the picks in pick order with their bodies; a deleted one is skipped`, async () => {
    selectResults.push([run({ skillIds: [OTHER_ID, SKILL_ID, `55555555-5555-4555-8555-555555555555`] })])
    selectResults.push([
      { id: SKILL_ID, name: `review-checklist`, description: `d`, body: `# Review` },
      { id: OTHER_ID, name: `api-style`, description: `d2`, body: `# API` },
    ])
    const out = await launcher.resolveForLaunch({})
    expect(out.skills.map((s) => s.name)).toEqual([`api-style`, `review-checklist`])
    expect(out.skills[1]).toEqual({ id: SKILL_ID, name: `review-checklist`, description: `d`, body: `# Review`, files: [] })
    expect(out.skipped).toEqual([`55555555-5555-4555-8555-555555555555`])
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, TEAM_ID)
  })

  it(`no pick = nothing, without reading skills`, async () => {
    selectResults.push([run({ skillIds: null })])
    expect(await launcher.resolveForLaunch({})).toEqual({ skills: [], skipped: [] })
  })

  it(`refuses an ended session or someone else's`, async () => {
    selectResults.push([run({ status: `ended` })])
    expect(await codeOf(launcher.resolveForLaunch({}))).toBe(`FORBIDDEN`)
    selectResults.push([run()])
    h.isSessionPrincipal.mockResolvedValueOnce(false)
    expect(await codeOf(launcher.resolveForLaunch({}))).toBe(`FORBIDDEN`)
    selectResults.push([])
    expect(await codeOf(launcher.resolveForLaunch({}))).toBe(`FORBIDDEN`)
  })
})

// #VAPP-213: the built-ins (fixtures/skills.json `builtin`).
describe(`built-in skills`, () => {
  const apps = builtinSkill(`exponential-apps`)!
  const BUILTIN_ID = `66666666-6666-4666-8666-666666666666`
  const builtinRow = row({ id: BUILTIN_ID, name: `exponential-apps`, body: `stale`, builtin: true })

  it(`get serves the shipped body and lists the files`, async () => {
    selectResults.push([builtinRow])
    const { skill } = await caller.get({ id: BUILTIN_ID })
    expect(skill.body).toBe(apps.body)
    expect(skill.files).toContain(`examples/crm.app.json`)
    expect(skill.files).toContain(`references/records.md`)
  })

  it(`a team skill lists no files`, async () => {
    selectResults.push([row()])
    expect((await caller.get({ id: SKILL_ID })).skill.files).toEqual([])
  })

  it(`file returns one file; an unknown path or a team skill 404s`, async () => {
    selectResults.push([builtinRow])
    const file = await caller.file({ id: BUILTIN_ID, path: `references/records.md` })
    expect(file.content).toBe(apps.files.find((f) => f.path === `references/records.md`)!.content)
    selectResults.push([builtinRow])
    expect(await codeOf(caller.file({ id: BUILTIN_ID, path: `nope.md` }))).toBe(`NOT_FOUND`)
    selectResults.push([row()])
    expect(await codeOf(caller.file({ id: SKILL_ID, path: `references/records.md` }))).toBe(`NOT_FOUND`)
  })

  it(`a built-in's name is taken: create and rename onto it are CONFLICT`, async () => {
    expect(await codeOf(caller.create({ ...valid, name: `exponential-genui` }))).toBe(`CONFLICT`)
    expect(inserts).toHaveLength(0)
    selectResults.push([row()])
    expect(await codeOf(caller.update({ id: SKILL_ID, name: `exponential-motion` }))).toBe(`CONFLICT`)
    expect(updates).toHaveLength(0)
  })

  it(`a built-in is read-only, owners included`, async () => {
    selectResults.push([builtinRow])
    expect(await codeOf(caller.update({ id: BUILTIN_ID, description: `mine now` }))).toBe(`FORBIDDEN`)
    selectResults.push([builtinRow])
    expect(await codeOf(caller.update({ id: BUILTIN_ID, enabledByDefault: true }))).toBe(`FORBIDDEN`)
    selectResults.push([builtinRow])
    expect(await codeOf(caller.delete({ id: BUILTIN_ID }))).toBe(`FORBIDDEN`)
    expect(updates).toHaveLength(0)
    expect(deletes).toHaveLength(0)
  })

  it(`an app editor session always gets the built-ins after its picks, with their files`, async () => {
    const launcher = callerWith({ "x-exp-session-id": SESSION_ID })
    const UI_ID = `77777777-7777-4777-8777-777777777777`
    const MOTION_ID = `88888888-8888-4888-8888-888888888888`
    selectResults.push([
      { id: SESSION_ID, userId: `actor`, deviceId: `dev-1`, teamId: TEAM_ID, status: `running`, skillIds: [SKILL_ID, UI_ID], appId: `app-1` },
    ])
    // the team's built-in rows (sessionSkillIds), then the picked rows
    selectResults.push([
      { id: MOTION_ID, name: `exponential-motion` },
      { id: UI_ID, name: `exponential-genui` },
      { id: BUILTIN_ID, name: `exponential-apps` },
    ])
    selectResults.push([
      { id: SKILL_ID, name: `review-checklist`, description: `d`, body: `# Review`, builtin: false },
      { id: BUILTIN_ID, name: `exponential-apps`, description: `old`, body: `stale`, builtin: true },
      { id: UI_ID, name: `exponential-genui`, description: `old`, body: `stale`, builtin: true },
      { id: MOTION_ID, name: `exponential-motion`, description: `old`, body: `stale`, builtin: true },
    ])
    const out = await launcher.resolveForLaunch({})
    expect(out.skills.map((s) => s.name)).toEqual([`review-checklist`, `exponential-genui`, `exponential-apps`, `exponential-motion`])
    const shipped = out.skills.find((s) => s.name === `exponential-apps`)!
    expect(shipped.body).toBe(apps.body)
    expect(shipped.description).toBe(apps.description)
    expect(shipped.files.map((f) => f.path)).toEqual(apps.files.map((f) => f.path))
    expect(out.skipped).toEqual([])
  })
})
