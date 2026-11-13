// @vitest-environment node
import { afterAll, afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { and, asc, eq, inArray, sql } from "drizzle-orm"
import type { TrainCheck, TrainPipelineInputs, TrainPipelineOutcome, TrainPipelineUnit } from "@exp/domain-contract/merge-train"
import { db } from "@/db/connection"
import { agents, boards, devices, mergeTrains, messages, pullRequestChecks, pullRequests, repositories, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb, retryingConflict } from "@/test/real-db"
import { inSession } from "@/lib/sessions/messages"
import { partialForgeForTests, type Forge } from "@/lib/ecode/forge"

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
const { confirmRetry } = await import(`@/lib/merge-train/confirm`)
confirmRetry.delaysMs = [0, 0, 0]

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await retryingConflict(() => db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`))
  if (made.users.length) await retryingConflict(() => db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`))
})

// ── the in-memory forge ──────────────────────────────────────────────────

interface FakePr {
  headSha: string
  title: string
  state: `open` | `closed`
  draft: boolean
  mergeable?: boolean | null
  updatedAt?: Date | null
}

class FakeForge {
  branch = new Map<string, string>([[`main`, `base0`]])
  prs = new Map<number, FakePr>()
  dispatches: Array<{ inputs: TrainPipelineInputs; units: TrainPipelineUnit[] }> = []
  closed: Array<{ number: number; sha: string }> = []
  squashed: number[] = []
  missingPipeline = false
  required = [`gate`]
  /** The pipeline reports nothing yet: a run stays live across passes. */
  hold = false
  refuseFastForward = false
  /** Every commit a branch ever pointed at (#FEED-212: `branchContains`). */
  history = new Set<string>([`base0`])
  /** #FEED-222: reads after a fast-forward that still see the old base
   *  (GitHub's compare lagging its ref write); each landing check eats one. */
  staleReads = 0
  private beforeFastForward = new Map<string, string>()
  /** Decides each run: which PR numbers conflict, whether CI is red, and
   *  whether it composed nothing (`empty`: its head is the base itself). */
  judge: (prs: number[]) => { conflicts?: number[]; siblings?: number[]; red?: boolean; error?: string; empty?: boolean } = () => ({})
  private outcomes = new Map<string, TrainPipelineOutcome>()
  private checks = new Map<string, TrainCheck[]>()

  forge(): Forge {
    return partialForgeForTests({
      kind: `github`,
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
          mergeable: pr.mergeable ?? null,
          updatedAt: pr.updatedAt ?? null,
        }
      },
      readBranch: async (_repo, branch) =>
        (this.staleReads > 0 ? this.beforeFastForward.get(branch) : undefined) ?? this.branch.get(branch) ?? null,
      requiredChecks: async () => this.required,
      dispatchPipeline: async (_repo, inputs) => {
        if (this.missingPipeline) return { ok: false, missing: true, message: `no train.yml` }
        const units = JSON.parse(inputs.units) as TrainPipelineUnit[]
        this.dispatches.push({ inputs, units })
        const numbers = units.flatMap((u) => u.prs.map((p) => p.number))
        const verdict = this.judge(numbers)
        const conflicts = new Set([...(verdict.conflicts ?? []), ...(verdict.siblings ?? [])])
        const siblings = new Set(verdict.siblings ?? [])
        const head = verdict.empty ? inputs.base_sha : `train-${inputs.train_id.slice(0, 8)}`
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
        const outcome = this.hold ? null : (this.outcomes.get(trainId) ?? null)
        return { outcome, finished: !!outcome }
      },
      checksAt: async (_repo, sha) => this.checks.get(sha) ?? [],
      fastForward: async (_repo, { branch, from, to }) => {
        if (this.refuseFastForward) return { ok: false, reason: `refused`, message: `ruleset` }
        if (this.branch.get(branch) !== from) return { ok: false, reason: `moved`, current: this.branch.get(branch) ?? null }
        this.beforeFastForward.set(branch, from)
        this.branch.set(branch, to)
        this.history.add(to)
        return { ok: true }
      },
      branchContains: async (_repo, branch, sha) => {
        if (this.staleReads > 0) {
          this.staleReads--
          return false
        }
        return this.branch.get(branch) === sha || this.history.has(sha)
      },
      closeAsLanded: async (_repo, { number, sha }) => {
        this.closed.push({ number, sha })
        const pr = this.prs.get(number)
        if (pr) pr.state = `closed`
      },
      squashMerge: async (_repo, { number }) => {
        this.squashed.push(number)
        this.branch.set(`main`, `sq-${number}`)
        this.history.add(`sq-${number}`)
        return { ok: true, sha: `sq-${number}` }
      },
    })
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
  async function pr(number: number, opts: { base?: string; checks?: boolean } = {}) {
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
    // The fake forge requires `gate`: a PR has its green run unless a case says not.
    if (opts.checks !== false) {
      await db.insert(pullRequestChecks).values({ teamId, pullRequestId: pullRequest.id, headSha: `head-${number}`, name: `gate`, status: `completed`, conclusion: `success`, required: true })
    }
    return pullRequest.id
  }
  // Every session idle: the opener's subtree does not hold the PR back.
  const idle = () => db.update(sessions).set({ status: `idle`, busy: false }).where(eq(sessions.rootId, chat))
  return { owner, teamId, deviceId, chat, repoId: repo!.id, fullName, pr, idle, children }
}

const gateRunning = (id: string) =>
  db.update(pullRequestChecks).set({ status: `in_progress`, conclusion: null }).where(and(eq(pullRequestChecks.pullRequestId, id), eq(pullRequestChecks.name, `gate`)))
const prRow = async (id: string) => (await db.select().from(pullRequests).where(eq(pullRequests.id, id)))[0]!
const lines = (sessionId: string) =>
  db
    .select({ body: messages.body, card: messages.card, consumedAt: messages.consumedAt })
    .from(messages)
    .where(and(inSession(sessionId), sql`${messages.card}->>'event' = 'pr_merge'`))
    .orderBy(asc(messages.createdAt), asc(messages.id))
const outcomeOf = (l: { card: unknown }) => (l.card as { mergeOutcome?: string } | null)?.mergeOutcome
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
    const a = await w.pr(221, { checks: false })
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
    await gateRunning(waiting)
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
    await gateRunning(a)
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

  it(`#FEED-221: a required name decides even when no row carries the flag`, async () => {
    const w = await world()
    const a = await w.pr(801, { checks: false })
    await w.idle()
    const row = (name: string, status: `completed` | `queued`, conclusion: `success` | null) => ({ teamId: w.teamId, pullRequestId: a, headSha: `head-801`, name, status, conclusion, required: false })
    await db.insert(pullRequestChecks).values([row(`gate`, `completed`, `success`), row(`ios-core / ios-core`, `queued`, null)])
    await pass(w.repoId)
    expect(await prRow(a)).toMatchObject({ mergeState: `testing`, mergeWait: null })
  })

  it(`#FEED-221: a head that conflicts with no CI is held at once and takes no slot; a push revives it`, async () => {
    const w = await world()
    const stuck = await w.pr(811, { checks: false })
    const green = await w.pr(812)
    fake.prs.get(811)!.mergeable = false
    await w.idle()
    await pass(w.repoId)
    expect(await prRow(stuck)).toMatchObject({ mergeState: `held`, mergeHold: `conflict`, mergePosition: null, mergeWait: null })
    expect((await lines(w.children[811]!)).map((l) => l.body)).toEqual([
      `Auto-merge of #811 is held: it conflicts with its base, so no CI ran. Rebase, resolve and push: it re-queues by itself.`,
    ])
    expect(fake.dispatches.map((d) => d.units.map((u) => u.prs.map((p) => p.number)))).toEqual([[[812]]])
    expect((await prRow(green)).mergeState).toBe(`testing`)
    // The agent rebased: the new head composes, its CI is on the way.
    fake.prs.get(811)!.mergeable = true
    await worker.onPullRequestPush(stuck)
    await worker.drainMergeTrainEdges()
    expect(await prRow(stuck)).toMatchObject({ mergeHold: null })
    expect((await prRow(stuck)).mergeState).not.toBe(`held`)
  })

  it(`#FEED-221: no CI waits out the grace as checks, then is held; check rows release it`, async () => {
    const w = await world()
    const a = await w.pr(821, { checks: false })
    fake.prs.get(821)!.mergeable = true
    await w.idle()
    await pass(w.repoId)
    expect(await prRow(a)).toMatchObject({ mergeState: `checks`, mergeWait: `checks`, mergeHold: null })
    expect(fake.dispatches).toHaveLength(0)
    fake.prs.get(821)!.updatedAt = new Date(Date.now() - 6 * 60_000)
    await pass(w.repoId)
    expect(await prRow(a)).toMatchObject({ mergeState: `held`, mergeHold: `no_checks` })
    expect(fake.dispatches).toHaveLength(0)
    await db.insert(pullRequestChecks).values({ teamId: w.teamId, pullRequestId: a, headSha: `head-821`, name: `gate`, status: `completed`, conclusion: `success`, required: true })
    await pass(w.repoId)
    expect(await prRow(a)).toMatchObject({ mergeState: `testing`, mergeHold: null })
  })

  it(`#FEED-221: a repo whose base requires nothing still lands a PR with no CI`, async () => {
    const w = await world()
    const a = await w.pr(831, { checks: false })
    fake.required = []
    await w.idle()
    await pass(w.repoId)
    expect((await prRow(a)).mergeState).toBe(`testing`)
  })

  it(`#FEED-221: a queued PR keeps its place through its opener's wake`, async () => {
    process.env[worker.MERGE_TRAIN_BATCH_MAX_ENV] = `1`
    try {
      const w = await world()
      const first = await w.pr(841)
      const second = await w.pr(842)
      await w.idle()
      await pass(w.repoId)
      const before = await prRow(second)
      expect(before).toMatchObject({ mergeState: `queued`, mergePosition: 2 })
      expect((await prRow(first)).mergeState).toBe(`testing`)
      await db.update(sessions).set({ status: `running`, busy: true }).where(eq(sessions.id, w.children[842]!))
      fake.hold = true
      await pass(w.repoId)
      const after = await prRow(second)
      expect(after).toMatchObject({ mergeState: `queued`, mergeWait: null, mergePosition: 2 })
      expect(after.mergeReadyAt).toEqual(before.mergeReadyAt)
    } finally {
      delete process.env[worker.MERGE_TRAIN_BATCH_MAX_ENV]
    }
  })

  it(`#FEED-221: a re-queued batch pushes nobody back; several green PRs ride one batch`, async () => {
    process.env[worker.MERGE_TRAIN_BATCH_MAX_ENV] = `2`
    try {
      const w = await world()
      const a = await w.pr(851)
      const b = await w.pr(852)
      const c = await w.pr(853)
      await w.idle()
      fake.judge = () => ({ error: `boom` })
      await pass(w.repoId)
      expect(fake.dispatches[0]!.units.map((u) => u.prs.map((p) => p.number))).toEqual([[851], [852]])
      expect((await prRow(a)).mergeState).toBe(`testing`)
      expect((await prRow(b)).mergeState).toBe(`testing`)
      expect((await prRow(c)).mergePosition).toBe(3)
      // The pipeline errors: the batch re-queues ahead of the PR behind it,
      // and that PR's place does not move.
      await pass(w.repoId)
      expect(await prRow(a)).toMatchObject({ mergeState: `queued`, mergePosition: 1 })
      expect(await prRow(b)).toMatchObject({ mergeState: `queued`, mergePosition: 2 })
      expect((await prRow(c)).mergePosition).toBe(3)
    } finally {
      delete process.env[worker.MERGE_TRAIN_BATCH_MAX_ENV]
    }
  })

  it(`#FEED-221: a closed PR keeps no queue place`, async () => {
    const w = await world()
    const a = await w.pr(861)
    await db.update(pullRequests).set({ state: `closed`, mergePosition: 4 }).where(eq(pullRequests.id, a))
    await pass(w.repoId)
    expect((await prRow(a)).mergePosition).toBeNull()
  })

  it(`#FEED-222: a landing check that reads the old base first re-reads, and lands`, async () => {
    const w = await world()
    const a = await w.pr(871)
    await w.idle()
    fake.staleReads = 2
    await pass(w.repoId, 2)
    const head = `train-${fake.dispatches[0]!.inputs.train_id.slice(0, 8)}`
    expect(await prRow(a)).toMatchObject({ state: `merged`, mergeState: `merged`, mergeAttempts: 0 })
    expect(fake.closed).toEqual([{ number: 871, sha: head }])
    expect((await lines(w.children[871]!)).map((l) => l.body)).toEqual([`Merged #871 into main.`])
  })

  // Every landing check after the push: confirmLanding's reads.
  const confirmReads = () => 1 + confirmRetry.delaysMs.length

  it(`#FEED-222: a landing no read confirmed is Merged in the same pass once the base shows it; never stuck`, async () => {
    const w = await world()
    const [agent] = await db
      .insert(agents)
      .values({ teamId: w.teamId, name: `Assistant`, instructions: ``, deviceId: w.deviceId, ownerId: w.owner, role: `assistant`, sortOrder: 0 })
      .returning({ id: agents.id })
    const conversation = (await sendSession({ userId: w.owner, input: { agentId: agent!.id, text: `hi` } })).sessionId
    const a = await w.pr(881)
    const b = await w.pr(882)
    await w.idle()
    await pass(w.repoId)
    // The run lands #881 + #882, but every confirmation read is stale.
    fake.staleReads = confirmReads()
    await pass(w.repoId)
    const landed = `train-${fake.dispatches[0]!.inputs.train_id.slice(0, 8)}`
    expect(fake.branch.get(`main`)).toBe(landed)
    expect(fake.dispatches).toHaveLength(1)
    for (const id of [a, b]) {
      expect(await prRow(id)).toMatchObject({ state: `merged`, mergeState: `merged`, mergeHold: null, mergeTrainId: null, mergeAttempts: 1 })
    }
    expect(fake.closed).toEqual([
      { number: 881, sha: landed },
      { number: 882, sha: landed },
    ])
    const outcomes = async (n: number) => (await lines(w.children[n]!)).map((l) => (l.card as { mergeOutcome?: string }).mergeOutcome)
    expect(await outcomes(881)).toEqual([`merged`])
    expect(await outcomes(882)).toEqual([`merged`])
    const root = (await db.select({ rootId: sessions.rootId }).from(sessions).where(eq(sessions.id, conversation)))[0]!.rootId
    expect((await lines(root)).map((l) => l.body)).toEqual([`Merged #881, #882 into main.`])
    await pass(w.repoId, 2)
    expect(fake.dispatches).toHaveLength(1)
  })

  it(`#FEED-222: a PR bounced as a conflict after an unconfirmed landing (re-squashed onto the moved base) is Merged`, async () => {
    const w = await world()
    const a = await w.pr(883)
    await w.idle()
    await pass(w.repoId)
    // Stale through the confirmation and the next two looks; the re-run conflicts.
    fake.staleReads = confirmReads() + 2
    fake.judge = () => ({ conflicts: [883] })
    await pass(w.repoId, 2)
    const landed = `train-${fake.dispatches[0]!.inputs.train_id.slice(0, 8)}`
    expect(fake.dispatches).toHaveLength(2)
    expect(await prRow(a)).toMatchObject({ state: `open`, mergeState: `conflict`, mergeAttempts: 2 })
    // The base reads true now: the next pass merges it from `conflict`.
    await pass(w.repoId)
    expect(await prRow(a)).toMatchObject({ state: `merged`, mergeState: `merged` })
    expect(fake.closed).toEqual([{ number: 883, sha: landed }])
    expect((await lines(w.children[883]!)).map((l) => (l.card as { mergeOutcome?: string }).mergeOutcome)).toEqual([`conflict`, `merged`])
    expect(fake.dispatches).toHaveLength(2)
  })

  it(`#FEED-222: a run that composes nothing lands what an earlier unconfirmed run landed`, async () => {
    const w = await world()
    const a = await w.pr(884)
    await w.idle()
    await pass(w.repoId)
    fake.staleReads = confirmReads() + 1
    fake.judge = () => ({ empty: true })
    await pass(w.repoId)
    const landed = `train-${fake.dispatches[0]!.inputs.train_id.slice(0, 8)}`
    expect(fake.dispatches[1]!.inputs.base_sha).toBe(landed)
    expect((await prRow(a)).mergeState).toBe(`testing`)
    await pass(w.repoId)
    expect(await prRow(a)).toMatchObject({ state: `merged`, mergeState: `merged` })
    expect(fake.closed).toEqual([{ number: 884, sha: landed }])
    const runs = await db.select({ state: mergeTrains.state }).from(mergeTrains).where(eq(mergeTrains.repositoryId, w.repoId)).orderBy(asc(mergeTrains.createdAt))
    expect(runs.map((r) => r.state)).toEqual([`error`, `landed`])
  })

  it(`#FEED-222: a head pushed since the unconfirmed landing never lands from it`, async () => {
    const w = await world()
    const a = await w.pr(885)
    await w.idle()
    await pass(w.repoId)
    fake.staleReads = confirmReads() + 1
    await pass(w.repoId)
    expect(await prRow(a)).toMatchObject({ state: `open`, mergeState: `testing`, mergeAttempts: 1 })
    fake.prs.get(885)!.headSha = `head-885-2`
    await worker.onPullRequestPush(a)
    await worker.drainMergeTrainEdges()
    await pass(w.repoId)
    // It lands only through a run that tested the new head.
    const landed = `train-${fake.dispatches[0]!.inputs.train_id.slice(0, 8)}`
    const last = fake.dispatches.at(-1)!
    expect(last.units[0]!.prs[0]!.head_sha).toBe(`head-885-2`)
    expect(fake.closed).not.toContainEqual({ number: 885, sha: landed })
    expect(fake.closed.every((c) => c.sha === `train-${last.inputs.train_id.slice(0, 8)}`)).toBe(true)
  })

  it(`#FEED-222: an empty compose after a landing that never reached the base still bounces`, async () => {
    const w = await world()
    const a = await w.pr(891)
    await w.idle()
    await pass(w.repoId)
    fake.staleReads = confirmReads() + 1
    fake.judge = () => ({ empty: true })
    await pass(w.repoId)
    // The base was put back: the earlier train commit is not on it.
    fake.branch.set(`main`, `base0`)
    fake.history = new Set([`base0`])
    await pass(w.repoId)
    expect(await prRow(a)).toMatchObject({ state: `open`, mergeAttempts: 2 })
    expect(fake.closed).toEqual([])
  })

  it(`#FEED-233: a Merge now restarts the 24 h clock: its bounce is a bounce, never an instant timeout`, async () => {
    const w = await world()
    const a = await w.pr(901)
    await w.idle()
    const old = new Date(Date.now() - 48 * 3_600_000)
    await db.update(pullRequests).set({ mergeQueuedSince: old }).where(eq(pullRequests.id, a))
    fake.hold = true
    await pass(w.repoId)
    expect(await prRow(a)).toMatchObject({ mergeState: `held`, mergeHold: `timeout` })
    fake.hold = false
    fake.judge = () => ({ conflicts: [901] })
    await worker.mergeNowPullRequest(a)
    await worker.drainMergeTrainEdges()
    await pass(w.repoId, 2)
    const row = await prRow(a)
    expect(row).toMatchObject({ mergeState: `conflict`, mergeHold: null, mergeNow: false })
    expect(row.mergeQueuedSince!.getTime()).toBeGreaterThan(old.getTime())
    expect((await lines(w.children[901]!)).map((l) => outcomeOf(l)).at(-1)).toBe(`conflict`)
  })

  it(`#FEED-233: a plain pr merge re-queues a PR held by the limits; a person's Hold stays`, async () => {
    const w = await world()
    const a = await w.pr(911)
    const b = await w.pr(912)
    await w.idle()
    await db.update(pullRequests).set({ mergeState: `held`, mergeHold: `timeout`, mergeQueuedSince: new Date(Date.now() - 48 * 3_600_000) }).where(eq(pullRequests.id, a))
    await db.update(pullRequests).set({ mergeState: `held`, mergeHold: `person` }).where(eq(pullRequests.id, b))
    await worker.queuePullRequest(a)
    await worker.queuePullRequest(b)
    await worker.drainMergeTrainEdges()
    await pass(w.repoId, 2)
    expect((await prRow(a)).mergeState).toBe(`merged`)
    expect(await prRow(b)).toMatchObject({ mergeState: `held`, mergeHold: `person` })
  })

  it(`#FEED-225: a PR's merged line supersedes its held line the agent never read`, async () => {
    const w = await world()
    const a = await w.pr(921)
    await w.idle()
    await db.update(pullRequests).set({ mergeQueuedSince: new Date(Date.now() - 48 * 3_600_000) }).where(eq(pullRequests.id, a))
    fake.hold = true
    await pass(w.repoId)
    expect(await prRow(a)).toMatchObject({ mergeState: `held`, mergeHold: `timeout` })
    const opener = w.children[921]!
    expect((await lines(opener)).map((l) => [outcomeOf(l), l.consumedAt])).toEqual([[`held`, null]])
    fake.hold = false
    await worker.resumePullRequest(a)
    await worker.drainMergeTrainEdges()
    await pass(w.repoId, 2)
    expect((await prRow(a)).mergeState).toBe(`merged`)
    const after = await lines(opener)
    expect(after.map((l) => outcomeOf(l))).toEqual([`held`, `merged`])
    expect(after[0]!.consumedAt).not.toBeNull()
    expect(after[1]!.consumedAt).toBeNull()
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
