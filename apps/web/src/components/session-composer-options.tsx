import { useCallback, useEffect, useState } from "react"
import { agentSupportsPlanMode } from "@/lib/coding-launch-prefs"
import { CLI_DEFAULT_EFFORT } from "@/components/launch-dialog/launch-options-pane"
import type { ComposerOptionsBinding } from "@/components/launch-dialog/composer-plus-menu"
import { useMcpServers } from "@/hooks/use-mcp-servers"
import { useTeamById } from "@/hooks/use-team-data"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { sessionCollection } from "@/lib/collections"
import type { SessionRow } from "@/lib/session-tree"
import { trpc } from "@/lib/trpc-client"

// EXP-1334: a live session's "+" options + its Plan switch, changeable
// MID-SESSION through `sessions.configure` (the row moves, the host device
// applies it). The synced row is the read; a pick shows at once (optimistic)
// and stays until the row confirms it. A DISPATCHER's picks are its WORK
// picks (server-only `work_launch`, inherited by the children it spawns), so
// they never come back on the synced row: the local pick is the read there.
// The MCP pick is server-only for every session: seeded once from
// `sessions.mcpPicks` (a dispatcher's work picks never read back: empty).

interface Overrides {
  planMode?: boolean
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
  const dispatch = row?.dispatch === true
  const mcp = useMcpServers(teamId, !ended)
  useEffect(() => {
    if (ended || dispatch) return
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
  }, [sessionId, ended, dispatch])
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
        // The synced row now says it (a dispatcher's never will). A slow
        // sync keeps the pick in place: the row effect below drops it once
        // the row lands, so the switch never flips back and forward.
        if (!dispatch && synced) setOver((current) => dropSettled(current, picks))
      } catch (error) {
        setOver((current) => dropSettled(current, picks))
        if (nextMcp) setMcpServerIds(previousMcp)
        failedAction(`change`, `the session options`, error)
      }
    },
    [sessionId, dispatch, mcpServerIds]
  )

  // A sync past the wait still settles its pick: whenever the row reads what
  // an override says, the override goes (a dispatcher's row never reflects
  // its work picks, so its overrides stay the read).
  const rowPlanMode = row?.planMode
  const rowEffort = row?.effort
  const rowUltracode = row?.ultracode
  const rowComputerUse = row?.computerUse
  useEffect(() => {
    if (dispatch || !row) return
    setOver((current) =>
      dropSettled(current, {
        planMode: rowPlanMode ?? false,
        effort: rowEffort ?? null,
        ultracode: rowUltracode ?? false,
        computerUse: rowComputerUse ?? false,
      })
    )
    // The row's four option fields are the deps (the row object moves on
    // every sync).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dispatch, row === null, rowPlanMode, rowEffort, rowUltracode, rowComputerUse])

  const effort = over.effort !== undefined ? over.effort : (row?.effort ?? null)
  const ultracode = over.ultracode ?? row?.ultracode ?? false
  const computerUse = over.computerUse ?? row?.computerUse ?? false
  const planMode = over.planMode ?? row?.planMode ?? false

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

  return {
    binding,
    ended,
    /** The row's own plan mode, the pick over it until the row says so. */
    planMode,
    /** True while a Plan pick waits for its row (the read-out leads). */
    planPending: over.planMode !== undefined,
    planSupported: agentSupportsPlanMode(agent),
    setPlanMode: (value: boolean) => void configure({ planMode: value }),
  }
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
