import { Link } from "@tanstack/react-router"
import { MonitorPlay } from "lucide-react"
import type { AgentSessionRow } from "@/hooks/use-agents-data"
import {
  sessionDisplayState,
  type SessionDisplayState,
} from "@/components/issue-coding-rows"
import { relativeTime } from "@/components/comment-rows/format"
import { displayUserName } from "@/lib/user-display"
import { Button } from "@/components/ui/button"

// The live coding-session row shared by the team Agents page and the Actions
// page (EXP-253) — extracted from routes/t/$teamSlug/agents/index.tsx so both
// surfaces render identical rows. Labeling is three-way: an action run
// (actionName snapshot set — survives the action's deletion) shows
// "Action" + the action name, an issueless batch run shows "Batch",
// everything else is the linked issue.

export function SectionLabel({ label, count }: { label: string; count: number }) {
  return (
    <div
      className="flex items-center gap-1.5 rounded-t-md border-b border-border/50 px-3 py-1.5"
      style={{ backgroundColor: `rgba(113, 113, 122, 0.08)` }}
    >
      <span className="text-sm font-medium">{label}</span>
      <span className="text-xs text-muted-foreground">{count}</span>
    </div>
  )
}

// Steady dot per parked display state (EXP-194/EXP-214): review green,
// done blue (both matching the issue-status palette), needs-input amber;
// running keeps the emerald ping.
const STATE_DOT: Record<Exclude<SessionDisplayState, `running`>, string> = {
  needs_input: `bg-amber-500`,
  review: `bg-emerald-500`,
  done: `bg-sky-500`,
}

const STATE_LABEL: Record<
  Exclude<SessionDisplayState, `running`>,
  { text: string; className: string }
> = {
  needs_input: { text: `Needs input`, className: `text-amber-400` },
  review: { text: `Ready for review`, className: `text-emerald-400` },
  done: { text: `Done`, className: `text-sky-400` },
}

export function RunningIndicator({ state }: { state: SessionDisplayState }) {
  if (state !== `running`) {
    return (
      <span
        className={`inline-flex size-2 rounded-full ${STATE_DOT[state]}`}
      />
    )
  }
  return (
    <span className="relative flex size-2">
      <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-60" />
      <span className="relative inline-flex size-2 rounded-full bg-emerald-500" />
    </span>
  )
}

export function SessionRow({
  row,
  canWatch,
  teamSlug,
  onOpen,
}: {
  row: AgentSessionRow
  /** Whether the Watch button shows at all (member + relay on). */
  canWatch: boolean
  teamSlug: string
  onOpen: () => void
}) {
  const { session, issue, board, user } = row
  const isAction = session.actionName != null
  const isBatch = !session.issueId
  const displayState = sessionDisplayState(session, issue?.prState)

  return (
    <div
      className="group/row grid cursor-pointer grid-cols-[1.5rem_4.5rem_1fr_auto] items-center border-b border-border/30 px-3 py-2 hover:bg-muted/50"
      onClick={onOpen}
      data-testid={`agent-session-${issue?.identifier ?? session.id}`}
    >
      <span className="flex items-center">
        <RunningIndicator state={displayState} />
      </span>
      <span className="truncate font-mono text-xs text-muted-foreground">
        {isAction ? (
          `Action`
        ) : issue && board ? (
          <Link
            to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
            params={{
              teamSlug,
              boardSlug: board.slug,
              issueIdentifier: issue.identifier,
            }}
            onClick={(e) => e.stopPropagation()}
            className="hover:underline"
          >
            {issue.identifier}
          </Link>
        ) : isBatch ? (
          `Batch`
        ) : (
          `—`
        )}
      </span>
      <div className="min-w-0 pr-2">
        <div className="flex min-w-0 items-center gap-1.5 text-sm">
          <span className="truncate">
            {isAction
              ? session.actionName
              : isBatch
                ? `Batch session`
                : (issue?.title ?? `Issue syncing…`)}
          </span>
          {displayState !== `running` && (
            <span
              className={`shrink-0 text-xs ${STATE_LABEL[displayState].className}`}
            >
              {STATE_LABEL[displayState].text}
            </span>
          )}
        </div>
        <div className="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
          {board && (
            <span className="inline-flex min-w-0 items-center gap-1">
              <span
                className="h-2 w-2 shrink-0 rounded-full"
                style={{ backgroundColor: board.color }}
              />
              <span className="truncate">{board.name}</span>
              <span aria-hidden>·</span>
            </span>
          )}
          <span className="truncate">
            {displayUserName(user, session.userId)}
            {session.deviceLabel ? ` · ${session.deviceLabel}` : ``}
          </span>
          <span className="shrink-0 whitespace-nowrap">
            {`· started ${relativeTime(session.startedAt)}`}
          </span>
        </div>
      </div>
      <div className="flex items-center gap-2">
        {canWatch && (
          <Button
            variant="outline"
            size="sm"
            onClick={(e) => {
              e.stopPropagation()
              onOpen()
            }}
          >
            <MonitorPlay />
            Watch
          </Button>
        )}
      </div>
    </div>
  )
}
