// EXP-891: the per-device MCP servers of one team over tRPC
// (`deviceMcpServers.list`, server-only: never a shape) — the
// repositories-section pattern, fetch on mount + `refresh()`. Read-only on
// the web: the rows are written by the machines themselves.
import { useCallback, useEffect, useState } from "react"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import type { DeviceMcpServerListRow } from "@/lib/mcp/device-mcp-servers"

export interface DeviceMcpServersQuery {
  /** null until the first fetch answers. */
  rows: DeviceMcpServerListRow[] | null
  error: string | null
  refresh: () => Promise<void>
}

export function useDeviceMcpServers(
  teamId: string | undefined,
  enabled = true
): DeviceMcpServersQuery {
  const [rows, setRows] = useState<DeviceMcpServerListRow[] | null>(null)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    if (!teamId || !enabled) return
    try {
      setRows(await trpc.deviceMcpServers.list.query({ teamId }))
      setError(null)
    } catch (err) {
      setError(trpcErrorMessage(err, `Couldn't load the devices' MCP servers.`))
    }
  }, [teamId, enabled])

  useEffect(() => {
    if (!teamId || !enabled) {
      setRows(null)
      return
    }
    let active = true
    trpc.deviceMcpServers.list
      .query({ teamId })
      .then((result) => {
        if (!active) return
        setRows(result)
        setError(null)
      })
      .catch((err) => {
        if (!active) return
        setError(trpcErrorMessage(err, `Couldn't load the devices' MCP servers.`))
      })
    return () => {
      active = false
    }
  }, [teamId, enabled])

  return { rows, error, refresh }
}
