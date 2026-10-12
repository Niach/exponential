import { useEffect, useMemo, useRef, useState } from "react"
import { actionErrorTitle, toast } from "@exp/ui"
import { FEEDBACK_STATES } from "@exp/domain-contract/feedback-mark"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { trpcErrorCode, trpcErrorMessage } from "@/lib/trpc-error"
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
import {
  composeDeviceList,
  deviceCanAgentLogin,
  deviceIsMine,
  type SteerDevice,
} from "@/lib/steer-devices"
import { requestAgentLogin } from "@/components/agent-login-dialog"
import { useOpenStartedSession } from "@/hooks/use-open-started-session"
import type { DetailOrigin } from "@/lib/detail-origin"
import {
  findStartedRun,
  startedRunKeyForIssues,
  STARTED_RUN_DEADLINE_MS,
  STARTED_RUN_SKEW_MS,
  type StartedRunKey,
} from "@/lib/started-run-match"
import { newStartId, startIntentKey } from "@/lib/start-intent"

/** Final review: how long the watch follows a sent start (feedback-states.json
 *  `starting.watchMs`, 30 min, the server's and the devices' dedupe window).
 *  The "never started" toast still fires at `STARTED_RUN_DEADLINE_MS`. */
const WATCH_MS = FEEDBACK_STATES.starting.watchMs

/** One send's watch state, created BEFORE the request goes out so a
 *  `quietWatch` while the mutation is still in flight applies to it. */
interface WatchedSend {
  quiet: boolean
  /** The caller's say on a late landing: false = do not navigate. */
  canOpen: (() => boolean) | null
  /** The draft start's id (none = every send is its own start). */
  startId: string | undefined
  /** `Date.now()` as the FIRST request of this startId went out: the
   *  watch's cutoff (a re-send's device drops it, the original row lands). */
  requestedAt: number
}

/** Final review (B6): a synced row created before the send went out is
 *  never its run (rows carry no startId: an earlier start's late row on the
 *  same subject would otherwise land here). `created_at` is server-stamped;
 *  the cutoff is taken BEFORE the request, so the run's own row is always
 *  younger by the round trip plus the launcher's spin-up. Fails closed on
 *  an unparseable stamp, like `matchesStartedRun`. */
export function createdSince(
  session: { createdAt: Date | string },
  sinceMs: number
): boolean {
  const createdAt = new Date(session.createdAt).getTime()
  return Number.isFinite(createdAt) && createdAt >= sinceMs
}

// Remote "Start on my desktop" (EXP-106/EXP-253, merged in EXP-257): the
// caller's machines, then a start command through the relay control socket —
// a single-issue session (`issueId`), a BATCH session (`issueIds`, 2+), or an
// action run (`actionId`). EXP-481: the device list rides the synced
// `devices` shape (no more 15s polling) — online-ness derives from
// `last_seen_at` freshness against a 30s ticking clock, and renames/removes/
// share toggles stream in like any other synced row. Only `latestVersions`
// (instance config, not a shape column) comes over tRPC, from
// `devices.latestVersions`, fetched once per mount.
//
// EXP-536: after ANY send — a single issue, a batch, an action run — the hook
// watches the synced coding_sessions rows for the desktop's run and opens it
// once (EXP-740: that is a navigation to the run's session page, or to the
// team chat page for a chat run, on every breakpoint — matching the natives).
// `lib/started-run-match.ts` owns the matching rules; note an action never
// matches on actionId — the builtin "Create action" run is inserted with
// actionId NULL.

/** The resolved launch choices sent with `steer.startSession` — the same
 * shape the prefs module persists. EXP-825: the ONE home of the alias (it
 * used to be re-exported by the launch dialog, which is gone). */
export type StartCodingOptions = CodingLaunchPrefs

/** EXP-792 (EXP-747 A2): the failure toast, `Couldn't start the run`
 * (FEED-79), with a "Sign in" action when the refusal is one a remote
 * sign-in can fix — the agent signed out on the machine, not installed
 * there, or any other precondition the desktop refuses on — and the machine
 * can run `agent_login` for the caller: one of their OWN devices with the
 * cap (EXP-849: every agent has a device-code flow). Everything else offers
 * the caller's Retry (nothing started, so starting again is safe). */
function startFailureToast(
  error: unknown,
  device: SteerDevice,
  agent: string,
  retry?: () => void
) {
  const title = actionErrorTitle(`start`, `the session`)
  const description = trpcErrorMessage(
    error,
    `The start command could not be delivered`
  )
  const precondition = trpcErrorCode(error) === `PRECONDITION_FAILED`
  const canSignIn =
    precondition &&
    deviceIsMine(device) &&
    deviceCanAgentLogin(device)
  if (!canSignIn) {
    failedAction(`start`, `the session`, error, { description, retry })
    return
  }
  toast.error(title, {
    description,
    action: {
      label: `Sign in`,
      onClick: () => requestAgentLogin({ device, agent }),
    },
  })
}

/** The minimal action identity a run needs: `teamId` rides the mutation only
 * for the builtin (there is no DB row to derive the team from), and `name`
 * keys the post-send session watch. */
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
   * synced row appears (its session page opens) or once the watch deadline
   * passes without one. */
  sentTo: string | null
  /** Resolves on delivery, rejects on failure (toast already shown; its
   * Retry runs `retry`, the caller's re-submit).
   * EXP-1259: every send carries a `startId`. Absent = this call is a new
   * press-intent and mints one; the failure toast's Retry hands that id to
   * the caller's re-submit of the SAME subject on the SAME device
   * (`startIntentKey`), so the server and the device drop a repeat.
   * EXP-825: `prompt` is the composer's free text (with its image embeds,
   * `buildSteerImageMessage`) — the run's "Additional instructions". */
  startIssues: (
    device: SteerDevice,
    options: StartCodingOptions,
    issueIds: string[],
    prompt?: string,
    retry?: () => void,
    /** Review #3: the draft start's id (Retry and a later Start of the
     *  same draft reuse it); the server and the device drop a repeat. */
    startId?: string
  ) => Promise<void>
  /** Resolves on delivery, rejects on failure (toast already shown; its
   * Retry runs `retry`).
   * EXP-825: `prompt` is REQUIRED for the Chat and Create action builtins
   * (it IS the request) and optional instructions for any other action. */
  runAction: (
    device: SteerDevice,
    action: RemoteStartAction,
    options: StartCodingOptions,
    inputs?: Record<string, string>,
    prompt?: string,
    retry?: () => void,
    startId?: string
  ) => Promise<void>
  /** Final review (B6): called at the TOP of a send (before its uploads).
   *  A DIFFERENT start (another startId, or none) cancels the previous
   *  send's watch: its run, if it lands later, only shows in Running and
   *  never opens or reaches the composer. A re-send of the SAME startId
   *  keeps the watch (the device drops the duplicate; the original row is
   *  the landing). `startIssues`/`runAction` call it too. */
  supersede: (startId?: string) => void
  /** Review #3: the composer's own 30 s starting deadline gave up on the
   * last start and toasted. Nothing is aborted: the watch keeps looking for
   * the run for `starting.watchMs` but its own deadline toast never fires.
   * Final review: `canOpen` = asked when the late row lands; false = the
   * row only counts as landed (`openedId`), nothing navigates. */
  quietWatch: (canOpen?: () => boolean) => void
  /** The id of the last run the watch matched (null until one did): the
   * composer's landing signal, distinct from `sentTo` falling at the
   * watch's deadline. Opened unless a quiet watch's `canOpen` said no. */
  openedId: string | null
  /** Re-fetch `latestVersions` now (device rows themselves are synced —
   * mutations stream in without any refetch). */
  refresh: () => void
  /** Informational CLIENT_LATEST_VERSION_* values — null until loaded or
   * when unset server-side. */
  latestVersions: { desktop: string | null; cli: string | null } | null
}

export function useRemoteStart(
  options: {
    /** Member + relay configured — gates the device list + version fetch. */
    enabled?: boolean
    /** Keys the post-send session watch to the caller's own runs — and,
     * EXP-481, splits own vs shared rows off the synced devices shape. */
    currentUserId?: string
    /** EXP-432: also list teammates' server devices shared with this team. */
    teamId?: string
    /** EXP-1186: list servers shared into ANY of these teams instead (the
     *  phone's Devices tab, cross-team); `teamId` still keys the starts. */
    listTeamIds?: readonly string[]
    /** EXP-870: where the started run OPENS. The key PRESENT (even as `null`
     * = context-free, a pinned row's start) is the caller's answer, handed to
     * `useOpenSession` verbatim; absent = derive it from the URL. */
    origin?: DetailOrigin | null
    /** EXP-1255: called with the started run as the watch opens it (the
     *  New run tab turns into that run's tab). EXP-1284: returning `true`
     *  means the caller opened it itself (the run's THREAD page), so the
     *  hook does not navigate to the run. */
    onOpened?: (
      session: CodingSession,
      start: { startId: string | null }
    ) => boolean | void
    /** EXP-1284: a start was DELIVERED under this start id (the client's,
     *  else the server-minted one) — the New session tab remembers it so the
     *  landing is matched per start. */
    onSent?: (start: { startId: string }) => void
    /** EXP-1284: every start from this hook goes INTO this thread
     *  (`steer.startSession({threadId})` — "Start a session here"). */
    threadId?: string | null
  } = {}
): RemoteStart {
  const { enabled = true, currentUserId, teamId, listTeamIds } = options
  const listKey = listTeamIds?.join(`,`)
  const hasOrigin = `origin` in options
  const origin = options.origin ?? null
  const openOptions = useMemo(
    () => (hasOrigin ? { origin } : undefined),
    [hasOrigin, origin]
  )
  const [latestVersions, setLatestVersions] = useState<{
    desktop: string | null
    cli: string | null
  } | null>(null)
  const [starting, setStarting] = useState(false)
  const [pending, setPending] = useState<{
    deviceLabel: string
    sentAt: number
    /** What the session watch below is looking for in the synced rows. */
    key: StartedRunKey
    send: WatchedSend
    /** Past `STARTED_RUN_DEADLINE_MS`: still followed, no longer "sent". */
    lapsed: boolean
    /** EXP-1284: the delivered start's id, handed to `onOpened` (the New
     *  session tab lands as the thread's tab by it). */
    startId: string | null
  } | null>(null)
  const timersRef = useRef<ReturnType<typeof setTimeout>[]>([])
  // The latest send (in flight or watched): what `quietWatch` quiets.
  const sendRef = useRef<WatchedSend | null>(null)
  const deadlineToastRef = useRef<string | number | null>(null)
  const mountedRef = useRef(true)
  const [openedId, setOpenedId] = useState<string | null>(null)
  const openStarted = useOpenStartedSession()
  const onOpenedRef = useRef(options.onOpened)
  onOpenedRef.current = options.onOpened
  const onSentRef = useRef(options.onSent)
  onSentRef.current = options.onSent
  const threadIdRef = useRef(options.threadId ?? null)
  threadIdRef.current = options.threadId ?? null
  // EXP-1259: the startId a failure toast's Retry hands to the caller's
  // re-submit (consumed by the next send of the same intent, dropped by any
  // other send).
  const retryIdRef = useRef<{ key: string; startId: string } | null>(null)
  /** The send's id: the caller's, else the pending Retry's for this same
   *  intent, else a fresh one (a new press). */
  const resolveStartId = (key: string, startId?: string): string => {
    const handed = retryIdRef.current
    retryIdRef.current = null
    if (startId) return startId
    return handed?.key === key ? handed.startId : newStartId()
  }
  /** The caller's Retry, re-sending under `startId`. */
  const retryUnder = (
    key: string,
    startId: string,
    retry: (() => void) | undefined
  ) =>
    retry &&
    (() => {
      retryIdRef.current = { key, startId }
      retry()
    })

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
      listTeamIds && listTeamIds.length > 0 ? listTeamIds : teamId
    )
    // `listKey` stands for `listTeamIds` (a fresh array per render).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, currentUserId, deviceRows, userRows, now, teamId, listKey])

  // `latestVersions` is instance config, not a shape column — one fetch per
  // mount (sync owns the device rows).
  const [refreshTick, setRefreshTick] = useState(0)
  useEffect(() => {
    if (!enabled) return
    let active = true
    trpc.devices.latestVersions
      .query()
      .then((res) => active && setLatestVersions(res ?? null))
      .catch(() => {})
    return () => {
      active = false
    }
  }, [enabled, refreshTick])

  const clearTimers = () => {
    for (const timer of timersRef.current) clearTimeout(timer)
    timersRef.current = []
  }
  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
      clearTimers()
    }
  }, [])

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
      ((sessionRows ?? []) as CodingSession[]).filter((session) =>
        createdSince(session, pending.sentAt)
      ),
      pending.key,
      currentUserId,
      pending.sentAt - STARTED_RUN_SKEW_MS
    )
    if (!match) return
    // EXP-740: navigate to the run exactly once — clearing `pending` stops
    // this effect from ever matching again for this send. Final review: a
    // late row clears the deadline's error, and a quiet watch's caller may
    // keep it from navigating (the person changed the draft meanwhile).
    clearTimers()
    setPending(null)
    if (deadlineToastRef.current !== null) {
      toast.dismiss(deadlineToastRef.current)
      deadlineToastRef.current = null
    }
    const { canOpen } = pending.send
    const open = !canOpen || canOpen()
    setOpenedId(match.id)
    if (!open) return
    // EXP-1284: `onOpened` answers true when it opened the session itself
    // (the New session tab landing as its THREAD's tab, by start id);
    // otherwise (review T11) the start lands on its thread's page.
    const handled =
      onOpenedRef.current?.(match, { startId: pending.startId }) === true
    if (!handled) {
      openStarted(match, { ...openOptions, knownThreadId: threadIdRef.current })
    }
  }, [sessionRows, pending, currentUserId, openStarted, openOptions])

  // The desktop inserts the coding_sessions row when the launcher spins up.
  // The deadline is not a silent re-enable: a run the desktop REFUSED — a
  // conflicted worktree, a failed doctor — would otherwise be indistinguishable
  // from one still starting, the caption would just vanish and no session would
  // ever open. The desktop holds the reason (it notifies there); say so.
  // Final review: the watch itself follows the send for `WATCH_MS` (a late
  // row still lands and clears the toast); a quieted send never toasts, and
  // a send that resolves after unmount arms nothing.
  // Final review (B6): a different start drops the previous send outright
  // (in flight or watched); the same startId keeps it.
  const supersede = (startId?: string) => {
    const previous = sendRef.current
    if (!previous) return
    if (startId !== undefined && previous.startId === startId) return
    sendRef.current = null
    clearTimers()
    setPending((current) => (current?.send === previous ? null : current))
  }
  const beginSend = (startId?: string): WatchedSend => {
    supersede(startId)
    const previous = sendRef.current
    const send: WatchedSend = {
      quiet: false,
      canOpen: null,
      startId,
      requestedAt: previous?.requestedAt ?? Date.now(),
    }
    sendRef.current = send
    return send
  }
  const markSent = (
    deviceLabel: string,
    key: StartedRunKey,
    send: WatchedSend,
    /** EXP-1284: the id the start was delivered under (the server echoes
     *  it), so the New session tab can wait on THIS start. */
    deliveredId: string | null
  ) => {
    // A later start superseded this one while its request was in flight.
    if (!mountedRef.current || sendRef.current !== send) return
    clearTimers()
    if (deadlineToastRef.current !== null) {
      toast.dismiss(deadlineToastRef.current)
      deadlineToastRef.current = null
    }
    setPending({
      deviceLabel,
      sentAt: send.requestedAt,
      key,
      send,
      lapsed: false,
      startId: deliveredId,
    })
    if (deliveredId) onSentRef.current?.({ startId: deliveredId })
    timersRef.current = [
      setTimeout(() => {
        setPending((current) =>
          current?.send === send ? { ...current, lapsed: true } : current
        )
        if (send.quiet) return
        deadlineToastRef.current = toast.error(
          `${deviceLabel} never started this session`,
          { description: `Open the Exponential desktop app there to see why.` }
        )
      }, STARTED_RUN_DEADLINE_MS),
      setTimeout(() => {
        setPending((current) => (current?.send === send ? null : current))
      }, WATCH_MS),
    ]
  }

  const startIssues = async (
    device: SteerDevice,
    options: StartCodingOptions,
    issueIds: string[],
    prompt?: string,
    retry?: () => void,
    startId?: string
  ) => {
    const key = startedRunKeyForIssues(issueIds)
    if (!key) return
    const intent = startIntentKey(device.deviceId, { issueIds })
    const id = resolveStartId(intent, startId)
    const send = beginSend(id)
    setStarting(true)
    try {
      // EXP-481: `resume` rides SINGLE-issue starts only (a batch has no
      // per-issue worktree; the server rejects it there).
      const { resume, ...rest } = options
      const base = {
        deviceId: device.deviceId,
        ...threadIdPayload(threadIdRef.current),
        ...rest,
        // A blank prompt is OMITTED — the server drops whitespace-only
        // anyway, and older relays never see the key.
        ...(prompt && prompt.trim() ? { prompt } : {}),
        startId: id,
      }
      const sent = await trpc.steer.startSession.mutate(
        // 1 issue → plain single-issue session; 2+ → one batch session on a
        // single pushed branch (the server contract owns the fan-out).
        issueIds.length === 1
          ? {
              issueId: issueIds[0],
              ...base,
              ...(resume ? { resume } : {}),
            }
          : { issueIds, ...base },
        SKIP_ERROR_TOAST
      )
      markSent(device.deviceLabel, key, send, sent?.startId ?? id)
    } catch (error) {
      startFailureToast(
        error,
        device,
        options.agent,
        retryUnder(intent, id, retry)
      )
      throw error
    } finally {
      setStarting(false)
    }
  }

  const runAction = async (
    device: SteerDevice,
    action: RemoteStartAction,
    options: StartCodingOptions,
    inputs?: Record<string, string>,
    prompt?: string,
    retry?: () => void,
    startId?: string
  ) => {
    const intent = startIntentKey(device.deviceId, { actionId: action.id })
    const id = resolveStartId(intent, startId)
    const send = beginSend(id)
    setStarting(true)
    try {
      // Action runs never resume an issue worktree (EXP-481).
      const { resume: _resume, ...rest } = options
      const sent = await trpc.steer.startSession.mutate(
        {
          actionId: action.id,
          deviceId: device.deviceId,
          // teamId is required iff the action is the virtual builtin (no DB
          // row to derive the team from) and forbidden otherwise.
          ...(isBuiltinActionId(action.id) ? { teamId: action.teamId } : {}),
          ...(inputs ? { inputs } : {}),
          ...(prompt && prompt.trim() ? { prompt } : {}),
          ...threadIdPayload(threadIdRef.current),
          startId: id,
          ...rest,
        },
        SKIP_ERROR_TOAST
      )
      markSent(
        device.deviceLabel,
        { kind: `action`, actionName: action.name },
        send,
        sent?.startId ?? id
      )
    } catch (error) {
      startFailureToast(
        error,
        device,
        options.agent,
        retryUnder(intent, id, retry)
      )
      throw error
    } finally {
      setStarting(false)
    }
  }

  return {
    devices,
    starting,
    sentTo: pending && !pending.lapsed ? pending.deviceLabel : null,
    startIssues,
    runAction,
    supersede,
    quietWatch: (canOpen) => {
      const send = sendRef.current
      if (!send) return
      send.quiet = true
      send.canOpen = canOpen ?? null
    },
    openedId,
    refresh: () => setRefreshTick((tick) => tick + 1),
    latestVersions,
  }
}

/** EXP-1284: the `threadId` a start carries, when it has one. */
function threadIdPayload(threadId: string | null): { threadId?: string } {
  return threadId ? { threadId } : {}
}
