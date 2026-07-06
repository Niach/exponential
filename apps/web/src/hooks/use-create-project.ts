import { useCallback } from "react"
import { TRPCClientError } from "@trpc/client"
import { trpc } from "@/lib/trpc-client"
import { isPlanLimitError } from "@/lib/plan-limit-error"
import { invalidateBillingCache } from "@/hooks/use-billing"

// The backing repository for a new project: either an existing registry repo
// (by id) or a brand-new one connected inline by projects.create in the same
// transaction.
export type CreateProjectRepository =
  | { repositoryId: string }
  | {
      fullName: string
      defaultBranch: string
      private: boolean
    }

export type CreateProjectInput = {
  workspaceId: string
  name: string
  prefix: string
  color: string
  repository: CreateProjectRepository
}

// Discriminated failure so each surface can render its own affordance:
//   planLimit  → plan-cap nudge / upgrade dialog
//   noGithubApp → PRECONDITION_FAILED (no GitHub App configured on this server)
//   error      → anything else
export type CreateProjectError =
  | { kind: `planLimit`; message: string }
  | { kind: `noGithubApp`; message: string }
  | { kind: `error`; message: string }

export type CreateProjectResult =
  | { ok: true }
  | { ok: false; error: CreateProjectError }

function mapError(err: unknown): CreateProjectError {
  const message = err instanceof Error ? err.message : String(err)
  if (isPlanLimitError(err)) return { kind: `planLimit`, message }
  if (err instanceof TRPCClientError && err.data?.code === `PRECONDITION_FAILED`)
    return {
      kind: `noGithubApp`,
      message: `This instance has no GitHub App configured`,
    }
  return { kind: `error`, message }
}

// Shared create-project mutation used by the onboarding wizard and the
// create-project dialog. Owns payload assembly (name/prefix trimming +
// repository), the projects.create call, billing-cache invalidation, and the
// error mapping. Callers keep their own layouts and map the returned error
// kind to their UI.
export function useCreateProject() {
  const createProject = useCallback(
    async (input: CreateProjectInput): Promise<CreateProjectResult> => {
      try {
        await trpc.projects.create.mutate(
          {
            workspaceId: input.workspaceId,
            name: input.name.trim(),
            prefix: input.prefix.trim(),
            color: input.color,
            repository: input.repository,
          },
          // Failures render inline at the call site; the global mutation-error
          // toast would be redundant noise.
          { context: { skipErrorToast: true } }
        )
        invalidateBillingCache()
        return { ok: true }
      } catch (err) {
        return { ok: false, error: mapError(err) }
      }
    },
    []
  )

  return { createProject }
}
