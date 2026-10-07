import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { PR_BODY_MAX } from "@/lib/pr-body-from-results"
import {
  GitHubMergeError,
  updatePullRequest,
} from "@/lib/integrations/github-pr"

// EXP-1139: rewriting a pull request's title/body, shared by the issue path
// (`issues.updatePr`: the Reviews card's Edit, the MCP `exponential_pr_update`
// issue subjects) and the chore path (`repositories.updatePull`: an issue-less
// PR named by `repositoryId + prNumber`). The routers own their guards
// (membership, open-state, repo-from-prUrl, the installation link-gate) and
// hand the resolved repo + token here; this maps GitHub's refusal onto the
// same error vocabulary `retargetPr`/`closePr` use.
//
// EXP-1154: a run's OWN PR body is its Results report (`pr-body-from-results.ts`,
// re-synced by `run-pr-body.ts` on every report text write); MCP `pr_update`
// drops `body` for such a run. A person's edit through `issues.updatePr`
// still lands and holds until the run's next report write.

/** The `pr_open` limits, so an update can never be refused for a size the
 *  open accepted. */
export const PR_TITLE_MAX = 255
export { PR_BODY_MAX }

export const prUpdateFields = {
  title: z.string().trim().min(1).max(PR_TITLE_MAX).optional(),
  body: z.string().max(PR_BODY_MAX).optional(),
}

export type PrUpdateFields = {
  title?: string
  body?: string
}

/** Nothing to change is a caller mistake, not a no-op: GitHub would accept an
 *  empty PATCH and the agent would believe the description changed. */
export function assertPrUpdateHasFields(fields: PrUpdateFields): void {
  if (fields.title === undefined && fields.body === undefined) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Pass a title, a body, or both.`,
    })
  }
}

export async function patchPullDescription(opts: {
  repoFullName: string
  prNumber: number
  token: string
  fields: PrUpdateFields
}): Promise<void> {
  try {
    await updatePullRequest({
      repo: opts.repoFullName,
      prNumber: opts.prNumber,
      title: opts.fields.title,
      body: opts.fields.body,
      token: opts.token,
    })
  } catch (err) {
    if (err instanceof GitHubMergeError) {
      if (err.status === 404) {
        throw new TRPCError({
          code: `NOT_FOUND`,
          message: `Pull request not found on GitHub`,
        })
      }
      if (err.status === 422) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `GitHub refused the update: ${err.message}`,
        })
      }
      throw new TRPCError({
        code: `INTERNAL_SERVER_ERROR`,
        // FEED-74: the status is the first diagnosis; the message is GitHub's
        // own (never empty — `empty response body (request …)` on a bare 500).
        message: `GitHub update failed (HTTP ${err.status}): ${err.message}`,
      })
    }
    throw err
  }
}
