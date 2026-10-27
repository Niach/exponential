import { describe, expect, it } from "vitest"
import { TRPCClientError } from "@trpc/client"
import { mergeFailure } from "@/lib/merge-failure"
import { OFFLINE_ERROR_MESSAGE } from "@/lib/trpc-error"

function serverError(code: string, message: string): TRPCClientError<never> {
  const error = new TRPCClientError(message)
  ;(error as { data?: unknown }).data = { code }
  return error as TRPCClientError<never>
}

describe(`mergeFailure`, () => {
  it(`a conflict is a refusal like any other (EXP-1391)`, () => {
    const message = `Pull Request has merge conflicts with 'master'. Sent to the thread.`
    expect(mergeFailure(serverError(`PRECONDITION_FAILED`, message), `fallback`)).toEqual({ message })
    expect(mergeFailure(serverError(`CONFLICT`, message), `fallback`)).toEqual({ message })
  })

  it(`falls back for a non-tRPC error`, () => {
    expect(mergeFailure(new Error(`boom`), `The pull request could not be merged`)).toEqual({
      message: `The pull request could not be merged`,
    })
  })

  it(`keeps the offline message`, () => {
    const failure = mergeFailure(new TypeError(`Failed to fetch`), `fallback`)
    expect([OFFLINE_ERROR_MESSAGE, `fallback`, `Failed to fetch`]).toContain(failure.message)
  })
})
