import type { CodingSession } from "@/db/schema"
import { relativeTime } from "@/components/comment-rows/format"
import type { PastRunRow } from "@/hooks/use-agents-data"
import {
  AgentRunMark,
  DropdownMenuContent,
  MenuContentEntries,
  type MenuEntry,
  type RunMarkState,
} from "@exp/ui"
import {
  isLiveRun,
  LIVE_RUN_LABEL,
  pastRunByline,
  pastRunEndedAt,
} from "@/lib/past-runs"
import {
  runningRowMarkState,
  sessionDisplayState,
  sessionRowIsWorking,
} from "@/lib/coding-session-display"

// EXP-886 / EXP-950: the run MENU — the select between an issue's runs of
// mine (`selectIssueRuns`, live first then newest end first), or (EXP-974)
// between the members of an issue-less run's RESUME CHAIN (`runChain`,
// newest first — the predecessor and its successor, one toggle). EXP-950 folded
// the separate switcher pill into the work header's face toggle: with two or
// more runs the `Runs` segment carries a caret (`WorkFaceToggle` `runMenu`)
// and this is the menu it opens, on the issue face and the run face alike.
// Picking a run opens it the way every list does — the tab's Run face
// follows the URL, and the work-tab reconcile never rebinds a run being read.
//
// Entries are the Recent byline (`<device> · <when>`) with the ended relative
// time, or the word `Live` in its place for a live-status run, each led by
// the run's MARK (`AgentRunMark`, the session lists' lead), not a dot. Desktop `work_header::face_toggle`'s run menu; the
// phones list the same rows in the face switcher (`mobile-face-switcher`).
//
// The UI cleanup batch: the rows are `MenuEntry` choice rows of the shared
// `Menu` (`issueRunMenuEntries`) — the run on show wears the trailing check.

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

/** A run's mark state in the menu: the live display state (only a working
 *  run animates), the dimmed mark once it ended. */
export function issueRunMarkState(row: PastRunRow): RunMarkState | undefined {
  const { session } = row
  if (!isLiveRun(session)) return `ended`
  const prState = row.issue?.prState ?? session.prState
  return runningRowMarkState(sessionDisplayState(session, prState), {
    paused: false,
    working: sessionRowIsWorking(session, prState),
  })
}

/** The run menu as `Menu` rows: one per run, the one on show checked. */
export function issueRunMenuEntries({
  runs,
  checkedRunId,
  onOpen,
}: {
  runs: readonly PastRunRow[]
  checkedRunId?: string
  onOpen: (session: CodingSession) => void
}): MenuEntry[] {
  return runs.map((row) => ({
    kind: `item`,
    id: row.session.id,
    "data-testid": `issue-run-${row.session.id}`,
    icon: (
      <AgentRunMark
        agent={row.session.agent}
        state={issueRunMarkState(row)}
      />
    ),
    label: issueRunEntryLabel(row),
    checked: row.session.id === checkedRunId,
    onSelect: () => onOpen(row.session),
  }))
}

/** The caret's menu inside a host that owns the dropdown root (the work
 *  face strip): one row per run, the one on show checked. */
export function IssueRunMenuContent({
  runs,
  checkedRunId,
  onOpen,
}: {
  /** `useIssueRuns` rows — the issue's runs of mine, in switcher order. */
  runs: readonly PastRunRow[]
  /** The run on show (the session view's), or the one the tab's Run face
   *  opens (the issue face). */
  checkedRunId?: string
  onOpen: (session: CodingSession) => void
}) {
  return (
    // Wider than the stock menu: a long device name must not push the
    // `<when>` (`Live`) out of sight.
    <DropdownMenuContent
      align="end"
      className="max-w-[360px]"
      data-testid="issue-run-switcher-menu"
    >
      <MenuContentEntries
        entries={issueRunMenuEntries({ runs, checkedRunId, onOpen })}
      />
    </DropdownMenuContent>
  )
}
