// EXP-792: the team's MCP servers over tRPC (`mcpServers.list`, server-only:
// never a shape, so this is the repositories-section pattern — fetch on
// mount, `refresh()` after a write). Every launch surface and the settings
// page read the same list shape; the caller's own `connection` rides each
// row. A connect usually finishes in ANOTHER tab (the launch picker opens
// the settings deep link there), so the list also refetches when this tab
// becomes visible again.
import { useCallback, useEffect, useState } from "react"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import { toast } from "@exp/ui"
import type { McpServerList } from "@/lib/mcp-servers"

export interface McpServersQuery {
  /** null until the first fetch answers. */
  servers: McpServerList | null
  /** EXP-1274: a refetch is running over a list that already answered (after
   * a write, or on return to the tab) — the section band spins its glyph;
   * the rows stay. Never true before the first answer. */
  reloading: boolean
  error: string | null
  refresh: () => Promise<void>
  /** FEED-73: share (or stop sharing) the caller's OWN connection to a server
   * with the team, like a shared device. Toasts a failure, refetches the
   * list either way; resolves true on success. */
  setShared: (serverId: string, shared: boolean) => Promise<boolean>
}

export function useMcpServers(
  teamId: string | undefined,
  enabled = true
): McpServersQuery {
  const [servers, setServers] = useState<McpServerList | null>(null)
  const [reloading, setReloading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    if (!teamId || !enabled) return
    setReloading(true)
    try {
      setServers(await trpc.mcpServers.list.query({ teamId }))
      setError(null)
    } catch (err) {
      setError(trpcErrorMessage(err, `Couldn't load the MCP servers.`))
    } finally {
      setReloading(false)
    }
  }, [teamId, enabled])

  useEffect(() => {
    if (!teamId || !enabled) {
      setServers(null)
      setReloading(false)
      return
    }
    let active = true
    let first = true
    const load = () => {
      if (!first) setReloading(true)
      first = false
      trpc.mcpServers.list
        .query({ teamId })
        .then((rows) => {
          if (!active) return
          setServers(rows)
          setError(null)
        })
        .catch((err) => {
          if (!active) return
          setError(trpcErrorMessage(err, `Couldn't load the MCP servers.`))
        })
        .finally(() => {
          if (active) setReloading(false)
        })
    }
    load()
    const onVisible = () => {
      if (document.visibilityState === `visible`) load()
    }
    document.addEventListener(`visibilitychange`, onVisible)
    return () => {
      active = false
      document.removeEventListener(`visibilitychange`, onVisible)
    }
  }, [teamId, enabled])

  const setShared = useCallback(
    async (serverId: string, shared: boolean) => {
      try {
        await trpc.mcpServers.setShared.mutate(
          { serverId, shared },
          { context: { skipErrorToast: true } }
        )
        return true
      } catch (err) {
        toast.error(shared ? `Could not share` : `Could not stop sharing`, {
          description: trpcErrorMessage(err, `Try again.`),
        })
        return false
      } finally {
        await refresh()
      }
    },
    [refresh]
  )

  return { servers, reloading, error, refresh, setShared }
}
