import { beforeEach, describe, expect, it, vi } from "vitest"
import { createHmac } from "node:crypto"

// The webhook's pull_request handling resolves the PR to ISSUES by exact
// pr_url first — PLURAL, because a batch coding run links all its issues to
// one combined PR — then falls back to the single-issue head-branch parse.
// The pr-sync writers are mocked wholesale so this file tests only the
// routing/fan-out; the writers themselves are covered in pr-sync.test.ts. The
// fake db serves resolveIssuesForPr's exact-prUrl lookup from a FIFO queue.

const h = vi.hoisted(() => {
  const selectQueue: unknown[][] = []
  // Writes are recorded with the drizzle table object they targeted so the
  // installation-lifecycle tests can assert WHICH table was written (and, for
  // `suspend`, that github_installations was never DELETEd — that delete is
  // what used to cascade every team's claim link away).
  const inserts: Array<{
    table: unknown
    values?: unknown
    conflictSet?: unknown
  }> = []
  const updates: Array<{
    table: unknown
    set: Record<string, unknown>
    where?: unknown
  }> = []
  const deletes: Array<{ table: unknown }> = []

  function selectChain(): Promise<unknown[]> & Record<string, () => unknown> {
    const p = Promise.resolve(
      selectQueue.shift() ?? []
    ) as Promise<unknown[]> & Record<string, () => unknown>
    for (const m of [`from`, `where`, `innerJoin`, `limit`]) {
      p[m] = () => p
    }
    return p
  }

  const select = vi.fn(() => selectChain())
  const insert = vi.fn((table: unknown) => {
    const record: { table: unknown; values?: unknown; conflictSet?: unknown } =
      { table }
    inserts.push(record)
    const chain = {
      values: (v: unknown) => {
        record.values = v
        return chain
      },
      onConflictDoUpdate: (arg: { set?: unknown }) => {
        record.conflictSet = arg?.set
        return Promise.resolve()
      },
      onConflictDoNothing: () => Promise.resolve(),
    }
    return chain
  })
  const update = vi.fn((table: unknown) => ({
    set: (set: Record<string, unknown>) => ({
      where: (where?: unknown) => {
        updates.push({ table, set, where })
        return Promise.resolve()
      },
    }),
  }))
  const del = vi.fn((table: unknown) => ({
    where: () => {
      deletes.push({ table })
      return Promise.resolve()
    },
  }))
  return {
    selectQueue,
    inserts,
    updates,
    deletes,
    select,
    fakeDb: { select, insert, update, delete: del },
  }
})

vi.mock(`@/db/connection`, () => ({ db: h.fakeDb }))
vi.mock(`@/lib/trpc/integrations`, () => ({
  invalidateRepoCacheForInstallation: vi.fn(async () => {}),
}))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applyPrClosedState: vi.fn(async () => {}),
  applyPrMergeState: vi.fn(async () => {}),
  applyPrOpenedState: vi.fn(async () => {}),
  applyPrReopenedState: vi.fn(async () => {}),
  findIssueIdByBranch: vi.fn(async () => null),
  endSessionsOnMergedBranch: vi.fn(async () => {}),
}))
// EXP-617: the resolver itself (bot filter, id-over-login rule) is covered in
// github-identity.test.ts — here we only assert WHICH actor the webhook hands
// it and that the answer reaches the writers.
vi.mock(`@/lib/integrations/github-identity`, () => ({
  resolveAppUserForGithubActor: vi.fn(async () => null),
}))

import * as prSync from "@/lib/integrations/pr-sync"
import { resolveAppUserForGithubActor } from "@/lib/integrations/github-identity"
import * as integrations from "@/lib/trpc/integrations"
import { githubInstallations, repositories } from "@/db/schema"
// Deliberately the REAL module (in-memory, no I/O): these tests exercise the
// claim → webhook handoff end to end.
import {
  _clearPrActorClaims,
  claimPrMerge,
  claimPrOpen,
} from "@/lib/integrations/pr-actor-claims"
import { Route } from "./github"

const prSyncMock = vi.mocked(prSync)

const SECRET = `test-webhook-secret`
const HTML_URL = `https://github.com/org/repo/pull/7`
const ISSUE_A = `33333333-3333-4333-8333-333333333333`
const ISSUE_B = `44444444-4444-4444-8444-444444444444`
const MERGED_AT_ISO = `2026-07-11T10:00:00Z`

// The route file exports only the Route; its POST handler wraps
// handleGithubWebhook. Dig it out of the route options.
const postHandler = (
  Route.options as unknown as {
    server: {
      handlers: { POST: (ctx: { request: Request }) => Promise<Response> }
    }
  }
).server.handlers.POST

function sign(rawBody: string): string {
  return `sha256=${createHmac(`sha256`, SECRET).update(rawBody).digest(`hex`)}`
}

function webhookRequest(
  event: string,
  payload: unknown,
  opts?: { signature?: string }
): Request {
  const body = JSON.stringify(payload)
  return new Request(`http://localhost/api/webhooks/github`, {
    method: `POST`,
    headers: {
      "content-type": `application/json`,
      "x-github-event": event,
      "x-hub-signature-256": opts?.signature ?? sign(body),
    },
    body,
  })
}

function pullRequestPayload(overrides: {
  action: string
  merged?: boolean
  merged_at?: string | null
}): unknown {
  return {
    action: overrides.action,
    pull_request: {
      html_url: HTML_URL,
      number: 7,
      merged: overrides.merged ?? false,
      merged_at: overrides.merged_at ?? null,
      head: { ref: `exp/batch-a1b2c3d4` },
    },
    repository: { full_name: `org/repo` },
  }
}

beforeEach(() => {
  process.env.GITHUB_WEBHOOK_SECRET = SECRET
  h.selectQueue.length = 0
  h.inserts.length = 0
  h.updates.length = 0
  h.deletes.length = 0
  _clearPrActorClaims()
  vi.clearAllMocks()
})

describe(`github webhook — batch PR fan-out (multi-issue pr_url resolution)`, () => {
  it(`closed+merged with TWO issues on the pr_url → applyPrMergeState for each`, async () => {
    h.selectQueue.push([{ id: ISSUE_A }, { id: ISSUE_B }])

    const res = await postHandler({
      request: webhookRequest(
        `pull_request`,
        pullRequestPayload({
          action: `closed`,
          merged: true,
          merged_at: MERGED_AT_ISO,
        })
      ),
    })

    expect(res.status).toBe(200)
    expect(prSyncMock.applyPrMergeState).toHaveBeenCalledTimes(2)
    for (const issueId of [ISSUE_A, ISSUE_B]) {
      expect(prSyncMock.applyPrMergeState).toHaveBeenCalledWith({
        issueId,
        prUrl: HTML_URL,
        prNumber: 7,
        headBranch: `exp/batch-a1b2c3d4`,
        mergedAt: new Date(MERGED_AT_ISO),
        actorUserId: null,
        githubActorUserId: null,
      })
    }
    // The exact-prUrl match short-circuits the branch parse.
    expect(prSyncMock.findIssueIdByBranch).not.toHaveBeenCalled()
  })

  it(`closed WITHOUT merging with two issues → applyPrClosedState for each`, async () => {
    h.selectQueue.push([{ id: ISSUE_A }, { id: ISSUE_B }])

    const res = await postHandler({
      request: webhookRequest(
        `pull_request`,
        pullRequestPayload({ action: `closed`, merged: false })
      ),
    })

    expect(res.status).toBe(200)
    expect(prSyncMock.applyPrMergeState).not.toHaveBeenCalled()
    expect(prSyncMock.applyPrClosedState).toHaveBeenCalledTimes(2)
    expect(prSyncMock.applyPrClosedState).toHaveBeenCalledWith({
      issueId: ISSUE_A,
      prUrl: HTML_URL,
    })
    expect(prSyncMock.applyPrClosedState).toHaveBeenCalledWith({
      issueId: ISSUE_B,
      prUrl: HTML_URL,
    })
  })

  it(`reopened with two issues → applyPrReopenedState for each`, async () => {
    h.selectQueue.push([{ id: ISSUE_A }, { id: ISSUE_B }])

    const res = await postHandler({
      request: webhookRequest(
        `pull_request`,
        pullRequestPayload({ action: `reopened` })
      ),
    })

    expect(res.status).toBe(200)
    expect(prSyncMock.applyPrReopenedState).toHaveBeenCalledTimes(2)
  })

  it(`no pr_url match falls back to the single-issue branch parse`, async () => {
    h.selectQueue.push([]) // exact-prUrl lookup misses
    prSyncMock.findIssueIdByBranch.mockResolvedValueOnce(ISSUE_A)

    const res = await postHandler({
      request: webhookRequest(
        `pull_request`,
        pullRequestPayload({
          action: `closed`,
          merged: true,
          merged_at: MERGED_AT_ISO,
        })
      ),
    })

    expect(res.status).toBe(200)
    expect(prSyncMock.findIssueIdByBranch).toHaveBeenCalledWith(
      `org/repo`,
      `exp/batch-a1b2c3d4`
    )
    expect(prSyncMock.applyPrMergeState).toHaveBeenCalledTimes(1)
    expect(prSyncMock.applyPrMergeState).toHaveBeenCalledWith({
      issueId: ISSUE_A,
      prUrl: HTML_URL,
      prNumber: 7,
      headBranch: `exp/batch-a1b2c3d4`,
      mergedAt: new Date(MERGED_AT_ISO),
      actorUserId: null,
      githubActorUserId: null,
    })
  })

  it(`no match anywhere → acked 200 with no writer calls`, async () => {
    h.selectQueue.push([])
    // findIssueIdByBranch keeps its default null resolution.

    const res = await postHandler({
      request: webhookRequest(
        `pull_request`,
        pullRequestPayload({
          action: `closed`,
          merged: true,
          merged_at: MERGED_AT_ISO,
        })
      ),
    })

    expect(res.status).toBe(200)
    expect(prSyncMock.applyPrMergeState).not.toHaveBeenCalled()
    expect(prSyncMock.applyPrClosedState).not.toHaveBeenCalled()
    // EXP-637/EXP-626: no issue resolved means the PR may still be an
    // issue-less chore PR an action run opened — the branch is the only
    // handle on the session that opened it.
    expect(prSyncMock.endSessionsOnMergedBranch).toHaveBeenCalledWith(
      `org/repo`,
      `exp/batch-a1b2c3d4`
    )
  })

  it(`opened out-of-band links every resolved issue`, async () => {
    h.selectQueue.push([{ id: ISSUE_A }, { id: ISSUE_B }])

    const res = await postHandler({
      request: webhookRequest(
        `pull_request`,
        pullRequestPayload({ action: `opened` })
      ),
    })

    expect(res.status).toBe(200)
    expect(prSyncMock.applyPrOpenedState).toHaveBeenCalledTimes(2)
    expect(prSyncMock.applyPrOpenedState).toHaveBeenCalledWith({
      issueId: ISSUE_A,
      prUrl: HTML_URL,
      prNumber: 7,
      branch: `exp/batch-a1b2c3d4`,
      actorUserId: null,
      githubActorUserId: null,
    })
  })

  // EXP-494: an in-app merge/open claims its initiator right before the
  // GitHub call; the webhook resolves the claim ONCE per delivery and passes
  // the actor into every per-issue apply, so the fan-out is attributed (and
  // the initiator excluded) even when no coding_sessions row survived.
  it(`closed+merged consumes the merge claim and attributes EVERY issue of a batch PR`, async () => {
    claimPrMerge(`org/repo`, 7, { userId: `u-merger`, viaAgent: true })
    h.selectQueue.push([{ id: ISSUE_A }, { id: ISSUE_B }])

    const res = await postHandler({
      request: webhookRequest(
        `pull_request`,
        pullRequestPayload({
          action: `closed`,
          merged: true,
          merged_at: MERGED_AT_ISO,
        })
      ),
    })

    expect(res.status).toBe(200)
    expect(prSyncMock.applyPrMergeState).toHaveBeenCalledTimes(2)
    for (const issueId of [ISSUE_A, ISSUE_B]) {
      expect(prSyncMock.applyPrMergeState).toHaveBeenCalledWith({
        issueId,
        prUrl: HTML_URL,
        prNumber: 7,
        headBranch: `exp/batch-a1b2c3d4`,
        mergedAt: new Date(MERGED_AT_ISO),
        actorUserId: `u-merger`,
        actorViaAgent: true,
        githubActorUserId: null,
      })
    }

    // Take = consume: a redelivery falls back to the anonymous shape.
    h.selectQueue.push([{ id: ISSUE_A }])
    prSyncMock.applyPrMergeState.mockClear()
    await postHandler({
      request: webhookRequest(
        `pull_request`,
        pullRequestPayload({
          action: `closed`,
          merged: true,
          merged_at: MERGED_AT_ISO,
        })
      ),
    })
    expect(prSyncMock.applyPrMergeState).toHaveBeenCalledWith({
      issueId: ISSUE_A,
      prUrl: HTML_URL,
      prNumber: 7,
      headBranch: `exp/batch-a1b2c3d4`,
      mergedAt: new Date(MERGED_AT_ISO),
      actorUserId: null,
      githubActorUserId: null,
    })
  })

  it(`opened consumes the open claim (keyed on the head branch) and attributes the link`, async () => {
    claimPrOpen(`org/repo`, `exp/batch-a1b2c3d4`, {
      userId: `u-opener`,
      viaAgent: true,
    })
    h.selectQueue.push([{ id: ISSUE_A }])

    const res = await postHandler({
      request: webhookRequest(
        `pull_request`,
        pullRequestPayload({ action: `opened` })
      ),
    })

    expect(res.status).toBe(200)
    expect(prSyncMock.applyPrOpenedState).toHaveBeenCalledWith({
      issueId: ISSUE_A,
      prUrl: HTML_URL,
      prNumber: 7,
      branch: `exp/batch-a1b2c3d4`,
      actorUserId: `u-opener`,
      actorViaAgent: true,
      githubActorUserId: null,
    })
  })

  it(`a merge claim never leaks into the opened path`, async () => {
    claimPrMerge(`org/repo`, 7, { userId: `u-merger`, viaAgent: false })
    h.selectQueue.push([{ id: ISSUE_A }])

    await postHandler({
      request: webhookRequest(
        `pull_request`,
        pullRequestPayload({ action: `opened` })
      ),
    })

    expect(prSyncMock.applyPrOpenedState).toHaveBeenCalledWith({
      issueId: ISSUE_A,
      prUrl: HTML_URL,
      prNumber: 7,
      branch: `exp/batch-a1b2c3d4`,
      actorUserId: null,
      githubActorUserId: null,
    })
  })

  it(`rejects a bad signature with 401 before touching any resolution`, async () => {
    const res = await postHandler({
      request: webhookRequest(
        `pull_request`,
        pullRequestPayload({ action: `closed`, merged: true }),
        { signature: `sha256=${`0`.repeat(64)}` }
      ),
    })

    expect(res.status).toBe(401)
    expect(h.select).not.toHaveBeenCalled()
    expect(prSyncMock.applyPrMergeState).not.toHaveBeenCalled()
  })
})

// REV2-29: GitHub suspension is REVERSIBLE, so `suspend` must never take the
// terminal path. It used to delete the github_installations row, which
// CASCADE-dropped every team's claim link — and `unsuspend` re-inserted a row
// with a fresh uuid PK, so those links were unrecoverable by construction. The
// marker column keeps the row (and the claims) and `unsuspend` clears it.
describe(`github webhook — installation lifecycle (suspend/unsuspend/deleted)`, () => {
  const INSTALLATION_ID = 424242

  function installationPayload(action: string): unknown {
    return {
      action,
      installation: {
        id: INSTALLATION_ID,
        account: { login: `acme`, type: `Organization` },
      },
    }
  }

  const installationUpdates = () =>
    h.updates.filter((u) => u.table === githubInstallations)
  const repositoryUpdates = () =>
    h.updates.filter((u) => u.table === repositories)

  it(`suspend MARKS the installation and never deletes it (claim links survive)`, async () => {
    const res = await postHandler({
      request: webhookRequest(`installation`, installationPayload(`suspend`)),
    })

    expect(res.status).toBe(200)
    // The row (and, through it, every team's claim link) stays put.
    expect(h.deletes).toHaveLength(0)
    expect(installationUpdates()).toHaveLength(1)
    expect(installationUpdates()[0].set.suspendedAt).toBeInstanceOf(Date)
    // Repos under it are flagged so the settings UI stops showing them healthy.
    expect(repositoryUpdates()).toHaveLength(1)
    expect(repositoryUpdates()[0].set.inaccessibleAt).toBeInstanceOf(Date)
    expect(
      vi.mocked(integrations.invalidateRepoCacheForInstallation)
    ).toHaveBeenCalledWith(INSTALLATION_ID)
  })

  it(`unsuspend CLEARS the marker on the surviving row (self-heal)`, async () => {
    const res = await postHandler({
      request: webhookRequest(`installation`, installationPayload(`unsuspend`)),
    })

    expect(res.status).toBe(200)
    expect(h.deletes).toHaveLength(0)
    expect(h.inserts).toHaveLength(1)
    expect(h.inserts[0].table).toBe(githubInstallations)
    expect(h.inserts[0].values).toMatchObject({
      installationId: INSTALLATION_ID,
      suspendedAt: null,
    })
    // …and on the conflict path too — the row always already exists here.
    expect(h.inserts[0].conflictSet).toMatchObject({ suspendedAt: null })
  })

  it(`created upserts with no suspension marker`, async () => {
    const res = await postHandler({
      request: webhookRequest(`installation`, installationPayload(`created`)),
    })

    expect(res.status).toBe(200)
    expect(h.inserts[0].values).toMatchObject({ suspendedAt: null })
    expect(h.updates).toHaveLength(0)
    expect(h.deletes).toHaveLength(0)
  })

  it(`deleted stays terminal: flags repos, then drops the row (links cascade)`, async () => {
    const res = await postHandler({
      request: webhookRequest(`installation`, installationPayload(`deleted`)),
    })

    expect(res.status).toBe(200)
    expect(repositoryUpdates()).toHaveLength(1)
    expect(repositoryUpdates()[0].set.inaccessibleAt).toBeInstanceOf(Date)
    expect(installationUpdates()).toHaveLength(0)
    expect(h.deletes).toEqual([{ table: githubInstallations }])
    // The cache invalidation must run BEFORE the delete — it resolves the
    // linked teams through the links that cascade away with the row.
    expect(
      vi.mocked(integrations.invalidateRepoCacheForInstallation)
    ).toHaveBeenCalled()
  })

  it(`ignores an installation event with no installation id`, async () => {
    const res = await postHandler({
      request: webhookRequest(`installation`, { action: `suspend` }),
    })

    expect(res.status).toBe(200)
    expect(h.updates).toHaveLength(0)
    expect(h.deletes).toHaveLength(0)
  })
})

// EXP-363 hardening: the `repositories_added` heal used to match rows by
// full_name ALONE, so any installation of the App could rebind another team's
// registry row (clear its no-access flag + repoint installation_id) just by
// adding a same-named repo — reachable from outside via a renamed-then-
// squatted account. The heal's where clause must now also scope by tenant:
// same installation, still-unbound, or a team that actually CLAIMED this
// installation (resolved via the claiming-teams subquery).
describe(`github webhook — installation_repositories heal scoping`, () => {
  const INSTALLATION_ID = 515151

  function repoSelectionPayload(overrides: {
    added?: string[]
    removed?: string[]
  }): unknown {
    return {
      action: `added`,
      installation: {
        id: INSTALLATION_ID,
        account: { login: `acme`, type: `Organization` },
      },
      repositories_added: (overrides.added ?? []).map((full_name) => ({
        full_name,
      })),
      repositories_removed: (overrides.removed ?? []).map((full_name) => ({
        full_name,
      })),
    }
  }

  const repositoryUpdates = () =>
    h.updates.filter((u) => u.table === repositories)

  it(`removed flags rows scoped to THIS installation`, async () => {
    const res = await postHandler({
      request: webhookRequest(
        `installation_repositories`,
        repoSelectionPayload({ removed: [`acme/app`] })
      ),
    })

    expect(res.status).toBe(200)
    expect(repositoryUpdates()).toHaveLength(1)
    expect(repositoryUpdates()[0].set.inaccessibleAt).toBeInstanceOf(Date)
  })

  it(`added heals with the claim-scoped where clause, not by full_name alone`, async () => {
    const res = await postHandler({
      request: webhookRequest(
        `installation_repositories`,
        repoSelectionPayload({ added: [`acme/app`] })
      ),
    })

    expect(res.status).toBe(200)
    expect(repositoryUpdates()).toHaveLength(1)
    expect(repositoryUpdates()[0].set).toMatchObject({
      inaccessibleAt: null,
      installationId: INSTALLATION_ID,
    })
    // The tenant scoping resolves the claiming teams through a subquery — the
    // old full_name-only heal never touched db.select at all.
    expect(h.select).toHaveBeenCalled()
    expect(repositoryUpdates()[0].where).toBeDefined()
  })
})

// EXP-617: the merge-side answer to "the webhook is all we get". GitHub tells
// us who acted; the mapping turns that into an app user so they can be kept
// out of a notification about their own click.
describe(`github webhook — GitHub actor identity (EXP-617)`, () => {
  const resolve = vi.mocked(resolveAppUserForGithubActor)
  const HUMAN = { id: 4242, login: `niach`, type: `User` }
  const BOT = { id: 1, login: `exponential[bot]`, type: `Bot` }

  function actorPayload(overrides: {
    action: string
    merged?: boolean
    sender?: unknown
    user?: unknown
    merged_by?: unknown
  }): unknown {
    return {
      action: overrides.action,
      pull_request: {
        html_url: HTML_URL,
        number: 7,
        merged: overrides.merged ?? false,
        merged_at: overrides.merged ? MERGED_AT_ISO : null,
        head: { ref: `exp/EXP-9` },
        user: overrides.user,
        merged_by: overrides.merged_by,
      },
      repository: { full_name: `org/repo` },
      sender: overrides.sender,
    }
  }

  it(`prefers merged_by over sender and forwards the resolved user`, async () => {
    h.selectQueue.push([{ id: ISSUE_A }])
    resolve.mockResolvedValue(`u-merger`)

    const res = await postHandler({
      request: webhookRequest(
        `pull_request`,
        actorPayload({
          action: `closed`,
          merged: true,
          merged_by: HUMAN,
          sender: BOT,
        })
      ),
    })

    expect(res.status).toBe(200)
    expect(resolve).toHaveBeenCalledWith(HUMAN)
    expect(prSyncMock.applyPrMergeState).toHaveBeenCalledWith(
      expect.objectContaining({ githubActorUserId: `u-merger` })
    )
  })

  it(`falls back to sender when merged_by is absent`, async () => {
    h.selectQueue.push([{ id: ISSUE_A }])
    resolve.mockResolvedValue(`u-sender`)

    await postHandler({
      request: webhookRequest(
        `pull_request`,
        actorPayload({ action: `closed`, merged: true, sender: HUMAN })
      ),
    })

    expect(resolve).toHaveBeenCalledWith(HUMAN)
  })

  it(`resolves once per delivery, so one lookup covers a batch PR's issues`, async () => {
    h.selectQueue.push([{ id: ISSUE_A }, { id: ISSUE_B }])
    resolve.mockResolvedValue(`u-merger`)

    await postHandler({
      request: webhookRequest(
        `pull_request`,
        actorPayload({ action: `closed`, merged: true, merged_by: HUMAN })
      ),
    })

    expect(prSyncMock.applyPrMergeState).toHaveBeenCalledTimes(2)
    expect(resolve).toHaveBeenCalledTimes(1)
  })

  it(`opened resolves the PR author and forwards it alongside the claim`, async () => {
    h.selectQueue.push([{ id: ISSUE_A }])
    resolve.mockResolvedValue(`u-author`)
    claimPrOpen(`org/repo`, `exp/EXP-9`, { userId: `u-claim`, viaAgent: true })

    await postHandler({
      request: webhookRequest(
        `pull_request`,
        actorPayload({ action: `opened`, user: HUMAN })
      ),
    })

    expect(resolve).toHaveBeenCalledWith(HUMAN)
    // The claim still owns the title; the GitHub identity rides along so both
    // views of the same act are excluded.
    expect(prSyncMock.applyPrOpenedState).toHaveBeenCalledWith(
      expect.objectContaining({
        actorUserId: `u-claim`,
        actorViaAgent: true,
        githubActorUserId: `u-author`,
      })
    )
  })

  it(`forwards a null for an unmapped or bot actor`, async () => {
    h.selectQueue.push([{ id: ISSUE_A }])
    resolve.mockResolvedValue(null)

    await postHandler({
      request: webhookRequest(
        `pull_request`,
        actorPayload({ action: `opened`, user: BOT })
      ),
    })

    expect(prSyncMock.applyPrOpenedState).toHaveBeenCalledWith(
      expect.objectContaining({ githubActorUserId: null, actorUserId: null })
    )
  })
})
