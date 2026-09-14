import type { Issue } from "@/db/schema"
import type { TeamAction } from "@/components/action-editor-dialog"
import { useIssueRefs } from "@/components/issue-ref-provider"
import { IssueChip } from "@/components/issue-chip"
import { Pill } from "@/components/ui/pill"
import { conceptIcon } from "@/lib/icons.generated"
import { getActionIcon } from "@/lib/board-icons"

// EXP-825: the composer's SUBJECT, as chips in the card's leading row — one
// per picked issue, or ONE action chip. They are how "exclusivity by swap"
// reads: picking the other kind replaces what is here, and the chips show it.
//
// EXP-827/EXP-885: an issue chip IS `<IssueChip>` — the one chip the session
// feed, the timeline and the markdown editor draw (small rounded rect, status
// glyph, mono identifier, muted title), here with its trailing ✕; the body
// opens the issue where an issue-ref provider is mounted and is inert
// otherwise. The action chip's body never dismisses the action — only its ✕
// does (the last issue chip going = a chat again).
// Test ids are byte-identical with the native suites
// (`agent-composer-chip-issue-<IDENT>` + `-remove`,
// `agent-composer-chip-action` + `-remove`).

const UiCloseIcon = conceptIcon(`ui-close`)
const IssueRefIcon = conceptIcon(`editor-issue-ref`)

export function SubjectChips({
  issues,
  pendingIssueCount,
  action,
  actionPending,
  onRemoveIssue,
  onClearAction,
  disabled,
}: {
  /** The checked issues that have synced rows. */
  issues: Issue[]
  /** Checked ids whose rows have not synced yet (a fresh deep link). */
  pendingIssueCount: number
  /** The subject action's row, when the subject is an action. */
  action: TeamAction | null
  /** The subject is an action whose row is not synced (or gone). */
  actionPending: boolean
  onRemoveIssue: (issueId: string) => void
  onClearAction: () => void
  disabled?: boolean
}) {
  const issueRefs = useIssueRefs()

  if (action || actionPending) {
    const RowIcon = action ? getActionIcon(action) : IssueRefIcon
    const name = action?.name ?? `action`
    return (
      <Pill
        size="sm"
        mode="readonly"
        className="max-w-[20rem] pr-1"
        data-testid="agent-composer-chip-action"
        title={action?.name}
      >
        <RowIcon className="size-3 shrink-0" />
        <span className="min-w-0 truncate">{action?.name ?? `Action…`}</span>
        <ChipRemove
          label={`Remove ${name}`}
          testId="agent-composer-chip-action-remove"
          disabled={disabled}
          onClick={onClearAction}
        />
      </Pill>
    )
  }
  return (
    <>
      {issues.map((issue) => (
        <IssueChip
          key={issue.id}
          issue={issue}
          className="max-w-[20rem]"
          // The composer is a typing surface — a hover card popping over the
          // field while picking issues is noise.
          preview={false}
          onClick={
            issueRefs ? () => issueRefs.open(issue.identifier) : undefined
          }
          onRemove={() => onRemoveIssue(issue.id)}
          removeDisabled={disabled}
          testId={`agent-composer-chip-issue-${issue.identifier}`}
          removeTestId={`agent-composer-chip-issue-${issue.identifier}-remove`}
        />
      ))}
      {pendingIssueCount > 0 && (
        <Pill size="sm">{`${pendingIssueCount} loading…`}</Pill>
      )}
    </>
  )
}

/** The chip's own ✕ — the one part of a chip that removes it. */
function ChipRemove({
  label,
  testId,
  disabled,
  onClick,
}: {
  label: string
  testId: string
  disabled?: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      data-testid={testId}
      disabled={disabled}
      onClick={onClick}
      className="flex size-4 shrink-0 cursor-pointer items-center justify-center rounded-full text-muted-foreground outline-none hover:bg-glass-active hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50"
    >
      <UiCloseIcon className="size-3" />
    </button>
  )
}
