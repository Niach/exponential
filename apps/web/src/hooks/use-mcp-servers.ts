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
import type { McpServerList } from "@/lib/mcp-servers"

export interface McpServersQuery {
  /** null until the first fetch answers. */
  servers: McpServerList | null
  error: string | null
  refresh: () => Promise<void>
}

export function useMcpServers(
  teamId: string | undefined,
  enabled = true
): McpServersQuery {
  const [servers, setServers] = useState<McpServerList | null>(null)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    if (!teamId || !enabled) return
    try {
      setServers(await trpc.mcpServers.list.query({ teamId }))
      setError(null)
    } catch (err) {
      setError(trpcErrorMessage(err, `Couldn't load the MCP servers.`))
    }
  }, [teamId, enabled])

  useEffect(() => {
    if (!teamId || !enabled) {
      setServers(null)
      return
    }
    let active = true
    const load = () => {
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

  return { servers, error, refresh }
}
