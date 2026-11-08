// @vitest-environment node
import { afterAll, afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { and, asc, eq, inArray, sql } from "drizzle-orm"
import type { TrainCheck, TrainPipelineInputs, TrainPipelineOutcome, TrainPipelineUnit } from "@exp/domain-contract/merge-train"
import { db } from "@/db/connection"
import { agents, boards, devices, mergeTrains, messages, pullRequestChecks, pullRequests, repositories, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { inSession } from "@/lib/sessions/messages"
import type { Forge } from "@/lib/ecode/forge"

// EXP-1709 against the REAL migrated Postgres with an in-memory forge: the
// worker composes, dispatches, reads the verdict, re-runs, bisects,
// fast-forwards, lands (pr_merged + the PR's merged line + Closed with a
// comment), bounces and holds, exactly as merge-train.json says.

vi.mock(`@/lib/pull-requests`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/pull-requests")>()),
  syncPullRequestBodies: async () => {},
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: async () => {},
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: false }),
}))
vi.mock(`@/lib/integrations/github-app`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/integrations/github-app")>()),
  resolveRepoInstallationToken: async () => null,
}))
vi.mock(`@/lib/integrations/github-client`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/integrations/github-client")>()),
  githubFetch: async () => new Response(`{}`, { status: 404 }),
}))

const ready = await probeRealDb(`
  SELECT (EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors')
    AND to_regclass('public.merge_trains') IS NOT NULL) AS ok
`)
if (!ready.ok) console.info(`[merge-train worker.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { recordPullRequestOpened } = await import(`@/lib/integrations/pr-sync`)
const worker = await import(`@/lib/merge-train/worker`)
const { setForgeForTests } = await import(`@/lib/ecode/forge`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

// ── the in-memory forge ──────────────────────────────────────────────────

interface FakePr {
  headSha: string
  title: string
  state: `open` | `closed`
  draft: boolean
}

class FakeForge {
  branch = new Map<string, string>([[`main`, `base0`]])
  prs = new Map<number, FakePr>()
  dispatches: Array<{ inputs: TrainPipelineInputs; units: TrainPipelineUnit[] }> = []
  closed: Array<{ number: number; sha: string }> = []
  squashed: number[] = []
  missingPipeline = false
  refuseFastForward = false
  /** Decides each run: which PR numbers conflict, and whether CI is red. */
  judge: (prs: number[]) => { conflicts?: number[]; siblings?: number[]; red?: boolean; error?: string } = () => ({})
  private outcomes = new Map<string, TrainPipelineOutcome>()
  private checks = new Map<string, TrainCheck[]>()

  forge(): Forge {
    return {
      kind: `fake`,
      readPullRequest: async (_repo, number) => {
        const pr = this.prs.get(number)
        if (!pr) return null
        return {
          number,
          state: pr.state,
          merged: false,
          draft: pr.draft,
          headSha: pr.headSha,
          headRef: `exp/${number}`,
          baseRef: `main`,
          title: pr.title,
          mergedAt: null,
          mergedBy: null,
          author: `dev <dev@example.com>`,
        }
      },
      readBranch: async (_repo, branch) => this.branch.get(branch) ?? null,
      requiredChecks: async () => [`gate`],
      dispatchPipeline: async (_repo, inputs) => {
        if (this.missingPipeline) return { ok: false, missing: true, message: `no train.yml` }
        const units = JSON.parse(inputs.units) as TrainPipelineUnit[]
        this.dispatches.push({ inputs, units })
        const numbers = units.flatMap((u) => u.prs.map((p) => p.number))
        const verdict = this.judge(numbers)
        const conflicts = new Set([...(verdict.conflicts ?? []), ...(verdict.siblings ?? [])])
        const siblings = new Set(verdict.siblings ?? [])
        const head = `train-${inputs.train_id.slice(0, 8)}`
        if (verdict.error) {
          this.outcomes.set(inputs.train_id, { train: inputs.train_id, base_sha: inputs.base_sha, head: null, checks: `none`, units: [], error: verdict.error })
          return { ok: true, runRef: null }
        }
        this.outcomes.set(inputs.train_id, {
          train: inputs.train_id,
          base_sha: inputs.base_sha,
          head,
          checks: inputs.mode === `untested` ? `skipped` : `dispatched`,
          units: units.map((u) => {
            const prs = u.prs.map((p) => p.number)
            const conflict = prs.some((n) => conflicts.has(n))
            const against = prs.some((n) => siblings.has(n)) ? { against: `batch` as const } : {}
            return { prs, outcome: conflict ? `conflict` : `composed`, files: conflict ? [`src/a.ts`] : [], resolved: [], ...against }
          }),
          error: null,
        })
        this.checks.set(head, [{ name: `gate`, status: `completed`, conclusion: verdict.red ? `failure` : `success` }])
        return { ok: true, runRef: null }
      },
      readPipeline: async (_repo, trainId) => {
        const outcome = this.outcomes.get(trainId) ?? null
        return { outcome, finished: !!outcome }
      },
      checksAt: async (_repo, sha) => this.checks.get(sha) ?? [],
      fastForward: async (_repo, { branch, from, to }) => {
        if (this.refuseFastForward) return { ok: false, reason: `refused`, message: `ruleset` }
        if (this.branch.get(branch) !== from) return { ok: false, reason: `moved`, current: this.branch.get(branch) ?? null }
        this.branch.set(branch, to)
        return { ok: true }
      },
      closeAsLanded: async (_repo, { number, sha }) => {
        this.closed.push({ number, sha })
        const pr = this.prs.get(number)
        if (pr) pr.state = `closed`
      },
      squashMerge: async (_repo, { number }) => {
        this.squashed.push(number)
        return { ok: true, sha: `sq-${number}` }
      },
    }
  }
}

let fake: FakeForge
beforeEach(() => {
  fake = new FakeForge()
  setForgeForTests(() => fake.forge())
  worker.resetMergeTrainForTests()
})
afterEach(() => {
  setForgeForTests(null)
})

// ── the world ────────────────────────────────────────────────────────────

async function world() {
  const owner = `mt-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${owner}, 'Tester', ${`${owner}@mt.test`}, true, false, now(), now())
  `)
  made.users.push(owner)
  const [team] = await db.insert(teams).values({ name: `MT`, slug: `mt-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  await db.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `MT` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const chat = (await sendSession({ userId: owner, input: { teamId, deviceId, text: `work` } })).sessionId
  const fullName = `o/mt-${id8()}`
  const [repo] = await db.insert(repositories).values({ teamId, fullName, defaultBranch: `main` }).returning({ id: repositories.id })
  const children: Record<number, string> = {}
  async function pr(number: number, opts: { base?: string } = {}) {
    const child = (await sendSession({ userId: owner, input: { parentId: chat, text: `part ${number}` } })).sessionId
    children[number] = child
    const { pullRequest } = await recordPullRequestOpened({
      teamId,
      rootSessionId: chat,
      sessionId: child,
      repositoryId: repo!.id,
      repoFullName: fullName,
      number,
      url: `https://github.com/${fullName}/pull/${number}`,
      title: `Part ${number}`,
      head: `exp/${number}`,
      base: opts.base ?? `main`,
      issueIds: [],
      actorUserId: owner,
    })
    fake.prs.set(number, { headSha: `head-${number}`, title: `Part ${number}`, state: `open`, draft: false })
    return pullRequest.id
  }
  // Every session idle: the opener's subtree does not hold the PR back.
  const idle = () => db.update(sessions).set({ status: `idle`, busy: false }).where(eq(sessions.rootId, chat))
  return { owner, teamId, deviceId, chat, repoId: repo!.id, fullName, pr, idle, children }
}

const prRow = async (id: string) => (await db.select().from(pullRequests).where(eq(pullRequests.id, id)))[0]!
const lines = (sessionId: string) =>
  db
    .select({ body: messages.body, card: messages.card, consumedAt: messages.consumedAt })
    .from(messages)
    .where(and(inSession(sessionId), sql`${messages.card}->>'event' = 'pr_merge'`))
    .orderBy(asc(messages.createdAt), asc(messages.id))
const pass = async (repoId: string, n = 1) => {
  for (let i = 0; i < n; i++) await worker.runRepository(repoId)
}

describe.skipIf(!ready.ok)(`EXP-1709: the merge train worker`, () => {
  it(`a conflict the train never tested (a pre-train row) is tried by the train (#EXP-1711)`, async () => {
    const w = await world()
    const a = await w.pr(241)
    await w.idle()
    await db.update(pullRequests).set({ mergeState: `conflict`, mergeHeadSha: null }).where(eq(pullRequests.id, a))
    await pass(w.repoId)
    expect(fake.dispatches.map((d) => d.units.map((u) => u.prs.map((p) => p.number)))).toEqual([[[241]]])
    expect((await prRow(a)).mergeState).toBe(`testing`)
  })

  it(`an approval team's ready PR waits on Needs approval; Hold holds it, Resume re-reads`, async () => {
    const w = await world()
    await db.update(teams).set({ mergeMode: `approval` }).where(eq(teams.id, w.teamId))
    const a = await w.pr(211)
    await w.idle()
    await pass(w.repoId)
    expect(await prRow(a)).toMatchObject({ mergeState: `checks`, mergeWait: `approval` })
    await worker.holdPullRequest(a)
    await worker.drainMergeTrainEdges()
    expect(await prRow(a)).toMatchObject({ mergeState: `held`, mergeHold: `person` })
    await pass(w.repoId)
    expect((await prRow(a)).mergeState).toBe(`held`)
    await worker.resumePullRequest(a)
    await worker.drainMergeTrainEdges()
    await pass(w.repoId)
    expect(await prRow(a)).toMatchObject({ mergeState: `checks`, mergeWait: `approval`, mergeHold: null })
    expect(fake.dispatches).toEqual([])
  })

  it(`an informational leg never holds a PR: only the required checks do (#EXP-1711)`, async () => {
    const w = await world()
    const a = await w.pr(221)
    await w.idle()
    const check = (name: string, status: `queued` | `completed` | `in_progress`, conclusion: `success` | null, required: boolean) => ({
      teamId: w.teamId,
      pullRequestId: a,
      headSha: `h221`,
      name,
      status,
      conclusion,
      required,
    })
    await db.insert(pullRequestChecks).values([check(`gate`, `in_progress`, null, true), check(`ios-core`, `queued`, null, false)])
    await db.update(pullRequests).set({ checksState: `pending` }).where(eq(pullRequests.id, a))
    await pass(w.repoId)
    expect(await prRow(a)).toMatchObject({ mergeState: `checks`, mergeWait: `checks` })
    await db.update(pullRequestChecks).set({ status: `completed`, conclusion: `success` }).where(and(eq(pullRequestChecks.pullRequestId, a), eq(pullRequestChecks.name, `gate`)))
    // checks_state still pending (ios-core has no runner): ready anyway,
    // so the same pass starts its run.
    await pass(w.repoId)
    expect(await prRow(a)).toMatchObject({ mergeState: `testing`, mergeWait: null })
  })

  it(`waits for the opener's subtree, then batches ready PRs, lands them on green and tells each opener`, async () => {
    const w = await world()
    const a = await w.pr(101)
    const b = await w.pr(102)
    await pass(w.repoId)
    expect((await prRow(a)).mergeWait).toBe(`session`)
    expect(fake.dispatches).toEqual([])
    await w.idle()
    await pass(w.repoId)
    expect(fake.dispatches).toHaveLength(1)
    const [d] = fake.dispatches
    expect(d!.inputs).toMatchObject({ base: `main`, base_sha: `base0`, mode: `test` })
    expect(d!.units.map((u) => u.prs.map((p) => [p.number, p.head_sha, p.title]))).toEqual([
      [[101, `head-101`, `Part 101`]],
      [[102, `head-102`, `Part 102`]],
    ])
    expect((await prRow(a)).mergeState).toBe(`testing`)
    // The next pass reads the green verdict and lands.
    await pass(w.repoId)
    const head = `train-${d!.inputs.train_id.slice(0, 8)}`
    expect(fake.branch.get(`main`)).toBe(head)
    for (const id of [a, b]) {
      const row = await prRow(id)
      expect([row.state, row.mergeState, row.mergeTrainId]).toEqual([`merged`, `merged`, null])
    }
    expect(fake.closed).toEqual([
      { number: 101, sha: head },
      { number: 102, sha: head },
    ])
    expect((await lines(w.children[101]!)).map((l) => l.body)).toEqual([`Merged #101 into main.`])
    const [run] = await db.select().from(mergeTrains).where(eq(mergeTrains.id, d!.inputs.train_id))
    expect(run!.state).toBe(`landed`)
  })

  it(`a pipeline error keeps its message and backs off: no re-run every pass (#EXP-1711)`, async () => {
    const w = await world()
    const a = await w.pr(231)
    await w.idle()
    fake.judge = () => ({ error: `git merge --squash: not our ref` })
    await pass(w.repoId)
    expect(fake.dispatches).toHaveLength(1)
    await pass(w.repoId, 3)
    const runs = await db.select().from(mergeTrains).where(eq(mergeTrains.repositoryId, w.repoId))
    expect(runs.map((r) => [r.state, r.error])).toEqual([[`error`, `git merge --squash: not our ref`]])
    expect(fake.dispatches).toHaveLength(1)
    expect(await prRow(a)).toMatchObject({ mergeState: `queued`, mergeAttempts: 0 })
    expect(await lines(w.children[231]!)).toEqual([])
  })

  it(`re-runs a red batch once, then bisects: the innocent half lands, the culprit bounces as failing`, async () => {
    const w = await world()
    const a = await w.pr(201)
    const b = await w.pr(202)
    await w.idle()
    fake.judge = (prs) => ({ red: prs.includes(202) })
    await pass(w.repoId, 6)
    expect(fake.dispatches.map((d) => d.units.flatMap((u) => u.prs.map((p) => p.number)))).toEqual([
      [201, 202],
      [201, 202],
      [201],
      [202],
    ])
    expect((await prRow(a)).mergeState).toBe(`merged`)
    const culprit = await prRow(b)
    expect([culprit.mergeState, culprit.mergeAttempts, culprit.mergeWait]).toEqual([`failing`, 1, null])
    expect((await lines(w.children[202]!)).map((l) => l.body)).toEqual([
      `Auto-merge of #202 failed in the merge train: gate. Fix and push: it re-queues by itself (bounce 1 of 3).`,
    ])
    // A bounced PR does not re-queue by itself until its head moves.
    await pass(w.repoId, 2)
    expect(fake.dispatches).toHaveLength(4)
    fake.prs.get(202)!.headSha = `head-202b`
    fake.judge = () => ({})
    await pass(w.repoId, 3)
    expect((await prRow(b)).mergeState).toBe(`merged`)
  })

  it(`a conflicting unit bounces with its files while the rest of the run lands`, async () => {
    const w = await world()
    const a = await w.pr(301)
    const b = await w.pr(302)
    await w.idle()
    fake.judge = () => ({ conflicts: [302] })
    await pass(w.repoId, 2)
    expect((await prRow(a)).mergeState).toBe(`merged`)
    expect((await prRow(b)).mergeState).toBe(`conflict`)
    expect((await lines(w.children[302]!))[0]!.body).toBe(
      `Auto-merge of #302 hit a conflict with main in src/a.ts. Rebase onto main, resolve and push: it re-queues by itself (bounce 1 of 3).`
    )
  })

  it(`a conflict only behind an unlanded sibling is no bounce: it re-queues and lands after (#EXP-1711)`, async () => {
    const w = await world()
    const a = await w.pr(351)
    const b = await w.pr(352)
    await w.idle()
    fake.judge = (prs) => (prs.includes(351) ? { siblings: [352] } : {})
    await pass(w.repoId)
    await pass(w.repoId)
    expect((await prRow(a)).state).toBe(`merged`)
    expect(await prRow(b)).toMatchObject({ mergeAttempts: 0 })
    expect(await lines(w.children[352]!)).toEqual([])
    await pass(w.repoId, 2)
    expect((await prRow(b)).state).toBe(`merged`)
  })

  it(`a new push resets the bounces: an agent rebasing against a busy master is never held for it (#EXP-1711)`, async () => {
    const w = await world()
    const a = await w.pr(401)
    await w.idle()
    fake.judge = () => ({ conflicts: [401] })
    for (let bounce = 1; bounce <= 4; bounce++) {
      if (bounce > 1) fake.prs.get(401)!.headSha = `head-401-${bounce}`
      await pass(w.repoId, 2)
      expect(await prRow(a)).toMatchObject({ mergeState: `conflict`, mergeAttempts: 1 })
    }
    expect(fake.dispatches).toHaveLength(4)
    expect((await lines(w.children[401]!)).map((l) => (l.card as { mergeOutcome?: string }).mergeOutcome)).toEqual([
      `conflict`,
      `conflict`,
      `conflict`,
      `conflict`,
    ])
    // No push: the bounce stays (an abandoned PR is held by the 24 h clock).
    await pass(w.repoId, 2)
    expect(fake.dispatches).toHaveLength(4)
  })

  it(`a moved base recomposes; a stack lands as one unit, bottom first`, async () => {
    const w = await world()
    const bottom = await w.pr(501)
    const top = await w.pr(502, { base: `exp/501` })
    await db.update(pullRequests).set({ basePullRequestId: bottom }).where(eq(pullRequests.id, top))
    await w.idle()
    await pass(w.repoId)
    expect(fake.dispatches[0]!.units.map((u) => u.prs.map((p) => p.number))).toEqual([[501, 502]])
    fake.branch.set(`main`, `someone-else`)
    // The landing finds the base moved: the same pass recomposes on it.
    await pass(w.repoId)
    expect(fake.dispatches).toHaveLength(2)
    expect(fake.dispatches[1]!.inputs.base_sha).toBe(`someone-else`)
    expect(fake.branch.get(`main`)).toBe(`someone-else`)
    await pass(w.repoId)
    expect((await prRow(bottom)).mergeState).toBe(`merged`)
    expect((await prRow(top)).mergeState).toBe(`merged`)
    expect(fake.closed.map((c) => c.number)).toEqual([501, 502])
  })

  it(`a tree-shaped stack lands its ready bottom while a branch above still waits (#EXP-1711)`, async () => {
    const w = await world()
    const bottom = await w.pr(511)
    const ready = await w.pr(512, { base: `exp/511` })
    const waiting = await w.pr(513, { base: `exp/511` })
    await db.update(pullRequests).set({ basePullRequestId: bottom }).where(inArray(pullRequests.id, [ready, waiting]))
    await db.update(pullRequests).set({ checksState: `pending` }).where(eq(pullRequests.id, waiting))
    await w.idle()
    await pass(w.repoId)
    expect(fake.dispatches[0]!.units.map((u) => u.prs.map((p) => p.number))).toEqual([[511, 512]])
    await pass(w.repoId)
    expect((await prRow(bottom)).mergeState).toBe(`merged`)
    expect((await prRow(ready)).mergeState).toBe(`merged`)
    expect((await prRow(waiting)).mergeState).toBe(`checks`)
    expect(fake.closed.map((c) => c.number)).toEqual([511, 512])
  })

  it(`a PR still on a landed PR's branch rides no batch (#EXP-1711)`, async () => {
    const w = await world()
    const landed = await w.pr(521)
    await db.update(pullRequests).set({ state: `merged`, mergeState: `merged` }).where(eq(pullRequests.id, landed))
    const [row] = await db.select({ head: pullRequests.head }).from(pullRequests).where(eq(pullRequests.id, landed))
    const stranded = await w.pr(522, { base: row!.head })
    const free = await w.pr(523)
    await w.idle()
    await pass(w.repoId)
    expect(fake.dispatches.map((d) => d.units.map((u) => u.prs.map((p) => p.number)))).toEqual([[[523]]])
    expect((await prRow(stranded)).mergeState).toBe(`queued`)
    expect((await prRow(free)).mergeState).toBe(`testing`)
  })

  it(`a run whose base landed meanwhile never fast-forwards it (#EXP-1711)`, async () => {
    const w = await world()
    fake.branch.set(`exp/530`, `dead0`)
    const child = await w.pr(531, { base: `exp/530` })
    await w.idle()
    await pass(w.repoId)
    expect(fake.dispatches[0]!.inputs.base).toBe(`exp/530`)
    const parent = await w.pr(530)
    await db.update(pullRequests).set({ state: `merged`, mergeState: `merged` }).where(eq(pullRequests.id, parent))
    await pass(w.repoId)
    expect(fake.branch.get(`exp/530`)).toBe(`dead0`)
    expect(fake.closed).toEqual([])
    expect((await prRow(child)).state).toBe(`open`)
    const [run] = await db.select().from(mergeTrains).where(eq(mergeTrains.id, fake.dispatches[0]!.inputs.train_id))
    expect(run).toMatchObject({ state: `cancelled`, error: `exp/530 already landed: nothing lands onto it` })
    expect(await lines(w.children[531]!)).toEqual([])
  })

  it(`#FEED-207: a repo without the pipeline never squash-merges onto a landed branch`, async () => {
    const w = await world()
    fake.branch.set(`exp/540`, `dead0`)
    const parent = await w.pr(540)
    await db.update(pullRequests).set({ state: `merged`, mergeState: `merged` }).where(eq(pullRequests.id, parent))
    const child = await w.pr(541, { base: `exp/540` })
    await w.idle()
    fake.missingPipeline = true
    const prs = await db.select().from(pullRequests).where(eq(pullRequests.id, child))
    await db.update(pullRequests).set({ mergeState: `queued`, mergeWait: null }).where(eq(pullRequests.id, child))
    const [run] = await db
      .insert(mergeTrains)
      .values({ teamId: w.teamId, repositoryId: w.repoId, base: `exp/540`, baseSha: `dead0`, units: [[child]], attempt: 0, pending: [], heads: { [child]: `head-541` } })
      .returning()
    await db.update(pullRequests).set({ mergeState: `testing`, mergeTrainId: run!.id }).where(eq(pullRequests.id, child))
    await worker.landDirectForTests(w.repoId, run!.id)
    expect(prs[0]!.base).toBe(`exp/540`)
    expect(fake.squashed).toEqual([])
    expect((await prRow(child)).state).toBe(`open`)
    expect((await prRow(child)).mergeState).toBe(`queued`)
    const [after] = await db.select().from(mergeTrains).where(eq(mergeTrains.id, run!.id))
    expect(after).toMatchObject({ state: `cancelled`, error: `exp/540 already landed: nothing lands onto it` })
  })

  it(`a push pulls a testing unit from its run; a repo without the pipeline lands directly`, async () => {
    const w = await world()
    const a = await w.pr(601)
    await w.idle()
    await pass(w.repoId)
    expect((await prRow(a)).mergeState).toBe(`testing`)
    // The push brings CI running on the new head.
    await db.update(pullRequests).set({ checksState: `pending` }).where(eq(pullRequests.id, a))
    await worker.onPullRequestPush(a)
    await worker.drainMergeTrainEdges()
    expect((await prRow(a)).mergeState).toBe(`checks`)
    const runs = await db.select().from(mergeTrains).where(eq(mergeTrains.repositoryId, w.repoId))
    expect(runs.map((r) => r.state).sort()).toContain(`cancelled`)

    const v = await world()
    const b = await v.pr(602)
    await v.idle()
    fake.missingPipeline = true
    await pass(v.repoId)
    expect(fake.squashed).toEqual([602])
    expect((await prRow(b)).mergeState).toBe(`merged`)
  })

  it(`a landed batch is ONE line in the starter's Assistant`, async () => {
    const w = await world()
    const [agent] = await db
      .insert(agents)
      .values({ teamId: w.teamId, name: `Assistant`, instructions: ``, deviceId: w.deviceId, ownerId: w.owner, role: `assistant`, sortOrder: 0 })
      .returning({ id: agents.id })
    const [conversation] = await db
      .select({ id: sessions.id })
      .from(sessions)
      .where(eq(sessions.id, (await sendSession({ userId: w.owner, input: { agentId: agent!.id, text: `hi` } })).sessionId))
    const a = await w.pr(801)
    const b = await w.pr(802)
    await w.idle()
    await pass(w.repoId, 2)
    expect((await prRow(a)).mergeState).toBe(`merged`)
    expect((await prRow(b)).mergeState).toBe(`merged`)
    const root = (await db.select({ rootId: sessions.rootId }).from(sessions).where(eq(sessions.id, conversation!.id)))[0]!.rootId
    expect((await lines(root)).map((l) => l.body)).toEqual([`Merged #801, #802 into main.`])
  })

  it(`hold, resume and the queue report`, async () => {
    const w = await world()
    const a = await w.pr(701)
    await worker.holdPullRequest(a)
    await worker.drainMergeTrainEdges()
    expect((await prRow(a)).mergeHold).toBe(`person`)
    expect((await lines(w.children[701]!)).map((l) => l.body)).toEqual([
      `A person put #701 on hold. It does not merge until they resume it.`,
    ])
    await worker.resumePullRequest(a)
    await worker.drainMergeTrainEdges()
    expect((await worker.queueReport(a))).toMatchObject({ holdReason: null, attempts: 0 })
  })
})
