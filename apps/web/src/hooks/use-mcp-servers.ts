// EXP-792: the team's MCP servers over tRPC (`mcpServers.list`, server-only:
// never a shape, so this is the repositories-section pattern — fetch on
// mount, `refresh()` after a write or an OAuth round trip). Every launch
// surface and the settings pane read the same list shape; the readiness
// matrix rides each row.
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
    return () => {
      active = false
    }
  }, [teamId, enabled])

  return { servers, error, refresh }
}
