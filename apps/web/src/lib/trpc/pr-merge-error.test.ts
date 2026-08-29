import { describe, expect, it } from "vitest"
import { GitHubMergeError } from "@/lib/integrations/github-pr"
import { isNotMergeable, prMergeFailureError } from "@/lib/trpc/pr-merge-error"

// EXP-533: the whole mapping table. The error CODE is the contract every
// client gates its "Fix conflicts" recovery run on, so each row is pinned.

describe(`isNotMergeable`, () => {
  it(`matches only GitHub's 405 "not mergeable"`, () => {
    expect(
      isNotMergeable(new GitHubMergeError(405, `Pull Request is not mergeable`))
    ).toBe(true)
    // Case-insensitive: GitHub has shipped both casings.
    expect(
      isNotMergeable(new GitHubMergeError(405, `pull request is NOT MERGEABLE`))
    ).toBe(true)
    expect(
      isNotMergeable(
        new GitHubMergeError(405, `Squash merges are not allowed on this repository`)
      )
    ).toBe(false)
    expect(
      isNotMergeable(new GitHubMergeError(409, `Pull Request is not mergeable`))
    ).toBe(false)
    expect(isNotMergeable(new Error(`Pull Request is not mergeable`))).toBe(false)
  })
})

describe(`prMergeFailureError`, () => {
  const notMergeable = new GitHubMergeError(405, `Pull Request is not mergeable`)

  it(`answers CONFLICT with the diagnosis message for a real content conflict`, () => {
    const error = prMergeFailureError(notMergeable, {
      conflict: true,
      message: `Pull Request has merge conflicts with 'master': rebase onto origin/master, resolve the conflicts, push with --force-with-lease, then retry the merge.`,
    })
    expect(error.code).toBe(`CONFLICT`)
    expect(error.message).toContain(`has merge conflicts with`)
  })

  it(`answers PRECONDITION_FAILED for a diagnosed dead base`, () => {
    const error = prMergeFailureError(notMergeable, {
      conflict: false,
      message: `Pull Request is not mergeable: its base branch 'exp/EXP-314' no longer exists.`,
    })
    expect(error.code).toBe(`PRECONDITION_FAILED`)
    expect(error.message).toContain(`no longer exists`)
  })

  it(`keeps today's offer when the diagnosis could not run`, () => {
    const error = prMergeFailureError(notMergeable, null)
    expect(error.code).toBe(`CONFLICT`)
    expect(error.message).toBe(`Pull Request is not mergeable`)
  })

  it(`passes a 405 policy refusal through verbatim as PRECONDITION_FAILED`, () => {
    const error = prMergeFailureError(
      new GitHubMergeError(405, `Squash merges are not allowed on this repository`),
      null
    )
    expect(error.code).toBe(`PRECONDITION_FAILED`)
    expect(error.message).toBe(`Squash merges are not allowed on this repository`)
  })

  it(`inverts GitHub's 409: a moved head branch is not a content conflict`, () => {
    const error = prMergeFailureError(
      new GitHubMergeError(409, `Head branch was modified.`),
      null
    )
    expect(error.code).toBe(`PRECONDITION_FAILED`)
    expect(error.message).toBe(`Head branch changed on GitHub. Refresh and try again.`)
  })

  it(`maps 404 onto NOT_FOUND`, () => {
    const error = prMergeFailureError(new GitHubMergeError(404, `Not Found`), null)
    expect(error.code).toBe(`NOT_FOUND`)
    expect(error.message).toBe(`Pull request not found on GitHub`)
  })

  it(`wraps anything else as INTERNAL_SERVER_ERROR`, () => {
    const error = prMergeFailureError(
      new GitHubMergeError(500, `Server Error`),
      null
    )
    expect(error.code).toBe(`INTERNAL_SERVER_ERROR`)
    expect(error.message).toBe(`GitHub merge failed: Server Error`)
  })
})
