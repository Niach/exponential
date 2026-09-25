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
  }
  map = new Map<string, Map<string, string>>()
  created = {
    boards: [] as string[],
    statuses: [] as string[],
    labels: [] as string[],
    members: [] as string[],
    invites: [] as string[],
  }
  // EXP-1076: addresses `createPlaceholder` must answer with `null` — an
  // Exponential account outside this team.
  foreignAccounts = new Set<string>()
  membersChanged = 0
  archived: string[] = []
  batches: PlannedIssueWrite[][] = []
  links: PlannedLink[] = []
  progress: ImportProgress[] = []
  fetched: string[] = []
  failFetches = 0
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
  async createPlaceholder(input: { email: string; name: string }) {
    this.created.members.push(input.email)
    if (this.foreignAccounts.has(input.email.trim().toLowerCase())) return { memberUserId: null }
    const memberUserId = `placeholder-${input.email}`
    this.state = {
      ...this.state,
      members: [...this.state.members, { userId: memberUserId, email: input.email, name: input.name }],
    }
    return { memberUserId }
  }
  async onMembersChanged() {
    this.membersChanged += 1
  }
  async createInvite(input: { email: string; name: string }) {
    this.created.invites.push(input.email)
    const memberUserId = `placeholder-${input.email}`
    this.state = {
      ...this.state,
      members: [...this.state.members, { userId: memberUserId, email: input.email, name: input.name }],
    }
    return { memberUserId }
  }
  async archiveBoard(boardId: string) {
    this.archived.push(boardId)
  }
  estimation: string[] = []
  async setEstimation(type: string) {
    this.estimation.push(type)
    this.state = { ...this.state, estimationType: type as ApplyTeamState[`estimationType`] }
  }
  async fetchAsset(ref: string): Promise<FetchedAsset | null> {
    this.fetched.push(ref)
    if (this.failFetches > 0) {
      this.failFetches -= 1
      throw new TypeError(`fetch failed`)
    }
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
    importArchived: true,
    boards: { "b-main": { mode: `create`, name: `Main`, prefix: `MAIN`, numbering: `preserve` } },
    statuses: {
      "st-open": { mode: `builtin`, builtinKey: `backlog` },
      "st-doing": { mode: `create`, name: `Doing`, color: `#f2c94c`, category: `started` },
      "st-done": { mode: `builtin`, builtinKey: `done` },
      "st-dup": { mode: `builtin`, builtinKey: `duplicate` },
    },
    labels: { "lb-bug": { mode: `existing`, labelId: `label-bug` }, "lb-new": { mode: `create` } },
    users: { "u-h": { mode: `member`, userId: `member-hannes` }, "u-x": { mode: `self` } },
    ...overrides,
  }
}

const options = { teamId: `team-1`, importerId: IMPORTER, batchSize: 2, fetchRetryDelaysMs: [0, 0] }

function tenWriteEstimate(ports: MemoryPorts): number | null {
  return ports.batches[0]![1]!.issue.estimate
}

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
      `archiving`,
      `done`,
    ])
    expect(ports.created).toEqual({
      boards: [`MAIN`],
      statuses: [`Doing`],
      labels: [`Brand new`],
      members: [],
      invites: [],
    })
    expect(result.counts).toMatchObject({
      boards: 1,
      statuses: 1,
      labels: 1,
      issues: 3,
      comments: 2,
      attachments: 1,
      events: 4,
      relations: 4,
      skippedIssues: 0,
    })
    expect(ports.fetched).toEqual([`https://files.example.com/a.png`])
    expect(ports.archived).toEqual([])
    // The fixture's t-shirt estimates switch the (off) team scale on once.
    expect(ports.estimation).toEqual([`tshirt`])
    expect(result.warnings.join(`\n`)).toMatch(/Estimates switched on for this team with the tshirt scale/)
  })

  it(`closes a batch early once its downloaded assets pass the byte cap`, async () => {
    const wide = new MemoryPorts()
    await applyBundle(bundleFixture(), plan(), wide, { ...options, batchSize: 25 })
    expect(wide.batches.map((batch) => batch.map((write) => write.issue.number))).toEqual([[5, 10, 11]])
    // MAIN-10 carries the fixture's one asset (3 fetched bytes): with a
    // 2-byte cap the batch flushes right after it, and MAIN-11 starts a new
    // one — the same batch size would otherwise take all three.
    const capped = new MemoryPorts()
    await applyBundle(bundleFixture(), plan(), capped, { ...options, batchSize: 25, assetFlushBytes: 2 })
    expect(capped.batches.map((batch) => batch.map((write) => write.issue.number))).toEqual([[5, 10], [11]])
    expect(capped.progress.filter((p) => p.phase === `issues`).map((p) => p.done)).toEqual([0, 2, 3])
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
      // Canonical `parent` direction: the PARENT is issueId (EXP-736).
      { type: `parent`, issueId: ten, relatedIssueId: five, externalRef: `MAIN-5` },
      { type: `blocks`, issueId: five, relatedIssueId: ten, externalRef: `MAIN-5` },
      { type: `related`, issueId: five, relatedIssueId: ten, externalRef: `MAIN-5` },
    ])
    expect(ports.batches[0]![0]!.issue.estimate).toBe(3)
    expect(tenWriteEstimate(ports)).toBeNull()
    // The mapped member is the assignee; the stranger attributed to the
    // importer left the creator empty and their comment carries the
    // attribution line.
    const tenWrite = ports.batches[0]![1]!
    expect(tenWrite.issue.assigneeId).toBe(`member-hannes`)
    expect(tenWrite.issue.creatorId).toBeNull()
    expect(tenWrite.comments[0]!.authorId).toBe(IMPORTER)
    expect(tenWrite.comments[0]!.body).toMatch(/^\*Imported from Test Tracker/)
  })

  it(`seats a planned placeholder once, attributes their content to it, and reuses it on a re-run`, async () => {
    const ports = new MemoryPorts()
    const seated = plan({
      users: {
        "u-h": { mode: `member`, userId: `member-hannes` },
        "u-x": { mode: `placeholder`, name: `Stranger`, email: `Stranger@example.com` },
      },
    })
    const result = await applyBundle(bundleFixture(), seated, ports, options)
    expect(ports.created.members).toEqual([`Stranger@example.com`])
    expect(ports.created.invites).toEqual([])
    expect(result.counts.members).toBe(1)
    expect(ports.membersChanged).toBe(1)
    const tenWrite = ports.batches[0]![1]!
    expect(tenWrite.issue.creatorId).toBe(`placeholder-Stranger@example.com`)
    expect(tenWrite.comments[0]!.authorId).toBe(`placeholder-Stranger@example.com`)
    expect(tenWrite.comments[0]!.body).not.toMatch(/^\*Imported from/)
    expect(ports.map.get(`user`)?.get(`u-x`)).toBe(`placeholder-Stranger@example.com`)
    // Second run: the placeholder is on the roster → nobody is seated again.
    const again = await applyBundle(bundleFixture(), seated, ports, options)
    expect(ports.created.members).toEqual([`Stranger@example.com`])
    expect(again.counts.members).toBe(0)
    expect(ports.membersChanged).toBe(1)
  })

  it(`falls back to the importer when the address belongs to an account outside the team`, async () => {
    const ports = new MemoryPorts()
    ports.foreignAccounts.add(`stranger@example.com`)
    const result = await applyBundle(
      bundleFixture(),
      plan({
        users: {
          "u-h": { mode: `member`, userId: `member-hannes` },
          "u-x": { mode: `placeholder`, name: `Stranger`, email: `Stranger@example.com` },
        },
      }),
      ports,
      options
    )
    expect(result.counts.members).toBe(0)
    const tenWrite = ports.batches[0]![1]!
    expect(tenWrite.issue.creatorId).toBeNull()
    expect(tenWrite.comments[0]!.authorId).toBe(IMPORTER)
    expect(tenWrite.comments[0]!.body).toMatch(/^\*Imported from Test Tracker/)
    expect(result.warnings.join(`\n`)).toMatch(
      /Stranger already has an Exponential account outside this team; their content is attributed to you\. Invite them from Settings → Members to reconnect it\./
    )
  })

  it(`never seats someone whose only content sits on a skipped board`, async () => {
    const ports = new MemoryPorts()
    await applyBundle(
      bundleFixture(),
      plan({
        boards: { "b-main": { mode: `skip` } },
        users: {
          "u-h": { mode: `placeholder`, name: `Hannes`, email: `hannes@example.com` },
          "u-x": { mode: `invite`, name: `Stranger`, email: `Stranger@example.com` },
        },
      }),
      ports,
      options
    )
    expect(ports.created.members).toEqual([])
    expect(ports.created.invites).toEqual([])
    expect(ports.membersChanged).toBe(0)
  })

  it(`invites a planned person once, attributes their content to the placeholder, and reuses it on a re-run`, async () => {
    const ports = new MemoryPorts()
    const invited = plan({
      users: { "u-h": { mode: `member`, userId: `member-hannes` }, "u-x": { mode: `invite`, name: `Stranger`, email: `Stranger@example.com` } },
    })
    const result = await applyBundle(bundleFixture(), invited, ports, options)
    expect(ports.created.invites).toEqual([`Stranger@example.com`])
    expect(result.counts.invites).toBe(1)
    const tenWrite = ports.batches[0]![1]!
    expect(tenWrite.issue.creatorId).toBe(`placeholder-Stranger@example.com`)
    expect(tenWrite.comments[0]!.authorId).toBe(`placeholder-Stranger@example.com`)
    expect(tenWrite.comments[0]!.body).not.toMatch(/^\*Imported from/)
    expect(ports.map.get(`user`)?.get(`u-x`)).toBe(`placeholder-Stranger@example.com`)
    // Second run: the placeholder is on the roster → no second invite.
    const again = await applyBundle(bundleFixture(), invited, ports, options)
    expect(ports.created.invites).toEqual([`Stranger@example.com`])
    expect(again.counts.invites).toBe(0)
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
      members: [],
      invites: [],
    })
    expect(result.counts).toMatchObject({ boards: 0, statuses: 0, labels: 0, issues: 0 })
    // The links pass is idempotent on the executor side; here it just runs again.
    expect(result.counts.relations).toBe(4)
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
    expect(result.counts).toMatchObject({ boards: 0, issues: 0, skippedIssues: 0, relations: 4 })
  })

  it(`archives the boards it created for archived issues, after the links, and never a picked board`, async () => {
    const withArchive = () => {
      const base = bundleFixture()
      return {
        ...base,
        boards: [...base.boards, { key: `b-arch`, name: `Main Archive`, prefix: `MAIA`, archive: true }],
        issues: [
          ...base.issues,
          {
            ...base.issues[2]!,
            key: `i-4`,
            boardKey: `b-arch`,
            number: 2,
            externalRef: `MAIN-2`,
            parentKey: null,
            relatedKeys: [],
            blocksKeys: [],
            archived: true,
          },
        ],
      }
    }
    const created = new MemoryPorts()
    const result = await applyBundle(
      withArchive(),
      plan({
        boards: {
          "b-main": { mode: `create`, name: `Main`, prefix: `MAIN`, numbering: `preserve` },
          "b-arch": { mode: `create`, name: `Main Archive`, prefix: `MAIA`, numbering: `preserve` },
        },
      }),
      created,
      options
    )
    expect(created.created.boards).toEqual([`MAIN`, `MAIA`])
    expect(created.archived).toEqual([`board-MAIA`])
    expect(result.counts.issues).toBe(4)
    const phases = created.progress.map((entry) => entry.phase)
    expect(phases.indexOf(`archiving`)).toBeGreaterThan(phases.lastIndexOf(`links`))

    // A resume: every issue is mapped already, the board too, and the first
    // run died before archiving it — this run archives it and nothing else.
    const resumed = new MemoryPorts()
    resumed.map = created.map
    resumed.state = { ...created.state }
    const again = await applyBundle(
      withArchive(),
      plan({
        boards: {
          "b-main": { mode: `create`, name: `Main`, prefix: `MAIN`, numbering: `preserve` },
          "b-arch": { mode: `create`, name: `Main Archive`, prefix: `MAIA`, numbering: `preserve` },
        },
      }),
      resumed,
      options
    )
    expect(again.counts.issues).toBe(0)
    expect(resumed.archived).toEqual([`board-MAIA`])
    resumed.state = {
      ...resumed.state,
      boards: resumed.state.boards.map((board) => (board.id === `board-MAIA` ? { ...board, archived: true } : board)),
    }
    resumed.archived = []
    await applyBundle(withArchive(), plan({ boards: { "b-main": { mode: `create`, name: `Main`, prefix: `MAIN`, numbering: `preserve` }, "b-arch": { mode: `create`, name: `Main Archive`, prefix: `MAIA`, numbering: `preserve` } } }), resumed, options)
    expect(resumed.archived).toEqual([])

    const picked = new MemoryPorts()
    picked.state = { ...picked.state, boards: [{ id: `board-old`, name: `Old`, prefix: `OLD` }] }
    await applyBundle(
      withArchive(),
      plan({
        boards: {
          "b-main": { mode: `create`, name: `Main`, prefix: `MAIN`, numbering: `preserve` },
          "b-arch": { mode: `existing`, boardId: `board-old`, numbering: `allocate` },
        },
      }),
      picked,
      options
    )
    expect(picked.archived).toEqual([])
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
    expect(ports.created).toEqual({ boards: [], statuses: [], labels: [], members: [], invites: [] })
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

  it(`leaves the estimate scale alone when the team already has one or nothing is estimated`, async () => {
    const fibonacci = new MemoryPorts()
    fibonacci.state = { ...fibonacci.state, estimationType: `fibonacci` }
    await applyBundle(bundleFixture(), plan(), fibonacci, options)
    expect(fibonacci.estimation).toEqual([])

    const unestimated = new MemoryPorts()
    const bare = { ...bundleFixture(), issues: bundleFixture().issues.map((issue) => ({ ...issue, estimate: null })) }
    await applyBundle(bare, plan(), unestimated, options)
    expect(unestimated.estimation).toEqual([])
  })

  it(`retries a failed asset download and keeps the original link when it stays down`, async () => {
    const flaky = new MemoryPorts()
    flaky.failFetches = 1
    const recovered = await applyBundle(bundleFixture(), plan(), flaky, options)
    expect(flaky.fetched).toEqual([`https://files.example.com/a.png`, `https://files.example.com/a.png`])
    expect(recovered.counts.attachments).toBe(1)
    expect(recovered.warnings.join(`\n`)).not.toMatch(/could not be downloaded/)

    const down = new MemoryPorts()
    down.failFetches = 3
    const result = await applyBundle(bundleFixture(), plan(), down, options)
    expect(down.fetched).toHaveLength(3)
    expect(result.counts.issues).toBe(3)
    expect(result.counts.attachments).toBe(0)
    expect(result.warnings.join(`\n`)).toMatch(/MAIN-10: a\.png could not be downloaded \(fetch failed\)/)
  })

  it(`falls back to the importer when a mapped member left the team`, async () => {
    const ports = new MemoryPorts()
    ports.state = { ...ports.state, members: [] }
    const result = await applyBundle(bundleFixture(), plan(), ports, options)
    expect(result.warnings.join(`\n`)).toMatch(/Hannes was mapped to a member who left/)
    expect(ports.batches[0]![1]!.issue.assigneeId).toBeNull()
  })
})
