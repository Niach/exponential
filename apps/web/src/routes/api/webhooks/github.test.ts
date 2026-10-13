import { beforeEach, describe, expect, it, vi } from "vitest"
import { createHmac } from "node:crypto"

// EXP-1291: the webhook's pull_request handling resolves the PR to its
// `pull_requests` row by url and hands each transition to the pr-sync
// writers (mocked wholesale here: this file tests the routing, the claims
// and the actor resolution; the writers own their idempotent claims).

const h = vi.hoisted(() => {
  const selectQueue: unknown[][] = []
  // Writes are recorded with the drizzle table object they targeted so the
  // installation-lifecycle tests can assert WHICH table was written.
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
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  adoptOutOfBandPullRequest: vi.fn(async () => {}),
  applyPullRequestBase: vi.fn(async () => ({ txid: 1 })),
  applyPullRequestClosed: vi.fn(async () => ({ applied: true, txid: 1 })),
  applyPullRequestDraft: vi.fn(async () => {}),
  applyPullRequestMerged: vi.fn(async () => ({ applied: true, txid: 1 })),
  applyPullRequestReopened: vi.fn(async () => {}),
  loadPullRequestByUrl: vi.fn(async (): Promise<{ id: string } | null> => null),
}))
// EXP-617: the resolver itself is covered in github-identity.test.ts — here
// we only assert WHICH actor the webhook hands it and that it reaches the
// writers.
vi.mock(`@/lib/integrations/github-identity`, () => ({
  resolveAppUserForGithubActor: vi.fn(async () => null),
}))

import * as prSync from "@/lib/integrations/pr-sync"
import { resolveAppUserForGithubActor } from "@/lib/integrations/github-identity"
import { repositories } from "@/db/schema"
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
const PR_ID = `55555555-5555-4555-8555-555555555555`
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
  base?: string
  draft?: boolean
  user?: unknown
  merged_by?: unknown
  sender?: unknown
  changes?: unknown
}): unknown {
  return {
    action: overrides.action,
    pull_request: {
      html_url: HTML_URL,
      number: 7,
      title: `Fix the nav`,
      merged: overrides.merged ?? false,
      merged_at: overrides.merged_at ?? null,
      draft: overrides.draft ?? false,
      head: { ref: `exp/EXP-9` },
      ...(overrides.base ? { base: { ref: overrides.base } } : {}),
      user: overrides.user,
      merged_by: overrides.merged_by,
    },
    repository: { full_name: `org/repo` },
    sender: overrides.sender,
    ...(overrides.changes ? { changes: overrides.changes } : {}),
  }
}

const known = () => prSyncMock.loadPullRequestByUrl.mockResolvedValue({ id: PR_ID } as never)

beforeEach(() => {
  process.env.GITHUB_WEBHOOK_SECRET = SECRET
  h.selectQueue.length = 0
  h.inserts.length = 0
  h.updates.length = 0
  h.deletes.length = 0
  _clearPrActorClaims()
  vi.clearAllMocks()
  prSyncMock.loadPullRequestByUrl.mockResolvedValue(null)
})

describe(`github webhook — pull_request routing (EXP-1291)`, () => {
  it(`closed+merged on a known PR → applyPullRequestMerged, unattributed without a claim`, async () => {
    known()
    const res = await postHandler({
      request: webhookRequest(
        `pull_request`,
        pullRequestPayload({ action: `closed`, merged: true, merged_at: MERGED_AT_ISO })
      ),
    })
    expect(res.status).toBe(200)
    expect(prSyncMock.applyPullRequestMerged).toHaveBeenCalledWith({
      pullRequestId: PR_ID,
      mergedAt: new Date(MERGED_AT_ISO),
      githubActorUserId: null,
      actorUserId: null,
    })
    expect(prSyncMock.adoptOutOfBandPullRequest).not.toHaveBeenCalled()
  })

  it(`closed+merged on an unknown PR adopts it first (a lost 'opened', REV-26)`, async () => {
    prSyncMock.loadPullRequestByUrl
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce({ id: PR_ID } as never)
    await postHandler({
      request: webhookRequest(
        `pull_request`,
        pullRequestPayload({ action: `closed`, merged: true, merged_at: MERGED_AT_ISO, base: `master` })
      ),
    })
    expect(prSyncMock.adoptOutOfBandPullRequest).toHaveBeenCalledWith(
      expect.objectContaining({ url: HTML_URL, number: 7, head: `exp/EXP-9`, base: `master` })
    )
    expect(prSyncMock.applyPullRequestMerged).toHaveBeenCalledWith(
      expect.objectContaining({ pullRequestId: PR_ID })
    )
  })

  it(`an unknown merged PR nobody can adopt is acked with no write`, async () => {
    const res = await postHandler({
      request: webhookRequest(`pull_request`, pullRequestPayload({ action: `closed`, merged: true })),
    })
    expect(res.status).toBe(200)
    expect(prSyncMock.applyPullRequestMerged).not.toHaveBeenCalled()
  })

  it(`closed WITHOUT merging → applyPullRequestClosed`, async () => {
    known()
    await postHandler({
      request: webhookRequest(`pull_request`, pullRequestPayload({ action: `closed` })),
    })
    expect(prSyncMock.applyPullRequestClosed).toHaveBeenCalledWith({ pullRequestId: PR_ID })
    expect(prSyncMock.applyPullRequestMerged).not.toHaveBeenCalled()
  })

  it(`reopened forwards the live base and the draft flag`, async () => {
    known()
    await postHandler({
      request: webhookRequest(
        `pull_request`,
        pullRequestPayload({ action: `reopened`, base: `exp/EXP-8`, draft: true })
      ),
    })
    expect(prSyncMock.applyPullRequestReopened).toHaveBeenCalledWith({
      pullRequestId: PR_ID,
      base: `exp/EXP-8`,
      draft: true,
    })
  })

  it(`opened hands the PR to the out-of-band adoption`, async () => {
    await postHandler({
      request: webhookRequest(`pull_request`, pullRequestPayload({ action: `opened`, base: `master` })),
    })
    expect(prSyncMock.adoptOutOfBandPullRequest).toHaveBeenCalledWith({
      repoFullName: `org/repo`,
      url: HTML_URL,
      number: 7,
      title: `Fix the nav`,
      head: `exp/EXP-9`,
      base: `master`,
      draft: false,
      githubActorUserId: null,
      actorUserId: null,
    })
  })

  it(`opened consumes the open claim (keyed on the head branch)`, async () => {
    claimPrOpen(`org/repo`, `exp/EXP-9`, { userId: `user-1`, viaAgent: true })
    await postHandler({
      request: webhookRequest(`pull_request`, pullRequestPayload({ action: `opened`, base: `master` })),
    })
    expect(prSyncMock.adoptOutOfBandPullRequest).toHaveBeenCalledWith(
      expect.objectContaining({ actorUserId: `user-1`, actorViaAgent: true })
    )
  })

  it(`closed+merged consumes the merge claim exactly once`, async () => {
    known()
    claimPrMerge(`org/repo`, 7, { userId: `user-2`, viaAgent: false })
    const payload = pullRequestPayload({ action: `closed`, merged: true, merged_at: MERGED_AT_ISO })
    await postHandler({ request: webhookRequest(`pull_request`, payload) })
    expect(prSyncMock.applyPullRequestMerged).toHaveBeenLastCalledWith(
      expect.objectContaining({ actorUserId: `user-2`, actorViaAgent: false })
    )
    await postHandler({ request: webhookRequest(`pull_request`, payload) })
    expect(prSyncMock.applyPullRequestMerged).toHaveBeenLastCalledWith(
      expect.objectContaining({ actorUserId: null })
    )
  })

  it(`a merge claim never leaks into the opened path`, async () => {
    claimPrMerge(`org/repo`, 7, { userId: `user-2`, viaAgent: false })
    await postHandler({
      request: webhookRequest(`pull_request`, pullRequestPayload({ action: `opened`, base: `master` })),
    })
    expect(prSyncMock.adoptOutOfBandPullRequest).toHaveBeenCalledWith(
      expect.objectContaining({ actorUserId: null })
    )
  })

  it(`edited stores a BASE change and ignores other edits`, async () => {
    known()
    await postHandler({
      request: webhookRequest(
        `pull_request`,
        pullRequestPayload({ action: `edited`, base: `exp/EXP-10`, changes: { base: { from: { ref: `master` } } } })
      ),
    })
    expect(prSyncMock.applyPullRequestBase).toHaveBeenCalledWith({
      pullRequestId: PR_ID,
      base: `exp/EXP-10`,
    })
    vi.clearAllMocks()
    known()
    await postHandler({
      request: webhookRequest(
        `pull_request`,
        pullRequestPayload({ action: `edited`, base: `exp/EXP-10`, changes: { title: { from: `old` } } })
      ),
    })
    expect(prSyncMock.applyPullRequestBase).not.toHaveBeenCalled()
  })

  it.each([
    [`converted_to_draft`, true],
    [`ready_for_review`, false],
  ])(`%s flips the stored draft state`, async (action, draft) => {
    known()
    await postHandler({ request: webhookRequest(`pull_request`, pullRequestPayload({ action })) })
    expect(prSyncMock.applyPullRequestDraft).toHaveBeenCalledWith({ pullRequestId: PR_ID, draft })
  })

  it(`rejects a bad signature with 401 before touching anything`, async () => {
    const res = await postHandler({
      request: webhookRequest(`pull_request`, pullRequestPayload({ action: `closed`, merged: true }), {
        signature: `sha256=bad`,
      }),
    })
    expect(res.status).toBe(401)
    expect(prSyncMock.loadPullRequestByUrl).not.toHaveBeenCalled()
  })
})

// EXP-617: GitHub tells us who acted; the mapping turns that into an app
// user so they can be kept out of a notification about their own click.
describe(`github webhook — GitHub actor identity (EXP-617)`, () => {
  const resolve = vi.mocked(resolveAppUserForGithubActor)
  const HUMAN = { id: 4242, login: `niach`, type: `User` }
  const BOT = { id: 1, login: `exponential[bot]`, type: `Bot` }

  it(`prefers merged_by over sender and forwards the resolved user`, async () => {
    known()
    resolve.mockResolvedValueOnce(`user-human`)
    await postHandler({
      request: webhookRequest(
        `pull_request`,
        pullRequestPayload({ action: `closed`, merged: true, merged_by: HUMAN, sender: BOT })
      ),
    })
    expect(resolve).toHaveBeenCalledWith(HUMAN)
    expect(prSyncMock.applyPullRequestMerged).toHaveBeenCalledWith(
      expect.objectContaining({ githubActorUserId: `user-human` })
    )
  })

  it(`falls back to sender when merged_by is absent`, async () => {
    known()
    await postHandler({
      request: webhookRequest(`pull_request`, pullRequestPayload({ action: `closed`, merged: true, sender: HUMAN })),
    })
    expect(resolve).toHaveBeenCalledWith(HUMAN)
  })

  it(`opened resolves the PR author`, async () => {
    resolve.mockResolvedValueOnce(`user-author`)
    await postHandler({
      request: webhookRequest(
        `pull_request`,
        pullRequestPayload({ action: `opened`, base: `master`, user: HUMAN, sender: BOT })
      ),
    })
    expect(resolve).toHaveBeenCalledWith(HUMAN)
    expect(prSyncMock.adoptOutOfBandPullRequest).toHaveBeenCalledWith(
      expect.objectContaining({ githubActorUserId: `user-author` })
    )
  })
})

// SLOP-24: installations are never stored (discovery lists them LIVE off the
// member's token). The lifecycle events only flag bound repos: `suspend`
// (reversible, REV2-29) and `deleted` (terminal) both stop token minting;
// `created`/`unsuspend` write nothing (a verified access proof clears flags).
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

  const repositoryUpdates = () =>
    h.updates.filter((u) => u.table === repositories)

  it.each([`suspend`, `deleted`])(
    `%s flags the bound repos as inaccessible and stores nothing`,
    async (action) => {
      const res = await postHandler({
        request: webhookRequest(`installation`, installationPayload(action)),
      })

      expect(res.status).toBe(200)
      expect(h.updates).toHaveLength(1)
      expect(repositoryUpdates()).toHaveLength(1)
      expect(repositoryUpdates()[0].set.inaccessibleAt).toBeInstanceOf(Date)
      expect(h.inserts).toHaveLength(0)
      expect(h.deletes).toHaveLength(0)
    }
  )

  it.each([`created`, `unsuspend`])(`%s writes nothing`, async (action) => {
    const res = await postHandler({
      request: webhookRequest(`installation`, installationPayload(action)),
    })

    expect(res.status).toBe(200)
    expect(h.inserts).toHaveLength(0)
    expect(h.updates).toHaveLength(0)
    expect(h.deletes).toHaveLength(0)
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
// squatted account. The heal's where clause must also scope by installation:
// the same installation, or a still-unbound row (SLOP-7: the claim subquery
// went with the claim tables).
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

  it(`added heals with the installation-scoped where clause, not by full_name alone`, async () => {
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
    expect(repositoryUpdates()[0].where).toBeDefined()
  })

})
