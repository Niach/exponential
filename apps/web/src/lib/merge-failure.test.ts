import { describe, expect, it } from "vitest"
import { TRPCClientError } from "@trpc/client"
import { MERGE_ANYWAY_LABEL, mergeFailure } from "@/lib/merge-failure"
import { OFFLINE_ERROR_MESSAGE } from "@/lib/trpc-error"

function serverError(code: string, message: string): TRPCClientError<never> {
  const error = new TRPCClientError(message)
  ;(error as { data?: unknown }).data = { code }
  return error as TRPCClientError<never>
}

describe(`mergeFailure`, () => {
  it(`flags the check gate's PRECONDITION_REQUIRED`, () => {
    const message = `Checks are still running on PR #12: "gate". Sent to the thread.`
    expect(mergeFailure(serverError(`PRECONDITION_REQUIRED`, message), `fallback`)).toEqual({
      message,
      checks: true,
    })
    expect(MERGE_ANYWAY_LABEL).toBe(`Merge anyway`)
  })

  it(`a conflict is a refusal like any other (EXP-1391)`, () => {
    const message = `Pull Request has merge conflicts with 'master'. Sent to the thread.`
    expect(mergeFailure(serverError(`PRECONDITION_FAILED`, message), `fallback`)).toEqual({
      message,
      checks: false,
    })
    expect(mergeFailure(serverError(`CONFLICT`, message), `fallback`).checks).toBe(false)
  })

  it(`falls back for a non-tRPC error`, () => {
    expect(mergeFailure(new Error(`boom`), `The pull request could not be merged`).checks).toBe(false)
  })

  it(`keeps the offline message`, () => {
    const failure = mergeFailure(new TypeError(`Failed to fetch`), `fallback`)
    expect([OFFLINE_ERROR_MESSAGE, `fallback`, `Failed to fetch`]).toContain(failure.message)
  })
})
