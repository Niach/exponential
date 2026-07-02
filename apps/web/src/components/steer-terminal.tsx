import { useEffect, useMemo, useRef, useState } from "react"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import { Terminal } from "@xterm/xterm"
import { FitAddon } from "@xterm/addon-fit"
import "@xterm/xterm/css/xterm.css"
import { toast } from "sonner"
import { TRPCClientError } from "@trpc/client"
import {
  ChevronDown,
  Eye,
  Keyboard,
  KeyboardOff,
  Loader2,
  MonitorPlay,
  MonitorUp,
  OctagonX,
  RotateCw,
  X,
} from "lucide-react"
import type { CodingSession, User } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import {
  codingSessionCollection,
  workspaceMemberCollection,
} from "@/lib/collections"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

// Live "coding now" badge + remote terminal viewer/steerer for the issue
// detail screen (masterplan §3.7). When a running coding_sessions row exists
// for the issue (synced over Electric) and steer.config reports the relay
// enabled, members can watch the desktop-hosted PTY live over the steer relay
// (xterm.js), claim the single steer token to type into it, and kill the
// session. With no running session, members with an online desktop get a
// "Start on my desktop" button (relay-routed remote start). Everything
// degrades to just the badge (or nothing) when the relay is off.

// ── Wire protocol (viewer side of apps/steer-relay/src/protocol.ts) ──────────

const OUTPUT_OPCODE = 0x01
// Relay rejects input frames > 8 KiB; chunk pastes well under that.
const INPUT_CHUNK_CHARS = 4096

interface PresenceViewer {
  userId: string
  name: string
  perm: `view` | `steer`
}

type ServerFrame =
  | { t: `presence`; viewers: PresenceViewer[]; steererId: string | null }
  | { t: `resize`; cols: number; rows: number }
  | { t: `bye`; outcome?: string }
  | { t: `error`; code: string; message?: string }
  | { t: string }

function parseServerFrame(raw: string): ServerFrame | null {
  try {
    const json = JSON.parse(raw) as unknown
    if (!json || typeof json !== `object`) return null
    if (typeof (json as { t?: unknown }).t !== `string`) return null
    return json as ServerFrame
  } catch {
    return null
  }
}

// The ticket is `base64url(JSON claims).base64url(sig)` — the claims carry the
// caller's perm (view|steer), which decides whether steering/kill controls
// show. Decoding locally is display-only; the relay enforces perm server-side.
function decodeTicketPerm(ticket: string): `view` | `steer` {
  try {
    const payload = ticket.slice(0, ticket.indexOf(`.`))
    const b64 = payload.replace(/-/g, `+`).replace(/_/g, `/`)
    const claims = JSON.parse(atob(b64)) as { perm?: string }
    return claims.perm === `steer` ? `steer` : `view`
  } catch {
    return `view`
  }
}

function trpcErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof TRPCClientError) {
    const message = error.message?.trim()
    if (message && !message.startsWith(`[`) && !message.startsWith(`{`)) {
      return message
    }
  }
  return fallback
}

// ── steer.config, fetched once per app lifetime (env-derived, static) ─────────

interface SteerConfig {
  enabled: boolean
  relayUrl: string | null
}

let steerConfigPromise: Promise<SteerConfig> | null = null

function fetchSteerConfigOnce(): Promise<SteerConfig> {
  steerConfigPromise ??= trpc.steer.config.query().catch((error) => {
    steerConfigPromise = null
    throw error
  })
  return steerConfigPromise
}

function useSteerConfig(): SteerConfig | null {
  const [config, setConfig] = useState<SteerConfig | null>(null)
  useEffect(() => {
    let active = true
    fetchSteerConfigOnce()
      .then((c) => active && setConfig(c))
      // Treat an unreachable config proc as "steer off" — the badge still shows.
      .catch(() => active && setConfig({ enabled: false, relayUrl: null }))
    return () => {
      active = false
    }
  }, [])
  return config
}

// ── Root: badge + viewer + remote start, driven by the synced session row ────

interface SteerTerminalProps {
  issueId: string
  workspaceId: string
  currentUserId: string
  users: User[]
}

export function SteerTerminal({
  issueId,
  workspaceId,
  currentUserId,
  users,
}: SteerTerminalProps) {
  const config = useSteerConfig()

  const { data: sessionRows } = useLiveQuery(
    (query) =>
      query
        .from({ s: codingSessionCollection })
        .where(({ s }) =>
          and(eq(s.issueId, issueId), eq(s.status, `running`))
        ),
    [issueId]
  )
  // Multi-window desktops can run several sessions on one issue; surface the
  // most recent (the badge counts them all).
  const sessions = (sessionRows ?? []) as CodingSession[]
  const session = useMemo(() => {
    if (sessions.length === 0) return null
    return sessions.reduce((latest, row) =>
      new Date(row.startedAt) > new Date(latest.startedAt) ? row : latest
    )
  }, [sessions])

  // Steer tickets require workspace membership — hide the interactive parts
  // from public-workspace visitors (the server enforces this regardless).
  const { data: memberRows } = useLiveQuery(
    (query) =>
      query
        .from({ m: workspaceMemberCollection })
        .where(({ m }) =>
          and(eq(m.workspaceId, workspaceId), eq(m.userId, currentUserId))
        ),
    [workspaceId, currentUserId]
  )
  const isMember = (memberRows?.length ?? 0) > 0

  if (!session) {
    if (!isMember || !config?.enabled) return null
    return <StartOnDesktop issueId={issueId} />
  }

  const owner = users.find((u) => u.id === session.userId)

  return (
    <div className="border-t border-border px-4 py-3">
      <div className="flex items-center gap-2 min-w-0">
        <Badge
          variant="outline"
          className="gap-1.5 border-emerald-500/40 text-emerald-400"
        >
          <span className="relative flex size-2">
            <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-60" />
            <span className="relative inline-flex size-2 rounded-full bg-emerald-500" />
          </span>
          Coding now
          {sessions.length > 1 ? ` (${sessions.length})` : ``}
        </Badge>
        <span className="truncate text-xs text-muted-foreground">
          {owner?.name ?? owner?.email ?? `Someone`}
          {session.deviceLabel ? ` · ${session.deviceLabel}` : ``}
        </span>
      </div>
      {isMember && config?.enabled ? (
        <SteerViewer
          key={session.id}
          session={session}
          currentUserId={currentUserId}
        />
      ) : isMember && config && !config.enabled ? (
        <div className="mt-2 text-xs text-muted-foreground">
          Live steering is unavailable on this instance.
        </div>
      ) : null}
    </div>
  )
}

// ── "Start on my desktop" (remote start via the relay control socket) ────────

interface SteerDevice {
  deviceId: string
  deviceLabel: string
  connectedAt: number
}

function StartOnDesktop({ issueId }: { issueId: string }) {
  const [devices, setDevices] = useState<SteerDevice[] | null>(null)
  const [starting, setStarting] = useState(false)
  const [sentTo, setSentTo] = useState<string | null>(null)

  useEffect(() => {
    let active = true
    trpc.steer.myDevices
      .query()
      .then((res) => active && setDevices(res.devices))
      .catch(() => active && setDevices([]))
    return () => {
      active = false
    }
  }, [])

  // No online desktop (or presence lookup failed) — hide cleanly.
  if (!devices || devices.length === 0) return null

  const start = async (device: SteerDevice) => {
    setStarting(true)
    try {
      await trpc.steer.startSession.mutate(
        { issueId, deviceId: device.deviceId },
        { context: { skipErrorToast: true } }
      )
      setSentTo(device.deviceLabel)
      // The desktop inserts the coding_sessions row when the launcher spins
      // up, which swaps this whole section for the live panel via Electric.
      // Re-enable after a grace window in case it never picks up.
      setTimeout(() => setSentTo(null), 30_000)
    } catch (error) {
      toast.error(`Couldn't start on your desktop`, {
        description: trpcErrorMessage(
          error,
          `The start command could not be delivered`
        ),
      })
    } finally {
      setStarting(false)
    }
  }

  const busy = starting || sentTo !== null

  return (
    <div className="flex items-center gap-2 border-t border-border px-4 py-3">
      {devices.length === 1 ? (
        <Button
          variant="outline"
          size="sm"
          onClick={() => void start(devices[0])}
          disabled={busy}
        >
          {starting ? (
            <Loader2 className="animate-spin" />
          ) : (
            <MonitorUp />
          )}
          Start coding on {devices[0].deviceLabel}
        </Button>
      ) : (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" size="sm" disabled={busy}>
              {starting ? (
                <Loader2 className="animate-spin" />
              ) : (
                <MonitorUp />
              )}
              Start on my desktop
              <ChevronDown />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            {devices.map((device) => (
              <DropdownMenuItem
                key={device.deviceId}
                onClick={() => void start(device)}
              >
                <MonitorPlay />
                {device.deviceLabel}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
      {sentTo && (
        <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
          <Loader2 className="size-3 animate-spin" />
          Start sent to {sentTo} — waiting for the desktop…
        </span>
      )}
    </div>
  )
}

// ── The live viewer: xterm over the relay viewer socket ──────────────────────

type ViewerPhase =
  | { kind: `idle` }
  | { kind: `connecting` }
  | { kind: `live` }
  // The session ended (relay `bye`, or the room was never live).
  | { kind: `ended`; detail?: string }
  // Unexpected socket loss — offer a manual Reconnect (fresh ticket).
  | { kind: `closed`; detail?: string }

function SteerViewer({
  session,
  currentUserId,
}: {
  session: CodingSession
  currentUserId: string
}) {
  // Bumping `attempt` (re)runs the whole connect lifecycle with a fresh
  // ticket; 0 = not watching.
  const [attempt, setAttempt] = useState(0)
  const [phase, setPhase] = useState<ViewerPhase>({ kind: `idle` })
  const [perm, setPerm] = useState<`view` | `steer`>(`view`)
  const [viewers, setViewers] = useState<PresenceViewer[]>([])
  const [steererId, setSteererId] = useState<string | null>(null)
  const [confirmKill, setConfirmKill] = useState(false)
  const [killing, setKilling] = useState(false)

  const containerRef = useRef<HTMLDivElement | null>(null)
  const wsRef = useRef<WebSocket | null>(null)
  const steeringRef = useRef(false)

  const steering = steererId === currentUserId
  steeringRef.current = steering

  useEffect(() => {
    if (attempt === 0) return
    const container = containerRef.current
    if (!container) return

    let disposed = false
    // `ended` (bye / no_such_session) must win over the close handler.
    let endedDetail: string | null = null
    let sawEnd = false
    let publisherSized = false

    setPhase({ kind: `connecting` })
    setViewers([])
    setSteererId(null)

    const term = new Terminal({
      cursorBlink: true,
      fontSize: 12,
      fontFamily: `ui-monospace, SFMono-Regular, Menlo, Consolas, monospace`,
      scrollback: 5000,
      theme: {
        background: `#09090b`,
        foreground: `#e4e4e7`,
        cursor: `#e4e4e7`,
        selectionBackground: `#3f3f46`,
      },
    })
    const fit = new FitAddon()
    term.loadAddon(fit)
    term.open(container)
    fit.fit()

    // The publisher's geometry is authoritative (the relay replays `resize`
    // on join); fit only sizes the pane until the first publisher resize.
    const observer = new ResizeObserver(() => {
      if (!publisherSized && !disposed) fit.fit()
    })
    observer.observe(container)

    // Keystrokes flow only while this viewer holds the steer claim.
    const dataSub = term.onData((data) => {
      const ws = wsRef.current
      if (!steeringRef.current || ws?.readyState !== WebSocket.OPEN) return
      for (let i = 0; i < data.length; ) {
        let end = Math.min(i + INPUT_CHUNK_CHARS, data.length)
        // Never split a surrogate pair across input frames.
        const last = end < data.length ? data.charCodeAt(end - 1) : 0
        if (last >= 0xd800 && last <= 0xdbff) end += 1
        ws.send(JSON.stringify({ t: `input`, data: data.slice(i, end) }))
        i = end
      }
    })

    const handleFrame = (frame: ServerFrame) => {
      switch (frame.t) {
        case `presence`: {
          const f = frame as Extract<ServerFrame, { t: `presence` }>
          setViewers(f.viewers)
          setSteererId(f.steererId)
          return
        }
        case `resize`: {
          const f = frame as Extract<ServerFrame, { t: `resize` }>
          publisherSized = true
          term.resize(f.cols, f.rows)
          return
        }
        case `bye`: {
          const f = frame as Extract<ServerFrame, { t: `bye` }>
          if (f.outcome === `publisher_lost`) {
            // The desktop's relay socket dropped but the session may still be
            // running — the synced row is the truth. Stay retryable.
            endedDetail = `The desktop's connection to the relay dropped — retry once it reconnects.`
          } else {
            sawEnd = true
            endedDetail = f.outcome && f.outcome !== `ended` ? f.outcome : null
          }
          return
        }
        case `error`: {
          const f = frame as Extract<ServerFrame, { t: `error` }>
          if (f.code === `no_such_session`) {
            // Not live on the relay (yet) — the desktop may still be dialing,
            // or the room went stale. Stay retryable; the running row governs
            // whether this panel exists at all.
            endedDetail = `The terminal isn't live on the relay yet — the desktop may still be connecting.`
          } else {
            endedDetail = f.message ?? f.code
          }
          return
        }
        default:
          return
      }
    }

    let ws: WebSocket | null = null
    void (async () => {
      try {
        const minted = await trpc.steer.mintTicket.mutate(
          { kind: `viewer`, codingSessionId: session.id },
          { context: { skipErrorToast: true } }
        )
        if (disposed) return
        if (`disabled` in minted && minted.disabled) {
          setPhase({
            kind: `closed`,
            detail: `Live steering is unavailable on this instance.`,
          })
          return
        }
        const { ticket, url } = minted as { ticket: string; url: string }
        setPerm(decodeTicketPerm(ticket))

        ws = new WebSocket(url)
        ws.binaryType = `arraybuffer`
        wsRef.current = ws
        ws.onopen = () => {
          if (disposed) return
          ws?.send(JSON.stringify({ t: `join` }))
          setPhase({ kind: `live` })
        }
        ws.onmessage = (event) => {
          if (disposed) return
          if (typeof event.data === `string`) {
            const frame = parseServerFrame(event.data)
            if (frame) handleFrame(frame)
            return
          }
          const bytes = new Uint8Array(event.data as ArrayBuffer)
          if (bytes.byteLength >= 1 && bytes[0] === OUTPUT_OPCODE) {
            term.write(bytes.subarray(1))
          }
        }
        ws.onclose = () => {
          if (disposed) return
          wsRef.current = null
          setViewers([])
          setSteererId(null)
          if (sawEnd) {
            setPhase({ kind: `ended`, detail: endedDetail ?? undefined })
          } else {
            setPhase({ kind: `closed`, detail: endedDetail ?? undefined })
          }
        }
      } catch (error) {
        if (disposed) return
        setPhase({
          kind: `closed`,
          detail: trpcErrorMessage(error, `Couldn't get a viewer ticket`),
        })
      }
    })()

    return () => {
      disposed = true
      observer.disconnect()
      dataSub.dispose()
      wsRef.current = null
      ws?.close()
      term.dispose()
    }
  }, [attempt, session.id])

  const sendFrame = (frame: { t: `claim` } | { t: `release` }) => {
    const ws = wsRef.current
    if (ws?.readyState === WebSocket.OPEN) ws.send(JSON.stringify(frame))
  }

  const kill = async () => {
    setKilling(true)
    try {
      await trpc.steer.killSession.mutate(
        { codingSessionId: session.id },
        { context: { skipErrorToast: true } }
      )
      setConfirmKill(false)
      // The synced row flips to ended, which unmounts the whole panel; the
      // relay `bye` tears the socket down in parallel.
    } catch (error) {
      toast.error(`Couldn't kill the session`, {
        description: trpcErrorMessage(error, `The kill could not be delivered`),
      })
    } finally {
      setKilling(false)
    }
  }

  const otherSteerer =
    steererId && steererId !== currentUserId
      ? (viewers.find((v) => v.userId === steererId)?.name ?? `Someone`)
      : null
  const live = phase.kind === `live`
  const watching = attempt > 0 && (live || phase.kind === `connecting`)

  return (
    <div className="mt-2">
      <div className="flex flex-wrap items-center gap-2">
        {!watching && (
          <Button
            variant="outline"
            size="sm"
            onClick={() => setAttempt((n) => n + 1)}
            disabled={phase.kind === `ended`}
          >
            {phase.kind === `closed` ? <RotateCw /> : <MonitorPlay />}
            {phase.kind === `closed` ? `Reconnect` : `Watch live`}
          </Button>
        )}
        {phase.kind === `connecting` && (
          <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
            <Loader2 className="size-3 animate-spin" /> Connecting…
          </span>
        )}
        {live &&
          perm === `steer` &&
          (steering ? (
            <Button
              variant="outline"
              size="sm"
              onClick={() => sendFrame({ t: `release` })}
            >
              <KeyboardOff />
              Release steering
            </Button>
          ) : (
            <Button
              variant="outline"
              size="sm"
              onClick={() => sendFrame({ t: `claim` })}
              disabled={otherSteerer !== null}
            >
              <Keyboard />
              Take steering
            </Button>
          ))}
        {/* steer.killSession also authorizes the session owner — a member who
            remote-started their own session can kill it without steer perm. */}
        {live && (perm === `steer` || session.userId === currentUserId) && (
          <Button
            variant="ghost"
            size="sm"
            className="text-destructive hover:text-destructive"
            onClick={() => setConfirmKill(true)}
          >
            <OctagonX />
            Kill session
          </Button>
        )}
        {watching && (
          <Button
            variant="ghost"
            size="sm"
            className="ml-auto"
            onClick={() => {
              setAttempt(0)
              setPhase({ kind: `idle` })
            }}
          >
            <X />
            Stop watching
          </Button>
        )}
      </div>

      {live && (viewers.length > 0 || otherSteerer) && (
        <div className="mt-2 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          <Eye className="size-3" />
          {viewers.map((viewer) => (
            <span
              key={viewer.userId}
              className={
                viewer.userId === steererId
                  ? `inline-flex items-center gap-1 text-foreground`
                  : `inline-flex items-center gap-1`
              }
            >
              {viewer.userId === steererId && <Keyboard className="size-3" />}
              {viewer.name}
              {viewer.userId === steererId ? ` (steering)` : ``}
            </span>
          ))}
        </div>
      )}

      {phase.kind === `ended` && (
        <div className="mt-2 text-xs text-muted-foreground">
          {phase.detail ?? `The session has ended.`}
        </div>
      )}
      {phase.kind === `closed` && (
        <div className="mt-2 text-xs text-muted-foreground">
          {phase.detail ?? `Connection lost.`}
        </div>
      )}

      <div
        ref={containerRef}
        className={
          watching
            ? `mt-2 h-80 overflow-auto rounded-md border border-border bg-[#09090b] p-2`
            : `hidden`
        }
      />

      <Dialog open={confirmKill} onOpenChange={setConfirmKill}>
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Kill this coding session?</DialogTitle>
            <DialogDescription>
              This force-terminates the terminal
              {session.deviceLabel ? ` on ${session.deviceLabel}` : ``} and
              ends the session. Uncommitted work in the worktree is kept, but
              Claude stops immediately.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="ghost"
              onClick={() => setConfirmKill(false)}
              disabled={killing}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={() => void kill()}
              disabled={killing}
            >
              {killing && <Loader2 className="animate-spin" />}
              Kill session
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
