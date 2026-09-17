import type { CodingSession } from "@/db/schema"
import { relativeTime } from "@/components/comment-rows/format"
import type { PastRunRow } from "@/hooks/use-agents-data"
import {
  conceptIcon,
  SESSION_DOT_CLASS,
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
  Pill,
} from "@exp/ui"
import {
  isLiveRun,
  LIVE_RUN_LABEL,
  pastRunByline,
  pastRunEndedAt,
} from "@/lib/past-runs"
import { cn } from "@/lib/utils"

// EXP-886: the run SWITCHER — the session view's select between an issue's
// runs of mine, shown only once there are two or more (`selectIssueRuns`, live
// first then newest end first). It lives on the session view alone, ×4: the
// issue page keeps its Issue | Run toggle (whose Run segment reads "Runs" in
// the same case), and the switcher is where a reader of ONE run reaches the
// others. Picking a run opens it the way every list does — the tab's Run
// face follows the URL, and the work-tab reconcile never rebinds a run being
// read (`viewedRunId`).
//
// Entries are the Recent byline (`<device> · <when>`) with the ended relative
// time, or the word `Live` in its place for a live-status run, which also
// wears the running dot. The trigger names the run on show by its `<when>`.
// Desktop `session_screen::run_switcher`, iOS `AgentSessionView.runSwitcher`,
// Android `AgentSessionScreen` twin.

const RunSwitcherIcon = conceptIcon(`run-switcher`)
const UiChevronDownIcon = conceptIcon(`ui-chevron-down`)

/** The `<when>` segment of a run's entry: `Live` for a live-status run, else
 *  when it ended (empty when the row stamped no honest time). */
export function issueRunWhen(
  session: Pick<CodingSession, `status` | `endedBy` | `endedAt` | `updatedAt`>
): string {
  if (isLiveRun(session)) return LIVE_RUN_LABEL
  const endedAt = pastRunEndedAt(session)
  return endedAt > 0 ? relativeTime(new Date(endedAt)) : ``
}

/** One entry's text: the Recent byline with `issueRunWhen` in the time slot. */
export function issueRunEntryLabel(row: {
  session: Pick<
    CodingSession,
    `status` | `endedBy` | `endedAt` | `updatedAt` | `deviceLabel`
  >
  device: Pick<PastRunRow[`device`], `label`>
}): string {
  return pastRunByline({
    deviceLabel: row.device.label ?? row.session.deviceLabel,
    relativeTime: issueRunWhen(row.session),
  })
}

export function IssueRunSwitcher({
  runs,
  viewedRunId,
  onOpen,
}: {
  /** `useIssueRuns` rows — the issue's runs of mine, in switcher order. */
  runs: readonly PastRunRow[]
  /** The run the session view shows. */
  viewedRunId: string
  onOpen: (session: CodingSession) => void
}) {
  if (runs.length < 2) return null
  const viewed = runs.find((row) => row.session.id === viewedRunId)
  const triggerLabel = viewed ? issueRunWhen(viewed.session) : ``
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Pill
          size="sm"
          mode="action"
          className="shrink-0"
          title="Switch run"
          aria-label="Switch run"
          leading={<RunSwitcherIcon className="size-3" />}
          data-testid="issue-run-switcher"
        >
          {triggerLabel || `${runs.length} runs`}
          <UiChevronDownIcon className="size-3 opacity-70" />
        </Pill>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" data-testid="issue-run-switcher-menu">
        {runs.map((row) => {
          const live = isLiveRun(row.session)
          return (
            <DropdownMenuCheckboxItem
              key={row.session.id}
              checked={row.session.id === viewedRunId}
              onSelect={() => {
                if (row.session.id !== viewedRunId) onOpen(row.session)
              }}
              data-testid={`issue-run-option-${row.session.id}`}
            >
              <span
                aria-hidden
                className={cn(
                  `size-1.5 shrink-0 rounded-full`,
                  live ? SESSION_DOT_CLASS.running : SESSION_DOT_CLASS.muted
                )}
              />
              <span className="min-w-0 truncate">
                {issueRunEntryLabel(row)}
              </span>
            </DropdownMenuCheckboxItem>
          )
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
