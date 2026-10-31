import { useCallback, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { appCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"

// VAPP-176 (fixtures/apps.json NEW APP): every New app entry creates a DRAFT
// ("Untitled app", no version) and opens its editor at once: no dialog, no
// form; the editor session names it.

export function useNewApp(teamId: string | undefined, teamSlug: string) {
  const navigate = useNavigate()
  const [pending, setPending] = useState(false)
  const create = useCallback(async () => {
    if (!teamId || pending) return
    setPending(true)
    try {
      const result = await trpc.apps.create.mutate({ teamId }, SKIP_ERROR_TOAST)
      await appCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
      void navigate({ to: `/t/$teamSlug/apps/$appSlug`, params: { teamSlug, appSlug: result.app.slug } })
    } catch (error) {
      failedAction(`create`, `the app`, error)
    } finally {
      setPending(false)
    }
  }, [teamId, teamSlug, pending, navigate])
  return { create, pending }
}
