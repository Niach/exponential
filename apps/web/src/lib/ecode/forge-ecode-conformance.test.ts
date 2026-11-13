import { describe, expect, it } from "vitest"
import forgeFixture from "@exp/ecode/fixtures/forge.json"
import {
  byCodePoint,
  capabilityValues,
  diffTrees,
  forge as reference,
  hydrate,
  mergeBase,
  reasonCode,
  snapshot,
  supports,
  threeWay,
  type ForgeGiven,
  type ForgeReason,
  type ForgeState,
} from "@exp/ecode"
import { ForgeError, type Forge, type ForgeRepo } from "@/lib/ecode/forge"
import {
  createEcodeForge,
  rebaseOnto,
  type EcodeEvent,
  type EcodeHost,
  type EcodeJob,
  type EcodeJobOutcome,
  type EcodePr,
} from "@/lib/ecode/forge-ecode"

// #EXP-1616: eCode's forge spec (packages/ecode fixtures/forge.json) replays
// against `ecodeForge` itself. The host is in memory: refs are a CAS map, the
// PRs are rows, the bus records what it is given, and the team DEVICE that
// runs the git jobs is eCode's reference model (its merge base, three-way
// merge and commit numbering). So every step proves the forge's own logic:
// its validation and refusal order, the ref moves it makes by CAS, the
// events it posts on the bus (compared with the fixture's, step by step) and
// the PR rows it leaves.
//
// `push` is the device's git (git-remote-exp): it applies to the model
// directly. `branch.rebase_onto` goes through `rebaseOnto` (a device job plus
// a CAS move).

const REPO: ForgeRepo = { id: `r1`, fullName: `team/repo`, provider: `ecode` }

type Json = Record<string, unknown>

class MemoryHost implements EcodeHost {
  rows: EcodePr[] = []
  events: EcodeEvent[] = []
  landings: Array<{ base: string; sha: string }> = []
  jobs: string[] = []
  sequence: number

  constructor(public state: ForgeState) {
    this.sequence = state.prs.reduce((max, pr) => Math.max(max, pr.number), 0)
    this.rows = state.prs.map((pr) => ({
      ...pr,
      url: `https://app.test/t/team/repos/repo/pulls/${pr.number}`,
      body: null,
      createdAt: new Date(0),
      mergedAt: null,
      updatedAt: null,
      authorLogin: null,
    }))
  }

  async branches() {
    return { ...this.state.branches }
  }

  async moveBranches(_repo: ForgeRepo, moves: Array<{ branch: string; expected: string | null; sha: string | null }>) {
    if (moves.some((m) => (this.state.branches[m.branch] ?? null) !== m.expected)) return false
    for (const m of moves) {
      if (m.sha === null) delete this.state.branches[m.branch]
      else this.state.branches[m.branch] = m.sha
    }
    return true
  }

  async nextNumber() {
    this.sequence += 1
    return this.sequence
  }

  pullUrl(_repo: ForgeRepo, number: number) {
    return `https://app.test/t/team/repos/repo/pulls/${number}`
  }

  async prs() {
    return this.rows.map((row) => ({ ...row }))
  }

  async recordOpened(_repo: ForgeRepo, pr: { number: number; url: string; head: string; base: string; title: string; body: string | null; draft: boolean }) {
    this.rows.push({
      number: pr.number,
      url: pr.url,
      head: pr.head,
      base: pr.base,
      title: pr.title,
      body: pr.body,
      state: pr.draft ? `draft` : `open`,
      headSha: null,
      mergedSha: null,
      createdAt: new Date(0),
      mergedAt: null,
      updatedAt: null,
      authorLogin: null,
    })
    this.events.push({ kind: `pr_opened`, number: pr.number })
  }

  private row(number: number): EcodePr {
    const row = this.rows.find((r) => r.number === number)
    if (!row) throw new Error(`no row #${number}`)
    return row
  }

  async annotate(_repo: ForgeRepo, number: number, patch: Partial<EcodePr>) {
    Object.assign(this.row(number), patch)
  }

  async setDraft(_repo: ForgeRepo, number: number, draft: boolean) {
    this.row(number).state = draft ? `draft` : `open`
  }

  async publish(_repo: ForgeRepo, events: EcodeEvent[]) {
    for (const event of events) {
      this.events.push(event)
      if (event.kind === `pr_merged`) Object.assign(this.row(event.number), { state: `merged`, mergedAt: new Date(0) })
      if (event.kind === `pr_closed`) this.row(event.number).state = `closed`
      if (event.kind === `pr_retargeted`) this.row(event.number).base = event.to
    }
  }

  /** The device: eCode's reference model over the same objects. */
  async job<K extends EcodeJob[`op`]>(_repo: ForgeRepo, job: Extract<EcodeJob, { op: K }>): Promise<EcodeJobOutcome<K>> {
    this.jobs.push(job.op)
    const s = this.state
    const ok = (result: unknown) => ({ ok: true, result }) as EcodeJobOutcome<K>
    switch (job.op) {
      case `mergeability`: {
        const j = job as Extract<EcodeJob, { op: `mergeability` }>
        const { conflicts } = threeWay(snapshot(s, mergeBase(s, j.head, j.base)), snapshot(s, j.base), snapshot(s, j.head))
        return ok({ mergeability: conflicts.length === 0 ? `clean` : `dirty`, conflicts })
      }
      case `squash`: {
        const j = job as Extract<EcodeJob, { op: `squash` }>
        const { tree, conflicts } = threeWay(snapshot(s, mergeBase(s, j.head, j.base)), snapshot(s, j.base), snapshot(s, j.head))
        if (conflicts.length > 0) return { ok: false, reason: `dirty`, message: `conflict` }
        const id = this.commitId(`m`)
        s.commits[id] = { parent: j.base, changes: diffTrees(snapshot(s, j.base), tree) }
        return ok({ sha: id })
      }
      case `files`: {
        const j = job as Extract<EcodeJob, { op: `files` }>
        const before = snapshot(s, mergeBase(s, j.head, j.base))
        const after = snapshot(s, j.head)
        const paths = [...new Set([...before.keys(), ...after.keys()])].sort(byCodePoint)
        const files = paths
          .filter((path) => (before.get(path) ?? null) !== (after.get(path) ?? null))
          .map((path) => ({
            path,
            previousPath: null,
            change: !after.has(path) ? `removed` : before.has(path) ? `modified` : `added`,
            additions: 0,
            deletions: 0,
            patch: null,
          }))
        return ok({ files })
      }
      case `rebase_onto`: {
        const j = job as Extract<EcodeJob, { op: `rebase_onto` }>
        // The model's own rebase on scratch branches (no ref of the repo moves).
        const scratch: ForgeState = { ...s, branches: { ...s.branches, "\0tip": j.tip, "\0onto": j.onto } }
        const out = reference(scratch, `branch.rebase_onto`, { branch: `\0tip`, onto: `\0onto`, upstream: j.upstream })
        if (out.failure) return { ok: false, reason: out.failure.reason, message: out.failure.reason }
        s.commits = out.state.commits
        s.nextCommit = out.state.nextCommit
        return ok({ sha: (out.result as { sha: string }).sha })
      }
      case `is_ancestor`: {
        const j = job as Extract<EcodeJob, { op: `is_ancestor` }>
        let cursor: string | null = j.of
        while (cursor !== null) {
          if (cursor === j.ancestor) return ok({ yes: true })
          cursor = s.commits[cursor]?.parent ?? null
        }
        return ok({ yes: false })
      }
    }
    throw new Error(`unknown job`)
  }

  private commitId(prefix: string): string {
    let id = `${prefix}${this.state.nextCommit}`
    while (this.state.commits[id]) {
      this.state.nextCommit += 1
      id = `${prefix}${this.state.nextCommit}`
    }
    this.state.nextCommit += 1
    return id
  }

  landed(_repo: ForgeRepo, base: string, sha: string) {
    this.landings.push({ base, sha })
  }

  async defaultBranch() {
    return this.state.defaultBranch
  }

  async requiredChecks() {
    return []
  }

  async checksAt() {
    return []
  }

  async credentials() {
    return null
  }
}

interface Step {
  op: string
  args: Json
  result?: Json
  failure?: { code: string; reason: ForgeReason }
  events?: Json[]
}

/** One fixture step through ecodeForge: its result, or its refusal. */
async function viaEcode(forge: Forge, host: MemoryHost, step: Step): Promise<{ result?: Json; failure?: Json }> {
  const a = step.args
  const n = a.number as number
  try {
    switch (step.op) {
      case `pr.open`:
        return {
          result: {
            number: (
              await forge.openPullRequest(REPO, {
                head: a.head as string,
                base: a.base as string,
                title: a.title as string,
                ...(a.draft ? { draft: true } : {}),
              })
            ).number,
          },
        }
      case `pr.list_by_base`:
        return { result: { numbers: (await forge.listOpenByBase(REPO, a.base as string)).map((p) => p.number) } }
      case `pr.retarget`:
        await forge.retarget(REPO, { number: n, base: a.base as string })
        return { result: {} }
      case `pr.mergeability`:
        return { result: { ...(await forge.mergeability(REPO, n)) } }
      case `pr.squash_merge`: {
        const title = host.rows.find((r) => r.number === n)?.title ?? ``
        const merged = await forge.squashMerge(REPO, { number: n, title, expectedHead: (a.expectedHead as string | undefined) ?? null })
        if (merged.ok) return { result: { sha: merged.sha } }
        return { failure: { code: reasonCode(merged.reason!), reason: merged.reason } }
      }
      case `pr.merge_ordered`: {
        const out = await forge.mergeOrdered(REPO, { numbers: a.numbers as number[] })
        return {
          result: {
            merged: out.merged,
            failed: out.failed && { number: out.failed.number, code: reasonCode(out.failed.reason!), reason: out.failed.reason },
          },
        }
      }
      case `pr.close`:
        await forge.close(REPO, { number: n })
        return { result: {} }
      case `pr.files`:
        return { result: { files: (await forge.files(REPO, n)).map((f) => ({ path: f.path, change: f.change })) } }
      case `branch.rebase_onto`:
        return {
          result: { sha: await rebaseOnto(host, REPO, { branch: a.branch as string, onto: a.onto as string, upstream: a.upstream as string }) },
        }
      case `push`: {
        // The device's git through git-remote-exp: straight onto the model.
        const outcome = reference(host.state, step.op, a)
        if (outcome.failure) return { failure: { ...outcome.failure } }
        host.state = outcome.state
        host.events.push(...(outcome.events as EcodeEvent[]))
        return { result: outcome.result as Json }
      }
      default:
        throw new Error(`unmapped op ${step.op}`)
    }
  } catch (e) {
    if (!(e instanceof ForgeError) || !e.reason) throw e
    return { failure: { code: reasonCode(e.reason), reason: e.reason } }
  }
}

const sortKeys = <T,>(record: Record<string, T>): Record<string, T> =>
  Object.fromEntries(Object.entries(record).sort(([x], [y]) => byCodePoint(x, y)))

describe(`ecodeForge conforms to forge.json (#EXP-1616)`, () => {
  for (const entry of forgeFixture.cases as unknown as Array<{ name: string; given: ForgeGiven; steps: Step[]; expect: Json }>) {
    it(entry.name, async () => {
      const start = hydrate({ ...entry.given, provider: `ecode` })
      const host = new MemoryHost(hydrate({ ...entry.given, provider: `ecode` }))
      const forge = createEcodeForge(host)
      for (const [index, step] of entry.steps.entries()) {
        host.events = []
        const got = await viaEcode(forge, host, step)
        const label = `step ${index}: ${step.op} ${JSON.stringify(step.args)}`
        expect(got, label).toEqual(step.failure ? { failure: { ...step.failure } } : { result: step.result ?? {} })
        // The bus carried exactly the step's events, in order.
        expect(host.events, `${label} events`).toEqual(step.events ?? [])
      }
      const commits: Json = {}
      for (const [id, commit] of Object.entries(host.state.commits)) if (!start.commits[id]) commits[id] = commit
      expect({
        branches: sortKeys(host.state.branches),
        prs: [...host.rows]
          .sort((x, y) => x.number - y.number)
          .map((pr) => ({ number: pr.number, head: pr.head, base: pr.base, title: pr.title, state: pr.state, headSha: pr.headSha, mergedSha: pr.mergedSha })),
        commits: sortKeys(commits),
      }).toEqual(entry.expect)
    })
  }

  it(`declares the capabilities of the ecode provider`, () => {
    const forge = createEcodeForge(new MemoryHost(hydrate({})))
    expect(forge.kind).toBe(`ecode`)
    expect([...forge.capabilities]).toEqual(capabilityValues.filter((c) => supports(`ecode`, c)))
    for (const row of forgeFixture.capabilities.filter((r) => r.provider === `ecode`)) {
      expect(forge.capabilities.includes(row.capability as never), row.capability).toBe(row.supported)
    }
  })

  it(`a squash merge is a device job plus ONE CAS move; a landing triggers the backup and mirror hook`, async () => {
    const host = new MemoryHost(
      hydrate({
        commits: { c1: { changes: { "a.txt": `1` } }, c2: { parent: `c1`, changes: { "b.txt": `x` } } },
        branches: { main: `c1`, topic: `c2` },
      })
    )
    const forge = createEcodeForge(host)
    await forge.openPullRequest(REPO, { head: `topic`, base: `main`, title: `T` })
    const merged = await forge.squashMerge(REPO, { number: 1, title: `T`, expectedHead: `c2` })
    expect(merged).toEqual({ ok: true, sha: `m1` })
    expect(host.jobs).toEqual([`mergeability`, `squash`])
    expect(host.landings).toEqual([{ base: `main`, sha: `m1` }])
    expect(host.state.branches).toEqual({ main: `m1` })
  })

  it(`a base that moves while the device works refuses head_moved and moves nothing`, async () => {
    const host = new MemoryHost(
      hydrate({
        commits: { c1: { changes: { "a.txt": `1` } }, c2: { parent: `c1`, changes: { "b.txt": `x` } }, c3: { parent: `c1`, changes: { "c.txt": `y` } } },
        branches: { main: `c1`, topic: `c2` },
      })
    )
    const forge = createEcodeForge(host)
    await forge.openPullRequest(REPO, { head: `topic`, base: `main`, title: `T` })
    const job = host.job.bind(host)
    host.job = (async (repo: ForgeRepo, j: EcodeJob) => {
      const out = await job(repo, j as never)
      if (j.op === `squash`) host.state.branches.main = `c3` // a push lands meanwhile
      return out
    }) as EcodeHost[`job`]
    const merged = await forge.squashMerge(REPO, { number: 1, title: `T`, expectedHead: null })
    expect(merged).toMatchObject({ ok: false, reason: `head_moved` })
    expect(host.state.branches).toEqual({ main: `c3`, topic: `c2` })
    expect(host.rows[0]!.state).toBe(`open`)
  })

  it(`fastForward is a CAS on the expected sha`, async () => {
    const host = new MemoryHost(hydrate({ commits: { c1: { changes: {} }, c2: { parent: `c1`, changes: { x: `1` } } }, branches: { main: `c1` } }))
    const forge = createEcodeForge(host)
    expect(await forge.fastForward(REPO, { branch: `main`, from: `c2`, to: `c2` })).toEqual({ ok: false, reason: `moved`, current: `c1` })
    expect(await forge.fastForward(REPO, { branch: `main`, from: `c1`, to: `c2` })).toEqual({ ok: true })
    expect(host.state.branches.main).toBe(`c2`)
    expect(await forge.branchContains(REPO, `main`, `c1`)).toBe(true)
  })
})
