import { useCallback, useEffect, useState } from "react"
import { trpc } from "@/lib/trpc-client"

export type SetupChecklistStatus = {
  hasProject: boolean
  githubConnected: boolean
  machineRegistered: boolean
  agentSeen: boolean
  repoLinked: boolean
  firstIssueAssignedToAgent: boolean
  dismissed: boolean
  allComplete: boolean
}

// Cross-mutation refresh signal (project create, GitHub connect, agent
// register, issue assign-to-agent all call this), mirroring use-billing.
const listeners = new Set<() => void>()
export function invalidateSetupChecklistCache(): void {
  listeners.forEach((l) => l())
}

function dismissKey(workspaceId: string): string {
  return `setup-checklist-dismissed:${workspaceId}`
}

export function useSetupChecklist(workspaceId: string | undefined): {
  status: SetupChecklistStatus | null
  dismiss: () => void
} {
  const [status, setStatus] = useState<SetupChecklistStatus | null>(null)
  const [fetchKey, setFetchKey] = useState(0)
  // Optimistic local dismissal so the UI hides instantly; the server flag is
  // the durable cross-device record (synced via the mutation below).
  const [localDismissed, setLocalDismissed] = useState(false)

  useEffect(() => {
    const listener = () => setFetchKey((k) => k + 1)
    listeners.add(listener)
    // Refetch when the tab regains focus so progress made elsewhere (GitHub
    // install popup, registering the agent in the desktop app) reflects here.
    window.addEventListener(`focus`, listener)
    return () => {
      listeners.delete(listener)
      window.removeEventListener(`focus`, listener)
    }
  }, [])

  useEffect(() => {
    if (!workspaceId) return
    setLocalDismissed(
      typeof window !== `undefined` &&
        window.localStorage.getItem(dismissKey(workspaceId)) === `1`
    )
  }, [workspaceId])

  useEffect(() => {
    if (!workspaceId) return
    let cancelled = false
    void trpc.agent.setupStatus
      .query({ workspaceId })
      .then((s) => {
        if (cancelled) return
        const allComplete =
          s.hasProject &&
          s.githubConnected &&
          s.machineRegistered &&
          s.firstIssueAssignedToAgent
        setStatus({ ...s, allComplete })
      })
      .catch(() => {
        // Non-owners (setupStatus is owner-gated) simply get no checklist.
        if (!cancelled) setStatus(null)
      })
    return () => {
      cancelled = true
    }
  }, [workspaceId, fetchKey])

  const dismiss = useCallback(() => {
    if (!workspaceId) return
    setLocalDismissed(true)
    if (typeof window !== `undefined`) {
      window.localStorage.setItem(dismissKey(workspaceId), `1`)
    }
    void trpc.onboarding.dismissSetupChecklist.mutate({ dismissed: true })
  }, [workspaceId])

  const effective: SetupChecklistStatus | null = status
    ? { ...status, dismissed: status.dismissed || localDismissed }
    : null

  return { status: effective, dismiss }
}
