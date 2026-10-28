import { useCallback, useEffect, useState } from "react"
import { CLI_DEFAULT_EFFORT } from "@/components/launch-dialog/launch-options-pane"
import type { ComposerOptionsBinding } from "@/components/launch-dialog/composer-plus-menu"
import { useMcpServers } from "@/hooks/use-mcp-servers"
import { useTeamById } from "@/hooks/use-team-data"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { sessionCollection } from "@/lib/collections"
import type { SessionRow } from "@/lib/session-tree"
import { trpc } from "@/lib/trpc-client"

// EXP-1334: a live session's "+" options (S-3: no Plan switch), changeable
// MID-SESSION through `sessions.configure` (the row moves, the host device
// applies it). The synced row is the read; a pick shows at once (optimistic)
// and stays until the row confirms it. The MCP pick is server-only for every
// session: seeded once from `sessions.mcpPicks`.

interface Overrides {
  effort?: string | null
  ultracode?: boolean
  computerUse?: boolean
}

type Patch = Overrides & { mcpServerIds?: string[] }

/** How long a configure waits (bounded) for its row to sync. */
const CONFIGURE_SYNC_WAIT_MS = 3_000

export function useSessionComposerOptions({
  teamId,
  sessionId,
  row,
  agent,
}: {
  teamId: string
  sessionId: string
  row: SessionRow | null
  agent: string
}) {
  const [over, setOver] = useState<Overrides>({})
  const [mcpServerIds, setMcpServerIds] = useState<string[]>([])
  const ended = row === null || row.status === `ended`
  const mcp = useMcpServers(teamId, !ended)
  useEffect(() => {
    if (ended) return
    let live = true
    trpc.sessions.mcpPicks
      .query({ sessionId }, SKIP_ERROR_TOAST)
      .then((picks) => {
        if (live) setMcpServerIds(picks.mcpServerIds)
      })
      .catch(() => undefined)
    return () => {
      live = false
    }
  }, [sessionId, ended])
  const team = useTeamById(teamId)
  const teamSlug = team?.slug ?? null

  const configure = useCallback(
    async (patch: Patch) => {
      const { mcpServerIds: nextMcp, ...picks } = patch
      const previousMcp = mcpServerIds
      setOver((current) => ({ ...current, ...picks }))
      if (nextMcp) setMcpServerIds(nextMcp)
      try {
        const { txid } = await trpc.sessions.configure.mutate(
          { sessionId, ...patch },
          SKIP_ERROR_TOAST
        )
        const synced = await sessionCollection.utils
          .awaitTxId(txid, CONFIGURE_SYNC_WAIT_MS)
          .then(
            () => true,
            () => false
          )
        // The synced row now says it. A slow sync keeps the pick in place:
        // the row effect below drops it once the row lands, so the switch
        // never flips back and forward.
        if (synced) setOver((current) => dropSettled(current, picks))
      } catch (error) {
        setOver((current) => dropSettled(current, picks))
        if (nextMcp) setMcpServerIds(previousMcp)
        failedAction(`change`, `the session options`, error)
      }
    },
    [sessionId, mcpServerIds]
  )

  // A sync past the wait still settles its pick: whenever the row reads what
  // an override says, the override goes.
  const rowEffort = row?.effort
  const rowUltracode = row?.ultracode
  const rowComputerUse = row?.computerUse
  useEffect(() => {
    if (!row) return
    setOver((current) =>
      dropSettled(current, {
        effort: rowEffort ?? null,
        ultracode: rowUltracode ?? false,
        computerUse: rowComputerUse ?? false,
      })
    )
    // The row's three option fields are the deps (the row object moves on
    // every sync).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [row === null, rowEffort, rowUltracode, rowComputerUse])

  const effort = over.effort !== undefined ? over.effort : (row?.effort ?? null)
  const ultracode = over.ultracode ?? row?.ultracode ?? false
  const computerUse = over.computerUse ?? row?.computerUse ?? false

  const binding: ComposerOptionsBinding = {
    agent,
    effortValue: effort ?? ``,
    setEffortValue: (value) =>
      void configure({ effort: value === CLI_DEFAULT_EFFORT || value === `` ? null : value }),
    ultracode,
    setUltracode: (value) => void configure({ ultracode: value }),
    computerUse,
    setComputerUse: (value) => void configure({ computerUse: value }),
    computerUseAvailable: Boolean(row?.deviceId),
    mcpServers: mcp.servers ?? [],
    mcpServerIds,
    toggleMcpServer: (id) =>
      void configure({
        mcpServerIds: mcpServerIds.includes(id)
          ? mcpServerIds.filter((current) => current !== id)
          : [...mcpServerIds, id],
      }),
    mcpConnectHref: (serverId) =>
      teamSlug
        ? `/t/${encodeURIComponent(teamSlug)}/settings/mcp-servers?connect=${encodeURIComponent(serverId)}`
        : null,
  }

  return { binding, ended }
}

/** Drops the overrides `picks` set, unless a later pick replaced them. */
function dropSettled(current: Overrides, picks: Overrides): Overrides {
  const next = { ...current }
  let dropped = false
  for (const key of Object.keys(picks) as (keyof Overrides)[]) {
    if (key in next && next[key] === picks[key]) {
      delete next[key]
      dropped = true
    }
  }
  // The same object when nothing settled (no re-render from the row effect).
  return dropped ? next : current
}
