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
  return { selectQueue, select, fakeDb: { select } }
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
}))

import * as prSync from "@/lib/integrations/pr-sync"
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
        mergedAt: new Date(MERGED_AT_ISO),
        actorUserId: null,
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
      mergedAt: new Date(MERGED_AT_ISO),
      actorUserId: null,
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
