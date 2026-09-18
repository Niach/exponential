import type { CodingSession } from "@/db/schema"
import { relativeTime } from "@/components/comment-rows/format"
import type { PastRunRow } from "@/hooks/use-agents-data"
import {
  SESSION_DOT_CLASS,
  ComboboxMenuItems,
  DropdownMenuContent,
  type PickerOption,
} from "@exp/ui"
import {
  isLiveRun,
  LIVE_RUN_LABEL,
  pastRunByline,
  pastRunEndedAt,
} from "@/lib/past-runs"
import { cn } from "@/lib/utils"

// EXP-886 / EXP-950: the run MENU — the select between an issue's runs of
// mine (`selectIssueRuns`, live first then newest end first). EXP-950 folded
// the separate switcher pill into the work header's face toggle: with two or
// more runs the `Runs` segment carries a caret (`WorkFaceToggle` `runMenu`)
// and this is the menu it opens, on the issue face and the run face alike.
// Picking a run opens it the way every list does — the tab's Run face
// follows the URL, and the work-tab reconcile never rebinds a run being read.
//
// Entries are the Recent byline (`<device> · <when>`) with the ended relative
// time, or the word `Live` in its place for a live-status run, which also
// wears the running dot. Desktop `work_header::face_toggle`'s run menu; the
// phones list the same rows in the face switcher (`mobile-face-switcher`).
//
// EXP-958: the rows are the Combobox's menu arm (`ComboboxMenuItems`), a
// single select whose picked row wears the picker's trailing check — not the
// dropdown's own checkbox tick, which was the last "this is picked" idiom
// the run menu drew on its own.

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

/** One run as a picker row: the session id is the identity, the byline the
 *  label. `runSessionDot` draws the leading dot the byline carries. */
export function issueRunOption(row: PastRunRow): PickerOption<string> {
  return { value: row.session.id, label: issueRunEntryLabel(row) }
}

/** The dot before a run's byline: running for a live-status run, muted
 *  otherwise. Shared with the phone's face switcher. */
export function RunSessionDot({ live }: { live: boolean }) {
  return (
    <span
      aria-hidden
      className={cn(
        `size-1.5 shrink-0 rounded-full`,
        live ? SESSION_DOT_CLASS.running : SESSION_DOT_CLASS.muted
      )}
    />
  )
}

/** The caret's menu: one row per run, the one on show checked. */
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
  const byId = new Map(runs.map((row) => [row.session.id, row]))
  return (
    // Wider than the stock menu: a long device name must not push the
    // `<when>` (`Live`) out of sight.
    <DropdownMenuContent
      align="end"
      className="max-w-[360px]"
      data-testid="issue-run-switcher-menu"
    >
      <ComboboxMenuItems
        menu="dropdown"
        options={runs.map(issueRunOption)}
        value={checkedRunId ?? null}
        onChange={(id) => {
          const row = id === null ? undefined : byId.get(id)
          if (row) onOpen(row.session)
        }}
        renderOption={(option) => (
          <>
            <RunSessionDot live={isLiveRun(byId.get(option.value)!.session)} />
            <span className="min-w-0 truncate">{option.label}</span>
          </>
        )}
      />
    </DropdownMenuContent>
  )
}
