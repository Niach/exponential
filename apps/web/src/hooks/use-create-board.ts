import { useCallback } from "react"
import type { BoardIcon } from "@exp/db-schema/domain"
import { TRPCClientError } from "@trpc/client"
import { trpc } from "@/lib/trpc-client"
import { isPlanLimitError } from "@/lib/plan-limit-error"
import { invalidateBillingCache } from "@/hooks/use-billing"

// The backing repository for a new board: either an existing registry repo
// (by id) or a brand-new one connected inline by boards.create in the same
// transaction.
export type CreateBoardRepository =
  | { repositoryId: string }
  | {
      fullName: string
      defaultBranch: string
      private: boolean
    }

export type CreateBoardInput = {
  teamId: string
  name: string
  prefix: string
  color: string
  // Curated icon name from the domain contract.
  icon: BoardIcon
  repository?: CreateBoardRepository
}

// Discriminated failure so each surface can render its own affordance:
//   planLimit  → plan-cap nudge / upgrade dialog
//   noGithubApp → PRECONDITION_FAILED (no GitHub App configured on this server)
//   error      → anything else
export type CreateBoardError =
  | { kind: `planLimit`; message: string }
  | { kind: `noGithubApp`; message: string }
  | { kind: `error`; message: string }

export type CreateBoardResult =
  | { ok: true }
  | { ok: false; error: CreateBoardError }

function mapError(err: unknown): CreateBoardError {
  const message = err instanceof Error ? err.message : String(err)
  if (isPlanLimitError(err)) return { kind: `planLimit`, message }
  if (err instanceof TRPCClientError && err.data?.code === `PRECONDITION_FAILED`)
    return {
      kind: `noGithubApp`,
      message: `This instance has no GitHub App configured`,
    }
  return { kind: `error`, message }
}

// Shared create-board mutation used by the onboarding wizard and the
// create-board dialog. Owns payload assembly (name/prefix trimming +
// repository), the boards.create call, billing-cache invalidation, and the
// error mapping. Callers keep their own layouts and map the returned error
// kind to their UI.
export function useCreateBoard() {
  const createBoard = useCallback(
    async (input: CreateBoardInput): Promise<CreateBoardResult> => {
      try {
        await trpc.boards.create.mutate(
          {
            teamId: input.teamId,
            name: input.name.trim(),
            prefix: input.prefix.trim(),
            color: input.color,
            icon: input.icon,
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

  return { createBoard }
}
