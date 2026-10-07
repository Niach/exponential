import { afterEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"
import { patchPullDescription } from "@/lib/trpc/pr-update"

// FEED-74: `exponential_pr_update` answered "GitHub update failed:" with
// nothing after the colon on a 500 with an empty body. The wrapper names the
// HTTP status and the formatter's text is never empty.

const headers = new Headers({ "x-github-request-id": `1234:ABCD` })

function response(status: number, text: string) {
  return {
    ok: status >= 200 && status < 300,
    status,
    headers,
    text: async () => text,
    json: async () => JSON.parse(text),
  }
}

async function failure(status: number, text: string): Promise<TRPCError> {
  vi.stubGlobal(`fetch`, vi.fn(async () => response(status, text)))
  const err = await patchPullDescription({
    repoFullName: `o/r`,
    prNumber: 977,
    token: `tok`,
    fields: { body: `report` },
  }).catch((e: unknown) => e)
  expect(err).toBeInstanceOf(TRPCError)
  return err as TRPCError
}

describe(`patchPullDescription`, () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it(`names the status and GitHub's request id on a 5xx with an empty body`, async () => {
    const err = await failure(500, ``)
    expect(err.code).toBe(`INTERNAL_SERVER_ERROR`)
    expect(err.message).toBe(
      `GitHub update failed (HTTP 500): empty response body (request 1234:ABCD)`
    )
  })

  it(`carries GitHub's validation detail on a 422`, async () => {
    const err = await failure(
      422,
      JSON.stringify({
        message: `Validation Failed`,
        errors: [{ resource: `PullRequest`, field: `body`, code: `too_long` }],
      })
    )
    expect(err.code).toBe(`PRECONDITION_FAILED`)
    expect(err.message).toBe(
      `GitHub refused the update: Validation Failed: PullRequest.body too_long`
    )
  })

  it(`maps 404 onto NOT_FOUND`, async () => {
    const err = await failure(404, `{"message":"Not Found"}`)
    expect(err.code).toBe(`NOT_FOUND`)
  })
})
