import { describe, expect, it } from "vitest"
import {
  applyBundle,
  ImportAborted,
  type ApplyPorts,
  type ApplyTeamState,
  type FetchedAsset,
  type MapRowInput,
  type PlannedLink,
} from "@/lib/import/apply"
import type { ImportEntityKind, ImportPlan, ImportProgress } from "@/lib/import/bundle"
import type { PlannedIssueWrite } from "@/lib/import/issue-write"
import { bundleFixture, BUILTIN_ROWS } from "@/lib/import/fixtures"

// EXP-630: the applier against in-memory ports — phase order, what gets
// created, the ascending-number batch order, the links pass, and the three
// properties the entity map buys: a re-run creates nothing, a crash mid-way
// resumes past what committed, and a lost claim / cancel stops the run.

const IMPORTER = `importer-id`

class MemoryPorts implements ApplyPorts {
  state: ApplyTeamState = {
    boards: [],
    statuses: BUILTIN_ROWS.map(({ id, name, category, builtinKey }) => ({ id, name, category, builtinKey })),
    labels: [{ id: `label-bug`, name: `bug` }],
    members: [{ userId: `member-hannes`, email: `hannes.robier@youspi.com`, name: `Hannes` }],
    pendingInviteEmails: [],
  }
  map = new Map<string, Map<string, string>>()
  created = { boards: [] as string[], statuses: [] as string[], labels: [] as string[], invites: [] as string[] }
  batches: PlannedIssueWrite[][] = []
  links: PlannedLink[] = []
  progress: ImportProgress[] = []
  fetched: string[] = []
  failOnBatch: number | null = null
  cancelled = false
  claimLost = false
  private seq = 0

  async loadTeamState() {
    return this.state
  }
  async loadMapped(kind: ImportEntityKind) {
    return new Map(this.map.get(kind) ?? [])
  }
  async recordMap(rows: MapRowInput[]) {
    for (const row of rows) {
      const bucket = this.map.get(row.kind) ?? new Map<string, string>()
      if (!bucket.has(row.externalId)) bucket.set(row.externalId, row.localId)
      this.map.set(row.kind, bucket)
    }
  }
  async createBoard(input: { name: string; prefix: string }) {
    const id = `board-${input.prefix}`
    this.created.boards.push(input.prefix)
    this.state = { ...this.state, boards: [...this.state.boards, { id, name: input.name, prefix: input.prefix }] }
    return { id }
  }
  async createStatus(input: { name: string; category: ApplyTeamState[`statuses`][number][`category`] }) {
    const id = `status-${input.name}`
    this.created.statuses.push(input.name)
    this.state = {
      ...this.state,
      statuses: [...this.state.statuses, { id, name: input.name, category: input.category, builtinKey: null }],
    }
    return { id, name: input.name }
  }
  async createLabel(input: { name: string }) {
    const id = `label-${input.name}`
    this.created.labels.push(input.name)
    this.state = { ...this.state, labels: [...this.state.labels, { id, name: input.name }] }
    return { id }
  }
  async createInvite(email: string) {
    this.created.invites.push(email)
  }
  async fetchAsset(ref: string): Promise<FetchedAsset | null> {
    this.fetched.push(ref)
    return { bytes: new Uint8Array([1, 2, 3]), contentType: `image/png`, filename: null }
  }
  async writeBatch({ writes }: { writes: PlannedIssueWrite[] }) {
    if (this.failOnBatch !== null && this.batches.length === this.failOnBatch) {
      throw new Error(`disk on fire`)
    }
    this.batches.push(writes)
    for (const write of writes) await this.recordMap(write.map)
  }
  async linkRelations(links: PlannedLink[]) {
    this.links.push(...links)
    return { written: links.length, warnings: [] }
  }
  async reportProgress(progress: ImportProgress) {
    this.progress.push({ ...progress, warnings: [...progress.warnings] })
    return !this.claimLost
  }
  async shouldStop() {
    return this.cancelled
  }
  newId() {
    this.seq += 1
    return `id-${this.seq}`
  }
}

function plan(overrides: Partial<ImportPlan> = {}): ImportPlan {
  return {
    routing: `team`,
    importHistory: true,
    boards: { "b-main": { mode: `create`, name: `Main`, prefix: `MAIN`, numbering: `preserve` } },
    statuses: {
      "st-open": { mode: `builtin`, builtinKey: `backlog` },
      "st-doing": { mode: `create`, name: `Doing`, color: `#f2c94c`, category: `started` },
      "st-done": { mode: `builtin`, builtinKey: `done` },
      "st-dup": { mode: `builtin`, builtinKey: `duplicate` },
    },
    labels: { "lb-bug": { mode: `existing`, labelId: `label-bug` }, "lb-new": { mode: `create` } },
    users: { "u-h": { mode: `member`, userId: `member-hannes` }, "u-x": { mode: `invite` } },
    ...overrides,
  }
}

const options = { teamId: `team-1`, importerId: IMPORTER, batchSize: 2 }

describe(`applyBundle`, () => {
  it(`runs the phases in order and creates only what the plan asks for`, async () => {
    const ports = new MemoryPorts()
    const result = await applyBundle(bundleFixture(), plan(), ports, options)

    expect(ports.progress.map((entry) => entry.phase)).toEqual([
      `users`,
      `boards`,
      `boards`,
      `statuses`,
      `labels`,
      `issues`,
      `issues`,
      `issues`,
      `links`,
      `done`,
    ])
    expect(ports.created).toEqual({
      boards: [`MAIN`],
      statuses: [`Doing`],
      labels: [`Brand new`],
      invites: [`stranger@example.com`],
    })
    expect(result.counts).toMatchObject({
      boards: 1,
      statuses: 1,
      labels: 1,
      invites: 1,
      issues: 3,
      comments: 2,
      attachments: 1,
      events: 4,
      relations: 3,
      skippedIssues: 0,
    })
    expect(ports.fetched).toEqual([`https://files.example.com/a.png`])
  })

  it(`writes issues ascending by number in batches and links in a second pass`, async () => {
    const ports = new MemoryPorts()
    await applyBundle(bundleFixture(), plan(), ports, options)
    expect(ports.batches.map((batch) => batch.map((write) => write.issue.number))).toEqual([[5, 10], [11]])
    const five = ports.map.get(`issue`)!.get(`i-3`)!
    const ten = ports.map.get(`issue`)!.get(`i-1`)!
    const eleven = ports.map.get(`issue`)!.get(`i-2`)!
    expect(ports.links).toEqual([
      { type: `duplicate`, issueId: eleven, relatedIssueId: ten, externalRef: `MAIN-11` },
      { type: `blocks`, issueId: five, relatedIssueId: ten, externalRef: `MAIN-5` },
      { type: `related`, issueId: five, relatedIssueId: ten, externalRef: `MAIN-5` },
    ])
    // The mapped member is the assignee; the invited stranger left the
    // creator empty and their comment carries the attribution line.
    const tenWrite = ports.batches[0]![1]!
    expect(tenWrite.issue.assigneeId).toBe(`member-hannes`)
    expect(tenWrite.issue.creatorId).toBeNull()
    expect(tenWrite.comments[0]!.authorId).toBe(IMPORTER)
    expect(tenWrite.comments[0]!.body).toMatch(/^\*Imported from Test Tracker/)
  })

  it(`re-running the same bundle creates nothing new`, async () => {
    const ports = new MemoryPorts()
    await applyBundle(bundleFixture(), plan(), ports, options)
    const batchesBefore = ports.batches.length
    const result = await applyBundle(bundleFixture(), plan(), ports, options)
    expect(ports.batches.length).toBe(batchesBefore)
    expect(ports.created).toEqual({
      boards: [`MAIN`],
      statuses: [`Doing`],
      labels: [`Brand new`],
      invites: [`stranger@example.com`],
    })
    expect(result.counts).toMatchObject({ boards: 0, statuses: 0, labels: 0, invites: 0, issues: 0 })
    // The links pass is idempotent on the executor side; here it just runs again.
    expect(result.counts.relations).toBe(3)
  })

  it(`creates no board on a re-run whose issues all exist, even under a new routing`, async () => {
    const ports = new MemoryPorts()
    await applyBundle(bundleFixture(), plan(), ports, options)
    const rerouted = {
      ...bundleFixture(),
      boards: [{ key: `b-other`, name: `Other`, prefix: `OTH` }],
      issues: bundleFixture().issues.map((issue) => ({ ...issue, boardKey: `b-other` })),
    }
    const result = await applyBundle(
      rerouted,
      plan({ boards: { "b-other": { mode: `create`, name: `Other`, prefix: `OTH`, numbering: `preserve` } } }),
      ports,
      options
    )
    expect(ports.created.boards).toEqual([`MAIN`])
    expect(result.counts).toMatchObject({ boards: 0, issues: 0, skippedIssues: 0, relations: 3 })
  })

  it(`resumes past what committed after a mid-run failure`, async () => {
    const ports = new MemoryPorts()
    ports.failOnBatch = 1
    await expect(applyBundle(bundleFixture(), plan(), ports, options)).rejects.toThrow(/disk on fire/)
    expect(ports.batches.map((batch) => batch.map((write) => write.issue.number))).toEqual([[5, 10]])

    ports.failOnBatch = null
    const result = await applyBundle(bundleFixture(), plan(), ports, options)
    expect(ports.batches.map((batch) => batch.map((write) => write.issue.number))).toEqual([[5, 10], [11]])
    expect(result.counts.issues).toBe(1)
    expect(ports.created.boards).toEqual([`MAIN`])
  })

  it(`adopts a board, status and label that exist by prefix/name (a resume whose map rows never landed)`, async () => {
    const ports = new MemoryPorts()
    ports.state = {
      ...ports.state,
      boards: [{ id: `board-old`, name: `Old`, prefix: `MAIN` }],
      statuses: [...ports.state.statuses, { id: `status-old`, name: `doing`, category: `started`, builtinKey: null }],
      labels: [...ports.state.labels, { id: `label-old`, name: `brand NEW` }],
    }
    await applyBundle(bundleFixture(), plan(), ports, options)
    expect(ports.created).toEqual({ boards: [], statuses: [], labels: [], invites: [`stranger@example.com`] })
    expect(ports.batches[0]![0]!.issue.boardId).toBe(`board-old`)
    expect(ports.batches[0]![0]!.issue.statusId).toBe(`status-old`)
  })

  it(`skips a board's issues and leaves its statuses alone`, async () => {
    const ports = new MemoryPorts()
    const result = await applyBundle(
      bundleFixture(),
      plan({ boards: { "b-main": { mode: `skip` } } }),
      ports,
      options
    )
    expect(result.counts).toMatchObject({ issues: 0, skippedIssues: 3, statuses: 0, labels: 0 })
    expect(ports.batches).toEqual([])
  })

  it(`does not invite twice: a pending invite or a mapped one is left alone`, async () => {
    const ports = new MemoryPorts()
    ports.state = { ...ports.state, pendingInviteEmails: [`STRANGER@example.com`] }
    await applyBundle(bundleFixture(), plan(), ports, options)
    expect(ports.created.invites).toEqual([])
  })

  it(`stops at the next report when cancelled, and when the claim is lost`, async () => {
    const cancelled = new MemoryPorts()
    cancelled.cancelled = true
    await expect(applyBundle(bundleFixture(), plan(), cancelled, options)).rejects.toBeInstanceOf(ImportAborted)
    expect(cancelled.batches).toEqual([])

    const lost = new MemoryPorts()
    lost.claimLost = true
    await expect(applyBundle(bundleFixture(), plan(), lost, options)).rejects.toMatchObject({
      reason: `claim_lost`,
    })
  })

  it(`falls back to the importer when a mapped member left the team`, async () => {
    const ports = new MemoryPorts()
    ports.state = { ...ports.state, members: [] }
    const result = await applyBundle(bundleFixture(), plan(), ports, options)
    expect(result.warnings.join(`\n`)).toMatch(/Hannes was mapped to a member who left/)
    expect(ports.batches[0]![1]!.issue.assigneeId).toBeNull()
  })
})
