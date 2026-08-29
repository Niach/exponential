import { describe, expect, it } from "vitest"
import { TRPCClientError } from "@trpc/client"
import { mergeFailure } from "@/lib/merge-failure"
import { OFFLINE_ERROR_MESSAGE } from "@/lib/trpc-error"

function serverError(code: string, message: string): TRPCClientError<never> {
  const error = new TRPCClientError(message)
  ;(error as { data?: unknown }).data = { code }
  return error as TRPCClientError<never>
}

const CONFLICT_MESSAGE = `Pull Request has merge conflicts with 'master': rebase onto origin/master, resolve the conflicts, push with --force-with-lease, then retry the merge.`

describe(`mergeFailure`, () => {
  it(`flags a real conflict from the server's CONFLICT code`, () => {
    expect(
      mergeFailure(serverError(`CONFLICT`, CONFLICT_MESSAGE), `fallback`)
    ).toEqual({ message: CONFLICT_MESSAGE, conflict: true })
  })

  it(`does not flag a stale base as a conflict`, () => {
    const message = `Pull Request is not mergeable: its base branch 'exp/EXP-314' is the head of already-merged PR #240. Retarget this PR to 'master' (call exponential_pr_retarget), rebase onto origin/master if needed, then retry the merge.`
    expect(
      mergeFailure(serverError(`PRECONDITION_FAILED`, message), `fallback`)
    ).toEqual({ message, conflict: false })
  })

  it(`still recognises a pre-EXP-533 server's 412 conflict message`, () => {
    // TRANSITIONAL: a self-host pinned to an older tag answers the same
    // diagnosis with PRECONDITION_FAILED. Auto-updating clients must keep the
    // recovery button until that server updates.
    expect(
      mergeFailure(serverError(`PRECONDITION_FAILED`, CONFLICT_MESSAGE), `fallback`)
    ).toEqual({ message: CONFLICT_MESSAGE, conflict: true })
  })

  it(`never offers the recovery run for a policy refusal`, () => {
    const message = `Squash merges are not allowed on this repository`
    expect(
      mergeFailure(serverError(`PRECONDITION_FAILED`, message), `fallback`)
    ).toEqual({ message, conflict: false })
  })

  it(`reads an unreachable server as offline, never as a conflict`, () => {
    const error = new TRPCClientError(`Failed to fetch`, {
      cause: new TypeError(`Failed to fetch`),
    })
    expect(mergeFailure(error, `The pull request could not be merged`)).toEqual({
      message: OFFLINE_ERROR_MESSAGE,
      conflict: false,
    })
  })

  it(`falls back for anything unrecognisable`, () => {
    expect(mergeFailure(new Error(`boom`), `The pull request could not be merged`)).toEqual(
      { message: `The pull request could not be merged`, conflict: false }
    )
  })
})
