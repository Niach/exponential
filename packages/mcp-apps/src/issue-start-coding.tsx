import { useCallback, useEffect, useState } from "react"
import {
  AgentBrandMark,
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
  IconTooltip,
  conceptIcon,
} from "@exp/ui"
import { useMcpActions } from "./actions"
import {
  startArgs,
  startTargets,
  type StartDeviceRow,
  type StartTarget,
} from "./issue-detail-logic"

const RunIcon = conceptIcon(`action-run`)
const LoadingIcon = conceptIcon(`ui-loading`)

type Devices =
  | { kind: `loading` }
  | { kind: `error`; message: string }
  | { kind: `ready`; targets: StartTarget[] }

type Outcome =
  | { kind: `started`; sessionId: string; target: StartTarget }
  | { kind: `error`; message: string }

// EXP-1183 — "Start coding" on the issue face: the caller's ONLINE machines
// (`exponential_devices_list`, plus the team's shared servers once `teamId`
// resolves) × their runnable agents, then `exponential_sessions_start`. The
// run lives on the device; the face only reports the id it got back.
export function IssueStartCoding({
  issueId,
  teamId,
  teamResolved,
  onStarted,
  onOpenChange,
}: {
  issueId: string
  teamId: string | null
  /** False while the team lookup is in flight — the list waits for it. */
  teamResolved: boolean
  onStarted?: () => void
  onOpenChange?: (open: boolean) => void
}) {
  const { call } = useMcpActions()
  const [devices, setDevices] = useState<Devices>({ kind: `loading` })
  const [starting, setStarting] = useState(false)
  const [outcome, setOutcome] = useState<Outcome | null>(null)
  const [menuOpen, setMenuOpen] = useState(false)
  const setOpen = (open: boolean) => {
    setMenuOpen(open)
    onOpenChange?.(open)
  }

  const load = useCallback(async () => {
    const result = await call<StartDeviceRow[]>(
      `exponential_devices_list`,
      teamId ? { teamId } : {}
    )
    setDevices(
      result.kind === `ok`
        ? { kind: `ready`, targets: startTargets(Array.isArray(result.data) ? result.data : []) }
        : { kind: `error`, message: result.message }
    )
  }, [call, teamId])

  useEffect(() => {
    if (teamResolved) void load()
  }, [teamResolved, load])

  const start = async (target: StartTarget) => {
    setStarting(true)
    setOutcome(null)
    const result = await call<{ sessionId?: string }>(
      `exponential_sessions_start`,
      startArgs(target, issueId)
    )
    setStarting(false)
    if (result.kind === `ok` && result.data.sessionId) {
      setOutcome({ kind: `started`, sessionId: result.data.sessionId, target })
      onStarted?.()
    } else {
      setOutcome({
        kind: `error`,
        message: result.kind === `error` ? result.message : `The device started no run.`,
      })
    }
  }

  const targets = devices.kind === `ready` ? devices.targets : []
  const unavailable = devices.kind !== `ready` || targets.length === 0
  const hint =
    devices.kind === `loading`
      ? `Looking for online devices…`
      : devices.kind === `error`
        ? devices.message
        : targets.length === 0
          ? `No device is online. Open the desktop app or run the exponential CLI.`
          : undefined

  // The refresh on open may find every machine gone: the menu unmounts
  // without its own close, so the parent hears it here.
  useEffect(() => {
    if (unavailable && menuOpen) {
      setMenuOpen(false)
      onOpenChange?.(false)
    }
  }, [unavailable, menuOpen, onOpenChange])

  const button = (
    <Button
      size="xs"
      disabled={unavailable || starting}
      aria-label={starting ? `Starting a run` : `Start coding`}
    >
      {starting || devices.kind === `loading` ? (
        <LoadingIcon className="animate-spin" />
      ) : (
        <RunIcon />
      )}
      Start coding
    </Button>
  )

  return (
    <div className="flex flex-col items-end gap-1">
      {unavailable ? (
        <IconTooltip label="Start coding" hint={hint}>
          {button}
        </IconTooltip>
      ) : (
        <DropdownMenu
          modal={false}
          open={menuOpen}
          onOpenChange={(open) => {
            setOpen(open)
            // A machine may have come or gone since the face opened.
            if (open) void load()
          }}
        >
          <DropdownMenuTrigger asChild>{button}</DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-60">
            <DropdownMenuLabel className="text-xs text-muted-foreground">
              Start a run on
            </DropdownMenuLabel>
            {targets.map((target) => (
              <DropdownMenuItem key={target.key} onSelect={() => void start(target)}>
                {target.agent ? (
                  <AgentBrandMark agent={target.agent} className="size-4" />
                ) : (
                  <RunIcon className="size-4" />
                )}
                <span className="min-w-0 truncate">{target.label}</span>
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
      {outcome?.kind === `started` && (
        <span className="text-xs text-muted-foreground" role="status">
          Run{` `}
          <span className="font-mono">{outcome.sessionId.slice(0, 8)}</span>
          {` `}started on {outcome.target.deviceLabel}
        </span>
      )}
      {outcome?.kind === `error` && (
        <span className="max-w-64 text-right text-xs text-destructive" role="alert">
          {outcome.message}
        </span>
      )}
    </div>
  )
}
