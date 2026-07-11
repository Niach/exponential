import { beforeEach, describe, expect, it, vi } from "vitest"
import { createHmac } from "node:crypto"

// EXP-56 Phase 2 — the webhook's pull_request handling falls back to RELEASE
// resolution (exact pr_url via findReleaseIdByPrUrl) only when no ISSUE
// matches; issue resolution stays first and short-circuits. The pr-sync
// writers are mocked wholesale so this file tests only the routing; the
// writers themselves are covered in pr-sync-releases.test.ts. The fake db
// serves resolveIssueForPr's exact-prUrl issue lookup from a FIFO queue.

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
  applyReleasePrClosedState: vi.fn(async () => {}),
  applyReleasePrMergeState: vi.fn(async () => {}),
  applyReleasePrReopenedState: vi.fn(async () => {}),
  findIssueIdByBranch: vi.fn(async () => null),
  findReleaseIdByPrUrl: vi.fn(async () => null),
}))

import * as prSync from "@/lib/integrations/pr-sync"
import { Route } from "./github"

const prSyncMock = vi.mocked(prSync)

const SECRET = `test-webhook-secret`
const HTML_URL = `https://github.com/org/repo/pull/7`
const RELEASE_ID = `22222222-2222-4222-8222-222222222222`
const ISSUE_ID = `33333333-3333-4333-8333-333333333333`
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
      head: { ref: `exp/rel-v1` },
    },
    repository: { full_name: `org/repo` },
  }
}

beforeEach(() => {
  process.env.GITHUB_WEBHOOK_SECRET = SECRET
  h.selectQueue.length = 0
  vi.clearAllMocks()
})

describe(`github webhook — release PR fallback (EXP-56)`, () => {
  it(`closed+merged with no issue match but a release match → applyReleasePrMergeState with htmlUrl + mergedAt`, async () => {
    h.selectQueue.push([]) // exact-prUrl issue lookup misses
    prSyncMock.findReleaseIdByPrUrl.mockResolvedValueOnce(RELEASE_ID)

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
    expect(prSyncMock.findReleaseIdByPrUrl).toHaveBeenCalledWith(HTML_URL)
    expect(prSyncMock.applyReleasePrMergeState).toHaveBeenCalledTimes(1)
    expect(prSyncMock.applyReleasePrMergeState).toHaveBeenCalledWith({
      releaseId: RELEASE_ID,
      prUrl: HTML_URL,
      mergedAt: new Date(MERGED_AT_ISO),
    })
  })

  it(`closed WITHOUT merging with a release match → applyReleasePrClosedState`, async () => {
    h.selectQueue.push([])
    prSyncMock.findReleaseIdByPrUrl.mockResolvedValueOnce(RELEASE_ID)

    const res = await postHandler({
      request: webhookRequest(
        `pull_request`,
        pullRequestPayload({ action: `closed`, merged: false })
      ),
    })

    expect(res.status).toBe(200)
    expect(prSyncMock.applyPrClosedState).not.toHaveBeenCalled()
    expect(prSyncMock.applyReleasePrMergeState).not.toHaveBeenCalled()
    expect(prSyncMock.applyReleasePrClosedState).toHaveBeenCalledTimes(1)
    expect(prSyncMock.applyReleasePrClosedState).toHaveBeenCalledWith({
      releaseId: RELEASE_ID,
      prUrl: HTML_URL,
    })
  })

  it(`reopened with a release match → applyReleasePrReopenedState`, async () => {
    h.selectQueue.push([])
    prSyncMock.findReleaseIdByPrUrl.mockResolvedValueOnce(RELEASE_ID)

    const res = await postHandler({
      request: webhookRequest(
        `pull_request`,
        pullRequestPayload({ action: `reopened` })
      ),
    })

    expect(res.status).toBe(200)
    expect(prSyncMock.applyPrReopenedState).not.toHaveBeenCalled()
    expect(prSyncMock.applyReleasePrReopenedState).toHaveBeenCalledTimes(1)
    expect(prSyncMock.applyReleasePrReopenedState).toHaveBeenCalledWith({
      releaseId: RELEASE_ID,
      prUrl: HTML_URL,
    })
  })

  it(`when an ISSUE matches, release resolution is NOT consulted (issue path stays first)`, async () => {
    h.selectQueue.push([{ id: ISSUE_ID }]) // exact-prUrl issue lookup hits

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
    expect(prSyncMock.applyPrMergeState).toHaveBeenCalledTimes(1)
    expect(prSyncMock.applyPrMergeState).toHaveBeenCalledWith({
      issueId: ISSUE_ID,
      prUrl: HTML_URL,
      mergedAt: new Date(MERGED_AT_ISO),
      actorUserId: null,
    })
    expect(prSyncMock.findReleaseIdByPrUrl).not.toHaveBeenCalled()
    expect(prSyncMock.applyReleasePrMergeState).not.toHaveBeenCalled()
  })

  it(`no issue AND no release match → acked 200 with no writer calls`, async () => {
    h.selectQueue.push([])
    // findReleaseIdByPrUrl keeps its default null resolution.

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
    expect(prSyncMock.findReleaseIdByPrUrl).toHaveBeenCalledWith(HTML_URL)
    expect(prSyncMock.applyPrMergeState).not.toHaveBeenCalled()
    expect(prSyncMock.applyReleasePrMergeState).not.toHaveBeenCalled()
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
    expect(prSyncMock.findReleaseIdByPrUrl).not.toHaveBeenCalled()
    expect(prSyncMock.applyReleasePrMergeState).not.toHaveBeenCalled()
  })
})
