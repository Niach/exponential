import { createElement, useCallback, useMemo, useState } from "react"
import { getDeviceIcon, suggestionKey, type ThreadSuggestionActions } from "@exp/ui"
import { modelLabel } from "@/components/launch-dialog/launch-options-pane"
import { useSession } from "@/hooks/use-session"
import { useSteerDevices } from "@/hooks/use-steer-devices"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { agentModelValues } from "@/lib/coding-launch-prefs"
import { trpc } from "@/lib/trpc-client"

// EXP-1323 (#EXP-1328): THE suggested tasks card's actions on web, one tRPC
// call each (`sessions.startSuggestion` child|here, `startAllSuggestions`,
// `dismissSuggestion`, `fileSuggestion`); the synced message row repaints
// the card. A failed tap toasts `Couldn't <verb> <thing>`.

export function useSuggestionActions(input: {
  teamId: string
  /** Members only; absent = the card is read-only. */
  enabled: boolean
}): ThreadSuggestionActions | undefined {
  const { teamId, enabled } = input
  const { data: session } = useSession()
  const { devices } = useSteerDevices({
    enabled,
    currentUserId: session?.user?.id,
    teamId,
  })
  const [pending, setPending] = useState<ReadonlySet<string>>(() => new Set())

  const run = useCallback(
    async (key: string, call: () => Promise<unknown>, verb: string, thing: string) => {
      setPending((current) => new Set(current).add(key))
      try {
        await call()
      } catch (error) {
        failedAction(verb, thing, error)
      } finally {
        setPending((current) => {
          const next = new Set(current)
          next.delete(key)
          return next
        })
      }
    },
    []
  )

  const deviceRows = useMemo(
    () =>
      (devices ?? [])
        .filter((device) => device.online !== false)
        .map((device) => ({
          id: device.deviceId,
          name: device.deviceLabel,
          icon: createElement(getDeviceIcon(device), { className: `size-4` }),
        })),
    [devices]
  )
  // The ONE model vocabulary (contract `codingModel`), never a default row.
  const models = useMemo(
    () => agentModelValues(`claude`).map((value) => ({ value, label: modelLabel(value) })),
    []
  )

  return useMemo(() => {
    if (!enabled) return undefined
    return {
      onStart: (messageId, itemId, options) =>
        void run(
          suggestionKey(messageId, itemId),
          () =>
            trpc.sessions.startSuggestion.mutate(
              { messageId, itemId, where: `child`, ...options },
              SKIP_ERROR_TOAST
            ),
          `start`,
          `task`
        ),
      onStartAll: (messageId) =>
        void run(
          suggestionKey(messageId),
          () => trpc.sessions.startAllSuggestions.mutate({ messageId }, SKIP_ERROR_TOAST),
          `start`,
          `tasks`
        ),
      onHere: (messageId, itemId) =>
        void run(
          suggestionKey(messageId, itemId),
          () =>
            trpc.sessions.startSuggestion.mutate(
              { messageId, itemId, where: `here` },
              SKIP_ERROR_TOAST
            ),
          `send`,
          `task`
        ),
      onDismiss: (messageId, itemId) =>
        void run(
          suggestionKey(messageId, itemId),
          () =>
            trpc.sessions.dismissSuggestion.mutate({ messageId, itemId }, SKIP_ERROR_TOAST),
          `dismiss`,
          `task`
        ),
      onFile: (messageId, itemId) =>
        void run(
          suggestionKey(messageId, itemId),
          () => trpc.sessions.fileSuggestion.mutate({ messageId, itemId }, SKIP_ERROR_TOAST),
          `file`,
          `task`
        ),
      devices: deviceRows,
      models,
      pending,
    } satisfies ThreadSuggestionActions
  }, [enabled, run, deviceRows, models, pending])
}
