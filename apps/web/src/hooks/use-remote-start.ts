import { useEffect, useMemo, useRef, useState } from "react"
import { toast } from "sonner"
import { trpcErrorMessage } from "@/lib/trpc-error"
import { useLiveQuery } from "@tanstack/react-db"
import type { CodingSession, Device, User } from "@/db/schema"
import {
  codingSessionCollection,
  deviceCollection,
  userCollection,
} from "@/lib/collections"
import { useNow } from "@/hooks/use-now"
import { trpc } from "@/lib/trpc-client"
import { isBuiltinActionId } from "@/lib/builtin-actions"
import type { CodingLaunchPrefs } from "@/lib/coding-launch-prefs"
import { composeDeviceList, type SteerDevice } from "@/lib/steer-devices"
import { useAgentDock } from "@/components/agent-dock/agent-dock-provider"
import {
  findStartedRun,
  startedRunKeyForIssues,
  STARTED_RUN_DEADLINE_MS,
  STARTED_RUN_SKEW_MS,
  type StartedRunKey,
} from "@/lib/started-run-match"

// Remote "Start on my desktop" (EXP-106/EXP-253, merged in EXP-257): the
// caller's machines, then a start command through the relay control socket —
// a single-issue session (`issueId`), a BATCH session (`issueIds`, 2+), or an
// action run (`actionId`). EXP-481: the device list rides the synced
// `devices` shape (no more 15s polling) — online-ness derives from
// `last_seen_at` freshness against a 30s ticking clock, and renames/removes/
// share toggles stream in like any other synced row. Only `latestVersions`
// (instance config, not a shape column) still comes from `devices.list`,
// fetched once per mount.
//
// EXP-536: after ANY send — a single issue, a batch, an action run — the hook
// watches the synced coding_sessions rows for the desktop's run and focuses
// the dock on it once (on mobile web the dock IS the full-viewport session
// takeover, so that reads as "navigate to the session", matching the natives).
// `lib/started-run-match.ts` owns the matching rules; note an action never
// matches on actionId — the builtin "Create action" run is inserted with
// actionId NULL.

/** The resolved launch-dialog choices sent with `steer.startSession` — the
 * same shape the prefs module persists. */
export type StartCodingOptions = CodingLaunchPrefs

/** The minimal action identity a run needs: `teamId` rides the mutation only
 * for the builtin (there is no DB row to derive the team from), and `name`
 * keys the post-send dock watch. */
export interface RemoteStartAction {
  id: string
  name: string
  teamId: string
}

export interface RemoteStart {
  /** The caller's registered machines (online AND offline — EXP-403);
   * null while the first lookup is in flight. */
  devices: SteerDevice[] | null
  starting: boolean
  /** Device label a start was just delivered to — cleared once the run's
   * synced row appears (dock auto-focused) or once the watch deadline passes
   * without one. */
  sentTo: string | null
  /** Resolves on delivery, rejects on failure (toast already shown). */
  startIssues: (
    device: SteerDevice,
    options: StartCodingOptions,
    issueIds: string[]
  ) => Promise<void>
  /** Resolves on delivery, rejects on failure (toast already shown). */
  runAction: (
    device: SteerDevice,
    action: RemoteStartAction,
    options: StartCodingOptions,
    inputs?: Record<string, string>
  ) => Promise<void>
  /** Re-fetch `latestVersions` now (device rows themselves are synced —
   * mutations stream in without any refetch). */
  refresh: () => void
  /** Informational CLIENT_LATEST_VERSION_* values — null until loaded or
   * when unset server-side. */
  latestVersions: { desktop: string | null; cli: string | null } | null
}

export function useRemoteStart({
  enabled = true,
  currentUserId,
  teamId,
}: {
  /** Member + relay configured — gates the device list + version fetch. */
  enabled?: boolean
  /** Keys the action dock watch to the caller's own runs — and, EXP-481,
   * splits own vs shared rows off the synced devices shape. */
  currentUserId?: string
  /** EXP-432: also list teammates' server devices shared with this team. */
  teamId?: string
} = {}): RemoteStart {
  const [latestVersions, setLatestVersions] = useState<{
    desktop: string | null
    cli: string | null
  } | null>(null)
  const [starting, setStarting] = useState(false)
  const [pending, setPending] = useState<{
    deviceLabel: string
    sentAt: number
    /** What the dock watch below is looking for in the synced rows. */
    key: StartedRunKey
  } | null>(null)
  const resetTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const dock = useAgentDock()

  // EXP-481: device rows off the synced shape (already server-scoped to own
  // rows + team-shared servers); user rows resolve shared owners' names.
  const { data: deviceRows } = useLiveQuery(
    (query) => (enabled ? query.from({ d: deviceCollection }) : undefined),
    [enabled]
  )
  const { data: userRows } = useLiveQuery(
    (query) => (enabled ? query.from({ u: userCollection }) : undefined),
    [enabled]
  )
  // 30s tick against the 90s online window (the EXP-153 staleness idiom).
  const now = useNow(30_000)
  const devices = useMemo<SteerDevice[] | null>(() => {
    if (!enabled || !currentUserId || deviceRows === undefined) return null
    const usersById = new Map(
      ((userRows ?? []) as User[]).map((user) => [user.id, user])
    )
    return composeDeviceList(
      deviceRows as Device[],
      usersById,
      now,
      currentUserId,
      teamId
    )
  }, [enabled, currentUserId, deviceRows, userRows, now, teamId])

  // `latestVersions` is instance config, not a shape column — one fetch per
  // mount (the returned device rows are ignored; sync owns those now).
  const [refreshTick, setRefreshTick] = useState(0)
  useEffect(() => {
    if (!enabled) return
    let active = true
    trpc.devices.list
      .query()
      .then((res) => active && setLatestVersions(res.latestVersions ?? null))
      .catch(() => {})
    return () => {
      active = false
    }
  }, [enabled, refreshTick])

  useEffect(
    () => () => {
      if (resetTimerRef.current) clearTimeout(resetTimerRef.current)
    },
    []
  )

  // Watch the synced coding_sessions rows for the desktop picking the start
  // up. The whole (team-scoped, short) collection is read rather than a
  // server-side filter: a BATCH row carries neither an issue id nor an action
  // name, so there is nothing narrower to key a `where` on.
  const watching = pending !== null
  const { data: sessionRows } = useLiveQuery(
    (query) =>
      watching ? query.from({ s: codingSessionCollection }) : undefined,
    [watching]
  )

  useEffect(() => {
    if (!pending || !currentUserId) return
    const match = findStartedRun(
      (sessionRows ?? []) as CodingSession[],
      pending.key,
      currentUserId,
      pending.sentAt - STARTED_RUN_SKEW_MS
    )
    if (!match) return
    // Focus the dock exactly once — clearing `pending` stops this effect from
    // ever matching again for this send.
    dock?.openDock(match.id)
    if (resetTimerRef.current) clearTimeout(resetTimerRef.current)
    setPending(null)
  }, [sessionRows, pending, currentUserId, dock])

  // The desktop inserts the coding_sessions row when the launcher spins up.
  // The deadline is not a silent re-enable: a run the desktop REFUSED — a
  // conflicted worktree, a failed doctor — would otherwise be indistinguishable
  // from one still starting, the caption would just vanish and no session would
  // ever open. The desktop holds the reason (it notifies there); say so.
  const markSent = (deviceLabel: string, key: StartedRunKey) => {
    if (resetTimerRef.current) clearTimeout(resetTimerRef.current)
    setPending({ deviceLabel, sentAt: Date.now(), key })
    resetTimerRef.current = setTimeout(() => {
      setPending(null)
      toast.error(`${deviceLabel} never started this run`, {
        description: `Open the Exponential desktop app there to see why.`,
      })
    }, STARTED_RUN_DEADLINE_MS)
  }

  const startIssues = async (
    device: SteerDevice,
    options: StartCodingOptions,
    issueIds: string[]
  ) => {
    const key = startedRunKeyForIssues(issueIds)
    if (!key) return
    setStarting(true)
    try {
      // EXP-481: `resume` rides SINGLE-issue starts only (a batch has no
      // per-issue worktree; the server rejects it there).
      const { resume, ...rest } = options
      const base = { deviceId: device.deviceId, ...rest }
      await trpc.steer.startSession.mutate(
        // 1 issue → plain single-issue session; 2+ → one batch session on a
        // single pushed branch (the server contract owns the fan-out).
        issueIds.length === 1
          ? { issueId: issueIds[0], ...base, ...(resume ? { resume } : {}) }
          : { issueIds, ...base },
        { context: { skipErrorToast: true } }
      )
      markSent(device.deviceLabel, key)
    } catch (error) {
      toast.error(`Couldn't start on your desktop`, {
        description: trpcErrorMessage(
          error,
          `The start command could not be delivered`
        ),
      })
      throw error
    } finally {
      setStarting(false)
    }
  }

  const runAction = async (
    device: SteerDevice,
    action: RemoteStartAction,
    options: StartCodingOptions,
    inputs?: Record<string, string>
  ) => {
    setStarting(true)
    try {
      // Action runs never resume an issue worktree (EXP-481).
      const { resume: _resume, ...rest } = options
      await trpc.steer.startSession.mutate(
        {
          actionId: action.id,
          deviceId: device.deviceId,
          // teamId is required iff the action is the virtual builtin (no DB
          // row to derive the team from) and forbidden otherwise.
          ...(isBuiltinActionId(action.id) ? { teamId: action.teamId } : {}),
          ...(inputs ? { inputs } : {}),
          ...rest,
        },
        { context: { skipErrorToast: true } }
      )
      markSent(device.deviceLabel, { kind: `action`, actionName: action.name })
    } catch (error) {
      toast.error(`Couldn't run the action on your desktop`, {
        description: trpcErrorMessage(
          error,
          `The start command could not be delivered`
        ),
      })
      throw error
    } finally {
      setStarting(false)
    }
  }

  return {
    devices,
    starting,
    sentTo: pending?.deviceLabel ?? null,
    startIssues,
    runAction,
    refresh: () => setRefreshTick((tick) => tick + 1),
    latestVersions,
  }
}
